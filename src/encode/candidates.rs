//! Candidate generators.
//!
//! Each generator proposes a bounded, deterministic reconstruction hypothesis.
//! Generators are cheap to decline and never trusted because their logic
//! "looks obvious": every candidate reaches the common court.

use crate::SOURCE_FORMAT_OPAQUE;
use crate::adapter::opaque;
use crate::container::{Descriptor, UNIVERSE};
use crate::dra::{Op, Program};
use crate::error::Result;
use crate::integrity::sha256;
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
///
/// Order is deterministic: RAW first, then RLE when it is expressible.
pub fn propose(input: &[u8], limits: Limits) -> Result<Vec<Candidate>> {
    let mut out = vec![Candidate {
        kind: CandidateKind::Raw,
        descriptor: opaque::propose(input, limits)?,
    }];
    if let Some(rle) = propose_rle(input, limits)? {
        out.push(rle);
    }
    Ok(out)
}

/// Propose an inline-run + `REPEAT_LAST` representation of `input`.
///
/// Each maximal run of equal bytes becomes one `INLINE` of that single byte,
/// optionally followed by a `REPEAT_LAST` repeating it `length - 1` more times.
/// The sequence is therefore always `INLINE, REPEAT_LAST, INLINE, ...`, so no
/// two `REPEAT_LAST` instructions are ever adjacent. `objects` is empty because
/// every byte is carried in the graph.
///
/// Returns `Ok(None)`, declining honestly, when the run list cannot be
/// expressed within `limits`: the program would need more instructions than
/// `max_graph_ops`, or a run is too long to reconstruct with a single
/// `REPEAT_LAST` (`length - 1` exceeds `max_repeat_count` or `u32::MAX`).
pub fn propose_rle(input: &[u8], limits: Limits) -> Result<Option<Candidate>> {
    // Maximal runs of equal bytes: (byte value, run length).
    let mut runs: Vec<(u8, u64)> = Vec::new();
    for &b in input {
        match runs.last_mut() {
            Some((last, len)) if *last == b => *len += 1,
            _ => runs.push((b, 1)),
        }
    }

    // Worst case is one INLINE plus one REPEAT_LAST per run.
    if runs.len().saturating_mul(2) > limits.max_graph_ops as usize {
        return Ok(None);
    }

    // A run must be reconstructible by a single INLINE + REPEAT_LAST pair.
    for &(_, length) in &runs {
        let extra = length - 1;
        if extra > limits.max_repeat_count || extra > u32::MAX as u64 {
            return Ok(None);
        }
    }

    let mut ops = Vec::with_capacity(runs.len() * 2);
    for &(byte, length) in &runs {
        ops.push(Op::Inline { bytes: vec![byte] });
        if length > 1 {
            ops.push(Op::RepeatLast {
                count: (length - 1) as u32,
            });
        }
    }

    let descriptor = Descriptor {
        universe: UNIVERSE.to_string(),
        source_format: SOURCE_FORMAT_OPAQUE,
        format_basis: "opaque;rle-runs".to_string(),
        models: vec![],
        channels: vec![],
        objects: vec![],
        program: Program::new(ops),
        source_sha256: sha256(input),
        source_len: input.len() as u64,
    };
    Ok(Some(Candidate {
        kind: CandidateKind::Rle,
        descriptor,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic xorshift64 PRNG for incompressible test data.
    fn xorshift64(state: &mut u64) -> u64 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *state = x;
        x
    }

    fn xorshift_bytes(n: usize, seed: u64) -> Vec<u8> {
        let mut state = seed | 1; // avoid the zero fixed point
        let mut out = Vec::with_capacity(n + 8);
        while out.len() < n {
            out.extend_from_slice(&xorshift64(&mut state).to_le_bytes());
        }
        out.truncate(n);
        out
    }

    fn assert_exact(bytes: &[u8], input: &[u8], limits: Limits) {
        let (out, parsed) = crate::materialize::decode_to_bytes(bytes, limits).unwrap();
        assert_eq!(out, input, "materialized bytes must equal the source");
        assert_eq!(out.len() as u64, parsed.descriptor.source_len);
        assert_eq!(
            crate::integrity::sha256(&out),
            crate::integrity::sha256(input)
        );
    }

    #[test]
    fn rle_wins_on_zeros() {
        let input = vec![0u8; 65536];
        let (bytes, report) = crate::encode::encode(&input, Limits::DEFAULT).unwrap();
        assert_eq!(report.kind, CandidateKind::Rle);
        assert!(
            report.encoded_len < 512,
            "RLE encoding of zeros was {} bytes",
            report.encoded_len
        );
        assert_exact(&bytes, &input, Limits::DEFAULT);
    }

    #[test]
    fn rle_wins_on_long_runs() {
        let mut input = Vec::new();
        input.extend_from_slice(&[0xAAu8; 1000]);
        input.extend_from_slice(&[0x00, 0x01]);
        input.extend_from_slice(&[0x55u8; 5000]);
        input.extend_from_slice(b"tail");

        let (bytes, report) = crate::encode::encode(&input, Limits::DEFAULT).unwrap();
        assert_eq!(report.kind, CandidateKind::Rle);
        assert!(report.encoded_len < 512);
        assert_exact(&bytes, &input, Limits::DEFAULT);
    }

    #[test]
    fn raw_wins_on_incompressible() {
        let input = xorshift_bytes(64 * 1024, 0x9E37_79B9_7F4A_7C15);
        let (bytes, report) = crate::encode::encode(&input, Limits::DEFAULT).unwrap();
        assert_eq!(
            report.kind,
            CandidateKind::Raw,
            "incompressible data must be stored RAW"
        );
        assert_exact(&bytes, &input, Limits::DEFAULT);
    }

    #[test]
    fn single_byte_is_rle() {
        let input = [7u8];
        let (bytes, report) = crate::encode::encode(&input, Limits::DEFAULT).unwrap();
        assert_eq!(report.kind, CandidateKind::Rle);
        assert!(report.encoded_len < 512);
        assert_exact(&bytes, &input, Limits::DEFAULT);
    }

    #[test]
    fn rle_declines_when_graph_too_big() {
        let input = vec![0u8; 100];
        let limits = Limits {
            max_graph_ops: 1,
            ..Limits::DEFAULT
        };
        assert!(
            propose_rle(&input, limits).unwrap().is_none(),
            "100 single-byte runs cannot fit in a one-op graph"
        );
        // The RAW candidate still reconstructs exactly under these limits.
        let (bytes, report) = crate::encode::encode(&input, limits).unwrap();
        assert_eq!(report.kind, CandidateKind::Raw);
        assert_exact(&bytes, &input, limits);
    }
}
