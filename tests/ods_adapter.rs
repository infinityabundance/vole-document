//! Phase 21.3.1 court: the OpenDocument Spreadsheet (ODS) adapter.
//!
//! Gated on `ods` (which implies `opc`, `package`, `field`), so a build without
//! the feature compiles an empty target.
//!
//! A hand-authored minimal ODS (a real ZIP whose first member is the stored
//! `mimetype`, with `META-INF/manifest.xml`, `content.xml`, `styles.xml`, and
//! `meta.xml`) is built in-test with no external tool. The court checks:
//!
//! 1. **Byte-based detection + capabilities** (`mimetype`/manifest, never a name).
//! 2. **Content**: sheets/cells resolve; typed values, stored formulas, displayed
//!    text, style names, and decoded-part spans are **separate** facets.
//! 3. **Common** `metadata`/`text`/`table`/`cell`/`search-match` and native
//!    `ods-sheet`/`ods-cell`/`ods-find`/`ods-styles`/`ods-named-expressions`/
//!    `ods-comments` resolve.
//! 4. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp).
//! 5. **Source + descriptor deleted, fresh process**: still queryable and exactly
//!    rematerializable.
//! 6. **Fail closed**: a missing/malformed manifest declines typed; random bytes
//!    never panic; a `table:number-rows-repeated` bomb declines typed
//!    (resource limit).

#![cfg(all(
    feature = "field",
    feature = "package",
    feature = "opc",
    feature = "ods"
))]

use std::path::PathBuf;

use vole_document::adapter::ods::OdsExtractProfile;
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

const ODS_MIMETYPE: &str = "application/vnd.oasis.opendocument.spreadsheet";

fn manifest_xml() -> String {
    concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        r#"<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.2">"#,
        r#"<manifest:file-entry manifest:full-path="/" manifest:version="1.2" manifest:media-type="application/vnd.oasis.opendocument.spreadsheet"/>"#,
        r#"<manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>"#,
        r#"<manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/>"#,
        r#"<manifest:file-entry manifest:full-path="meta.xml" manifest:media-type="text/xml"/>"#,
        r#"</manifest:manifest>"#,
    )
    .to_string()
}

fn content_xml() -> String {
    concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        r#"<office:document-content"#,
        r#" xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0""#,
        r#" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0""#,
        r#" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0""#,
        r#" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0""#,
        r#" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0""#,
        r#" xmlns:number="urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0""#,
        r#" xmlns:dc="http://purl.org/dc/elements/1.1/""#,
        r#" office:version="1.2">"#,
        r#"<office:automatic-styles>"#,
        r#"<style:style style:name="ce1" style:family="table-cell"><style:text-properties fo:font-weight="bold"/></style:style>"#,
        r##"<style:style style:name="ce2" style:family="table-cell" style:parent-style-name="ce1" style:data-style-name="N1"><style:table-cell-properties fo:background-color="#ffff00"/></style:style>"##,
        r#"<number:number-style style:name="N1"><number:number number:decimal-places="2"/></number:number-style>"#,
        r#"</office:automatic-styles>"#,
        r#"<office:body><office:spreadsheet>"#,
        r#"<table:table table:name="Sheet1">"#,
        r#"<table:table-row>"#,
        r#"<table:table-cell office:value-type="string"><text:p>Name</text:p></table:table-cell>"#,
        r#"<table:table-cell office:value-type="float" office:value="42" table:style-name="ce1"><text:p>42</text:p></table:table-cell>"#,
        r#"<table:table-cell table:number-columns-repeated="2" office:value-type="string"><text:p>x</text:p></table:table-cell>"#,
        r#"</table:table-row>"#,
        r#"<table:table-row>"#,
        r#"<table:table-cell office:value-type="float" office:value="3" table:formula="of:=SUM(A1:A1)" table:number-columns-spanned="2" table:number-rows-spanned="1" table:style-name="ce2"><text:p>3</text:p></table:table-cell>"#,
        r#"<table:covered-table-cell/>"#,
        r#"</table:table-row>"#,
        r#"<table:table-row table:number-rows-repeated="2"><table:table-cell office:value-type="string"><text:p>r</text:p></table:table-cell></table:table-row>"#,
        r#"</table:table>"#,
        r#"<table:table table:name="Sheet2" table:display="false"><table:table-row>"#,
        r#"<table:table-cell office:value-type="string"><text:p>hidden-cell</text:p></table:table-cell>"#,
        r#"</table:table-row></table:table>"#,
        r#"<table:table table:name="Notes"><table:table-row>"#,
        r#"<table:table-cell office:value-type="string"><text:p>annotated</text:p>"#,
        r#"<office:annotation><dc:creator>Alice</dc:creator><dc:date>2026-01-01T00:00:00</dc:date><text:p>a note</text:p></office:annotation>"#,
        r#"</table:table-cell></table:table-row></table:table>"#,
        r#"<table:named-expressions>"#,
        r#"<table:named-range table:name="Rate" table:base-cell-address="Sheet1.A1" table:cell-range-address="Sheet1.A1:Sheet1.A1"/>"#,
        r#"<table:named-expression table:name="DoubleRate" table:base-cell-address="Sheet1.A1" table:expression="of:=Sheet1.A1*2"/>"#,
        r#"</table:named-expressions>"#,
        r#"</office:spreadsheet></office:body></office:document-content>"#,
    )
    .to_string()
}

