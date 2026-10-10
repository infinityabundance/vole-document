//! Phase 21.10 court: the error-recovering HTML adapter (a Wave-2 markup format).
//!
//! Gated on `html`, so a build without the feature compiles an empty target.
//!
//! HTML has no package layer: the exact leaf is the whole source (a
//! `DocumentExact`), and every derived observation is a bounded,
//! representation-preserving, **error-recovering** projection of it. The court
//! checks:
//!
//! 1. **Byte-based conservative detection**: an HTML document is detected; a
//!    plain-prose source, a non-HTML `<`-junk source, and a source carrying only a
//!    couple of known tags are Opaque; a DOCTYPE with an internal subset (an entity
//!    surface) is refused. XML is tried first, so a fully well-formed XHTML source
//!    stays `Xml`.
//! 2. **Representation preservation**: element/attribute/text/comment/DOCTYPE spans,
//!    attribute quoting (double/single/unquoted/boolean), void elements, and raw
//!    `<script>`/`<style>` content captured as bytes.
//! 3. **Error recovery**: implicit tag closing, stray end tags, and unterminated
//!    constructs parse without a panic.
//! 4. **Scripts are never executed or parsed**: `</div>` inside a `<script>` does
//!    not create an element, and the raw bytes are returned verbatim.
//! 5. **Selectors**: native `html-path`/`html-element`/`html-attr`/`html-scripts`/
//!    `html-find` plus common `metadata`/`text`/`heading`/`link`/`search-match`.
//! 6. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 7. **No cross-field aliasing** (ADR-0060): two HTML fixtures interleaved in one
//!    store each answer from their own source spans.

#![cfg(all(feature = "field", feature = "html"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_HTML_MODEL, SelectorKey, lookup};
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
        "vole-html-{label}-{}-{}",
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
        format_basis: "opaque:html-test".to_string(),
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
    Selector::HtmlPath {
        path: p.to_string(),
    }
}

const DOC: &[u8] = br#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>basic</title>
  <style>body { color: red; }</style>
</head>
<body id="main" data-x='1' hidden>
  <h1>Title</h1>
  <p>hello<br>world</p>
  <!-- a comment -->
  <p>a &amp; b</p>
  <a href="http://example.invalid/x">link text</a>
  <script>var s = "</div>"; if (1 < 2 && 3 > 2) { alert(s); }</script>
