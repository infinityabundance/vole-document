//! Phase 21.32 court: the RTF (Rich Text Format) adapter.
//!
//! Gated on `field` + `rtf`, so a build without the feature compiles an empty target.
//!
//! RTF is a **word-processor markup** Wave-2 format: a text, group-brace grammar with
//! control words. It is *not* a package (no OPC/ZIP layer): the exact leaf is the whole
//! source (a `DocumentExact`, RAW-like authority) and everything the adapter produces is
//! a bounded, deterministic (`Q_gen`) projection that never sits on the exactness path.
//! The court checks:
//!
//! 1. **Byte-based detection and the boundaries**: the strong header magic
//!    `{\rtf<digit>` plus a bounded brace-balance scan → `Rtf`; a leading-`{` junk blob,
//!    plain prose, a missing `\rtf` header, and trailing bytes after the root group all
//!    stay `Opaque`.
//! 2. **Representation preservation**: the header (`\rtf` version, charset, `\deffN`),
//!    every group with its exact brace span/parent/depth, every control word (with its
//!    numeric parameter and terminating delimiter space) and control symbol, every
//!    `\'hh` hex escape and `\uN` Unicode escape (with the active `\ucN` fallback count,
//!    the fallback never dropped), every destination (`\fonttbl`/`\colortbl`/
//!    `\stylesheet`/`\info`/`\pict`/`\object`/`\*` ignore) verbatim, and every plain text
//!    run byte-exactly (never whitespace-normalized) — nothing normalized or
//!    re-serialized.
//! 3. **Selectors**: native `rtf-header`/`rtf-group`/`rtf-control`/`rtf-hex`/
//!    `rtf-unicode`/`rtf-text`/`rtf-dest`/`rtf-find` plus common `metadata`/`text`/
//!    `search-match`.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 5. **Fail closed**: malformed/misshaped inputs decline typed; out-of-range
//!    selectors and absent destinations decline typed; random bytes never panic.
//! 6. **No cross-field aliasing** (ADR-0060): the model node depends on the exact
//!    `sha256(source)` root.

#![cfg(all(feature = "field", feature = "rtf"))]

use std::path::PathBuf;

use vole_document::adapter::rtf::{
    self, C_SYMBOL, C_WORD, CS_ANSI, D_COLOR_TABLE, D_FONT_TABLE, D_IGNORE, D_INFO, D_OBJECT,
    D_PICT, D_STYLESHEET, NO_GROUP,
};
use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_RTF_MODEL, SelectorKey, lookup};
use vole_document::field::ingest::{IngestReport, ingest_pdf};
use vole_document::field::node::NodeKind;
use vole_document::field::observe::{ObserveRequest, Representation, Selector, observe};
use vole_document::field::provenance::{AnswerValue, FieldAnswer};
use vole_document::field::{Field, FieldId, FieldStore};
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::store::NodeId;

/// A self-contained RTF document exercising every construct the adapter records: the
/// header, a nested `\fonttbl`, a `\colortbl`, a `\stylesheet`, an `\info` group, a
/// `\pict` binary group, an `\object` group, a `\*` ignore destination, control words
/// (with and without a numeric parameter, and with a terminating delimiter space),
/// control symbols, a `\'hh` hex escape, `\uN` Unicode escapes with a `\ucN` fallback
/// count, and plain text runs.
const RTF: &[u8] = br#"{\rtf1\ansi\deff0
{\fonttbl{\f0\fnil Arial;}{\f1\fnil Times New Roman;}}
{\colortbl;\red0\green0\blue0;\red255\green0\blue0;}
{\stylesheet{\s0 Normal;}}
{\info{\title Sample Document}{\author Ada}}
{\pict\pngblip \'89PNGdata}
{\object\objemb object}
{\*\generator Sample}
Hello \b bold\b0 \fs24 big\fs20 and \i italic\i0 . caf\'e9
\u233? \uc0\u8212
\{braces\} \\ \~ \- \_
{grouped text}
}"#;

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-rtf-{label}-{}-{}",
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
        format_basis: "opaque:rtf-test".to_string(),
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

/// The model parsed directly, for computing selector indices in the observe tests.
fn model() -> rtf::RtfModel {
    rtf::parse(RTF, Limits::DEFAULT).unwrap()
}

