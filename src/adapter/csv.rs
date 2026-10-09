//! Bounded, representation-preserving CSV/TSV adapter (Phase 21.7.1).
//!
//! CSV/TSV is the first **tabular** Wave-2 format. It is *not* an office package:
//! there is no OPC/ZIP layer, no `mimetype`, and no relationship graph. The exact
//! leaf is therefore the **whole source** (a `DocumentExact`, a RAW-like
//! authority), and everything this module produces is a bounded, deterministic
//! (`Q_gen`) projection that never sits on the exactness path.
//!
//! ## Why a bespoke parser
//!
//! The point of a CSV adapter is to preserve **representation**, not merely
//! values. For every record and every field the parser records its exact **byte
//! span** in the source, so it preserves, and can report:
//!
//! * the **dialect** (delimiter `,` or tab, quote `"`, line terminator CRLF/LF/CR,
//!   an optional UTF-8 BOM) — never guessed silently, always reported;
//! * the **original quoting** (`"a,b"` keeps its quote bytes; `""` escapes are not
//!   collapsed away in `ExactBytes`);
//! * **embedded delimiters, newlines, and quotes** inside quoted fields (a record
//!   may span several physical lines);
//! * **ragged rows** (a record whose field count differs from the header is kept,
//!   not normalized or dropped), and **blank lines** (a one-empty-field record);
//! * the **header row** (record 0) and its field names.
//!
//! ## Bounded, streaming core
//!
//! Detection and every selective read are **streaming**: a single forward scan
//! that holds at most one record's fields in memory, so a very large source is
//! handled in bounded memory and any cap breach is a typed decline. The optional
//! canonical model ([`CsvModel`]) records the full record/field span table and is
//! bounded by the `max_csv_*` caps (and an internal field-count ceiling); it is
//! the materialization of a `CsvModel` node and is used only for whole-table
//! structural projections.
//!
//! ## Detection
//!
//! CSV has **no magic bytes**, so detection is deliberately conservative: after
//! the PDF/ZIP/JSON/YAML families have been declined, the source must parse under
//! a specific delimiter (`,` or tab) as a table with a **consistent field count
//! across a sampled majority of at least two records** and **at least two
//! columns**, with the first record (the header) sharing that count. When in
//! doubt the input stays
//! [`Opaque`](crate::field::document_format::DocumentFormat::Opaque).

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model records (defends the decoder against a hostile blob).
pub const MAX_MODEL_RECORDS: u32 = 1 << 24;
/// Hard cap on decoded model fields (defends the decoder against a hostile blob).
pub const MAX_MODEL_FIELDS: u64 = 1 << 24;

/// Delimiter byte for a comma-separated file.
pub const DELIM_COMMA: u8 = b',';
/// Delimiter byte for a tab-separated file.
pub const DELIM_TAB: u8 = b'\t';
/// Terminator tag: `\n`.
pub const TERM_LF: u8 = 0;
/// Terminator tag: `\r\n`.
pub const TERM_CRLF: u8 = 1;
/// Terminator tag: a bare `\r`.
pub const TERM_CR: u8 = 2;
/// The (only) quote character admitted; a field is quoted iff it opens with it.
pub const QUOTE: u8 = b'"';

/// The recorded dialect of a CSV/TSV source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dialect {
    /// The field delimiter (`DELIM_COMMA` or `DELIM_TAB`).
    pub delimiter: u8,
    /// The quote character (always `QUOTE`).
    pub quote: u8,
    /// The dominant line terminator (`TERM_LF`/`TERM_CRLF`/`TERM_CR`).
    pub terminator: u8,
    /// Length of a leading UTF-8 BOM (`0` or `3`), a document-level prefix.
    pub bom_len: u8,
    /// Whether records used more than one terminator spelling.
    pub mixed_terminators: bool,
}

impl Dialect {
    /// Stable name for the field delimiter.
    pub const fn delimiter_name(&self) -> &'static str {
        match self.delimiter {
            DELIM_TAB => "tab",
            _ => "comma",
        }
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

/// One field's exact source span (`[start, end)`, quotes included when quoted).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CsvField {
    /// The field's first source byte.
    pub start: u64,
    /// One past the field's last source byte.
    pub end: u64,
    /// Whether the field was written as a quoted (`"…"`) token.
    pub quoted: bool,
}

