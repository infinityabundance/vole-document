//! The PDF adapter: byte-authoritative lexical cover for Phase 3.
//!
//! Subphase 3.1 provides only the lexical layer: a span partition of the raw
//! bytes with no structural interpretation. Structural passes build on this
//! cover so that every interpretation remains anchored to exact offsets.

pub mod lexer;
pub mod physical;
pub mod span;

pub use lexer::{LexIssue, LexResult, lex};
pub use physical::{PdfObjectSpan, PdfPhysical, PhysicalKind, PhysicalSpan, scan};
pub use span::{Span, SpanKind, SpanSet};
