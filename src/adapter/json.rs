//! Bounded, representation-preserving JSON adapter (Phase 21.5.1).
//!
//! JSON is the first **Wave-2 structured-tree** format. It is *not* an office
//! package: there is no OPC/ZIP layer, no `mimetype`, and no relationship graph.
//! The exact leaf is therefore the **whole source** (a `DocumentExact`, a
//! RAW-like authority), and everything this module produces is a bounded,
//! deterministic (`Q_gen`) projection that never sits on the exactness path.
//!
//! ## Why a bespoke parser
//!
//! The point of a JSON adapter is to preserve **representation**, not merely
//! values. For every token the parser records its exact **byte span** in the
//! source, so it preserves, and can report:
//!
//! * object **member order** (never normalized or sorted);
//! * **duplicate keys** (kept as distinct members, never dropped/overwritten);
//! * **whitespace** (each token's span is exact, so inter-token whitespace is
//!   recoverable as the gaps);
//! * **numeric spelling** (`1e3`, `1.0`, `-0` are kept as their literal text,
//!   never parsed into a binary float);
//! * **string-escape spelling** (`\u00e9` is distinct from `é`).
//!
//! The parser is a bounded recursive-descent scanner. Recursion is safe because
//! the nesting depth is capped by [`Limits::max_json_depth`]; the total node
//! count by [`Limits::max_json_nodes`]; the raw string bytes by
//! [`Limits::max_json_string_bytes`]; and the source length by
//! [`Limits::max_json_document_bytes`]. Untrusted input can only ever yield a
//! typed decline, never a panic or an unbounded allocation.
//!
//! ## Detection
//!
//! [`detect`] is deliberately conservative: the whole source must parse as
//! **exactly one** JSON value (trailing bytes other than whitespace reject), and
//! it must stay within every cap. Anything else is not JSON. Detection runs the
//! same parser with node-building disabled, so it is O(1) in extra memory and
//! cannot be forced to allocate by a large near-JSON input.

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model nodes (defends the decoder against a hostile blob).
pub const MAX_MODEL_NODES: u32 = 1 << 24;

/// JSON node kind: an object (`{ … }`).
pub const K_OBJECT: u8 = 0;
/// JSON node kind: an array (`[ … ]`).
pub const K_ARRAY: u8 = 1;
/// JSON node kind: a string (`"…"`).
pub const K_STRING: u8 = 2;
/// JSON node kind: a number.
pub const K_NUMBER: u8 = 3;
/// JSON node kind: the literal `true`.
pub const K_TRUE: u8 = 4;
/// JSON node kind: the literal `false`.
pub const K_FALSE: u8 = 5;
/// JSON node kind: the literal `null`.
pub const K_NULL: u8 = 6;

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
        _ => "unknown",
    }
}

/// Whether `kind` is a container (has children).
pub const fn is_container(kind: u8) -> bool {
    matches!(kind, K_OBJECT | K_ARRAY)
}

/// One parsed JSON node: its kind, its exact source token span, and its children.
///
/// For an object, `children` is the interleaved document-order list
/// `[key0, value0, key1, value1, …]`, so member order and duplicate keys are both
/// preserved verbatim. For an array, `children` is the element list in order. For
/// a scalar, `children` is empty and `[start, end)` is the exact token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JNode {
    /// One of the `K_*` tags.
    pub kind: u8,
    /// The token's first source byte (for a string, the opening quote).
    pub start: u64,
    /// One past the token's last source byte (for a string, past the close quote).
    pub end: u64,
    /// Object: interleaved key/value indices; array: element indices.
    pub children: Vec<u32>,
}

/// The canonical derived JSON model (the materialization of a `JsonModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonModel {
    /// Index of the root node.
    pub root: u32,
    /// The root's kind tag.
    pub top_type: u8,
    /// The observed maximum container nesting depth (root = 1).
    pub max_depth: u32,
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// The node arena.
    pub nodes: Vec<JNode>,
}