/// One record's exact source span (`[start, end)`, the line terminator *excluded*)
/// and its fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvRecord {
    /// The record's first source byte.
    pub start: u64,
    /// One past the record's last content byte (its terminator is not included).
    pub end: u64,
    /// The record's fields, in order.
    pub fields: Vec<CsvField>,
}

/// A streamed single record (as returned by [`record_at`]); carries the exact
/// terminator spelling, which the canonical model records only in the dialect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamRecord {
    /// The record's first source byte.
    pub start: u64,
    /// One past the record's last content byte (its terminator is not included).
    pub end: u64,
    /// The terminator tag that ended the record (`TERM_LF`/`TERM_CRLF`/`TERM_CR`).
    pub terminator: u8,
    /// The record's fields, in order.
    pub fields: Vec<CsvField>,
}

/// The canonical derived CSV/TSV model (the materialization of a `CsvModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvModel {
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// The recorded dialect.
    pub dialect: Dialect,
    /// Whether record 0 is treated as a header (always true in v1).
    pub has_header: bool,
    /// The record arena, in physical order.
    pub records: Vec<CsvRecord>,
}

impl CsvModel {
    /// The record at `index`, if present.
    pub fn node(&self, index: u32) -> Option<&CsvRecord> {
        self.records.get(index as usize)
    }

    /// The number of records (including the header).
    pub fn row_count(&self) -> usize {
        self.records.len()
    }

    /// The header record (record 0), if any.
    pub fn header(&self) -> Option<&CsvRecord> {
        self.records.first()
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32 + self.records.len() * 24);
        out.extend_from_slice(b"CSVM");
        out.push(MODEL_VERSION);
        out.push(self.dialect.delimiter);
        out.push(self.dialect.quote);
        out.push(self.dialect.terminator);
        out.push(self.dialect.bom_len);
        let mut flags = 0u8;
        if self.has_header {
            flags |= 1;
        }
        if self.dialect.mixed_terminators {
            flags |= 2;
        }
        out.push(flags);
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&(self.records.len() as u32).to_le_bytes());
        for r in &self.records {
            out.extend_from_slice(&r.start.to_le_bytes());
            out.extend_from_slice(&r.end.to_le_bytes());
            out.extend_from_slice(&(r.fields.len() as u32).to_le_bytes());
            for f in &r.fields {
                out.extend_from_slice(&f.start.to_le_bytes());
                out.extend_from_slice(&f.end.to_le_bytes());
                out.push(u8::from(f.quoted));
            }
        }
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<CsvModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"CSVM" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let delimiter = r.u8()?;
        if delimiter != DELIM_COMMA && delimiter != DELIM_TAB {
            return Err(corrupt("unknown delimiter"));
        }
        let quote = r.u8()?;
        if quote != QUOTE {
            return Err(corrupt("unknown quote character"));
        }
        let terminator = r.u8()?;
        if terminator > TERM_CR {
            return Err(corrupt("unknown terminator"));
        }
        let bom_len = r.u8()?;
        if bom_len > 3 {
            return Err(corrupt("implausible BOM length"));
        }
        let flags = r.u8()?;
        if flags & !3 != 0 {
            return Err(corrupt("unknown model flags"));
        }
        let doc_len = r.u64()?;
        let count = r.u32()?;
        if count > MAX_MODEL_RECORDS {
            return Err(corrupt("model record count is implausible"));
        }
        let mut records = Vec::with_capacity(count as usize);
        let mut total_fields = 0u64;
        for _ in 0..count {
            let start = r.u64()?;
            let end = r.u64()?;
            if start > end || end > doc_len {
                return Err(corrupt("record span is outside the document"));
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
                let quoted = r.u8()?;
                if quoted > 1 {
                    return Err(corrupt("invalid quoted flag"));
                }
                if fs > fe || fs < start || fe > end {
                    return Err(corrupt("field span is outside its record"));
                }
                fields.push(CsvField {
                    start: fs,
                    end: fe,
                    quoted: quoted == 1,
                });
            }
            records.push(CsvRecord { start, end, fields });
        }
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        if bom_len as u64 > doc_len {
            return Err(corrupt("BOM length exceeds the document"));
        }
        Ok(CsvModel {
            doc_len,
            dialect: Dialect {
                delimiter,
                quote,
                terminator,
                bom_len,
                mixed_terminators: flags & 2 != 0,
            },
            has_header: flags & 1 != 0,
            records,
        })
    }
}

