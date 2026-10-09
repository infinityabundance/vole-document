//! Phase 21.2.1 court: the PresentationML (PPTX) adapter.
//!
//! Gated on `pptx` (which implies `opc`, `package`, `field`), so a build without
//! the feature compiles an empty target.
//!
//! The fixtures are **self-authored**, generated deterministically by
//! `tools/fixtures/make-pptx.py` (Python stdlib: `zipfile` + hand-written XML, no
//! `python-pptx`) and committed under `tools/fixtures/pptx/`. The court checks:
//!
//! 1. **Byte-based detection + capabilities** (content types, never a name).
//! 2. **The model**: slide inventory, slide size, text shapes and run-level text.
//! 3. **Slide order** comes from `p:sldIdLst`, never from `slideN.xml` file names.
//! 4. **Common `metadata`/`text`/`table`/`cell`** and native slide/shape/find.
//! 5. **Embedded tables**, a picture in `ppt/media/`, and a notes slide.
//! 6. **Exactness**: `materialize(field) == original_bytes` (len + SHA-256 + cmp).
//! 7. **Source + descriptor deleted, fresh process**: still queryable and exactly
//!    rematerializable.
//! 8. **Fail closed**: missing/ambiguous relationship and malformed XML decline
//!    typed, with exactness preserved.
//! 9. **Random bytes never panic** and the cover holds.

#![cfg(all(
    feature = "field",
    feature = "package",
    feature = "opc",
    feature = "pptx"
))]

use std::path::PathBuf;

use vole_document::adapter::pptx::{PptxExtractProfile, SlideModel, build_pptx_model, parse_slide};
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
        .join("tools/fixtures/pptx")
        .join(name)
}

fn fixture_bytes(name: &str) -> Vec<u8> {
    std::fs::read(fixture_path(name)).unwrap_or_else(|e| panic!("read fixture {name}: {e}"))
}

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-pptx-{label}-{}-{}",
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
        format_basis: "opaque:pptx-test".to_string(),
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

