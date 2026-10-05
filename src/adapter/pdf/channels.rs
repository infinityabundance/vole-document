//! Typed lexical channels: a deterministic, exactly-reversible transposition of
//! a raw byte stream.
//!
//! [`split`] lexes `input` with the Phase-3.1 span cover and then "transposes" the
//! cover into three parallel views:
//!
//! - `kinds` — one stable kind id per token, in file order;
//! - `lengths` — one byte length per token, aligned with `kinds`;
//! - `payloads` — one concatenated byte stream per kind, in file order.
//!
//! [`join`] is the exact inverse: it replays the kind/length sequence against a
//! per-kind cursor into each payload and reproduces the original bytes
//! bit-for-bit.
//!
//! This is a *transposition*, not a compression: `split` performs no
//! interpretation and no modelling. Because the lexer cover is a partition, every
//! source byte is carried in exactly one payload, so `join(split(x)) == x` holds
//! by construction for every byte string; the payload total equals the input
//! length and no byte can be silently lost or invented.

use crate::error::{Error, ErrorClass, Result};
use crate::limits::Limits;

use super::lexer::lex;
use super::span::SpanKind;

/// Number of lexical kinds. The stable id space is exactly `0..KIND_COUNT`.
pub const KIND_COUNT: usize = 12;

/// Stable kind id for a [`SpanKind`]. The mapping is fixed, total, and injective.
pub const fn kind_id(k: SpanKind) -> u8 {
    match k {
        SpanKind::Whitespace => 0,
        SpanKind::Comment => 1,
        SpanKind::LiteralString => 2,
        SpanKind::HexString => 3,
        SpanKind::DictOpen => 4,
        SpanKind::DictClose => 5,
        SpanKind::ArrayOpen => 6,
        SpanKind::ArrayClose => 7,
        SpanKind::BraceOpen => 8,
        SpanKind::BraceClose => 9,
        SpanKind::Name => 10,
        SpanKind::Regular => 11,
    }
}

/// Inverse of [`kind_id`]; `None` for any id outside `0..KIND_COUNT`.
pub const fn kind_from_id(id: u8) -> Option<SpanKind> {
    Some(match id {
        0 => SpanKind::Whitespace,
        1 => SpanKind::Comment,
        2 => SpanKind::LiteralString,
        3 => SpanKind::HexString,
        4 => SpanKind::DictOpen,
        5 => SpanKind::DictClose,
        6 => SpanKind::ArrayOpen,
        7 => SpanKind::ArrayClose,
        8 => SpanKind::BraceOpen,
        9 => SpanKind::BraceClose,
        10 => SpanKind::Name,
        11 => SpanKind::Regular,
        _ => return None,
    })
}

/// A typed transposition of a byte stream into parallel channels.
///
/// The three views are aligned: `kinds[i]` and `lengths[i]` describe token `i`,
/// while `payloads[kinds[i]]` holds that token's bytes at the position determined
/// by the running total of preceding tokens of the same kind.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TokenChannelPlan {
    /// One kind id per token, in file order.
    pub kinds: Vec<u8>,
    /// Byte length of each token, aligned with `kinds`.
    pub lengths: Vec<u32>,
    /// `payloads[k]` is the concatenation of the bytes of every token of kind `k`,
    /// in file order.
    pub payloads: Vec<Vec<u8>>,
}

impl TokenChannelPlan {
    /// Number of tokens (equal to `kinds.len()` for a well-formed plan).
    pub fn token_count(&self) -> usize {
        self.kinds.len()
    }

    /// Total bytes represented (sum of `lengths`). Saturates rather than panicking
    /// on an absurd plan.
    pub fn total_len(&self) -> u64 {
        self.lengths
            .iter()
            .fold(0u64, |acc, &len| acc.saturating_add(u64::from(len)))
    }
}

