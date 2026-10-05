#![no_main]

//! Fuzz the exact-DEFLATE-replay wrapper with arbitrary `(plaintext,
//! corrections)` pairs.
//!
//! Invariants asserted:
//! * `replay_raw` returns `Ok`/typed `Err`, never panics (the `preflate` panic
//!   boundary is inside `replay_raw`; see the panic-hook note below), and never
//!   reports an `InternalInvariant`;
//! * replay is deterministic for the same bytes;
//! * work is bounded: the input is capped.
//!
//! Panic-hook note: libFuzzer installs a panic hook that aborts the process
//! *before* unwinding, which would defeat `replay_raw`'s deliberate
//! `catch_unwind` isolation (preflate 0.7.6 can panic on hostile correction
//! blobs). We replace it with a printing-but-non-aborting hook so the library's
//! boundary behaves as in production; genuinely uncaught panics still reach
//! libfuzzer-sys's outer `catch_unwind` and abort, so real crashes are still
//! reported.

use libfuzzer_sys::fuzz_target;
use vole_document::codec::deflate::replay_raw;
use vole_document::error::ErrorClass;

const MAX_INPUT: usize = 1 << 16;

fn install_unwinding_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("fuzz panic (may be caught by a library boundary): {info}");
    }));
}

fuzz_target!(init: { install_unwinding_panic_hook(); }, |data: &[u8]| -> () {
    let data = &data[..data.len().min(MAX_INPUT)];
    let split = if data.is_empty() {
        0
    } else {
        (data[0] as usize).wrapping_mul(31) % (data.len() + 1)
    };
    let (plaintext, corrections) = data.split_at(split);

    let first = replay_raw(plaintext, corrections);
    let second = replay_raw(plaintext, corrections);
    assert_eq!(
        first.is_ok(),
        second.is_ok(),
        "replay is not deterministic (ok/err)"
    );
    if let (Ok(a), Ok(b)) = (&first, &second) {
        assert_eq!(a, b, "replay is not deterministic (bytes)");
    }
    if let Err(e) = &first {
        assert_ne!(
            e.class(),
            ErrorClass::InternalInvariant,
            "replay reported an internal invariant"
        );
    }
});
