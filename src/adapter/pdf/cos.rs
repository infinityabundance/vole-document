//! Minimal, conservative PDF COS token helpers over the lexical cover.
//!
//! These routines never re-parse strings or invent tokens: every decision is
//! made from the existing [`Span`] partition produced by [`super::lexer`]. They
//! recognise only the simple token shapes needed to resolve a stream `/Length`
//! and to read a bare-integer object body. Anything ambiguous returns `None` so
//! callers fall back to a conservative keyword search rather than guessing.
//!
//! The functions are deliberately narrow. A dictionary is identified by byte
//! range `[dict_lo, dict_hi)`; a `/Length` key is a `Name` span whose bytes are
//! exactly `/Length`, and its value is either a single `Regular` decimal integer
//! or the reference shape `int Whitespace int Whitespace R`. Separators between
//! tokens may be whitespace or comments, exactly as PDF tokenisation treats them.

use super::span::{Span, SpanKind};

/// A resolved stream `/Length` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthValue {
    /// `/Length N`: a direct integer.
    Direct(u64),
    /// `/Length N G R`: an indirect reference to object `N`, generation `G`.
    Indirect { number: u64, generation: u64 },
}

/// Find the value of key `/Length` within the dictionary byte range
/// `[dict_lo, dict_hi)`.
///
/// Returns `None` if the key is absent or its value is not a simple integer or
/// `int int R` reference. Only `Regular`/`Name` lexemes are inspected; string
/// and comment spans can never be mistaken for a key or value.
pub fn dict_length(input: &[u8], lex: &[Span], dict_lo: u64, dict_hi: u64) -> Option<LengthValue> {
    let name = find_length_name(input, lex, dict_lo, dict_hi)?;
    let t0 = next_significant(lex, name + 1, dict_hi)?;
    let number = integer_of(input, lex[t0])?;

    // The reference shape `int Whitespace int Whitespace R` takes precedence.
    if let Some(t1) = next_significant(lex, t0 + 1, dict_hi)
        && let Some(generation) = integer_of(input, lex[t1])
        && let Some(t2) = next_significant(lex, t1 + 1, dict_hi)
        && is_regular_r(input, lex[t2])
    {
        return Some(LengthValue::Indirect { number, generation });
    }

    Some(LengthValue::Direct(number))
}

/// Whether a `/Length` key is present in the dictionary range at all, even if
/// its value is malformed. Lets callers distinguish "absent" from "present but
/// unusable" when choosing a [`super::physical::LengthSource`].
pub fn dict_has_length(input: &[u8], lex: &[Span], dict_lo: u64, dict_hi: u64) -> bool {
    find_length_name(input, lex, dict_lo, dict_hi).is_some()
}

/// Interpret the significant tokens of an object body `[body_lo, body_hi)` as a
/// single bare non-negative decimal integer.
///
/// Leading and trailing whitespace/comments are ignored; if anything other than
/// exactly one `Regular` decimal integer remains, `None` is returned. This is
/// used for indirect `/Length` targets such as `5 0 obj 12 endobj`.
pub fn body_as_u64(input: &[u8], lex: &[Span], body_lo: u64, body_hi: u64) -> Option<u64> {
    let mut only: Option<usize> = None;
    for (idx, sp) in lex.iter().enumerate() {
        if sp.start < body_lo {
            continue;
        }
        if sp.start >= body_hi {
            break;
        }
        // A token that straddles the region boundary makes the shape ambiguous.
        if !sp
            .start
            .checked_add(sp.len)
            .is_some_and(|end| end <= body_hi)
        {
            return None;
        }
        if matches!(sp.kind, SpanKind::Whitespace | SpanKind::Comment) {
            continue;
        }
        if only.is_some() {
            return None;
        }
        only = Some(idx);
    }
    integer_of(input, lex[only?])
}

/// Index of a `Name` span equal to `/Length` fully inside `[lo, hi)`.
fn find_length_name(input: &[u8], lex: &[Span], lo: u64, hi: u64) -> Option<usize> {
    lex.iter().position(|sp| {
        sp.kind == SpanKind::Name
            && sp.start >= lo
            && sp.start.checked_add(sp.len).is_some_and(|end| end <= hi)
            && span_bytes(input, *sp) == Some(b"/Length".as_slice())
    })
}

