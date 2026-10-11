//! Phase 21.35 court: the CFBF / OLE2 compound-file physical-layer adapter.
//!
//! Gated on `cfbf`, so a build without the feature compiles an empty target.
//!
//! A compound file has no package layer: the exact leaf is the whole source (a
//! `DocumentExact`) and every derived observation is a bounded, representation-
//! preserving projection of it. The court checks:
//!
//! 1. **Byte-based detection** and the boundaries: a valid compound file →
//!    `Cfbf`; a signature-only blob, a corrupted-signature blob, a bad-sector-shift
//!    blob, and prose stay `Opaque`.
//! 2. **Representation preservation**: the directory entries with their verbatim
//!    UTF-16LE names (incl. non-ASCII), object types, sibling/child tree, and exact
//!    spans; a FAT-chained large stream (>= 4096 B) and a MiniFAT-chained small
//!    stream (< 4096 B) resolved to exact byte spans; a 4096-byte-sector (v4) file.
//! 3. **Selectors**: native `cfbf-dialect`/`cfbf-entry`/`cfbf-stream`/`cfbf-find`
//!    plus common `metadata`/`text`/`search-match`.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 5. **Fail closed**: malformed inputs decline typed (invalid type, cyclic FAT,
//!    truncated stream); random bytes never panic.
//! 6. **No cross-field aliasing** (ADR-0060): the model node depends on the exact
//!    `sha256(source)` root.

#![cfg(all(feature = "field", feature = "cfbf"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_CFBF_MODEL, SelectorKey, lookup};
use vole_document::field::ingest::{IngestReport, ingest_pdf};
use vole_document::field::node::NodeKind;
use vole_document::field::observe::{ObserveRequest, Representation, Selector, observe};
use vole_document::field::provenance::{AnswerValue, FieldAnswer};
use vole_document::field::{Field, FieldId, FieldStore};
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::store::NodeId;

// ---------------------------------------------------------------------------
// Dependency-free CFBF writer (test ground truth)
// ---------------------------------------------------------------------------

const SIG: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const ENDOFCHAIN: u32 = 0xFFFF_FFFE;
const FATSECT: u32 = 0xFFFF_FFFD;
const FREESECT: u32 = 0xFFFF_FFFF;
const NOSTREAM: u32 = 0xFFFF_FFFF;
const MINI_SS: usize = 64;