impl JsonModel {
    /// The node at `index`, if present.
    pub fn node(&self, index: u32) -> Option<&JNode> {
        self.nodes.get(index as usize)
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32 + self.nodes.len() * 24);
        out.extend_from_slice(b"JSONM");
        out.push(MODEL_VERSION);
        out.extend_from_slice(&self.root.to_le_bytes());
        out.push(self.top_type);
        out.extend_from_slice(&self.max_depth.to_le_bytes());
        out.extend_from_slice(&self.doc_len.to_le_bytes());
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
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<JsonModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(5)? != b"JSONM" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let root = r.u32()?;
        let top_type = r.u8()?;
        if top_type > K_NULL {
            return Err(corrupt("unknown top-level kind"));
        }
        let max_depth = r.u32()?;
        let doc_len = r.u64()?;
        let count = r.u32()?;
        if count > MAX_MODEL_NODES {
            return Err(corrupt("model node count is implausible"));
        }
        let mut nodes = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let kind = r.u8()?;
            if kind > K_NULL {
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
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        if root as usize >= nodes.len() {
            return Err(corrupt("root index is out of range"));
        }
        Ok(JsonModel {
            root,
            top_type,
            max_depth,
            doc_len,
            nodes,
        })
    }
}

/// A resolved pointer query: the node index and how many members matched the
/// final key segment (so duplicate keys are reported, never hidden).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolved {
    /// The resolved node index.
    pub index: u32,
    /// The number of object members with the final segment's key (`1` for an
    /// array index or the root; `>1` witnesses duplicate keys).
    pub matches: u32,
}

/// One lexical-match record from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonMatch {
    /// Canonical RFC 6901 pointer to the matching node.
    pub pointer: String,
    /// Whether the match is an object key or a string value.
    pub role: MatchRole,
    /// The matching node's kind (always `K_STRING`).
    pub kind: u8,
    /// The exact source span of the matching string token.
    pub start: u64,
    /// One past the matching string token.
    pub end: u64,
    /// The decoded text of the matching string.
    pub text: String,
}

/// Whether a lexical match is an object member key or a string value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchRole {
    /// An object member key.
    Key,
    /// A string value (object member value or array element).
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

/// Fast-fail JSON detector: does the whole `source` parse as exactly one JSON
/// value within `limits`? Conservative by construction.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_json_document_bytes {
        return false;
    }
    parse(source, limits, false).is_ok()
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of a `JsonModel` node).
pub fn build_json_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits, true)?.encode())
}

/// Parse `source` into a [`JsonModel`]. `build` selects whether the node arena is
/// populated (detection runs with `build = false` to stay O(1) in memory).
pub fn parse(source: &[u8], limits: Limits, build: bool) -> Result<JsonModel> {
    if source.len() as u64 > limits.max_json_document_bytes {
        return Err(Error::resource_limit(format!(
            "JSON source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_json_document_bytes
        )));
    }
    let mut p = Parser::new(source, limits, build);
    p.skip_ws();
    if p.at >= p.b.len() {
        return Err(corrupt("empty input is not a JSON value"));
    }
    let root = p.value(1)?;
    p.skip_ws();
    if p.at != p.b.len() {
        return Err(corrupt("trailing bytes after the single JSON value"));
    }
    Ok(JsonModel {
        root,
        top_type: p.nodes.first().map_or(0, |_| p.root_kind),
        max_depth: p.max_depth_seen,
        doc_len: source.len() as u64,
        nodes: p.nodes,
    })
}

/// RFC 6901 pointer resolution against a parsed model. A missing node, an
/// out-of-range array index, or indexing into a scalar is a typed decline
/// (never a silent empty answer).
pub fn resolve_pointer(model: &JsonModel, source: &[u8], pointer: &str) -> Result<Resolved> {
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
                    if key_node.kind != K_STRING {
                        continue;
                    }
                    let key = decode_string(source, key_node)?;
                    if key == *seg {
                        found = found.saturating_add(1);
                        if first.is_none() {
                            first = Some(val_idx);
                        }
                    }
                }
                index = first.ok_or_else(|| {
                    Error::unsupported_feature(format!(
                        "JSON object has no member {seg:?} at this pointer"
                    ))
                })?;
                matches = found;
            }
            K_ARRAY => {
                let idx = parse_array_index(seg)?;
                let len = node.children.len();
                if idx as usize >= len {
                    return Err(Error::unsupported_feature(format!(
                        "JSON array index {idx} is out of range (length {len})"
                    )));
                }
                index = node.children[idx as usize];
                matches = 1;
            }
            _ => {
                return Err(Error::unsupported_feature(format!(
                    "cannot index into a JSON {} at {seg:?}",
                    kind_name(node.kind)
                )));
            }
        }
    }
    Ok(Resolved { index, matches })
}

