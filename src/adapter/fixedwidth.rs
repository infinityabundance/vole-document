//! Bounded, representation-preserving fixed-width (column-position) adapter
//! (Phase 21.25).
//!
//! Fixed-width text ("fixed-format", "columnar") is the second **tabular** Wave-2
//! format, but unlike CSV/TSV/PSV its columns are defined by **character
//! positions**, not by a delimiter, so it is a *distinct* adapter (and a distinct
//! [`DocumentFormat`](crate::field::document_format::DocumentFormat)) rather than a
//! CSV dialect. Like every other Wave-2 format it is *not* a package: the exact
//! leaf is the whole source (a `DocumentExact`, RAW-like authority), and everything
//! here is a bounded, deterministic (`Q_gen`) projection that never sits on the
//! exactness path.
//!
//! ## Why the detector is maximally conservative
//!
//! Fixed-width has **no magic bytes** and is *genuinely ambiguous*: nearly any
//! aligned text can look tabular. The detector therefore claims an input only on a
//! strong, documented signal, and declines (leaving the input
//! [`Opaque`](crate::field::document_format::DocumentFormat::Opaque)) otherwise:
//!
//! 1. **Reject other families first.** A source that begins with a PDF/ZIP magic, or
//!    that the JSON/YAML detectors accept, is not fixed-width; and a source that
//!    parses as a **delimited** table (CSV/TSV/PSV) declines here (the delimited
//!    detector is the more specific claim for a delimiter-bearing file). A source
//!    carrying a GFM/Markdown table delimiter row also declines, so a Markdown table
//!    is never stolen.
//! 2. **Require at least [`MIN_ROWS`] sampled records of *identical byte width*.**
//!    Uniform record width is the classic, unambiguous fixed-width signal; it rejects
//!    variable-length prose (which almost never has equal line lengths).
//! 3. **Require the shared column-boundary positions to be stable across the whole
//!    sample**: a position is a separator column iff *every* sampled record has a
//!    space there. Only then are the maximal runs of non-separator positions columns.
//! 4. **Require at least [`MIN_COLS`] non-empty columns** and that every interior
//!    whitespace gap is at least [`MIN_GAP`] columns wide, so an *accidental* single
//!    aligned space does not split a column (that is how aligned prose is rejected).
//!
//! ## What it cannot distinguish (recorded negative)
//!
//! The detector is a **bounded sample** of the first
//! [`Limits::max_fixedwidth_sampled_lines_for_detection`] lines; [`parse`] then
//! re-validates **every** line and declines the whole document typed
//! (`InvalidFixedWidthStructure`) if any line's width differs from the sample. A
//! shorter (unpadded) final field, a single-space-separated two-column layout, and a
//! genuinely ambiguous aligned-text blob (equal-length lines, ≥2-wide gaps, ≥3 lines)
//! are **not** distinguished and stay `Opaque` (or are declined) rather than guessed.
//! There is no notion of a declared column map, and character positions are byte
//! positions (multibyte UTF-8 shifts the columns).
//!
//! ## Bounded, streaming core
//!
//! Detection and every selective read are streaming: at most one record's fields and
//! one record-width bit vector of separator flags are held in memory. Any cap breach
//! is a typed decline.

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model records (defends the decoder against a hostile blob).
pub const MAX_MODEL_RECORDS: u32 = 1 << 24;
/// Hard cap on decoded model fields (defends the decoder against a hostile blob).
pub const MAX_MODEL_FIELDS: u64 = 1 << 24;

/// Terminator tag: `\n`.
pub const TERM_LF: u8 = 0;
/// Terminator tag: `\r\n`.
pub const TERM_CRLF: u8 = 1;
/// Terminator tag: a bare `\r`.
pub const TERM_CR: u8 = 2;

/// The minimum number of sampled records required to claim a fixed-width table.
pub const MIN_ROWS: usize = 3;
/// The minimum number of columns required.
pub const MIN_COLS: usize = 2;
/// The minimum width, in bytes, of the whitespace gap that must separate two
/// columns. A single accidental aligned space is deliberately not enough.
pub const MIN_GAP: usize = 2;

