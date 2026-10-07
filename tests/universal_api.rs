//! Phase 12.7 court: the universal observation API.
//!
//! A PDF, a hand-authored DOCX and a hand-authored EPUB are each ingested through
//! the **one** format-agnostic ingest, then queried through the **one** common
//! `DocumentField` API. The court checks:
//!
//! 1. **Format detection is byte-based** for each source (never a file name), and
//!    malformed/plain bytes fall back to `Opaque`.
//! 2. **Capabilities** list the right common selector sets per format.
//! 3. A common `SearchMatch` (`find`) works on all three formats.
//! 4. A common `Table`/`Cell` observation works on DOCX and EPUB, and is a typed
//!    capability error on PDF (which has no table coordinate).
//! 5. Native selectors still work per format, unchanged.
//! 6. **EXPLAIN** reports the detected format, adapter, capability resolution,
//!    index route, and XML-parse accounting.
//! 7. PDF behavior/exactness is not regressed by the widened API.
//!
//! Gated on the full feature stack; a build without it compiles an empty target.

#![cfg(all(
    feature = "field",
    feature = "package",
    feature = "opc",
    feature = "docx",
    feature = "epub"
))]

use std::path::PathBuf;
use std::process::Command;

use vole_document::adapter::docx::{DocxExtractProfile, DocxStory};
use vole_document::adapter::epub::EpubExtractProfile;
use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::explain::explain_analyze;
use vole_document::field::ingest::{IngestOutcome, ingest};
use vole_document::field::observe::{ObserveRequest, Representation, Selector, observe};
use vole_document::field::provenance::AnswerValue;
use vole_document::field::{Field, FieldStore};
use vole_document::limits::Limits;

// ---------------------------------------------------------------------------
// Minimal dependency-free ZIP writer (all members stored; CRC-32/ISO-HDLC).
// ---------------------------------------------------------------------------

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    const UTF8_FLAG: u16 = 0x0800;
    let mut out: Vec<u8> = Vec::new();
    let mut offsets: Vec<u32> = Vec::new();
    let mut crcs: Vec<u32> = Vec::new();
    for (name, content) in entries {
        offsets.push(out.len() as u32);
        crcs.push(vole_document::adapter::package::crc32_iso_hdlc(content));
        put_u32(&mut out, 0x0403_4b50);
        put_u16(&mut out, 20);
        put_u16(&mut out, UTF8_FLAG);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u32(&mut out, *crcs.last().unwrap());
        put_u32(&mut out, content.len() as u32);
        put_u32(&mut out, content.len() as u32);
        put_u16(&mut out, name.len() as u16);
        put_u16(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(content);
    }
    let cd_start = out.len() as u32;
    for (i, (name, content)) in entries.iter().enumerate() {
        put_u32(&mut out, 0x0201_4b50);
        put_u16(&mut out, 20);
        put_u16(&mut out, 20);
        put_u16(&mut out, UTF8_FLAG);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u32(&mut out, crcs[i]);
        put_u32(&mut out, content.len() as u32);
        put_u32(&mut out, content.len() as u32);
        put_u16(&mut out, name.len() as u16);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u32(&mut out, 0);
        put_u32(&mut out, offsets[i]);
        out.extend_from_slice(name.as_bytes());
    }
    let cd_end = out.len() as u32;
    put_u32(&mut out, 0x0605_4b50);
    put_u16(&mut out, 0);
    put_u16(&mut out, 0);
    put_u16(&mut out, entries.len() as u16);
    put_u16(&mut out, entries.len() as u16);
    put_u32(&mut out, cd_end - cd_start);
    put_u32(&mut out, cd_start);
    put_u16(&mut out, 0);
    out
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const DOCX_MAIN_CT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";

fn docx_source() -> Vec<u8> {
    let content_types = format!(
        concat!(
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
            r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#,
            r#"<Default Extension="xml" ContentType="application/xml"/>"#,
            r#"<Override PartName="/word/document.xml" ContentType="{}"/>"#,
            r#"</Types>"#
        ),
        DOCX_MAIN_CT
    );
    let rels = concat!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
        r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>"#,
        r#"</Relationships>"#
    );
    let mut table = String::from("<w:tbl>");
    for r in 1..=7 {
        table.push_str("<w:tr>");
        for c in 1..=2 {
            table.push_str(&format!(
                "<w:tc><w:p><w:r><w:t>r{r}c{c}</w:t></w:r></w:p></w:tc>"
            ));
        }
        table.push_str("</w:tr>");
    }
    table.push_str("</w:tbl>");
    let document = format!(
        concat!(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#,
            r#"<w:body><w:p><w:r><w:t>Body one</w:t></w:r></w:p>{}<w:sectPr/></w:body>"#,
            r#"</w:document>"#
        ),
        table
    );
    build_zip(&[
        ("[Content_Types].xml", content_types.as_bytes()),
        ("_rels/.rels", rels.as_bytes()),
        ("word/document.xml", document.as_bytes()),
    ])
}