/// Split `input` into typed channels.
///
/// Returns `Ok(None)` only when the lexeme count would exceed
/// `limits.max_pdf_spans` — an honest decline rather than a partial or lossy
/// plan. The concatenated payload is exactly `input.len()` bytes, so work is also
/// bounded by `limits.max_output_bytes`.
pub fn split(input: &[u8], limits: Limits) -> Result<Option<TokenChannelPlan>> {
    // The payloads together hold exactly `input.len()` bytes; refuse to build a
    // plan whose represented bytes exceed the declared output bound.
    if input.len() as u64 > limits.max_output_bytes {
        return Err(Error::resource_limit(format!(
            "pdf channel payload {} exceeds max_output_bytes {}",
            input.len(),
            limits.max_output_bytes
        )));
    }

    // `lex` either produces the exact cover or, when the span-count bound is
    // exceeded, reports `ResourceLimit` (its only use of that class). That is the
    // single decline path; any other failure propagates unchanged.
    let lexed = match lex(input, limits) {
        Ok(result) => result,
        Err(e) if e.class() == ErrorClass::ResourceLimit => return Ok(None),
        Err(e) => return Err(e),
    };

    let spans = &lexed.spans.spans;
    let mut plan = TokenChannelPlan {
        kinds: Vec::with_capacity(spans.len()),
        lengths: Vec::with_capacity(spans.len()),
        payloads: vec![Vec::new(); KIND_COUNT],
    };

    for span in spans {
        let len = u32::try_from(span.len)
            .map_err(|_| Error::resource_limit(format!("span length {} exceeds u32", span.len)))?;
        let id = kind_id(span.kind);
        let start = span.start as usize;
        let end = start + span.len as usize;
        plan.kinds.push(id);
        plan.lengths.push(len);
        plan.payloads[id as usize].extend_from_slice(&input[start..end]);
    }

    Ok(Some(plan))
}

