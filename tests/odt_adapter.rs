//! Phase 13.3 court: the OpenDocument Text (ODT) adapter.
//!
//! Gated on `odt` (which implies `opc`, `package`, `field`), so a build without
//! the feature compiles an empty target.
//!
//! A hand-authored minimal ODT (a real ZIP whose first member is the stored
//! `mimetype`, with `META-INF/manifest.xml`, `content.xml`, `styles.xml`,
//! `meta.xml`, and a `Pictures/` resource) is built in-test with no external tool.
//! The court checks:
//!
//! 1. **Byte-based detection + capabilities** (`mimetype`/manifest, never a name).
//! 2. **Content**: headings/paragraphs/table cells resolve; common `cell` and
//!    links/resources resolve.
//! 3. **Tracked-changes profiles** `Final` vs `Original` differ as specified.
//! 4. **Provenance** carries `format=odt;` and the part/profile.
//! 5. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp).
//! 6. **Source + descriptor deleted, fresh process**: still queryable and exactly
//!    rematerializable.
//! 7. **Fail closed**: a missing/malformed manifest declines typed, with exactness
//!    preserved.

#![cfg(all(
    feature = "field",
    feature = "package",
    feature = "opc",
    feature = "odt"
))]

use std::path::PathBuf;

use vole_document::adapter::odt::OdtExtractProfile;
use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::capabilities::capabilities_for_format;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::ingest_package::{PackageIngestReport, ingest_package};
use vole_document::field::observe::{
    ObserveRequest, ObserveStats, Representation, Selector, observe,
};
use vole_document::field::provenance::{AnswerValue, FieldAnswer};
use vole_document::field::{Field, FieldStore};
use vole_document::integrity::sha256;
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

const ODT_MIMETYPE: &str = "application/vnd.oasis.opendocument.text";

fn manifest_xml(with_picture: bool) -> String {
    let mut s = String::from(concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        r#"<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.2">"#,
        r#"<manifest:file-entry manifest:full-path="/" manifest:version="1.2" manifest:media-type="application/vnd.oasis.opendocument.text"/>"#,
        r#"<manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>"#,
        r#"<manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/>"#,
        r#"<manifest:file-entry manifest:full-path="meta.xml" manifest:media-type="text/xml"/>"#
    ));
    if with_picture {
        s.push_str(
            r#"<manifest:file-entry manifest:full-path="Pictures/pixel.png" manifest:media-type="image/png"/>"#,
        );
    }
    s.push_str("</manifest:manifest>");
    s
}

fn table_xml(rows: u32, cols: u32) -> String {
    let mut t = String::from("<table:table>");
    for r in 1..=rows {
        t.push_str("<table:table-row>");
        for c in 1..=cols {
            t.push_str(&format!(
                "<table:table-cell><text:p>r{r}c{c}</text:p></table:table-cell>"
            ));
        }
        t.push_str("</table:table-row>");
    }
    t.push_str("</table:table>");
    t
}

fn content_xml() -> String {
    format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            r#"<office:document-content"#,
            r#" xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0""#,
            r#" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0""#,
            r#" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0""#,
            r#" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0""#,
            r#" xmlns:xlink="http://www.w3.org/1999/xlink""#,
            r#" office:version="1.2">"#,
            r#"<office:body><office:text>"#,
            r#"<text:h text:outline-level="1">Title</text:h>"#,
            r#"<text:p>Body one</text:p>"#,
            r#"<text:p>Base <text:change-start text:change-id="ct-ins"/>added<text:change-end text:change-id="ct-ins"/> <text:change-start text:change-id="ct-del"/>gone<text:change-end text:change-id="ct-del"/></text:p>"#,
            r#"<text:p><text:a xlink:href="https://example.com/x">link text</text:a></text:p>"#,
            r#"<text:p><draw:frame><draw:image xlink:href="Pictures/pixel.png"/></draw:frame></text:p>"#,
            r#"<text:p>Text with note<text:note text:id="ft1" text:note-class="footnote"><text:note-citation>1</text:note-citation><text:note-body><text:p>note body</text:p></text:note-body></text:note></text:p>"#,
            r#"<text:p><text:bookmark text:name="bm1"/>bookmarked</text:p>"#,
            r#"<text:list><text:list-item><text:p>item one</text:p></text:list-item><text:list-item><text:p>item two</text:p></text:list-item></text:list>"#,
            r#"{}"#,
            r#"<text:section text:name="sec1"><text:p>section text</text:p></text:section>"#,
            r#"<text:tracked-changes>"#,
            r#"<text:changed-region text:id="ct-ins"><text:insertion><office:change-info/></text:insertion></text:changed-region>"#,
            r#"<text:changed-region text:id="ct-del"><text:deletion><office:change-info/></text:deletion></text:changed-region>"#,
            r#"</text:tracked-changes>"#,
            r#"</office:text></office:body></office:document-content>"#
        ),
        table_xml(7, 2)
    )
}

