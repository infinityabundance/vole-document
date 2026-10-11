//! Phase 21.26.3 court: the MDX (Markdown + JSX/ESM) adapter.
//!
//! Gated on `mdx`, so a build without the feature compiles an empty target.
//!
//! MDX is a **superset** of Markdown that reuses the Markdown parser/model and layers
//! the MDX-specific constructs on top. MDX has no package layer: the exact leaf is the
//! whole source (a `DocumentExact`), and every derived observation is a bounded,
//! representation-preserving projection of it. The court checks:
//!
//! 1. **Byte-based detection is conservative and MDX-specific**: a source carrying an
//!    MDX signal (a top-level ESM statement, a JSX component/fragment, a JSX-specific
//!    attribute, or a whole-line block expression) is detected; plain Markdown, plain
//!    HTML, and plain prose are not (the Markdown control stays `Markdown`, the HTML
//!    control stays `Html`, prose stays `Opaque`).
//! 2. **Representation preservation**: exact ESM/JSX/expression spans and bytes, exact
//!    Markdown block/inline bytes and spans for the reused prose surface, JSX
//!    attributes and nested children, fragments, and brace-balanced expressions
//!    (including braces inside strings).
//! 3. **Selectors**: native
//!    `mdx-heading`/`mdx-block`/`mdx-esm`/`mdx-jsx`/`mdx-expression`/`mdx-find` and
//!    common `metadata`/`text`/`heading`/`block`/`search-match` resolve.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 5. **Fail closed**: a JSX bomb declines typed (resource limit); random bytes never
//!    panic.
//! 6. **No cross-field aliasing** (ADR-0060): two MDX fixtures interleaved in one
//!    store each answer from their own source, and the model node depends on the
//!    exact root.

#![cfg(all(feature = "field", feature = "mdx"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_MDX_MODEL, SelectorKey, lookup};
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
        "vole-mdx-{label}-{}-{}",
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
        format_basis: "opaque:mdx-test".to_string(),
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

fn esm(index: u32) -> Selector {
    Selector::MdxEsm { index }
}

fn jsx(index: u32) -> Selector {
    Selector::MdxJsx { index }
}

fn expression(index: u32) -> Selector {
    Selector::MdxExpression { index }
}

// A fixture with a default + named multi-line ESM import, an `export const`, an ATX
// heading, a paragraph with inline markup, a JSX component with attributes and a
// nested child plus a child expression, a fragment, a whole-line block expression
// comment, and a fenced code block that contains JSX-looking text (which must **not**
// be read as JSX).
const DOC: &[u8] = b"import React, {\n  useState,\n  useEffect,\n} from 'react'\nexport const meta = { title: 'Doc' }\n\n# Heading One\n\nSome *emphasis* and a [link](http://ex \"t\").\n\n<Widget name=\"w\" count={3}>\n  <Item.Child />\n  {frontmatter.title}\n</Widget>\n\n<>\n  fragment body\n</>\n\n{/* a comment */}\n\n```js\nconst x = <Foo />\n```\n";

