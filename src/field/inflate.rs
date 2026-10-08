//! The single shipped DEFLATE inflate seam (Phase 16.1).
//!
//! One backend — [`zlib_rs`]'s safe Rust `Inflate` API — serves every production
//! inflate: PDF `FlateDecode` zlib (RFC 1950) streams and ZIP method-8 bare
//! DEFLATE (RFC 1951) members. The wrapper is a parameter, not a second
//! implementation: a zlib-wrapped stream is decoded with the RFC 1950 header and
//! adler-32 trailer enabled, a raw member with the wrapper disabled.
//!
//! Inflating is a **deterministic derived projection** (`Q_gen`): any conforming
//! DEFLATE decoder yields the same bytes, so swapping the backend never changes
//! the exactness contract. The exact leaf of a stream is still its raw compressed
//! span; this recovers decoded stream state for observations only.
//!
//! Allocation matters as much as the decoder: a caller that knows the decoded
//! length passes it as a *tight* `cap` (one exact-sized buffer), while a caller
//! that only knows a *ceiling* (a stream whose length must be learned) must never
//! preallocate that ceiling — the output window grows from the compressed size
//! instead, so a tiny stream never zeroes a large buffer.
//!
//! [`miniz_oxide`] is retained as the differential reference for the
//! `deflate-ablation` harness and the byte-identity tests; it is **not** on this
//! path.

/// The compression wrapper around a DEFLATE bitstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wrapper {
    /// zlib (RFC 1950): a 2-byte header, the DEFLATE body, and a trailing
    /// adler-32. Used by PDF `FlateDecode`.
    Zlib,
    /// Bare DEFLATE (RFC 1951): no header and no checksum. Used by ZIP method 8.
    Raw,
}

/// A bounded inflate failure.
///
/// Every variant is a typed decline: a decode failure is never a panic and never
/// a partial or clamped answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InflateError {
    /// The stream would emit more than the caller's `cap` bytes.
    OutputExhausted,
    /// The input ended before a complete DEFLATE stream was consumed.
    Incomplete,
    /// The stream is malformed (bad header, blocks, or adler-32), or its decoded
    /// length did not fit in `usize`.
    Malformed,
}

impl core::fmt::Display for InflateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            InflateError::OutputExhausted => "decoded length exceeds the bound",
            InflateError::Incomplete => "truncated deflate stream",
            InflateError::Malformed => "malformed deflate stream",
        };
        f.write_str(s)
    }
}

/// The largest output buffer preallocated for a *tight* `cap`. A `cap` above
/// this (a hostile declared length) goes through the growth path, so a small
/// stream can never force a huge allocation.
const DIRECT_PREALLOC_MAX: usize = 256 * 1024 * 1024;

/// The initial output window for a stream whose decoded length is not known:
/// twice the compressed size, mirroring `miniz_oxide`'s `2 * input.len()`. It is
/// grown as needed, so a small stream never zeroes a large buffer.
fn initial_window(encoded: &[u8], cap: usize) -> usize {
    encoded.len().saturating_mul(2).max(1).min(cap.max(1))
}

/// Inflate `encoded` into at most `cap` bytes, requiring a clean `StreamEnd`.
///
/// Use this when `cap` is a **tight** bound (the declared decoded length): the
/// single exact-sized buffer is what the ablation measured. A `cap` above
/// [`DIRECT_PREALLOC_MAX`] falls back to the growing path. The returned bytes are
/// exactly those the stream encodes; a stream that would exceed `cap`, is
/// truncated, or is malformed is a typed error rather than a partial buffer.
pub(crate) fn inflate_bounded(
    encoded: &[u8],
    cap: usize,
    wrapper: Wrapper,
) -> Result<Vec<u8>, InflateError> {
    if cap <= DIRECT_PREALLOC_MAX {
        inflate_direct(encoded, cap, wrapper)
    } else {
        inflate_growing(encoded, cap, wrapper)
    }
}

