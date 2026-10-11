//! Phase 26.4 court: the legacy Outlook message (`.msg`, MS-OXMSG) semantic adapter
//! built on the CFBF/OLE2 physical layer.
//!
//! Gated on `msg`, so a build without the feature compiles an empty target.
//!
//! A `.msg` has no package layer: the exact leaf is the whole source (a
//! `DocumentExact`) and every derived observation is a bounded, representation-
//! preserving projection of it. The court checks:
//!
//! 1. **Byte-based detection** and the boundaries: a `.msg` → `Msg`; a generic
//!    compound file → `Cfbf`; an OLE file without a root `__properties_version1.0`
//!    → `Cfbf`; prose → `Opaque`.
//! 2. **Representation preservation**: the top-level message header, the fixed-size
//!    16-byte property entries (raw tag/id/type, flags, value bytes, exact span,
//!    owner) in order, the `__substg1.0_<PPPPTTTT>` substreams (UTF-16LE / ANSI /
//!    binary) with their exact spans and decoded text, and the recipient/attachment
//!    sub-storages with their own property lists and substreams (order and ids
//!    preserved).
//! 3. **Selectors**: native `msg-summary`/`msg-property`/`msg-recipient`/
//!    `msg-attachment`/`msg-find` plus common `metadata`/`text`/`search-match`.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted.
//! 5. **Fail closed**: malformed inputs decline typed (a short property stream, an
//!    overrunning entry, a bad substream name, an unreferenced substream, an
//!    unsupported substream type, a count mismatch, a sub-storage without its own
//!    property stream, a non-decimal sub-storage id); random bytes never panic.
//! 6. **No cross-field aliasing** (ADR-0060): the model node depends on the exact
//!    `sha256(source)` root.

#![cfg(all(feature = "field", feature = "msg"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_MSG_MODEL, SelectorKey, lookup};
use vole_document::field::ingest::{IngestReport, ingest_pdf};
use vole_document::field::node::NodeKind;
use vole_document::field::observe::{ObserveRequest, Representation, Selector, observe};
use vole_document::field::provenance::{AnswerValue, FieldAnswer};
use vole_document::field::{Field, FieldId, FieldStore};
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::store::NodeId;

// ---------------------------------------------------------------------------
// Dependency-free `.msg` (MS-OXMSG in CFBF) writer (test ground truth)
// ---------------------------------------------------------------------------

const SIG: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const ENDOFCHAIN: u32 = 0xFFFF_FFFE;
const FATSECT: u32 = 0xFFFF_FFFD;
const FREESECT: u32 = 0xFFFF_FFFF;
const NOSTREAM: u32 = 0xFFFF_FFFF;
const MINI_SS: usize = 64;
const SECTOR: usize = 512;
const MINI_CUTOFF: u32 = 4096;

const OBJ_STORAGE: u8 = 1;
const OBJ_STREAM: u8 = 2;
const OBJ_ROOT: u8 = 5;

/// A directory entry to write (a stream or a storage with children).
enum Ent {
    Stream(String, Vec<u8>),
    Storage(String, Vec<Ent>),
}

struct DirNode {
    name: String,
    obj_type: u8,
    start: u32,
    size: u64,
    child: u32,
    right: u32,
}

