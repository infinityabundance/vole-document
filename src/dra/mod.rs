//! The Document Reconstruction Algebra.

pub mod op;
pub mod program;

pub use op::Op;
pub use program::{Authority, CoverageMap, DRA_VERSION, Program, Span};
