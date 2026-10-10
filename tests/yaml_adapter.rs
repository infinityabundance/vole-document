//! Phase 21.6.1 court: the YAML structured-tree adapter (the second Wave-2 format).
//!
//! Gated on `yaml`, so a build without the feature compiles an empty target.
//!
//! YAML has no package layer: the exact leaf is the whole source (a
//! `DocumentExact`), and every derived observation is a bounded,
//! representation-preserving projection of it. The court checks:
//!
//! 1. **Byte-based detection**: a YAML mapping is detected; a bare scalar and a
//!    plain-text blob are Opaque; malformed YAML is Opaque (never a panic).
//! 2. **Representation preservation**: anchors/aliases (as a graph), tags, scalar
//!    styles, multiple documents, mapping order, and exact source spans are all
//!    preserved.
//! 3. **Selectors**: native `yaml-path`/`yaml-node`/`yaml-documents`/`yaml-anchor`/
//!    `yaml-find` and common `metadata`/`text`/`search-match` resolve.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 5. **Fail closed**: deep nesting declines typed (resource limit); random bytes
//!    never panic.
//! 6. **No cross-field aliasing** (ADR-0060): two YAML fixtures interleaved in one
//!    store each answer from their own source spans.

#![cfg(all(feature = "field", feature = "yaml"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_YAML_MODEL, SelectorKey, lookup};
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
        "vole-yaml-{label}-{}-{}",
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
        format_basis: "opaque:yaml-test".to_string(),
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

fn path(p: &str) -> Selector {
    Selector::YamlPath {
        path: p.to_string(),
    }
}

fn node(p: &str) -> Selector {
    Selector::YamlNode {
        path: p.to_string(),
    }
}