fn utf16le(name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for u in name.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

fn pad64(d: &[u8]) -> Vec<u8> {
    let mut v = d.to_vec();
    let r = v.len() % MINI_SS;
    if r != 0 {
        v.resize(v.len() + (MINI_SS - r), 0);
    }
    v
}

fn dir_entry_bytes(n: &DirNode) -> [u8; 128] {
    let mut e = [0u8; 128];
    let name = utf16le(&n.name);
    assert!(name.len() <= 64, "CFBF name is too long: {}", n.name);
    e[..name.len()].copy_from_slice(&name);
    e[64..66].copy_from_slice(&(name.len() as u16).to_le_bytes());
    e[66] = n.obj_type;
    e[67] = 1; // black (a right-chain has no balancing requirement for our layout)
    e[68..72].copy_from_slice(&NOSTREAM.to_le_bytes()); // left
    e[72..76].copy_from_slice(&n.right.to_le_bytes());
    e[76..80].copy_from_slice(&n.child.to_le_bytes());
    e[116..120].copy_from_slice(&n.start.to_le_bytes());
    e[120..128].copy_from_slice(&n.size.to_le_bytes());
    e
}

/// Assign directory indices (pre-order sibling blocks) to `entries`, laying every
/// stream's content into the mini stream. Returns the first sibling's index.
fn build_nodes(entries: &[Ent], out: &mut Vec<DirNode>, mini: &mut Vec<u8>) -> u32 {
    if entries.is_empty() {
        return NOSTREAM;
    }
    let start = out.len() as u32;
    for _ in entries {
        out.push(DirNode {
            name: String::new(),
            obj_type: 0,
            start: 0,
            size: 0,
            child: NOSTREAM,
            right: NOSTREAM,
        });
    }
    for (k, e) in entries.iter().enumerate() {
        let idx = (start as usize) + k;
        let right = if k + 1 < entries.len() {
            start + k as u32 + 1
        } else {
            NOSTREAM
        };
        out[idx] = match e {
            Ent::Stream(name, data) => {
                let s = (mini.len() / MINI_SS) as u32;
                mini.extend_from_slice(&pad64(data));
                DirNode {
                    name: name.clone(),
                    obj_type: OBJ_STREAM,
                    start: s,
                    size: data.len() as u64,
                    child: NOSTREAM,
                    right,
                }
            }
            Ent::Storage(name, children) => {
                let child = build_nodes(children, out, mini);
                DirNode {
                    name: name.clone(),
                    obj_type: OBJ_STORAGE,
                    start: 0,
                    size: 0,
                    child,
                    right,
                }
            }
        };
    }
    start
}

/// Build a CFBF v3 compound file whose streams all live in the root mini stream.
fn build_cfbf(root_children: &[Ent]) -> Vec<u8> {
    let epf = SECTOR / 4;
    let epd = SECTOR / 128;

    let mut nodes: Vec<DirNode> = Vec::new();
    nodes.push(DirNode {
        name: String::new(),
        obj_type: OBJ_ROOT,
        start: 0,
        size: 0,
        child: NOSTREAM,
        right: NOSTREAM,
    });
    let mut mini: Vec<u8> = Vec::new();
    let child = build_nodes(root_children, &mut nodes, &mut mini);
    nodes[0] = DirNode {
        name: "Root Entry".to_string(),
        obj_type: OBJ_ROOT,
        start: 0,
        size: mini.len() as u64,
        child,
        right: NOSTREAM,
    };

    let mini_secs = mini.len() / MINI_SS;
    let minifat_secs = mini_secs.div_ceil(epf);
    let mut minifat = vec![FREESECT; minifat_secs * epf];
    for n in &nodes {
        if n.obj_type != OBJ_STREAM || n.size == 0 {
            continue;
        }
        let count = (n.size as usize).div_ceil(MINI_SS);
        for j in 0..count {
            minifat[n.start as usize + j] = if j + 1 < count {
                n.start + j as u32 + 1
            } else {
                ENDOFCHAIN
            };
        }
    }

    let mini_stream_secs = mini.len().div_ceil(SECTOR).max(1);
    let dir_secs = nodes.len().div_ceil(epd);
    let fat_secs = 1usize;

    let mini_stream_start = 0usize;
    let minifat_start = mini_stream_secs;
    let dir_start = minifat_start + minifat_secs;
    let fat_start = dir_start + dir_secs;
    let total = fat_start + fat_secs;
    assert!(total <= epf, "fixture is too large for one FAT sector");

    let mut fat = vec![FREESECT; epf];
    for i in 0..mini_stream_secs {
        fat[mini_stream_start + i] = if i + 1 < mini_stream_secs {
            (mini_stream_start + i + 1) as u32
        } else {
            ENDOFCHAIN
        };
    }
    for i in 0..minifat_secs {
        fat[minifat_start + i] = if i + 1 < minifat_secs {
            (minifat_start + i + 1) as u32
        } else {
            ENDOFCHAIN
        };
    }
    for i in 0..dir_secs {
        fat[dir_start + i] = if i + 1 < dir_secs {
            (dir_start + i + 1) as u32
        } else {
            ENDOFCHAIN
        };
    }
    for i in 0..fat_secs {
        fat[fat_start + i] = FATSECT;
    }

    let mut out = vec![0u8; SECTOR];
    out[..8].copy_from_slice(&SIG);
    out[24..26].copy_from_slice(&0x003Eu16.to_le_bytes());
    out[26..28].copy_from_slice(&3u16.to_le_bytes());
    out[28..30].copy_from_slice(&0xFFFEu16.to_le_bytes());
    out[30..32].copy_from_slice(&9u16.to_le_bytes());
    out[32..34].copy_from_slice(&6u16.to_le_bytes());
    out[40..44].copy_from_slice(&0u32.to_le_bytes());
    out[44..48].copy_from_slice(&(fat_secs as u32).to_le_bytes());
    out[48..52].copy_from_slice(&(dir_start as u32).to_le_bytes());
    out[56..60].copy_from_slice(&MINI_CUTOFF.to_le_bytes());
    out[60..64].copy_from_slice(&(minifat_start as u32).to_le_bytes());
    out[64..68].copy_from_slice(&(minifat_secs as u32).to_le_bytes());
    out[68..72].copy_from_slice(&ENDOFCHAIN.to_le_bytes());
    out[72..76].copy_from_slice(&0u32.to_le_bytes());
    out[76..80].copy_from_slice(&(fat_start as u32).to_le_bytes());
    for i in 1..109 {
        out[76 + i * 4..80 + i * 4].copy_from_slice(&FREESECT.to_le_bytes());
    }

    let mut sectors: Vec<Vec<u8>> = Vec::new();
    for i in 0..mini_stream_secs {
        let mut s = vec![0u8; SECTOR];
        let off = i * SECTOR;
        let end = (off + SECTOR).min(mini.len());
        if end > off {
            s[..end - off].copy_from_slice(&mini[off..end]);
        }
        sectors.push(s);
    }
    for sec in 0..minifat_secs {
        let mut s = vec![0u8; SECTOR];
        for k in 0..epf {
            let idx = sec * epf + k;
            if idx >= minifat.len() {
                break;
            }
            s[k * 4..k * 4 + 4].copy_from_slice(&minifat[idx].to_le_bytes());
        }
        sectors.push(s);
    }
    for sec in 0..dir_secs {
        let mut s = vec![0u8; SECTOR];
        for k in 0..epd {
            let idx = sec * epd + k;
            if idx >= nodes.len() {
                break;
            }
            let off = k * 128;
            s[off..off + 128].copy_from_slice(&dir_entry_bytes(&nodes[idx]));
        }
        sectors.push(s);
    }
    for sec in 0..fat_secs {
        let mut s = vec![0u8; SECTOR];
        for k in 0..epf {
            let v = fat[sec * epf + k];
            s[k * 4..k * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        sectors.push(s);
    }
    for s in &sectors {
        out.extend_from_slice(s);
    }
    out
}

// ---------------------------------------------------------------------------
// MS-OXMSG writer
// ---------------------------------------------------------------------------

const STREAM_PROPERTIES: &str = "__properties_version1.0";
const STREAM_NAMEID: &str = "__nameid_version1.0";

const PT_UNICODE: u16 = 0x001F;
const PT_STRING8: u16 = 0x001E;
const PT_BINARY: u16 = 0x0102;
const PT_SYSTIME: u16 = 0x0040;

const FLAG_VARIABLE: u32 = 0x0000_0004;
const FLAG_FIXED: u32 = 0x0000_0000;

/// A 16-byte fixed-size property entry.
fn entry(tag: u32, flags: u32, value: [u8; 8]) -> [u8; 16] {
    let mut e = [0u8; 16];
    e[0..4].copy_from_slice(&tag.to_le_bytes());
    e[4..8].copy_from_slice(&flags.to_le_bytes());
    e[8..16].copy_from_slice(&value);
    e
}

/// A variable-size property entry whose 8-byte value field holds the substream size.
fn var_entry(prop_type: u16, id: u16, size: u32) -> [u8; 16] {
    let tag = ((id as u32) << 16) | prop_type as u32;
    let mut value = [0u8; 8];
    value[0..4].copy_from_slice(&size.to_le_bytes());
    entry(tag, FLAG_VARIABLE, value)
}

/// A fixed-size property entry with a raw 8-byte value.
fn fixed_entry(prop_type: u16, id: u16, value: [u8; 8]) -> [u8; 16] {
    let tag = ((id as u32) << 16) | prop_type as u32;
    entry(tag, FLAG_FIXED, value)
}

fn substream_name(prop_type: u16, id: u16) -> String {
    format!("__substg1.0_{id:04X}{prop_type:04X}")
}

fn utf16_units(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for u in s.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

const SUBJECT: &str = "Hello Ω";
const BODY: &str = "Body text\nSecond line";
const SENDER_ANSI: &[u8] = b"Caf\xE9 \x80"; // CP1252 for "Café €"
const SENDER_DECODED: &str = "Café €";
const MESSAGE_CLASS: &str = "IPM.Note";
const BINARY_VALUE: &[u8] = b"\x00\x01\x02\xFF\xFE";
const RECIP_DISPLAY: &str = "Carol <carol@example.test>";
const ATTACH_SHORT: &[u8] = b"file.txt";
const ATTACH_LONG: &str = "long file.txt";
const ATTACH_MIME: &[u8] = b"text/plain";
const ATTACH_DATA: &[u8] = b"\xCA\xFE\xBA\xBE";

/// The fixture options (all default to a *valid* message).
#[derive(Clone, Copy, Default)]
struct Opts {
    /// The message `__properties_version1.0` is truncated below its 32-byte header.
    short_props: bool,
    /// Append 4 stray bytes, so a fixed property entry overruns the stream.
    entry_overrun: bool,
    /// A root substream name is not `PPPPTTTT` hex.
    bad_substream_name: bool,
    /// Add a root substream that no fixed entry references.
    unreferenced_substream: bool,
    /// Add a referenced root substream with an unsupported property type (PT_SYSTIME).
    unsupported_substream: bool,
    /// The header declares 2 recipients but only one storage exists.
    count_mismatch: bool,
    /// The recipient storage omits its own `__properties_version1.0`.
    recip_no_props: bool,
    /// The recipient storage name suffix is not decimal.
    bad_recip_suffix: bool,
    /// Omit the root `__properties_version1.0` (so detection is `cfbf`).
    missing_props: bool,
}

fn message_props(o: &Opts) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    // Header: Reserved(8), NextRecipientId, NextAttachmentId, RecipientCount,
    // AttachmentCount, Reserved(8).
    out.extend_from_slice(&[0u8; 8]);
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    let recip_count: u32 = if o.count_mismatch { 2 } else { 1 };
    out.extend_from_slice(&recip_count.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&[0u8; 8]);
    // Fixed-size property entries.
    out.extend_from_slice(&var_entry(
        PT_UNICODE,
        0x001A,
        MESSAGE_CLASS.len() as u32 * 2,
    ));
    out.extend_from_slice(&var_entry(
        PT_UNICODE,
        0x0037,
        SUBJECT.encode_utf16().count() as u32 * 2,
    ));
    out.extend_from_slice(&var_entry(
        PT_UNICODE,
        0x1000,
        BODY.encode_utf16().count() as u32 * 2,
    ));
    out.extend_from_slice(&var_entry(PT_STRING8, 0x0C1A, SENDER_ANSI.len() as u32));
    out.extend_from_slice(&var_entry(PT_BINARY, 0x1234, BINARY_VALUE.len() as u32));
    out.extend_from_slice(&fixed_entry(PT_SYSTIME, 0x0039, [0x11; 8]));
    if o.short_props {
        out.truncate(24);
    }
    if o.entry_overrun {
        out.extend_from_slice(&[0u8; 4]);
    }
    out
}

fn recipient_props() -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&[0u8; 8]); // Reserved(8)
    out.extend_from_slice(&var_entry(
        PT_UNICODE,
        0x0E04,
        RECIP_DISPLAY.encode_utf16().count() as u32 * 2,
    ));
    out.extend_from_slice(&fixed_entry(
        0x0003,
        0x0C15,
        [0x01, 0x00, 0x00, 0x00, 0, 0, 0, 0],
    ));
    out
}

fn attachment_props() -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&[0u8; 8]); // Reserved(8)
    out.extend_from_slice(&var_entry(PT_STRING8, 0x3704, ATTACH_SHORT.len() as u32));
    out.extend_from_slice(&var_entry(
        PT_UNICODE,
        0x3707,
        ATTACH_LONG.encode_utf16().count() as u32 * 2,
    ));
    out.extend_from_slice(&var_entry(PT_STRING8, 0x370E, ATTACH_MIME.len() as u32));
    out.extend_from_slice(&var_entry(PT_BINARY, 0x3701, ATTACH_DATA.len() as u32));
    out
}

