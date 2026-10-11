//! Phase 21.25 court: the fixed-width (column-position) tabular adapter.
//!
//! Gated on `fixedwidth`, so a build without the feature compiles an empty target.
//!
//! Fixed-width has no package layer: the exact leaf is the whole source (a
//! `DocumentExact`), and every derived observation is a bounded,
//! representation-preserving projection of it. The court checks:
//!
//! 1. **Byte-based detection is maximally conservative**: a uniform-width table with
//!    a stable column layout is detected; variable-length prose, a single-space
//!    two-column blob, a delimited (CSV) table, a Markdown table, and a one-column
//!    blob are all *not* fixed-width.
//! 2. **Representation preservation**: the inferred per-column start/end positions and
//!    widths, the uniform record width, each record's exact content span, each field's
//!    exact padded bytes, CRLF vs LF, and the header row are all preserved.
//! 3. **Selectors**: native `fixedwidth-row`/`fixedwidth-cell`/`fixedwidth-header`/
//!    `fixedwidth-columns`/`fixedwidth-range`/`fixedwidth-find` and common `metadata`/
//!    `text`/`table`/`cell`/`search-match` resolve.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 5. **Fail closed**: an out-of-range record and an unknown column name decline typed;
//!    a cap breach declines typed; random bytes never panic.
//! 6. **No cross-field aliasing** (ADR-0060): the model node depends on the exact root.

#![cfg(all(feature = "field", feature = "fixedwidth"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_FIXEDWIDTH_MODEL, SelectorKey, lookup};
use vole_document::field::ingest::{IngestReport, ingest_pdf};
use vole_document::field::node::NodeKind;
use vole_document::field::observe::{ObserveRequest, Representation, Selector, observe};
use vole_document::field::provenance::{AnswerValue, FieldAnswer};
use vole_document::field::{Field, FieldId, FieldStore};
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::store::NodeId;

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-fixedwidth-{label}-{}-{}",
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
        format_basis: "opaque:fixedwidth-test".to_string(),
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
    limits: Limits,
) -> ErrorClass {
    let req = ObserveRequest::new(selector, representation);
    observe(store, field, &req, limits).unwrap_err().class()
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

fn row(index: u32) -> Selector {
    Selector::FixedWidthRow { index }
}

fn cell(spec: &str) -> Selector {
    Selector::FixedWidthCell {
        spec: spec.to_string(),
    }
}

fn range(spec: &str) -> Selector {
    Selector::FixedWidthRange {
        spec: spec.to_string(),
    }
}

fn find(pattern: &str) -> Selector {
    Selector::FixedWidthFind {
        pattern: pattern.to_string(),
    }
}

// Three records of width 10; columns `[0,5)` and `[7,10)` separated by a two-wide
// all-space gap at positions 5 and 6.
const DOC: &[u8] = b"Name   Age\nAlice   30\nBob     25\n";

fn parse_span(json: &str) -> (usize, usize) {
    let tag = "\"span\":[";
    let i = json.find(tag).unwrap() + tag.len();
    let rest = &json[i..];
    let close = rest.find(']').unwrap();
    let mut it = rest[..close].split(',');
    let s = it.next().unwrap().parse().unwrap();
    let e = it.next().unwrap().parse().unwrap();
    (s, e)
}

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    assert_eq!(
        detect_document_format(DOC, Limits::DEFAULT),
        DocumentFormat::FixedWidth
    );
    // Variable-length prose is not uniform-width and stays Opaque.
    assert_eq!(
        detect_document_format(
            b"Hello world\nThis is text.\nMore words here.\n",
            Limits::DEFAULT
        ),
        DocumentFormat::Opaque
    );
    // A single-space-separated two-column blob is too ambiguous and stays Opaque.
    assert_eq!(
        detect_document_format(b"a b\nc d\ne f\n", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    // A delimited (CSV) table stays CSV, never fixed-width.
    assert_eq!(
        detect_document_format(b"a,b\nc,d\n", Limits::DEFAULT),
        DocumentFormat::Csv
    );
    assert_eq!(
        detect_document_format(b"a\tb\nc\td\n", Limits::DEFAULT),
        DocumentFormat::Csv
    );
    assert_eq!(
        detect_document_format(b"a|b\nc|d\n", Limits::DEFAULT),
        DocumentFormat::Csv
    );
    // Two records are too few.
    assert_eq!(
        detect_document_format(b"Name  Age\nAlice  30\n", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    assert_eq!(
        detect_document_format(b"", Limits::DEFAULT),
        DocumentFormat::Opaque
    );

    let caps = capabilities_for_format(DocumentFormat::FixedWidth);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "table", "cell", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in [
        "fixedwidth-row",
        "fixedwidth-cell",
        "fixedwidth-header",
        "fixedwidth-columns",
        "fixedwidth-range",
        "fixedwidth-find",
    ] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"fixedwidth\""));
}

#[test]
fn preserves_layout_spans_padding_and_terminators() {
    // CRLF variant: the terminator, the layout, and every padded field must survive.
    let src = b"Name   Age\r\nAlice   30\r\nBob     25\r\n";
    let mut fx = Fixture::new("preserve", src);
    let field = fx.report.field;

    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"columns\":2"), "{meta}");
    assert!(meta.contains("\"width\":10"), "{meta}");
    assert!(meta.contains("\"terminator\":\"crlf\""), "{meta}");
    assert!(meta.contains("\"rows\":3"), "{meta}");
    assert!(meta.contains("\"ragged_records\":0"), "{meta}");

    // Field 0 of record 1 is `Alice` (exactly fills the 5-wide column).
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        cell("1:0"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"Alice");
    // Field 1 of record 1 is ` 30` (leading padding preserved in ExactBytes), `30`
    // after trimming in Text.
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        cell("1:1"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b" 30");
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        cell("1:1"),
        Representation::Text,
    ));
    assert_eq!(text, "30");

    // The reported field span is the exact source span.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        cell("2:0"),
        Representation::Metadata,
    ));
    let (start, end) = parse_span(&j);
    assert_eq!(&src[start..end], b"Bob  ");

    // The record's exact bytes exclude the terminator and keep the padding.
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        row(1),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"Alice   30");
}

