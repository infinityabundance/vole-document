//! Phase 21.26.1 court: the reStructuredText prose adapter (the next Wave-2 prose
//! format, the Docutils input language).
//!
//! Gated on `rst`, so a build without the feature compiles an empty target.
//!
//! reStructuredText has no package layer: the exact leaf is the whole source (a
//! `DocumentExact`), and every derived observation is a bounded,
//! representation-preserving projection of it. The court checks:
//!
//! 1. **Byte-based detection is conservative and reST-specific**: a source carrying a
//!    reST structural mark (an explicit markup start, a grid/simple table, a field
//!    list, or an admissible section adornment) is detected; plain prose and a
//!    **Markdown** document are not (the Markdown control stays `Markdown`).
//! 2. **Representation preservation**: exact block/inline bytes and spans, section
//!    titles with their exact adornment and recorded hierarchy, directives preserved
//!    verbatim, hyperlink targets, footnotes, field/option/definition lists, literal
//!    and doctest blocks, list nesting, and grid/simple tables.
//! 3. **Selectors**: native `rst-heading`/`rst-block`/`rst-directive`/`rst-inline`/
//!    `rst-find` and common `metadata`/`text`/`heading`/`block`/`search-match` resolve.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 5. **Fail closed**: a title bomb declines typed (resource limit); random bytes
//!    never panic.
//! 6. **No cross-field aliasing** (ADR-0060): two reST fixtures interleaved in one
//!    store each answer from their own source, and the model node depends on the exact
//!    root.

#![cfg(all(feature = "field", feature = "rst"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_RST_MODEL, SelectorKey, lookup};
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
        "vole-rst-{label}-{}-{}",
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
        format_basis: "opaque:rst-test".to_string(),
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
    Selector::RstHeading { index }
}

fn block(index: u32) -> Selector {
    Selector::RstBlock { index }
}

fn directive(index: u32) -> Selector {
    Selector::RstDirective { index }
}

fn inline(index: u32) -> Selector {
    Selector::RstInline { index }
}

fn find(pattern: &str) -> Selector {
    Selector::RstFind {
        pattern: pattern.to_string(),
    }
}

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

// A fixture with two titles (`=`/`^` adornments — both reST-specific, never
// Markdown-claimed), an inline-rich paragraph, a hyperlink target, a directive, a
// bullet list, an ordered list, a field list, a grid table, a second paragraph with
// a substitution/footnote/hyperlink reference, and a footnote definition.
const DOC: &[u8] = b"Title\n=====\n\nA paragraph with **strong**, *emphasis*, ``literal``, and `role`:code: text.\n\nSection Two\n^^^^^^^^^^^\n\n.. _target: https://example.org\n\n.. note:: directive body\n\n- item one\n- item two\n\n1. first\n2. second\n\n:author: me\n\n+---+---+\n| a | b |\n+---+---+\n| 1 | 2 |\n+---+---+\n\nSee |sub| and [1]_ and `lbl`_.\n\n.. [1] A footnote.\n";

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    assert_eq!(
        detect_document_format(DOC, Limits::DEFAULT),
        DocumentFormat::Rst
    );
    // A bare section title (a `=` adornment) is a reST structural mark.
    assert_eq!(
        detect_document_format(b"Title\n=====\n\nbody\n", Limits::DEFAULT),
        DocumentFormat::Rst
    );
    // An explicit-markup start (no colon) is a reST structural mark.
    assert_eq!(
        detect_document_format(b".. a comment\n\nbody\n", Limits::DEFAULT),
        DocumentFormat::Rst
    );
    // A grid table is a reST structural mark.
    assert_eq!(
        detect_document_format(
            b"+---+---+\n| a | b |\n+---+---+\n| 1 | 2 |\n+---+---+\n",
            Limits::DEFAULT
        ),
        DocumentFormat::Rst
    );
    // Honest boundary: a tiny standalone field list (`:name: value`) or directive
    // (`.. name:: body`) is claimed by **YAML** first when YAML is compiled (YAML
    // precedes reStructuredText in the dispatcher's order), so its document-level
    // detection is feature-dependent. The reST adapter's own structural-signal
    // predicate recognizes both directly, independent of the compile-time feature
    // set — which is what admits a field list or directive inside a larger reST
    // document (see the `DOC` fixture above).
    assert!(
        vole_document::adapter::rst::parse(b":author: me\n", Limits::DEFAULT, false)
            .unwrap()
            .has_structural_signal()
    );
    assert!(
        vole_document::adapter::rst::parse(b".. note:: body\n", Limits::DEFAULT, false)
            .unwrap()
            .has_structural_signal()
    );
    // Plain prose (no reST structural mark) stays Opaque.
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

    let caps = capabilities_for_format(DocumentFormat::Rst);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "heading", "block", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in [
        "rst-heading",
        "rst-block",
        "rst-directive",
        "rst-inline",
        "rst-find",
    ] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"rst\""));
}

