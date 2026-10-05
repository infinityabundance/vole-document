//! The PDF adapter: byte-authoritative lexical cover for Phase 3.
//!
//! Subphase 3.1 provides only the lexical layer: a span partition of the raw
//! bytes with no structural interpretation. Structural passes build on this
//! cover so that every interpretation remains anchored to exact offsets.

pub mod adapter;
pub mod channels;
pub mod cos;
pub mod layout;
pub mod lexer;
pub mod physical;
pub mod samples;
pub mod span;

#[cfg(feature = "deflate-replay")]
pub mod replay;
#[cfg(feature = "rans")]
pub use adapter::propose_pdf_channels;
pub use adapter::{detect, propose_pdf};
pub use channels::{KIND_COUNT, TokenChannelPlan, join, kind_from_id, kind_id, split};
pub use cos::{
    FilterClass, LengthValue, body_as_u64, dict_filter, dict_has_length, dict_int_or_ref,
    dict_length, dict_name_value,
};
#[cfg(feature = "rans")]
pub use layout::propose_pdf_layout_rans;
pub use layout::{build_layout_plan, propose_pdf_layout};
pub use lexer::{LexIssue, LexResult, lex};
pub use physical::{
    LengthSource, ObjRole, PdfObjectSpan, PdfPhysical, PdfStreamSpan, PhysicalKind, PhysicalSpan,
    RevisionInfo, scan,
};
#[cfg(feature = "deflate-replay")]
pub use replay::propose_pdf_deflate_replay;
#[cfg(all(feature = "deflate-replay", feature = "rans"))]
pub use replay::propose_pdf_deflate_replay_rans;
#[cfg(feature = "deflate-replay")]
pub use replay::{DeflateStats, DeflateSummary, StreamStats, deflate_stats};
pub use samples::{is_negative_control, sample_pdfs};
pub use span::{Span, SpanKind, SpanSet};
