//! Phase 21.12 court: the per-line JSONL / NDJSON adapter (the line/event-stream
//! Wave-2 format).
//!
//! Gated on `jsonl`, so a build without the feature compiles an empty target.
//!
//! JSONL has no package layer: the exact leaf is the whole source (a
//! `DocumentExact`), and every derived observation is a bounded,
//! representation-preserving projection of it (each line parsed by the shared JSON
//! parser). The court checks:
//!
//! 1. **Byte-based conservative detection** and the **Json-vs-Jsonl boundary**: a
//!    newline-separated stream is JSONL; a single JSON value — including one spread
//!    across several lines — stays `Json`; a malformed line, a single record, and a
//!    bag of JSON values with a *non-newline* separator stay `Opaque`.
//! 2. **Per-line spans / terminators / blank lines**: each record's exact line span,
//!    value span, terminator (LF/CRLF/none), physical line number, and the reported
//!    blank-line / CRLF / trailing-newline counts.
//! 3. **Representation preservation**: duplicate keys, member order, numeric and
//!    escape spelling, and exact token spans within a line.
//! 4. **Selectors**: native `jsonl-line`/`jsonl-pointer`/`jsonl-find` plus common
//!    `metadata`/`text`/`search-match`.
//! 5. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 6. **Fail closed**: over-cap lines/records and deep nesting decline typed; random
//!    bytes never panic.
//! 7. **No cross-field aliasing** (ADR-0060): two JSONL fixtures interleaved in one
//!    store each answer from their own source spans, and the model node depends on
//!    the exact `sha256(source)` root.

#![cfg(all(feature = "field", feature = "jsonl"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_JSONL_MODEL, SelectorKey, lookup};
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
        "vole-jsonl-{label}-{}-{}",
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
        format_basis: "opaque:jsonl-test".to_string(),
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

fn line(index: u32) -> Selector {
    Selector::JsonlLine { index }
}

fn ptr(spec: &str) -> Selector {
    Selector::JsonlPointer {
        spec: spec.to_string(),
    }
}

/// Three records: a CRLF-terminated object with a duplicate key, a record with a
/// unicode escape and an array, a blank line, and a final record. Trailing newline.
const DOC: &[u8] = b"{\"b\":1e3,\"a\":1.0,\"a\":-0}\r\n{\"c\":\"\\u00e9\",\"n\":null,\"list\":[true,42]}\n\n{\"msg\":\"hello\"}\n";

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    assert_eq!(
        detect_document_format(DOC, Limits::DEFAULT),
        DocumentFormat::Jsonl
    );
    assert_eq!(
        detect_document_format(b"1\n2\n3\n", Limits::DEFAULT),
        DocumentFormat::Jsonl
    );
    // A single JSON value stays `Json`, even when spread across several lines
    // (the Json-vs-Jsonl boundary).
    assert_eq!(
        detect_document_format(b"{\n \"a\": 1\n}\n", Limits::DEFAULT),
        DocumentFormat::Json
    );
    assert_eq!(
        detect_document_format(b"[\n1,\n2\n]\n", Limits::DEFAULT),
        DocumentFormat::Json
    );
    assert_eq!(
        detect_document_format(b"42\n", Limits::DEFAULT),
        DocumentFormat::Json
    );
    // A single record is not a stream (and its source is one JSON value).
    assert_eq!(
        detect_document_format(b"42", Limits::DEFAULT),
        DocumentFormat::Json
    );
    // A malformed second line is not JSONL (and not JSON): Opaque.
    for bad in [&b"1\n[1, 2,\n"[..], &b"{\"a\":1}\n{oops}\n"[..]] {
        assert_eq!(
            detect_document_format(bad, Limits::DEFAULT),
            DocumentFormat::Opaque,
            "{bad:?}"
        );
    }
    // A bag of JSON values with a non-newline separator is not JSONL.
    assert_ne!(
        detect_document_format(b"{\"a\":1}{\"b\":2}\n", Limits::DEFAULT),
        DocumentFormat::Jsonl
    );
    assert_ne!(
        detect_document_format(b"1,2,3\n", Limits::DEFAULT),
        DocumentFormat::Jsonl
    );
    // Blank lines alone are not records.
    assert_ne!(
        detect_document_format(b"1\n\n\n", Limits::DEFAULT),
        DocumentFormat::Jsonl
    );

    let caps = capabilities_for_format(DocumentFormat::Jsonl);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in ["jsonl-line", "jsonl-pointer", "jsonl-find"] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"jsonl\""));
}

