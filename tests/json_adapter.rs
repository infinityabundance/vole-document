//! Phase 21.5.1 court: the JSON structured-tree adapter (the first Wave-2 format).
//!
//! Gated on `json`, so a build without the feature compiles an empty target.
//!
//! JSON has no package layer: the exact leaf is the whole source (a
//! `DocumentExact`), and every derived observation is a bounded,
//! representation-preserving projection of it. The court checks:
//!
//! 1. **Byte-based detection**: a JSON document is detected; a `[`-prefixed
//!    non-JSON byte string and a non-JSON binary are Opaque; malformed JSON is
//!    Opaque (never a panic).
//! 2. **Representation preservation**: numeric spelling, string-escape spelling,
//!    member order, and duplicate keys are all preserved, and exact source spans
//!    are reported.
//! 3. **Selectors**: native `json-pointer`/`json-node`/`json-find` and common
//!    `metadata`/`text`/`search-match` resolve.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 5. **Fail closed**: deep nesting declines typed (resource limit); random bytes
//!    never panic.
//! 6. **No cross-field aliasing** (ADR-0060): two JSON fixtures interleaved in one
//!    store each answer from their own source spans.

#![cfg(all(feature = "field", feature = "json"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_JSON_MODEL, SelectorKey, lookup};
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
        "vole-json-{label}-{}-{}",
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
        format_basis: "opaque:json-test".to_string(),
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

fn pointer(p: &str) -> Selector {
    Selector::JsonPointer {
        pointer: p.to_string(),
    }
}

const DOC: &[u8] = br#"{
  "b": 1e3,
  "a": 1.0,
  "a": -0,
  "c": "\u00e9",
  "list": [true, false, null, 42],
  "a/b": 7,
  "c~d": 8
}"#;

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    assert_eq!(
        detect_document_format(DOC, Limits::DEFAULT),
        DocumentFormat::Json
    );
    assert_eq!(
        detect_document_format(b"42", Limits::DEFAULT),
        DocumentFormat::Json
    );
    // A `[`-prefixed byte string that is not valid JSON is Opaque (not a guess).
    assert_eq!(
        detect_document_format(b"[1, 2, ", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    // A binary blob is Opaque.
    let mut bin = vec![0u8; 64];
    bin[0] = 0x7F;
    bin[1] = b'E';
    bin[2] = b'L';
    bin[3] = b'F';
    assert_eq!(
        detect_document_format(&bin, Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    // Malformed JSON must never panic; with the YAML adapter compiled it is
    // simply not JSON (a few of these are valid YAML, e.g. `{"a":}`).
    for bad in [
        &b""[..],
        &b"{"[..],
        &b"{\"a\":}"[..],
        &b"{\"a\":1} trailing"[..],
        &b"01"[..],
        &b"nul"[..],
    ] {
        assert_ne!(
            detect_document_format(bad, Limits::DEFAULT),
            DocumentFormat::Json,
            "{bad:?}"
        );
    }

    let caps = capabilities_for_format(DocumentFormat::Json);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in ["json-pointer", "json-node", "json-find"] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"json\""));
}

#[test]
fn model_preserves_spelling_order_and_duplicate_keys() {
    // The model itself (through the field) keeps the literal spelling and order.
    let m = vole_document::adapter::json::parse(DOC, Limits::DEFAULT, true).unwrap();
    let txt = vole_document::adapter::json::canonical_text(&m, DOC).unwrap();
    assert!(txt.contains("\"b\":1e3"), "{txt}");
    assert!(txt.contains("\"a\":1.0"), "{txt}");
    assert!(txt.contains("\"a\":-0"), "{txt}");
    assert!(txt.contains("\\u00e9"), "{txt}");
    // Member order is the source order, not sorted.
    let b_pos = txt.find("\"b\"").unwrap();
    let a_pos = txt.find("\"a\"").unwrap();
    assert!(b_pos < a_pos, "{txt}");

    // Through the field: duplicate keys are reported, never collapsed.
    let mut fx = Fixture::new("dup", DOC);
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &fx.report.field,
        pointer("/a"),
        Representation::Metadata,
    ));
    assert!(j.contains("\"matches\":2"), "{j}");
    // The first `a` wins pointer resolution: its token is `1.0`.
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &fx.report.field,
        pointer("/a"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"1.0");
}

