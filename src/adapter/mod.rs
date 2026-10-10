//! Format adapters.
//!
//! An adapter proposes a reconstruction hypothesis. It never overrides the
//! byte authority: a hypothesis is only admitted after it reproduces the exact
//! source bytes and wins the complete-cost court.

#[cfg(feature = "arrow")]
pub mod arrow;
#[cfg(feature = "cbor")]
pub mod cbor;
#[cfg(feature = "csv")]
pub mod csv;
#[cfg(feature = "docx")]
pub mod docx;
#[cfg(feature = "eml")]
pub mod eml;
#[cfg(feature = "epub")]
pub mod epub;
#[cfg(feature = "html")]
pub mod html;
#[cfg(feature = "json")]
pub mod json;
#[cfg(feature = "json5")]
pub mod json5;
#[cfg(feature = "jsonl")]
pub mod jsonl;
#[cfg(feature = "markdown")]
pub mod markdown;
#[cfg(feature = "odp")]
pub mod odp;
#[cfg(feature = "ods")]
pub mod ods;
#[cfg(feature = "odt")]
pub mod odt;
pub mod opaque;
#[cfg(feature = "package")]
pub mod package;
#[cfg(feature = "parquet")]
pub mod parquet;
pub mod pdf;
#[cfg(feature = "pptx")]
pub mod pptx;
#[cfg(feature = "toml")]
pub mod toml;
#[cfg(feature = "xlsx")]
pub mod xlsx;
#[cfg(feature = "xml")]
pub mod xml;
#[cfg(feature = "xml")]
pub(crate) mod xml_policy;
#[cfg(feature = "yaml")]
pub mod yaml;

/// The source-format class decision for an input.
///
/// File extensions are hints, never authority. In Phase 1 every input is
/// preserved by the opaque adapter, which is the unconditional exact floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatDecision {
    /// Unstructured bytes; preserved exactly by the opaque adapter.
    Opaque,
}