fn text_runs(m: &rtf::RtfModel) -> Vec<&[u8]> {
    m.text_runs
        .iter()
        .map(|t| rtf::text_run_bytes(RTF, t).unwrap())
        .collect()
}

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_content_only_and_bounded() {
    // A well-formed RTF document is claimed by the RTF adapter.
    assert_eq!(
        detect_document_format(RTF, Limits::DEFAULT),
        DocumentFormat::Rtf
    );
    assert!(rtf::detect(RTF, Limits::DEFAULT));
    // A UTF-8 BOM is tolerated.
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(b"{\\rtf1\\ansi x}");
    assert_eq!(
        detect_document_format(&bom, Limits::DEFAULT),
        DocumentFormat::Rtf
    );

    // The recorded boundary: a leading-`{` junk blob is not RTF and stays Opaque.
    assert!(!rtf::detect(b"{ not rtf }", Limits::DEFAULT));
    assert_eq!(
        detect_document_format(b"{ not rtf }", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    // Prose stays Opaque.
    assert!(!rtf::detect(
        b"This is just prose.\nWith a second line.\n",
        Limits::DEFAULT
    ));
    assert_eq!(
        detect_document_format(
            b"This is just prose.\nWith a second line.\n",
            Limits::DEFAULT
        ),
        DocumentFormat::Opaque
    );
    // A missing `\rtf` version digit is not RTF.
    assert!(!rtf::detect(b"{\\rtfX}", Limits::DEFAULT));
    // The outermost group must close exactly at end of input.
    assert!(!rtf::detect(b"{\\rtf1 x} trailing", Limits::DEFAULT));
    assert!(!rtf::detect(b"{\\rtf1 { unbalanced", Limits::DEFAULT));
    // Escaped braces do not count toward balance.
    assert!(rtf::detect(b"{\\rtf1 \\{ \\} ok}", Limits::DEFAULT));

    let caps = capabilities_for_format(DocumentFormat::Rtf);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in [
        "rtf-header",
        "rtf-group",
        "rtf-control",
        "rtf-hex",
        "rtf-unicode",
        "rtf-text",
        "rtf-dest",
        "rtf-find",
    ] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"rtf\""));
}

#[test]
fn header_groups_and_destinations_are_exact() {
    let m = model();
    // Header: `\rtf1`, `\ansi`, `\deff0`.
    assert_eq!(m.header.version, 1);
    assert_eq!(m.header.charset, CS_ANSI);
    assert_eq!(m.header.deff_value(), Some(0));
    assert_eq!(
        &RTF[m.header.rtf_start as usize..m.header.rtf_end as usize],
        b"\\rtf1"
    );
    // The outermost group is depth 1 with no parent; nesting increases inward.
    assert_eq!(m.groups[0].depth, 1);
    assert_eq!(m.groups[0].parent, NO_GROUP);
    assert_eq!(m.max_depth(), 3);
    // The `\fonttbl` group is a recorded destination; its brace span is exact.
    let ft_group = m.groups[1].clone();
    assert_eq!(ft_group.depth, 2);
    assert_eq!(ft_group.dest, D_FONT_TABLE);
    let ft_bytes = rtf::group_bytes(RTF, &ft_group).unwrap();
    assert!(ft_bytes.starts_with(b"{\\fonttbl"), "{ft_bytes:?}");
    assert!(ft_bytes.ends_with(b"}"));
    // Every destination kind in the fixture is recorded.
    for k in [
        D_FONT_TABLE,
        D_COLOR_TABLE,
        D_STYLESHEET,
        D_INFO,
        D_PICT,
        D_OBJECT,
        D_IGNORE,
    ] {
        assert!(
            !m.destinations_of_kind(k).is_empty(),
            "missing destination kind {k}"
        );
    }
    // A destination's exact group bytes are verbatim.
    let di = m.destinations_of_kind(D_PICT)[0];
    let d = m.destination(di).unwrap();
    let pict = rtf::destination_bytes(RTF, &m, d).unwrap();
    assert!(pict.starts_with(b"{\\pict"), "{pict:?}");
}

