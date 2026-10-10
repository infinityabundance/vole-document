//! Bounded, representation-preserving TOML adapter (Phase 21.11).
//!
//! TOML is the next **Wave-2 structured-tree** format after JSON/YAML/CSV/
//! Markdown/XML/HTML. Like them it is *not* a package: there is no OPC/ZIP layer,
//! no `mimetype`, and no relationship graph. The exact leaf is therefore the
//! **whole source** (a `DocumentExact`, a RAW-like authority), and everything this
//! module produces is a bounded, deterministic (`Q_gen`) projection that never sits
//! on the exactness path.
//!
//! ## Why a bespoke, span-preserving parser
//!
//! A conventional TOML load (`toml`, `tomli`, a `dict`) keeps *values* and drops
//! the *representation*: it discards comments, key quoting, numeric spelling
//! (`1_000`, `0x1F`), string-escape spelling, and every source offset. This adapter
//! does the opposite. For every construct it records the exact **byte span** in the
//! source, so it preserves and can report:
//!
//! * every **table** (`[a.b]`), **array of tables** (`[[a]]`), **inline table**
//!   (`{ … }`), **array** (`[ … ]`), and every **key / value** pair, in document
//!   order;
//! * every scalar's **exact spelling**: basic/literal/multiline strings, integers
//!   with `_`/`0x`/`0o`/`0b`, floats including `inf`/`nan`, booleans, and
//!   offset/local date-times — the literal token is never rewritten or parsed into
//!   a binary float;
//! * every **comment**, with its exact span;
//! * **dotted keys** (`a.b.c = 1`) build the nested tables they name, preserving the
//!   exact key spans.
//!
//! ## Duplicate-key / redefinition policy (explicit)
//!
//! TOML's duplicate-key and redefinition rules are **enforced**, not preserved: a
//! redefined table, a duplicate key in one table, an array of tables redefined as a
//! table (or vice versa), or a dotted/header path that extends a value is a typed
//! decline ([`ErrorClass::InvalidTomlStructure`] via
//! [`crate::error::Error::invalid_toml_structure`]). A doc that violates them is
//! therefore **not** detected as TOML and stays `Opaque`. (JSON preserves duplicate
//! keys; TOML deliberately declines them.)
//!
//! ## Dependency policy
//!
//! This module is **dependency-free** (the non-default `toml` feature adds no
//! crate). The scanner is bespoke; it does **not** use the `toml` crate.
//!
//! ## Detection
//!
//! [`detect`] is deliberately conservative. TOML is tried after the strong,
//! fully-parsed tree formats (JSON, YAML) but **before** the weak, no-magic-byte
//! heuristics (CSV, Markdown, XML, HTML) in `detect_document_format`, because its
//! positive signal is a complete error-free parse and a TOML comment (`# …` at
//! column 0) would otherwise be misread as a Markdown ATX heading. A source
//! qualifies as TOML only if the whole source parses as TOML under every cap **and**
//! it carries at least one key/value assignment (a comment-only, header-only, or
//! empty source is not TOML). Plain prose and non-TOML text stay
//! [`DocumentFormat::Opaque`](crate::field::document_format::DocumentFormat::Opaque).

use std::collections::HashMap;

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model nodes (defends the decoder against a hostile blob).
pub const MAX_MODEL_NODES: u32 = 1 << 24;

/// TOML node kind: the implicit document root table.
pub const K_ROOT: u8 = 0;
/// TOML node kind: a named table (`[a.b]`, a dotted-key table, or an implicit
/// super-table).
pub const K_TABLE: u8 = 1;
/// TOML node kind: an array of tables (`[[a]]`); its children are element tables.
pub const K_ARRAY_TABLE: u8 = 2;
/// TOML node kind: an inline table (`{ … }`).
pub const K_INLINE_TABLE: u8 = 3;
/// TOML node kind: an array (`[ … ]`); its children are the element values.
pub const K_ARRAY: u8 = 4;
/// TOML node kind: a key token (bare, basic-quoted, or literal-quoted).
pub const K_KEY: u8 = 5;
/// TOML node kind: a single-line basic string (`"…"`).
pub const K_STRING_BASIC: u8 = 6;
/// TOML node kind: a single-line literal string (`'…'`).
pub const K_STRING_LITERAL: u8 = 7;
/// TOML node kind: a multiline basic string (`"""…"""`).
pub const K_STRING_ML_BASIC: u8 = 8;
/// TOML node kind: a multiline literal string (`'''…'''`).
pub const K_STRING_ML_LITERAL: u8 = 9;
/// TOML node kind: an integer (`42`, `1_000`, `0x1F`, `0o17`, `0b101`).
pub const K_INTEGER: u8 = 10;
/// TOML node kind: a float (`1.0`, `1e3`, `inf`, `nan`).
pub const K_FLOAT: u8 = 11;
/// TOML node kind: a boolean (`true`/`false`).
pub const K_BOOL: u8 = 12;
/// TOML node kind: an offset/local date-time, date, or time.
pub const K_DATETIME: u8 = 13;

/// Stable lower-case kind name for reports and JSON output.
pub const fn kind_name(kind: u8) -> &'static str {
    match kind {
        K_ROOT => "root",
        K_TABLE => "table",
        K_ARRAY_TABLE => "array-table",
        K_INLINE_TABLE => "inline-table",
        K_ARRAY => "array",
        K_KEY => "key",
        K_STRING_BASIC => "string",
        K_STRING_LITERAL => "literal-string",
        K_STRING_ML_BASIC => "multiline-string",
        K_STRING_ML_LITERAL => "multiline-literal-string",
        K_INTEGER => "integer",
        K_FLOAT => "float",
        K_BOOL => "boolean",
        K_DATETIME => "datetime",
        _ => "unknown",
    }
}

/// Whether `kind` is a table-like container whose children are interleaved
/// `[key, value, …]` entries.
pub const fn is_table_like(kind: u8) -> bool {
    matches!(kind, K_ROOT | K_TABLE | K_INLINE_TABLE)
}

/// Whether `kind` is a string scalar.
pub const fn is_string(kind: u8) -> bool {
    matches!(
        kind,
        K_STRING_BASIC | K_STRING_LITERAL | K_STRING_ML_BASIC | K_STRING_ML_LITERAL
    )
}

/// Whether `kind` is a scalar (not a container).
pub const fn is_scalar(kind: u8) -> bool {
    is_string(kind) || matches!(kind, K_INTEGER | K_FLOAT | K_BOOL | K_DATETIME)
}

/// One parsed TOML node.
///
/// * For a table-like container, `children` is the interleaved document-order list
///   `[key0, value0, key1, value1, …]`.
/// * For an array, `children` is the element list in order.
/// * For an array of tables, `children` is the element table list in order.
/// * For a key or a scalar, `children` is empty and `[start, end)` is the exact
///   token.
///
/// `[key_start, key_end)` is the span of the key token that names this node in its
/// parent table (`0, 0` for the root and for array elements).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TNode {
    /// One of the `K_*` tags.
    pub kind: u8,
    /// The construct's first source byte.
    pub start: u64,
    /// One past the construct's last source byte.
    pub end: u64,
    /// The naming key's first source byte (`0` when unnamed).
    pub key_start: u64,
    /// One past the naming key's last source byte (`0` when unnamed).
    pub key_end: u64,
    /// Table-like: interleaved key/value indices; array: element indices.
    pub children: Vec<u32>,
}

/// One preserved comment span (delimiters included).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Comment {
    /// The comment's first source byte (the `#`).
    pub start: u64,
    /// One past the comment's last source byte (before the newline).
    pub end: u64,
}

/// The canonical derived TOML model (the materialization of a `TomlModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TomlModel {
    /// Index of the root table node.
    pub root: u32,
    /// The observed maximum container nesting depth (root = 1).
    pub max_depth: u32,
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// The number of key/value **assignments** (`k = v`), a detection signal.
    pub assignments: u64,
    /// The node arena, in document order.
    pub nodes: Vec<TNode>,
    /// Every comment, in document order.
    pub comments: Vec<Comment>,
}

