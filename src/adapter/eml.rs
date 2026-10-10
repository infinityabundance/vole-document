//! Bounded, representation-preserving EML / MIME adapter (Phase 21.13).
//!
//! EML (an RFC 5322 internet message, usually MIME-structured) is the **messaging**
//! Wave-2 format. Like JSON/YAML/CSV/Markdown/XML/HTML/TOML/JSONL it is *not* a
//! package: there is no OPC/ZIP layer, so the exact leaf is the **whole source** (a
//! `DocumentExact`, a RAW-like authority) and everything this module produces is a
//! bounded, deterministic (`Q_gen`) projection that never sits on the exactness
//! path.
//!
//! ## Representation preservation
//!
//! A conventional mail load (`email`, `mailparse`, a `dict`) keeps *meaning* and
//! drops *representation*: it normalizes header order, collapses duplicate headers
//! (keeping only the first or last), unfolds and re-encodes header values, and
//! reserializes the body. This adapter preserves representation instead:
//!
//! * every header keeps its **exact name span**, **exact raw (folded) value span**,
//!   and **exact full span** (including its continuations); header **order** is the
//!   source order and **duplicate** headers (e.g. several `Received:`) stay distinct;
//! * every MIME part keeps its **exact entity/header/body spans**; the
//!   `Content-Transfer-Encoding` decoded bytes are the exact constituent bytes while
//!   the encoded form's span is retained, so the encoded bytes are never a
//!   substitute for the decoded ones (and vice versa);
//! * `multipart/*` trees are resolved by their declared `boundary` (preamble and
//!   epilogue retained as spans of the parent's body, never re-emitted), and nested
//!   `message/rfc822` parts are recursed under a bounded depth;
//! * attachments (`Content-Disposition: attachment`/`inline`, or a filenamed part)
//!   are exposed in document order with their exact decoded bytes.
//!
//! ## Detection
//!
//! [`detect`] is deliberately conservative and requires a genuine RFC 5322 **header
//! block terminated by a blank line** that carries a `From` / `Date` / `Message-ID`
//! header, or an explicit `MIME-Version` header. Prose without a header block, a
//! colon-bearing note, and any earlier format stay
//! [`DocumentFormat::Opaque`](crate::field::document_format::DocumentFormat::Opaque).
//! Detection runs the header scanner only (never the full MIME tree), so it is
//! cheap and cannot be forced to allocate by a large body.
//!
//! ## Declines
//!
//! This adapter declines **typed**, never guessing, the constructs it does not
//! support: a `multipart/*` without a `boundary`, an unknown
//! `Content-Transfer-Encoding`, a non-UTF-8 charset for a *text* observation, and
//! encrypted/signed S/MIME (`application/pkcs7-mime`, `multipart/encrypted`, …).
//! Exact *bytes* stay available for opaque S/MIME payloads; only the text/decoded
//! projections decline.
//!
//! ## Bounds and no panics
//!
//! Untrusted input can only ever yield a typed decline, never a panic or an
//! unbounded allocation: the nesting depth by [`Limits::max_eml_depth`], the part
//! count by [`Limits::max_eml_parts`], the header count by
//! [`Limits::max_eml_headers`], one part's encoded body by
//! [`Limits::max_eml_part_bytes`], the total decoded bytes by
//! [`Limits::max_eml_decoded_bytes`], and the source length by
//! [`Limits::max_eml_document_bytes`].

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model parts (defends the decoder against a hostile blob).
pub const MAX_MODEL_PARTS: u32 = 1 << 22;
/// Hard cap on decoded model headers (defends the decoder against a hostile blob).
pub const MAX_MODEL_HEADERS: u32 = 1 << 22;

/// MIME part kind tag: the whole message root.
pub const K_MESSAGE: u8 = 0;
/// MIME part kind tag: a `text/*` leaf part.
pub const K_TEXT: u8 = 1;
/// MIME part kind tag: a `multipart/*` container.
pub const K_MULTIPART: u8 = 2;
/// MIME part kind tag: a `message/rfc822` container (a nested message).
pub const K_MESSAGE_RFC822: u8 = 3;
/// MIME part kind tag: any other leaf part (application/image/audio/video/…).
pub const K_OTHER: u8 = 4;
/// MIME part kind tag: an encrypted/signed S/MIME part that is declined typed.
pub const K_ENCRYPTED: u8 = 5;

/// Content-Transfer-Encoding tag: `7bit`.
pub const CTE_7BIT: u8 = 0;
/// Content-Transfer-Encoding tag: `8bit`.
pub const CTE_8BIT: u8 = 1;
/// Content-Transfer-Encoding tag: `binary`.
pub const CTE_BINARY: u8 = 2;
/// Content-Transfer-Encoding tag: `quoted-printable`.
pub const CTE_QP: u8 = 3;
/// Content-Transfer-Encoding tag: `base64`.
pub const CTE_BASE64: u8 = 4;
/// Content-Transfer-Encoding tag: any other (unsupported) encoding.
pub const CTE_OTHER: u8 = 5;

/// Stable lower-case name of a part kind tag.
pub const fn kind_name(kind: u8) -> &'static str {
    match kind {
        K_MESSAGE => "message",
        K_TEXT => "text",
        K_MULTIPART => "multipart",
        K_MESSAGE_RFC822 => "message-rfc822",
        K_ENCRYPTED => "encrypted",
        _ => "other",
    }
}

/// Stable lower-case name of a content-transfer-encoding tag.
pub const fn cte_name(cte: u8) -> &'static str {
    match cte {
        CTE_7BIT => "7bit",
        CTE_8BIT => "8bit",
        CTE_BINARY => "binary",
        CTE_QP => "quoted-printable",
        CTE_BASE64 => "base64",
        _ => "other",
    }
}

/// Location tag for [`EmlMatch`]: a header name.
pub const M_HEADER_NAME: u8 = 0;
/// Location tag for [`EmlMatch`]: a header value.
pub const M_HEADER_VALUE: u8 = 1;
/// Location tag for [`EmlMatch`]: decoded body text.
pub const M_BODY: u8 = 2;

/// One message (or MIME part) header: its **exact source spans** and its unfolded
/// name/value text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmlHeader {
    /// The first source byte of the header (its first line).
    pub full_start: u64,
    /// One past the header's last continuation line (terminator included).
    pub full_end: u64,
    /// The first byte of the field name.
    pub name_start: u64,
    /// One past the field name (the `:` is excluded).
    pub name_end: u64,
    /// The first byte of the raw (folded) value (right after the `:`).
    pub value_start: u64,
    /// One past the raw value's last continuation line's content (terminator excluded).
    pub value_end: u64,
    /// The field name exactly as written (case preserved).
    pub name: String,
    /// The unfolded value with leading/trailing ASCII whitespace trimmed.
    pub value: String,
}

impl EmlHeader {
    /// Whether this header's name matches `name` case-insensitively.
    pub fn name_is(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name)
    }
}