#[test]
fn control_words_symbols_and_text_roundtrip() {
    let m = model();
    // `\fs24` kept its numeric parameter span exactly.
    let fs24 = m
        .controls
        .iter()
        .find(|c| {
            rtf::control_name_bytes(RTF, c).unwrap() == b"fs"
                && rtf::control_param(RTF, c) == Some(24)
        })
        .expect("missing \\fs24");
    assert!(fs24.has_param);
    assert_eq!(
        &RTF[fs24.param_start as usize..fs24.param_end as usize],
        b"24"
    );
    // `\b` consumed its terminating delimiter space, so the following text run is
    // exactly `bold` (never ` bold`).
    let b = m
        .controls
        .iter()
        .find(|c| rtf::control_name_bytes(RTF, c).unwrap() == b"b")
        .expect("missing \\b");
    assert!(b.delimited, "\\b did not record its delimiter space");
    // Control symbols are recorded with their exact spelling.
    let symbols: Vec<&[u8]> = m
        .controls
        .iter()
        .filter(|c| c.kind == C_SYMBOL)
        .map(|c| rtf::control_name_bytes(RTF, c).unwrap())
        .collect();
    for want in [
        &b"{"[..],
        &b"}"[..],
        &b"\\"[..],
        &b"~"[..],
        &b"-"[..],
        &b"_"[..],
        &b"*"[..],
    ] {
        assert!(symbols.contains(&want), "missing symbol {want:?}");
    }
    // Plain text runs are byte-exact: the delimiter space is consumed, and the text
    // before `\b` ("Hello ") keeps its trailing space.
    let runs = text_runs(&m);
    assert!(runs.iter().any(|t| *t == b"bold"), "{runs:?}");
    assert!(runs.iter().any(|t| t.ends_with(b"Hello ")), "{runs:?}");
    assert!(runs.iter().any(|t| *t == b"big"), "{runs:?}");
    assert!(runs.iter().any(|t| *t == b"italic"), "{runs:?}");
    // A control word without a parameter is still recorded.
    assert!(m.controls.iter().any(|c| c.kind == C_WORD
        && rtf::control_name_bytes(RTF, c).unwrap() == b"i"
        && !c.has_param));
}

#[test]
fn hex_and_unicode_preserve_spelling_and_fallback() {
    let m = model();
    // The `\'e9` hex escape keeps its spelling and decodes under the declared charset.
    let he = m
        .hex
        .iter()
        .find(|h| h.byte == 0xE9)
        .expect("missing \\'e9");
    assert_eq!(rtf::hex_bytes(RTF, he).unwrap(), b"\\'e9");
    assert_eq!(rtf::hex_char(CS_ANSI, 0xE9), 'é');
    // `\u233` under the default `\uc1`, and `\u8212` under `\uc0`.
    let u233 = m
        .unicode
        .iter()
        .find(|u| u.code == 233)
        .expect("missing \\u233");
    assert_eq!(u233.uc, 1);
    assert_eq!(rtf::unicode_bytes(RTF, u233).unwrap(), b"\\u233");
    assert_eq!(rtf::unicode_char(u233.code), Some('é'));
    let u8212 = m
        .unicode
        .iter()
        .find(|u| u.code == 8212)
        .expect("missing \\u8212");
    assert_eq!(u8212.uc, 0);
    // The fallback text (`?`) is preserved verbatim, never dropped.
    let all: Vec<u8> = m
        .text_runs
        .iter()
        .flat_map(|t| rtf::text_run_bytes(RTF, t).unwrap().to_vec())
        .collect();
    let all = String::from_utf8_lossy(&all);
    assert!(all.contains('?'), "fallback `?` dropped: {all:?}");
}

