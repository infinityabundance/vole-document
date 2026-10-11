//! Phase 21.26.2 court: the AsciiDoc prose adapter (the third Wave-2 prose format,
//! the Asciidoctor input language).
//!
//! Gated on `asciidoc`, so a build without the feature compiles an empty target.
//!
//! AsciiDoc has no package layer: the exact leaf is the whole source (a
//! `DocumentExact`), and every derived observation is a bounded,
//! representation-preserving projection of it. The court checks:
//!
//! 1. **Byte-based detection is conservative and AsciiDoc-specific**: a source
//!    carrying an AsciiDoc structural mark (a document title followed by a block, a
//!    `==`+ section, a `|===` table, a complete delimited block, or a block attribute
//!    line followed by a block) is detected; plain prose and a **Markdown** document
//!    are not (the Markdown control stays `Markdown`, and a reST control stays `Rst`).
//! 2. **Representation preservation**: exact block/inline bytes and spans, a level-0
//!    document title and sections with their level and marker, document attributes
//!    with literal attribute references, block attribute lines attached to the next
//!    block, every delimited block kind with its exact delimiter and verbatim content,
//!    list nesting, tables, and admonitions (paragraph and block).
//! 3. **Selectors**: native `adoc-heading`/`adoc-block`/`adoc-attribute`/`adoc-inline`/
//!    `adoc-find` and common `metadata`/`text`/`heading`/`block`/`search-match` resolve.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp),
//!    including after the source and the descriptor are deleted (fresh process).
//! 5. **Fail closed**: a block bomb declines typed (resource limit); random bytes
//!    never panic.
//! 6. **No cross-field aliasing** (ADR-0060): two AsciiDoc fixtures interleaved in one
//!    store each answer from their own source, and the model node depends on the exact
//!    root.

#![cfg(all(feature = "field", feature = "asciidoc"))]

use std::path::PathBuf;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::dag::load_node;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::index::{FsIndexStore, SEL_ASCIIDOC_MODEL, SelectorKey, lookup};
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
        "vole-asciidoc-{label}-{}-{}",
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
        format_basis: "opaque:asciidoc-test".to_string(),
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
    Selector::AdocHeading { index }
}

fn block(index: u32) -> Selector {
    Selector::AdocBlock { index }
}

fn attribute(index: u32) -> Selector {
    Selector::AdocAttribute { index }
}

fn inline(index: u32) -> Selector {
    Selector::AdocInline { index }
}

