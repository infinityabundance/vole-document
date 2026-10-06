//! # VOLE-Document
//!
//! A persistent procedural document runtime with byte-exact reconstruction.
//!
//! The governing invariants of the exact profile are:
//!
//! ```text
//! materialize(descriptor)  == original_bytes
//! materialize(field_root)  == original_bytes
//! ```
//!
//! Parsing, semantic equivalence, canonicalization, or a successful round trip
//! through a document writer are **not** substitutes for this invariant.
//!
//! ## Layered representation
//!
//! ```text
//! source bytes X
//!     -> deterministic format analysis (advisory; never destructive)
//!     -> bounded reconstruction hypothesis
//!     -> Document Reconstruction Algebra program + parameters + typed residuals
//!     -> typed entropy channels / rANS entropy-seed capsules
//!     -> canonical .voldoc descriptor
//!     -> bounded deterministic materializer
//!     -> X exactly
//! ```
//!
//! rANS is the *entropy substrate beneath* the representation. It is not the
//! procedural model, and an integer state is not a magic seed: a complete
//! decoder-entry capsule carries model, state, renormalization payload, counts,
//! and integrity.
//!
//! ## Persistent procedural field (Phase 11)
//!
//! From Phase 11 the persisted document is also a **queryable procedural field**
//! (feature `field`, on by default; see the `field` module): a content-addressed,
//! immutable procedural **seed DAG**, a bounded hierarchical observation index,
//! a typed observation API with provenance and `EXPLAIN`/`EXPLAIN ANALYZE`, and
//! selective late materialization. A narrow observation resolves its minimum
//! dependency closure (and, when warm, never opens the descriptor); the exact
//! original bytes always rematerialize, including after the source is deleted and
//! across process restarts. The exactness path (`materialize`) never depends on
//! the field, a cache, an index, or any search process.
//!
//! See `PROJECT_STATE.md` for the mechanism ledger, `FINDINGS.md` for the
//! authoritative measured results, `docs/adr/` for the decisions, and `SPEC.md`
//! for the wire format.

#![forbid(unsafe_code)]

pub mod accounting;
pub mod adapter;
pub mod codec;
pub mod container;
pub mod dra;
pub mod encode;
pub mod entropy;
pub mod error;
#[cfg(feature = "field")]
pub mod field;
pub mod integrity;
pub mod limits;
pub mod materialize;
pub mod store;

pub use error::{Error, ErrorClass, Result};
pub use limits::Limits;

/// The exactness profile mandated for the archival path.
///
/// Only `EXACT_BYTES` is normative. Canonical and semantic profiles are
/// deliberately absent until they can be named and measured separately.
pub const EXACTNESS_PROFILE_EXACT_BYTES: u8 = 0;

/// Identifier for the opaque source-format class.
pub const SOURCE_FORMAT_OPAQUE: u8 = 0;

/// Identifier for the PDF source-format class.
///
/// A descriptor declaring this class is still bound by the exact profile: its
/// program must reconstruct the original PDF bytes exactly. The class records
/// only how the source was analyzed, never a license to alter it.
pub const SOURCE_FORMAT_PDF: u8 = 1;
