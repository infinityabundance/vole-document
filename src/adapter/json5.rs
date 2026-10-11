//! Bounded, representation-preserving JSON5 / JSONC adapter (Phase 21.17.1).
//!
//! JSON5 (and its tsconfig-style "JSON with Comments" subset, JSONC) is the
//! **structured-extra** Wave-2 format. Like the other Wave-2 formats it is *not* an
//! office package: there is no OPC/ZIP layer, no `mimetype`, and no relationship
//! graph. The exact leaf is therefore the **whole source** (a `DocumentExact`, a
//! RAW-like authority), and everything this module produces is a bounded,
//! deterministic (`Q_gen`) projection that never sits on the exactness path.
//!
//! ## Reuse, not a second node arena
//!
//! JSON5 is an ECMAScript 5.1 **superset** of JSON. Rather than invent a second node
//! arena, this adapter produces the **same** [`crate::adapter::json::JNode`] arena
//! (kind, exact `[start, end)` token span, ordered children) the JSON adapter
//! defines, so member **order**, **duplicate keys**, numeric/escape **spelling**, and
//! exact token spans carry the identical guarantees — with two additional node
//! kinds: an unquoted object key is [`K_UNQUOTED`] (its span is the bare
//! IdentifierName), and a string token may be single- or double-quoted (the span
//! preserves which).
//!
//! ## What is preserved
//!
//! * `//` line comments and `/* … */` block comments — their exact spans are
//!   recorded in [`Json5Model::comments`], **never silently dropped**;
//! * unquoted object keys (ECMAScript IdentifierName, including `$`, `_`, and
//!   Unicode letters);
//! * single-quoted strings, trailing commas in objects and arrays;
//! * numbers with a leading `+`, a leading or trailing decimal point (`.5`, `5.`),
//!   hexadecimal (`0xFF`), `Infinity`, `-Infinity`, `NaN` — every scalar's **exact
//!   spelling** is the token span, never parsed into a binary float;
//! * multi-line strings via `\` line continuation, and the additional escapes
//!   (`\x` hex, `\0`, `\v`), with their literal spelling preserved;
//! * the extended JSON5 whitespace set (VT, FF, NBSP, BOM, Unicode Zs, LS, PS).
//!
//! ## Dialect recording
//!
//! The model records a [`dialect`](Json5Model::dialect):
//!
//! * [`DIALECT_JSONC`] when the **only** extensions used are comments and/or
//!   trailing commas (tsconfig-style "JSON with Comments");
//! * [`DIALECT_JSON5`] when any other extension is used (unquoted key, single
//!   quote, hex / leading-dot / `Infinity` / `NaN`, string continuation, the
//!   extended whitespace set, …).
//!
//! A source that is **strict JSON** is therefore never reclassified: [`detect`]
//! requires at least one JSON5/JSONC-only construct, so a plain JSON document stays
//! [`crate::field::document_format::DocumentFormat::Json`].
//!
//! ## Bounds and no panics
//!
//! Untrusted input can only ever yield a typed decline, never a panic or an
//! unbounded allocation: the nesting depth by [`Limits::max_json5_depth`], the node
//! count by [`Limits::max_json5_nodes`], the raw string bytes by
//! [`Limits::max_json5_string_bytes`], the comment count by
//! [`Limits::max_json5_comments`], and the source length by
//! [`Limits::max_json5_document_bytes`].
//!
//! ## Implementation note (honest)
//!
//! ECMAScript's `IdentifierName` is defined by the Unicode `ID_Start`/`ID_Continue`
//! tables. This dependency-free module approximates `ID_Start` with
//! `char::is_alphabetic` and `ID_Continue` with `char::is_alphanumeric` (plus `$`,
//! `_`, ZWNJ, ZWJ). That is a bounded, deterministic superset/subset of the exact
//! table; it covers the letters the format exists to carry but is not a byte-exact
//! port of the Unicode tables.

use crate::adapter::json::{self, JNode};
use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model nodes (defends the decoder against a hostile blob).
pub const MAX_MODEL_NODES: u32 = 1 << 24;
/// Hard cap on decoded comment spans (defends the decoder against a hostile blob).
pub const MAX_MODEL_COMMENTS: u32 = 1 << 24;

/// JSON node kind: an object (`{ … }`).
pub const K_OBJECT: u8 = 0;
/// JSON node kind: an array (`[ … ]`).
pub const K_ARRAY: u8 = 1;
/// JSON node kind: a string (`"…"` or `'…'`).
pub const K_STRING: u8 = 2;
/// JSON node kind: a number (decimal, hex, `Infinity`, `NaN`).
pub const K_NUMBER: u8 = 3;
/// JSON node kind: the literal `true`.
pub const K_TRUE: u8 = 4;
/// JSON node kind: the literal `false`.
pub const K_FALSE: u8 = 5;
/// JSON node kind: the literal `null`.
pub const K_NULL: u8 = 6;
/// JSON node kind: an **unquoted** object key (an ECMAScript IdentifierName).
pub const K_UNQUOTED: u8 = 7;

/// Stable lower-case kind name for reports and JSON output.
pub const fn kind_name(kind: u8) -> &'static str {
    match kind {
        K_OBJECT => "object",
        K_ARRAY => "array",
        K_STRING => "string",
        K_NUMBER => "number",
        K_TRUE => "true",
        K_FALSE => "false",
        K_NULL => "null",
        K_UNQUOTED => "unquoted-key",
        _ => "unknown",
    }
}

/// Whether `kind` is a container (has children).
pub const fn is_container(kind: u8) -> bool {
    matches!(kind, K_OBJECT | K_ARRAY)
}

/// The recorded dialect: a strict-JSON source (never detected as JSON5).
pub const DIALECT_STRICT: u8 = 0;
/// The recorded dialect: the only extensions are comments and/or trailing commas.
pub const DIALECT_JSONC: u8 = 1;
/// The recorded dialect: at least one non-JSONC JSON5 extension is used.
pub const DIALECT_JSON5: u8 = 2;

/// Stable lower-case dialect name.
pub const fn dialect_name(dialect: u8) -> &'static str {
    match dialect {
        DIALECT_JSONC => "jsonc",
        DIALECT_JSON5 => "json5",
        _ => "strict",
    }
}