</body>
</html>
"#;

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    assert_eq!(
        detect_document_format(DOC, Limits::DEFAULT),
        DocumentFormat::Html
    );
    assert_eq!(
        detect_document_format(b"<html><body><br></body></html>", Limits::DEFAULT),
        DocumentFormat::Html
    );
    // A document-level HTML marker (an `<html>` root, or a `<!doctype html>`) wins
    // `Html` over the generic `Xml` fallback, even when the source is also fully
    // well-formed XML (XHTML).
    assert_eq!(
        detect_document_format(b"<html><body><p>x</p></body></html>", Limits::DEFAULT),
        DocumentFormat::Html
    );
    assert_eq!(
        detect_document_format(
            b"<!DOCTYPE html><html><body><p>x</p></body></html>",
            Limits::DEFAULT
        ),
        DocumentFormat::Html
    );
    // A bare XML tree whose root is not `html` stays Xml.
    assert_eq!(
        detect_document_format(b"<catalog><item>x</item></catalog>", Limits::DEFAULT),
        DocumentFormat::Xml
    );
    // An XML tree that only *mentions* `<html>` as a non-root descendant is Xml: the
    // marker rule requires the root element (or a doctype), never a nested tag.
    assert_eq!(
        detect_document_format(
            b"<xsl:stylesheet xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\"><xsl:template><html/></xsl:template></xsl:stylesheet>",
            Limits::DEFAULT
        ),
        DocumentFormat::Xml
    );
    // Plain prose and non-HTML `<`-junk stay Opaque.
    assert_eq!(
        detect_document_format(b"plain prose paragraph", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    assert_eq!(
        detect_document_format(b"<<< not html >>> <a <b> <<", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    assert_eq!(
        detect_document_format(b"#include <stdio.h>\nint main(){}\n", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    // A DOCTYPE internal subset (entity surface) is refused -> Opaque.
    assert_eq!(
        detect_document_format(
            b"<!DOCTYPE html [<!ENTITY x \"y\">]><html><body></body></html>",
            Limits::DEFAULT
        ),
        DocumentFormat::Opaque
    );
    // HTML here is UTF-8 only: a UTF-16 BOM or a non-UTF-8 byte string is Opaque.
    assert_eq!(
        detect_document_format(&[0xFF, 0xFE, b'<', b'h', b't', b'm', b'l'], Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    assert_eq!(
        detect_document_format(b"<html><body>\xC3\x28</body></html>", Limits::DEFAULT),
        DocumentFormat::Opaque
    );

    let caps = capabilities_for_format(DocumentFormat::Html);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "heading", "link", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in [
        "html-path",
        "html-element",
        "html-attr",
        "html-scripts",
        "html-find",
    ] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"html\""));
}

#[test]
fn internal_subset_doctype_declines_typed_but_the_floor_closes_exactly() {
    let bomb: &[u8] = b"<!DOCTYPE html [<!ENTITY a \"x\">]><html><body>&a;</body></html>";
    assert_eq!(
        detect_document_format(bomb, Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    let e = vole_document::adapter::html::parse(bomb, Limits::DEFAULT, true).unwrap_err();
    assert_eq!(e.class(), ErrorClass::InvalidHtmlStructure);
    let fx = Fixture::new("bomb", bomb);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn model_preserves_spans_attributes_and_raw_text() {
    let m = vole_document::adapter::html::parse(DOC, Limits::DEFAULT, true).unwrap();
    use vole_document::adapter::html::{
        Q_NONE, Q_SINGLE, attr_value_bytes, canonical_text, element_name, raw_texts, resolve_attr,
        token_bytes,
    };
    let root = m.node(m.root).unwrap();
    assert_eq!(element_name(DOC, root).unwrap(), "html");
    // The DOCTYPE and a comment keep exact delimited spans.
    let doctype = m
        .nodes
        .iter()
        .find(|n| n.kind == vole_document::adapter::html::K_DOCTYPE)
        .unwrap();
    assert_eq!(token_bytes(DOC, doctype).unwrap(), b"<!DOCTYPE html>");
    let comment = m
        .nodes
        .iter()
        .find(|n| n.kind == vole_document::adapter::html::K_COMMENT)
        .unwrap();
    assert_eq!(token_bytes(DOC, comment).unwrap(), b"<!-- a comment -->");
    // Attribute quoting styles are preserved.
    let ra = resolve_attr(&m, DOC, "/html/body@data-x").unwrap();
    assert_eq!(m.attr(ra.index).unwrap().quote, Q_SINGLE);
    let rb = resolve_attr(&m, DOC, "/html/body@hidden").unwrap();
    assert_eq!(m.attr(rb.index).unwrap().quote, Q_NONE);
    let rc = resolve_attr(&m, DOC, "/html/head/meta@charset").unwrap();
    assert_eq!(
        attr_value_bytes(DOC, m.attr(rc.index).unwrap()).unwrap(),
        b"utf-8"
    );
    // Raw script/style content is captured verbatim, never parsed or executed.
    let raws = raw_texts(&m, DOC).unwrap();
    assert_eq!(raws.len(), 2);
    assert_eq!(raws[0].name, "style");
    assert_eq!(
        &DOC[raws[0].start as usize..raws[0].end as usize],
        b"body { color: red; }"
    );
    assert_eq!(raws[1].name, "script");
    // `</div>` inside the script did NOT create an element.
    assert!(m.nodes.iter().all(|n| {
        n.kind != vole_document::adapter::html::K_ELEMENT || element_name(DOC, n).unwrap() != "div"
    }));
    // Entity references are surfaced literally, never expanded.
    let text = canonical_text(&m, DOC).unwrap();
    assert!(text.contains("a &amp; b"), "{text}");
    assert!(
        !text.contains("color: red"),
        "raw style leaked into text: {text}"
    );
}

#[test]
fn error_recovery_parses_malformed_html() {
    // Implicit `<li>` closing and a missing `</ul>`.
    let src: &[u8] = b"<ul><li>a<li>b<li>c";
    let mut fx = Fixture::new("recover", src);
    let field = fx.report.field;
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        path("/ul/li[2]"),
        Representation::Text,
    ));
    assert_eq!(text, "b");
    // A stray end tag is ignored.
    let src2: &[u8] = b"<div><p>x</span>y";
    let m = vole_document::adapter::html::parse(src2, Limits::DEFAULT, true).unwrap();
    let t = vole_document::adapter::html::subtree_text(&m, src2, m.root).unwrap();
    assert_eq!(t, "xy");
}

#[test]
fn scripts_are_raw_and_never_parsed() {
    let src: &[u8] =
        b"<html><body><br><script>document.write(\"<div>no</div>\");</script><p>ok</p></body></html>";
    let mut fx = Fixture::new("scripts", src);
    let field = fx.report.field;
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        Selector::HtmlScripts,
        Representation::ExactBytes,
    ));
    assert_eq!(
        bytes, b"document.write(\"<div>no</div>\");",
        "script content must be raw"
    );
    let json = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::HtmlScripts,
        Representation::Metadata,
    ));
    assert!(json.contains("\"name\":\"script\""), "{json}");
}

#[test]
fn native_and_common_selectors_resolve() {
    let mut fx = Fixture::new("selectors", DOC);
    let field = fx.report.field;

    // A native element path, its exact bytes and text.
    assert_eq!(
        answer_bytes(&observe_ok(
            &mut fx.store,
            &field,
            path("/html/head/title"),
            Representation::ExactBytes,
        )),
        b"<title>basic</title>"
    );
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            path("/html/head/title"),
            Representation::Text,
        )),
        "basic"
    );

    // An attribute's exact value and span.
    let val = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        Selector::HtmlAttr {
            spec: "/html/body@id".to_string(),
        },
        Representation::ExactBytes,
    ));
    assert_eq!(val, b"main");
    let aj = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::HtmlAttr {
            spec: "/html/body@data-x".to_string(),
        },
        Representation::Metadata,
    ));
    assert!(aj.contains("\"quote\":\"single\""), "{aj}");

    // The structural view exposes spans and child kinds.
    let el = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::HtmlElement {
            path: "/html/body".to_string(),
        },
        Representation::Structure,
    ));
    assert!(el.contains("\"kind\":\"element\""), "{el}");
    assert!(el.contains("\"quote\":\"none\""), "{el}");

    // A void element has no close tag.
    let br = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        path("/html/body/p/br"),
        Representation::Metadata,
    ));
    assert!(br.contains("\"close_span\":[0,"), "{br}");

    // Lexical find reports element/attribute/text matches.
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::HtmlFind {
            pattern: "link".to_string(),
        },
        Representation::Text,
    ));
    assert!(found.contains("\"role\":\"text\""), "{found}");
    assert!(found.contains("link text"), "{found}");

    // Common metadata/text/heading/link/search-match.
    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"format\":\"html\""), "{meta}");
    assert!(meta.contains("\"headings\":1"), "{meta}");
    assert!(meta.contains("\"links\":1"), "{meta}");
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Text,
        Representation::Text,
    ));
    assert!(text.contains("hello"));
    assert!(text.contains("a &amp; b"));
    let heading = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Heading(0),
        Representation::Text,
    ));
    assert_eq!(heading, "Title");
    let link = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Link(0),
        Representation::Metadata,
    ));
    assert!(link.contains("http://example.invalid/x"), "{link}");
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("world".to_string()),
        Representation::Text,
    ));
    assert!(found.contains("\"matches\""), "{found}");

    // Missing/out-of-range/malformed paths decline typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            path("/html/nope"),
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
            Selector::Link(99),
            Representation::Metadata
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
    let src_path = root.join("doc.html");
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
        path("/html/head/title"),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"<title>basic</title>");

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
    deep.extend_from_slice(b"<html>");
    for _ in 0..200 {
        deep.extend_from_slice(b"<div>");
    }
    deep.extend_from_slice(b"x");
    for _ in 0..200 {
        deep.extend_from_slice(b"</div>");
    }
    deep.extend_from_slice(b"</html>");
    assert_eq!(
        detect_document_format(&deep, Limits::STRICT),
        DocumentFormat::Opaque
    );
    let e = vole_document::adapter::html::parse(&deep, Limits::STRICT, true).unwrap_err();
    assert_eq!(e.class(), ErrorClass::ResourceLimit);
    let fx = Fixture::new("deep", &deep);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn interleaved_fixtures_do_not_alias() {
    let root = temp_dir("alias");
    let mut store = FieldStore::open(&root).unwrap();

    let a = br#"<html><body><h1>aaa</h1><br></body></html>"#;
    let b = br#"<html><body><h1>bbb</h1><br></body></html>"#;
    let da = opaque_descriptor(a);
    let db = opaque_descriptor(b);
    let fa = ingest_pdf(&mut store, &da, Limits::DEFAULT).unwrap().field;
    let fb = ingest_pdf(&mut store, &db, Limits::DEFAULT).unwrap().field;

    for (field, src, want) in [(fa, a, "aaa".to_string()), (fb, b, "bbb".to_string())] {
        let got = answer_text(&observe_ok(
            &mut store,
            &field,
            path("/html/body/h1"),
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
fn html_model_node_depends_on_the_exact_root() {
    let fx = Fixture::new("adr0060", DOC);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_HTML_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::HtmlModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the HTML model must depend on exactly the exact root"
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
        if let Ok(m) = vole_document::adapter::html::parse(&buf, Limits::STRICT, true) {
            let _ = vole_document::adapter::html::canonical_text(&m, &buf);
            let _ = vole_document::adapter::html::build_html_model(&buf, Limits::STRICT);
            let _ = vole_document::adapter::html::raw_texts(&m, &buf);
        }
    }
}
