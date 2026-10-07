//! Phase 12.9 / 12.10 courts: cross-format equivalence and source removal.
//!
//! This is the library-level mirror of the two shell courts. It builds the
//! **same known logical report** as a PDF, a DOCX and an EPUB in-process, runs
//! the one common observation API over all three, and checks that:
//!
//! 1. the same logical answer is produced (headings, a paragraph, Table/Cell
//!    `B7`, the one shared image resource, the link, the unique markers) with
//!    `format=<fmt>;common;<native>` provenance;
//! 2. PDF, which has no structural coordinate, declines the same selectors with
//!    a typed capability error rather than inventing an answer;
//! 3. each format is byte-exactly rematerializable after the source is gone and
//!    the store is reopened in a new process (`materialize_exact == source`,
//!    length + SHA-256 + byte identity), including through the real CLI binary.
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

use vole_document::adapter::package::crc32_iso_hdlc;
use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::error::ErrorClass;
use vole_document::field::document_format::{DocumentFormat, detect_document_format};
use vole_document::field::ingest::{IngestOutcome, ingest};
use vole_document::field::observe::{ObserveRequest, Representation, Selector, observe};
use vole_document::field::provenance::{AnswerValue, Basis, FieldAnswer};
use vole_document::field::{Field, FieldId, FieldStore};
use vole_document::limits::Limits;

// ---------------------------------------------------------------------------
// The canonical logical report (must match tools/fixtures/doc-triplet-gen.py).
// ---------------------------------------------------------------------------

const HEADINGS: [&str; 3] = ["Introduction", "Method", "Findings"];
const PARAGRAPHS: [&str; 2] = [
    "Alpha paragraph carries marker XF12A.",
    "Bravo paragraph carries marker XF12B.",
];
const LIST_ITEMS: [&str; 3] = ["First step", "Second step", "Third step"];
const TABLE_ROWS: [[&str; 2]; 8] = [
    ["Key", "Value"],
    ["alpha", "one"],
    ["bravo", "two"],
    ["charlie", "three"],
    ["delta", "four"],
    ["echo", "five"],
    ["golf", "bravo-seven"], // column B, row 7 -> B7
    ["hotel", "eight"],
];
const B7: &str = "bravo-seven";
const LINK_TEXT: &str = "external";
const LINK_HREF: &str = "https://example.com/phase12";
const MARKERS: [&str; 2] = ["XF12A", "XF12B"];
const TITLE: &str = "Phase Twelve Equivalence Report";

/// The one shared image resource: a deterministic PNG-signature blob (mirrors
/// the Python generator's `shared_image()`).
fn image_bytes() -> Vec<u8> {
    let mut v = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
    for i in 0..1024u32 {
        v.push(((i * 7 + 11) & 0xFF) as u8);
    }
    v
}

// ---------------------------------------------------------------------------
// Minimal dependency-free ZIP writer (stored members, CRC-32/ISO-HDLC).
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
        crcs.push(crc32_iso_hdlc(content));
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
// DOCX
// ---------------------------------------------------------------------------

const MAIN_CT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const STYLES_CT: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";
const CORE_CT: &str = "application/vnd.openxmlformats-package.core-properties+xml";

