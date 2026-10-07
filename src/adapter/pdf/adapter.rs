//! Validated PDF detection and the Phase-3 physical candidate.
//!
//! Detection is structural and byte-authoritative: it requires positive evidence
//! in the bytes themselves — a `%PDF-` header comment, at least one complete
//! indirect object, and at least one `%%EOF`. A file name or extension is never
//! authority. Anything that does not satisfy all three conditions falls back to
//! the opaque adapter, which is always exact.
//!
//! The proposed candidate persists the exact physical span partition as one
//! [`Op::Inline`] literal instruction per span, in ascending offset order. That
//! program reconstructs the source byte-for-byte by construction; the span kinds
//! remain deterministic analysis metadata that [`scan`] can recompute at any
//! time. This subphase deliberately performs **no** structural compression:
//! typed residuals, stream de-duplication, and cross-reference replay are
//! Phase 5. The literal PDF candidate is therefore honest but almost always
//! loses the complete-cost court to RAW, which is the expected Phase-3 result.

use crate::SOURCE_FORMAT_PDF;
use crate::container::{Descriptor, UNIVERSE};
use crate::dra::{Op, Program};
use crate::encode::candidates::{Candidate, CandidateKind};
use crate::error::Result;
use crate::integrity::sha256;
use crate::limits::Limits;

#[cfg(feature = "rans")]
use crate::entropy::{
    CODER_ORDER0_BYTE_RANS, CODER_VERSION_1, EntropyChannelDescriptor, EntropyModel, encode_channel,
};

#[cfg(feature = "rans")]
use super::channels::KIND_COUNT;
use super::physical::{PdfPhysical, scan};

/// Validated PDF detection: a `%PDF-` header AND at least one indirect object
/// AND at least one `%%EOF`. Extensions are never authority.
///
/// Returns `false` whenever the structural scan cannot complete under `limits`;
/// declining here preserves the unconditional opaque fallback.
pub fn detect(input: &[u8], limits: Limits) -> bool {
    match scan(input, limits) {
        Ok(physical) => is_validated_pdf(&physical),
        Err(_) => false,
    }
}

/// All three positive structural conditions required to call a file a PDF.
fn is_validated_pdf(physical: &PdfPhysical) -> bool {
    physical.header.is_some() && !physical.objects.is_empty() && !physical.eofs.is_empty()
}

/// Propose a PDF physical candidate, or `None` (opaque fallback) when the file
/// is not a validated PDF.
///
/// The program is one [`Op::Inline`] per physical span, so it reconstructs the
/// source exactly; span kinds remain deterministic analysis metadata recomputable
/// by [`scan`]. The candidate declines (`Ok(None)`) rather than truncating when:
///
/// - the input is not a validated PDF, or the structural scan fails; or
/// - the span count would exceed `limits.max_graph_ops`, so the program cannot
///   be expressed within the declared bounds.
pub fn propose_pdf(input: &[u8], limits: Limits) -> Result<Option<Candidate>> {
    let physical = match scan(input, limits) {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };
    propose_pdf_with(input, limits, &physical)
}

/// [`propose_pdf`] against an already-computed physical scan. Sharing one scan
/// across the PDF proposers removes the redundant `O(n)` re-scan that made the
/// candidate portfolio quadratic-ish on large scanned PDFs (see
/// `tools/phase14-*`).
pub(crate) fn propose_pdf_with(
    input: &[u8],
    limits: Limits,
    physical: &PdfPhysical,
) -> Result<Option<Candidate>> {
    if !is_validated_pdf(physical) {
        return Ok(None);
    }
    if physical.spans.len() as u64 > limits.max_graph_ops as u64 {
        return Ok(None);
    }

    // One inline literal per span, preserving exact offset order. The physical
    // cover is contiguous and non-overlapping, so concatenating these spans
    // reproduces the source unchanged.
    let ops: Vec<Op> = physical
        .spans
        .iter()
        .map(|span| {
            let start = span.start as usize;
            let end = start + span.len as usize;
            Op::Inline {
                bytes: input[start..end].to_vec(),
            }
        })
        .collect();

    let format_basis = format!(
        "pdf;spans={};objects={};revisions={}",
        physical.spans.len(),
        physical.objects.len(),
        physical.revisions.len()
    );

    let descriptor = Descriptor {
        universe: UNIVERSE.to_string(),
        source_format: SOURCE_FORMAT_PDF,
        format_basis,
        models: vec![],
        channels: vec![],
        objects: vec![],
        program: Program::new(ops),
        observation_index: None,
        seek_directory: false,
        checkpoints: None,
        source_sha256: sha256(input),
        source_len: input.len() as u64,
    };

    Ok(Some(Candidate {
        kind: CandidateKind::PdfPhysical,
        descriptor,
    }))
}

