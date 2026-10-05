//! The PDF adapter: byte-authoritative lexical cover for Phase 3.
//!
//! Subphase 3.1 provides only the lexical layer: a span partition of the raw
//! bytes with no structural interpretation. Structural passes build on this
//! cover so that every interpretation remains anchored to exact offsets.

pub mod cos;
pub mod lexer;
pub mod physical;
pub mod span;

pub use cos::{
    LengthValue, body_as_u64, dict_has_length, dict_int_or_ref, dict_length, dict_name_value,
};
pub use lexer::{LexIssue, LexResult, lex};
pub use physical::{
    LengthSource, ObjRole, PdfObjectSpan, PdfPhysical, PdfStreamSpan, PhysicalKind, PhysicalSpan,
    RevisionInfo, scan,
};
pub use span::{Span, SpanKind, SpanSet};