/// One MIME entity (the root message or a nested part): exact spans, its header
/// list, and the decoded content-type parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmlPart {
    /// The 0-based index of this part in document (pre-order) order.
    pub index: u32,
    /// The parent part index (`u32::MAX` for the root message).
    pub parent: u32,
    /// The nesting depth (root = 0).
    pub depth: u32,
    /// The part kind tag ([`K_MESSAGE`] … [`K_ENCRYPTED`]).
    pub kind: u8,
    /// The first byte of the entity ([`Self::header_start`]).
    pub entity_start: u64,
    /// One past the entity's body (the parent's boundary excluded).
    pub entity_end: u64,
    /// The first byte of the entity's header block.
    pub header_start: u64,
    /// One past the header block (== [`Self::body_start`]).
    pub header_end: u64,
    /// The first byte of the entity's body.
    pub body_start: u64,
    /// One past the entity's body.
    pub body_end: u64,
    /// The lower-cased media type (e.g. `text/plain`, `multipart/mixed`).
    pub media_type: String,
    /// Whether the media type is `multipart/*`.
    pub is_multipart: bool,
    /// The decoded `boundary` parameter (only meaningful for `multipart/*`).
    pub boundary: Option<String>,
    /// The lower-cased `charset` parameter, if any.
    pub charset: Option<String>,
    /// The content-transfer-encoding tag ([`CTE_7BIT`] … [`CTE_OTHER`]).
    pub cte: u8,
    /// The lower-cased `Content-Disposition` type (`attachment`/`inline`), if any.
    pub disposition: Option<String>,
    /// The decoded `filename` parameter (from `Content-Disposition`), if any.
    pub filename: Option<String>,
    /// The child part indices, in document order (for `multipart/*`/`message/rfc822`).
    pub children: Vec<u32>,
    /// The entity's headers, in source order (duplicates preserved).
    pub headers: Vec<EmlHeader>,
    /// A non-empty reason when this part's text/decoded projection must decline
    /// typed (encrypted S/MIME, an unsupported transfer encoding).
    pub unsupported: Option<String>,
}

impl EmlPart {
    /// Whether this part is a leaf (not a `multipart/*` or `message/rfc822`
    /// container).
    pub fn is_leaf(&self) -> bool {
        self.kind != K_MULTIPART && self.kind != K_MESSAGE_RFC822
    }

    /// The first header named `name` (case-insensitive), if any.
    pub fn header(&self, name: &str) -> Option<&EmlHeader> {
        self.headers.iter().find(|h| h.name_is(name))
    }

    /// The last header named `name` (case-insensitive), if any.
    pub fn last_header(&self, name: &str) -> Option<&EmlHeader> {
        self.headers.iter().rev().find(|h| h.name_is(name))
    }
}

/// The canonical derived EML model (the materialization of an `EmlModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmlModel {
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// Every part (the root message first), in document order.
    pub parts: Vec<EmlPart>,
    /// The observed maximum part nesting depth (root = 0).
    pub max_depth: u32,
    /// The total number of headers across every part.
    pub header_count: u32,
    /// The number of attachment leaf parts.
    pub attachment_count: u32,
    /// The number of `text/*` leaf parts.
    pub text_parts: u32,
    /// The number of `multipart/*` parts.
    pub multipart_parts: u32,
    /// The number of nested `message/rfc822` parts.
    pub message_parts: u32,
    /// The number of quoted-printable-encoded leaf parts.
    pub qp_parts: u32,
    /// The number of base64-encoded leaf parts.
    pub base64_parts: u32,
    /// The number of bytes in every part's raw body span (summed).
    pub body_bytes_total: u64,
    /// Whether the message carries a `MIME-Version` header.
    pub has_mime_version: bool,
    /// Whether the message carries a `From` header.
    pub has_from: bool,
    /// Whether the message carries a `Date` header.
    pub has_date: bool,
    /// Whether the message carries a `Message-ID` header.
    pub has_message_id: bool,
}

impl EmlModel {
    /// The part at `index`, if present.
    pub fn part(&self, index: u32) -> Option<&EmlPart> {
        self.parts.get(index as usize)
    }

    /// The attachment leaf part indices, in document order.
    pub fn attachment_indices(&self) -> Vec<u32> {
        self.parts
            .iter()
            .filter(|p| {
                p.is_leaf()
                    && (p.filename.is_some() || p.disposition.as_deref() == Some("attachment"))
            })
            .map(|p| p.index)
            .collect()
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(128 + self.parts.len() * 128);
        out.extend_from_slice(b"EMLM");
        out.push(MODEL_VERSION);
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&self.max_depth.to_le_bytes());
        out.extend_from_slice(&self.header_count.to_le_bytes());
        out.extend_from_slice(&self.attachment_count.to_le_bytes());
        out.extend_from_slice(&self.text_parts.to_le_bytes());
        out.extend_from_slice(&self.multipart_parts.to_le_bytes());
        out.extend_from_slice(&self.message_parts.to_le_bytes());
        out.extend_from_slice(&self.qp_parts.to_le_bytes());
        out.extend_from_slice(&self.base64_parts.to_le_bytes());
        out.extend_from_slice(&self.body_bytes_total.to_le_bytes());
        out.push(flags(self));
        out.extend_from_slice(&(self.parts.len() as u32).to_le_bytes());
        for p in &self.parts {
            encode_part(&mut out, p);
        }
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<EmlModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"EMLM" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let doc_len = r.u64()?;
        let max_depth = r.u32()?;
        let header_count = r.u32()?;
        let attachment_count = r.u32()?;
        let text_parts = r.u32()?;
        let multipart_parts = r.u32()?;
        let message_parts = r.u32()?;
        let qp_parts = r.u32()?;
        let base64_parts = r.u32()?;
        let body_bytes_total = r.u64()?;
        let fl = r.u8()?;
        let count = r.u32()?;
        if count > MAX_MODEL_PARTS {
            return Err(corrupt("model part count is implausible"));
        }
        let mut parts = Vec::with_capacity(count as usize);
        for _ in 0..count {
            parts.push(decode_part(&mut r, doc_len)?);
        }
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        Ok(EmlModel {
            doc_len,
            parts,
            max_depth,
            header_count,
            attachment_count,
            text_parts,
            multipart_parts,
            message_parts,
            qp_parts,
            base64_parts,
            body_bytes_total,
            has_mime_version: fl & 1 != 0,
            has_from: fl & 2 != 0,
            has_date: fl & 4 != 0,
            has_message_id: fl & 8 != 0,
        })
    }
}

fn flags(m: &EmlModel) -> u8 {
    u8::from(m.has_mime_version)
        | (u8::from(m.has_from) << 1)
        | (u8::from(m.has_date) << 2)
        | (u8::from(m.has_message_id) << 3)
}

fn encode_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn encode_opt_str(out: &mut Vec<u8>, s: Option<&str>) {
    match s {
        Some(s) => {
            out.push(1);
            encode_str(out, s);
        }
        None => out.push(0),
    }
}

