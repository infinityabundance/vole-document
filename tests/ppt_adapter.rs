//! Phase 26.3 court: the legacy PowerPoint 97–2003 (`.ppt`, MS-PPT) semantic adapter
//! built on the CFBF/OLE2 physical layer.
//!
//! Gated on `ppt`, so a build without the feature compiles an empty target.
//!
//! A `.ppt` has no package layer: the exact leaf is the whole source (a
//! `DocumentExact`) and every derived observation is a bounded, representation-
//! preserving projection of it. The court checks:
//!
//! 1. **Byte-based detection** and the boundaries: a `.ppt` → `Ppt`; a generic
//!    compound file → `Cfbf`; a `PowerPoint Document` stream whose first record is not
//!    a `Document` → `Cfbf`; prose → `Opaque`.
//! 2. **Representation preservation**: the MS-PPT record stream (each record's
//!    `recVer`/`recInstance`/`recType`/`recLen`, exact span, stream offset, and
//!    nesting depth), the `Document` container, the `SlideListWithText` slides (with
//!    their `persistIdRef`, in order), and the `TextCharsAtom` (UTF-16LE) /
//!    `TextBytesAtom` (CP1252) runs with their exact spans and decoded text.
//! 3. **Selectors**: native `ppt-document`/`ppt-slide`/`ppt-run`/`ppt-record`/
//!    `ppt-find` plus common `metadata`/`text`/`search-match`.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted.
//! 5. **Fail closed**: malformed inputs decline typed (a bad `Document` record, an
//!    overrunning `recLen`, a truncated record header, a text-atom/`recLen` mismatch,
//!    an out-of-range `Current User` offset); random bytes never panic.
//! 6. **No cross-field aliasing** (ADR-0060): the model node depends on the exact
//!    `sha256(source)` root.

#![cfg(all(feature = "field", feature = "ppt"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_PPT_MODEL, SelectorKey, lookup};
use vole_document::field::ingest::{IngestReport, ingest_pdf};
use vole_document::field::node::NodeKind;
use vole_document::field::observe::{ObserveRequest, Representation, Selector, observe};
use vole_document::field::provenance::{AnswerValue, FieldAnswer};
use vole_document::field::{Field, FieldId, FieldStore};
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::store::NodeId;

// ---------------------------------------------------------------------------
// Dependency-free `.ppt` (MS-PPT record stream in CFBF) writer (test ground truth)
// ---------------------------------------------------------------------------

const SIG: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const ENDOFCHAIN: u32 = 0xFFFF_FFFE;
const FATSECT: u32 = 0xFFFF_FFFD;
const FREESECT: u32 = 0xFFFF_FFFF;
const NOSTREAM: u32 = 0xFFFF_FFFF;
const MINI_SS: usize = 64;
const SECTOR: usize = 512;
const MINI_CUTOFF: u32 = 4096;

const REC_VER_CONTAINER: u8 = 0x0F;
const REC_DOCUMENT: u16 = 1000;
const REC_END_DOCUMENT: u16 = 1001;
const REC_SLIDE_PERSIST_ATOM: u16 = 1011;
const REC_TEXT_HEADER_ATOM: u16 = 3999;
const REC_TEXT_CHARS_ATOM: u16 = 4000;
const REC_TEXT_BYTES_ATOM: u16 = 4008;
const REC_USER_EDIT_ATOM: u16 = 4085;
const REC_SLIDE_LIST_WITH_TEXT: u16 = 4080;

const CHARS_TEXT: &str = "Hello Ω";
const BYTES_TEXT: &[u8] = b"Caf\xE9 \x80";
/// The decoded form of `BYTES_TEXT` under CP1252.
const BYTES_DECODED: &str = "Café €";

/// Phase 27.2 aux-list fixture texts.
const MASTER_TEXT: &str = "Master Title";
const NOTES_CHARS: &str = "Speaker notes";
/// CP1252 bytes for "Pöints" (`ö` = 0xF6).
const NOTES_BYTES: &[u8] = b"P\xF6ints";
/// The decoded form of `NOTES_BYTES` under CP1252.
const NOTES_BYTES_DECODED: &str = "Pöints";
const HANDOUT_TEXT: &str = "Handout footer";
/// A non-conformant `other` list (`recInstance` 4, which MS-PPT does not define).
const OTHER_TEXT: &str = "Other list text";

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

