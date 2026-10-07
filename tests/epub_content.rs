//! Phase 12.6 court: bounded XHTML content observations for EPUB spine items.
//!
//! Gated on `epub` (which implies `package`, `field`, and the shared `xml`
//! policy), so a build without the feature compiles an empty target.
//!
//! A hand-authored minimal **EPUB 3** is built in-test as a real ZIP (no external
//! tool): `mimetype` stored first, `META-INF/container.xml`, `OEBPS/package.opf`,
//! three linear XHTML content documents (the third is rich: heading, paragraph,
//! list, table with a known cell, internal + external links, an image), a nav
//! document, and one image resource. The court checks:
//!
//! 1. **`SpineItem(2) + Text`** returns the known reading text.
//! 2. **`SpineItem(2) + Structure`** exposes headings/lists/tables.
//! 3. **`EpubCell`** returns the known table-cell value.
//! 4. **Links/resources are classified** (internal vs external-inert).
//! 5. **Progressive inversion**: one item's query parses **only** that item.
//! 6. **Preview** is an honest structured-line preview, never a page render.
//! 7. **Hostile/deep/entity XHTML declines typed** without panic.
//! 8. **Exactness** is untouched by any content observation or decline.
//! 9. **Restart reuse** serves the persisted content model from disk.

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
// Dependency-free ZIP writer (test ground truth), mirroring the 12.5 court.
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

const CHAPTER1: &[u8] =
    br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>CHAPTER ONE</p></body></html>"#;
const CHAPTER2: &[u8] =
    br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>CHAPTER TWO</p></body></html>"#;

/// The rich third content document: heading + paragraph + list + table (with a
/// known cell) + internal and external links + an image reference.
const CHAPTER3: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head><title>Three</title></head>
<body>
<section epub:type="chapter" id="sec3">
<h1 id="h-three">Chapter Three</h1>
<p>The quick brown fox jumps over the lazy dog.</p>
<ul><li>alpha</li><li>beta</li></ul>
<table id="t1">
<tr><th>Key</th><th>Value</th></tr>
<tr><td>answer</td><td>42</td></tr>
</table>
<p>See <a href="chapter1.xhtml#c1">chapter one</a> and <a href="https://example.com/ext">external</a>.</p>
<img src="images/pic.png" alt="pic"/>
</section>
</body>
</html>"#;

const NAV_XHTML: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8"?>"#,
    r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">"#,
    r#"<head><title>Nav</title></head><body>"#,
    r#"<nav epub:type="toc" id="toc"><ol>"#,
    r#"<li><a href="chapter1.xhtml">Chapter One</a></li>"#,
    r#"<li><a href="chapter2.xhtml">Chapter Two</a></li>"#,
    r#"<li><a href="chapter3.xhtml">Chapter Three</a></li>"#,
    r#"</ol></nav>"#,
    r#"</body></html>"#
);

const PIC: &[u8] = b"\x89PNG\r\n\x1a\nFAKE-IMAGE-BYTES";

