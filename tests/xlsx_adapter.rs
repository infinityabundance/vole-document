//! Phase 21.1.1 court: the SpreadsheetML (XLSX) adapter.
//!
//! Gated on `xlsx` (which implies `opc`, `package`, `field`), so a build without
//! the feature compiles an empty target.
//!
//! The fixtures are **self-authored**, generated deterministically by
//! `tools/fixtures/make-xlsx.py` (Python stdlib: `zipfile` + hand-written XML, no
//! `openpyxl`) and committed under `tools/fixtures/xlsx/`. The court checks:
//!
//! 1. **Byte-based detection + capabilities** (content types, never a name).
//! 2. **The model**: workbook/sheet inventory (names, order, visibility), shared
//!    strings, inline strings, numbers/booleans/errors, a stored formula with a
//!    cached value, a minimal style, and a merged range.
//! 3. **Common `metadata`/`text`/`table`/`cell`** and native `sheet`/`cell`/`find`.
//! 4. **The crucial distinctions**: a cell's stored formula, its cached result,
//!    and its XML span are distinct facets, never conflated.
//! 5. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp).
//! 6. **Source + descriptor deleted, fresh process**: still queryable and exactly
//!    rematerializable.
//! 7. **Fail closed**: missing/ambiguous relationship and malformed XML decline
//!    typed, with exactness preserved.
//! 8. **Random bytes never panic** and the cover holds.

#![cfg(all(
    feature = "field",
    feature = "package",
    feature = "opc",
    feature = "xlsx"
))]

use std::path::PathBuf;

use vole_document::adapter::xlsx::{
    SheetModel, XlsxExtractProfile, XlsxModel, a1_to_col_row, build_xlsx_model, col_row_to_a1,
    parse_comments, parse_drawing, parse_shared_strings, parse_styles_table, parse_table,
    parse_vml_notes, parse_workbook, parse_worksheet,
};
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
// Dependency-free ZIP writer (test ground truth for hostile fixtures).
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

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tools/fixtures/xlsx")
        .join(name)
}

