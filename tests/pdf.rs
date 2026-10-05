//! Phase 3.6 court: a deterministic, byte-exact PDF corpus and the acceptance
//! gates it must satisfy.
//!
//! Every builder writes real bytes with a correct direct `/Length` and correct
//! classic cross-reference offsets *by construction*, so the corpus exercises the
//! physical scanner on inputs whose ground truth is known exactly. The courts
//! assert the predeclared Phase-3 gates:
//!
//! 1. Coverage — the span partition is exactly `[0, len)`.
//! 2. Exactness — forced physical materialization reproduces the source bytes and
//!    its SHA-256 (the authoritative triple).
//! 3. Conservative fallback — non-PDFs and malformed PDFs decline to the opaque
//!    floor without inventing bytes.
//! 4. Hostile-safe — arbitrary bytes never panic and never violate the cover.
//!
//! The literal PDF lane is expected to lose the complete-cost court to RAW in
//! Phase 3; that outcome is recorded, not hidden.

use vole_document::adapter::pdf::{LengthSource, ObjRole, PhysicalKind, detect, propose_pdf, scan};
use vole_document::container::Descriptor;
use vole_document::encode;
use vole_document::encode::candidates::CandidateKind;
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::materialize;

const DEFAULT: Limits = Limits::DEFAULT;

// ---------------------------------------------------------------------------
// Deterministic PRNG (xorshift64), matching the pattern used in tests/exact.rs.
// ---------------------------------------------------------------------------

fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

// ---------------------------------------------------------------------------
// Byte-exact PDF builders.
// ---------------------------------------------------------------------------

/// A classic `xref`-table entry: 10-digit offset, generation 0, in-use, `LF`.
fn xref_entry(offset: u64) -> String {
    format!("{offset:010} 00000 n \n")
}

/// A classic `xref`-table entry terminated with `CRLF`.
fn xref_entry_crlf(offset: u64) -> String {
    format!("{offset:010} 00000 n \r\n")
}

/// `%PDF-1.7`, a binary marker, three objects (one a stored stream behind a
/// Flate-like filter), a classic `xref` table with correct offsets, `trailer`,
/// a correct `startxref`, and `%%EOF`.
fn classic_pdf() -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"%PDF-1.7\n");
    out.extend_from_slice(b"%\xe2\xe3\xcf\xd3\n");

    let obj1 = out.len() as u64;
    out.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let obj2 = out.len() as u64;
    out.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let payload: &[u8] = b"BT /F1 12 Tf (Hello, PDF) Tj ET";
    let n = payload.len();
    let obj3 = out.len() as u64;
    out.extend_from_slice(
        format!("3 0 obj\n<< /Length {n} /Filter /FlateDecode >>\nstream\n").as_bytes(),
    );
    out.extend_from_slice(payload);
    out.extend_from_slice(b"\nendstream\nendobj\n");

    let xref = out.len() as u64;
    out.extend_from_slice(b"xref\n0 4\n");
    out.extend_from_slice(b"0000000000 65535 f \n");
    out.extend_from_slice(xref_entry(obj1).as_bytes());
    out.extend_from_slice(xref_entry(obj2).as_bytes());
    out.extend_from_slice(xref_entry(obj3).as_bytes());
    out.extend_from_slice(b"trailer\n<< /Size 4 /Root 1 0 R >>\n");
    out.extend_from_slice(format!("startxref\n{xref}\n").as_bytes());
    out.extend_from_slice(b"%%EOF\n");
    out
}

/// Objects plus a `/Type /XRef` cross-reference stream; `startxref` points at it.
fn xref_stream_pdf() -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"%PDF-1.5\n");
    out.extend_from_slice(b"%\xe2\xe3\xcf\xd3\n");

    out.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let payload: &[u8] = b"\x00\x00\x01\x00\x02\x00\x00";
    let n = payload.len();
    let obj2 = out.len() as u64;
    out.extend_from_slice(
        format!("2 0 obj\n<< /Type /XRef /Length {n} /Size 3 /W [1 2 1] /Root 1 0 R >>\nstream\n")
            .as_bytes(),
    );
    out.extend_from_slice(payload);
    out.extend_from_slice(b"\nendstream\nendobj\n");

    out.extend_from_slice(format!("startxref\n{obj2}\n").as_bytes());
    out.extend_from_slice(b"%%EOF\n");
    out
}

