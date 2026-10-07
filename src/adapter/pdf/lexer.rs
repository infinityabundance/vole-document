//! Byte-authoritative PDF lexical scanner.
//!
//! `lex` partitions an input into ordered [`Span`]s such that every byte belongs
//! to exactly one span. It performs no structural interpretation: keywords,
//! numbers, references, and object boundaries remain inside `Regular` and
//! `LiteralString` spans for the Phase 3.2 parser to resolve. In particular a
//! literal string is a single opaque span, so occurrences of `obj`, `endobj`, or
//! `stream` *inside* a string can never be mistaken for structure.
//!
//! A `stream` keyword followed by an EOL switches to an **opaque payload** span
//! that runs to the next `endstream` keyword. This is essential: compressed
//! stream bytes are high-entropy and routinely contain unbalanced string
//! delimiters; tokenizing them would swallow all later structure. The payload is
//! emitted as one `Regular` span (its exact bounds are re-derived from `/Length`
//! by the physical scanner), and `endstream` is emitted as a `Regular` keyword so
//! the object loop can resume.
//!
//! Unterminated constructs are not errors here: they extend to EOF and are
//! reported as non-fatal [`LexIssue`]s, because a byte cover must still be
//! produced for hostile or truncated input.

use crate::error::{Error, Result};
use crate::limits::Limits;

use super::span::{Span, SpanKind, SpanSet};

/// A tolerated lexical anomaly. Coverage is still complete when one is present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexIssue {
    /// A `(` literal string reached EOF before its matching `)`.
    UnterminatedLiteralString { at: u64 },
    /// A `<` hex string reached EOF before its closing `>`.
    UnterminatedHexString { at: u64 },
    /// A `%` comment reached EOF without a closing CR or LF.
    UnterminatedComment { at: u64 },
}

/// The lexical cover of an input plus any non-fatal issues encountered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexResult {
    /// The exact byte cover.
    pub spans: SpanSet,
    /// Anomalies tolerated while building the cover, in source order.
    pub issues: Vec<LexIssue>,
}

/// PDF whitespace: NUL, HT, LF, FF, CR, SP.
const fn is_whitespace(b: u8) -> bool {
    matches!(b, 0x00 | 0x09 | 0x0A | 0x0C | 0x0D | 0x20)
}

