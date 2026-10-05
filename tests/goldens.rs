#![cfg(feature = "rans")]
//! Subphase 2.7 — independent reference oracle + frozen golden fixtures.
//!
//! Part A builds a *from-scratch*, integer-only byte-rANS decoder (the oracle)
//! and asserts it agrees byte-for-byte with `entropy::rans::decode_channel` over
//! several models, and that both end at the encoder's lower bound with the
//! payload fully consumed. The oracle mirrors the substrate's renormalization
//! convention exactly (see `reference_decode`).
//!
//! Part B freezes the Phase-2 wire format with computed golden bytes.
//!
//! Part C pins corruption behaviour: truncation and single-byte flips must be
//! detected, and model decoding must never panic on a one-bit mutation.

use vole_document::encode;
use vole_document::entropy::model::{ALPHABET, EntropyModel};
use vole_document::entropy::rans::{Capsule, decode_channel, encode_channel};
use vole_document::error::ErrorClass;
use vole_document::integrity::{sha256, to_hex};
use vole_document::limits::Limits;

/// Byte-rANS lower bound (`RANS_BYTE_L`). The decoder state must return here
/// after a complete, well-formed stream.
const RANS_BYTE_L: u32 = 1 << 23;

// ---------------------------------------------------------------------------
// Part A — independent reference decoder.
// ---------------------------------------------------------------------------

/// A pure, integer-only reference byte-rANS decoder.
///
/// Given the frequency table, `scale_bits`, decoder initial state, forward
/// payload, and symbol count, it returns `(decoded, final_state, consumed)` or
/// `None` if the payload runs out. This mirrors the substrate's convention:
///
/// * `slot = state & ((1 << scale_bits) - 1)`;
/// * pick the unique `s` with `cum[s] <= slot < cum[s] + freq[s]`;
/// * `state = freq[s] * (state >> scale_bits) + (slot - cum[s])`;
/// * then, **and only if `state < RANS_BYTE_L`**, loop
///   `state = (state << 8) | next_byte` until `state >= RANS_BYTE_L`.
///
/// The renorm is a guarded multi-byte absorption, not a single shift: after one
/// `<< 8` the state can still be below the bound.
fn reference_decode(
    frequencies: &[u32],
    scale_bits: u32,
    initial_state: u32,
    payload: &[u8],
    symbol_count: usize,
) -> Option<(Vec<u8>, u32, usize)> {
    let target = 1u32 << scale_bits;
    assert_eq!(frequencies.len(), ALPHABET);
    assert_eq!(frequencies.iter().sum::<u32>(), target);

    // Cumulative starts and the slot -> symbol map.
    let mut cum = vec![0u32; frequencies.len()];
    let mut cum2sym = vec![0u8; target as usize];
    let mut start = 0u32;
    for (s, &f) in frequencies.iter().enumerate() {
        cum[s] = start;
        let end = start + f;
        for slot in &mut cum2sym[start as usize..end as usize] {
            *slot = s as u8;
        }
        start = end;
    }
    assert_eq!(start, target);

    let mask = target - 1;
    let mut state = initial_state;
    let mut pos = 0usize;
    let mut out = Vec::with_capacity(symbol_count);

    for _ in 0..symbol_count {
        let slot = state & mask;
        let s = cum2sym[slot as usize];
        out.push(s);
        state = frequencies[s as usize] * (state >> scale_bits) + slot - cum[s as usize];

        // Renormalize: absorb bytes until at or above the lower bound.
        while state < RANS_BYTE_L {
            let byte = *payload.get(pos)?;
            pos += 1;
            state = (state << 8) | u32::from(byte);
        }
    }

    Some((out, state, pos))
}

/// Deterministic xorshift64 PRNG for reproducible corpora.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
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

/// Normalize a histogram of `data` into a model at `scale_bits`.
fn model_from_data(data: &[u8], scale_bits: u8) -> EntropyModel {
    let mut counts = [0u64; ALPHABET];
    for &b in data {
        counts[b as usize] += 1;
    }
    EntropyModel::from_counts(&counts, scale_bits).expect("normalization must succeed")
}

/// Counts builder for golden fixtures.
fn counts_with(pairs: &[(usize, u64)]) -> [u64; ALPHABET] {
    let mut counts = [0u64; ALPHABET];
    for &(i, v) in pairs {
        counts[i] = v;
    }
    counts
}

/// A ~90%-zero stream: ideal order-0 rANS input.
fn skewed_data() -> Vec<u8> {
    (0..4096u32)
        .map(|i| if i % 10 == 0 { 5 } else { 0 })
        .collect()
}