/// Learn the decoded length of a stream without retaining its bytes.
///
/// `cap` is a **ceiling** that may be far larger than the output (e.g. the 32 MiB
/// ingest bound), so the output window grows from the compressed size and is
/// reused across growth: the total zeroed memory is the final window, not the
/// ceiling. Only `total_out` is kept, so no output is copied or retained.
pub(crate) fn inflate_len(
    encoded: &[u8],
    cap: usize,
    wrapper: Wrapper,
) -> Result<u64, InflateError> {
    let mut inflate = zlib_rs::Inflate::new(wrapper == Wrapper::Zlib, 15);
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = initial_window(encoded, cap);
    let mut consumed: u64 = 0;
    let mut produced: u64 = 0;
    loop {
        if buf.len() < chunk {
            buf.resize(chunk, 0u8);
        }
        let start = usize::try_from(consumed).map_err(|_| InflateError::Malformed)?;
        let remaining = encoded.get(start..).ok_or(InflateError::Malformed)?;
        let status =
            inflate.decompress(remaining, &mut buf[..chunk], zlib_rs::InflateFlush::Finish);
        let total_out = inflate.total_out();
        let total_in = inflate.total_in();
        match status {
            Ok(zlib_rs::Status::StreamEnd) => {
                if total_out > cap as u64 {
                    return Err(InflateError::OutputExhausted);
                }
                return Ok(total_out);
            }
            Ok(zlib_rs::Status::BufError) | Ok(zlib_rs::Status::Ok) => {
                // The stream did not end within the ceiling: it is exhausted.
                if total_out >= cap as u64 {
                    return Err(InflateError::OutputExhausted);
                }
                // No output and no input since the last call: the input ended
                // mid-stream (truncated) or the stream cannot make progress.
                if total_out == produced && total_in == consumed {
                    return Err(InflateError::Incomplete);
                }
                consumed = total_in;
                produced = total_out;
                chunk = chunk
                    .saturating_mul(2)
                    .min(cap.max(1))
                    .min(u32::MAX as usize);
            }
            Err(_) => return Err(InflateError::Malformed),
        }
    }
}

/// Single-call decode into an exactly-sized buffer. This is the path the
/// Phase-15.5 ablation measured; no extra copy is made.
fn inflate_direct(encoded: &[u8], cap: usize, wrapper: Wrapper) -> Result<Vec<u8>, InflateError> {
    // A zero-length stream still needs one output byte to observe `StreamEnd`.
    let mut out = vec![0u8; cap.max(1)];
    let mut inflate = zlib_rs::Inflate::new(wrapper == Wrapper::Zlib, 15);
    match inflate.decompress(encoded, &mut out, zlib_rs::InflateFlush::Finish) {
        Ok(zlib_rs::Status::StreamEnd) => {
            let n = usize::try_from(inflate.total_out()).map_err(|_| InflateError::Malformed)?;
            if n > cap {
                return Err(InflateError::OutputExhausted);
            }
            out.truncate(n);
            Ok(out)
        }
        // The output buffer filled before the stream ended.
        Ok(zlib_rs::Status::BufError) => Err(InflateError::OutputExhausted),
        // With `Finish`, a stream that cannot progress is reported as `BufError`;
        // a bare `Ok` here would mean it stopped mid-stream.
        Ok(zlib_rs::Status::Ok) => Err(InflateError::Incomplete),
        Err(_) => Err(InflateError::Malformed),
    }
}

