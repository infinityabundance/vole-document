//! Bounded, per-line JSONL / NDJSON adapter (Phase 21.12).
//!
//! JSONL (a.k.a. NDJSON, JSON Lines) is a **line/event stream**: one JSON value
//! per physical line, newline-separated. Like the other Wave-2 formats it is *not*
//! an office package: there is no OPC/ZIP layer, so the exact leaf is the **whole
//! source** (a `DocumentExact`, a RAW-like authority) and everything this module
//! produces is a bounded, deterministic (`Q_gen`) projection that never sits on the
//! exactness path.
//!
//! ## Reuse, not a second parser
//!
//! A JSONL record is defined to be *exactly one JSON value*, and the point of the
//! adapter is to preserve **representation**, not merely values. Rather than grow a
//! second JSON parser, JSONL **reuses the existing bounded JSON parser**
//! ([`crate::adapter::json`]) once per line: the source is split into physical lines
//! and each non-blank line's JSON text is handed to [`crate::adapter::json::parse`].
//! The resulting per-record node arena keeps exactly the guarantees the JSON adapter
//! already proves — object **member order** (never normalized), **duplicate keys**
//! (kept distinct, never overwritten), **numeric spelling** (`1e3`, `-0` kept
//! literally), **string-escape spelling** (`\u00e9` distinct from `é`), and the
//! **exact byte span** of every token — with those spans lifted into whole-source
//! coordinates by adding the line's start offset.
//!
//! ## Detection
//!
//! [`detect`] is deliberately conservative and **distinguishable from a single JSON
//! value**: the source must carry **at least two** non-blank lines and **every**
//! non-blank line must parse as exactly one JSON value under the caps. A
//! single-value file (even one spread across several lines, e.g. `[\n1\n]`) stays
//! [`crate::field::document_format::DocumentFormat::Json`] — the detection order
//! tries JSON first, and the per-line rule rejects it anyway. A malformed line, a
//! bag of JSON values with a **non-newline** separator (e.g. `{"a":1}{"b":2}` or
//! `1,2,3`), or a file that exceeds a cap is not JSONL.
//!
//! Detection runs the JSON parser with node-building **disabled**, so it is O(1) in
//! extra memory and cannot be forced to allocate by a large near-JSONL input; the
//! cumulative node budget is enforced when the model is built.
//!
//! ## Bounds and no panics
//!
//! Untrusted input can only ever yield a typed decline, never a panic or an
//! unbounded allocation: the number of records by [`Limits::max_jsonl_records`],
//! one line's JSON text by [`Limits::max_jsonl_line_bytes`], the total node count
//! across every record by [`Limits::max_jsonl_nodes`] (and one line's nodes by
//! [`Limits::max_json_nodes`] and its nesting depth by [`Limits::max_json_depth`]),
//! and the source length by [`Limits::max_jsonl_document_bytes`].

use crate::adapter::json::{self, JNode, JsonMatch, JsonModel, MatchRole};
use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model records (defends the decoder against a hostile blob).
pub const MAX_MODEL_RECORDS: u32 = 1 << 24;

/// Line terminator tag: the last line ended at EOF (no terminator).
pub const T_NONE: u8 = 0;
/// Line terminator tag: a bare `\n`.
pub const T_LF: u8 = 1;
/// Line terminator tag: a `\r\n` (CRLF) pair.
pub const T_CRLF: u8 = 2;

/// Stable lower-case name of a line terminator tag.
pub const fn terminator_name(t: u8) -> &'static str {
    match t {
        T_LF => "lf",
        T_CRLF => "crlf",
        _ => "none",
    }
}

/// One JSONL record: its **exact source line span**, its terminator, its physical
/// line number, and its parse (a [`JsonModel`] whose node spans are lifted into
/// whole-source coordinates and whose `doc_len` is the whole source length).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonlRecord {
    /// The first source byte of the physical line.
    pub line_start: u64,
    /// One past the line's terminator (or past its last byte at EOF).
    pub line_end: u64,
    /// The line's terminator tag (`T_NONE`/`T_LF`/`T_CRLF`).
    pub terminator: u8,
    /// The 0-based physical line number (blank lines are counted).
    pub line_number: u32,
    /// The record's parse; node spans are absolute and `doc_len` is the source.
    pub model: JsonModel,
}

