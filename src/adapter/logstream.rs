//! Bounded, representation-preserving syslog / log-stream adapter (Phase 21.28).
//!
//! A **log stream** is a line/event stream: one event per physical line,
//! newline-separated. Like the other Wave-2 formats it is *not* a package — there
//! is no OPC/ZIP layer, so the exact leaf is the **whole source** (a
//! `DocumentExact`, a RAW-like authority) and everything this module produces is a
//! bounded, deterministic (`Q_gen`) projection that never sits on the exactness
//! path.
//!
//! ## Dialects (recorded, like CSV/config)
//!
//! One adapter covers three dialects and records the dialect per record:
//!
//! * **RFC 5424 syslog** (`DIALECT_RFC5424`):
//!   `<PRI>VERSION SP TIMESTAMP SP HOSTNAME SP APP-NAME SP PROCID SP MSGID SP
//!   STRUCTURED-DATA [SP MSG]`. The `<PRI>` (facility × 8 + severity), the
//!   version, and the six NILVALUE-or-value fields are parsed as exact spans; the
//!   **structured-data** elements (`[id k="v" ...]`, with `\"`, `\\`, and `\]`
//!   escapes, possibly several concatenated) are parsed into element / id /
//!   parameter-name / parameter-value spans; the message is the verbatim
//!   remainder. NILVALUE (`-`) spelling is preserved.
//! * **RFC 3164 (BSD) syslog** (`DIALECT_RFC3164`):
//!   `<PRI>Mmm dd hh:mm:ss HOSTNAME TAG[pid]: MSG`. The timestamp spelling
//!   (single- or double-space day padding), the host, the tag, the optional pid,
//!   and the message are preserved as exact spans.
//! * **Generic application log** (`DIALECT_GENERIC`): an optional leading
//!   timestamp (`2026-10-11T…Z`, `2026-10-11 12:34:56`, or `[2026-10-11 …]`)
//!   and/or a **level** token (`DEBUG`/`INFO`/`WARN`/`WARNING`/`ERROR`/`FATAL`/
//!   `TRACE`/`NOTICE`/`CRIT`/`CRITICAL`, or a bracketed `[INFO]`) followed by the
//!   message. The level spelling and the raw remainder are preserved verbatim.
//!
//! ## Representation preservation (never normalized)
//!
//! Every record keeps its **exact line span** and terminator (LF/CRLF/none), its
//! physical line number, and — per dialect — exact field spans. A message is
//! **never** re-encoded, re-flowed, or unescaped: the exact source bytes are the
//! authority and every field span is a `Q_gen` view of them. Blank lines, original
//! order, and a leading UTF-8 BOM are handled and reported.
//!
//! ## Detection (conservative; no magic bytes)
//!
//! [`detect`] claims a log stream only when the source carries **at least two**
//! non-blank lines and **every** non-blank line matches one of the dialects above
//! (a `<PRI>` prefix with a valid syslog header, or a leading generic
//! timestamp/level). It is byte-based and never consults a file name. Because the
//! whole-source dispatcher tries every higher-priority format first (JSON/JSON5/
//! JSONL/CBOR/Msgpack, YAML/TOML/config/CSV, and the prose family Markdown/RST/
//! AsciiDoc), a JSON/YAML/CSV/prose document is never stolen.
//!
//! ### What it cannot distinguish (recorded honestly)
//!
//! * **Prose that begins every line with a level word** (e.g. a file whose lines
//!   are all `INFO …`) is indistinguishable from an application log and **is**
//!   claimed. A single log-looking line inside otherwise-plain prose is *not*
//!   claimed, because **every** non-blank line must match.
//! * A lone leading **timestamp** is accepted (the message is the remainder), so a
//!   file whose every line begins with an ISO date is claimed as a generic log.
//! * RFC 3164 has no unambiguous field grammar beyond the timestamp and the
//!   `TAG[pid]:` marker; a line that merely begins with `<PRI>` and a BSD-looking
//!   timestamp is treated as BSD syslog even if a stricter reader would not.
//!
//! ## Bounds and no panics
//!
//! Untrusted input can only ever yield a typed decline, never a panic or an
//! unbounded allocation: the record count by [`Limits::max_logstream_records`],
//! the field-span count by [`Limits::max_logstream_fields`], the structured-data
//! element count by [`Limits::max_logstream_sd_elements`], one line by
//! [`Limits::max_logstream_line_bytes`], the deepest structured-data nesting by
//! [`Limits::max_logstream_depth`], and the source length by
//! [`Limits::max_logstream_document_bytes`].

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model records (defends the decoder against a hostile blob).
pub const MAX_MODEL_RECORDS: u32 = 1 << 24;
/// Hard cap on decoded field spans (defends the decoder against a hostile blob).
pub const MAX_MODEL_FIELDS: u64 = 1 << 26;
/// Hard cap on decoded structured-data elements (defends the decoder).
pub const MAX_MODEL_SD_ELEMENTS: u64 = 1 << 22;

/// Dialect tag: RFC 5424 syslog.
pub const DIALECT_RFC5424: u8 = 0;
/// Dialect tag: RFC 3164 (BSD) syslog.
pub const DIALECT_RFC3164: u8 = 1;
/// Dialect tag: a generic application log line.
pub const DIALECT_GENERIC: u8 = 2;

/// Line terminator tag: the last line ended at EOF (no terminator).
pub const T_NONE: u8 = 0;
/// Line terminator tag: a bare `\n`.
pub const T_LF: u8 = 1;
/// Line terminator tag: a `\r\n` (CRLF) pair.
pub const T_CRLF: u8 = 2;

/// Field role: the `<PRI>` priority token (angle brackets included).
pub const F_PRI: u8 = 0;
/// Field role: the RFC 5424 VERSION token.
pub const F_VERSION: u8 = 1;
/// Field role: a timestamp token (RFC 5424/3164 or generic).
pub const F_TIMESTAMP: u8 = 2;
/// Field role: the HOSTNAME token.
pub const F_HOSTNAME: u8 = 3;
/// Field role: the RFC 5424 APP-NAME token.
pub const F_APP_NAME: u8 = 4;
/// Field role: the RFC 5424 PROCID token.
pub const F_PROCID: u8 = 5;
/// Field role: the RFC 5424 MSGID token.
pub const F_MSGID: u8 = 6;
/// Field role: the whole STRUCTURED-DATA token (`-`, or every `[…]` element).
pub const F_STRUCTURED_DATA: u8 = 7;
/// Field role: one structured-data element (`[id …]`, brackets included).
pub const F_SD_ELEMENT: u8 = 8;
/// Field role: one structured-data element's id (SD-ID).
pub const F_SD_ID: u8 = 9;
/// Field role: one structured-data parameter name.
pub const F_SD_PARAM_NAME: u8 = 10;
/// Field role: one structured-data parameter value (quotes included).
pub const F_SD_PARAM_VALUE: u8 = 11;
/// Field role: the RFC 3164 TAG token.
pub const F_TAG: u8 = 12;
/// Field role: the RFC 3164 `[pid]` body (brackets excluded).
pub const F_PID: u8 = 13;
/// Field role: a generic level token (brackets included when bracketed).
pub const F_LEVEL: u8 = 14;
/// Field role: the message (the verbatim remainder, possibly empty).
pub const F_MSG: u8 = 15;

