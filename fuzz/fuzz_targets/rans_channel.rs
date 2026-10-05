#![no_main]

//! Fuzz the single-lane order-0 byte rANS channel decoder.
//!
//! A model is derived from the fuzz bytes and a capsule is assembled from
//! other fuzz bytes. Invariants asserted:
//! * `decode_channel` returns `Ok`/typed `Err`, never panics and never reports
//!   an `InternalInvariant`;
//! * a successful decode reproduces exactly `decoded_length` bytes;
//! * a successful decode is canonically stable (re-encode → decode again).

use libfuzzer_sys::fuzz_target;
use vole_document::entropy::EntropyModel;
use vole_document::entropy::rans::{Capsule, decode_channel, encode_channel};
use vole_document::error::ErrorClass;
use vole_document::limits::Limits;

/// A deliberately tight policy: each decode may allocate at most 64 KiB, so a
/// hostile `symbol_count` cannot make the fuzzer allocate unboundedly.
fn policy() -> Limits {
    Limits {
        max_channel_symbols: 1 << 16,
        max_output_bytes: 1 << 16,
        max_record_len: 1 << 16,
        ..Limits::STRICT
    }
}

fuzz_target!(init: {
    // See deflate_replay.rs: replace libFuzzer's abort-before-unwind hook so the
    // library's deliberate catch_unwind boundaries work; uncaught panics abort.
    std::panic::set_hook(Box::new(|info| {
        eprintln!("fuzz panic (may be caught by a library boundary): {info}");
    }));
}, |data: &[u8]| -> () {
    if data.len() < 2 {
        return;
    }
    let limits = policy();

    // Derive a valid model from the fuzz bytes.
    let scale_bits = 8 + (data[0] % 5); // 8..=12
    let mut counts = [0u64; 256];
    for &b in &data[1..] {
        counts[b as usize] = counts[b as usize].saturating_add(1);
    }
    let model = match EntropyModel::from_counts(&counts, scale_bits) {
        Ok(m) => m,
        Err(e) => {
            assert_ne!(
                e.class(),
                ErrorClass::InternalInvariant,
                "from_counts reported an internal invariant"
            );
            return;
        }
    };

    // Derive arbitrary capsule fields from the same bytes.
    let rest = &data[1..];
    let mut state_bytes = [0u8; 4];
    for (i, b) in rest.iter().take(4).enumerate() {
        state_bytes[i] ^= *b;
    }
    let mut count_bytes = [0u8; 8];
    for (i, b) in rest.iter().skip(4).take(8).enumerate() {
        count_bytes[i] ^= *b;
    }
    let symbol_count = u64::from_le_bytes(count_bytes) % (1u64 << 20);
    let payload = if rest.len() > 12 {
        rest[12..].to_vec()
    } else {
        Vec::new()
    };

    let capsule = Capsule {
        initial_state: u32::from_le_bytes(state_bytes),
        payload,
        symbol_count,
        decoded_length: symbol_count,
    };

    match decode_channel(&model, &capsule, limits) {
        Ok(out) => {
            assert_eq!(
                out.len() as u64,
                capsule.decoded_length,
                "decoded length disagrees with the capsule"
            );
            let re = encode_channel(&model, &out).expect("re-encoding decoded bytes must succeed");
            let out2 = decode_channel(&model, &re, limits)
                .expect("re-decoding a freshly encoded capsule must succeed");
            assert_eq!(out, out2, "channel round trip is not stable");
        }
        Err(e) => assert_ne!(
            e.class(),
            ErrorClass::InternalInvariant,
            "channel decode reported an internal invariant"
        ),
    }
});
