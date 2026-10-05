//! The complete-cost court.
//!
//! Every candidate is fully serialized, then round-tripped through the
//! normative decoder and byte-compared against the source, then priced from its
//! *actual* serialized bytes. No candidate is admitted on an entropy estimate
//! or on the plausibility of its logic.

use crate::accounting::CostBreakdown;
use crate::container::Descriptor;
use crate::encode::candidates::{Candidate, CandidateKind};
use crate::error::{Error, Result};
use crate::limits::Limits;
use crate::materialize::materialize;

/// The winning candidate after the court.
#[derive(Debug, Clone)]
pub struct CourtResult {
    /// Winning family.
    pub kind: CandidateKind,
    /// The exact serialized `.voldoc` bytes that passed the court.
    pub bytes: Vec<u8>,
    /// Physical byte attribution of `bytes`.
    pub cost: CostBreakdown,
    /// How many candidates were evaluated.
    pub candidates_evaluated: u32,
    /// Reconstruction work (DRA instruction count) of the winner.
    pub graph_ops: usize,
}

/// Run the court and return the strictly smallest exact candidate.
///
/// Deterministic tie-breaking: lower complete cost, then lower reconstruction
/// work, then lower [`CandidateKind`] ordinal.
pub fn run(input: &[u8], candidates: Vec<Candidate>, limits: Limits) -> Result<CourtResult> {
    let mut best: Option<(u64, usize, CandidateKind, Vec<u8>, CostBreakdown)> = None;
    let mut evaluated: u32 = 0;

    for c in candidates {
        evaluated += 1;
        let (bytes, cost) = c.descriptor.serialize()?;

        // Decode-before-commit: the normative decoder must reproduce the source.
        let parsed = Descriptor::parse(&bytes, limits)?;
        let out = materialize(&parsed, limits)?;
        if out != input {
            return Err(Error::reconstruction_mismatch(format!(
                "candidate {} did not reproduce the source exactly",
                c.kind.name()
            )));
        }

        let total = cost.total();
        debug_assert_eq!(total, bytes.len() as u64);
        let work = c.descriptor.program.ops.len();
        let key = (total, work, c.kind);
        let replace = match &best {
            None => true,
            Some((bt, bw, bk, ..)) => key < (*bt, *bw, *bk),
        };
        if replace {
            best = Some((total, work, c.kind, bytes, cost));
        }
    }

    let (_, work, kind, bytes, cost) =
        best.ok_or_else(|| Error::internal_invariant("no candidates were evaluated"))?;
    Ok(CourtResult {
        kind,
        bytes,
        cost,
        candidates_evaluated: evaluated,
        graph_ops: work,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::candidates;

    #[test]
    fn raw_candidate_wins_and_is_exact() {
        let input = b"some bytes that must round trip".to_vec();
        let cands = candidates::propose(&input, Limits::DEFAULT).unwrap();
        let r = run(&input, cands, Limits::DEFAULT).unwrap();
        assert_eq!(r.kind, CandidateKind::Raw);
        // RAW, RLE, and BYTE_RANS are all priced for a non-empty, in-limit input.
        assert_eq!(r.candidates_evaluated, 3);
        assert_eq!(r.cost.total(), r.bytes.len() as u64);
        let (out, _) = crate::materialize::decode_to_bytes(&r.bytes, Limits::DEFAULT).unwrap();
        assert_eq!(out, input);
    }

    #[test]
    fn empty_input_is_exact() {
        let input: Vec<u8> = Vec::new();
        let cands = candidates::propose(&input, Limits::DEFAULT).unwrap();
        let r = run(&input, cands, Limits::DEFAULT).unwrap();
        let (out, _) = crate::materialize::decode_to_bytes(&r.bytes, Limits::DEFAULT).unwrap();
        assert!(out.is_empty());
    }
}