/// Record flag: a `<PRI>` prefix was parsed.
pub const FLAG_HAS_PRI: u16 = 1 << 0;
/// Record flag: an RFC 5424 VERSION was parsed.
pub const FLAG_HAS_VERSION: u16 = 1 << 1;
/// Record flag: a timestamp was parsed.
pub const FLAG_HAS_TIMESTAMP: u16 = 1 << 2;
/// Record flag: a HOSTNAME was parsed.
pub const FLAG_HAS_HOSTNAME: u16 = 1 << 3;
/// Record flag: an RFC 5424 APP-NAME was parsed.
pub const FLAG_HAS_APP_NAME: u16 = 1 << 4;
/// Record flag: an RFC 5424 PROCID was parsed.
pub const FLAG_HAS_PROCID: u16 = 1 << 5;
/// Record flag: an RFC 5424 MSGID was parsed.
pub const FLAG_HAS_MSGID: u16 = 1 << 6;
/// Record flag: structured-data elements were parsed (not NILVALUE).
pub const FLAG_HAS_SD: u16 = 1 << 7;
/// Record flag: a message was present.
pub const FLAG_HAS_MSG: u16 = 1 << 8;
/// Record flag: an RFC 3164 TAG was parsed.
pub const FLAG_HAS_TAG: u16 = 1 << 9;
/// Record flag: an RFC 3164 `[pid]` was parsed.
pub const FLAG_HAS_PID: u16 = 1 << 10;
/// Record flag: a generic level token was parsed.
pub const FLAG_HAS_LEVEL: u16 = 1 << 11;
/// Record flag: the structured-data was the NILVALUE `-`.
pub const FLAG_NIL_SD: u16 = 1 << 12;
/// Record flag: the generic level token was bracketed (`[…]`).
pub const FLAG_BRACKETED_LEVEL: u16 = 1 << 13;

/// The set of admissible record flags.
const FLAG_MASK: u16 = FLAG_HAS_PRI
    | FLAG_HAS_VERSION
    | FLAG_HAS_TIMESTAMP
    | FLAG_HAS_HOSTNAME
    | FLAG_HAS_APP_NAME
    | FLAG_HAS_PROCID
    | FLAG_HAS_MSGID
    | FLAG_HAS_SD
    | FLAG_HAS_MSG
    | FLAG_HAS_TAG
    | FLAG_HAS_PID
    | FLAG_HAS_LEVEL
    | FLAG_NIL_SD
    | FLAG_BRACKETED_LEVEL;

/// Stable lower-case name of a dialect tag.
pub const fn dialect_name(d: u8) -> &'static str {
    match d {
        DIALECT_RFC3164 => "rfc3164",
        DIALECT_GENERIC => "generic",
        _ => "rfc5424",
    }
}

/// Stable lower-case name of a field role.
pub const fn role_name(r: u8) -> &'static str {
    match r {
        F_PRI => "pri",
        F_VERSION => "version",
        F_TIMESTAMP => "timestamp",
        F_HOSTNAME => "hostname",
        F_APP_NAME => "app-name",
        F_PROCID => "procid",
        F_MSGID => "msgid",
        F_STRUCTURED_DATA => "structured-data",
        F_SD_ELEMENT => "sd-element",
        F_SD_ID => "sd-id",
        F_SD_PARAM_NAME => "sd-param-name",
        F_SD_PARAM_VALUE => "sd-param-value",
        F_TAG => "tag",
        F_PID => "pid",
        F_LEVEL => "level",
        _ => "msg",
    }
}

/// The role tag for a stable lower-case role name, if any.
pub fn role_for_name(name: &str) -> Option<u8> {
    Some(match name {
        "pri" => F_PRI,
        "version" => F_VERSION,
        "timestamp" => F_TIMESTAMP,
        "hostname" => F_HOSTNAME,
        "app-name" => F_APP_NAME,
        "procid" => F_PROCID,
        "msgid" => F_MSGID,
        "structured-data" => F_STRUCTURED_DATA,
        "sd-element" => F_SD_ELEMENT,
        "sd-id" => F_SD_ID,
        "sd-param-name" => F_SD_PARAM_NAME,
        "sd-param-value" => F_SD_PARAM_VALUE,
        "tag" => F_TAG,
        "pid" => F_PID,
        "level" => F_LEVEL,
        "msg" => F_MSG,
        _ => return None,
    })
}

/// Stable lower-case name of a line terminator tag.
pub const fn terminator_name(t: u8) -> &'static str {
    match t {
        T_LF => "lf",
        T_CRLF => "crlf",
        _ => "none",
    }
}

/// One exact-span field of a log record (offsets are absolute in the source).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogField {
    /// The field role (`F_*`).
    pub role: u8,
    /// The first source byte of the field.
    pub start: u64,
    /// One past the field.
    pub end: u64,
}

/// One log record: its **exact source line span**, its terminator, its physical
/// line number, the recorded dialect, the parsed flags/priority/depth, and its
/// exact-span fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRecord {
    /// The first source byte of the physical line.
    pub line_start: u64,
    /// One past the line's terminator (or past its last byte at EOF).
    pub line_end: u64,
    /// The line's terminator tag (`T_NONE`/`T_LF`/`T_CRLF`).
    pub terminator: u8,
    /// The 0-based physical line number (blank lines are counted).
    pub line_number: u32,
    /// The recorded dialect (`DIALECT_*`).
    pub dialect: u8,
    /// The record flags (`FLAG_*`).
    pub flags: u16,
    /// The decoded priority (`facility * 8 + severity`), or `0` when absent.
    pub pri: u16,
    /// The deepest structured-data nesting (record = 1; an element = 2; a
    /// parameter = 3).
    pub depth: u32,
    /// The record's exact-span fields, in parse order.
    pub fields: Vec<LogField>,
}

impl LogRecord {
    /// The first field with `role`, if any.
    pub fn field(&self, role: u8) -> Option<&LogField> {
        self.fields.iter().find(|f| f.role == role)
    }

    /// Every field with `role`, in parse order.
    pub fn fields_with(&self, role: u8) -> impl Iterator<Item = &LogField> {
        self.fields.iter().filter(move |f| f.role == role)
    }