fn encode_part(out: &mut Vec<u8>, p: &EmlPart) {
    out.extend_from_slice(&p.index.to_le_bytes());
    out.extend_from_slice(&p.parent.to_le_bytes());
    out.extend_from_slice(&p.depth.to_le_bytes());
    out.push(p.kind);
    out.extend_from_slice(&p.entity_start.to_le_bytes());
    out.extend_from_slice(&p.entity_end.to_le_bytes());
    out.extend_from_slice(&p.header_start.to_le_bytes());
    out.extend_from_slice(&p.header_end.to_le_bytes());
    out.extend_from_slice(&p.body_start.to_le_bytes());
    out.extend_from_slice(&p.body_end.to_le_bytes());
    encode_str(out, &p.media_type);
    out.push(u8::from(p.is_multipart));
    encode_opt_str(out, p.boundary.as_deref());
    encode_opt_str(out, p.charset.as_deref());
    out.push(p.cte);
    encode_opt_str(out, p.disposition.as_deref());
    encode_opt_str(out, p.filename.as_deref());
    encode_opt_str(out, p.unsupported.as_deref());
    out.extend_from_slice(&(p.children.len() as u32).to_le_bytes());
    for c in &p.children {
        out.extend_from_slice(&c.to_le_bytes());
    }
    out.extend_from_slice(&(p.headers.len() as u32).to_le_bytes());
    for h in &p.headers {
        out.extend_from_slice(&h.full_start.to_le_bytes());
        out.extend_from_slice(&h.full_end.to_le_bytes());
        out.extend_from_slice(&h.name_start.to_le_bytes());
        out.extend_from_slice(&h.name_end.to_le_bytes());
        out.extend_from_slice(&h.value_start.to_le_bytes());
        out.extend_from_slice(&h.value_end.to_le_bytes());
        encode_str(out, &h.name);
        encode_str(out, &h.value);
    }
}

fn decode_opt_str(r: &mut BinReader<'_>) -> Result<Option<String>> {
    match r.u8()? {
        0 => Ok(None),
        1 => Ok(Some(r.string()?)),
        _ => Err(corrupt("bad optional-string flag")),
    }
}

fn decode_part(r: &mut BinReader<'_>, doc_len: u64) -> Result<EmlPart> {
    let index = r.u32()?;
    let parent = r.u32()?;
    let depth = r.u32()?;
    let kind = r.u8()?;
    let entity_start = r.u64()?;
    let entity_end = r.u64()?;
    let header_start = r.u64()?;
    let header_end = r.u64()?;
    let body_start = r.u64()?;
    let body_end = r.u64()?;
    if entity_start > entity_end
        || entity_end > doc_len
        || header_start < entity_start
        || header_end < header_start
        || body_start < header_end
        || body_end < body_start
        || body_end > entity_end
    {
        return Err(corrupt("part span is outside the document"));
    }
    let media_type = r.string()?;
    let is_multipart = match r.u8()? {
        0 => false,
        1 => true,
        _ => return Err(corrupt("bad is-multipart flag")),
    };
    let boundary = decode_opt_str(r)?;
    let charset = decode_opt_str(r)?;
    let cte = r.u8()?;
    let disposition = decode_opt_str(r)?;
    let filename = decode_opt_str(r)?;
    let unsupported = decode_opt_str(r)?;
    let ccount = r.u32()?;
    if ccount > MAX_MODEL_PARTS {
        return Err(corrupt("part child count is implausible"));
    }
    let mut children = Vec::with_capacity(ccount as usize);
    for _ in 0..ccount {
        children.push(r.u32()?);
    }
    let hcount = r.u32()?;
    if hcount > MAX_MODEL_HEADERS {
        return Err(corrupt("part header count is implausible"));
    }
    let mut headers = Vec::with_capacity(hcount as usize);
    for _ in 0..hcount {
        let full_start = r.u64()?;
        let full_end = r.u64()?;
        let name_start = r.u64()?;
        let name_end = r.u64()?;
        let value_start = r.u64()?;
        let value_end = r.u64()?;
        if full_start > full_end
            || name_start < full_start
            || name_end < name_start
            || name_end > full_end
            || value_start < name_end
            || value_end < value_start
            || value_end > full_end
            || full_end > doc_len
        {
            return Err(corrupt("header span is outside the document"));
        }
        let name = r.string()?;
        let value = r.string()?;
        headers.push(EmlHeader {
            full_start,
            full_end,
            name_start,
            name_end,
            value_start,
            value_end,
            name,
            value,
        });
    }
    Ok(EmlPart {
        index,
        parent,
        depth,
        kind,
        entity_start,
        entity_end,
        header_start,
        header_end,
        body_start,
        body_end,
        media_type,
        is_multipart,
        boundary,
        charset,
        cte,
        disposition,
        filename,
        children,
        headers,
        unsupported,
    })
}

/// One lexical match from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmlMatch {
    /// The location tag ([`M_HEADER_NAME`]/[`M_HEADER_VALUE`]/[`M_BODY`]).
    pub location: u8,
    /// The 0-based part index the match lies in.
    pub part: u32,
    /// The header name for a header match (`""` for a body match).
    pub name: String,
    /// The exact source span of the matching text.
    pub start: u64,
    /// One past the matching text.
    pub end: u64,
    /// The decoded matching text.
    pub text: String,
}

/// Stable lower-case name of a match location tag.
pub const fn loc_name(loc: u8) -> &'static str {
    match loc {
        M_HEADER_NAME => "header-name",
        M_HEADER_VALUE => "header-value",
        _ => "body",
    }
}

/// A physical line: byte offsets and terminator tag.
#[derive(Debug, Clone, Copy)]
struct Line {
    /// First byte of the line.
    start: u64,
    /// One past the line content (the terminator excluded).
    content_end: u64,
    /// One past the line (the terminator included, or the source end at EOF).
    end: u64,
}

/// Split `source` into physical lines (bounded by the source length).
fn build_lines(source: &[u8]) -> Vec<Line> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at < source.len() {
        let start = at;
        let mut i = at;
        while i < source.len() && source[i] != b'\n' {
            i += 1;
        }
        let (content_end, end) = if i < source.len() {
            let end = i + 1;
            if i > start && source[i - 1] == b'\r' {
                (i - 1, end)
            } else {
                (i, end)
            }
        } else {
            (source.len(), source.len())
        };
        out.push(Line {
            start: start as u64,
            content_end: content_end as u64,
            end: end as u64,
        });
        at = end;
    }
    out
}

/// Whether a byte is a valid RFC 5322 field-name character (printable US-ASCII
/// except `:`).
fn is_ftext(b: u8) -> bool {
    (33..=126).contains(&b) && b != b':'
}

fn trim_ascii_ws(b: &[u8]) -> &[u8] {
    let mut s = 0;
    let mut e = b.len();
    while s < e && b[s].is_ascii_whitespace() {
        s += 1;
    }
    while e > s && b[e - 1].is_ascii_whitespace() {
        e -= 1;
    }
    &b[s..e]
}

fn rtrim_ascii_ws(b: &[u8]) -> &[u8] {
    let mut e = b.len();
    while e > 0 && b[e - 1].is_ascii_whitespace() {
        e -= 1;
    }
    &b[..e]
}