impl TomlModel {
    /// The node at `index`, if present.
    pub fn node(&self, index: u32) -> Option<&TNode> {
        self.nodes.get(index as usize)
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(48 + self.nodes.len() * 41 + self.comments.len() * 16);
        out.extend_from_slice(b"TOML");
        out.push(MODEL_VERSION);
        out.extend_from_slice(&self.root.to_le_bytes());
        out.extend_from_slice(&self.max_depth.to_le_bytes());
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&self.assignments.to_le_bytes());
        out.extend_from_slice(&(self.nodes.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.comments.len() as u32).to_le_bytes());
        for c in &self.comments {
            out.extend_from_slice(&c.start.to_le_bytes());
            out.extend_from_slice(&c.end.to_le_bytes());
        }
        for n in &self.nodes {
            out.push(n.kind);
            out.extend_from_slice(&n.start.to_le_bytes());
            out.extend_from_slice(&n.end.to_le_bytes());
            out.extend_from_slice(&n.key_start.to_le_bytes());
            out.extend_from_slice(&n.key_end.to_le_bytes());
            out.extend_from_slice(&(n.children.len() as u32).to_le_bytes());
            for c in &n.children {
                out.extend_from_slice(&c.to_le_bytes());
            }
        }
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<TomlModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"TOML" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let root = r.u32()?;
        let max_depth = r.u32()?;
        let doc_len = r.u64()?;
        let assignments = r.u64()?;
        let node_count = r.u32()?;
        let comment_count = r.u32()?;
        if node_count > MAX_MODEL_NODES {
            return Err(corrupt("model node count is implausible"));
        }
        if comment_count > node_count.saturating_add(1) {
            return Err(corrupt("model comment count is implausible"));
        }
        let mut comments = Vec::with_capacity(comment_count as usize);
        for _ in 0..comment_count {
            let start = r.u64()?;
            let end = r.u64()?;
            if start > end || end > doc_len {
                return Err(corrupt("comment span is outside the document"));
            }
            comments.push(Comment { start, end });
        }
        let mut nodes = Vec::with_capacity(node_count as usize);
        for _ in 0..node_count {
            let kind = r.u8()?;
            if kind > K_DATETIME {
                return Err(corrupt("unknown node kind"));
            }
            let start = r.u64()?;
            let end = r.u64()?;
            let key_start = r.u64()?;
            let key_end = r.u64()?;
            for (lo, hi) in [(start, end), (key_start, key_end)] {
                if lo > hi || hi > doc_len {
                    return Err(corrupt("node span is outside the document"));
                }
            }
            let cn = r.u32()?;
            if cn as u64 > node_count as u64 {
                return Err(corrupt("node child count is implausible"));
            }
            let mut children = Vec::with_capacity(cn as usize);
            for _ in 0..cn {
                let c = r.u32()?;
                if c >= node_count {
                    return Err(corrupt("child index is out of range"));
                }
                children.push(c);
            }
            nodes.push(TNode {
                kind,
                start,
                end,
                key_start,
                key_end,
                children,
            });
        }
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        if root as usize >= nodes.len() {
            return Err(corrupt("root index is out of range"));
        }
        Ok(TomlModel {
            root,
            max_depth,
            doc_len,
            assignments,
            nodes,
            comments,
        })
    }
}

/// A resolved path query: the node index and how many entries matched the final
/// segment (always `1` here, because duplicate keys are declined typed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolved {
    /// The resolved node index.
    pub index: u32,
    /// The number of matches for the final segment (`1`).
    pub matches: u32,
}

/// One table entry reported by [`table_keys`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEntry {
    /// The decoded key text.
    pub key: String,
    /// The key token's exact span start.
    pub key_start: u64,
    /// The key token's exact span end.
    pub key_end: u64,
    /// The value node index.
    pub value_index: u32,
    /// The value's kind tag.
    pub kind: u8,
    /// The value's exact span start.
    pub value_start: u64,
    /// The value's exact span end.
    pub value_end: u64,
}

/// One lexical-match record from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TomlMatch {
    /// A dotted path to the matching construct (e.g. `server.ports[0]`).
    pub path: String,
    /// What matched.
    pub role: MatchRole,
    /// The exact source span of the matching construct.
    pub start: u64,
    /// One past the matching construct.
    pub end: u64,
    /// The matched text.
    pub text: String,
}

/// Whether a lexical match is a key or a string value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchRole {
    /// A key token.
    Key,
    /// A string value.
    Value,
}

impl MatchRole {
    /// Stable lower-case name.
    pub const fn name(self) -> &'static str {
        match self {
            MatchRole::Key => "key",
            MatchRole::Value => "value",
        }
    }
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Byte-based, conservative TOML detector. See the module docs.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_toml_document_bytes {
        return false;
    }
    match parse(source, limits) {
        Ok(m) => m.assignments >= 1,
        Err(_) => false,
    }
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of a `TomlModel` node).
pub fn build_toml_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits)?.encode())
}

/// Parse `source` into a [`TomlModel`].
pub fn parse(source: &[u8], limits: Limits) -> Result<TomlModel> {
    if source.len() as u64 > limits.max_toml_document_bytes {
        return Err(Error::resource_limit(format!(
            "TOML source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_toml_document_bytes
        )));
    }
    let mut p = Parser::new(source, limits);
    p.parse_document()?;
    Ok(TomlModel {
        root: p.root_node,
        max_depth: p.max_depth_seen,
        doc_len: source.len() as u64,
        assignments: p.assignments,
        nodes: p.nodes,
        comments: p.comments,
    })
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn corrupt(msg: &str) -> Error {
    Error::invalid_toml_structure(format!("malformed TOML: {msg}"))
}

fn slice(source: &[u8], start: u64, end: u64) -> Result<&[u8]> {
    let s = usize::try_from(start).map_err(|_| corrupt("span overflow"))?;
    let e = usize::try_from(end).map_err(|_| corrupt("span overflow"))?;
    source
        .get(s..e)
        .ok_or_else(|| corrupt("span is outside the source"))
}

/// The exact source bytes of a node's token.
pub fn token_bytes<'a>(source: &'a [u8], node: &TNode) -> Result<&'a [u8]> {
    slice(source, node.start, node.end)
}

fn utf8_string(bytes: &[u8]) -> Result<String> {
    core::str::from_utf8(bytes)
        .map(|s| s.to_string())
        .map_err(|_| corrupt("string is not valid UTF-8"))
}

fn trim_leading_newline(bytes: &[u8]) -> &[u8] {
    if let Some(rest) = bytes.strip_prefix(b"\r\n") {
        rest
    } else if let Some(rest) = bytes.strip_prefix(b"\n") {
        rest
    } else {
        bytes
    }
}

/// The decoded text of a key node (bare, basic-quoted, or literal-quoted).
pub fn key_text(source: &[u8], node: &TNode) -> Result<String> {
    if node.kind != K_KEY {
        return Err(corrupt("node is not a key"));
    }
    let tok = token_bytes(source, node)?;
    match tok.first() {
        Some(b'"') => {
            if tok.len() < 2 {
                return Err(corrupt("key token is too short"));
            }
            unescape_basic(&tok[1..tok.len() - 1], false)
        }
        Some(b'\'') => {
            if tok.len() < 2 {
                return Err(corrupt("key token is too short"));
            }
            utf8_string(&tok[1..tok.len() - 1])
        }
        _ => utf8_string(tok),
    }
}

/// The decoded content of a string node (quotes stripped, escapes resolved for
/// basic strings; literal strings are taken verbatim).
pub fn string_content(source: &[u8], node: &TNode) -> Result<String> {
    let tok = token_bytes(source, node)?;
    match node.kind {
        K_STRING_BASIC => {
            if tok.len() < 2 {
                return Err(corrupt("string token is too short"));
            }
            unescape_basic(&tok[1..tok.len() - 1], false)
        }
        K_STRING_LITERAL => {
            if tok.len() < 2 {
                return Err(corrupt("string token is too short"));
            }
            utf8_string(&tok[1..tok.len() - 1])
        }
        K_STRING_ML_BASIC => {
            if tok.len() < 6 {
                return Err(corrupt("multiline string token is too short"));
            }
            unescape_basic(trim_leading_newline(&tok[3..tok.len() - 3]), true)
        }
        K_STRING_ML_LITERAL => {
            if tok.len() < 6 {
                return Err(corrupt("multiline string token is too short"));
            }
            utf8_string(trim_leading_newline(&tok[3..tok.len() - 3]))
        }
        _ => Err(corrupt("node is not a string")),
    }
}