#[test]
fn native_selectors_resolve() {
    let mut fx = Fixture::new("native", DOC);
    let field = fx.report.field;

    // The recovered column layout.
    let cols = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::FixedWidthColumns,
        Representation::Text,
    ));
    assert_eq!(cols, "0:5,7:10");
    let cj = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::FixedWidthColumns,
        Representation::Metadata,
    ));
    assert!(cj.contains("\"start\":0"), "{cj}");
    assert!(cj.contains("\"end\":5"), "{cj}");
    assert!(cj.contains("\"width\":5"), "{cj}");
    assert!(cj.contains("\"start\":7"), "{cj}");

    // Header names.
    let h = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::FixedWidthHeader,
        Representation::Metadata,
    ));
    assert!(h.contains("\"count\":2"), "{h}");
    assert!(h.contains("\"Name\""), "{h}");
    assert!(h.contains("\"Age\""), "{h}");

    // A cell addressed by header name (`R:COLNAME`).
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        cell("1:Name"),
        Representation::Text,
    ));
    assert_eq!(text, "Alice");

    // A range.
    let r = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        range("0:0:1:1"),
        Representation::Metadata,
    ));
    assert!(r.contains("\"row\":0"), "{r}");
    assert!(r.contains("\"row\":1"), "{r}");
    assert!(r.contains("\"text\":\"Alice\""), "{r}");

    // A lexical find over trimmed field text.
    let f = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        find("li"),
        Representation::Text,
    ));
    assert!(f.contains("\"record\":1"), "{f}");
    assert!(f.contains("\"column\":0"), "{f}");
    assert!(f.contains("\"text\":\"Alice\""), "{f}");

    // Missing records and unknown header names decline typed (never empty).
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            row(999),
            Representation::Metadata,
            Limits::DEFAULT
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            cell("0:nope"),
            Representation::Metadata,
            Limits::DEFAULT
        ),
        ErrorClass::UnsupportedFeature
    );
}

#[test]
fn common_selectors_resolve() {
    let mut fx = Fixture::new("common", DOC);
    let field = fx.report.field;

    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.starts_with("Name   Age"), "{text}");

    let table = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Table(0),
        Representation::Text,
    ));
    assert_eq!(table, text);

    let cellv = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Cell {
            table: 0,
            row: 1,
            col: 0,
        },
        Representation::Text,
    ));
    assert_eq!(cellv, "Alice");

    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("Bob".to_string()),
        Representation::Text,
    ));
    assert!(found.contains("\"record\":2"), "{found}");

    // A second table does not exist and declines typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::Table(1),
            Representation::Text,
            Limits::DEFAULT
        ),
        ErrorClass::UnsupportedFeature
    );
}

#[test]
fn exact_materialization_is_byte_identical() {
    let fx = Fixture::new("exact", DOC);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let out = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(out.len(), fx.source.len());
    assert_eq!(sha256(&out), sha256(&fx.source));
    assert_eq!(out, fx.source);
}

#[test]
fn removal_then_fresh_process_is_queryable_and_exact() {
    let root = temp_dir("removal");
    let src_path = root.join("doc.fw");
    let desc_path = root.join("doc.voldoc");
    std::fs::write(&src_path, DOC).unwrap();
    let descriptor = opaque_descriptor(DOC);
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
    let bytes = answer_bytes(&observe_ok(
        &mut store2,
        &field_id,
        cell("1:1"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b" 30");

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), DOC.len());
    assert_eq!(sha256(&exact), sha256(DOC));
    assert_eq!(exact, DOC);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn fixedwidth_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived fixed-width model reads the source bytes, so its single
    // dependency must be the exact `DocumentExact` root (keyed by `sha256(source)`).
    let fx = Fixture::new("adr0060", DOC);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_FIXEDWIDTH_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::FixedWidthModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the fixed-width model must depend on exactly the exact root"
    );
}

#[test]
fn caps_decline_typed_but_exactness_holds() {
    // A table with more records than the STRICT row cap: detection (a bounded sample)
    // admits the shape; the whole-table model build declines typed.
    let mut big = String::new();
    for _ in 0..70_000 {
        big.push_str("Name   Age\n");
    }
    let big = big.into_bytes();
    assert_eq!(
        detect_document_format(&big, Limits::STRICT),
        DocumentFormat::FixedWidth
    );
    let mut fx = Fixture::new("caps", &big);
    let field = fx.report.field;
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::Metadata,
            Representation::Metadata,
            Limits::STRICT
        ),
        ErrorClass::ResourceLimit
    );
    // Exactness is independent of the derived decline.
    let f = Field::open(&fx.store, &field, Limits::DEFAULT).unwrap();
    assert_eq!(f.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..128 {
        let mut buf = vec![0u8; 512];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        if let Ok(l) = vole_document::adapter::fixedwidth::infer_layout(&buf, Limits::STRICT) {
            let _ = vole_document::adapter::fixedwidth::canonical_text(
                &buf,
                &l,
                Limits::STRICT,
                1 << 20,
            );
            let _ =
                vole_document::adapter::fixedwidth::build_fixedwidth_model(&buf, Limits::STRICT);
            let _ =
                vole_document::adapter::fixedwidth::find(&buf, &l, "a", Limits::STRICT, 1 << 20);
        }
    }
}