/// One declared column: the byte range `[start, end)` **within a record's content**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Column {
    /// The column's first byte offset within a record's content.
    pub start: u64,
    /// One past the column's last byte offset within a record's content.
    pub end: u64,
}

impl Column {
    /// The column's fixed width in bytes.
    pub const fn width(&self) -> u64 {
        self.end - self.start
    }
}

/// One field's exact source span (`[start, end)`, absolute in the source). A field
/// is the intersection of its column with the record's content, so an unpadded record
/// yields a shorter or empty field span (the padding it lacks is not invented).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FwField {
    /// The field's first source byte.
    pub start: u64,
    /// One past the field's last source byte.
    pub end: u64,
}

/// One record's exact source span (`[start, end)`, the line terminator excluded) and
/// its per-column fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FwRecord {
    /// The record's first source byte.
    pub start: u64,
    /// One past the record's last content byte (its terminator is not included).
    pub end: u64,
    /// The record's fields, one per column, in column order.
    pub fields: Vec<FwField>,
}

/// A streamed single record (as returned by [`record_at`]); carries the exact
/// terminator spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamRecord {
    /// The record's first source byte.
    pub start: u64,
    /// One past the record's last content byte (its terminator is not included).
    pub end: u64,
    /// The terminator tag that ended the record (`TERM_LF`/`TERM_CRLF`/`TERM_CR`).
    pub terminator: u8,
    /// The record's fields, one per column, in column order.
    pub fields: Vec<FwField>,
}

/// The recorded, inferred column layout of a fixed-width source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// Length of a leading UTF-8 BOM (`0` or `3`), a document-level prefix.
    pub bom_len: u8,
    /// The dominant line terminator (`TERM_LF`/`TERM_CRLF`/`TERM_CR`).
    pub terminator: u8,
    /// Whether records used more than one terminator spelling.
    pub mixed_terminators: bool,
    /// The uniform content width (in bytes) every record must have.
    pub width: u64,
    /// The inferred columns, in order (each a byte range within the content).
    pub columns: Vec<Column>,
}

impl Layout {
    /// The number of columns.
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    /// Stable name for the dominant line terminator.
    pub const fn terminator_name(&self) -> &'static str {
        match self.terminator {
            TERM_CRLF => "crlf",
            TERM_CR => "cr",
            _ => "lf",
        }
    }
}

/// The canonical derived fixed-width model (the materialization of a
/// `FixedWidthModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedWidthModel {
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// The recorded layout.
    pub layout: Layout,
    /// Whether record 0 is treated as a header (always true in v1).
    pub has_header: bool,
    /// The record arena, in physical order.
    pub records: Vec<FwRecord>,
}

impl FixedWidthModel {
    /// The record at `index`, if present.
    pub fn node(&self, index: u32) -> Option<&FwRecord> {
        self.records.get(index as usize)
    }

    /// The number of records (including the header).
    pub fn row_count(&self) -> usize {
        self.records.len()
    }

    /// The header record (record 0), if any.
    pub fn header(&self) -> Option<&FwRecord> {
        self.records.first()
    }

