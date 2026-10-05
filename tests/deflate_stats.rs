#![cfg(all(feature = "deflate-replay", feature = "rans"))]
//! Phase-7.0 gate for the `deflate-stats` correction-ratio harness.
//!
//! The harness is a *measurement* of `correction_bytes / compressed_bytes` per
//! `FlateDecode` stream, not a candidate: it never changes what the complete-cost
//! court selects. These tests exercise the underlying function
//! ([`deflate_stats`]) on the real-zlib `flate.pdf` sample and assert the
//! aggregate is self-consistent with the per-stream rows.

use vole_document::adapter::pdf::deflate_stats;
use vole_document::adapter::pdf::samples::sample_pdfs;
use vole_document::limits::Limits;

/// Fetch a named sample from the canonical corpus.
fn corpus(name: &str) -> Vec<u8> {
    sample_pdfs()
        .into_iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("sample {name} is missing from the corpus"))
        .1
}

/// Gate 1: `flate.pdf` exposes six `FlateDecode` streams, every one replays
/// exactly, the aggregate `compressed_bytes` equals the sum of the per-stream
/// `data_len`, and every replayed stream carries a plaintext, a correction blob,
/// and an order-0 rANS plaintext cost.
#[test]
fn flate_sample_stats_are_self_consistent() {
    let src = corpus("flate.pdf");
    let stats = deflate_stats(&src, Limits::DEFAULT).expect("deflate_stats must not error");

    assert!(stats.is_pdf, "flate.pdf must be detected as a PDF");
    assert_eq!(
        stats.summary.flate_streams, 6,
        "flate.pdf must expose exactly six FlateDecode streams"
    );
    assert_eq!(stats.streams.len(), 6, "one row per FlateDecode stream");
    assert_eq!(
        stats.summary.replayed, 6,
        "every stream must replay exactly"
    );
    assert_eq!(stats.summary.declined, 0, "no stream may decline");

    let compressed_sum: u64 = stats.streams.iter().map(|s| s.compressed_bytes).sum();
    assert_eq!(
        stats.summary.compressed_bytes, compressed_sum,
        "the aggregate compressed_bytes must equal the sum of stream data_len"
    );

    let mut plain_sum = 0u64;
    let mut corr_sum = 0u64;
    let mut rans_sum = 0u64;
    for s in &stats.streams {
        assert!(s.replayed, "stream {} must replay", s.object);
        assert!(
            s.decline_reason.is_none(),
            "a replayed stream has no reason"
        );
        let plain = s.plaintext_bytes.expect("replayed stream has a plaintext");
        let corr = s.correction_bytes.expect("replayed stream has corrections");
        let rans = s
            .rans_plaintext_bytes
            .expect("the rans feature is enabled in this test");
        assert!(plain > 0, "plaintext must be non-empty");
        assert!(corr > 0, "correction blob must be non-empty");
        assert!(rans > 0, "rANS plaintext cost must be non-empty");
        // The correction blob is a small residual, not a second copy of the
        // stream: it must be strictly smaller than the compressed bytes for a
        // stream that was admitted by the exact gate.
        assert!(
            corr < s.compressed_bytes,
            "correction ({corr}) must be smaller than the compressed stream ({} bytes)",
            s.compressed_bytes
        );
        plain_sum += plain;
        corr_sum += corr;
        rans_sum += rans;
    }
    assert_eq!(stats.summary.plaintext_bytes, plain_sum);
    assert_eq!(stats.summary.correction_bytes, corr_sum);
    assert_eq!(stats.summary.rans_plaintext_bytes, rans_sum);

    // Ratios are self-consistent with the aggregate integers.
    assert_eq!(
        stats.summary.compressed_bytes, compressed_sum,
        "compressed_bytes is the denominator of correction/compressed"
    );
}

/// Gate 2: a non-PDF yields `is_pdf:false`, no streams, and a zeroed summary —
/// never an error.
#[test]
fn non_pdf_yields_is_pdf_false() {
    let stats = deflate_stats(
        b"this is plain text, definitely not a PDF\n",
        Limits::DEFAULT,
    )
    .expect("a non-PDF must not error");
    assert!(!stats.is_pdf);
    assert!(stats.streams.is_empty(), "a non-PDF has no streams");
    assert_eq!(stats.summary.flate_streams, 0);
    assert_eq!(stats.summary.replayed, 0);
    assert_eq!(stats.summary.declined, 0);
    assert_eq!(stats.summary.compressed_bytes, 0);
    assert_eq!(stats.summary.correction_bytes, 0);
}

/// Gate 3: a PDF with no `FlateDecode` streams reports zero streams rather than
/// an error, and `is_pdf` still reflects detection.
#[test]
fn pdf_without_flate_reports_zero_streams() {
    let src = corpus("classic.pdf");
    let stats = deflate_stats(&src, Limits::DEFAULT).expect("deflate_stats must not error");
    assert!(stats.is_pdf, "classic.pdf is a valid PDF");
    assert_eq!(stats.summary.flate_streams, 0);
    assert!(stats.streams.is_empty());
}