/// Objects including a `/Type /ObjStm` object stream, wrapped in a classic
/// `xref`/`trailer`/`startxref`/`%%EOF` tail.
fn objstm_pdf() -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"%PDF-1.5\n");
    out.extend_from_slice(b"%\xe2\xe3\xcf\xd3\n");

    let obj1 = out.len() as u64;
    out.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let payload: &[u8] = b"3 0 4 0";
    let n = payload.len();
    let obj2 = out.len() as u64;
    out.extend_from_slice(
        format!("2 0 obj\n<< /Type /ObjStm /Length {n} /N 1 /First 3 >>\nstream\n").as_bytes(),
    );
    out.extend_from_slice(payload);
    out.extend_from_slice(b"\nendstream\nendobj\n");

    let xref = out.len() as u64;
    out.extend_from_slice(b"xref\n0 3\n");
    out.extend_from_slice(b"0000000000 65535 f \n");
    out.extend_from_slice(xref_entry(obj1).as_bytes());
    out.extend_from_slice(xref_entry(obj2).as_bytes());
    out.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
    out.extend_from_slice(format!("startxref\n{xref}\n").as_bytes());
    out.extend_from_slice(b"%%EOF\n");
    out
}

/// A base revision terminated by `%%EOF`, followed by an appended update: a
/// modified object, its own `xref`, a `trailer` with `/Prev`, `startxref`, `%%EOF`.
fn incremental_pdf() -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();

    // --- Revision 1 ---
    out.extend_from_slice(b"%PDF-1.4\n");
    out.extend_from_slice(b"%\xe2\xe3\xcf\xd3\n");
    let obj1 = out.len() as u64;
    out.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let obj2 = out.len() as u64;
    out.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Count 0 >>\nendobj\n");
    let x1 = out.len() as u64;
    out.extend_from_slice(b"xref\n0 3\n");
    out.extend_from_slice(b"0000000000 65535 f \n");
    out.extend_from_slice(xref_entry(obj1).as_bytes());
    out.extend_from_slice(xref_entry(obj2).as_bytes());
    out.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
    out.extend_from_slice(format!("startxref\n{x1}\n").as_bytes());
    out.extend_from_slice(b"%%EOF\n");

    // --- Revision 2 (incremental update) ---
    let obj2b = out.len() as u64;
    out.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Count 1 >>\nendobj\n");
    let x2 = out.len() as u64;
    out.extend_from_slice(b"xref\n0 3\n");
    out.extend_from_slice(b"0000000000 65535 f \n");
    out.extend_from_slice(xref_entry(obj1).as_bytes());
    out.extend_from_slice(xref_entry(obj2b).as_bytes());
    out.extend_from_slice(format!("trailer\n<< /Size 3 /Prev {x1} /Root 1 0 R >>\n").as_bytes());
    out.extend_from_slice(format!("startxref\n{x2}\n").as_bytes());
    out.extend_from_slice(b"%%EOF\n");
    out
}

/// A PDF mixing `CRLF` and `LF` line endings across structural constructs.
fn mixed_eol_pdf() -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"%PDF-1.6\r\n");
    out.extend_from_slice(b"%\xe2\xe3\xcf\xd3\r\n");

    let obj1 = out.len() as u64;
    out.extend_from_slice(b"1 0 obj\r\n<< /Type /Catalog >>\nendobj\r\n");

    let payload: &[u8] = b"mixed-eol-payload";
    let n = payload.len();
    let obj2 = out.len() as u64;
    out.extend_from_slice(format!("2 0 obj\r\n<< /Length {n} >>\nstream\r\n").as_bytes());
    out.extend_from_slice(payload);
    out.extend_from_slice(b"\nendstream\r\nendobj\n");

    let xref = out.len() as u64;
    out.extend_from_slice(b"xref\r\n0 3\r\n");
    out.extend_from_slice(b"0000000000 65535 f \r\n");
    out.extend_from_slice(xref_entry_crlf(obj1).as_bytes());
    out.extend_from_slice(xref_entry_crlf(obj2).as_bytes());
    out.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\r\n");
    out.extend_from_slice(format!("startxref\n{xref}\r\n").as_bytes());
    out.extend_from_slice(b"%%EOF\n");
    out
}

