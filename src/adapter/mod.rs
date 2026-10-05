//! Format adapters.
//!
//! An adapter proposes a reconstruction hypothesis. It never overrides the
//! byte authority: a hypothesis is only admitted after it reproduces the exact
//! source bytes and wins the complete-cost court.

pub mod opaque;

/// The source-format class decision for an input.
///
/// File extensions are hints, never authority. In Phase 1 every input is
/// preserved by the opaque adapter, which is the unconditional exact floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatDecision {
    /// Unstructured bytes; preserved exactly by the opaque adapter.
    Opaque,
}