fn bomb_content_xml() -> String {
    concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" "#,
        r#"xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" "#,
        r#"xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0"><office:body><office:spreadsheet>"#,
        r#"<table:table table:name="Bomb"><table:table-row table:number-rows-repeated="4000000">"#,
        r#"<table:table-cell office:value-type="string"><text:p>b</text:p></table:table-cell>"#,
        r#"</table:table-row></table:table>"#,
        r#"</office:spreadsheet></office:body></office:document-content>"#,
    )
    .to_string()
}

fn styles_xml() -> &'static [u8] {
    br#"<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"><office:styles><style:style style:name="ceNamed" style:family="table-cell"><style:text-properties fo:font-style="italic"/></style:style></office:styles></office:document-styles>"#
}

fn ods_entries() -> Vec<Entry> {
    vec![
        Entry::stored("mimetype", ODS_MIMETYPE.as_bytes()),
        Entry::stored("META-INF/manifest.xml", manifest_xml().as_bytes()),
        Entry::deflated("content.xml", content_xml().as_bytes()),
        Entry::stored("styles.xml", styles_xml()),
        Entry::stored(
            "meta.xml",
            br#"<office:document-meta xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"/>"#,
        ),
    ]
}

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-ods-{label}-{}-{}",
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
        format_basis: "opaque:ods-test".to_string(),
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
        Fixture::from_entries(label, &ods_entries())
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

const P: OdsExtractProfile = OdsExtractProfile::DEFAULT;

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_and_capabilities_are_byte_based() {
    let source = build_zip(&ods_entries());
    assert_eq!(
        detect_document_format(&source, Limits::DEFAULT),
        DocumentFormat::Ods
    );
    // A ZIP that is neither DOCX/EPUB/ODT/ODS/XLSX/PPTX is opaque.
    let plain = build_zip(&[Entry::stored("hello.txt", b"hi")]);
    assert_eq!(
        detect_document_format(&plain, Limits::DEFAULT),
        DocumentFormat::Opaque
    );

    let caps = capabilities_for_format(DocumentFormat::Ods);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "table", "cell", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    assert!(caps.native_selectors.contains(&"ods-sheet"));
    assert!(caps.native_selectors.contains(&"ods-cell"));
    assert!(caps.native_selectors.contains(&"ods-styles"));
    assert!(
        !caps.profiles.is_empty(),
        "ODS declares a profile fingerprint"
    );
    let json = caps.to_json();
    assert!(json.contains("\"format\":\"ods\""), "{json}");
}