    /// The facility (`pri >> 3`) of a syslog record, or `0` for a generic record.
    pub fn facility(&self) -> u8 {
        (self.pri >> 3) as u8
    }

    /// The severity (`pri & 7`) of a syslog record, or `0` for a generic record.
    pub fn severity(&self) -> u8 {
        (self.pri & 7) as u8
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

/// The canonical derived log-stream model (the materialization of a
/// `LogstreamModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogstreamModel {
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// The length in bytes of a leading UTF-8 BOM (`0` or `3`).
    pub bom_len: u32,
    /// The non-blank records, in source order.
    pub records: Vec<LogRecord>,
    /// The number of physical lines that were blank (whitespace-only).
    pub blank_lines: u32,
    /// The number of records terminated by a CRLF pair.
    pub crlf_records: u32,
    /// Whether the source ended with a line terminator.
    pub trailing_newline: bool,
}

impl LogstreamModel {
    /// The record at `index`, if present.
    pub fn record(&self, index: u32) -> Option<&LogRecord> {
        self.records.get(index as usize)
    }

    /// The total number of field spans across every record.
    pub fn field_count(&self) -> u64 {
        self.records.iter().map(|r| r.fields.len() as u64).sum()
    }

    /// The total number of structured-data elements across every record.
    pub fn sd_element_count(&self) -> u64 {
        self.records
            .iter()
            .map(|r| r.fields_with(F_SD_ELEMENT).count() as u64)
            .sum()
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let fields: usize = self.records.iter().map(|r| r.fields.len()).sum();
        let mut out = Vec::with_capacity(40 + self.records.len() * 44 + fields * 17);
        out.extend_from_slice(b"LOGS");
        out.push(MODEL_VERSION);
        out.extend_from_slice(&self.bom_len.to_le_bytes());
        out.push(u8::from(self.trailing_newline));
        out.push(0); // reserved
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&self.blank_lines.to_le_bytes());
        out.extend_from_slice(&self.crlf_records.to_le_bytes());
        out.extend_from_slice(&(self.records.len() as u32).to_le_bytes());
        for r in &self.records {
            out.extend_from_slice(&r.line_start.to_le_bytes());
            out.extend_from_slice(&r.line_end.to_le_bytes());
            out.push(r.terminator);
            out.push(r.dialect);
            out.extend_from_slice(&r.line_number.to_le_bytes());
            out.extend_from_slice(&r.flags.to_le_bytes());
            out.extend_from_slice(&r.pri.to_le_bytes());
            out.extend_from_slice(&r.depth.to_le_bytes());
            out.extend_from_slice(&(r.fields.len() as u32).to_le_bytes());
            for f in &r.fields {
                out.push(f.role);
                out.extend_from_slice(&f.start.to_le_bytes());
                out.extend_from_slice(&f.end.to_le_bytes());
            }
        }
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<LogstreamModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"LOGS" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let bom_len = r.u32()?;
        let trailing_newline = match r.u8()? {
            0 => false,
            1 => true,
            _ => return Err(corrupt("bad trailing-newline flag")),
        };
        let _reserved = r.u8()?;
        let doc_len = r.u64()?;
        if bom_len as u64 > doc_len {
            return Err(corrupt("BOM runs past the document"));
        }
        let blank_lines = r.u32()?;
        let crlf_records = r.u32()?;
        let count = r.u32()?;
        if count > MAX_MODEL_RECORDS {
            return Err(corrupt("model record count is implausible"));
        }
        let mut records = Vec::with_capacity(count as usize);
        let mut prev_end = bom_len as u64;
        let mut total_fields: u64 = 0;
        let mut total_sd: u64 = 0;
        for _ in 0..count {
            let line_start = r.u64()?;
            let line_end = r.u64()?;
            let terminator = r.u8()?;
            if terminator > T_CRLF {
                return Err(corrupt("unknown line terminator tag"));
            }
            let dialect = r.u8()?;
            if dialect > DIALECT_GENERIC {
                return Err(corrupt("unknown dialect"));
            }
            let line_number = r.u32()?;
            let flags = r.u16()?;
            if flags & !FLAG_MASK != 0 {
                return Err(corrupt("unknown record flags"));
            }
            let pri = r.u16()?;
            if pri > 191 {
                return Err(corrupt("priority is out of range"));
            }
            let depth = r.u32()?;
            let field_count = r.u32()?;
            if line_start < prev_end || line_end < line_start || line_end > doc_len {
                return Err(corrupt("record line span is outside the document"));
            }
            let content_end = match terminator {
                T_LF => line_end - 1,
                T_CRLF => line_end - 2,
                _ => line_end,
            };
            if content_end < line_start {
                return Err(corrupt("record content span is inverted"));
            }
            if field_count as u64 > MAX_MODEL_FIELDS {
                return Err(corrupt("record field count is implausible"));
            }
            let mut fields = Vec::with_capacity((field_count as usize).min(1024));
            for _ in 0..field_count {
                let role = r.u8()?;
                if role > F_MSG {
                    return Err(corrupt("unknown field role"));
                }
                let start = r.u64()?;
                let end = r.u64()?;
                if start > end || start < line_start || end > content_end {
                    return Err(corrupt("field span is outside its record"));
                }
                if role == F_SD_ELEMENT {
                    total_sd += 1;
                    if total_sd > MAX_MODEL_SD_ELEMENTS {
                        return Err(corrupt("model SD element count is implausible"));
                    }
                }
                fields.push(LogField { role, start, end });
            }
            total_fields += fields.len() as u64;
            if total_fields > MAX_MODEL_FIELDS {
                return Err(corrupt("model field count is implausible"));
            }
            prev_end = line_end;
            records.push(LogRecord {
                line_start,
                line_end,
                terminator,
                line_number,
                dialect,
                flags,
                pri,
                depth,
                fields,
            });
        }
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        Ok(LogstreamModel {
            doc_len,
            bom_len,
            records,
            blank_lines,
            crlf_records,
            trailing_newline,
        })
    }
}

/// One lexical match from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogMatch {
    /// The 0-based record index the match lies in.
    pub record: u32,
    /// The role tag of the matching field.
    pub role: u8,
    /// The exact source span of the matching bytes.
    pub start: u64,
    /// One past the matching bytes.
    pub end: u64,
    /// The matched field's bytes, decoded lossily for reporting.
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

/// The length in bytes of a leading UTF-8 BOM (`0` or `3`).
fn bom_len(source: &[u8]) -> u32 {
    if source.starts_with(&[0xEF, 0xBB, 0xBF]) {
        3
    } else {
        0
    }
}

// ---------------------------------------------------------------------------
// Line classification
// ---------------------------------------------------------------------------

/// The result of classifying one line: offsets are relative to the line content.
struct LineParse {
    dialect: u8,
    flags: u16,
    pri: u16,
    depth: u32,
    fields: Vec<LogField>,
}