fn root_children(o: &Opts) -> Vec<Ent> {
    let mut v: Vec<Ent> = Vec::new();
    if !o.missing_props {
        v.push(Ent::Stream(STREAM_PROPERTIES.to_string(), message_props(o)));
    }
    v.push(Ent::Stream(
        STREAM_NAMEID.to_string(),
        b"\x01\x00\x00\x00nameid".to_vec(),
    ));
    // Variable-size substreams (unicode, ANSI, binary).
    v.push(Ent::Stream(
        substream_name(PT_UNICODE, 0x001A),
        utf16_units(MESSAGE_CLASS),
    ));
    v.push(Ent::Stream(
        substream_name(PT_UNICODE, 0x0037),
        utf16_units(SUBJECT),
    ));
    v.push(Ent::Stream(
        substream_name(PT_UNICODE, 0x1000),
        utf16_units(BODY),
    ));
    v.push(Ent::Stream(
        substream_name(PT_STRING8, 0x0C1A),
        SENDER_ANSI.to_vec(),
    ));
    v.push(Ent::Stream(
        substream_name(PT_BINARY, 0x1234),
        BINARY_VALUE.to_vec(),
    ));
    if o.bad_substream_name {
        v.push(Ent::Stream(
            "__substg1.0_ZZZZ001F".to_string(),
            utf16_units("bad name"),
        ));
    }
    if o.unreferenced_substream {
        v.push(Ent::Stream(
            substream_name(PT_UNICODE, 0x9999),
            utf16_units("unreferenced"),
        ));
    }
    if o.unsupported_substream {
        // 0x00390040 is a referenced fixed entry (PT_SYSTIME) but its type is not
        // decodable as a substream.
        v.push(Ent::Stream(
            substream_name(PT_SYSTIME, 0x0039),
            vec![0u8; 8],
        ));
    }
    // One recipient sub-storage.
    let recip_name = if o.bad_recip_suffix {
        "__recip_version1.0_x".to_string()
    } else {
        "__recip_version1.0_1".to_string()
    };
    let mut recip_children: Vec<Ent> = Vec::new();
    if !o.recip_no_props {
        recip_children.push(Ent::Stream(
            STREAM_PROPERTIES.to_string(),
            recipient_props(),
        ));
    }
    recip_children.push(Ent::Stream(
        substream_name(PT_UNICODE, 0x0E04),
        utf16_units(RECIP_DISPLAY),
    ));
    v.push(Ent::Storage(recip_name, recip_children));
    // One attachment sub-storage.
    let attach_children: Vec<Ent> = vec![
        Ent::Stream(STREAM_PROPERTIES.to_string(), attachment_props()),
        Ent::Stream(substream_name(PT_STRING8, 0x3704), ATTACH_SHORT.to_vec()),
        Ent::Stream(substream_name(PT_UNICODE, 0x3707), utf16_units(ATTACH_LONG)),
        Ent::Stream(substream_name(PT_STRING8, 0x370E), ATTACH_MIME.to_vec()),
        Ent::Stream(substream_name(PT_BINARY, 0x3701), ATTACH_DATA.to_vec()),
    ];
    v.push(Ent::Storage(
        "__attach_version1.0_1".to_string(),
        attach_children,
    ));
    v
}

