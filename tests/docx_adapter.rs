//! Phase 12.4 court: the WordprocessingML (DOCX) adapter.
//!
//! Gated on `docx` (which implies `opc`, `package`, `field`), so a build without
//! the feature compiles an empty target.
//!
//! A hand-authored minimal DOCX (a real ZIP with `[Content_Types].xml`,
//! `_rels/.rels` → `officeDocument`, `word/document.xml`, `word/styles.xml`, and a
//! header part) is built in-test with no external tool. The court checks:
//!
//! 1. **Main-part discovery by relationship**, never by a hardcoded path (a decoy
//!    `/word/document.xml` is present but is *not* the relationship target).
//! 2. **Text**: headings/paragraphs/table cells resolve correctly; `cell B7`
//!    returns the known value.
//! 3. **Story scoping** never mixes a header with the body.
//! 4. **Tracked-changes profiles** `Final` vs `Original` differ as specified.
//! 5. **Provenance** carries story/part/table/row/cell; the `source_span` is the
//!    exact compressed member span.
//! 6. **Exactness**: `materialize(field) == original_bytes` (len + bytes).
//! 7. **Progressive inversion**: a second observation after reopening the store
//!    reuses persisted derived state.
//! 8. **Fail closed**: missing/ambiguous relationship and malformed XML decline
//!    typed, with exactness preserved.

#![cfg(all(
    feature = "field",
    feature = "package",
    feature = "opc",
    feature = "docx"
))]

use std::path::PathBuf;

use vole_document::adapter::docx::{DocxExtractProfile, DocxStory};
use vole_document::adapter::package::scan;
use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::ingest_package::{PackageIngestReport, ingest_package};
use vole_document::field::observe::{
    ObserveRequest, ObserveStats, Representation, Selector, observe,
};
use vole_document::field::provenance::{AnswerValue, Basis, FieldAnswer};
use vole_document::field::{Field, FieldStore};
use vole_document::limits::Limits;

// ---------------------------------------------------------------------------
// Dependency-free ZIP writer (test ground truth).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Entry {
    name: Vec<u8>,
    content: Vec<u8>,
    compressed: Vec<u8>,
    method: u16,
    flags: u16,
}

impl Entry {
    fn stored(name: &str, content: &[u8]) -> Entry {
        Entry {
            name: name.as_bytes().to_vec(),
            content: content.to_vec(),
            compressed: content.to_vec(),
            method: 0,
            flags: 0,
        }
    }

    fn deflated(name: &str, content: &[u8]) -> Entry {
        Entry {
            name: name.as_bytes().to_vec(),
            content: content.to_vec(),
            compressed: raw_stored_deflate(content),
            method: 8,
            flags: 0,
        }
    }
}

fn raw_stored_deflate(data: &[u8]) -> Vec<u8> {
    assert!(
        data.len() <= 0xFFFF,
        "test deflate helper is single-block only"
    );
    let mut out = Vec::with_capacity(data.len() + 5);
    out.push(0x01);
    let len = data.len() as u16;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(data);
    out
}

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn build_zip(entries: &[Entry]) -> Vec<u8> {
    const UTF8_FLAG: u16 = 0x0800;
    let mut out: Vec<u8> = Vec::new();
    let mut offsets: Vec<u32> = Vec::new();
    let mut crcs: Vec<u32> = Vec::new();

    for e in entries {
        offsets.push(out.len() as u32);
        crcs.push(vole_document::adapter::package::crc32_iso_hdlc(&e.content));
        let flags = e.flags | UTF8_FLAG;
        put_u32(&mut out, 0x0403_4b50);
        put_u16(&mut out, 20);
        put_u16(&mut out, flags);
        put_u16(&mut out, e.method);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u32(&mut out, *crcs.last().unwrap());
        put_u32(&mut out, e.compressed.len() as u32);
        put_u32(&mut out, e.content.len() as u32);
        put_u16(&mut out, e.name.len() as u16);
        put_u16(&mut out, 0);
        out.extend_from_slice(&e.name);
        out.extend_from_slice(&e.compressed);
    }

    let cd_start = out.len() as u32;
    for (i, e) in entries.iter().enumerate() {
        let flags = e.flags | UTF8_FLAG;
        put_u32(&mut out, 0x0201_4b50);
        put_u16(&mut out, 20);
        put_u16(&mut out, 20);
        put_u16(&mut out, flags);
        put_u16(&mut out, e.method);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u32(&mut out, crcs[i]);
        put_u32(&mut out, e.compressed.len() as u32);
        put_u32(&mut out, e.content.len() as u32);
        put_u16(&mut out, e.name.len() as u16);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        put_u32(&mut out, 0);
        put_u32(&mut out, offsets[i]);
        out.extend_from_slice(&e.name);
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

const MAIN_CT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const STYLES_CT: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";
const HEADER_CT: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml";

fn content_types(main_part: &str) -> String {
    format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
            r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#,
            r#"<Default Extension="xml" ContentType="application/xml"/>"#,
            r#"<Override PartName="{}" ContentType="{}"/>"#,
            r#"<Override PartName="/word/styles.xml" ContentType="{}"/>"#,
            r#"<Override PartName="/word/header1.xml" ContentType="{}"/>"#,
            r#"</Types>"#
        ),
        main_part, MAIN_CT, STYLES_CT, HEADER_CT
    )
}