impl JsonlRecord {
    /// The record's root value node.
    pub fn value_node(&self) -> Result<&JNode> {
        self.model
            .node(self.model.root)
            .ok_or_else(|| Error::internal_invariant("JSONL record root is out of range"))
    }

    /// The record's value token span (absolute in the source).
    pub fn value_span(&self) -> Result<(u64, u64)> {
        let n = self.value_node()?;
        Ok((n.start, n.end))
    }

    /// The record's value kind tag.
    pub fn value_kind(&self) -> Result<u8> {
        Ok(self.value_node()?.kind)
    }

    /// One past the line's content (the terminator excluded).
    pub fn content_end(&self) -> u64 {
        match self.terminator {
            T_LF => self.line_end - 1,
            T_CRLF => self.line_end - 2,
            _ => self.line_end,
        }
    }
}

/// The canonical derived JSONL model (the materialization of a `JsonlModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonlModel {
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// The non-blank records, in source order.
    pub records: Vec<JsonlRecord>,
    /// The number of physical lines that were blank (whitespace-only).
    pub blank_lines: u32,
    /// The number of records terminated by a CRLF pair.
    pub crlf_records: u32,
    /// Whether the source ended with a line terminator.
    pub trailing_newline: bool,
    /// The observed maximum container nesting depth across records (root = 1).
    pub max_depth: u32,
    /// The sum of every record line's JSON-text byte length.
    pub total_line_bytes: u64,
    /// The smallest record line's JSON-text byte length (`0` when there are none).
    pub min_line_bytes: u64,
    /// The largest record line's JSON-text byte length.
    pub max_line_bytes: u64,
}

impl JsonlModel {
    /// The record at `index`, if present.
    pub fn record(&self, index: u32) -> Option<&JsonlRecord> {
        self.records.get(index as usize)
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64 + self.records.len() * 64);
        out.extend_from_slice(b"JSONL");
        out.push(MODEL_VERSION);
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&self.blank_lines.to_le_bytes());
        out.extend_from_slice(&self.crlf_records.to_le_bytes());
        out.push(u8::from(self.trailing_newline));
        out.extend_from_slice(&self.max_depth.to_le_bytes());
        out.extend_from_slice(&self.total_line_bytes.to_le_bytes());
        out.extend_from_slice(&self.min_line_bytes.to_le_bytes());
        out.extend_from_slice(&self.max_line_bytes.to_le_bytes());
        out.extend_from_slice(&(self.records.len() as u32).to_le_bytes());
        for r in &self.records {
            out.extend_from_slice(&r.line_start.to_le_bytes());
            out.extend_from_slice(&r.line_end.to_le_bytes());
            out.push(r.terminator);
            out.extend_from_slice(&r.line_number.to_le_bytes());
            let mb = r.model.encode();
            out.extend_from_slice(&(mb.len() as u32).to_le_bytes());
            out.extend_from_slice(&mb);
        }
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<JsonlModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(5)? != b"JSONL" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let doc_len = r.u64()?;
        let blank_lines = r.u32()?;
        let crlf_records = r.u32()?;
        let trailing_newline = match r.u8()? {
            0 => false,
            1 => true,
            _ => return Err(corrupt("bad trailing-newline flag")),
        };
        let max_depth = r.u32()?;
        let total_line_bytes = r.u64()?;
        let min_line_bytes = r.u64()?;
        let max_line_bytes = r.u64()?;
        let count = r.u32()?;
        if count > MAX_MODEL_RECORDS {
            return Err(corrupt("model record count is implausible"));
        }
        let mut records = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let line_start = r.u64()?;
            let line_end = r.u64()?;
            let terminator = r.u8()?;
            if terminator > T_CRLF {
                return Err(corrupt("unknown line terminator tag"));
            }
            let line_number = r.u32()?;
            if line_start > line_end || line_end > doc_len {
                return Err(corrupt("record line span is outside the document"));
            }
            let mlen = r.u32()? as usize;
            let mbytes = r.bytes(mlen)?;
            let model = JsonModel::decode(mbytes)?;
            if model.doc_len != doc_len {
                return Err(corrupt("record model doc_len does not match the document"));
            }
            let root = model
                .node(model.root)
                .ok_or_else(|| corrupt("record model root is out of range"))?;
            if root.start < line_start || root.end > line_end {
                return Err(corrupt("record value span is outside its line"));
            }
            records.push(JsonlRecord {
                line_start,
                line_end,
                terminator,
                line_number,
                model,
            });
        }
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        Ok(JsonlModel {
            doc_len,
            records,
            blank_lines,
            crlf_records,
            trailing_newline,
            max_depth,
            total_line_bytes,
            min_line_bytes,
            max_line_bytes,
        })
    }
}

