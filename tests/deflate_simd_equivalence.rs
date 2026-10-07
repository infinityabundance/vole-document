#![cfg(all(feature = "miniz-simd", feature = "field"))]
//! Phase-15.5: prove `miniz_oxide`'s optional `simd` adler-32 path
//! (`simd-adler32`) is **output-preserving**.
//!
//! The `simd` feature swaps only the zlib adler-32 hasher; it never touches the
//! DEFLATE bit decoder, so the decoded bytes must be identical. This is a
//! genuine differential witness: the streams are produced by an *independent*
//! zlib implementation (`flate2`'s `zlib-rs` backend, a dev-dependency, never
//! `miniz_oxide`) and then decoded by `miniz_oxide` in whatever adler
//! configuration this binary was compiled with. For zlib streams `miniz_oxide`
//! verifies the trailing adler-32 against its own computation, so a wrong SIMD
//! adler would be a decode *error*, and a correct one must reproduce the source
//! bytes exactly.
//!
//! The test is gated on `miniz-simd` so it only runs (and only asserts anything)
//! when the SIMD path is actually compiled in — i.e. under `--all-features`.

use std::io::Write;

use flate2::Compression;
use flate2::write::{DeflateEncoder, ZlibEncoder};

fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

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

/// A battery spanning empty, tiny, highly compressible, repetitive, and
/// high-entropy (incompressible) inputs — the last exercises long literal runs,
/// while the repetitive inputs exercise match copies.
fn battery() -> Vec<Vec<u8>> {
    let mut v: Vec<Vec<u8>> = vec![
        Vec::new(),
        b"hello, world".to_vec(),
        vec![0u8; 200_000],
        b"abcdefghijklmnop".repeat(20_000),
        (0..=255u8).cycle().take(100_000).collect(),
    ];

    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..4 {
        let n = (xorshift64(&mut state) % 262_144) as usize;
        let mut buf = Vec::with_capacity(n);
        for _ in 0..n {
            buf.push((xorshift64(&mut state) & 0xFF) as u8);
        }
        v.push(buf);
    }
    v
}

#[test]
fn miniz_simd_zlib_decodes_identically_to_independent_encoder() {
    for (i, data) in battery().into_iter().enumerate() {
        let encoded = zlib_encode(&data);
        let decoded = miniz_oxide::inflate::decompress_to_vec_zlib(&encoded)
            .unwrap_or_else(|e| panic!("case {i}: miniz zlib decode failed: {:?}", e.status));
        assert_eq!(
            decoded, data,
            "case {i}: zlib round-trip must be byte-exact"
        );
    }
}

#[test]
fn miniz_simd_raw_deflate_decodes_identically_to_independent_encoder() {
    for (i, data) in battery().into_iter().enumerate() {
        let encoded = raw_deflate_encode(&data);
        let decoded = miniz_oxide::inflate::decompress_to_vec(&encoded)
            .unwrap_or_else(|e| panic!("case {i}: miniz raw decode failed: {:?}", e.status));
        assert_eq!(
            decoded, data,
            "case {i}: raw DEFLATE round-trip must be byte-exact"
        );
    }
}

/// The bounded entry points the library actually ships must agree with the
/// unbounded ones under the SIMD adler.
#[test]
fn miniz_simd_bounded_decode_matches_unbounded() {
    for data in battery() {
        let encoded = zlib_encode(&data);
        let unbounded = miniz_oxide::inflate::decompress_to_vec_zlib(&encoded).unwrap();
        let bounded =
            miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&encoded, data.len()).unwrap();
        assert_eq!(bounded, unbounded);
        assert_eq!(bounded, data);
    }
}