#[test]
fn native_and_common_selectors_resolve() {
    let mut fx = Fixture::new("selectors", RTF);
    let field = fx.report.field;
    let m = model();

    // Native header: Text is the compact canonical string.
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::RtfHeader,
            Representation::Text,
        )),
        "rtf1 ansi deff0"
    );
    let hdr = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::RtfHeader,
        Representation::Metadata,
    ));
    assert!(hdr.contains("\"format\":\"rtf\""), "{hdr}");
    assert!(hdr.contains("\"version\":1"), "{hdr}");

    // Native group: the `\fonttbl` group by its index.
    let ft_group_idx = m.groups[1].clone();
    assert_eq!(ft_group_idx.dest, D_FONT_TABLE);
    let g = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::RtfGroup { index: 1 },
        Representation::Metadata,
    ));
    assert!(g.contains("\"dest\":\"fonttbl\""), "{g}");
    assert!(g.contains("\"depth\":2"), "{g}");
    assert!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            Selector::RtfGroup { index: 1 },
            Representation::ExactBytes,
        ))
        .starts_with(b"{\\fonttbl")
    );

    // Native control: index 0 is `\rtf1`.
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            Selector::RtfControl { index: 0 },
            Representation::ExactBytes,
        )),
        b"\\rtf1"
    );
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::RtfControl { index: 0 },
            Representation::Text,
        )),
        "rtf"
    );

    // Native hex: the `\'e9` escape by its index.
    let h9 = m.hex.iter().position(|h| h.byte == 0xE9).unwrap() as u32;
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            Selector::RtfHex { index: h9 },
            Representation::ExactBytes,
        )),
        b"\\'e9"
    );
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::RtfHex { index: h9 },
            Representation::Text,
        )),
        "é"
    );

    // Native unicode: index 0 is `\u233` with `uc` 1.
    let u0 = m.unicode.iter().position(|u| u.code == 233).unwrap() as u32;
    let uj = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::RtfUnicode { index: u0 },
        Representation::Metadata,
    ));
    assert!(uj.contains("\"code\":233"), "{uj}");
    assert!(uj.contains("\"uc\":1"), "{uj}");

    // Native text: the run carrying "Hello ".
    let hi = m
        .text_runs
        .iter()
        .position(|t| rtf::text_run_bytes(RTF, t).unwrap().ends_with(b"Hello "))
        .unwrap() as u32;
    assert!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::RtfText { index: hi },
            Representation::Text,
        ))
        .ends_with("Hello ")
    );

    // Native destination: `fonttbl` ExactBytes is its whole group.
    assert!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            Selector::RtfDestination {
                spec: "fonttbl".to_string(),
            },
            Representation::ExactBytes,
        ))
        .starts_with(b"{\\fonttbl")
    );
    assert!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            Selector::RtfDestination {
                spec: "fonttbl".to_string(),
            },
            Representation::Text,
        ))
        .contains("Arial")
    );

    // Native find.
    let find = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::RtfFind {
            pattern: "Hello".to_string(),
        },
        Representation::Text,
    ));
    assert!(find.contains("\"matches\":["), "{find}");

    // Common metadata / text / search-match.
    let md = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(md.contains("\"format\":\"rtf\""), "{md}");
    assert!(md.contains("\"groups\":14"), "{md}");
    assert!(md.contains("\"destinations\":7"), "{md}");
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.contains("Hello "), "{text}");
    let sm = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("Hello".to_string()),
        Representation::Text,
    ));
    assert!(sm.contains("\"matches\":["), "{sm}");
}

#[test]
fn declines_are_typed_never_silent() {
    let mut fx = Fixture::new("declines", RTF);
    let field = fx.report.field;
    // Out-of-range native indices decline typed.
    for selector in [
        Selector::RtfGroup { index: 9999 },
        Selector::RtfControl { index: 9999 },
        Selector::RtfHex { index: 9999 },
        Selector::RtfUnicode { index: 9999 },
        Selector::RtfText { index: 9999 },
    ] {
        assert_eq!(
            observe_err(&mut fx.store, &field, selector, Representation::Metadata),
            ErrorClass::UnsupportedFeature
        );
    }
    // An unknown destination name is a usage error.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::RtfDestination {
                spec: "nope".to_string(),
            },
            Representation::Text,
        ),
        ErrorClass::Usage
    );
    // A non-canonical ordinal is a usage error.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::RtfDestination {
                spec: "fonttbl:01".to_string(),
            },
            Representation::Text,
        ),
        ErrorClass::Usage
    );
    // An absent ordinal for a present destination declines typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::RtfDestination {
                spec: "fonttbl:9".to_string(),
            },
            Representation::Text,
        ),
        ErrorClass::UnsupportedFeature
    );
    // An unsupported common pair declines typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::Table(0),
            Representation::Text,
        ),
        ErrorClass::UnsupportedFeature
    );
}