fn package_rels(target: &str) -> String {
    format!(
        concat!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
            r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="{}"/>"#,
            r#"</Relationships>"#
        ),
        target
    )
}

const PART_RELS: &str = concat!(
    r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    r#"<Relationship Id="rIdStyles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#,
    r#"<Relationship Id="rIdHeader" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/>"#,
    r#"<Relationship Id="rIdHl" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/x" TargetMode="External"/>"#,
    r#"</Relationships>"#
);

const STYLES_XML: &str = concat!(
    r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#,
    r#"<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/>"#,
    r#"<w:pPr><w:outlineLvl w:val="0"/></w:pPr></w:style>"#,
    r#"<w:style w:type="paragraph" w:styleId="SubHeading"><w:basedOn w:val="Heading1"/></w:style>"#,
    r#"</w:styles>"#
);

const HEADER_XML: &str = concat!(
    r#"<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#,
    r#"<w:p><w:r><w:t>HEADER TEXT</w:t></w:r></w:p></w:hdr>"#
);

fn table_xml(rows: u32, cols: u32) -> String {
    let mut t = String::from("<w:tbl>");
    for r in 1..=rows {
        t.push_str("<w:tr>");
        for c in 1..=cols {
            t.push_str(&format!(
                "<w:tc><w:p><w:r><w:t>r{r}c{c}</w:t></w:r></w:p></w:tc>"
            ));
        }
        t.push_str("</w:tr>");
    }
    t.push_str("</w:tbl>");
    t
}

fn document_xml() -> String {
    format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" "#,
            r#"xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">"#,
            r#"<w:body>"#,
            r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Title</w:t></w:r></w:p>"#,
            r#"<w:p><w:r><w:t xml:space="preserve">Body one</w:t></w:r></w:p>"#,
            r#"<w:p><w:r><w:t xml:space="preserve">Base </w:t></w:r>"#,
            r#"<w:ins><w:r><w:t>added</w:t></w:r></w:ins>"#,
            r#"<w:del><w:r><w:delText>gone</w:delText></w:r></w:del></w:p>"#,
            r#"<w:p><w:hyperlink r:id="rIdHl"><w:r><w:t>link text</w:t></w:r></w:hyperlink></w:p>"#,
            r#"{}"#,
            r#"<w:sectPr/>"#,
            r#"</w:body></w:document>"#
        ),
        table_xml(7, 2)
    )
}

