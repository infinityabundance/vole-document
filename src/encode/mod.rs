//! Encoding: candidate portfolio, complete-cost court, and decode-before-commit.

pub mod candidates;
pub mod court;

/// Encoder-only search governance, compiled only under the non-default,
/// dependency-free `dsfb-search` feature. Encoder-only: the decode path never
/// imports it (see `docs/adr/0022-encoder-only-search-governance.md`).
#[cfg(feature = "dsfb-search")]
pub mod governor;

use crate::accounting::CostBreakdown;
use crate::encode::candidates::CandidateKind;
use crate::error::{Error, Result};
use crate::integrity::{sha256, to_hex};
use crate::limits::Limits;

/// A report describing encode-time decisions and physical attribution.
#[derive(Debug, Clone)]
pub struct EncodeReport {
    /// Winning candidate family.
    pub kind: CandidateKind,
    /// Source length in bytes.
    pub source_len: u64,
    /// Serialized `.voldoc` length in bytes.
    pub encoded_len: u64,
    /// Physical byte attribution of the serialized descriptor.
    pub cost: CostBreakdown,
    /// Lower-case hex SHA-256 of the source.
    pub sha256_hex: String,
    /// Number of candidates evaluated.
    pub candidates_evaluated: u32,
    /// Reconstruction work (DRA instruction count) of the winner.
    pub graph_ops: usize,
}

impl EncodeReport {
    /// Source bytes divided by encoded bytes (0.0 when the source is empty).
    pub fn compression_ratio(&self) -> f64 {
        if self.encoded_len == 0 {
            return 0.0;
        }
        self.source_len as f64 / self.encoded_len as f64
    }
}

/// Encode `input` exactly, returning the serialized descriptor and a report.
///
/// Every returned byte sequence has already been round-tripped through the
/// normative decoder and byte-compared against `input`.
pub fn encode(input: &[u8], limits: Limits) -> Result<(Vec<u8>, EncodeReport)> {
    encode_with(input, limits, None)
}

/// Encode `input` exactly, optionally forcing a single candidate family.
///
/// With `force == None` this is the ordinary complete-cost court over every
/// proposed candidate. With `force == Some(kind)` the candidate set is first
/// filtered to proposals of exactly that kind, then the *same* court runs over
/// the filtered set: the forced lane is still serialized, decoded, and
/// byte-compared before it may be returned, so forcing never weakens exactness
/// and determinism.
///
/// Returns [`Error::usage`] when `force` names a kind that this input does not
/// propose (for example PDF_CHANNELS on a non-PDF, or BYTE_RANS on empty input),
/// rather than silently substituting another lane. That failure is the honest
/// signal that the mechanism does not apply.
pub fn encode_with(
    input: &[u8],
    limits: Limits,
    force: Option<CandidateKind>,
) -> Result<(Vec<u8>, EncodeReport)> {
    let mut cands = candidates::propose_all(input, limits)?;
    if let Some(kind) = force {
        cands.retain(|c| c.kind == kind);
        if cands.is_empty() {
            return Err(Error::usage(format!(
                "candidate {kind:?} is not proposed for this input"
            )));
        }
    }
    let result = court::run(input, cands, limits)?;
    let report = EncodeReport {
        kind: result.kind,
        source_len: input.len() as u64,
        encoded_len: result.bytes.len() as u64,
        cost: result.cost,
        sha256_hex: to_hex(&sha256(input)),
        candidates_evaluated: result.candidates_evaluated,
        graph_ops: result.graph_ops,
    };
    Ok((result.bytes, report))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_identity() {
        let input: Vec<u8> = (0..=255u8).cycle().take(10_000).collect();
        let (bytes, report) = encode(&input, Limits::DEFAULT).unwrap();
        assert_eq!(report.source_len, input.len() as u64);
        assert_eq!(report.encoded_len, bytes.len() as u64);
        let (out, _) = crate::materialize::decode_to_bytes(&bytes, Limits::DEFAULT).unwrap();
        assert_eq!(out, input);
    }

    #[test]
    fn deterministic_output() {
        let input = b"determinism check".to_vec();
        let (a, _) = encode(&input, Limits::DEFAULT).unwrap();
        let (b, _) = encode(&input, Limits::DEFAULT).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn encode_with_none_matches_encode() {
        let input = b"the court must not care how it was invoked".repeat(40);
        let (auto, auto_report) = encode(&input, Limits::DEFAULT).unwrap();
        let (explicit, explicit_report) = encode_with(&input, Limits::DEFAULT, None).unwrap();
        assert_eq!(auto, explicit, "None must equal the unforced court");
        assert_eq!(auto_report.kind, explicit_report.kind);
        assert_eq!(auto_report.encoded_len, explicit_report.encoded_len);
        assert_eq!(
            auto_report.candidates_evaluated,
            explicit_report.candidates_evaluated
        );
    }

    #[test]
    fn encode_with_force_raw_is_exact() {
        // RAW normally loses to RLE on this input, so forcing RAW proves the
        // filter really overrides the court's own choice.
        let input = vec![0u8; 8192];
        let (_, auto_report) = encode(&input, Limits::DEFAULT).unwrap();
        assert_ne!(auto_report.kind, CandidateKind::Raw);

        let (bytes, report) =
            encode_with(&input, Limits::DEFAULT, Some(CandidateKind::Raw)).unwrap();
        assert_eq!(report.kind, CandidateKind::Raw);
        assert_eq!(report.candidates_evaluated, 1);
        let (out, _) = crate::materialize::decode_to_bytes(&bytes, Limits::DEFAULT).unwrap();
        assert_eq!(out, input);
    }

    #[cfg(feature = "rans")]
    #[test]
    fn encode_with_force_byte_rans_is_exact() {
        let input = b"The quick brown fox jumps over the lazy dog. ".repeat(800);
        let (bytes, report) =
            encode_with(&input, Limits::DEFAULT, Some(CandidateKind::ByteRans)).unwrap();
        assert_eq!(report.kind, CandidateKind::ByteRans);
        assert_eq!(report.candidates_evaluated, 1);
        let (out, _) = crate::materialize::decode_to_bytes(&bytes, Limits::DEFAULT).unwrap();
        assert_eq!(out, input);
    }

    #[test]
    fn encode_with_unproposed_kind_errors() {
        // A non-PDF cannot propose PDF_CHANNELS, so forcing it must fail with a
        // typed Usage error rather than silently falling back to another lane.
        let input = b"definitely not a pdf, just text".to_vec();
        let err = encode_with(&input, Limits::DEFAULT, Some(CandidateKind::PdfChannels))
            .expect_err("PDF_CHANNELS is not proposed for a non-PDF");
        assert_eq!(err.class(), crate::error::ErrorClass::Usage);
        assert!(
            err.message().contains("is not proposed"),
            "unexpected message: {}",
            err.message()
        );
    }
}