fn field(role: u8, start: usize, end: usize) -> LogField {
    LogField {
        role,
        start: start as u64,
        end: end as u64,
    }
}

/// Classify one non-blank line into a dialect, or decline (`None`).
fn classify_line(c: &[u8]) -> Option<LineParse> {
    if c.first() == Some(&b'<') {
        // Both syslog dialects require a `<PRI>` prefix; the RFC 5424 grammar is
        // the more specific, so it is tried first.
        if let Some(p) = try_rfc5424(c) {
            return Some(p);
        }
        return try_rfc3164(c);
    }
    try_generic(c)
}

/// Parse `<` 1*3DIGIT `>` with a value in `0..=191`. Returns the value and the
/// index one past the `>`.
fn parse_pri(c: &[u8]) -> Option<(u16, usize)> {
    if c.first() != Some(&b'<') {
        return None;
    }
    let mut i = 1usize;
    let mut val: u32 = 0;
    let mut digits = 0usize;
    while i < c.len() && c[i].is_ascii_digit() && digits < 3 {
        val = val * 10 + (c[i] - b'0') as u32;
        i += 1;
        digits += 1;
    }
    if digits == 0 || i >= c.len() || c[i] != b'>' || val > 191 {
        return None;
    }
    Some((val as u16, i + 1))
}

/// Whether `tok` is a valid RFC 5424 header field token: 1..=255 PRINTUSASCII
/// bytes (`0x21..=0x7E`).
fn is_header_token(tok: &[u8]) -> bool {
    !tok.is_empty() && tok.len() <= 255 && tok.iter().all(|b| (0x21..=0x7E).contains(b))
}

/// Whether `tok` is a valid RFC 5424 SD-NAME: 1..=32 PRINTUSASCII bytes other
/// than `=`, ` `, `]`, and `"`.
fn is_sd_name(tok: &[u8]) -> bool {
    !tok.is_empty()
        && tok.len() <= 32
        && tok
            .iter()
            .all(|b| (0x21..=0x7E).contains(b) && *b != b'=' && *b != b']' && *b != b'"')
}

/// Parse an RFC 5424 line. Declines (`None`) on any grammar violation.
fn try_rfc5424(c: &[u8]) -> Option<LineParse> {
    let mut fields: Vec<LogField> = Vec::new();
    let mut flags = 0u16;
    let (pri, mut pos) = parse_pri(c)?;
    fields.push(field(F_PRI, 0, pos));
    flags |= FLAG_HAS_PRI;

    // VERSION = NONZERO-DIGIT 0*2DIGIT
    let vstart = pos;
    let mut i = pos;
    while i < c.len() && c[i].is_ascii_digit() && i - vstart < 3 {
        i += 1;
    }
    if i == vstart || c[vstart] == b'0' || i >= c.len() || c[i] != b' ' {
        return None;
    }
    fields.push(field(F_VERSION, vstart, i));
    flags |= FLAG_HAS_VERSION;
    pos = i + 1;

    // Five NILVALUE-or-value header fields, each terminated by a single SP.
    let header: [(u8, u16); 5] = [
        (F_TIMESTAMP, FLAG_HAS_TIMESTAMP),
        (F_HOSTNAME, FLAG_HAS_HOSTNAME),
        (F_APP_NAME, FLAG_HAS_APP_NAME),
        (F_PROCID, FLAG_HAS_PROCID),
        (F_MSGID, FLAG_HAS_MSGID),
    ];
    for (role, flag) in header {
        let start = pos;
        while pos < c.len() && c[pos] != b' ' {
            pos += 1;
        }
        if pos == start || !is_header_token(&c[start..pos]) {
            return None;
        }
        fields.push(field(role, start, pos));
        flags |= flag;
        if pos >= c.len() {
            return None; // a space is required before the next element
        }
        pos += 1; // skip the single space
    }

    // STRUCTURED-DATA = NILVALUE / 1*SD-ELEMENT
    let sd_start = pos;
    if pos < c.len() && c[pos] == b'-' {
        let e = pos + 1;
        if e != c.len() && c[e] != b' ' {
            return None;
        }
        fields.push(field(F_STRUCTURED_DATA, sd_start, e));
        flags |= FLAG_NIL_SD;
        pos = e;
    } else if pos < c.len() && c[pos] == b'[' {
        let mut any = false;
        let mut has_param = false;
        while pos < c.len() && c[pos] == b'[' {
            let el_start = pos;
            pos += 1;
            let id_start = pos;
            while pos < c.len() && c[pos] != b' ' && c[pos] != b']' {
                pos += 1;
            }
            if pos == id_start || !is_sd_name(&c[id_start..pos]) {
                return None;
            }
            fields.push(field(F_SD_ID, id_start, pos));
            while pos < c.len() && c[pos] == b' ' {
                pos += 1;
                let pn_start = pos;
                while pos < c.len() && c[pos] != b'=' {
                    pos += 1;
                }
                if pos == pn_start || !is_sd_name(&c[pn_start..pos]) {
                    return None;
                }
                fields.push(field(F_SD_PARAM_NAME, pn_start, pos));
                pos += 1; // skip '='
                if pos >= c.len() || c[pos] != b'"' {
                    return None;
                }
                let pv_start = pos; // include the opening quote
                pos += 1;
                loop {
                    if pos >= c.len() {
                        return None;
                    }
                    match c[pos] {
                        b'\\' => {
                            pos += 2;
                            if pos > c.len() {
                                return None;
                            }
                        }
                        b'"' => break,
                        b']' => return None,
                        _ => pos += 1,
                    }
                }
                // Include the closing quote in the parameter-value span.
                fields.push(field(F_SD_PARAM_VALUE, pv_start, pos + 1));
                pos += 1;
                has_param = true;
            }
            if pos >= c.len() || c[pos] != b']' {
                return None;
            }
            pos += 1;
            fields.push(field(F_SD_ELEMENT, el_start, pos));
            any = true;
        }
        if !any {
            return None;
        }
        fields.push(field(F_STRUCTURED_DATA, sd_start, pos));
        flags |= FLAG_HAS_SD;
        let mut depth = 2u32; // record (1) + one structured-data element (2)
        if has_param {
            depth += 1; // a parameter (3)
        }
        return finish_record(fields, flags, pri, depth, pos, c);
    } else {
        return None;
    }
    // NILVALUE structured-data path.
    finish_record(fields, flags, pri, 1, pos, c)
}