#[test]
fn sheets_cells_values_and_formulas_resolve() {
    let mut fx = Fixture::standard("content");

    // Native sheet metadata.
    let (sheet, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdsSheet {
            index: 0,
            profile: P,
        },
        Representation::Metadata,
    );
    let sj = answer_json(&sheet);
    assert!(sj.contains("\"sheet\":\"Sheet1\""), "{sj}");
    assert!(sj.contains("\"rows\":4"), "{sj}");

    // Cell B1: typed value + displayed text + style, all distinct.
    let (b1, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdsCell {
            sheet: 0,
            cell: "B1".to_string(),
            profile: P,
        },
        Representation::Metadata,
    );
    let bj = answer_json(&b1);
    assert!(bj.contains("\"value_type\":\"float\""), "{bj}");
    assert!(bj.contains("\"value\":\"42\""), "{bj}");
    assert!(bj.contains("\"text\":\"42\""), "{bj}");
    assert!(bj.contains("\"style\":\"ce1\""), "{bj}");
    assert!(bj.contains("\"span_len\":"), "{bj}");

    // Cell A2: a stored formula (never evaluated) distinct from its value/text.
    let (a2, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdsCell {
            sheet: 0,
            cell: "A2".to_string(),
            profile: P,
        },
        Representation::Metadata,
    );
    let aj = answer_json(&a2);
    assert!(aj.contains("\"formula\":\"of:=SUM(A1:A1)\""), "{aj}");
    assert!(aj.contains("\"value\":\"3\""), "{aj}");
    assert!(aj.contains("\"cols_spanned\":2"), "{aj}");

    // The exact decoded-part span of a cell is a derived byte slice.
    let (raw, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdsCell {
            sheet: 0,
            cell: "A2".to_string(),
            profile: P,
        },
        Representation::ExactBytes,
    );
    match &raw.value {
        AnswerValue::Bytes(b) => {
            assert!(
                String::from_utf8_lossy(b).contains("table-cell"),
                "span slice: {:?}",
                String::from_utf8_lossy(b)
            );
        }
        other => panic!("expected bytes, got {other:?}"),
    }

    // Repeated cells: row 0 has A/B + two repeated `x` cells => C1 and D1 are `x`.
    let (c1, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdsCell {
            sheet: 0,
            cell: "C1".to_string(),
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&c1), "x");
    let (d1, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdsCell {
            sheet: 0,
            cell: "D1".to_string(),
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&d1), "x");

    // `row:col` addressing resolves the same cell.
    let (c1b, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdsCell {
            sheet: 0,
            cell: "0:2".to_string(),
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&c1b), "x");
}

#[test]
fn common_metadata_text_table_cell_and_find_resolve() {
    let mut fx = Fixture::standard("common");

    let (meta, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Metadata,
        Representation::Metadata,
    );
    let mj = answer_json(&meta);
    assert!(mj.contains("\"format\":\"ods\""), "{mj}");
    assert!(mj.contains("\"part\":\"content.xml\""), "{mj}");
    assert!(mj.contains("\"sheets\":3"), "{mj}");
    assert!(
        mj.contains("\"sheet_names\":[\"Sheet1\",\"Sheet2\",\"Notes\"]"),
        "{mj}"
    );
    assert!(mj.contains("\"named_expressions\":2"), "{mj}");
    assert!(mj.contains("\"comments\":1"), "{mj}");

    // Whole-spreadsheet text excludes the hidden sheet by default.
    let (text, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Text,
        Representation::Text,
    );
    let t = answer_text(&text);
    assert!(t.contains("Name"), "{t}");
    assert!(!t.contains("hidden-cell"), "hidden sheet leaked: {t}");

    // Common table (sheet) text.
    let (table, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Table(0),
        Representation::Text,
    );
    let tt = answer_text(&table);
    assert!(tt.starts_with("Name\t42\tx\tx"), "{tt}");

    // Common cell.
    let (cell, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Cell {
            table: 0,
            row: 0,
            col: 1,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&cell), "42");

    // Common find.
    let (found, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::SearchMatch("Name".to_string()),
        Representation::Text,
    );
    assert!(answer_json(&found).contains("Name"), "{found:?}");

    // A hidden sheet is still addressable by index.
    let mut hidden = P;
    hidden.hidden = true;
    let (hs, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdsSheet {
            index: 1,
            profile: hidden,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&hs), "hidden-cell");
}