/// A resolved JSONL pointer query: the record index, the node index inside that
/// record's parse, and how many members matched the final key segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolved {
    /// The 0-based record index.
    pub record: u32,
    /// The resolved node index inside the record's parse.
    pub index: u32,
    /// The number of members matched by the final key segment (`1` for an array
    /// index or the record root; `>1` witnesses duplicate keys).
    pub matches: u32,
}

/// One lexical-match record from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonlMatch {
    /// The 0-based record index the match lies in.
    pub record: u32,
    /// The canonical RFC 6901 pointer **within** the record (e.g. `/c`).
    pub pointer: String,
    /// Whether the match is an object key or a string value.
    pub role: MatchRole,
    /// The exact source span of the matching string token.
    pub start: u64,
    /// One past the matching string token.
    pub end: u64,
    /// The decoded text of the matching string.
    pub text: String,
}

/// One physical line produced by [`LineIter`].
struct Line {
    /// First byte of the line.
    start: usize,
    /// One past the line's content (the terminator excluded).
    content_end: usize,
    /// One past the line's terminator (or the source end at EOF).
    end: usize,
    /// The terminator tag.
    term: u8,
}

/// A bounded physical-line splitter. A line's terminator is `\n`, an optional
/// preceding `\r` (CRLF), or nothing at the final line (EOF).
struct LineIter<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> LineIter<'a> {
    fn new(b: &'a [u8]) -> Self {
        LineIter { b, at: 0 }
    }

    fn next_line(&mut self) -> Option<Line> {
        if self.at >= self.b.len() {
            return None;
        }
        let start = self.at;
        let mut i = self.at;
        while i < self.b.len() && self.b[i] != b'\n' {
            i += 1;
        }
        let (content_end, end, term) = if i < self.b.len() {
            let end = i + 1;
            if i > start && self.b[i - 1] == b'\r' {
                (i - 1, end, T_CRLF)
            } else {
                (i, end, T_LF)
            }
        } else {
            (self.b.len(), self.b.len(), T_NONE)
        };
        self.at = end;
        Some(Line {
            start,
            content_end,
            end,
            term,
        })
    }
}

/// Whether a line's content is blank (empty or only ASCII whitespace).
fn is_blank(content: &[u8]) -> bool {
    content.iter().all(|b| b.is_ascii_whitespace())
}

/// The per-line limits handed to the shared JSON parser: a line is bounded by
/// [`Limits::max_jsonl_line_bytes`] as well as the JSON caps, and by whatever node
/// budget remains.
fn line_limits(limits: Limits, nodes_budget: u32, line_bytes: u64) -> Limits {
    Limits {
        max_json_document_bytes: limits.max_json_document_bytes.min(line_bytes),
        max_json_nodes: limits.max_json_nodes.min(nodes_budget),
        max_json_string_bytes: limits.max_json_string_bytes.min(line_bytes),
        ..limits
    }
}

/// Fast-fail JSONL detector: does `source` carry **at least two** non-blank lines
/// and does every non-blank line parse as exactly one JSON value under `limits`?
///
/// Conservative by construction. A whole-source single JSON value, a malformed
/// line, and a non-newline-separated bag of values are all rejected. Node-building
/// is disabled, so detection is O(1) in extra memory.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_jsonl_document_bytes {
        return false;
    }
    // A single JSON value is JSON, never JSONL. The format detector already tries
    // JSON first, but the per-line rule guarantees it independently.
    let mut it = LineIter::new(source);
    let mut records: u32 = 0;
    while let Some(line) = it.next_line() {
        let content = &source[line.start..line.content_end];
        if is_blank(content) {
            continue;
        }
        if content.len() as u64 > limits.max_jsonl_line_bytes {
            return false;
        }
        records = records.saturating_add(1);
        if records > limits.max_jsonl_records {
            return false;
        }
        let ll = line_limits(limits, limits.max_jsonl_nodes, content.len() as u64);
        if json::parse(content, ll, false).is_err() {
            return false;
        }
    }
    records >= 2
}

