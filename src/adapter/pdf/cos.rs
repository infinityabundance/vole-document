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

/// Classification of a stream dictionary's `/Filter` entry.
///
/// The exact-replay candidate only admits a stream whose encoded payload is a
/// lone DEFLATE/zlib stream, i.e. exactly one `FlateDecode` filter. Anything
/// else — no key, a different filter, a filter chain, or an unrecognized shape —
/// is conservative `Other`/`Absent` and is never a replay candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterClass {
    /// The `/Filter` key is absent.
    Absent,
    /// Exactly one filter, `FlateDecode` (bare name or a single-element array).
    FlateDecode,
    /// Any other value (different filter, chain, empty array, odd shape).
    Other,
}

/// Classify `/Filter` within the dictionary range `[dict_lo, dict_hi)`.
///
/// Recognizes only the canonical spelling `/FlateDecode` (not the legal PDF name
/// abbreviations) and only the two shapes `/Filter /FlateDecode` and
/// `/Filter [/FlateDecode]`. Only `Name` spans and the array `[`/`]` spans are
/// inspected, so a `/Filter` spelling inside a string or comment cannot match.
pub fn dict_filter(input: &[u8], lex: &[Span], dict_lo: u64, dict_hi: u64) -> FilterClass {
    let Some(name) = find_name_key(input, lex, dict_lo, dict_hi, b"Filter") else {
        return FilterClass::Absent;
    };

    let is_flate = |sp: Span| {
        sp.kind == SpanKind::Name
            && span_bytes(input, sp).is_some_and(|b| b == b"/FlateDecode".as_slice())
    };

    let Some(t0) = next_significant(lex, name + 1, dict_hi) else {
        return FilterClass::Other;
    };
    if is_flate(lex[t0]) {
        return FilterClass::FlateDecode;
    }
    if lex[t0].kind != SpanKind::ArrayOpen {
        return FilterClass::Other;
    }

    let Some(t1) = next_significant(lex, t0 + 1, dict_hi) else {
        return FilterClass::Other;
    };
    if !is_flate(lex[t1]) {
        return FilterClass::Other;
    }
    let Some(t2) = next_significant(lex, t1 + 1, dict_hi) else {
        return FilterClass::Other;
    };
    if lex[t2].kind == SpanKind::ArrayClose {
        FilterClass::FlateDecode
    } else {
        FilterClass::Other
    }
}

/// The value of a key whose value is a name (e.g. `/Type /XRef`).
///
/// The key is matched as a `Name` span equal to `/<key>` fully inside
/// `[dict_lo, dict_hi)`; the value must be a `Name` span. Returns the raw bytes
/// after the value's leading `/` (the slash is not included), or `None` when the
/// key is absent or its value is not a simple name. Strings and comments can
/// never be mistaken for a key or value because only `Name` spans are inspected.
pub fn dict_name_value<'a>(
    input: &'a [u8],
    lex: &[Span],
    dict_lo: u64,
    dict_hi: u64,
    key: &[u8],
) -> Option<&'a [u8]> {
    let name = find_name_key(input, lex, dict_lo, dict_hi, key)?;
    let t = next_significant(lex, name + 1, dict_hi)?;
    let sp = lex[t];
    if sp.kind != SpanKind::Name {
        return None;
    }
    span_bytes(input, sp)?.strip_prefix(b"/")
}

/// The value of a key that is either a direct non-negative integer or an
/// indirect reference of shape `int int R`.
///
/// The key is matched exactly as in [`dict_name_value`]. Returns
/// [`LengthValue::Direct`] for a bare integer, [`LengthValue::Indirect`] for the
/// reference shape, and `None` when the key is absent or its value is neither.
/// Only simple token shapes are considered; nothing is re-parsed from strings.
pub fn dict_int_or_ref(
    input: &[u8],
    lex: &[Span],
    dict_lo: u64,
    dict_hi: u64,
    key: &[u8],
) -> Option<LengthValue> {
    let name = find_name_key(input, lex, dict_lo, dict_hi, key)?;
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
    find_name_key(input, lex, lo, hi, b"Length")
}