/// Finish a record after the structured-data: append the optional ` MSG` and
/// apply the caps-free structural checks.
fn finish_record(
    mut fields: Vec<LogField>,
    mut flags: u16,
    pri: u16,
    depth: u32,
    pos: usize,
    c: &[u8],
) -> Option<LineParse> {
    let mut pos = pos;
    if pos < c.len() {
        if c[pos] != b' ' {
            return None;
        }
        pos += 1;
        fields.push(field(F_MSG, pos, c.len()));
        flags |= FLAG_HAS_MSG;
    }
    Some(LineParse {
        dialect: DIALECT_RFC5424,
        flags,
        pri,
        depth,
        fields,
    })
}

/// Parse an RFC 3164 (BSD) line. Declines (`None`) on any grammar violation.
fn try_rfc3164(c: &[u8]) -> Option<LineParse> {
    let mut fields: Vec<LogField> = Vec::new();
    let mut flags = 0u16;
    let (pri, mut pos) = parse_pri(c)?;
    fields.push(field(F_PRI, 0, pos));
    flags |= FLAG_HAS_PRI;

    let ts_start = pos;
    let ts_end = parse_bsd_timestamp(c, pos)?;
    fields.push(field(F_TIMESTAMP, ts_start, ts_end));
    flags |= FLAG_HAS_TIMESTAMP;
    pos = ts_end;
    if pos >= c.len() || c[pos] != b' ' {
        return None;
    }
    pos += 1;

    let hs = pos;
    while pos < c.len() && c[pos] != b' ' {
        pos += 1;
    }
    if pos == hs {
        return None;
    }
    fields.push(field(F_HOSTNAME, hs, pos));
    flags |= FLAG_HAS_HOSTNAME;
    if pos >= c.len() {
        return None;
    }
    pos += 1;

    // TAG is everything up to `[`, `:`, or a space.
    let tag_start = pos;
    while pos < c.len() && c[pos] != b'[' && c[pos] != b':' && c[pos] != b' ' {
        pos += 1;
    }
    if pos == tag_start {
        return None;
    }
    fields.push(field(F_TAG, tag_start, pos));
    flags |= FLAG_HAS_TAG;

    if pos < c.len() && c[pos] == b'[' {
        let ps = pos + 1;
        let mut j = ps;
        while j < c.len() && c[j] != b']' {
            j += 1;
        }
        if j >= c.len() || j == ps || !c[ps..j].iter().all(|b| b.is_ascii_digit()) {
            return None;
        }
        fields.push(field(F_PID, ps, j));
        flags |= FLAG_HAS_PID;
        pos = j + 1;
    }
    if pos >= c.len() || c[pos] != b':' {
        return None;
    }
    pos += 1;
    if pos < c.len() {
        if c[pos] == b' ' {
            pos += 1;
        }
        fields.push(field(F_MSG, pos, c.len()));
        flags |= FLAG_HAS_MSG;
    }
    Some(LineParse {
        dialect: DIALECT_RFC3164,
        flags,
        pri,
        depth: 1,
        fields,
    })
}

/// Parse an RFC 3164 timestamp (`Mmm dd hh:mm:ss`, the day space-padded to two
/// columns). Returns the index one past the timestamp.
fn parse_bsd_timestamp(c: &[u8], pos: usize) -> Option<usize> {
    const MONTHS: [&[u8]; 12] = [
        b"Jan", b"Feb", b"Mar", b"Apr", b"May", b"Jun", b"Jul", b"Aug", b"Sep", b"Oct", b"Nov",
        b"Dec",
    ];
    if pos + 3 > c.len() {
        return None;
    }
    if !MONTHS.contains(&&c[pos..pos + 3]) {
        return None;
    }
    let mut i = pos + 3;
    if i >= c.len() || c[i] != b' ' {
        return None;
    }
    i += 1;
    if i + 2 > c.len() {
        return None;
    }
    let (d1, d2) = (c[i], c[i + 1]);
    if !((d1 == b' ' || d1.is_ascii_digit()) && d2.is_ascii_digit()) {
        return None;
    }
    let day = if d1 == b' ' {
        (d2 - b'0') as u32
    } else {
        ((d1 - b'0') as u32) * 10 + (d2 - b'0') as u32
    };
    if !(1..=31).contains(&day) {
        return None;
    }
    i += 2;
    if i >= c.len() || c[i] != b' ' {
        return None;
    }
    i += 1;
    if i + 8 > c.len() {
        return None;
    }
    let t = &c[i..i + 8];
    let digits_ok = t[0].is_ascii_digit()
        && t[1].is_ascii_digit()
        && t[2] == b':'
        && t[3].is_ascii_digit()
        && t[4].is_ascii_digit()
        && t[5] == b':'
        && t[6].is_ascii_digit()
        && t[7].is_ascii_digit();
    if !digits_ok {
        return None;
    }
    let hh = ((t[0] - b'0') as u32) * 10 + (t[1] - b'0') as u32;
    let mm = ((t[3] - b'0') as u32) * 10 + (t[4] - b'0') as u32;
    let ss = ((t[6] - b'0') as u32) * 10 + (t[7] - b'0') as u32;
    if hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    Some(i + 8)
}

/// Parse a generic application-log line. Declines (`None`) unless it carries a
/// leading timestamp and/or a leading level token.
fn try_generic(c: &[u8]) -> Option<LineParse> {
    let mut fields: Vec<LogField> = Vec::new();
    let mut flags = 0u16;
    let mut pos: usize;

    if let Some(tend) = parse_generic_timestamp(c, 0) {
        fields.push(field(F_TIMESTAMP, 0, tend));
        flags |= FLAG_HAS_TIMESTAMP;
        pos = tend;
        if pos < c.len() && c[pos] == b' ' {
            pos += 1;
        }
        if let Some((lend, bracketed)) = parse_level(c, pos) {
            fields.push(field(F_LEVEL, pos, lend));
            flags |= FLAG_HAS_LEVEL;
            if bracketed {
                flags |= FLAG_BRACKETED_LEVEL;
            }
            pos = lend;
            if pos < c.len() {
                if c[pos] != b' ' {
                    return None;
                }
                pos += 1;
            }
        }
    } else if let Some((lend, bracketed)) = parse_level(c, 0) {
        fields.push(field(F_LEVEL, 0, lend));
        flags |= FLAG_HAS_LEVEL;
        if bracketed {
            flags |= FLAG_BRACKETED_LEVEL;
        }
        pos = lend;
        if pos < c.len() && c[pos] == b' ' {
            pos += 1;
        }
        if let Some(tend) = parse_generic_timestamp(c, pos) {
            fields.push(field(F_TIMESTAMP, pos, tend));
            flags |= FLAG_HAS_TIMESTAMP;
            pos = tend;
            if pos < c.len() && c[pos] == b' ' {
                pos += 1;
            }
        }
    } else {
        return None;
    }

    if flags & (FLAG_HAS_TIMESTAMP | FLAG_HAS_LEVEL) == 0 {
        return None;
    }
    fields.push(field(F_MSG, pos, c.len()));
    flags |= FLAG_HAS_MSG;
    Some(LineParse {
        dialect: DIALECT_GENERIC,
        flags,
        pri: 0,
        depth: 1,
        fields,
    })
}