fn hexval(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn corrupt(msg: &str) -> Error {
    Error::invalid_eml_structure(format!("malformed EML model: {msg}"))
}

/// The header block scan result: the headers, the body start offset, and whether a
/// blank line (not EOF) terminated the block.
struct HeaderScan {
    headers: Vec<EmlHeader>,
    body_start: u64,
    terminated_by_blank: bool,
}

/// Scan an RFC 5322 header block in `[start, end)`. Folding continuations are
/// attached to the preceding header. A line that is neither a valid header nor a
/// continuation is a typed decline.
fn scan_headers(
    source: &[u8],
    lines: &[Line],
    start: u64,
    end: u64,
    max_headers: u32,
) -> Result<HeaderScan> {
    let mut headers: Vec<EmlHeader> = Vec::new();
    if start >= end {
        return Ok(HeaderScan {
            headers,
            body_start: end,
            terminated_by_blank: false,
        });
    }
    let mut idx = line_index(lines, start);
    let mut body_start = end;
    let mut terminated_by_blank = false;
    while idx < lines.len() {
        let line = lines[idx];
        if line.start >= end {
            body_start = end;
            break;
        }
        let content = &source[line.start as usize..line.content_end as usize];
        if content.is_empty() {
            body_start = line.end.min(end);
            terminated_by_blank = true;
            break;
        }
        if (content[0] == b' ' || content[0] == b'\t') && !headers.is_empty() {
            let last = headers
                .last_mut()
                .ok_or_else(|| Error::internal_invariant("folded header without a predecessor"))?;
            last.full_end = line.end.min(end);
            last.value_end = line.content_end;
            let cont = trim_ascii_ws(content);
            if !last.value.is_empty() && !cont.is_empty() {
                last.value.push(' ');
            }
            last.value.push_str(&String::from_utf8_lossy(cont));
            idx += 1;
            continue;
        }
        let colon = content
            .iter()
            .position(|&b| b == b':')
            .ok_or_else(|| Error::invalid_eml_structure("header line has no colon"))?;
        let name = &content[..colon];
        if name.is_empty() || !name.iter().all(|&b| is_ftext(b)) {
            return Err(Error::invalid_eml_structure(
                "header line has an invalid field name",
            ));
        }
        if headers.len() as u64 >= max_headers as u64 {
            return Err(Error::resource_limit(format!(
                "EML document exceeds the {max_headers}-header cap"
            )));
        }
        let name_start = line.start;
        let name_end = line.start + colon as u64;
        let value_start = name_end + 1;
        let value_end = line.content_end;
        let vraw = &source[value_start as usize..value_end as usize];
        headers.push(EmlHeader {
            full_start: line.start,
            full_end: line.end.min(end),
            name_start,
            name_end,
            value_start,
            value_end,
            name: String::from_utf8_lossy(name).into_owned(),
            value: String::from_utf8_lossy(trim_ascii_ws(vraw)).into_owned(),
        });
        idx += 1;
    }
    Ok(HeaderScan {
        headers,
        body_start,
        terminated_by_blank,
    })
}

/// The first line index whose `start <= off` (0 when `off` precedes every line).
fn line_index(lines: &[Line], off: u64) -> usize {
    lines.partition_point(|l| l.start <= off).saturating_sub(1)
}

/// A parsed `Content-Type` value.
struct ContentType {
    media_type: String,
    boundary: Option<String>,
    charset: Option<String>,
}

fn parse_content_type(raw: Option<&str>) -> ContentType {
    let Some(raw) = raw else {
        return ContentType {
            media_type: "text/plain".to_string(),
            boundary: None,
            charset: None,
        };
    };
    let mut segments = raw.split(';');
    let media_type = segments
        .next()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "text/plain".to_string());
    let mut boundary = None;
    let mut charset = None;
    for seg in segments {
        let Some((k, v)) = seg.split_once('=') else {
            continue;
        };
        let key = k.trim().to_ascii_lowercase();
        let val = unquote(v.trim());
        match key.as_str() {
            "boundary" => boundary = Some(val),
            "charset" => charset = Some(val.to_ascii_lowercase()),
            _ => {}
        }
    }
    ContentType {
        media_type,
        boundary,
        charset,
    }
}

/// Strip a single level of surrounding double quotes.
fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Extract a `name=value` parameter from a header value (e.g. the `filename` of a
/// `Content-Disposition`).
fn header_param(raw: Option<&str>, param: &str) -> Option<String> {
    let raw = raw?;
    for seg in raw.split(';').skip(1) {
        let Some((k, v)) = seg.split_once('=') else {
            continue;
        };
        if k.trim().eq_ignore_ascii_case(param) {
            return Some(unquote(v));
        }
    }
    None
}

/// The main media type (before `;`) of a raw `Content-Type` value.
fn media_type_of(raw: Option<&str>) -> String {
    raw.map(|r| {
        r.split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase()
    })
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "text/plain".to_string())
}

fn cte_of(raw: Option<&str>) -> u8 {
    match raw.map(|r| r.trim().to_ascii_lowercase()) {
        None => CTE_7BIT,
        Some(s) if s.is_empty() => CTE_7BIT,
        Some(s) => match s.as_str() {
            "7bit" => CTE_7BIT,
            "8bit" => CTE_8BIT,
            "binary" => CTE_BINARY,
            "quoted-printable" => CTE_QP,
            "base64" => CTE_BASE64,
            _ => CTE_OTHER,
        },
    }
}

fn is_multipart_media_type(mt: &str) -> bool {
    mt.starts_with("multipart/")
}

/// Whether a media type denotes encrypted/signed S/MIME the adapter declines.
fn is_encrypted_media_type(mt: &str) -> bool {
    matches!(
        mt,
        "application/pkcs7-mime"
            | "application/pkcs7-signature"
            | "application/x-pkcs7-mime"
            | "application/x-pkcs7-signature"
            | "multipart/encrypted"
            | "multipart/signed"
    )
}

/// Whether a lower-cased charset is one this adapter will turn into text.
fn charset_is_utf8(charset: Option<&str>) -> bool {
    match charset {
        None => true,
        Some(s) => matches!(
            s,
            "" | "utf-8" | "utf8" | "us-ascii" | "ascii" | "ansi_x3.4-1968"
        ),
    }
}

/// Bounded fallible parser over one message.
struct Parser<'a> {
    source: &'a [u8],
    lines: Vec<Line>,
    limits: Limits,
    parts: Vec<EmlPart>,
    header_count: u32,
}