fn build_msg(o: &Opts) -> Vec<u8> {
    build_cfbf(&root_children(o))
}

fn build_valid() -> Vec<u8> {
    build_msg(&Opts::default())
}

/// A generic compound file with no root `__properties_version1.0`.
fn build_generic_cfbf() -> Vec<u8> {
    build_cfbf(&[
        Ent::Stream("Data".to_string(), b"not a message".to_vec()),
        Ent::Stream("Meta".to_string(), b"generic compound file".to_vec()),
    ])
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-msg-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn opaque_descriptor(source: &[u8]) -> Vec<u8> {
    let d = Descriptor {
        universe: vole_document::container::UNIVERSE.to_string(),
        source_format: vole_document::SOURCE_FORMAT_OPAQUE,
        format_basis: "opaque:msg-test".to_string(),
        models: vec![],
        channels: vec![],
        objects: vec![ObjectSource::Inline(source.to_vec())],
        program: Program::new(vec![Op::EmitObject { object_id: 0 }]),
        observation_index: None,
        seek_directory: false,
        checkpoints: None,
        source_sha256: sha256(source),
        source_len: source.len() as u64,
    };
    d.serialize().unwrap().0
}

struct Fixture {
    root: PathBuf,
    store: FieldStore,
    report: IngestReport,
    source: Vec<u8>,
}

impl Fixture {
    fn new(label: &str, source: &[u8]) -> Fixture {
        let root = temp_dir(label);
        let mut store = FieldStore::open(&root).unwrap();
        let descriptor = opaque_descriptor(source);
        let report = ingest_pdf(&mut store, &descriptor, Limits::DEFAULT).unwrap();
        Fixture {
            root,
            store,
            report,
            source: source.to_vec(),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

fn observe_ok(
    store: &mut FieldStore,
    field: &FieldId,
    selector: Selector,
    representation: Representation,
) -> FieldAnswer {
    let req = ObserveRequest::new(selector, representation);
    observe(store, field, &req, Limits::DEFAULT).unwrap().0
}

fn observe_err(
    store: &mut FieldStore,
    field: &FieldId,
    selector: Selector,
    representation: Representation,
) -> ErrorClass {
    let req = ObserveRequest::new(selector, representation);
    observe(store, field, &req, Limits::DEFAULT)
        .unwrap_err()
        .class()
}

fn answer_text(answer: &FieldAnswer) -> String {
    match &answer.value {
        AnswerValue::Text(t) => t.clone(),
        other => panic!("expected text, got {other:?}"),
    }
}

fn answer_json(answer: &FieldAnswer) -> String {
    match &answer.value {
        AnswerValue::Json(j) => j.clone(),
        other => panic!("expected json, got {other:?}"),
    }
}

fn answer_bytes(answer: &FieldAnswer) -> Vec<u8> {
    match &answer.value {
        AnswerValue::Bytes(b) => b.clone(),
        other => panic!("expected bytes, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    // A real `.msg` is `Msg`.
    assert_eq!(
        detect_document_format(&build_valid(), Limits::DEFAULT),
        DocumentFormat::Msg
    );
    // A generic compound file (no root `__properties_version1.0`) → `Cfbf`.
    assert_eq!(
        detect_document_format(&build_generic_cfbf(), Limits::DEFAULT),
        DocumentFormat::Cfbf
    );
    // An OLE file whose root property stream is absent → `Cfbf`.
    assert_eq!(
        detect_document_format(
            &build_msg(&Opts {
                missing_props: true,
                ..Opts::default()
            }),
            Limits::DEFAULT
        ),
        DocumentFormat::Cfbf
    );
    // Prose → `Opaque`.
    assert_eq!(
        detect_document_format(
            b"The quick brown fox jumps over the lazy dog.\nPlain prose.\n",
            Limits::DEFAULT
        ),
        DocumentFormat::Opaque
    );
    // A `.pptx` (an OPC ZIP) never reaches the CFBF detector.
    let zip = b"PK\x03\x04\x14\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
    assert_ne!(
        detect_document_format(zip, Limits::DEFAULT),
        DocumentFormat::Msg
    );
}

#[test]
fn capabilities_name_the_native_selectors() {
    let caps = vole_document::field::capabilities::capabilities_for_format(DocumentFormat::Msg);
    for n in [
        "msg-summary",
        "msg-property",
        "msg-recipient",
        "msg-attachment",
        "msg-find",
    ] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    for c in ["metadata", "text", "search-match"] {
        assert!(
            caps.selectors.iter().any(|s| s.selector == c),
            "missing common {c}"
        );
    }
}

#[test]
fn message_properties_and_substreams_are_preserved() {
    let src = build_valid();
    let mut fx = Fixture::new("surface", &src);
    let field = fx.report.field;

    // The native `msg-summary` text is the stable version name.
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::MsgSummary,
            Representation::Text,
        )),
        "outlook-msg-ms-oxmsg"
    );

    // Common metadata names the format, the stream, and the inventory.
    let common = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(common.contains("\"format\":\"msg\""), "{common}");
    assert!(
        common.contains("\"stream\":\"__properties_version1.0\""),
        "{common}"
    );
    assert!(
        common.contains("\"version\":\"outlook-msg-ms-oxmsg\""),
        "{common}"
    );
    assert!(common.contains("\"nameid\":true"), "{common}");
    assert!(common.contains("\"properties\":6"), "{common}");
    assert!(common.contains("\"substreams\":10"), "{common}");
    assert!(common.contains("\"recipients\":1"), "{common}");
    assert!(common.contains("\"attachments\":1"), "{common}");
    assert!(common.contains("\"text_units\":99"), "{common}");
    assert!(common.contains("\"next_recipient_id\":1"), "{common}");
    assert!(common.contains("\"next_attachment_id\":1"), "{common}");
    assert!(common.contains("\"recipient_count\":1"), "{common}");
    assert!(common.contains("\"attachment_count\":1"), "{common}");
    assert!(common.contains("\"property_span\":["), "{common}");

    // Fixed-size property entries are preserved in order with ids, tags, flags,
    // value bytes, and exact spans.
    let expected = [
        (
            0x001A001F_u32,
            26u32,
            "message-class",
            31u32,
            "unicode",
            4u32,
            "1000000000000000",
        ),
        (
            0x0037001F_u32,
            55,
            "subject",
            31,
            "unicode",
            4,
            "0e00000000000000",
        ),
        (
            0x1000001F_u32,
            4096,
            "body",
            31,
            "unicode",
            4,
            "2a00000000000000",
        ),
        (
            0x0C1A001E_u32,
            3098,
            "sender-name",
            30,
            "string8",
            4,
            "0600000000000000",
        ),
        (
            0x12340102_u32,
            4660,
            "unknown",
            258,
            "binary",
            4,
            "0500000000000000",
        ),
        (
            0x00390040_u32,
            57,
            "client-submit-time",
            64,
            "systime",
            0,
            "1111111111111111",
        ),
    ];
    for (i, (tag, id, name, ty, ty_name, flags, value_hex)) in expected.iter().enumerate() {
        let p = answer_json(&observe_ok(
            &mut fx.store,
            &field,
            Selector::MsgProperty { index: i as u32 },
            Representation::Metadata,
        ));
        assert!(p.contains(&format!("\"index\":{i}")), "{p}");
        assert!(p.contains(&format!("\"tag\":{tag}")), "{p}");
        assert!(p.contains(&format!("\"tag_hex\":\"0x{tag:08X}\"")), "{p}");
        assert!(p.contains(&format!("\"id\":{id}")), "{p}");
        assert!(p.contains(&format!("\"property\":\"{name}\"")), "{p}");
        assert!(p.contains(&format!("\"type\":{ty}")), "{p}");
        assert!(p.contains(&format!("\"type_name\":\"{ty_name}\"")), "{p}");
        assert!(p.contains(&format!("\"flags\":{flags}")), "{p}");
        assert!(p.contains(&format!("\"value_hex\":\"{value_hex}\"")), "{p}");
        assert!(p.contains("\"owner\":0"), "{p}");
        // The `Text` representation is the well-known property name.
        assert_eq!(
            answer_text(&observe_ok(
                &mut fx.store,
                &field,
                Selector::MsgProperty { index: i as u32 },
                Representation::Text,
            )),
            *name
        );
        // `ExactBytes` returns the exact 16-byte entry.
        let bytes = answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            Selector::MsgProperty { index: i as u32 },
            Representation::ExactBytes,
        ));
        assert_eq!(bytes.len(), 16, "property {i} entry length");
        let mut want = [0u8; 16];
        want[0..4].copy_from_slice(&tag.to_le_bytes());
        want[4..8].copy_from_slice(&flags.to_le_bytes());
        want[8..16].copy_from_slice(&(value_hex_as_u64(value_hex)).to_le_bytes());
        assert_eq!(bytes, want, "property {i} exact bytes");
    }

    // The substreams preserve decoded Unicode / ANSI / binary values.
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.contains(MESSAGE_CLASS), "{text}");
    assert!(text.contains(SUBJECT), "{text}");
    assert!(text.contains(BODY), "{text}");
    assert!(text.contains(SENDER_DECODED), "{text}");
    assert!(text.contains(RECIP_DISPLAY), "{text}");
    assert!(text.contains(ATTACH_LONG), "{text}");

    // The recipient sub-storage is preserved with its id, index, properties, and
    // decoded substream text.
    let recip = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::MsgRecipient { index: 0 },
        Representation::Metadata,
    ));
    assert!(recip.contains("\"kind\":\"recipient\""), "{recip}");
    assert!(recip.contains("\"index\":0"), "{recip}");
    assert!(recip.contains("\"id\":1"), "{recip}");
    assert!(recip.contains("\"properties\":2"), "{recip}");
    assert!(recip.contains("\"substreams\":[{"), "{recip}");
    assert!(recip.contains(RECIP_DISPLAY), "{recip}");
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::MsgRecipient { index: 0 },
            Representation::Text,
        )),
        RECIP_DISPLAY
    );
    let recip_entry = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        Selector::MsgRecipient { index: 0 },
        Representation::ExactBytes,
    ));
    assert_eq!(
        recip_entry.len(),
        128,
        "storage directory entry is 128 bytes"
    );

    // The attachment sub-storage is preserved with its ids, properties, and
    // substreams (ANSI, Unicode, binary).
    let attach = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::MsgAttachment { index: 0 },
        Representation::Metadata,
    ));
    assert!(attach.contains("\"kind\":\"attachment\""), "{attach}");
    assert!(attach.contains("\"id\":1"), "{attach}");
    assert!(attach.contains("\"properties\":4"), "{attach}");
    assert!(
        attach.contains("\"stream\":\"__substg1.0_3707001F\""),
        "{attach}"
    );
    assert!(attach.contains("\"type_name\":\"binary\""), "{attach}");
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::MsgAttachment { index: 0 },
            Representation::Text,
        )),
        "file.txt\nlong file.txt\ntext/plain"
    );

    // Substream tags and types are preserved (the global ordinal order is
    // top-level, then recipients, then attachments).
    let s1 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::MsgFind {
            pattern: "Hello".to_string(),
        },
        Representation::Metadata,
    ));
    assert!(s1.contains("\"field\":\"substream\""), "{s1}");
    assert!(s1.contains("\"index\":1"), "{s1}");
    assert!(s1.contains("\"owner\":0"), "{s1}");
    assert!(s1.contains(SUBJECT), "{s1}");

    // An out-of-range property/recipient/attachment index declines typed.
    for sel in [
        Selector::MsgProperty { index: 6 },
        Selector::MsgRecipient { index: 1 },
        Selector::MsgAttachment { index: 9 },
    ] {
        assert_eq!(
            observe_err(&mut fx.store, &field, sel, Representation::Metadata),
            ErrorClass::UnsupportedFeature
        );
    }

    // The common `search-match` agrees with `msg-find`.
    let common_find = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch(SENDER_DECODED.to_string()),
        Representation::Text,
    ));
    assert!(common_find.contains("\"matches\":[{"), "{common_find}");
    assert!(common_find.contains(SENDER_DECODED), "{common_find}");
}

