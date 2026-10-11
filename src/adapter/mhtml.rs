//! Bounded, representation-preserving MHTML (MIME HTML) adapter (Phase 21.27).
//!
//! MHTML (a "web archive" / `.mht` / `.mhtml` file, RFC 2557) is a MIME
//! `multipart/related` document whose root part is an HTML page and whose remaining
//! parts are the page's sub-resources (images, stylesheets, scripts, fonts) linked
//! by `Content-Location` / `Content-ID`. It is therefore **not a new container**: it
//! **reuses the bounded MIME layer** of the [`EML adapter`](crate::adapter::eml) for
//! the envelope, the ordered parts, their headers, and their exact spans, and it
//! **reuses the bounded, error-recovering HTML scanner** of the
//! [`HTML adapter`](crate::adapter::html) for the root part — never a second MIME or
//! HTML parser.
//!
//! Like every other Wave-2 format MHTML has no package layer (there is no OPC/ZIP
//! container): the exact leaf is the **whole source** (a `DocumentExact`, a RAW-like
//! authority) and everything this module produces is a bounded, deterministic
//! (`Q_gen`) projection that never sits on the exactness path.
//!
//! ## Representation preservation
//!
//! Because the model embeds the complete, span-preserving [`EmlModel`], MHTML
//! preserves, for the envelope and for every part:
//!
//! * the top-level MIME envelope (`MIME-Version`, `Content-Type:
//!   multipart/related; boundary=…`, and the optional MHTML envelope headers
//!   `From`/`Subject`/`Date`/`Snapshot-Content-Location`/`Content-Base`);
//! * every part's headers (`Content-Type`, `Content-Transfer-Encoding` — including
//!   `quoted-printable` and `base64` — `Content-Location`, `Content-ID`,
//!   `Content-Base`, …) with their **exact name/value/full spans**, in source order,
//!   duplicates kept distinct;
//! * every part's **exact entity/header/body spans**, so a `Content-Transfer-
//!   Encoding` value is decoded only for a derived observation and the encoded form
//!   is never substituted for it (and vice versa);
//! * the ordered **sub-resource** table (each resource keyed by its
//!   `Content-Location`/`Content-ID`, in document order) with the exact header spans
//!   that carry those keys and its exact body span;
//! * the **root HTML part** identified by the `multipart/related` `start=`
//!   parameter when present, else the first `text/html` part, else the first part.
//!   The embedded [`HtmlModel`] is the bounded, span-preserving parse of the root
//!   part's **decoded** body; its spans are relative to that decoded body (recorded
//!   as `root_body_decoded_len`), never re-encoded into the exact leaf.
//!
//! ## Detection (semantic sub-detection — the critical part)
//!
//! MHTML's physical bytes are a MIME message, so a plain MIME message must not be
//! stolen. Detection is therefore run **before** the generic EML detector and
//! requires, on top of a `multipart/related` MIME root, an **MHTML-specific**
//! signal:
//!
//! * a top-level `Snapshot-Content-Location` or `Content-Base`; **or**
//! * a `From:`/`Subject:` MHTML envelope **and** a `multipart/related` document that
//!   carries a `text/html` part.
//!
//! ### What the detector cannot distinguish (recorded honestly)
//!
//! * A plain `multipart/related` **email** (e.g. a `multipart/related` message with a
//!   `text/html` and an inline image, but no `Snapshot-Content-Location`/
//!   `Content-Base` and no `From`+`Subject` envelope) is **not** claimed as MHTML: it
//!   declines here and is claimed by the EML detector, staying `Eml`.
//! * A plain HTML document (no MIME envelope) declines here and stays `Html`.
//! * A plain MIME message (not `multipart/related`) declines here and stays `Eml`.
//! * A source whose root part cannot be decoded/parsed as UTF-8 HTML declines typed
//!   (MHTML declines); the detection gate does not build the HTML model, so a source
//!   that is admitted by detection but whose root part is unparseable only declines
//!   at observation time — this is stated, not hidden.
//! * The `start=` parameter is matched against a part's `Content-ID` after angle
//!   brackets are stripped; a `start=` value that names no part falls back to the
//!   first `text/html` part, then to the first part.
//! * **Content-Base / relative-URL resolution is not performed**: the adapter
//!   records the raw `Content-Location`/`Content-Base`/`Content-ID` spellings and
//!   never rewrites or resolves a sub-resource URL.
//!
//! ## Bounds and no panics
//!
//! Untrusted input can only ever yield a typed decline, never a panic or an
//! unbounded allocation: the MIME part count by [`Limits::max_mhtml_parts`], the
//! header count by [`Limits::max_mhtml_headers`], the MIME depth by
//! [`Limits::max_mhtml_depth`], the sub-resource count by
//! [`Limits::max_mhtml_resources`], the total decoded bytes by
//! [`Limits::max_mhtml_decoded_bytes`], the embedded HTML node count by
//! [`Limits::max_mhtml_nodes`], and the source length by
//! [`Limits::max_mhtml_document_bytes`] (the reused MIME and HTML parsers enforce
//! their own caps as well).

