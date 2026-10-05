//! Byte-authoritative physical classifier for PDF input (Phase 3.2).
//!
//! This pass builds on the lexical cover from [`super::lexer`] and partitions the
//! whole input `[0, len)` into structural [`PhysicalSpan`]s. It is deliberately
//! conservative:
//!
//! * Keyword recognition looks **only** at `Regular` lexemes. Because a literal
//!   string, hex string, or comment is a single opaque span, an `endobj`,
//!   `stream`, or `xref` spelling inside such a span can never be mistaken for
//!   structure.
//! * Stream data is the one region treated as raw, opaque bytes: between the EOL
//!   following the `stream` keyword and the matching `endstream` keyword nothing
//!   is interpreted, so `obj`/`endobj`/`stream` spellings inside payload bytes
//!   cannot split an object.
//! * When a construct cannot be recognised confidently it falls back to
//!   `Unclassified` (for non-structural lexemes) or `ObjBody` (for a trailing
//!   object with no `endobj`), never inventing or dropping bytes.
//!
//! The invariant is the same as the lexical layer: the emitted spans are a
//! contiguous cover of exactly `input.len()` bytes. [`scan`] verifies this before
//! returning, so a classifier bug becomes a classified
//! [`crate::ErrorClass::CoverageViolation`] instead of silent loss.

use crate::error::{Error, Result};
use crate::limits::Limits;

use super::lexer::lex;
use super::span::{Span, SpanKind};

/// The physical role of a byte span in a PDF file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalKind {
    /// The leading `%PDF-` header comment.
    Header,
    /// A `%` comment line (including binary/UTF-8 markers).
    Comment,
    /// A run of PDF whitespace bytes.
    Whitespace,
    /// The `N G obj` introducer of an indirect object.
    ObjHeader,
    /// Indirect-object body bytes that are not stream data.
    ObjBody,
    /// The `endobj` keyword.
    EndObj,
    /// Raw bytes of a stream payload (between EOL and `endstream`).
    StreamData,
    /// A classic `xref`-to-`trailer` cross-reference section.
    XrefSection,
    /// A `trailer` keyword plus its dictionary, when present.
    Trailer,
    /// A `startxref` keyword plus its offset value.
    StartXref,
    /// A `%%EOF` marker.
    Eof,
    /// A lexeme that could not be confidently classified (residual authority).
    Unclassified,
}

/// One physical span: a half-open byte range `[start, start + len)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalSpan {
    /// Offset of the first byte of the span.
    pub start: u64,
    /// Number of bytes in the span (always > 0 in a valid cover).
    pub len: u64,
    /// The physical role of the span.
    pub kind: PhysicalKind,
}

/// An indirect object discovered in file order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfObjectSpan {
    /// The object number `N`.
    pub number: u64,
    /// The generation number `G`.
    pub generation: u64,
    /// Offset of the `N` introducer.
    pub start: u64,
    /// Offset just past the closing `endobj`.
    pub end: u64,
}

/// The physical summary of a PDF input.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PdfPhysical {
    /// A contiguous cover of the input, in ascending offset order.
    pub spans: Vec<PhysicalSpan>,
    /// Indirect objects found, in file order.
    pub objects: Vec<PdfObjectSpan>,
    /// Recorded `startxref` values, in file order.
    pub startxref: Vec<u64>,
    /// Offsets of `%%EOF` markers, in file order.
    pub eofs: Vec<u64>,
    /// `(start, len)` of the `%PDF-` header comment, if present.
    pub header: Option<(u64, u64)>,
}

impl PdfPhysical {
    /// Sum of all span lengths. Saturates rather than panicking.
    pub fn total_len(&self) -> u64 {
        self.spans
            .iter()
            .fold(0u64, |acc, s| acc.saturating_add(s.len))
    }