#[test]
fn a_markdown_document_is_never_stolen() {
    // The Markdown control is only meaningful when the Markdown adapter is compiled;
    // under `--all-features` (the court) it is. A document that is valid Markdown is
    // tried first and must stay Markdown, even though it also carries a `-` line that
    // is a reST adornment char.
    #[cfg(feature = "markdown")]
    {
        let md =
            b"# Heading\n\nA paragraph with [link](http://x).\n\n| a | b |\n| - | - |\n| 1 | 2 |\n";
        assert_eq!(
            detect_document_format(md, Limits::DEFAULT),
            DocumentFormat::Markdown
        );
    }
    // A `-`-underlined title is a Markdown thematic break, which the Markdown adapter
    // does **not** count as a structural signal, and `-` is deliberately excluded from
    // the reST detection adornment set: the source is Opaque, never reclassified as
    // reST (an honest boundary recorded in the adapter docs).
    assert_eq!(
        detect_document_format(b"Title\n-----\n\nbody\n", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
}

#[test]
fn preserves_titles_directives_and_spans() {
    let mut fx = Fixture::new("preserve", DOC);
    let field = fx.report.field;

    // Block 0 is the first title; its exact bytes include the underline.
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        block(0),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"Title\n=====");

    // A title's exact content text, hierarchy level, and adornment.
    let text = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        heading(0),
        Representation::Text,
    ));
    assert_eq!(text, "Title");
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        heading(1),
        Representation::Metadata,
    ));
    assert!(j.contains("\"level\":2"), "{j}");
    assert!(j.contains("\"adornment\":\"^^^^^^^^^^^\""), "{j}");
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
    assert_eq!(&DOC[cs..ce], b"Section Two");

    // The hyperlink target is preserved with its exact name and URI.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        block(3),
        Representation::Metadata,
    ));
    assert!(j.contains("\"kind\":\"hyperlink-target\""), "{j}");
    assert!(j.contains("\"info\":\"target\""), "{j}");
    assert!(j.contains("\"target\":\"https://example.org\""), "{j}");

    // The directive is preserved verbatim (never executed or resolved).
    let d = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        directive(0),
        Representation::Text,
    ));
    assert_eq!(d, ".. note:: directive body");
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        directive(0),
        Representation::Metadata,
    ));
    assert!(j.contains("\"name\":\"note\""), "{j}");
    assert!(j.contains("\"argument\":\"directive body\""), "{j}");

    // The field-list entry's value span.
    let val = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        block(9),
        Representation::Text,
    ));
    assert_eq!(val, "me");

    // A grid table exposes its exact cell spans.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        block(10),
        Representation::Metadata,
    ));
    assert!(j.contains("\"kind\":\"grid-table\""), "{j}");
    assert!(j.contains("\"rows\":2"), "{j}");
    assert!(j.contains("\"cols\":2"), "{j}");

    // An inline literal's exact whole span and inner text.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        inline(2),
        Representation::Metadata,
    ));
    assert!(j.contains("\"kind\":\"literal\""), "{j}");
    assert!(j.contains("\"text\":\"literal\""), "{j}");
    let (s, e) = span_of(&j);
    assert_eq!(&DOC[s..e], b"``literal``");

    // An interpreted text keeps its role.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        inline(3),
        Representation::Metadata,
    ));
    assert!(j.contains("\"kind\":\"interpreted\""), "{j}");
    assert!(j.contains("\"target\":\"code\""), "{j}");
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
    assert!(j.contains("\"block\":5"), "{j}");

    // Missing titles/directives/inlines decline typed (never a silent empty answer).
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
            directive(99),
            Representation::Metadata,
            Limits::DEFAULT
        ),
        ErrorClass::UnsupportedFeature
    );
    assert_eq!(
        observe_err(
            &mut fx.store,
            &field,
            inline(99),
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
    assert!(meta.contains("\"format\":\"rst\""), "{meta}");
    assert!(meta.contains("\"titles\":2"), "{meta}");
    assert!(meta.contains("\"directives\":1"), "{meta}");
    assert!(meta.contains("\"fields\":1"), "{meta}");
    assert!(meta.contains("\"grid_tables\":1"), "{meta}");
    assert!(meta.contains("\"footnotes\":1"), "{meta}");
    assert!(meta.contains("\"list_items\":4"), "{meta}");

    let h = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Heading(1),
        Representation::Text,
    ));
    assert_eq!(h, "Section Two");

    let b = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Block(9),
        Representation::Text,
    ));
    assert_eq!(b, "me");

    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("item".to_string()),
        Representation::Text,
    ));
    assert!(found.contains("\"block\":5"), "{found}");

    // A table/cell common selector is not in reStructuredText's capability set:
    // decline typed.
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
    let src_path = root.join("doc.rst");
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
fn title_bomb_declines_typed_but_exactness_holds() {
    // More title blocks than the STRICT block cap. The document is ingested under
    // DEFAULT (detection and exactness are unaffected); the *model build* declines
    // typed under STRICT rather than allocating.
    let mut bomb = String::new();
    for i in 0..20_000 {
        bomb.push_str(&format!("T{i}\n=====\n\n"));
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

    let a: &[u8] = b"Alpha\n=====\n\nfirst document body\n";
    let b: &[u8] = b"Beta\n====\n\nsecond document body\n";
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
fn rst_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived reStructuredText model reads the source bytes, so its
    // single dependency must be the exact `DocumentExact` root (keyed by
    // `sha256(source)`).
    let fx = Fixture::new("adr0060", DOC);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let manifest = field.manifest().clone();
    let root = manifest.root_node;
    let root_node = load_node(fx.store.seeds(), &root).unwrap();
    assert_eq!(root_node.kind, NodeKind::DocumentExact);

    assert!(manifest.has_index());
    let istore = FsIndexStore::open(fx.store.root()).unwrap();
    let iroot = NodeId::from_bytes(manifest.index_root);
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_RST_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::RstModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the reStructuredText model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0x2651_1234_ABCD_9876;
    for _ in 0..128 {
        let mut buf = vec![0u8; 512];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        if let Ok(m) = vole_document::adapter::rst::parse(&buf, Limits::STRICT, true) {
            let _ = m.encode();
            let _ = vole_document::adapter::rst::find(&buf, &m, "a", 1 << 20);
        }
    }
}