/// PDF delimiters, none of which may appear inside a name or regular token.
const fn is_delimiter(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

/// A regular byte is neither whitespace nor a delimiter.
const fn is_regular(b: u8) -> bool {
    !is_whitespace(b) && !is_delimiter(b)
}

/// Lex `input` under `limits`, returning a complete byte cover.
///
/// Every byte of `input` appears in exactly one span, in ascending order. The
/// cover is validated against the input length before returning, so an internal
/// scanner bug surfaces as [`crate::ErrorClass::CoverageViolation`] rather than
/// silent data loss. Exceeding `limits.max_pdf_spans` returns
/// [`crate::ErrorClass::ResourceLimit`].
pub fn lex(input: &[u8], limits: Limits) -> Result<LexResult> {
    let n = input.len();
    let max = limits.max_pdf_spans;
    let mut spans: Vec<Span> = Vec::new();
    let mut issues: Vec<LexIssue> = Vec::new();
    let mut pos: usize = 0;

    while pos < n {
        let b = input[pos];
        if is_whitespace(b) {
            let start = pos;
            while pos < n && is_whitespace(input[pos]) {
                pos += 1;
            }
            push(&mut spans, start, pos - start, SpanKind::Whitespace, max)?;
        } else if b == b'%' {
            let start = pos;
            pos += 1;
            while pos < n && input[pos] != b'\r' && input[pos] != b'\n' {
                pos += 1;
            }
            if pos >= n {
                issues.push(LexIssue::UnterminatedComment { at: start as u64 });
            }
            push(&mut spans, start, pos - start, SpanKind::Comment, max)?;
        } else if b == b'(' {
            let start = pos;
            pos = scan_literal_string(input, start, &mut issues);
            push(&mut spans, start, pos - start, SpanKind::LiteralString, max)?;
        } else if b == b'<' {
            if pos + 1 < n && input[pos + 1] == b'<' {
                push(&mut spans, pos, 2, SpanKind::DictOpen, max)?;
                pos += 2;
            } else {
                let start = pos;
                pos += 1;
                while pos < n && input[pos] != b'>' {
                    pos += 1;
                }
                if pos < n {
                    pos += 1; // include the closing '>'
                } else {
                    issues.push(LexIssue::UnterminatedHexString { at: start as u64 });
                }
                push(&mut spans, start, pos - start, SpanKind::HexString, max)?;
            }
        } else if b == b'>' {
            if pos + 1 < n && input[pos + 1] == b'>' {
                push(&mut spans, pos, 2, SpanKind::DictClose, max)?;
                pos += 2;
            } else {
                // A lone '>' is a delimiter with no lexical role; keep it as a
                // single-byte token so the cover stays complete.
                push(&mut spans, pos, 1, SpanKind::Regular, max)?;
                pos += 1;
            }
        } else if b == b'[' {
            push(&mut spans, pos, 1, SpanKind::ArrayOpen, max)?;
            pos += 1;
        } else if b == b']' {
            push(&mut spans, pos, 1, SpanKind::ArrayClose, max)?;
            pos += 1;
        } else if b == b'{' {
            push(&mut spans, pos, 1, SpanKind::BraceOpen, max)?;
            pos += 1;
        } else if b == b'}' {
            push(&mut spans, pos, 1, SpanKind::BraceClose, max)?;
            pos += 1;
        } else if b == b'/' {
            let start = pos;
            pos += 1;
            while pos < n && is_regular(input[pos]) {
                pos += 1;
            }
            push(&mut spans, start, pos - start, SpanKind::Name, max)?;
        } else if b == b')' {
            // Unmatched ')' cannot open a literal string; keep it as a
            // single-byte token to preserve coverage.
            push(&mut spans, pos, 1, SpanKind::Regular, max)?;
            pos += 1;
        } else {
            let start = pos;
            while pos < n && is_regular(input[pos]) {
                pos += 1;
            }
            push(&mut spans, start, pos - start, SpanKind::Regular, max)?;
            // A `stream` keyword followed by an EOL introduces an opaque payload.
            if input[start..pos] == *b"stream"
                && let Some(eol_len) = post_stream_eol_len(input, pos)
            {
                let eol_start = pos;
                pos += eol_len;
                push(&mut spans, eol_start, eol_len, SpanKind::Whitespace, max)?;
                let data_start = pos;
                match find_endstream(input, data_start) {
                    Some(es) => {
                        if es > data_start {
                            push(
                                &mut spans,
                                data_start,
                                es - data_start,
                                SpanKind::Regular,
                                max,
                            )?;
                        }
                        push(&mut spans, es, b"endstream".len(), SpanKind::Regular, max)?;
                        pos = es + b"endstream".len();
                    }
                    None => {
                        // No terminating keyword: the rest is one opaque span.
                        if data_start < n {
                            push(
                                &mut spans,
                                data_start,
                                n - data_start,
                                SpanKind::Regular,
                                max,
                            )?;
                        }
                        pos = n;
                    }
                }
            }
        }
    }

    let spans = SpanSet { spans };
    spans.validate(n as u64)?;
    Ok(LexResult { spans, issues })
}

/// Length of the EOL directly after a `stream` keyword: `LF` (1) or `CRLF` (2).
/// A lone `CR` is not a valid stream EOL, matching the physical scanner.
fn post_stream_eol_len(input: &[u8], pos: usize) -> Option<usize> {
    match input.get(pos) {
        Some(b'\n') => Some(1),
        Some(b'\r') if input.get(pos + 1) == Some(&b'\n') => Some(2),
        _ => None,
    }
}

/// First offset of the `endstream` keyword at or after `from`, or `None` if there
/// is none.
///
/// A keyword is terminated on the right, so this requires the byte immediately
/// after `endstream` to be PDF whitespace, a PDF delimiter, or EOF; the byte
/// *before* may be anything (it is the last payload byte). Requiring a preceding
/// EOL is wrong for real producers: Ghostscript 10.00.0 (and others) emit the
/// stream payload immediately followed by `endstream` with no intervening EOL,
/// so an EOL-preceded search over-reads the payload to a *later* `endstream` and
/// corrupts all subsequent structure.
///
/// The false-positive risk is low: payload bytes would have to contain the
/// literal 10-byte run `endstream` followed by a delimiter or whitespace, and the
/// physical scanner additionally re-derives the exact bounds from `/Length` when
/// one is present. The scan is bounded by `input.len()` and returns the first
/// such occurrence.
///
/// Two implementations satisfy this contract and are proven to agree for every
/// `(input, from)` by the differential test in this module: a scalar reference
/// scan ([`find_endstream_scalar`]) and, with the `memmem-scan` feature, a scan
/// over a reused [`memchr::memmem::Finder`] (SIMD prefilter + two-way). The
/// scalar loop advances one byte at a time, but `endstream` has no self-overlap
/// (no proper prefix equals a proper suffix), so no valid match can begin inside
/// another; `Finder::find_iter`'s non-overlapping matches therefore visit exactly
/// the same candidate start offsets with the same right-termination rule.
#[cfg(feature = "memmem-scan")]
fn find_endstream(input: &[u8], from: usize) -> Option<usize> {
    use std::sync::OnceLock;

    const NEEDLE: &[u8] = b"endstream";
    // `Finder` is `Sync` and its prefilter build is amortized across every stream
    // in a process; the free function `memmem::find` would rebuild it per call.
    static FINDER: OnceLock<memchr::memmem::Finder<'static>> = OnceLock::new();
    let finder = FINDER.get_or_init(|| memchr::memmem::Finder::new(NEEDLE));

    let start = from.min(input.len());
    finder.find_iter(&input[start..]).find_map(|rel| {
        let i = start + rel;
        match input.get(i + NEEDLE.len()) {
            None => Some(i),
            Some(&b) if is_whitespace(b) || is_delimiter(b) => Some(i),
            _ => None,
        }
    })
}

/// Scalar reference scan for [`find_endstream`] (same contract). Compiled as the
/// `--no-default-features` fallback and always under `cfg(test)`, so the
/// differential test can compare it against the SIMD scan.
#[cfg(any(test, not(feature = "memmem-scan")))]
fn find_endstream_scalar(input: &[u8], from: usize) -> Option<usize> {
    let needle = b"endstream";
    let mut i = from;
    while i + needle.len() <= input.len() {
        if &input[i..i + needle.len()] == needle {
            let terminated = match input.get(i + needle.len()) {
                None => true,
                Some(&b) => is_whitespace(b) || is_delimiter(b),
            };
            if terminated {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Without `memmem-scan`, `find_endstream` is the scalar reference scan.
#[cfg(not(feature = "memmem-scan"))]
fn find_endstream(input: &[u8], from: usize) -> Option<usize> {
    find_endstream_scalar(input, from)
}

/// Consume a `(` literal string starting at `start`; returns the first offset
/// after the span. `\` escapes the next byte, and `\` before a CR, LF, or CRLF
/// is a line continuation that also swallows the EOL.
fn scan_literal_string(input: &[u8], start: usize, issues: &mut Vec<LexIssue>) -> usize {
    let n = input.len();
    let mut pos = start;
    let mut depth: u64 = 0;
    loop {
        if pos >= n {
            issues.push(LexIssue::UnterminatedLiteralString { at: start as u64 });
            return pos;
        }
        let c = input[pos];
        if c == b'\\' {
            pos += 1;
            if pos < n {
                let escaped = input[pos];
                pos += 1;
                if escaped == b'\r' && pos < n && input[pos] == b'\n' {
                    pos += 1;
                }
            }
        } else if c == b'(' {
            depth = depth.saturating_add(1);
            pos += 1;
        } else if c == b')' {
            depth = depth.saturating_sub(1);
            pos += 1;
            if depth == 0 {
                return pos;
            }
        } else {
            pos += 1;
        }
    }
}

/// Append a span, enforcing the span-count bound.
fn push(spans: &mut Vec<Span>, start: usize, len: usize, kind: SpanKind, max: u32) -> Result<()> {
    if spans.len() as u64 >= max as u64 {
        return Err(Error::resource_limit(format!(
            "pdf span count exceeds limit {max}"
        )));
    }
    spans.push(Span {
        start: start as u64,
        len: len as u64,
        kind,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorClass;

    fn run(input: &[u8]) -> LexResult {
        lex(input, Limits::DEFAULT).expect("lex must succeed")
    }

    fn kinds(r: &LexResult) -> Vec<SpanKind> {
        r.spans.spans.iter().map(|s| s.kind).collect()
    }

    #[test]
    fn empty_input() {
        let r = run(b"");
        assert!(r.spans.spans.is_empty());
        assert!(r.issues.is_empty());
        assert!(r.spans.validate(0).is_ok());
    }

    #[test]
    fn single_whitespace() {
        let r = run(b" ");
        assert_eq!(r.spans.spans.len(), 1);
        assert_eq!(r.spans.spans[0].kind, SpanKind::Whitespace);
        assert_eq!(r.spans.spans[0].len, 1);
    }

    #[test]
    fn whitespace_run_coalesces() {
        let input = b" \t\r\n\x0c\x00 ";
        let r = run(input);
        assert_eq!(r.spans.spans.len(), 1);
        assert_eq!(r.spans.spans[0].kind, SpanKind::Whitespace);
        assert_eq!(r.spans.spans[0].len, input.len() as u64);
    }

    #[test]
    fn comment_to_eol_excludes_eol() {
        let r = run(b"%hello\n");
        assert_eq!(kinds(&r), vec![SpanKind::Comment, SpanKind::Whitespace]);
        assert_eq!(r.spans.spans[0].start, 0);
        assert_eq!(r.spans.spans[0].len, 6);
        assert_eq!(r.spans.spans[1].start, 6);
        assert_eq!(r.spans.spans[1].len, 1);
        assert!(r.issues.is_empty());
    }

    #[test]
    fn comment_stops_before_cr() {
        let r = run(b"%x\r\n");
        assert_eq!(kinds(&r), vec![SpanKind::Comment, SpanKind::Whitespace]);
        assert_eq!(r.spans.spans[0].len, 2);
        assert_eq!(r.spans.spans[1].len, 2);
        assert!(r.issues.is_empty());
    }

    #[test]
    fn comment_to_eof_is_issue() {
        let r = run(b"%abc");
        assert_eq!(kinds(&r), vec![SpanKind::Comment]);
        assert_eq!(r.spans.spans[0].len, 4);
        assert_eq!(r.issues, vec![LexIssue::UnterminatedComment { at: 0 }]);
    }

    #[test]
    fn literal_string_with_escape() {
        // ( a \ ) b )  -- the escaped ')' does not close the string.
        let r = run(b"(a\\)b)");
        assert_eq!(kinds(&r), vec![SpanKind::LiteralString]);
        assert_eq!(r.spans.spans[0].len, 6);
        assert!(r.issues.is_empty());
    }

    #[test]
    fn literal_string_with_line_continuation() {
        // ( a \ CR LF b ) -- backslash before CRLF swallows the EOL.
        let r = run(b"(a\\\r\nb)");
        assert_eq!(kinds(&r), vec![SpanKind::LiteralString]);
        assert_eq!(r.spans.spans[0].len, 7);
        assert!(r.issues.is_empty());
    }

    #[test]
    fn literal_string_with_nested_parens() {
        let r = run(b"(a(b)c)");
        assert_eq!(kinds(&r), vec![SpanKind::LiteralString]);
        assert_eq!(r.spans.spans[0].len, 7);
        assert!(r.issues.is_empty());
    }

    #[test]
    fn unterminated_literal_string_is_issue() {
        let r = run(b"(abc");
        assert_eq!(kinds(&r), vec![SpanKind::LiteralString]);
        assert_eq!(r.spans.spans[0].len, 4);
        assert_eq!(
            r.issues,
            vec![LexIssue::UnterminatedLiteralString { at: 0 }]
        );
    }

    #[test]
    fn percent_inside_literal_string_is_not_a_comment() {
        let r = run(b"( % )");
        assert_eq!(kinds(&r), vec![SpanKind::LiteralString]);
        assert_eq!(r.spans.spans[0].len, 5);
        assert!(r.issues.is_empty());
    }

    #[test]
    fn paren_inside_hex_string_is_not_special() {
        let r = run(b"<4(2>");
        assert_eq!(kinds(&r), vec![SpanKind::HexString]);
        assert_eq!(r.spans.spans[0].len, 5);
        assert!(r.issues.is_empty());
    }

    #[test]
    fn hex_string_basic() {
        let r = run(b"<4142>");
        assert_eq!(kinds(&r), vec![SpanKind::HexString]);
        assert_eq!(r.spans.spans[0].len, 6);
    }

    #[test]
    fn unterminated_hex_string_is_issue() {
        let r = run(b"<41");
        assert_eq!(kinds(&r), vec![SpanKind::HexString]);
        assert_eq!(r.spans.spans[0].len, 3);
        assert_eq!(r.issues, vec![LexIssue::UnterminatedHexString { at: 0 }]);
    }

    #[test]
    fn dict_delimiters() {
        let r = run(b"<<>>");
        assert_eq!(kinds(&r), vec![SpanKind::DictOpen, SpanKind::DictClose]);
        assert_eq!(r.spans.spans[0].len, 2);
        assert_eq!(r.spans.spans[1].len, 2);
    }

    #[test]
    fn lone_closers_are_regular_tokens() {
        let r = run(b")>");
        assert_eq!(kinds(&r), vec![SpanKind::Regular, SpanKind::Regular]);
        let r = run(b">>>");
        assert_eq!(kinds(&r), vec![SpanKind::DictClose, SpanKind::Regular]);
    }

    #[test]
    fn name_and_empty_name() {
        let r = run(b"/Name");
        assert_eq!(kinds(&r), vec![SpanKind::Name]);
        assert_eq!(r.spans.spans[0].len, 5);
        let r = run(b"/");
        assert_eq!(kinds(&r), vec![SpanKind::Name]);
        assert_eq!(r.spans.spans[0].len, 1);
    }

    #[test]
    fn arrays_and_braces() {
        let r = run(b"[]{}");
        assert_eq!(
            kinds(&r),
            vec![
                SpanKind::ArrayOpen,
                SpanKind::ArrayClose,
                SpanKind::BraceOpen,
                SpanKind::BraceClose,
            ]
        );
    }

    #[test]
    fn numbers_and_reference_are_regular_tokens() {
        let r = run(b"12 0 R");
        assert_eq!(
            kinds(&r),
            vec![
                SpanKind::Regular,
                SpanKind::Whitespace,
                SpanKind::Regular,
                SpanKind::Whitespace,
                SpanKind::Regular,
            ]
        );
        let text: Vec<&[u8]> = r
            .spans
            .spans
            .iter()
            .map(|s| &b"12 0 R"[s.start as usize..(s.start + s.len) as usize])
            .collect();
        assert_eq!(text, vec![&b"12"[..], b" ", b"0", b" ", b"R"]);
    }

    #[test]
    fn endobj_inside_literal_string_stays_one_span() {
        let r = run(b"(1 0 obj endobj)5");
        assert_eq!(kinds(&r), vec![SpanKind::LiteralString, SpanKind::Regular]);
        assert_eq!(r.spans.spans[0].start, 0);
        assert_eq!(r.spans.spans[0].len, 16);
        assert_eq!(r.spans.spans[1].start, 16);
        assert_eq!(r.spans.spans[1].len, 1);
    }

    #[test]
    fn stream_payload_without_trailing_eol_is_one_opaque_span() {
        // Real producers (e.g. Ghostscript 10.00.0) write the payload directly
        // before `endstream` with no intervening EOL. The payload here also
        // contains an unbalanced `(` and `endstream`/`stream`-like runs that must
        // not be mistaken for the terminating keyword.
        let payload = b"(unbalanced ( with endstreamZ and streamY bytes";
        let mut input = Vec::new();
        input.extend_from_slice(b"stream\n");
        input.extend_from_slice(payload);
        input.extend_from_slice(b"endstream\n");

        let r = run(&input);
        assert_eq!(
            kinds(&r),
            vec![
                SpanKind::Regular,    // stream
                SpanKind::Whitespace, // \n
                SpanKind::Regular,    // opaque payload
                SpanKind::Regular,    // endstream
                SpanKind::Whitespace, // \n
            ]
        );
        let p = &r.spans.spans[2];
        assert_eq!(
            &input[p.start as usize..(p.start + p.len) as usize],
            payload
        );
        let es = &r.spans.spans[3];
        assert_eq!(
            &input[es.start as usize..(es.start + es.len) as usize],
            b"endstream"
        );
        assert!(r.issues.is_empty());
        r.spans.validate(input.len() as u64).unwrap();
    }

    #[test]
    fn stream_payload_followed_by_eol_then_endstream_still_works() {
        let input = b"stream\nhello\nendstream\n";
        let r = run(input);
        assert_eq!(
            kinds(&r),
            vec![
                SpanKind::Regular,    // stream
                SpanKind::Whitespace, // \n
                SpanKind::Regular,    // opaque payload (hello + trailing EOL)
                SpanKind::Regular,    // endstream
                SpanKind::Whitespace, // \n
            ]
        );
        let p = &r.spans.spans[2];
        assert_eq!(
            &input[p.start as usize..(p.start + p.len) as usize],
            b"hello\n"
        );
        let es = &r.spans.spans[3];
        assert_eq!(
            &input[es.start as usize..(es.start + es.len) as usize],
            b"endstream"
        );
        assert!(r.issues.is_empty());
        r.spans.validate(input.len() as u64).unwrap();
    }

    #[test]
    fn endstream_at_eof_terminates_payload() {
        let input = b"stream\npayloadendstream";
        let r = run(input);
        assert_eq!(
            kinds(&r),
            vec![
                SpanKind::Regular,    // stream
                SpanKind::Whitespace, // \n
                SpanKind::Regular,    // payload
                SpanKind::Regular,    // endstream (EOF-terminated)
            ]
        );
        let es = &r.spans.spans[3];
        assert_eq!(
            &input[es.start as usize..(es.start + es.len) as usize],
            b"endstream"
        );
        assert_eq!(es.start + es.len, input.len() as u64);
        assert!(r.issues.is_empty());
        r.spans.validate(input.len() as u64).unwrap();
    }

    #[test]
    fn endstream_like_run_without_right_terminator_is_not_a_keyword() {
        // `endstreamZ` (regular byte after) must not terminate the payload, so the
        // real `endstream\n` is the first recognised keyword.
        let input = b"stream\nxx endstreamZ yyendstream\n";
        let r = run(input);
        assert_eq!(
            kinds(&r),
            vec![
                SpanKind::Regular,    // stream
                SpanKind::Whitespace, // \n
                SpanKind::Regular,    // opaque payload
                SpanKind::Regular,    // endstream
                SpanKind::Whitespace, // \n
            ]
        );
        let p = &r.spans.spans[2];
        assert_eq!(
            &input[p.start as usize..(p.start + p.len) as usize],
            b"xx endstreamZ yy"
        );
        r.spans.validate(input.len() as u64).unwrap();
    }

    #[test]
    fn span_at_over_lexed_input() {
        let r = run(b"12 0 R");
        assert_eq!(r.spans.span_at(0).map(|s| s.kind), Some(SpanKind::Regular));
        assert_eq!(r.spans.span_at(1).map(|s| s.kind), Some(SpanKind::Regular));
        assert_eq!(
            r.spans.span_at(2).map(|s| s.kind),
            Some(SpanKind::Whitespace)
        );
        assert_eq!(r.spans.span_at(3).map(|s| s.kind), Some(SpanKind::Regular));
        assert_eq!(
            r.spans.span_at(4).map(|s| s.kind),
            Some(SpanKind::Whitespace)
        );
        assert_eq!(r.spans.span_at(5).map(|s| s.kind), Some(SpanKind::Regular));
        assert_eq!(r.spans.span_at(6), None);
    }

    #[test]
    fn span_limit_triggers_resource_limit() {
        let limits = Limits {
            max_pdf_spans: 2,
            ..Limits::DEFAULT
        };
        let e = lex(b"a b c", limits).unwrap_err();
        assert_eq!(e.class(), ErrorClass::ResourceLimit);
    }

    #[test]
    fn cover_invariant_holds_for_a_battery() {
        let inputs: [&[u8]; 15] = [
            b"",
            b" ",
            b"%%EOF",
            b"<< /Type /Catalog >>",
            b"[1 2.5 -3 (str) <4142> /Name]",
            b"(unterminated",
            b"<414243",
            b"%comment with ( and < and >>",
            b"()<>[]{}",
            b"\x00\x09\x0a\x0c\x0d\x20mixed",
            b"trailing>",
            b")))",
            b"<<<<<<",
            b"(nested (deep (deeper)) end)",
            b"1 0 obj\n<< /A (x) >>\nendobj",
        ];
        for input in inputs {
            let r = lex(input, Limits::DEFAULT).expect("lex must succeed");
            r.spans
                .validate(input.len() as u64)
                .expect("cover must validate");
        }
    }

    fn xorshift64(state: &mut u64) -> u64 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *state = x;
        x
    }

    #[test]
    fn cover_invariant_holds_for_random_bytes() {
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        for _ in 0..500 {
            let len = (xorshift64(&mut state) % 300) as usize;
            let mut buf = Vec::with_capacity(len);
            for _ in 0..len {
                buf.push((xorshift64(&mut state) & 0xFF) as u8);
            }
            let r = lex(&buf, Limits::STRICT).expect("lex must not fail on bounded input");
            r.spans
                .validate(buf.len() as u64)
                .expect("random cover must validate");
            assert!(r.spans.spans.len() <= buf.len());
        }
    }

    /// The SIMD (`memmem`) scan must return exactly the same `endstream` offset as
    /// the scalar reference for every `from`, including offsets past the input and
    /// matches at EOF. A mismatch would change the physical span cover, so this is
    /// the representation-safety witness for the Part-A change.
    #[cfg(feature = "memmem-scan")]
    #[test]
    fn memmem_find_endstream_matches_scalar_over_a_battery() {
        let inputs: [&[u8]; 13] = [
            b"",
            b"endstream",
            b"endstreamX",
            b"endstream ",
            b"endstream\r\n",
            b"xendstream",
            b"payload endstream)) ",
            b"endstreamendstream",
            b"endstreamen",
            b"end\x00stream",
            b"aaaaendstream/",
            b"endstream\nendstream\r\n",
            b"stream\nendstream\nendobj",
        ];
        for input in inputs {
            for from in 0..=input.len() + 2 {
                assert_eq!(
                    find_endstream(input, from),
                    find_endstream_scalar(input, from),
                    "mismatch for from={from} input={input:?}"
                );
            }
        }
    }

    /// Randomized differential over token soup that is dense in `endstream`
    /// occurrences, terminators, and near-miss prefixes.
    #[cfg(feature = "memmem-scan")]
    #[test]
    fn memmem_find_endstream_matches_scalar_random() {
        let toks: [&[u8]; 10] = [
            b"endstream",
            b"end",
            b"stream",
            b"endstreamx",
            b"x",
            b" ",
            b"/",
            b"\n",
            b"\r",
            b"e",
        ];
        let mut state: u64 = 0xDEAD_BEEF_1234_5678;
        for _ in 0..2000 {
            let n = (xorshift64(&mut state) % 40) as usize;
            let mut buf: Vec<u8> = Vec::new();
            for _ in 0..n {
                buf.extend_from_slice(toks[(xorshift64(&mut state) as usize) % toks.len()]);
            }
            for from in 0..=buf.len() {
                assert_eq!(
                    find_endstream(&buf, from),
                    find_endstream_scalar(&buf, from),
                    "mismatch for from={from} buf={buf:?}"
                );
            }
        }
    }
}