/// The set of recognized level words (case-sensitive, uppercase).
const LEVELS: [&[u8]; 10] = [
    b"TRACE",
    b"DEBUG",
    b"INFO",
    b"NOTICE",
    b"WARN",
    b"WARNING",
    b"ERROR",
    b"CRIT",
    b"CRITICAL",
    b"FATAL",
];

/// Parse a level token at `pos`: a bare uppercase level word followed by a space
/// or end-of-line, or a bracketed `[LEVEL]`. Returns the index one past the token
/// and whether it was bracketed.
fn parse_level(c: &[u8], pos: usize) -> Option<(usize, bool)> {
    if pos >= c.len() {
        return None;
    }
    if c[pos] == b'[' {
        let inner = pos + 1;
        let mut i = inner;
        while i < c.len() && c[i] != b']' {
            i += 1;
        }
        if i >= c.len() || !LEVELS.contains(&&c[inner..i]) {
            return None;
        }
        return Some((i + 1, true));
    }
    let mut i = pos;
    while i < c.len() && c[i].is_ascii_uppercase() {
        i += 1;
    }
    if i == pos || !LEVELS.contains(&&c[pos..i]) {
        return None;
    }
    if i < c.len() && c[i] != b' ' {
        return None;
    }
    Some((i, false))
}

/// Parse a generic leading timestamp at `pos`: `[<date-time>]` or a bare ISO
/// date with an optional time. Returns the index one past the timestamp.
fn parse_generic_timestamp(c: &[u8], pos: usize) -> Option<usize> {
    if pos >= c.len() {
        return None;
    }
    if c[pos] == b'[' {
        let inner = pos + 1;
        let mut i = inner;
        while i < c.len() && c[i] != b']' {
            i += 1;
        }
        if i >= c.len() || !looks_like_datetime(&c[inner..i]) {
            return None;
        }
        return Some(i + 1);
    }
    let d = parse_iso_date(c, pos)?;
    let mut i = d;
    if i < c.len() && (c[i] == b'T' || c[i] == b' ') {
        if let Some(tend) = parse_time(c, i + 1) {
            i = tend;
        } else if c[i] == b'T' {
            return None; // a 'T' with no valid time is not a timestamp
        }
    }
    Some(i)
}

/// Parse a leading `YYYY-MM-DD` date. Returns the index one past the date.
fn parse_iso_date(c: &[u8], pos: usize) -> Option<usize> {
    let four = |s: &[u8]| s.iter().all(|b| b.is_ascii_digit());
    if pos + 10 > c.len() {
        return None;
    }
    let d = &c[pos..pos + 10];
    if d[4] != b'-' || d[7] != b'-' || !four(&d[0..4]) || !four(&d[5..7]) || !four(&d[8..10]) {
        return None;
    }
    let month = ((d[5] - b'0') as u32) * 10 + (d[6] - b'0') as u32;
    let day = ((d[8] - b'0') as u32) * 10 + (d[9] - b'0') as u32;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(pos + 10)
}

/// Parse a `HH:MM:SS` time with an optional fraction and zone. Returns the index
/// one past the time.
fn parse_time(c: &[u8], pos: usize) -> Option<usize> {
    if pos + 8 > c.len() {
        return None;
    }
    let t = &c[pos..pos + 8];
    let digits_ok = t[0].is_ascii_digit()
        && t[1].is_ascii_digit()
        && t[2] == b':'
        && t[3].is_ascii_digit()
        && t[4].is_ascii_digit()
        && t[5] == b':'
        && t[6].is_ascii_digit()
        && t[7].is_ascii_digit();
    if !digits_ok {
        return None;
    }
    let hh = ((t[0] - b'0') as u32) * 10 + (t[1] - b'0') as u32;
    let mm = ((t[3] - b'0') as u32) * 10 + (t[4] - b'0') as u32;
    let ss = ((t[6] - b'0') as u32) * 10 + (t[7] - b'0') as u32;
    if hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    let mut i = pos + 8;
    // Optional fractional seconds.
    if i < c.len() && c[i] == b'.' {
        let fs = i + 1;
        let mut j = fs;
        while j < c.len() && c[j].is_ascii_digit() && j - fs < 9 {
            j += 1;
        }
        if j == fs {
            return None;
        }
        i = j;
    }
    // Optional zone: `Z`, or `+HH:MM`/`-HH:MM`/`+HHMM`.
    if i < c.len() {
        match c[i] {
            b'Z' => i += 1,
            b'+' | b'-'
                if i + 3 <= c.len() && c[i + 1].is_ascii_digit() && c[i + 2].is_ascii_digit() =>
            {
                i += 3;
                if i < c.len() && c[i] == b':' {
                    i += 1;
                }
                if i + 2 > c.len() || !c[i].is_ascii_digit() || !c[i + 1].is_ascii_digit() {
                    return None;
                }
                i += 2;
            }
            _ => {}
        }
    }
    Some(i)
}