fn fixture_bytes(name: &str) -> Vec<u8> {
    std::fs::read(fixture_path(name)).unwrap_or_else(|e| panic!("read fixture {name}: {e}"))
}

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-xlsx-{label}-{}-{}",
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
        format_basis: "opaque:xlsx-test".to_string(),
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
    fn from_source(label: &str, source: &[u8]) -> Fixture {
        let root = temp_dir(label);
        let mut store = FieldStore::open(&root).unwrap();
        let descriptor = opaque_descriptor(source);
        let report = ingest_package(&mut store, &descriptor, Limits::DEFAULT).unwrap();
        Fixture {
            root,
            store,
            report,
            source: source.to_vec(),
        }
    }
    fn named(name: &str, label: &str) -> Fixture {
        Fixture::from_source(label, &fixture_bytes(name))
    }
    fn from_entries(label: &str, entries: &[Entry]) -> Fixture {
        Fixture::from_source(label, &build_zip(entries))
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

fn answer_bytes(answer: &FieldAnswer) -> Vec<u8> {
    match &answer.value {
        AnswerValue::Bytes(b) => b.clone(),
        other => panic!("expected bytes, got {other:?}"),
    }
}

const P: XlsxExtractProfile = XlsxExtractProfile::DEFAULT;

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_and_capabilities_are_byte_based() {
    assert_eq!(
        detect_document_format(&fixture_bytes("single.xlsx"), Limits::DEFAULT),
        DocumentFormat::Xlsx
    );
    assert_eq!(
        detect_document_format(&fixture_bytes("multi.xlsx"), Limits::DEFAULT),
        DocumentFormat::Xlsx
    );
    // A plain ZIP is opaque, not XLSX.
    let plain = build_zip(&[Entry::stored("hello.txt", b"hi")]);
    assert_eq!(
        detect_document_format(&plain, Limits::DEFAULT),
        DocumentFormat::Opaque
    );

    let caps = capabilities_for_format(DocumentFormat::Xlsx);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "table", "cell", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    assert!(caps.native_selectors.contains(&"xlsx-sheet"));
    assert!(caps.native_selectors.contains(&"xlsx-cell"));
    assert!(caps.profiles.iter().any(|p| p.contains("cached")));
    let json = caps.to_json();
    assert!(json.contains("\"format\":\"xlsx\""), "{json}");
}

#[test]
fn metadata_names_sheets_and_visibility() {
    let mut fx = Fixture::named("multi.xlsx", "meta-multi");
    let (meta, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Metadata,
        Representation::Metadata,
    );
    let j = answer_json(&meta);
    assert!(j.contains("\"format\":\"xlsx\""), "{j}");
    assert!(j.contains("\"sheets\":3"), "{j}");
    assert!(j.contains("\"First\""), "{j}");
    assert!(j.contains("\"Second\""), "{j}");
    assert!(j.contains("\"Hidden\""), "{j}");
    assert!(j.contains("\"hidden\""), "{j}");
    assert!(j.contains("/xl/workbook.xml"), "{j}");
}

#[test]
fn shared_inline_numbers_booleans_errors_formula_style_and_merge() {
    let mut fx = Fixture::named("single.xlsx", "content");

    // Native sheet text (cached results).
    let (sheet, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxSheet {
            index: 0,
            profile: P,
        },
        Representation::Text,
    );
    let t = answer_text(&sheet);
    assert_eq!(t, "Inline\n42\t1\t#DIV/0!\nHello\t42\tWorld", "{t:?}");

    // Shared string resolves.
    let (a3, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxCell {
            sheet: 0,
            cell: "A3".into(),
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&a3), "Hello");

    // Inline string resolves.
    let (a1, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxCell {
            sheet: 0,
            cell: "A1".into(),
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&a1), "Inline");

    // The stored formula and the cached result are distinct facets.
    let (b3v, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxCell {
            sheet: 0,
            cell: "B3".into(),
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&b3v), "42");
    let (b3m, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxCell {
            sheet: 0,
            cell: "B3".into(),
            profile: P,
        },
        Representation::Structure,
    );
    let bm = answer_json(&b3m);
    assert!(bm.contains("\"formula\":\"SUM(A2:A2)\""), "{bm}");
    assert!(bm.contains("\"value\":\"42\""), "{bm}");

    // Formula-mode profile projects the stored formula as the cell text.
    let mut formula = P;
    formula.values = vole_document::adapter::xlsx::ValueMode::Formula;
    let (b3f, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxCell {
            sheet: 0,
            cell: "B3".into(),
            profile: formula,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&b3f), "SUM(A2:A2)");

    // A minimal style resolves (numFmtId 164 on C3).
    let (c3m, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxCell {
            sheet: 0,
            cell: "C3".into(),
            profile: P,
        },
        Representation::Metadata,
    );
    let cm = answer_json(&c3m);
    assert!(cm.contains("\"numFmtId\":164"), "{cm}");

    // The exact cell bytes are the decoded `<c>` span.
    let (c3x, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxCell {
            sheet: 0,
            cell: "A2".into(),
            profile: P,
        },
        Representation::ExactBytes,
    );
    let xb = answer_bytes(&c3x);
    assert!(!xb.is_empty(), "empty exact span");
    assert!(
        String::from_utf8_lossy(&xb).contains("A2"),
        "{}",
        String::from_utf8_lossy(&xb)
    );

    // Sheet metadata reports the merged range and dimension.
    let (sm, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxSheet {
            index: 0,
            profile: P,
        },
        Representation::Metadata,
    );
    let smj = answer_json(&sm);
    assert!(smj.contains("\"merges\":1"), "{smj}");
    assert!(smj.contains("A1:C3"), "{smj}");
}

#[test]
fn common_text_table_cell_and_search_match_resolve() {
    let mut fx = Fixture::named("multi.xlsx", "common-multi");

    // Whole-workbook text excludes the hidden sheet by default.
    let (text, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Text,
        Representation::Text,
    );
    let t = answer_text(&text);
    assert_eq!(t, "Alpha\nBeta", "{t:?}");
    assert!(
        text.provenance.starts_with("format=xlsx;common;"),
        "{}",
        text.provenance
    );

    // Common table 0 / 1 are the visible sheets.
    let (t0, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Table(0),
        Representation::Text,
    );
    assert_eq!(answer_text(&t0), "Alpha");
    let (t1, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Table(1),
        Representation::Text,
    );
    assert_eq!(answer_text(&t1), "Beta");
    // There is no visible sheet 2.
    let class = observe_err(
        &mut fx.store,
        &fx.report.field,
        Selector::Table(2),
        Representation::Text,
    );
    assert_eq!(class, ErrorClass::UnsupportedFeature);

    // Common cell (table 1 -> Second, row 0 col 0 -> A1).
    let (cell, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Cell {
            table: 1,
            row: 0,
            col: 0,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&cell), "Beta");

    // Search-match over cell values.
    let (found, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::SearchMatch("Alp".into()),
        Representation::Text,
    );
    assert!(answer_json(&found).contains("Alpha"), "{found:?}");

    // A hidden sheet is still addressable by native index.
    let (hidden, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxSheet {
            index: 2,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&hidden), "Gamma");
}

#[test]
fn unsupported_common_pairs_decline_typed() {
    let mut fx = Fixture::named("single.xlsx", "declines");
    // XLSX does not map heading/block/resource/link cleanly: typed declines.
    for selector in [
        Selector::Heading(0),
        Selector::Block(0),
        Selector::Resource(0),
        Selector::Link(0),
    ] {
        let class = observe_err(
            &mut fx.store,
            &fx.report.field,
            selector,
            Representation::Text,
        );
        assert_eq!(class, ErrorClass::UnsupportedFeature);
    }
    // A cell outside the sheet declines.
    let class = observe_err(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxCell {
            sheet: 0,
            cell: "Z99".into(),
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(class, ErrorClass::UnsupportedFeature);
    // A malformed A1 reference is a usage error.
    let class = observe_err(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxCell {
            sheet: 0,
            cell: "!!".into(),
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(class, ErrorClass::Usage);
}

#[test]
fn exact_materialization_is_byte_identical() {
    for name in ["single.xlsx", "multi.xlsx"] {
        let mut fx = Fixture::named(name, "exact");
        let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
        let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
        assert_eq!(exact.len(), fx.source.len(), "{name}");
        assert_eq!(sha256(&exact), sha256(&fx.source), "{name}");
        assert_eq!(exact, fx.source, "{name}");
        drop(field);

        // An observation must not disturb exactness.
        let (_, _) = observe_eq(
            &mut fx.store,
            &fx.report.field,
            Selector::Text,
            Representation::Text,
        );
        let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
        assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
    }
}

#[test]
fn removal_then_fresh_process_is_queryable_and_exact() {
    let root = temp_dir("removal");
    let source = fixture_bytes("single.xlsx");
    let descriptor = opaque_descriptor(&source);
    let src_path = root.join("doc.xlsx");
    let desc_path = root.join("doc.voldoc");
    std::fs::write(&src_path, &source).unwrap();
    std::fs::write(&desc_path, &descriptor).unwrap();

    let store_root = root.join("store");
    let field_id = {
        let mut store = FieldStore::open(&store_root).unwrap();
        ingest_package(&mut store, &descriptor, Limits::DEFAULT)
            .unwrap()
            .field
        // store drops here: a fresh process reopens the directory.
    };

    std::fs::remove_file(&src_path).unwrap();
    std::fs::remove_file(&desc_path).unwrap();

    let mut store2 = FieldStore::open(&store_root).unwrap();
    let (text, _) = observe_eq(
        &mut store2,
        &field_id,
        Selector::XlsxCell {
            sheet: 0,
            cell: "B3".into(),
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&text), "42");

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), source.len());
    assert_eq!(sha256(&exact), sha256(&source));
    assert!(exact == source);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn missing_relationship_declines_typed_and_keeps_exactness() {
    // Content types declare the workbook main type, but no `_rels/.rels` names it.
    let ct = concat!(
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
        r#"<Default Extension="xml" ContentType="application/xml"/>"#,
        r#"<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>"#,
        r#"</Types>"#
    );
    let wb = r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheets/></workbook>"#;
    let entries = vec![
        Entry::stored("[Content_Types].xml", ct.as_bytes()),
        Entry::deflated("xl/workbook.xml", wb.as_bytes()),
    ];
    let mut fx = Fixture::from_entries("no-rel", &entries);
    // Detection still sees the SpreadsheetML content type, but the derived model
    // declines typed because the workbook is unresolvable.
    assert_eq!(fx.report.format, DocumentFormat::Xlsx);
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
fn malformed_workbook_and_worksheet_decline_typed() {
    // A valid package whose workbook.xml has the wrong root element.
    let ct = concat!(
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
        r#"<Default Extension="xml" ContentType="application/xml"/>"#,
        r#"<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>"#,
        r#"</Types>"#
    );
    let rels = concat!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
        r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>"#,
        r#"</Relationships>"#
    );
    let entries = vec![
        Entry::stored("[Content_Types].xml", ct.as_bytes()),
        Entry::stored("_rels/.rels", rels.as_bytes()),
        Entry::stored("xl/workbook.xml", b"<notworkbook/>"),
    ];
    let mut fx = Fixture::from_entries("bad-root", &entries);
    let class = observe_err(
        &mut fx.store,
        &fx.report.field,
        Selector::Metadata,
        Representation::Metadata,
    );
    assert_eq!(class, ErrorClass::InvalidPackageStructure);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn random_bytes_never_panic_and_cover_holds() {
    let mut state: u64 = 0x2101_1DE5_u64.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for round in 0..200 {
        let n = (next() % 512) as usize;
        let mut buf = Vec::with_capacity(n);
        for _ in 0..n {
            buf.push((next() & 0xFF) as u8);
        }
        // Detection / capabilities must never panic.
        let _ = detect_document_format(&buf, Limits::STRICT);
        let _ = capabilities_for_format(DocumentFormat::Xlsx);
        // The adapter parsers must decline typed, never panic.
        assert!(
            build_xlsx_model(&buf, Limits::STRICT).is_err(),
            "round {round}"
        );
        let _ = parse_workbook(&buf, Limits::STRICT);
        let _ = parse_worksheet(&buf, "/x", "s", None, Limits::STRICT);
        let _ = parse_shared_strings(&buf, Limits::STRICT);
        let _ = parse_styles_table(&buf, Limits::STRICT);
        let _ = parse_comments(&buf, Limits::STRICT);
        let _ = parse_vml_notes(&buf, Limits::STRICT);
        let _ = parse_table(&buf, Limits::STRICT);
        let _ = parse_drawing(&buf, Limits::STRICT);
    }
    // A benign real fixture at STRICT limits still parses and covers exactly.
    let source = fixture_bytes("single.xlsx");
    let physical = vole_document::adapter::package::scan(&source, Limits::DEFAULT).unwrap();
    physical.validate(source.len() as u64).unwrap();
    physical.reemits(&source).unwrap();
}

#[test]
fn semantic_style_table_fonts_fills_alignments_and_number_formats() {
    let mut fx = Fixture::named("semantic.xlsx", "sem-styles");
    let (styles, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxStyles,
        Representation::Metadata,
    );
    let j = answer_json(&styles);
    assert!(j.contains("\"fonts\":3"), "{j}");
    assert!(j.contains("\"fills\":2"), "{j}");
    assert!(j.contains("\"num_fmts\":2"), "{j}");
    assert!(j.contains("\"cell_xfs\":4"), "{j}");
    assert!(j.contains("\"formatCode\":\"0.00\""), "{j}");
    assert!(j.contains("\"formatCode\":\"0%\""), "{j}");
    assert!(j.contains("\"bold\":true"), "{j}");
    assert!(j.contains("\"italic\":true"), "{j}");
    assert!(j.contains("\"name\":\"Arial\""), "{j}");
    assert!(j.contains("\"patternType\":\"solid\""), "{j}");
    assert!(j.contains("\"fgColor\":\"FFFF0000\""), "{j}");
    assert!(j.contains("\"horizontal\":\"center\""), "{j}");
    assert!(j.contains("\"wrapText\":true"), "{j}");
}

#[test]
fn semantic_cell_keeps_value_formula_displayed_and_style_distinct() {
    let mut fx = Fixture::named("semantic.xlsx", "sem-cell");
    // B2: a formula whose cached value (2469) differs from its number-format
    // rendering (2469.00); the comment is keyed to the same cell.
    let (b2, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxCell {
            sheet: 0,
            cell: "B2".into(),
            profile: P,
        },
        Representation::Metadata,
    );
    let j = answer_json(&b2);
    assert!(j.contains("\"formula\":\"A1*2\""), "{j}");
    assert!(j.contains("\"value\":\"2469\""), "{j}");
    assert!(j.contains("\"displayed\":\"2469.00\""), "{j}");
    assert!(j.contains("\"display_basis\":\"numFmt:0.00\""), "{j}");
    assert!(j.contains("\"comment\":{\"author\":\"Alice\""), "{j}");
    assert!(j.contains("Check this"), "{j}");
    assert!(j.contains("\"bold\":true"), "{j}");
    assert!(j.contains("\"name\":\"Arial\""), "{j}");

    // B1: a percentage format makes the displayed value differ from the cached one.
    let (b1, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxCell {
            sheet: 0,
            cell: "B1".into(),
            profile: P,
        },
        Representation::Metadata,
    );
    let j1 = answer_json(&b1);
    assert!(j1.contains("\"value\":\"0.5\""), "{j1}");
    assert!(j1.contains("\"displayed\":\"50%\""), "{j1}");

    // C1: alignment + italic (font 2 is explicitly not bold) + solid fill.
    let (c1, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxCell {
            sheet: 0,
            cell: "C1".into(),
            profile: P,
        },
        Representation::Structure,
    );
    let jc = answer_json(&c1);
    assert!(jc.contains("\"italic\":true"), "{jc}");
    assert!(jc.contains("\"bold\":false"), "{jc}");
    assert!(jc.contains("\"name\":\"Courier New\""), "{jc}");
    assert!(jc.contains("\"patternType\":\"solid\""), "{jc}");
    assert!(jc.contains("\"horizontal\":\"center\""), "{jc}");
}

#[test]
fn semantic_defined_names_comments_hyperlinks_tables_and_drawing() {
    let mut fx = Fixture::named("semantic.xlsx", "sem-parts");

    let (dn, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxDefinedNames,
        Representation::Metadata,
    );
    let jd = answer_json(&dn);
    assert!(jd.contains("\"count\":2"), "{jd}");
    assert!(jd.contains("\"name\":\"TaxRate\""), "{jd}");
    assert!(jd.contains("\"refersTo\":\"Data!$C$1\""), "{jd}");
    assert!(jd.contains("\"HiddenName\""), "{jd}");
    assert!(jd.contains("\"hidden\":true"), "{jd}");

    let (cm, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxComments { sheet: 0 },
        Representation::Metadata,
    );
    let jc = answer_json(&cm);
    assert!(jc.contains("\"cell\":\"B2\""), "{jc}");
    assert!(jc.contains("\"author\":\"Alice\""), "{jc}");
    assert!(jc.contains("\"text\":\"Check this\""), "{jc}");
    assert!(jc.contains("_x0000_s1025"), "{jc}");
    assert!(jc.contains("\"vml_notes\":[{\"cell\":\"B2\""), "{jc}");

    let (hl, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxHyperlinks { sheet: 0 },
        Representation::Metadata,
    );
    let jh = answer_json(&hl);
    assert!(jh.contains("\"count\":2"), "{jh}");
    assert!(jh.contains("\"ref\":\"A1\""), "{jh}");
    assert!(jh.contains("\"external\":true"), "{jh}");
    assert!(jh.contains("\"target\":\"https://example.com/\""), "{jh}");
    assert!(jh.contains("\"location\":\"Sheet1!C1\""), "{jh}");

    let (tb, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxTables { sheet: 0 },
        Representation::Metadata,
    );
    let jt = answer_json(&tb);
    assert!(jt.contains("\"name\":\"Table1\""), "{jt}");
    assert!(jt.contains("\"ref\":\"A1:C3\""), "{jt}");
    assert!(jt.contains("\"name\":\"One\""), "{jt}");
    assert!(jt.contains("\"name\":\"Three\""), "{jt}");

    let (dr, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxDrawing { sheet: 0 },
        Representation::Metadata,
    );
    let jdr = answer_json(&dr);
    assert!(jdr.contains("\"anchors\":2"), "{jdr}");
    assert!(jdr.contains("/xl/charts/chart1.xml"), "{jdr}");
    assert!(jdr.contains("/xl/media/image1.png"), "{jdr}");

    // The drawing part's exact/decoded bytes resolve.
    let (db, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxDrawing { sheet: 0 },
        Representation::DecodedBytes,
    );
    let bytes = answer_bytes(&db);
    assert!(
        String::from_utf8_lossy(&bytes).contains("wsDr"),
        "drawing bytes not resolved"
    );

    let (ex, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxExternalRels,
        Representation::Metadata,
    );
    let je = answer_json(&ex);
    assert!(je.contains("\"target_mode\":\"external\""), "{je}");
    assert!(je.contains("https://example.com/"), "{je}");
    assert!(je.contains("file:///C:/tmp/other.xlsx"), "{je}");

    // Sheet metadata reports the new counts without conflating them.
    let (sm, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxSheet {
            index: 0,
            profile: P,
        },
        Representation::Metadata,
    );
    let js = answer_json(&sm);
    assert!(js.contains("\"hyperlinks\":2"), "{js}");
    assert!(js.contains("\"tables\":1"), "{js}");
    assert!(js.contains("\"drawing\":true"), "{js}");
    assert!(js.contains("\"legacy_drawing\":true"), "{js}");
    assert!(js.contains("\"defined_names\":2"), "{js}");
    assert!(js.contains("\"merges\":1"), "{js}");
    assert!(js.contains("Data"), "{js}");
}

#[test]
fn semantic_unsupported_pairs_and_missing_parts_decline_typed() {
    let mut fx = Fixture::named("semantic.xlsx", "sem-decline");
    // A per-sheet selector on a sheet that does not exist declines typed.
    let class = observe_err(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxComments { sheet: 9 },
        Representation::Metadata,
    );
    assert_eq!(class, ErrorClass::UnsupportedFeature);
    // A representation the styles observation does not serve declines typed.
    let class = observe_err(
        &mut fx.store,
        &fx.report.field,
        Selector::XlsxStyles,
        Representation::ExactBytes,
    );
    assert_eq!(class, ErrorClass::UnsupportedFeature);
    // `single.xlsx` carries no drawing: asking for its drawing bytes declines typed.
    let mut s = Fixture::named("single.xlsx", "sem-nodraw");
    let class = observe_err(
        &mut s.store,
        &s.report.field,
        Selector::XlsxDrawing { sheet: 0 },
        Representation::DecodedBytes,
    );
    assert_eq!(class, ErrorClass::UnsupportedFeature);
}

#[test]
fn semantic_fixture_is_exact_and_queryable_after_removal() {
    let root = temp_dir("sem-removal");
    let source = fixture_bytes("semantic.xlsx");
    let descriptor = opaque_descriptor(&source);
    let src_path = root.join("doc.xlsx");
    let desc_path = root.join("doc.voldoc");
    std::fs::write(&src_path, &source).unwrap();
    std::fs::write(&desc_path, &descriptor).unwrap();

    let store_root = root.join("store");
    let field_id = {
        let mut store = FieldStore::open(&store_root).unwrap();
        ingest_package(&mut store, &descriptor, Limits::DEFAULT)
            .unwrap()
            .field
    };

    std::fs::remove_file(&src_path).unwrap();
    std::fs::remove_file(&desc_path).unwrap();

    let mut store2 = FieldStore::open(&store_root).unwrap();
    // Fully queryable in a fresh process.
    let (dn, _) = observe_eq(
        &mut store2,
        &field_id,
        Selector::XlsxDefinedNames,
        Representation::Metadata,
    );
    assert!(answer_json(&dn).contains("TaxRate"));
    let (hl, _) = observe_eq(
        &mut store2,
        &field_id,
        Selector::XlsxHyperlinks { sheet: 0 },
        Representation::Metadata,
    );
    assert!(answer_json(&hl).contains("https://example.com/"));

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), source.len());
    assert_eq!(sha256(&exact), sha256(&source));
    assert!(exact == source);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn adapter_unit_roundtrips_are_stable() {
    assert_eq!(a1_to_col_row("A1"), Some((0, 0)));
    assert_eq!(col_row_to_a1(0, 0), "A1");
    let wb = parse_workbook(
        br#"<workbook xmlns:r="x"><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
        Limits::DEFAULT,
    )
    .unwrap();
    assert_eq!(wb.sheets[0].rel_id.as_deref(), Some("rId1"));
    let sheet = parse_worksheet(
        br#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData></worksheet>"#,
        "/x",
        "S",
        None,
        Limits::DEFAULT,
    )
    .unwrap();
    assert_eq!(sheet.cell_at(0, 0).unwrap().value.as_deref(), Some("1"));
    let _ = SheetModel::decode(&sheet.encode()).unwrap();
    let _ = XlsxModel::decode(
        &XlsxModel {
            workbook: vole_document::adapter::xlsx::XlsxPartRef {
                name: "/xl/workbook.xml".into(),
                ordinal: 0,
                content_type: None,
            },
            styles: None,
            shared_strings: None,
            sheets: vec![],
        }
        .encode(),
    )
    .unwrap();
}