impl<'a> Parser<'a> {
    fn parse_entity(
        &mut self,
        entity_start: u64,
        header_start: u64,
        end: u64,
        parent: u32,
        depth: u32,
    ) -> Result<u32> {
        if depth > self.limits.max_eml_depth {
            return Err(Error::resource_limit(format!(
                "EML part nesting exceeds the {}-deep cap",
                self.limits.max_eml_depth
            )));
        }
        if self.parts.len() as u64 >= self.limits.max_eml_parts as u64 {
            return Err(Error::resource_limit(format!(
                "EML document exceeds the {}-part cap",
                self.limits.max_eml_parts
            )));
        }
        if end < entity_start {
            return Err(corrupt("entity span is inverted"));
        }
        if end - entity_start > self.limits.max_eml_part_bytes {
            return Err(Error::resource_limit(format!(
                "EML part is {} bytes, above the {}-byte part cap",
                end - entity_start,
                self.limits.max_eml_part_bytes
            )));
        }
        let scan = scan_headers(
            self.source,
            &self.lines,
            header_start,
            end,
            self.limits
                .max_eml_headers
                .saturating_sub(self.header_count),
        )?;
        self.header_count = self.header_count.saturating_add(scan.headers.len() as u32);
        let body_start = scan.body_start.max(header_start);
        let body_end = end;

        let content_type = scan
            .headers
            .iter()
            .rev()
            .find(|h| h.name_is("content-type"))
            .map(|h| h.value.clone());
        let ct_raw = content_type.as_deref();
        let ct = parse_content_type(ct_raw);
        let media_type = if ct_raw.is_none() {
            media_type_of(None)
        } else {
            ct.media_type.clone()
        };
        let raw_cte = scan
            .headers
            .iter()
            .rev()
            .find(|h| h.name_is("content-transfer-encoding"))
            .map(|h| h.value.as_str());
        let cte = cte_of(raw_cte);
        let raw_disp = scan
            .headers
            .iter()
            .rev()
            .find(|h| h.name_is("content-disposition"))
            .map(|h| h.value.as_str());
        let disposition = raw_disp.map(|d| {
            d.split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase()
        });
        let disposition = disposition.filter(|s| !s.is_empty());
        let filename = header_param(raw_disp, "filename");

        let is_multipart = is_multipart_media_type(&media_type);
        let encrypted = is_encrypted_media_type(&media_type);

        let index = self.parts.len() as u32;
        let mut unsupported = None;
        if encrypted {
            unsupported = Some(format!(
                "encrypted/signed S/MIME ({media_type}) is not supported"
            ));
        } else if cte == CTE_OTHER {
            unsupported = Some(format!(
                "unsupported Content-Transfer-Encoding ({})",
                raw_cte.unwrap_or("").trim()
            ));
        }

        self.parts.push(EmlPart {
            index,
            parent,
            depth,
            kind: K_MESSAGE,
            entity_start,
            entity_end: end,
            header_start,
            header_end: body_start,
            body_start,
            body_end,
            media_type: media_type.clone(),
            is_multipart,
            boundary: ct.boundary.clone(),
            charset: ct.charset.clone(),
            cte,
            disposition,
            filename,
            children: Vec::new(),
            headers: scan.headers,
            unsupported,
        });

        let mut children: Vec<u32> = Vec::new();
        if encrypted {
            // Declined typed: never descend into S/MIME ciphertext.
        } else if is_multipart {
            let boundary = ct
                .boundary
                .clone()
                .filter(|b| !b.is_empty())
                .ok_or_else(|| {
                    Error::invalid_eml_structure("multipart/* without a boundary parameter")
                })?;
            let ranges = self.split_multipart(body_start, body_end, &boundary)?;
            for (ps, pe) in ranges {
                let child = self.parse_entity(ps, ps, pe, index, depth + 1)?;
                children.push(child);
            }
        } else if media_type == "message/rfc822" && body_start < body_end {
            let child = self.parse_entity(body_start, body_start, body_end, index, depth + 1)?;
            children.push(child);
        }

        let kind = part_kind(&media_type, is_multipart, encrypted);
        let part = self
            .parts
            .get_mut(index as usize)
            .ok_or_else(|| Error::internal_invariant("EML part index drifted"))?;
        part.kind = kind;
        part.children = children;
        Ok(index)
    }

    /// Split a `multipart/*` body into `(start, end)` ranges. A part's body excludes
    /// the CRLF that precedes the next boundary delimiter (RFC 2046).
    fn split_multipart(
        &self,
        body_start: u64,
        body_end: u64,
        boundary: &str,
    ) -> Result<Vec<(u64, u64)>> {
        let delim = format!("--{boundary}");
        let delim_close = format!("--{boundary}--");
        let mut ranges: Vec<(u64, u64)> = Vec::new();
        let mut idx = line_index(&self.lines, body_start);
        let mut current: Option<(u64, u64, bool)> = None; // (start, last_content_end, had_line)
        while idx < self.lines.len() {
            let line = self.lines[idx];
            if line.start >= body_end {
                break;
            }
            let content = &self.source[line.start as usize..line.content_end as usize];
            let stripped = rtrim_ascii_ws(content);
            let is_close = stripped == delim_close.as_bytes();
            if stripped == delim.as_bytes() || is_close {
                if let Some((cs, lce, had)) = current.take() {
                    if ranges.len() as u64 >= self.limits.max_eml_parts as u64 {
                        return Err(Error::resource_limit(format!(
                            "EML document exceeds the {}-part cap",
                            self.limits.max_eml_parts
                        )));
                    }
                    ranges.push((cs, if had { lce } else { cs }));
                }
                if !is_close {
                    current = Some((line.end.min(body_end), line.end.min(body_end), false));
                }
                idx += 1;
                continue;
            }
            if let Some(slot) = current.as_mut() {
                slot.1 = line.content_end;
                slot.2 = true;
            }
            idx += 1;
        }
        if let Some((cs, lce, had)) = current.take() {
            if ranges.len() as u64 >= self.limits.max_eml_parts as u64 {
                return Err(Error::resource_limit(format!(
                    "EML document exceeds the {}-part cap",
                    self.limits.max_eml_parts
                )));
            }
            let end = if had { lce.min(body_end) } else { cs };
            ranges.push((cs, end.max(cs)));
        }
        Ok(ranges)
    }
}

fn part_kind(media_type: &str, is_multipart: bool, encrypted: bool) -> u8 {
    if encrypted {
        K_ENCRYPTED
    } else if is_multipart {
        K_MULTIPART
    } else if media_type == "message/rfc822" {
        K_MESSAGE_RFC822
    } else if media_type.starts_with("text/") {
        K_TEXT
    } else {
        K_OTHER
    }
}