/// Whether `s` is a whole `YYYY-MM-DD[T or space]HH:MM:SS…` date-time.
fn looks_like_datetime(s: &[u8]) -> bool {
    match parse_iso_date(s, 0) {
        Some(d) if d < s.len() && (s[d] == b'T' || s[d] == b' ') => {
            matches!(parse_time(s, d + 1), Some(end) if end == s.len())
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Detection and parsing
// ---------------------------------------------------------------------------

/// Fast-fail log-stream detector: does `source` carry **at least two** non-blank
/// lines and does **every** non-blank line match one of the dialects under
/// `limits`?
///
/// Conservative by construction. A single log-looking line inside otherwise-plain
/// prose is rejected (every non-blank line must match), and the whole-source
/// dispatcher tries every higher-priority format first.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_logstream_document_bytes {
        return false;
    }
    let bom = bom_len(source);
    let body = &source[bom as usize..];
    let mut it = LineIter::new(body);
    let mut records: u32 = 0;
    let mut fields: u64 = 0;
    let mut sd: u64 = 0;
    while let Some(line) = it.next_line() {
        let content = &body[line.start..line.content_end];
        if is_blank(content) {
            continue;
        }
        if content.len() as u64 > limits.max_logstream_line_bytes {
            return false;
        }
        records = records.saturating_add(1);
        if records > limits.max_logstream_records {
            return false;
        }
        let Some(p) = classify_line(content) else {
            return false;
        };
        if p.depth > limits.max_logstream_depth {
            return false;
        }
        fields = fields.saturating_add(p.fields.len() as u64);
        if fields > limits.max_logstream_fields {
            return false;
        }
        sd = sd.saturating_add(p.fields.iter().filter(|f| f.role == F_SD_ELEMENT).count() as u64);
        if sd > limits.max_logstream_sd_elements {
            return false;
        }
    }
    records >= 2
}

/// Parse `source` into a [`LogstreamModel`]. A source that is not a log stream
/// (fewer than two records, a line that matches no dialect, or an over-cap line/
/// record/field/SD-element/depth/size budget) is a typed decline.
pub fn parse(source: &[u8], limits: Limits) -> Result<LogstreamModel> {
    if source.len() as u64 > limits.max_logstream_document_bytes {
        return Err(Error::resource_limit(format!(
            "log-stream source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_logstream_document_bytes
        )));
    }
    let bom = bom_len(source);
    let body = &source[bom as usize..];
    let mut records: Vec<LogRecord> = Vec::new();
    let mut blank_lines: u32 = 0;
    let mut crlf_records: u32 = 0;
    let mut trailing_newline = false;
    let mut line_number: u32 = 0;
    let mut total_fields: u64 = 0;
    let mut total_sd: u64 = 0;
    let mut it = LineIter::new(body);
    while let Some(line) = it.next_line() {
        trailing_newline = line.term != T_NONE;
        let content = &body[line.start..line.content_end];
        if is_blank(content) {
            blank_lines = blank_lines.saturating_add(1);
            line_number = line_number.saturating_add(1);
            continue;
        }
        let clen = content.len() as u64;
        if clen > limits.max_logstream_line_bytes {
            return Err(Error::resource_limit(format!(
                "log-stream line {line_number} is {clen} bytes, above the {}-byte line cap",
                limits.max_logstream_line_bytes
            )));
        }
        if records.len() as u64 >= limits.max_logstream_records as u64 {
            return Err(Error::resource_limit(format!(
                "log-stream document exceeds the {}-record cap",
                limits.max_logstream_records
            )));
        }
        let p = classify_line(content).ok_or_else(|| {
            Error::invalid_logstream_structure(format!(
                "not a log stream: line {line_number} matches no syslog/generic dialect"
            ))
        })?;
        if p.depth > limits.max_logstream_depth {
            return Err(Error::resource_limit(format!(
                "log-stream line {line_number} nests to depth {}, above the {}-depth cap",
                p.depth, limits.max_logstream_depth
            )));
        }
        total_fields = total_fields.saturating_add(p.fields.len() as u64);
        if total_fields > limits.max_logstream_fields {
            return Err(Error::resource_limit(format!(
                "log-stream document exceeds the {}-field cap",
                limits.max_logstream_fields
            )));
        }
        total_sd = total_sd
            .saturating_add(p.fields.iter().filter(|f| f.role == F_SD_ELEMENT).count() as u64);
        if total_sd > limits.max_logstream_sd_elements {
            return Err(Error::resource_limit(format!(
                "log-stream document exceeds the {}-structured-data-element cap",
                limits.max_logstream_sd_elements
            )));
        }
        let base = (bom + line.start as u32) as u64;
        let fields = p
            .fields
            .into_iter()
            .map(|f| LogField {
                role: f.role,
                start: f.start + base,
                end: f.end + base,
            })
            .collect();
        if line.term == T_CRLF {
            crlf_records = crlf_records.saturating_add(1);
        }
        records.push(LogRecord {
            line_start: (bom as u64) + line.start as u64,
            line_end: (bom as u64) + line.end as u64,
            terminator: line.term,
            line_number,
            dialect: p.dialect,
            flags: p.flags,
            pri: p.pri,
            depth: p.depth,
            fields,
        });
        line_number = line_number.saturating_add(1);
    }
    if records.len() < 2 {
        return Err(Error::invalid_logstream_structure(
            "not a log stream: fewer than two non-blank record lines",
        ));
    }
    Ok(LogstreamModel {
        doc_len: source.len() as u64,
        bom_len: bom,
        records,
        blank_lines,
        crlf_records,
        trailing_newline,
    })
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of a `LogstreamModel` node).
pub fn build_logstream_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits)?.encode())
}

/// One line's exact source bytes (`[line_start, line_end)`; terminator included).
pub fn line_bytes<'a>(source: &'a [u8], record: &LogRecord) -> Result<&'a [u8]> {
    let s = usize::try_from(record.line_start).map_err(|_| corrupt("span overflow"))?;
    let e = usize::try_from(record.line_end).map_err(|_| corrupt("span overflow"))?;
    source
        .get(s..e)
        .ok_or_else(|| corrupt("record line span is outside the source"))
}

/// A field's exact source bytes.
pub fn field_bytes<'a>(source: &'a [u8], f: &LogField) -> Result<&'a [u8]> {
    let s = usize::try_from(f.start).map_err(|_| corrupt("span overflow"))?;
    let e = usize::try_from(f.end).map_err(|_| corrupt("span overflow"))?;
    source
        .get(s..e)
        .ok_or_else(|| corrupt("field span is outside the source"))
}

/// A record's message field's exact source bytes, if it has one.
pub fn message_bytes<'a>(source: &'a [u8], record: &LogRecord) -> Result<Option<&'a [u8]>> {
    match record.field(F_MSG) {
        Some(f) => Ok(Some(field_bytes(source, f)?)),
        None => Ok(None),
    }
}

/// Render a deterministic canonical text projection: each record's message text,
/// one record per line (a derived, lossy view — the exact bytes stay available).
pub fn canonical_text(model: &LogstreamModel, source: &[u8]) -> Result<String> {
    let mut out = String::new();
    for (i, record) in model.records.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if let Some(bytes) = message_bytes(source, record)? {
            out.push_str(&String::from_utf8_lossy(bytes));
        }
    }
    Ok(out)
}

/// A bounded, case-sensitive byte search across **every** record's fields.
/// Matches are returned in record/field order, each with its record index, role,
/// and exact span.
pub fn find(
    model: &LogstreamModel,
    source: &[u8],
    pattern: &str,
    _limits: Limits,
) -> Result<Vec<LogMatch>> {
    let needle = pattern.as_bytes();
    let mut out: Vec<LogMatch> = Vec::new();
    if needle.is_empty() {
        return Ok(out);
    }
    for (i, record) in model.records.iter().enumerate() {
        for f in &record.fields {
            let bytes = field_bytes(source, f)?;
            if contains(bytes, needle) {
                out.push(LogMatch {
                    record: i as u32,
                    role: f.role,
                    start: f.start,
                    end: f.end,
                    text: String::from_utf8_lossy(bytes).into_owned(),
                });
            }
        }
    }
    Ok(out)
}

/// Whether `hay` contains `needle` as a contiguous byte substring.
fn contains(hay: &[u8], needle: &[u8]) -> bool {
    if needle.len() > hay.len() {
        return false;
    }
    hay.windows(needle.len()).any(|w| w == needle)
}

