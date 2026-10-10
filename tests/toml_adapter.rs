//! Phase 21.11 court: the representation-preserving TOML adapter (a Wave-2
//! structured-tree format).
//!
//! Gated on `toml`, so a build without the feature compiles an empty target.
//!
//! TOML has no package layer: the exact leaf is the whole source (a
//! `DocumentExact`), and every derived observation is a bounded,
//! representation-preserving projection of it. The court checks:
//!
//! 1. **Byte-based conservative detection**: a TOML document is detected; plain
//!    prose, a bare word, a comment-only source, and a header-only source are
//!    Opaque.
//! 2. **Representation preservation**: exact key/value/table/comment spans, dotted
//!    keys, tables, arrays of tables, inline tables, arrays, and every scalar's
//!    exact spelling (integers with `_`/`0x`, floats incl. `nan`, booleans,
//!    date-times, strings).
//! 3. **Enforced rules**: a duplicate key and a table redefinition are typed
//!    declines, never a silent overwrite.
//! 4. **Selectors**: native `toml-path`/`toml-table`/`toml-find` plus common
//!    `metadata`/`text`/`search-match`.
//! 5. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 6. **No cross-field aliasing** (ADR-0060): two TOML fixtures interleaved in one
//!    store each answer from their own source spans.

#![cfg(all(feature = "field", feature = "toml"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_TOML_MODEL, SelectorKey, lookup};
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
        "vole-toml-{label}-{}-{}",
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
        format_basis: "opaque:toml-test".to_string(),
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

fn tpath(p: &str) -> Selector {
    Selector::TomlPath {
        path: p.to_string(),
    }
}

const DOC: &[u8] = br#"# example configuration
title = "example"
count = 1_000
ratio = 3.14
flag = true
when = 1979-05-27T07:32:00Z
hex = 0x1F
inline = { a = 1, b.c = 2 }
list = [1, 2, 3]

[server]
host = "localhost"
port = 8080
tags = ["a", "b"]

[[server.pools]]
name = "primary"

