//! Candidate generators.
//!
//! Each generator proposes a bounded, deterministic reconstruction hypothesis.
//! Generators are cheap to decline and never trusted because their logic
//! "looks obvious": every candidate reaches the common court.

use crate::adapter::opaque;
use crate::container::Descriptor;
use crate::error::Result;
use crate::limits::Limits;

/// Stable candidate identity. The discriminant order is the final
/// deterministic tie-breaker when prices and work are equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum CandidateKind {
    /// Single literal object, literal program (Phase 1 floor).
    Raw = 0,
    /// Run-length generated repeats (Phase 2).
    Rle = 1,
    /// Typed byte rANS entropy channel (Phase 2).
    ByteRans = 2,
}

impl CandidateKind {
    /// Stable short name for reports and receipts.
    pub const fn name(self) -> &'static str {
        match self {
            CandidateKind::Raw => "RAW",
            CandidateKind::Rle => "RLE",
            CandidateKind::ByteRans => "BYTE_RANS",
        }
    }
}

/// A proposed descriptor together with its originating family.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Which family produced this proposal.
    pub kind: CandidateKind,
    /// The proposed descriptor.
    pub descriptor: Descriptor,
}

/// Generate the bounded candidate set for `input`.
pub fn propose(input: &[u8], limits: Limits) -> Result<Vec<Candidate>> {
    let out = vec![Candidate {
        kind: CandidateKind::Raw,
        descriptor: opaque::propose(input, limits)?,
    }];
    Ok(out)
}