use crate::adapter::eml::{self, EmlModel, EmlPart};
use crate::adapter::html::{self, HtmlModel};
use crate::error::{Error, ErrorClass, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model resources (defends the decoder against a hostile blob).
pub const MAX_MODEL_RESOURCES: u32 = 1 << 20;

/// The `multipart/related` media type that roots an MHTML document.
pub const M_RELATED: &str = "multipart/related";
/// The `text/html` media type of the root part.
pub const M_HTML: &str = "text/html";

/// Signal flag: the envelope carries a `Snapshot-Content-Location` header.
pub const F_HAS_SNAPSHOT_LOCATION: u8 = 1;
/// Signal flag: the envelope carries a top-level `Content-Base` header.
pub const F_HAS_CONTENT_BASE: u8 = 2;
/// Signal flag: the envelope carries a `From` header.
pub const F_HAS_FROM: u8 = 4;
/// Signal flag: the envelope carries a `Subject` header.
pub const F_HAS_SUBJECT: u8 = 8;
/// Signal flag: the document carries at least one `text/html` part.
pub const F_HAS_HTML_PART: u8 = 16;

/// One exact source span `[start, end)`.
pub type Span = (u64, u64);

/// One MHTML sub-resource: a MIME part (other than the root `text/html` part) that
/// carries a `Content-Location` and/or a `Content-ID`, with the exact spans of the
/// headers that key it and its exact raw body span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MhtmlResource {
    /// The 0-based MIME part index (into the embedded [`EmlModel`]).
    pub index: u32,
    /// The 0-based sub-resource ordinal in document order.
    pub ordinal: u32,
    /// The decoded `Content-Location` value, if any.
    pub location: Option<String>,
    /// The exact value span of the `Content-Location` header, if any.
    pub location_span: Option<Span>,
    /// The decoded `Content-ID` value, if any.
    pub content_id: Option<String>,
    /// The exact value span of the `Content-ID` header, if any.
    pub content_id_span: Option<Span>,
    /// The decoded `Content-Base` value, if any.
    pub content_base: Option<String>,
    /// The exact value span of the `Content-Base` header, if any.
    pub content_base_span: Option<Span>,
    /// The lower-cased media type of the resource part.
    pub media_type: String,
    /// The content-transfer-encoding tag.
    pub cte: u8,
    /// The exact first byte of the resource part's body.
    pub body_start: u64,
    /// One past the resource part's body.
    pub body_end: u64,
    /// The decoded byte length of the resource body (`0` when it declines).
    pub decoded_len: u64,
}

/// The canonical derived MHTML model (the materialization of an `MhtmlModel` node).
///
/// It embeds the complete, span-preserving [`EmlModel`] (the MIME envelope and every
/// part) and the span-preserving [`HtmlModel`] of the root part's decoded body, plus
/// the MHTML-specific anchors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MhtmlModel {
    /// The exact source length the MIME spans are relative to.
    pub doc_len: u64,
    /// The `F_*` MHTML signal flags observed.
    pub flags: u8,
    /// The reused, complete MIME model of the same source (exact spans).
    pub eml: EmlModel,
    /// The 0-based MIME part index of the selected root HTML part.
    pub root_part: u32,
    /// The decoded byte length of the root part's body (the HTML span basis).
    pub root_body_decoded_len: u64,
    /// The HTML model of the root part's **decoded** body (spans relative to it).
    pub html: HtmlModel,
    /// The decoded `start=` parameter of the root `Content-Type`, if any.
    pub start_param: Option<String>,
    /// The decoded `type=` parameter of the root `Content-Type`, if any.
    pub type_param: Option<String>,
    /// The decoded top-level `Snapshot-Content-Location` value, if any.
    pub snapshot_location: Option<String>,
    /// The decoded top-level `Content-Base` value, if any.
    pub content_base: Option<String>,
    /// The sub-resources (parts with a `Content-Location`/`Content-ID`), in document
    /// order.
    pub resources: Vec<MhtmlResource>,
}