    /// The number of columns.
    pub fn column_count(&self) -> usize {
        self.layout.columns.len()
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64 + self.records.len() * 24);
        out.extend_from_slice(b"FWM1");
        out.push(MODEL_VERSION);
        out.push(self.layout.bom_len);
        out.push(self.layout.terminator);
        let mut flags = 0u8;
        if self.has_header {
            flags |= 1;
        }
        if self.layout.mixed_terminators {
            flags |= 2;
        }
        out.push(flags);
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&self.layout.width.to_le_bytes());
        out.extend_from_slice(&(self.layout.columns.len() as u32).to_le_bytes());
        for c in &self.layout.columns {
            out.extend_from_slice(&c.start.to_le_bytes());
            out.extend_from_slice(&c.end.to_le_bytes());
        }
        out.extend_from_slice(&(self.records.len() as u32).to_le_bytes());
        for r in &self.records {
            out.extend_from_slice(&r.start.to_le_bytes());
            out.extend_from_slice(&r.end.to_le_bytes());
            out.extend_from_slice(&(r.fields.len() as u32).to_le_bytes());
            for f in &r.fields {
                out.extend_from_slice(&f.start.to_le_bytes());
                out.extend_from_slice(&f.end.to_le_bytes());
            }
        }
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<FixedWidthModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"FWM1" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let bom_len = r.u8()?;
        if bom_len > 3 {
            return Err(corrupt("implausible BOM length"));
        }
        let terminator = r.u8()?;
        if terminator > TERM_CR {
            return Err(corrupt("unknown terminator"));
        }
        let flags = r.u8()?;
        if flags & !3 != 0 {
            return Err(corrupt("unknown model flags"));
        }
        let doc_len = r.u64()?;
        let width = r.u64()?;
        let ncols = r.u32()?;
        if ncols as u64 > MAX_MODEL_FIELDS {
            return Err(corrupt("model column count is implausible"));
        }
        let mut columns = Vec::with_capacity(ncols as usize);
        for _ in 0..ncols {
            let start = r.u64()?;
            let end = r.u64()?;
            if start > end || end > width {
                return Err(corrupt("column span is outside the record width"));
            }
            columns.push(Column { start, end });
        }
        let count = r.u32()?;
        if count > MAX_MODEL_RECORDS {
            return Err(corrupt("model record count is implausible"));
        }
        if bom_len as u64 > doc_len {
            return Err(corrupt("BOM length exceeds the document"));
        }
        let mut records = Vec::with_capacity(count as usize);
        let mut total_fields = 0u64;
        for _ in 0..count {
            let start = r.u64()?;
            let end = r.u64()?;
            if start > end || end > doc_len {
                return Err(corrupt("record span is outside the document"));
            }
            if end - start != width {
                return Err(corrupt("record width does not match the layout"));
            }
            let fn_ = r.u32()?;
            total_fields = total_fields.saturating_add(fn_ as u64);
            if total_fields > MAX_MODEL_FIELDS {
                return Err(corrupt("model field count is implausible"));
            }
            let mut fields = Vec::with_capacity(fn_ as usize);
            for _ in 0..fn_ {
                let fs = r.u64()?;
                let fe = r.u64()?;
                if fs > fe || fs < start || fe > end {
                    return Err(corrupt("field span is outside its record"));
                }
                fields.push(FwField { start: fs, end: fe });
            }
            records.push(FwRecord { start, end, fields });
        }
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        Ok(FixedWidthModel {
            doc_len,
            layout: Layout {
                bom_len,
                terminator,
                mixed_terminators: flags & 2 != 0,
                width,
                columns,
            },
            has_header: flags & 1 != 0,
            records,
        })
    }
}

/// One lexical-match record from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FwMatch {
    /// The 0-based record index.
    pub record: u32,
    /// The 0-based column index within the record.
    pub column: u32,
    /// The exact source span of the matching field (padding included).
    pub start: u64,
    /// One past the matching field.
    pub end: u64,
    /// The decoded, whitespace-trimmed field text.
    pub text: String,
}

/// Byte-based fixed-width detector. See the module docs for the exact heuristic.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.is_empty() {
        return false;
    }
    if source.len() as u64 > limits.max_fixedwidth_document_bytes {
        return false;
    }
    if looks_like_other(source) {
        return false;
    }
    // Reject anything that also parses as a **delimited** table (CSV/TSV/PSV): the
    // delimited detector is the more specific claim for a delimiter-bearing file.
    if crate::adapter::csv::detect(source, limits) {
        return false;
    }
    // Reject a Markdown table (a delimiter row is a Markdown-only signal).
    if crate::adapter::csv::has_markdown_delimiter_row(source) {
        return false;
    }
    infer_layout(source, limits).is_ok()
}

/// Whether `source` carries a family marker that is *not* fixed-width (PDF/ZIP).
/// The JSON/YAML families are declined by the [`crate::adapter::csv::detect`] guard
/// above (its `looks_like_other` consults them), so this only needs the strong
/// magic-byte prefixes.
fn looks_like_other(source: &[u8]) -> bool {
    if source.starts_with(b"%PDF-") {
        return true;
    }
    source.starts_with(b"PK\x03\x04")
        || source.starts_with(b"PK\x05\x06")
        || source.starts_with(b"PK\x07\x08")
}

