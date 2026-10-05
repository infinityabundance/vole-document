//! Byte-span vocabulary for the PDF lexical cover.
//!
//! A [`SpanSet`] is a *partition* of an input: an ordered list of spans whose
//! lengths sum to the declared length, with no gap and no overlap. The cover is
//! the authority for later structural passes: every source byte belongs to
//! exactly one span, so interpretation is always anchored to raw offsets and no
//! byte can be silently lost.

use crate::error::{Error, Result};

/// The lexical class of a byte span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    /// A maximal run of PDF whitespace bytes.
    Whitespace,
    /// A `%` comment, including the `%` and excluding the terminating EOL.
    Comment,
    /// A `(` ... `)` literal string (possibly with escaped/nested parens).
    LiteralString,
    /// A `<` ... `>` hexadecimal string.
    HexString,
    /// A `<<` dictionary opener.
    DictOpen,
    /// A `>>` dictionary closer.
    DictClose,
    /// A `[` array opener.
    ArrayOpen,
    /// A `]` array closer.
    ArrayClose,
    /// A `{` brace opener.
    BraceOpen,
    /// A `}` brace closer.
    BraceClose,
    /// A `/` name, including the leading slash.
    Name,
    /// A maximal run of regular bytes (keywords, numbers, references, ...).
    Regular,
}

/// One lexical span: a half-open byte range `[start, start + len)` with a class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Offset of the first byte of the span.
    pub start: u64,
    /// Number of bytes in the span (always > 0 for a valid cover).
    pub len: u64,
    /// The lexical class of the span.
    pub kind: SpanKind,
}

/// An ordered cover of an input by lexical spans.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpanSet {
    /// Spans in ascending, non-overlapping order.
    pub spans: Vec<Span>,
}

impl SpanSet {
    /// Sum of all span lengths. Saturates rather than panicking on absurd input.
    pub fn total_len(&self) -> u64 {
        self.spans
            .iter()
            .fold(0u64, |acc, s| acc.saturating_add(s.len))
    }

    /// Require a contiguous cover of exactly `[0, declared_len)`.
    ///
    /// Returns [`crate::ErrorClass::CoverageViolation`] for a gap, overlap,
    /// wrong total, or length overflow, and
    /// [`crate::ErrorClass::InvalidPdfStructure`] for a degenerate zero-length
    /// span. This is the internal self-check that turns a scanner bug into a
    /// loud classified failure instead of silent byte loss.
    pub fn validate(&self, declared_len: u64) -> Result<()> {
        let mut cursor: u64 = 0;
        for (i, span) in self.spans.iter().enumerate() {
            if span.len == 0 {
                return Err(Error::invalid_pdf_structure(format!(
                    "span {i} has zero length at offset {}",
                    span.start
                )));
            }
            if span.start != cursor {
                let why = if span.start < cursor {
                    "overlap"
                } else {
                    "gap"
                };
                return Err(Error::coverage_violation(format!(
                    "span {i} {why}: expected start {cursor}, found {}",
                    span.start
                )));
            }
            cursor = cursor.checked_add(span.len).ok_or_else(|| {
                Error::coverage_violation("span lengths overflow the address space")
            })?;
        }
        if cursor != declared_len {
            return Err(Error::coverage_violation(format!(
                "cover ends at {cursor}, declared length is {declared_len}"
            )));
        }
        Ok(())
    }

    /// The span containing `offset`, if any. Binary search over sorted starts.
    pub fn span_at(&self, offset: u64) -> Option<Span> {
        let idx = self.spans.partition_point(|s| s.start <= offset);
        if idx == 0 {
            return None;
        }
        let span = self.spans[idx - 1];
        if offset < span.start.saturating_add(span.len) {
            Some(span)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorClass;

    fn s(start: u64, len: u64, kind: SpanKind) -> Span {
        Span { start, len, kind }
    }

    #[test]
    fn empty_set_validates_zero() {
        let set = SpanSet::default();
        assert_eq!(set.total_len(), 0);
        assert!(set.validate(0).is_ok());
        assert_eq!(set.span_at(0), None);
    }

    #[test]
    fn contiguous_set_validates_and_totals() {
        let set = SpanSet {
            spans: vec![s(0, 3, SpanKind::Whitespace), s(3, 2, SpanKind::Regular)],
        };
        assert_eq!(set.total_len(), 5);
        assert!(set.validate(5).is_ok());
    }

    #[test]
    fn gap_is_coverage_violation() {
        let set = SpanSet {
            spans: vec![s(0, 1, SpanKind::Regular), s(2, 1, SpanKind::Regular)],
        };
        let e = set.validate(3).unwrap_err();
        assert_eq!(e.class(), ErrorClass::CoverageViolation);
    }

    #[test]
    fn overlap_is_coverage_violation() {
        let set = SpanSet {
            spans: vec![s(0, 2, SpanKind::Regular), s(1, 2, SpanKind::Regular)],
        };
        let e = set.validate(3).unwrap_err();
        assert_eq!(e.class(), ErrorClass::CoverageViolation);
    }

    #[test]
    fn short_cover_is_coverage_violation() {
        let set = SpanSet {
            spans: vec![s(0, 2, SpanKind::Regular)],
        };
        let e = set.validate(3).unwrap_err();
        assert_eq!(e.class(), ErrorClass::CoverageViolation);
    }

    #[test]
    fn zero_length_span_is_invalid_structure() {
        let set = SpanSet {
            spans: vec![s(0, 0, SpanKind::Regular)],
        };
        let e = set.validate(0).unwrap_err();
        assert_eq!(e.class(), ErrorClass::InvalidPdfStructure);
    }

    #[test]
    fn span_at_finds_the_right_span() {
        let set = SpanSet {
            spans: vec![
                s(0, 2, SpanKind::Regular),
                s(2, 3, SpanKind::Whitespace),
                s(5, 1, SpanKind::Name),
            ],
        };
        assert_eq!(set.span_at(0), Some(s(0, 2, SpanKind::Regular)));
        assert_eq!(set.span_at(1), Some(s(0, 2, SpanKind::Regular)));
        assert_eq!(set.span_at(2), Some(s(2, 3, SpanKind::Whitespace)));
        assert_eq!(set.span_at(4), Some(s(2, 3, SpanKind::Whitespace)));
        assert_eq!(set.span_at(5), Some(s(5, 1, SpanKind::Name)));
        assert_eq!(set.span_at(6), None);
        assert_eq!(set.span_at(u64::MAX), None);
    }
}