/// Comment kind: a `//` line comment.
pub const C_LINE: u8 = 0;
/// Comment kind: a `/* … */` block comment.
pub const C_BLOCK: u8 = 1;

/// Stable lower-case comment-kind name.
pub const fn comment_kind_name(kind: u8) -> &'static str {
    match kind {
        C_LINE => "line",
        _ => "block",
    }
}

/// One recorded comment: its kind and its exact source span (delimiters included).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Comment {
    /// `C_LINE` or `C_BLOCK`.
    pub kind: u8,
    /// The comment's first source byte.
    pub start: u64,
    /// One past the comment's last source byte.
    pub end: u64,
}

/// The canonical derived JSON5 / JSONC model (the materialization of a
/// `Json5Model` node). It reuses the JSON [`JNode`] arena so the representation
/// guarantees (order, duplicates, spelling, exact spans) are identical.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Json5Model {
    /// The recorded dialect (`DIALECT_JSONC` or `DIALECT_JSON5`).
    pub dialect: u8,
    /// Index of the root node.
    pub root: u32,
    /// The root's kind tag.
    pub top_type: u8,
    /// The observed maximum container nesting depth (root = 1).
    pub max_depth: u32,
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// The node arena (reused from the JSON adapter).
    pub nodes: Vec<JNode>,
    /// Every comment in source order.
    pub comments: Vec<Comment>,
    /// The number of trailing commas observed (objects and arrays).
    pub trailing_commas: u32,
    /// The number of unquoted object keys observed.
    pub unquoted_keys: u32,
}

impl Json5Model {
    /// The node at `index`, if present.
    pub fn node(&self, index: u32) -> Option<&JNode> {
        self.nodes.get(index as usize)
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(48 + self.nodes.len() * 24 + self.comments.len() * 16);
        out.extend_from_slice(b"JSN5");
        out.push(MODEL_VERSION);
        out.push(self.dialect);
        out.extend_from_slice(&self.root.to_le_bytes());
        out.push(self.top_type);
        out.extend_from_slice(&self.max_depth.to_le_bytes());
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&self.trailing_commas.to_le_bytes());
        out.extend_from_slice(&self.unquoted_keys.to_le_bytes());
        out.extend_from_slice(&(self.nodes.len() as u32).to_le_bytes());
        for n in &self.nodes {
            out.push(n.kind);
            out.extend_from_slice(&n.start.to_le_bytes());
            out.extend_from_slice(&n.end.to_le_bytes());
            out.extend_from_slice(&(n.children.len() as u32).to_le_bytes());
            for c in &n.children {
                out.extend_from_slice(&c.to_le_bytes());
            }
        }
        out.extend_from_slice(&(self.comments.len() as u32).to_le_bytes());
        for c in &self.comments {
            out.push(c.kind);
            out.extend_from_slice(&c.start.to_le_bytes());
            out.extend_from_slice(&c.end.to_le_bytes());
        }
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<Json5Model> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"JSN5" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let dialect = r.u8()?;
        if dialect > DIALECT_JSON5 {
            return Err(corrupt("unknown dialect"));
        }
        let root = r.u32()?;
        let top_type = r.u8()?;
        if top_type > K_UNQUOTED {
            return Err(corrupt("unknown top-level kind"));
        }
        let max_depth = r.u32()?;
        let doc_len = r.u64()?;
        let trailing_commas = r.u32()?;
        let unquoted_keys = r.u32()?;
        let count = r.u32()?;
        if count > MAX_MODEL_NODES {
            return Err(corrupt("model node count is implausible"));
        }
        let mut nodes = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let kind = r.u8()?;
            if kind > K_UNQUOTED {
                return Err(corrupt("unknown node kind"));
            }
            let start = r.u64()?;
            let end = r.u64()?;
            if start > end || end > doc_len {
                return Err(corrupt("node span is outside the document"));
            }
            let cn = r.u32()?;
            if cn as u64 > count as u64 {
                return Err(corrupt("node child count is implausible"));
            }
            let mut children = Vec::with_capacity(cn as usize);
            for _ in 0..cn {
                let c = r.u32()?;
                if c >= count {
                    return Err(corrupt("child index is out of range"));
                }
                children.push(c);
            }
            nodes.push(JNode {
                kind,
                start,
                end,
                children,
            });
        }
        let ccount = r.u32()?;
        if ccount > MAX_MODEL_COMMENTS {
            return Err(corrupt("model comment count is implausible"));
        }
        let mut comments = Vec::with_capacity(ccount as usize);
        for _ in 0..ccount {
            let kind = r.u8()?;
            if kind > C_BLOCK {
                return Err(corrupt("unknown comment kind"));
            }
            let start = r.u64()?;
            let end = r.u64()?;
            if start > end || end > doc_len {
                return Err(corrupt("comment span is outside the document"));
            }
            comments.push(Comment { kind, start, end });
        }
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        if root as usize >= nodes.len() {
            return Err(corrupt("root index is out of range"));
        }
        Ok(Json5Model {
            dialect,
            root,
            top_type,
            max_depth,
            doc_len,
            nodes,
            comments,
            trailing_commas,
            unquoted_keys,
        })
    }
}

/// A resolved pointer query: the node index and how many members matched the final
/// key segment (so duplicate keys are reported, never hidden).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolved {
    /// The resolved node index.
    pub index: u32,
    /// The number of object members with the final segment's key (`1` for an array
    /// index or the root; `>1` witnesses duplicate keys).
    pub matches: u32,
}

/// One lexical-match record from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Json5Match {
    /// Canonical RFC 6901 pointer to the matching node.
    pub pointer: String,
    /// Whether the match is an object key or a string value.
    pub role: json::MatchRole,
    /// The matching node's kind (always `K_STRING`).
    pub kind: u8,
    /// The exact source span of the matching string token.
    pub start: u64,
    /// One past the matching string token.
    pub end: u64,
    /// The decoded text of the matching string.
    pub text: String,
}