fn find(pattern: &str) -> Selector {
    Selector::AdocFind {
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

// A fixture with a document title, a rich inline paragraph, two document attributes
// (`:name:value` — the **no-space** spelling, since the spaced `:name: value` form and
// the `:name!:` unset form are indistinguishable from a reStructuredText field list
// and reST is tried first), two sections, a macro-bearing paragraph, a
// `[source,rust]` block attribute attached to a listing block,
// unordered/ordered/description lists, a `[cols="2"]`-attributed table, a `NOTE:`
// paragraph admonition, and a `[WARNING]` block admonition.
const DOC: &[u8] = b"= AsciiDoc Primer\n\nA paragraph with *strong*, _emphasis_, `mono`, +pass+, ^sup^, ~sub~, and #mark#.\n\n:author:Jane Doe\n:version:1.0\n\n== First Section\n\nA paragraph that references {author} and {version} literally, links link:https://example.org[Example], includes include::chapter.adoc[], cross-references xref:sec-1[Section One], shows image:logo.png[Logo], and a bare URL https://bare.example[Site].\n\n[source,rust]\n----\nfn main() {\n    println!(\"hi\");\n}\n----\n\n=== Subsection\n\n* item one\n* item two\n\n. first\n. second\n\nterm:: a definition body\n\n[cols=\"2\"]\n|===\n| Name | Value\n| alpha | 1\n| beta | 2\n|===\n\nNOTE: This is a note paragraph.\n\n[WARNING]\nA block admonition body.\n";

// A source carrying the canonical **spaced** set form and the `:name!:` unset form,
// plus a literal `{ref}`. Parsed directly (see the boundary test): the spaced/unset
// forms are reStructuredText field lists, so the dispatcher claims this source as Rst
// (reST is tried before AsciiDoc).
const ATTRS: &[u8] = b"= Attributes\n\n:author: Jane Doe\n:version: 1.0\n:obsolete!:\n\nThe attribute {author} stays literal.\n";

// A fixture exercising every delimited block kind. Each `====`/`++++`/`....` body
// spans two lines so a single-line reST overline title (`====\nX\n====`) is never
// formed; reST is tried first anyway, so those remain AsciiDoc here.
const DELIM: &[u8] = b"= Delimited\n\n----\nlisting one\nlisting two\n----\n\n....\nliteral one\nliteral two\n....\n\n====\nexample one\nexample two\n====\n\n****\nsidebar one\nsidebar two\n****\n\n____\nquote one\nquote two\n____\n\n--\nopen one\nopen two\n--\n\n++++\npassthrough one\npassthrough two\n++++\n";

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_is_byte_based_and_conservative() {
    assert_eq!(
        detect_document_format(DOC, Limits::DEFAULT),
        DocumentFormat::Asciidoc
    );
    // A `==` section is an AsciiDoc structural mark.
    assert_eq!(
        detect_document_format(b"== Section\n\nbody\n", Limits::DEFAULT),
        DocumentFormat::Asciidoc
    );
    // A `|===` table inside a document (with a title) is an AsciiDoc structural mark.
    // A *bare* `|===` table whose lines are a majority pipe-delimited is claimed by
    // the CSV/PSV adapter first (CSV precedes AsciiDoc in the dispatcher order), so it
    // is not asserted here as AsciiDoc.
    assert_eq!(
        detect_document_format(b"= T\n\n|===\n| a | b\n| 1 | 2\n|===\n", Limits::DEFAULT),
        DocumentFormat::Asciidoc
    );
    // A complete delimited block is an AsciiDoc structural mark.
    assert_eq!(
        detect_document_format(b"----\nlisting\n----\n", Limits::DEFAULT),
        DocumentFormat::Asciidoc
    );
    // A block attribute line followed by a block is an AsciiDoc structural mark.
    assert_eq!(
        detect_document_format(b"[quote, Author]\nA quoted line.\n", Limits::DEFAULT),
        DocumentFormat::Asciidoc
    );
    // A document title *followed by a further block* is an AsciiDoc structural mark.
    assert_eq!(
        detect_document_format(b"= Title\n\nA paragraph.\n", Limits::DEFAULT),
        DocumentFormat::Asciidoc
    );
    // Plain prose (no AsciiDoc structural mark) stays Opaque.
    for prose in [
        &b"just some prose\nwith more lines\n"[..],
        &b"Hello, world.\nThis is a test.\n"[..],
        &b""[..],
        // A lone `= text` line is too weak to claim on its own.
        &b"= lonely\n"[..],
    ] {
        assert_eq!(
            detect_document_format(prose, Limits::DEFAULT),
            DocumentFormat::Opaque,
            "{prose:?}"
        );
    }

    let caps = capabilities_for_format(DocumentFormat::Asciidoc);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "heading", "block", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for n in [
        "adoc-heading",
        "adoc-block",
        "adoc-attribute",
        "adoc-inline",
        "adoc-find",
    ] {
        assert!(caps.native_selectors.contains(&n), "missing {n}");
    }
    assert!(caps.to_json().contains("\"format\":\"asciidoc\""));
}

#[test]
fn a_markdown_or_rst_document_is_never_stolen() {
    // Each control is only meaningful when the earlier adapter is compiled; under
    // `--all-features` (the court) both are.
    #[cfg(feature = "markdown")]
    {
        let md =
            b"# Heading\n\nA paragraph with [link](http://x).\n\n| a | b |\n| - | - |\n| 1 | 2 |\n";
        assert_eq!(
            detect_document_format(md, Limits::DEFAULT),
            DocumentFormat::Markdown
        );
    }
    #[cfg(feature = "rst")]
    {
        let rst = b"Title\n=====\n\n.. note:: a directive body\n";
        assert_eq!(
            detect_document_format(rst, Limits::DEFAULT),
            DocumentFormat::Rst
        );
    }
}

#[test]
fn attributes_parse_but_the_spaced_form_is_rst_claimed() {
    use vole_document::adapter::asciidoc::{B_ATTRIBUTE, F_UNSET, I_ATTR_REF};
    // Direct parse (no dispatcher): the canonical spaced set form and the `:name!:`
    // unset form parse into attribute blocks, and `{ref}` is surfaced literally.
    let m = vole_document::adapter::asciidoc::parse(ATTRS, Limits::DEFAULT, true).unwrap();
    let attrs = m.blocks_of_kind(B_ATTRIBUTE);
    assert_eq!(attrs.len(), 3);
    assert_eq!(m.blocks[attrs[0] as usize].info.as_deref(), Some("author"));
    assert_eq!(
        m.blocks[attrs[0] as usize].title.as_deref(),
        Some("Jane Doe")
    );
    assert_ne!(m.blocks[attrs[2] as usize].flags & F_UNSET, 0);
    assert_eq!(m.inlines_of_kind(I_ATTR_REF).len(), 1);

    // Honest boundary: the spaced `:name: value` and `:name!:` lines are
    // reStructuredText field lists, and reST is tried first, so the dispatcher does
    // **not** claim this source as AsciiDoc. (The no-space `:name:value` spelling
    // used by `DOC` avoids the overlap.)
    #[cfg(feature = "rst")]
    assert_eq!(
        detect_document_format(ATTRS, Limits::DEFAULT),
        DocumentFormat::Rst
    );
}

#[test]
fn preserves_headings_attributes_and_spans() {
    let mut fx = Fixture::new("preserve", DOC);
    let field = fx.report.field;

    // Block 0 is the document title; its exact bytes are the `= …` line only.
    let bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        block(0),
        Representation::ExactBytes,
    ));
    assert_eq!(bytes, b"= AsciiDoc Primer");

    // The document title is heading 0, at level 0.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        heading(0),
        Representation::Metadata,
    ));
    assert!(j.contains("\"level\":0"), "{j}");
    assert!(j.contains("\"document_title\":true"), "{j}");
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            heading(0),
            Representation::Text
        )),
        "AsciiDoc Primer"
    );

    // Section levels and exact marker.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        heading(1),
        Representation::Metadata,
    ));
    assert!(j.contains("\"level\":1"), "{j}");
    assert!(j.contains("\"marker\":\"==\""), "{j}");
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        heading(2),
        Representation::Metadata,
    ));
    assert!(j.contains("\"level\":2"), "{j}");
    assert!(j.contains("\"marker\":\"===\""), "{j}");
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            heading(2),
            Representation::Text
        )),
        "Subsection"
    );

    // Document attributes: the `:name:value` (no-space) form parses to a name + value.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        attribute(0),
        Representation::Metadata,
    ));
    assert!(j.contains("\"name\":\"author\""), "{j}");
    assert!(j.contains("\"value\":\"Jane Doe\""), "{j}");
    assert!(j.contains("\"unset\":false"), "{j}");

    // Block attribute line (index 6) is preserved with its exact inner text and is
    // attached to the following listing block (index 7).
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        block(6),
        Representation::Metadata,
    ));
    assert!(j.contains("\"kind\":\"block-attr\""), "{j}");
    assert!(j.contains("\"info\":\"source,rust\""), "{j}");
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        block(7),
        Representation::Metadata,
    ));
    assert!(j.contains("\"kind\":\"listing\""), "{j}");
    assert!(j.contains("\"attrs\":\"source,rust\""), "{j}");
    let listing_bytes = answer_bytes(&observe_ok(
        &mut fx.store,
        &field,
        block(7),
        Representation::ExactBytes,
    ));
    assert_eq!(
        listing_bytes,
        b"----\nfn main() {\n    println!(\"hi\");\n}\n----"
    );

    // The table exposes rows/cols and is attributed.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        block(15),
        Representation::Metadata,
    ));
    assert!(j.contains("\"kind\":\"table\""), "{j}");
    assert!(j.contains("\"rows\":3"), "{j}");
    assert!(j.contains("\"cols\":2"), "{j}");
    assert!(j.contains("\"attrs\":\"cols=\\\"2\\\"\""), "{j}");

    // A description list item keeps its term.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        block(13),
        Representation::Metadata,
    ));
    assert!(j.contains("\"kind\":\"list-item\""), "{j}");
    assert!(j.contains("\"target\":\"term\""), "{j}");
    assert_eq!(
        answer_text(&observe_ok(
            &mut fx.store,
            &field,
            block(13),
            Representation::Text
        )),
        "a definition body"
    );

    // Paragraph and block admonitions.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        block(16),
        Representation::Metadata,
    ));
    assert!(j.contains("\"kind\":\"admonition\""), "{j}");
    assert!(j.contains("\"info\":\"NOTE\""), "{j}");
    assert!(j.contains("\"flags\":0"), "{j}");
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        block(17),
        Representation::Metadata,
    ));
    assert!(j.contains("\"info\":\"WARNING\""), "{j}");
    assert!(
        !j.contains("\"flags\":0"),
        "block admonition must carry F_BLOCK: {j}"
    );
    assert!(
        !j.contains("\"flags\":0"),
        "block admonition must carry F_BLOCK: {j}"
    );

    // Inline markup: exact whole span and inner text.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        inline(2),
        Representation::Metadata,
    ));
    assert!(j.contains("\"kind\":\"mono\""), "{j}");
    assert!(j.contains("\"text\":\"mono\""), "{j}");
    let (s, e) = span_of(&j);
    assert_eq!(&DOC[s..e], b"`mono`");

    // A macro is preserved verbatim with its exact target.
    let mut found_link = false;
    let mut found_url = false;
    for i in 0..14 {
        let j = answer_json(&observe_ok(
            &mut fx.store,
            &field,
            inline(i),
            Representation::Metadata,
        ));
        if j.contains("\"kind\":\"link\"") {
            assert!(j.contains("\"target\":\"https://example.org\""), "{j}");
            found_link = true;
        }
        if j.contains("\"kind\":\"url\"") {
            assert!(j.contains("\"target\":\"https://bare.example\""), "{j}");
            found_url = true;
        }
        if j.contains("\"kind\":\"attribute-ref\"") {
            assert!(
                j.contains("\"text\":\"author\"") || j.contains("\"text\":\"version\""),
                "{j}"
            );
        }
    }
    assert!(found_link && found_url, "macro kinds missing");
}