fn docx_source() -> Vec<u8> {
    let content_types = format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
            r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#,
            r#"<Default Extension="xml" ContentType="application/xml"/>"#,
            r#"<Default Extension="png" ContentType="image/png"/>"#,
            r#"<Override PartName="/word/document.xml" ContentType="{}"/>"#,
            r#"<Override PartName="/word/styles.xml" ContentType="{}"/>"#,
            r#"<Override PartName="/docProps/core.xml" ContentType="{}"/>"#,
            r#"</Types>"#
        ),
        MAIN_CT, STYLES_CT, CORE_CT
    );
    let package_rels = concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
        r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>"#,
        r#"</Relationships>"#
    );
    let doc_rels = concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
        r#"<Relationship Id="rIdStyles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#,
        r#"<Relationship Id="rIdHl" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/phase12" TargetMode="External"/>"#,
        r#"<Relationship Id="rIdImg" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/>"#,
        r#"</Relationships>"#
    );
    let styles = concat!(
        r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#,
        r#"<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:pPr><w:outlineLvl w:val="0"/></w:pPr></w:style>"#,
        r#"<w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:pPr><w:outlineLvl w:val="1"/></w:pPr></w:style>"#,
        r#"<w:style w:type="paragraph" w:styleId="Heading3"><w:name w:val="heading 3"/><w:pPr><w:outlineLvl w:val="2"/></w:pPr></w:style>"#,
        r#"</w:styles>"#
    );
    let core = format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
            r#"<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/">"#,
            r#"<dc:title>{}</dc:title><dc:language>en</dc:language>"#,
            r#"</cp:coreProperties>"#
        ),
        TITLE
    );
    let p = |text: &str, style: Option<&str>| -> String {
        let ppr = match style {
            Some(s) => format!(r#"<w:pPr><w:pStyle w:val="{s}"/></w:pPr>"#),
            None => String::new(),
        };
        format!(r#"<w:p>{ppr}<w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p>"#)
    };
    let mut table = String::from("<w:tbl>");
    for row in TABLE_ROWS {
        table.push_str("<w:tr>");
        for cell in row {
            table.push_str(&format!(
                "<w:tc><w:p><w:r><w:t>{cell}</w:t></w:r></w:p></w:tc>"
            ));
        }
        table.push_str("</w:tr>");
    }
    table.push_str("</w:tbl>");

    let mut body = String::new();
    body.push_str(&p(HEADINGS[0], Some("Heading1")));
    body.push_str(&p(PARAGRAPHS[0], None));
    body.push_str(&p(HEADINGS[1], Some("Heading2")));
    body.push_str(&p(PARAGRAPHS[1], None));
    body.push_str(&p(HEADINGS[2], Some("Heading3")));
    for item in LIST_ITEMS {
        body.push_str(&p(item, None));
    }
    body.push_str(&p(
        &format!(r#"<w:hyperlink r:id="rIdHl"><w:r><w:t>{LINK_TEXT}</w:t></w:r></w:hyperlink>"#),
        None,
    ));
    body.push_str(&table);
    body.push_str(
        r#"<w:p><w:r><w:drawing><a:blip xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" r:embed="rIdImg"/></w:drawing></w:r></w:p>"#,
    );
    let document = format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" "#,
            r#"xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">"#,
            r#"<w:body>{}<w:sectPr/></w:body></w:document>"#
        ),
        body
    );
    let image = image_bytes();
    build_zip(&[
        ("[Content_Types].xml", content_types.as_bytes()),
        ("_rels/.rels", package_rels.as_bytes()),
        ("word/document.xml", document.as_bytes()),
        ("word/styles.xml", styles.as_bytes()),
        ("word/_rels/document.xml.rels", doc_rels.as_bytes()),
        ("docProps/core.xml", core.as_bytes()),
        ("word/media/image1.png", &image),
    ])
}

// ---------------------------------------------------------------------------
// EPUB
// ---------------------------------------------------------------------------

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
        r#"<dc:identifier id="pub-id">urn:uuid:8e0f2c4a-1b2d-4c3e-9f10-1234567890ab</dc:identifier>"#,
        r#"<dc:title>Phase Twelve Equivalence Report</dc:title><dc:language>en</dc:language>"#,
        r#"</metadata>"#,
        r#"<manifest>"#,
        r#"<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#,
        r#"<item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>"#,
        r#"<item id="img" href="images/image1.png" media-type="image/png"/>"#,
        r#"</manifest>"#,
        r#"<spine><itemref idref="ch1"/></spine>"#,
        r#"</package>"#
    );
    let nav = concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">"#,
        r#"<head><title>Nav</title></head><body>"#,
        r#"<nav epub:type="toc" id="toc"><ol><li><a href="chapter1.xhtml">Introduction</a></li></ol></nav>"#,
        r#"</body></html>"#
    );
    let mut table = String::from("<table>");
    for (i, row) in TABLE_ROWS.iter().enumerate() {
        table.push_str("<tr>");
        let tag = if i == 0 { "th" } else { "td" };
        for cell in row {
            table.push_str(&format!("<{tag}>{cell}</{tag}>"));
        }
        table.push_str("</tr>");
    }
    table.push_str("</table>");
    let list = format!(
        "<ol>{}</ol>",
        LIST_ITEMS
            .iter()
            .map(|i| format!("<li>{i}</li>"))
            .collect::<String>()
    );
    let chapter = format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>One</title></head><body>"#,
            r#"<h1>{h0}</h1><p>{p0}</p>"#,
            r#"<h2>{h1}</h2><p>{p1}</p>"#,
            r#"<h3>{h2}</h3>{list}{table}"#,
            r#"<p>See <a href="{href}">{lt}</a>.</p>"#,
            r#"<img src="images/image1.png" alt="figure"/>"#,
            r#"</body></html>"#
        ),
        h0 = HEADINGS[0],
        p0 = PARAGRAPHS[0],
        h1 = HEADINGS[1],
        p1 = PARAGRAPHS[1],
        h2 = HEADINGS[2],
        list = list,
        table = table,
        href = LINK_HREF,
        lt = LINK_TEXT
    );
    let image = image_bytes();
    build_zip(&[
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", container.as_bytes()),
        ("OEBPS/package.opf", opf.as_bytes()),
        ("OEBPS/nav.xhtml", nav.as_bytes()),
        ("OEBPS/chapter1.xhtml", chapter.as_bytes()),
        ("OEBPS/images/image1.png", &image),
    ])
}

