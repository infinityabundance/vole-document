//! Deterministic derived projections (Phase 11, `Q_gen`).
//!
//! These functions turn a lower frontier (encoded stream bytes, decoded stream
//! bytes, an operator token stream) into a higher one (decoded bytes, operators,
//! text, a structured preview). They are **deterministic pure functions**, never
//! authority: their outputs are labelled `Q_gen` and carry `basis =
//! heuristically-inferred` where the projection is lossy (PDF text extraction
//! without font/CMap semantics is best-effort).
//!
//! ## The serialize-then-reparse rule (plan §13)
//!
//! We never re-bake DEFLATE and then re-inflate it, and never re-serialize an
//! operator stream and then re-parse it, to answer a higher-level observation.
//! The projection is computed once from the retained lower-frontier state and the
//! result is what is cached/reused.

use crate::error::{Error, Result};
use crate::limits::Limits;

use super::super::adapter::pdf::lexer::lex;
use super::super::adapter::pdf::span::SpanKind;
use super::inflate::{self, Wrapper};

/// Inflate a zlib stream with a hard bound on the decoded length.
///
/// Deterministic: any conforming DEFLATE implementation yields identical bytes,
/// so this is a legitimate reproducible materializer. The declared
/// `expected_len` is enforced exactly. Backed by the shipped `zlib-rs` seam
/// (`super::inflate`); a decode failure is a typed error, never a panic.
pub fn inflate_zlib(encoded: &[u8], expected_len: u64, limits: Limits) -> Result<Vec<u8>> {
    let cap = usize::try_from(expected_len.min(limits.max_output_bytes)).unwrap_or(usize::MAX);
    let decoded = inflate::inflate_bounded(encoded, cap, Wrapper::Zlib)
        .map_err(|e| Error::usage(format!("zlib inflate failed: {e}")))?;
    if decoded.len() as u64 != expected_len {
        return Err(Error::reconstruction_mismatch(format!(
            "decoded stream is {} bytes but the node declared {expected_len}",
            decoded.len()
        )));
    }
    Ok(decoded)
}

/// Inflate a **raw** DEFLATE stream (RFC 1951, no zlib/gzip wrapper) with a hard
/// bound on the decoded length.
///
/// This is the ZIP `method 8` decodable: ZIP stores bare DEFLATE, not a
/// zlib-wrapped stream, so a zlib decoder would reject it. Like [`inflate_zlib`],
/// it is a deterministic pure function and the declared `expected_len` is
/// enforced exactly; it uses the same `zlib-rs` seam with the zlib wrapper
/// disabled.
pub fn inflate_raw_deflate(encoded: &[u8], expected_len: u64, limits: Limits) -> Result<Vec<u8>> {
    let cap = usize::try_from(expected_len.min(limits.max_output_bytes)).unwrap_or(usize::MAX);
    let decoded = inflate::inflate_bounded(encoded, cap, Wrapper::Raw)
        .map_err(|e| Error::reconstruction_mismatch(format!("raw deflate inflate failed: {e}")))?;
    if decoded.len() as u64 != expected_len {
        return Err(Error::reconstruction_mismatch(format!(
            "decoded member is {} bytes but the node declared {expected_len}",
            decoded.len()
        )));
    }
    Ok(decoded)
}

/// Map a lexical span kind to a compact canonical byte.
const fn span_kind_byte(k: SpanKind) -> u8 {
    match k {
        SpanKind::Whitespace => 0,
        SpanKind::Comment => 1,
        SpanKind::LiteralString => 2,
        SpanKind::HexString => 3,
        SpanKind::DictOpen => 4,
        SpanKind::DictClose => 5,
        SpanKind::ArrayOpen => 6,
        SpanKind::ArrayClose => 7,
        SpanKind::BraceOpen => 8,
        SpanKind::BraceClose => 9,
        SpanKind::Name => 10,
        SpanKind::Regular => 11,
    }
}

