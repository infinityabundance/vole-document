//! Seek-based partial descriptor reads for field observations (Phase 11.12).
//!
//! These courts exercise review priority #2: a *narrow* observation must read
//! only the descriptor record closure it needs, not the whole `.voldoc`. They
//! cover four claims:
//!
//! * **Equivalence** — the partial path returns byte-identical answers to the
//!   full path for a byte range, an object, an encoded stream, a revision, and a
//!   page's text/structure/preview.
//! * **Accounting** — on a descriptor far larger than the closure,
//!   `descriptor_bytes_read` is a small fraction of the blob.
//! * **Integrity** — a corrupted *needed* record is a typed error; a corrupted
//!   *unneeded* record does not break a narrow observation.
//! * **Fallback** — a descriptor without an observation index still observes
//!   correctly via the full path.
//!
//! The whole file is gated on the `field` feature so `--no-default-features`
//! builds an empty (compiling) test target.

#![cfg(feature = "field")]

use std::path::PathBuf;

use vole_document::container::record::{RECORD_HEADER_LEN, RECORD_TRAILER_LEN, RecordTag};
use vole_document::container::{Descriptor, HEADER_LEN, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::field::observe::{
    DescriptorReadMode, ObserveRequest, Representation, Selector, observe, observe_with_field,
};
use vole_document::field::provenance::AnswerValue;
use vole_document::field::{Field, FieldStore, ingest as field_ingest};
use vole_document::limits::Limits;

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-partial-{label}-{}-{}",
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

/// A classic-xref PDF with one page and a lone-Flate content stream `(Hello) Tj`.
fn fixture_pdf() -> Vec<u8> {
    let content = b"BT /F1 12 Tf 72 720 Td (Hello) Tj ET\n";
    let encoded = zlib_stored(content);
    let mut w = PdfBuilder::new();
    w.text("%PDF-1.5\n");
    w.obj(1, b"<< /Type /Catalog /Pages 2 0 R >>");
    w.obj(2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.obj(
        3,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
    );
    w.stream_obj(4, " /Filter /FlateDecode", &encoded);
    w.obj(5, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
    w.classic_trailer(6, " /Root 1 0 R");
    w.buf
}

fn opaque_descriptor(source: &[u8]) -> Vec<u8> {
    let d = Descriptor {
        universe: vole_document::container::UNIVERSE.to_string(),
        source_format: vole_document::SOURCE_FORMAT_PDF,
        format_basis: "pdf:partial-test".to_string(),
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

/// A descriptor with `objects` inline objects, each `chunk` bytes, emitted in
/// order by one `EmitObject` per object. The descriptor is dominated by the
/// OBJECT records, so a narrow range's closure is a small fraction of it.
fn many_object_descriptor(objects: usize, chunk: usize) -> (Vec<u8>, Vec<u8>) {
    let chunks: Vec<Vec<u8>> = (0..objects)
        .map(|i| vec![(i * 7 + 3) as u8; chunk])
        .collect();
    let source: Vec<u8> = chunks.iter().flatten().copied().collect();
    let program = Program::new(
        (0..objects)
            .map(|i| Op::EmitObject {
                object_id: i as u32,
            })
            .collect(),
    );
    let d = Descriptor {
        universe: vole_document::container::UNIVERSE.to_string(),
        source_format: vole_document::SOURCE_FORMAT_OPAQUE,
        format_basis: "opaque:partial-test".to_string(),
        models: vec![],
        channels: vec![],
        objects: chunks.into_iter().map(ObjectSource::Inline).collect(),
        program,
        observation_index: None,
        seek_directory: false,
        source_sha256: vole_document::integrity::sha256(&source),
        source_len: source.len() as u64,
    };
    (d.serialize().unwrap().0, source)
}

fn bytes_of(answer: &vole_document::field::provenance::FieldAnswer) -> Vec<u8> {
    match &answer.value {
        AnswerValue::Bytes(b) => b.clone(),
        other => panic!("expected bytes, got {other:?}"),
    }
}

/// The partial path serves byte-identical answers to the full path.
#[test]
fn partial_path_is_byte_identical_to_the_full_path() {
    let dir = temp_dir("equiv");
    let source = fixture_pdf();
    let desc = opaque_descriptor(&source);
    let mut store = FieldStore::open(&dir).unwrap();
    let id = field_ingest::ingest_pdf(&mut store, &desc, Limits::DEFAULT)
        .unwrap()
        .field;

    let cases = [
        (
            Selector::ByteRange { offset: 5, len: 16 },
            Representation::ExactBytes,
        ),
        (Selector::Object(4), Representation::ExactBytes),
        (Selector::Stream(4), Representation::EncodedBytes),
        (Selector::Revision(0), Representation::ExactBytes),
        (Selector::Page(1), Representation::Text),
        (Selector::Page(1), Representation::Structure),
        (Selector::Page(1), Representation::Preview),
    ];
    for (selector, representation) in cases {
        let req = ObserveRequest::new(selector.clone(), representation);
        // Full path: the descriptor is parsed in full, then the same seed nodes
        // are materialized. (This is not `materialize_exact` for page text; it is
        // the full-source variant of the *same* derived projection.)
        let field = Field::open(&store, &id, Limits::DEFAULT).unwrap();
        let (full, _, _) = observe_with_field(&mut store, &field, &req, Limits::DEFAULT).unwrap();
        // Partial path: only the referenced records are read.
        let (partial, stats, _) = observe(&mut store, &id, &req, Limits::DEFAULT).unwrap();
        assert_eq!(
            full.value,
            partial.value,
            "selector {} representation {}",
            req.selector.canonical(),
            req.representation.name()
        );
        assert_eq!(
            stats.descriptor_read_mode,
            DescriptorReadMode::Partial,
            "the narrow request must take the partial lane: {stats:?}"
        );
    }

    // A whole-document observation is not partial-eligible and stays exact.
    let field = Field::open(&store, &id, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), source);

    std::fs::remove_dir_all(&dir).ok();
}

/// On a descriptor far larger than the closure, the partial read is a small
/// fraction of the file.
#[test]
fn narrow_read_is_a_small_fraction_of_a_large_descriptor() {
    let dir = temp_dir("accounting");
    let (desc, source) = many_object_descriptor(128, 16 * 1024);
    let descriptor_len = desc.len() as u64;
    let mut store = FieldStore::open(&dir).unwrap();
    let id = field_ingest::ingest_pdf(&mut store, &desc, Limits::DEFAULT)
        .unwrap()
        .field;

    // The stored (enriched) blob is at least as large as the input.
    let stored_len = std::fs::read_dir(store.root().join("descriptor"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .metadata()
        .unwrap()
        .len();

    let req = ObserveRequest::new(
        Selector::ByteRange { offset: 0, len: 64 },
        Representation::ExactBytes,
    );
    let (answer, stats, _) = observe(&mut store, &id, &req, Limits::DEFAULT).unwrap();
    assert_eq!(bytes_of(&answer), source[0..64].to_vec());
    assert_eq!(stats.descriptor_read_mode, DescriptorReadMode::Partial);
    assert!(
        stats.descriptor_bytes_read < stored_len / 20,
        "narrow read {} must be < 5% of the {stored_len}-byte descriptor (input {descriptor_len})",
        stats.descriptor_bytes_read
    );
    assert!(
        stats.descriptor_bytes_read < descriptor_len,
        "narrow read must be far below the descriptor size"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// The stored descriptor's OBJECT records, in order, as `(file_offset, payload_len)`.
///
/// This is a raw framing walk (no CRC check) so it still works after a payload
/// byte has been deliberately corrupted.
fn object_sites(bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut pos = HEADER_LEN;
    let mut out = Vec::new();
    while pos + RECORD_HEADER_LEN + RECORD_TRAILER_LEN <= bytes.len() {
        let tag = bytes[pos];
        let len = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        if tag == RecordTag::Object as u8 {
            out.push((pos, len));
        }
        pos += RECORD_HEADER_LEN + len + RECORD_TRAILER_LEN;
    }
    out
}

/// Flip one payload byte of the `index`-th OBJECT record in the stored blob.
fn corrupt_object(store: &FieldStore, index: usize) {
    let path = std::fs::read_dir(store.root().join("descriptor"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut bytes = std::fs::read(&path).unwrap();
    let sites = object_sites(&bytes);
    let (off, len) = sites[index];
    assert!(len > 0, "object must be non-empty");
    bytes[off + HEADER_LEN] ^= 0xFF;
    std::fs::write(&path, &bytes).unwrap();
}

/// Integrity trade-off: a corrupted *needed* record is a typed error; a
/// corrupted *unneeded* record does not break a narrow observation.
#[test]
fn corrupt_needed_record_errors_and_unneeded_record_is_ignored() {
    let dir = temp_dir("integrity");
    let (desc, source) = many_object_descriptor(32, 8 * 1024);
    let mut store = FieldStore::open(&dir).unwrap();
    let id = field_ingest::ingest_pdf(&mut store, &desc, Limits::DEFAULT)
        .unwrap()
        .field;

    let req = ObserveRequest::new(
        Selector::ByteRange { offset: 0, len: 32 },
        Representation::ExactBytes,
    );
    // A cold court: no derived cache may mask the record read.
    let mut cold = req.clone();
    cold.use_cache = false;

    // Object 31 is *not* referenced by the range at offset 0: its corrupted
    // payload (and CRC) is never read, so the observation still succeeds.
    corrupt_object(&store, 31);
    let (answer, stats, _) = observe(&mut store, &id, &cold, Limits::DEFAULT).unwrap();
    assert_eq!(bytes_of(&answer), source[0..32].to_vec());
    assert_eq!(stats.descriptor_read_mode, DescriptorReadMode::Partial);

    // Object 0 *is* referenced: its CRC is verified when it is materialized, so
    // the corruption is a typed container error, never wrong bytes.
    corrupt_object(&store, 0);
    let err = observe(&mut store, &id, &cold, Limits::DEFAULT).unwrap_err();
    assert_eq!(err.class(), vole_document::ErrorClass::InvalidContainer);

    std::fs::remove_dir_all(&dir).ok();
}

/// A descriptor without an observation index observes correctly via the full
/// path, and reports the full read mode honestly.
#[test]
fn descriptor_without_an_index_falls_back_to_the_full_path() {
    let dir = temp_dir("fallback");
    let source = fixture_pdf();
    let desc = opaque_descriptor(&source);
    let mut store = FieldStore::open(&dir).unwrap();
    // `FieldStore::ingest` stores the blob verbatim (no observation index is
    // added), so the seek lane is ineligible and the full path must serve.
    let id = store.ingest(&desc, Limits::DEFAULT).unwrap();

    let req = ObserveRequest::new(
        Selector::ByteRange { offset: 0, len: 8 },
        Representation::ExactBytes,
    );
    let (answer, stats, _) = observe(&mut store, &id, &req, Limits::DEFAULT).unwrap();
    assert_eq!(bytes_of(&answer), source[0..8].to_vec());
    assert_eq!(stats.descriptor_read_mode, DescriptorReadMode::Full);
    assert!(stats.descriptor_bytes_read > 0, "{stats:?}");

    std::fs::remove_dir_all(&dir).ok();
}

/// `Field::materialize_exact` never uses the partial lane: the archival path
/// reads and verifies the whole source.
#[test]
fn archival_materialize_remains_exact_after_partial_use() {
    let dir = temp_dir("archival");
    let source = fixture_pdf();
    let desc = opaque_descriptor(&source);
    let mut store = FieldStore::open(&dir).unwrap();
    let id = field_ingest::ingest_pdf(&mut store, &desc, Limits::DEFAULT)
        .unwrap()
        .field;

    let req = ObserveRequest::new(
        Selector::ByteRange { offset: 0, len: 8 },
        Representation::ExactBytes,
    );
    let _ = observe(&mut store, &id, &req, Limits::DEFAULT).unwrap();
    let field = Field::open(&store, &id, Limits::DEFAULT).unwrap();
    assert_eq!(field.materialize_exact(Limits::DEFAULT).unwrap(), source);

    std::fs::remove_dir_all(&dir).ok();
}
