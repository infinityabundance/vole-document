#![no_main]

//! Fuzz the exact `encode` → `decode` round trip.
//!
//! The fuzz input is treated as a source document (bounded so the candidate
//! court stays cheap). Invariants asserted:
//! * `encode` never reports an `InternalInvariant` for arbitrary bytes;
//! * freshly encoded bytes decode back to the exact input bytes;
//! * `encode` is deterministic (encoded twice → identical bytes).

use libfuzzer_sys::fuzz_target;
use vole_document::encode;
use vole_document::error::ErrorClass;
use vole_document::limits::Limits;
use vole_document::materialize;

const MAX_SOURCE: usize = 4096;

fuzz_target!(init: {
    // See deflate_replay.rs: replace libFuzzer's abort-before-unwind hook so the
    // library's deliberate catch_unwind boundaries work; uncaught panics abort.
    std::panic::set_hook(Box::new(|info| {
        eprintln!("fuzz panic (may be caught by a library boundary): {info}");
    }));
}, |data: &[u8]| -> () {
    let limits = Limits::STRICT;
    let input = &data[..data.len().min(MAX_SOURCE)];

    let (bytes, _report) = match encode::encode(input, limits) {
        Ok(v) => v,
        Err(e) => {
            assert_ne!(
                e.class(),
                ErrorClass::InternalInvariant,
                "encode of arbitrary bytes reported an internal invariant"
            );
            return;
        }
    };

    let (out, _parsed) = materialize::decode_to_bytes(&bytes, limits)
        .expect("freshly encoded bytes must decode under the same limits");
    assert_eq!(out, input, "encode/decode is not byte-exact");

    let (bytes2, _r2) = encode::encode(input, limits).expect("encode must be deterministic");
    assert_eq!(bytes, bytes2, "encode is not deterministic");
});