/// The exact source bytes of a node's token (`[start, end)`), bounded by the
/// document length.
pub fn token_bytes<'a>(source: &'a [u8], node: &JNode) -> Result<&'a [u8]> {
    let s = usize::try_from(node.start).map_err(|_| corrupt("span overflow"))?;
    let e = usize::try_from(node.end).map_err(|_| corrupt("span overflow"))?;
    source
        .get(s..e)
        .ok_or_else(|| corrupt("node span is outside the source"))
}

/// Decode a JSON string node's token into its Rust text (escapes resolved,
/// `\uXXXX` surrogate pairs combined). Never panics; a malformed token is typed.
pub fn decode_string(source: &[u8], node: &JNode) -> Result<String> {
    let tok = token_bytes(source, node)?;
    if node.kind != K_STRING || tok.len() < 2 || tok[0] != b'"' || tok[tok.len() - 1] != b'"' {
        return Err(corrupt("node is not a well-formed JSON string token"));
    }
    unescape(&tok[1..tok.len() - 1])
}

/// Render a deterministic canonical text projection of the whole model: member
/// order and token spelling (numeric/escape) are preserved, separators are
/// canonical (`:`, `,`, no insignificant whitespace).
pub fn canonical_text(model: &JsonModel, source: &[u8]) -> Result<String> {
    subtree_text(model, source, model.root)
}

/// Render one node's subtree to canonical text (see [`canonical_text`]).
pub fn subtree_text(model: &JsonModel, source: &[u8], index: u32) -> Result<String> {
    let mut out = String::new();
    render(model, source, index, &mut out, 0)?;
    Ok(out)
}

/// The index of a node's parent, if any (the root has none).
pub fn find_parent(model: &JsonModel, index: u32) -> Option<u32> {
    model
        .nodes
        .iter()
        .position(|n| n.children.contains(&index))
        .map(|i| i as u32)
}

