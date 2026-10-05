//! Exact DEFLATE bitstream replay, built on `preflate-rs` (Phase 6).
//!
//! `preflate` reconstructs the *original* raw DEFLATE (RFC 1951) bitstream from
//! the decompressed plaintext plus a compact correction blob. This module wraps
//! that capability in the guarantees VOLE-Document requires:
//!
//! 1. **Bit-exactness before admission.** [`try_replay`] only returns a plan when
//!    a fresh replay of `(plaintext, corrections)` reproduces the raw DEFLATE
//!    payload byte-for-byte. A stream that cannot be reproduced is declined.
//! 2. **No panic, ever.** Both analysis and reconstruction are isolated behind
//!    [`std::panic::catch_unwind`]: `preflate` can panic on hostile correction
//!    data (observed: index-out-of-bounds and explicit `panic!`), so a panic is
//!    converted into a typed [`crate::ErrorClass::CodecReplay`] error rather than
//!    escaping. The catch is not a substitute for the whole-source SHA-256 court;
//!    it only bounds failure.
//! 3. **Bounded work.** Analysis runs with a pinned chain bound and a plaintext
//!    limit derived from [`Limits`], and a stream that is not fully consumed
//!    (a silently-truncated result) is declined.
//!
//! The caller owns zlib (RFC 1950) framing: `preflate` operates on the raw
//! DEFLATE payload, so the 2-byte zlib header and 4-byte Adler-32 trailer are
//! returned verbatim in the [`ReplayPlan`] and re-emitted as literal program
//! bytes. Adler-32 *regeneration* is deliberately not assumed here.

use std::panic::{self, AssertUnwindSafe};

use preflate_rs::{PreflateConfig, preflate_whole_deflate_stream, recreate_whole_deflate_stream};

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Size of the zlib (RFC 1950) 2-byte header.
pub const ZLIB_HEADER_LEN: usize = 2;
/// Size of the zlib (RFC 1950) 4-byte Adler-32 trailer.
pub const ZLIB_TRAILER_LEN: usize = 4;
/// Minimum viable zlib stream: header + at least one DEFLATE byte + trailer.
pub const MIN_ZLIB_LEN: usize = ZLIB_HEADER_LEN + 1 + ZLIB_TRAILER_LEN;

/// Pinned hash-chain lookup bound. Fixed so an encoded result does not depend on
/// a `preflate-rs` default changing between versions in the sealed universe.
pub const MAX_CHAIN_LENGTH: u32 = 4096;

/// A verified exact-replay plan for one zlib-wrapped stream.
///
/// The reconstruction is `header · raw_deflate(plaintext, corrections) · adler`.
/// `plaintext` is the decompressed data; `corrections` is the opaque,
/// version-coupled `preflate` state that makes the replay exact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayPlan {
    /// The exact 2-byte zlib header from the source.
    pub header: [u8; ZLIB_HEADER_LEN],
    /// Decompressed plaintext (stored as an object or entropy channel).
    pub plaintext: Vec<u8>,
    /// Opaque `preflate` correction blob (stored verbatim).
    pub corrections: Vec<u8>,
    /// The exact 4-byte Adler-32 trailer from the source.
    pub adler: [u8; ZLIB_TRAILER_LEN],
    /// Length of the original raw DEFLATE payload that the op reproduces.
    pub raw_len: u32,
}

/// Whether `bytes` begins with a structurally valid zlib (RFC 1950) header.
///
/// Checks only the header invariants: compression method 8 (DEFLATE), a window
/// size within the DEFLATE maximum (`CINFO <= 7`), and the mod-31 FCHECK. This
/// is a shape test, never authority: the exact-reconstruction gate decides.
pub fn zlib_header_valid(bytes: &[u8]) -> bool {
    if bytes.len() < ZLIB_HEADER_LEN {
        return false;
    }
    let cmf = bytes[0];
    let flg = bytes[1];
    let method = cmf & 0x0f;
    let cinfo = cmf >> 4;
    method == 8 && cinfo <= 7 && (u16::from(cmf) * 256 + u16::from(flg)).is_multiple_of(31)
}