#[test]
fn every_delimited_block_kind_is_exact() {
    assert_eq!(
        detect_document_format(DELIM, Limits::DEFAULT),
        DocumentFormat::Asciidoc
    );
    let mut fx = Fixture::new("delim", DELIM);
    let field = fx.report.field;

    let meta = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Metadata,
        Representation::Metadata,
    ));
    for key in [
        "\"listings\":1",
        "\"literals\":1",
        "\"examples\":1",
        "\"sidebars\":1",
        "\"quotes\":1",
        "\"opens\":1",
        "\"passthrough_blocks\":1",
        "\"delimited\":7",
    ] {
        assert!(meta.contains(key), "missing {key}: {meta}");
    }

    // Each delimited block carries its exact delimiter and its content verbatim.
    let blocks_n = {
        let tag = "\"blocks\":";
        let i = meta.find(tag).unwrap() + tag.len();
        let rest = &meta[i..];
        let end = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        rest[..end].parse::<u32>().unwrap()
    };
    let mut seen: Vec<&str> = Vec::new();
    for i in 0..blocks_n {
        let j = answer_json(&observe_ok(
            &mut fx.store,
            &field,
            block(i),
            Representation::Metadata,
        ));
        for (kind, delim) in [
            ("listing", "----"),
            ("literal", "...."),
            ("example", "===="),
            ("sidebar", "****"),
            ("quote", "____"),
            ("open", "--"),
            ("passthrough", "++++"),
        ] {
            if j.contains(&format!("\"kind\":\"{kind}\"")) {
                assert!(j.contains(&format!("\"info\":\"{delim}\"")), "{j}");
                seen.push(delim);
            }
        }
    }
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), 7, "expected 7 delimited kinds, saw {seen:?}");
}