/// Fast-fail JSON5/JSONC detector: does `source` parse as exactly one JSON5 value
/// within `limits` **and** use at least one JSON5/JSONC-only construct?
///
/// Conservative by construction: a strict JSON source (which the JSON detector
/// claims first) is rejected here, so a plain JSON document is never reclassified.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_json5_document_bytes {
        return false;
    }
    match parse(source, limits, false) {
        Ok(m) => m.dialect != DIALECT_STRICT,
        Err(_) => false,
    }
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of a `Json5Model` node).
pub fn build_json5_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits, true)?.encode())
}

/// Parse `source` into a [`Json5Model`]. `build` selects whether the node/comment
/// arenas are populated (detection runs with `build = false` to stay O(1) in
/// memory).
pub fn parse(source: &[u8], limits: Limits, build: bool) -> Result<Json5Model> {
    if source.len() as u64 > limits.max_json5_document_bytes {
        return Err(Error::resource_limit(format!(
            "JSON5 source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_json5_document_bytes
        )));
    }
    let mut p = Parser::new(source, limits, build);
    p.skip_ws()?;
    if p.at >= p.b.len() {
        return Err(corrupt("empty input is not a JSON5 value"));
    }
    let root = p.value(1)?;
    p.skip_ws()?;
    if p.at != p.b.len() {
        return Err(corrupt("trailing bytes after the single JSON5 value"));
    }
    let dialect = if p.uses_json5 {
        DIALECT_JSON5
    } else if p.uses_jsonc_only {
        DIALECT_JSONC
    } else {
        DIALECT_STRICT
    };
    Ok(Json5Model {
        dialect,
        root,
        top_type: p.root_kind,
        max_depth: p.max_depth_seen,
        doc_len: source.len() as u64,
        nodes: p.nodes,
        comments: p.comments,
        trailing_commas: p.trailing_commas,
        unquoted_keys: p.unquoted_keys,
    })
}

/// The exact source bytes of a node's token (`[start, end)`), bounded by the
/// document length.
pub fn token_bytes<'a>(source: &'a [u8], node: &JNode) -> Result<&'a [u8]> {
    json::token_bytes(source, node)
}

/// Decode a JSON5 string node's token (single- or double-quoted) into its Rust
/// text (escapes resolved). Never panics; a malformed token is typed.
pub fn decode_string(source: &[u8], node: &JNode) -> Result<String> {
    let tok = token_bytes(source, node)?;
    if node.kind != K_STRING || tok.len() < 2 {
        return Err(corrupt("node is not a well-formed JSON5 string token"));
    }
    let q = tok[0];
    if (q != b'"' && q != b'\'') || tok[tok.len() - 1] != q {
        return Err(corrupt("node is not a well-formed JSON5 string token"));
    }
    unescape(&tok[1..tok.len() - 1])
}

/// Decode an object key node (quoted string or unquoted IdentifierName) to text.
pub fn decode_key(source: &[u8], node: &JNode) -> Result<String> {
    match node.kind {
        K_STRING => decode_string(source, node),
        K_UNQUOTED => Ok(String::from_utf8_lossy(token_bytes(source, node)?).into_owned()),
        _ => Err(corrupt("node is not an object key")),
    }
}

/// Render a deterministic canonical text projection of the whole model: member
/// order and token spelling (numeric/escape/quoting) are preserved, separators are
/// canonical (`:`, `,`, no insignificant whitespace; comments are not re-emitted).
pub fn canonical_text(model: &Json5Model, source: &[u8]) -> Result<String> {
    subtree_text(model, source, model.root)
}

/// Render one node's subtree to canonical text (see [`canonical_text`]).
pub fn subtree_text(model: &Json5Model, source: &[u8], index: u32) -> Result<String> {
    let mut out = String::new();
    render(model, source, index, &mut out, 0)?;
    Ok(out)
}

/// The index of a node's parent, if any (the root has none).
pub fn find_parent(model: &Json5Model, index: u32) -> Option<u32> {
    model
        .nodes
        .iter()
        .position(|n| n.children.contains(&index))
        .map(|i| i as u32)
}

fn render(
    model: &Json5Model,
    source: &[u8],
    index: u32,
    out: &mut String,
    depth: u32,
) -> Result<()> {
    if depth > MAX_MODEL_NODES {
        return Err(Error::resource_limit(
            "JSON5 render exceeded its depth bound",
        ));
    }
    let node = model
        .node(index)
        .ok_or_else(|| corrupt("render hit an out-of-range node"))?;
    match node.kind {
        K_OBJECT => {
            out.push('{');
            let mut i = 0usize;
            while i + 1 < node.children.len() {
                if i > 0 {
                    out.push(',');
                }
                render(model, source, node.children[i], out, depth + 1)?;
                out.push(':');
                render(model, source, node.children[i + 1], out, depth + 1)?;
                i += 2;
            }
            out.push('}');
        }
        K_ARRAY => {
            out.push('[');
            for (i, child) in node.children.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                render(model, source, *child, out, depth + 1)?;
            }
            out.push(']');
        }
        _ => {
            let tok = token_bytes(source, node)?;
            out.push_str(&String::from_utf8_lossy(tok));
        }
    }
    Ok(())
}

/// RFC 6901 pointer resolution against a parsed model. A missing node, an
/// out-of-range array index, or indexing into a scalar is a typed decline (never a
/// silent empty answer).
pub fn resolve_pointer(model: &Json5Model, source: &[u8], pointer: &str) -> Result<Resolved> {
    let segments = parse_pointer(pointer)?;
    let mut index = model.root;
    let mut matches = 1u32;
    for seg in &segments {
        let node = model
            .node(index)
            .ok_or_else(|| corrupt("pointer traversal left the model"))?;
        match node.kind {
            K_OBJECT => {
                let mut found = 0u32;
                let mut first = None;
                let mut i = 0usize;
                while i + 1 < node.children.len() {
                    let key_idx = node.children[i];
                    let val_idx = node.children[i + 1];
                    i += 2;
                    let key_node = model
                        .node(key_idx)
                        .ok_or_else(|| corrupt("object key index is out of range"))?;
                    let key = decode_key(source, key_node)?;
                    if key == *seg {
                        found = found.saturating_add(1);
                        if first.is_none() {
                            first = Some(val_idx);
                        }
                    }
                }
                index = first.ok_or_else(|| {
                    Error::unsupported_feature(format!(
                        "JSON5 object has no member {seg:?} at this pointer"
                    ))
                })?;
                matches = found;
            }
            K_ARRAY => {
                let idx = parse_array_index(seg)?;
                let len = node.children.len();
                if idx as usize >= len {
                    return Err(Error::unsupported_feature(format!(
                        "JSON5 array index {idx} is out of range (length {len})"
                    )));
                }
                index = node.children[idx as usize];
                matches = 1;
            }
            _ => {
                return Err(Error::unsupported_feature(format!(
                    "cannot index into a JSON5 {} at {seg:?}",
                    kind_name(node.kind)
                )));
            }
        }
    }
    Ok(Resolved { index, matches })
}