/// Build a package document with `n` linear chapters (chapter1..chapterN) and a
/// non-linear nav item.
fn package_opf(n: u32) -> String {
    let mut manifest = String::new();
    let mut spine = String::new();
    manifest.push_str(
        r#"<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#,
    );
    for i in 1..=n {
        manifest.push_str(&format!(
            r#"<item id="ch{i}" href="chapter{i}.xhtml" media-type="application/xhtml+xml"/>"#
        ));
        spine.push_str(&format!(r#"<itemref idref="ch{i}"/>"#));
    }
    manifest.push_str(r#"<item id="pic" href="images/pic.png" media-type="image/png"/>"#);
    format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="pub-id">"#,
            r#"<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">"#,
            r#"<dc:identifier id="pub-id">urn:uuid:12345678-1234-1234-1234-123456789012</dc:identifier>"#,
            r#"<dc:title>Content Test</dc:title>"#,
            r#"</metadata>"#,
            r#"<manifest>{manifest}</manifest>"#,
            r#"<spine>{spine}<itemref idref="nav" linear="no"/></spine>"#,
            r#"</package>"#
        ),
        manifest = manifest,
        spine = spine,
    )
}

fn entries_with_chapter3(ch3: &[u8]) -> Vec<Entry> {
    vec![
        Entry::stored("mimetype", MIMETYPE),
        Entry::stored("META-INF/container.xml", CONTAINER_XML.as_bytes()),
        Entry::deflated("OEBPS/package.opf", package_opf(3).as_bytes()),
        Entry::stored("OEBPS/chapter1.xhtml", CHAPTER1),
        Entry::stored("OEBPS/chapter2.xhtml", CHAPTER2),
        Entry::stored("OEBPS/chapter3.xhtml", ch3),
        Entry::stored("OEBPS/nav.xhtml", NAV_XHTML.as_bytes()),
        Entry::stored("OEBPS/images/pic.png", PIC),
    ]
}

fn entries_chapters(n: u32) -> Vec<Entry> {
    let mut v = vec![
        Entry::stored("mimetype", MIMETYPE),
        Entry::stored("META-INF/container.xml", CONTAINER_XML.as_bytes()),
        Entry::deflated("OEBPS/package.opf", package_opf(n).as_bytes()),
    ];
    for i in 1..=n {
        let body = match i {
            1 => CHAPTER1.to_vec(),
            2 => CHAPTER2.to_vec(),
            3 => CHAPTER3.to_vec(),
            _ => format!(
                r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>FILLER {i}</p></body></html>"#
            )
            .into_bytes(),
        };
        v.push(Entry::stored(&format!("OEBPS/chapter{i}.xhtml"), &body));
    }
    v.push(Entry::stored("OEBPS/nav.xhtml", NAV_XHTML.as_bytes()));
    v.push(Entry::stored("OEBPS/images/pic.png", PIC));
    v
}

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-epubc-{label}-{}-{}",
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
        format_basis: "opaque:epub-content-test".to_string(),
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
        Fixture::from_entries(label, &entries_with_chapter3(CHAPTER3))
    }

    fn with_chapters(label: &str, n: u32) -> Fixture {
        Fixture::from_entries(label, &entries_chapters(n))
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

fn spine(index: u32, profile: EpubExtractProfile) -> Selector {
    Selector::EpubSpineItem { index, profile }
}

const DEFAULT: EpubExtractProfile = EpubExtractProfile::DEFAULT;

// ---------------------------------------------------------------------------
// Courts
// ---------------------------------------------------------------------------

#[test]
fn spine_item_text_returns_known_content() {
    let mut fx = Fixture::standard("text");
    let (answer, stats) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        spine(2, DEFAULT),
        Representation::Text,
    );
    let text = answer_text(&answer);
    assert!(
        text.contains("The quick brown fox jumps over the lazy dog."),
        "{text}"
    );
    assert!(text.contains("Chapter Three"), "{text}");
    assert!(text.contains("alpha"), "{text}");
    assert!(text.contains("answer\t42"), "{text}");
    // Derived, never exact.
    assert_eq!(answer.basis, Basis::DeterministicallyDerived);
    assert!(!answer.exact);
    assert!(
        answer.provenance.contains("spine=2"),
        "{}",
        answer.provenance
    );
    assert!(
        answer.provenance.contains("content"),
        "{}",
        answer.provenance
    );
    // A one-item query executes only a handful of nodes.
    assert!(
        stats.seed_nodes_materialized > 0 && stats.seed_nodes_materialized <= 6,
        "{stats:?}"
    );
}

