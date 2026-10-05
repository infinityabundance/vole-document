//! Phase-5.3 acceptance/rejection gates for the PDF *layout* candidate.
//!
//! These tests exercise the classic-cross-reference lane (`ProposePdfLayout`),
//! which replaces literal xref offsets and `startxref` values with positions
//! marked during materialization. Two gates are deliberately adversarial:
//!
//! * `layout_bad_offset_falls_back` proves that a wrong source offset is still
//!   reproduced byte-for-byte by emitting the entry literally — exactness is
//!   mandatory even when the prediction precondition fails.
//! * `layout_not_selected_on_corpus` records the honest complete-cost outcome:
//!   for `classic.pdf` the literal lanes are cheaper, so the layout candidate
//!   must lose the court. We assert the loss and verify the real winner
//!   round-trips exactly; we never assert that layout wins.
//!
//! Every descriptor admitted here must satisfy the authoritative exact triple:
//! `materialized_length == source_length`, `SHA256(materialized) == SHA256(source)`,
//! and `byte_compare(materialized, source) == equal`.

use vole_document::ErrorClass;
use vole_document::adapter::pdf::propose_pdf_layout;
use vole_document::adapter::pdf::samples::sample_pdfs;
use vole_document::container::Descriptor;
use vole_document::dra::Op;
use vole_document::encode;
use vole_document::encode::candidates::{Candidate, CandidateKind};
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::materialize;

/// Fetch a named sample from the canonical corpus.
fn corpus(name: &str) -> Vec<u8> {
    sample_pdfs()
        .into_iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("sample {name} is missing from the corpus"))
        .1
}

/// The accepted classic-cross-reference samples exercised by the exact gate.
///
/// `classic.pdf`, `incremental.pdf`, and `bigtext.pdf` come from the shipped
/// corpus; `twopage.pdf` is not part of that corpus, so it is assembled by the
/// deterministic builder below (its offsets and `/Length`s are correct by
/// construction).
fn accepted_samples() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("classic.pdf", corpus("classic.pdf")),
        ("twopage.pdf", twopage_pdf()),
        ("incremental.pdf", corpus("incremental.pdf")),
        ("bigtext.pdf", corpus("bigtext.pdf")),
    ]
}

/// Append `N 0 obj\n<body>\nendobj\n`, recording the introducer offset.
fn push_obj(b: &mut Vec<u8>, offsets: &mut Vec<u64>, number: u64, body: &[u8]) {
    offsets.push(b.len() as u64);
    b.extend_from_slice(format!("{number} 0 obj\n").as_bytes());
    b.extend_from_slice(body);
    b.extend_from_slice(b"\nendobj\n");
}

/// Append a stream object whose `/Length` is the exact payload length.
fn push_stream_obj(b: &mut Vec<u8>, offsets: &mut Vec<u64>, number: u64, data: &[u8]) {
    offsets.push(b.len() as u64);
    b.extend_from_slice(
        format!("{number} 0 obj\n<< /Length {} >>\nstream\n", data.len()).as_bytes(),
    );
    b.extend_from_slice(data);
    b.extend_from_slice(b"\nendstream\nendobj\n");
}