#[test]
fn malformed_and_non_rtf_decline_typed() {
    // An unbalanced brace.
    assert_eq!(
        rtf::parse(b"{\\rtf1\\ansi x", Limits::DEFAULT)
            .unwrap_err()
            .class(),
        ErrorClass::InvalidRtfStructure
    );
    // A trailing brace with no open group.
    assert_eq!(
        rtf::parse(b"{\\rtf1\\ansi}}", Limits::DEFAULT)
            .unwrap_err()
            .class(),
        ErrorClass::InvalidRtfStructure
    );
    // A missing `\rtf` header magic.
    assert_eq!(
        rtf::parse(b"{ not rtf }", Limits::DEFAULT)
            .unwrap_err()
            .class(),
        ErrorClass::InvalidRtfStructure
    );
    // A `{`-junk blob is never claimed as RTF.
    assert_eq!(
        detect_document_format(b"{ not rtf }", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    // A depth bomb declines typed under STRICT.
    let mut deep = Vec::new();
    deep.extend_from_slice(b"{\\rtf1\\ansi");
    deep.extend_from_slice(&[b'{'; 100]);
    deep.extend_from_slice(b"x}");
    assert_eq!(
        rtf::parse(&deep, Limits::STRICT).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );
    // An over-long control word declines typed.
    let mut long = Vec::new();
    long.extend_from_slice(b"{\\rtf1\\ansi\\");
    long.extend_from_slice(&[b'a'; 200]);
    long.extend_from_slice(b" x}");
    assert_eq!(
        rtf::parse(&long, Limits::DEFAULT).unwrap_err().class(),
        ErrorClass::InvalidRtfStructure
    );
}

#[test]
fn exact_materialization_is_byte_identical() {
    let fx = Fixture::new("exact", RTF);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let out = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(out.len(), fx.source.len());
    assert_eq!(sha256(&out), sha256(&fx.source));
    assert_eq!(out, fx.source);
}

#[test]
fn removal_then_fresh_process_is_queryable_and_exact() {
    let root = temp_dir("removal");
    let src_path = root.join("doc.rtf");
    let desc_path = root.join("doc.voldoc");
    std::fs::write(&src_path, RTF).unwrap();
    let descriptor = opaque_descriptor(RTF);
    std::fs::write(&desc_path, &descriptor).unwrap();

    let store_root = root.join("store");
    let field_id = {
        let mut store = FieldStore::open(&store_root).unwrap();
        let report = ingest_pdf(&mut store, &descriptor, Limits::DEFAULT).unwrap();
        report.field
        // store drops here: a fresh process reopens the directory.
    };

    std::fs::remove_file(&src_path).unwrap();
    std::fs::remove_file(&desc_path).unwrap();

    let mut store2 = FieldStore::open(&store_root).unwrap();
    let hdr = answer_json(&observe_ok(
        &mut store2,
        &field_id,
        Selector::RtfHeader,
        Representation::Metadata,
    ));
    assert!(hdr.contains("\"version\":1"), "{hdr}");

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), RTF.len());
    assert_eq!(sha256(&exact), sha256(RTF));
    assert_eq!(exact, RTF);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn rtf_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived RTF model reads the source bytes, so its single dependency
    // must be the exact `DocumentExact` root (keyed by `sha256(source)`).
    let fx = Fixture::new("adr0060", RTF);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_RTF_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::RtfModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the RTF model must depend on exactly the exact root"
    );
}

#[test]
fn caps_decline_typed() {
    // Group count.
    let tight = Limits {
        max_rtf_groups: 1,
        ..Limits::DEFAULT
    };
    assert_eq!(
        rtf::parse(RTF, tight).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );
    // Nesting depth (and detection declines too).
    let tight = Limits {
        max_rtf_depth: 1,
        ..Limits::DEFAULT
    };
    assert!(!rtf::detect(RTF, tight));
    assert_eq!(
        rtf::parse(RTF, tight).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );
    // Control-word count.
    let tight = Limits {
        max_rtf_control_words: 1,
        ..Limits::DEFAULT
    };
    assert_eq!(
        rtf::parse(RTF, tight).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );
    // Accumulated plain-text bytes.
    let tight = Limits {
        max_rtf_text_bytes: 1,
        ..Limits::DEFAULT
    };
    assert_eq!(
        rtf::parse(RTF, tight).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );
    // Hex-escape count.
    let tight = Limits {
        max_rtf_hex_escapes: 1,
        ..Limits::DEFAULT
    };
    assert_eq!(
        rtf::parse(RTF, tight).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );
    // Source length (and detection declines too).
    let tight = Limits {
        max_rtf_document_bytes: 10,
        ..Limits::DEFAULT
    };
    assert!(!rtf::detect(RTF, tight));
    assert_eq!(
        rtf::parse(RTF, tight).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );

    // The opaque floor still closes the source exactly.
    let fx = Fixture::new("caps", RTF);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0x0DDB_1A5E_5BAD_5EED;
    for _ in 0..256 {
        let mut buf = vec![0u8; 256];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = rtf::detect(&buf, Limits::DEFAULT);
        if let Ok(m) = rtf::parse(&buf, Limits::STRICT) {
            let _ = m.encode();
            let _ = rtf::canonical_text(&buf, Limits::STRICT, 1 << 20);
            let _ = rtf::find(&buf, &m, "a", 1 << 20);
        }
        let _ = rtf::build_rtf_model(&buf, Limits::STRICT);
        let _ = rtf::RtfModel::decode(&buf);
    }
}
