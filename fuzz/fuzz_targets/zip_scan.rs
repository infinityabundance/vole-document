#![no_main]

//! Fuzz the Phase-12 byte-authoritative ZIP physical scanner.
//!
//! Invariants (research I §8): arbitrary bytes either scan to a cover or a typed
//! `Err` (never `InternalInvariant`); a successful cover is a contiguous
//! partition of the whole input (`ZipPhysical::validate`) with `total_len ==
//! input.len()` and re-emits the input exactly.

use libfuzzer_sys::fuzz_target;
use vole_document::adapter::package::scan;
use vole_document::error::ErrorClass;
use vole_document::limits::Limits;

fuzz_target!(init: {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("fuzz panic (may be caught by a library boundary): {info}");
    }));
}, |data: &[u8]| -> () {
    match scan(data, Limits::STRICT) {
        Ok(physical) => {
            physical
                .validate(data.len() as u64)
                .expect("scan produced an invalid physical cover");
            assert_eq!(
                physical.total_len(),
                data.len() as u64,
                "scan cover total disagrees with the input length"
            );
            physical
                .reemits(data)
                .expect("scan cover does not re-emit the input exactly");
        }
        Err(e) => assert_ne!(
            e.class(),
            ErrorClass::InternalInvariant,
            "scan reported an internal invariant"
        ),
    }
});