#[test]
fn structure_exposes_headings_lists_and_tables() {
    let mut fx = Fixture::standard("structure");
    let (answer, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        spine(2, DEFAULT),
        Representation::Structure,
    );
    let json = answer_json(&answer);
    assert!(json.contains("\"kind\":\"heading\""), "{json}");
    assert!(json.contains("\"level\":1"), "{json}");
    assert!(json.contains("\"Chapter Three\""), "{json}");
    assert!(json.contains("\"kind\":\"list\""), "{json}");
    assert!(json.contains("\"ordered\":false"), "{json}");
    assert!(json.contains("\"alpha\""), "{json}");
    assert!(json.contains("\"kind\":\"table\""), "{json}");
    assert!(json.contains("\"rows\":2"), "{json}");
    assert!(json.contains("\"scripted\":false"), "{json}");
    assert!(json.contains("\"root\":\"html\""), "{json}");
    assert!(json.contains("\"epub_type\":\"chapter\""), "{json}");
    // The structure carries the bounded per-item XHTML node counter.
    assert!(json.contains("\"xhtml_nodes\":"), "{json}");
}

#[test]
fn table_cell_observation_returns_known_value() {
    let mut fx = Fixture::standard("cell");
    // Table 0, physical row 1, physical cell 1 is `42`.
    let (answer, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::EpubCell {
            index: 2,
            table: 0,
            row: 1,
            col: 1,
            profile: DEFAULT,
        },
        Representation::Text,
    );
    assert_eq!(answer_text(&answer), "42");

    // The header cell is a header, addressed at row 0, cell 0.
    let (meta, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::EpubCell {
            index: 2,
            table: 0,
            row: 0,
            col: 0,
            profile: DEFAULT,
        },
        Representation::Metadata,
    );
    let json = answer_json(&meta);
    assert!(json.contains("\"header\":true"), "{json}");
    assert!(json.contains("\"colspan\":1"), "{json}");

    // An out-of-range cell declines typed rather than inventing one.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &fx.report.field,
            Selector::EpubCell {
                index: 2,
                table: 0,
                row: 9,
                col: 0,
                profile: DEFAULT,
            },
            Representation::Text,
        ),
        ErrorClass::UnsupportedFeature
    );
}

#[test]
fn links_and_resources_are_classified() {
    let mut fx = Fixture::standard("links");
    let (answer, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        spine(2, DEFAULT),
        Representation::Structure,
    );
    let json = answer_json(&answer);
    // Internal link with a fragment.
    assert!(
        json.contains("\"member\":\"OEBPS/chapter1.xhtml\""),
        "{json}"
    );
    assert!(json.contains("\"fragment\":\"c1\""), "{json}");
    // External link is inert.
    assert!(
        json.contains("\"href\":\"https://example.com/ext\""),
        "{json}"
    );
    assert!(json.contains("\"external\":true"), "{json}");
    // Image resource resolved to the container member.
    assert!(json.contains("\"kind\":\"img\""), "{json}");
    assert!(
        json.contains("\"member\":\"OEBPS/images/pic.png\""),
        "{json}"
    );

    // A single-link observation is separately queryable.
    let (link, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::EpubLink {
            index: 2,
            link: 1,
            profile: DEFAULT,
        },
        Representation::Metadata,
    );
    let lj = answer_json(&link);
    assert!(lj.contains("https://example.com/ext"), "{lj}");
    assert!(lj.contains("\"external\":true"), "{lj}");
}

#[test]
fn block_observation_is_scoped() {
    let mut fx = Fixture::standard("block");
    // Block 1 is the paragraph with the known sentence.
    let (answer, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::EpubBlock {
            index: 2,
            block: 1,
            profile: DEFAULT,
        },
        Representation::Text,
    );
    assert_eq!(
        answer_text(&answer),
        "The quick brown fox jumps over the lazy dog."
    );
}

