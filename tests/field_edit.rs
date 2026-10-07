//! Immutable node-level edit witness (Phase 11.12).
//!
//! Proves on a two-page fixture that one small node-level edit produces a new
//! field root that (a) shares every unaffected selector binding and index node
//! by content id with the old root, (b) leaves the old root byte-valid and
//! exact, (c) keeps `materialize(root) == original_bytes` for **both** roots,
//! and (d) reads no descriptor bytes to make the edit.
//!
//! The whole file is gated on the `field` feature so `--no-default-features`
//! builds an empty (compiling) test target.

#![cfg(feature = "field")]

use std::path::{Path, PathBuf};

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::field::edit::{MAX_EDIT_CONTENT_BYTES, replace_page_content};
use vole_document::field::observe::{ObserveRequest, Representation, Selector, observe};
use vole_document::field::provenance::AnswerValue;
use vole_document::field::{Field, FieldId, FieldStore, ingest as field_ingest};
use vole_document::limits::Limits;

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-edit-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let chunks: Vec<&[u8]> = data.chunks(0xFFFF).collect();
    for (i, chunk) in chunks.iter().enumerate() {
        out.push(u8::from(i + 1 == chunks.len()));
        let len = chunk.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(chunk);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

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

/// A classic-xref PDF with two pages, each a lone-Flate content stream.
fn fixture_pdf() -> Vec<u8> {
    let one = zlib_stored(b"BT /F1 12 Tf 72 720 Td (Hello one) Tj ET\n");
    let two = zlib_stored(b"BT /F1 12 Tf 72 720 Td (Hello two) Tj ET\n");
    let mut w = PdfBuilder::new();
    w.text("%PDF-1.5\n");
    w.obj(1, b"<< /Type /Catalog /Pages 2 0 R >>");
    w.obj(2, b"<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>");
    w.obj(
        3,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
    );
    w.stream_obj(4, " /Filter /FlateDecode", &one);
    w.obj(5, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
    w.obj(
        6,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 7 0 R >>",
    );
    w.stream_obj(7, " /Filter /FlateDecode", &two);
    w.classic_trailer(8, " /Root 1 0 R");
    w.buf
}

fn opaque_descriptor(source: &[u8]) -> Vec<u8> {
    let d = Descriptor {
        universe: vole_document::container::UNIVERSE.to_string(),
        source_format: vole_document::SOURCE_FORMAT_PDF,
        format_basis: "pdf:edit-test".to_string(),
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

fn materialize_exact(store: &FieldStore, id: &FieldId) -> Vec<u8> {
    Field::open(store, id, Limits::DEFAULT)
        .unwrap()
        .materialize_exact(Limits::DEFAULT)
        .unwrap()
}

fn page_text(store: &mut FieldStore, id: &FieldId, page: u32) -> String {
    let req = ObserveRequest::new(Selector::Page(page), Representation::Text);
    let (answer, _stats, _field) = observe(store, id, &req, Limits::DEFAULT).unwrap();
    match answer.value {
        AnswerValue::Text(t) => t,
        other => panic!("expected text, got {other:?}"),
    }
}

fn ingest_two_pages(dir: &Path, source: &[u8]) -> (FieldStore, FieldId) {
    let descriptor = opaque_descriptor(source);
    let mut store = FieldStore::open(dir.join("store")).unwrap();
    let report = field_ingest::ingest_pdf(&mut store, &descriptor, Limits::DEFAULT).unwrap();
    assert_eq!(report.page_nodes, 2, "fixture must recover two pages");
    let id = report.field;
    (store, id)
}

#[test]
fn edit_shares_unaffected_state_and_both_roots_stay_exact() {
    let dir = temp_dir("share");
    let source = fixture_pdf();
    let (mut store, r0) = ingest_two_pages(&dir, &source);

    // R0 is exact before the edit.
    assert_eq!(materialize_exact(&store, &r0), source);
    let r0_page1 = page_text(&mut store, &r0, 1);
    let r0_page2 = page_text(&mut store, &r0, 2);
    assert!(r0_page1.contains("Hello one"), "R0 page1: {r0_page1:?}");
    assert!(r0_page2.contains("Hello two"), "R0 page2: {r0_page2:?}");

    let new_content = b"BT /F1 12 Tf 72 720 Td (Hello edited page one) Tj ET\n";
    let report = replace_page_content(&mut store, &r0, 1, new_content).unwrap();

    // The edit read no descriptor bytes and re-parsed nothing.
    assert_eq!(
        report.descriptor_bytes_read, 0,
        "edit must not read the descriptor: {report:?}"
    );
    assert_eq!(report.previous, r0);
    assert_ne!(report.field, r0, "the edit must mint a new root");

    // Two new seed nodes (the literal and its page-content node); every other
    // binding is carried forward by content id.
    assert_eq!(report.seed_nodes_new, 2, "{report:?}");
    assert!(report.index_entries >= 3, "object+page entries: {report:?}");
    assert_eq!(report.index_entries_replaced, 1, "{report:?}");
    assert!(
        report.index_entries_reused > 0,
        "unaffected bindings must be reused: {report:?}"
    );
    // This fixture has fewer than one full leaf of entries, so the whole index
    // is a single node; sharing is witnessed at *entry* level here and at
    // *node* level by `edit_shares_unchanged_index_leaves` below.

    // R0 is untouched and still exact.
    assert_eq!(materialize_exact(&store, &r0), source);
    assert_eq!(page_text(&mut store, &r0, 1), r0_page1);

    // R1 materializes the *same original* bytes exactly (the exact archive is
    // unchanged), and observes the edited page with the new content while the
    // unaffected page is byte-identical to R0.
    assert_eq!(materialize_exact(&store, &report.field), source);
    let r1_page1 = page_text(&mut store, &report.field, 1);
    let r1_page2 = page_text(&mut store, &report.field, 2);
    assert!(
        r1_page1.contains("Hello edited page one"),
        "R1 page1 must show the edit: {r1_page1:?}"
    );
    assert!(
        !r1_page1.contains("Hello one"),
        "R1 page1 must not show the old content: {r1_page1:?}"
    );
    assert_eq!(r1_page2, r0_page2, "unaffected page must be unchanged");

    std::fs::remove_dir_all(&dir).ok();
}

/// A fixture with enough physical objects that the hierarchical index needs more
/// than one leaf, so an unaffected leaf must be shared by content id.
fn fixture_pdf_many_objects(filler: u64) -> Vec<u8> {
    let one = zlib_stored(b"BT /F1 12 Tf 72 720 Td (Hello one) Tj ET\n");
    let two = zlib_stored(b"BT /F1 12 Tf 72 720 Td (Hello two) Tj ET\n");
    let mut w = PdfBuilder::new();
    w.text("%PDF-1.5\n");
    w.obj(1, b"<< /Type /Catalog /Pages 2 0 R >>");
    w.obj(2, b"<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>");
    w.obj(
        3,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
    );
    w.stream_obj(4, " /Filter /FlateDecode", &one);
    w.obj(5, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
    w.obj(
        6,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 7 0 R >>",
    );
    w.stream_obj(7, " /Filter /FlateDecode", &two);
    for n in 8..8 + filler {
        w.obj(n, format!("<< /Type /Filler /N {n} >>").as_bytes());
    }
    w.classic_trailer(8 + filler, " /Root 1 0 R");
    w.buf
}

#[test]
fn edit_shares_unchanged_index_leaves() {
    let dir = temp_dir("leaves");
    let source = fixture_pdf_many_objects(200);
    let (mut store, r0) = ingest_two_pages(&dir, &source);

    let report = replace_page_content(&mut store, &r0, 1, b"BT (leaf share) Tj ET\n").unwrap();
    assert!(
        report.index_entries > 148,
        "fixture must need a multi-leaf index: {report:?}"
    );
    assert!(
        report.index_nodes_reused > 0,
        "an unaffected index leaf must be shared by id: {report:?}"
    );
    assert_eq!(report.index_entries_replaced, 1, "{report:?}");
    assert_eq!(report.descriptor_bytes_read, 0, "{report:?}");

    assert_eq!(materialize_exact(&store, &r0), source);
    assert_eq!(materialize_exact(&store, &report.field), source);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn edit_is_content_addressed_and_idempotent() {
    let dir = temp_dir("idem");
    let source = fixture_pdf();
    let (mut store, r0) = ingest_two_pages(&dir, &source);

    let content = b"BT (first edit) Tj ET\n";
    let a = replace_page_content(&mut store, &r0, 2, content).unwrap();
    // Repeating the identical edit yields the identical root and writes nothing
    // new: every node it would create already exists by content id.
    let b = replace_page_content(&mut store, &r0, 2, content).unwrap();
    assert_eq!(a.field, b.field, "identical edits are idempotent");
    assert_eq!(b.seed_nodes_new, 0, "{b:?}");
    assert_eq!(b.seed_nodes_reused, 2, "{b:?}");
    assert_eq!(b.index_nodes_new, 0, "{b:?}");
    assert_eq!(b.bytes_newly_persisted, 0, "{b:?}");

    // Different content yields a different root.
    let c = replace_page_content(&mut store, &r0, 2, b"BT (second edit) Tj ET\n").unwrap();
    assert_ne!(c.field, a.field);

    // All three roots still materialize the original bytes exactly.
    assert_eq!(materialize_exact(&store, &r0), source);
    assert_eq!(materialize_exact(&store, &a.field), source);
    assert_eq!(materialize_exact(&store, &c.field), source);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn edit_boundaries_fail_closed() {
    let dir = temp_dir("bounds");
    let source = fixture_pdf();
    let (mut store, r0) = ingest_two_pages(&dir, &source);
    let before = store.list_fields().unwrap().len();

    // Page numbers are 1-based.
    assert!(replace_page_content(&mut store, &r0, 0, b"x").is_err());
    // A page the index does not know is a typed error, not a silent no-op.
    assert!(replace_page_content(&mut store, &r0, 99, b"x").is_err());
    // Oversized content fails before anything new is written.
    let huge = vec![b'x'; MAX_EDIT_CONTENT_BYTES + 1];
    assert!(replace_page_content(&mut store, &r0, 1, &huge).is_err());

    // No new field root was minted by the failed calls, and R0 is unharmed.
    assert_eq!(store.list_fields().unwrap().len(), before);
    assert_eq!(materialize_exact(&store, &r0), source);

    std::fs::remove_dir_all(&dir).ok();
}