fn corrupt(msg: &str) -> Error {
    Error::invalid_logstream_structure(format!("malformed log-stream model: {msg}"))
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

    fn u16(&mut self) -> Result<u16> {
        let b = self.bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
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

    const SYSLOG5424: &[u8] = b"<165>1 2026-10-11T12:34:56.789Z host app 1234 ID47 [ex@1 k=\"a b\" x=\"q\\\"r\"] hello <world>\n<165>1 2026-10-11T12:34:57Z host app 1234 ID48 -\n";
    const SYSLOG3164: &[u8] =
        b"<34>Oct 11 22:14:15 mymachine su[123]: 'su root' failed\n<34>Oct  1 01:02:03 h x: y\n";
    const GENERIC: &[u8] = b"2026-10-11T12:34:56Z INFO starting up\n[2026-10-11 12:00:00] ERROR boom: x\nDEBUG raw remainder kept\n";

    #[test]
    fn detects_and_parses_the_three_dialects() {
        assert!(detect(SYSLOG5424, Limits::DEFAULT));
        let m = parse(SYSLOG5424, Limits::DEFAULT).unwrap();
        assert_eq!(m.records.len(), 2);
        let r = &m.records[0];
        assert_eq!(r.dialect, DIALECT_RFC5424);
        assert_eq!(r.pri, 165);
        assert_eq!(r.facility(), 20);
        assert_eq!(r.severity(), 5);
        assert_eq!(r.field(F_VERSION).map(|f| f.role), Some(F_VERSION));
        assert!(r.field(F_SD_ELEMENT).is_some());
        assert_eq!(r.fields_with(F_SD_ELEMENT).count(), 1);
        assert_eq!(r.fields_with(F_SD_PARAM_NAME).count(), 2);

        assert!(detect(SYSLOG3164, Limits::DEFAULT));
        let m = parse(SYSLOG3164, Limits::DEFAULT).unwrap();
        assert_eq!(m.records.len(), 2);
        assert_eq!(m.records[0].dialect, DIALECT_RFC3164);
        assert_eq!(m.records[0].pri, 34);
        assert!(m.records[0].field(F_PID).is_some());
        assert_eq!(
            &SYSLOG3164[m.records[1].field(F_TIMESTAMP).unwrap().start as usize
                ..m.records[1].field(F_TIMESTAMP).unwrap().end as usize],
            b"Oct  1 01:02:03"
        );

        assert!(detect(GENERIC, Limits::DEFAULT));
        let m = parse(GENERIC, Limits::DEFAULT).unwrap();
        assert_eq!(m.records.len(), 3);
        assert_eq!(m.records[0].dialect, DIALECT_GENERIC);
        assert!(m.records[0].field(F_LEVEL).is_some());
        assert_eq!(
            field_bytes(GENERIC, m.records[0].field(F_LEVEL).unwrap()).unwrap(),
            b"INFO"
        );
    }

    #[test]
    fn preserves_exact_spans_and_nilvalue() {
        let m = parse(SYSLOG5424, Limits::DEFAULT).unwrap();
        let r = &m.records[0];
        let sd = field_bytes(SYSLOG5424, r.field(F_STRUCTURED_DATA).unwrap()).unwrap();
        assert_eq!(sd, b"[ex@1 k=\"a b\" x=\"q\\\"r\"]");
        let msg = message_bytes(SYSLOG5424, r).unwrap().unwrap();
        assert_eq!(msg, b"hello <world>");
        // NILVALUE is preserved verbatim (the SD NILVALUE `-`).
        let nil = b"<13>1 - - - - - -\n<13>1 - - - - - -\n";
        let m = parse(nil, Limits::DEFAULT).unwrap();
        assert_eq!(m.records.len(), 2);
        assert!(m.records[0].flags & FLAG_NIL_SD != 0);
        assert!(m.records[0].flags & FLAG_HAS_SD == 0);
        assert_eq!(
            field_bytes(nil, m.records[0].field(F_STRUCTURED_DATA).unwrap()).unwrap(),
            b"-"
        );
    }

    #[test]
    fn declines_prose_and_single_lines() {
        assert!(!detect(
            b"just some prose\nwithout structure\n",
            Limits::DEFAULT
        ));
        assert!(!detect(b"INFO only one line\n", Limits::DEFAULT));
        assert_eq!(
            parse(b"INFO only one line\n", Limits::DEFAULT)
                .unwrap_err()
                .class(),
            ErrorClass::InvalidLogstreamStructure
        );
        // A single log-looking line inside prose is not claimed.
        assert!(!detect(
            b"INFO a thing happened\nbut this line is just prose\n",
            Limits::DEFAULT
        ));
    }

    #[test]
    fn caps_decline_typed() {
        let tight_line = Limits {
            max_logstream_line_bytes: 4,
            ..Limits::DEFAULT
        };
        assert!(!detect(GENERIC, tight_line));
        assert_eq!(
            parse(GENERIC, tight_line).unwrap_err().class(),
            ErrorClass::ResourceLimit
        );
        let tight_depth = Limits {
            max_logstream_depth: 1,
            ..Limits::DEFAULT
        };
        assert!(!detect(SYSLOG5424, tight_depth));
        assert_eq!(
            parse(SYSLOG5424, tight_depth).unwrap_err().class(),
            ErrorClass::ResourceLimit
        );
        let tight_records = Limits {
            max_logstream_records: 1,
            ..Limits::DEFAULT
        };
        assert_eq!(
            parse(SYSLOG3164, tight_records).unwrap_err().class(),
            ErrorClass::ResourceLimit
        );
    }

    #[test]
    fn model_roundtrips_and_fails_closed() {
        let m = parse(SYSLOG5424, Limits::DEFAULT).unwrap();
        let bytes = m.encode();
        assert_eq!(LogstreamModel::decode(&bytes).unwrap(), m);
        for cut in 0..bytes.len() {
            let _ = LogstreamModel::decode(&bytes[..cut]);
        }
        let mut bad = bytes.clone();
        bad[0] ^= 0xFF;
        assert!(LogstreamModel::decode(&bad).is_err());
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0x0BAD_F00D_DEAD_BEEF;
        for _ in 0..256 {
            let mut buf = vec![0u8; 256];
            for b in buf.iter_mut() {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                *b = (x & 0xFF) as u8;
            }
            let _ = detect(&buf, Limits::DEFAULT);
            if let Ok(m) = parse(&buf, Limits::STRICT) {
                let _ = canonical_text(&m, &buf);
                let _ = find(&m, &buf, "a", Limits::STRICT);
            }
            let _ = build_logstream_model(&buf, Limits::STRICT);
        }
    }
}