/// Parse `source` into a [`JsonlModel`]. Each non-blank line's JSON text is parsed
/// by the shared JSON parser; its node spans are lifted into whole-source
/// coordinates. A source that is not JSONL (fewer than two records, a malformed
/// line, an over-cap line, or a node/depth/size budget) is a typed decline.
pub fn parse(source: &[u8], limits: Limits) -> Result<JsonlModel> {
    if source.len() as u64 > limits.max_jsonl_document_bytes {
        return Err(Error::resource_limit(format!(
            "JSONL source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_jsonl_document_bytes
        )));
    }
    let mut records: Vec<JsonlRecord> = Vec::new();
    let mut blank_lines: u32 = 0;
    let mut crlf_records: u32 = 0;
    let mut max_depth: u32 = 0;
    let mut nodes_used: u64 = 0;
    let mut total_line_bytes: u64 = 0;
    let mut min_line_bytes = u64::MAX;
    let mut max_line_bytes: u64 = 0;
    let mut line_number: u32 = 0;
    let mut trailing_newline = false;
    let mut it = LineIter::new(source);
    while let Some(line) = it.next_line() {
        trailing_newline = line.term != T_NONE;
        let content = &source[line.start..line.content_end];
        if is_blank(content) {
            blank_lines = blank_lines.saturating_add(1);
            line_number = line_number.saturating_add(1);
            continue;
        }
        let clen = content.len() as u64;
        if clen > limits.max_jsonl_line_bytes {
            return Err(Error::resource_limit(format!(
                "JSONL line {} is {clen} bytes, above the {}-byte line cap",
                line_number, limits.max_jsonl_line_bytes
            )));
        }
        if records.len() as u64 >= limits.max_jsonl_records as u64 {
            return Err(Error::resource_limit(format!(
                "JSONL document exceeds the {}-record cap",
                limits.max_jsonl_records
            )));
        }
        let remaining = (limits.max_jsonl_nodes as u64).saturating_sub(nodes_used);
        if remaining == 0 {
            return Err(Error::resource_limit(format!(
                "JSONL document exceeds the {}-node cap",
                limits.max_jsonl_nodes
            )));
        }
        let ll = line_limits(limits, remaining as u32, clen);
        let mut model = json::parse(content, ll, true)?;
        // Lift the record's node spans into whole-source coordinates so the shared
        // JSON helpers (`token_bytes`, `resolve_pointer`, `find`, `canonical_text`)
        // work unchanged against the whole source.
        let base = line.start as u64;
        for n in &mut model.nodes {
            n.start += base;
            n.end += base;
        }
        model.doc_len = source.len() as u64;
        nodes_used = nodes_used.saturating_add(model.nodes.len() as u64);
        if nodes_used > limits.max_jsonl_nodes as u64 {
            return Err(Error::resource_limit(format!(
                "JSONL document exceeds the {}-node cap",
                limits.max_jsonl_nodes
            )));
        }
        max_depth = max_depth.max(model.max_depth);
        if min_line_bytes == u64::MAX || clen < min_line_bytes {
            min_line_bytes = clen;
        }
        max_line_bytes = max_line_bytes.max(clen);
        total_line_bytes = total_line_bytes.saturating_add(clen);
        if line.term == T_CRLF {
            crlf_records = crlf_records.saturating_add(1);
        }
        records.push(JsonlRecord {
            line_start: line.start as u64,
            line_end: line.end as u64,
            terminator: line.term,
            line_number,
            model,
        });
        line_number = line_number.saturating_add(1);
    }
    if records.len() < 2 {
        return Err(Error::invalid_jsonl_structure(
            "not JSONL: fewer than two non-blank record lines (a single JSON value is JSON, \
             not JSONL)",
        ));
    }
    Ok(JsonlModel {
        doc_len: source.len() as u64,
        records,
        blank_lines,
        crlf_records,
        trailing_newline,
        max_depth,
        total_line_bytes,
        min_line_bytes: if min_line_bytes == u64::MAX {
            0
        } else {
            min_line_bytes
        },
        max_line_bytes,
    })
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of a `JsonlModel` node).
pub fn build_jsonl_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits)?.encode())
}

