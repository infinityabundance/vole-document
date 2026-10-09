//! Phase 21.4.1 court: the OpenDocument Presentation (ODP) adapter.
//!
//! Gated on `odp` (which implies `opc`, `package`, `field`), so a build without
//! the feature compiles an empty target.
//!
//! The fixtures are **self-authored**, generated deterministically by
//! `tools/fixtures/make-odp.py` (Python stdlib: `zipfile` + hand-written
//! OpenDocument XML, no `odfpy`, no `python-pptx`) and committed under
//! `tools/fixtures/odp/`. The court checks:
//!
//! 1. **Byte-based detection + capabilities** (`mimetype`/manifest, never a name).
//! 2. **The model**: the `draw:page` slide inventory, slide names, titles, shapes,
//!    run-level text, embedded tables, notes, master pages, and media.
//! 3. **Slide order** comes from the `draw:page` document order, never a page-name
//!    order.
//! 4. **Common** `metadata`/`text`/`table`/`cell`/`search-match` and native
//!    `odp-slide`/`odp-shape`/`odp-notes`/`odp-masters`/`odp-media`/`odp-tables`/
//!    `odp-find` resolve.
//! 5. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp).
//! 6. **Source + descriptor deleted, fresh process**: still queryable and exactly
//!    rematerializable.
//! 7. **Fail closed**: a missing/malformed manifest declines typed; random bytes
//!    never panic.
//! 8. **Interleaved multi-fixture store does not alias** (ADR-0060).

#![cfg(all(
    feature = "field",
    feature = "package",
    feature = "opc",
    feature = "odp"
))]

use std::path::PathBuf;

use vole_document::adapter::odp::{OdpExtractProfile, build_odp_model, parse_content};
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
// Fixtures (committed, generated by tools/fixtures/make-odp.py)
// ---------------------------------------------------------------------------

fn fixture_path(label: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tools/fixtures/odp")
        .join(label)
}