/// One lexical-match record from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvMatch {
    /// The 0-based record index.
    pub record: u32,
    /// The 0-based column index within the record.
    pub column: u32,
    /// Whether the matched field was quoted (raw bytes keep their quotes).
    pub quoted: bool,
    /// The exact source span of the matching field (quotes included when quoted).
    pub start: u64,
    /// One past the matching field.
    pub end: u64,
    /// The decoded field text (quotes removed, `""` unescaped).
    pub text: String,
}

/// Byte-based CSV/TSV detector. See the module docs for the exact heuristic.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if looks_like_other(source, limits) {
        return false;
    }
    sniff_dialect(source, limits).is_ok()
}

/// Whether `source` belongs to a family that is *not* CSV (defence in depth; the
/// dispatcher also orders PDF/ZIP/JSON/YAML ahead of CSV).
fn looks_like_other(source: &[u8], limits: Limits) -> bool {
    let _ = limits;
    if source.starts_with(b"%PDF-") {
        return true;
    }
    if source.starts_with(b"PK\x03\x04")
        || source.starts_with(b"PK\x05\x06")
        || source.starts_with(b"PK\x07\x08")
    {
        return true;
    }
    #[cfg(feature = "json")]
    if crate::adapter::json::detect(source, limits) {
        return true;
    }
    #[cfg(feature = "yaml")]
    if crate::adapter::yaml::detect(source, limits) {
        return true;
    }
    false
}

/// Determine the dialect of a CSV/TSV source, or decline typed when the source is
/// not a table under the conservative heuristic.
pub fn sniff_dialect(source: &[u8], limits: Limits) -> Result<Dialect> {
    if source.is_empty() {
        return Err(corrupt("empty input is not a CSV table"));
    }
    if source.len() as u64 > limits.max_csv_document_bytes {
        return Err(Error::resource_limit(format!(
            "CSV source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_csv_document_bytes
        )));
    }
    let bom_len = if source.starts_with(&[0xEF, 0xBB, 0xBF]) {
        3
    } else {
        0
    };
    for delimiter in [DELIM_COMMA, DELIM_TAB] {
        if let Some(dialect) = try_dialect(source, delimiter, bom_len, limits) {
            return Ok(dialect);
        }
    }
    Err(corrupt(
        "source is not a CSV table under a comma or tab delimiter",
    ))
}

/// Try one delimiter: stream up to the detection sample cap and check that the
/// field count is consistent across a majority of at least two records, that the
/// count is at least two, and that the header shares it.
fn try_dialect(source: &[u8], delimiter: u8, bom_len: u8, limits: Limits) -> Option<Dialect> {
    let cap = limits.max_csv_sampled_records_for_detection.max(2) as usize;
    let base = Dialect {
        delimiter,
        quote: QUOTE,
        terminator: TERM_LF,
        bom_len,
        mixed_terminators: false,
    };
    let mut scanner = Scanner::new(source, base, limits);
    let mut counts: Vec<u64> = Vec::new();
    for _ in 0..cap {
        match scanner.next_record(true) {
            Ok(Some(rec)) => counts.push(rec.nfields),
            Ok(None) => break,
            Err(_) => return None,
        }
    }
    if counts.len() < 2 {
        return None;
    }
    // Modal field count over the sample.
    let mut best = 0u64;
    let mut best_freq = 0usize;
    for &c in &counts {
        let freq = counts.iter().filter(|&&x| x == c).count();
        if freq > best_freq || (freq == best_freq && c > best) {
            best = c;
            best_freq = freq;
        }
    }
    // A majority must be consistent, the header must share the count, and the
    // count must be at least two (a one-column file is indistinguishable from
    // plain text and is deliberately left Opaque).
    if best < 2 || best_freq * 2 <= counts.len() || counts[0] != best {
        return None;
    }
    Some(Dialect {
        terminator: scanner.terminator,
        mixed_terminators: scanner.mixed_terminators,
        ..base
    })
}