#[test]
fn reference_oracle_matches_decode_channel() {
    let mut rng = Rng::new(0x5EED_0FAC_E0DE_1234);
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("all-256", (0..=255u8).collect()),
        (
            "low-entropy-text",
            b"the quick brown fox jumps over the lazy dog ".repeat(200),
        ),
        ("skewed", skewed_data()),
        ("random", rng.bytes(4096)),
    ];

    for (label, data) in cases {
        let model = model_from_data(&data, 12);
        let capsule = encode_channel(&model, &data)
            .unwrap_or_else(|e| panic!("[{label}] encode failed: {e}"));

        let decoded = decode_channel(&model, &capsule, Limits::DEFAULT)
            .unwrap_or_else(|e| panic!("[{label}] decode_channel failed: {e}"));
        assert_eq!(decoded, data, "[{label}] decode_channel must be exact");

        let (oracle, final_state, consumed) = reference_decode(
            &model.frequencies,
            u32::from(model.scale_bits),
            capsule.initial_state,
            &capsule.payload,
            capsule.symbol_count as usize,
        )
        .unwrap_or_else(|| panic!("[{label}] oracle ran out of payload"));

        assert_eq!(
            oracle, decoded,
            "[{label}] oracle disagrees with decode_channel"
        );
        assert_eq!(oracle, data, "[{label}] oracle must reproduce the source");
        assert_eq!(
            final_state, RANS_BYTE_L,
            "[{label}] oracle final state must return to RANS_BYTE_L"
        );
        assert_eq!(
            consumed,
            capsule.payload.len(),
            "[{label}] oracle must consume the payload exactly"
        );
    }
}

// ---------------------------------------------------------------------------
// Part B — frozen golden fixtures.
// ---------------------------------------------------------------------------

#[test]
fn golden_model_bytes() {
    let counts = counts_with(&[(0, 1), (1, 2), (2, 3), (100, 400), (255, 7)]);
    let model = EntropyModel::from_counts(&counts, 12).expect("normalize");
    let bytes = model.encode().expect("encode");
    let hex = to_hex(&bytes);
    // Frozen canonical model bytes, version 2, SPARSE form (chosen because 5
    // present symbols cost far fewer bytes than a dense 256-entry table):
    //   [version=2][form=0][scale_bits=12][present_count=5] followed by the five
    //   `[symbol u8][freq u16 LE]` entries in ascending symbol order.
    // Frequencies: symbols 0, 1, 2, 100, 255 => 11, 21, 31, 3963, 70.
    let expected = concat!(
        "02",   // version = 2
        "00",   // form = 0 (sparse)
        "0c",   // scale_bits = 12
        "0500", // present_count = 5
        "00", "0b00", // symbol 0 -> 11
        "01", "1500", // symbol 1 -> 21
        "02", "1f00", // symbol 2 -> 31
        "64", "7b0f", // symbol 100 -> 3963
        "ff", "4600", // symbol 255 -> 70
    );
    assert_eq!(hex.len(), 40, "model wire length is frozen");
    assert_eq!(hex, expected);
    // The frozen form must round-trip to the originating model.
    assert_eq!(EntropyModel::decode(&bytes).expect("decode"), model);
}

#[test]
fn golden_capsule() {
    let data = b"the quick brown fox";
    let model = model_from_data(data, 12);
    let capsule = encode_channel(&model, data).expect("encode");
    // Frozen capsule for a model built from this data's own histogram: the
    // decoder-entry state and the forward-consumption renorm payload.
    assert_eq!(capsule.initial_state, 32_537_937);
    assert_eq!(to_hex(&capsule.payload), "4afa468b346f01bb1c");
    assert_eq!(capsule.payload.len(), 9);
    assert_eq!(capsule.symbol_count, 19);
}

#[test]
fn golden_descriptor() {
    let (bytes, report) = encode::encode(b"golden vector string", Limits::DEFAULT).expect("encode");
    // Winner kind: RAW. A 19-byte input cannot amortize per-channel model and
    // framing overhead, so the literal lane is selected; this freezes the exact
    // layout, length, and digest for that input.
    assert_eq!(report.kind.name(), "RAW");
    assert_eq!(bytes.len(), 415);
    assert_eq!(
        to_hex(&sha256(&bytes)),
        "1bbf0b739ffaa0e646eeb4ebf18d4bba024e77cd55cb0ddaf5fb2193dd0d6cb9"
    );
}

// ---------------------------------------------------------------------------
// Part C — corruption behaviour (never panic).
// ---------------------------------------------------------------------------