fn epub_source() -> Vec<u8> {
    let container = concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        r#"<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">"#,
        r#"<rootfiles><rootfile full-path="OEBPS/package.opf" media-type="application/oebps-package+xml"/></rootfiles>"#,
        r#"</container>"#
    );
    let opf = concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="pub-id">"#,
        r#"<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">"#,
        r#"<dc:identifier id="pub-id">urn:uuid:12345678-1234-1234-1234-123456789012</dc:identifier>"#,
        r#"<dc:title>Universal Test</dc:title></metadata>"#,
        r#"<manifest>"#,
        r#"<item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>"#,
        r#"<item id="pic" href="images/pic.png" media-type="image/png"/>"#,
        r#"</manifest>"#,
        r#"<spine><itemref idref="ch1"/></spine>"#,
        r#"</package>"#
    );
    let chapter1 = concat!(
        r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>One</title></head><body>"#,
        r#"<h1>Chapter One</h1>"#,
        r#"<p>The quick brown fox jumps over the lazy dog.</p>"#,
        r#"<table><tr><th>Key</th><th>Value</th></tr><tr><td>answer</td><td>42</td></tr></table>"#,
        r#"<p>See <a href="https://example.com/ext">external</a>.</p>"#,
        r#"<img src="images/pic.png" alt="pic"/>"#,
        r#"</body></html>"#
    );
    build_zip(&[
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", container.as_bytes()),
        ("OEBPS/package.opf", opf.as_bytes()),
        ("OEBPS/chapter1.xhtml", chapter1.as_bytes()),
        ("OEBPS/images/pic.png", b"\x89PNG\r\n\x1a\nFAKE"),
    ])
}

fn pdf_source() -> Vec<u8> {
    vole_document::adapter::pdf::sample_pdfs()
        .into_iter()
        .find(|(n, _)| *n == "bigtext.pdf")
        .map(|(_, b)| b)
        .expect("bigtext.pdf sample is present")
}

fn opaque_descriptor(source: &[u8]) -> Vec<u8> {
    let d = Descriptor {
        universe: vole_document::container::UNIVERSE.to_string(),
        source_format: vole_document::SOURCE_FORMAT_OPAQUE,
        format_basis: "opaque:universal-api-test".to_string(),
        models: vec![],
        channels: vec![],
        objects: vec![ObjectSource::Inline(source.to_vec())],
        program: Program::new(vec![Op::EmitObject { object_id: 0 }]),
        observation_index: None,
        seek_directory: false,
        checkpoints: None,
        source_sha256: vole_document::integrity::sha256(source),
        source_len: source.len() as u64,
    };
    d.serialize().unwrap().0
}

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-universal-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

struct Fixture {
    root: PathBuf,
    store: FieldStore,
    field: vole_document::field::FieldId,
    source: Vec<u8>,
    format: DocumentFormat,
}