impl MhtmlModel {
    /// Whether the flags carry an MHTML-specific signal: a top-level
    /// `Snapshot-Content-Location`/`Content-Base`, or a `From`+`Subject` envelope with
    /// a `text/html` part.
    pub fn has_mhtml_signal(&self) -> bool {
        let envelope =
            self.flags & F_HAS_SNAPSHOT_LOCATION != 0 || self.flags & F_HAS_CONTENT_BASE != 0;
        let mhtml_envelope = self.flags & F_HAS_FROM != 0
            && self.flags & F_HAS_SUBJECT != 0
            && self.flags & F_HAS_HTML_PART != 0;
        envelope || mhtml_envelope
    }

    /// The root MIME part, if present.
    pub fn root_part(&self) -> Option<&EmlPart> {
        self.eml.part(self.root_part)
    }

    /// The sub-resource at `ordinal`, if present.
    pub fn resource(&self, ordinal: u32) -> Option<&MhtmlResource> {
        self.resources.get(ordinal as usize)
    }

    /// Whether the envelope carries a `Snapshot-Content-Location` header.
    pub fn has_snapshot_location(&self) -> bool {
        self.flags & F_HAS_SNAPSHOT_LOCATION != 0
    }

    /// Whether the envelope carries a top-level `Content-Base` header.
    pub fn has_content_base(&self) -> bool {
        self.flags & F_HAS_CONTENT_BASE != 0
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let eml = self.eml.encode();
        let html = self.html.encode();
        let mut out = Vec::with_capacity(64 + eml.len() + html.len() + self.resources.len() * 96);
        out.extend_from_slice(b"MHT1");
        out.push(MODEL_VERSION);
        out.push(self.flags);
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&(eml.len() as u32).to_le_bytes());
        out.extend_from_slice(&eml);
        out.extend_from_slice(&self.root_part.to_le_bytes());
        out.extend_from_slice(&self.root_body_decoded_len.to_le_bytes());
        out.extend_from_slice(&(html.len() as u32).to_le_bytes());
        out.extend_from_slice(&html);
        put_opt(&mut out, self.start_param.as_deref());
        put_opt(&mut out, self.type_param.as_deref());
        put_opt(&mut out, self.snapshot_location.as_deref());
        put_opt(&mut out, self.content_base.as_deref());
        out.extend_from_slice(&(self.resources.len() as u32).to_le_bytes());
        for r in &self.resources {
            out.extend_from_slice(&r.index.to_le_bytes());
            out.extend_from_slice(&r.ordinal.to_le_bytes());
            put_opt(&mut out, r.location.as_deref());
            put_span(&mut out, r.location_span);
            put_opt(&mut out, r.content_id.as_deref());
            put_span(&mut out, r.content_id_span);
            put_opt(&mut out, r.content_base.as_deref());
            put_span(&mut out, r.content_base_span);
            encode_str(&mut out, &r.media_type);
            out.push(r.cte);
            out.extend_from_slice(&r.body_start.to_le_bytes());
            out.extend_from_slice(&r.body_end.to_le_bytes());
            out.extend_from_slice(&r.decoded_len.to_le_bytes());
        }
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<MhtmlModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"MHT1" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let flags = r.u8()?;
        let doc_len = r.u64()?;
        let eml_len = r.u32()? as usize;
        let eml_bytes = r.bytes(eml_len)?;
        let eml = EmlModel::decode(eml_bytes)?;
        if eml.doc_len != doc_len {
            return Err(corrupt("embedded MIME model length mismatch"));
        }
        let root_part = r.u32()?;
        if eml.part(root_part).is_none() {
            return Err(corrupt("root part index is out of range"));
        }
        let root_body_decoded_len = r.u64()?;
        let html_len = r.u32()? as usize;
        let html_bytes = r.bytes(html_len)?;
        let html = HtmlModel::decode(html_bytes)?;
        if html.doc_len != root_body_decoded_len {
            return Err(corrupt("embedded HTML model length mismatch"));
        }
        let start_param = r.opt_str()?;
        let type_param = r.opt_str()?;
        let snapshot_location = r.opt_str()?;
        let content_base = r.opt_str()?;
        let count = r.u32()?;
        if count > MAX_MODEL_RESOURCES {
            return Err(corrupt("model resource count is implausible"));
        }
        let mut resources = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let index = r.u32()?;
            if eml.part(index).is_none() {
                return Err(corrupt("resource part index is out of range"));
            }
            let ordinal = r.u32()?;
            let location = r.opt_str()?;
            let location_span = r.span(doc_len)?;
            let content_id = r.opt_str()?;
            let content_id_span = r.span(doc_len)?;
            let content_base = r.opt_str()?;
            let content_base_span = r.span(doc_len)?;
            let media_type = r.string()?;
            let cte = r.u8()?;
            let body_start = r.u64()?;
            let body_end = r.u64()?;
            if body_start > body_end || body_end > doc_len {
                return Err(corrupt("resource body span is outside the document"));
            }
            let decoded_len = r.u64()?;
            resources.push(MhtmlResource {
                index,
                ordinal,
                location,
                location_span,
                content_id,
                content_id_span,
                content_base,
                content_base_span,
                media_type,
                cte,
                body_start,
                body_end,
                decoded_len,
            });
        }
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        Ok(MhtmlModel {
            doc_len,
            flags,
            eml,
            root_part,
            root_body_decoded_len,
            html,
            start_param,
            type_param,
            snapshot_location,
            content_base,
            resources,
        })
    }
}