/// A literal string containing `endobj`, and a stream whose opaque payload
/// contains the bytes `endobj` and `stream`, behind a correct direct `/Length`.
fn string_trap_pdf() -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"%PDF-1.7\n");
    out.extend_from_slice(b"%\xe2\xe3\xcf\xd3\n");

    let obj1 = out.len() as u64;
    out.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Trap (trap endobj inside) >>\nendobj\n");

    let payload: &[u8] = b"payload endobj stream here";
    let n = payload.len();
    let obj2 = out.len() as u64;
    out.extend_from_slice(format!("2 0 obj\n<< /Length {n} >>\nstream\n").as_bytes());
    out.extend_from_slice(payload);
    out.extend_from_slice(b"\nendstream\nendobj\n");

    let xref = out.len() as u64;
    out.extend_from_slice(b"xref\n0 3\n");
    out.extend_from_slice(b"0000000000 65535 f \n");
    out.extend_from_slice(xref_entry(obj1).as_bytes());
    out.extend_from_slice(xref_entry(obj2).as_bytes());
    out.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
    out.extend_from_slice(format!("startxref\n{xref}\n").as_bytes());
    out.extend_from_slice(b"%%EOF\n");
    out
}

/// A truncated object: header and body present, but no `endobj` and no `%%EOF`.
fn malformed_pdf() -> Vec<u8> {
    b"%PDF-1.7\n1 0 obj\n<< /Type /Catalog >>\n".to_vec()
}

/// Deterministic non-PDF bytes. The prefix guarantees no `%PDF-` header.
fn not_a_pdf() -> Vec<u8> {
    let mut state: u64 = 0xC0FF_EE12_3456_789A;
    let mut out: Vec<u8> = Vec::with_capacity(256);
    out.extend_from_slice(b"NOTAPDF!");
    while out.len() < 256 {
        out.push((xorshift64(&mut state) & 0xff) as u8);
    }
    out
}

/// The valid corpus, in deterministic order.
fn valid_corpus() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("classic", classic_pdf()),
        ("xref_stream", xref_stream_pdf()),
        ("objstm", objstm_pdf()),
        ("incremental", incremental_pdf()),
        ("mixed_eol", mixed_eol_pdf()),
        ("string_trap", string_trap_pdf()),
    ]
}

// ---------------------------------------------------------------------------
// Gate 1: total coverage.
// ---------------------------------------------------------------------------

#[test]
fn coverage_is_total_for_every_pdf() {
    for (name, bytes) in valid_corpus() {
        let p = scan(&bytes, DEFAULT).unwrap_or_else(|e| panic!("[{name}] scan failed: {e}"));
        p.validate(bytes.len() as u64)
            .unwrap_or_else(|e| panic!("[{name}] cover invalid: {e}"));
        assert_eq!(p.total_len(), bytes.len() as u64, "[{name}] total_len");
        assert!(!p.spans.is_empty(), "[{name}] empty cover");
        assert!(p.header.is_some(), "[{name}] header missing");
        assert!(
            p.spans.iter().any(|s| s.kind == PhysicalKind::Header),
            "[{name}] no Header span"
        );
        assert!(!p.objects.is_empty(), "[{name}] no objects");
        assert!(!p.eofs.is_empty(), "[{name}] no %%EOF");
        assert!(
            p.spans.iter().any(|s| s.kind == PhysicalKind::Eof),
            "[{name}] no Eof span"
        );
    }

    // Structural roles are recognised.
    let xref = xref_stream_pdf();
    let p = scan(&xref, DEFAULT).unwrap();
    assert!(
        p.objects.iter().any(|o| o.role == ObjRole::XRefStream),
        "xref stream role"
    );

    let objstm = objstm_pdf();
    let p = scan(&objstm, DEFAULT).unwrap();
    assert!(
        p.objects.iter().any(|o| o.role == ObjRole::ObjectStream),
        "object stream role"
    );

    // Malformed PDF: conservative, but still a valid, total cover. Nothing is
    // invented: the truncated object is not admitted as a complete object.
    let bytes = malformed_pdf();
    let p = scan(&bytes, DEFAULT).unwrap();
    p.validate(bytes.len() as u64).unwrap();
    assert_eq!(p.total_len(), bytes.len() as u64);
    assert!(!p.spans.is_empty());
    assert!(p.header.is_some());
    assert!(
        p.objects.is_empty(),
        "truncated object must not be admitted"
    );
    assert!(p.eofs.is_empty(), "no %%EOF was present");

    // Non-PDF random bytes: still a valid, total cover, no header invented.
    let bytes = not_a_pdf();
    let p = scan(&bytes, DEFAULT).unwrap();
    p.validate(bytes.len() as u64).unwrap();
    assert_eq!(p.total_len(), bytes.len() as u64);
    assert!(!p.spans.is_empty());
    assert!(p.header.is_none());
}