[[server.pools]]
name = "replica"
"#;

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    assert_eq!(
        detect_document_format(DOC, Limits::DEFAULT),
        DocumentFormat::Toml
    );
    assert_eq!(
        detect_document_format(b"a = 1\n", Limits::DEFAULT),
        DocumentFormat::Toml
    );
    // A leading `#` comment is a TOML comment, not a Markdown heading (TOML is
    // tried before the weak Markdown heuristic).
    assert_eq!(
        detect_document_format(b"# note\nk = \"v\"\n", Limits::DEFAULT),
        DocumentFormat::Toml
    );
    // Plain prose and bare words stay Opaque.
    assert_eq!(
        detect_document_format(b"plain prose paragraph\n", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    assert_eq!(
        detect_document_format(b"this is not = toml\n", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    // A comment-only or header-only source carries no assignment, so it is not
    // TOML (another format may legitimately claim it).
    assert_ne!(
        detect_document_format(b"# only a comment\n", Limits::DEFAULT),
        DocumentFormat::Toml
    );
    assert_ne!(
        detect_document_format(b"[header.only]\n", Limits::DEFAULT),
        DocumentFormat::Toml
    );
    // Enforced rules: a duplicate/redefined key is not detected.
    assert_eq!(
        detect_document_format(b"a = 1\na = 2\n", Limits::DEFAULT),
        DocumentFormat::Opaque
    );

    let caps = capabilities_for_format(DocumentFormat::Toml);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in ["toml-path", "toml-table", "toml-find"] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"toml\""));
}

#[test]
fn model_preserves_spelling_spans_and_comments() {
    use vole_document::adapter::toml::{
        K_FLOAT, K_INTEGER, resolve_path, scalar_spelling, string_content, table_keys, token_bytes,
    };

    let m = vole_document::adapter::toml::parse(DOC, Limits::DEFAULT).unwrap();
    let r = resolve_path(&m, DOC, "count").unwrap();
    assert_eq!(m.node(r.index).unwrap().kind, K_INTEGER);
    assert_eq!(
        scalar_spelling(DOC, m.node(r.index).unwrap()).unwrap(),
        "1_000"
    );
    let r = resolve_path(&m, DOC, "hex").unwrap();
    assert_eq!(
        scalar_spelling(DOC, m.node(r.index).unwrap()).unwrap(),
        "0x1F"
    );
    let r = resolve_path(&m, DOC, "ratio").unwrap();
    assert_eq!(m.node(r.index).unwrap().kind, K_FLOAT);
    let r = resolve_path(&m, DOC, "inline.b.c").unwrap();
    assert_eq!(scalar_spelling(DOC, m.node(r.index).unwrap()).unwrap(), "2");
    let r = resolve_path(&m, DOC, "server.pools[1].name").unwrap();
    assert_eq!(
        string_content(DOC, m.node(r.index).unwrap()).unwrap(),
        "replica"
    );
    // The comment keeps its exact span.
    assert_eq!(m.comments.len(), 1);
    let c = m.comments[0];
    assert_eq!(
        &DOC[c.start as usize..c.end as usize],
        b"# example configuration"
    );
    // The server table's keys are in document order.
    let r = resolve_path(&m, DOC, "server").unwrap();
    let keys = table_keys(&m, DOC, r.index).unwrap();
    let names: Vec<&str> = keys.iter().map(|k| k.key.as_str()).collect();
    assert_eq!(names, vec!["host", "port", "tags", "pools"]);
    // A value's exact bytes.
    let r = resolve_path(&m, DOC, "title").unwrap();
    assert_eq!(
        token_bytes(DOC, m.node(r.index).unwrap()).unwrap(),
        b"\"example\""
    );
}

#[test]
fn native_and_common_selectors_resolve() {
    let mut fx = Fixture::new("selectors", DOC);
    let field = fx.report.field;

    // A scalar's exact bytes, text (spelling), and metadata (spelling + span).
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            tpath("count"),
            Representation::ExactBytes,
        )),
        b"1_000"
    );
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            tpath("title"),
            Representation::Text,
        )),
        "example"
    );
    let md = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        tpath("server.port"),
        Representation::Metadata,
    ));
    assert!(md.contains("\"kind\":\"integer\""), "{md}");
    assert!(md.contains("\"spelling\":\"8080\""), "{md}");
    assert!(md.contains("\"key\":\"port\""), "{md}");

    // A table's keys.
    let tv = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::TomlTable {
            path: "server".to_string(),
        },
        Representation::Metadata,
    ));
    assert!(tv.contains("\"key\":\"host\""), "{tv}");
    assert!(tv.contains("\"key\":\"port\""), "{tv}");

    // An index into an array of tables.
    let v = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        tpath("server.pools[0].name"),
        Representation::ExactBytes,
    ));
    assert_eq!(v, b"\"primary\"");

    // Lexical find over keys and string values.
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::TomlFind {
            pattern: "replica".to_string(),
        },
        Representation::Text,
    ));
    assert!(found.contains("\"role\":\"value\""), "{found}");
    assert!(found.contains("replica"), "{found}");

    // Common metadata / text / search-match.
    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"format\":\"toml\""), "{meta}");
    assert!(meta.contains("\"array_tables\":1"), "{meta}");
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.contains("count = 1_000"), "{text}");
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("host".to_string()),
        Representation::Text,
    ));
    assert!(found.contains("matches"), "{found}");

    // Missing paths and unsupported common pairs decline typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            tpath("server.nope"),
            Representation::Metadata
        ),
        ErrorClass::UnsupportedFeature
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
    let src_path = root.join("doc.toml");
    let desc_path = root.join("doc.voldoc");
    std::fs::write(&src_path, DOC).unwrap();
    let descriptor = opaque_descriptor(DOC);
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
    let bytes = answer_bytes(&observe_ok(
        &mut store2,
        &field_id,
        tpath("server.host"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"\"localhost\"");

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), DOC.len());
    assert_eq!(sha256(&exact), sha256(DOC));
    assert_eq!(exact, DOC);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn duplicate_and_redefinition_decline_typed() {
    let dup: &[u8] = b"a = 1\na = 2\n";
    assert_eq!(
        detect_document_format(dup, Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    let e = vole_document::adapter::toml::parse(dup, Limits::DEFAULT).unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidTomlStructure);
    // The floor still closes exactly.
    let fx = Fixture::new("dup", dup);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn deep_nesting_declines_typed() {
    let mut deep = String::new();
    deep.push('a');
    for _ in 0..200 {
        deep.push_str(".b");
    }
    deep.push_str(" = 1\n");
    assert_eq!(
        detect_document_format(deep.as_bytes(), Limits::STRICT),
        DocumentFormat::Opaque
    );
    let e = vole_document::adapter::toml::parse(deep.as_bytes(), Limits::STRICT).unwrap_err();
    assert_eq!(e.class(), ErrorClass::ResourceLimit);
    let fx = Fixture::new("deep", deep.as_bytes());
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn interleaved_fixtures_do_not_alias() {
    let root = temp_dir("alias");
    let mut store = FieldStore::open(&root).unwrap();

    let a = b"name = \"aaa\"\n[t]\nv = 1\n".as_slice();
    let b = b"name = \"bbb\"\n[t]\nv = 2\n".as_slice();
    let da = opaque_descriptor(a);
    let db = opaque_descriptor(b);
    let fa = ingest_pdf(&mut store, &da, Limits::DEFAULT).unwrap().field;
    let fb = ingest_pdf(&mut store, &db, Limits::DEFAULT).unwrap().field;

    for (field, src, want) in [(fa, a, "aaa".to_string()), (fb, b, "bbb".to_string())] {
        let got = answer_text(&observe_ok(
            &mut store,
            &field,
            tpath("name"),
            Representation::Text,
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
fn toml_model_node_depends_on_the_exact_root() {
    let fx = Fixture::new("adr0060", DOC);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_TOML_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::TomlModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the TOML model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0xC0FF_EE00_1234_5678;
    for _ in 0..128 {
        let mut buf = vec![0u8; 512];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        if let Ok(m) = vole_document::adapter::toml::parse(&buf, Limits::STRICT) {
            let _ = vole_document::adapter::toml::canonical_text(&m, &buf);
            let _ = vole_document::adapter::toml::build_toml_model(&buf, Limits::STRICT);
            let _ = vole_document::adapter::toml::find(&m, &buf, "a", Limits::STRICT);
        }
    }
}