    /// Require a contiguous cover of exactly `[0, declared_len)`.
    ///
    /// Returns [`crate::ErrorClass::CoverageViolation`] for a gap, overlap,
    /// wrong total, or length overflow, and
    /// [`crate::ErrorClass::InvalidPdfStructure`] for a zero-length span.
    pub fn validate(&self, declared_len: u64) -> Result<()> {
        let mut cursor: u64 = 0;
        for (i, span) in self.spans.iter().enumerate() {
            if span.len == 0 {
                return Err(Error::invalid_pdf_structure(format!(
                    "physical span {i} has zero length at offset {}",
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
                    "physical span {i} {why}: expected start {cursor}, found {}",
                    span.start
                )));
            }
            cursor = cursor.checked_add(span.len).ok_or_else(|| {
                Error::coverage_violation("physical span lengths overflow the address space")
            })?;
        }
        if cursor != declared_len {
            return Err(Error::coverage_violation(format!(
                "physical cover ends at {cursor}, declared length is {declared_len}"
            )));
        }
        Ok(())
    }
}

/// Build a conservative physical classification of `input` under `limits`.
pub fn scan(input: &[u8], limits: Limits) -> Result<PdfPhysical> {
    let declared_len = input.len() as u64;
    let lexed = lex(input, limits)?;
    let spans = lexed.spans.spans;

    let mut state = ScanState::new(limits);
    let mut i = 0usize;
    while i < spans.len() {
        let sp = spans[i];
        let bytes = bytes_of(input, sp);

        // 2. Header: the comment at offset 0 beginning `%PDF-`.
        if state.header.is_none()
            && sp.start == 0
            && sp.kind == SpanKind::Comment
            && bytes.starts_with(b"%PDF-")
        {
            state.push(sp.start, sp.len, PhysicalKind::Header)?;
            state.header = Some((sp.start, sp.len));
            i += 1;
            continue;
        }

        // 6. End-of-file marker.
        if sp.kind == SpanKind::Comment && bytes.starts_with(b"%%EOF") {
            state.push(sp.start, sp.len, PhysicalKind::Eof)?;
            state.eofs.push(sp.start);
            i += 1;
            continue;
        }

        // 3-5. Structural keywords are recognised only on Regular lexemes.
        if sp.kind == SpanKind::Regular {
            if bytes == b"startxref" {
                if let Some(next) = state.try_startxref(input, &spans, i)? {
                    i = next;
                    continue;
                }
            } else if bytes == b"xref" {
                i = state.emit_xref(input, &spans, i)?;
                continue;
            } else if bytes == b"trailer" {
                i = state.emit_trailer(&spans, i)?;
                continue;
            } else if let Some((number, generation)) = obj_header_at(input, &spans, i) {
                i = state.emit_object(input, &spans, i, number, generation)?;
                continue;
            }
        }

        // 7. Residual lexemes retain their lexical class or fall to Unclassified.
        let kind = match sp.kind {
            SpanKind::Comment => PhysicalKind::Comment,
            SpanKind::Whitespace => PhysicalKind::Whitespace,
            _ => PhysicalKind::Unclassified,
        };
        state.push(sp.start, sp.len, kind)?;
        i += 1;
    }

    let physical = state.finish();
    physical.validate(declared_len)?;
    Ok(physical)
}

/// Mutable accumulator for [`scan`].
struct ScanState {
    builder: Builder,
    objects: Vec<PdfObjectSpan>,
    startxref: Vec<u64>,
    eofs: Vec<u64>,
    header: Option<(u64, u64)>,
}

impl ScanState {
    fn new(limits: Limits) -> Self {
        ScanState {
            builder: Builder::new(limits),
            objects: Vec::new(),
            startxref: Vec::new(),
            eofs: Vec::new(),
            header: None,
        }
    }

    fn push(&mut self, start: u64, len: u64, kind: PhysicalKind) -> Result<()> {
        self.builder.push(start, len, kind)
    }

    fn finish(self) -> PdfPhysical {
        PdfPhysical {
            spans: self.builder.spans,
            objects: self.objects,
            startxref: self.startxref,
            eofs: self.eofs,
            header: self.header,
        }
    }