const P: PptxExtractProfile = PptxExtractProfile::DEFAULT;

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn detection_and_capabilities_are_byte_based() {
    for name in [
        "basic.pptx",
        "table.pptx",
        "picture.pptx",
        "notes.pptx",
        "order.pptx",
    ] {
        assert_eq!(
            detect_document_format(&fixture_bytes(name), Limits::DEFAULT),
            DocumentFormat::Pptx,
            "{name}"
        );
    }
    // A plain ZIP is opaque, not PPTX.
    let plain = build_zip(&[Entry::stored("hello.txt", b"hi")]);
    assert_eq!(
        detect_document_format(&plain, Limits::DEFAULT),
        DocumentFormat::Opaque
    );

    let caps = capabilities_for_format(DocumentFormat::Pptx);
    assert!(caps.compiled);
    let names: Vec<&str> = caps.selectors.iter().map(|s| s.selector).collect();
    for expected in ["metadata", "text", "table", "cell", "search-match"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    assert!(caps.native_selectors.contains(&"pptx-slide"));
    assert!(caps.native_selectors.contains(&"pptx-shape"));
    assert!(caps.native_selectors.contains(&"pptx-notes"));
    let json = caps.to_json();
    assert!(json.contains("\"format\":\"pptx\""), "{json}");
}

#[test]
fn metadata_reports_slide_count_size_and_title() {
    let mut fx = Fixture::named("basic.pptx", "meta-basic");
    let (meta, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Metadata,
        Representation::Metadata,
    );
    let j = answer_json(&meta);
    assert!(j.contains("\"format\":\"pptx\""), "{j}");
    assert!(j.contains("\"slides\":2"), "{j}");
    assert!(j.contains("\"slide_size_cx\":9144000"), "{j}");
    assert!(j.contains("\"title\":\"Quarterly Report\""), "{j}");
    assert!(j.contains("/ppt/presentation.xml"), "{j}");
    assert!(
        meta.provenance.starts_with("format=pptx;common;"),
        "{}",
        meta.provenance
    );
}

#[test]
fn slide_text_shapes_and_structure() {
    let mut fx = Fixture::named("basic.pptx", "shapes-basic");
    // Native slide 0 text: title then body, pre-order.
    let (s0, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::PptxSlide {
            index: 0,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&s0), "Quarterly Report\nRevenue up\nCosts flat");
    // Slide 1.
    let (s1, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::PptxSlide {
            index: 1,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&s1), "Thank you");

    // Slide 0 metadata: two shapes.
    let (sm, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::PptxSlide {
            index: 0,
            profile: P,
        },
        Representation::Metadata,
    );
    let j = answer_json(&sm);
    assert!(j.contains("\"shapes\":2"), "{j}");
    assert!(j.contains("\"hidden\":false"), "{j}");

    // Shape 0 is the title.
    let (sh0, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::PptxShape {
            slide: 0,
            index: 0,
            profile: P,
        },
        Representation::Metadata,
    );
    let j0 = answer_json(&sh0);
    assert!(j0.contains("\"kind\":\"text\""), "{j0}");
    assert!(j0.contains("\"placeholder\":\"title\""), "{j0}");
    assert!(j0.contains("\"text\":\"Quarterly Report\""), "{j0}");

    let (sh0t, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::PptxShape {
            slide: 0,
            index: 0,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&sh0t), "Quarterly Report");
}

#[test]
fn common_text_table_cell_and_find() {
    let mut fx = Fixture::named("basic.pptx", "common-basic");
    let (text, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Text,
        Representation::Text,
    );
    assert_eq!(
        answer_text(&text),
        "Quarterly Report\nRevenue up\nCosts flat\nThank you"
    );

    // Search-match over slide text.
    let (found, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::SearchMatch("Revenue".into()),
        Representation::Text,
    );
    assert!(answer_json(&found).contains("Revenue up"), "{found:?}");

    // Native find.
    let (nf, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::PptxFind {
            pattern: "Costs".into(),
            profile: P,
        },
        Representation::Text,
    );
    assert!(answer_json(&nf).contains("Costs flat"), "{nf:?}");

    // The table deck: common table 0 and cell (row 1, col 1).
    let mut t = Fixture::named("table.pptx", "common-table");
    let (tb, _) = observe_eq(
        &mut t.store,
        &t.report.field,
        Selector::Table(0),
        Representation::Text,
    );
    assert_eq!(answer_text(&tb), "Name\tQty\nWidget\t3");
    let (cell, _) = observe_eq(
        &mut t.store,
        &t.report.field,
        Selector::Cell {
            table: 0,
            row: 1,
            col: 1,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&cell), "3");
    // There is no second table.
    let class = observe_err(
        &mut t.store,
        &t.report.field,
        Selector::Table(1),
        Representation::Text,
    );
    assert_eq!(class, ErrorClass::UnsupportedFeature);
}

#[test]
fn table_common_text_includes_embedded_table() {
    // Decision: an embedded `a:tbl` contributes its cell text to the slide/shape
    // text projection (matching DOCX, where a table block's text is part of the
    // body). The structured table is also exposed natively via `--pptx-tables`.
    let mut fx = Fixture::named("table.pptx", "table-text");
    let (text, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Text,
        Representation::Text,
    );
    assert_eq!(answer_text(&text), "Name\tQty\nWidget\t3");
    // The table shape's own text is the table text.
    let (sh, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::PptxShape {
            slide: 0,
            index: 0,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&sh), "Name\tQty\nWidget\t3");
    // The native tables observation agrees.
    let (tb, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::PptxTables {
            slide: 0,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&tb), "Name\tQty\nWidget\t3");
    // Search finds the table text.
    let (found, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::PptxFind {
            pattern: "Widget".into(),
            profile: P,
        },
        Representation::Text,
    );
    assert!(answer_json(&found).contains("Widget"), "{found:?}");
}

/// Regression: observing one PPTX field must never corrupt a later PPTX field's
/// observations in the same store. The derived cache is keyed by node id alone
/// and is shared across fields, so a member/derived node whose identity ignored
/// the source bytes aliased byte-different members (two `presentation.xml` parts
/// of equal `(offset, len)`) and returned the wrong slide order.
#[test]
fn interleaved_pptx_fields_do_not_alias_in_a_shared_store() {
    let root = temp_dir("interleaved");
    let store_root = root.join("store");
    let basic = fixture_bytes("basic.pptx");
    let order = fixture_bytes("order.pptx");

    let f_order = {
        let mut store = FieldStore::open(&store_root).unwrap();
        let fb = ingest_package(&mut store, &opaque_descriptor(&basic), Limits::DEFAULT)
            .unwrap()
            .field;
        // Observe basic FIRST: this populates the shared derived cache, exactly the
        // interleaving that used to corrupt a later PPTX field.
        let (t, _) = observe_eq(&mut store, &fb, Selector::Text, Representation::Text);
        assert_eq!(
            answer_text(&t),
            "Quarterly Report\nRevenue up\nCosts flat\nThank you"
        );
        ingest_package(&mut store, &opaque_descriptor(&order), Limits::DEFAULT)
            .unwrap()
            .field
    };

    let mut store = FieldStore::open(&store_root).unwrap();
    // The full common text must be in `p:sldIdLst` order, not collapsed.
    let (t, _) = observe_eq(&mut store, &f_order, Selector::Text, Representation::Text);
    assert_eq!(answer_text(&t), "SECOND FILE\nFIRST FILE");
    // Both native slides resolve correctly too.
    let (s0, _) = observe_eq(
        &mut store,
        &f_order,
        Selector::PptxSlide {
            index: 0,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&s0), "SECOND FILE");
    let (s1, _) = observe_eq(
        &mut store,
        &f_order,
        Selector::PptxSlide {
            index: 1,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&s1), "FIRST FILE");
    drop(store);

    // The field id is a function of the source, not of the store's other
    // contents: building `order.pptx` alone must yield the same field id.
    let alone_root = root.join("alone");
    let f_order_alone = {
        let mut s = FieldStore::open(&alone_root).unwrap();
        ingest_package(&mut s, &opaque_descriptor(&order), Limits::DEFAULT)
            .unwrap()
            .field
    };
    assert_eq!(
        f_order, f_order_alone,
        "a field id must not depend on the store's other PPTX fields"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn native_layouts_masters_theme_and_media() {
    let mut fx = Fixture::named("basic.pptx", "parts-basic");
    for (selector, needle) in [
        (Selector::PptxLayouts, "slideLayout1.xml"),
        (Selector::PptxMasters, "slideMaster1.xml"),
        (Selector::PptxTheme, "theme1.xml"),
    ] {
        let (a, _) = observe_eq(
            &mut fx.store,
            &fx.report.field,
            selector,
            Representation::Metadata,
        );
        assert!(answer_json(&a).contains(needle), "{}", answer_json(&a));
    }

    // The picture deck: a media resource by ordinal.
    let mut p = Fixture::named("picture.pptx", "parts-picture");
    let (m, _) = observe_eq(
        &mut p.store,
        &p.report.field,
        Selector::PptxMedia { ordinal: 0 },
        Representation::Metadata,
    );
    let jm = answer_json(&m);
    assert!(jm.contains("image1.png"), "{jm}");
    assert!(jm.contains("\"contentType\":\"image/png\""), "{jm}");

    let (mb, _) = observe_eq(
        &mut p.store,
        &p.report.field,
        Selector::PptxMedia { ordinal: 0 },
        Representation::DecodedBytes,
    );
    let bytes = answer_bytes(&mb);
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "not a PNG");
    // A media ordinal past the end declines typed.
    let class = observe_err(
        &mut p.store,
        &p.report.field,
        Selector::PptxMedia { ordinal: 9 },
        Representation::Metadata,
    );
    assert_eq!(class, ErrorClass::UnsupportedFeature);

    // The picture shape references the media relationship.
    let (sh, _) = observe_eq(
        &mut p.store,
        &p.report.field,
        Selector::PptxShape {
            slide: 0,
            index: 0,
            profile: P,
        },
        Representation::Metadata,
    );
    let js = answer_json(&sh);
    assert!(js.contains("\"kind\":\"picture\""), "{js}");
    assert!(js.contains("\"media\":\"rId2\""), "{js}");
}

#[test]
fn slide_order_follows_sldidlst() {
    // `order.pptx` lists rId3 (→ slide2.xml) then rId4 (→ slide1.xml), so
    // presentation-order slide 0 is the *file* `slide2.xml`.
    let mut fx = Fixture::named("order.pptx", "order");
    let (s0, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::PptxSlide {
            index: 0,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&s0), "SECOND FILE");
    let (s1, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::PptxSlide {
            index: 1,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&s1), "FIRST FILE");
    // The whole-deck text is in `sldIdLst` order, not file-name order.
    let (text, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Text,
        Representation::Text,
    );
    assert_eq!(answer_text(&text), "SECOND FILE\nFIRST FILE");
}

#[test]
fn notes_selector_and_profile() {
    let mut fx = Fixture::named("notes.pptx", "notes");
    let (n, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::PptxNotes {
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
    // A profile that includes notes is a distinct, fingerprinted identity; the
    // native slide path still resolves the same slide body.
    let mut with_notes = P;
    with_notes.include_notes = true;
    assert!(with_notes.fingerprint().contains("n1"));
    assert_ne!(with_notes.encode(), P.encode());
    let (again, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::PptxSlide {
            index: 0,
            profile: with_notes,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&again), "Body text");
    // A notes index past the end declines typed.
    let class = observe_err(
        &mut fx.store,
        &fx.report.field,
        Selector::PptxNotes {
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
        "basic.pptx",
        "table.pptx",
        "picture.pptx",
        "notes.pptx",
        "order.pptx",
    ] {
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
    let source = fixture_bytes("basic.pptx");
    let descriptor = opaque_descriptor(&source);
    let src_path = root.join("doc.pptx");
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
        Selector::PptxSlide {
            index: 0,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(
        answer_text(&text),
        "Quarterly Report\nRevenue up\nCosts flat"
    );

    let field = Field::open(&store2, &field_id, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), source.len());
    assert_eq!(sha256(&exact), sha256(&source));
    assert!(exact == source);
    drop(store2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn unsupported_common_pairs_decline_typed() {
    let mut fx = Fixture::named("basic.pptx", "declines");
    // PPTX does not map heading/block/resource/link cleanly: typed declines.
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
    // A slide index past the end declines.
    let class = observe_err(
        &mut fx.store,
        &fx.report.field,
        Selector::PptxSlide {
            index: 9,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(class, ErrorClass::UnsupportedFeature);
    // A shape index past the end declines.
    let class = observe_err(
        &mut fx.store,
        &fx.report.field,
        Selector::PptxShape {
            slide: 0,
            index: 9,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(class, ErrorClass::UnsupportedFeature);
    // A representation the layouts observation does not serve declines.
    let class = observe_err(
        &mut fx.store,
        &fx.report.field,
        Selector::PptxLayouts,
        Representation::ExactBytes,
    );
    assert_eq!(class, ErrorClass::UnsupportedFeature);
}

#[test]
fn missing_relationship_declines_typed_and_keeps_exactness() {
    // Content types declare the presentation main type, but no `_rels/.rels`.
    let ct = concat!(
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
        r#"<Default Extension="xml" ContentType="application/xml"/>"#,
        r#"<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>"#,
        r#"</Types>"#
    );
    let pres = r#"<p:presentation xmlns:p="x"><p:sldIdLst/></p:presentation>"#;
    let entries = vec![
        Entry::stored("[Content_Types].xml", ct.as_bytes()),
        Entry::deflated("ppt/presentation.xml", pres.as_bytes()),
    ];
    let mut fx = Fixture::from_entries("no-rel", &entries);
    assert_eq!(fx.report.format, DocumentFormat::Pptx);
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
fn malformed_slide_declines_typed() {
    let ct = concat!(
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
        r#"<Default Extension="xml" ContentType="application/xml"/>"#,
        r#"<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>"#,
        r#"<Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>"#,
        r#"</Types>"#
    );
    let rels = concat!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
        r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/>"#,
        r#"</Relationships>"#
    );
    let pres = r#"<p:presentation xmlns:p="x" xmlns:r="z"><p:sldIdLst><p:sldId id="256" r:id="rId2"/></p:sldIdLst></p:presentation>"#;
    let pres_rels = concat!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
        r#"<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/>"#,
        r#"</Relationships>"#
    );
    let entries = vec![
        Entry::stored("[Content_Types].xml", ct.as_bytes()),
        Entry::stored("_rels/.rels", rels.as_bytes()),
        Entry::stored("ppt/presentation.xml", pres.as_bytes()),
        Entry::stored("ppt/_rels/presentation.xml.rels", pres_rels.as_bytes()),
        Entry::stored("ppt/slides/slide1.xml", b"<notaslide/>"),
    ];
    let mut fx = Fixture::from_entries("bad-slide", &entries);
    let class = observe_err(
        &mut fx.store,
        &fx.report.field,
        Selector::PptxSlide {
            index: 0,
            profile: P,
        },
        Representation::Text,
    );
    assert_eq!(class, ErrorClass::InvalidPackageStructure);
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn random_bytes_never_panic_and_cover_holds() {
    let mut state: u64 = 0x2120_2A01_u64.wrapping_mul(0x9E37_79B9_7F4A_7C15);
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
        let _ = capabilities_for_format(DocumentFormat::Pptx);
        // The adapter parsers must decline typed, never panic.
        assert!(
            build_pptx_model(&buf, Limits::STRICT).is_err(),
            "round {round}"
        );
        let _ = parse_slide(&buf, "/x", &P, Limits::STRICT);
        let _ = vole_document::adapter::pptx::parse_presentation(&buf, Limits::STRICT);
        let _ = vole_document::adapter::pptx::parse_notes(&buf, "/x", &P, Limits::STRICT);
    }
    // A benign real fixture at STRICT limits still parses and covers exactly.
    let source = fixture_bytes("basic.pptx");
    let physical = vole_document::adapter::package::scan(&source, Limits::DEFAULT).unwrap();
    physical.validate(source.len() as u64).unwrap();
    physical.reemits(&source).unwrap();
}

#[test]
fn adapter_unit_roundtrips_are_stable() {
    let slide = parse_slide(
        br#"<p:sld><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>x</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#,
        "/x",
        &P,
        Limits::DEFAULT,
    )
    .unwrap();
    assert_eq!(slide.text(), "x");
    assert_eq!(SlideModel::decode(&slide.encode()).unwrap(), slide);
}

#[test]
fn docx_embedding_a_pptx_is_detected_as_docx() {
    let content_types = concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
        r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#,
        r#"<Default Extension="xml" ContentType="application/xml"/>"#,
        r#"<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>"#,
        r#"<Override PartName="/word/embeddings/Deck1.pptx" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation"/>"#,
        r#"</Types>"#
    );
    let rels = concat!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
        r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>"#,
        r#"</Relationships>"#
    );
    let entries = vec![
        Entry::stored("[Content_Types].xml", content_types.as_bytes()),
        Entry::stored("_rels/.rels", rels.as_bytes()),
        Entry::stored(
            "word/document.xml",
            br#"<w:document xmlns:w="x"><w:body/></w:document>"#,
        ),
        Entry::stored(
            "word/embeddings/Deck1.pptx",
            b"PK\x03\x04not-a-real-embedded-presentation",
        ),
    ];
    let zip = build_zip(&entries);
    assert_eq!(
        detect_document_format(&zip, Limits::DEFAULT),
        DocumentFormat::Docx
    );
}
