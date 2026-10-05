//! # VOLE-Document
//!
//! Byte-exact procedural document storage.
//!
//! The governing invariant of the exact profile is:
//!
//! ```text
//! materialize(descriptor) == original_bytes
//! ```
//!
//! Parsing, semantic equivalence, canonicalization, or a successful round trip
//! through a document writer are **not** substitutes for this invariant.
//!
//! ## Layered representation
//!
//! A persisted `.voldoc` descriptor is a bounded, deterministic reconstruction
//! description:
//!
//! ```text
//! source bytes X
//!     -> deterministic format analysis (advisory; never destructive)
//!     -> bounded reconstruction hypothesis
//!     -> Document Reconstruction Algebra program + parameters + typed residuals
//!     -> (later) typed entropy channels / rANS entropy-seed capsules
//!     -> canonical .voldoc descriptor
//!     -> bounded deterministic materializer
//!     -> X exactly
//! ```
//!
//! rANS, when it eventually arrives, is the *entropy substrate beneath* the
//! representation. It is not the procedural model, and an integer state is not
//! a magic seed: a complete decoder-entry capsule carries model, state,
//! renormalization payload, counts, and integrity.
//!
//! ## Phase status
//!
//! This crate currently implements the **exact container core** (paper Phase A
//! / brief Phase 1): framing, typed errors, resource limits, integrity, the
//! literal Document Reconstruction Algebra, the coverage certificate, RAW exact
//! representation, the CLI, and decode-before-commit. No compression claim is
//! made yet.
//!
//! See `PROJECT_STATE.md` for the mechanism ledger and `SPEC.md` for the
//! provisional wire format.

#![forbid(unsafe_code)]

pub mod accounting;
pub mod adapter;
pub mod container;
pub mod dra;
pub mod encode;
pub mod entropy;
pub mod error;
pub mod integrity;
pub mod limits;
pub mod materialize;

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
