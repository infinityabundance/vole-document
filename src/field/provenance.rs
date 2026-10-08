//! Provenance and typed answers for field observations (Phase 11.5, ADR-0026).
//!
//! Every observation returns a [`FieldAnswer`] that carries its epistemic
//! [`Basis`], the scope over which integrity was actually verified, the exact
//! source span (when one exists), and the dependency ids that were read. An
//! authored or directly-observed answer is exact; a derived, inferred, or
//! heuristic answer is **never** labelled exact, so `Q_ref` and `Q_gen` cannot be
//! blurred (ADR-0026, DEC-8).

use crate::store::NodeId;

/// The epistemic basis of an answer.
///
/// `is_exact` is true only for answers that are byte-identical observations of
/// the source. A deterministically-derived projection (e.g. inflated stream
/// bytes) is reproducible but is not an exact source span, so it is not exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Basis {
    /// The value was authored as a literal (never produced by this module yet;
    /// reserved for `Literal` seed nodes).
    Authored,
    /// The exact source bytes were observed directly.
    DirectlyObserved,
    /// A deterministic projection of retained state (inflated bytes, structure).
    DeterministicallyDerived,
    /// Inferred from partial evidence; never exact.
    Inferred,
    /// A best-effort heuristic projection (e.g. text runs); never exact.
    Heuristic,
    /// Supplied by an explicit external context beside the field (Phase 20.4),
    /// e.g. corpus/dataset lineage. It is not document-derived, never exact, and
    /// never on the decode path.
    ExternalMetadata,
    /// Could not be resolved.
    Unresolved,
}

impl Basis {
    /// Stable lower-case name (used in JSON and receipts).
    pub const fn name(self) -> &'static str {
        match self {
            Basis::Authored => "authored",
            Basis::DirectlyObserved => "directly-observed",
            Basis::DeterministicallyDerived => "deterministically-derived",
            Basis::Inferred => "inferred",
            Basis::Heuristic => "heuristic",
            Basis::ExternalMetadata => "external-metadata",
            Basis::Unresolved => "unresolved",
        }
    }

    /// Whether an answer with this basis is an exact observation of the source.
    pub const fn is_exact(self) -> bool {
        matches!(self, Basis::Authored | Basis::DirectlyObserved)
    }
}

/// How much of the source integrity an observation actually verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegrityScope {
    /// No source integrity was re-verified by this observation.
    None,
    /// A single seed node's bytes were content-verified.
    Node,
    /// The whole source SHA-256 was verified (full materialization only).
    WholeSource,
}

impl IntegrityScope {
    /// Stable lower-case name (used in JSON).
    pub const fn name(self) -> &'static str {
        match self {
            IntegrityScope::None => "none",
            IntegrityScope::Node => "node",
            IntegrityScope::WholeSource => "whole-source",
        }
    }
}

/// The payload of a [`FieldAnswer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnswerValue {
    /// Exact bytes.
    Bytes(Vec<u8>),
    /// UTF-8 (possibly lossy) text.
    Text(String),
    /// A JSON document body.
    Json(String),
    /// No value.
    None,
}

impl AnswerValue {
    /// The length in bytes this value contributes to the output budget.
    pub fn byte_len(&self) -> u64 {
        match self {
            AnswerValue::Bytes(b) => b.len() as u64,
            AnswerValue::Text(s) | AnswerValue::Json(s) => s.len() as u64,
            AnswerValue::None => 0,
        }
    }
}

/// A typed observation with full provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldAnswer {
    /// The observed value.
    pub value: AnswerValue,
    /// The epistemic basis.
    pub basis: Basis,
    /// Canonical selector text, e.g. `page:1` or `byte-range:10:4`.
    pub selector: String,
    /// Canonical representation text, e.g. `text` or `exact`.
    pub representation: String,
    /// The exact source span this answer maps to, when one exists.
    pub source_span: Option<(u64, u64)>,
    /// Short provenance/basis string: which layer and format-specific identity
    /// produced this answer (e.g. a DOCX story, part, table/row/cell, and the
    /// extraction profile). Advisory and deterministic; never authority.
    pub provenance: String,
    /// The seed nodes actually read to produce this answer.
    pub dependency_ids: Vec<NodeId>,
    /// How much source integrity was verified.
    pub integrity_scope: IntegrityScope,
    /// Whether this answer is an exact observation of the source.
    pub exact: bool,
}

/// Minimal deterministic JSON string escaping (no escaping of `/`).
pub(crate) fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_authored_and_observed_are_exact() {
        assert!(Basis::Authored.is_exact());
        assert!(Basis::DirectlyObserved.is_exact());
        assert!(!Basis::DeterministicallyDerived.is_exact());
        assert!(!Basis::Inferred.is_exact());
        assert!(!Basis::Heuristic.is_exact());
        assert!(!Basis::ExternalMetadata.is_exact());
        assert!(!Basis::Unresolved.is_exact());
    }

    #[test]
    fn json_escape_is_deterministic_and_safe() {
        assert_eq!(json_escape("a\"b\\c\nd"), "a\\\"b\\\\c\\nd");
        assert_eq!(json_escape("plain"), "plain");
    }
}