/// Infer the column layout of a fixed-width source, or decline typed when no strong
/// fixed-width signal is present. The sample is bounded by
/// [`Limits::max_fixedwidth_sampled_lines_for_detection`]; every sampled record must
/// share an identical byte width.
pub fn infer_layout(source: &[u8], limits: Limits) -> Result<Layout> {
    if source.is_empty() {
        return Err(decline("empty input is not a fixed-width table"));
    }
    if source.len() as u64 > limits.max_fixedwidth_document_bytes {
        return Err(Error::resource_limit(format!(
            "fixed-width source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_fixedwidth_document_bytes
        )));
    }
    let bom_len = if source.starts_with(&[0xEF, 0xBB, 0xBF]) {
        3
    } else {
        0
    };
    let cap = limits
        .max_fixedwidth_sampled_lines_for_detection
        .max(MIN_ROWS as u32) as usize;

    let mut starts: Vec<usize> = Vec::with_capacity(cap);
    let mut width: Option<usize> = None;
    let mut terminator = TERM_LF;
    let mut terminator_seen = false;
    let mut mixed_terminators = false;

    let mut at = bom_len as usize;
    while starts.len() < cap {
        let (start, content_end, tag, next) = match split_line(source, at) {
            Some(x) => x,
            None => break,
        };
        let len = content_end - start;
        if len == 0 {
            return Err(decline("a blank line is not a fixed-width record"));
        }
        if len as u64 > limits.max_fixedwidth_record_bytes as u64 {
            return Err(Error::resource_limit(format!(
                "fixed-width record exceeds the {}-byte cap",
                limits.max_fixedwidth_record_bytes
            )));
        }
        match width {
            None => width = Some(len),
            Some(w) if w != len => {
                return Err(decline(
                    "sampled records do not share an identical byte width",
                ));
            }
            Some(_) => {}
        }
        if !terminator_seen {
            terminator = tag;
            terminator_seen = true;
        } else if tag != terminator {
            mixed_terminators = true;
        }
        starts.push(start);
        at = next;
    }

    if starts.len() < MIN_ROWS {
        return Err(decline(
            "fewer than the minimum sampled fixed-width records",
        ));
    }
    let w = width.expect("a non-empty sample has a width");

    // A position is a separator column iff **every** sampled record has a space there.
    // The sample is bounded by `max_fixedwidth_record_bytes` per line, so this bit
    // vector is bounded as well.
    let mut sep = vec![false; w];
    for (p, s) in sep.iter_mut().enumerate() {
        let mut all_space = true;
        for &rec_start in &starts {
            if source[rec_start + p] != b' ' {
                all_space = false;
                break;
            }
        }
        *s = all_space;
    }

    // Maximal runs of equal `sep` value, then drop the leading/trailing padding runs.
    let mut runs: Vec<(bool, usize, usize)> = Vec::new();
    let mut p = 0usize;
    while p < w {
        let s = sep[p];
        let st = p;
        while p < w && sep[p] == s {
            p += 1;
        }
        runs.push((s, st, p));
    }
    if !runs.is_empty() && runs[0].0 {
        runs.remove(0);
    }
    if !runs.is_empty() && runs[runs.len() - 1].0 {
        runs.pop();
    }

    let mut columns: Vec<Column> = Vec::new();
    for &(is_sep, s, e) in &runs {
        if is_sep {
            if e - s < MIN_GAP {
                return Err(decline(
                    "a column gap is narrower than the minimum two columns",
                ));
            }
        } else {
            columns.push(Column {
                start: s as u64,
                end: e as u64,
            });
        }
    }
    if columns.len() < MIN_COLS {
        return Err(decline("fewer than two non-empty columns"));
    }
    if columns.len() as u64 > limits.max_fixedwidth_cols as u64 {
        return Err(Error::resource_limit(format!(
            "fixed-width table exceeds the {}-column cap",
            limits.max_fixedwidth_cols
        )));
    }

    Ok(Layout {
        bom_len,
        terminator,
        mixed_terminators,
        width: w as u64,
        columns,
    })
}