#[allow(clippy::too_many_arguments)]
fn dir_entry(
    name: &str,
    obj_type: u8,
    color: u8,
    left: u32,
    right: u32,
    child: u32,
    start: u32,
    size: u64,
) -> [u8; 128] {
    let mut e = [0u8; 128];
    let n = utf16le(name);
    assert!(n.len() <= 64);
    e[..n.len()].copy_from_slice(&n);
    e[64..66].copy_from_slice(&(n.len() as u16).to_le_bytes());
    e[66] = obj_type;
    e[67] = color;
    e[68..72].copy_from_slice(&left.to_le_bytes());
    e[72..76].copy_from_slice(&right.to_le_bytes());
    e[76..80].copy_from_slice(&child.to_le_bytes());
    e[116..120].copy_from_slice(&start.to_le_bytes());
    e[120..128].copy_from_slice(&size.to_le_bytes());
    e
}

fn build_cfbf(streams: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let epf = SECTOR / 4;
    let epd = SECTOR / 128;

    let mut mini_stream: Vec<u8> = Vec::new();
    let mut starts: Vec<usize> = Vec::new();
    for (_, data) in streams {
        assert!(data.len() < MINI_CUTOFF as usize);
        starts.push(mini_stream.len() / MINI_SS);
        mini_stream.extend_from_slice(&pad64(data));
    }
    let mini_secs = mini_stream.len() / MINI_SS;
    let minifat_secs = mini_secs.div_ceil(epf);
    let mut minifat = vec![FREESECT; minifat_secs * epf];
    for (k, (_, data)) in streams.iter().enumerate() {
        let count = pad64(data).len() / MINI_SS;
        let s = starts[k];
        for j in 0..count {
            minifat[s + j] = if j + 1 < count {
                (s + j + 1) as u32
            } else {
                ENDOFCHAIN
            };
        }
    }

    let mini_stream_secs = mini_stream.len().div_ceil(SECTOR).max(1);
    let dir_count = 1 + streams.len();
    let dir_secs = dir_count.div_ceil(epd);
    let fat_secs = 1usize;

    let mini_stream_start = 0usize;
    let minifat_start = mini_stream_secs;
    let dir_start = minifat_start + minifat_secs;
    let fat_start = dir_start + dir_secs;
    let total = fat_start + fat_secs;
    assert!(total <= epf);

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
        let end = (off + SECTOR).min(mini_stream.len());
        s[..end - off].copy_from_slice(&mini_stream[off..end]);
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
    let mut dirs: Vec<[u8; 128]> = Vec::new();
    dirs.push(dir_entry(
        "Root Entry",
        5,
        1,
        NOSTREAM,
        NOSTREAM,
        if streams.is_empty() { NOSTREAM } else { 1 },
        mini_stream_start as u32,
        mini_stream.len() as u64,
    ));
    for (k, (name, data)) in streams.iter().enumerate() {
        let right = if k + 1 < streams.len() {
            (k + 2) as u32
        } else {
            NOSTREAM
        };
        dirs.push(dir_entry(
            name,
            2,
            (k % 2) as u8,
            NOSTREAM,
            right,
            NOSTREAM,
            starts[k] as u32,
            data.len() as u64,
        ));
    }
    for sec in 0..dir_secs {
        let mut s = vec![0u8; SECTOR];
        for k in 0..epd {
            let idx = sec * epd + k;
            if idx >= dirs.len() {
                break;
            }
            let off = k * 128;
            s[off..off + 128].copy_from_slice(&dirs[idx]);
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

fn rec(out: &mut Vec<u8>, rec_ver: u8, rec_instance: u16, rec_type: u16, payload: &[u8]) {
    let w: u16 = (rec_ver as u16) | (rec_instance << 4);
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&rec_type.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
}

fn utf16_units(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for u in s.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

fn slide_persist_atom(persist_id_ref: u32, c_texts: u32) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&persist_id_ref.to_le_bytes());
    p.extend_from_slice(&c_texts.to_le_bytes());
    p.extend_from_slice(&[0u8; 8]);
    p
}

#[derive(Clone, Copy, Default)]
struct Opts {
    bad_first_record: bool,
    bad_document_len: bool,
    overrun_document: bool,
    odd_chars_atom: bool,
    trailing_truncated: bool,
    bad_current_user: bool,
    no_current_user: bool,
    bad_aux_list: bool,
}

fn powerpoint_document(o: &Opts) -> Vec<u8> {
    let mut doc_payload: Vec<u8> = Vec::new();
    let mut slwt: Vec<u8> = Vec::new();
    rec(
        &mut slwt,
        0,
        0,
        REC_SLIDE_PERSIST_ATOM,
        &slide_persist_atom(1, 1),
    );
    rec(&mut slwt, 0, 0, REC_TEXT_HEADER_ATOM, &[0u8; 4]);
    let mut chars = utf16_units(CHARS_TEXT);
    if o.odd_chars_atom {
        chars.push(0);
    }
    rec(&mut slwt, 0, 0, REC_TEXT_CHARS_ATOM, &chars);
    rec(
        &mut slwt,
        0,
        0,
        REC_SLIDE_PERSIST_ATOM,
        &slide_persist_atom(2, 1),
    );
    rec(&mut slwt, 0, 0, REC_TEXT_HEADER_ATOM, &[0u8; 4]);
    rec(&mut slwt, 0, 0, REC_TEXT_BYTES_ATOM, BYTES_TEXT);

    rec(
        &mut doc_payload,
        REC_VER_CONTAINER,
        0,
        REC_SLIDE_LIST_WITH_TEXT,
        &slwt,
    );
    if o.bad_aux_list {
        // A notes list (recInstance 2) whose TextCharsAtom payload length is odd: it was
        // previously skipped, and is now modeled, so it declines typed (Phase 27.2).
        let mut notes: Vec<u8> = Vec::new();
        rec(
            &mut notes,
            0,
            0,
            REC_SLIDE_PERSIST_ATOM,
            &slide_persist_atom(9, 1),
        );
        rec(&mut notes, 0, 0, REC_TEXT_HEADER_ATOM, &[0u8; 4]);
        let mut odd = utf16_units("odd");
        odd.push(0);
        rec(&mut notes, 0, 0, REC_TEXT_CHARS_ATOM, &odd);
        rec(
            &mut doc_payload,
            REC_VER_CONTAINER,
            2,
            REC_SLIDE_LIST_WITH_TEXT,
            &notes,
        );
    }
    rec(&mut doc_payload, 0, 0, REC_END_DOCUMENT, &[]);

    let mut stream: Vec<u8> = Vec::new();
    let (rec_ver, rec_type) = if o.bad_first_record {
        (0u8, REC_END_DOCUMENT)
    } else {
        (REC_VER_CONTAINER, REC_DOCUMENT)
    };
    let w: u16 = rec_ver as u16;
    let doc_len: u32 = if o.overrun_document {
        0xFFFF_FFFF
    } else if o.bad_document_len {
        4
    } else {
        doc_payload.len() as u32
    };
    stream.extend_from_slice(&w.to_le_bytes());
    stream.extend_from_slice(&rec_type.to_le_bytes());
    stream.extend_from_slice(&doc_len.to_le_bytes());
    stream.extend_from_slice(&doc_payload);
    if o.trailing_truncated {
        stream.extend_from_slice(&[0u8, 0u8, 0u8]);
    }
    stream
}

fn current_user(bad: bool) -> Vec<u8> {
    let mut s: Vec<u8> = Vec::new();
    let offset: u32 = if bad { 1000 } else { 8 };
    s.extend_from_slice(&offset.to_le_bytes());
    s.extend_from_slice(&[0u8; 4]);
    s.extend_from_slice(&0u16.to_le_bytes());
    s.extend_from_slice(&REC_USER_EDIT_ATOM.to_le_bytes());
    s.extend_from_slice(&0u32.to_le_bytes());
    s
}

fn build_ppt(o: &Opts) -> Vec<u8> {
    let doc = powerpoint_document(o);
    if o.no_current_user {
        build_cfbf(&[("PowerPoint Document", doc)])
    } else {
        build_cfbf(&[
            ("PowerPoint Document", doc),
            ("Current User", current_user(o.bad_current_user)),
        ])
    }
}

fn build_valid() -> Vec<u8> {
    build_ppt(&Opts::default())
}

/// Build a `Document` stream carrying all four `SlideListWithText` lists, each with
/// text runs (`masters`/`notes`/`handout` were previously skipped).
fn lists_document() -> Vec<u8> {
    let mut doc: Vec<u8> = Vec::new();
    // slides list (recInstance 0): persist 1 (UTF-16LE) + persist 2 (CP1252).
    let mut slwt: Vec<u8> = Vec::new();
    rec(
        &mut slwt,
        0,
        0,
        REC_SLIDE_PERSIST_ATOM,
        &slide_persist_atom(1, 1),
    );
    rec(&mut slwt, 0, 0, REC_TEXT_HEADER_ATOM, &[0u8; 4]);
    rec(
        &mut slwt,
        0,
        0,
        REC_TEXT_CHARS_ATOM,
        &utf16_units(CHARS_TEXT),
    );
    rec(
        &mut slwt,
        0,
        0,
        REC_SLIDE_PERSIST_ATOM,
        &slide_persist_atom(2, 1),
    );
    rec(&mut slwt, 0, 0, REC_TEXT_HEADER_ATOM, &[0u8; 4]);
    rec(&mut slwt, 0, 0, REC_TEXT_BYTES_ATOM, BYTES_TEXT);
    rec(
        &mut doc,
        REC_VER_CONTAINER,
        0,
        REC_SLIDE_LIST_WITH_TEXT,
        &slwt,
    );
    // masters list (recInstance 1): one slide with a UTF-16LE run.
    let mut slwt: Vec<u8> = Vec::new();
    rec(
        &mut slwt,
        0,
        0,
        REC_SLIDE_PERSIST_ATOM,
        &slide_persist_atom(3, 1),
    );
    rec(&mut slwt, 0, 0, REC_TEXT_HEADER_ATOM, &[0u8; 4]);
    rec(
        &mut slwt,
        0,
        0,
        REC_TEXT_CHARS_ATOM,
        &utf16_units(MASTER_TEXT),
    );
    rec(
        &mut doc,
        REC_VER_CONTAINER,
        1,
        REC_SLIDE_LIST_WITH_TEXT,
        &slwt,
    );
    // notes list (recInstance 2): one slide with two runs (UTF-16LE then CP1252).
    let mut slwt: Vec<u8> = Vec::new();
    rec(
        &mut slwt,
        0,
        0,
        REC_SLIDE_PERSIST_ATOM,
        &slide_persist_atom(4, 2),
    );
    rec(&mut slwt, 0, 0, REC_TEXT_HEADER_ATOM, &[0u8; 4]);
    rec(
        &mut slwt,
        0,
        0,
        REC_TEXT_CHARS_ATOM,
        &utf16_units(NOTES_CHARS),
    );
    rec(&mut slwt, 0, 0, REC_TEXT_HEADER_ATOM, &[0u8; 4]);
    rec(&mut slwt, 0, 0, REC_TEXT_BYTES_ATOM, NOTES_BYTES);
    rec(
        &mut doc,
        REC_VER_CONTAINER,
        2,
        REC_SLIDE_LIST_WITH_TEXT,
        &slwt,
    );
    // handout list (recInstance 3): one slide with a UTF-16LE run.
    let mut slwt: Vec<u8> = Vec::new();
    rec(
        &mut slwt,
        0,
        0,
        REC_SLIDE_PERSIST_ATOM,
        &slide_persist_atom(5, 1),
    );
    rec(&mut slwt, 0, 0, REC_TEXT_HEADER_ATOM, &[0u8; 4]);
    rec(
        &mut slwt,
        0,
        0,
        REC_TEXT_CHARS_ATOM,
        &utf16_units(HANDOUT_TEXT),
    );
    rec(
        &mut doc,
        REC_VER_CONTAINER,
        3,
        REC_SLIDE_LIST_WITH_TEXT,
        &slwt,
    );
    // other list (recInstance 4): a non-conformant instance, materialized losslessly
    // and addressable as `other:4`.
    let mut slwt: Vec<u8> = Vec::new();
    rec(
        &mut slwt,
        0,
        0,
        REC_SLIDE_PERSIST_ATOM,
        &slide_persist_atom(6, 1),
    );
    rec(&mut slwt, 0, 0, REC_TEXT_HEADER_ATOM, &[0u8; 4]);
    rec(
        &mut slwt,
        0,
        0,
        REC_TEXT_CHARS_ATOM,
        &utf16_units(OTHER_TEXT),
    );
    rec(
        &mut doc,
        REC_VER_CONTAINER,
        4,
        REC_SLIDE_LIST_WITH_TEXT,
        &slwt,
    );
    rec(&mut doc, 0, 0, REC_END_DOCUMENT, &[]);

    let mut stream: Vec<u8> = Vec::new();
    stream.extend_from_slice(&(REC_VER_CONTAINER as u16).to_le_bytes());
    stream.extend_from_slice(&REC_DOCUMENT.to_le_bytes());
    stream.extend_from_slice(&(doc.len() as u32).to_le_bytes());
    stream.extend_from_slice(&doc);
    stream
}

fn build_lists() -> Vec<u8> {
    build_cfbf(&[
        ("PowerPoint Document", lists_document()),
        ("Current User", current_user(false)),
    ])
}

/// A `Document` stream with the slides list and a **present but empty** masters list
/// (a `SlideListWithText` container with no children).
fn empty_list_document() -> Vec<u8> {
    let mut doc: Vec<u8> = Vec::new();
    let mut slwt: Vec<u8> = Vec::new();
    rec(
        &mut slwt,
        0,
        0,
        REC_SLIDE_PERSIST_ATOM,
        &slide_persist_atom(1, 1),
    );
    rec(&mut slwt, 0, 0, REC_TEXT_HEADER_ATOM, &[0u8; 4]);
    rec(
        &mut slwt,
        0,
        0,
        REC_TEXT_CHARS_ATOM,
        &utf16_units(CHARS_TEXT),
    );
    rec(
        &mut doc,
        REC_VER_CONTAINER,
        0,
        REC_SLIDE_LIST_WITH_TEXT,
        &slwt,
    );
    // An empty masters list container (recVer 0xF, recLen 0).
    rec(
        &mut doc,
        REC_VER_CONTAINER,
        1,
        REC_SLIDE_LIST_WITH_TEXT,
        &[],
    );
    rec(&mut doc, 0, 0, REC_END_DOCUMENT, &[]);
    let mut stream: Vec<u8> = Vec::new();
    stream.extend_from_slice(&(REC_VER_CONTAINER as u16).to_le_bytes());
    stream.extend_from_slice(&REC_DOCUMENT.to_le_bytes());
    stream.extend_from_slice(&(doc.len() as u32).to_le_bytes());
    stream.extend_from_slice(&doc);
    stream
}

fn build_empty_list() -> Vec<u8> {
    build_cfbf(&[("PowerPoint Document", empty_list_document())])
}

fn build_generic_cfbf() -> Vec<u8> {
    build_cfbf(&[
        ("Data", b"not a powerpoint document".to_vec()),
        ("Meta", b"generic compound file".to_vec()),
    ])
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-ppt-{label}-{}-{}",
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
        format_basis: "opaque:ppt-test".to_string(),
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
    // A real `.ppt` is `Ppt`.
    assert_eq!(
        detect_document_format(&build_valid(), Limits::DEFAULT),
        DocumentFormat::Ppt
    );
    // A `PowerPoint Document` stream whose first record is not a `Document` → `Cfbf`.
    assert_eq!(
        detect_document_format(
            &build_ppt(&Opts {
                bad_first_record: true,
                ..Opts::default()
            }),
            Limits::DEFAULT
        ),
        DocumentFormat::Cfbf
    );
    // A generic compound file (no PowerPoint Document stream) → `Cfbf`.
    assert_eq!(
        detect_document_format(&build_generic_cfbf(), Limits::DEFAULT),
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
        DocumentFormat::Ppt
    );
}

#[test]
fn capabilities_name_the_native_selectors() {
    let caps = vole_document::field::capabilities::capabilities_for_format(DocumentFormat::Ppt);
    for n in [
        "ppt-document",
        "ppt-slide",
        "ppt-run",
        "ppt-record",
        "ppt-list",
        "ppt-find",
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
fn document_slides_runs_and_records_are_preserved() {
    let src = build_valid();
    let mut fx = Fixture::new("surface", &src);
    let field = fx.report.field;

    // Common metadata names the format, the stream, and the inventory.
    let common = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(common.contains("\"format\":\"ppt\""), "{common}");
    assert!(
        common.contains("\"stream\":\"PowerPoint Document\""),
        "{common}"
    );
    assert!(common.contains("\"records\":9"), "{common}");
    assert!(common.contains("\"slides\":2"), "{common}");
    assert!(common.contains("\"runs\":2"), "{common}");
    assert!(common.contains("\"chars\":1"), "{common}");
    assert!(common.contains("\"bytes\":1"), "{common}");
    assert!(common.contains("\"current_user\":true"), "{common}");

    // The native `ppt-document` selector reports the recorded version.
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::PptDocument,
            Representation::Text,
        )),
        "powerpoint97-2003"
    );

    // Slide order and persist ids are preserved.
    let s0 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::PptSlide { index: 0 },
        Representation::Metadata,
    ));
    assert!(s0.contains("\"persist_id_ref\":1"), "{s0}");
    assert!(s0.contains("\"list\":\"slides\""), "{s0}");
    assert!(s0.contains("\"runs\":[0,1]"), "{s0}");
    let s1 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::PptSlide { index: 1 },
        Representation::Metadata,
    ));
    assert!(s1.contains("\"persist_id_ref\":2"), "{s1}");
    assert!(s1.contains("\"runs\":[1,2]"), "{s1}");
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::PptSlide { index: 1 },
            Representation::Text,
        )),
        BYTES_DECODED
    );

    // Run order, kind, and decoded text are preserved (UTF-16LE and CP1252).
    let r0 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::PptRun { index: 0 },
        Representation::Metadata,
    ));
    assert!(r0.contains("\"kind\":\"chars\""), "{r0}");
    assert!(r0.contains("\"slide\":0"), "{r0}");
    assert!(r0.contains("\"type\":\"text-chars-atom\""), "{r0}");
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::PptRun { index: 0 },
            Representation::Text,
        )),
        CHARS_TEXT
    );
    let r1 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::PptRun { index: 1 },
        Representation::Metadata,
    ));
    assert!(r1.contains("\"kind\":\"bytes\""), "{r1}");
    assert!(r1.contains("\"slide\":1"), "{r1}");
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::PptRun { index: 1 },
            Representation::Text,
        )),
        BYTES_DECODED
    );

    // An out-of-range slide/run/record index declines typed.
    for sel in [
        Selector::PptSlide { index: 9 },
        Selector::PptRun { index: 99 },
        Selector::PptRecord { index: 9_999 },
    ] {
        assert_eq!(
            observe_err(&mut fx.store, &field, sel, Representation::Metadata),
            ErrorClass::UnsupportedFeature
        );
    }

    // The record stream exposes exact ids, depths, and spans: record 0 is the
    // `Document` container, record 1 the `SlideListWithText`, record 2 the first
    // `SlidePersistAtom`.
    let rec0 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::PptRecord { index: 0 },
        Representation::Metadata,
    ));
    assert!(rec0.contains("\"id_hex\":\"0x03E8\""), "{rec0}");
    assert!(rec0.contains("\"type\":\"document\""), "{rec0}");
    assert!(rec0.contains("\"rec_ver\":15"), "{rec0}");
    assert!(rec0.contains("\"depth\":0"), "{rec0}");
    let rec1 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::PptRecord { index: 1 },
        Representation::Metadata,
    ));
    assert!(rec1.contains("\"id_hex\":\"0x0FF0\""), "{rec1}");
    assert!(rec1.contains("\"type\":\"slide-list-with-text\""), "{rec1}");
    assert!(rec1.contains("\"depth\":1"), "{rec1}");
    let rec2 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::PptRecord { index: 2 },
        Representation::Metadata,
    ));
    assert!(rec2.contains("\"id_hex\":\"0x03F3\""), "{rec2}");
    assert!(rec2.contains("\"depth\":2"), "{rec2}");
    let rec_bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        Selector::PptRecord { index: 0 },
        Representation::ExactBytes,
    ));
    // The Document header (recVer 0xF, recType 1000) is `0F 00 E8 03`.
    assert_eq!(&rec_bytes[..4], &[0x0F, 0x00, 0xE8, 0x03]);

    // `ppt-find` finds a run; the common search-match agrees.
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::PptFind {
            pattern: "Hello".to_string(),
        },
        Representation::Metadata,
    ));
    assert!(found.contains("\"field\":\"run\""), "{found}");
    assert!(found.contains(CHARS_TEXT), "{found}");
    let common_find = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch(BYTES_DECODED.to_string()),
        Representation::Text,
    ));
    assert!(common_find.contains("\"matches\":[{"), "{common_find}");

    // The common text carries both run texts.
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.contains(CHARS_TEXT), "{text}");
    assert!(text.contains(BYTES_DECODED), "{text}");
}