/// The bounded `preflate` analysis configuration derived from `limits`.
///
/// `plain_text_limit` caps the decompressed size at the smaller of the output
/// and single-record limits, so an admitted plaintext is always storable as one
/// object. `verify_compression` is on: `preflate` internally recompresses and
/// checks, turning many otherwise-silent corruptions into errors.
fn config(limits: Limits) -> PreflateConfig {
    let cap = limits
        .max_output_bytes
        .min(u64::from(limits.max_record_len));
    PreflateConfig {
        max_chain_length: MAX_CHAIN_LENGTH,
        plain_text_limit: cap.min(usize::MAX as u64) as usize,
        verify_compression: true,
    }
}

/// Recreate a raw DEFLATE stream from plaintext plus `preflate` corrections.
///
/// Never panics: a `preflate` panic on malformed corrections is caught and
/// returned as [`crate::ErrorClass::CodecReplay`]. A returned `Ok` is **not**
/// by itself proof of exactness — callers must compare against the source.
pub fn replay_raw(plaintext: &[u8], corrections: &[u8]) -> Result<Vec<u8>> {
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        recreate_whole_deflate_stream(plaintext, corrections)
    }));
    match outcome {
        Ok(Ok(bytes)) => Ok(bytes),
        Ok(Err(e)) => Err(Error::codec_replay(format!("deflate replay failed: {e}"))),
        Err(_) => Err(Error::codec_replay(
            "deflate replay panicked on malformed correction state",
        )),
    }
}

/// Attempt an exact replay plan for a zlib-wrapped stream.
///
/// Returns `Some(plan)` only when the plan's replay reproduces the raw DEFLATE
/// payload **byte-for-byte** and every bound is respected. Returns `None`
/// (decline, never panic) for a stream that is not zlib-shaped, too short, not
/// fully consumed by the analyzer, larger than the limits, or not exactly
/// reproducible. Declining is always safe: the caller keeps the raw bytes.
pub fn try_replay(bytes: &[u8], limits: Limits) -> Option<ReplayPlan> {
    let total = bytes.len();
    if total < MIN_ZLIB_LEN || total > limits.max_input_bytes as usize {
        return None;
    }
    if !zlib_header_valid(bytes) {
        return None;
    }
    let raw = &bytes[ZLIB_HEADER_LEN..total - ZLIB_TRAILER_LEN];
    if raw.is_empty() {
        return None;
    }

    // Analysis itself can panic (a debug-build `u16` overflow inside preflate on
    // very large, highly repetitive input), so it is isolated too.
    let analyzed = panic::catch_unwind(AssertUnwindSafe(|| {
        preflate_whole_deflate_stream(raw, &config(limits))
    }))
    .ok()?
    .ok()?;
    let (chunk, plain) = analyzed;

    // Full consumption is mandatory: a too-small `plain_text_limit` can return a
    // silently truncated stream with `compressed_size < raw.len()`.
    if chunk.compressed_size != raw.len() {
        return None;
    }
    // A whole-stream first chunk has no dictionary prefix; a non-empty prefix
    // would mean the plaintext is not self-contained and we decline rather than
    // guess the concatenation order.
    if !plain.prefix().is_empty() {
        return None;
    }
    if plaintext_len_exceeds(plain.text(), limits) {
        return None;
    }
    if chunk.corrections.len() as u64 > u64::from(limits.max_record_len) {
        return None;
    }

    let plaintext = plain.text().to_vec();
    // The decisive gate: the exact replay must reproduce the raw payload.
    let replayed = replay_raw(&plaintext, &chunk.corrections).ok()?;
    if replayed != raw {
        return None;
    }

    let raw_len = u32::try_from(raw.len()).ok()?;
    let mut header = [0u8; ZLIB_HEADER_LEN];
    header.copy_from_slice(&bytes[..ZLIB_HEADER_LEN]);
    let mut adler = [0u8; ZLIB_TRAILER_LEN];
    adler.copy_from_slice(&bytes[total - ZLIB_TRAILER_LEN..]);

    Some(ReplayPlan {
        header,
        plaintext,
        corrections: chunk.corrections,
        adler,
        raw_len,
    })
}