fn odt_entries() -> Vec<Entry> {
    vec![
        Entry::stored("mimetype", ODT_MIMETYPE.as_bytes()),
        Entry::stored("META-INF/manifest.xml", manifest_xml(true).as_bytes()),
        Entry::deflated("content.xml", content_xml().as_bytes()),
        Entry::stored(
            "styles.xml",
            br#"<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"/>"#,
        ),
        Entry::stored(
            "meta.xml",
            br#"<office:document-meta xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"/>"#,
        ),
        Entry::stored("Pictures/pixel.png", b"\x89PNG\r\n\x1a\nfake"),
    ]
}

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-odt-{label}-{}-{}",
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
        format_basis: "opaque:odt-test".to_string(),
        models: vec![],
        channels: vec![],
        objects: vec![ObjectSource::Inline(source.to_vec())],
        program: Program::new(vec![Op::EmitObject { object_id: 0 }]),
        observation_index: None,
        seek_directory: false,
        source_sha256: sha256(source),
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
        Fixture::from_entries(label, &odt_entries())
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

const P: OdtExtractProfile = OdtExtractProfile::DEFAULT;

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_and_capabilities_are_byte_based() {
    let source = build_zip(&odt_entries());
    assert_eq!(
        detect_document_format(&source, Limits::DEFAULT),
        DocumentFormat::Odt
    );
    // A ZIP that is neither DOCX/EPUB/ODT is opaque.
    let plain = build_zip(&[Entry::stored("hello.txt", b"hi")]);
    assert_eq!(
        detect_document_format(&plain, Limits::DEFAULT),
        DocumentFormat::Opaque
    );

    let caps = capabilities_for_format(DocumentFormat::Odt);
    assert!(caps.compiled);
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
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    assert!(caps.native_selectors.contains(&"odt-paragraph"));
    assert!(caps.native_selectors.contains(&"odt-table"));
    assert!(
        caps.profiles.iter().any(|p| p.contains("final")),
        "profiles: {:?}",
        caps.profiles
    );
    let json = caps.to_json();
    assert!(json.contains("\"format\":\"odt\""), "{json}");
}

#[test]
fn headings_paragraphs_table_cell_and_links_resolve() {
    let mut fx = Fixture::standard("content");

    // Native paragraph (index among non-heading paragraphs).
    let (p1, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdtParagraph {
            index: 1,
            profile: P,
        },
        Representation::Text,
    );
    assert!(
        answer_text(&p1).starts_with("Base "),
        "{:?}",
        answer_text(&p1)
    );

    // Native heading with its outline level.
    let (h, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdtHeading {
            index: 0,
            profile: P,
        },
        Representation::Metadata,
    );
    let hj = answer_json(&h);
    assert!(hj.contains("\"level\":1"), "{hj}");
    let (ht, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdtHeading {
            index: 0,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&ht), "Title");

    // Common heading / block.
    let (ch, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Heading(0),
        Representation::Text,
    );
    assert_eq!(answer_text(&ch), "Title");

    // Table cell by physical (table, row, col): r7c2.
    let (cell, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdtCell {
            table: 0,
            row: 6,
            col: 1,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&cell), "r7c2");

    // Common cell selector resolves the same value.
    let (ccell, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Cell {
            table: 0,
            row: 6,
            col: 1,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&ccell), "r7c2");

    // Common table text.
    let (table, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Table(0),
        Representation::Text,
    );
    let tt = answer_text(&table);
    assert!(tt.starts_with("r1c1\tr1c2"), "{tt}");
    assert!(tt.contains("r7c1\tr7c2"), "{tt}");

    // Common link.
    let (link, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Link(0),
        Representation::Metadata,
    );
    let lj = answer_json(&link);
    assert!(lj.contains("link text"), "{lj}");
    assert!(lj.contains("https://example.com/x"), "{lj}");
    assert!(lj.contains("\"external\":true"), "{lj}");

    // Common resource resolves to a package member.
    let (res, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Resource(0),
        Representation::Metadata,
    );
    let rj = answer_json(&res);
    assert!(rj.contains("Pictures/pixel.png"), "{rj}");
    assert!(rj.contains("\"external\":false"), "{rj}");

    // Native list.
    let (list, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdtList {
            index: 0,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&list), "item one\nitem two");

    // Native + common find.
    let (found, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdtFind {
            pattern: "Text with note".to_string(),
            profile: P,
        },
        Representation::Text,
    );
    assert!(answer_json(&found).contains("Text with note"), "{found:?}");

    // Metadata names the format and part.
    let (meta, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Metadata,
        Representation::Metadata,
    );
    let mj = answer_json(&meta);
    assert!(mj.contains("\"format\":\"odt\""), "{mj}");
    assert!(mj.contains("\"part\":\"content.xml\""), "{mj}");
    assert!(mj.contains("\"notes\":1"), "{mj}");
    assert!(mj.contains("\"resources\":1"), "{mj}");
    assert!(mj.contains("\"sections\":1"), "{mj}");
}