/// Reconstruct the exact bytes from a plan; the inverse of [`split`].
///
/// Validates that `kinds` and `lengths` are aligned, that every kind id is in
/// `0..KIND_COUNT`, and that every per-kind cursor reads within its payload.
/// Finally, every payload byte must be consumed and the output must equal
/// `total_len`; otherwise the plan is rejected rather than silently truncated.
pub fn join(plan: &TokenChannelPlan, limits: Limits) -> Result<Vec<u8>> {
    if plan.kinds.len() != plan.lengths.len() {
        return Err(Error::invalid_pdf_structure(format!(
            "kinds ({}), lengths ({}) are misaligned",
            plan.kinds.len(),
            plan.lengths.len()
        )));
    }
    if plan.payloads.len() != KIND_COUNT {
        return Err(Error::invalid_pdf_structure(format!(
            "expected {KIND_COUNT} payload streams, found {}",
            plan.payloads.len()
        )));
    }

    let total = plan.total_len();
    if total > limits.max_output_bytes {
        return Err(Error::resource_limit(format!(
            "plan total {total} exceeds max_output_bytes {}",
            limits.max_output_bytes
        )));
    }
    let capacity = usize::try_from(total)
        .map_err(|_| Error::coverage_violation("plan total exceeds the address space"))?;

    let mut cursors = [0usize; KIND_COUNT];
    let mut out = Vec::with_capacity(capacity);

    for (&id, &len) in plan.kinds.iter().zip(plan.lengths.iter()) {
        let k = id as usize;
        if k >= KIND_COUNT {
            return Err(Error::invalid_pdf_structure(format!(
                "kind id {id} is outside 0..{KIND_COUNT}"
            )));
        }
        let payload = &plan.payloads[k];
        let start = cursors[k];
        let end = start
            .checked_add(len as usize)
            .ok_or_else(|| Error::coverage_violation("channel cursor overflow"))?;
        if end > payload.len() {
            return Err(Error::coverage_violation(format!(
                "kind {id} needs bytes {start}..{end} but its payload holds {}",
                payload.len()
            )));
        }
        out.extend_from_slice(&payload[start..end]);
        cursors[k] = end;
    }

    for ((id, &cursor), payload) in cursors.iter().enumerate().zip(plan.payloads.iter()) {
        if cursor != payload.len() {
            return Err(Error::coverage_violation(format!(
                "kind {id} left {} payload bytes unconsumed",
                payload.len() - cursor
            )));
        }
    }

    if out.len() as u64 != total {
        return Err(Error::coverage_violation(format!(
            "joined {} bytes but the plan declares {total}",
            out.len()
        )));
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::pdf::samples::sample_pdfs;

    fn xorshift64(state: &mut u64) -> u64 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *state = x;
        x
    }

    #[test]
    fn kind_map_is_a_bijection_onto_0_to_12() {
        let all = [
            SpanKind::Whitespace,
            SpanKind::Comment,
            SpanKind::LiteralString,
            SpanKind::HexString,
            SpanKind::DictOpen,
            SpanKind::DictClose,
            SpanKind::ArrayOpen,
            SpanKind::ArrayClose,
            SpanKind::BraceOpen,
            SpanKind::BraceClose,
            SpanKind::Name,
            SpanKind::Regular,
        ];
        let mut seen = [false; KIND_COUNT];
        for k in all {
            let id = kind_id(k);
            assert!((id as usize) < KIND_COUNT, "id {id} out of range");
            assert!(!seen[id as usize], "duplicate id {id}");
            seen[id as usize] = true;
            assert_eq!(kind_from_id(id), Some(k));
        }
        assert!(seen.iter().all(|&s| s), "every id must be used");
        assert_eq!(kind_from_id(KIND_COUNT as u8), None);
        assert_eq!(kind_from_id(u8::MAX), None);
    }

    #[test]
    fn split_then_join_is_identity() {
        for (name, bytes) in sample_pdfs() {
            let plan = split(&bytes, Limits::DEFAULT)
                .unwrap()
                .unwrap_or_else(|| panic!("{name} must split"));
            let out = join(&plan, Limits::DEFAULT).unwrap();
            assert_eq!(out, bytes, "{name} must rejoin byte-exactly");
        }
    }

    #[test]
    fn deterministic() {
        for (name, bytes) in sample_pdfs() {
            let a = split(&bytes, Limits::DEFAULT).unwrap();
            let b = split(&bytes, Limits::DEFAULT).unwrap();
            assert_eq!(a, b, "{name} split must be deterministic");
        }
    }

    #[test]
    fn payload_partition() {
        for (name, bytes) in sample_pdfs() {
            let plan = split(&bytes, Limits::DEFAULT).unwrap().unwrap();
            assert_eq!(plan.kinds.len(), plan.lengths.len(), "{name} alignment");
            assert_eq!(plan.kinds.len(), plan.token_count(), "{name} token_count");
            assert!(
                plan.kinds.iter().all(|&k| (k as usize) < KIND_COUNT),
                "{name} kind ids in range"
            );
            let payload_sum: u64 = plan.payloads.iter().map(|p| p.len() as u64).sum();
            assert_eq!(payload_sum, plan.total_len(), "{name} payload sum");
            assert_eq!(plan.total_len(), bytes.len() as u64, "{name} total_len");
        }
    }

    #[test]
    fn random_roundtrip() {
        let mut state: u64 = 0x243F_6A88_85A3_08D3;
        let mut split_count = 0usize;
        for i in 0..500 {
            let len = (xorshift64(&mut state) % 400) as usize;
            let mut buf = Vec::with_capacity(len);
            for _ in 0..len {
                buf.push((xorshift64(&mut state) & 0xFF) as u8);
            }
            if let Some(plan) = split(&buf, Limits::DEFAULT).unwrap() {
                split_count += 1;
                let out = join(&plan, Limits::DEFAULT).unwrap();
                assert_eq!(out, buf, "case {i} must rejoin byte-exactly");
            }
        }
        assert!(split_count > 0, "the court must actually exercise split");
    }

    #[test]
    fn declines_when_too_many_tokens() {
        let limits = Limits {
            max_pdf_spans: 1,
            ..Limits::DEFAULT
        };
        // `a b` lexes to three spans (Regular, Whitespace, Regular).
        assert!(
            split(b"a b", limits).unwrap().is_none(),
            "a token count above max_pdf_spans must decline honestly"
        );
    }

    #[test]
    fn join_rejects_malformed_plan() {
        let limits = Limits::DEFAULT;

        // A kind id outside 0..KIND_COUNT.
        let bad_kind = TokenChannelPlan {
            kinds: vec![KIND_COUNT as u8],
            lengths: vec![1],
            payloads: vec![Vec::new(); KIND_COUNT],
        };
        assert_eq!(
            join(&bad_kind, limits).unwrap_err().class(),
            ErrorClass::InvalidPdfStructure
        );

        // Payload bytes left unconsumed: one kind-0 token of length 1 against a
        // two-byte kind-0 payload.
        let mut payloads = vec![Vec::new(); KIND_COUNT];
        payloads[0] = b"ab".to_vec();
        let leftover = TokenChannelPlan {
            kinds: vec![0],
            lengths: vec![1],
            payloads,
        };
        assert_eq!(
            join(&leftover, limits).unwrap_err().class(),
            ErrorClass::CoverageViolation
        );

        // A token that overruns its payload must error, not panic.
        let overrun = TokenChannelPlan {
            kinds: vec![0],
            lengths: vec![5],
            payloads: vec![Vec::new(); KIND_COUNT],
        };
        assert_eq!(
            join(&overrun, limits).unwrap_err().class(),
            ErrorClass::CoverageViolation
        );

        // Misaligned kinds and lengths.
        let misaligned = TokenChannelPlan {
            kinds: vec![0, 0],
            lengths: vec![1],
            payloads: vec![Vec::new(); KIND_COUNT],
        };
        assert_eq!(
            join(&misaligned, limits).unwrap_err().class(),
            ErrorClass::InvalidPdfStructure
        );
    }
}
