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

use super::cos::{
    FilterClass, LengthValue, body_as_u64, dict_filter, dict_has_length, dict_int_or_ref,
    dict_length, dict_name_value,
};
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

/// The structural role of an indirect object, inferred from its leading
/// dictionary.
///
/// The classification is deliberately conservative: only a leading `<<...>>`
/// dictionary whose `/Type` is a simple name is inspected, so anything ambiguous
/// remains [`ObjRole::Generic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjRole {
    /// An ordinary object, or one whose role could not be established.
    Generic,
    /// An object whose leading dictionary is `/Type /XRef` (a cross-reference
    /// stream).
    XRefStream,
    /// An object whose leading dictionary is `/Type /ObjStm` (an object stream).
    ObjectStream,
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
    /// Structural role inferred from the leading dictionary.
    pub role: ObjRole,
}

/// How a stream's data length was determined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthSource {
    /// A direct `/Length N` was read and verified against `endstream`.
    Direct,
    /// An indirect `/Length N G R` was resolved and verified.
    Indirect,
    /// No usable `/Length`; the conservative 3.2 keyword search was used.
    Fallback,
    /// No `/Length` key was present at all; the keyword search was used.
    Missing,
}

/// The exact data span of one stream, with the provenance of its length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfStreamSpan {
    /// Object number of the enclosing indirect object.
    pub object: u64,
    /// Generation number of the enclosing indirect object.
    pub generation: u64,
    /// Offset of the first payload byte (after the post-`stream` EOL).
    pub data_start: u64,
    /// Number of payload bytes.
    pub data_len: u64,
    /// How `data_len` was established.
    pub length_source: LengthSource,
    /// Classification of the stream dictionary's `/Filter` entry.
    pub filter: FilterClass,
}

/// One incremental-update revision, delimited by a terminating `%%EOF`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevisionInfo {
    /// Zero-based position of the revision in file order.
    pub index: u32,
    /// First byte of the revision.
    pub start: u64,
    /// Byte just past the terminating `%%EOF` comment.
    pub end: u64,
    /// The recorded `startxref` value whose keyword lies in this revision.
    pub startxref: Option<u64>,
    /// The resolved `/Prev` offset of this revision's cross-reference anchor, if
    /// any.
    pub prev: Option<u64>,
}

/// The physical summary of a PDF input.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PdfPhysical {
    /// A contiguous cover of the input, in ascending offset order.
    pub spans: Vec<PhysicalSpan>,
    /// Indirect objects found, in file order.
    pub objects: Vec<PdfObjectSpan>,
    /// Resolved stream payload spans, in file order.
    pub streams: Vec<PdfStreamSpan>,
    /// Revision records delimited by `%%EOF`, in file order.
    pub revisions: Vec<RevisionInfo>,
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

    // A prior pass resolves object body ranges, so an indirect `/Length` can be
    // resolved even when the target object appears later in the file.
    let obj_bodies = collect_bodies(input, &spans);
    let mut state = ScanState::new(limits, obj_bodies);
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

    let physical = state.finish(input, &spans);
    physical.validate(declared_len)?;
    Ok(physical)
}

/// Mutable accumulator for [`scan`].
struct ScanState {
    builder: Builder,
    objects: Vec<PdfObjectSpan>,
    streams: Vec<PdfStreamSpan>,
    obj_bodies: Vec<ObjBody>,
    startxref: Vec<(u64, u64)>,
    eofs: Vec<u64>,
    trailer_dicts: Vec<(u64, u64)>,
    header: Option<(u64, u64)>,
}

impl ScanState {
    fn new(limits: Limits, obj_bodies: Vec<ObjBody>) -> Self {
        ScanState {
            builder: Builder::new(limits),
            objects: Vec::new(),
            streams: Vec::new(),
            obj_bodies,
            startxref: Vec::new(),
            eofs: Vec::new(),
            trailer_dicts: Vec::new(),
            header: None,
        }
    }

    fn push(&mut self, start: u64, len: u64, kind: PhysicalKind) -> Result<()> {
        self.builder.push(start, len, kind)
    }