/// Whether the plaintext exceeds the storable single-record limit.
fn plaintext_len_exceeds(plaintext: &[u8], limits: Limits) -> bool {
    plaintext.len() as u64 > u64::from(limits.max_record_len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write;

    fn zlib(data: &[u8], level: u32) -> Vec<u8> {
        let mut e = ZlibEncoder::new(Vec::new(), Compression::new(level));
        e.write_all(data).unwrap();
        e.finish().unwrap()
    }

    fn content_like() -> Vec<u8> {
        let mut v = Vec::new();
        for i in 0..400 {
            v.extend_from_slice(
                format!(
                    "BT /F1 12 Tf 72 {} Td (Invoice line {i:05} amount 456.78) Tj ET\n",
                    700 - (i % 40)
                )
                .as_bytes(),
            );
        }
        v
    }

    #[test]
    fn zlib_header_shape_is_checked() {
        assert!(zlib_header_valid(&[0x78, 0x9c, 0x00, 0x00, 0x00, 0x00]));
        assert!(zlib_header_valid(&[0x78, 0x01, 0x00, 0x00, 0x00, 0x00]));
        assert!(!zlib_header_valid(&[0x00, 0x00]));
        // Wrong method (3) even though mod-31 holds.
        assert!(!zlib_header_valid(&[0x3b, 0x00, 0x00, 0x00, 0x00, 0x00]));
        // Bad FCHECK.
        assert!(!zlib_header_valid(&[0x78, 0x9d, 0x00, 0x00, 0x00, 0x00]));
        assert!(!zlib_header_valid(&[0x78]));
    }

    #[test]
    fn replay_plan_round_trips_byte_exactly() {
        for (name, data, level) in [
            ("content", content_like(), 6),
            (
                "text",
                b"the quick brown fox jumps over the lazy dog. ".repeat(200),
                9,
            ),
            ("small", b"hello hello hello".to_vec(), 6),
            (
                "incompressible",
                (0..4096u32)
                    .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
                    .collect(),
                6,
            ),
        ] {
            let z = zlib(&data, level);
            let plan = try_replay(&z, Limits::DEFAULT)
                .unwrap_or_else(|| panic!("{name} must produce a replay plan"));
            // Reassemble exactly as the DRA op will.
            let mut rebuilt = Vec::new();
            rebuilt.extend_from_slice(&plan.header);
            rebuilt.extend_from_slice(&replay_raw(&plan.plaintext, &plan.corrections).unwrap());
            rebuilt.extend_from_slice(&plan.adler);
            assert_eq!(rebuilt, z, "{name} must replay byte-for-byte");
        }
    }

    #[test]
    fn replay_declines_non_zlib_and_short() {
        assert!(try_replay(b"not zlib at all", Limits::DEFAULT).is_none());
        assert!(try_replay(&[0x78, 0x9c, 0x00, 0x00, 0x00], Limits::DEFAULT).is_none());
        assert!(try_replay(&[], Limits::DEFAULT).is_none());
        assert!(try_replay(&[0x78, 0x9c], Limits::DEFAULT).is_none());
    }

    #[test]
    fn hostile_corrections_never_panic() {
        // `preflate` can panic on malformed correction state; replay_raw must
        // catch it and return a typed error. A caught panic may still print via
        // the default hook — that is log-only and does not affect the result.
        let plain = content_like();
        let mut state = 0x1234_5678_9abc_def0u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..48 {
            let mut blob = vec![0u8; (next() % 96) as usize];
            for b in blob.iter_mut() {
                *b = next() as u8;
            }
            // Must return Ok or Err, never unwind.
            let _ = replay_raw(&plain, &blob);
            let _ = replay_raw(&blob, &plain);
            let _ = replay_raw(&[], &blob);
        }
        assert!(replay_raw(&plain, &[]).is_err() || replay_raw(&plain, &[]).is_ok());
    }

    #[test]
    fn tiny_plaintext_limit_declines_instead_of_truncating() {
        let data = content_like();
        let z = zlib(&data, 6);
        let limits = Limits {
            max_record_len: 8,
            ..Limits::DEFAULT
        };
        // The plaintext cannot fit in 8 bytes, so the plan must decline rather
        // than return a silently truncated stream.
        assert!(try_replay(&z, limits).is_none());
    }
}