/// Parse `source` into a [`CsvModel`]. When `build` is false the record arena is
/// left empty (the scan still validates every record and counts fields), so a
/// detection-style call is O(1) in extra memory.
pub fn parse(source: &[u8], limits: Limits, build: bool) -> Result<CsvModel> {
    let dialect = sniff_dialect(source, limits)?;
    let mut scanner = Scanner::new(source, dialect, limits);
    let mut records: Vec<CsvRecord> = Vec::new();
    let mut total_fields = 0u64;
    while let Some(rec) = scanner.next_record(!build)? {
        total_fields = total_fields.saturating_add(rec.nfields);
        if total_fields > MAX_MODEL_FIELDS {
            return Err(Error::resource_limit(format!(
                "CSV table exceeds the {MAX_MODEL_FIELDS}-field model cap"
            )));
        }
        if records.len() as u64 >= limits.max_csv_rows as u64 {
            return Err(Error::resource_limit(format!(
                "CSV table exceeds the {}-record cap",
                limits.max_csv_rows
            )));
        }
        if build {
            records.push(CsvRecord {
                start: rec.start,
                end: rec.end,
                fields: rec.fields,
            });
        }
    }
    Ok(CsvModel {
        doc_len: source.len() as u64,
        dialect,
        has_header: true,
        records,
    })
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of a `CsvModel` node).
pub fn build_csv_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits, true)?.encode())
}

/// Read record `index` with a bounded-memory forward scan. Skips earlier records
/// without retaining their fields, so the peak memory is one record.
pub fn record_at(
    source: &[u8],
    dialect: Dialect,
    index: u32,
    limits: Limits,
) -> Result<StreamRecord> {
    if source.len() as u64 > limits.max_csv_document_bytes {
        return Err(Error::resource_limit(format!(
            "CSV source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_csv_document_bytes
        )));
    }
    if index as u64 >= limits.max_csv_rows as u64 {
        return Err(Error::resource_limit(format!(
            "CSV record index {index} is at or above the {}-record cap",
            limits.max_csv_rows
        )));
    }
    let mut scanner = Scanner::new(source, dialect, limits);
    let mut i: u32 = 0;
    loop {
        match scanner.next_record(i != index)? {
            Some(rec) => {
                if i == index {
                    return Ok(StreamRecord {
                        start: rec.start,
                        end: rec.end,
                        terminator: rec.terminator,
                        fields: rec.fields,
                    });
                }
                i += 1;
            }
            None => {
                return Err(Error::unsupported_feature(format!(
                    "CSV table has no record {index}"
                )));
            }
        }
    }
}

/// The exact source bytes of a field's token (`[start, end)`).
pub fn field_bytes<'a>(source: &'a [u8], field: &CsvField) -> Result<&'a [u8]> {
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

