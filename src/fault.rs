//! Phase 20.1 crash-court fault injection (non-default `fault-inject` feature).
//!
//! [`hit`] aborts the process (`SIGABRT`) when the environment variable
//! `VOLE_FAULT_POINT` equals the named point, so the crash court can terminate
//! the packed writer deterministically at a named durability boundary instead of
//! relying on timing.
//!
//! This module is compiled **only** under the `fault-inject` feature, and every
//! call site is `#[cfg(feature = "fault-inject")]`-gated, so a build without the
//! feature contains neither the module nor the calls: wire bytes, on-disk
//! layout, performance, and behavior of the shipped build are unchanged.

/// Abort the process if `VOLE_FAULT_POINT == point`.
///
/// No-op when the variable is unset or names a different point.
pub fn hit(point: &str) {
    if std::env::var("VOLE_FAULT_POINT").as_deref() == Ok(point) {
        eprintln!("VOLE_FAULT_POINT={point} reached; aborting");
        std::process::abort();
    }
}