/// The literal token text of a scalar node (the exact source spelling).
pub fn scalar_spelling(source: &[u8], node: &TNode) -> Result<String> {
    utf8_string(token_bytes(source, node)?)
}

/// Resolve a dotted path (`server.ports[0].host`; `""` is the root table) against
/// a parsed model. A missing key, an out-of-range index, or indexing a non-array is
/// a typed decline, never a silent empty answer.
pub fn resolve_path(model: &TomlModel, source: &[u8], path: &str) -> Result<Resolved> {
    let segs = parse_toml_path(path)?;
    let mut cur = model.root;
    for seg in &segs {
        let node = model
            .node(cur)
            .ok_or_else(|| corrupt("path traversal left the model"))?;
        let mut found = None;
        if is_table_like(node.kind) {
            let mut i = 0usize;
            while i + 1 < node.children.len() {
                let k = node.children[i];
                let v = node.children[i + 1];
                i += 2;
                let kn = model
                    .node(k)
                    .ok_or_else(|| corrupt("key index is out of range"))?;
                if key_text(source, kn)? == seg.key {
                    found = Some(v);
                    break;
                }
            }
        }
        let mut idx = found.ok_or_else(|| {
            Error::unsupported_feature(format!(
                "TOML has no key {:?} at the requested path",
                seg.key
            ))
        })?;
        for &n in &seg.indices {
            let elem = model
                .node(idx)
                .ok_or_else(|| corrupt("path traversal left the model"))?;
            match elem.kind {
                K_ARRAY | K_ARRAY_TABLE => {
                    idx = *elem.children.get(n as usize).ok_or_else(|| {
                        Error::unsupported_feature(format!(
                            "TOML index {n} is out of range (length {})",
                            elem.children.len()
                        ))
                    })?;
                }
                _ => {
                    return Err(Error::unsupported_feature(
                        "cannot index a non-array TOML value",
                    ));
                }
            }
        }
        cur = idx;
    }
    Ok(Resolved {
        index: cur,
        matches: 1,
    })
}

/// The entries of a table-like node at `index`, in document order.
pub fn table_keys(model: &TomlModel, source: &[u8], index: u32) -> Result<Vec<KeyEntry>> {
    let node = model
        .node(index)
        .ok_or_else(|| corrupt("table index is out of range"))?;
    if !is_table_like(node.kind) {
        return Err(Error::unsupported_feature(format!(
            "TOML node is a {}, not a table",
            kind_name(node.kind)
        )));
    }
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 1 < node.children.len() {
        let k = node.children[i];
        let v = node.children[i + 1];
        i += 2;
        let kn = model
            .node(k)
            .ok_or_else(|| corrupt("key index is out of range"))?;
        let vn = model
            .node(v)
            .ok_or_else(|| corrupt("value index is out of range"))?;
        out.push(KeyEntry {
            key: key_text(source, kn)?,
            key_start: kn.start,
            key_end: kn.end,
            value_index: v,
            kind: vn.kind,
            value_start: vn.start,
            value_end: vn.end,
        });
    }
    Ok(out)
}

/// A deterministic canonical text projection of the whole model: member order and
/// token spelling are preserved, comments are omitted, and nested tables are
/// re-emitted as `[header]` sections.
pub fn canonical_text(model: &TomlModel, source: &[u8]) -> Result<String> {
    let mut out = String::new();
    render(model, source, model.root, "", &mut out, 0)?;
    Ok(out)
}

fn render(
    model: &TomlModel,
    source: &[u8],
    index: u32,
    prefix: &str,
    out: &mut String,
    depth: u32,
) -> Result<()> {
    if depth > MAX_MODEL_NODES {
        return Err(Error::resource_limit(
            "TOML render exceeded its depth bound",
        ));
    }
    let node = model
        .node(index)
        .ok_or_else(|| corrupt("render hit an out-of-range node"))?;
    if !is_table_like(node.kind) {
        return Ok(());
    }
    let mut i = 0usize;
    while i + 1 < node.children.len() {
        let k = node.children[i];
        let v = node.children[i + 1];
        i += 2;
        let kn = model
            .node(k)
            .ok_or_else(|| corrupt("render key out of range"))?;
        let vn = model
            .node(v)
            .ok_or_else(|| corrupt("render value out of range"))?;
        let name = key_text(source, kn)?;
        let full = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}.{name}")
        };
        match vn.kind {
            K_TABLE => {
                out.push('[');
                out.push_str(&full);
                out.push_str("]\n");
                render(model, source, v, &full, out, depth + 1)?;
            }
            K_ARRAY_TABLE => {
                for elem in &vn.children {
                    out.push_str("[[");
                    out.push_str(&full);
                    out.push_str("]]\n");
                    render(model, source, *elem, &full, out, depth + 1)?;
                }
            }
            _ => {
                out.push_str(&full);
                out.push_str(" = ");
                out.push_str(&String::from_utf8_lossy(token_bytes(source, vn)?));
                out.push('\n');
            }
        }
    }
    Ok(())
}