#[test]
fn find_is_scoped_to_the_spine_item() {
    let mut fx = Fixture::standard("find");
    let (answer, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::EpubFind {
            index: 2,
            pattern: "quick brown".to_string(),
            profile: DEFAULT,
        },
        Representation::Text,
    );
    let json = answer_json(&answer);
    assert!(json.contains("The quick brown fox"), "{json}");
    assert!(json.contains("\"kind\":\"paragraph\""), "{json}");

    // A pattern only present in another item is not found here.
    let (none, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        Selector::EpubFind {
            index: 2,
            pattern: "CHAPTER TWO".to_string(),
            profile: DEFAULT,
        },
        Representation::Text,
    );
    assert_eq!(answer_json(&none), "[]");
}

#[test]
fn preview_is_structured_and_not_a_page_render() {
    let mut fx = Fixture::standard("preview");
    let (answer, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        spine(2, DEFAULT),
        Representation::Preview,
    );
    let text = answer_text(&answer);
    assert!(text.starts_with("VOLE-EPUB-PREVIEW v1\n"), "{text}");
    assert!(text.contains("not a page render"), "{text}");
    assert!(text.contains("H1 [0] Chapter Three"), "{text}");
    assert!(text.contains("TABLE"), "{text}");
    assert!(text.contains("OEBPS/images/pic.png"), "{text}");
}

#[test]
fn one_item_query_does_not_parse_the_whole_publication() {
    // The same first item is queried in a 3-chapter and a 5-chapter publication.
    // A scoped parse reads the *same* bounded work in both; parsing every spine
    // item would make the 5-chapter case strictly larger.
    let mut small = Fixture::with_chapters("small", 3);
    let mut large = Fixture::with_chapters("large", 5);
    let (_, sstats) = observe_eq(
        &mut small.store,
        &small.report.field,
        spine(0, DEFAULT),
        Representation::Text,
    );
    let (_, lstats) = observe_eq(
        &mut large.store,
        &large.report.field,
        spine(0, DEFAULT),
        Representation::Text,
    );
    assert_eq!(
        sstats.seed_nodes_materialized, lstats.seed_nodes_materialized,
        "a one-item query must not scale with the publication: small={sstats:?} large={lstats:?}"
    );
    assert!(sstats.seed_nodes_materialized <= 6, "{sstats:?}");

    // The per-item XHTML node counter proves the parse touched only one document.
    let (s0, _) = observe_eq(
        &mut small.store,
        &small.report.field,
        spine(0, DEFAULT),
        Representation::Structure,
    );
    let (s2, _) = observe_eq(
        &mut small.store,
        &small.report.field,
        spine(2, DEFAULT),
        Representation::Structure,
    );
    let n0: u64 = extract_u64(&answer_json(&s0), "\"xhtml_nodes\":");
    let n2: u64 = extract_u64(&answer_json(&s2), "\"xhtml_nodes\":");
    assert!(n0 >= 3, "chapter 1 should have a few nodes, got {n0}");
    assert!(
        n2 > n0,
        "the rich chapter should scan more nodes: {n2} vs {n0}"
    );
}

#[test]
fn ingest_does_not_parse_content_documents() {
    // Ingest node count is independent of the size of the chapter bodies: no
    // content document is parsed at ingest.
    let simple = Fixture::from_entries(
        "ingest-simple",
        &entries_with_chapter3(
            br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>TINY</p></body></html>"#,
        ),
    );
    let rich = Fixture::standard("ingest-rich");
    assert_eq!(simple.report.node_count, rich.report.node_count);
    assert_eq!(rich.report.epub_model_nodes, 1);
}

