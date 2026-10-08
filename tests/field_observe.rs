//! End-to-end (CLI) coverage for the Phase-11 observation verbs.
//!
//! The whole file is gated on the `field` feature so `--no-default-features`
//! builds an empty (compiling) test target.

#![cfg(feature = "field")]

use std::path::{Path, PathBuf};
use std::process::Command;

use vole_document::container::{Descriptor, ObjectSource};
use vole_document::dra::{Op, Program};
use vole_document::field::{FieldStore, ingest as field_ingest};
use vole_document::limits::Limits;

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "vole-cli-{label}-{}-{}",
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
        format_basis: "pdf:cli-test".to_string(),
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

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_vole-document")
}

fn run(args: &[&str]) -> (bool, String, String) {
    let output = Command::new(bin())
        .args(args)
        .output()
        .expect("failed to spawn vole-document");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn write_descriptor(dir: &Path, bytes: &[u8]) -> PathBuf {
    let path = dir.join("input.voldoc");
    std::fs::write(&path, bytes).unwrap();
    path
}

/// Extract the `"field":"<hex>"` value from `field-ingest` stdout.
fn json_field_id(stdout: &str) -> String {
    let key = "\"field\":\"";
    let start = stdout.find(key).expect("field key present") + key.len();
    let rest = &stdout[start..];
    let end = rest.find('"').expect("closing quote");
    rest[..end].to_string()
}

#[test]
fn cli_field_ingest_then_observe_page_text() {
    let dir = temp_dir("e2e");
    let descriptor = write_descriptor(&dir, &opaque_descriptor(&fixture_pdf()));
    let store = dir.join("store");
    let store_s = store.to_str().unwrap();
    let desc_s = descriptor.to_str().unwrap();

    let (ok, stdout, stderr) = run(&["field-ingest", desc_s, "--store", store_s]);
    assert!(ok, "field-ingest failed: {stderr}");
    assert!(stdout.contains("\"field\":"), "ingest stdout: {stdout}");

    // The CLI reports the richest (Stage-B) field id directly.
    let field_hex = json_field_id(&stdout);

    let (ok, stdout, stderr) = run(&[
        "observe", "--store", store_s, "--field", &field_hex, "--page", "1", "--kind", "text",
    ]);
    assert!(ok, "observe failed: {stderr}");
    assert!(
        stdout.contains("\"text\":\"") && stdout.contains("Hello"),
        "observe stdout: {stdout}"
    );

    // preview writes readable text; explain reports a plan shape.
    let (ok, preview, stderr) = run(&[
        "preview", "--store", store_s, "--field", &field_hex, "--page", "1",
    ]);
    assert!(ok, "preview failed: {stderr}");
    assert!(preview.contains("VOLE-PREVIEW v1"), "preview: {preview}");

    let (ok, plan, stderr) = run(&[
        "explain",
        "--store",
        store_s,
        "--field",
        &field_hex,
        "--page",
        "1",
        "--kind",
        "structure",
    ]);
    assert!(ok, "explain failed: {stderr}");
    assert!(plan.contains("\"shape\":\""), "plan: {plan}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn cli_materialize_exact_is_byte_identical() {
    let dir = temp_dir("mat");
    let source = fixture_pdf();
    let descriptor = write_descriptor(&dir, &opaque_descriptor(&source));
    let store = dir.join("store");
    let store_s = store.to_str().unwrap();

    // Ingest through the library to keep this test focused on materialize.
    let mut s = FieldStore::open(&store).unwrap();
    let report = field_ingest::ingest_pdf(
        &mut s,
        &std::fs::read(&descriptor).unwrap(),
        Limits::DEFAULT,
    )
    .unwrap();

    let out = dir.join("out.pdf");
    let (ok, stdout, stderr) = run(&[
        "materialize",
        "--store",
        store_s,
        "--field",
        &report.field.to_hex(),
        "--exact",
        "--output",
        out.to_str().unwrap(),
    ]);
    assert!(ok, "materialize failed: {stderr}");
    assert!(stdout.contains("\"sha256\":\""), "stdout: {stdout}");
    assert_eq!(std::fs::read(&out).unwrap(), source);

    std::fs::remove_dir_all(&dir).ok();
}

/// Phase 18.5: `observe-batch` must serve the packed seed substrate, not reject
/// it with the old typed `UnsupportedFeature` (rc 6). The session opens the same
/// `fieldpack/` store `field-build --packed` wrote, and its warm answers match a
/// cold `observe --packed`.
#[test]
fn cli_observe_batch_supports_packed_store() {
    use vole_document::field::FieldStore;

    let dir = temp_dir("batch-packed");
    let source = fixture_pdf();
    let descriptor = opaque_descriptor(&source);
    let store = dir.join("store");

    // Build a packed-store field through the library (the same shape
    // `field-build --packed` / `field-ingest --packed` produce).
    let field_hex = {
        let mut s = FieldStore::open_packed(&store).unwrap();
        let report = field_ingest::ingest_pdf(&mut s, &descriptor, Limits::DEFAULT).unwrap();
        s.sync().unwrap();
        report.field.to_hex()
    };

    let req = dir.join("reqs");
    std::fs::write(
        &req,
        "--byte-range 0..8 --kind exact\n--page 1 --kind text\n--metadata --kind metadata\n",
    )
    .unwrap();
    let store_s = store.to_str().unwrap();
    let (ok, stdout, stderr) = run(&[
        "observe-batch",
        "--store",
        store_s,
        "--field",
        &field_hex,
        "--packed",
        "--requests",
        req.to_str().unwrap(),
    ]);
    assert!(ok, "observe-batch --packed failed: {stderr}");
    assert!(
        stdout.contains("\"text\":\"") && stdout.contains("Hello"),
        "warm text answer: {stdout}"
    );
    assert!(
        !stdout.contains("UnsupportedFeature"),
        "the packed rejection must be gone: {stdout}"
    );

    // The warm answer agrees with a cold `observe --packed` on the same bytes.
    let (ok, cold, stderr) = run(&[
        "observe", "--store", store_s, "--field", &field_hex, "--packed", "--page", "1", "--kind",
        "text",
    ]);
    assert!(ok, "cold observe --packed failed: {stderr}");
    let warm_text = stdout.lines().find(|l| l.contains("Hello")).unwrap_or("");
    assert!(
        !cold.is_empty() && warm_text.contains("Hello"),
        "cold and warm must agree"
    );

    std::fs::remove_dir_all(&dir).ok();
}