/// Build a canonical operator token stream from decoded content-stream bytes.
///
/// `record := kind:u8 len:u32 bytes[len]`, in source order. Deterministic and
/// bounded by `limits.max_pdf_spans`.
pub fn content_operators(decoded: &[u8], limits: Limits) -> Result<Vec<u8>> {
    let lexed = lex(decoded, limits)?;
    let mut out = Vec::new();
    for span in &lexed.spans.spans {
        let len = u32::try_from(span.len)
            .map_err(|_| Error::resource_limit("content token exceeds u32 length"))?;
        out.push(span_kind_byte(span.kind));
        out.extend_from_slice(&len.to_le_bytes());
        let start = usize::try_from(span.start).unwrap_or(usize::MAX);
        let end = start.saturating_add(len as usize);
        if end > decoded.len() {
            return Err(Error::internal_invariant(
                "lexer span exceeds the decoded buffer",
            ));
        }
        out.extend_from_slice(&decoded[start..end]);
    }
    Ok(out)
}

/// Iterate a canonical operator token stream: `(kind, bytes)` pairs.
fn iter_tokens(ops: &[u8]) -> Result<Vec<(u8, &[u8])>> {
    let mut at = 0usize;
    let mut out = Vec::new();
    while at < ops.len() {
        if at + 5 > ops.len() {
            return Err(Error::usage("truncated operator token stream"));
        }
        let kind = ops[at];
        let len = u32::from_le_bytes([ops[at + 1], ops[at + 2], ops[at + 3], ops[at + 4]]) as usize;
        at += 5;
        let end = at
            .checked_add(len)
            .ok_or_else(|| Error::usage("operator token length overflow"))?;
        if end > ops.len() {
            return Err(Error::usage("operator token exceeds the stream"));
        }
        out.push((kind, &ops[at..end]));
        at = end;
    }
    Ok(out)
}

/// Decode a PDF literal string body (the bytes between the parentheses).
fn decode_literal(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len());
    let mut i = 0;
    while i < body.len() {
        let b = body[i];
        if b != b'\\' {
            out.push(b);
            i += 1;
            continue;
        }
        i += 1;
        if i >= body.len() {
            break;
        }
        let e = body[i];
        i += 1;
        match e {
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'b' => out.push(0x08),
            b'f' => out.push(0x0C),
            b'(' => out.push(b'('),
            b')' => out.push(b')'),
            b'\\' => out.push(b'\\'),
            b'\r' => {
                if i < body.len() && body[i] == b'\n' {
                    i += 1;
                }
            }
            b'\n' => {}
            b'0'..=b'7' => {
                let mut v = (e - b'0') as u32;
                let mut n = 1;
                while n < 3 && i < body.len() && (b'0'..=b'7').contains(&body[i]) {
                    v = v * 8 + (body[i] - b'0') as u32;
                    i += 1;
                    n += 1;
                }
                out.push((v & 0xFF) as u8);
            }
            other => out.push(other),
        }
    }
    out
}

/// Decode a PDF hexadecimal string body (the bytes between `<` and `>`).
fn decode_hex(body: &[u8]) -> Vec<u8> {
    let mut nibbles = Vec::with_capacity(body.len());
    for &b in body {
        let v = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => continue,
        };
        nibbles.push(v);
    }
    let mut out = Vec::with_capacity(nibbles.len() / 2);
    for pair in nibbles.chunks(2) {
        let hi = pair[0];
        let lo = if pair.len() == 2 { pair[1] } else { 0 };
        out.push((hi << 4) | lo);
    }
    out
}

/// A bounded text-run projection of a canonical operator token stream.
///
/// This is a **heuristic** projection (`basis = heuristic`): it extracts the
/// string operands of `Tj` / `'` / `"` / `TJ` and inserts a line break on
/// `Td` / `TD` / `T*` / `ET`. It does not implement font encodings or CMaps, so
/// it is not a substitute for a conforming text extractor and is never authority.
pub fn text_runs(ops: &[u8], limits: Limits) -> Result<Vec<u8>> {
    let tokens = iter_tokens(ops)?;
    let mut out: Vec<u8> = Vec::new();
    let mut pending_strings: Vec<u8> = Vec::new();

    let flush = |out: &mut Vec<u8>, pending: &mut Vec<u8>| {
        if !pending.is_empty() {
            out.extend_from_slice(pending);
            pending.clear();
        }
    };

    for (kind, bytes) in tokens {
        match kind {
            2 => {
                // LiteralString: strip the outer parentheses.
                if bytes.len() >= 2 {
                    pending_strings.extend_from_slice(&decode_literal(&bytes[1..bytes.len() - 1]));
                }
            }
            3 => {
                // HexString: strip the outer angle brackets.
                if bytes.len() >= 2 {
                    pending_strings.extend_from_slice(&decode_hex(&bytes[1..bytes.len() - 1]));
                }
            }
            11 => match bytes {
                b"Tj" | b"'" | b"\"" | b"TJ" => {
                    flush(&mut out, &mut pending_strings);
                }
                b"Td" | b"TD" | b"T*" | b"ET" => {
                    if !out.is_empty() && *out.last().unwrap() != b'\n' {
                        out.push(b'\n');
                    }
                    pending_strings.clear();
                }
                _ => {
                    pending_strings.clear();
                }
            },
            _ => {}
        }
        if out.len() as u64 > limits.max_output_bytes {
            return Err(Error::resource_limit("text projection exceeded its bound"));
        }
    }
    Ok(out)
}