#[test]
fn lists_are_materialized_as_typed_answers() {
    let src = build_lists();
    let mut fx = Fixture::new("lists", &src);
    let field = fx.report.field;

    // The common metadata reports the four modeled lists.
    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"lists\":5"), "{meta}");
    // The slides-list projection is unchanged by the wider model.
    assert!(meta.contains("\"slides\":2"), "{meta}");
    assert!(meta.contains("\"runs\":2"), "{meta}");

    // The slides list (recInstance 0) keeps its Phase-26.3 shape.
    let slides = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::PptList { instance: 0 },
        Representation::Metadata,
    ));
    assert!(slides.contains("\"list\":\"slides\""), "{slides}");
    assert!(slides.contains("\"rec_instance\":0"), "{slides}");
    assert!(slides.contains("\"empty\":false"), "{slides}");
    assert!(slides.contains("\"persist_id_ref\":1"), "{slides}");
    assert!(slides.contains("\"persist_id_ref\":2"), "{slides}");
    let slides_text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::PptList { instance: 0 },
        Representation::Text,
    ));
    assert!(slides_text.contains(CHARS_TEXT), "{slides_text}");
    assert!(slides_text.contains(BYTES_DECODED), "{slides_text}");

    // The masters list.
    let masters = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::PptList { instance: 1 },
        Representation::Metadata,
    ));
    assert!(masters.contains("\"list\":\"masters\""), "{masters}");
    assert!(masters.contains("\"rec_instance\":1"), "{masters}");
    assert!(masters.contains("\"persist_id_ref\":3"), "{masters}");
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::PptList { instance: 1 },
            Representation::Text,
        )),
        MASTER_TEXT
    );

    // The notes list: one slide, two runs in order (UTF-16LE then CP1252).
    let notes = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::PptList { instance: 2 },
        Representation::Metadata,
    ));
    assert!(notes.contains("\"list\":\"notes\""), "{notes}");
    assert!(notes.contains("\"persist_id_ref\":4"), "{notes}");
    assert!(notes.contains("\"kind\":\"chars\""), "{notes}");
    assert!(notes.contains("\"kind\":\"bytes\""), "{notes}");
    let chars_at = notes.find(NOTES_CHARS).expect("chars run");
    let bytes_at = notes.find(NOTES_BYTES_DECODED).expect("bytes run");
    assert!(chars_at < bytes_at, "notes run order: {notes}");
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::PptList { instance: 2 },
            Representation::Text,
        )),
        format!("{NOTES_CHARS}\n{NOTES_BYTES_DECODED}")
    );

    // The handout list.
    let handout = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::PptList { instance: 3 },
        Representation::Metadata,
    ));
    assert!(handout.contains("\"list\":\"handout\""), "{handout}");
    assert!(handout.contains("\"rec_instance\":3"), "{handout}");
    assert!(handout.contains("\"persist_id_ref\":5"), "{handout}");

    // The non-conformant `other:4` list is named unambiguously and selectable.
    let other = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::PptList { instance: 4 },
        Representation::Metadata,
    ));
    assert!(other.contains("\"list\":\"other:4\""), "{other}");
    assert!(other.contains("\"rec_instance\":4"), "{other}");
    assert!(other.contains("\"persist_id_ref\":6"), "{other}");
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::PptList { instance: 4 },
            Representation::Text,
        )),
        OTHER_TEXT
    );

    // An **absent** list is a typed empty answer, never an error.
    let plain = build_valid();
    let mut px = Fixture::new("lists-absent", &plain);
    let pfield = px.report.field;
    let absent = answer_json(&observe_ok(
        &mut px.store,
        &pfield,
        Selector::PptList { instance: 1 },
        Representation::Metadata,
    ));
    assert!(absent.contains("\"empty\":true"), "{absent}");
    assert!(absent.contains("\"slides\":0"), "{absent}");
    assert!(absent.contains("\"runs\":0"), "{absent}");
    assert!(absent.contains("\"list\":\"masters\""), "{absent}");

    // A **present but empty** list is also a typed empty answer.
    let esrc = build_empty_list();
    let mut ex = Fixture::new("lists-present-empty", &esrc);
    let efield = ex.report.field;
    let present_empty = answer_json(&observe_ok(
        &mut ex.store,
        &efield,
        Selector::PptList { instance: 1 },
        Representation::Metadata,
    ));
    assert!(present_empty.contains("\"empty\":true"), "{present_empty}");
    assert!(present_empty.contains("\"slides\":0"), "{present_empty}");
}

