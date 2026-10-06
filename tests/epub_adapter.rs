//! Phase 12.5 court: the EPUB (OCF) package adapter.
//!
//! Gated on `epub` (which implies `package`, `field`, and the shared `xml` policy),
//! so a build without the feature compiles an empty target.
//!
//! A hand-authored minimal **EPUB 3** is built in-test as a real ZIP (no external
//! tool): `mimetype` stored first, `META-INF/container.xml`, `OEBPS/package.opf`
//! with metadata/manifest/spine, two XHTML content documents, a nav document, and
//! one image resource. The court checks:
//!
//! 1. **Container/rootfile** resolved semantically from `container.xml`.
//! 2. **Package** metadata/manifest/spine parsed; nav/cover/layout/progression.
//! 3. **`SpineItem(n)`** is the reading coordinate and returns the right resource;
//!    a `linear="no"` item is excluded by the default profile and included by `All`.
//! 4. **No intrinsic `Page(n)`** for reflowable EPUB (a typed decline).
//! 5. **External references are inert** (metadata shows external, bytes decline).
//! 6. **`mimetype` conformance is recorded but independent of exactness**.
//! 7. **Malformed container** declines typed with exact preservation.
//! 8. **Exactness**: `materialize(field) == original_bytes`.
//! 9. **Progressive inversion** persists derived state across a restart.

#![cfg(all(feature = "field", feature = "package", feature = "epub"))]

use std::path::PathBuf;

use vole_document::adapter::epub::{EpubExtractProfile, SpineScope};
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
// Dependency-free ZIP writer (test ground truth), mirroring the 12.4 court.
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

const MIMETYPE: &[u8] = b"application/epub+zip";

const CONTAINER_XML: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8"?>"#,
    r#"<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">"#,
    r#"<rootfiles>"#,
    r#"<rootfile full-path="OEBPS/package.opf" media-type="application/oebps-package+xml"/>"#,
    r#"</rootfiles>"#,
    r#"</container>"#
);

const PACKAGE_OPF: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8"?>"#,
    r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="pub-id" xml:lang="en">"#,
    r#"<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">"#,
    r#"<dc:identifier id="pub-id">urn:uuid:12345678-1234-1234-1234-123456789012</dc:identifier>"#,
    r#"<dc:title>Test Book</dc:title>"#,
    r#"<dc:language>en</dc:language>"#,
    r#"<meta property="dcterms:modified">2024-01-01T00:00:00Z</meta>"#,
    r#"<meta property="rendition:layout">reflowable</meta>"#,
    r#"<meta name="cover" content="cover-img"/>"#,
    r#"</metadata>"#,
    r#"<manifest>"#,
    r#"<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#,
    r#"<item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>"#,
    r#"<item id="ch2" href="chapter2.xhtml" media-type="application/xhtml+xml"/>"#,
    r#"<item id="cover-img" href="images/pic.png" media-type="image/png" properties="cover-image"/>"#,
    r#"<item id="remote" href="https://example.com/remote.xhtml" media-type="application/xhtml+xml"/>"#,
    r#"</manifest>"#,
    r#"<spine page-progression-direction="ltr">"#,
    r#"<itemref idref="ch1"/>"#,
    r#"<itemref idref="ch2"/>"#,
    r#"<itemref idref="nav" linear="no"/>"#,
    r#"</spine>"#,
    r#"</package>"#
);

const CHAPTER1: &[u8] =
    br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>CHAPTER ONE</p></body></html>"#;
const CHAPTER2: &[u8] =
    br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>CHAPTER TWO</p></body></html>"#;

const NAV_XHTML: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8"?>"#,
    r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">"#,
    r#"<head><title>Nav</title></head><body>"#,
    r#"<nav epub:type="toc" id="toc"><ol>"#,
    r#"<li><a href="chapter1.xhtml">Chapter One</a></li>"#,
    r#"<li><a href="chapter2.xhtml">Chapter Two</a></li>"#,
    r#"</ol></nav>"#,
    r#"<nav epub:type="landmarks" hidden="hidden"><ol>"#,
    r#"<li><a epub:type="bodymatter" href="chapter1.xhtml">Start</a></li>"#,
    r#"</ol></nav>"#,
    r#"</body></html>"#
);

const PIC: &[u8] = b"\x89PNG\r\n\x1a\nFAKE-IMAGE-BYTES";

const ENCRYPTION_XML: &str = concat!(
    r#"<?xml version="1.0"?>"#,
    r#"<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container"/>"#
);

fn epub_entries(mimetype: Entry, container: &str) -> Vec<Entry> {
    vec![
        mimetype,
        Entry::stored("META-INF/container.xml", container.as_bytes()),
        Entry::stored("META-INF/encryption.xml", ENCRYPTION_XML.as_bytes()),
        Entry::deflated("OEBPS/package.opf", PACKAGE_OPF.as_bytes()),
        Entry::stored("OEBPS/chapter1.xhtml", CHAPTER1),
        Entry::stored("OEBPS/chapter2.xhtml", CHAPTER2),
        Entry::stored("OEBPS/nav.xhtml", NAV_XHTML.as_bytes()),
        Entry::stored("OEBPS/images/pic.png", PIC),
    ]
}

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-epub-{label}-{}-{}",
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
        format_basis: "opaque:epub-test".to_string(),
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
        let entries = epub_entries(Entry::stored("mimetype", MIMETYPE), CONTAINER_XML);
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

