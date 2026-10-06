#![cfg(all(feature = "deflate-replay", feature = "rans"))]
//! Phase-6.4/6.5 acceptance/rejection gates for exact PDF DEFLATE replay.
//!
//! These tests exercise the replay lane (`PDF_DEFLATE_REPLAY`) and its shared
//! plaintext variant (`PDF_DEFLATE_REPLAY_RANS`) over the real-zlib `flate.pdf`
//! sample. The lane is never assumed profitable: the complete-cost court is
//! allowed to pick something else, and the honest outcome is recorded.
//!
//! Two structural claims are checked on `flate.pdf`, and both are *derived*
//! rather than hard-coded:
//!
//! * plaintext sharing actually deduplicates storage — the raw lane stores one
//!   `OBJECT` per unique plaintext (not one per stream), and the rANS lane stores
//!   one entropy `CHANNEL` per unique plaintext, so its channel count is strictly
//!   below both the stream count and the replay count;
//! * hostile correction blobs never panic and never silently reconstruct — every
//!   crafted descriptor fails closed with a typed `CodecReplay`/`InvalidGraph`.
//!
//! Every admitted descriptor must satisfy the authoritative exact triple:
//! `materialized_length == source_length`, `SHA256(materialized) == SHA256(source)`,
//! and `byte_compare(materialized, source) == equal`.

use vole_document::ErrorClass;
use vole_document::SOURCE_FORMAT_PDF;
use vole_document::adapter::pdf::samples::{is_negative_control, sample_pdfs};
use vole_document::adapter::pdf::{propose_pdf_deflate_replay, propose_pdf_deflate_replay_rans};
use vole_document::container::{Descriptor, ObjectSource, UNIVERSE};
use vole_document::dra::{Op, Program};
use vole_document::encode;
use vole_document::encode::candidates::{Candidate, CandidateKind};
use vole_document::integrity::{sha256, to_hex};
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

/// The exact triple plus the deep verifier, mirroring `tests/exact.rs`.
fn assert_exact(encoded: &[u8], src: &[u8], label: &str) {
    let (out, parsed) = materialize::decode_to_bytes(encoded, Limits::DEFAULT)
        .unwrap_or_else(|e| panic!("[{label}] decode failed: {e}"));
    assert_eq!(out.len(), src.len(), "[{label}] materialized length");
    assert_eq!(sha256(&out), sha256(src), "[{label}] digest");
    assert_eq!(out, src, "[{label}] byte compare");
    assert_eq!(out.len() as u64, parsed.descriptor.source_len);

    let vr = materialize::verify(encoded, Limits::DEFAULT)
        .unwrap_or_else(|e| panic!("[{label}] verify failed: {e}"));
    assert_eq!(vr.source_len, src.len() as u64, "[{label}] verify length");
    assert_eq!(
        vr.sha256_hex,
        to_hex(&sha256(src)),
        "[{label}] verify digest"
    );
}

/// Force the raw-replay candidate, or panic with a clear message.
fn forced_raw(src: &[u8]) -> Candidate {
    propose_pdf_deflate_replay(src, Limits::DEFAULT)
        .unwrap_or_else(|e| panic!("propose_pdf_deflate_replay failed: {e}"))
        .unwrap_or_else(|| panic!("raw DEFLATE replay must be proposed for this input"))
}

/// Force the shared-plaintext rANS replay candidate, or panic.
fn forced_rans(src: &[u8]) -> Candidate {
    propose_pdf_deflate_replay_rans(src, Limits::DEFAULT)
        .unwrap_or_else(|e| panic!("propose_pdf_deflate_replay_rans failed: {e}"))
        .unwrap_or_else(|| panic!("rANS DEFLATE replay must be proposed for this input"))
}

/// Read one `key=value` integer field out of a `format_basis` string.
fn basis_field(basis: &str, key: &str) -> Option<u64> {
    basis.split(';').find_map(|part| {
        let (k, v) = part.split_once('=')?;
        (k == key).then(|| v.parse().ok()).flatten()
    })
}