/// One line's exact source bytes (`[line_start, line_end)`; the terminator
/// included).
pub fn line_bytes<'a>(source: &'a [u8], record: &JsonlRecord) -> Result<&'a [u8]> {
    let s = usize::try_from(record.line_start).map_err(|_| corrupt("span overflow"))?;
    let e = usize::try_from(record.line_end).map_err(|_| corrupt("span overflow"))?;
    source
        .get(s..e)
        .ok_or_else(|| corrupt("record line span is outside the source"))
}

/// A record's value's exact source bytes (the JSON token, no whitespace).
pub fn value_bytes<'a>(source: &'a [u8], record: &JsonlRecord) -> Result<&'a [u8]> {
    let node = record.value_node()?;
    json::token_bytes(source, node)
}

/// Parse a `N:POINTER` record reference. `N` is a 0-based record index; the
/// remainder is an RFC 6901 pointer into record `N` (`N` alone, or `N:`, addresses
/// the whole record value). A missing or non-numeric `N` is a usage error.
pub fn parse_record_ref(spec: &str) -> Result<(u32, &str)> {
    let (n, pointer) = match spec.split_once(':') {
        Some((n, p)) => (n, p),
        None => (spec, ""),
    };
    if n.is_empty() || (n.len() > 1 && n.starts_with('0')) {
        return Err(Error::usage(format!(
            "JSONL record reference {spec:?} must be N or N:POINTER with a canonical index"
        )));
    }
    let index: u32 = n.parse().map_err(|_| {
        Error::usage(format!(
            "JSONL record reference {spec:?} has a non-numeric record index"
        ))
    })?;
    Ok((index, pointer))
}

/// Resolve a `N:POINTER` reference: the record index, the node inside its parse, and
/// the duplicate-key match count. An out-of-range record or pointer is a typed
/// decline, never a silent empty answer.
pub fn resolve_record_pointer(model: &JsonlModel, source: &[u8], spec: &str) -> Result<Resolved> {
    let (index, pointer) = parse_record_ref(spec)?;
    let record = model.record(index).ok_or_else(|| {
        Error::unsupported_feature(format!(
            "JSONL record {index} is out of range (record count {})",
            model.records.len()
        ))
    })?;
    let r = json::resolve_pointer(&record.model, source, pointer)?;
    Ok(Resolved {
        record: index,
        index: r.index,
        matches: r.matches,
    })
}

/// Render a deterministic canonical text projection: each record's value rendered
/// by the JSON adapter's canonical renderer (member order and token spelling kept,
/// separators canonicalized), one record per line.
pub fn canonical_text(model: &JsonlModel, source: &[u8]) -> Result<String> {
    let mut out = String::new();
    for (i, record) in model.records.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&json::canonical_text(&record.model, source)?);
    }
    Ok(out)
}

/// A bounded, case-sensitive lexical search across **every** record's object keys
/// and string values. Matches are returned in record order (records before
/// descendants), each with its record index, its within-record canonical pointer,
/// and its exact source span.
pub fn find(
    model: &JsonlModel,
    source: &[u8],
    pattern: &str,
    limits: Limits,
) -> Result<Vec<JsonlMatch>> {
    let mut out: Vec<JsonlMatch> = Vec::new();
    for (i, record) in model.records.iter().enumerate() {
        let matches: Vec<JsonMatch> = json::find(&record.model, source, pattern, limits)?;
        for m in matches {
            out.push(JsonlMatch {
                record: i as u32,
                pointer: m.pointer,
                role: m.role,
                start: m.start,
                end: m.end,
                text: m.text,
            });
        }
    }
    Ok(out)
}