/// A deterministic classic-xref, two-page PDF with six indirect objects.
fn twopage_pdf() -> Vec<u8> {
    let mut b: Vec<u8> = Vec::new();
    let mut offsets: Vec<u64> = Vec::new();
    b.extend_from_slice(b"%PDF-1.4\n");
    push_obj(
        &mut b,
        &mut offsets,
        1,
        b"<< /Type /Catalog /Pages 2 0 R >>",
    );
    push_obj(
        &mut b,
        &mut offsets,
        2,
        b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
    );
    push_obj(
        &mut b,
        &mut offsets,
        3,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 5 0 R >>",
    );
    push_obj(
        &mut b,
        &mut offsets,
        4,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 6 0 R >>",
    );
    push_stream_obj(&mut b, &mut offsets, 5, b"page one\n");
    push_stream_obj(&mut b, &mut offsets, 6, b"page two\n");

    let xref = b.len() as u64;
    b.extend_from_slice(b"xref\n0 7\n");
    b.extend_from_slice(b"0000000000 65535 f \n");
    for &off in &offsets {
        b.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    b.extend_from_slice(
        format!("trailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
    );
    b
}

/// A classic-xref PDF with `n` indirect objects and correct offsets.
fn classic_with_objects(n: usize) -> Vec<u8> {
    let mut b: Vec<u8> = Vec::new();
    b.extend_from_slice(b"%PDF-1.4\n");
    let mut offsets: Vec<u64> = Vec::with_capacity(n);
    for number in 1..=n {
        offsets.push(b.len() as u64);
        b.extend_from_slice(format!("{number} 0 obj\n<< >>\nendobj\n").as_bytes());
    }
    let xref = b.len() as u64;
    b.extend_from_slice(format!("xref\n0 {}\n", n + 1).as_bytes());
    b.extend_from_slice(b"0000000000 65535 f \n");
    for &off in &offsets {
        b.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    b.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            n + 1
        )
        .as_bytes(),
    );
    b
}

/// A classic-xref PDF whose single xref entry for object 1 is deliberately
/// wrong. Returns the bytes and the wrong 10-digit field for inspection.
fn classic_with_wrong_offset() -> (Vec<u8>, String) {
    let mut b: Vec<u8> = Vec::new();
    b.extend_from_slice(b"%PDF-1.4\n");
    let off1 = b.len() as u64;
    b.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");
    let xref = b.len() as u64;
    let wrong = format!("{:010}", off1 + 3);
    b.extend_from_slice(format!("xref\n0 2\n0000000000 65535 f \n{wrong} 00000 n \n").as_bytes());
    b.extend_from_slice(
        format!("trailer\n<< /Size 2 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
    );
    (b, wrong)
}

/// Force the layout descriptor for `src`, or panic with a clear message.
fn forced_layout(src: &[u8]) -> Candidate {
    propose_pdf_layout(src, Limits::DEFAULT)
        .unwrap_or_else(|e| panic!("propose_pdf_layout failed: {e}"))
        .unwrap_or_else(|| panic!("layout must be proposed for this input"))
}

/// Assert the authoritative exact triple for a serialized descriptor.
fn assert_exact(encoded: &[u8], src: &[u8], label: &str) {
    let (out, parsed) = materialize::decode_to_bytes(encoded, Limits::DEFAULT)
        .unwrap_or_else(|e| panic!("[{label}] decode failed: {e}"));
    assert_eq!(out.len(), src.len(), "[{label}] materialized length");
    assert_eq!(out, src, "[{label}] materialized bytes");
    assert_eq!(sha256(&out), sha256(src), "[{label}] digest");
    assert_eq!(out.len() as u64, parsed.descriptor.source_len);
}

/// Read one `key=value` integer field out of a `format_basis` string.
fn basis_field(basis: &str, key: &str) -> Option<u64> {
    basis.split(';').find_map(|part| {
        let (k, v) = part.split_once('=')?;
        (k == key).then(|| v.parse().ok()).flatten()
    })
}

/// Every accepted sample proposes a layout candidate that is byte-exact, and
/// both serialize-time and parse-time cost attribution sum to the persisted
/// length.
#[test]
fn forced_layout_is_exact() {
    for (name, src) in accepted_samples() {
        let cand = forced_layout(&src);
        assert_eq!(cand.kind, CandidateKind::PdfLayout, "[{name}] kind");
        assert_eq!(
            cand.descriptor.objects.len(),
            0,
            "[{name}] all bytes live in the graph"
        );
        assert!(cand.descriptor.models.is_empty(), "[{name}] no models");
        assert!(cand.descriptor.channels.is_empty(), "[{name}] no channels");

        let (encoded, cost) = cand.descriptor.serialize().unwrap();
        assert_eq!(
            cost.total(),
            encoded.len() as u64,
            "[{name}] serialize-time cost must sum to the serialized length"
        );
        assert_exact(&encoded, &src, name);

        let parsed = Descriptor::parse(&encoded, Limits::DEFAULT).unwrap();
        assert_eq!(
            parsed.cost.total(),
            encoded.len() as u64,
            "[{name}] parsed cost must sum to the serialized length"
        );
        assert_eq!(parsed.descriptor.source_len, src.len() as u64);
    }
}

/// `classic.pdf` must contain both kinds of offset instruction: positions are
/// marked and predicted entries are emitted.
#[test]
fn layout_predicts_entries() {
    let src = corpus("classic.pdf");
    let cand = forced_layout(&src);
    let emits = cand
        .descriptor
        .program
        .ops
        .iter()
        .filter(|op| matches!(op, Op::EmitOffset { .. }))
        .count();
    let marks = cand
        .descriptor
        .program
        .ops
        .iter()
        .filter(|op| matches!(op, Op::MarkOffset { .. }))
        .count();
    assert!(emits >= 1, "classic.pdf must emit at least one offset");
    assert!(marks >= 1, "classic.pdf must mark at least one position");
    assert!(
        basis_field(&cand.descriptor.format_basis, "xref_predicted").unwrap_or(0) >= 1,
        "the format basis must record at least one predicted xref entry"
    );
}

/// A deliberately wrong source offset must never be predicted or invented: the
/// candidate still reproduces the wrong digits literally and stays exact.
#[test]
fn layout_bad_offset_falls_back() {
    let (src, wrong) = classic_with_wrong_offset();
    let cand = forced_layout(&src);

    assert_eq!(
        basis_field(&cand.descriptor.format_basis, "xref_predicted"),
        Some(0),
        "a mismatched offset must never be predicted"
    );

    // The wrong 10-digit field survives verbatim as inline literal bytes.
    let mut inline: Vec<u8> = Vec::new();
    for op in &cand.descriptor.program.ops {
        if let Op::Inline { bytes } = op {
            inline.extend_from_slice(bytes);
        }
    }
    assert!(
        inline.windows(wrong.len()).any(|w| w == wrong.as_bytes()),
        "the wrong offset must be emitted literally"
    );

    let (encoded, _) = cand.descriptor.serialize().unwrap();
    assert_exact(&encoded, &src, "bad-offset");
}

/// The layout lane must decline precisely: cross-reference streams, non-PDFs,
/// and tables too large for the reserved slot space are all rejected.
#[test]
fn layout_declines() {
    for name in ["xrefstream.pdf", "notpdf.bin", "malformed.pdf"] {
        let src = corpus(name);
        assert!(
            propose_pdf_layout(&src, Limits::DEFAULT).unwrap().is_none(),
            "{name} must decline the layout candidate"
        );
    }

    let too_many = classic_with_objects(256);
    assert!(
        propose_pdf_layout(&too_many, Limits::DEFAULT)
            .unwrap()
            .is_none(),
        "256 objects exceeds the 255 markable slots"
    );
    // The boundary case still fits: indices 0..=254, slot 255 for the xref.
    let boundary = classic_with_objects(255);
    assert!(
        propose_pdf_layout(&boundary, Limits::DEFAULT)
            .unwrap()
            .is_some(),
        "255 objects must still be markable"
    );
}

/// Honest rejection gate: on `classic.pdf` the layout lane is proposed but the
/// literal lanes are cheaper, so the complete-cost court must not pick it. The
/// actual winner is still required to round-trip exactly.
#[test]
fn layout_not_selected_on_corpus() {
    let src = corpus("classic.pdf");
    assert!(
        propose_pdf_layout(&src, Limits::DEFAULT).unwrap().is_some(),
        "the layout lane must really be on the table for this loss to be honest"
    );

    let (encoded, report) = encode::encode(&src, Limits::DEFAULT).unwrap();
    assert_exact(&encoded, &src, "classic-court");
    assert_ne!(
        report.kind,
        CandidateKind::PdfLayout,
        "PDF_LAYOUT must lose the complete-cost court on classic.pdf"
    );
    assert_eq!(report.encoded_len, encoded.len() as u64);
    assert_eq!(report.cost.total(), encoded.len() as u64);

    eprintln!(
        "court[classic.pdf]: winner={} source={} encoded={}",
        report.kind.name(),
        report.source_len,
        report.encoded_len
    );
}

/// Identical input yields identical layout descriptor bytes.
#[test]
fn layout_deterministic() {
    for (name, src) in accepted_samples() {
        let a = forced_layout(&src).descriptor.serialize().unwrap().0;
        let b = forced_layout(&src).descriptor.serialize().unwrap().0;
        assert_eq!(a, b, "{name} layout bytes must be deterministic");
    }
}

/// Hostile input: a flipped payload byte — before or after CRC recomputation —
/// must yield a typed error or a non-matching reconstruction, never a panic.
#[test]
fn layout_hostile() {
    let src = corpus("classic.pdf");
    let (encoded, _) = forced_layout(&src).descriptor.serialize().unwrap();
    assert_exact(&encoded, &src, "hostile-baseline");

    // (a) Raw flip in the serialized descriptor. It lands in a record payload
    // (CRC32C catches it) or in framing (structural validation catches it).
    let mut corrupted = encoded.clone();
    let mid = corrupted.len() / 2;
    corrupted[mid] ^= 0xFF;
    match materialize::decode_to_bytes(&corrupted, Limits::DEFAULT) {
        Ok((out, _)) => assert_ne!(
            out, src,
            "a corrupted descriptor must not silently reconstruct the source"
        ),
        Err(e) => assert_ne!(
            e.class(),
            ErrorClass::InternalInvariant,
            "hostile input must fail with a typed class, not an internal invariant"
        ),
    }

    // (b) Corrupt at a layer that passes CRC: mutate a decoded inline literal and
    // re-serialize, which recomputes every record CRC.
    let mut parsed = Descriptor::parse(&encoded, Limits::DEFAULT).unwrap();
    let victim = parsed
        .descriptor
        .program
        .ops
        .iter_mut()
        .find_map(|op| match op {
            Op::Inline { bytes } if !bytes.is_empty() => Some(bytes),
            _ => None,
        })
        .expect("the layout program carries inline literals");
    victim[0] ^= 0xFF;
    let (reserialized, _) = parsed.descriptor.serialize().unwrap();
    match materialize::decode_to_bytes(&reserialized, Limits::DEFAULT) {
        Ok((out, _)) => assert_ne!(
            out, src,
            "a re-serialized corrupted literal must not silently reconstruct"
        ),
        Err(e) => assert_ne!(
            e.class(),
            ErrorClass::InternalInvariant,
            "a CRC-passing corruption must still fail with a typed class"
        ),
    }
}
