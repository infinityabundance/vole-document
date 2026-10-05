#![no_main]

//! Fuzz the PDF physical scanner.
//!
//! Invariants asserted:
//! * arbitrary bytes either scan to a result or a typed `Err`, never an
//!   `InternalInvariant`;
//! * on success the physical spans are a contiguous partition of the whole
//!   input (`PdfPhysical::validate`) with `total_len == input.len()`.

use libfuzzer_sys::fuzz_target;
use vole_document::adapter::pdf::physical;
use vole_document::error::ErrorClass;
use vole_document::limits::Limits;

fuzz_target!(init: {
    // See deflate_replay.rs: replace libFuzzer's abort-before-unwind hook so the
    // library's deliberate catch_unwind boundaries work; uncaught panics abort.
    std::panic::set_hook(Box::new(|info| {
        eprintln!("fuzz panic (may be caught by a library boundary): {info}");
    }));
}, |data: &[u8]| -> () {
    let limits = Limits::STRICT;
    match physical::scan(data, limits) {
        Ok(pdf) => {
            pdf.validate(data.len() as u64)
                .expect("scan produced an invalid physical cover");
            assert_eq!(
                pdf.total_len(),
                data.len() as u64,
                "scan cover total disagrees with the input length"
            );
        }
        Err(e) => assert_ne!(
            e.class(),
            ErrorClass::InternalInvariant,
            "scan reported an internal invariant"
        ),
    }
});
