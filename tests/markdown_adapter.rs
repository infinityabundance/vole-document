//! Phase 21.8.1 court: the Markdown prose adapter (the first Wave-2 prose format).
//!
//! Gated on `markdown`, so a build without the feature compiles an empty target.
//!
//! Markdown has no package layer: the exact leaf is the whole source (a
//! `DocumentExact`), and every derived observation is a bounded,
//! representation-preserving projection of it. The court checks:
//!
//! 1. **Byte-based detection is conservative**: a source carrying a structural mark
//!    (an ATX heading, a fenced code block, front matter, a table, a reference
//!    definition, or a footnote definition) is detected; plain prose, an empty
//!    input, and a bare paragraph are all Opaque (never a guess, never a panic).
//! 2. **Representation preservation**: exact block/inline bytes and spans, heading
//!    levels, fenced code language tags, list items, table cells, links/images with
//!    targets and titles, reference definitions, footnotes, and front matter.
//! 3. **Selectors**: native `md-heading`/`md-block`/`md-code`/`md-link`/`md-find`
//!    and common `metadata`/`text`/`heading`/`block`/`search-match` resolve.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 5. **Fail closed**: a block bomb declines typed (resource limit); random bytes
//!    never panic.
//! 6. **No cross-field aliasing** (ADR-0060): two Markdown fixtures interleaved in
//!    one store each answer from their own source, and the model node depends on
//!    the exact root.

#![cfg(all(feature = "field", feature = "markdown"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_MARKDOWN_MODEL, SelectorKey, lookup};
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
        "vole-md-{label}-{}-{}",
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
        format_basis: "opaque:markdown-test".to_string(),
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

fn heading(index: u32) -> Selector {
    Selector::MdHeading { index }
}

fn block(index: u32) -> Selector {
    Selector::MdBlock { index }
}

fn code(index: u32) -> Selector {
    Selector::MdCode { index }
}

fn link(index: u32) -> Selector {
    Selector::MdLink { index }
}

fn find(pattern: &str) -> Selector {
    Selector::MdFind {
        pattern: pattern.to_string(),
    }
}

// A fixture with front matter, two headings, a paragraph with an inline link and a
// code span, a list, a Rust fence, a table, a reference definition, a reference
// link + image paragraph, and a footnote definition.
const DOC: &[u8] = b"---\ntitle: T\n---\n\n# Heading One\n\ntext with [link](http://ex \"ti\") and `code`.\n\n## Heading Two\n\n- item one\n- item two\n\n```rust\nlet x = 1;\n```\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n[ref]: http://ref\n\nsee [text][ref] and ![img](p.png \"t\").\n\n[^n]: a note\n";

fn span_of(json: &str) -> (usize, usize) {
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
        DocumentFormat::Markdown
    );
    // A table alone is a structural mark.
    assert_eq!(
        detect_document_format(b"| a | b |\n| - | - |\n| 1 | 2 |\n", Limits::DEFAULT),
        DocumentFormat::Markdown
    );
    // A fenced code block alone is a structural mark.
    assert_eq!(
        detect_document_format(b"```rust\nlet x = 1;\n```\n", Limits::DEFAULT),
        DocumentFormat::Markdown
    );
    // Plain prose (a valid Markdown paragraph) has no structural mark: Opaque.
    for prose in [
        &b"just some prose\nwith more lines\n"[..],
        &b"Hello, world.\nThis is a test.\n"[..],
        &b"a hyphen - appears here, and no colon at all,\n"[..],
        &b""[..],
    ] {
        assert_eq!(
            detect_document_format(prose, Limits::DEFAULT),
            DocumentFormat::Opaque,
            "{prose:?}"
        );
    }

    let caps = capabilities_for_format(DocumentFormat::Markdown);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "heading", "block", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in ["md-heading", "md-block", "md-code", "md-link", "md-find"] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"markdown\""));
}