fn corrupt(msg: &str) -> Error {
    Error::invalid_mhtml_structure(format!("malformed MHTML model: {msg}"))
}

fn encode_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn put_opt(out: &mut Vec<u8>, s: Option<&str>) {
    match s {
        Some(s) => {
            out.push(1);
            encode_str(out, s);
        }
        None => out.push(0),
    }
}

fn put_span(out: &mut Vec<u8>, s: Option<Span>) {
    match s {
        Some((a, b)) => {
            out.push(1);
            out.extend_from_slice(&a.to_le_bytes());
            out.extend_from_slice(&b.to_le_bytes());
        }
        None => out.push(0),
    }
}

/// The empty HTML model used by a detection-only parse (no HTML build).
fn empty_html() -> HtmlModel {
    HtmlModel {
        root: u32::MAX,
        max_depth: 0,
        doc_len: 0,
        nodes: Vec::new(),
        attrs: Vec::new(),
        top: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Detection / parse
// ---------------------------------------------------------------------------

/// Byte-based, conservative MHTML detector. See the module docs for the exact
/// semantic boundary.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    parse(source, limits, false).is_ok()
}

/// Parse `source` into an [`MhtmlModel`]. `build` selects whether the root HTML model
/// and the sub-resource table are built (detection runs with `build = false` to stay
/// cheap); parsing the MIME envelope and the signal test are always done.
pub fn parse(source: &[u8], limits: Limits, build: bool) -> Result<MhtmlModel> {
    if source.len() as u64 > limits.max_mhtml_document_bytes {
        return Err(Error::resource_limit(format!(
            "MHTML source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_mhtml_document_bytes
        )));
    }
    // Reuse the bounded MIME layer. A source that is not a well-formed RFC 5322 +
    // MIME message is a typed decline.
    let eml = eml::parse(source, limits).map_err(|e| match e.class() {
        ErrorClass::ResourceLimit => e,
        _ => invalid_mhtml(format!("not MHTML: {}", e.message())),
    })?;
    enforce_mime_caps(&eml, limits)?;
    let root = eml
        .part(0)
        .ok_or_else(|| invalid_mhtml("MHTML has no MIME root part"))?;
    if root.media_type != M_RELATED {
        return Err(invalid_mhtml(format!(
            "MHTML root is not multipart/related (found {})",
            root.media_type
        )));
    }
    let ct_raw = root.last_header("content-type").map(|h| h.value.as_str());
    let start_param = param_value(ct_raw, "start");
    let type_param = param_value(ct_raw, "type");

    let snapshot_location = root
        .last_header("snapshot-content-location")
        .map(|h| h.value.clone());
    let content_base = root.last_header("content-base").map(|h| h.value.clone());
    let mut flags = 0u8;
    if snapshot_location.is_some() {
        flags |= F_HAS_SNAPSHOT_LOCATION;
    }
    if content_base.is_some() {
        flags |= F_HAS_CONTENT_BASE;
    }
    if root.last_header("from").is_some() {
        flags |= F_HAS_FROM;
    }
    if root.last_header("subject").is_some() {
        flags |= F_HAS_SUBJECT;
    }
    if eml
        .parts
        .iter()
        .skip(1)
        .any(|p| p.is_leaf() && p.media_type == M_HTML)
    {
        flags |= F_HAS_HTML_PART;
    }
    if flags & (F_HAS_SNAPSHOT_LOCATION | F_HAS_CONTENT_BASE) == 0
        && !(flags & F_HAS_FROM != 0 && flags & F_HAS_SUBJECT != 0 && flags & F_HAS_HTML_PART != 0)
    {
        return Err(invalid_mhtml(
            "multipart/related without an MHTML signal (no Snapshot-Content-Location/\
             Content-Base and no From+Subject envelope with a text/html part)",
        ));
    }

    let root_part = select_root_part(&eml, start_param.as_deref())
        .ok_or_else(|| invalid_mhtml("MHTML has no addressable root part"))?;

    if !build {
        return Ok(MhtmlModel {
            doc_len: source.len() as u64,
            flags,
            eml,
            root_part,
            root_body_decoded_len: 0,
            html: empty_html(),
            start_param,
            type_param,
            snapshot_location,
            content_base,
            resources: Vec::new(),
        });
    }

    // Decode the root part's body and parse it with the reused HTML scanner. The
    // decoded body is the basis of the embedded `HtmlModel`'s spans; the raw body
    // span stays available through the embedded MIME part.
    let root_body = eml::decode_body(source, eml.part(root_part).unwrap(), limits)?;
    let root_body_decoded_len = root_body.len() as u64;
    let html = html::parse(&root_body, limits, true)?;
    if html.nodes.len() as u64 > u64::from(limits.max_mhtml_nodes) {
        return Err(Error::resource_limit(format!(
            "MHTML root HTML exceeds the {}-node cap",
            limits.max_mhtml_nodes
        )));
    }

    let resources = build_resources(source, &eml, root_part, limits)?;
    let total_decoded = resources.iter().fold(root_body_decoded_len, |acc, r| {
        acc.saturating_add(r.decoded_len)
    });
    if total_decoded > limits.max_mhtml_decoded_bytes {
        return Err(Error::resource_limit(format!(
            "MHTML decodes to {total_decoded} bytes, above the {}-byte decoded cap",
            limits.max_mhtml_decoded_bytes
        )));
    }

    Ok(MhtmlModel {
        doc_len: source.len() as u64,
        flags,
        eml,
        root_part,
        root_body_decoded_len,
        html,
        start_param,
        type_param,
        snapshot_location,
        content_base,
        resources,
    })
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of an `MhtmlModel` node).
pub fn build_mhtml_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits, true)?.encode())
}