/// A bounded, case-sensitive lexical search over object keys and string values.
/// Returns matches in document order (object members before descendants), each with
/// its canonical pointer and exact source span.
pub fn find(
    model: &Json5Model,
    source: &[u8],
    pattern: &str,
    limits: Limits,
) -> Result<Vec<Json5Match>> {
    let mut out = Vec::new();
    let mut path: Vec<String> = Vec::new();
    walk_find(
        model, source, model.root, true, &mut path, pattern, &mut out, limits, 0,
    )?;
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn walk_find(
    model: &Json5Model,
    source: &[u8],
    index: u32,
    is_value: bool,
    path: &mut Vec<String>,
    pattern: &str,
    out: &mut Vec<Json5Match>,
    limits: Limits,
    depth: u32,
) -> Result<()> {
    if depth > limits.max_json5_depth {
        return Err(Error::resource_limit(
            "JSON5 find exceeded the nesting-depth cap",
        ));
    }
    let node = model
        .node(index)
        .ok_or_else(|| corrupt("find hit an out-of-range node"))?;
    match node.kind {
        K_OBJECT => {
            let mut i = 0usize;
            while i + 1 < node.children.len() {
                let key_idx = node.children[i];
                let val_idx = node.children[i + 1];
                i += 2;
                let key_node = model
                    .node(key_idx)
                    .ok_or_else(|| corrupt("object key index is out of range"))?;
                let key = decode_key(source, key_node)?;
                if key.contains(pattern) {
                    out.push(Json5Match {
                        pointer: pointer_of(path, Some(&key)),
                        role: json::MatchRole::Key,
                        kind: K_STRING,
                        start: key_node.start,
                        end: key_node.end,
                        text: key.clone(),
                    });
                }
                path.push(json::escape_segment(&key));
                walk_find(
                    model,
                    source,
                    val_idx,
                    true,
                    path,
                    pattern,
                    out,
                    limits,
                    depth + 1,
                )?;
                path.pop();
            }
        }
        K_ARRAY => {
            for (i, child) in node.children.iter().enumerate() {
                path.push(i.to_string());
                walk_find(
                    model,
                    source,
                    *child,
                    true,
                    path,
                    pattern,
                    out,
                    limits,
                    depth + 1,
                )?;
                path.pop();
            }
        }
        K_STRING if is_value => {
            let text = decode_string(source, node)?;
            if text.contains(pattern) {
                out.push(Json5Match {
                    pointer: pointer_of(path, None),
                    role: json::MatchRole::Value,
                    kind: K_STRING,
                    start: node.start,
                    end: node.end,
                    text,
                });
            }
        }
        _ => {}
    }
    Ok(())
}

fn pointer_of(path: &[String], leaf_key: Option<&str>) -> String {
    let mut s = String::new();
    for seg in path {
        s.push('/');
        s.push_str(seg);
    }
    if let Some(k) = leaf_key {
        s.push('/');
        s.push_str(&json::escape_segment(k));
    }
    s
}

fn parse_pointer(pointer: &str) -> Result<Vec<String>> {
    if pointer.is_empty() {
        return Ok(Vec::new());
    }
    if !pointer.starts_with('/') {
        return Err(Error::usage(format!(
            "JSON5 pointer {pointer:?} must be empty or start with '/'"
        )));
    }
    let mut out = Vec::new();
    for raw in pointer.split('/').skip(1) {
        out.push(unescape_segment(raw)?);
    }
    Ok(out)
}

fn unescape_segment(raw: &str) -> Result<String> {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '~' {
            match chars.next() {
                Some('0') => out.push('~'),
                Some('1') => out.push('/'),
                _ => {
                    return Err(Error::usage(format!(
                        "invalid JSON5 pointer escape in segment {raw:?}"
                    )));
                }
            }
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

fn parse_array_index(seg: &str) -> Result<u32> {
    if seg == "-" || (seg.len() > 1 && seg.starts_with('0')) || seg.is_empty() {
        return Err(Error::usage(format!(
            "JSON5 array segment {seg:?} is not a canonical index"
        )));
    }
    seg.parse::<u32>()
        .map_err(|_| Error::usage(format!("JSON5 array index {seg:?} is not a u32")))
}

// ---------------------------------------------------------------------------
// String unescaping (JSON5 escapes)
// ---------------------------------------------------------------------------

fn push_cp(out: &mut Vec<u8>, cp: u32) -> Result<()> {
    let ch = char::from_u32(cp).ok_or_else(|| corrupt("invalid code point"))?;
    let mut buf = [0u8; 4];
    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
    Ok(())
}

fn unescape(inner: &[u8]) -> Result<String> {
    let mut out: Vec<u8> = Vec::with_capacity(inner.len());
    let mut i = 0usize;
    while i < inner.len() {
        let c = inner[i];
        if c != b'\\' {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        let e = *inner
            .get(i)
            .ok_or_else(|| corrupt("string ends inside an escape"))?;
        i += 1;
        match e {
            b'"' => out.push(b'"'),
            b'\'' => out.push(b'\''),
            b'\\' => out.push(b'\\'),
            b'/' => out.push(b'/'),
            b'b' => out.push(0x08),
            b'f' => out.push(0x0C),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'v' => out.push(0x0B),
            b'0' => {
                // ES5/JSON5: `\0` must not be followed by a decimal digit.
                if matches!(inner.get(i), Some(d) if d.is_ascii_digit()) {
                    return Err(corrupt("\\0 escape may not be followed by a digit"));
                }
                out.push(0x00);
            }
            b'\n' => {}
            b'\r' => {
                if inner.get(i) == Some(&b'\n') {
                    i += 1;
                }
            }
            b'x' => {
                let v = read_hex(inner, &mut i, 2)?;
                push_cp(&mut out, v)?;
            }
            b'u' => {
                let cp = read_hex(inner, &mut i, 4)?;
                if (0xD800..0xDC00).contains(&cp) {
                    if inner.get(i) != Some(&b'\\') || inner.get(i + 1) != Some(&b'u') {
                        return Err(corrupt("lone high surrogate in string"));
                    }
                    i += 2;
                    let lo = read_hex(inner, &mut i, 4)?;
                    if !(0xDC00..0xE000).contains(&lo) {
                        return Err(corrupt("high surrogate not followed by a low surrogate"));
                    }
                    let combined = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                    push_cp(&mut out, combined)?;
                } else if (0xDC00..0xE000).contains(&cp) {
                    return Err(corrupt("lone low surrogate in string"));
                } else {
                    push_cp(&mut out, cp)?;
                }
            }
            _ => {
                // A `\` followed by a raw LS/PS is a line continuation.
                let rest = &inner[i - 1..];
                if rest.starts_with(&[0xE2, 0x80, 0xA8]) || rest.starts_with(&[0xE2, 0x80, 0xA9]) {
                    i += 2;
                } else {
                    return Err(corrupt("invalid string escape"));
                }
            }
        }
    }
    String::from_utf8(out).map_err(|_| corrupt("string is not valid UTF-8"))
}

fn read_hex(inner: &[u8], i: &mut usize, n: usize) -> Result<u32> {
    let mut v = 0u32;
    for _ in 0..n {
        let b = *inner
            .get(*i)
            .ok_or_else(|| corrupt("truncated hex escape"))?;
        *i += 1;
        let d = match b {
            b'0'..=b'9' => (b - b'0') as u32,
            b'a'..=b'f' => (b - b'a' + 10) as u32,
            b'A'..=b'F' => (b - b'A' + 10) as u32,
            _ => return Err(corrupt("hex escape has a non-hex digit")),
        };
        v = (v << 4) | d;
    }
    Ok(v)
}

// ---------------------------------------------------------------------------
// The bounded parser
// ---------------------------------------------------------------------------

struct Parser<'a> {
    b: &'a [u8],
    at: usize,
    limits: Limits,
    build: bool,
    nodes: Vec<JNode>,
    count: u64,
    string_bytes: u64,
    comments: Vec<Comment>,
    comment_count: u64,
    max_depth_seen: u32,
    root_kind: u8,
    trailing_commas: u32,
    unquoted_keys: u32,
    uses_jsonc_only: bool,
    uses_json5: bool,
}

impl<'a> Parser<'a> {
    fn new(b: &'a [u8], limits: Limits, build: bool) -> Self {
        Parser {
            b,
            at: 0,
            limits,
            build,
            nodes: Vec::new(),
            count: 0,
            string_bytes: 0,
            comments: Vec::new(),
            comment_count: 0,
            max_depth_seen: 0,
            root_kind: 0,
            trailing_commas: 0,
            unquoted_keys: 0,
            uses_jsonc_only: false,
            uses_json5: false,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.at).copied()
    }

    /// Decode one UTF-8 char at `self.at` (never advances). `None` for invalid UTF-8.
    fn next_char(&self) -> Option<(char, usize)> {
        let bytes = self.b.get(self.at..)?;
        let b0 = *bytes.first()?;
        if b0 < 0x80 {
            return Some((b0 as char, 1));
        }
        let len = if b0 >= 0xF0 {
            4
        } else if b0 >= 0xE0 {
            3
        } else if b0 >= 0xC0 {
            2
        } else {
            return None;
        };
        let slice = bytes.get(..len)?;
        let s = core::str::from_utf8(slice).ok()?;
        Some((s.chars().next()?, len))
    }

    fn skip_ws(&mut self) -> Result<()> {
        loop {
            let Some(c) = self.b.get(self.at).copied() else {
                return Ok(());
            };
            match c {
                b' ' | b'\t' | b'\n' | b'\r' => self.at += 1,
                // JSON5 adds VT and FF to the JSON whitespace set.
                0x0B | 0x0C => {
                    self.at += 1;
                    self.uses_json5 = true;
                }
                b'/' => match self.b.get(self.at + 1) {
                    Some(b'/') => self.line_comment()?,
                    Some(b'*') => self.block_comment()?,
                    _ => return Ok(()),
                },
                _ => {
                    if !self.skip_one_ext_ws() {
                        return Ok(());
                    }
                }
            }
        }
    }

    /// Consume one extended (non-ASCII) JSON5 whitespace char if present.
    fn skip_one_ext_ws(&mut self) -> bool {
        let Some((c, len)) = self.next_char() else {
            return false;
        };
        if is_json5_ext_ws(c) {
            self.at += len;
            self.uses_json5 = true;
            true
        } else {
            false
        }
    }

    fn line_comment(&mut self) -> Result<()> {
        let start = self.at;
        self.at += 2;
        while let Some(c) = self.b.get(self.at).copied() {
            if c == b'\n' || c == b'\r' {
                break;
            }
            if c < 0x80 {
                self.at += 1;
                continue;
            }
            match self.next_char() {
                Some((ch, len)) => {
                    if ch == '\u{2028}' || ch == '\u{2029}' {
                        break;
                    }
                    self.at += len;
                }
                None => self.at += 1,
            }
        }
        self.record_comment(C_LINE, start as u64, self.at as u64)
    }

    fn block_comment(&mut self) -> Result<()> {
        let start = self.at;
        self.at += 2;
        loop {
            match self.b.get(self.at) {
                None => return Err(corrupt("unterminated block comment")),
                Some(b'*') if self.b.get(self.at + 1) == Some(&b'/') => {
                    self.at += 2;
                    break;
                }
                _ => self.at += 1,
            }
        }
        self.record_comment(C_BLOCK, start as u64, self.at as u64)
    }

    fn record_comment(&mut self, kind: u8, start: u64, end: u64) -> Result<()> {
        self.uses_jsonc_only = true;
        self.comment_count = self.comment_count.saturating_add(1);
        if self.comment_count > self.limits.max_json5_comments as u64 {
            return Err(Error::resource_limit(format!(
                "JSON5 document exceeds the {}-comment cap",
                self.limits.max_json5_comments
            )));
        }
        if self.build {
            self.comments.push(Comment { kind, start, end });
        }
        Ok(())
    }

    fn bump_count(&mut self) -> Result<()> {
        self.count = self.count.saturating_add(1);
        if self.count > self.limits.max_json5_nodes as u64 {
            return Err(Error::resource_limit(format!(
                "JSON5 document exceeds the {}-node cap",
                self.limits.max_json5_nodes
            )));
        }
        Ok(())
    }

    fn value(&mut self, depth: u32) -> Result<u32> {
        if depth > self.limits.max_json5_depth {
            return Err(Error::resource_limit(format!(
                "JSON5 nesting exceeds the {}-level cap",
                self.limits.max_json5_depth
            )));
        }
        self.max_depth_seen = self.max_depth_seen.max(depth);
        self.skip_ws()?;
        let start = self.at as u64;
        let c = self
            .peek()
            .ok_or_else(|| corrupt("unexpected end of input"))?;
        match c {
            b'{' => self.object(depth, start),
            b'[' => self.array(depth, start),
            b'"' => {
                self.string_token(b'"')?;
                self.scalar(K_STRING, start, self.at as u64)
            }
            b'\'' => {
                self.string_token(b'\'')?;
                self.scalar(K_STRING, start, self.at as u64)
            }
            b't' => {
                self.keyword(b"true")?;
                self.scalar(K_TRUE, start, self.at as u64)
            }
            b'f' => {
                self.keyword(b"false")?;
                self.scalar(K_FALSE, start, self.at as u64)
            }
            b'n' => {
                self.keyword(b"null")?;
                self.scalar(K_NULL, start, self.at as u64)
            }
            b'I' => {
                self.keyword(b"Infinity")?;
                self.uses_json5 = true;
                self.scalar(K_NUMBER, start, self.at as u64)
            }
            b'N' => {
                self.keyword(b"NaN")?;
                self.uses_json5 = true;
                self.scalar(K_NUMBER, start, self.at as u64)
            }
            b'+' | b'-' | b'.' | b'0'..=b'9' => {
                self.number()?;
                self.scalar(K_NUMBER, start, self.at as u64)
            }
            _ => Err(corrupt("not a JSON5 value")),
        }
    }

    fn scalar(&mut self, kind: u8, start: u64, end: u64) -> Result<u32> {
        self.bump_count()?;
        if self.count == 1 {
            self.root_kind = kind;
        }
        if self.build {
            let idx = self.nodes.len() as u32;
            self.nodes.push(JNode {
                kind,
                start,
                end,
                children: Vec::new(),
            });
            Ok(idx)
        } else {
            Ok(0)
        }
    }

    fn placeholder(&mut self, kind: u8, start: u64) -> Result<u32> {
        self.bump_count()?;
        if self.count == 1 {
            self.root_kind = kind;
        }
        if self.build {
            let idx = self.nodes.len() as u32;
            self.nodes.push(JNode {
                kind,
                start,
                end: start,
                children: Vec::new(),
            });
            Ok(idx)
        } else {
            Ok(0)
        }
    }

    fn finish(&mut self, idx: u32, children: Vec<u32>, end: u64) -> u32 {
        if self.build {
            let n = &mut self.nodes[idx as usize];
            n.children = children;
            n.end = end;
        }
        idx
    }

    fn object(&mut self, depth: u32, start: u64) -> Result<u32> {
        let idx = self.placeholder(K_OBJECT, start)?;
        self.at += 1; // consume '{'
        let mut children: Vec<u32> = Vec::new();
        self.skip_ws()?;
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(self.finish(idx, children, self.at as u64));
        }
        loop {
            self.skip_ws()?;
            let kstart = self.at as u64;
            let key = match self.peek() {
                Some(b'"') => {
                    self.string_token(b'"')?;
                    self.scalar(K_STRING, kstart, self.at as u64)?
                }
                Some(b'\'') => {
                    self.string_token(b'\'')?;
                    self.scalar(K_STRING, kstart, self.at as u64)?
                }
                Some(c) if is_identifier_start_byte(c) => {
                    self.identifier_key()?;
                    self.unquoted_keys = self.unquoted_keys.saturating_add(1);
                    self.uses_json5 = true;
                    self.scalar(K_UNQUOTED, kstart, self.at as u64)?
                }
                _ => {
                    return Err(corrupt(
                        "object member key must be an identifier or a string",
                    ));
                }
            };
            self.skip_ws()?;
            if self.peek() != Some(b':') {
                return Err(corrupt("object member key must be followed by ':'"));
            }
            self.at += 1;
            let val = self.value(depth + 1)?;
            if self.build {
                children.push(key);
                children.push(val);
            }
            self.skip_ws()?;
            match self.peek() {
                Some(b',') => {
                    self.at += 1;
                    self.skip_ws()?;
                    if self.peek() == Some(b'}') {
                        self.at += 1;
                        self.trailing_commas = self.trailing_commas.saturating_add(1);
                        self.uses_jsonc_only = true;
                        break;
                    }
                }
                Some(b'}') => {
                    self.at += 1;
                    break;
                }
                _ => return Err(corrupt("expected ',' or '}' in object")),
            }
        }
        Ok(self.finish(idx, children, self.at as u64))
    }

    fn array(&mut self, depth: u32, start: u64) -> Result<u32> {
        let idx = self.placeholder(K_ARRAY, start)?;
        self.at += 1; // consume '['
        let mut children: Vec<u32> = Vec::new();
        self.skip_ws()?;
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(self.finish(idx, children, self.at as u64));
        }
        loop {
            let val = self.value(depth + 1)?;
            if self.build {
                children.push(val);
            }
            self.skip_ws()?;
            match self.peek() {
                Some(b',') => {
                    self.at += 1;
                    self.skip_ws()?;
                    if self.peek() == Some(b']') {
                        self.at += 1;
                        self.trailing_commas = self.trailing_commas.saturating_add(1);
                        self.uses_jsonc_only = true;
                        break;
                    }
                }
                Some(b']') => {
                    self.at += 1;
                    break;
                }
                _ => return Err(corrupt("expected ',' or ']' in array")),
            }
        }
        Ok(self.finish(idx, children, self.at as u64))
    }

    fn keyword(&mut self, kw: &[u8]) -> Result<()> {
        let end = self
            .at
            .checked_add(kw.len())
            .ok_or_else(|| corrupt("keyword overflow"))?;
        if self.b.get(self.at..end) != Some(kw) {
            return Err(corrupt("invalid JSON5 keyword"));
        }
        self.at = end;
        Ok(())
    }

    fn identifier_key(&mut self) -> Result<()> {
        let (c, len) = self
            .next_char()
            .ok_or_else(|| corrupt("invalid identifier"))?;
        if !is_id_start(c) {
            return Err(corrupt("invalid identifier start"));
        }
        self.at += len;
        loop {
            match self.peek() {
                Some(b) if b < 0x80 => {
                    if is_id_continue_ascii(b) {
                        self.at += 1;
                    } else {
                        break;
                    }
                }
                Some(_) => {
                    let (c, len) = self
                        .next_char()
                        .ok_or_else(|| corrupt("invalid identifier"))?;
                    if is_id_continue(c) {
                        self.at += len;
                    } else {
                        break;
                    }
                }
                None => break,
            }
        }
        Ok(())
    }

    fn number(&mut self) -> Result<()> {
        match self.peek() {
            Some(b'+') => {
                self.at += 1;
                self.uses_json5 = true;
            }
            Some(b'-') => self.at += 1,
            _ => {}
        }
        // Infinity / NaN (case-sensitive), optionally signed.
        if self.peek() == Some(b'I') {
            self.keyword(b"Infinity")?;
            self.uses_json5 = true;
            return Ok(());
        }
        if self.peek() == Some(b'N') {
            self.keyword(b"NaN")?;
            self.uses_json5 = true;
            return Ok(());
        }
        // Hexadecimal.
        if self.peek() == Some(b'0') && matches!(self.b.get(self.at + 1), Some(b'x') | Some(b'X')) {
            self.at += 2;
            if !matches!(self.peek(), Some(c) if c.is_ascii_hexdigit()) {
                return Err(corrupt("hexadecimal literal has no digits"));
            }
            while matches!(self.peek(), Some(c) if c.is_ascii_hexdigit()) {
                self.at += 1;
            }
            self.uses_json5 = true;
            return Ok(());
        }
        let mut int_digits = false;
        match self.peek() {
            Some(b'0') => {
                self.at += 1;
                int_digits = true;
            }
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.at += 1;
                }
                int_digits = true;
            }
            _ => {}
        }
        let mut saw_dot = false;
        if self.peek() == Some(b'.') {
            saw_dot = true;
            self.at += 1;
            let before = self.at;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.at += 1;
            }
            let frac_digits = self.at > before;
            if !frac_digits {
                // A trailing decimal point (`5.`) is JSON5-only.
                if !int_digits {
                    return Err(corrupt("number has no digits"));
                }
                self.uses_json5 = true;
            } else if !int_digits {
                // A leading decimal point (`.5`) is JSON5-only.
                self.uses_json5 = true;
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.at += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(corrupt("number exponent has no digits"));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.at += 1;
            }
        }
        if !int_digits && !saw_dot {
            return Err(corrupt("number has no integer part"));
        }
        Ok(())
    }

    /// Parse one string token (single- or double-quoted), validating escapes and
    /// UTF-8, and charge its raw byte length against `max_json5_string_bytes`.
    fn string_token(&mut self, quote: u8) -> Result<()> {
        if quote == b'\'' {
            self.uses_json5 = true;
        }
        let open = self.at;
        self.at += 1; // consume opening quote
        loop {
            let c = self
                .peek()
                .ok_or_else(|| corrupt("unterminated JSON5 string"))?;
            if c == quote {
                self.at += 1;
                break;
            }
            if c == b'\\' {
                self.at += 1;
                let e = self
                    .peek()
                    .ok_or_else(|| corrupt("string ends inside an escape"))?;
                match e {
                    b'\n' => {
                        self.at += 1;
                        self.uses_json5 = true;
                    }
                    b'\r' => {
                        self.at += 1;
                        if self.peek() == Some(b'\n') {
                            self.at += 1;
                        }
                        self.uses_json5 = true;
                    }
                    b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => self.at += 1,
                    b'\'' => {
                        self.at += 1;
                        self.uses_json5 = true;
                    }
                    b'0' | b'v' => {
                        self.at += 1;
                        self.uses_json5 = true;
                    }
                    b'x' => {
                        self.at += 1;
                        for _ in 0..2 {
                            let h = self.peek().ok_or_else(|| corrupt("truncated \\x escape"))?;
                            if !h.is_ascii_hexdigit() {
                                return Err(corrupt("\\x escape has a non-hex digit"));
                            }
                            self.at += 1;
                        }
                        self.uses_json5 = true;
                    }
                    b'u' => {
                        self.at += 1;
                        for _ in 0..4 {
                            let h = self.peek().ok_or_else(|| corrupt("truncated \\u escape"))?;
                            if !h.is_ascii_hexdigit() {
                                return Err(corrupt("\\u escape has a non-hex digit"));
                            }
                            self.at += 1;
                        }
                    }
                    0x00..=0x7F => return Err(corrupt("invalid string escape")),
                    _ => {
                        // A `\` followed by LS/PS is a line continuation.
                        let (ch, len) = self
                            .next_char()
                            .ok_or_else(|| corrupt("invalid string escape"))?;
                        if ch == '\u{2028}' || ch == '\u{2029}' {
                            self.at += len;
                            self.uses_json5 = true;
                        } else {
                            return Err(corrupt("invalid string escape"));
                        }
                    }
                }
                continue;
            }
            match c {
                0x00..=0x1F => {
                    return Err(corrupt("unescaped control character in string"));
                }
                _ => {
                    if c < 0x80 {
                        self.at += 1;
                        continue;
                    }
                    let (ch, len) = self
                        .next_char()
                        .ok_or_else(|| corrupt("string is not valid UTF-8"))?;
                    if ch == '\u{2028}' || ch == '\u{2029}' {
                        return Err(corrupt("unescaped line terminator in string"));
                    }
                    self.at += len;
                }
            }
        }
        let inner = self
            .b
            .get(open + 1..self.at - 1)
            .ok_or_else(|| corrupt("string span overflow"))?;
        core::str::from_utf8(inner).map_err(|_| corrupt("string is not valid UTF-8"))?;
        self.string_bytes = self.string_bytes.saturating_add(inner.len() as u64);
        if self.string_bytes > self.limits.max_json5_string_bytes {
            return Err(Error::resource_limit(format!(
                "JSON5 string bytes exceed the {}-byte cap",
                self.limits.max_json5_string_bytes
            )));
        }
        Ok(())
    }
}

