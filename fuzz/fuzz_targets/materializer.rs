#![no_main]

//! Fuzz the deterministic materializer's `verify` / `decode_to_bytes` entry
//! points on arbitrary bytes.
//!
//! Invariants asserted:
//! * no panic and no `InternalInvariant`;
//! * a successful `verify` respects the configured output policy;
//! * a successful `decode_to_bytes` reproduces exactly `source_len` bytes.

use libfuzzer_sys::fuzz_target;
use vole_document::error::ErrorClass;
use vole_document::limits::Limits;
use vole_document::materialize;

fuzz_target!(init: {
    // See deflate_replay.rs: replace libFuzzer's abort-before-unwind hook so the
    // library's deliberate catch_unwind boundaries work; uncaught panics abort.
    std::panic::set_hook(Box::new(|info| {
        eprintln!("fuzz panic (may be caught by a library boundary): {info}");
    }));
}, |data: &[u8]| -> () {
    let limits = Limits::STRICT;

    match materialize::verify(data, limits) {
        Ok(report) => {
            assert!(
                report.source_len <= limits.max_output_bytes,
                "verify exceeded the configured output policy"
            );
            assert_eq!(
                report.sha256_hex.len(),
                64,
                "sha256 hex must be 64 characters"
            );
        }
        Err(e) => assert_ne!(
            e.class(),
            ErrorClass::InternalInvariant,
            "verify reported an internal invariant"
        ),
    }

    match materialize::decode_to_bytes(data, limits) {
        Ok((out, parsed)) => {
            assert_eq!(
                out.len() as u64,
                parsed.descriptor.source_len,
                "materialized length disagrees with the declared source length"
            );
        }
        Err(e) => assert_ne!(
            e.class(),
            ErrorClass::InternalInvariant,
            "decode reported an internal invariant"
        ),
    }
});