/// Enforce the MHTML-specific MIME caps on top of the reused MIME parser.
fn enforce_mime_caps(eml: &EmlModel, limits: Limits) -> Result<()> {
    if eml.parts.len() as u64 > u64::from(limits.max_mhtml_parts) {
        return Err(Error::resource_limit(format!(
            "MHTML document exceeds the {}-part cap",
            limits.max_mhtml_parts
        )));
    }
    if eml.header_count as u64 > u64::from(limits.max_mhtml_headers) {
        return Err(Error::resource_limit(format!(
            "MHTML document exceeds the {}-header cap",
            limits.max_mhtml_headers
        )));
    }
    if eml.max_depth > limits.max_mhtml_depth {
        return Err(Error::resource_limit(format!(
            "MHTML MIME nesting exceeds the {}-deep cap",
            limits.max_mhtml_depth
        )));
    }
    Ok(())
}

/// Select the root HTML part: the `start=` `Content-ID`, else the first `text/html`
/// part, else the first part.
fn select_root_part(eml: &EmlModel, start: Option<&str>) -> Option<u32> {
    if let Some(start) = start {
        let want = trim_id(start);
        if !want.is_empty() {
            for p in eml.parts.iter().skip(1) {
                if let Some(h) = p.last_header("content-id")
                    && trim_id(&h.value) == want
                {
                    return Some(p.index);
                }
            }
        }
    }
    if let Some(p) = eml
        .parts
        .iter()
        .skip(1)
        .find(|p| p.is_leaf() && p.media_type == M_HTML)
    {
        return Some(p.index);
    }
    eml.parts.iter().skip(1).map(|p| p.index).next()
}