/// A bounded deterministic structured page preview.
///
/// The v1 preview is a compact text-plus-structure view, not a raster image and
/// not a conforming renderer: it reports the page's text runs and a deterministic
/// count of its drawing operators. Unsupported visual semantics are therefore
/// explicitly *not* claimed.
pub fn page_preview(page: u32, content: &[u8], limits: Limits) -> Result<Vec<u8>> {
    let ops = content_operators(content, limits)?;
    let tokens = iter_tokens(&ops)?;
    let mut text = Vec::new();
    let mut pending: Vec<u8> = Vec::new();
    let mut draw_ops: u64 = 0;
    let mut path_ops: u64 = 0;
    for (kind, bytes) in &tokens {
        match kind {
            2 => {
                if bytes.len() >= 2 {
                    pending.extend_from_slice(&decode_literal(&bytes[1..bytes.len() - 1]));
                }
            }
            3 => {
                if bytes.len() >= 2 {
                    pending.extend_from_slice(&decode_hex(&bytes[1..bytes.len() - 1]));
                }
            }
            11 => {
                if matches!(*bytes, b"Tj" | b"'" | b"\"" | b"TJ") {
                    text.extend_from_slice(&pending);
                    pending.clear();
                } else if matches!(*bytes, b"Td" | b"TD" | b"T*" | b"ET") {
                    if !text.is_empty() && *text.last().unwrap() != b'\n' {
                        text.push(b'\n');
                    }
                    pending.clear();
                } else if matches!(
                    *bytes,
                    b"S" | b"s" | b"f" | b"F" | b"f*" | b"B" | b"B*" | b"b"
                ) {
                    draw_ops += 1;
                } else if matches!(*bytes, b"m" | b"l" | b"c" | b"v" | b"y" | b"re" | b"h") {
                    path_ops += 1;
                } else {
                    pending.clear();
                }
            }
            _ => {}
        }
        if text.len() as u64 > limits.max_output_bytes {
            return Err(Error::resource_limit("preview text exceeded its bound"));
        }
    }
    let header = format!(
        "VOLE-PREVIEW v1\npage {page}\ntext-bytes {}\ndraw-ops {draw_ops}\npath-ops {path_ops}\n--\n",
        text.len()
    );
    let mut out = header.into_bytes();
    out.extend_from_slice(&text);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::Limits;

    #[test]
    fn text_runs_extract_tj_strings() {
        let content = b"BT /F1 12 Tf (Hello) Tj ( World) Tj ET";
        let ops = content_operators(content, Limits::DEFAULT).unwrap();
        let text = text_runs(&ops, Limits::DEFAULT).unwrap();
        assert_eq!(text, b"Hello World\n");
    }

    #[test]
    fn hex_and_escapes_decode() {
        let content = b"(a\\)b) Tj <4869> Tj";
        let ops = content_operators(content, Limits::DEFAULT).unwrap();
        let text = text_runs(&ops, Limits::DEFAULT).unwrap();
        assert_eq!(text, b"a)bHi");
    }

    #[test]
    fn preview_is_deterministic() {
        let content = b"0 0 m 10 10 l S BT (Hi) Tj ET";
        let a = page_preview(3, content, Limits::DEFAULT).unwrap();
        let b = page_preview(3, content, Limits::DEFAULT).unwrap();
        assert_eq!(a, b);
        assert!(a.starts_with(b"VOLE-PREVIEW v1\npage 3\n"));
    }
}