/// Parse `source` into a [`FixedWidthModel`]. When `build` is false the record arena
/// is left empty (the scan still validates every record), so a detection-style call is
/// O(1) in extra memory. Every line must match the inferred uniform width, else the
/// whole document declines typed (never a silent truncation).
pub fn parse(source: &[u8], limits: Limits, build: bool) -> Result<FixedWidthModel> {
    let layout = infer_layout(source, limits)?;
    if layout.columns.len() as u64 > limits.max_fixedwidth_cols as u64 {
        return Err(Error::resource_limit(format!(
            "fixed-width table exceeds the {}-column cap",
            limits.max_fixedwidth_cols
        )));
    }
    let mut records: Vec<FwRecord> = Vec::new();
    let mut total_fields = 0u64;
    let mut at = layout.bom_len as usize;
    let mut terminator = TERM_LF;
    let mut terminator_seen = false;
    let mut mixed_terminators = false;

    while let Some((start, content_end, tag, next)) = split_line(source, at) {
        let len = content_end - start;
        if len as u64 != layout.width {
            return Err(corrupt("a record's width differs from the inferred layout"));
        }
        if !terminator_seen {
            terminator = tag;
            terminator_seen = true;
        } else if tag != terminator {
            mixed_terminators = true;
        }
        total_fields = total_fields.saturating_add(layout.columns.len() as u64);
        if total_fields > MAX_MODEL_FIELDS {
            return Err(Error::resource_limit(format!(
                "fixed-width table exceeds the {MAX_MODEL_FIELDS}-field model cap"
            )));
        }
        if records.len() as u64 >= limits.max_fixedwidth_rows as u64 {
            return Err(Error::resource_limit(format!(
                "fixed-width table exceeds the {}-record cap",
                limits.max_fixedwidth_rows
            )));
        }
        if build {
            let mut fields: Vec<FwField> = Vec::with_capacity(layout.columns.len());
            for c in &layout.columns {
                let s = (c.start as usize).min(len);
                let e = (c.end as usize).min(len);
                if (e - s) as u64 > limits.max_fixedwidth_field_bytes as u64 {
                    return Err(Error::resource_limit(format!(
                        "fixed-width field exceeds the {}-byte cap",
                        limits.max_fixedwidth_field_bytes
                    )));
                }
                fields.push(FwField {
                    start: (start + s) as u64,
                    end: (start + e) as u64,
                });
            }
            records.push(FwRecord {
                start: start as u64,
                end: content_end as u64,
                fields,
            });
        }
        at = next;
    }

    Ok(FixedWidthModel {
        doc_len: source.len() as u64,
        layout: Layout {
            terminator,
            mixed_terminators,
            ..layout
        },
        has_header: true,
        records,
    })
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of a `FixedWidthModel` node).
pub fn build_fixedwidth_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits, true)?.encode())
}

/// Read record `index` with a bounded-memory forward scan, re-validating the layout.
pub fn record_at(
    source: &[u8],
    layout: &Layout,
    index: u32,
    limits: Limits,
) -> Result<StreamRecord> {
    if source.len() as u64 > limits.max_fixedwidth_document_bytes {
        return Err(Error::resource_limit(format!(
            "fixed-width source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_fixedwidth_document_bytes
        )));
    }
    if index as u64 >= limits.max_fixedwidth_rows as u64 {
        return Err(Error::resource_limit(format!(
            "fixed-width record index {index} is at or above the {}-record cap",
            limits.max_fixedwidth_rows
        )));
    }
    let mut at = layout.bom_len as usize;
    let mut i: u32 = 0;
    while let Some((start, content_end, tag, next)) = split_line(source, at) {
        let len = content_end - start;
        if len as u64 != layout.width {
            return Err(corrupt("a record's width differs from the inferred layout"));
        }
        if i == index {
            let mut fields: Vec<FwField> = Vec::with_capacity(layout.columns.len());
            for c in &layout.columns {
                let s = (c.start as usize).min(len);
                let e = (c.end as usize).min(len);
                fields.push(FwField {
                    start: (start + s) as u64,
                    end: (start + e) as u64,
                });
            }
            return Ok(StreamRecord {
                start: start as u64,
                end: content_end as u64,
                terminator: tag,
                fields,
            });
        }
        i += 1;
        at = next;
    }
    Err(Error::unsupported_feature(format!(
        "fixed-width table has no record {index}"
    )))
}