    /// Emit the classification of the indirect object whose introducer starts at
    /// lexeme `i`; returns the index of the first lexeme after the object.
    fn emit_object(
        &mut self,
        input: &[u8],
        spans: &[Span],
        i: usize,
        number: u64,
        generation: u64,
    ) -> Result<usize> {
        let obj_header_start = spans[i].start;
        let obj_kw_end = spans[i + 4].start + spans[i + 4].len;
        self.push(
            obj_header_start,
            obj_kw_end - obj_header_start,
            PhysicalKind::ObjHeader,
        )?;

        // Locate the object end: the first `endobj` after the introducer, with an
        // optional stream payload skipped wholesale.
        let mut stream: Option<(usize, u64)> = None; // (endstream index, data start)
        let mut endobj: Option<usize> = None;
        let mut j = i + 5;
        while j < spans.len() {
            if regular_eq(input, spans[j], b"endobj") {
                endobj = Some(j);
                break;
            }
            if regular_eq(input, spans[j], b"stream") {
                let kw_end = spans[j].start + spans[j].len;
                if let Some(data_start) = eol_start_after(input, kw_end)
                    && let Some(k) = find_regular(input, spans, j + 1, b"endstream")
                    && spans[k].start >= data_start
                {
                    stream = Some((k, data_start));
                    j = k + 1;
                    continue;
                }
            }
            j += 1;
        }

        match endobj {
            Some(m) => {
                if let Some((k, data_start)) = stream {
                    if data_start > obj_kw_end {
                        self.push(obj_kw_end, data_start - obj_kw_end, PhysicalKind::ObjBody)?;
                    }
                    let data_end = spans[k].start;
                    if data_end > data_start {
                        self.push(data_start, data_end - data_start, PhysicalKind::StreamData)?;
                    }
                    let body_end = spans[m].start;
                    if body_end > data_end {
                        self.push(data_end, body_end - data_end, PhysicalKind::ObjBody)?;
                    }
                } else {
                    let body_end = spans[m].start;
                    if body_end > obj_kw_end {
                        self.push(obj_kw_end, body_end - obj_kw_end, PhysicalKind::ObjBody)?;
                    }
                }

                self.push(spans[m].start, spans[m].len, PhysicalKind::EndObj)?;
                self.objects.push(PdfObjectSpan {
                    number,
                    generation,
                    start: obj_header_start,
                    end: spans[m].start + spans[m].len,
                });
                Ok(m + 1)
            }
            None => {
                // Malformed: no `endobj`. Keep the remainder as body bytes.
                let end = input.len() as u64;
                if end > obj_kw_end {
                    self.push(obj_kw_end, end - obj_kw_end, PhysicalKind::ObjBody)?;
                }
                Ok(spans.len())
            }
        }
    }

    /// Emit a classic `xref` section, stopping at the following `trailer`,
    /// `startxref`, or `%%EOF`. Returns the next lexeme index.
    fn emit_xref(&mut self, input: &[u8], spans: &[Span], i: usize) -> Result<usize> {
        let start = spans[i].start;
        let mut end_idx = spans.len();
        for (k, sp) in spans.iter().enumerate().skip(i + 1) {
            let sp = *sp;
            if regular_eq(input, sp, b"trailer") || regular_eq(input, sp, b"startxref") {
                end_idx = k;
                break;
            }
            if sp.kind == SpanKind::Comment && bytes_of(input, sp).starts_with(b"%%EOF") {
                end_idx = k;
                break;
            }
        }

        let end = if end_idx < spans.len() {
            spans[end_idx].start
        } else {
            input.len() as u64
        };
        if end > start {
            self.push(start, end - start, PhysicalKind::XrefSection)?;
        }

        if end_idx < spans.len() && regular_eq(input, spans[end_idx], b"trailer") {
            self.emit_trailer(spans, end_idx)
        } else {
            Ok(end_idx)
        }
    }