fn utf16le(name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for u in name.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out.extend_from_slice(&0u16.to_le_bytes());
    out
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

fn pad64(d: &[u8]) -> Vec<u8> {
    let mut v = d.to_vec();
    let r = v.len() % MINI_SS;
    if r != 0 {
        v.resize(v.len() + (MINI_SS - r), 0);
    }
    v
}

/// The deterministic large-stream content (5000 bytes).
fn large_content() -> Vec<u8> {
    (0..5000u32).map(|i| (i % 251) as u8).collect()
}
/// The deterministic small-stream content (100 bytes).
fn small_content() -> Vec<u8> {
    (0..100u32).map(|i| (i * 7 % 253) as u8).collect()
}
/// The deterministic sub-storage stream content (40 bytes).
fn inner_content() -> Vec<u8> {
    (0..40u32).map(|i| (i * 13 % 251) as u8).collect()
}

/// Build the standard fixture with the given sector size (512 or 4096).
fn build(ss: u32) -> Vec<u8> {
    assert!(ss == 512 || ss == 4096);
    let shift: u16 = if ss == 512 { 9 } else { 12 };
    let major: u16 = if ss == 512 { 3 } else { 4 };
    let epf = (ss / 4) as usize;
    let epd = (ss / 128) as usize;

    let large_data = large_content();
    let small_data = small_content();
    let inner_data = inner_content();

    let small_padded = pad64(&small_data);
    let inner_padded = pad64(&inner_data);
    let small_secs = small_padded.len() / MINI_SS;
    let inner_start = small_secs as u32;
    let mut mini_stream = Vec::new();
    mini_stream.extend_from_slice(&small_padded);
    mini_stream.extend_from_slice(&inner_padded);

    let mut minifat = vec![FREESECT; epf];
    for (i, slot) in minifat.iter_mut().enumerate().take(small_secs) {
        *slot = if i + 1 < small_secs {
            (i + 1) as u32
        } else {
            ENDOFCHAIN
        };
    }
    minifat[small_secs] = ENDOFCHAIN;

    let large_secs = large_data.len().div_ceil(ss as usize);
    let dir_secs = 5usize.div_ceil(epd);

    let large_start = 0usize;
    let minifat_start = large_secs;
    let mini_stream_start = large_secs + 1;
    let dir_start = large_secs + 2;
    let fat_start = dir_start + dir_secs;
    let total = fat_start + 1;

    let mut fat = vec![FREESECT; epf];
    for i in 0..large_secs {
        fat[large_start + i] = if i + 1 < large_secs {
            (large_start + i + 1) as u32
        } else {
            ENDOFCHAIN
        };
    }
    fat[minifat_start] = ENDOFCHAIN;
    fat[mini_stream_start] = ENDOFCHAIN;
    for i in 0..dir_secs {
        fat[dir_start + i] = if i + 1 < dir_secs {
            (dir_start + i + 1) as u32
        } else {
            ENDOFCHAIN
        };
    }
    fat[fat_start] = FATSECT;

    let mut out = vec![0u8; ss as usize];
    out[..8].copy_from_slice(&SIG);
    out[24..26].copy_from_slice(&0x003Eu16.to_le_bytes());
    out[26..28].copy_from_slice(&major.to_le_bytes());
    out[28..30].copy_from_slice(&0xFFFEu16.to_le_bytes());
    out[30..32].copy_from_slice(&shift.to_le_bytes());
    out[32..34].copy_from_slice(&6u16.to_le_bytes());
    let ndir = if major == 3 { 0u32 } else { dir_secs as u32 };
    out[40..44].copy_from_slice(&ndir.to_le_bytes());
    out[44..48].copy_from_slice(&1u32.to_le_bytes());
    out[48..52].copy_from_slice(&(dir_start as u32).to_le_bytes());
    out[52..56].copy_from_slice(&0u32.to_le_bytes());
    out[56..60].copy_from_slice(&4096u32.to_le_bytes());
    out[60..64].copy_from_slice(&(minifat_start as u32).to_le_bytes());
    out[64..68].copy_from_slice(&1u32.to_le_bytes());
    out[68..72].copy_from_slice(&ENDOFCHAIN.to_le_bytes());
    out[72..76].copy_from_slice(&0u32.to_le_bytes());
    out[76..80].copy_from_slice(&(fat_start as u32).to_le_bytes());
    for i in 1..109 {
        out[76 + i * 4..80 + i * 4].copy_from_slice(&FREESECT.to_le_bytes());
    }

    let mut sectors: Vec<Vec<u8>> = Vec::with_capacity(total);
    for i in 0..large_secs {
        let mut s = vec![0u8; ss as usize];
        let off = i * ss as usize;
        let end = (off + ss as usize).min(large_data.len());
        s[..end - off].copy_from_slice(&large_data[off..end]);
        sectors.push(s);
    }
    {
        let mut s = vec![0u8; ss as usize];
        for (i, &v) in minifat.iter().enumerate() {
            s[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        sectors.push(s);
    }
    {
        let mut s = vec![0u8; ss as usize];
        s[..mini_stream.len()].copy_from_slice(&mini_stream);
        sectors.push(s);
    }
    let dirs = [
        dir_entry(
            "Root Entry",
            5,
            1,
            NOSTREAM,
            NOSTREAM,
            2,
            mini_stream_start as u32,
            mini_stream.len() as u64,
        ),
        dir_entry("Sub", 1, 0, NOSTREAM, NOSTREAM, 4, ENDOFCHAIN, 0),
        dir_entry(
            "Large",
            2,
            1,
            1,
            3,
            NOSTREAM,
            large_start as u32,
            large_data.len() as u64,
        ),
        dir_entry(
            "caf\u{e9}",
            2,
            0,
            NOSTREAM,
            NOSTREAM,
            NOSTREAM,
            0,
            small_data.len() as u64,
        ),
        dir_entry(
            "Inner",
            2,
            1,
            NOSTREAM,
            NOSTREAM,
            NOSTREAM,
            inner_start,
            inner_data.len() as u64,
        ),
    ];
    for sec in 0..dir_secs {
        let mut s = vec![0u8; ss as usize];
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
    {
        let mut s = vec![0u8; ss as usize];
        for (i, &v) in fat.iter().enumerate() {
            s[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        sectors.push(s);
    }
    for s in &sectors {
        out.extend_from_slice(s);
    }
    out
}

// The v3 fixture's known byte offsets (see `build`).
const V3_DIR_OFF: usize = 13 * 512;
const V3_FAT_OFF: usize = 15 * 512;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-cfbf-{label}-{}-{}",
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
        format_basis: "opaque:cfbf-test".to_string(),
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
    let v3 = build(512);
    assert_eq!(
        detect_document_format(&v3, Limits::DEFAULT),
        DocumentFormat::Cfbf
    );
    let v4 = build(4096);
    assert_eq!(
        detect_document_format(&v4, Limits::DEFAULT),
        DocumentFormat::Cfbf
    );

    // A signature-only blob (a zero header) is NOT a compound file.
    let mut sig_only = vec![0u8; 512];
    sig_only[..8].copy_from_slice(&SIG);
    assert_eq!(
        detect_document_format(&sig_only, Limits::DEFAULT),
        DocumentFormat::Opaque
    );

    // A corrupted signature is NOT a compound file.
    let mut badsig = v3.clone();
    badsig[0] = 0x00;
    assert_eq!(
        detect_document_format(&badsig, Limits::DEFAULT),
        DocumentFormat::Opaque
    );

    // A bad sector shift is NOT a compound file.
    let mut badshift = v3.clone();
    badshift[30..32].copy_from_slice(&7u16.to_le_bytes());
    assert_eq!(
        detect_document_format(&badshift, Limits::DEFAULT),
        DocumentFormat::Opaque
    );

    // An out-of-range first directory sector is NOT a compound file (detection
    // validates it).
    let mut bad_dir = v3.clone();
    bad_dir[48..52].copy_from_slice(&9999u32.to_le_bytes());
    assert_eq!(
        detect_document_format(&bad_dir, Limits::DEFAULT),
        DocumentFormat::Opaque
    );

    // Prose stays Opaque.
    assert_eq!(
        detect_document_format(
            b"The quick brown fox jumps over the lazy dog.\nPlain prose.\n",
            Limits::DEFAULT
        ),
        DocumentFormat::Opaque
    );
}

#[test]
fn v3_directory_streams_and_tree_are_preserved() {
    let src = build(512);
    let mut fx = Fixture::new("v3model", &src);
    let field = fx.report.field;

    // Dialect.
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::CfbfDialect,
            Representation::Text,
        )),
        "cfb3"
    );

    // Entry 0 is the root storage.
    let e0 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::CfbfEntry { index: 0 },
        Representation::Metadata,
    ));
    assert!(e0.contains("\"type\":\"root\""), "{e0}");
    assert!(e0.contains("\"depth\":0"), "{e0}");

    // Entry 1 (`Sub`) is a storage owned by the root.
    let e1 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::CfbfEntry { index: 1 },
        Representation::Metadata,
    ));
    assert!(e1.contains("\"type\":\"storage\""), "{e1}");
    assert!(e1.contains("\"name\":\"Sub\""), "{e1}");
    assert!(e1.contains("\"parent\":0"), "{e1}");

    // Entry 2 (`Large`) is a FAT-chained stream (>= 4096), depth 1.
    let e2 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::CfbfEntry { index: 2 },
        Representation::Metadata,
    ));
    assert!(e2.contains("\"type\":\"stream\""), "{e2}");
    assert!(e2.contains("\"stream_size\":5000"), "{e2}");
    assert!(e2.contains("\"mini\":false"), "{e2}");
    assert!(e2.contains("\"contiguous\":true"), "{e2}");
    assert!(e2.contains("\"depth\":1"), "{e2}");

    // ExactBytes of the entry is exactly one 128-byte directory entry starting with
    // the UTF-16LE name "Large".
    let raw2 = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        Selector::CfbfEntry { index: 2 },
        Representation::ExactBytes,
    ));
    assert_eq!(raw2.len(), 128);
    assert_eq!(&raw2[..10], &utf16le("Large")[..10]);

    // The large stream's exact content bytes are the FAT-chained data.
    let large = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        Selector::CfbfStream { index: 2 },
        Representation::ExactBytes,
    ));
    assert_eq!(large, large_content());

    // Entry 4 (`Inner`) is owned by `Sub` at depth 2.
    let e4 = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::CfbfEntry { index: 4 },
        Representation::Metadata,
    ));
    assert!(e4.contains("\"parent\":1"), "{e4}");
    assert!(e4.contains("\"depth\":2"), "{e4}");
    assert!(e4.contains("\"type\":\"stream\""), "{e4}");

    // The out-of-range index declines typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::CfbfEntry { index: 99 },
            Representation::Metadata
        ),
        ErrorClass::UnsupportedFeature
    );
    // A storage has no stream bytes.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::CfbfStream { index: 1 },
            Representation::ExactBytes
        ),
        ErrorClass::UnsupportedFeature
    );

    // Common surface.
    let common = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(common.contains("\"format\":\"cfbf\""), "{common}");
    assert!(common.contains("\"dialect\":\"cfb3\""), "{common}");
    assert!(common.contains("\"sector_size\":512"), "{common}");
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.contains("Large"), "{text}");
    assert!(text.contains("/Sub/Inner"), "{text}");
}