/// Parse `source` into an [`EmlModel`]. A source that is not an RFC 5322 message (no
/// header block), a malformed header, or a bound violation is a typed decline.
pub fn parse(source: &[u8], limits: Limits) -> Result<EmlModel> {
    if source.len() as u64 > limits.max_eml_document_bytes {
        return Err(Error::resource_limit(format!(
            "EML source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_eml_document_bytes
        )));
    }
    let lines = build_lines(source);
    let envelope = mbox_envelope_len(source);
    let scan = scan_headers(
        source,
        &lines,
        envelope,
        source.len() as u64,
        limits.max_eml_headers,
    )?;
    if scan.headers.is_empty() {
        return Err(Error::invalid_eml_structure(
            "not EML: the source has no RFC 5322 header block",
        ));
    }
    // Build the model with a fresh parser (re-scans the root header block, which is
    // cheap, and keeps the recursive parser uniform). The root entity spans the whole
    // document (including a leading mbox `From ` envelope, which is not RFC 5322
    // content); the header block begins after that envelope.
    let mut parser = Parser {
        source,
        lines,
        limits,
        parts: Vec::new(),
        header_count: 0,
    };
    parser.parse_entity(0, envelope, source.len() as u64, u32::MAX, 0)?;

    let mut model = EmlModel {
        doc_len: source.len() as u64,
        parts: parser.parts,
        max_depth: 0,
        header_count: 0,
        attachment_count: 0,
        text_parts: 0,
        multipart_parts: 0,
        message_parts: 0,
        qp_parts: 0,
        base64_parts: 0,
        body_bytes_total: 0,
        has_mime_version: false,
        has_from: false,
        has_date: false,
        has_message_id: false,
    };
    let root = model.parts.first();
    if let Some(root) = root {
        model.has_mime_version = root.has_header("mime-version");
        model.has_from = root.has_header("from");
        model.has_date = root.has_header("date");
        model.has_message_id = root.has_header("message-id");
    }
    for p in &model.parts {
        model.max_depth = model.max_depth.max(p.depth);
        model.header_count = model.header_count.saturating_add(p.headers.len() as u32);
        model.body_bytes_total = model
            .body_bytes_total
            .saturating_add(p.body_end.saturating_sub(p.body_start));
        if p.is_multipart {
            model.multipart_parts += 1;
        } else if p.kind == K_MESSAGE_RFC822 {
            model.message_parts += 1;
        } else if p.kind == K_TEXT {
            model.text_parts += 1;
        }
        if p.is_leaf() {
            match p.cte {
                CTE_QP => model.qp_parts += 1,
                CTE_BASE64 => model.base64_parts += 1,
                _ => {}
            }
        }
        if p.is_leaf() && (p.filename.is_some() || p.disposition.as_deref() == Some("attachment")) {
            model.attachment_count += 1;
        }
    }
    Ok(model)
}

impl EmlPart {
    fn has_header(&self, name: &str) -> bool {
        self.headers.iter().any(|h| h.name_is(name))
    }
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of an `EmlModel` node).
pub fn build_eml_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits)?.encode())
}

/// Fast-fail EML detector: does `source` carry a genuine RFC 5322 header block
/// terminated by a blank line, with a `From`/`Date`/`Message-ID` header or an
/// explicit `MIME-Version` header?
///
/// Conservative by construction: prose without a header block, a colon-bearing note
/// without a body separator, and any earlier format are all rejected. Only the
/// header block is scanned, never the MIME tree — except that a `multipart/*` root
/// naming no `boundary` is declined too, because `parse` cannot model it. A leading
/// Unix-mbox `From ` envelope line is skipped before the header scan.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_eml_document_bytes {
        return false;
    }
    let lines = build_lines(source);
    let envelope = mbox_envelope_len(source);
    let Ok(scan) = scan_headers(
        source,
        &lines,
        envelope,
        source.len() as u64,
        limits.max_eml_headers,
    ) else {
        return false;
    };
    if scan.headers.is_empty() || !scan.terminated_by_blank {
        return false;
    }
    if !scan.headers.iter().any(|h| {
        h.name_is("from")
            || h.name_is("date")
            || h.name_is("message-id")
            || h.name_is("mime-version")
    }) {
        return false;
    }
    // Keep detection consistent with `parse`: a `multipart/*` root that declares no
    // `boundary` is not modellable (the MIME tree cannot be split), so it is not
    // admitted as EML even though its header block is otherwise well-formed.
    if let Some(ct) = scan
        .headers
        .iter()
        .rev()
        .find(|h| h.name_is("content-type"))
    {
        let parsed = parse_content_type(Some(ct.value.as_str()));
        if is_multipart_media_type(&parsed.media_type)
            && parsed.boundary.as_deref().is_none_or(|b| b.is_empty())
        {
            return false;
        }
    }
    true
}

/// The byte length of a leading Unix-mbox `From ` separator line (the "From_"
/// envelope), or `0` when the source does not begin with one.
///
/// An mbox envelope is a line beginning with the five bytes `From ` — a space, not
/// the colon of the RFC 5322 `From:` header — at column 0, followed by a sender and
/// an `asctime` stamp. It is a mailbox delimiter, not part of the RFC 5322 message,
/// but it is common in real message files, so the header scan skips it. The line
/// must be non-empty after `From ` and contain a four-digit year so that prose
/// beginning "From ..." is not mistaken for an envelope; anything else falls
/// through to the ordinary header scan and stays Opaque.
fn mbox_envelope_len(source: &[u8]) -> u64 {
    const PREFIX: &[u8] = b"From ";
    if !source.starts_with(PREFIX) {
        return 0;
    }
    let mut eol = PREFIX.len();
    while eol < source.len() && source[eol] != b'\n' && source[eol] != b'\r' {
        eol += 1;
    }
    let line = &source[PREFIX.len()..eol];
    if line.is_empty() || !line.windows(4).any(|w| w.iter().all(u8::is_ascii_digit)) {
        return 0;
    }
    // Include the line ending so the header block starts on the next line.
    if eol < source.len() && source[eol] == b'\r' {
        eol += 1;
    }
    if eol < source.len() && source[eol] == b'\n' {
        eol += 1;
    }
    eol as u64
}

/// The exact raw (encoded) body bytes of a part.
pub fn raw_body_bytes<'a>(source: &'a [u8], part: &EmlPart) -> Result<&'a [u8]> {
    let s = usize::try_from(part.body_start).map_err(|_| corrupt("span overflow"))?;
    let e = usize::try_from(part.body_end).map_err(|_| corrupt("span overflow"))?;
    source
        .get(s..e)
        .ok_or_else(|| corrupt("part body span is outside the source"))
}

/// Decode a part's body to its **exact constituent bytes** per its
/// `Content-Transfer-Encoding`. Bounded by [`Limits::max_eml_decoded_bytes`].
pub fn decode_body(source: &[u8], part: &EmlPart, limits: Limits) -> Result<Vec<u8>> {
    if let Some(reason) = &part.unsupported {
        return Err(Error::unsupported_feature(format!(
            "EML part {} cannot be decoded: {reason}",
            part.index
        )));
    }
    let raw = raw_body_bytes(source, part)?;
    match part.cte {
        CTE_7BIT | CTE_8BIT | CTE_BINARY => {
            if raw.len() as u64 > limits.max_eml_decoded_bytes {
                return Err(Error::resource_limit(format!(
                    "EML part decodes to {} bytes, above the {}-byte decoded cap",
                    raw.len(),
                    limits.max_eml_decoded_bytes
                )));
            }
            Ok(raw.to_vec())
        }
        CTE_QP => decode_qp(raw, limits.max_eml_decoded_bytes),
        CTE_BASE64 => decode_base64(raw, limits.max_eml_decoded_bytes),
        _ => Err(Error::unsupported_feature(format!(
            "EML part {} uses an unsupported Content-Transfer-Encoding",
            part.index
        ))),
    }
}