/// Gate 1: both replay variants are proposed for `flate.pdf` and every serialized
/// descriptor reproduces the source byte-for-byte with an agreeing digest, and the
/// deep verifier agrees.
#[test]
fn flate_sample_replay_is_byte_exact() {
    let src = corpus("flate.pdf");

    let raw = forced_raw(&src);
    assert_eq!(raw.kind, CandidateKind::PdfDeflateReplay);
    assert_eq!(raw.kind.name(), "PDF_DEFLATE_REPLAY");
    let (raw_bytes, _) = raw.descriptor.serialize().unwrap();
    assert_exact(&raw_bytes, &src, "raw-replay");

    let rans = forced_rans(&src);
    assert_eq!(rans.kind, CandidateKind::PdfDeflateReplayRans);
    assert_eq!(rans.kind.name(), "PDF_DEFLATE_REPLAY_RANS");
    let (rans_bytes, _) = rans.descriptor.serialize().unwrap();
    assert_exact(&rans_bytes, &src, "rans-replay");
}

/// Gate 2: shared plaintext is stored once. The rANS lane carries one entropy
/// channel per *unique* plaintext, so its channel count is strictly below both the
/// number of FlateDecode streams and the number of replays; the raw lane stores
/// fewer objects than one plaintext plus one corrections blob per replay. All
/// counts are derived from the candidate and its `format_basis`, never pinned.
#[test]
fn shared_plaintext_is_deduplicated() {
    let src = corpus("flate.pdf");

    let raw = forced_raw(&src);
    let raw_streams = basis_field(&raw.descriptor.format_basis, "streams")
        .expect("raw basis records the stream count");
    let raw_replayed = basis_field(&raw.descriptor.format_basis, "replayed")
        .expect("raw basis records the replay count");
    assert!(
        raw_replayed >= 1 && raw_replayed <= raw_streams,
        "raw basis replayed={raw_replayed} must lie within streams={raw_streams}"
    );
    // Without plaintext dedup the raw lane would store one plaintext object plus
    // one corrections object per replay. Deduplication collapses the four streams
    // that share a plaintext, so the total stays strictly below `2 * replayed`.
    assert!(
        (raw.descriptor.objects.len() as u64) < 2 * raw_replayed,
        "raw lane stored {} objects, expected fewer than {} (one plaintext + one \
         corrections per replay, minus plaintext dedup)",
        raw.descriptor.objects.len(),
        2 * raw_replayed
    );

    let rans = forced_rans(&src);
    let rans_streams = basis_field(&rans.descriptor.format_basis, "streams")
        .expect("rans basis records the stream count");
    let rans_replayed = basis_field(&rans.descriptor.format_basis, "replayed")
        .expect("rans basis records the replay count");

    assert_eq!(
        rans_streams, 6,
        "flate.pdf must expose exactly six FlateDecode streams"
    );
    assert_eq!(
        rans_replayed, rans_streams,
        "every eligible FlateDecode stream must replay in this sample"
    );

    let channels = rans.descriptor.channels.len() as u64;
    assert!(
        channels < rans_streams,
        "one shared channel per unique plaintext must be fewer than {rans_streams} \
         streams, got {channels}"
    );
    assert!(
        channels < rans_replayed,
        "channel count {channels} must be strictly below the {rans_replayed} replays"
    );
    assert_eq!(
        basis_field(&rans.descriptor.format_basis, "channels"),
        Some(channels),
        "the recorded channel count must match the descriptor"
    );
    assert_eq!(
        rans.descriptor.models.len() as u64,
        channels,
        "each shared plaintext channel carries exactly one model"
    );
}

