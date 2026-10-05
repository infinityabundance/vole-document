//! Regression fixtures found by the Phase-7 coverage-guided fuzzing campaign.
//!
//! Every fixture is byte-exact and committed under `tests/fixtures/`. Each test
//! asserts the *fixed behavior* of the library for that hostile input, so a
//! future regression fails loudly here instead of only in a fuzz run.

#![cfg(feature = "deflate-replay")]

use vole_document::codec::deflate::{replay_raw, try_replay};
use vole_document::error::ErrorClass;
use vole_document::limits::Limits;

/// Reproduce the fuzz target's `(plaintext, corrections)` split.
fn split(artifact: &[u8]) -> (&[u8], &[u8]) {
    let at = if artifact.is_empty() {
        0
    } else {
        (artifact[0] as usize).wrapping_mul(31) % (artifact.len() + 1)
    };
    artifact.split_at(at)
}

/// Finding F1 — `preflate-rs` 0.7.6 does an unchecked `1 << params.window_bits`
/// while reconstructing from a correction blob (`hash_chain_holder.rs`). Hostile
/// corrections therefore panic with "attempt to shift left with overflow" under
/// debug assertions. `replay_raw` isolates the third-party panic behind
/// `catch_unwind` and returns a typed [`ErrorClass::CodecReplay`] error: the
/// reconstruction is declined, never unwound into the caller.
///
/// Found by the `deflate_replay` fuzz target (minimized to 35 bytes).
#[test]
fn replay_raw_isolates_preflate_shift_overflow_panic() {
    const FIXTURE: &[u8] = include_bytes!("fixtures/deflate_replay_shift_overflow.bin");
    let (plaintext, corrections) = split(FIXTURE);
    let err = replay_raw(plaintext, corrections)
        .expect_err("hostile corrections must be declined, not reconstructed");
    assert_eq!(
        err.class(),
        ErrorClass::CodecReplay,
        "hostile corrections must fail closed as CodecReplay, got {:?}",
        err.class()
    );
}

/// Finding F2 — `preflate-rs` 0.7.6 reconstructs with *unbounded* memory: a 33-byte
/// hostile `(plaintext, corrections)` pair drives a multi-gigabyte allocation
/// inside `recreate_whole_deflate_stream` (peak RSS 2532 MiB in the sealed
/// Phase-7.1 campaign). Our `replay_profile_limit` bounds only the *declared
/// output* length, not the third-party decoder's internal allocation; ADR-0016
/// already records that preflate 0.7.6 offers no bounded streaming
/// reconstruction sink. This is an upstream limitation with no in-tree fix that
/// avoids `unsafe` or process isolation.
///
/// The minimized fixture is committed as the durable record. It is deliberately
/// **not** passed to `replay_raw` here (that would exhaust memory in CI);
/// instead we assert the safe, fail-closed behavior of the public analysis entry
/// point.
#[test]
fn replay_unbounded_alloc_fixture_fails_closed_at_the_analysis_boundary() {
    const FIXTURE: &[u8] = include_bytes!("fixtures/deflate_replay_unbounded_alloc.bin");
    assert_eq!(FIXTURE.len(), 33, "committed OOM fixture changed");
    // A non-zlib byte stream is declined before any preflate reconstruction.
    assert!(
        try_replay(FIXTURE, Limits::DEFAULT).is_none(),
        "a non-zlib stream must be declined"
    );
}
