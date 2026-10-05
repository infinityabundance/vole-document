#![no_main]

//! Fuzz the `.voldoc` container parser and materializer.
//!
//! Invariants asserted:
//! * hostile bytes never panic and never yield an `InternalInvariant` (a
//!   malformed input is always a typed error, never a claimed implementation bug);
//! * a successful materialization reproduces exactly `source_len` bytes;
//! * materialization is deterministic for the same bytes.

use libfuzzer_sys::fuzz_target;
use vole_document::container::Descriptor;
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
    let parsed = match Descriptor::parse(data, limits) {
        Ok(p) => p,
        Err(e) => {
            assert_ne!(
                e.class(),
                ErrorClass::InternalInvariant,
                "parse of untrusted input reported an internal invariant"
            );
            return;
        }
    };

    match materialize::materialize(&parsed, limits) {
        Ok(out) => {
            assert_eq!(
                out.len() as u64,
                parsed.descriptor.source_len,
                "materialized length disagrees with the declared source length"
            );
            let again = materialize::materialize(&parsed, limits)
                .expect("a materializable descriptor must materialize deterministically");
            assert_eq!(out, again, "materialize is not deterministic");
        }
        Err(e) => assert_ne!(
            e.class(),
            ErrorClass::InternalInvariant,
            "materialize of untrusted input reported an internal invariant"
        ),
    }
});