fn answer_json(answer: &FieldAnswer) -> String {
    match &answer.value {
        AnswerValue::Json(j) => j.clone(),
        other => panic!("expected json, got {other:?}"),
    }
}

fn spine(index: u32, profile: EpubExtractProfile) -> Selector {
    Selector::EpubSpineItem { index, profile }
}

const DEFAULT: EpubExtractProfile = EpubExtractProfile::DEFAULT;

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn container_rootfile_and_package_resolved() {
    let mut fx = Fixture::standard("container");
    let (meta, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::EpubPackage,
        Representation::Metadata,
    );
    let json = answer_json(&meta);
    assert!(json.contains("\"package\":\"OEBPS/package.opf\""), "{json}");
    assert!(json.contains("\"conformant\":true"), "{json}");
    assert!(json.contains("\"first\":true"), "{json}");
    assert!(json.contains("\"stored\":true"), "{json}");
    assert!(json.contains("\"nav_item\":0"), "{json}");
    assert!(json.contains("\"cover_id\":\"cover-img\""), "{json}");
    assert!(
        json.contains("\"rendition_layout\":\"reflowable\""),
        "{json}"
    );
    assert!(
        json.contains("\"page_progression_direction\":\"ltr\""),
        "{json}"
    );
    assert!(json.contains("\"unique_identifier\":\"pub-id\""), "{json}");
    assert!(!meta.exact);

    // The structure view resolves the rootfile and both spine documents.
    let (structure, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::EpubPackage,
        Representation::Structure,
    );
    let sj = answer_json(&structure);
    assert!(sj.contains("\"full_path\":\"OEBPS/package.opf\""), "{sj}");
    assert!(sj.contains("\"chapter1.xhtml\""), "{sj}");
    assert!(sj.contains("\"idref\":\"ch2\""), "{sj}");
}

#[test]
fn spine_item_is_the_reading_coordinate() {
    let mut fx = Fixture::standard("spine");

    // Coordinate 1 under the default (linear-only) profile is chapter 2.
    let (meta, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        spine(1, DEFAULT),
        Representation::Metadata,
    );
    let json = answer_json(&meta);
    assert!(json.contains("\"idref\":\"ch2\""), "{json}");
    assert!(json.contains("\"linear\":true"), "{json}");
    assert!(meta.provenance.contains("spine=1"), "{}", meta.provenance);
    assert!(
        meta.provenance.contains("profile=v1-linear-only"),
        "{}",
        meta.provenance
    );

    // Bytes resolve to the exact stored member bytes.
    let (bytes, stats) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        spine(1, DEFAULT),
        Representation::ExactBytes,
    );
    assert!(bytes.exact);
    assert_eq!(answer_bytes(&bytes), CHAPTER2);
    assert!(stats.seed_nodes_fetched > 0);

    // The `linear="no"` nav item is excluded by default ...
    assert_eq!(
        observe_err(
            &mut fx.store,
            &fx.report.field,
            spine(2, DEFAULT),
            Representation::Metadata,
        ),
        ErrorClass::UnsupportedFeature
    );

    // ... and included by the `All` profile.
    let all = EpubExtractProfile {
        spine: SpineScope::All,
        ..DEFAULT
    };
    let (meta_all, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        spine(2, all),
        Representation::Metadata,
    );
    let json_all = answer_json(&meta_all);
    assert!(json_all.contains("\"idref\":\"nav\""), "{json_all}");
    assert!(json_all.contains("\"linear\":false"), "{json_all}");
    assert!(json_all.contains("v1-all"), "{json_all}");
}

#[test]
fn no_intrinsic_pages_for_reflowable() {
    let mut fx = Fixture::standard("nopages");
    // There is no `Page(n)` coordinate in a reflowable EPUB: the page selector is
    // never populated, so it declines typed rather than inventing a page.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &fx.report.field,
            Selector::Page(1),
            Representation::Text,
        ),
        ErrorClass::UnsupportedFeature
    );
}

#[test]
fn external_reference_is_inert() {
    let mut fx = Fixture::standard("external");
    let (meta, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::EpubManifestItem { index: 4 },
        Representation::Metadata,
    );
    let json = answer_json(&meta);
    assert!(json.contains("\"external\":true"), "{json}");
    assert!(json.contains("\"resolved\":null"), "{json}");
    assert!(json.contains("https://example.com/remote.xhtml"), "{json}");

    // Asking for the external target's bytes is a typed decline, never a fetch.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &fx.report.field,
            Selector::EpubManifestItem { index: 4 },
            Representation::ExactBytes,
        ),
        ErrorClass::InvalidPackageStructure
    );
}

