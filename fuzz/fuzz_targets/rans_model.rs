#![no_main]

//! Fuzz the canonical entropy-model decoder.
//!
//! Invariants asserted:
//! * arbitrary model bytes never produce an `InternalInvariant`;
//! * a decoded model re-encodes to bytes that decode back to the *same* model
//!   (canonical stability);
//! * model encoding is deterministic.

use libfuzzer_sys::fuzz_target;
use vole_document::entropy::EntropyModel;
use vole_document::error::ErrorClass;

fuzz_target!(init: {
    // See deflate_replay.rs: replace libFuzzer's abort-before-unwind hook so the
    // library's deliberate catch_unwind boundaries work; uncaught panics abort.
    std::panic::set_hook(Box::new(|info| {
        eprintln!("fuzz panic (may be caught by a library boundary): {info}");
    }));
}, |data: &[u8]| -> () {
    let model = match EntropyModel::decode(data) {
        Ok(m) => m,
        Err(e) => {
            assert_ne!(
                e.class(),
                ErrorClass::InternalInvariant,
                "model decode reported an internal invariant"
            );
            return;
        }
    };

    let enc = model.encode().expect("a decoded model must re-encode");
    let model2 = EntropyModel::decode(&enc).expect("a re-encoded model must decode");
    assert_eq!(
        model.scale_bits, model2.scale_bits,
        "model re-encode changed scale_bits"
    );
    assert_eq!(
        model.frequencies, model2.frequencies,
        "model re-encode changed the frequency table"
    );

    let enc2 = model.encode().expect("model encode must be deterministic");
    assert_eq!(enc, enc2, "model encode is not deterministic");
});