    fn finish(self, input: &[u8], spans: &[Span]) -> PdfPhysical {
        let revisions = build_revisions(
            input,
            spans,
            &self.eofs,
            &self.objects,
            &self.startxref,
            &self.trailer_dicts,
        );
        PdfPhysical {
            spans: self.builder.spans,
            objects: self.objects,
            streams: self.streams,
            revisions,
            startxref: self.startxref.iter().map(|&(_, value)| value).collect(),
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
        let role = leading_dict_role(input, spans, i + 5);
        self.push(
            obj_header_start,
            obj_kw_end - obj_header_start,
            PhysicalKind::ObjHeader,
        )?;

        // Locate the object end: the first `endobj` after the introducer, with an
        // optional stream payload skipped wholesale. Streams use a resolved
        // `/Length` when possible and the conservative 3.2 keyword search otherwise.
        let mut stream: Option<ResolvedStream> = None;
        let mut endobj: Option<usize> = None;
        let mut j = i + 5;
        while j < spans.len() {
            if regular_eq(input, spans[j], b"endobj") {
                endobj = Some(j);
                break;
            }
            if stream.is_none()
                && regular_eq(input, spans[j], b"stream")
                && let Some(rs) = resolve_stream(input, spans, j, &self.obj_bodies, i + 5)
            {
                j = rs.endstream + 1;
                stream = Some(rs);
                continue;
            }
            j += 1;
        }

        match endobj {
            Some(m) => {
                if let Some(rs) = stream {
                    if rs.data_start > obj_kw_end {
                        self.push(
                            obj_kw_end,
                            rs.data_start - obj_kw_end,
                            PhysicalKind::ObjBody,
                        )?;
                    }
                    if rs.data_len > 0 {
                        self.push(rs.data_start, rs.data_len, PhysicalKind::StreamData)?;
                    }
                    // Resume at the end of the declared payload so any optional
                    // trailing EOL is still covered (it is not part of the span).
                    let data_end = rs.data_start + rs.data_len;
                    let body_end = spans[m].start;
                    if body_end > data_end {
                        self.push(data_end, body_end - data_end, PhysicalKind::ObjBody)?;
                    }
                    self.streams.push(PdfStreamSpan {
                        object: number,
                        generation,
                        data_start: rs.data_start,
                        data_len: rs.data_len,
                        length_source: rs.source,
                        filter: rs.filter,
                    });
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
                    role,
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
            let lo = spans[dict_idx].start;
            let hi = spans[close].start + spans[close].len;
            self.trailer_dicts.push((lo, hi));
            end = hi;
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
            self.startxref.push((start, value));
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

// ---------------------------------------------------------------------------
// Stream length resolution.
// ---------------------------------------------------------------------------

/// A pre-resolved indirect-object body range, keyed by `(number, generation)`.
#[derive(Debug, Clone, Copy)]
struct ObjBody {
    number: u64,
    generation: u64,
    body_lo: u64,
    body_hi: u64,
}

/// A resolved stream payload: exact data span plus the length's provenance.
#[derive(Debug, Clone, Copy)]
struct ResolvedStream {
    data_start: u64,
    data_len: u64,
    endstream: usize,
    source: LengthSource,
    filter: FilterClass,
}

/// One prior pass over the lexical cover records every object's body range using
/// the same conservative stream skip as the main pass. This lets the main pass
/// resolve an indirect `/Length` even when the target object appears later.
fn collect_bodies(input: &[u8], spans: &[Span]) -> Vec<ObjBody> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < spans.len() {
        if let Some((number, generation)) = obj_header_at(input, spans, i) {
            let body_lo = spans[i + 4].start + spans[i + 4].len;
            match find_endobj(input, spans, i + 5) {
                Some(m) => {
                    out.push(ObjBody {
                        number,
                        generation,
                        body_lo,
                        body_hi: spans[m].start,
                    });
                    i = m + 1;
                }
                None => {
                    out.push(ObjBody {
                        number,
                        generation,
                        body_lo,
                        body_hi: input.len() as u64,
                    });
                    i = spans.len();
                }
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Index of the terminating `endobj`, skipping a stream payload wholesale with
/// the conservative keyword rule used by the 3.2 scanner.
fn find_endobj(input: &[u8], spans: &[Span], from: usize) -> Option<usize> {
    let mut j = from;
    while j < spans.len() {
        if regular_eq(input, spans[j], b"endobj") {
            return Some(j);
        }
        if regular_eq(input, spans[j], b"stream") {
            let kw_end = spans[j].start + spans[j].len;
            if let Some(data_start) = eol_start_after(input, kw_end)
                && let Some(k) = find_regular(input, spans, j + 1, b"endstream")
                && spans[k].start >= data_start
            {
                j = k + 1;
                continue;
            }
        }
        j += 1;
    }
    None
}

/// Body range of object `(number, generation)`, if present.
fn lookup_body(bodies: &[ObjBody], number: u64, generation: u64) -> Option<(u64, u64)> {
    bodies
        .iter()
        .find(|b| b.number == number && b.generation == generation)
        .map(|b| (b.body_lo, b.body_hi))
}

/// Resolve a `stream` keyword at lexeme `s` to its exact payload span.
///
/// Precedence: direct `/Length`, then indirect `/Length` (resolved through the
/// object-body index), then the conservative 3.2 keyword search. Returns `None`
/// only when not even the keyword search finds a terminating `endstream`.
fn resolve_stream(
    input: &[u8],
    spans: &[Span],
    s: usize,
    bodies: &[ObjBody],
    lower: usize,
) -> Option<ResolvedStream> {
    let kw_end = spans[s].start + spans[s].len;

    let mut resolved: Option<ResolvedStream> = None;
    let mut source = LengthSource::Fallback;
    let mut filter = FilterClass::Absent;

    if let Some((open, close)) = preceding_dict(spans, s, lower) {
        let dict_lo = spans[open].start;
        let dict_hi = spans[close].start + spans[close].len;
        filter = dict_filter(input, spans, dict_lo, dict_hi);
        match dict_length(input, spans, dict_lo, dict_hi) {
            Some(LengthValue::Direct(n)) => {
                if let Some(rs) = verify_direct(input, spans, kw_end, n, filter) {
                    resolved = Some(rs);
                    source = LengthSource::Direct;
                }
            }
            Some(LengthValue::Indirect { number, generation }) => {
                if let Some((lo, hi)) = lookup_body(bodies, number, generation)
                    && let Some(n) = body_as_u64(input, spans, lo, hi)
                    && let Some(rs) = verify_direct(input, spans, kw_end, n, filter)
                {
                    resolved = Some(rs);
                    source = LengthSource::Indirect;
                }
            }
            None => {
                if !dict_has_length(input, spans, dict_lo, dict_hi) {
                    source = LengthSource::Missing;
                }
            }
        }
    } else {
        source = LengthSource::Missing;
    }

    if let Some(mut rs) = resolved {
        rs.source = source;
        return Some(rs);
    }

    // Conservative 3.2 fallback: the payload runs from the first EOL after the
    // `stream` keyword to the first following `endstream` keyword.
    let data_start = eol_start_after(input, kw_end)?;
    let k = find_regular(input, spans, s + 1, b"endstream")?;
    if spans[k].start < data_start {
        return None;
    }
    Some(ResolvedStream {
        data_start,
        data_len: spans[k].start - data_start,
        endstream: k,
        source,
        filter,
    })
}

/// The `<<...>>` dictionary immediately preceding `stream` at `s`, as
/// `(open, close)` lexeme indices: the nearest `DictOpen` whose matching
/// `DictClose` lies before `s`.
fn preceding_dict(spans: &[Span], s: usize, lower: usize) -> Option<(usize, usize)> {
    let mut j = s;
    while j > lower {
        j -= 1;
        if spans[j].kind == SpanKind::DictOpen
            && let Some(close) = matching_dict_close(spans, j)
            && close < s
        {
            return Some((j, close));
        }
    }
    None
}

/// Verify a direct length `n` against the bytes after `stream`: exactly one EOL
/// must follow the keyword, and after `n` bytes an optional EOL must reach a
/// `Regular` `endstream`. A lone CR after `stream` is not a valid EOL.
fn verify_direct(
    input: &[u8],
    spans: &[Span],
    kw_end: u64,
    n: u64,
    filter: FilterClass,
) -> Option<ResolvedStream> {
    let eol = post_stream_eol_len(input, kw_end)?;
    let data_start = kw_end + eol;
    let data_end = data_start.checked_add(n)?;
    let k = first_regular_at_or_after(input, spans, data_end, b"endstream")?;
    let gap = spans[k].start.checked_sub(data_end)?;
    let ok = gap == 0
        || (gap == 1 && byte_at(input, data_end) == Some(b'\n'))
        || (gap == 2
            && byte_at(input, data_end) == Some(b'\r')
            && byte_at(input, data_end + 1) == Some(b'\n'));
    if !ok {
        return None;
    }
    Some(ResolvedStream {
        data_start,
        data_len: n,
        endstream: k,
        source: LengthSource::Direct,
        filter,
    })
}

/// Length of the mandatory EOL directly after the `stream` keyword: `LF` (1),
/// `CRLF` (2), or `None` (including a lone `CR`).
fn post_stream_eol_len(input: &[u8], kw_end: u64) -> Option<u64> {
    match byte_at(input, kw_end) {
        Some(b'\n') => Some(1),
        Some(b'\r') if byte_at(input, kw_end + 1) == Some(b'\n') => Some(2),
        _ => None,
    }
}

/// Index of the first `Regular` lexeme equal to `keyword` whose start is at or
/// after `offset`.
fn first_regular_at_or_after(
    input: &[u8],
    spans: &[Span],
    offset: u64,
    keyword: &[u8],
) -> Option<usize> {
    let idx = spans.partition_point(|sp| sp.start < offset);
    spans[idx..]
        .iter()
        .position(|sp| regular_eq(input, *sp, keyword))
        .map(|off| idx + off)
}

/// One byte at `offset`, if in range.
fn byte_at(input: &[u8], offset: u64) -> Option<u8> {
    usize::try_from(offset)
        .ok()
        .and_then(|i| input.get(i).copied())
}

/// Build revision records from the `%%EOF` offsets. `end` is the byte just past
/// the comment; `start` is `0` for the first revision, otherwise the previous
/// revision's end advanced by at most one optional EOL.
fn build_revisions(
    input: &[u8],
    spans: &[Span],
    eofs: &[u64],
    objects: &[PdfObjectSpan],
    startxref: &[(u64, u64)],
    trailers: &[(u64, u64)],
) -> Vec<RevisionInfo> {
    let mut out = Vec::with_capacity(eofs.len());
    let mut start = 0u64;
    for (index, &e) in eofs.iter().enumerate() {
        let end = span_end_at(spans, e).unwrap_or(e);
        let startxref = startxref
            .iter()
            .find(|&&(keyword, _)| keyword >= start && keyword < end)
            .map(|&(_, value)| value);
        let prev = resolve_prev(input, spans, start, end, objects, trailers);
        out.push(RevisionInfo {
            index: index as u32,
            start,
            end,
            startxref,
            prev,
        });
        start = end + eol_len_after(input, end);
    }
    out
}

/// Resolve a revision's `/Prev` offset from its cross-reference anchor: the
/// classic `trailer` dict if present in the revision, otherwise an `XRefStream`
/// object's leading dict. A direct integer is used as-is; a reference is mapped
/// to the referenced object's offset when that object exists. Returns `None` when
/// there is no anchor, no `/Prev`, or an unresolvable reference.
fn resolve_prev(
    input: &[u8],
    spans: &[Span],
    rev_start: u64,
    rev_end: u64,
    objects: &[PdfObjectSpan],
    trailers: &[(u64, u64)],
) -> Option<u64> {
    let trailer = trailers
        .iter()
        .rev()
        .find(|&&(lo, _)| lo >= rev_start && lo < rev_end);
    let (dict_lo, dict_hi) = match trailer {
        Some(&(lo, hi)) => (lo, hi),
        None => {
            let object = objects.iter().find(|o| {
                o.role == ObjRole::XRefStream && o.start >= rev_start && o.start < rev_end
            })?;
            leading_dict_range_at(input, spans, object.start)?
        }
    };
    match dict_int_or_ref(input, spans, dict_lo, dict_hi, b"Prev") {
        Some(LengthValue::Direct(n)) => Some(n),
        Some(LengthValue::Indirect { number, generation }) => objects
            .iter()
            .find(|o| o.number == number && o.generation == generation)
            .map(|o| o.start),
        None => None,
    }
}

/// Range `[lo, hi)` of the `<<...>>` dictionary immediately following the `obj`
/// keyword (skipping whitespace/comments), or `None` if the next significant
/// token is not a dict opener.
fn leading_dict_range(spans: &[Span], after: usize) -> Option<(u64, u64)> {
    let mut j = after;
    while j < spans.len() && matches!(spans[j].kind, SpanKind::Whitespace | SpanKind::Comment) {
        j += 1;
    }
    if j >= spans.len() || spans[j].kind != SpanKind::DictOpen {
        return None;
    }
    let close = matching_dict_close(spans, j)?;
    Some((spans[j].start, spans[close].start + spans[close].len))
}

/// Classify an object by its leading dictionary's `/Type`. Conservative: only a
/// simple name value yields a non-generic role.
fn leading_dict_role(input: &[u8], spans: &[Span], after: usize) -> ObjRole {
    let Some((lo, hi)) = leading_dict_range(spans, after) else {
        return ObjRole::Generic;
    };
    match dict_name_value(input, spans, lo, hi, b"Type") {
        Some(name) if name == b"XRef".as_slice() => ObjRole::XRefStream,
        Some(name) if name == b"ObjStm".as_slice() => ObjRole::ObjectStream,
        _ => ObjRole::Generic,
    }
}

/// Range `[lo, hi)` of the leading dictionary of the object whose introducer
/// starts at `start`, or `None` if no object introducer begins there.
fn leading_dict_range_at(input: &[u8], spans: &[Span], start: u64) -> Option<(u64, u64)> {
    let idx = spans.partition_point(|sp| sp.start < start);
    if idx >= spans.len() || spans[idx].start != start {
        return None;
    }
    obj_header_at(input, spans, idx)?;
    leading_dict_range(spans, idx + 5)
}

/// End offset of the span containing `offset`, if the offset lies within one.
fn span_end_at(spans: &[Span], offset: u64) -> Option<u64> {
    let idx = spans.partition_point(|sp| sp.start <= offset);
    if idx == 0 {
        return None;
    }
    let sp = spans[idx - 1];
    if offset < sp.start.saturating_add(sp.len) {
        Some(sp.start.saturating_add(sp.len))
    } else {
        None
    }
}

/// Length of one optional EOL at `offset`: `LF` (1), `CRLF` (2), lone `CR` (1),
/// or `0` when none is present.
fn eol_len_after(input: &[u8], offset: u64) -> u64 {
    match byte_at(input, offset) {
        Some(b'\n') => 1,
        Some(b'\r') if byte_at(input, offset + 1) == Some(b'\n') => 2,
        Some(b'\r') => 1,
        _ => 0,
    }
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

    fn offset_of(hay: &[u8], needle: &[u8]) -> u64 {
        hay.windows(needle.len())
            .position(|w| w == needle)
            .expect("needle present") as u64
    }

    fn slice_of(pdf: &[u8], s: PdfStreamSpan) -> &[u8] {
        &pdf[s.data_start as usize..(s.data_start + s.data_len) as usize]
    }

    fn only_stream(p: &PdfPhysical) -> PdfStreamSpan {
        assert_eq!(p.streams.len(), 1, "expected exactly one stream");
        p.streams[0]
    }

    #[test]
    fn direct_length_yields_exact_span() {
        let pdf =
            b"%PDF-1.5\n1 0 obj\n<< /Length 5 >>\nstream\nhello\nendstream\nendobj\n".to_vec();
        let p = scan(&pdf, Limits::DEFAULT).unwrap();

        let s = only_stream(&p);
        assert_eq!(s.object, 1);
        assert_eq!(s.generation, 0);
        assert_eq!(s.length_source, LengthSource::Direct);
        assert_eq!(s.data_start, offset_of(&pdf, b"hello"));
        assert_eq!(s.data_len, 5);
        assert_eq!(slice_of(&pdf, s), b"hello");

        let ds = p
            .spans
            .iter()
            .find(|sp| sp.kind == PhysicalKind::StreamData)
            .unwrap();
        assert_eq!(ds.start, s.data_start);
        assert_eq!(ds.len, s.data_len);
        p.validate(pdf.len() as u64).unwrap();
    }

    #[test]
    fn length_including_trailing_eol_is_accepted() {
        let pdf =
            b"%PDF-1.5\n1 0 obj\n<< /Length 6 >>\nstream\nhello\nendstream\nendobj\n".to_vec();
        let p = scan(&pdf, Limits::DEFAULT).unwrap();
        let s = only_stream(&p);
        assert_eq!(s.length_source, LengthSource::Direct);
        assert_eq!(slice_of(&pdf, s), b"hello\n");
        p.validate(pdf.len() as u64).unwrap();
    }

    #[test]
    fn crlf_and_lf_after_stream_are_both_handled() {
        let crlf =
            b"%PDF-1.5\n1 0 obj\n<< /Length 5 >>\r\nstream\r\nhello\r\nendstream\r\nendobj\n"
                .to_vec();
        let p = scan(&crlf, Limits::DEFAULT).unwrap();
        let s = only_stream(&p);
        assert_eq!(s.length_source, LengthSource::Direct);
        assert_eq!(slice_of(&crlf, s), b"hello");
        assert_eq!(s.data_start, offset_of(&crlf, b"hello"));
        p.validate(crlf.len() as u64).unwrap();

        let lf = b"%PDF-1.5\n1 0 obj\n<< /Length 5 >>\nstream\nhello\nendstream\nendobj\n".to_vec();
        let p = scan(&lf, Limits::DEFAULT).unwrap();
        let s = only_stream(&p);
        assert_eq!(s.length_source, LengthSource::Direct);
        assert_eq!(slice_of(&lf, s), b"hello");
        p.validate(lf.len() as u64).unwrap();
    }

    #[test]
    fn lone_cr_after_stream_falls_back() {
        let pdf =
            b"%PDF-1.5\n1 0 obj\n<< /Length 5 >>\nstream\rhello\r\nendstream\nendobj\n".to_vec();
        let p = scan(&pdf, Limits::DEFAULT).unwrap();
        let s = only_stream(&p);
        assert_eq!(s.length_source, LengthSource::Fallback);
        assert!(slice_of(&pdf, s).windows(5).any(|w| w == b"hello"));
        p.validate(pdf.len() as u64).unwrap();
    }

    #[test]
    fn indirect_length_is_resolved_forward_reference() {
        // Object 5 (the `/Length` target) appears *after* the stream object.
        let pdf =
            b"%PDF-1.5\n1 0 obj\n<< /Length 5 0 R >>\nstream\nhello\nendstream\nendobj\n5 0 obj\n5\nendobj\n"
                .to_vec();
        let p = scan(&pdf, Limits::DEFAULT).unwrap();
        let s = only_stream(&p);
        assert_eq!(s.length_source, LengthSource::Indirect);
        assert_eq!(s.data_start, offset_of(&pdf, b"hello"));
        assert_eq!(s.data_len, 5);
        assert_eq!(slice_of(&pdf, s), b"hello");
        p.validate(pdf.len() as u64).unwrap();
    }

    #[test]
    fn stream_filter_classification_reads_flate_and_absent() {
        let flate = b"%PDF-1.5\n1 0 obj\n<< /Length 5 /Filter /FlateDecode >>\nstream\nhello\nendstream\nendobj\n"
            .to_vec();
        let p = scan(&flate, Limits::DEFAULT).unwrap();
        let s = only_stream(&p);
        assert_eq!(s.filter, FilterClass::FlateDecode);
        assert_eq!(slice_of(&flate, s), b"hello");
        p.validate(flate.len() as u64).unwrap();

        let absent =
            b"%PDF-1.5\n1 0 obj\n<< /Length 5 >>\nstream\nhello\nendstream\nendobj\n".to_vec();
        let p = scan(&absent, Limits::DEFAULT).unwrap();
        let s = only_stream(&p);
        assert_eq!(s.filter, FilterClass::Absent);
        assert_eq!(slice_of(&absent, s), b"hello");
        p.validate(absent.len() as u64).unwrap();
    }

    #[test]
    fn missing_length_uses_keyword_fallback() {
        let pdf = b"%PDF-1.5\n1 0 obj\n<< /Type /X >>\nstream\nhello\nendstream\nendobj\n".to_vec();
        let p = scan(&pdf, Limits::DEFAULT).unwrap();
        let s = only_stream(&p);
        assert_eq!(s.length_source, LengthSource::Missing);
        assert!(slice_of(&pdf, s).windows(5).any(|w| w == b"hello"));
        p.validate(pdf.len() as u64).unwrap();
    }

    #[test]
    fn wrong_length_past_endstream_falls_back() {
        let pdf =
            b"%PDF-1.5\n1 0 obj\n<< /Length 100 >>\nstream\nhello\nendstream\nendobj\n".to_vec();
        let p = scan(&pdf, Limits::DEFAULT).unwrap();
        let s = only_stream(&p);
        assert_eq!(s.length_source, LengthSource::Fallback);
        assert!(slice_of(&pdf, s).windows(5).any(|w| w == b"hello"));
        p.validate(pdf.len() as u64).unwrap();
    }

    #[test]
    fn correct_length_beats_endstream_bytes_in_payload() {
        // The payload contains a standalone `endstream` token. The keyword-only
        // fallback would stop early; a verified `/Length` must win.
        let payload = b"endstream\nfoo";
        let pdf =
            b"%PDF-1.5\n1 0 obj\n<< /Length 13 >>\nstream\nendstream\nfoo\nendstream\nendobj\n"
                .to_vec();
        let p = scan(&pdf, Limits::DEFAULT).unwrap();
        let s = only_stream(&p);
        assert_eq!(s.length_source, LengthSource::Direct);
        assert_eq!(s.data_start, offset_of(&pdf, b"endstream\nfoo"));
        assert_eq!(s.data_len, payload.len() as u64);
        assert_eq!(slice_of(&pdf, s), payload);
        p.validate(pdf.len() as u64).unwrap();
    }

    #[test]
    fn two_revisions_have_correct_boundaries() {
        let r1 = b"%PDF-1.4\n1 0 obj\n<< >>\nendobj\n%%EOF\n";
        let r2 = b"2 0 obj\n<< >>\nendobj\n%%EOF";
        let mut pdf = Vec::new();
        pdf.extend_from_slice(r1);
        pdf.extend_from_slice(r2);

        let eof_positions: Vec<u64> = pdf
            .windows(5)
            .enumerate()
            .filter(|(_, w)| *w == b"%%EOF")
            .map(|(i, _)| i as u64)
            .collect();
        assert_eq!(eof_positions.len(), 2);

        let p = scan(&pdf, Limits::DEFAULT).unwrap();
        assert_eq!(p.eofs.len(), 2);
        assert_eq!(p.revisions.len(), 2);

        let rev1_end = eof_positions[0] + 5;
        let rev2_end = eof_positions[1] + 5;
        assert_eq!(
            p.revisions,
            vec![
                RevisionInfo {
                    index: 0,
                    start: 0,
                    end: rev1_end,
                    startxref: None,
                    prev: None,
                },
                RevisionInfo {
                    index: 1,
                    start: rev1_end + 1,
                    end: rev2_end,
                    startxref: None,
                    prev: None,
                },
            ]
        );
        assert_eq!(rev2_end, pdf.len() as u64);
        assert_eq!(p.objects.len(), 2);
        p.validate(pdf.len() as u64).unwrap();
    }

    #[test]
    fn classic_xref_revision_records_startxref_and_no_prev() {
        let pdf = canonical_pdf();
        let p = scan(&pdf, Limits::DEFAULT).unwrap();
        assert_eq!(p.revisions.len(), 1);
        let r = p.revisions[0];
        assert_eq!(r.index, 0);
        assert_eq!(r.start, 0);
        assert_eq!(r.end, pdf.len() as u64);
        assert_eq!(r.startxref, Some(321));
        assert_eq!(r.prev, None);
        p.validate(pdf.len() as u64).unwrap();
    }

    #[test]
    fn incremental_classic_trailer_prev_resolves_to_first_xref() {
        let rev1 = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog >>\nendobj\nxref\n0 2\n0000000000 65535 f \n0000000009 00000 n \ntrailer\n<< /Size 2 /Root 1 0 R >>\nstartxref\n9\n%%EOF\n";
        let x1 = rev1
            .windows(4)
            .position(|w| w == b"xref")
            .expect("xref present") as u64;
        let rev2 = format!(
            "2 0 obj\n<< /Type /Pages >>\nendobj\nxref\n0 3\n0000000000 65535 f \n0000000009 00000 n \n0000000042 00000 n \ntrailer\n<< /Size 3 /Prev {x1} /Root 1 0 R >>\nstartxref\n777\n%%EOF"
        );
        let mut pdf = Vec::new();
        pdf.extend_from_slice(rev1);
        pdf.extend_from_slice(rev2.as_bytes());

        let p = scan(&pdf, Limits::DEFAULT).unwrap();
        assert_eq!(p.revisions.len(), 2);
        assert_eq!(p.revisions[0].index, 0);
        assert_eq!(p.revisions[0].startxref, Some(9));
        assert_eq!(p.revisions[0].prev, None);
        assert_eq!(p.revisions[1].index, 1);
        assert_eq!(p.revisions[1].startxref, Some(777));
        assert_eq!(p.revisions[1].prev, Some(x1));
        p.validate(pdf.len() as u64).unwrap();
    }

    #[test]
    fn xref_stream_anchor_prev_reference_is_resolved() {
        let rev1 = b"%PDF-1.5\n1 0 obj\n<< >>\nendobj\nstartxref\n0\n%%EOF\n";
        let rev2 = b"2 0 obj\n<< /Type /XRef /Prev 3 0 R >>\nendobj\n3 0 obj\n<< >>\nendobj\nstartxref\n0\n%%EOF";
        let mut pdf = Vec::new();
        pdf.extend_from_slice(rev1);
        pdf.extend_from_slice(rev2);
        let obj3_start = pdf
            .windows(8)
            .position(|w| w == b"3 0 obj\n")
            .expect("object 3 present") as u64;

        let p = scan(&pdf, Limits::DEFAULT).unwrap();
        assert_eq!(p.revisions.len(), 2);
        assert_eq!(p.revisions[0].prev, None);
        assert_eq!(p.revisions[1].prev, Some(obj3_start));
        assert_eq!(
            p.objects.iter().find(|o| o.number == 2).unwrap().role,
            ObjRole::XRefStream
        );
        p.validate(pdf.len() as u64).unwrap();
    }

    #[test]
    fn object_roles_are_classified_from_leading_dict() {
        let pdf = b"%PDF-1.5\n1 0 obj\n<< /Type /Catalog >>\nendobj\n2 0 obj\n<< /Type /XRef >>\nendobj\n3 0 obj\n<< /Type /ObjStm /N 0 >>\nendobj\n4 0 obj\n<< /Foo /Bar >>\nendobj\n5 0 obj\n<< /Type 5 >>\nendobj\n6 0 obj\n42\nendobj\n7 0 obj\n<< /Foo ( /Type /XRef ) >>\nendobj\n".to_vec();
        let p = scan(&pdf, Limits::DEFAULT).unwrap();
        let role = |n: u64| p.objects.iter().find(|o| o.number == n).unwrap().role;
        assert_eq!(role(1), ObjRole::Generic);
        assert_eq!(role(2), ObjRole::XRefStream);
        assert_eq!(role(3), ObjRole::ObjectStream);
        assert_eq!(role(4), ObjRole::Generic);
        assert_eq!(role(5), ObjRole::Generic);
        assert_eq!(role(6), ObjRole::Generic);
        assert_eq!(role(7), ObjRole::Generic);
        p.validate(pdf.len() as u64).unwrap();
    }

    #[test]
    fn random_inputs_keep_streams_and_revisions_consistent() {
        let mut state: u64 = 0xdead_beef_cafe_f00d;
        for _ in 0..500 {
            let len = (xorshift64(&mut state) % 128) as usize;
            let mut buf = Vec::with_capacity(len);
            for _ in 0..len {
                buf.push((xorshift64(&mut state) & 0xff) as u8);
            }
            let p = scan(&buf, Limits::DEFAULT).unwrap();
            p.validate(buf.len() as u64).unwrap();
            for s in &p.streams {
                assert!(s.data_start + s.data_len <= buf.len() as u64);
            }
            for r in &p.revisions {
                assert!(r.start <= r.end && r.end <= buf.len() as u64);
            }
        }
    }
}