#[test]
fn small_streams_are_minifat_chained_and_exact() {
    let src = build(512);
    let mut fx = Fixture::new("mini", &src);
    let field = fx.report.field;

    // `café` (100 bytes) and `Sub/Inner` (40 bytes) are mini streams.
    for (index, expected, name) in [
        (3u32, small_content(), "caf"),
        (4u32, inner_content(), "Inner"),
    ] {
        let meta = answer_json(&observe_ok(
            &mut fx.store,
            &field,
            Selector::CfbfEntry { index },
            Representation::Metadata,
        ));
        assert!(meta.contains("\"mini\":true"), "{meta}");
        assert!(meta.contains("\"type\":\"stream\""), "{meta}");
        let bytes = answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            Selector::CfbfStream { index },
            Representation::ExactBytes,
        ));
        assert_eq!(bytes, expected, "stream {index}");
        // Text view names the stream (lossy UTF-8).
        let t = answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::CfbfEntry { index },
            Representation::Text,
        ));
        assert!(t.contains(name) || t.starts_with("caf"), "{t:?}");
    }
}

#[test]
fn non_ascii_utf16_name_is_preserved_verbatim() {
    let src = build(512);
    let mut fx = Fixture::new("nonascii", &src);
    let field = fx.report.field;

    // The name of entry 3 is `café` (UTF-16LE, incl. 0x00E9), preserved verbatim.
    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::CfbfEntry { index: 3 },
        Representation::Metadata,
    ));
    assert!(meta.contains("caf\u{e9}"), "{meta}");

    // Find matches the non-ASCII name.
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::CfbfFind {
            pattern: "caf".to_string(),
        },
        Representation::Metadata,
    ));
    assert!(found.contains("\"entry\":3"), "{found}");

    // The exact leaf is unchanged.
    let fh = Field::open(&fx.store, &field, Limits::DEFAULT).unwrap();
    assert_eq!(fh.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn v4_sector_file_is_parsed() {
    let src = build(4096);
    let mut fx = Fixture::new("v4", &src);
    let field = fx.report.field;
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::CfbfDialect,
            Representation::Text,
        )),
        "cfb4"
    );
    let common = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(common.contains("\"sector_size\":4096"), "{common}");
    assert!(common.contains("\"major\":4"), "{common}");
    let large = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        Selector::CfbfStream { index: 2 },
        Representation::ExactBytes,
    ));
    assert_eq!(large, large_content());
}