#[test]
fn native_selectors_resolve_and_decline() {
    let mut fx = Fixture::new("native", DOC);
    let field = fx.report.field;

    // A lexical find returns the block index and span.
    let j = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        find("item one"),
        Representation::Text,
    ));
    assert!(j.contains("\"block\":9"), "{j}");

    // Missing headings/attributes/inlines/blocks decline typed.
    for sel in [heading(99), attribute(99), inline(99), block(99)] {
        assert_eq!(
            observe_err(
                &mut fx.store,
                &field,
                sel,
                Representation::Metadata,
                Limits::DEFAULT
            ),
            ErrorClass::UnsupportedFeature
        );
    }
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
    assert!(meta.contains("\"format\":\"asciidoc\""), "{meta}");
    assert!(meta.contains("\"doc_title\":1"), "{meta}");
    assert!(meta.contains("\"sections\":2"), "{meta}");
    assert!(meta.contains("\"attributes\":2"), "{meta}");
    assert!(meta.contains("\"block_attrs\":2"), "{meta}");
    assert!(meta.contains("\"listings\":1"), "{meta}");
    assert!(meta.contains("\"list_items\":5"), "{meta}");
    assert!(meta.contains("\"tables\":1"), "{meta}");
    assert!(meta.contains("\"admonitions\":2"), "{meta}");
    assert!(meta.contains("\"block_admonitions\":1"), "{meta}");
    assert!(meta.contains("\"strong\":1"), "{meta}");
    assert!(meta.contains("\"emphasis\":1"), "{meta}");
    assert!(meta.contains("\"links\":1"), "{meta}");
    assert!(meta.contains("\"includes\":1"), "{meta}");
    assert!(meta.contains("\"xrefs\":1"), "{meta}");
    assert!(meta.contains("\"urls\":1"), "{meta}");
    assert!(meta.contains("\"attribute_refs\":2"), "{meta}");
    assert!(meta.contains("\"table_cells\":6"), "{meta}");

    let h = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Heading(1),
        Representation::Text,
    ));
    assert_eq!(h, "First Section");

    let b = answer_text(&observe_ok(
        &mut fx.store,
        &field,
        Selector::Block(4),
        Representation::Text,
    ));
    assert_eq!(b, "First Section");

    let found = answer_json(&observe_ok(
        &mut fx.store,
        &field,
        Selector::SearchMatch("item one".to_string()),
        Representation::Text,
    ));
    assert!(found.contains("\"block\":9"), "{found}");

    // A table/cell common selector is not in AsciiDoc's capability set: decline typed.
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
    let src_path = root.join("doc.adoc");
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
    assert!(j.contains("\"level\":0"), "{j}");

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
    // More section blocks than the STRICT block cap. The document is ingested under
    // DEFAULT (detection and exactness are unaffected); the *model build* declines
    // typed under STRICT rather than allocating.
    let mut bomb = String::new();
    for i in 0..20_000 {
        bomb.push_str(&format!("== S{i}\n\nbody\n\n"));
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

    let a: &[u8] = b"= Alpha\n\nfirst document body\n";
    let b: &[u8] = b"= Beta\n\nsecond document body\n";
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
fn asciidoc_model_node_depends_on_the_exact_root() {
    // ADR-0060: the derived AsciiDoc model reads the source bytes, so its single
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
    let entries = lookup(&istore, &iroot, &SelectorKey::new(SEL_ASCIIDOC_MODEL, 0)).unwrap();
    assert_eq!(entries.len(), 1);
    let model = load_node(fx.store.seeds(), &entries[0].node_id).unwrap();
    assert_eq!(model.kind, NodeKind::AsciidocModel);
    assert_eq!(
        model.deps,
        vec![root],
        "the AsciiDoc model must depend on exactly the exact root"
    );
}

#[test]
fn random_bytes_never_panic() {
    let mut x: u64 = 0x0adc_9876_5432_1abc;
    for _ in 0..128 {
        let mut buf = vec![0u8; 512];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        if let Ok(m) = vole_document::adapter::asciidoc::parse(&buf, Limits::STRICT, true) {
            let _ = m.encode();
            let _ = vole_document::adapter::asciidoc::find(&buf, &m, "a", 1 << 20);
        }
    }
}