// ---------------------------------------------------------------------------
// PDF (classic xref, one page, unfiltered content stream of text runs)
// ---------------------------------------------------------------------------

struct PdfBuilder {
    buf: Vec<u8>,
    offsets: Vec<(u64, u64)>,
}

impl PdfBuilder {
    fn new() -> Self {
        PdfBuilder {
            buf: Vec::new(),
            offsets: Vec::new(),
        }
    }
    fn text(&mut self, s: &str) {
        self.buf.extend_from_slice(s.as_bytes());
    }
    fn raw(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }
    fn obj(&mut self, number: u64, body: &[u8]) {
        self.offsets.push((number, self.buf.len() as u64));
        self.text(&format!("{number} 0 obj\n"));
        self.raw(body);
        self.text("\nendobj\n");
    }
    fn stream_obj(&mut self, number: u64, extra: &str, data: &[u8]) {
        self.offsets.push((number, self.buf.len() as u64));
        self.text(&format!(
            "{number} 0 obj\n<< /Length {}{extra} >>\nstream\n",
            data.len()
        ));
        self.raw(data);
        self.text("\nendstream\nendobj\n");
    }
    fn offset_of(&self, number: u64) -> u64 {
        self.offsets
            .iter()
            .find(|&&(n, _)| n == number)
            .map(|&(_, off)| off)
            .unwrap()
    }
    fn classic_trailer(&mut self, size: u64, extra: &str) {
        let xref = self.buf.len() as u64;
        self.text(&format!("xref\n0 {size}\n"));
        self.raw(b"0000000000 65535 f \n");
        for number in 1..size {
            let off = self.offset_of(number);
            self.text(&format!("{off:010} 00000 n \n"));
        }
        self.text(&format!(
            "trailer\n<< /Size {size}{extra} >>\nstartxref\n{xref}\n%%EOF\n"
        ));
    }
}