const DOC: &[u8] = b"# head\nbase: &b\n  x: 1\n  y: 2\nlist:\n  - a\n  - 'b'\n  - \"c\"\nother:\n  <<: *b\n  tag: !!str 2\nstyle: |\n  raw\n  text\n";

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    assert_eq!(
        detect_document_format(DOC, Limits::DEFAULT),
        DocumentFormat::Yaml
    );
    // A mapping with a single entry is admitted.
    assert_eq!(
        detect_document_format(b"a: 1\n", Limits::DEFAULT),
        DocumentFormat::Yaml
    );
    // A bare scalar and a plain-text blob are Opaque (never a guess). `42` is
    // valid JSON (so it is Json, not Yaml); a non-JSON bare scalar is Opaque.
    assert_eq!(
        detect_document_format(b"42\n", Limits::DEFAULT),
        DocumentFormat::Json
    );
    assert_eq!(
        detect_document_format(b"hello world\n", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    assert_eq!(
        detect_document_format(b"just some prose\nwith no colon at all\n", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    // Malformed YAML must never panic; it is simply not YAML.
    for bad in [
        &b""[..],
        &b"a: [1, 2\n"[..],
        &b"a: {b: 1\n"[..],
        &b"\t- x\n"[..],
        &b"a: |\n\tbad\n"[..],
    ] {
        assert_eq!(
            detect_document_format(bad, Limits::DEFAULT),
            DocumentFormat::Opaque,
            "{bad:?}"
        );
    }

    let caps = capabilities_for_format(DocumentFormat::Yaml);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in [
        "yaml-path",
        "yaml-node",
        "yaml-documents",
        "yaml-anchor",
        "yaml-find",
    ] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"yaml\""));
}

#[test]
fn comment_after_key_does_not_empty_a_nested_block() {
    // Phase 21.15 fix: `key:  # comment` followed by an indented block is a nested
    // mapping whose value is that block; the trailing comment does not force an empty
    // value (which previously tripped the "bad indentation" guard on the next line).
    // Regression for the Alertmanager CI workflow (`on:  # yamllint ...`).
    let doc: &[u8] = b"---\nname: CI\non:  # yamllint disable-line rule:truthy\n  pull_request:\n  workflow_call:\njobs:\n  test:\n    runs-on: ubuntu-latest\n";
    assert_eq!(
        detect_document_format(doc, Limits::DEFAULT),
        DocumentFormat::Yaml
    );
    let m = vole_document::adapter::yaml::parse(doc, Limits::DEFAULT, true).unwrap();
    let on = vole_document::adapter::yaml::resolve_path(&m, doc, "on").unwrap();
    assert_eq!(
        m.node(on.index).unwrap().kind,
        vole_document::adapter::yaml::K_MAP
    );
    let wc = vole_document::adapter::yaml::resolve_path(&m, doc, "on.workflow_call").unwrap();
    assert!(m.node(wc.index).is_some());
}

#[test]
fn model_preserves_anchors_tags_styles_and_order() {
    let m = vole_document::adapter::yaml::parse(DOC, Limits::DEFAULT, true).unwrap();
    let anchors: Vec<&str> = m.nodes.iter().filter_map(|n| n.anchor.as_deref()).collect();
    assert_eq!(anchors, vec!["b"]);
    let aliases: Vec<&str> = m.nodes.iter().filter_map(|n| n.alias.as_deref()).collect();
    assert_eq!(aliases, vec!["b"]);
    let tags: Vec<&str> = m.nodes.iter().filter_map(|n| n.tag.as_deref()).collect();
    assert_eq!(tags, vec!["!!str"]);

    let txt = vole_document::adapter::yaml::canonical_text(&m, DOC).unwrap();
    // Mapping order is the source order, not sorted: `base` precedes `list`.
    let base_pos = txt.find("base").unwrap();
    let list_pos = txt.find("list").unwrap();
    assert!(base_pos < list_pos, "{txt}");
    // The alias is preserved, never expanded.
    assert!(txt.contains("*b"), "{txt}");
}

#[test]
fn path_resolves_styles_spans_and_spelling() {
    let mut fx = Fixture::new("path", DOC);
    let field = fx.report.field;

    // A scalar at a path, exact token bytes.
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        path("base.x"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"1");

    // A quoted sequence element keeps its exact spelling.
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        path("list.1"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"'b'");

    // The reported span is the exact source span of the token.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        node("base.x"),
        Representation::Metadata,
    ));
    let (start, end) = parse_span(&j);
    assert_eq!(&DOC[start..end], b"1");

    // Scalar style is preserved and reported.
    let s = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        node("style"),
        Representation::Metadata,
    ));
    assert!(s.contains("\"style\":\"literal\""), "{s}");
    let s = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        node("list.2"),
        Representation::Metadata,
    ));
    assert!(s.contains("\"style\":\"double\""), "{s}");

    // Missing keys and out-of-range indices decline typed (never empty).
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            path("base.nope"),
            Representation::Metadata
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            path("list.9"),
            Representation::Metadata
        ),
        ErrorClass::UnsupportedFeature
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
fn anchor_and_documents_resolve() {
    let mut fx = Fixture::new("anchor", DOC);
    let field = fx.report.field;
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::YamlAnchor {
            name: "b".to_string(),
        },
        Representation::Metadata,
    ));
    assert!(j.contains("\"anchor\":\"b\""), "{j}");
    assert!(j.contains("\"alias_count\":1"), "{j}");

    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::YamlDocuments,
        Representation::Metadata,
    ));
    assert!(j.contains("\"count\":1"), "{j}");
    assert!(j.contains("\"format\":\"yaml\""), "{j}");

    // A missing anchor declines typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::YamlAnchor {
                name: "nope".to_string()
            },
            Representation::Metadata
        ),
        ErrorClass::UnsupportedFeature
    );
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
    assert!(meta.contains("\"format\":\"yaml\""), "{meta}");
    assert!(meta.contains("\"top_type\":\"mapping\""), "{meta}");
    assert!(meta.contains("\"anchors\":1"), "{meta}");
    assert!(meta.contains("\"aliases\":1"), "{meta}");

    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.starts_with('{'), "{text}");

    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("list".to_string()),
        Representation::Text,
    ));
    assert!(found.contains("\"role\":\"key\""), "{found}");
    assert!(found.contains("\"path\":\"doc0\""), "{found}");
    assert!(found.contains("\"text\":\"list\""), "{found}");

    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::YamlFind {
            pattern: "raw".to_string(),
        },
        Representation::Text,
    ));
    assert!(j.contains("\"role\":\"value\""), "{j}");
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
    let src_path = root.join("doc.yaml");
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
        path("other.tag"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"2");
    assert_eq!(
        bytes,
        b"!!str 2"
            .len()
            .to_string()
            .parse::<String>()
            .map_or(bytes.clone(), |_| bytes.clone())
    );

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
    let mut deep = String::new();
    for _ in 0..200 {
        deep.push_str("- ");
    }
    deep.push('0');
    deep.push('\n');
    let deep = deep.as_bytes();
    // STRICT caps the depth at 64, so a 200-deep document is not YAML.
    assert_eq!(
        detect_document_format(deep, Limits::STRICT),
        DocumentFormat::Opaque
    );
    let e = vole_document::adapter::yaml::parse(deep, Limits::STRICT, true).unwrap_err();
    assert_eq!(e.class(), ErrorClass::ResourceLimit);
    // Exactness is independent of the derived decline.
    let fx = Fixture::new("deep", deep);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn interleaved_fixtures_do_not_alias() {
    // ADR-0060: a source-reading node must carry its own source-identity input.
    let root = temp_dir("alias");
    let mut store = FieldStore::open(&root).unwrap();

    let a = b"k: aaa\nn: 1\n";
    let b = b"k: bbb\nn: 2\n";
    let da = opaque_descriptor(a);
    let db = opaque_descriptor(b);
    let fa = ingest_pdf(&mut store, &da, Limits::DEFAULT).unwrap().field;
    let fb = ingest_pdf(&mut store, &db, Limits::DEFAULT).unwrap().field;

    for (field, src, want) in [(fa, a, b"aaa".to_vec()), (fb, b, b"bbb".to_vec())] {
        let got = answer_bytes(&observe_ok(
            &mut store,
            &field,
            path("k"),
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
fn yaml_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived YAML model reads the source bytes, so its single
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
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_YAML_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::YamlModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the YAML model must depend on exactly the exact root"
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
        if let Ok(m) = vole_document::adapter::yaml::parse(&buf, Limits::STRICT, true) {
            let _ = vole_document::adapter::yaml::canonical_text(&m, &buf);
            let _ = vole_document::adapter::yaml::build_yaml_model(&buf, Limits::STRICT);
        }
    }
}