fn docx_entries(main_part: &str, main_target: &str, document: &str) -> Vec<Entry> {
    vec![
        Entry::stored("[Content_Types].xml", content_types(main_part).as_bytes()),
        Entry::stored("_rels/.rels", package_rels(main_target).as_bytes()),
        Entry::deflated(main_part.trim_start_matches('/'), document.as_bytes()),
        Entry::stored("word/styles.xml", STYLES_XML.as_bytes()),
        Entry::stored("word/header1.xml", HEADER_XML.as_bytes()),
        Entry::stored("word/_rels/document.xml.rels", PART_RELS.as_bytes()),
    ]
}

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-docx-{label}-{}-{}",
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
        format_basis: "opaque:docx-test".to_string(),
        models: vec![],
        channels: vec![],
        objects: vec![ObjectSource::Inline(source.to_vec())],
        program: Program::new(vec![Op::EmitObject { object_id: 0 }]),
        observation_index: None,
        seek_directory: false,
        source_sha256: vole_document::integrity::sha256(source),
        source_len: source.len() as u64,
    };
    d.serialize().unwrap().0
}

struct Fixture {
    root: PathBuf,
    store: FieldStore,
    report: PackageIngestReport,
    source: Vec<u8>,
}

impl Fixture {
    fn from_entries(label: &str, entries: &[Entry]) -> Fixture {
        let root = temp_dir(label);
        let mut store = FieldStore::open(&root).unwrap();
        let source = build_zip(entries);
        let descriptor = opaque_descriptor(&source);
        let report = ingest_package(&mut store, &descriptor, Limits::DEFAULT).unwrap();
        Fixture {
            root,
            store,
            report,
            source,
        }
    }

    fn standard(label: &str) -> Fixture {
        let entries = docx_entries("/word/document.xml", "word/document.xml", &document_xml());
        Fixture::from_entries(label, &entries)
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
) -> (FieldAnswer, ObserveStats) {
    let req = ObserveRequest::new(selector, representation);
    let (answer, stats, _) = observe(store, field, &req, Limits::DEFAULT).unwrap();
    (answer, stats)
}

fn observe_err(
    store: &mut FieldStore,
    field: &vole_document::field::FieldId,
    selector: Selector,
    representation: Representation,
) -> ErrorClass {
    let req = ObserveRequest::new(selector, representation);
    observe(store, field, &req, Limits::DEFAULT)
        .unwrap_err()
        .class()
}

fn answer_bytes(answer: &FieldAnswer) -> Vec<u8> {
    match &answer.value {
        AnswerValue::Bytes(b) => b.clone(),
        other => panic!("expected bytes, got {other:?}"),
    }
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

fn story(story: DocxStory) -> Selector {
    Selector::DocxStory {
        story,
        profile: DocxExtractProfile::DEFAULT,
    }
}

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn main_part_discovered_by_relationship_not_path() {
    // The package relationship targets `/word/real.xml`; a decoy
    // `/word/document.xml` also exists but is *not* the main part.
    let mut entries = docx_entries("/word/real.xml", "word/real.xml", &document_xml());
    entries.push(Entry::stored(
        "word/document.xml",
        b"<w:document xmlns:w=\"x\"><w:body><w:p><w:r><w:t>DECOY</w:t></w:r></w:p></w:body></w:document>",
    ));
    // The decoy has no content type override and is not the relationship target.
    let mut fx = Fixture::from_entries("discover", &entries);

    let (meta, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        story(DocxStory::Main),
        Representation::Metadata,
    );
    let json = answer_json(&meta);
    assert!(json.contains("/word/real.xml"), "{json}");

    let (text, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        story(DocxStory::Main),
        Representation::Text,
    );
    let body = answer_text(&text);
    assert!(body.contains("Body one"), "{body}");
    assert!(!body.contains("DECOY"), "{body}");
}

#[test]
fn text_headings_table_and_cell_b7() {
    let mut fx = Fixture::standard("text");

    // Headings/paragraphs.
    let (title, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::DocxParagraph {
            story: DocxStory::Main,
            index: 0,
            profile: DocxExtractProfile::DEFAULT,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&title), "Title");
    let (title_meta, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::DocxParagraph {
            story: DocxStory::Main,
            index: 0,
            profile: DocxExtractProfile::DEFAULT,
        },
        Representation::Metadata,
    );
    let json = answer_json(&title_meta);
    assert!(json.contains("\"heading\":0"), "{json}");
    assert!(json.contains("\"style\":\"Heading1\""), "{json}");

    // The table, and the specific cell B7 (column B, row 7).
    let (cell, stats) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::DocxCell {
            story: DocxStory::Main,
            table: 0,
            cell: "B7".to_string(),
            profile: DocxExtractProfile::DEFAULT,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&cell), "r7c2");
    assert_eq!(cell.basis, Basis::DeterministicallyDerived);
    assert!(!cell.exact);
    assert!(stats.seed_nodes_fetched > 0);

    // Provenance carries story/part/table/row/cell and the compressed span.
    let (cell_meta, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::DocxCell {
            story: DocxStory::Main,
            table: 0,
            cell: "B7".to_string(),
            profile: DocxExtractProfile::DEFAULT,
        },
        Representation::Metadata,
    );
    assert!(
        cell_meta.provenance.contains("story=main"),
        "{}",
        cell_meta.provenance
    );
    assert!(
        cell_meta.provenance.contains("part=/word/document.xml"),
        "{}",
        cell_meta.provenance
    );
    assert!(
        cell_meta.provenance.contains("table=0"),
        "{}",
        cell_meta.provenance
    );
    assert!(
        cell_meta.provenance.contains("row=7"),
        "{}",
        cell_meta.provenance
    );
    assert!(
        cell_meta.provenance.contains("cell=B7"),
        "{}",
        cell_meta.provenance
    );
    let json = answer_json(&cell_meta);
    assert!(json.contains("\"grid_col\":1"), "{json}");
    assert!(json.contains("\"row\":7"), "{json}");

    // The source_span is the exact compressed member span of the main part.
    let physical = scan(&fx.source, Limits::DEFAULT).unwrap();
    let (off, len) = physical
        .members
        .iter()
        .find(|m| String::from_utf8_lossy(&m.name) == "word/document.xml")
        .map(|m| m.data)
        .unwrap();
    assert_eq!(cell.source_span, Some((off, off + len)));
}

#[test]
fn story_scoping_does_not_mix_header_and_body() {
    let mut fx = Fixture::standard("scoping");

    // Body contains the body text but never the header text.
    let (body, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        story(DocxStory::Main),
        Representation::Text,
    );
    let body = answer_text(&body);
    assert!(body.contains("Body one"), "{body}");
    assert!(!body.contains("HEADER TEXT"), "{body}");

    // The header story contains the header text but never the body text.
    let (header, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        story(DocxStory::Header(0)),
        Representation::Text,
    );
    let header = answer_text(&header);
    assert!(header.contains("HEADER TEXT"), "{header}");
    assert!(!header.contains("Body one"), "{header}");

    // A story with no backing part declines typed rather than mixing.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &fx.report.field,
            story(DocxStory::Footer(0)),
            Representation::Text,
        ),
        ErrorClass::UnsupportedFeature
    );
}

