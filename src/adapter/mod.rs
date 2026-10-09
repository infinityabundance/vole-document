//! Format adapters.
//!
//! An adapter proposes a reconstruction hypothesis. It never overrides the
//! byte authority: a hypothesis is only admitted after it reproduces the exact
//! source bytes and wins the complete-cost court.

#[cfg(feature = "docx")]
pub mod docx;
#[cfg(feature = "epub")]
pub mod epub;
#[cfg(feature = "odp")]
pub mod odp;
#[cfg(feature = "ods")]
pub mod ods;
#[cfg(feature = "odt")]
pub mod odt;
pub mod opaque;
#[cfg(feature = "package")]
pub mod package;
pub mod pdf;
#[cfg(feature = "pptx")]
pub mod pptx;
#[cfg(feature = "xlsx")]
pub mod xlsx;

/// The source-format class decision for an input.
///
/// File extensions are hints, never authority. In Phase 1 every input is
/// preserved by the opaque adapter, which is the unconditional exact floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatDecision {
    /// Unstructured bytes; preserved exactly by the opaque adapter.
    Opaque,
}