#[test]
fn manifest_item_and_resource_bytes() {
    let mut fx = Fixture::standard("resource");

    let (meta, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::EpubManifestItem { index: 1 },
        Representation::Metadata,
    );
    let json = answer_json(&meta);
    assert!(json.contains("\"id\":\"ch1\""), "{json}");
    assert!(json.contains("\"href\":\"chapter1.xhtml\""), "{json}");
    assert!(
        json.contains("\"resolved\":\"OEBPS/chapter1.xhtml\""),
        "{json}"
    );

    let (bytes, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::EpubManifestItem { index: 1 },
        Representation::ExactBytes,
    );
    assert_eq!(answer_bytes(&bytes), CHAPTER1);

    // A resource addressed by its container member name.
    let (pic, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::EpubResource("OEBPS/images/pic.png".to_string()),
        Representation::ExactBytes,
    );
    assert!(pic.exact);
    assert_eq!(answer_bytes(&pic), PIC);
    assert!(pic.source_span.is_some());

    let (pic_meta, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::EpubResource("OEBPS/images/pic.png".to_string()),
        Representation::Metadata,
    );
    assert!(
        pic_meta.provenance.contains("OEBPS/images/pic.png"),
        "{}",
        pic_meta.provenance
    );
}

#[test]
fn nav_document_and_nodes() {
    let mut fx = Fixture::standard("nav");
    let (nav, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::EpubNav,
        Representation::Structure,
    );
    let json = answer_json(&nav);
    assert!(json.contains("\"nav\":\"toc\""), "{json}");
    assert!(json.contains("\"label\":\"Chapter One\""), "{json}");
    assert!(
        json.contains("\"member\":\"OEBPS/chapter1.xhtml\""),
        "{json}"
    );

    let (node, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::EpubNavNode { index: 0 },
        Representation::Metadata,
    );
    let nj = answer_json(&node);
    assert!(nj.contains("\"label\":\"Chapter One\""), "{nj}");
    assert!(nj.contains("\"nav\":\"toc\""), "{nj}");
    assert!(nj.contains("\"depth\":1"), "{nj}");
}

#[test]
fn mimetype_conformance_is_recorded_but_not_an_exactness_gate() {
    // A DEFLATE-compressed (non-conformant) mimetype: conformance is false, the
    // observation still succeeds, and the exact source is still recoverable.
    let entries = epub_entries(Entry::deflated("mimetype", MIMETYPE), CONTAINER_XML);
    let mut fx = Fixture::from_entries("nonconformant", &entries);
    let (meta, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::EpubPackage,
        Representation::Metadata,
    );
    let json = answer_json(&meta);
    assert!(json.contains("\"conformant\":false"), "{json}");
    assert!(json.contains("\"stored\":false"), "{json}");
    assert!(json.contains("\"exact_bytes\":true"), "{json}");

    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn malformed_container_declines_and_keeps_exactness() {
    let bad = concat!(
        r#"<!DOCTYPE container>"#,
        r#"<container><rootfiles/></container>"#
    );
    let entries = epub_entries(Entry::stored("mimetype", MIMETYPE), bad);
    let mut fx = Fixture::from_entries("malformed", &entries);
    assert_eq!(
        observe_err(
            &mut fx.store,
            &fx.report.field,
            Selector::EpubPackage,
            Representation::Metadata,
        ),
        ErrorClass::InvalidXmlStructure
    );

    // The invalid-but-preservable container is still an exact archival object.
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), fx.source.len());
    assert_eq!(exact, fx.source);
}

#[test]
fn exact_materialization_is_byte_identical() {
    let mut fx = Fixture::standard("exact");
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), fx.source.len());
    assert_eq!(exact, fx.source);
    drop(field);

    // The shared package member selector still resolves on an EPUB container.
    let (raw, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::Member(0),
        Representation::EncodedBytes,
    );
    assert!(raw.exact);
    assert_eq!(answer_bytes(&raw), MIMETYPE);
}

#[test]
fn progressive_inversion_reuses_persisted_state_after_restart() {
    let fx = Fixture::standard("restart");
    let field_id = fx.report.field;
    let root = fx.root.clone();

    let mut store = FieldStore::open(&root).unwrap();
    let (_, cold) = observe_eq(
        &mut store,
        &field_id,
        spine(1, DEFAULT),
        Representation::Metadata,
    );
    drop(store);

    // A fresh handle on the same store directory simulates a process restart.
    let mut store2 = FieldStore::open(&root).unwrap();
    let (warm, warm_stats) = observe_eq(
        &mut store2,
        &field_id,
        spine(1, DEFAULT),
        Representation::Metadata,
    );
    let json = answer_json(&warm);
    assert!(json.contains("\"idref\":\"ch2\""), "{json}");
    assert!(
        warm_stats.seed_nodes_reused >= 1,
        "expected a persisted-state reuse, got {warm_stats:?}"
    );
    assert!(warm_stats.seed_nodes_reused >= cold.seed_nodes_reused);
}

#[test]
fn answers_are_derived_and_never_exact() {
    let mut fx = Fixture::standard("basis");
    let (meta, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::EpubPackage,
        Representation::Metadata,
    );
    assert_eq!(meta.basis, Basis::DeterministicallyDerived);
    assert!(!meta.exact);
}