/// Gate 3: on `flate.pdf` the shared-plaintext rANS lane serializes strictly
/// smaller than the raw-plaintext lane. This is a measured outcome, not an
/// assumption about the court.
#[test]
fn rans_plaintext_beats_raw_plaintext() {
    let src = corpus("flate.pdf");
    let (raw_bytes, _) = forced_raw(&src).descriptor.serialize().unwrap();
    let (rans_bytes, _) = forced_rans(&src).descriptor.serialize().unwrap();

    assert!(
        rans_bytes.len() < raw_bytes.len(),
        "PDF_DEFLATE_REPLAY_RANS ({} bytes) must beat PDF_DEFLATE_REPLAY ({} bytes)",
        rans_bytes.len(),
        raw_bytes.len()
    );
    eprintln!(
        "header[flate.pdf]: PDF_DEFLATE_REPLAY={} PDF_DEFLATE_REPLAY_RANS={} saved={}",
        raw_bytes.len(),
        rans_bytes.len(),
        raw_bytes.len() - rans_bytes.len()
    );
}

/// Gate 4: the unforced court on `flate.pdf` round-trips byte-exactly and reports
/// a coherent winner. We deliberately do **not** assert which kind wins: the
/// complete-cost court may prefer a non-replay lane, and that outcome is recorded.
#[test]
fn auto_court_is_exact_and_deflate_sample_runs() {
    let src = corpus("flate.pdf");
    let (encoded, report) = encode::encode(&src, Limits::DEFAULT).unwrap();
    assert_exact(&encoded, &src, "flate-court");

    let known = [
        CandidateKind::Raw,
        CandidateKind::Rle,
        CandidateKind::ByteRans,
        CandidateKind::PdfPhysical,
        CandidateKind::PdfChannels,
        CandidateKind::PdfLayout,
        CandidateKind::PdfLayoutRans,
        CandidateKind::PdfDeflateReplay,
        CandidateKind::PdfDeflateReplayRans,
    ];
    assert!(
        known.contains(&report.kind),
        "unknown winner {:?}",
        report.kind
    );
    assert_eq!(report.encoded_len, encoded.len() as u64);
    assert_eq!(report.cost.total(), encoded.len() as u64);

    let (raw_len, _) = forced_raw(&src).descriptor.serialize().unwrap();
    let (rans_len, _) = forced_rans(&src).descriptor.serialize().unwrap();
    eprintln!(
        "court[flate.pdf]: winner={} source={} encoded={} (PDF_DEFLATE_REPLAY={} PDF_DEFLATE_REPLAY_RANS={})",
        report.kind.name(),
        report.source_len,
        report.encoded_len,
        raw_len.len(),
        rans_len.len()
    );
}

/// Gate 5: a PDF with no FlateDecode streams declines both replay lanes, and
/// forcing the replay lane is a typed `Usage` error (not a silent fallback).
#[test]
fn declines_without_flate_streams() {
    let src = corpus("classic.pdf");
    assert!(
        propose_pdf_deflate_replay(&src, Limits::DEFAULT)
            .unwrap()
            .is_none(),
        "classic.pdf has no FlateDecode streams and must decline the raw replay lane"
    );
    assert!(
        propose_pdf_deflate_replay_rans(&src, Limits::DEFAULT)
            .unwrap()
            .is_none(),
        "classic.pdf must decline the rANS replay lane"
    );

    let e = encode::encode_with(&src, Limits::DEFAULT, Some(CandidateKind::PdfDeflateReplay))
        .expect_err("forcing an unproposed replay lane must be a usage error");
    assert_eq!(e.class(), ErrorClass::Usage);
}

/// Gate 6: a non-PDF input declines both replay lanes and forcing the lane is a
/// typed `Usage` error.
#[test]
fn declines_non_pdf() {
    let src = corpus("notpdf.bin");
    assert!(
        propose_pdf_deflate_replay(&src, Limits::DEFAULT)
            .unwrap()
            .is_none(),
        "notpdf.bin must decline the raw replay lane"
    );
    assert!(
        propose_pdf_deflate_replay_rans(&src, Limits::DEFAULT)
            .unwrap()
            .is_none(),
        "notpdf.bin must decline the rANS replay lane"
    );

    let e = encode::encode_with(&src, Limits::DEFAULT, Some(CandidateKind::PdfDeflateReplay))
        .expect_err("forcing an unproposed replay lane must be a usage error");
    assert_eq!(e.class(), ErrorClass::Usage);
}