/// Build the ordered sub-resource table from the MIME parts.
fn build_resources(
    source: &[u8],
    eml: &EmlModel,
    root_part: u32,
    limits: Limits,
) -> Result<Vec<MhtmlResource>> {
    let mut out: Vec<MhtmlResource> = Vec::new();
    let mut total_decoded: u64 = 0;
    for p in eml.parts.iter().skip(1) {
        if p.index == root_part {
            continue;
        }
        let loc = p.last_header("content-location");
        let cid = p.last_header("content-id");
        let cbase = p.last_header("content-base");
        if loc.is_none() && cid.is_none() {
            continue;
        }
        if out.len() as u64 >= u64::from(limits.max_mhtml_resources) {
            return Err(Error::resource_limit(format!(
                "MHTML document exceeds the {}-resource cap",
                limits.max_mhtml_resources
            )));
        }
        let decoded_len = match eml::decode_body(source, p, limits) {
            Ok(b) => b.len() as u64,
            Err(e) if e.class() == ErrorClass::ResourceLimit => return Err(e),
            Err(_) => 0,
        };
        total_decoded = total_decoded.saturating_add(decoded_len);
        if total_decoded > limits.max_mhtml_decoded_bytes {
            return Err(Error::resource_limit(format!(
                "MHTML resources decode above the {}-byte decoded cap",
                limits.max_mhtml_decoded_bytes
            )));
        }
        let ordinal = out.len() as u32;
        out.push(MhtmlResource {
            index: p.index,
            ordinal,
            location: loc.map(|h| h.value.clone()),
            location_span: loc.map(|h| (h.value_start, h.value_end)),
            content_id: cid.map(|h| h.value.clone()),
            content_id_span: cid.map(|h| (h.value_start, h.value_end)),
            content_base: cbase.map(|h| h.value.clone()),
            content_base_span: cbase.map(|h| (h.value_start, h.value_end)),
            media_type: p.media_type.clone(),
            cte: p.cte,
            body_start: p.body_start,
            body_end: p.body_end,
            decoded_len,
        });
    }
    Ok(out)
}

/// Extract a `name=value` parameter from a header value, unquoting the value.
fn param_value(raw: Option<&str>, param: &str) -> Option<String> {
    let raw = raw?;
    for seg in raw.split(';').skip(1) {
        let Some((k, v)) = seg.split_once('=') else {
            continue;
        };
        if k.trim().eq_ignore_ascii_case(param) {
            let v = v.trim();
            let v = if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
                &v[1..v.len() - 1]
            } else {
                v
            };
            return Some(v.to_string());
        }
    }
    None
}

/// Trim surrounding whitespace and a single level of `<…>` from a `Content-ID`.
fn trim_id(s: &str) -> &str {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('<') && s.ends_with('>') {
        s[1..s.len() - 1].trim()
    } else {
        s
    }
}

/// The exact decoded bytes of the selected root part's body.
pub fn root_body_bytes(source: &[u8], model: &MhtmlModel, limits: Limits) -> Result<Vec<u8>> {
    let p = model
        .root_part()
        .ok_or_else(|| corrupt("root part index is out of range"))?;
    eml::decode_body(source, p, limits)
}

/// The exact decoded bytes of the sub-resource at `ordinal`.
pub fn resource_bytes(
    source: &[u8],
    model: &MhtmlModel,
    ordinal: u32,
    limits: Limits,
) -> Result<Vec<u8>> {
    let r = model.resource(ordinal).ok_or_else(|| {
        Error::unsupported_feature(format!(
            "MHTML has no resource {ordinal} (resource count {})",
            model.resources.len()
        ))
    })?;
    let p = model
        .eml
        .part(r.index)
        .ok_or_else(|| corrupt("resource part index is out of range"))?;
    eml::decode_body(source, p, limits)
}