#[test]
fn list_runs_preserve_exact_spans_and_order() {
    let src = build_lists();
    let model = vole_document::adapter::ppt::parse(&src, Limits::DEFAULT).unwrap();

    // All lists are modeled, in record order (including the non-conformant `other:4`).
    let instances: Vec<u16> = model.lists.iter().map(|l| l.instance).collect();
    assert_eq!(instances, vec![0, 1, 2, 3, 4]);
    // The container spans are in stream order and enclose their children.
    for w in model.lists.windows(2) {
        assert!(w[0].span.1 <= w[1].span.0, "list spans are ordered");
    }

    // The legacy slides projection is exactly the slides list.
    assert_eq!(model.slides.len(), 2);
    assert_eq!(model.runs.len(), 2);
    assert!(model.slides.iter().all(|s| s.list_instance == 0));

    // The masters list: one slide, one UTF-16LE run whose span points at the exact
    // text-atom record (header + payload) in the source.
    let masters = model.list(1).expect("masters list");
    assert_eq!(masters.slides.len(), 1);
    assert_eq!(masters.slides[0].persist_id_ref, 3);
    let runs = masters.slide_runs(&masters.slides[0]);
    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    assert_eq!(run.kind, vole_document::adapter::ppt::RUN_CHARS);
    assert_eq!(run.rec_type, REC_TEXT_CHARS_ATOM);
    assert!(run.header_span.1 <= run.span.0, "header precedes the atom");
    // The header span is a TextHeaderAtom record.
    let hs = run.header_span.0 as usize;
    assert_eq!(
        u16::from_le_bytes([src[hs + 2], src[hs + 3]]),
        REC_TEXT_HEADER_ATOM
    );
    // The atom span is the exact record (8-byte header + UTF-16LE payload).
    let s0 = run.span.0 as usize;
    assert_eq!(
        u16::from_le_bytes([src[s0 + 2], src[s0 + 3]]),
        REC_TEXT_CHARS_ATOM
    );
    let payload = &src[s0 + 8..run.span.1 as usize];
    assert_eq!(payload, utf16_units(MASTER_TEXT).as_slice());
    assert_eq!(String::from_utf16_lossy(&run.text), MASTER_TEXT);

    // The notes list: two runs in order, the second a CP1252 `TextBytesAtom`.
    let notes = model.list(2).expect("notes list");
    let nruns = notes.slide_runs(&notes.slides[0]);
    assert_eq!(nruns.len(), 2);
    assert_eq!(nruns[0].kind, vole_document::adapter::ppt::RUN_CHARS);
    assert_eq!(nruns[1].kind, vole_document::adapter::ppt::RUN_BYTES);
    let bs = nruns[1].span.0 as usize;
    assert_eq!(
        u16::from_le_bytes([src[bs + 2], src[bs + 3]]),
        REC_TEXT_BYTES_ATOM
    );
    assert_eq!(&src[bs + 8..nruns[1].span.1 as usize], NOTES_BYTES);
    assert_eq!(
        String::from_utf16_lossy(&nruns[1].text),
        NOTES_BYTES_DECODED
    );
    // Run spans strictly increase within the list.
    assert!(nruns[0].span.1 <= nruns[1].span.0);

    // The handout list.
    let handout = model.list(3).expect("handout list");
    assert_eq!(handout.slides.len(), 1);
    assert_eq!(handout.slides[0].persist_id_ref, 5);
    assert_eq!(
        String::from_utf16_lossy(&handout.slide_runs(&handout.slides[0])[0].text),
        HANDOUT_TEXT
    );
}