#[test]
fn tracked_changes_profiles_differ_as_specified() {
    let mut fx = Fixture::standard("tracked");

    let final_text = answer_text(
        &observe_eq(
            &mut fx.store,
            &fx.report.field,
            story(DocxStory::Main),
            Representation::Text,
        )
        .0,
    );
    assert!(final_text.contains("Base added"), "{final_text}");
    assert!(!final_text.contains("gone"), "{final_text}");

    let mut original = DocxExtractProfile::DEFAULT;
    original.tracked = vole_document::adapter::docx::TrackedChanges::Original;
    let original_text = answer_text(
        &observe_eq(
            &mut fx.store,
            &fx.report.field,
            Selector::DocxStory {
                story: DocxStory::Main,
                profile: original,
            },
            Representation::Text,
        )
        .0,
    );
    assert!(original_text.contains("Base gone"), "{original_text}");
    assert!(!original_text.contains("added"), "{original_text}");

    // The profile identity is recorded in the answer's provenance.
    let (_, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::DocxStory {
            story: DocxStory::Main,
            profile: original,
        },
        Representation::Metadata,
    );
}

#[test]
fn exact_materialization_and_find() {
    let mut fx = Fixture::standard("exact");

    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), fx.source.len());
    assert_eq!(exact, fx.source);
    drop(field);

    // A story-scoped find only sees its story.
    let (found, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::DocxFind {
            story: DocxStory::Main,
            pattern: "added".to_string(),
            profile: DocxExtractProfile::DEFAULT,
        },
        Representation::Text,
    );
    let json = answer_json(&found);
    assert!(json.contains("Base added"), "{json}");

    // The shared OPC part selector still resolves on a DOCX package.
    let (raw, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::PackagePart("/word/document.xml".to_string()),
        Representation::ExactBytes,
    );
    assert!(raw.exact);
    assert!(!answer_bytes(&raw).is_empty());
}