#[test]
fn detection_is_byte_based_and_conservative() {
    assert_eq!(
        detect_document_format(DOC, Limits::DEFAULT),
        DocumentFormat::Mdx
    );
    // A JSX component alone is an MDX signal.
    assert_eq!(
        detect_document_format(b"# T\n\n<Foo />\n", Limits::DEFAULT),
        DocumentFormat::Mdx
    );
    // A fragment alone is an MDX signal.
    assert_eq!(
        detect_document_format(b"<>a</>\n", Limits::DEFAULT),
        DocumentFormat::Mdx
    );
    // A JSX-specific attribute (a brace value) is an MDX signal even on a lowercase tag.
    assert_eq!(
        detect_document_format(b"<div className={x}>y</div>\n", Limits::DEFAULT),
        DocumentFormat::Mdx
    );
    // A whole-line block expression is an MDX signal **when** the reused Markdown
    // model also carries a structural mark.
    assert_eq!(
        detect_document_format(b"# T\n\n{frontmatter.title}\n", Limits::DEFAULT),
        DocumentFormat::Mdx
    );
    // A top-level ESM statement is an MDX signal.
    assert_eq!(
        detect_document_format(
            b"import A from './a'\n\nexport default A\n",
            Limits::DEFAULT
        ),
        DocumentFormat::Mdx
    );

    // A plain Markdown document (a heading) stays Markdown.
    assert_eq!(
        detect_document_format(b"# Title\n\nplain *markdown* paragraph\n", Limits::DEFAULT),
        DocumentFormat::Markdown
    );
    // A Markdown document whose inline code contains JSX-looking text stays Markdown.
    assert_eq!(
        detect_document_format(b"# Title\n\nuse `<Foo />` here\n", Limits::DEFAULT),
        DocumentFormat::Markdown
    );
    // An inline `{x}` expression alone does not admit MDX.
    assert_eq!(
        detect_document_format(b"# Title\n\nvalue is {x} here\n", Limits::DEFAULT),
        DocumentFormat::Markdown
    );
    // A brace-bearing JSON-shaped blob with no Markdown structural mark stays Opaque.
    assert_eq!(
        detect_document_format(b"{\"a\":1}\n{oops}\n", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    // Plain prose has neither an MDX nor a Markdown structural signal: Opaque.
    for prose in [
        &b"just some prose\nwith more lines\n"[..],
        &b"Hello, world.\nThis is a test.\n"[..],
        &b""[..],
    ] {
        assert_eq!(
            detect_document_format(prose, Limits::DEFAULT),
            DocumentFormat::Opaque,
            "{prose:?}"
        );
    }

    // A plain HTML document stays Html (requires the html feature to classify).
    #[cfg(feature = "html")]
    assert_eq!(
        detect_document_format(
            b"<!doctype html>\n<html><body><h1>Hi</h1></body></html>\n",
            Limits::DEFAULT
        ),
        DocumentFormat::Html
    );

    let caps = capabilities_for_format(DocumentFormat::Mdx);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "heading", "block", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in [
        "mdx-heading",
        "mdx-block",
        "mdx-esm",
        "mdx-jsx",
        "mdx-expression",
        "mdx-find",
    ] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"mdx\""));
}

#[test]
fn preserves_esm_jsx_and_expressions() {
    let mut fx = Fixture::new("preserve", DOC);
    let field = fx.report.field;

    // The multi-line ESM import keeps its exact span and bytes.
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        esm(0),
        Representation::ExactBytes,
    ));
    assert_eq!(
        bytes,
        b"import React, {\n  useState,\n  useEffect,\n} from 'react'"
    );
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        esm(0),
        Representation::Metadata,
    ));
    assert!(j.contains("\"kind\":\"import\""), "{j}");
    // The `export const` statement.
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        esm(1),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"export const meta = { title: 'Doc' }");

    // The component element: exact whole span, name, attributes, nested child.
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        jsx(0),
        Representation::ExactBytes,
    ));
    assert_eq!(
        bytes,
        b"<Widget name=\"w\" count={3}>\n  <Item.Child />\n  {frontmatter.title}\n</Widget>"
    );
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        jsx(0),
        Representation::Metadata,
    ));
    assert!(j.contains("\"name\":\"Widget\""), "{j}");
    assert!(j.contains("\"component\":true"), "{j}");
    assert!(j.contains("\"jsx_attr\":true"), "{j}");
    assert!(j.contains("\"attrs\":2"), "{j}");
    assert!(j.contains("\"children\":1"), "{j}");

    // A nested namespaced component child.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        jsx(1),
        Representation::Metadata,
    ));
    assert!(j.contains("\"name\":\"Item.Child\""), "{j}");
    assert!(j.contains("\"namespaced\":true"), "{j}");
    assert!(j.contains("\"self_closing\":true"), "{j}");

    // The fragment.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        jsx(2),
        Representation::Metadata,
    ));
    assert!(j.contains("\"kind\":\"fragment\""), "{j}");

    // The block-expression comment is preserved verbatim.
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        expression(1),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"{/* a comment */}");
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        expression(1),
        Representation::Metadata,
    ));
    assert!(j.contains("\"comment\":true"), "{j}");

    // A JSX-looking tag inside a fenced code block is **not** a JSX element: only the
    // three real elements (Widget, Item.Child, fragment) are recorded.
    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"jsx\":3"), "{meta}");
    assert!(meta.contains("\"esm\":2"), "{meta}");
    assert!(meta.contains("\"expressions\":2"), "{meta}");
    assert!(meta.contains("\"has_block_expression\":true"), "{meta}");

    // The reused Markdown surface still resolves.
    let h = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Heading(0),
        Representation::Text,
    ));
    assert_eq!(h, "Heading One");
}

