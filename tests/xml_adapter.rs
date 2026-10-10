//! Phase 21.9 court: the XML structured-tree adapter (a Wave-2 format).
//!
//! Gated on `xml`, so a build without the feature compiles an empty target.
//!
//! XML has no package layer: the exact leaf is the whole source (a
//! `DocumentExact`), and every derived observation is a bounded,
//! representation-preserving projection of it. The court checks:
//!
//! 1. **Byte-based detection**: a well-formed XML document is detected; a
//!    `<-prefixed` non-XML byte string and plain prose are Opaque; a malformed
//!    document is Opaque (never a panic); a benign DOCTYPE is accepted.
//! 2. **Security**: an internal-subset DOCTYPE (XXE / billion-laughs surface) is
//!    refused, and no entity is ever expanded — the document stays Opaque and a
//!    direct parse declines typed.
//! 3. **Representation preservation**: element/attribute/text/CDATA/comment/PI
//!    exact source spans, namespace declarations, and literal entity references.
//! 4. **Selectors**: native `xml-path`/`xml-element`/`xml-attr`/`xml-namespaces`/
//!    `xml-find` and common `metadata`/`text`/`search-match` resolve.
//! 5. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 6. **No cross-field aliasing** (ADR-0060): two XML fixtures interleaved in one
//!    store each answer from their own source spans.

#![cfg(all(feature = "field", feature = "xml"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_XML_MODEL, SelectorKey, lookup};
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
        "vole-xml-{label}-{}-{}",
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
        format_basis: "opaque:xml-test".to_string(),
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
    Selector::XmlPath {
        path: p.to_string(),
    }
}

const DOC: &[u8] = br#"<?xml version="1.0"?>
<!-- a comment -->
<?target data?>
<doc xmlns="urn:default" xmlns:p="urn:p">
  <name>basic</name>
  <nested>
    <a id='1'><b>deep</b></a>
    <a id="2"><b>deeper</b></a>
  </nested>
  <cdata><![CDATA[a < b && c]]></cdata>
  <ents>a &amp; b</ents>
  <p:child p:attr="pv">prefixed</p:child>