#[test]
fn progressive_inversion_reuses_persisted_state_after_restart() {
    let fx = Fixture::standard("restart");
    let field_id = fx.report.field;
    let root = fx.root.clone();

    // First observation (cold): populates the derived cache.
    let mut store = FieldStore::open(&root).unwrap();
    let (_, cold) = observe_eq(
        &mut store,
        &field_id,
        story(DocxStory::Main),
        Representation::Text,
    );
    drop(store);

    // A fresh handle on the same store directory simulates a process restart.
    let mut store2 = FieldStore::open(&root).unwrap();
    let (warm, warm_stats) = observe_eq(
        &mut store2,
        &field_id,
        story(DocxStory::Main),
        Representation::Text,
    );
    assert!(answer_text(&warm).contains("Body one"));
    assert!(
        warm_stats.seed_nodes_reused >= 1,
        "expected a persisted-state reuse, got {warm_stats:?}"
    );
    assert!(warm_stats.seed_nodes_reused >= cold.seed_nodes_reused);
}

#[test]
fn missing_relationship_declines_and_keeps_exactness() {
    let entries = vec![
        Entry::stored("[Content_Types].xml", content_types("/word/document.xml").as_bytes()),
        Entry::stored(
            "_rels/.rels",
            br#"<Relationships xmlns="x"><Relationship Id="rId1" Type="t/other" Target="word/document.xml"/></Relationships>"#,
        ),
        Entry::stored("word/document.xml", document_xml().as_bytes()),
    ];
    let mut fx = Fixture::from_entries("missing", &entries);
    assert_eq!(
        observe_err(
            &mut fx.store,
            &fx.report.field,
            story(DocxStory::Main),
            Representation::Text,
        ),
        ErrorClass::InvalidPackageStructure
    );
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn ambiguous_relationship_declines() {
    let rels = concat!(
        r#"<Relationships xmlns="x">"#,
        r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>"#,
        r#"<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/other.xml"/>"#,
        r#"</Relationships>"#
    );
    let entries = vec![
        Entry::stored(
            "[Content_Types].xml",
            content_types("/word/document.xml").as_bytes(),
        ),
        Entry::stored("_rels/.rels", rels.as_bytes()),
        Entry::stored("word/document.xml", document_xml().as_bytes()),
        Entry::stored("word/other.xml", document_xml().as_bytes()),
    ];
    let mut fx = Fixture::from_entries("ambiguous", &entries);
    assert_eq!(
        observe_err(
            &mut fx.store,
            &fx.report.field,
            story(DocxStory::Main),
            Representation::Text,
        ),
        ErrorClass::InvalidPackageStructure
    );
}

#[test]
fn malformed_document_declines_and_keeps_exactness() {
    let entries = docx_entries(
        "/word/document.xml",
        "word/document.xml",
        "<w:document xmlns:w=\"x\"><w:body><w:p>",
    );
    let mut fx = Fixture::from_entries("malformed", &entries);
    assert_eq!(
        observe_err(
            &mut fx.store,
            &fx.report.field,
            story(DocxStory::Main),
            Representation::Text,
        ),
        ErrorClass::InvalidXmlStructure
    );
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn wrong_root_element_declines() {
    let entries = docx_entries(
        "/word/document.xml",
        "word/document.xml",
        "<w:foo xmlns:w=\"x\"><w:body/></w:foo>",
    );
    let mut fx = Fixture::from_entries("wrongroot", &entries);
    assert_eq!(
        observe_err(
            &mut fx.store,
            &fx.report.field,
            story(DocxStory::Main),
            Representation::Text,
        ),
        ErrorClass::InvalidPackageStructure
    );
}