/// A bounded, case-sensitive lexical search over keys and string values. Returns
/// matches in document order (a key before its value's descendants), each with its
/// dotted path and exact source span.
pub fn find(
    model: &TomlModel,
    source: &[u8],
    pattern: &str,
    limits: Limits,
) -> Result<Vec<TomlMatch>> {
    let mut out = Vec::new();
    walk_find(
        model, source, model.root, "", pattern, &mut out, limits, 0, true,
    )?;
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn walk_find(
    model: &TomlModel,
    source: &[u8],
    index: u32,
    prefix: &str,
    pattern: &str,
    out: &mut Vec<TomlMatch>,
    limits: Limits,
    depth: u32,
    root: bool,
) -> Result<()> {
    if depth > limits.max_toml_depth {
        return Err(Error::resource_limit("TOML find exceeded the depth cap"));
    }
    let node = model
        .node(index)
        .ok_or_else(|| corrupt("find hit an out-of-range node"))?;
    if out.len() as u64 > limits.max_toml_nodes as u64 {
        return Err(Error::resource_limit("TOML find exceeded the match cap"));
    }
    match node.kind {
        K_ROOT | K_TABLE | K_INLINE_TABLE => {
            let mut i = 0usize;
            while i + 1 < node.children.len() {
                let k = node.children[i];
                let v = node.children[i + 1];
                i += 2;
                let kn = model
                    .node(k)
                    .ok_or_else(|| corrupt("find key out of range"))?;
                let name = key_text(source, kn)?;
                let child = if root || prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}.{name}")
                };
                if name.contains(pattern) {
                    out.push(TomlMatch {
                        path: child.clone(),
                        role: MatchRole::Key,
                        start: kn.start,
                        end: kn.end,
                        text: name,
                    });
                }
                walk_find(
                    model,
                    source,
                    v,
                    &child,
                    pattern,
                    out,
                    limits,
                    depth + 1,
                    false,
                )?;
            }
        }
        K_ARRAY_TABLE | K_ARRAY => {
            for (j, elem) in node.children.iter().enumerate() {
                let child = format!("{prefix}[{j}]");
                walk_find(
                    model,
                    source,
                    *elem,
                    &child,
                    pattern,
                    out,
                    limits,
                    depth + 1,
                    false,
                )?;
            }
        }
        _ => {
            if is_string(node.kind) {
                let content = string_content(source, node)?;
                if content.contains(pattern) {
                    out.push(TomlMatch {
                        path: prefix.to_string(),
                        role: MatchRole::Value,
                        start: node.start,
                        end: node.end,
                        text: content,
                    });
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Path parsing
// ---------------------------------------------------------------------------

struct Seg {
    key: String,
    indices: Vec<u32>,
}

fn parse_toml_path(path: &str) -> Result<Vec<Seg>> {
    let bytes = path.as_bytes();
    let mut i = 0usize;
    let mut segs = Vec::new();
    while i < bytes.len() {
        let key;
        if bytes.get(i) == Some(&b'"') {
            let start = i + 1;
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            if i >= bytes.len() {
                return Err(Error::usage("unterminated quoted segment in TOML path"));
            }
            key = unescape_basic(&bytes[start..i], false)?;
            i += 1;
        } else {
            let start = i;
            while i < bytes.len() && bytes[i] != b'.' && bytes[i] != b'[' {
                i += 1;
            }
            if i == start {
                return Err(Error::usage("empty segment in TOML path"));
            }
            key = utf8_string(&bytes[start..i])?;
        }
        let mut indices = Vec::new();
        while bytes.get(i) == Some(&b'[') {
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != b']' {
                if !bytes[i].is_ascii_digit() {
                    return Err(Error::usage("TOML path index must be numeric"));
                }
                i += 1;
            }
            if i >= bytes.len() || i == start {
                return Err(Error::usage("malformed index in TOML path"));
            }
            let n: u32 = core::str::from_utf8(&bytes[start..i])
                .map_err(|_| Error::usage("TOML path index is not UTF-8"))?
                .parse()
                .map_err(|_| Error::usage("TOML path index overflows u32"))?;
            indices.push(n);
            i += 1;
        }
        segs.push(Seg { key, indices });
        if i < bytes.len() {
            if bytes.get(i) == Some(&b'.') {
                i += 1;
            } else {
                return Err(Error::usage("expected '.' between TOML path segments"));
            }
        }
    }
    Ok(segs)
}

// ---------------------------------------------------------------------------
// String escape resolution
// ---------------------------------------------------------------------------

fn unescape_basic(raw: &[u8], multiline: bool) -> Result<String> {
    let mut out: Vec<u8> = Vec::with_capacity(raw.len());
    let mut i = 0usize;
    while i < raw.len() {
        let c = raw[i];
        if c != b'\\' {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        let e = *raw
            .get(i)
            .ok_or_else(|| corrupt("string ends inside an escape"))?;
        if multiline && matches!(e, b' ' | b'\t' | b'\n' | b'\r') {
            while i < raw.len() && matches!(raw[i], b' ' | b'\t') {
                i += 1;
            }
            match raw.get(i) {
                Some(b'\r') => {
                    i += 1;
                    if raw.get(i) == Some(&b'\n') {
                        i += 1;
                    }
                }
                Some(b'\n') => i += 1,
                _ => {
                    return Err(corrupt(
                        "a multiline line-ending backslash must be followed by a newline",
                    ));
                }
            }
            continue;
        }
        match e {
            b'b' => {
                out.push(0x08);
                i += 1;
            }
            b't' => {
                out.push(b'\t');
                i += 1;
            }
            b'n' => {
                out.push(b'\n');
                i += 1;
            }
            b'f' => {
                out.push(0x0C);
                i += 1;
            }
            b'r' => {
                out.push(b'\r');
                i += 1;
            }
            b'"' => {
                out.push(b'"');
                i += 1;
            }
            b'\\' => {
                out.push(b'\\');
                i += 1;
            }
            b'u' => {
                i += 1;
                let cp = read_hex(raw, &mut i, 4)?;
                push_cp(&mut out, cp)?;
            }
            b'U' => {
                i += 1;
                let cp = read_hex(raw, &mut i, 8)?;
                push_cp(&mut out, cp)?;
            }
            _ => return Err(corrupt("invalid string escape")),
        }
    }
    String::from_utf8(out).map_err(|_| corrupt("string is not valid UTF-8"))
}

fn read_hex(raw: &[u8], i: &mut usize, n: usize) -> Result<u32> {
    let mut v: u32 = 0;
    for _ in 0..n {
        let b = *raw
            .get(*i)
            .ok_or_else(|| corrupt("truncated hexadecimal escape"))?;
        let d = (b as char)
            .to_digit(16)
            .ok_or_else(|| corrupt("hexadecimal escape has a non-hex digit"))?;
        v = v.wrapping_mul(16).wrapping_add(d);
        *i += 1;
    }
    Ok(v)
}

fn push_cp(out: &mut Vec<u8>, cp: u32) -> Result<()> {
    let c = char::from_u32(cp).ok_or_else(|| corrupt("escape is not a Unicode scalar value"))?;
    let mut buf = [0u8; 4];
    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
    Ok(())
}

// ---------------------------------------------------------------------------
// Value classification
// ---------------------------------------------------------------------------

fn strip_sign(t: &[u8]) -> &[u8] {
    match t.first() {
        Some(b'+') | Some(b'-') => &t[1..],
        _ => t,
    }
}

fn is_bare_date(t: &[u8]) -> bool {
    t.len() == 10
        && t[4] == b'-'
        && t[7] == b'-'
        && t[..4].iter().all(u8::is_ascii_digit)
        && t[5..7].iter().all(u8::is_ascii_digit)
        && t[8..].iter().all(u8::is_ascii_digit)
}

fn classify_numeric(t: &[u8]) -> Option<u8> {
    if t.is_empty() {
        return None;
    }
    if t.contains(&b':') || is_bare_date(t) {
        return Some(K_DATETIME);
    }
    if is_integer(t) {
        return Some(K_INTEGER);
    }
    if is_float(t) {
        return Some(K_FLOAT);
    }
    None
}

fn digit_run_ok(t: &[u8], ok: impl Fn(u8) -> bool) -> bool {
    if t.is_empty() {
        return false;
    }
    let mut prev_us = false;
    for (i, &c) in t.iter().enumerate() {
        if c == b'_' {
            if i == 0 || prev_us || i + 1 == t.len() {
                return false;
            }
            prev_us = true;
        } else if ok(c) {
            prev_us = false;
        } else {
            return false;
        }
    }
    !prev_us
}

fn is_integer(t: &[u8]) -> bool {
    let t = strip_sign(t);
    if t == b"0" {
        return true;
    }
    if let Some(rest) = t.strip_prefix(b"0x") {
        return digit_run_ok(rest, |c| c.is_ascii_hexdigit());
    }
    if let Some(rest) = t.strip_prefix(b"0o") {
        return digit_run_ok(rest, |c| (b'0'..=b'7').contains(&c));
    }
    if let Some(rest) = t.strip_prefix(b"0b") {
        return digit_run_ok(rest, |c| c == b'0' || c == b'1');
    }
    if t.first() == Some(&b'0') {
        return false;
    }
    digit_run_ok(t, |c| c.is_ascii_digit())
}

fn is_float(t: &[u8]) -> bool {
    let t = strip_sign(t);
    if t == b"inf" || t == b"nan" {
        return true;
    }
    let (mantissa, exp) = match t.iter().position(|&c| c == b'e' || c == b'E') {
        Some(i) => (&t[..i], Some(&t[i + 1..])),
        None => (t, None),
    };
    let mut has_exp = false;
    if let Some(e) = exp {
        has_exp = true;
        if !digit_run_ok(strip_sign(e), |c| c.is_ascii_digit()) {
            return false;
        }
    }
    let (intp, frac) = match mantissa.iter().position(|&c| c == b'.') {
        Some(i) => (&mantissa[..i], Some(&mantissa[i + 1..])),
        None => (mantissa, None),
    };
    let mut has_dot = false;
    if let Some(f) = frac {
        has_dot = true;
        if !digit_run_ok(f, |c| c.is_ascii_digit()) {
            return false;
        }
    }
    if !digit_run_ok(intp, |c| c.is_ascii_digit()) {
        return false;
    }
    // A leading zero is only allowed for `0` itself.
    let digits: Vec<u8> = intp.iter().copied().filter(|&c| c != b'_').collect();
    if digits.first() == Some(&b'0') && digits.len() > 1 {
        return false;
    }
    has_dot || has_exp
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

struct KeyTok {
    text: String,
    start: u64,
    end: u64,
}

struct Tbl {
    node: u32,
    map: HashMap<String, u32>,
    explicit: bool,
    is_array: bool,
    elements: Vec<u32>,
    last_elem: u32,
}

struct Parser<'a> {
    b: &'a [u8],
    at: usize,
    limits: Limits,
    nodes: Vec<TNode>,
    comments: Vec<Comment>,
    tables: Vec<Tbl>,
    root_node: u32,
    current: u32,
    node_count: u64,
    key_count: u64,
    string_bytes: u64,
    assignments: u64,
    max_depth_seen: u32,
}

impl<'a> Parser<'a> {
    fn new(b: &'a [u8], limits: Limits) -> Self {
        Parser {
            b,
            at: 0,
            limits,
            nodes: Vec::new(),
            comments: Vec::new(),
            tables: Vec::new(),
            root_node: 0,
            current: 0,
            node_count: 0,
            key_count: 0,
            string_bytes: 0,
            assignments: 0,
            max_depth_seen: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.at).copied()
    }

    fn peek_at(&self, off: usize) -> Option<u8> {
        self.b.get(self.at + off).copied()
    }

    fn starts_with(&self, pat: &[u8]) -> bool {
        self.b.get(self.at..self.at + pat.len()) == Some(pat)
    }

    fn skip_spaces(&mut self) {
        while matches!(self.peek(), Some(b' ') | Some(b'\t')) {
            self.at += 1;
        }
    }

    fn charge_string(&mut self, bytes: u64) -> Result<()> {
        self.string_bytes = self.string_bytes.saturating_add(bytes);
        if self.string_bytes > self.limits.max_toml_string_bytes {
            return Err(Error::resource_limit(format!(
                "TOML string bytes exceed the {}-byte cap",
                self.limits.max_toml_string_bytes
            )));
        }
        Ok(())
    }

    fn new_node(
        &mut self,
        kind: u8,
        start: u64,
        end: u64,
        key_start: u64,
        key_end: u64,
    ) -> Result<u32> {
        self.node_count = self.node_count.saturating_add(1);
        if self.node_count > self.limits.max_toml_nodes as u64 {
            return Err(Error::resource_limit(format!(
                "TOML document exceeds the {}-node cap",
                self.limits.max_toml_nodes
            )));
        }
        let idx = self.nodes.len() as u32;
        self.nodes.push(TNode {
            kind,
            start,
            end,
            key_start,
            key_end,
            children: Vec::new(),
        });
        Ok(idx)
    }

    fn new_key_node(&mut self, part: &KeyTok) -> Result<u32> {
        self.key_count = self.key_count.saturating_add(1);
        if self.key_count > self.limits.max_toml_keys {
            return Err(Error::resource_limit(format!(
                "TOML document exceeds the {}-key cap",
                self.limits.max_toml_keys
            )));
        }
        self.charge_string(part.end - part.start)?;
        self.new_node(K_KEY, part.start, part.end, 0, 0)
    }

    fn alloc_table(
        &mut self,
        kind: u8,
        start: u64,
        end: u64,
        key_start: u64,
        key_end: u64,
    ) -> Result<u32> {
        let node = self.new_node(kind, start, end, key_start, key_end)?;
        let slot = self.tables.len() as u32;
        self.tables.push(Tbl {
            node,
            map: HashMap::new(),
            explicit: false,
            is_array: kind == K_ARRAY_TABLE,
            elements: Vec::new(),
            last_elem: u32::MAX,
        });
        Ok(slot)
    }

    fn link(&mut self, parent: u32, keynode: u32, valuenode: u32) {
        let pn = self.tables[parent as usize].node;
        let (ks, ke) = match self.nodes.get(keynode as usize) {
            Some(k) => (k.start, k.end),
            None => (0, 0),
        };
        if let Some(v) = self.nodes.get_mut(valuenode as usize) {
            v.key_start = ks;
            v.key_end = ke;
        }
        if let Some(n) = self.nodes.get_mut(pn as usize) {
            n.children.push(keynode);
            n.children.push(valuenode);
        }
    }

    fn append_element(&mut self, container: u32, elem: u32) {
        let cn = self.tables[container as usize].node;
        if let Some(n) = self.nodes.get_mut(cn as usize) {
            n.children.push(elem);
        }
    }

    // -- document ------------------------------------------------------------

    fn parse_document(&mut self) -> Result<()> {
        if self.starts_with(&[0xEF, 0xBB, 0xBF]) {
            self.at += 3;
        }
        let root = self.alloc_table(K_ROOT, 0, 0, 0, 0)?;
        self.root_node = self.tables[root as usize].node;
        self.current = root;
        loop {
            self.skip_spaces();
            match self.peek() {
                None => break,
                Some(b'\n') => self.at += 1,
                Some(b'\r') => {
                    if self.peek_at(1) == Some(b'\n') {
                        self.at += 2;
                    } else {
                        return Err(corrupt("a lone carriage return is not a line ending"));
                    }
                }
                Some(b'#') => {
                    self.parse_comment();
                }
                Some(b'[') => {
                    let slot = self.parse_header()?;
                    self.finish_line()?;
                    self.current = slot;
                }
                Some(_) => {
                    let base = self.current;
                    self.parse_assignment(base)?;
                    self.finish_line()?;
                }
            }
        }
        let end = self.b.len() as u64;
        if let Some(n) = self.nodes.get_mut(self.root_node as usize) {
            n.end = end;
        }
        Ok(())
    }

    fn finish_line(&mut self) -> Result<()> {
        self.skip_spaces();
        if self.peek() == Some(b'#') {
            self.parse_comment();
        }
        self.skip_spaces();
        match self.peek() {
            None => Ok(()),
            Some(b'\n') => {
                self.at += 1;
                Ok(())
            }
            Some(b'\r') => {
                if self.peek_at(1) == Some(b'\n') {
                    self.at += 2;
                    Ok(())
                } else {
                    Err(corrupt("a lone carriage return is not a line ending"))
                }
            }
            Some(_) => Err(corrupt("trailing bytes after a value")),
        }
    }

    fn parse_comment(&mut self) {
        let start = self.at as u64;
        self.at += 1;
        while let Some(c) = self.peek() {
            if c == b'\n' || c == b'\r' {
                break;
            }
            self.at += 1;
        }
        self.comments.push(Comment {
            start,
            end: self.at as u64,
        });
    }

    // -- headers -------------------------------------------------------------

    fn parse_header(&mut self) -> Result<u32> {
        let hstart = self.at as u64;
        let arr = self.starts_with(b"[[");
        self.at += if arr { 2 } else { 1 };
        self.skip_spaces();
        let parts = self.parse_dotted_key()?;
        self.skip_spaces();
        if arr {
            if !self.starts_with(b"]]") {
                return Err(corrupt("unterminated array-of-tables header"));
            }
            self.at += 2;
        } else {
            if self.peek() != Some(b']') {
                return Err(corrupt("unterminated table header"));
            }
            self.at += 1;
        }
        let hend = self.at as u64;
        self.resolve_header(&parts, arr, hstart, hend)
    }

    #[allow(clippy::too_many_lines)]
    fn resolve_header(
        &mut self,
        parts: &[KeyTok],
        arr: bool,
        hstart: u64,
        hend: u64,
    ) -> Result<u32> {
        let n = parts.len();
        if n == 0 {
            return Err(corrupt("empty table header"));
        }
        if n as u32 > self.limits.max_toml_depth {
            return Err(Error::resource_limit(format!(
                "TOML table depth exceeds the {}-level cap",
                self.limits.max_toml_depth
            )));
        }
        self.max_depth_seen = self.max_depth_seen.max(n as u32);
        let mut slot: u32 = 0;
        for (i, part) in parts.iter().enumerate() {
            let last = i + 1 == n;
            if last && arr {
                match self.tables[slot as usize].map.get(&part.text).copied() {
                    None => {
                        let cont =
                            self.alloc_table(K_ARRAY_TABLE, hstart, hend, part.start, part.end)?;
                        let keynode = self.new_key_node(part)?;
                        let cnode = self.tables[cont as usize].node;
                        self.link(slot, keynode, cnode);
                        self.tables[slot as usize]
                            .map
                            .insert(part.text.clone(), (cont << 1) | 1);
                        let elem = self.alloc_table(K_TABLE, hstart, hend, part.start, part.end)?;
                        self.tables[cont as usize].elements.push(elem);
                        self.tables[cont as usize].last_elem = elem;
                        let enode = self.tables[elem as usize].node;
                        self.append_element(cont, enode);
                        return Ok(elem);
                    }
                    Some(x) if x & 1 == 1 => {
                        let cont = x >> 1;
                        if !self.tables[cont as usize].is_array {
                            return Err(corrupt("cannot redefine a table as an array of tables"));
                        }
                        let elem = self.alloc_table(K_TABLE, hstart, hend, part.start, part.end)?;
                        self.tables[cont as usize].elements.push(elem);
                        self.tables[cont as usize].last_elem = elem;
                        let enode = self.tables[elem as usize].node;
                        self.append_element(cont, enode);
                        return Ok(elem);
                    }
                    Some(_) => {
                        return Err(corrupt("cannot define an array of tables over a value"));
                    }
                }
            }
            if last {
                match self.tables[slot as usize].map.get(&part.text).copied() {
                    None => {
                        let t = self.alloc_table(K_TABLE, hstart, hend, part.start, part.end)?;
                        let keynode = self.new_key_node(part)?;
                        let tnode = self.tables[t as usize].node;
                        self.link(slot, keynode, tnode);
                        self.tables[slot as usize]
                            .map
                            .insert(part.text.clone(), (t << 1) | 1);
                        self.tables[t as usize].explicit = true;
                        return Ok(t);
                    }
                    Some(x) if x & 1 == 1 => {
                        let t = x >> 1;
                        if self.tables[t as usize].is_array {
                            return Err(corrupt("cannot redefine an array of tables as a table"));
                        }
                        if self.tables[t as usize].explicit {
                            return Err(corrupt("table is defined more than once"));
                        }
                        self.tables[t as usize].explicit = true;
                        return Ok(t);
                    }
                    Some(_) => return Err(corrupt("cannot redefine a value as a table")),
                }
            }
            match self.tables[slot as usize].map.get(&part.text).copied() {
                None => {
                    let t =
                        self.alloc_table(K_TABLE, part.start, part.end, part.start, part.end)?;
                    let keynode = self.new_key_node(part)?;
                    let tnode = self.tables[t as usize].node;
                    self.link(slot, keynode, tnode);
                    self.tables[slot as usize]
                        .map
                        .insert(part.text.clone(), (t << 1) | 1);
                    slot = t;
                }
                Some(x) if x & 1 == 1 => {
                    let t = x >> 1;
                    slot = if self.tables[t as usize].is_array {
                        self.tables[t as usize].last_elem
                    } else {
                        t
                    };
                }
                Some(_) => return Err(corrupt("dotted header path extends a value")),
            }
        }
        Err(corrupt("empty table header"))
    }

    // -- assignments ---------------------------------------------------------

    fn parse_assignment(&mut self, base: u32) -> Result<()> {
        let parts = self.parse_dotted_key()?;
        self.skip_spaces();
        if self.peek() != Some(b'=') {
            return Err(corrupt("expected '=' after a key"));
        }
        self.at += 1;
        self.skip_spaces();
        let value = self.parse_value(1)?;
        self.resolve_assign(base, &parts, value)?;
        self.assignments = self.assignments.saturating_add(1);
        Ok(())
    }

    fn resolve_assign(&mut self, base: u32, parts: &[KeyTok], value: u32) -> Result<()> {
        let n = parts.len();
        if n == 0 {
            return Err(corrupt("empty key"));
        }
        if n as u32 > self.limits.max_toml_depth {
            return Err(Error::resource_limit(format!(
                "TOML key depth exceeds the {}-level cap",
                self.limits.max_toml_depth
            )));
        }
        self.max_depth_seen = self.max_depth_seen.max(n as u32);
        let mut slot = base;
        for part in &parts[..n - 1] {
            match self.tables[slot as usize].map.get(&part.text).copied() {
                None => {
                    let t =
                        self.alloc_table(K_TABLE, part.start, part.end, part.start, part.end)?;
                    let keynode = self.new_key_node(part)?;
                    let tnode = self.tables[t as usize].node;
                    self.link(slot, keynode, tnode);
                    self.tables[slot as usize]
                        .map
                        .insert(part.text.clone(), (t << 1) | 1);
                    self.tables[t as usize].explicit = true;
                    slot = t;
                }
                Some(x) if x & 1 == 1 => {
                    let t = x >> 1;
                    slot = if self.tables[t as usize].is_array {
                        self.tables[t as usize].last_elem
                    } else {
                        t
                    };
                }
                Some(_) => return Err(corrupt("a dotted key extends a value")),
            }
        }
        let last = &parts[n - 1];
        if self.tables[slot as usize].map.contains_key(&last.text) {
            return Err(corrupt("key is defined more than once"));
        }
        let keynode = self.new_key_node(last)?;
        self.link(slot, keynode, value);
        self.tables[slot as usize]
            .map
            .insert(last.text.clone(), value << 1);
        Ok(())
    }

    // -- keys ----------------------------------------------------------------

    fn parse_dotted_key(&mut self) -> Result<Vec<KeyTok>> {
        let mut parts = Vec::new();
        loop {
            self.skip_spaces();
            parts.push(self.parse_key_part()?);
            self.skip_spaces();
            if self.peek() == Some(b'.') {
                self.at += 1;
                continue;
            }
            break;
        }
        Ok(parts)
    }

    fn parse_key_part(&mut self) -> Result<KeyTok> {
        self.skip_spaces();
        let start = self.at as u64;
        match self.peek() {
            Some(b'"') => {
                if self.starts_with(b"\"\"\"") {
                    return Err(corrupt("a key cannot be a multiline string"));
                }
                self.scan_basic(false)?;
                let end = self.at as u64;
                let tok = slice(self.b, start, end)?;
                if tok.len() < 2 {
                    return Err(corrupt("key token is too short"));
                }
                let text = unescape_basic(&tok[1..tok.len() - 1], false)?;
                self.charge_string(tok.len() as u64)?;
                Ok(KeyTok { text, start, end })
            }
            Some(b'\'') => {
                if self.starts_with(b"'''") {
                    return Err(corrupt("a key cannot be a multiline string"));
                }
                self.scan_literal(false)?;
                let end = self.at as u64;
                let tok = slice(self.b, start, end)?;
                if tok.len() < 2 {
                    return Err(corrupt("key token is too short"));
                }
                let text = utf8_string(&tok[1..tok.len() - 1])?;
                self.charge_string(tok.len() as u64)?;
                Ok(KeyTok { text, start, end })
            }
            Some(c) if is_bare_key_char(c) => {
                self.at += 1;
                while matches!(self.peek(), Some(c) if is_bare_key_char(c)) {
                    self.at += 1;
                }
                let end = self.at as u64;
                let tok = slice(self.b, start, end)?;
                let text = utf8_string(tok)?;
                Ok(KeyTok { text, start, end })
            }
            _ => Err(corrupt("expected a key")),
        }
    }

    // -- values --------------------------------------------------------------

    fn parse_value(&mut self, depth: u32) -> Result<u32> {
        if depth > self.limits.max_toml_depth {
            return Err(Error::resource_limit(format!(
                "TOML value nesting exceeds the {}-level cap",
                self.limits.max_toml_depth
            )));
        }
        self.max_depth_seen = self.max_depth_seen.max(depth);
        let start = self.at as u64;
        match self.peek() {
            Some(b'"') => {
                let ml = self.starts_with(b"\"\"\"");
                self.scan_basic(ml)?;
                let end = self.at as u64;
                self.charge_string(end - start)?;
                let kind = if ml {
                    K_STRING_ML_BASIC
                } else {
                    K_STRING_BASIC
                };
                self.new_node(kind, start, end, 0, 0)
            }
            Some(b'\'') => {
                let ml = self.starts_with(b"'''");
                self.scan_literal(ml)?;
                let end = self.at as u64;
                self.charge_string(end - start)?;
                let kind = if ml {
                    K_STRING_ML_LITERAL
                } else {
                    K_STRING_LITERAL
                };
                self.new_node(kind, start, end, 0, 0)
            }
            Some(b'[') => self.parse_array(depth),
            Some(b'{') => self.parse_inline_table(depth),
            Some(b't') | Some(b'f') => {
                let (kw, kind) = if self.starts_with(b"true") {
                    (b"true".as_slice(), K_BOOL)
                } else if self.starts_with(b"false") {
                    (b"false".as_slice(), K_BOOL)
                } else {
                    return Err(corrupt("invalid bare value"));
                };
                self.at += kw.len();
                if !is_value_end(self.b.get(self.at).copied()) {
                    return Err(corrupt("a bare word is not a TOML value"));
                }
                let end = self.at as u64;
                self.new_node(kind, start, end, 0, 0)
            }
            Some(c) if c == b'+' || c == b'-' || c.is_ascii_digit() || c == b'i' || c == b'n' => {
                self.scan_number()?;
                let end = self.at as u64;
                let tok = slice(self.b, start, end)?;
                let kind = classify_numeric(tok)
                    .ok_or_else(|| corrupt("not a valid TOML number or date-time"))?;
                self.new_node(kind, start, end, 0, 0)
            }
            _ => Err(corrupt("expected a value")),
        }
    }

    fn scan_number(&mut self) -> Result<()> {
        let start = self.at;
        loop {
            match self.peek() {
                Some(c)
                    if c.is_ascii_alphanumeric()
                        || matches!(c, b'+' | b'-' | b'_' | b'.' | b':') =>
                {
                    self.at += 1;
                }
                Some(b' ') if is_bare_date(&self.b[start..self.at]) => {
                    if matches!(self.peek_at(1), Some(d) if d.is_ascii_digit()) {
                        self.at += 1;
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }
        if self.at == start {
            return Err(corrupt("empty numeric token"));
        }
        Ok(())
    }

    fn parse_array(&mut self, depth: u32) -> Result<u32> {
        let start = self.at as u64;
        self.at += 1;
        let node = self.new_node(K_ARRAY, start, start, 0, 0)?;
        let mut children: Vec<u32> = Vec::new();
        loop {
            self.skip_ws_nl_comments()?;
            match self.peek() {
                Some(b']') => {
                    self.at += 1;
                    break;
                }
                None => return Err(corrupt("unterminated array")),
                _ => {}
            }
            let v = self.parse_value(depth + 1)?;
            children.push(v);
            self.skip_ws_nl_comments()?;
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    break;
                }
                _ => return Err(corrupt("expected ',' or ']' in an array")),
            }
        }
        let end = self.at as u64;
        if let Some(n) = self.nodes.get_mut(node as usize) {
            n.end = end;
            n.children = children;
        }
        Ok(node)
    }

    fn parse_inline_table(&mut self, depth: u32) -> Result<u32> {
        let start = self.at as u64;
        self.at += 1;
        let node = self.new_node(K_INLINE_TABLE, start, start, 0, 0)?;
        self.skip_spaces();
        if self.peek() == Some(b'}') {
            self.at += 1;
            if let Some(n) = self.nodes.get_mut(node as usize) {
                n.end = self.at as u64;
            }
            return Ok(node);
        }
        loop {
            self.skip_spaces();
            let parts = self.parse_dotted_key()?;
            self.skip_spaces();
            if self.peek() != Some(b'=') {
                return Err(corrupt("expected '=' in an inline table"));
            }
            self.at += 1;
            self.skip_spaces();
            let v = self.parse_value(depth + 1)?;
            self.inline_insert(node, &parts, v)?;
            self.skip_spaces();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    break;
                }
                _ => return Err(corrupt("expected ',' or '}' in an inline table")),
            }
        }
        if let Some(n) = self.nodes.get_mut(node as usize) {
            n.end = self.at as u64;
        }
        Ok(node)
    }

    fn find_entry(&self, table: u32, key: &str) -> Result<Option<u32>> {
        let node = self
            .nodes
            .get(table as usize)
            .ok_or_else(|| corrupt("inline table index is out of range"))?;
        let mut i = 0usize;
        while i + 1 < node.children.len() {
            let k = node.children[i];
            let v = node.children[i + 1];
            i += 2;
            let kn = self
                .nodes
                .get(k as usize)
                .ok_or_else(|| corrupt("inline key out of range"))?;
            if key_text(self.b, kn)? == key {
                return Ok(Some(v));
            }
        }
        Ok(None)
    }

    fn inline_insert(&mut self, table: u32, parts: &[KeyTok], value: u32) -> Result<()> {
        let n = parts.len();
        if n == 0 {
            return Err(corrupt("empty key in an inline table"));
        }
        let mut cur = table;
        for part in &parts[..n - 1] {
            match self.find_entry(cur, &part.text)? {
                Some(v) => {
                    let kind = self
                        .nodes
                        .get(v as usize)
                        .ok_or_else(|| corrupt("inline value out of range"))?
                        .kind;
                    if kind != K_INLINE_TABLE {
                        return Err(corrupt(
                            "a dotted key extends a non-table inside an inline table",
                        ));
                    }
                    cur = v;
                }
                None => {
                    let keynode = self.new_key_node(part)?;
                    let tnode = self.new_node(K_INLINE_TABLE, part.start, part.end, 0, 0)?;
                    if let Some(t) = self.nodes.get_mut(cur as usize) {
                        t.children.push(keynode);
                        t.children.push(tnode);
                    }
                    cur = tnode;
                }
            }
        }
        let last = &parts[n - 1];
        if self.find_entry(cur, &last.text)?.is_some() {
            return Err(corrupt("duplicate key in an inline table"));
        }
        let keynode = self.new_key_node(last)?;
        if let Some(t) = self.nodes.get_mut(cur as usize) {
            t.children.push(keynode);
            t.children.push(value);
        }
        Ok(())
    }

    fn skip_ws_nl_comments(&mut self) -> Result<()> {
        loop {
            match self.peek() {
                Some(b' ') | Some(b'\t') | Some(b'\n') => self.at += 1,
                Some(b'\r') => {
                    if self.peek_at(1) == Some(b'\n') {
                        self.at += 2;
                    } else {
                        return Err(corrupt("a lone carriage return is not a line ending"));
                    }
                }
                Some(b'#') => self.parse_comment(),
                _ => break,
            }
        }
        Ok(())
    }

    // -- string scanners -----------------------------------------------------

    fn scan_escape(&mut self, multiline: bool) -> Result<()> {
        let e = self
            .peek()
            .ok_or_else(|| corrupt("string ends inside an escape"))?;
        match e {
            b'"' | b'\\' | b'b' | b't' | b'n' | b'f' | b'r' => self.at += 1,
            b'u' => {
                self.at += 1;
                self.skip_hex(4)?;
            }
            b'U' => {
                self.at += 1;
                self.skip_hex(8)?;
            }
            b' ' | b'\t' | b'\n' | b'\r' if multiline => {
                while matches!(self.peek(), Some(b' ') | Some(b'\t')) {
                    self.at += 1;
                }
                match self.peek() {
                    Some(b'\r') => {
                        self.at += 1;
                        if self.peek() == Some(b'\n') {
                            self.at += 1;
                        }
                    }
                    Some(b'\n') => self.at += 1,
                    _ => {
                        return Err(corrupt(
                            "a multiline line-ending backslash must be followed by a newline",
                        ));
                    }
                }
            }
            _ => return Err(corrupt("invalid string escape")),
        }
        Ok(())
    }

    fn skip_hex(&mut self, n: usize) -> Result<()> {
        for _ in 0..n {
            match self.peek() {
                Some(c) if c.is_ascii_hexdigit() => self.at += 1,
                _ => return Err(corrupt("escape has a non-hex digit")),
            }
        }
        Ok(())
    }

    fn scan_basic(&mut self, multiline: bool) -> Result<()> {
        if multiline {
            self.at += 3;
            loop {
                if self.starts_with(b"\"\"\"") {
                    let mut q = 0usize;
                    while self.peek_at(q) == Some(b'"') {
                        q += 1;
                    }
                    if q > 5 {
                        return Err(corrupt("too many quotes closing a multiline string"));
                    }
                    self.at += q;
                    return Ok(());
                }
                match self.peek() {
                    None => return Err(corrupt("unterminated multiline basic string")),
                    Some(b'\\') => {
                        self.at += 1;
                        self.scan_escape(true)?;
                    }
                    Some(_) => self.at += 1,
                }
            }
        } else {
            self.at += 1;
            loop {
                match self.peek() {
                    None => return Err(corrupt("unterminated basic string")),
                    Some(b'"') => {
                        self.at += 1;
                        return Ok(());
                    }
                    Some(b'\\') => {
                        self.at += 1;
                        self.scan_escape(false)?;
                    }
                    Some(c) if (c < 0x20 && c != b'\t') || c == 0x7F => {
                        return Err(corrupt("control character in string"));
                    }
                    Some(_) => self.at += 1,
                }
            }
        }
    }

    fn scan_literal(&mut self, multiline: bool) -> Result<()> {
        if multiline {
            self.at += 3;
            loop {
                if self.starts_with(b"'''") {
                    let mut q = 0usize;
                    while self.peek_at(q) == Some(b'\'') {
                        q += 1;
                    }
                    if q > 5 {
                        return Err(corrupt("too many quotes closing a multiline string"));
                    }
                    self.at += q;
                    return Ok(());
                }
                match self.peek() {
                    None => return Err(corrupt("unterminated multiline literal string")),
                    Some(_) => self.at += 1,
                }
            }
        } else {
            self.at += 1;
            loop {
                match self.peek() {
                    None => return Err(corrupt("unterminated literal string")),
                    Some(b'\'') => {
                        self.at += 1;
                        return Ok(());
                    }
                    Some(c) if (c < 0x20 && c != b'\t') || c == 0x7F => {
                        return Err(corrupt("control character in string"));
                    }
                    Some(_) => self.at += 1,
                }
            }
        }
    }
}

fn is_bare_key_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'-' || c == b'_'
}

fn is_value_end(c: Option<u8>) -> bool {
    match c {
        None => true,
        // A bare value (`true`/`false`) must be followed by a value terminator: end
        // of input, whitespace, a comment, or — inside an array or inline table — a
        // structural `,`/`]`/`}` with no intervening space (`[true,false]`,
        // `{ a = true, b = false }`). Omitting the structural terminators declined
        // every valid TOML document that packed a boolean against `,`.
        Some(c) => matches!(c, b' ' | b'\t' | b'\n' | b'\r' | b'#' | b',' | b']' | b'}'),
    }
}

// ---------------------------------------------------------------------------
// Binary reader
// ---------------------------------------------------------------------------

struct BinReader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> BinReader<'a> {
    fn new(b: &'a [u8]) -> Self {
        BinReader { b, at: 0 }
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let out = self
            .b
            .get(self.at..self.at + n)
            .ok_or_else(|| corrupt("truncated model"))?;
        self.at += n;
        Ok(out)
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

    fn model(src: &[u8]) -> TomlModel {
        parse(src, Limits::DEFAULT).expect("valid TOML")
    }

    #[test]
    fn detects_toml_and_rejects_non_toml() {
        assert!(detect(b"a = 1\n", Limits::DEFAULT));
        assert!(detect(b"[t]\nx = \"y\"\n", Limits::DEFAULT));
        // Comment-only / header-only / empty stay Opaque (no assignment).
        assert!(!detect(b"# just a comment\n", Limits::DEFAULT));
        assert!(!detect(b"[only.a.header]\n", Limits::DEFAULT));
        assert!(!detect(b"", Limits::DEFAULT));
        // Plain prose is not TOML.
        assert!(!detect(b"plain prose paragraph\n", Limits::DEFAULT));
        assert!(!detect(b"this is not = toml\n", Limits::DEFAULT));
    }

    #[test]
    fn preserves_spelling_and_spans() {
        let src = b"count = 1_000\nbig = 0x1F\nx = nan\nf = 1.0e3\ns = \"a\\tb\"\n";
        let m = model(src);
        let r = resolve_path(&m, src, "count").unwrap();
        assert_eq!(
            scalar_spelling(src, m.node(r.index).unwrap()).unwrap(),
            "1_000"
        );
        let r = resolve_path(&m, src, "big").unwrap();
        assert_eq!(
            scalar_spelling(src, m.node(r.index).unwrap()).unwrap(),
            "0x1F"
        );
        let r = resolve_path(&m, src, "x").unwrap();
        assert_eq!(m.node(r.index).unwrap().kind, K_FLOAT);
        assert_eq!(
            scalar_spelling(src, m.node(r.index).unwrap()).unwrap(),
            "nan"
        );
        let r = resolve_path(&m, src, "s").unwrap();
        assert_eq!(
            string_content(src, m.node(r.index).unwrap()).unwrap(),
            "a\tb"
        );
    }

    #[test]
    fn dotted_keys_tables_and_array_tables() {
        let src = b"a.b.c = 1\n[t]\nx = 2\n[[s]]\nh = \"one\"\n[[s]]\nh = \"two\"\n";
        let m = model(src);
        let r = resolve_path(&m, src, "a.b.c").unwrap();
        assert_eq!(scalar_spelling(src, m.node(r.index).unwrap()).unwrap(), "1");
        let r = resolve_path(&m, src, "t.x").unwrap();
        assert_eq!(scalar_spelling(src, m.node(r.index).unwrap()).unwrap(), "2");
        let r = resolve_path(&m, src, "s[1].h").unwrap();
        assert_eq!(
            string_content(src, m.node(r.index).unwrap()).unwrap(),
            "two"
        );
        let r = resolve_path(&m, src, "t").unwrap();
        let keys = table_keys(&m, src, r.index).unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "x");
    }

    #[test]
    fn duplicate_and_redefinition_decline_typed() {
        assert!(parse(b"a = 1\na = 2\n", Limits::DEFAULT).is_err());
        assert!(parse(b"[t]\nx = 1\n[t]\ny = 2\n", Limits::DEFAULT).is_err());
        assert!(parse(b"a = 1\n[a]\n", Limits::DEFAULT).is_err());
        assert!(parse(b"[a]\n[a]\n", Limits::DEFAULT).is_err());
    }

    #[test]
    fn comments_are_preserved() {
        let src = b"a = 1 # trailing\n# whole line\nb = 2\n";
        let m = model(src);
        assert_eq!(m.comments.len(), 2);
        assert_eq!(
            &src[m.comments[0].start as usize..m.comments[0].end as usize],
            b"# trailing"
        );
        assert_eq!(
            &src[m.comments[1].start as usize..m.comments[1].end as usize],
            b"# whole line"
        );
    }

    #[test]
    fn inline_tables_arrays_and_dates() {
        let src =
            b"p = { x = 1, y.z = 2 }\nd = 1979-05-27T07:32:00Z\nl = 07:32:00\na = [1, 2, 3]\n";
        let m = model(src);
        let r = resolve_path(&m, src, "p.y.z").unwrap();
        assert_eq!(scalar_spelling(src, m.node(r.index).unwrap()).unwrap(), "2");
        let r = resolve_path(&m, src, "d").unwrap();
        assert_eq!(m.node(r.index).unwrap().kind, K_DATETIME);
        let r = resolve_path(&m, src, "l").unwrap();
        assert_eq!(m.node(r.index).unwrap().kind, K_DATETIME);
        let r = resolve_path(&m, src, "a[2]").unwrap();
        assert_eq!(scalar_spelling(src, m.node(r.index).unwrap()).unwrap(), "3");
    }

    #[test]
    fn model_roundtrips() {
        let src = b"a = 1\n[t]\nb = \"x\" # c\n";
        let m = model(src);
        let enc = m.encode();
        let dec = TomlModel::decode(&enc).unwrap();
        assert_eq!(m, dec);
    }

    #[test]
    fn deep_nesting_declines_typed() {
        let mut src = String::new();
        src.push('a');
        for _ in 0..200 {
            src.push_str(".b");
        }
        src.push_str(" = 1\n");
        let e = parse(src.as_bytes(), Limits::STRICT).unwrap_err();
        assert_eq!(e.class(), crate::error::ErrorClass::ResourceLimit);
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0x1234_5678_9ABC_DEF0;
        for _ in 0..128 {
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
                let _ = build_toml_model(&buf, Limits::STRICT);
            }
        }
    }
}
