#![no_main]

//! Fuzz xref/revision construction and classic `/Prev` resolution.
//!
//! Fuzz bytes are wrapped in an incremental-update-shaped document so that the
//! revision builder and the `trailer`/`XRefStream` `/Prev` resolver are reached.
//! Invariants asserted:
//! * no panic and no `InternalInvariant`;
//! * revision records are contiguous, ordered, and bounded by the input;
//! * the physical cover is exact.

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
    let mut doc = Vec::with_capacity(data.len() + 64);
    doc.extend_from_slice(b"%PDF-1.7\n");
    doc.extend_from_slice(data);
    doc.extend_from_slice(b"\ntrailer\n<< /Size 3 /Prev 9 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n");
    if doc.len() as u64 > limits.max_input_bytes {
        return;
    }

    match physical::scan(&doc, limits) {
        Ok(pdf) => {
            pdf.validate(doc.len() as u64)
                .expect("scan produced an invalid physical cover");
            for (i, rev) in pdf.revisions.iter().enumerate() {
                assert_eq!(rev.index as usize, i, "revision indices must be contiguous");
                assert!(rev.start <= rev.end, "revision start exceeds its end");
                assert!(
                    rev.end <= doc.len() as u64,
                    "revision end exceeds the input length"
                );
            }
        }
        Err(e) => assert_ne!(
            e.class(),
            ErrorClass::InternalInvariant,
            "xref scan reported an internal invariant"
        ),
    }
});