/// Index of the next non-whitespace, non-comment span at or after `from` whose
/// start is before `hi`.
fn next_significant(lex: &[Span], from: usize, hi: u64) -> Option<usize> {
    let mut j = from;
    while j < lex.len() {
        let sp = lex[j];
        if sp.start >= hi {
            return None;
        }
        match sp.kind {
            SpanKind::Whitespace | SpanKind::Comment => j += 1,
            _ => return Some(j),
        }
    }
    None
}

/// Parse a `Regular` span as a non-negative decimal integer.
fn integer_of(input: &[u8], sp: Span) -> Option<u64> {
    if sp.kind != SpanKind::Regular {
        return None;
    }
    let bytes = span_bytes(input, sp)?;
    if bytes.is_empty() {
        return None;
    }
    let mut value: u64 = 0;
    for &b in bytes {
        if !b.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u64::from(b - b'0'))?;
    }
    Some(value)
}

/// Whether `sp` is the `Regular` keyword `R`.
fn is_regular_r(input: &[u8], sp: Span) -> bool {
    sp.kind == SpanKind::Regular && span_bytes(input, sp) == Some(b"R".as_slice())
}

/// The bytes backing `sp`, or `None` if the offset is out of range.
fn span_bytes(input: &[u8], sp: Span) -> Option<&[u8]> {
    let start = usize::try_from(sp.start).ok()?;
    let end = usize::try_from(sp.start.checked_add(sp.len)?).ok()?;
    if start > end || end > input.len() {
        return None;
    }
    Some(&input[start..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::pdf::lexer::lex;
    use crate::limits::Limits;

    fn lexed(input: &[u8]) -> Vec<Span> {
        lex(input, Limits::DEFAULT)
            .expect("lex must succeed")
            .spans
            .spans
    }

    #[test]
    fn direct_length_is_read() {
        let input = b"<< /Length 42 >>";
        let spans = lexed(input);
        assert_eq!(
            dict_length(input, &spans, 0, input.len() as u64),
            Some(LengthValue::Direct(42))
        );
        assert!(dict_has_length(input, &spans, 0, input.len() as u64));
    }

    #[test]
    fn indirect_length_is_read() {
        let input = b"<< /Length 5 0 R >>";
        let spans = lexed(input);
        assert_eq!(
            dict_length(input, &spans, 0, input.len() as u64),
            Some(LengthValue::Indirect {
                number: 5,
                generation: 0
            })
        );
    }

    #[test]
    fn absent_key_returns_none() {
        let input = b"<< /Type /X /N 3 >>";
        let spans = lexed(input);
        assert_eq!(dict_length(input, &spans, 0, input.len() as u64), None);
        assert!(!dict_has_length(input, &spans, 0, input.len() as u64));
    }

    #[test]
    fn non_integer_value_returns_none_but_key_is_present() {
        let input = b"<< /Length /Foo >>";
        let spans = lexed(input);
        assert_eq!(dict_length(input, &spans, 0, input.len() as u64), None);
        assert!(dict_has_length(input, &spans, 0, input.len() as u64));
    }

    #[test]
    fn length_name_inside_string_is_not_a_key() {
        let input = b"<< /X ( /Length 9 ) >>";
        let spans = lexed(input);
        assert_eq!(dict_length(input, &spans, 0, input.len() as u64), None);
        assert!(!dict_has_length(input, &spans, 0, input.len() as u64));
    }

    #[test]
    fn body_as_u64_accepts_single_integer_with_padding() {
        let input = b"12";
        let spans = lexed(input);
        assert_eq!(body_as_u64(input, &spans, 0, 2), Some(12));

        let padded = b"\n 12 \n";
        let spans = lexed(padded);
        assert_eq!(
            body_as_u64(padded, &spans, 0, padded.len() as u64),
            Some(12)
        );
    }

    #[test]
    fn body_as_u64_rejects_multi_token_body() {
        let input = b"12 0";
        let spans = lexed(input);
        assert_eq!(body_as_u64(input, &spans, 0, input.len() as u64), None);

        let reference = b"12 0 R";
        let spans = lexed(reference);
        assert_eq!(
            body_as_u64(reference, &spans, 0, reference.len() as u64),
            None
        );
    }

    #[test]
    fn body_as_u64_rejects_non_integer() {
        let input = b"1.5";
        let spans = lexed(input);
        assert_eq!(body_as_u64(input, &spans, 0, input.len() as u64), None);
    }
}