    /// Emit a `trailer` keyword plus its first dictionary, if any. Returns the
    /// next lexeme index.
    fn emit_trailer(&mut self, spans: &[Span], t: usize) -> Result<usize> {
        let start = spans[t].start;
        let mut end = spans[t].start + spans[t].len;
        let mut next = t + 1;
        let dict_idx = if next < spans.len() && spans[next].kind == SpanKind::Whitespace {
            next + 1
        } else {
            next
        };
        if dict_idx < spans.len()
            && spans[dict_idx].kind == SpanKind::DictOpen
            && let Some(close) = matching_dict_close(spans, dict_idx)
        {
            end = spans[close].start + spans[close].len;
            next = close + 1;
        }
        if end > start {
            self.push(start, end - start, PhysicalKind::Trailer)?;
        }
        Ok(next)
    }

    /// Emit a `startxref` keyword plus its value if the pattern matches.
    fn try_startxref(&mut self, input: &[u8], spans: &[Span], i: usize) -> Result<Option<usize>> {
        if i + 2 < spans.len()
            && spans[i + 1].kind == SpanKind::Whitespace
            && spans[i + 2].kind == SpanKind::Regular
            && let Some(value) = parse_uint(bytes_of(input, spans[i + 2]), 19)
        {
            let start = spans[i].start;
            let end = spans[i + 2].start + spans[i + 2].len;
            self.push(start, end - start, PhysicalKind::StartXref)?;
            self.startxref.push(value);
            return Ok(Some(i + 3));
        }
        Ok(None)
    }
}

/// Append-only physical span builder that merges adjacent equal kinds and
/// enforces the span-count bound.
struct Builder {
    max: u32,
    spans: Vec<PhysicalSpan>,
}

impl Builder {
    fn new(limits: Limits) -> Self {
        Builder {
            max: limits.max_pdf_spans,
            spans: Vec::new(),
        }
    }

    fn push(&mut self, start: u64, len: u64, kind: PhysicalKind) -> Result<()> {
        if len == 0 {
            return Ok(());
        }
        if let Some(last) = self.spans.last_mut()
            && last.kind == kind
            && last.start.checked_add(last.len) == Some(start)
        {
            last.len = last
                .len
                .checked_add(len)
                .ok_or_else(|| Error::coverage_violation("physical span length overflow"))?;
            return Ok(());
        }
        if self.spans.len() as u64 >= self.max as u64 {
            return Err(Error::resource_limit(format!(
                "pdf physical span count exceeds limit {}",
                self.max
            )));
        }
        self.spans.push(PhysicalSpan { start, len, kind });
        Ok(())
    }
}

/// The byte slice backing `sp`, or empty if the offset is out of range.
fn bytes_of(input: &[u8], sp: Span) -> &[u8] {
    let Ok(start) = usize::try_from(sp.start) else {
        return &[];
    };
    let Some(end) = sp
        .start
        .checked_add(sp.len)
        .and_then(|e| usize::try_from(e).ok())
    else {
        return &[];
    };
    if start > end || end > input.len() {
        return &[];
    }
    &input[start..end]
}

/// Whether `sp` is a `Regular` lexeme exactly equal to `keyword`.
fn regular_eq(input: &[u8], sp: Span, keyword: &[u8]) -> bool {
    sp.kind == SpanKind::Regular && bytes_of(input, sp) == keyword
}