// ---------------------------------------------------------------------------
// Gate 2: forced physical materialization is byte-exact.
// ---------------------------------------------------------------------------

#[test]
fn every_pdf_materializes_exactly_via_physical_candidate() {
    for (name, bytes) in valid_corpus() {
        let cand = propose_pdf(&bytes, DEFAULT)
            .unwrap_or_else(|e| panic!("[{name}] propose failed: {e}"))
            .unwrap_or_else(|| panic!("[{name}] expected a PDF candidate"));
        assert_eq!(
            cand.kind,
            CandidateKind::PdfPhysical,
            "[{name}] candidate kind"
        );

        let (serialized, cost) = cand.descriptor.serialize().unwrap();
        assert_eq!(cost.total(), serialized.len() as u64, "[{name}] cost");

        // The serialized descriptor must parse back cleanly.
        let parsed = Descriptor::parse(&serialized, DEFAULT)
            .unwrap_or_else(|e| panic!("[{name}] parse failed: {e}"));
        assert_eq!(
            parsed.descriptor.source_len,
            bytes.len() as u64,
            "[{name}] len"
        );
        assert_eq!(
            parsed.descriptor.source_sha256,
            sha256(&bytes),
            "[{name}] sha"
        );

        let (out, _) = materialize::decode_to_bytes(&serialized, DEFAULT)
            .unwrap_or_else(|e| panic!("[{name}] decode failed: {e}"));

        // The authoritative exact-profile triple.
        assert_eq!(out.len(), bytes.len(), "[{name}] materialized length");
        assert_eq!(sha256(&out), sha256(&bytes), "[{name}] sha256 mismatch");
        assert_eq!(out, bytes, "[{name}] byte compare");
    }
}

// ---------------------------------------------------------------------------
// Gate 3: detection is validated, not extension-based.
// ---------------------------------------------------------------------------

#[test]
fn detection_is_validated_not_extension_based() {
    for (name, bytes) in valid_corpus() {
        assert!(detect(&bytes, DEFAULT), "[{name}] must be detected as PDF");
    }
    assert!(
        !detect(&not_a_pdf(), DEFAULT),
        "non-PDF must not be detected"
    );
    assert!(
        !detect(&malformed_pdf(), DEFAULT),
        "malformed PDF (no endobj / no %%EOF) must not be detected"
    );
}

#[test]
fn non_pdf_falls_back() {
    assert!(
        propose_pdf(&not_a_pdf(), DEFAULT).unwrap().is_none(),
        "non-PDF must fall back to opaque"
    );
    assert!(
        propose_pdf(&malformed_pdf(), DEFAULT).unwrap().is_none(),
        "incomplete PDF must fall back to opaque"
    );
}

// ---------------------------------------------------------------------------
// Gate 4 (structure): incremental revisions.
// ---------------------------------------------------------------------------

