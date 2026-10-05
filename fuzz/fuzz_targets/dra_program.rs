#![no_main]

//! Fuzz the Document Reconstruction Algebra program decoder and evaluator.
//!
//! Arbitrary graph bytes are decoded, then analyzed and evaluated against small
//! synthetic object/channel tables. Invariants asserted:
//! * decode/analyze/eval never report an `InternalInvariant`;
//! * nothing allocates proportionally to a hostile declared length (`STRICT`
//!   limits plus a tiny table).

use libfuzzer_sys::fuzz_target;
use vole_document::dra::Program;
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
    let program = match Program::decode(data, limits) {
        Ok(p) => p,
        Err(e) => {
            assert_ne!(
                e.class(),
                ErrorClass::InternalInvariant,
                "DRA decode reported an internal invariant"
            );
            return;
        }
    };

    // Small synthetic tables: enough to exercise object/channel references,
    // too small for a hostile graph to allocate anything large.
    let objects: Vec<Vec<u8>> = vec![b"alpha".to_vec(), b"beta-gamma".to_vec(), Vec::new()];
    let channels: Vec<Vec<u8>> = vec![vec![0x41; 8], vec![0x42; 40], Vec::new()];

    match program.analyze_inputs(&objects, &channels, limits) {
        Ok((_len, coverage)) => {
            let _ = coverage.total_len();
        }
        Err(e) => assert_ne!(
            e.class(),
            ErrorClass::InternalInvariant,
            "DRA analyze reported an internal invariant"
        ),
    }

    if let Err(e) = program.eval(&objects, &channels, limits) {
        assert_ne!(
            e.class(),
            ErrorClass::InternalInvariant,
            "DRA eval reported an internal invariant"
        );
    }
});