#[test]
fn hostile_content_declines_typed_and_keeps_exactness() {
    // Deep nesting, a DOCTYPE (no DTD/entity expansion), and a custom entity via
    // an internal subset: each declines typed, never panics.
    let mut deep = String::from("<html xmlns=\"http://www.w3.org/1999/xhtml\"><body>");
    for _ in 0..(Limits::DEFAULT.max_xml_depth + 8) {
        deep.push_str("<div>");
    }
    deep.push_str("x</body></html>");

    let cases: [(&str, Vec<u8>); 2] = [
        ("deep", deep.into_bytes()),
        (
            "entity",
            b"<!DOCTYPE html [<!ENTITY x \"boom\">]><html><body><p>&x;</p></body></html>".to_vec(),
        ),
    ];
    for (label, body) in cases {
        let mut fx = Fixture::from_entries(label, &entries_with_chapter3(&body));
        assert_eq!(
            observe_err(
                &mut fx.store,
                &fx.report.field,
                spine(2, DEFAULT),
                Representation::Text,
            ),
            ErrorClass::InvalidXmlStructure,
            "case {label}"
        );
        // The hostile document is still an exact archival object.
        let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
        let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
        assert_eq!(exact.len(), fx.source.len());
        assert_eq!(exact, fx.source);
    }
}

#[test]
fn undeclared_entity_does_not_panic_or_expand() {
    // Without a DOCTYPE there is no entity resolver; an undeclared reference is
    // dropped rather than expanded (no billion-laughs, no XXE).
    let body = br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>a&nope;b</p></body></html>"#;
    let mut fx = Fixture::from_entries("undecl", &entries_with_chapter3(body));
    let (answer, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        spine(2, DEFAULT),
        Representation::Text,
    );
    assert_eq!(answer_text(&answer).trim(), "ab");
}

#[test]
fn exact_materialization_is_byte_identical() {
    let mut fx = Fixture::standard("exact");
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    let exact = field.materialize_exact(Limits::DEFAULT).unwrap();
    assert_eq!(exact.len(), fx.source.len());
    assert_eq!(exact, fx.source);
    drop(field);

    // A content observation does not disturb exact authority.
    let (_, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        spine(2, DEFAULT),
        Representation::Text,
    );
    let field = Field::open(&fx.store, &fx.report.field, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), fx.source);
}

#[test]
fn non_linear_item_is_excluded_by_default() {
    let mut fx = Fixture::standard("nonlinear");
    // Index 3 is the non-linear nav item; the default profile excludes it.
    assert_eq!(
        observe_err(
            &mut fx.store,
            &fx.report.field,
            spine(3, DEFAULT),
            Representation::Text,
        ),
        ErrorClass::UnsupportedFeature
    );
    let all = EpubExtractProfile {
        spine: SpineScope::All,
        ..DEFAULT
    };
    // Under `All` the nav document is a content document and parses.
    let (answer, _) = observe_eq(
        &mut fx.store,
        &fx.report.field,
        spine(3, all),
        Representation::Structure,
    );
    let json = answer_json(&answer);
    assert!(json.contains("\"profile\":\"v1-all-"), "{json}");
}

#[test]
fn progressive_inversion_reuses_content_after_restart() {
    let fx = Fixture::standard("restart");
    let field_id = fx.report.field;
    let root = fx.root.clone();

    let mut store = FieldStore::open(&root).unwrap();
    let (_, cold) = observe_eq(
        &mut store,
        &field_id,
        spine(2, DEFAULT),
        Representation::Text,
    );
    assert!(cold.seed_nodes_materialized > 0);
    drop(store);

    // A fresh handle on the same store directory simulates a process restart.
    let mut store2 = FieldStore::open(&root).unwrap();
    let (warm, warm_stats) = observe_eq(
        &mut store2,
        &field_id,
        spine(2, DEFAULT),
        Representation::Text,
    );
    assert!(answer_text(&warm).contains("The quick brown fox"));
    assert!(
        warm_stats.seed_nodes_reused >= 1,
        "expected a persisted content reuse, got {warm_stats:?}"
    );
    assert!(
        warm_stats.seed_nodes_executed <= cold.seed_nodes_executed,
        "warm={warm_stats:?} cold={cold:?}"
    );
}

/// Extract the `u64` following `needle` in a JSON string.
fn extract_u64(json: &str, needle: &str) -> u64 {
    let at = json
        .find(needle)
        .unwrap_or_else(|| panic!("{needle} in {json}"));
    let rest = &json[at + needle.len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().unwrap()
}
