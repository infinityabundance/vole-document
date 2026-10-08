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
use crate::materialize::materialize_in_place;

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
    let mut court = Court::new();
    for c in candidates {
        court.offer(input, c, limits)?;
    }
    court.finish()
}

/// Incremental complete-cost court.
///
/// [`Court::offer`] serializes, decode-round-trips, and prices **one** candidate,
/// keeping only the current best. A streaming caller (see
/// [`crate::encode::candidates::propose_each`]) therefore holds at most one
/// unpriced candidate's payload at a time instead of the whole portfolio, which
/// bounds encoder memory on large inputs.
pub struct Court {
    best: Option<(u64, usize, CandidateKind, Vec<u8>, CostBreakdown)>,
    evaluated: u32,
}

impl Court {
    /// An empty court.
    pub fn new() -> Self {
        Court {
            best: None,
            evaluated: 0,
        }
    }

    /// Price one candidate and retain it only if it beats the current best.
    pub fn offer(&mut self, input: &[u8], c: Candidate, limits: Limits) -> Result<()> {
        self.evaluated += 1;
        let Candidate { kind, descriptor } = c;
        let work = descriptor.program.ops.len();
        let (bytes, cost) = descriptor.serialize()?;

        // The descriptor's object payloads (a full copy of the source for the
        // RAW floor) are no longer needed once serialized. Drop them before the
        // decode round trip so the exactness proof holds at most the serialized
        // authority, never the authority *and* its source-sized input copy.
        drop(descriptor);

        // Decode-before-commit: the normative decoder must reproduce the source.
        let mut parsed = Descriptor::parse(&bytes, limits)?;
        let out = materialize_in_place(&mut parsed, limits)?;
        if out != input {
            return Err(Error::reconstruction_mismatch(format!(
                "candidate {} did not reproduce the source exactly",
                kind.name()
            )));
        }

        let total = cost.total();
        debug_assert_eq!(total, bytes.len() as u64);
        let key = (total, work, kind);
        let replace = match &self.best {
            None => true,
            Some((bt, bw, bk, ..)) => key < (*bt, *bw, *bk),
        };
        if replace {
            self.best = Some((total, work, kind, bytes, cost));
        }
        Ok(())
    }

    /// The winning candidate.
    pub fn finish(self) -> Result<CourtResult> {
        let (_, work, kind, bytes, cost) = self
            .best
            .ok_or_else(|| Error::internal_invariant("no candidates were evaluated"))?;
        Ok(CourtResult {
            kind,
            bytes,
            cost,
            candidates_evaluated: self.evaluated,
            graph_ops: work,
        })
    }
}

impl Default for Court {
    fn default() -> Self {
        Court::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::candidates;

    #[test]
    fn raw_candidate_wins_and_is_exact() {
        let input = b"some bytes that must round trip".to_vec();
        let cands = candidates::propose(&input, Limits::DEFAULT).unwrap();
        let expected = cands.len() as u32;
        let r = run(&input, cands, Limits::DEFAULT).unwrap();
        assert_eq!(r.kind, CandidateKind::Raw);
        // Every proposed candidate for a non-empty, in-limit input is priced, so
        // the evaluated count tracks the portfolio size (which varies with the
        // `rans` feature and the PDF adapter's proposals).
        assert_eq!(r.candidates_evaluated, expected);
        assert!(r.candidates_evaluated >= 2);
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