#[test]
fn pointer_resolves_rfc6901_escapes_indices_and_spans() {
    let mut fx = Fixture::new("ptr", DOC);
    let field = fx.report.field;

    // Array index (Q5): /list/3 == 42.
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        pointer("/list/3"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"42");

    // `~1` decodes to `/`; `~0` to `~`.
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        pointer("/a~1b"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"7");
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        pointer("/c~0d"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"8");

    // The reported span is the exact source span of the token: slicing the source
    // at that span reproduces the token.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        pointer("/c"),
        Representation::Metadata,
    ));
    let (start, end) = parse_span(&j);
    assert_eq!(&DOC[start..end], br#""\u00e9""#);

    // Missing members and out-of-range indices decline typed (never empty).
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            pointer("/nope"),
            Representation::Metadata
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            pointer("/list/9"),
            Representation::Metadata
        ),
        ErrorClass::UnsupportedFeature
    );
    // A malformed pointer is a usage error.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            pointer("no-slash"),
            Representation::Metadata
        ),
        ErrorClass::Usage
    );
}

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

#[test]
fn node_structure_key_span_and_kind_resolve() {
    let mut fx = Fixture::new("node", DOC);
    let field = fx.report.field;
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::JsonNode {
            pointer: "/list".to_string(),
        },
        Representation::Structure,
    ));
    assert!(j.contains("\"kind\":\"array\""), "{j}");
    assert!(j.contains("\"elements\""), "{j}");
    // The root object reports member key/value spans and duplicate keys.
    let r = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::JsonNode {
            pointer: String::new(),
        },
        Representation::Structure,
    ));
    assert!(r.contains("\"kind\":\"object\""), "{r}");
    assert!(r.contains("\"key\":\"a\""), "{r}");
    assert!(r.contains("\"duplicate_keys\":[\"a\"]"), "{r}");
}

#[test]
fn common_metadata_text_and_search_resolve() {
    let mut fx = Fixture::new("common", DOC);
    let field = fx.report.field;
    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"format\":\"json\""), "{meta}");
    assert!(meta.contains("\"top_type\":\"object\""), "{meta}");
    assert!(meta.contains("\"duplicate_keys\":1"), "{meta}");

    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(
        text.starts_with('{') && text.contains("\"b\":1e3"),
        "{text}"
    );

    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("list".to_string()),
        Representation::Text,
    ));
    assert!(found.contains("\"role\":\"key\""), "{found}");
    assert!(found.contains("\"pointer\":\"/list\""), "{found}");
}

#[test]
fn native_find_reports_key_and_value_matches() {
    let mut fx = Fixture::new("find", DOC);
    let field = fx.report.field;
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::JsonFind {
            pattern: "c".to_string(),
        },
        Representation::Text,
    ));
    assert!(j.contains("\"role\":\"key\""), "{j}");
    assert!(j.contains("\"pointer\":\"/c\""), "{j}");
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
    let src_path = root.join("doc.json");
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
        pointer("/list/3"),
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
fn deep_nesting_declines_typed() {
    let mut deep = vec![b'['; 200];
    deep.push(b'0');
    deep.extend(std::iter::repeat_n(b']', 200));
    // STRICT caps the depth at 64, so a 200-deep document is not JSON.
    assert_eq!(
        detect_document_format(&deep, Limits::STRICT),
        DocumentFormat::Opaque
    );
    // The parser declines typed (resource limit), never panics.
    let e = vole_document::adapter::json::parse(&deep, Limits::STRICT, true).unwrap_err();
    assert_eq!(e.class(), ErrorClass::ResourceLimit);
    // Exactness is independent of the derived decline.
    let fx = Fixture::new("deep", &deep);
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

    let a = br#"{"k":"aaa","n":1}"#;
    let b = br#"{"k":"bbb","n":2}"#;
    let da = opaque_descriptor(a);
    let db = opaque_descriptor(b);
    let fa = ingest_pdf(&mut store, &da, Limits::DEFAULT).unwrap().field;
    let fb = ingest_pdf(&mut store, &db, Limits::DEFAULT).unwrap().field;

    for (field, src, want) in [(fa, a, b"\"aaa\"".to_vec()), (fb, b, b"\"bbb\"".to_vec())] {
        let got = answer_bytes(&observe_ok(
            &mut store,
            &field,
            pointer("/k"),
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
fn json_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived JSON model reads the source bytes, so its single
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
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_JSON_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::JsonModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the JSON model must depend on exactly the exact root"
    );
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
        if let Ok(m) = vole_document::adapter::json::parse(&buf, Limits::STRICT, true) {
            let _ = vole_document::adapter::json::canonical_text(&m, &buf);
            let _ = vole_document::adapter::json::build_json_model(&buf, Limits::STRICT);
        }
    }
}