impl Fixture {
    fn new(label: &str, source: Vec<u8>) -> Fixture {
        let root = temp_dir(label);
        let mut store = FieldStore::open(&root).unwrap();
        let descriptor = opaque_descriptor(&source);
        let format = detect_document_format(&source, Limits::DEFAULT);
        let field = match ingest(&mut store, &descriptor, Limits::DEFAULT).unwrap() {
            IngestOutcome::Pdf(r) => r.field,
            IngestOutcome::Package(r) => r.field,
        };
        store.sync().unwrap();
        Fixture {
            root,
            store,
            field,
            source,
            format,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

fn observe_eq(
    store: &mut FieldStore,
    field: &vole_document::field::FieldId,
    selector: Selector,
    representation: Representation,
) -> (AnswerValue, vole_document::field::provenance::Basis, String) {
    let req = ObserveRequest::new(selector, representation);
    let (answer, _stats, _) = observe(store, field, &req, Limits::DEFAULT).unwrap();
    (answer.value, answer.basis, answer.provenance)
}

fn text_of(value: &AnswerValue) -> String {
    match value {
        AnswerValue::Text(t) => t.clone(),
        other => panic!("expected text, got {other:?}"),
    }
}

fn json_of(value: &AnswerValue) -> String {
    match value {
        AnswerValue::Json(j) => j.clone(),
        other => panic!("expected json, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn format_detection_is_byte_based() {
    assert_eq!(
        detect_document_format(&pdf_source(), Limits::DEFAULT),
        DocumentFormat::Pdf
    );
    assert_eq!(
        detect_document_format(&docx_source(), Limits::DEFAULT),
        DocumentFormat::Docx
    );
    assert_eq!(
        detect_document_format(&epub_source(), Limits::DEFAULT),
        DocumentFormat::Epub
    );
    assert_eq!(
        detect_document_format(b"just some bytes", Limits::DEFAULT),
        DocumentFormat::Opaque
    );
    // A malformed PDF (a declared negative control) is not detected as PDF.
    let malformed = vole_document::adapter::pdf::sample_pdfs()
        .into_iter()
        .find(|(n, _)| *n == "malformed.pdf")
        .map(|(_, b)| b)
        .unwrap();
    assert_ne!(
        detect_document_format(&malformed, Limits::DEFAULT),
        DocumentFormat::Pdf
    );
}

#[test]
fn capabilities_list_the_right_sets() {
    let pdf = capabilities_for_format(DocumentFormat::Pdf);
    let names: Vec<&str> = pdf.selectors.iter().map(|s| s.selector).collect();
    assert!(names.contains(&"text"));
    assert!(names.contains(&"search-match"));
    assert!(!names.contains(&"table"));
    assert!(!names.contains(&"cell"));

    for fmt in [DocumentFormat::Docx, DocumentFormat::Epub] {
        let caps = capabilities_for_format(fmt);
        let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
        for expected in [
            "metadata",
            "text",
            "heading",
            "block",
            "table",
            "cell",
            "resource",
            "link",
            "search-match",
        ] {
            assert!(
                names.contains(&expected),
                "{fmt:?} missing {expected}: {names:?}"
            );
        }
        assert!(!caps.native_selectors.is_empty());
    }
    let json = capabilities_for_format(DocumentFormat::Epub).to_json();
    assert!(json.contains("\"format\":\"epub\""), "{json}");
    assert!(json.contains("\"root\":\"document\""), "{json}");
}

#[test]
fn common_find_works_on_all_three_formats() {
    let mut pdf = Fixture::new("find-pdf", pdf_source());
    let (v, _, _) = observe_eq(
        &mut pdf.store,
        &pdf.field,
        Selector::SearchMatch("Invoice line".to_string()),
        Representation::Text,
    );
    assert!(json_of(&v).contains("Invoice line"), "{v:?}");

    let mut docx = Fixture::new("find-docx", docx_source());
    let (v, _, _) = observe_eq(
        &mut docx.store,
        &docx.field,
        Selector::SearchMatch("Body one".to_string()),
        Representation::Text,
    );
    assert!(json_of(&v).contains("Body one"), "{v:?}");

    let mut epub = Fixture::new("find-epub", epub_source());
    let (v, _, _) = observe_eq(
        &mut epub.store,
        &epub.field,
        Selector::SearchMatch("quick brown".to_string()),
        Representation::Text,
    );
    assert!(json_of(&v).contains("quick brown"), "{v:?}");
}

#[test]
fn common_table_and_cell_work_on_docx_and_epub() {
    let mut docx = Fixture::new("table-docx", docx_source());
    let (v, basis, prov) = observe_eq(
        &mut docx.store,
        &docx.field,
        Selector::Table(0),
        Representation::Text,
    );
    assert!(text_of(&v).contains("r1c1"), "{v:?}");
    assert!(!basis.is_exact());
    assert!(prov.starts_with("format=docx;common;"), "{prov}");
    let (v, _, _) = observe_eq(
        &mut docx.store,
        &docx.field,
        Selector::Cell {
            table: 0,
            row: 6,
            col: 1,
        },
        Representation::Text,
    );
    assert_eq!(text_of(&v), "r7c2");

    let mut epub = Fixture::new("table-epub", epub_source());
    let (v, _, _) = observe_eq(
        &mut epub.store,
        &epub.field,
        Selector::Table(0),
        Representation::Text,
    );
    assert!(text_of(&v).contains("answer"), "{v:?}");
    let (v, _, _) = observe_eq(
        &mut epub.store,
        &epub.field,
        Selector::Cell {
            table: 0,
            row: 1,
            col: 1,
        },
        Representation::Text,
    );
    assert_eq!(text_of(&v), "42");
}

#[test]
fn common_heading_block_resource_and_link() {
    let mut docx = Fixture::new("struct-docx", docx_source());
    let (v, _, _) = observe_eq(
        &mut docx.store,
        &docx.field,
        Selector::Block(0),
        Representation::Text,
    );
    assert_eq!(text_of(&v), "Body one");

    let mut epub = Fixture::new("struct-epub", epub_source());
    let (v, _, _) = observe_eq(
        &mut epub.store,
        &epub.field,
        Selector::Heading(0),
        Representation::Text,
    );
    assert_eq!(text_of(&v), "Chapter One");
    let (v, _, _) = observe_eq(
        &mut epub.store,
        &epub.field,
        Selector::Link(0),
        Representation::Metadata,
    );
    assert!(json_of(&v).contains("https://example.com/ext"), "{v:?}");
    let (v, _, _) = observe_eq(
        &mut epub.store,
        &epub.field,
        Selector::Resource(0),
        Representation::Metadata,
    );
    assert!(json_of(&v).contains("image/png"), "{v:?}");
}

#[test]
fn common_text_and_metadata_are_available() {
    let mut epub = Fixture::new("text-epub", epub_source());
    let (v, _, _) = observe_eq(
        &mut epub.store,
        &epub.field,
        Selector::Text,
        Representation::Text,
    );
    assert!(text_of(&v).contains("Chapter One"), "{v:?}");
    let (v, _, _) = observe_eq(
        &mut epub.store,
        &epub.field,
        Selector::Metadata,
        Representation::Metadata,
    );
    assert!(json_of(&v).contains("package"), "{v:?}");

    let mut pdf = Fixture::new("text-pdf", pdf_source());
    let (v, _, _) = observe_eq(
        &mut pdf.store,
        &pdf.field,
        Selector::Text,
        Representation::Text,
    );
    assert!(text_of(&v).contains("Invoice line"), "{v:?}");
}

#[test]
fn native_selectors_still_work_per_format() {
    let mut pdf = Fixture::new("native-pdf", pdf_source());
    let (v, _, _) = observe_eq(
        &mut pdf.store,
        &pdf.field,
        Selector::Page(1),
        Representation::Text,
    );
    assert!(text_of(&v).contains("Invoice line"), "{v:?}");

    let mut docx = Fixture::new("native-docx", docx_source());
    let (v, _, prov) = observe_eq(
        &mut docx.store,
        &docx.field,
        Selector::DocxParagraph {
            story: DocxStory::Main,
            index: 0,
            profile: DocxExtractProfile::DEFAULT,
        },
        Representation::Text,
    );
    assert_eq!(text_of(&v), "Body one");
    assert!(prov.starts_with("docx;"), "{prov}");

    let mut epub = Fixture::new("native-epub", epub_source());
    let (v, _, prov) = observe_eq(
        &mut epub.store,
        &epub.field,
        Selector::EpubSpineItem {
            index: 0,
            profile: EpubExtractProfile::DEFAULT,
        },
        Representation::Text,
    );
    assert!(text_of(&v).contains("Chapter One"), "{v:?}");
    assert!(prov.starts_with("epub;"), "{prov}");
}

#[test]
fn explain_reports_format_and_xml_accounting() {
    let mut docx = Fixture::new("explain-docx", docx_source());
    let req = ObserveRequest::new(Selector::Table(0), Representation::Text);
    let (plan, actual) =
        explain_analyze(&mut docx.store, &docx.field, &req, Limits::DEFAULT).unwrap();
    assert!(plan.json.contains("\"format\":\"docx\""), "{}", plan.json);
    assert!(plan.json.contains("\"adapter\":\"docx\""), "{}", plan.json);
    assert!(
        plan.json
            .contains("\"capability\":\"common:table -> docx\""),
        "{}",
        plan.json
    );
    assert!(
        plan.json.contains("\"index_route\":\"hier-index\""),
        "{}",
        plan.json
    );
    assert_eq!(actual.format, "docx");
    assert_eq!(actual.adapter, "docx");
    assert!(!actual.whole_source_materialized);
    assert!(actual.stats.xml_parses >= 1, "{:?}", actual.stats);
    assert!(actual.stats.member_decodes >= 1, "{:?}", actual.stats);
    let json = actual.to_json();
    for key in [
        "\"member_decodes\":",
        "\"xml_parses\":",
        "\"whole_source_materialized\":false",
    ] {
        assert!(json.contains(key), "missing {key}: {json}");
    }
}

#[test]
fn unsupported_common_pair_is_a_typed_capability_error() {
    let mut pdf = Fixture::new("unsupported", pdf_source());
    let req = ObserveRequest::new(Selector::Table(0), Representation::Text);
    let err = observe(&mut pdf.store, &pdf.field, &req, Limits::DEFAULT).unwrap_err();
    assert_eq!(err.class(), ErrorClass::UnsupportedFeature);
    // The planner fails closed on the same pair.
    let field = Field::open(&pdf.store, &pdf.field, Limits::DEFAULT).unwrap();
    let perr = vole_document::field::plan::plan(field.manifest(), &pdf.store, &req).unwrap_err();
    assert_eq!(perr.class(), ErrorClass::UnsupportedFeature);
}

#[test]
fn pdf_exactness_and_native_behavior_are_unchanged() {
    let mut pdf = Fixture::new("regress", pdf_source());
    // Exact materialization is untouched by the widened API.
    let field = Field::open(&pdf.store, &pdf.field, Limits::DEFAULT).unwrap();
    assert_eq!(
        field.materialize_exact(Limits::DEFAULT).unwrap(),
        pdf.source
    );
    drop(field);
    // A native PDF metadata observation keeps its shape/basis.
    let req = ObserveRequest::new(Selector::Document, Representation::Metadata);
    let (answer, _, _) = observe(&mut pdf.store, &pdf.field, &req, Limits::DEFAULT).unwrap();
    assert_eq!(
        answer.basis,
        vole_document::field::provenance::Basis::DeterministicallyDerived
    );
    assert!(!answer.exact);
}

#[test]
fn capabilities_cli_detects_a_raw_document_from_bytes() {
    let dir = temp_dir("caps-cli");
    let pdf_path = dir.join("document.bin"); // a deliberately non-PDF extension
    std::fs::write(&pdf_path, pdf_source()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_vole-document"))
        .arg("capabilities")
        .arg(&pdf_path)
        .output()
        .expect("spawn capabilities");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"format\":\"pdf\""), "{stdout}");
    assert!(stdout.contains("\"root\":\"document\""), "{stdout}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn recorded_format_matches_detection() {
    for (label, source, expected) in [
        ("rec-pdf", pdf_source(), DocumentFormat::Pdf),
        ("rec-docx", docx_source(), DocumentFormat::Docx),
        ("rec-epub", epub_source(), DocumentFormat::Epub),
    ] {
        let mut fx = Fixture::new(label, source);
        assert_eq!(fx.format, expected);
        let field = Field::open(&fx.store, &fx.field, Limits::DEFAULT).unwrap();
        assert_eq!(
            DocumentFormat::from_provenance(&field.manifest().provenance),
            Some(expected)
        );
        // Silence the unused-mut warning on stores reopened through Field.
        let _ = &mut fx.store;
    }
}