fn pdf_source() -> Vec<u8> {
    // The PDF carries the same logical report as flat text runs; it has no
    // structural coordinates, so only text/search-match are common.
    let mut content = String::from("BT /F1 12 Tf 72 720 Td\n");
    content.push_str(&format!("({TITLE}) Tj T*\n"));
    content.push_str(&format!("({}) Tj T*\n", HEADINGS[0]));
    content.push_str(&format!("({}) Tj T*\n", PARAGRAPHS[0]));
    content.push_str(&format!("({}) Tj T*\n", HEADINGS[1]));
    content.push_str(&format!("({}) Tj T*\n", PARAGRAPHS[1]));
    content.push_str(&format!("({}) Tj T*\n", HEADINGS[2]));
    for item in LIST_ITEMS {
        content.push_str(&format!("({item}) Tj T*\n"));
    }
    content.push_str(&format!("(Key: golf Value: {B7}) Tj T*\n"));
    content.push_str(&format!("(Link: {LINK_HREF}) Tj\n"));
    content.push_str("ET\n");

    let mut w = PdfBuilder::new();
    w.text("%PDF-1.5\n");
    w.obj(1, b"<< /Type /Catalog /Pages 2 0 R >>");
    w.obj(2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.obj(
        3,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
    );
    w.stream_obj(4, "", content.as_bytes());
    w.obj(5, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
    w.classic_trailer(6, " /Root 1 0 R");
    w.buf
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn opaque_descriptor(source: &[u8]) -> Vec<u8> {
    let d = Descriptor {
        universe: vole_document::container::UNIVERSE.to_string(),
        source_format: vole_document::SOURCE_FORMAT_OPAQUE,
        format_basis: "opaque:phase12-triplet".to_string(),
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
        "vole-phase12-{label}-{}-{}",
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
    field: FieldId,
    source: Vec<u8>,
    format: DocumentFormat,
    preserve: bool,
}

impl Fixture {
    fn new(label: &str, source: Vec<u8>) -> Fixture {
        let root = temp_dir(label);
        let format = detect_document_format(&source, Limits::DEFAULT);
        let mut store = FieldStore::open(&root).unwrap();
        let field = match ingest(&mut store, &opaque_descriptor(&source), Limits::DEFAULT).unwrap()
        {
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
            preserve: false,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if !self.preserve {
            std::fs::remove_dir_all(&self.root).ok();
        }
    }
}

fn observe_answer(
    store: &mut FieldStore,
    field: &FieldId,
    selector: Selector,
    representation: Representation,
) -> FieldAnswer {
    let req = ObserveRequest::new(selector, representation);
    let (answer, _stats, _) = observe(store, field, &req, Limits::DEFAULT).unwrap();
    answer
}

fn text(answer: &FieldAnswer) -> String {
    match &answer.value {
        AnswerValue::Text(t) => t.clone(),
        other => panic!("expected text, got {other:?}"),
    }
}

fn json(answer: &FieldAnswer) -> String {
    match &answer.value {
        AnswerValue::Json(j) => j.clone(),
        other => panic!("expected json, got {other:?}"),
    }
}

fn assert_native_provenance(answer: &FieldAnswer, fmt: &str) {
    let p = &answer.provenance;
    assert!(
        p.starts_with(&format!("format={fmt};common;")),
        "provenance does not carry the common format tag: {p}"
    );
    assert!(
        p.contains(&format!("{fmt};")),
        "provenance does not name the native adapter: {p}"
    );
}

// ---------------------------------------------------------------------------
// 12.9 — cross-format equivalence
// ---------------------------------------------------------------------------

#[test]
fn triplet_common_observations_match_ground_truth() {
    let mut docx = Fixture::new("triplet-docx", docx_source());
    let mut epub = Fixture::new("triplet-epub", epub_source());
    let mut pdf = Fixture::new("triplet-pdf", pdf_source());
    assert_eq!(docx.format, DocumentFormat::Docx);
    assert_eq!(epub.format, DocumentFormat::Epub);
    assert_eq!(pdf.format, DocumentFormat::Pdf);

    // --- find "<marker>" on all three (SearchMatch) ------------------------
    for (fx, fmt) in [(&mut docx, "docx"), (&mut epub, "epub"), (&mut pdf, "pdf")] {
        for marker in MARKERS {
            let a = observe_answer(
                &mut fx.store,
                &fx.field,
                Selector::SearchMatch(marker.to_string()),
                Representation::Text,
            );
            assert!(
                json(&a).contains(marker),
                "{fmt} find {marker} missed: {}",
                json(&a)
            );
            assert_native_provenance(&a, fmt);
        }
    }

    // --- whole-document text on all three ----------------------------------
    for (fx, fmt) in [(&mut docx, "docx"), (&mut epub, "epub"), (&mut pdf, "pdf")] {
        let a = observe_answer(
            &mut fx.store,
            &fx.field,
            Selector::Text,
            Representation::Text,
        );
        let t = text(&a);
        for h in HEADINGS {
            assert!(t.contains(h), "{fmt} text missing heading {h}: {t}");
        }
        for p in PARAGRAPHS {
            assert!(t.contains(p), "{fmt} text missing paragraph {p}: {t}");
        }
        assert!(t.contains("Third step"), "{fmt} text missing list: {t}");
        assert!(t.contains(B7), "{fmt} text missing B7: {t}");
        assert_native_provenance(&a, fmt);
    }

    // --- DOCX/EPUB: heading, block, table/cell, link, resource -------------
    for (fx, fmt) in [(&mut docx, "docx"), (&mut epub, "epub")] {
        for (i, expected) in HEADINGS.iter().enumerate() {
            let a = observe_answer(
                &mut fx.store,
                &fx.field,
                Selector::Heading(i as u32),
                Representation::Text,
            );
            assert_eq!(&text(&a), expected, "{fmt} heading {i}");
            assert_native_provenance(&a, fmt);
        }

        let a = observe_answer(
            &mut fx.store,
            &fx.field,
            Selector::Block(0),
            Representation::Text,
        );
        assert_eq!(text(&a), HEADINGS[0], "{fmt} block 0");

        let a = observe_answer(
            &mut fx.store,
            &fx.field,
            Selector::Table(0),
            Representation::Text,
        );
        assert!(text(&a).contains(B7), "{fmt} table 0: {}", text(&a));

        let a = observe_answer(
            &mut fx.store,
            &fx.field,
            Selector::Cell {
                table: 0,
                row: 6,
                col: 1,
            },
            Representation::Text,
        );
        assert_eq!(text(&a), B7, "{fmt} cell B7");
        assert_eq!(a.basis, Basis::DeterministicallyDerived);
        assert!(!a.exact);
        assert_native_provenance(&a, fmt);

        let a = observe_answer(
            &mut fx.store,
            &fx.field,
            Selector::Link(0),
            Representation::Metadata,
        );
        assert!(
            json(&a).contains(LINK_TEXT),
            "{fmt} link text: {}",
            json(&a)
        );

        let a = observe_answer(
            &mut fx.store,
            &fx.field,
            Selector::Resource(0),
            Representation::Metadata,
        );
        let j = json(&a);
        if fmt == "docx" {
            assert!(j.contains("rIdImg"), "docx resource rel: {j}");
        } else {
            assert!(j.contains("images/image1.png"), "epub resource href: {j}");
            assert!(j.contains("image/png"), "epub resource media type: {j}");
        }
        assert_native_provenance(&a, fmt);
    }

    // EPUB serves the resource *bytes*; they are the one shared image.
    let a = observe_answer(
        &mut epub.store,
        &epub.field,
        Selector::Resource(0),
        Representation::DecodedBytes,
    );
    match &a.value {
        AnswerValue::Bytes(b) => assert_eq!(b, &image_bytes()),
        other => panic!("expected resource bytes, got {other:?}"),
    }

    // --- PDF metadata is the structural descriptor (native), not a title ---
    let a = observe_answer(
        &mut pdf.store,
        &pdf.field,
        Selector::Metadata,
        Representation::Metadata,
    );
    let j = json(&a);
    assert!(j.contains("\"source_len\":"), "{j}");
    assert!(j.contains(&vole_document::integrity::to_hex(
        &vole_document::integrity::sha256(&pdf.source)
    )));
    assert_native_provenance(&a, "pdf");
}

#[test]
fn pdf_declines_the_structural_vocabulary_typed() {
    let mut pdf = Fixture::new("decline-pdf", pdf_source());
    let selectors = [
        Selector::Heading(0),
        Selector::Block(0),
        Selector::Table(0),
        Selector::Cell {
            table: 0,
            row: 6,
            col: 1,
        },
        Selector::Resource(0),
        Selector::Link(0),
    ];
    for sel in selectors {
        let req = ObserveRequest::new(sel.clone(), Representation::Text);
        let err = observe(&mut pdf.store, &pdf.field, &req, Limits::DEFAULT).unwrap_err();
        assert_eq!(
            err.class(),
            ErrorClass::UnsupportedFeature,
            "selector {} did not decline typed",
            sel.canonical()
        );
    }
    // Exactness is untouched by the declined observations.
    let field = Field::open(&pdf.store, &pdf.field, Limits::DEFAULT).unwrap();
    assert_eq!(
        field.materialize_exact(Limits::DEFAULT).unwrap(),
        pdf.source
    );
}

// ---------------------------------------------------------------------------
// 12.10 — source removal / restart
// ---------------------------------------------------------------------------

fn assert_exact_rematerialization(label: &str, source: &[u8], out: &[u8]) {
    assert_eq!(out.len(), source.len(), "{label}: length");
    assert_eq!(
        vole_document::integrity::sha256(out),
        vole_document::integrity::sha256(source),
        "{label}: sha256"
    );
    assert_eq!(out, source, "{label}: bytes");
}

#[test]
fn source_removal_and_restart_is_byte_exact() {
    for (label, source, fmt) in [
        ("restart-pdf", pdf_source(), DocumentFormat::Pdf),
        ("restart-docx", docx_source(), DocumentFormat::Docx),
        ("restart-epub", epub_source(), DocumentFormat::Epub),
    ] {
        // Ingest, then drop the store (simulating process exit). The source Vec
        // is the only copy the caller held; the store is self-contained.
        let fx = Fixture::new(label, source.clone());
        assert_eq!(fx.format, fmt);
        let root = fx.root.clone();
        let field_id = fx.field;
        // Keep the store on disk across the simulated process boundary.
        let mut fx = fx;
        fx.preserve = true;
        drop(fx); // closes the store

        // A fresh store handle, a fresh observation and exact materialization.
        let mut store = FieldStore::open(&root).unwrap();
        let a = observe_answer(&mut store, &field_id, Selector::Text, Representation::Text);
        assert!(
            text(&a).contains(MARKERS[0]),
            "{label}: query after restart"
        );
        let field = Field::open(&store, &field_id, Limits::DEFAULT).unwrap();
        let out = field.materialize_exact(Limits::DEFAULT).unwrap();
        assert_exact_rematerialization(label, &source, &out);
        drop(store);
        std::fs::remove_dir_all(&root).ok();
    }
}

/// Extract a top-level `"key":"value"` JSON string from a small CLI response.
fn extract_json_string(s: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = s.find(&needle)? + needle.len();
    let rest = &s[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

#[test]
fn source_removal_and_restart_child_process_is_byte_exact() {
    let bin = env!("CARGO_BIN_EXE_vole-document");
    for (fmt, source) in [
        ("pdf", pdf_source()),
        ("docx", docx_source()),
        ("epub", epub_source()),
    ] {
        let dir = temp_dir(&format!("cli-{fmt}"));
        let src_path = dir.join(format!("source.{fmt}"));
        let descriptor_path = dir.join("descriptor.voldoc");
        let store_dir = dir.join("store");
        let out_path = dir.join("rematerialized");

        std::fs::write(&src_path, &source).unwrap();
        std::fs::write(&descriptor_path, opaque_descriptor(&source)).unwrap();
        let oracle = vole_document::integrity::sha256(&source);

        // Ingest in one process.
        let out = Command::new(bin)
            .args(["field-ingest"])
            .arg(&descriptor_path)
            .arg("--store")
            .arg(&store_dir)
            .output()
            .expect("spawn field-ingest");
        assert!(
            out.status.success(),
            "{fmt} field-ingest failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        let field = extract_json_string(&stdout, "field")
            .unwrap_or_else(|| panic!("{fmt}: no field id in {stdout}"));

        // Delete the source and the ingest descriptor; only the store remains.
        std::fs::remove_file(&src_path).unwrap();
        std::fs::remove_file(&descriptor_path).unwrap();
        assert!(!src_path.exists() && !descriptor_path.exists());

        // Query in a fresh process.
        let out = Command::new(bin)
            .args(["find", "--store"])
            .arg(&store_dir)
            .args(["--field", &field, "--text", MARKERS[0]])
            .output()
            .expect("spawn find");
        assert!(out.status.success(), "{fmt} find failed");
        assert!(
            String::from_utf8_lossy(&out.stdout).contains(MARKERS[0]),
            "{fmt} find missed the marker"
        );

        // Materialize exact in another fresh process, then length+SHA-256+bytes.
        let out = Command::new(bin)
            .args(["materialize", "--store"])
            .arg(&store_dir)
            .args(["--field", &field, "--exact", "--output"])
            .arg(&out_path)
            .output()
            .expect("spawn materialize");
        assert!(
            out.status.success(),
            "{fmt} materialize failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let got = std::fs::read(&out_path).unwrap();
        assert_eq!(got.len(), source.len(), "{fmt}: length");
        assert_eq!(
            vole_document::integrity::sha256(&got),
            oracle,
            "{fmt}: sha256"
        );
        assert_eq!(got, source, "{fmt}: bytes");
        std::fs::remove_dir_all(&dir).ok();
    }
}