fn fixture_bytes(label: &str) -> Vec<u8> {
    std::fs::read(fixture_path(label)).unwrap_or_else(|e| panic!("read fixture {label}: {e}"))
}

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-odp-{label}-{}-{}",
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
        format_basis: "opaque:odp-test".to_string(),
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
    fn named(label: &str, file: &str) -> Fixture {
        Fixture::from_source(label, &fixture_bytes(file))
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

const P: OdpExtractProfile = OdpExtractProfile::DEFAULT;

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_and_capabilities_are_byte_based() {
    for name in [
        "basic.odp",
        "table.odp",
        "picture.odp",
        "notes.odp",
        "order.odp",
    ] {
        assert_eq!(
            detect_document_format(&fixture_bytes(name), Limits::DEFAULT),
            DocumentFormat::Odp,
            "{name}"
        );
    }
    // A plain ZIP is opaque, not ODP.
    let plain = build_zip(&[Entry::stored("hello.txt", b"hi")]);
    assert_eq!(
        detect_document_format(&plain, Limits::DEFAULT),
        DocumentFormat::Opaque
    );

    let caps = capabilities_for_format(DocumentFormat::Odp);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "table", "cell", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for expected in [
        "odp-slide",
        "odp-shape",
        "odp-notes",
        "odp-masters",
        "odp-media",
        "odp-tables",
        "odp-find",
    ] {
        assert!(
            caps.native_selectors.contains(&expected),
            "missing native {expected}"
        );
    }
    assert!(
        !caps.profiles.is_empty(),
        "ODP declares a profile fingerprint"
    );
    let json = caps.to_json();
    assert!(json.contains("\"format\":\"odp\""), "{json}");
}

#[test]
fn metadata_reports_slide_count_names_and_title() {
    let mut fx = Fixture::named("meta-basic", "basic.odp");
    let (meta, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Metadata,
        Representation::Metadata,
    );
    let mj = answer_json(&meta);
    assert!(mj.contains("\"format\":\"odp\""), "{mj}");
    assert!(mj.contains("\"part\":\"content.xml\""), "{mj}");
    assert!(mj.contains("\"slides\":3"), "{mj}");
    assert!(
        mj.contains("\"slide_names\":[\"Slide 1\",\"Slide 2\",\"Slide 3\"]"),
        "{mj}"
    );
    assert!(mj.contains("\"title\":\"Hello\""), "{mj}");
    assert!(mj.contains("\"masters\":1"), "{mj}");
}

#[test]
fn slide_text_shapes_and_structure() {
    let mut fx = Fixture::named("slide-basic", "basic.odp");
    let (s0, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdpSlide {
            index: 0,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(
        answer_text(&s0),
        "Hello\nWorld\nSecond paragraph",
        "slide 0 text"
    );

    // Slide metadata reports the declared name, master page, and shape counts.
    let (m0, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdpSlide {
            index: 0,
            profile: P,
        },
        Representation::Metadata,
    );
    let mj = answer_json(&m0);
    assert!(mj.contains("\"name\":\"Slide 1\""), "{mj}");
    assert!(mj.contains("\"master\":\"Default\""), "{mj}");
    assert!(mj.contains("\"shapes\":2"), "{mj}");

    // Shape 0 is a title placeholder text frame.
    let (sh, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdpShape {
            slide: 0,
            index: 0,
            profile: P,
        },
        Representation::Metadata,
    );
    let sj = answer_json(&sh);
    assert!(sj.contains("\"kind\":\"text\""), "{sj}");
    assert!(sj.contains("\"placeholder\":\"title\""), "{sj}");
    let (sht, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdpShape {
            slide: 0,
            index: 0,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&sht), "Hello");

    // Structure exposes the shape tree.
    let (st, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdpSlide {
            index: 0,
            profile: P,
        },
        Representation::Structure,
    );
    assert!(answer_json(&st).contains("\"shapes\":["), "{st:?}");

    // A slide index past the end declines typed.
    let class = observe_err(
        &mut fx.store,
        &fx.report.field,
        Selector::OdpSlide {
            index: 9,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(class, ErrorClass::UnsupportedFeature);
}

#[test]
fn common_metadata_text_table_cell_and_find() {
    let mut fx = Fixture::named("common-basic", "basic.odp");
    let (text, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Text,
        Representation::Text,
    );
    assert_eq!(
        answer_text(&text),
        "Hello\nWorld\nSecond paragraph\nSecond\nBody text\nThird"
    );

    // Common find / search-match.
    let (found, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::SearchMatch("parag".to_string()),
        Representation::Text,
    );
    assert!(answer_json(&found).contains("parag"), "{found:?}");

    // The table deck: the common table (projected across slides) and a cell.
    let mut t = Fixture::named("common-table", "table.odp");
    let (tbl, _) = observe_eq(
        &mut t.store,
        &t.report.field,
        Selector::Table(0),
        Representation::Text,
    );
    assert_eq!(answer_text(&tbl), "a\tb\nc\td");
    let (cell, _) = observe_eq(
        &mut t.store,
        &t.report.field,
        Selector::Cell {
            table: 0,
            row: 1,
            col: 0,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&cell), "c");
    let (cmeta, _) = observe_eq(
        &mut t.store,
        &t.report.field,
        Selector::Cell {
            table: 0,
            row: 0,
            col: 1,
        },
        Representation::Metadata,
    );
    assert!(answer_json(&cmeta).contains("\"text_len\":1"), "{cmeta:?}");

    // Provenance records the format, part, and profile.
    let (text2, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Text,
        Representation::Text,
    );
    assert!(
        text2.provenance.starts_with("format=odp;common;"),
        "{}",
        text2.provenance
    );
    assert!(
        text2.provenance.contains("odp;part=content.xml"),
        "{}",
        text2.provenance
    );
    assert!(!text2.exact, "derived observations are never exact");
}

#[test]
fn native_masters_media_tables_and_find() {
    // Masters come from the styles part.
    let mut fx = Fixture::named("native-basic", "basic.odp");
    let (m, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdpMasters,
        Representation::Metadata,
    );
    assert!(answer_json(&m).contains("Default"), "{m:?}");

    // Tables + native find.
    let mut t = Fixture::named("native-table", "table.odp");
    let (tb, _) = observe_eq(
        &mut t.store,
        &t.report.field,
        Selector::OdpTables {
            slide: 0,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&tb), "a\tb\nc\td");
    let (nf, _) = observe_eq(
        &mut t.store,
        &t.report.field,
        Selector::OdpFind {
            pattern: "b".to_string(),
            profile: P,
        },
        Representation::Text,
    );
    assert!(answer_json(&nf).contains("\"text\""), "{nf:?}");

    // The picture deck: a media resource by ordinal, metadata and bytes.
    let mut p = Fixture::named("native-picture", "picture.odp");
    let (md, _) = observe_eq(
        &mut p.store,
        &p.report.field,
        Selector::OdpMedia { ordinal: 0 },
        Representation::Metadata,
    );
    let jm = answer_json(&md);
    assert!(jm.contains("Pictures/image1.png"), "{jm}");
    assert!(jm.contains("\"mediaType\":\"image/png\""), "{jm}");
    let (mb, _) = observe_eq(
        &mut p.store,
        &p.report.field,
        Selector::OdpMedia { ordinal: 0 },
        Representation::DecodedBytes,
    );
    let bytes = answer_bytes(&mb);
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "not a PNG");
    // A media ordinal past the end declines typed.
    let class = observe_err(
        &mut p.store,
        &p.report.field,
        Selector::OdpMedia { ordinal: 9 },
        Representation::Metadata,
    );
    assert_eq!(class, ErrorClass::UnsupportedFeature);

    // The picture shape references the media href.
    let (sh, _) = observe_eq(
        &mut p.store,
        &p.report.field,
        Selector::OdpShape {
            slide: 0,
            index: 1,
            profile: P,
        },
        Representation::Metadata,
    );
    let js = answer_json(&sh);
    assert!(js.contains("\"kind\":\"picture\""), "{js}");
    assert!(js.contains("\"media\":\"Pictures/image1.png\""), "{js}");
}

#[test]
fn slide_order_follows_document_not_page_names() {
    // `order.odp` names the FIRST page "Slide 2" and the SECOND page "Slide 1", so
    // document order yields "SECOND FILE" then "FIRST FILE" — never the
    // page-name/name-sorted order.
    let mut fx = Fixture::named("order", "order.odp");
    let (s0, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdpSlide {
            index: 0,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&s0), "SECOND FILE");
    let (s1, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdpSlide {
            index: 1,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&s1), "FIRST FILE");
    let (text, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Text,
        Representation::Text,
    );
    assert_eq!(answer_text(&text), "SECOND FILE\nFIRST FILE");
    // The declared page names are recorded verbatim (Slide 2, then Slide 1).
    let (meta, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Metadata,
        Representation::Metadata,
    );
    assert!(
        answer_json(&meta).contains("\"slide_names\":[\"Slide 2\",\"Slide 1\"]"),
        "{:?}",
        answer_json(&meta)
    );
}

#[test]
fn notes_selector_and_profile() {
    let mut fx = Fixture::named("notes", "notes.odp");
    let (n, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::OdpNotes {
            index: 0,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&n), "Speaker notes here");
    // The default common text excludes notes: notes are a distinct observation.
    let (text, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Text,
        Representation::Text,
    );
    assert_eq!(answer_text(&text), "Body text");
    // A profile that includes notes is a distinct, fingerprinted identity.
    let mut with_notes = P;
    with_notes.include_notes = true;
    assert!(with_notes.fingerprint().contains("n1"));
    assert_ne!(with_notes.encode(), P.encode());
    // A slide with no notes page declines typed.
    let class = observe_err(
        &mut fx.store,
        &fx.report.field,
        Selector::OdpNotes {
            index: 5,
            profile: P,
        },
        Representation::Metadata,
    );
    assert_eq!(class, ErrorClass::UnsupportedFeature);
}

#[test]
fn exact_materialization_is_byte_identical() {
    for name in [
        "basic.odp",
        "table.odp",
        "picture.odp",
        "notes.odp",
        "order.odp",
    ] {
        let mut fx = Fixture::named(&format!("exact-{name}"), name);
        let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
        let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
        assert_eq!(exact.len(), fx.source.len(), "{name}");
        assert_eq!(sha256(&exact), sha256(&fx.source), "{name}");
        assert_eq!(exact, fx.source, "{name}");
        drop(field);
        // A native observation still leaves exactness intact.
        let (_t, _) = observe_eq(
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
    let source = fixture_bytes("basic.odp");
    let descriptor = opaque_descriptor(&source);
    let src_path = root.join("doc.odp");
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

    std::fs::remove_file(&src_path).unwrap();
    std::fs::remove_file(&desc_path).unwrap();

    let mut store2 = FieldStore::open(&store_root).unwrap();
    let (s0, _) = observe_eq(
        &mut store2,
        &field_id,
        Selector::OdpSlide {
            index: 0,
            profile: P,
        },
        Representation::Text,
    );
    assert!(answer_text(&s0).contains("Hello"), "{s0:?}");

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
    let mimetype = b"application/vnd.oasis.opendocument.presentation";
    let content = br#"<office:document-content xmlns:office="o" xmlns:draw="d" xmlns:text="t"><office:body><office:presentation><draw:page draw:name="S"><draw:frame><draw:text-box><text:p>x</text:p></draw:text-box></draw:frame></draw:page></office:presentation></office:body></office:document-content>"#;
    let entries = vec![
        Entry::stored("mimetype", mimetype),
        Entry::deflated("content.xml", content),
    ];
    let mut fx = Fixture::from_entries("nomanifest", &entries);
    assert_eq!(fx.report.format, DocumentFormat::Odp);
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
        Entry::stored(
            "mimetype",
            b"application/vnd.oasis.opendocument.presentation",
        ),
        Entry::stored(
            "META-INF/manifest.xml",
            b"<manifest:manifest></notmanifest>",
        ),
        Entry::deflated(
            "content.xml",
            br#"<office:document-content xmlns:office="o"><office:body><office:presentation/></office:body></office:document-content>"#,
        ),
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
fn unsupported_common_pairs_decline_typed() {
    let mut fx = Fixture::named("unsupported", "basic.odp");
    for selector in [Selector::Heading(0), Selector::Block(0), Selector::Link(0)] {
        let class = observe_err(
            &mut fx.store,
            &fx.report.field,
            selector,
            Representation::Text,
        );
        assert_eq!(class, ErrorClass::UnsupportedFeature);
    }
}

#[test]
fn interleaved_odp_fields_do_not_alias_in_a_shared_store() {
    let root = temp_dir("interleaved");
    let store_root = root.join("store");
    let basic = fixture_bytes("basic.odp");
    let order = fixture_bytes("order.odp");

    let f_order = {
        let mut store = FieldStore::open(&store_root).unwrap();
        let fb = ingest_package(&mut store, &opaque_descriptor(&basic), Limits::DEFAULT)
            .unwrap()
            .field;
        // Observe basic FIRST: this populates the shared derived cache, the
        // interleaving that a source-independent node identity would alias.
        let (t, _) = observe_eq(&mut store, &fb, Selector::Text, Representation::Text);
        assert!(answer_text(&t).starts_with("Hello"));
        ingest_package(&mut store, &opaque_descriptor(&order), Limits::DEFAULT)
            .unwrap()
            .field
    };

    let mut store = FieldStore::open(&store_root).unwrap();
    let (t, _) = observe_eq(&mut store, &f_order, Selector::Text, Representation::Text);
    assert_eq!(answer_text(&t), "SECOND FILE\nFIRST FILE");
    let (s0, _) = observe_eq(
        &mut store,
        &f_order,
        Selector::OdpSlide {
            index: 0,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&s0), "SECOND FILE");
    drop(store);

    // The field id is a function of the source, not of the store's other fields.
    let alone_root = root.join("alone");
    let f_order_alone = {
        let mut s = FieldStore::open(&alone_root).unwrap();
        ingest_package(&mut s, &opaque_descriptor(&order), Limits::DEFAULT)
            .unwrap()
            .field
    };
    assert_eq!(
        f_order, f_order_alone,
        "a field id must not depend on the store's other ODP fields"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn random_bytes_never_panic() {
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
        let _ = parse_content(&buf, "content.xml", &P, Limits::STRICT);
        let _ = build_odp_model(&buf, Limits::STRICT);
    }
}

#[test]
fn adapter_unit_roundtrips_are_stable() {
    // The canonical content model round-trips through its codec.
    let bytes = fixture_bytes("basic.odp");
    let model = build_odp_model(&bytes, Limits::DEFAULT).unwrap();
    let decoded = vole_document::adapter::odp::OdpModel::decode(&model).unwrap();
    assert_eq!(decoded.encode(), model);
}