/// The exact source bytes of a field's token (`[start, end)`, padding included).
pub fn field_bytes<'a>(source: &'a [u8], field: &FwField) -> Result<&'a [u8]> {
    let s = usize::try_from(field.start).map_err(|_| corrupt("span overflow"))?;
    let e = usize::try_from(field.end).map_err(|_| corrupt("span overflow"))?;
    source
        .get(s..e)
        .ok_or_else(|| corrupt("field span is outside the source"))
}

/// The exact source bytes of a record (`[start, end)`, its terminator excluded).
pub fn record_bytes<'a>(source: &'a [u8], record: &StreamRecord) -> Result<&'a [u8]> {
    let s = usize::try_from(record.start).map_err(|_| corrupt("span overflow"))?;
    let e = usize::try_from(record.end).map_err(|_| corrupt("span overflow"))?;
    source
        .get(s..e)
        .ok_or_else(|| corrupt("record span is outside the source"))
}

/// Decode a field's raw text (padding preserved), interpreted lossily as UTF-8 (use
/// [`field_bytes`] for the exact bytes, which are authoritative).
pub fn decode_field(source: &[u8], field: &FwField) -> Result<String> {
    Ok(String::from_utf8_lossy(field_bytes(source, field)?).into_owned())
}

/// Decode a field's text with surrounding ASCII spaces trimmed (the usual fixed-width
/// "cell value"). The exact padded bytes remain available via [`field_bytes`].
pub fn trimmed_text(source: &[u8], field: &FwField) -> Result<String> {
    let raw = field_bytes(source, field)?;
    let s = raw.iter().position(|&c| c != b' ').unwrap_or(raw.len());
    let e = raw.iter().rposition(|&c| c != b' ').map_or(s, |i| i + 1);
    Ok(String::from_utf8_lossy(&raw[s..e]).into_owned())
}

/// Decode every field of a streamed record, in order (padding preserved).
pub fn decode_record_fields(source: &[u8], record: &StreamRecord) -> Result<Vec<String>> {
    record
        .fields
        .iter()
        .map(|f| decode_field(source, f))
        .collect()
}

/// Render a deterministic canonical text projection: each record's exact content
/// bytes (padding preserved), records joined by `\n`. Only the terminator spelling is
/// normalized; every field keeps its source spelling. Declines typed if the rendered
/// text would exceed `max_out`.
pub fn canonical_text(
    source: &[u8],
    layout: &Layout,
    limits: Limits,
    max_out: u64,
) -> Result<String> {
    if source.len() as u64 > limits.max_fixedwidth_document_bytes {
        return Err(Error::resource_limit(format!(
            "fixed-width source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_fixedwidth_document_bytes
        )));
    }
    let mut at = layout.bom_len as usize;
    let mut out = String::new();
    let mut first = true;
    while let Some((start, content_end, _tag, next)) = split_line(source, at) {
        let len = content_end - start;
        if len as u64 != layout.width {
            return Err(corrupt("a record's width differs from the inferred layout"));
        }
        if !first {
            out.push('\n');
        }
        first = false;
        out.push_str(&String::from_utf8_lossy(
            source.get(start..content_end).unwrap_or_default(),
        ));
        if out.len() as u64 > max_out {
            return Err(Error::resource_limit(format!(
                "fixed-width text projection exceeds the {max_out}-byte budget"
            )));
        }
        at = next;
    }
    Ok(out)
}