#[test]
fn native_selectors_resolve_and_decline() {
    let mut fx = Fixture::new("native", DOC);
    let field = fx.report.field;

    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::MdxHeading { index: 0 },
        Representation::Metadata,
    ));
    assert!(j.contains("\"level\":1"), "{j}");
    let b = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::MdxBlock { index: 0 },
        Representation::Text,
    ));
    // Block 0 is the multi-line ESM import (a paragraph in the reused Markdown model).
    assert!(b.contains("import React"), "{b}");
    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::MdxFind {
            pattern: "Heading".to_string(),
        },
        Representation::Text,
    ));
    assert!(found.contains("\"matches\":["), "{found}");

    // Out-of-range native selectors decline typed.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            esm(99),
            Representation::Metadata,
            Limits::DEFAULT
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            jsx(99),
            Representation::Text,
            Limits::DEFAULT
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            expression(99),
            Representation::Text,
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
    assert_eq!(text.as_bytes(), DOC);

    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    assert!(meta.contains("\"format\":\"mdx\""), "{meta}");
    assert!(meta.contains("\"headings\":1"), "{meta}");

    let h = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Heading(0),
        Representation::Text,
    ));
    assert_eq!(h, "Heading One");

    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("Heading".to_string()),
        Representation::Text,
    ));
    assert!(found.contains("\"matches\":"), "{found}");

    // A table common selector is not in the MDX capability set: decline.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            Selector::Table(0),
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
    let src_path = root.join("doc.mdx");
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
    let j = answer_json(&observe_ok(
        &mut store2,
        &field_id,
        esm(0),
        Representation::Metadata,
    ));
    assert!(j.contains("\"kind\":\"import\""), "{j}");

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), DOC.len());
    assert_eq!(sha256(&exact), sha256(DOC));
    assert_eq!(exact, DOC);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn jsx_bomb_declines_typed_but_exactness_holds() {
    // More JSX elements than the STRICT JSX cap. The document is ingested under
    // DEFAULT (detection and exactness are unaffected); the *model build* declines
    // typed under STRICT rather than allocating.
    let mut bomb = String::new();
    for _ in 0..20_000 {
        bomb.push_str("<A />\n\n");
    }
    let bomb = bomb.into_bytes();
    let mut fx = Fixture::new("bomb", &bomb);
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
fn interleaved_fixtures_do_not_alias() {
    // ADR-0060: a source-reading node must carry its own source-identity input.
    let root = temp_dir("alias");
    let mut store = FieldStore::open(&root).unwrap();

    let a: &[u8] = b"# Alpha\n\n<Alpha />\n";
    let b: &[u8] = b"# Beta\n\n<Beta />\n";
    let da = opaque_descriptor(a);
    let db = opaque_descriptor(b);
    let fa = ingest_pdf(&mut store, &da, Limits::DEFAULT).unwrap().field;
    let fb = ingest_pdf(&mut store, &db, Limits::DEFAULT).unwrap().field;

    for (field, src, want) in [(fa, a, "Alpha"), (fb, b, "Beta")] {
        let got = answer_text(&observe_ok(
            &mut store,
            &field,
            jsx(0),
            Representation::Text,
        ));
        assert_eq!(got, format!("<{want} />"), "aliased source for {field:?}");
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
fn mdx_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived MDX model reads the source bytes, so its single
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
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_MDX_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::MdxModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the MDX model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0x0021_2603_5EED_C0DE;
    for _ in 0..128 {
        let mut buf = vec![0u8; 512];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        if let Ok(m) = vole_document::adapter::mdx::parse(&buf, Limits::STRICT, true) {
            let _ = m.encode();
            let _ = vole_document::adapter::mdx::find(&buf, &m, "a", 1 << 20);
        }
    }
}
