//! Exact court: `materialize(encode(X)) == X` for an arbitrary binary corpus.
//!
//! Every case asserts the authoritative triple:
//!   materialized_length == source_length
//!   SHA256(materialized) == SHA256(source)
//!   byte_compare(materialized, source) == equal

use vole_document::encode;
use vole_document::integrity::sha256;
use vole_document::limits::Limits;
use vole_document::materialize;

/// Deterministic, seedable PRNG so corpus generation is reproducible.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // Avoid the zero fixed point of xorshift.
        Rng(seed | 1)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        let mut v = Vec::with_capacity(n + 8);
        while v.len() < n {
            v.extend_from_slice(&self.next_u64().to_le_bytes());
        }
        v.truncate(n);
        v
    }
}

fn exact_court(input: &[u8], label: &str) {
    let (bytes, report) = encode::encode(input, Limits::DEFAULT)
        .unwrap_or_else(|e| panic!("[{label}] encode failed: {e}"));
    let (out, parsed) = materialize::decode_to_bytes(&bytes, Limits::DEFAULT)
        .unwrap_or_else(|e| panic!("[{label}] decode failed: {e}"));

    assert_eq!(out.len(), input.len(), "[{label}] length mismatch");
    assert_eq!(out, input, "[{label}] byte mismatch");
    assert_eq!(sha256(&out), sha256(input), "[{label}] digest mismatch");
    assert_eq!(out.len() as u64, parsed.descriptor.source_len);
    assert_eq!(report.encoded_len, bytes.len() as u64);
    assert_eq!(report.cost.total(), bytes.len() as u64);

    // Deep verify agrees.
    let vr = materialize::verify(&bytes, Limits::DEFAULT).unwrap();
    assert_eq!(vr.source_len, input.len() as u64);
    assert_eq!(
        vr.sha256_hex,
        vole_document::integrity::to_hex(&sha256(input))
    );

    // Encoding is deterministic.
    let (again, _) = encode::encode(input, Limits::DEFAULT).unwrap();
    assert_eq!(bytes, again, "[{label}] encode not deterministic");
}

#[test]
fn empty() {
    exact_court(b"", "empty");
}

#[test]
fn single_byte_values() {
    for b in 0u8..=255 {
        exact_court(&[b], "single-byte");
    }
}

#[test]
fn all_byte_values() {
    let all: Vec<u8> = (0u8..=255).collect();
    exact_court(&all, "all-256");
}

#[test]
fn zeros() {
    exact_court(&vec![0u8; 64 * 1024], "zeros-64k");
}

#[test]
fn ones() {
    exact_court(&vec![0xFFu8; 4096], "ones-4k");
}

#[test]
fn sequential_cycle() {
    let data: Vec<u8> = (0..=255u8).cycle().take(1_000_000).collect();
    exact_court(&data, "sequential-1M");
}

#[test]
fn random_small() {
    let mut rng = Rng::new(0x1234_5678_9ABC_DEF0);
    for n in [0usize, 1, 2, 3, 7, 8, 15, 16, 31, 32, 100, 1000] {
        let data = rng.bytes(n);
        exact_court(&data, "random-small");
    }
}

#[test]
fn random_large_incompressible() {
    let mut rng = Rng::new(0xDEAD_BEEF_CAFE_F00D);
    let data = rng.bytes(8 * 1024 * 1024);
    exact_court(&data, "random-8MiB");
}

#[test]
fn text_like() {
    let text = "The quick brown fox jumps over the lazy dog.\n".repeat(5000);
    exact_court(text.as_bytes(), "text-5000-lines");
}

#[test]
fn tiny_boundary_sizes() {
    let mut rng = Rng::new(7);
    for n in 0usize..=64 {
        let data = rng.bytes(n);
        exact_court(&data, "boundary");
    }
}

#[test]
fn encoded_overhead_is_bounded_for_incompressible_data() {
    // The Phase-1 floor must not expand incompressible input beyond a small,
    // fixed framing overhead. This is the negative-control expansion bound.
    let mut rng = Rng::new(42);
    let data = rng.bytes(1 << 20);
    let (bytes, _) = encode::encode(&data, Limits::DEFAULT).unwrap();
    let overhead = bytes.len() - data.len();
    // Fixed framing overhead for the Phase-9 RAW descriptor (DRA v8 universe
    // plus the `+observation-index-v1+seek-directory-v1+external-objects-v1`
    // suffixes; this descriptor carries no directory record, so the only change
    // is the longer universe string).
    assert_eq!(overhead, 454, "unexpected fixed overhead");
}