</doc>
"#;

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    assert_eq!(
        detect_document_format(DOC, Limits::DEFAULT),
        DocumentFormat::Xml
    );
    assert_eq!(
        detect_document_format(b"<a><b>x</b></a>", Limits::DEFAULT),
        DocumentFormat::Xml
    );
    // A benign external DOCTYPE is accepted (and never fetched).
    assert_eq!(
        detect_document_format(
            b"<!DOCTYPE a PUBLIC \"p\" \"http://example.invalid/d.dtd\"><a/>",
            Limits::DEFAULT
        ),
        DocumentFormat::Xml
    );
    // `<-prefixed` junk and prose are Opaque (never a guess).
    assert_eq!(
        detect_document_format(b"<<< not xml >>>", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    assert_eq!(
        detect_document_format(b"plain prose paragraph", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    assert_eq!(
        detect_document_format(b"<a>", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    assert_eq!(
        detect_document_format(b"<a></b>", Limits::DEFAULT),
        DocumentFormat::Opaque
    );

    let caps = capabilities_for_format(DocumentFormat::Xml);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in [
        "xml-path",
        "xml-element",
        "xml-attr",
        "xml-namespaces",
        "xml-find",
    ] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"xml\""));
}

#[test]
fn xxe_and_billion_laughs_are_refused_never_expanded() {
    // An internal subset (where entities are declared) is refused outright: the
    // document is Opaque and a direct parse is a typed decline.
    let xxe: &[u8] = b"<!DOCTYPE f [<!ENTITY x SYSTEM \"file:///etc/passwd\">]><f>&x;</f>";
    let bomb: &[u8] = b"<!DOCTYPE l [<!ENTITY a \"x\"><!ENTITY b \"&a;&a;&a;&a;\">]><l>&b;</l>";
    for src in [xxe, bomb] {
        assert_eq!(
            detect_document_format(src, Limits::DEFAULT),
            DocumentFormat::Opaque
        );
        let e = vole_document::adapter::xml::parse(src, Limits::DEFAULT, true).unwrap_err();
        assert_eq!(e.class(), ErrorClass::InvalidXmlStructure);
        // The opaque floor still closes it exactly.
        let fx = Fixture::new("bomb", src);
        let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
        assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
    }
}

#[test]
fn model_preserves_element_attribute_cdata_and_entity_spelling() {
    let m = vole_document::adapter::xml::parse(DOC, Limits::DEFAULT, true).unwrap();
    // Element spans: the root's start tag begins at the opening `<`.
    let root = m.node(m.root).unwrap();
    assert_eq!(
        vole_document::adapter::xml::element_name(DOC, root).unwrap(),
        "doc"
    );
    assert_eq!(&DOC[root.start as usize..root.name_end as usize], b"<doc");
    // The comment and CDATA keep their exact delimited spans.
    let comment = m
        .nodes
        .iter()
        .find(|n| n.kind == vole_document::adapter::xml::K_COMMENT)
        .unwrap();
    assert_eq!(
        vole_document::adapter::xml::token_bytes(DOC, comment).unwrap(),
        b"<!-- a comment -->"
    );
    let cdata = m
        .nodes
        .iter()
        .find(|n| n.kind == vole_document::adapter::xml::K_CDATA)
        .unwrap();
    assert_eq!(
        vole_document::adapter::xml::content_bytes(DOC, cdata).unwrap(),
        b"a < b && c"
    );
    // Entity references are surfaced literally, never expanded.
    let ents = vole_document::adapter::xml::resolve_path(&m, DOC, "/doc/ents").unwrap();
    assert_eq!(
        vole_document::adapter::xml::subtree_text(&m, DOC, ents.index).unwrap(),
        "a &amp; b"
    );
}

#[test]
fn path_selectors_spans_and_attribute_values_resolve() {
    let mut fx = Fixture::new("path", DOC);
    let field = fx.report.field;

    // A positional element path.
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        path("/doc/nested/a[2]/b"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"<b>deeper</b>");
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        path("/doc/nested/a[2]/b"),
        Representation::Text,
    ));
    assert_eq!(text, "deeper");

    // The reported span is exact: slicing the source at it reproduces the element.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        path("/doc/name"),
        Representation::Metadata,
    ));
    let (start, end) = parse_span(&j);
    assert_eq!(&DOC[start..end], b"<name>basic</name>");

    // An attribute's exact quoted value (single quotes preserved in the span).
    let val = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        Selector::XmlAttr {
            spec: "/doc/nested/a@id".to_string(),
        },
        Representation::ExactBytes,
    ));
    assert_eq!(val, b"1");

    // Missing/out-of-range/malformed paths decline typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            path("/doc/nope"),
            Representation::Metadata
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            path("/doc/nested/a[9]"),
            Representation::Metadata
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            path("no-slash"),
            Representation::Metadata
        ),
        ErrorClass::Usage
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::XmlAttr {
                spec: "/doc@missing".to_string(),
            },
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
fn namespaces_element_structure_and_find_resolve() {
    let mut fx = Fixture::new("ns", DOC);
    let field = fx.report.field;

    // A prefixed element path resolves.
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        path("/doc/p:child"),
        Representation::Text,
    ));
    assert_eq!(text, "prefixed");

    // Namespace declarations are listed with their URIs.
    let ns = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::XmlNamespaces,
        Representation::Metadata,
    ));
    assert!(ns.contains("\"prefix\":\"\""), "{ns}");
    assert!(ns.contains("\"uri\":\"urn:default\""), "{ns}");
    assert!(ns.contains("\"prefix\":\"p\""), "{ns}");
    assert!(ns.contains("\"uri\":\"urn:p\""), "{ns}");

    // The structural view exposes attribute spans and the namespace flag.
    let el = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::XmlElement {
            path: "/doc".to_string(),
        },
        Representation::Structure,
    ));
    assert!(el.contains("\"kind\":\"element\""), "{el}");
    assert!(el.contains("\"ns_decl\":true"), "{el}");
    assert!(el.contains("\"name\":\"xmlns\""), "{el}");

    // A lexical find reports element/attribute/text matches.
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::XmlFind {
            pattern: "child".to_string(),
        },
        Representation::Text,
    ));
    assert!(found.contains("\"role\":\"element\""), "{found}");
    assert!(found.contains("p:child"), "{found}");
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
    assert!(meta.contains("\"format\":\"xml\""), "{meta}");
    assert!(meta.contains("\"elements\":"), "{meta}");
    assert!(meta.contains("\"namespaces\":2"), "{meta}");

    // Common text is the whole document's character data (entity refs literal).
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.contains("basic"), "{text}");
    assert!(text.contains("a &amp; b"), "{text}");

    // Common search-match maps to the lexical find.
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("nested".to_string()),
        Representation::Text,
    ));
    assert!(found.contains("\"matches\""), "{found}");
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
    let src_path = root.join("doc.xml");
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
        path("/doc/nested/a[2]/b"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"<b>deeper</b>");

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
    let mut deep = Vec::new();
    for _ in 0..200 {
        deep.extend_from_slice(b"<a>");
    }
    deep.extend_from_slice(b"x");
    for _ in 0..200 {
        deep.extend_from_slice(b"</a>");
    }
    assert_eq!(
        detect_document_format(&deep, Limits::STRICT),
        DocumentFormat::Opaque
    );
    let e = vole_document::adapter::xml::parse(&deep, Limits::STRICT, true).unwrap_err();
    assert_eq!(e.class(), ErrorClass::ResourceLimit);
    let fx = Fixture::new("deep", &deep);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn interleaved_fixtures_do_not_alias() {
    let root = temp_dir("alias");
    let mut store = FieldStore::open(&root).unwrap();

    let a = br#"<r><k>aaa</k></r>"#;
    let b = br#"<r><k>bbb</k></r>"#;
    let da = opaque_descriptor(a);
    let db = opaque_descriptor(b);
    let fa = ingest_pdf(&mut store, &da, Limits::DEFAULT).unwrap().field;
    let fb = ingest_pdf(&mut store, &db, Limits::DEFAULT).unwrap().field;

    for (field, src, want) in [(fa, a, b"aaa".to_vec()), (fb, b, b"bbb".to_vec())] {
        let got = answer_text(&observe_ok(
            &mut store,
            &field,
            path("/r/k"),
            Representation::Text,
        ));
        assert_eq!(got.as_bytes(), want, "aliased source for {field:?}");
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
fn xml_model_node_depends_on_the_exact_root() {
    let fx = Fixture::new("adr0060", DOC);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_XML_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::XmlModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the XML model must depend on exactly the exact root"
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
        if let Ok(m) = vole_document::adapter::xml::parse(&buf, Limits::STRICT, true) {
            let _ = vole_document::adapter::xml::canonical_text(&m, &buf);
            let _ = vole_document::adapter::xml::build_xml_model(&buf, Limits::STRICT);
        }
    }
}