/// The little-endian value a `value_hex` string denotes.
fn value_hex_as_u64(hex: &str) -> u64 {
    let mut bytes = [0u8; 8];
    for (i, b) in bytes.iter_mut().enumerate() {
        let hi = u8::from_str_radix(&hex[i * 2..i * 2 + 1], 16).unwrap();
        let lo = u8::from_str_radix(&hex[i * 2 + 1..i * 2 + 2], 16).unwrap();
        *b = (hi << 4) | lo;
    }
    u64::from_le_bytes(bytes)
}

#[test]
fn model_encoding_round_trips() {
    let src = build_valid();
    let model = vole_document::adapter::msg::parse(&src, Limits::DEFAULT).unwrap();
    let bytes = model.encode();
    let back = vole_document::adapter::msg::MsgModel::decode(&bytes).unwrap();
    assert_eq!(model, back);
    let mut bad = bytes.clone();
    bad[0] = b'X';
    assert!(vole_document::adapter::msg::MsgModel::decode(&bad).is_err());
}

#[test]
fn malformed_inputs_decline_typed() {
    for (kind, label) in [
        (
            Opts {
                short_props: true,
                ..Opts::default()
            },
            "short-props",
        ),
        (
            Opts {
                entry_overrun: true,
                ..Opts::default()
            },
            "entry-overrun",
        ),
        (
            Opts {
                bad_substream_name: true,
                ..Opts::default()
            },
            "bad-substream-name",
        ),
        (
            Opts {
                unreferenced_substream: true,
                ..Opts::default()
            },
            "unreferenced-substream",
        ),
        (
            Opts {
                unsupported_substream: true,
                ..Opts::default()
            },
            "unsupported-substream",
        ),
        (
            Opts {
                count_mismatch: true,
                ..Opts::default()
            },
            "count-mismatch",
        ),
        (
            Opts {
                recip_no_props: true,
                ..Opts::default()
            },
            "recip-no-props",
        ),
        (
            Opts {
                bad_recip_suffix: true,
                ..Opts::default()
            },
            "bad-recip-suffix",
        ),
    ] {
        let src = build_msg(&kind);
        assert_eq!(
            detect_document_format(&src, Limits::DEFAULT),
            DocumentFormat::Msg,
            "{label}"
        );
        let e = vole_document::adapter::msg::parse(&src, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), ErrorClass::InvalidMsgStructure, "{label}");
    }

    // A `.msg` without a root property stream is not detected as `msg` at all.
    assert_ne!(
        detect_document_format(
            &build_msg(&Opts {
                missing_props: true,
                ..Opts::default()
            }),
            Limits::DEFAULT
        ),
        DocumentFormat::Msg
    );
}