/// Decode a field's text: a quoted token drops its outer quotes and unescapes
/// `""` to `"`; the decoded bytes are interpreted lossily as UTF-8 (use
/// [`field_bytes`] for the exact bytes, which are authoritative).
pub fn decode_field(source: &[u8], field: &CsvField) -> Result<String> {
    let tok = field_bytes(source, field)?;
    if !field.quoted {
        return Ok(String::from_utf8_lossy(tok).into_owned());
    }
    if tok.len() < 2 || tok[0] != QUOTE || tok[tok.len() - 1] != QUOTE {
        return Err(corrupt("quoted field is not a well-formed quoted token"));
    }
    let inner = &tok[1..tok.len() - 1];
    let mut out: Vec<u8> = Vec::with_capacity(inner.len());
    let mut i = 0usize;
    while i < inner.len() {
        if inner[i] == QUOTE && inner.get(i + 1) == Some(&QUOTE) {
            out.push(QUOTE);
            i += 2;
        } else {
            out.push(inner[i]);
            i += 1;
        }
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// Decode every field of a streamed record, in order.
pub fn decode_record_fields(source: &[u8], record: &StreamRecord) -> Result<Vec<String>> {
    record
        .fields
        .iter()
        .map(|f| decode_field(source, f))
        .collect()
}

/// Render a deterministic canonical text projection: the exact field bytes (their
/// quoting preserved) joined by the delimiter, records joined by `\n`. Only the
/// terminator spelling is normalized; every field keeps its source spelling.
/// Declines typed if the rendered text would exceed `max_out`.
pub fn canonical_text(
    source: &[u8],
    dialect: Dialect,
    limits: Limits,
    max_out: u64,
) -> Result<String> {
    let mut scanner = Scanner::new(source, dialect, limits);
    let mut out = String::new();
    let mut first = true;
    while let Some(rec) = scanner.next_record(false)? {
        if !first {
            out.push('\n');
        }
        first = false;
        for (i, f) in rec.fields.iter().enumerate() {
            if i > 0 {
                out.push(dialect.delimiter as char);
            }
            out.push_str(&String::from_utf8_lossy(field_bytes(source, f)?));
            if out.len() as u64 > max_out {
                return Err(Error::resource_limit(format!(
                    "CSV text projection exceeds the {max_out}-byte budget"
                )));
            }
        }
    }
    Ok(out)
}

/// A bounded, case-sensitive lexical search over decoded field text. Returns
/// matches in document order (record, then column). Declines typed if the match
/// list would exceed `max_out` bytes (an approximate bound).
pub fn find(
    source: &[u8],
    dialect: Dialect,
    pattern: &str,
    limits: Limits,
    max_out: u64,
) -> Result<Vec<CsvMatch>> {
    let mut scanner = Scanner::new(source, dialect, limits);
    let mut out: Vec<CsvMatch> = Vec::new();
    let mut estimated: u64 = 0;
    let mut record: u32 = 0;
    while let Some(rec) = scanner.next_record(false)? {
        for (col, f) in rec.fields.iter().enumerate() {
            let text = decode_field(source, f)?;
            if text.contains(pattern) {
                estimated = estimated.saturating_add(32 + text.len() as u64);
                if estimated > max_out {
                    return Err(Error::resource_limit(format!(
                        "CSV find exceeded the {max_out}-byte budget"
                    )));
                }
                out.push(CsvMatch {
                    record,
                    column: col as u32,
                    quoted: f.quoted,
                    start: f.start,
                    end: f.end,
                    text,
                });
            }
        }
        record = record.saturating_add(1);
        if record as u64 > MAX_MODEL_RECORDS as u64 {
            return Err(Error::resource_limit(
                "CSV find exceeded the record scan cap",
            ));
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// The streaming scanner
// ---------------------------------------------------------------------------

/// A record as produced by the scanner: `fields` is empty when `count_only` was
/// requested (the field count is still exact in `nfields`).
struct RawRecord {
    start: u64,
    end: u64,
    terminator: u8,
    nfields: u64,
    fields: Vec<CsvField>,
}

struct Scanner<'a> {
    b: &'a [u8],
    at: usize,
    dialect: Dialect,
    limits: Limits,
    terminator: u8,
    terminator_seen: bool,
    mixed_terminators: bool,
}

impl<'a> Scanner<'a> {
    fn new(b: &'a [u8], dialect: Dialect, limits: Limits) -> Self {
        Scanner {
            b,
            at: dialect.bom_len as usize,
            dialect,
            limits,
            terminator: dialect.terminator,
            terminator_seen: false,
            mixed_terminators: false,
        }
    }

    #[inline]
    fn peek(&self) -> Option<u8> {
        self.b.get(self.at).copied()
    }

    /// Consume one line terminator and return its tag.
    fn consume_eol(&mut self) -> u8 {
        let tag = if self.b[self.at] == b'\r' {
            if self.b.get(self.at + 1) == Some(&b'\n') {
                self.at += 2;
                TERM_CRLF
            } else {
                self.at += 1;
                TERM_CR
            }
        } else {
            self.at += 1;
            TERM_LF
        };
        if !self.terminator_seen {
            self.terminator = tag;
            self.terminator_seen = true;
        } else if tag != self.terminator {
            self.mixed_terminators = true;
        }
        tag
    }

    /// Parse the next record. With `count_only`, fields are not retained (only
    /// counted), so a hostile record cannot drive an unbounded allocation.
    fn next_record(&mut self, count_only: bool) -> Result<Option<RawRecord>> {
        if self.at >= self.b.len() {
            return Ok(None);
        }
        let d = self.dialect.delimiter;
        let q = self.dialect.quote;
        let start = self.at as u64;
        let mut fields: Vec<CsvField> = Vec::new();
        let mut nfields: u64 = 0;
        let end: u64;
        let terminator: u8;
        loop {
            let field_start = self.at;
            let quoted;
            if self.peek() == Some(q) {
                quoted = true;
                self.at += 1;
                loop {
                    let c = self
                        .peek()
                        .ok_or_else(|| corrupt("unterminated quoted field"))?;
                    if c == q {
                        if self.b.get(self.at + 1) == Some(&q) {
                            self.at += 2; // an escaped `""`
                        } else {
                            self.at += 1; // the close quote
                            break;
                        }
                    } else {
                        self.at += 1; // includes embedded delimiters/newlines
                    }
                }
            } else {
                quoted = false;
                while let Some(c) = self.peek() {
                    if c == d || c == b'\r' || c == b'\n' {
                        break;
                    }
                    self.at += 1;
                }
            }
            let field_end = self.at;
            // Charge caps only when retaining (streaming a record is bounded by
            // the record's own length, which the document cap already bounds).
            if !count_only {
                if field_end - field_start > self.limits.max_csv_field_bytes as usize {
                    return Err(Error::resource_limit(format!(
                        "CSV field exceeds the {}-byte cap",
                        self.limits.max_csv_field_bytes
                    )));
                }
                if nfields >= self.limits.max_csv_cols as u64 {
                    return Err(Error::resource_limit(format!(
                        "CSV record exceeds the {}-column cap",
                        self.limits.max_csv_cols
                    )));
                }
                fields.push(CsvField {
                    start: field_start as u64,
                    end: field_end as u64,
                    quoted,
                });
            }
            nfields += 1;
            match self.peek() {
                Some(c) if c == d => {
                    self.at += 1;
                }
                Some(b'\r') | Some(b'\n') => {
                    end = field_end as u64;
                    terminator = self.consume_eol();
                    break;
                }
                Some(_) => {
                    return Err(corrupt(
                        "unexpected byte after a closing quote (expected a delimiter or end of record)",
                    ));
                }
                None => {
                    end = self.at as u64;
                    terminator = TERM_LF;
                    break;
                }
            }
        }
        if !count_only && end - start > self.limits.max_csv_record_bytes as u64 {
            return Err(Error::resource_limit(format!(
                "CSV record exceeds the {}-byte cap",
                self.limits.max_csv_record_bytes
            )));
        }
        Ok(Some(RawRecord {
            start,
            end,
            terminator,
            nfields,
            fields: if count_only { Vec::new() } else { fields },
        }))
    }
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn corrupt(msg: &str) -> Error {
    Error::invalid_csv_structure(format!("malformed CSV: {msg}"))
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

    fn dialect(source: &[u8]) -> Dialect {
        sniff_dialect(source, Limits::DEFAULT).unwrap()
    }

    #[test]
    fn detects_tables_and_rejects_plain_text() {
        assert!(detect(b"a,b\nc,d\n", Limits::DEFAULT));
        assert!(detect(b"a\tb\nc\td\n", Limits::DEFAULT));
        // A one-column blob is not a table (indistinguishable from text).
        assert!(!detect(b"hello\nworld\n", Limits::DEFAULT));
        // A single record is not enough; the heuristic needs >= 2 records.
        assert!(!detect(b"a,b,c", Limits::DEFAULT));
        // Inconsistent field counts (a prose two-liner) are declined.
        assert!(!detect(
            b"Hello, world.\nThis is a test.\n",
            Limits::DEFAULT
        ));
        assert!(!detect(b"", Limits::DEFAULT));
        // A JSON object is not CSV (even though it has commas).
        assert!(!detect(b"{\"a\":1,\"b\":2}", Limits::DEFAULT));
    }

    #[test]
    fn reports_dialect() {
        let d = dialect(b"a,b\r\nc,d\r\n");
        assert_eq!(d.delimiter, DELIM_COMMA);
        assert_eq!(d.terminator, TERM_CRLF);
        assert_eq!(d.bom_len, 0);
        let d = dialect(b"a\tb\nc\td\n");
        assert_eq!(d.delimiter_name(), "tab");
        let d = dialect(b"\xEF\xBB\xBFa,b\nc,d\n");
        assert_eq!(d.bom_len, 3);
    }

    #[test]
    fn preserves_quotes_embedded_delimiters_and_newlines() {
        let src = b"a,\"b,c\",d\n\"x\ny\",2,3\n";
        let m = parse(src, Limits::DEFAULT, true).unwrap();
        assert_eq!(m.records.len(), 2);
        assert_eq!(m.records[0].fields.len(), 3);
        // The quoted field keeps its exact bytes, commas included.
        let f = m.records[0].fields[1];
        assert!(f.quoted);
        assert_eq!(field_bytes(src, &f).unwrap(), b"\"b,c\"");
        assert_eq!(decode_field(src, &f).unwrap(), "b,c");
        // The record with an embedded newline keeps its exact bytes (no terminator).
        assert_eq!(
            record_bytes(src, &stream(m.records[1].clone())).unwrap(),
            b"\"x\ny\",2,3"
        );
    }

    fn stream(r: CsvRecord) -> StreamRecord {
        StreamRecord {
            start: r.start,
            end: r.end,
            terminator: TERM_LF,
            fields: r.fields,
        }
    }

    #[test]
    fn unescapes_doubled_quotes() {
        let src = b"\"a\"\"b\",c\n1,2\n";
        let d = dialect(src);
        let r = record_at(src, d, 0, Limits::DEFAULT).unwrap();
        assert_eq!(decode_field(src, &r.fields[0]).unwrap(), "a\"b");
        assert_eq!(field_bytes(src, &r.fields[0]).unwrap(), b"\"a\"\"b\"");
    }

    #[test]
    fn ragged_rows_are_preserved() {
        let src = b"a,b,c\n1,2\n3,4,5\n";
        let m = parse(src, Limits::DEFAULT, true).unwrap();
        assert_eq!(m.records[0].fields.len(), 3);
        assert_eq!(m.records[1].fields.len(), 2);
        assert_eq!(m.records[2].fields.len(), 3);
    }

    #[test]
    fn model_roundtrips_and_fails_closed() {
        let src = b"a,b\r\n\"c\",d\r\n";
        let m = parse(src, Limits::DEFAULT, true).unwrap();
        let enc = m.encode();
        assert_eq!(CsvModel::decode(&enc).unwrap(), m);
        let mut bad = enc.clone();
        bad[0] = b'X';
        assert!(CsvModel::decode(&bad).is_err());
        let mut truncated = enc;
        truncated.truncate(truncated.len() - 1);
        assert!(CsvModel::decode(&truncated).is_err());
    }

    #[test]
    fn column_cap_declines_typed() {
        // STRICT caps columns at 4096; 5,000 fields per record exceed it.
        let mut line = String::new();
        for _ in 0..5000 {
            line.push_str("x,");
        }
        line.push_str("x\n");
        let src = format!("{line}{line}").into_bytes();
        let e = parse(&src, Limits::STRICT, true).unwrap_err();
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
            if let Ok(d) = sniff_dialect(&buf, Limits::DEFAULT) {
                let _ = canonical_text(&buf, d, Limits::STRICT, 1 << 20);
                let _ = build_csv_model(&buf, Limits::STRICT);
            }
        }
    }
}
