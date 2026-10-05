#![no_main]

//! Fuzz the PDF lexical cover.
//!
//! Invariants asserted:
//! * arbitrary bytes either lex to a result or a typed `Err`, never an
//!   `InternalInvariant`;
//! * on success the cover is a contiguous partition of the whole input
//!   (`SpanSet::validate`) with a matching total length.

use libfuzzer_sys::fuzz_target;
use vole_document::adapter::pdf::lex;
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
    match lex(data, limits) {
        Ok(result) => {
            result
                .spans
                .validate(data.len() as u64)
                .expect("lex produced an invalid byte cover");
            assert_eq!(
                result.spans.total_len(),
                data.len() as u64,
                "lex cover total disagrees with the input length"
            );
        }
        Err(e) => assert_ne!(
            e.class(),
            ErrorClass::InternalInvariant,
            "lex reported an internal invariant"
        ),
    }
});