fn corrupt(msg: &str) -> Error {
    Error::invalid_jsonl_structure(format!("malformed JSONL model: {msg}"))
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
    use crate::error::ErrorClass;

    const DOC: &[u8] = b"{\"b\":1e3,\"a\":1.0,\"a\":-0}\n{\"c\":\"\\u00e9\",\"n\":null}\n";

    #[test]
    fn detects_jsonl_and_rejects_a_single_value() {
        assert!(detect(DOC, Limits::DEFAULT));
        // One JSON value spread across lines is JSON, not JSONL.
        assert!(!detect(b"[\n1,\n2\n]", Limits::DEFAULT));
        assert!(!detect(b"42\n", Limits::DEFAULT));
        assert!(!detect(b"42", Limits::DEFAULT));
        // Blank lines do not count as records.
        assert!(!detect(b"1\n\n\n", Limits::DEFAULT));
        assert!(detect(b"1\n\n2\n", Limits::DEFAULT));
        // A malformed line or a non-newline-separated bag is not JSONL.
        assert!(!detect(b"1\n[1, 2, \n", Limits::DEFAULT));
        assert!(!detect(b"{\"a\":1}{\"b\":2}\n", Limits::DEFAULT));
        assert!(!detect(b"1,2,3\n", Limits::DEFAULT));
    }

    #[test]
    fn preserves_per_line_spans_and_terminators() {
        let m = parse(DOC, Limits::DEFAULT).unwrap();
        assert_eq!(m.records.len(), 2);
        assert_eq!(m.crlf_records, 0);
        assert!(m.trailing_newline);
        let first = &m.records[0];
        let (vs, ve) = first.value_span().unwrap();
        assert_eq!(
            &DOC[vs as usize..ve as usize],
            b"{\"b\":1e3,\"a\":1.0,\"a\":-0}"
        );
        assert_eq!(
            line_bytes(DOC, first).unwrap(),
            &DOC[0..first.line_end as usize]
        );
        // Duplicate keys are preserved inside a line, and the first wins resolution.
        let r = resolve_record_pointer(&m, DOC, "0:/a").unwrap();
        let a = r.index;
        assert_eq!(r.matches, 2);
        let node = m.records[0].model.node(a).unwrap();
        assert_eq!(&DOC[node.start as usize..node.end as usize], b"1.0");
    }

    #[test]
    fn crlf_and_trailing_newline_are_reported() {
        let m = parse(b"1\r\n2\r\n", Limits::DEFAULT).unwrap();
        assert_eq!(m.crlf_records, 2);
        assert!(m.trailing_newline);
        assert_eq!(m.records[0].terminator, T_CRLF);
        assert_eq!(m.records[0].content_end(), 1);
        let m = parse(b"1\n2", Limits::DEFAULT).unwrap();
        assert_eq!(m.crlf_records, 0);
        assert!(!m.trailing_newline);
        assert_eq!(m.records[1].terminator, T_NONE);
    }

    #[test]
    fn find_and_canonical_text_span_records() {
        let m = parse(DOC, Limits::DEFAULT).unwrap();
        let found = find(&m, DOC, "c", Limits::DEFAULT).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].record, 1);
        assert_eq!(found[0].pointer, "/c");
        assert_eq!(found[0].role, MatchRole::Key);
        let text = canonical_text(&m, DOC).unwrap();
        assert_eq!(
            text,
            "{\"b\":1e3,\"a\":1.0,\"a\":-0}\n{\"c\":\"\\u00e9\",\"n\":null}"
        );
    }

    #[test]
    fn declines_typed_on_bad_inputs() {
        assert_eq!(
            parse(b"1\n", Limits::DEFAULT).unwrap_err().class(),
            ErrorClass::InvalidJsonlStructure
        );
        assert_eq!(
            parse(b"1\n{oops}\n", Limits::DEFAULT).unwrap_err().class(),
            ErrorClass::InvalidJsonStructure
        );
        let tight = Limits {
            max_jsonl_line_bytes: 2,
            ..Limits::DEFAULT
        };
        assert_eq!(
            parse(b"[1,2,3]\n[4,5,6]\n", tight).unwrap_err().class(),
            ErrorClass::ResourceLimit
        );
        assert!(!detect(b"[1,2,3]\n[4,5,6]\n", tight));
    }

    #[test]
    fn model_roundtrips() {
        let m = parse(DOC, Limits::DEFAULT).unwrap();
        let bytes = m.encode();
        assert_eq!(JsonlModel::decode(&bytes).unwrap(), m);
        for cut in 0..bytes.len() {
            let _ = JsonlModel::decode(&bytes[..cut]);
        }
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0x1234_5678_9ABC_DEF0;
        for _ in 0..128 {
            let mut buf = vec![0u8; 512];
            for b in buf.iter_mut() {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                *b = (x & 0xFF) as u8;
            }
            let _ = detect(&buf, Limits::DEFAULT);
            if let Ok(m) = parse(&buf, Limits::STRICT) {
                let _ = canonical_text(&m, &buf);
                let _ = m.encode();
                let _ = find(&m, &buf, "a", Limits::STRICT);
            }
            let _ = build_jsonl_model(&buf, Limits::STRICT);
        }
    }
}