/// A bounded, case-sensitive lexical search over decoded (trimmed) field text.
/// Returns matches in document order (record, then column). Declines typed if the
/// match list would exceed `max_out` bytes (an approximate bound).
pub fn find(
    source: &[u8],
    layout: &Layout,
    pattern: &str,
    limits: Limits,
    max_out: u64,
) -> Result<Vec<FwMatch>> {
    if source.len() as u64 > limits.max_fixedwidth_document_bytes {
        return Err(Error::resource_limit(format!(
            "fixed-width source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_fixedwidth_document_bytes
        )));
    }
    let mut at = layout.bom_len as usize;
    let mut out: Vec<FwMatch> = Vec::new();
    let mut estimated: u64 = 0;
    let mut record: u32 = 0;
    while let Some((start, content_end, _tag, next)) = split_line(source, at) {
        let len = content_end - start;
        if len as u64 != layout.width {
            return Err(corrupt("a record's width differs from the inferred layout"));
        }
        for (col, c) in layout.columns.iter().enumerate() {
            let s = (c.start as usize).min(len);
            let e = (c.end as usize).min(len);
            let field = FwField {
                start: (start + s) as u64,
                end: (start + e) as u64,
            };
            let text = trimmed_text(source, &field)?;
            if text.contains(pattern) {
                estimated = estimated.saturating_add(32 + text.len() as u64);
                if estimated > max_out {
                    return Err(Error::resource_limit(format!(
                        "fixed-width find exceeded the {max_out}-byte budget"
                    )));
                }
                out.push(FwMatch {
                    record,
                    column: col as u32,
                    start: field.start,
                    end: field.end,
                    text,
                });
            }
        }
        record = record.saturating_add(1);
        if record as u64 > MAX_MODEL_RECORDS as u64 {
            return Err(Error::resource_limit(
                "fixed-width find exceeded the record scan cap",
            ));
        }
        at = next;
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// Split one physical line at `at`. Returns `(content_start, content_end,
/// terminator_tag, next_offset)`; `None` at end of input. A trailing terminator does
/// not produce a phantom empty line.
fn split_line(b: &[u8], at: usize) -> Option<(usize, usize, u8, usize)> {
    if at >= b.len() {
        return None;
    }
    let start = at;
    let mut i = at;
    while i < b.len() && b[i] != b'\n' && b[i] != b'\r' {
        i += 1;
    }
    if i >= b.len() {
        return Some((start, i, TERM_LF, i));
    }
    if b[i] == b'\r' {
        if b.get(i + 1) == Some(&b'\n') {
            Some((start, i, TERM_CRLF, i + 2))
        } else {
            Some((start, i, TERM_CR, i + 1))
        }
    } else {
        Some((start, i, TERM_LF, i + 1))
    }
}

fn corrupt(msg: &str) -> Error {
    Error::invalid_fixedwidth_structure(format!("malformed fixed-width: {msg}"))
}

fn decline(msg: &str) -> Error {
    Error::invalid_fixedwidth_structure(format!("not a fixed-width table: {msg}"))
}

struct BinReader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> BinReader<'a> {
    fn new(b: &'a [u8]) -> Self {
        BinReader { b, at: 0 }
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| corrupt("reader overflow"))?;
        let s = self
            .b
            .get(self.at..end)
            .ok_or_else(|| corrupt("reader underrun"))?;
        self.at = end;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    fn u32(&mut self) -> Result<u32> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64> {
        let b = self.bytes(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    fn at_end(&self) -> bool {
        self.at == self.b.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three records of identical width 10, two fixed columns: `[0,5)` and `[7,10)`,
    /// separated by a two-wide (positions 5 and 6) all-space gap.
    const TABLE: &[u8] = b"Name   Age\nAlice   30\nBob     25\n";

    fn layout(source: &[u8]) -> Layout {
        infer_layout(source, Limits::DEFAULT).unwrap()
    }

    #[test]
    fn detects_uniform_width_tables_and_rejects_prose() {
        assert!(detect(TABLE, Limits::DEFAULT));
        let l = layout(TABLE);
        assert_eq!(l.columns.len(), 2);
        assert_eq!(l.columns[0], Column { start: 0, end: 5 });
        assert_eq!(l.columns[1], Column { start: 7, end: 10 });
        assert_eq!(l.width, 10);

        // Variable-length prose is not uniform-width and is rejected.
        assert!(!detect(
            b"Hello world\nThis is text.\nMore words here.\n",
            Limits::DEFAULT
        ));
        // A two-column blob separated by a *single* space is too ambiguous.
        assert!(!detect(b"a b\nc d\ne f\n", Limits::DEFAULT));
        // Too few records.
        assert!(!detect(b"Name  Age\nAlice  30\n", Limits::DEFAULT));
        // A delimited (CSV) table stays CSV, not fixed-width.
        assert!(!detect(b"a,b\nc,d\n", Limits::DEFAULT));
        // A Markdown table is not fixed-width.
        assert!(!detect(
            b"| a | b |\n| --- | --- |\n| c | d |\n",
            Limits::DEFAULT
        ));
        assert!(!detect(b"", Limits::DEFAULT));
    }

    #[test]
    fn preserves_spans_padding_and_terminators() {
        let src = b"Name   Age\r\nAlice   30\r\nBob     25\r\n";
        let m = parse(src, Limits::DEFAULT, true).unwrap();
        assert_eq!(m.layout.terminator, TERM_CRLF);
        assert_eq!(m.records.len(), 3);
        assert_eq!(m.records[0].fields.len(), 2);
        // Field 0 of record 1 is `Alice` (column width 5, exactly filled).
        assert_eq!(field_bytes(src, &m.records[1].fields[0]).unwrap(), b"Alice");
        assert_eq!(decode_field(src, &m.records[1].fields[0]).unwrap(), "Alice");
        assert_eq!(trimmed_text(src, &m.records[1].fields[0]).unwrap(), "Alice");
        // Field 1 of record 1 is ` 30` (leading padding preserved in ExactBytes).
        assert_eq!(field_bytes(src, &m.records[1].fields[1]).unwrap(), b" 30");
        assert_eq!(trimmed_text(src, &m.records[1].fields[1]).unwrap(), "30");
        // The record's exact bytes exclude the terminator.
        assert_eq!(
            &src[m.records[1].start as usize..m.records[1].end as usize],
            b"Alice   30"
        );
        // The model round-trips and fails closed on corruption.
        let enc = m.encode();
        assert_eq!(FixedWidthModel::decode(&enc).unwrap(), m);
        let mut bad = enc.clone();
        bad[0] = b'X';
        assert!(FixedWidthModel::decode(&bad).is_err());
        let mut truncated = enc;
        truncated.truncate(truncated.len() - 1);
        assert!(FixedWidthModel::decode(&truncated).is_err());
    }

    #[test]
    fn a_shorter_line_declines_the_whole_document() {
        // Detection samples only the first `cap` lines (here 3); a later line that is
        // short still makes the *model* build decline typed (never a truncation).
        let mut src = TABLE.to_vec();
        src.extend_from_slice(b"Ted  1\n");
        let tight = Limits {
            max_fixedwidth_sampled_lines_for_detection: 3,
            ..Limits::DEFAULT
        };
        assert!(detect(&src, tight));
        let e = parse(&src, tight, true).unwrap_err();
        assert_eq!(
            e.class(),
            crate::error::ErrorClass::InvalidFixedWidthStructure
        );
        // With the default (wider) sample the whole document is seen up front and is
        // not even detected.
        assert!(!detect(&src, Limits::DEFAULT));
    }

    #[test]
    fn caps_decline_typed() {
        let tight = Limits {
            max_fixedwidth_rows: 1,
            ..Limits::DEFAULT
        };
        let e = parse(TABLE, tight, true).unwrap_err();
        assert_eq!(e.class(), crate::error::ErrorClass::ResourceLimit);
        let tight = Limits {
            max_fixedwidth_cols: 1,
            ..Limits::DEFAULT
        };
        let e = parse(TABLE, tight, true).unwrap_err();
        assert_eq!(e.class(), crate::error::ErrorClass::ResourceLimit);
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        for _ in 0..128 {
            let mut buf = vec![0u8; 512];
            for b in buf.iter_mut() {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                *b = (x & 0xFF) as u8;
            }
            let _ = detect(&buf, Limits::DEFAULT);
            if let Ok(l) = infer_layout(&buf, Limits::STRICT) {
                let _ = canonical_text(&buf, &l, Limits::STRICT, 1 << 20);
                let _ = build_fixedwidth_model(&buf, Limits::STRICT);
                let _ = find(&buf, &l, "a", Limits::STRICT, 1 << 20);
            }
        }
    }
}