/// Propose a typed-channel PDF candidate, or `None` if the file cannot be split
/// (not a validated PDF, or too many tokens).
///
/// The validated lexical cover is transposed into fixed parallel channels and
/// each channel is entropy-coded independently with its own order-0 byte-rANS
/// model. Reconstruction is a single [`Op::InterleaveChannels`] that replays the
/// kind/length sequence against the per-kind payload channels, so exactness holds
/// by construction; the complete-cost court still decides whether the extra model
/// records pay for themselves.
///
/// Fixed channel layout (indices are a wire contract of this candidate):
///
/// - channel `0` — kinds, one kind byte per token;
/// - channel `1` — lengths, four little-endian bytes per token;
/// - channels `2..2+KIND_COUNT` — `payloads[k]` for kind `k`, all of them, even
///   when empty.
#[cfg(feature = "rans")]
pub fn propose_pdf_channels(input: &[u8], limits: Limits) -> Result<Option<Candidate>> {
    let physical = match scan(input, limits) {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };
    propose_pdf_channels_with(input, limits, &physical)
}

/// [`propose_pdf_channels`] against an already-computed physical scan.
#[cfg(feature = "rans")]
pub(crate) fn propose_pdf_channels_with(
    input: &[u8],
    limits: Limits,
    physical: &PdfPhysical,
) -> Result<Option<Candidate>> {
    if !is_validated_pdf(physical) {
        return Ok(None);
    }
    let plan = match super::channels::split(input, limits)? {
        Some(p) => p,
        None => return Ok(None),
    };

    // Build the raw channel streams in the fixed layout. `kinds` and `lengths`
    // are aligned one entry per token; the payload streams are already aligned
    // per kind by `split`.
    let mut streams: Vec<Vec<u8>> = Vec::with_capacity(2 + KIND_COUNT);
    streams.push(plan.kinds.clone());
    let mut lengths = Vec::with_capacity(plan.lengths.len() * 4);
    for &len in &plan.lengths {
        lengths.extend_from_slice(&len.to_le_bytes());
    }
    streams.push(lengths);
    for payload in &plan.payloads {
        streams.push(payload.clone());
    }

    // Encode each channel with a model normalized from its own byte histogram.
    // An empty stream yields the canonical uniform model and an empty capsule.
    let mut models = Vec::with_capacity(streams.len());
    let mut channels = Vec::with_capacity(streams.len());
    for stream in &streams {
        let mut counts = [0u64; crate::entropy::ALPHABET];
        for &b in stream {
            counts[b as usize] += 1;
        }
        let model = EntropyModel::from_counts(&counts, 12)?;
        let scale_bits = model.scale_bits;
        let capsule = encode_channel(&model, stream)?;
        let model_id = models.len() as u32;
        models.push(model);
        channels.push(EntropyChannelDescriptor {
            coder: CODER_ORDER0_BYTE_RANS,
            coder_version: CODER_VERSION_1,
            scale_bits,
            lane_count: 1,
            model_id,
            symbol_count: capsule.symbol_count,
            decoded_length: capsule.decoded_length,
            initial_state: capsule.initial_state,
            payload: capsule.payload,
        });
    }

    let first_payload_channel = 2;
    let payload_channel_count = KIND_COUNT as u8;
    let program = Program::new(vec![Op::InterleaveChannels {
        kinds_channel: 0,
        lengths_channel: 1,
        first_payload_channel,
        payload_channel_count,
    }]);

    let format_basis = format!(
        "pdf-channels;kinds={};tokens={};channels={}",
        KIND_COUNT,
        plan.token_count(),
        channels.len()
    );

    let descriptor = Descriptor {
        universe: UNIVERSE.to_string(),
        source_format: SOURCE_FORMAT_PDF,
        format_basis,
        models,
        channels,
        objects: vec![],
        program,
        observation_index: None,
        seek_directory: false,
        checkpoints: None,
        source_sha256: sha256(input),
        source_len: input.len() as u64,
    };

    Ok(Some(Candidate {
        kind: CandidateKind::PdfChannels,
        descriptor,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canonical_pdf() -> Vec<u8> {
        let mut s = String::new();
        s.push_str("%PDF-1.7\n");
        s.push_str("1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
        s.push_str("2 0 obj\n<< /Length 6 >>\nstream\nhello\nendstream\nendobj\n");
        s.push_str("3 0 obj\n<< /Length 7 >>\nstream\nworld\nendstream\nendobj\n");
        s.push_str("4 0 obj\n<< /Length 4 >>\nstream\nxyz\nendstream\nendobj\n");
        s.push_str("xref\n0 5\n0000000000 65535 f \n0000000010 00000 n \n");
        s.push_str("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n321\n%%EOF");
        s.into_bytes()
    }

    #[test]
    fn detect_accepts_canonical_pdf() {
        assert!(detect(&canonical_pdf(), Limits::DEFAULT));
    }

    #[test]
    fn detect_rejects_non_pdf_and_truncated() {
        assert!(!detect(b"this is definitely not a PDF", Limits::DEFAULT));
        assert!(!detect(b"", Limits::DEFAULT));
        // Header only: no indirect object and no `%%EOF`.
        assert!(!detect(b"%PDF-1.7\n", Limits::DEFAULT));
        // Object and `%%EOF` but no `%PDF-` header: still not a PDF.
        assert!(!detect(b"1 0 obj\n<< >>\nendobj\n%%EOF", Limits::DEFAULT));
    }

    #[test]
    fn propose_declines_non_pdf() {
        assert!(
            propose_pdf(b"not a pdf", Limits::DEFAULT)
                .unwrap()
                .is_none()
        );
        assert!(
            propose_pdf(b"%PDF-1.7\n", Limits::DEFAULT)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn propose_returns_pdf_candidate() {
        let pdf = canonical_pdf();
        let cand = propose_pdf(&pdf, Limits::DEFAULT).unwrap().unwrap();
        assert_eq!(cand.kind, CandidateKind::PdfPhysical);
        assert_eq!(cand.descriptor.source_format, SOURCE_FORMAT_PDF);
        assert!(cand.descriptor.objects.is_empty());
        assert!(cand.descriptor.models.is_empty());
        assert!(cand.descriptor.channels.is_empty());
        assert_eq!(cand.descriptor.source_len, pdf.len() as u64);
        assert_eq!(cand.descriptor.source_sha256, sha256(&pdf));
    }

    #[test]
    fn pdf_candidate_round_trips_exactly() {
        let pdf = canonical_pdf();
        let cand = propose_pdf(&pdf, Limits::DEFAULT).unwrap().unwrap();
        let (bytes, cost) = cand.descriptor.serialize().unwrap();
        assert_eq!(cost.total(), bytes.len() as u64);

        let (out, parsed) = crate::materialize::decode_to_bytes(&bytes, Limits::DEFAULT).unwrap();
        assert_eq!(out.len(), pdf.len());
        assert_eq!(out, pdf, "PDF candidate must materialize byte-for-byte");
        assert_eq!(sha256(&out), sha256(&pdf));
        assert_eq!(parsed.descriptor.source_sha256, sha256(&pdf));
        assert_eq!(parsed.descriptor.source_format, SOURCE_FORMAT_PDF);
    }

    #[test]
    fn court_prefers_raw_for_small_pdf() {
        // Honest Phase-3 outcome: the literal PDF candidate carries no structural
        // compression, so for a small PDF the complete-cost court still prefers
        // RAW. Phase 5 is where structural wins may change this.
        let pdf = canonical_pdf();
        let (bytes, report) = crate::encode::encode(&pdf, Limits::DEFAULT).unwrap();
        let (out, _) = crate::materialize::decode_to_bytes(&bytes, Limits::DEFAULT).unwrap();
        assert_eq!(out, pdf);
        assert_eq!(report.kind, CandidateKind::Raw);
    }

    #[test]
    fn propose_declines_when_span_count_exceeds_graph_ops() {
        let pdf = canonical_pdf();
        let limits = Limits {
            max_graph_ops: 1,
            ..Limits::DEFAULT
        };
        assert!(
            propose_pdf(&pdf, limits).unwrap().is_none(),
            "a multi-span PDF cannot fit in a one-op graph"
        );
    }

    #[cfg(feature = "rans")]
    #[test]
    fn pdf_channels_exact() {
        use crate::adapter::pdf::samples::{is_negative_control, sample_pdfs};

        let mut checked = 0usize;
        for (name, bytes) in sample_pdfs() {
            if is_negative_control(name) {
                continue;
            }
            let cand = propose_pdf_channels(&bytes, Limits::DEFAULT)
                .unwrap()
                .unwrap_or_else(|| panic!("{name} must propose a typed-channel candidate"));
            assert_eq!(cand.kind, CandidateKind::PdfChannels);

            let (encoded, _) = cand.descriptor.serialize().unwrap();
            let parsed = Descriptor::parse(&encoded, Limits::DEFAULT).unwrap();
            let out = crate::materialize::materialize(&parsed, Limits::DEFAULT).unwrap();
            assert_eq!(
                out, bytes,
                "{name} typed channels must materialize byte-for-byte"
            );
            assert_eq!(sha256(&out), sha256(&bytes), "{name} typed-channel sha");
            checked += 1;
        }
        assert!(
            checked >= 3,
            "must force exactness on at least three samples"
        );
    }

    #[cfg(feature = "rans")]
    #[test]
    fn pdf_channels_none_for_non_pdf() {
        use crate::adapter::pdf::samples::{is_negative_control, sample_pdfs};

        let mut seen = 0usize;
        for (name, bytes) in sample_pdfs() {
            if !is_negative_control(name) {
                continue;
            }
            assert!(
                propose_pdf_channels(&bytes, Limits::DEFAULT)
                    .unwrap()
                    .is_none(),
                "{name} must decline the typed-channel candidate"
            );
            seen += 1;
        }
        assert_eq!(seen, 2, "both negative controls must be exercised");
        assert!(
            propose_pdf_channels(b"%PDF-1.7\n", Limits::DEFAULT)
                .unwrap()
                .is_none(),
            "a header-only file is not a validated PDF"
        );
    }

    #[cfg(feature = "rans")]
    #[test]
    fn pdf_channels_deterministic() {
        use crate::adapter::pdf::samples::{is_negative_control, sample_pdfs};

        for (name, bytes) in sample_pdfs() {
            if is_negative_control(name) {
                continue;
            }
            let a = propose_pdf_channels(&bytes, Limits::DEFAULT)
                .unwrap()
                .unwrap()
                .descriptor
                .serialize()
                .unwrap()
                .0;
            let b = propose_pdf_channels(&bytes, Limits::DEFAULT)
                .unwrap()
                .unwrap()
                .descriptor
                .serialize()
                .unwrap()
                .0;
            assert_eq!(a, b, "{name} typed-channel bytes must be deterministic");
        }
    }

    #[cfg(feature = "rans")]
    #[test]
    fn report_court_winner_per_sample() {
        use crate::adapter::pdf::samples::sample_pdfs;

        for (name, bytes) in sample_pdfs() {
            let (encoded, report) = crate::encode::encode(&bytes, Limits::DEFAULT).unwrap();
            let (out, _) = crate::materialize::decode_to_bytes(&encoded, Limits::DEFAULT).unwrap();
            assert_eq!(out, bytes, "{name} court winner must be exact");
            eprintln!(
                "court[{name}]: winner={} source={} encoded={} ratio={:.3}",
                report.kind.name(),
                report.source_len,
                report.encoded_len,
                report.compression_ratio()
            );
        }
    }
}