/// Bounded-growth decode for a `cap` too large to preallocate safely, retaining
/// the decoded bytes: each call's new output is appended to `out`.
fn inflate_growing(encoded: &[u8], cap: usize, wrapper: Wrapper) -> Result<Vec<u8>, InflateError> {
    let mut inflate = zlib_rs::Inflate::new(wrapper == Wrapper::Zlib, 15);
    let mut out: Vec<u8> = Vec::new();
    let mut consumed: u64 = 0;
    let mut produced: u64 = 0;
    let mut chunk = initial_window(encoded, cap);
    loop {
        let mut buf = vec![0u8; chunk];
        let start = usize::try_from(consumed).map_err(|_| InflateError::Malformed)?;
        let remaining = encoded.get(start..).ok_or(InflateError::Malformed)?;
        let status = inflate.decompress(remaining, &mut buf, zlib_rs::InflateFlush::Finish);
        let total_out = inflate.total_out();
        let total_in = inflate.total_in();
        let written = usize::try_from(total_out - produced).map_err(|_| InflateError::Malformed)?;
        if written > buf.len() {
            return Err(InflateError::Malformed);
        }
        match status {
            Ok(zlib_rs::Status::StreamEnd) => {
                out.extend_from_slice(&buf[..written]);
                if out.len() > cap {
                    return Err(InflateError::OutputExhausted);
                }
                return Ok(out);
            }
            Ok(zlib_rs::Status::BufError) | Ok(zlib_rs::Status::Ok) => {
                out.extend_from_slice(&buf[..written]);
                if out.len() >= cap {
                    return Err(InflateError::OutputExhausted);
                }
                if written == 0 && total_in == consumed {
                    return Err(InflateError::Incomplete);
                }
                consumed = total_in;
                produced = total_out;
                chunk = chunk
                    .saturating_mul(2)
                    .min(cap.max(1))
                    .min(u32::MAX as usize);
            }
            Err(_) => return Err(InflateError::Malformed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Write;

    use flate2::Compression;
    use flate2::write::{DeflateEncoder, ZlibEncoder};

    fn zlib_encode(data: &[u8]) -> Vec<u8> {
        let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
        e.write_all(data).expect("zlib encode");
        e.finish().expect("zlib finish")
    }

    fn raw_deflate_encode(data: &[u8]) -> Vec<u8> {
        let mut e = DeflateEncoder::new(Vec::new(), Compression::default());
        e.write_all(data).expect("deflate encode");
        e.finish().expect("deflate finish")
    }

    fn battery() -> Vec<Vec<u8>> {
        vec![
            Vec::new(),
            b"hello, world".to_vec(),
            vec![0u8; 200_000],
            b"abcdefghijklmnop".repeat(20_000),
            (0..=255u8).cycle().take(100_000).collect(),
        ]
    }

    #[test]
    fn direct_path_roundtrips_both_wrappers() {
        for (i, data) in battery().into_iter().enumerate() {
            let z = zlib_encode(&data);
            assert_eq!(
                inflate_bounded(&z, data.len(), Wrapper::Zlib).unwrap(),
                data,
                "case {i}: zlib"
            );
            let r = raw_deflate_encode(&data);
            assert_eq!(
                inflate_bounded(&r, data.len(), Wrapper::Raw).unwrap(),
                data,
                "case {i}: raw"
            );
        }
    }

    /// The `cap > DIRECT_PREALLOC_MAX` path grows its window; a tiny stream must
    /// still decode without ever allocating the (huge) declared cap.
    #[test]
    fn growth_path_decodes_a_small_stream() {
        let data = b"growth path witness".to_vec();
        let z = zlib_encode(&data);
        let got = inflate_bounded(&z, DIRECT_PREALLOC_MAX + 1, Wrapper::Zlib).unwrap();
        assert_eq!(got, data);
    }

    /// `inflate_len` must agree with a retained decode for both wrappers and for
    /// a ceiling far above the output.
    #[test]
    fn len_path_agrees_with_bounded_and_never_overallocates() {
        for (i, data) in battery().into_iter().enumerate() {
            let z = zlib_encode(&data);
            let n = inflate_len(&z, DIRECT_PREALLOC_MAX + 1, Wrapper::Zlib).unwrap();
            assert_eq!(n as usize, data.len(), "case {i}: zlib len");
            let r = raw_deflate_encode(&data);
            let n = inflate_len(&r, 32 * 1024 * 1024 + 1, Wrapper::Raw).unwrap();
            assert_eq!(n as usize, data.len(), "case {i}: raw len");
        }
    }

    #[test]
    fn wrong_wrapper_is_a_typed_error() {
        let z = zlib_encode(b"wrapped");
        assert!(inflate_bounded(&z, 64, Wrapper::Raw).is_err());
    }

    #[test]
    fn truncated_and_corrupt_streams_are_typed_errors() {
        let data = b"abcdefghij".repeat(100);
        let z = zlib_encode(&data);
        assert!(inflate_bounded(&z[..z.len() / 2], data.len(), Wrapper::Zlib).is_err());
        assert!(inflate_len(&z[..z.len() / 2], 1 << 20, Wrapper::Zlib).is_err());
        let mut corrupt = z.clone();
        corrupt[0] ^= 0xFF; // break the zlib header
        assert!(inflate_bounded(&corrupt, data.len(), Wrapper::Zlib).is_err());
        assert!(inflate_len(&corrupt, 1 << 20, Wrapper::Zlib).is_err());
        // A valid stream with a cap one byte short must decline, never clamp.
        assert_eq!(
            inflate_bounded(&z, data.len() - 1, Wrapper::Zlib),
            Err(InflateError::OutputExhausted)
        );
        assert_eq!(
            inflate_len(&z, data.len() - 1, Wrapper::Zlib),
            Err(InflateError::OutputExhausted)
        );
    }
}