fn decode_qp(input: &[u8], max: u64) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len().min(max as usize));
    let mut i = 0usize;
    while i < input.len() {
        let b = input[i];
        if b == b'=' {
            if i + 1 < input.len() && input[i + 1] == b'\n' {
                i += 2;
                continue;
            }
            if i + 2 < input.len() && input[i + 1] == b'\r' && input[i + 2] == b'\n' {
                i += 3;
                continue;
            }
            if i + 2 < input.len()
                && let (Some(a), Some(c)) = (hexval(input[i + 1]), hexval(input[i + 2]))
            {
                out.push(a * 16 + c);
                i += 3;
                if out.len() as u64 > max {
                    return Err(Error::resource_limit(format!(
                        "EML quoted-printable decodes above the {max}-byte decoded cap"
                    )));
                }
                continue;
            }
            return Err(Error::invalid_eml_structure(
                "invalid quoted-printable escape",
            ));
        }
        out.push(b);
        i += 1;
        if out.len() as u64 > max {
            return Err(Error::resource_limit(format!(
                "EML quoted-printable decodes above the {max}-byte decoded cap"
            )));
        }
    }
    Ok(out)
}

fn decode_base64(input: &[u8], max: u64) -> Result<Vec<u8>> {
    let mut chars: Vec<u8> = Vec::with_capacity(input.len());
    for &b in input {
        if b.is_ascii_whitespace() {
            continue;
        }
        chars.push(b);
    }
    let mut end = chars.len();
    while end > 0 && chars[end - 1] == b'=' {
        end -= 1;
    }
    let pad = chars.len() - end;
    if pad > 2 {
        return Err(Error::invalid_eml_structure("invalid base64 padding"));
    }
    let data = &chars[..end];
    if data.len() % 4 == 1 {
        return Err(Error::invalid_eml_structure("invalid base64 length"));
    }
    let mut out = Vec::with_capacity(data.len() / 4 * 3 + 3);
    let mut acc: u32 = 0;
    let mut n = 0u32;
    for &b in data {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err(Error::invalid_eml_structure("invalid base64 character")),
        };
        acc = (acc << 6) | u32::from(v);
        n += 1;
        if n == 4 {
            out.push(((acc >> 16) & 0xff) as u8);
            out.push(((acc >> 8) & 0xff) as u8);
            out.push((acc & 0xff) as u8);
            acc = 0;
            n = 0;
            if out.len() as u64 > max {
                return Err(Error::resource_limit(format!(
                    "EML base64 decodes above the {max}-byte decoded cap"
                )));
            }
        }
    }
    match n {
        0 => {}
        2 => out.push(((acc >> 4) & 0xff) as u8),
        3 => {
            out.push(((acc >> 10) & 0xff) as u8);
            out.push(((acc >> 2) & 0xff) as u8);
        }
        _ => return Err(Error::invalid_eml_structure("invalid base64 tail")),
    }
    if out.len() as u64 > max {
        return Err(Error::resource_limit(format!(
            "EML base64 decodes above the {max}-byte decoded cap"
        )));
    }
    Ok(out)
}

/// The message body text: the first `text/plain` leaf's decoded text, else the first
/// `text/*` leaf's, else the empty string. A non-UTF-8 charset declines typed.
pub fn body_text(model: &EmlModel, source: &[u8], limits: Limits) -> Result<String> {
    for want_plain in [true, false] {
        for p in &model.parts {
            if p.kind != K_TEXT || !p.is_leaf() {
                continue;
            }
            if want_plain && p.media_type != "text/plain" {
                continue;
            }
            if !charset_is_utf8(p.charset.as_deref()) {
                return Err(Error::unsupported_feature(format!(
                    "EML text part {} uses the non-UTF-8 charset {}",
                    p.index,
                    p.charset.as_deref().unwrap_or("")
                )));
            }
            let bytes = decode_body(source, p, limits)?;
            return Ok(String::from_utf8_lossy(&bytes).into_owned());
        }
    }
    // No readable text part. If the message is encrypted/signed S/MIME, decline
    // typed rather than silently returning an empty body.
    if model.parts.iter().any(|p| p.kind == K_ENCRYPTED) {
        return Err(Error::unsupported_feature(
            "encrypted/signed S/MIME has no readable text body",
        ));
    }
    Ok(String::new())
}