#[test]
fn incremental_has_two_revisions() {
    let bytes = incremental_pdf();
    let p = scan(&bytes, DEFAULT).unwrap();
    p.validate(bytes.len() as u64).unwrap();

    assert_eq!(p.revisions.len(), 2);
    assert!(
        p.revisions[1].start > p.revisions[0].start,
        "revision 2 must start after revision 1"
    );
    assert!(p.revisions[0].prev.is_none(), "base revision has no /Prev");
    assert!(
        p.revisions[1].startxref.is_some(),
        "the update records a startxref"
    );

    // The update's /Prev resolves to the first cross-reference offset.
    let first_xref = bytes
        .windows(4)
        .position(|w| w == b"xref")
        .expect("xref present") as u64;
    assert_eq!(p.revisions[0].startxref, Some(first_xref));
    assert_eq!(p.revisions[1].prev, Some(first_xref));
}

// ---------------------------------------------------------------------------
// Gate 4 (structure): string / stream keyword traps.
// ---------------------------------------------------------------------------

#[test]
fn string_and_stream_traps_do_not_split_objects() {
    let bytes = string_trap_pdf();
    let p = scan(&bytes, DEFAULT).unwrap();
    p.validate(bytes.len() as u64).unwrap();

    assert_eq!(p.objects.len(), 2, "traps must not create phantom objects");
    assert_eq!(p.objects[0].number, 1);
    assert_eq!(p.objects[1].number, 2);

    assert_eq!(p.streams.len(), 1);
    let s = p.streams[0];
    assert_eq!(s.object, 2);
    assert_eq!(s.length_source, LengthSource::Direct);

    let payload: &[u8] = b"payload endobj stream here";
    assert_eq!(
        s.data_len,
        payload.len() as u64,
        "declared /Length must win"
    );
    let slice = &bytes[s.data_start as usize..(s.data_start + s.data_len) as usize];
    assert_eq!(slice, payload);
    // The trap spellings live inside the opaque payload, not as boundaries.
    assert!(slice.windows(6).any(|w| w == b"endobj"));
    assert!(slice.windows(6).any(|w| w == b"stream"));

    let datas: Vec<_> = p
        .spans
        .iter()
        .filter(|sp| sp.kind == PhysicalKind::StreamData)
        .collect();
    assert_eq!(datas.len(), 1, "exactly one stream data span");
    assert_eq!(datas[0].len, payload.len() as u64);
}

// ---------------------------------------------------------------------------
// Gate 4 (hostile-safe): arbitrary bytes never panic and never violate cover.
// ---------------------------------------------------------------------------

#[test]
fn random_bytes_never_panic() {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..1000 {
        let len = (xorshift64(&mut state) as usize) % 2049;
        let mut buf = Vec::with_capacity(len);
        for _ in 0..len {
            buf.push((xorshift64(&mut state) & 0xff) as u8);
        }

        match scan(&buf, DEFAULT) {
            Ok(p) => {
                p.validate(buf.len() as u64).unwrap();
                assert_eq!(p.total_len(), buf.len() as u64);
            }
            Err(e) => {
                // Any decline must still be a typed, classified error.
                let _ = e.class();
            }
        }

        // The proposal path must never error on arbitrary bytes.
        let _ = propose_pdf(&buf, DEFAULT).expect("propose_pdf must not error");
    }
}

// ---------------------------------------------------------------------------
// Gate 6: honest cost — RAW wins in Phase 3.
// ---------------------------------------------------------------------------

#[test]
fn court_still_prefers_raw_on_phase3() {
    // Expected Phase-3 outcome: the literal PDF candidate carries no structural
    // compression, so the complete-cost court still prefers RAW. This is
    // recorded as expected, not treated as a failure.
    let pdf = classic_pdf();
    let (bytes, report) = encode::encode(&pdf, DEFAULT).unwrap();
    let (out, _) = materialize::decode_to_bytes(&bytes, DEFAULT).unwrap();
    assert_eq!(out, pdf, "RAW outcome must still round-trip exactly");
    assert_eq!(report.kind, CandidateKind::Raw);
}