fn render(
    model: &JsonModel,
    source: &[u8],
    index: u32,
    out: &mut String,
    depth: u32,
) -> Result<()> {
    if depth > MAX_MODEL_NODES {
        return Err(Error::resource_limit(
            "JSON render exceeded its depth bound",
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

/// A bounded, case-sensitive lexical search over object keys and string values.
/// Returns matches in document order (object members before descendants), each
/// with its canonical pointer and exact source span.
pub fn find(
    model: &JsonModel,
    source: &[u8],
    pattern: &str,
    limits: Limits,
) -> Result<Vec<JsonMatch>> {
    let mut out = Vec::new();
    let mut path: Vec<String> = Vec::new();
    walk_find(
        model, source, model.root, true, &mut path, pattern, &mut out, limits, 0,
    )?;
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn walk_find(
    model: &JsonModel,
    source: &[u8],
    index: u32,
    is_value: bool,
    path: &mut Vec<String>,
    pattern: &str,
    out: &mut Vec<JsonMatch>,
    limits: Limits,
    depth: u32,
) -> Result<()> {
    if depth > limits.max_json_depth {
        return Err(Error::resource_limit(
            "JSON find exceeded the nesting-depth cap",
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
                let key = decode_string(source, key_node)?;
                if key.contains(pattern) {
                    out.push(JsonMatch {
                        pointer: pointer_of(path, Some(&key)),
                        role: MatchRole::Key,
                        kind: K_STRING,
                        start: key_node.start,
                        end: key_node.end,
                        text: key.clone(),
                    });
                }
                path.push(escape_segment(&key));
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
                out.push(JsonMatch {
                    pointer: pointer_of(path, None),
                    role: MatchRole::Value,
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
        s.push_str(&escape_segment(k));
    }
    s
}

/// Escape one pointer segment per RFC 6901 (`~` → `~0`, `/` → `~1`).
pub fn escape_segment(seg: &str) -> String {
    let mut out = String::with_capacity(seg.len());
    for c in seg.chars() {
        match c {
            '~' => out.push_str("~0"),
            '/' => out.push_str("~1"),
            _ => out.push(c),
        }
    }
    out
}

/// Parse an RFC 6901 pointer into decoded segments.
fn parse_pointer(pointer: &str) -> Result<Vec<String>> {
    if pointer.is_empty() {
        return Ok(Vec::new());
    }
    if !pointer.starts_with('/') {
        return Err(Error::usage(format!(
            "JSON pointer {pointer:?} must be empty or start with '/'"
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
                        "invalid JSON pointer escape in segment {raw:?}"
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
            "JSON array segment {seg:?} is not a canonical index"
        )));
    }
    seg.parse::<u32>()
        .map_err(|_| Error::usage(format!("JSON array index {seg:?} is not a u32")))
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
            b'\\' => out.push(b'\\'),
            b'/' => out.push(b'/'),
            b'b' => out.push(0x08),
            b'f' => out.push(0x0C),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'u' => {
                let cp = read_hex4(inner, &mut i)?;
                if (0xD800..0xDC00).contains(&cp) {
                    // High surrogate: a low surrogate must follow.
                    if inner.get(i) != Some(&b'\\') || inner.get(i + 1) != Some(&b'u') {
                        return Err(corrupt("lone high surrogate in string"));
                    }
                    i += 2;
                    let lo = read_hex4(inner, &mut i)?;
                    if !(0xDC00..0xE000).contains(&lo) {
                        return Err(corrupt("high surrogate not followed by a low surrogate"));
                    }
                    let combined = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                    let ch = char::from_u32(combined)
                        .ok_or_else(|| corrupt("invalid surrogate pair"))?;
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                } else if (0xDC00..0xE000).contains(&cp) {
                    return Err(corrupt("lone low surrogate in string"));
                } else {
                    let ch = char::from_u32(cp).ok_or_else(|| corrupt("invalid code point"))?;
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                }
            }
            _ => return Err(corrupt("invalid string escape")),
        }
    }
    String::from_utf8(out).map_err(|_| corrupt("string is not valid UTF-8"))
}

fn read_hex4(inner: &[u8], i: &mut usize) -> Result<u32> {
    let mut v = 0u32;
    for _ in 0..4 {
        let b = *inner
            .get(*i)
            .ok_or_else(|| corrupt("truncated \\u escape"))?;
        *i += 1;
        let d = match b {
            b'0'..=b'9' => (b - b'0') as u32,
            b'a'..=b'f' => (b - b'a' + 10) as u32,
            b'A'..=b'F' => (b - b'A' + 10) as u32,
            _ => return Err(corrupt("\\u escape has a non-hex digit")),
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
    max_depth_seen: u32,
    root_kind: u8,
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
            max_depth_seen: 0,
            root_kind: 0,
        }
    }

    fn skip_ws(&mut self) {
        while let Some(&c) = self.b.get(self.at) {
            match c {
                b' ' | b'\t' | b'\n' | b'\r' => self.at += 1,
                _ => break,
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.at).copied()
    }

    fn bump_count(&mut self) -> Result<()> {
        self.count = self.count.saturating_add(1);
        if self.count > self.limits.max_json_nodes as u64 {
            return Err(Error::resource_limit(format!(
                "JSON document exceeds the {}-node cap",
                self.limits.max_json_nodes
            )));
        }
        Ok(())
    }

    fn value(&mut self, depth: u32) -> Result<u32> {
        if depth > self.limits.max_json_depth {
            return Err(Error::resource_limit(format!(
                "JSON nesting exceeds the {}-level cap",
                self.limits.max_json_depth
            )));
        }
        self.max_depth_seen = self.max_depth_seen.max(depth);
        self.skip_ws();
        let start = self.at as u64;
        let c = self
            .peek()
            .ok_or_else(|| corrupt("unexpected end of input"))?;
        match c {
            b'{' => self.object(depth, start),
            b'[' => self.array(depth, start),
            b'"' => {
                self.string_token()?;
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
            b'-' | b'0'..=b'9' => {
                self.number()?;
                self.scalar(K_NUMBER, start, self.at as u64)
            }
            _ => Err(corrupt("not a JSON value")),
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
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(self.finish(idx, children, self.at as u64));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                return Err(corrupt("object member key must be a string"));
            }
            let kstart = self.at as u64;
            self.string_token()?;
            let key = self.scalar(K_STRING, kstart, self.at as u64)?;
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Err(corrupt("object member key must be followed by ':'"));
            }
            self.at += 1;
            let val = self.value(depth + 1)?;
            if self.build {
                children.push(key);
                children.push(val);
            }
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.at += 1,
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
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(self.finish(idx, children, self.at as u64));
        }
        loop {
            let val = self.value(depth + 1)?;
            if self.build {
                children.push(val);
            }
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.at += 1,
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
            return Err(corrupt("invalid JSON keyword"));
        }
        self.at = end;
        Ok(())
    }

    fn number(&mut self) -> Result<()> {
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        match self.peek() {
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.at += 1;
                }
            }
            _ => return Err(corrupt("number has no integer part")),
        }
        if self.peek() == Some(b'.') {
            self.at += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(corrupt("number fraction has no digits"));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.at += 1;
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
        Ok(())
    }

    /// Parse one string token, validating escapes and UTF-8, and charge its raw
    /// byte length against `max_json_string_bytes`.
    fn string_token(&mut self) -> Result<()> {
        let open = self.at;
        self.at += 1; // consume opening quote
        loop {
            let c = self
                .peek()
                .ok_or_else(|| corrupt("unterminated JSON string"))?;
            match c {
                b'"' => {
                    self.at += 1;
                    break;
                }
                b'\\' => {
                    self.at += 1;
                    let e = self
                        .peek()
                        .ok_or_else(|| corrupt("string ends inside an escape"))?;
                    match e {
                        b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => self.at += 1,
                        b'u' => {
                            self.at += 1;
                            for _ in 0..4 {
                                let h =
                                    self.peek().ok_or_else(|| corrupt("truncated \\u escape"))?;
                                if !h.is_ascii_hexdigit() {
                                    return Err(corrupt("\\u escape has a non-hex digit"));
                                }
                                self.at += 1;
                            }
                        }
                        _ => return Err(corrupt("invalid string escape")),
                    }
                }
                0x00..=0x1F => {
                    return Err(corrupt("unescaped control character in string"));
                }
                _ => self.at += 1,
            }
        }
        // The raw inner bytes must be valid UTF-8 (escapes are ASCII, so this
        // checks the literal multi-byte characters).
        let inner = self
            .b
            .get(open + 1..self.at - 1)
            .ok_or_else(|| corrupt("string span overflow"))?;
        core::str::from_utf8(inner).map_err(|_| corrupt("string is not valid UTF-8"))?;
        self.string_bytes = self.string_bytes.saturating_add(inner.len() as u64);
        if self.string_bytes > self.limits.max_json_string_bytes {
            return Err(Error::resource_limit(format!(
                "JSON string bytes exceed the {}-byte cap",
                self.limits.max_json_string_bytes
            )));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn corrupt(msg: &str) -> Error {
    Error::invalid_json_structure(format!("malformed JSON: {msg}"))
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

    fn model(src: &[u8]) -> JsonModel {
        parse(src, Limits::DEFAULT, true).unwrap()
    }

    #[test]
    fn detects_simple_documents_and_rejects_others() {
        assert!(detect(b"{\"a\":1}", Limits::DEFAULT));
        assert!(detect(b"  [1, 2, 3]\n", Limits::DEFAULT));
        assert!(detect(b"42", Limits::DEFAULT));
        assert!(detect(b"\"hi\"", Limits::DEFAULT));
        assert!(detect(b"true", Limits::DEFAULT));
        assert!(detect(b"null", Limits::DEFAULT));
        assert!(!detect(b"", Limits::DEFAULT));
        assert!(!detect(b"{", Limits::DEFAULT));
        assert!(!detect(b"{\"a\":1} trailing", Limits::DEFAULT));
        assert!(!detect(b"[1,2,]", Limits::DEFAULT));
        assert!(!detect(b"not json", Limits::DEFAULT));
        assert!(!detect(b"01", Limits::DEFAULT));
        assert!(!detect(b"+1", Limits::DEFAULT));
    }

    #[test]
    fn preserves_spelling_order_and_duplicate_keys() {
        let src = br#"{"b":1e3,"a":1.0,"a":-0,"c":"\u00e9"}"#;
        let m = model(src);
        assert_eq!(m.top_type, K_OBJECT);
        let root = m.node(m.root).unwrap();
        // 4 members = 8 children, duplicates kept.
        assert_eq!(root.children.len(), 8);
        // Member order preserved: first key is "b".
        let k0 = m.node(root.children[0]).unwrap();
        assert_eq!(token_bytes(src, k0).unwrap(), br#""b""#);
        // Numeric spelling preserved verbatim.
        let v0 = m.node(root.children[1]).unwrap();
        assert_eq!(token_bytes(src, v0).unwrap(), b"1e3");
        // The two "a" keys are distinct children.
        let k1 = m.node(root.children[2]).unwrap();
        let k2 = m.node(root.children[4]).unwrap();
        assert_eq!(token_bytes(src, k1).unwrap(), br#""a""#);
        assert_eq!(token_bytes(src, k2).unwrap(), br#""a""#);
        let v1 = m.node(root.children[3]).unwrap();
        let v2 = m.node(root.children[5]).unwrap();
        assert_eq!(token_bytes(src, v1).unwrap(), b"1.0");
        assert_eq!(token_bytes(src, v2).unwrap(), b"-0");
        // Escape spelling preserved verbatim; decoded value is `é`.
        let v3 = m.node(root.children[7]).unwrap();
        assert_eq!(token_bytes(src, v3).unwrap(), br#""\u00e9""#);
        assert_eq!(decode_string(src, v3).unwrap(), "é");
    }

    #[test]
    fn pointer_resolution_handles_escapes_and_indices() {
        let src = br#"{"a/b":0,"c":{"~x":[10,20,30]}}"#;
        let m = model(src);
        let r = resolve_pointer(&m, src, "/a~1b").unwrap();
        assert_eq!(token_bytes(src, m.node(r.index).unwrap()).unwrap(), b"0");
        let r = resolve_pointer(&m, src, "/c/~0x/2").unwrap();
        assert_eq!(token_bytes(src, m.node(r.index).unwrap()).unwrap(), b"30");
        // Out of range and missing are typed declines.
        assert!(resolve_pointer(&m, src, "/c/~0x/9").is_err());
        assert!(resolve_pointer(&m, src, "/nope").is_err());
        assert!(resolve_pointer(&m, src, "no-slash").is_err());
    }

    #[test]
    fn duplicate_key_pointer_reports_matches() {
        let src = br#"{"a":1,"a":2}"#;
        let m = model(src);
        let r = resolve_pointer(&m, src, "/a").unwrap();
        assert_eq!(r.matches, 2);
        assert_eq!(token_bytes(src, m.node(r.index).unwrap()).unwrap(), b"1");
    }

    #[test]
    fn canonical_text_is_deterministic_and_order_preserving() {
        let src = b"{\n  \"b\" : 1e3 ,\n  \"a\" : [ 1 , 2 ]\n}";
        let m = model(src);
        assert_eq!(canonical_text(&m, src).unwrap(), r#"{"b":1e3,"a":[1,2]}"#);
    }

    #[test]
    fn find_matches_keys_and_string_values() {
        let src = br#"{"hello":"world","k":[ "hello" ]}"#;
        let m = model(src);
        let ms = find(&m, src, "hello", Limits::DEFAULT).unwrap();
        assert_eq!(ms.len(), 2);
        assert_eq!(ms[0].role, MatchRole::Key);
        assert_eq!(ms[0].pointer, "/hello");
        assert_eq!(ms[1].role, MatchRole::Value);
        assert_eq!(ms[1].pointer, "/k/0");
    }

    #[test]
    fn deep_nesting_declines_typed() {
        let mut src = Vec::new();
        src.extend(std::iter::repeat_n(b'[', 200));
        src.push(b'0');
        src.extend(std::iter::repeat_n(b']', 200));
        assert!(!detect(&src, Limits::STRICT));
        let e = parse(&src, Limits::STRICT, true).unwrap_err();
        assert_eq!(e.class(), crate::error::ErrorClass::ResourceLimit);
    }

    #[test]
    fn model_roundtrips() {
        let src = br#"{"z":[true,false,null,1.5],"a":{"b":"c"}}"#;
        let m = model(src);
        let bytes = m.encode();
        assert_eq!(JsonModel::decode(&bytes).unwrap(), m);
        // Truncation is a typed decline, never a panic.
        for cut in 0..bytes.len() {
            let _ = JsonModel::decode(&bytes[..cut]);
        }
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        for _ in 0..512 {
            let mut buf = vec![0u8; 256];
            for b in buf.iter_mut() {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                *b = (x & 0xFF) as u8;
            }
            let _ = detect(&buf, Limits::STRICT);
            if let Ok(m) = parse(&buf, Limits::STRICT, true) {
                let _ = m.encode();
                let _ = canonical_text(&m, &buf);
                let _ = find(&m, &buf, "a", Limits::STRICT);
            }
        }
    }
}