#[test]
fn malformed_inputs_decline_typed() {
    // An invalid directory object type (3) is admitted by detection but declines on
    // parse.
    let mut badtype = build(512);
    badtype[V3_DIR_OFF + 128 + 66] = 3;
    assert_eq!(
        detect_document_format(&badtype, Limits::DEFAULT),
        DocumentFormat::Cfbf
    );
    let e = vole_document::adapter::cfbf::parse(&badtype, Limits::DEFAULT).unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidCfbfStructure);

    // A cyclic FAT chain.
    let mut cyclic = build(512);
    cyclic[V3_FAT_OFF..V3_FAT_OFF + 4].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(
        detect_document_format(&cyclic, Limits::DEFAULT),
        DocumentFormat::Cfbf
    );
    let e = vole_document::adapter::cfbf::parse(&cyclic, Limits::DEFAULT).unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidCfbfStructure);

    // A truncated stream chain.
    let mut trunc = build(512);
    trunc[V3_FAT_OFF..V3_FAT_OFF + 4].copy_from_slice(&1u32.to_le_bytes());
    trunc[V3_FAT_OFF + 4..V3_FAT_OFF + 8].copy_from_slice(&2u32.to_le_bytes());
    trunc[V3_FAT_OFF + 8..V3_FAT_OFF + 12].copy_from_slice(&ENDOFCHAIN.to_le_bytes());
    let e = vole_document::adapter::cfbf::parse(&trunc, Limits::DEFAULT).unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidCfbfStructure);

    // A truncated header.
    let e = vole_document::adapter::cfbf::parse(&build(512)[..300], Limits::DEFAULT).unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidCfbfStructure);
}