/// Parse an unsigned ASCII integer of at most `max_digits` digits.
fn parse_uint(bytes: &[u8], max_digits: usize) -> Option<u64> {
    if bytes.is_empty() || bytes.len() > max_digits {
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

/// Offset of the first CR or LF at or after `offset`.
fn eol_start_after(input: &[u8], offset: u64) -> Option<u64> {
    let start = usize::try_from(offset).ok()?;
    if start > input.len() {
        return None;
    }
    (start..input.len())
        .find(|&i| input[i] == b'\n' || input[i] == b'\r')
        .map(|i| i as u64)
}

/// Index of the first `Regular` lexeme equal to `keyword` at or after `from`.
fn find_regular(input: &[u8], spans: &[Span], from: usize, keyword: &[u8]) -> Option<usize> {
    if from >= spans.len() {
        return None;
    }
    spans[from..]
        .iter()
        .position(|sp| regular_eq(input, *sp, keyword))
        .map(|off| from + off)
}

/// Index of the `>>` matching the `<<` at `open`, honouring nesting.
fn matching_dict_close(spans: &[Span], open: usize) -> Option<usize> {
    let mut depth: u64 = 0;
    for (k, sp) in spans.iter().enumerate().skip(open) {
        match sp.kind {
            SpanKind::DictOpen => depth = depth.saturating_add(1),
            SpanKind::DictClose => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                if depth == 0 {
                    return Some(k);
                }
            }
            _ => {}
        }
    }
    None
}

/// Recognise `N G obj` starting at lexeme `i`, returning `(N, G)`.
fn obj_header_at(input: &[u8], spans: &[Span], i: usize) -> Option<(u64, u64)> {
    if i + 4 >= spans.len() {
        return None;
    }
    if spans[i].kind != SpanKind::Regular {
        return None;
    }
    let number = parse_uint(bytes_of(input, spans[i]), 10)?;
    if spans[i + 1].kind != SpanKind::Whitespace {
        return None;
    }
    if spans[i + 2].kind != SpanKind::Regular {
        return None;
    }
    let generation = parse_uint(bytes_of(input, spans[i + 2]), 10)?;
    if spans[i + 3].kind != SpanKind::Whitespace {
        return None;
    }
    if !regular_eq(input, spans[i + 4], b"obj") {
        return None;
    }
    Some((number, generation))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorClass;

    fn count(p: &PdfPhysical, kind: PhysicalKind) -> usize {
        p.spans.iter().filter(|s| s.kind == kind).count()
    }

    fn canonical_pdf() -> Vec<u8> {
        let mut s = String::new();
        s.push_str("%PDF-1.7\n");
        s.push_str("1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
        s.push_str("2 0 obj\n<< /Length 6 >>\nstream\nhello\nendstream\nendobj\n");
        s.push_str("3 0 obj\n<< /Length 7 >>\nstream\nworld\nendstream\nendobj\n");
        s.push_str("4 0 obj\n<< /Length 4 >>\nstream\nxyz\nendstream\nendobj\n");
        s.push_str("xref\n0 5\n0000000000 65535 f \n0000000010 00000 n \n");
        s.push_str("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n321\n%%EOF");
        s.into_bytes()
    }

    #[test]
    fn canonical_pdf_is_fully_classified() {
        let pdf = canonical_pdf();
        let p = scan(&pdf, Limits::DEFAULT).unwrap();

        assert_eq!(p.header, Some((0, 8)));
        assert_eq!(p.objects.len(), 4);
        let nums: Vec<(u64, u64)> = p.objects.iter().map(|o| (o.number, o.generation)).collect();
        assert_eq!(nums, [(1, 0), (2, 0), (3, 0), (4, 0)]);
        assert_eq!(p.startxref, [321]);
        assert_eq!(p.eofs.len(), 1);
        assert_eq!(count(&p, PhysicalKind::EndObj), 4);
        assert_eq!(count(&p, PhysicalKind::StreamData), 3);
        assert_eq!(count(&p, PhysicalKind::XrefSection), 1);
        assert_eq!(count(&p, PhysicalKind::Trailer), 1);
        assert_eq!(count(&p, PhysicalKind::ObjHeader), 4);
        assert_eq!(p.total_len(), pdf.len() as u64);
        p.validate(pdf.len() as u64).unwrap();
    }

    #[test]
    fn endobj_inside_literal_string_does_not_split_object() {
        let pdf = b"%PDF-1.4\n1 0 obj\n(endobj)\nendobj".to_vec();
        let p = scan(&pdf, Limits::DEFAULT).unwrap();

        assert_eq!(p.objects.len(), 1);
        assert_eq!(p.objects[0].number, 1);
        assert_eq!(p.objects[0].generation, 0);
        assert_eq!(p.objects[0].end, pdf.len() as u64);
        assert_eq!(count(&p, PhysicalKind::EndObj), 1);
        assert_eq!(count(&p, PhysicalKind::StreamData), 0);
        p.validate(pdf.len() as u64).unwrap();
    }

    #[test]
    fn stream_data_with_keyword_spellings_stays_opaque() {
        let pdf =
            b"%PDF-1.4\n1 0 obj\n<< /Length 30 >>\nstream\nendobj stream bytes here\nendstream\nendobj"
                .to_vec();
        let p = scan(&pdf, Limits::DEFAULT).unwrap();

        assert_eq!(p.objects.len(), 1);
        assert_eq!(p.objects[0].number, 1);
        assert_eq!(p.objects[0].end, pdf.len() as u64);
        assert_eq!(count(&p, PhysicalKind::EndObj), 1);
        assert_eq!(count(&p, PhysicalKind::StreamData), 1);

        // The opaque stream payload must contain the fake keywords verbatim.
        let data = p
            .spans
            .iter()
            .find(|s| s.kind == PhysicalKind::StreamData)
            .unwrap();
        let slice = &pdf[data.start as usize..(data.start + data.len) as usize];
        assert!(slice.windows(6).any(|w| w == b"endobj"));
        assert!(slice.windows(6).any(|w| w == b"stream"));
        p.validate(pdf.len() as u64).unwrap();
    }

    #[test]
    fn two_objects_with_xref_trailer_and_startxref() {
        let pdf = b"%PDF-1.4\n1 0 obj\n<< >>\nendobj\n2 0 obj\n<< >>\nendobj\nxref\n0 3\n0000000000 65535 f \n0000000009 00000 n \ntrailer\n<< /Size 3 >>\nstartxref\n99\n%%EOF".to_vec();
        let p = scan(&pdf, Limits::DEFAULT).unwrap();

        assert_eq!(p.objects.len(), 2);
        assert_eq!(p.objects[0].number, 1);
        assert_eq!(p.objects[1].number, 2);
        assert!(p.objects[0].end <= p.objects[1].start);
        assert_eq!(count(&p, PhysicalKind::XrefSection), 1);
        assert_eq!(count(&p, PhysicalKind::Trailer), 1);
        assert_eq!(p.startxref, [99]);
        assert_eq!(p.eofs.len(), 1);
        p.validate(pdf.len() as u64).unwrap();
    }

    #[test]
    fn malformed_object_without_endobj_is_conservative() {
        let pdf = b"1 0 obj".to_vec();
        let p = scan(&pdf, Limits::DEFAULT).unwrap();

        assert!(p.objects.is_empty());
        assert_eq!(count(&p, PhysicalKind::ObjHeader), 1);
        assert_eq!(count(&p, PhysicalKind::EndObj), 0);
        assert_eq!(p.total_len(), pdf.len() as u64);
        p.validate(pdf.len() as u64).unwrap();
    }

    #[test]
    fn missing_header_leaves_cover_total() {
        let pdf = b"1 0 obj\nendobj\n".to_vec();
        let p = scan(&pdf, Limits::DEFAULT).unwrap();

        assert_eq!(p.header, None);
        assert_eq!(count(&p, PhysicalKind::EndObj), 1);
        p.validate(pdf.len() as u64).unwrap();
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
    fn random_bytes_never_panic_and_keep_cover() {
        let mut state: u64 = 0x1234_5678_9abc_def0;
        for _ in 0..500 {
            let len = (xorshift64(&mut state) % 96) as usize;
            let mut buf = Vec::with_capacity(len);
            for _ in 0..len {
                buf.push((xorshift64(&mut state) & 0xff) as u8);
            }
            match scan(&buf, Limits::DEFAULT) {
                Ok(p) => {
                    p.validate(buf.len() as u64).unwrap();
                    assert_eq!(p.total_len(), buf.len() as u64);
                }
                Err(e) => {
                    // Any failure must be a typed, classified error.
                    let _ = e.class();
                }
            }
        }
    }

    #[test]
    fn tiny_span_limit_is_resource_limit() {
        let pdf = canonical_pdf();
        let limits = Limits {
            max_pdf_spans: 1,
            ..Limits::DEFAULT
        };
        let err = scan(&pdf, limits).unwrap_err();
        assert_eq!(err.class(), ErrorClass::ResourceLimit);
    }
}
