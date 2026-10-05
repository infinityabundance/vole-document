//! Encoding: candidate portfolio, complete-cost court, and decode-before-commit.

pub mod candidates;
pub mod court;

use crate::accounting::CostBreakdown;
use crate::encode::candidates::CandidateKind;
use crate::error::Result;
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
    let cands = candidates::propose(input, limits)?;
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
}