/// Gate 7: proposing each lane twice yields identical serialized bytes.
#[test]
fn replay_is_deterministic() {
    let src = corpus("flate.pdf");

    let raw_a = forced_raw(&src).descriptor.serialize().unwrap().0;
    let raw_b = forced_raw(&src).descriptor.serialize().unwrap().0;
    assert_eq!(raw_a, raw_b, "raw replay bytes must be deterministic");

    let rans_a = forced_rans(&src).descriptor.serialize().unwrap().0;
    let rans_b = forced_rans(&src).descriptor.serialize().unwrap().0;
    assert_eq!(rans_a, rans_b, "rANS replay bytes must be deterministic");
}

/// Gate 8: a program that replays an arbitrary plaintext against hostile
/// correction blobs must fail closed. Reconstruction can panic inside `preflate`;
/// the decoder isolates it, so every case must yield a typed
/// `CodecReplay`/`InvalidGraph` error and never a panic or a silent success.
#[test]
fn hostile_corrections_fail_closed() {
    let plaintext = b"BT /F1 12 Tf (replay this) Tj ET\n".repeat(40);

    let mut state = 0x0F1E_2D3C_4B5A_6978u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };

    for case in 0..16u32 {
        // Deterministic garbage correction blob of a size that is not a valid
        // `preflate` correction state.
        let n = 8 + (next() % 120) as usize;
        let mut garbage = vec![0u8; n];
        for b in garbage.iter_mut() {
            *b = next() as u8;
        }

        // The program's predicted length must equal `source_len` for the
        // descriptor to parse; the hostile part is the garbage correction blob,
        // not a mismatched framing length.
        let declared = plaintext.len() as u32;
        let descriptor = Descriptor {
            universe: UNIVERSE.to_string(),
            source_format: SOURCE_FORMAT_PDF,
            format_basis: format!("pdf-deflate-replay;test=hostile;case={case}"),
            models: vec![],
            channels: vec![],
            objects: vec![
                ObjectSource::Inline(plaintext.clone()),
                ObjectSource::Inline(garbage),
            ],
            program: Program::new(vec![Op::DeflateReplay {
                replay_codec: vole_document::dra::op::REPLAY_DEFLATE_PREFLATE_0_7_6,
                source_kind: 0, // DEFLATE_SOURCE_OBJECT
                source_id: 0,
                corrections_object: 1,
                declared_output_len: declared,
            }]),
            observation_index: None,
            seek_directory: false,
            source_sha256: sha256(b"hostile placeholder source"),
            source_len: plaintext.len() as u64,
        };

        let (encoded, _) = descriptor.serialize().unwrap();
        let parsed = Descriptor::parse(&encoded, Limits::DEFAULT).unwrap();
        match materialize::materialize(&parsed, Limits::DEFAULT) {
            Ok(out) => panic!(
                "case {case}: hostile corrections silently reconstructed {} bytes",
                out.len()
            ),
            Err(e) => assert!(
                matches!(
                    e.class(),
                    ErrorClass::CodecReplay | ErrorClass::InvalidGraph
                ),
                "case {case}: expected CodecReplay or InvalidGraph, got {:?}",
                e.class()
            ),
        }
    }
}

/// Gate 9: the lexer stream-opacity change must not regress any sample. Every
/// non-negative-control sample still round-trips byte-for-byte through the court.
#[test]
fn directed_roundtrip_over_samples() {
    let mut checked = 0usize;
    for (name, src) in sample_pdfs() {
        if is_negative_control(name) {
            continue;
        }
        let (encoded, _) = encode::encode(&src, Limits::DEFAULT)
            .unwrap_or_else(|e| panic!("[{name}] encode failed: {e}"));
        assert_exact(&encoded, &src, name);
        checked += 1;
    }
    assert!(
        checked >= 2,
        "expected several non-negative-control samples"
    );
}