#[test]
fn preserves_headings_code_links_and_front_matter() {
    let mut fx = Fixture::new("preserve", DOC);
    let field = fx.report.field;

    // Front matter is the first block, with its exact flavour and span.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        block(0),
        Representation::Metadata,
    ));
    assert!(j.contains("\"kind\":\"front-matter\""), "{j}");
    let (s, e) = span_of(&j);
    assert_eq!(&DOC[s..e], b"---\ntitle: T\n---");

    // A heading's exact content text and level.
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        heading(0),
        Representation::Text,
    ));
    assert_eq!(text, "Heading One");
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        heading(1),
        Representation::Metadata,
    ));
    assert!(j.contains("\"level\":2"), "{j}");
    let (cs, ce) = {
        let tag = "\"content_span\":[";
        let i = j.find(tag).unwrap() + tag.len();
        let rest = &j[i..];
        let close = rest.find(']').unwrap();
        let mut it = rest[..close].split(',');
        (
            it.next().unwrap().parse::<usize>().unwrap(),
            it.next().unwrap().parse::<usize>().unwrap(),
        )
    };
    assert_eq!(&DOC[cs..ce], b"Heading Two");

    // A heading block's exact source bytes include the marker.
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        block(1),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"# Heading One");

    // A fenced code block's exact content and language tag.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        code(0),
        Representation::Metadata,
    ));
    assert!(j.contains("\"language\":\"rust\""), "{j}");
    let content = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        code(0),
        Representation::Text,
    ));
    assert_eq!(content, "let x = 1;\n");

    // A list item's content text (marker stripped).
    let item = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        block(4),
        Representation::Text,
    ));
    assert_eq!(item, "item one");

    // An inline link's target and title.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        link(0),
        Representation::Metadata,
    ));
    assert!(j.contains("\"target\":\"http://ex\""), "{j}");
    assert!(j.contains("\"title\":\"ti\""), "{j}");
    // A reference link resolves its target from the definition.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        link(1),
        Representation::Metadata,
    ));
    assert!(j.contains("\"target\":\"http://ref\""), "{j}");
    // An image.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        link(2),
        Representation::Metadata,
    ));
    assert!(j.contains("\"kind\":\"image\""), "{j}");
    assert!(j.contains("\"target\":\"p.png\""), "{j}");
}

#[test]
fn native_selectors_resolve_and_decline() {
    let mut fx = Fixture::new("native", DOC);
    let field = fx.report.field;

    // A lexical find returns the block index and span.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        find("item"),
        Representation::Text,
    ));
    assert!(j.contains("\"block\":4"), "{j}");

    // Missing headings/code/links decline typed (never a silent empty answer).
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            heading(99),
            Representation::Metadata,
            Limits::DEFAULT
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            code(99),
            Representation::Metadata,
            Limits::DEFAULT
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            link(99),
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

    // The whole-document text is the source itself (never re-flowed).
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
    assert!(meta.contains("\"format\":\"markdown\""), "{meta}");
    assert!(meta.contains("\"headings\":2"), "{meta}");
    assert!(meta.contains("\"code_blocks\":1"), "{meta}");
    assert!(meta.contains("\"tables\":1"), "{meta}");
    assert!(meta.contains("\"list_items\":2"), "{meta}");
    assert!(meta.contains("\"front_matter\":true"), "{meta}");

    let h = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Heading(1),
        Representation::Text,
    ));
    assert_eq!(h, "Heading Two");

    let b = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Block(4),
        Representation::Text,
    ));
    assert_eq!(b, "item one");

    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("item".to_string()),
        Representation::Text,
    ));
    assert!(found.contains("\"block\":4"), "{found}");

    // A table/cell common selector is not in the Markdown capability set: decline.
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
    let src_path = root.join("doc.md");
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
        heading(0),
        Representation::Metadata,
    ));
    assert!(j.contains("\"level\":1"), "{j}");

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), DOC.len());
    assert_eq!(sha256(&exact), sha256(DOC));
    assert_eq!(exact, DOC);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn block_bomb_declines_typed_but_exactness_holds() {
    // More heading blocks than the STRICT block cap. The document is ingested under
    // DEFAULT (detection and exactness are unaffected); the *model build* declines
    // typed under STRICT rather than allocating.
    let mut bomb = String::new();
    for i in 0..20_000 {
        bomb.push_str(&format!("# H{i}\n\n"));
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

    let a = b"# Alpha\n\nfirst document body\n";
    let b = b"# Beta\n\nsecond document body\n";
    let da = opaque_descriptor(a);
    let db = opaque_descriptor(b);
    let fa = ingest_pdf(&mut store, &da, Limits::DEFAULT).unwrap().field;
    let fb = ingest_pdf(&mut store, &db, Limits::DEFAULT).unwrap().field;

    for (field, src, want) in [(fa, a, "Alpha"), (fb, b, "Beta")] {
        let got = answer_text(&observe_ok(
            &mut store,
            &field,
            heading(0),
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
fn markdown_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived Markdown model reads the source bytes, so its single
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
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_MARKDOWN_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::MarkdownModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the Markdown model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0x0DDB_1A5E_5BAD_5EED;
    for _ in 0..128 {
        let mut buf = vec![0u8; 512];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        if let Ok(m) = vole_document::adapter::markdown::parse(&buf, Limits::STRICT, true) {
            let _ = m.encode();
            let _ = vole_document::adapter::markdown::find(&buf, &m, "a", 1 << 20);
        }
    }
}