/// A bounded, case-sensitive lexical search over **every** part's header names,
/// header values, and decoded body text (of `text/*` leaves), in document order.
pub fn find(
    model: &EmlModel,
    source: &[u8],
    pattern: &str,
    limits: Limits,
) -> Result<Vec<EmlMatch>> {
    let max_matches = (limits.max_eml_headers as u64).max(1);
    let mut out: Vec<EmlMatch> = Vec::new();
    for p in &model.parts {
        for h in &p.headers {
            if h.name.contains(pattern) {
                out.push(EmlMatch {
                    location: M_HEADER_NAME,
                    part: p.index,
                    name: h.name.clone(),
                    start: h.name_start,
                    end: h.name_end,
                    text: h.name.clone(),
                });
            }
            if h.value.contains(pattern) {
                out.push(EmlMatch {
                    location: M_HEADER_VALUE,
                    part: p.index,
                    name: h.name.clone(),
                    start: h.value_start,
                    end: h.value_end,
                    text: h.value.clone(),
                });
            }
            if out.len() as u64 > max_matches {
                return Err(Error::resource_limit(format!(
                    "EML find exceeded the {max_matches}-match cap"
                )));
            }
        }
        if p.kind == K_TEXT && p.is_leaf() && charset_is_utf8(p.charset.as_deref()) {
            let bytes = decode_body(source, p, limits)?;
            let text = String::from_utf8_lossy(&bytes).into_owned();
            if text.contains(pattern) {
                out.push(EmlMatch {
                    location: M_BODY,
                    part: p.index,
                    name: String::new(),
                    start: p.body_start,
                    end: p.body_end,
                    text,
                });
            }
        }
    }
    Ok(out)
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

    fn string(&mut self) -> Result<String> {
        let n = self.u32()? as usize;
        let bytes = self.bytes(n)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| corrupt("model string is not UTF-8"))
    }

    fn at_end(&self) -> bool {
        self.at == self.b.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorClass;

    const SIMPLE: &[u8] =
        b"From: Alice <alice@example.com>\r\nTo: Bob <bob@example.com>\r\nSubject: Hi\r\nDate: Mon, 01 Jan 2029 00:00:00 +0000\r\nMessage-ID: <a@b>\r\n\r\nHello, world!\r\n";

    const MULTIPART: &[u8] = b"From: a@b\r\nDate: x\r\nMessage-ID: <m>\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"BND\"\r\n\r\npreamble\r\n--BND\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nHi there\r\n--BND\r\nContent-Type: application/octet-stream\r\nContent-Transfer-Encoding: base64\r\nContent-Disposition: attachment; filename=\"x.bin\"\r\n\r\nSGVsbG8=\r\n--BND--\r\nepilogue\r\n";

    #[test]
    fn detects_a_message_and_rejects_prose() {
        assert!(detect(SIMPLE, Limits::DEFAULT));
        assert!(detect(MULTIPART, Limits::DEFAULT));
        // Prose without a header block is Opaque.
        assert!(!detect(b"just a paragraph of prose.\n", Limits::DEFAULT));
        // A colon-bearing note without a blank-line body separator is Opaque.
        assert!(!detect(b"From: Alpha to Beta\n", Limits::DEFAULT));
        // A header block without From/Date/Message-ID/MIME-Version is Opaque.
        assert!(!detect(b"X-Other: 1\nX-More: 2\n\nbody\n", Limits::DEFAULT));
    }

    #[test]
    fn header_order_and_duplicates_are_preserved() {
        let doc = b"Received: one\r\nReceived: two\r\nFrom: a@b\r\nDate: x\r\nMessage-ID: <m>\r\n\r\nbody\r\n";
        let m = parse(doc, Limits::DEFAULT).unwrap();
        let root = m.part(0).unwrap();
        let received: Vec<&str> = root
            .headers
            .iter()
            .filter(|h| h.name_is("received"))
            .map(|h| h.value.as_str())
            .collect();
        assert_eq!(received, vec!["one", "two"]);
        assert_eq!(root.headers[0].name, "Received");
        // The exact folded value span is retained.
        let h = &root.headers[0];
        assert_eq!(&doc[h.value_start as usize..h.value_end as usize], b" one");
    }

    #[test]
    fn multipart_tree_and_base64_decode_exactly() {
        let m = parse(MULTIPART, Limits::DEFAULT).unwrap();
        let root = m.part(0).unwrap();
        assert_eq!(root.kind, K_MULTIPART);
        assert_eq!(root.boundary.as_deref(), Some("BND"));
        assert_eq!(root.children.len(), 2);
        let text = m.part(root.children[0]).unwrap();
        assert_eq!(text.media_type, "text/plain");
        assert_eq!(text.kind, K_TEXT);
        assert_eq!(
            body_text(&m, MULTIPART, Limits::DEFAULT).unwrap(),
            "Hi there"
        );
        let att = m.part(root.children[1]).unwrap();
        assert_eq!(att.cte, CTE_BASE64);
        assert_eq!(att.filename.as_deref(), Some("x.bin"));
        assert_eq!(
            decode_body(MULTIPART, att, Limits::DEFAULT).unwrap(),
            b"Hello"
        );
        let atts = m.attachment_indices();
        assert_eq!(atts, vec![2]);
    }

    #[test]
    fn quoted_printable_decodes_exactly() {
        let doc = b"From: a@b\r\nDate: x\r\nMessage-ID: <m>\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\nCaf=C3=A9 =\r\nsoft";
        let m = parse(doc, Limits::DEFAULT).unwrap();
        let root = m.part(0).unwrap();
        assert_eq!(root.cte, CTE_QP);
        assert_eq!(
            decode_body(doc, root, Limits::DEFAULT).unwrap(),
            "Café soft".as_bytes()
        );
    }

    #[test]
    fn nested_message_rfc822_recurses() {
        let inner = b"From: inner@b\r\nDate: y\r\nMessage-ID: <inner>\r\n\r\ninner body\r\n";
        let mut doc = Vec::new();
        doc.extend_from_slice(b"From: outer@b\r\nDate: x\r\nMessage-ID: <outer>\r\nContent-Type: message/rfc822\r\n\r\n");
        doc.extend_from_slice(inner);
        let m = parse(&doc, Limits::DEFAULT).unwrap();
        assert_eq!(m.parts.len(), 2);
        assert_eq!(m.part(0).unwrap().kind, K_MESSAGE_RFC822);
        assert_eq!(m.part(1).unwrap().depth, 1);
        assert_eq!(m.part(1).unwrap().media_type, "text/plain");
    }

    #[test]
    fn declines_typed_on_bad_inputs() {
        assert_eq!(
            parse(b"not a message at all", Limits::DEFAULT)
                .unwrap_err()
                .class(),
            ErrorClass::InvalidEmlStructure
        );
        // A multipart without a boundary declines typed.
        let bad = b"From: a@b\r\nDate: x\r\nMessage-ID: <m>\r\nContent-Type: multipart/mixed\r\n\r\nx\r\n";
        assert_eq!(
            parse(bad, Limits::DEFAULT).unwrap_err().class(),
            ErrorClass::InvalidEmlStructure
        );
        // An unknown transfer encoding declines typed on decode, not parse.
        let weird = b"From: a@b\r\nDate: x\r\nMessage-ID: <m>\r\nContent-Transfer-Encoding: x-uuencode\r\n\r\nx\r\n";
        let m = parse(weird, Limits::DEFAULT).unwrap();
        assert_eq!(
            decode_body(weird, m.part(0).unwrap(), Limits::DEFAULT)
                .unwrap_err()
                .class(),
            ErrorClass::UnsupportedFeature
        );
        // A non-UTF-8 charset declines typed for text.
        let latin = b"From: a@b\r\nDate: x\r\nMessage-ID: <m>\r\nContent-Type: text/plain; charset=iso-8859-1\r\n\r\ncaf\xe9\r\n";
        let m = parse(latin, Limits::DEFAULT).unwrap();
        assert_eq!(
            body_text(&m, latin, Limits::DEFAULT).unwrap_err().class(),
            ErrorClass::UnsupportedFeature
        );
        // Encrypted S/MIME declines typed.
        let enc = b"From: a@b\r\nDate: x\r\nMessage-ID: <m>\r\nContent-Type: multipart/encrypted; boundary=B\r\n\r\n--B\r\n\r\nx\r\n--B--\r\n";
        let m = parse(enc, Limits::DEFAULT).unwrap();
        assert_eq!(m.part(0).unwrap().kind, K_ENCRYPTED);
        assert_eq!(
            body_text(&m, enc, Limits::DEFAULT).unwrap_err().class(),
            ErrorClass::UnsupportedFeature
        );
    }

    #[test]
    fn model_roundtrips() {
        for doc in [SIMPLE, MULTIPART] {
            let m = parse(doc, Limits::DEFAULT).unwrap();
            let bytes = m.encode();
            assert_eq!(EmlModel::decode(&bytes).unwrap(), m);
            for cut in 0..bytes.len() {
                let _ = EmlModel::decode(&bytes[..cut]);
            }
        }
    }

    #[test]
    fn find_spans_headers_and_body() {
        let m = parse(SIMPLE, Limits::DEFAULT).unwrap();
        let hits = find(&m, SIMPLE, "Alice", Limits::DEFAULT).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].location, M_HEADER_VALUE);
        assert_eq!(hits[0].name, "From");
        let hits = find(&m, SIMPLE, "world", Limits::DEFAULT).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].location, M_BODY);
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0xE31A_1357_9BDF_0246;
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
                let _ = body_text(&m, &buf, Limits::STRICT);
                let _ = find(&m, &buf, "a", Limits::STRICT);
                for p in &m.parts {
                    let _ = decode_body(&buf, p, Limits::STRICT);
                }
                let _ = m.encode();
            }
            let _ = build_eml_model(&buf, Limits::STRICT);
        }
    }
}