#[test]
fn exact_materialization_is_byte_identical() {
    for (label, src) in [("exact-v3", build(512)), ("exact-v4", build(4096))] {
        let fx = Fixture::new(label, &src);
        let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
        let out = field.materialize_exact(Limits::DEFAULT).unwrap();
        assert_eq!(out.len(), src.len(), "{label}");
        assert_eq!(sha256(&out), sha256(&src), "{label}");
        assert_eq!(out, src, "{label}");
    }
}

#[test]
fn removal_then_fresh_process_is_queryable_and_exact() {
    let root = temp_dir("removal");
    let src_path = root.join("doc.doc");
    let desc_path = root.join("doc.voldoc");
    let source = build(512);
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
            Selector::CfbfDialect,
            Representation::Text,
        )),
        "cfb3"
    );
    let large = answer_bytes(&observe_ok(
        &mut store2,
        &field_id,
        Selector::CfbfStream { index: 2 },
        Representation::ExactBytes,
    ));
    assert_eq!(large, large_content());

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), source.len());
    assert_eq!(sha256(&exact), sha256(&source));
    assert_eq!(exact, source);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn cfbf_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived compound-file model reads the source bytes, so its single
    // dependency must be the exact `DocumentExact` root (keyed by `sha256(source)`).
    let source = build(512);
    let fx = Fixture::new("adr0060", &source);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_CFBF_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::CfbfModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the cfbf model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0xC0FF_EE35_2135_5566;
    for _ in 0..256 {
        let mut buf = vec![0u8; 4096];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        let _ = vole_document::adapter::cfbf::detect(&buf, Limits::STRICT);
        let _ = vole_document::adapter::cfbf::parse(&buf, Limits::STRICT);
    }
    // The signature-prefixed path is the most likely false-positive route.
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
        let _ = vole_document::adapter::cfbf::parse(&buf, Limits::STRICT);
    }
}