#[test]
fn exact_materialization_is_byte_identical() {
    let src = build_valid();
    let fx = Fixture::new("exact", &src);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let out = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(out.len(), src.len());
    assert_eq!(sha256(&out), sha256(&src));
    assert_eq!(out, fx.source);
}

#[test]
fn removal_then_fresh_process_is_queryable_and_exact() {
    let root = temp_dir("removal");
    let src_path = root.join("message.msg");
    let desc_path = root.join("message.voldoc");
    let source = build_valid();
    std::fs::write(&src_path, &source).unwrap();
    let descriptor = opaque_descriptor(&source);
    std::fs::write(&desc_path, &descriptor).unwrap();

    let store_root = root.join("store");
    let field_id = {
        let mut store = FieldStore::open(&store_root).unwrap();
        let report = ingest_pdf(&mut store, &descriptor, Limits::DEFAULT).unwrap();
        report.field
    };

    std::fs::remove_file(&src_path).unwrap();
    std::fs::remove_file(&desc_path).unwrap();

    let mut store2 = FieldStore::open(&store_root).unwrap();
    assert_eq!(
        answer_text(&observe_ok(
            &mut store2,
            &field_id,
            Selector::MsgRecipient { index: 0 },
            Representation::Text,
        )),
        RECIP_DISPLAY
    );
    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), source.len());
    assert_eq!(sha256(&exact), sha256(&source));
    assert_eq!(exact, source);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn msg_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived MSG model reads the source bytes, so its single dependency
    // must be the exact `DocumentExact` root (keyed by `sha256(source)`).
    let source = build_valid();
    let fx = Fixture::new("adr0060", &source);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_MSG_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::MsgModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the msg model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0x4D53_4726_2056_4F4C;
    for _ in 0..256 {
        let mut buf = vec![0u8; 4096];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        let _ = vole_document::adapter::msg::detect(&buf, Limits::STRICT);
        let _ = vole_document::adapter::msg::parse(&buf, Limits::STRICT);
    }
    // The CFBF-signature-prefixed path is the most likely false-positive route.
    for _ in 0..256 {
        let mut buf = SIG.to_vec();
        buf.resize(4096, 0);
        for b in buf[8..].iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        let _ = vole_document::adapter::msg::parse(&buf, Limits::STRICT);
    }
}