/// A bounded lexical search over the whole document (reusing the MIME finder over the
/// embedded [`EmlModel`] — every part's header names/values and every decoded
/// `text/*` body, including the root HTML part).
pub fn find(
    source: &[u8],
    model: &MhtmlModel,
    pattern: &str,
    limits: Limits,
) -> Result<Vec<eml::EmlMatch>> {
    eml::find(&model.eml, source, pattern, limits)
}

fn invalid_mhtml(message: impl Into<String>) -> Error {
    Error::invalid_mhtml_structure(message)
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
            .ok_or_else(|| corrupt("model offset overflow"))?;
        let s = self
            .b
            .get(self.at..end)
            .ok_or_else(|| corrupt("model is truncated"))?;
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
        let b = self.bytes(n)?;
        String::from_utf8(b.to_vec()).map_err(|_| corrupt("model string is not UTF-8"))
    }

    fn opt_str(&mut self) -> Result<Option<String>> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.string()?)),
            _ => Err(corrupt("bad optional-string flag")),
        }
    }

    fn span(&mut self, doc_len: u64) -> Result<Option<Span>> {
        match self.u8()? {
            0 => Ok(None),
            1 => {
                let a = self.u64()?;
                let b = self.u64()?;
                if a > b || b > doc_len {
                    return Err(corrupt("span is outside the document"));
                }
                Ok(Some((a, b)))
            }
            _ => Err(corrupt("bad optional-span flag")),
        }
    }

    fn at_end(&self) -> bool {
        self.at == self.b.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &[u8] = b"From: <Saved by Blink>\r\nSubject: Example\r\nDate: Mon, 01 Jan 2029 00:00:00 +0000\r\nMIME-Version: 1.0\r\nContent-Type: multipart/related; boundary=\"B\"; type=\"text/html\"; start=\"<root@x>\"\r\nSnapshot-Content-Location: http://example.com/\r\n\r\n--B\r\nContent-Type: text/html; charset=utf-8\r\nContent-Transfer-Encoding: quoted-printable\r\nContent-Location: http://example.com/\r\nContent-ID: <root@x>\r\n\r\n<html><body><p>Caf=C3=A9</p><img src=3D\"i.png\"></body></html>\r\n--B\r\nContent-Type: image/png\r\nContent-Transfer-Encoding: base64\r\nContent-Location: http://example.com/i.png\r\n\r\nAAAA\r\n--B--\r\n";

    #[test]
    fn detects_mhtml_and_rejects_other_formats() {
        assert!(detect(DOC, Limits::DEFAULT));
        // A plain multipart/related email without the MHTML markers is not MHTML.
        let plain: &[u8] = b"From: a@b\r\nSubject: hi\r\nMIME-Version: 1.0\r\nContent-Type: multipart/related; boundary=\"x\"\r\n\r\n--x\r\nContent-Type: text/plain\r\n\r\nyo\r\n--x--\r\n";
        assert!(!detect(plain, Limits::DEFAULT));
        // A plain HTML document is not MHTML (no MIME envelope).
        assert!(!detect(
            b"<!doctype html><html><body>hi</body></html>",
            Limits::DEFAULT
        ));
    }

    #[test]
    fn model_roundtrips() {
        let m = parse(DOC, Limits::DEFAULT, true).unwrap();
        assert_eq!(m.root_part, 1);
        assert_eq!(m.resources.len(), 1);
        assert_eq!(
            m.resources[0].location.as_deref(),
            Some("http://example.com/i.png")
        );
        assert_eq!(m.snapshot_location.as_deref(), Some("http://example.com/"));
        assert_eq!(m.start_param.as_deref(), Some("<root@x>"));
        assert!(m.html.nodes.len() >= 2);
        let bytes = m.encode();
        let back = MhtmlModel::decode(&bytes).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0x2127_5EED_C0DE_0001;
        for _ in 0..128 {
            let mut buf = vec![0u8; 768];
            for b in buf.iter_mut() {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                *b = (x & 0xFF) as u8;
            }
            if let Ok(m) = parse(&buf, Limits::STRICT, true) {
                let _ = m.encode();
                let _ = find(&buf, &m, "a", Limits::STRICT);
            }
        }
    }
}
