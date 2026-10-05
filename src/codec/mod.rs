//! Exact codec replay (Phase 6+).
//!
//! A codec-replay module inverse-proceduralizes an already-entropy-coded byte
//! region into `(plaintext, correction state)` and reproduces the **original
//! encoded bytes** deterministically. Replay is a *candidate*, never a blanket
//! transform, and never searches at decode time.

#[cfg(feature = "deflate-replay")]
pub mod deflate;