#[test]
fn per_line_spans_terminators_and_blank_lines() {
    use vole_document::adapter::jsonl::{T_CRLF, T_LF, line_bytes, parse};

    let m = parse(DOC, Limits::DEFAULT).unwrap();
    assert_eq!(m.records.len(), 3);
    assert_eq!(m.blank_lines, 1);
    assert_eq!(m.crlf_records, 1);
    assert!(m.trailing_newline);
    assert_eq!(m.doc_len, DOC.len() as u64);

    // Record 0: CRLF-terminated; its value is the object without the newline.
    let r0 = &m.records[0];
    assert_eq!(r0.terminator, T_CRLF);
    assert_eq!(r0.line_number, 0);
    assert_eq!(&DOC[0..26], line_bytes(DOC, r0).unwrap());
    let (vs, ve) = r0.value_span().unwrap();
    assert_eq!(
        &DOC[vs as usize..ve as usize],
        b"{\"b\":1e3,\"a\":1.0,\"a\":-0}"
    );
    assert_eq!(r0.content_end(), 24);

    // Record 1 is LF-terminated; record 2 has physical line number 3 (a blank
    // line lies between).
    assert_eq!(m.records[1].terminator, T_LF);
    assert_eq!(m.records[2].line_number, 3);

    // Duplicate keys are preserved; the first `a` wins resolution.
    let r = vole_document::adapter::jsonl::resolve_record_pointer(&m, DOC, "0:/a").unwrap();
    assert_eq!(r.matches, 2);
    let node = m.records[0].model.node(r.index).unwrap();
    assert_eq!(&DOC[node.start as usize..node.end as usize], b"1.0");

    // The unicode escape's exact token bytes and canonical text are preserved.
    let r = vole_document::adapter::jsonl::resolve_record_pointer(&m, DOC, "1:/c").unwrap();
    let node = m.records[1].model.node(r.index).unwrap();
    assert_eq!(&DOC[node.start as usize..node.end as usize], br#""\u00e9""#);
    let text = vole_document::adapter::jsonl::canonical_text(&m, DOC).unwrap();
    assert!(text.contains("\\u00e9"), "{text}");
    assert!(
        text.starts_with("{\"b\":1e3,\"a\":1.0,\"a\":-0}\n"),
        "{text}"
    );

    // A no-terminator final line is reported as `T_NONE` with no trailing newline.
    let m = parse(b"1\n2", Limits::DEFAULT).unwrap();
    assert!(!m.trailing_newline);
    assert_eq!(m.records[1].terminator, 0);
}

#[test]
fn native_and_common_selectors_resolve() {
    let mut fx = Fixture::new("selectors", DOC);
    let field = fx.report.field;

    // `jsonl-line` — exact value bytes and a metadata descriptor with the line span.
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            line(0),
            Representation::ExactBytes,
        )),
        b"{\"b\":1e3,\"a\":1.0,\"a\":-0}"
    );
    let md = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        line(0),
        Representation::Metadata,
    ));
    assert!(md.contains("\"line_span\":[0,26]"), "{md}");
    assert!(md.contains("\"value_span\":[0,24]"), "{md}");
    assert!(md.contains("\"terminator\":\"crlf\""), "{md}");

    // `jsonl-pointer` — RFC 6901 into one record, with duplicate-key reporting.
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            ptr("0:/a"),
            Representation::ExactBytes,
        )),
        b"1.0"
    );
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            ptr("1:/list/1"),
            Representation::ExactBytes,
        )),
        b"42"
    );
    let pj = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        ptr("0:/a"),
        Representation::Metadata,
    ));
    assert!(pj.contains("\"matches\":2"), "{pj}");
    assert!(pj.contains("\"record\":0"), "{pj}");

    // `jsonl-find` reports the record index and within-record pointer.
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::JsonlFind {
            pattern: "list".to_string(),
        },
        Representation::Text,
    ));
    assert!(found.contains("\"record\":1"), "{found}");
    assert!(found.contains("\"pointer\":\"/list\""), "{found}");
    assert!(found.contains("\"role\":\"key\""), "{found}");

    // Common metadata / text / search-match.
    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"format\":\"jsonl\""), "{meta}");
    assert!(meta.contains("\"records\":3"), "{meta}");
    assert!(meta.contains("\"blank_lines\":1"), "{meta}");
    assert!(meta.contains("\"crlf_records\":1"), "{meta}");
    assert!(meta.contains("\"trailing_newline\":true"), "{meta}");
    assert!(meta.contains("\"duplicate_keys\":1"), "{meta}");
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.contains("\"b\":1e3"), "{text}");
    assert_eq!(text.lines().count(), 3, "{text}");
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("hello".to_string()),
        Representation::Text,
    ));
    assert!(found.contains("hello"), "{found}");

    // Missing records/pointers and unsupported common pairs decline typed.
    assert_eq!(
        observe_err(&mut fx.store, &field, line(9), Representation::Metadata),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            ptr("0:/nope"),
            Representation::Metadata
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            ptr("no-index:/a"),
            Representation::Metadata
        ),
        ErrorClass::Usage
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::Table(0),
            Representation::Text
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
    let src_path = root.join("doc.ndjson");
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
        ptr("1:/list/1"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"42");

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), DOC.len());
    assert_eq!(sha256(&exact), sha256(DOC));
    assert_eq!(exact, DOC);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn caps_and_deep_nesting_decline_typed() {
    use vole_document::adapter::jsonl::parse;

    // A deep line declines typed (the shared JSON depth cap).
    let mut deep = vec![b'['; 200];
    deep.push(b'0');
    deep.extend(std::iter::repeat_n(b']', 200));
    let mut two = deep.clone();
    two.push(b'\n');
    two.extend_from_slice(&deep);
    two.push(b'\n');
    assert_eq!(
        detect_document_format(&two, Limits::STRICT),
        DocumentFormat::Opaque
    );
    let e = parse(&two, Limits::STRICT).unwrap_err();
    assert_eq!(e.class(), ErrorClass::ResourceLimit);

    // An over-cap line declines typed.
    let tight_line = Limits {
        max_jsonl_line_bytes: 4,
        ..Limits::DEFAULT
    };
    assert!(!vole_document::adapter::jsonl::detect(
        b"[1,2,3]\n[4,5,6]\n",
        tight_line
    ));
    assert_eq!(
        parse(b"[1,2,3]\n[4,5,6]\n", tight_line)
            .unwrap_err()
            .class(),
        ErrorClass::ResourceLimit
    );

    // An over-cap record count declines typed.
    let tight_records = Limits {
        max_jsonl_records: 1,
        ..Limits::DEFAULT
    };
    assert_eq!(
        parse(b"1\n2\n3\n", tight_records).unwrap_err().class(),
        ErrorClass::ResourceLimit
    );

    // The opaque floor still closes the deep source exactly.
    let fx = Fixture::new("deep", &two);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn interleaved_fixtures_do_not_alias() {
    // ADR-0060: a source-reading node must carry its own source-identity input.
    // Two fixtures with distinct sources share one store, interleaved; each must
    // answer from its own spans (never the other document's bytes).
    let root = temp_dir("alias");
    let mut store = FieldStore::open(&root).unwrap();

    let a = b"{\"msg\":\"aaa\"}\n{\"n\":1}\n".as_slice();
    let b = b"{\"msg\":\"bbb\"}\n{\"n\":2}\n".as_slice();
    let da = opaque_descriptor(a);
    let db = opaque_descriptor(b);
    let fa = ingest_pdf(&mut store, &da, Limits::DEFAULT).unwrap().field;
    let fb = ingest_pdf(&mut store, &db, Limits::DEFAULT).unwrap().field;

    for (field, src, want) in [(fa, a, b"\"aaa\"".to_vec()), (fb, b, b"\"bbb\"".to_vec())] {
        let got = answer_bytes(&observe_ok(
            &mut store,
            &field,
            ptr("0:/msg"),
            Representation::ExactBytes,
        ));
        assert_eq!(got, want, "aliased source for {field:?}");
        let field_handle = Field::open(&store, &field, Limits::DEFAULT).unwrap();
        assert_eq!(
            field_handle.materialize_exact(Limits::DEFAULT).unwrap(),
            src
        );
    }
    drop(store);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn jsonl_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived JSONL model reads the source bytes, so its single
    // dependency must be the exact `DocumentExact` root (keyed by
    // `sha256(source)`). This is what prevents cross-field source aliasing.
    let fx = Fixture::new("adr0060", DOC);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_JSONL_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::JsonlModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the JSONL model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0x0BAD_F00D_DEAD_BEEF;
    for _ in 0..128 {
        let mut buf = vec![0u8; 512];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        if let Ok(m) = vole_document::adapter::jsonl::parse(&buf, Limits::STRICT) {
            let _ = vole_document::adapter::jsonl::canonical_text(&m, &buf);
            let _ = vole_document::adapter::jsonl::find(&m, &buf, "a", Limits::STRICT);
        }
        let _ = vole_document::adapter::jsonl::build_jsonl_model(&buf, Limits::STRICT);
    }
}