/// Index of a `Name` span equal to `/<key>` fully inside `[lo, hi)`.
fn find_name_key(input: &[u8], lex: &[Span], lo: u64, hi: u64, key: &[u8]) -> Option<usize> {
    lex.iter().position(|sp| {
        sp.kind == SpanKind::Name
            && sp.start >= lo
            && sp.start.checked_add(sp.len).is_some_and(|end| end <= hi)
            && span_bytes(input, *sp)
                .is_some_and(|b| b.len() == key.len() + 1 && b[0] == b'/' && &b[1..] == key)
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
    fn dict_name_value_reads_xref_and_objstm_types() {
        let xref = b"<< /Type /XRef /Length 4 >>";
        let spans = lexed(xref);
        assert_eq!(
            dict_name_value(xref, &spans, 0, xref.len() as u64, b"Type"),
            Some(b"XRef".as_slice())
        );

        let objstm = b"<< /Type /ObjStm /N 3 >>";
        let spans = lexed(objstm);
        assert_eq!(
            dict_name_value(objstm, &spans, 0, objstm.len() as u64, b"Type"),
            Some(b"ObjStm".as_slice())
        );
    }

    #[test]
    fn dict_name_value_absent_key_and_non_name_value_return_none() {
        let absent = b"<< /Foo /XRef >>";
        let spans = lexed(absent);
        assert_eq!(
            dict_name_value(absent, &spans, 0, absent.len() as u64, b"Type"),
            None
        );

        let non_name = b"<< /Type 5 >>";
        let spans = lexed(non_name);
        assert_eq!(
            dict_name_value(non_name, &spans, 0, non_name.len() as u64, b"Type"),
            None
        );
    }

    #[test]
    fn dict_name_value_inside_string_is_not_a_key() {
        let input = b"<< /X ( /Type /XRef ) >>";
        let spans = lexed(input);
        assert_eq!(
            dict_name_value(input, &spans, 0, input.len() as u64, b"Type"),
            None
        );
    }

    #[test]
    fn dict_int_or_ref_reads_direct_and_reference() {
        let direct = b"<< /Prev 1234 >>";
        let spans = lexed(direct);
        assert_eq!(
            dict_int_or_ref(direct, &spans, 0, direct.len() as u64, b"Prev"),
            Some(LengthValue::Direct(1234))
        );

        let reference = b"<< /Prev 7 0 R >>";
        let spans = lexed(reference);
        assert_eq!(
            dict_int_or_ref(reference, &spans, 0, reference.len() as u64, b"Prev"),
            Some(LengthValue::Indirect {
                number: 7,
                generation: 0,
            })
        );
    }

    #[test]
    fn dict_int_or_ref_rejects_non_integer_and_absent() {
        let name_value = b"<< /Prev /X >>";
        let spans = lexed(name_value);
        assert_eq!(
            dict_int_or_ref(name_value, &spans, 0, name_value.len() as u64, b"Prev"),
            None
        );

        let absent = b"<< /Size 4 >>";
        let spans = lexed(absent);
        assert_eq!(
            dict_int_or_ref(absent, &spans, 0, absent.len() as u64, b"Prev"),
            None
        );
    }

    #[test]
    fn dict_filter_absent_without_key() {
        let input = b"<< /Length 1 >>";
        let spans = lexed(input);
        assert_eq!(
            dict_filter(input, &spans, 0, input.len() as u64),
            FilterClass::Absent
        );
    }

    #[test]
    fn dict_filter_reads_bare_and_single_element_array() {
        let bare = b"<< /Filter /FlateDecode >>";
        let spans = lexed(bare);
        assert_eq!(
            dict_filter(bare, &spans, 0, bare.len() as u64),
            FilterClass::FlateDecode
        );

        let array = b"<< /Filter [ /FlateDecode ] >>";
        let spans = lexed(array);
        assert_eq!(
            dict_filter(array, &spans, 0, array.len() as u64),
            FilterClass::FlateDecode
        );
    }

    #[test]
    fn dict_filter_rejects_chain_other_filter_and_empty_array() {
        let chain = b"<< /Filter [ /FlateDecode /ASCIIHexDecode ] >>";
        let spans = lexed(chain);
        assert_eq!(
            dict_filter(chain, &spans, 0, chain.len() as u64),
            FilterClass::Other
        );

        let other = b"<< /Filter /LZWDecode >>";
        let spans = lexed(other);
        assert_eq!(
            dict_filter(other, &spans, 0, other.len() as u64),
            FilterClass::Other
        );

        let empty = b"<< /Filter [] >>";
        let spans = lexed(empty);
        assert_eq!(
            dict_filter(empty, &spans, 0, empty.len() as u64),
            FilterClass::Other
        );
    }

    #[test]
    fn dict_filter_ignores_string_spelling_and_non_name_value() {
        let inside_string = b"<< /X ( /Filter /FlateDecode ) >>";
        let spans = lexed(inside_string);
        assert_eq!(
            dict_filter(inside_string, &spans, 0, inside_string.len() as u64),
            FilterClass::Absent
        );

        let non_name = b"<< /Filter 5 >>";
        let spans = lexed(non_name);
        assert_eq!(
            dict_filter(non_name, &spans, 0, non_name.len() as u64),
            FilterClass::Other
        );
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