#[test]
fn tracked_changes_profiles_differ_as_specified() {
    let mut fx = Fixture::standard("tracked");

    let (final_p, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdtParagraph {
            index: 1,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&final_p), "Base added");

    let mut original = P;
    original.tracked = vole_document::adapter::odt::OdtTrackedChanges::Original;
    let (original_p, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdtParagraph {
            index: 1,
            profile: original,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&original_p), "Base gone");

    // The profile identity is recorded in the answer's provenance.
    assert!(
        final_p.provenance.contains("final"),
        "{}",
        final_p.provenance
    );
    assert!(
        original_p.provenance.contains("original"),
        "{}",
        original_p.provenance
    );
}

#[test]
fn provenance_carries_format_part_and_profile() {
    let mut fx = Fixture::standard("prov");
    let (text, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Text,
        Representation::Text,
    );
    let p = &text.provenance;
    assert!(p.starts_with("format=odt;common;"), "{p}");
    assert!(p.contains("odt;part=content.xml"), "{p}");
    assert!(p.contains("profile=v1-final"), "{p}");
    assert!(!text.exact, "derived observations are never exact");
}

#[test]
fn exact_materialization_is_byte_identical() {
    let mut fx = Fixture::standard("exact");
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), fx.source.len());
    assert_eq!(sha256(&exact), sha256(&fx.source));
    assert_eq!(exact, fx.source);
    drop(field);

    // A native part observation also stays exact through the raw member span.
    let (raw, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdtPart("content.xml".to_string()),
        Representation::ExactBytes,
    );
    match &raw.value {
        AnswerValue::Bytes(b) => assert!(!b.is_empty()),
        other => panic!("expected bytes, got {other:?}"),
    }
    // still exact after the observation
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn removal_then_fresh_process_is_queryable_and_exact() {
    let root = temp_dir("removal");
    let source = build_zip(&odt_entries());
    let descriptor = opaque_descriptor(&source);
    let src_path = root.join("doc.odt");
    let desc_path = root.join("doc.voldoc");
    std::fs::write(&src_path, &source).unwrap();
    std::fs::write(&desc_path, &descriptor).unwrap();

    let store_root = root.join("store");
    let field_id = {
        let mut store = FieldStore::open(&store_root).unwrap();
        let report = ingest_package(&mut store, &descriptor, Limits::DEFAULT).unwrap();
        report.field
        // store drops here: a fresh process reopens the directory.
    };

    // Delete the physical source and the external descriptor.
    std::fs::remove_file(&src_path).unwrap();
    std::fs::remove_file(&desc_path).unwrap();

    // Fresh handle on the same store directory (a new process).
    let mut store2 = FieldStore::open(&store_root).unwrap();
    let (text, _) = observe_eq(&mut store2, &field_id, Selector::Text, Representation::Text);
    assert!(answer_text(&text).contains("Body one"), "{text:?}");

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), source.len());
    assert_eq!(sha256(&exact), sha256(&source));
    assert_eq!(exact, source);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn missing_manifest_declines_typed_and_keeps_exactness() {
    // No `META-INF/manifest.xml`: detection still sees the ODT `mimetype`, but the
    // derived model declines typed and exactness is untouched.
    let entries = vec![
        Entry::stored("mimetype", ODT_MIMETYPE.as_bytes()),
        Entry::deflated("content.xml", content_xml().as_bytes()),
    ];
    let mut fx = Fixture::from_entries("nomanifest", &entries);
    assert_eq!(fx.report.format, DocumentFormat::Odt);
    let class = observe_err(
        &mut fx.store,
        &fx.report.field,
        Selector::Text,
        Representation::Text,
    );
    assert_eq!(class, ErrorClass::InvalidPackageStructure);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn malformed_manifest_declines_typed_and_keeps_exactness() {
    let entries = vec![
        Entry::stored("mimetype", ODT_MIMETYPE.as_bytes()),
        Entry::stored(
            "META-INF/manifest.xml",
            b"<manifest:manifest></notmanifest>",
        ),
        Entry::deflated("content.xml", content_xml().as_bytes()),
    ];
    let mut fx = Fixture::from_entries("badmanifest", &entries);
    let class = observe_err(
        &mut fx.store,
        &fx.report.field,
        Selector::Text,
        Representation::Text,
    );
    assert_eq!(class, ErrorClass::InvalidXmlStructure);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn progressive_inversion_reuses_persisted_state_after_restart() {
    let fx = Fixture::standard("restart");
    let field_id = fx.report.field;
    let root = fx.root.clone();

    // First observation (cold): populates the derived cache.
    let mut store = FieldStore::open(&root).unwrap();
    let (_, cold) = observe_eq(&mut store, &field_id, Selector::Text, Representation::Text);
    drop(store);

    // A fresh handle on the same store directory simulates a process restart.
    let mut store2 = FieldStore::open(&root).unwrap();
    let (warm, warm_stats) =
        observe_eq(&mut store2, &field_id, Selector::Text, Representation::Text);
    assert!(answer_text(&warm).contains("Body one"));
    assert!(
        warm_stats.seed_nodes_reused >= 1,
        "expected a persisted-state reuse, got {warm_stats:?}"
    );
    assert!(warm_stats.seed_nodes_reused >= cold.seed_nodes_reused);
}