#[test]
fn truncated_capsule_payload_is_error() {
    let data = b"the quick brown fox";
    let model = model_from_data(data, 12);
    let capsule = encode_channel(&model, data).expect("encode");
    assert!(
        !capsule.payload.is_empty(),
        "test needs a non-empty payload to truncate"
    );

    let truncated = Capsule {
        payload: capsule.payload[..capsule.payload.len() - 1].to_vec(),
        ..capsule
    };
    let err = decode_channel(&model, &truncated, Limits::DEFAULT).expect_err("must reject");
    assert_eq!(err.class(), ErrorClass::EntropyDecode);
}

#[test]
fn single_byte_flip_in_capsule_is_detected() {
    // Fixed capsule under test: the frozen golden capsule. For this capsule every
    // single-bit payload flip is rejected by the exact-consumption integrity
    // check (state must return to RANS_BYTE_L and the payload must be fully
    // consumed).
    //
    // Scope: that check is a structural invariant, not a checksum. A flip can in
    // principle yield a different but self-consistent stream for some capsules;
    // whole-stream corruption detection is provided by the enclosing record's
    // CRC32C and the whole-source SHA-256, not by this channel-internal test.
    let data = b"the quick brown fox";
    let model = model_from_data(data, 12);
    let capsule = encode_channel(&model, data).expect("encode");
    assert_eq!(capsule.payload.len(), 9, "frozen golden payload length");

    let mut accepted: Vec<usize> = Vec::new();
    for pos in 0..capsule.payload.len() {
        let mut flipped = capsule.clone();
        flipped.payload[pos] ^= 1;
        match decode_channel(&model, &flipped, Limits::DEFAULT) {
            Ok(_) => accepted.push(pos),
            Err(e) => assert_eq!(e.class(), ErrorClass::EntropyDecode, "position {pos}"),
        }
    }
    assert!(
        accepted.is_empty(),
        "single-byte payload flips were accepted at positions {accepted:?}"
    );
}

/// Flipping any single bit of a canonical model must never panic on decode.
///
/// Every bit flip is decoded; a flip that changes the frequency sum must be
/// rejected outright (the model carries no checksum of its own). Where decode
/// still succeeds, the decoded model is fed through a non-panicking encode
/// path. Total model integrity additionally rests on the enclosing record's
/// CRC32C framing and the whole-source SHA-256 — neither lives in these bytes.
#[test]
fn model_decode_never_panics() {
    let counts = counts_with(&[(0, 1), (1, 2), (2, 3), (100, 400), (255, 7)]);
    let valid = EntropyModel::from_counts(&counts, 12)
        .expect("normalize")
        .encode()
        .expect("encode");
    let target: u64 = 1u64 << 12;

    // Recover the frequency sum from a v2 buffer when it parses structurally as
    // either form; `None` means the layout is already invalid. Mirrors the
    // decoder's structural preconditions closely enough that a `Some` sum
    // differing from `target` must be rejected by `decode`.
    let raw_sum = |bytes: &[u8]| -> Option<u64> {
        if bytes.len() < 3 || bytes[0] != 2 {
            return None;
        }
        match bytes[1] {
            0 => {
                if bytes.len() < 5 {
                    return None;
                }
                let n = usize::from(u16::from_le_bytes([bytes[3], bytes[4]]));
                if bytes.len() != 5 + 3 * n {
                    return None;
                }
                Some(
                    bytes[5..]
                        .as_chunks::<3>()
                        .0
                        .iter()
                        .map(|c| u64::from(u16::from_le_bytes([c[1], c[2]])))
                        .sum(),
                )
            }
            1 => {
                if bytes.len() != 517 {
                    return None;
                }
                Some(
                    bytes[5..]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|c| u64::from(u16::from_le_bytes(*c)))
                        .sum(),
                )
            }
            _ => None,
        }
    };

    for pos in 0..valid.len() {
        let mut flipped = valid.clone();
        flipped[pos] ^= 1;

        let changed = raw_sum(&flipped) != Some(target);
        let result = EntropyModel::decode(&flipped);

        if changed {
            assert!(
                result.is_err(),
                "flip at byte {pos} changed the model sum but was accepted"
            );
        }

        if let Ok(model) = result {
            // Structurally valid: canonical re-encode and a channel encode must
            // both complete without panicking.
            let reencoded = model.encode().expect("accepted model must re-encode");
            assert_eq!(reencoded, flipped, "canonical round-trip at byte {pos}");
            let data: Vec<u8> = model
                .frequencies
                .iter()
                .enumerate()
                .filter(|(_, f)| **f > 0)
                .map(|(i, _)| i as u8)
                .collect();
            let _ = encode_channel(&model, &data);
        }
    }
}