#[test]
fn native_styles_named_expressions_and_comments_resolve() {
    let mut fx = Fixture::standard("native");

    // Cell styles: automatic (content.xml) + named (styles.xml) both report.
    let (styles, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdsStyles,
        Representation::Metadata,
    );
    let st = answer_json(&styles);
    assert!(st.contains("ce1"), "{st}");
    assert!(st.contains("ce2"), "{st}");
    assert!(st.contains("ceNamed"), "{st}");
    assert!(st.contains("N1"), "{st}");
    assert!(st.contains("table_cell_properties"), "{st}");

    // Named expressions (never evaluated).
    let (named, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdsNamedExpressions,
        Representation::Metadata,
    );
    let nj = answer_json(&named);
    assert!(nj.contains("\"name\":\"Rate\""), "{nj}");
    assert!(nj.contains("\"kind\":\"range\""), "{nj}");
    assert!(nj.contains("\"kind\":\"expression\""), "{nj}");
    assert!(nj.contains("of:=Sheet1.A1*2"), "{nj}");

    // Cell comments.
    let (comments, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdsComments { sheet: 2 },
        Representation::Metadata,
    );
    let cj = answer_json(&comments);
    assert!(cj.contains("Alice"), "{cj}");
    assert!(cj.contains("a note"), "{cj}");

    // Native find.
    let (found, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdsFind {
            pattern: "annotated".to_string(),
            profile: P,
        },
        Representation::Text,
    );
    assert!(answer_json(&found).contains("annotated"), "{found:?}");

    // Provenance records the format, part, and profile.
    let (text, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Text,
        Representation::Text,
    );
    assert!(
        text.provenance.starts_with("format=ods;common;"),
        "{}",
        text.provenance
    );
    assert!(
        text.provenance.contains("ods;part=content.xml"),
        "{}",
        text.provenance
    );
    assert!(
        text.provenance.contains("profile=v1-c1-h0"),
        "{}",
        text.provenance
    );
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

    // A native observation still leaves exactness intact.
    let (_text, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Text,
        Representation::Text,
    );
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn removal_then_fresh_process_is_queryable_and_exact() {
    let root = temp_dir("removal");
    let source = build_zip(&ods_entries());
    let descriptor = opaque_descriptor(&source);
    let src_path = root.join("doc.ods");
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
    assert!(answer_text(&text).contains("Name"), "{text:?}");

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
    let entries = vec![
        Entry::stored("mimetype", ODS_MIMETYPE.as_bytes()),
        Entry::deflated("content.xml", content_xml().as_bytes()),
    ];
    let mut fx = Fixture::from_entries("nomanifest", &entries);
    assert_eq!(fx.report.format, DocumentFormat::Ods);
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
        Entry::stored("mimetype", ODS_MIMETYPE.as_bytes()),
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
fn repeated_row_bomb_declines_typed() {
    // A `table:number-rows-repeated` bomb is bounded: observation declines typed
    // (resource limit), never allocating the declared grid.
    let entries = vec![
        Entry::stored("mimetype", ODS_MIMETYPE.as_bytes()),
        Entry::stored("META-INF/manifest.xml", manifest_xml().as_bytes()),
        Entry::deflated("content.xml", bomb_content_xml().as_bytes()),
        Entry::stored("styles.xml", styles_xml()),
        Entry::stored(
            "meta.xml",
            br#"<office:document-meta xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"/>"#,
        ),
    ];
    let mut fx = Fixture::from_entries("bomb", &entries);
    assert_eq!(fx.report.format, DocumentFormat::Ods);
    let class = observe_err(
        &mut fx.store,
        &fx.report.field,
        Selector::Text,
        Representation::Text,
    );
    assert_eq!(class, ErrorClass::ResourceLimit);
    // Exactness is untouched by the derived decline.
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn random_bytes_never_panic() {
    // Detection and the derived content parse must fail closed, never panic.
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..64 {
        let mut buf = vec![0u8; 512];
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let _ = detect_document_format(&buf, Limits::DEFAULT);
        let _ = vole_document::adapter::ods::parse_content(&buf, "content.xml", &P, Limits::STRICT);
        let _ = vole_document::adapter::ods::build_ods_model(&buf, Limits::STRICT);
    }
}