#[test]
fn model_encoding_round_trips() {
    let src = build_valid();
    let model = vole_document::adapter::ppt::parse(&src, Limits::DEFAULT).unwrap();
    let bytes = model.encode();
    let back = vole_document::adapter::ppt::PptModel::decode(&bytes).unwrap();
    assert_eq!(model, back);
    let mut bad = bytes.clone();
    bad[0] = b'X';
    assert!(vole_document::adapter::ppt::PptModel::decode(&bad).is_err());
}

#[test]
fn malformed_inputs_decline_typed() {
    for (kind, label) in [
        (
            Opts {
                bad_document_len: true,
                ..Opts::default()
            },
            "bad-document",
        ),
        (
            Opts {
                overrun_document: true,
                ..Opts::default()
            },
            "overrun",
        ),
        (
            Opts {
                odd_chars_atom: true,
                ..Opts::default()
            },
            "text-mismatch",
        ),
        (
            Opts {
                trailing_truncated: true,
                ..Opts::default()
            },
            "truncated",
        ),
        (
            Opts {
                bad_current_user: true,
                ..Opts::default()
            },
            "bad-current-user",
        ),
        (
            Opts {
                bad_aux_list: true,
                ..Opts::default()
            },
            "bad-aux-list",
        ),
    ] {
        let src = build_ppt(&kind);
        assert_eq!(
            detect_document_format(&src, Limits::DEFAULT),
            DocumentFormat::Ppt,
            "{label}"
        );
        let e = vole_document::adapter::ppt::parse(&src, Limits::DEFAULT).unwrap_err();
        assert_eq!(e.class(), ErrorClass::InvalidPptStructure, "{label}");
    }

    // A bad first record is not detected as `ppt` at all.
    assert_ne!(
        detect_document_format(
            &build_ppt(&Opts {
                bad_first_record: true,
                ..Opts::default()
            }),
            Limits::DEFAULT
        ),
        DocumentFormat::Ppt
    );

    // A valid `.ppt` without a `Current User` stream still parses.
    let ok = build_ppt(&Opts {
        no_current_user: true,
        ..Opts::default()
    });
    assert_eq!(
        detect_document_format(&ok, Limits::DEFAULT),
        DocumentFormat::Ppt
    );
    let m = vole_document::adapter::ppt::parse(&ok, Limits::DEFAULT).unwrap();
    assert!(m.current_user.is_none());
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
    let src_path = root.join("deck.ppt");
    let desc_path = root.join("deck.voldoc");
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
            Selector::PptRun { index: 1 },
            Representation::Text,
        )),
        BYTES_DECODED
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
fn ppt_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived PPT model reads the source bytes, so its single dependency
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
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_PPT_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::PptModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the ppt model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0x5050_5426_2056_4F4C;
    for _ in 0..256 {
        let mut buf = vec![0u8; 4096];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        let _ = vole_document::adapter::ppt::detect(&buf, Limits::STRICT);
        let _ = vole_document::adapter::ppt::parse(&buf, Limits::STRICT);
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
        let _ = vole_document::adapter::ppt::parse(&buf, Limits::STRICT);
    }
}