fn is_identifier_start_byte(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'$' || c == b'_' || c >= 0x80
}

fn is_id_start(c: char) -> bool {
    c == '$' || c == '_' || c.is_alphabetic()
}

fn is_id_continue(c: char) -> bool {
    is_id_start(c) || c.is_alphanumeric() || c == '\u{200C}' || c == '\u{200D}'
}

fn is_id_continue_ascii(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'$' || c == b'_'
}

/// The extended JSON5 whitespace set (ECMAScript `WhiteSpace` + `LineTerminator`
/// beyond the JSON ASCII set).
fn is_json5_ext_ws(c: char) -> bool {
    matches!(
        c,
        '\u{00A0}' | '\u{FEFF}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
    )
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn corrupt(msg: &str) -> Error {
    Error::invalid_json5_structure(format!("malformed JSON5: {msg}"))
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

    const DOC: &[u8] = b"{\n  // a line comment\n  unquoted: 'single',\n  hex: 0xFF,\n  /* block */ trailing: [1, 2,],\n}";

    #[test]
    fn detects_json5_and_jsonc_and_rejects_strict_json() {
        assert!(detect(DOC, Limits::DEFAULT));
        assert_eq!(
            parse(DOC, Limits::DEFAULT, true).unwrap().dialect,
            DIALECT_JSON5
        );
        // Strict JSON is rejected (the JSON detector claims it).
        assert!(!detect(br#"{"a": 1}"#, Limits::DEFAULT));
        assert_eq!(
            parse(br#"{"a": 1}"#, Limits::DEFAULT, true)
                .unwrap()
                .dialect,
            DIALECT_STRICT
        );
        // Comments + trailing comma only → jsonc.
        let c: &[u8] = b"{ // c\n  \"a\": 1, }";
        assert_eq!(
            parse(c, Limits::DEFAULT, true).unwrap().dialect,
            DIALECT_JSONC
        );
        // Malformed input is a typed decline.
        assert!(!detect(b"{", Limits::DEFAULT));
        assert_eq!(
            parse(b"{", Limits::DEFAULT, true).unwrap_err().class(),
            ErrorClass::InvalidJson5Structure
        );
    }

    #[test]
    fn preserves_comments_order_duplicates_and_spelling() {
        let src: &[u8] =
            b"{ unquoted: 1e3, 'a': .5, a: -0, hex: 0x10, inf: Infinity, n: NaN, b: 5., }";
        let m = parse(src, Limits::DEFAULT, true).unwrap();
        assert_eq!(m.dialect, DIALECT_JSON5);
        assert!(m.trailing_commas >= 1);
        assert!(m.unquoted_keys >= 3);
        let txt = canonical_text(&m, src).unwrap();
        assert!(txt.contains("unquoted:1e3"), "{txt}");
        assert!(txt.contains("'a':.5"), "{txt}");
        assert!(txt.contains("hex:0x10"), "{txt}");
        assert!(txt.contains("b:5."), "{txt}");
        // Duplicate `a` is kept distinct.
        let r = resolve_pointer(&m, src, "/a").unwrap();
        assert_eq!(r.matches, 2);
        // Comments are recorded, not dropped.
        let cdoc: &[u8] = b"{ // one\n x: 1 /* two */ }";
        let cm = parse(cdoc, Limits::DEFAULT, true).unwrap();
        assert_eq!(cm.comments.len(), 2);
        assert_eq!(cm.comments[0].kind, C_LINE);
        assert_eq!(cm.comments[1].kind, C_BLOCK);
    }

    #[test]
    fn model_roundtrips() {
        let m = parse(DOC, Limits::DEFAULT, true).unwrap();
        let bytes = m.encode();
        assert_eq!(Json5Model::decode(&bytes).unwrap(), m);
        for cut in 0..bytes.len() {
            let _ = Json5Model::decode(&bytes[..cut]);
        }
    }

    #[test]
    fn find_matches_keys_and_values() {
        let src: &[u8] = b"{ name: 'alpha', other: 'beta', n: 1 }";
        let m = parse(src, Limits::DEFAULT, true).unwrap();
        let found = find(&m, src, "a", Limits::DEFAULT).unwrap();
        assert!(found.iter().any(|x| x.role == json::MatchRole::Key));
        assert!(found.iter().any(|x| x.role == json::MatchRole::Value));
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0x00C0_FFEE_1234_5678;
        for _ in 0..128 {
            let mut buf = vec![0u8; 512];
            for b in buf.iter_mut() {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                *b = (x & 0xFF) as u8;
            }
            let _ = detect(&buf, Limits::DEFAULT);
            if let Ok(m) = parse(&buf, Limits::STRICT, true) {
                let _ = canonical_text(&m, &buf);
                let _ = m.encode();
                let _ = find(&m, &buf, "a", Limits::STRICT);
            }
            let _ = build_json5_model(&buf, Limits::STRICT);
        }
    }
}
