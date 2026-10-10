//! Bounded, representation-preserving YAML adapter (Phase 21.6.1).
//!
//! YAML is the second **Wave-2 structured-tree** format. Like JSON it is *not* an
//! office package: there is no OPC/ZIP layer, no `mimetype`, and no relationship
//! graph. The exact leaf is the **whole source** (a `DocumentExact`, a RAW-like
//! authority), and everything this module produces is a bounded, deterministic
//! (`Q_gen`) projection that never sits on the exactness path.
//!
//! ## Why a bespoke parser
//!
//! The point of a YAML adapter is to preserve **representation**, not merely the
//! value a "YAML → native object" pipeline would yield. A conventional stack
//! *expands* anchors/aliases, *drops* tags and comments, *normalizes* scalar
//! styles, and *merges* `<<` merge keys. This parser instead records, for every
//! node, its exact **byte span** in the source, and preserves:
//!
//! * **anchors & aliases** (`&a` / `*a`) as a graph — an alias node reports the
//!   anchor it targets and is never expanded and rewritten;
//! * **tags** (`!!str`, `!<...>`, `!custom`) as the literal tag text;
//! * **multiple documents** (`---` / `...`) as an ordered document list;
//! * **scalar styles** (plain, single-quoted, double-quoted, literal `|`, folded
//!   `>`) as distinct styles with their exact token bytes;
//! * **merge keys** (`<<`) as a literal key, never silently merged;
//! * **comments** — their existence and exact spans are retained;
//! * **mapping order** and **duplicate keys** (kept as distinct members).
//!
//! ## The supported subset (and what is DECLINED, typed)
//!
//! This is a **bounded YAML 1.2 core subset**, deliberately conservative. What it
//! *supports*:
//!
//! * a stream of documents separated by a bare `---` line and optionally ended by
//!   a bare `...` line; a stream with no marker is a single document;
//! * block mappings (`key: value`, compact `- key: value`), block sequences
//!   (`- value`), with nested values on more-indented lines;
//! * flow mappings `{ … }` and flow sequences `[ … ]`;
//! * plain, single-quoted, double-quoted, literal (`|`) and folded (`>`) scalars,
//!   a quoted scalar may not span lines (multi-line flow scalars are declined);
//! * anchors (`&a`), tags (`!!x`, `!<…>`, `!x`), and aliases (`*a`) as node
//!   properties; tags are preserved as literal text, never interpreted;
//! * comments (full-line and trailing).
//!
//! What it **DECLINES with a typed error** rather than guessing:
//!
//! * `%YAML`/`%TAG` directives;
//! * explicit keys (`? …`);
//! * flow-collection keys and anchored/tagged/aliased mapping keys;
//! * multi-line plain/flow scalars (a plain scalar is single-line in block context;
//!   a flow plain scalar may not cross a line) and line continuations inside a
//!   quoted scalar;
//! * a document whose content appears on the same line as its `---` marker;
//! * tabs used for indentation, and `\t` inside a scalar's leading whitespace.
//!
//! A declined construct makes the whole source "not YAML" for detection, so the
//! input stays [`crate::field::document_format::DocumentFormat::Opaque`] — never a
//! wrong answer. Detection additionally requires that **every** document's root is
//! a mapping or a sequence, so a bare scalar or a plain-text blob is never
//! admitted.
//!
//! Untrusted input can only ever yield a typed decline, never a panic or an
//! unbounded allocation: nesting depth is capped by [`Limits::max_yaml_depth`]; the
//! node count by [`Limits::max_yaml_nodes`]; the scalar count by
//! [`Limits::max_yaml_scalars`]; the anchor count by [`Limits::max_yaml_anchors`];
//! the document count by [`Limits::max_yaml_documents`]; the raw scalar bytes by
//! [`Limits::max_yaml_string_bytes`]; and the source length by
//! [`Limits::max_yaml_document_bytes`].

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model nodes (defends the decoder against a hostile blob).
pub const MAX_MODEL_NODES: u32 = 1 << 24;

/// YAML node kind: a mapping.
pub const K_MAP: u8 = 0;
/// YAML node kind: a sequence.
pub const K_SEQ: u8 = 1;
/// YAML node kind: a scalar.
pub const K_SCALAR: u8 = 2;
/// YAML node kind: an alias (`*name`).
pub const K_ALIAS: u8 = 3;
/// YAML node kind: an empty node (the YAML null value).
pub const K_EMPTY: u8 = 4;

/// Scalar style: plain.
pub const S_PLAIN: u8 = 0;
/// Scalar style: single-quoted.
pub const S_SINGLE: u8 = 1;
/// Scalar style: double-quoted.
pub const S_DOUBLE: u8 = 2;
/// Scalar style: literal block (`|`).
pub const S_LITERAL: u8 = 3;
/// Scalar style: folded block (`>`).
pub const S_FOLDED: u8 = 4;
/// Style sentinel for aliases and empty nodes.
pub const S_NONE: u8 = 255;

/// Container style: block.
pub const C_BLOCK: u8 = 0;
/// Container style: flow.
pub const C_FLOW: u8 = 1;

/// Stable lower-case kind name for reports and JSON output.
pub const fn kind_name(kind: u8) -> &'static str {
    match kind {
        K_MAP => "mapping",
        K_SEQ => "sequence",
        K_SCALAR => "scalar",
        K_ALIAS => "alias",
        K_EMPTY => "null",
        _ => "unknown",
    }
}

/// Stable lower-case style name for a node, or `"none"` where style is not
/// meaningful (aliases, empty nodes).
pub const fn style_name(kind: u8, style: u8) -> &'static str {
    match kind {
        K_MAP | K_SEQ => match style {
            C_BLOCK => "block",
            C_FLOW => "flow",
            _ => "none",
        },
        K_SCALAR => match style {
            S_PLAIN => "plain",
            S_SINGLE => "single",
            S_DOUBLE => "double",
            S_LITERAL => "literal",
            S_FOLDED => "folded",
            _ => "none",
        },
        _ => "none",
    }
}

/// Whether `kind` is a container (has children).
pub const fn is_container(kind: u8) -> bool {
    matches!(kind, K_MAP | K_SEQ)
}

/// One parsed YAML node: its kind and style, its exact source span, its node
/// properties (anchor/tag, or the anchor targeted by an alias), and its children.
///
/// For a mapping, `children` is the interleaved document-order list
/// `[key0, value0, key1, value1, …]`, so member order and duplicate keys are both
/// preserved verbatim. For a sequence, `children` is the element list in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YNode {
    /// One of the `K_*` tags.
    pub kind: u8,
    /// The scalar style (`S_*`) or container style (`C_*`); `S_NONE` otherwise.
    pub style: u8,
    /// The anchor text after `&`, if this node declares one.
    pub anchor: Option<String>,
    /// The literal tag text (including the leading `!`), if this node carries one.
    pub tag: Option<String>,
    /// For an alias node, the anchor name it targets; otherwise `None`.
    pub alias: Option<String>,
    /// The node's first source byte.
    pub start: u64,
    /// One past the node's last source byte.
    pub end: u64,
    /// Mapping: interleaved key/value indices; sequence: element indices.
    pub children: Vec<u32>,
}

/// One document in the stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YDoc {
    /// Index of the document's root node.
    pub root: u32,
    /// The root's kind tag (retained even when the node arena is not built).
    pub root_kind: u8,
    /// The document's source span start (its `---` marker, if explicit).
    pub start: u64,
    /// The document's source span end (the last root byte).
    pub end: u64,
    /// Whether the document was introduced by an explicit `---` marker.
    pub explicit: bool,
}

/// The canonical derived YAML model (the materialization of a `YamlModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YamlModel {
    /// The ordered document list.
    pub docs: Vec<YDoc>,
    /// The observed maximum container nesting depth (root = 1).
    pub max_depth: u32,
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// Comment spans `[start, end)` in document order (the `#` through the EOL).
    pub comments: Vec<(u64, u64)>,
    /// The node arena.
    pub nodes: Vec<YNode>,
}

impl YamlModel {
    /// The node at `index`, if present.
    pub fn node(&self, index: u32) -> Option<&YNode> {
        self.nodes.get(index as usize)
    }

    /// The kind of the first document's root (`K_EMPTY` for an empty stream).
    pub fn top_type(&self) -> u8 {
        self.docs.first().map_or(K_EMPTY, |d| d.root_kind)
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64 + self.nodes.len() * 32);
        out.extend_from_slice(b"YAMLM");
        out.push(MODEL_VERSION);
        out.extend_from_slice(&self.max_depth.to_le_bytes());
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&(self.docs.len() as u32).to_le_bytes());
        for d in &self.docs {
            out.extend_from_slice(&d.root.to_le_bytes());
            out.push(d.root_kind);
            out.push(u8::from(d.explicit));
            out.extend_from_slice(&d.start.to_le_bytes());
            out.extend_from_slice(&d.end.to_le_bytes());
        }
        out.extend_from_slice(&(self.comments.len() as u32).to_le_bytes());
        for (s, e) in &self.comments {
            out.extend_from_slice(&s.to_le_bytes());
            out.extend_from_slice(&e.to_le_bytes());
        }
        out.extend_from_slice(&(self.nodes.len() as u32).to_le_bytes());
        for n in &self.nodes {
            out.push(n.kind);
            out.push(n.style);
            put_opt(&mut out, n.anchor.as_deref());
            put_opt(&mut out, n.tag.as_deref());
            put_opt(&mut out, n.alias.as_deref());
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
    pub fn decode(bytes: &[u8]) -> Result<YamlModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(5)? != b"YAMLM" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let max_depth = r.u32()?;
        let doc_len = r.u64()?;
        let doc_count = r.u32()?;
        if doc_count > doc_len as u32 + 1 {
            return Err(corrupt("implausible document count"));
        }
        let mut docs = Vec::with_capacity(doc_count as usize);
        for _ in 0..doc_count {
            let root = r.u32()?;
            let root_kind = r.u8()?;
            if root_kind > K_EMPTY {
                return Err(corrupt("unknown document root kind"));
            }
            let explicit = r.u8()?;
            if explicit > 1 {
                return Err(corrupt("invalid document marker flag"));
            }
            let start = r.u64()?;
            let end = r.u64()?;
            if start > end || end > doc_len {
                return Err(corrupt("document span is outside the source"));
            }
            docs.push(YDoc {
                root,
                root_kind,
                start,
                end,
                explicit: explicit == 1,
            });
        }
        let comment_count = r.u32()?;
        if comment_count as u64 > doc_len + 1 {
            return Err(corrupt("implausible comment count"));
        }
        let mut comments = Vec::with_capacity(comment_count as usize);
        for _ in 0..comment_count {
            let s = r.u64()?;
            let e = r.u64()?;
            if s > e || e > doc_len {
                return Err(corrupt("comment span is outside the source"));
            }
            comments.push((s, e));
        }
        let count = r.u32()?;
        if count > MAX_MODEL_NODES {
            return Err(corrupt("model node count is implausible"));
        }
        let mut nodes = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let kind = r.u8()?;
            if kind > K_EMPTY {
                return Err(corrupt("unknown node kind"));
            }
            let style = r.u8()?;
            match kind {
                K_SCALAR => {
                    if style > S_FOLDED {
                        return Err(corrupt("unknown scalar style"));
                    }
                }
                K_MAP | K_SEQ => {
                    if style > C_FLOW {
                        return Err(corrupt("unknown container style"));
                    }
                }
                _ => {
                    if style != S_NONE {
                        return Err(corrupt("unexpected style on a non-scalar node"));
                    }
                }
            }
            let anchor = r.opt_str()?;
            let tag = r.opt_str()?;
            let alias = r.opt_str()?;
            let start = r.u64()?;
            let end = r.u64()?;
            if start > end || end > doc_len {
                return Err(corrupt("node span is outside the source"));
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
            nodes.push(YNode {
                kind,
                style,
                anchor,
                tag,
                alias,
                start,
                end,
                children,
            });
        }
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        for d in &docs {
            if d.root as usize >= nodes.len() {
                return Err(corrupt("document root index is out of range"));
            }
        }
        Ok(YamlModel {
            docs,
            max_depth,
            doc_len,
            comments,
            nodes,
        })
    }
}

/// Fast-fail YAML detector: does the whole `source` parse as a YAML stream within
/// `limits`, with a mapping or sequence at the root of every document? Conservative
/// by construction: a bare scalar, a plain-text blob, or any declined construct is
/// not YAML.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_yaml_document_bytes {
        return false;
    }
    match parse(source, limits, false) {
        Ok(m) => {
            !m.docs.is_empty()
                && m.docs
                    .iter()
                    .all(|d| d.root_kind == K_MAP || d.root_kind == K_SEQ)
        }
        Err(_) => false,
    }
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of a `YamlModel` node).
pub fn build_yaml_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits, true)?.encode())
}

/// Parse `source` into a [`YamlModel`]. `build` selects whether the node arena is
/// populated (detection runs with `build = false`).
pub fn parse(source: &[u8], limits: Limits, build: bool) -> Result<YamlModel> {
    if source.len() as u64 > limits.max_yaml_document_bytes {
        return Err(Error::resource_limit(format!(
            "YAML source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_yaml_document_bytes
        )));
    }
    let mut p = Parser::new(source, limits, build);
    p.parse_stream()?;
    Ok(YamlModel {
        docs: p.docs,
        max_depth: p.max_depth_seen,
        doc_len: source.len() as u64,
        comments: p.comments,
        nodes: p.nodes,
    })
}

/// The exact source bytes of a node's span (`[start, end)`), bounded by the
/// document length.
pub fn token_bytes<'a>(source: &'a [u8], node: &YNode) -> Result<&'a [u8]> {
    let s = usize::try_from(node.start).map_err(|_| corrupt("span overflow"))?;
    let e = usize::try_from(node.end).map_err(|_| corrupt("span overflow"))?;
    source
        .get(s..e)
        .ok_or_else(|| corrupt("node span is outside the source"))
}

/// A resolved path query: the document, the node index, and how many members
/// matched the final key segment (so duplicate keys are reported, never hidden).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolved {
    /// The document index.
    pub doc: usize,
    /// The resolved node index.
    pub index: u32,
    /// The number of members with the final segment's key (`1` for an index/root).
    pub matches: u32,
}

/// Find the first node in `model` that declares `anchor`.
pub fn resolve_anchor(model: &YamlModel, name: &str) -> Option<u32> {
    model
        .nodes
        .iter()
        .position(|n| n.anchor.as_deref() == Some(name))
        .map(|i| i as u32)
}

/// Resolve a dotted path into `model`. The first segment may be `docN`; a numeric
/// segment indexes a sequence; any other segment matches a mapping key by its
/// decoded scalar text (an alias step is followed to its anchor target). A missing
/// key/element or indexing into a scalar is a typed decline (never empty).
pub fn resolve_path(model: &YamlModel, source: &[u8], path: &str) -> Result<Resolved> {
    let mut segs: Vec<&str> = Vec::new();
    let mut doc = 0usize;
    let parts: Vec<&str> = if path.is_empty() {
        Vec::new()
    } else {
        path.split('.').collect()
    };
    let mut parts_iter = parts.iter().peekable();
    if let Some(first) = parts_iter.peek()
        && let Some(rest) = first.strip_prefix("doc")
        && let Ok(n) = rest.parse::<usize>()
    {
        doc = n;
        parts_iter.next();
    }
    for s in parts_iter {
        segs.push(s);
    }
    let doc_info = model
        .docs
        .get(doc)
        .ok_or_else(|| Error::unsupported_feature(format!("YAML stream has no document {doc}")))?;
    let mut index = doc_info.root;
    let mut matches = 1u32;
    for seg in segs {
        // Follow alias nodes to their anchor target before descending.
        let mut hops = 0u32;
        loop {
            let node = model
                .node(index)
                .ok_or_else(|| corrupt("path traversal left the model"))?;
            if node.kind != K_ALIAS {
                break;
            }
            let target = node
                .alias
                .as_deref()
                .and_then(|name| resolve_anchor(model, name))
                .ok_or_else(|| {
                    Error::unsupported_feature(format!(
                        "YAML alias targets unknown anchor {:?}",
                        node.alias
                    ))
                })?;
            index = target;
            hops += 1;
            if hops > MAX_MODEL_NODES {
                return Err(corrupt("alias cycle while resolving a path"));
            }
        }
        let node = model
            .node(index)
            .ok_or_else(|| corrupt("path traversal left the model"))?;
        match node.kind {
            K_MAP => {
                let mut found = 0u32;
                let mut first = None;
                let mut i = 0usize;
                while i + 1 < node.children.len() {
                    let key_idx = node.children[i];
                    let val_idx = node.children[i + 1];
                    i += 2;
                    let key_node = model
                        .node(key_idx)
                        .ok_or_else(|| corrupt("map key index is out of range"))?;
                    let key = decode_scalar_value(source, key_node)?;
                    if key == *seg {
                        found = found.saturating_add(1);
                        if first.is_none() {
                            first = Some(val_idx);
                        }
                    }
                }
                index = first.ok_or_else(|| {
                    Error::unsupported_feature(format!("YAML mapping has no key {seg:?}"))
                })?;
                matches = found;
            }
            K_SEQ => {
                let idx = parse_index(seg)?;
                if idx as usize >= node.children.len() {
                    return Err(Error::unsupported_feature(format!(
                        "YAML sequence index {idx} is out of range (length {})",
                        node.children.len()
                    )));
                }
                index = node.children[idx as usize];
                matches = 1;
            }
            _ => {
                return Err(Error::unsupported_feature(format!(
                    "cannot descend into a YAML {} at {seg:?}",
                    kind_name(node.kind)
                )));
            }
        }
    }
    Ok(Resolved {
        doc,
        index,
        matches,
    })
}

fn parse_index(seg: &str) -> Result<u32> {
    if seg.is_empty() || (seg.len() > 1 && seg.starts_with('0')) {
        return Err(Error::usage(format!(
            "YAML sequence segment {seg:?} is not a canonical index"
        )));
    }
    seg.parse::<u32>()
        .map_err(|_| Error::usage(format!("YAML sequence index {seg:?} is not a u32")))
}

/// Decode a scalar node's token into its Rust text. Never panics; a malformed
/// token is typed.
pub fn decode_scalar_value(source: &[u8], node: &YNode) -> Result<String> {
    let tok = token_bytes(source, node)?;
    match node.style {
        S_PLAIN => Ok(String::from_utf8_lossy(tok).into_owned()),
        S_SINGLE => decode_single(tok),
        S_DOUBLE => decode_double(tok),
        S_LITERAL => decode_block(tok, false),
        S_FOLDED => decode_block(tok, true),
        _ => Ok(String::from_utf8_lossy(tok).into_owned()),
    }
}

/// Render a deterministic canonical text projection of the whole stream: member
/// order and scalar/alias spelling are preserved, containers use canonical
/// separators, documents are joined by a newline.
pub fn canonical_text(model: &YamlModel, source: &[u8]) -> Result<String> {
    let mut out = String::new();
    for (i, doc) in model.docs.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        render(model, source, doc.root, &mut out, 0)?;
    }
    Ok(out)
}

/// Render one node's subtree to canonical text (see [`canonical_text`]).
pub fn subtree_text(model: &YamlModel, source: &[u8], index: u32) -> Result<String> {
    let mut out = String::new();
    render(model, source, index, &mut out, 0)?;
    Ok(out)
}

/// The index of a node's parent, if any (a document root has none).
pub fn find_parent(model: &YamlModel, index: u32) -> Option<u32> {
    model
        .nodes
        .iter()
        .position(|n| n.children.contains(&index))
        .map(|i| i as u32)
}

fn render(
    model: &YamlModel,
    source: &[u8],
    index: u32,
    out: &mut String,
    depth: u32,
) -> Result<()> {
    if depth > MAX_MODEL_NODES {
        return Err(Error::resource_limit(
            "YAML render exceeded its depth bound",
        ));
    }
    let node = model
        .node(index)
        .ok_or_else(|| corrupt("render hit an out-of-range node"))?;
    match node.kind {
        K_MAP => {
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
        K_SEQ => {
            out.push('[');
            for (i, child) in node.children.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                render(model, source, *child, out, depth + 1)?;
            }
            out.push(']');
        }
        K_ALIAS => {
            out.push('*');
            out.push_str(node.alias.as_deref().unwrap_or(""));
        }
        K_EMPTY => out.push_str("null"),
        _ => {
            let tok = token_bytes(source, node)?;
            out.push_str(&String::from_utf8_lossy(tok));
        }
    }
    Ok(())
}

/// Whether a lexical match is a mapping key or a scalar value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchRole {
    /// A mapping key.
    Key,
    /// A scalar value (mapping value or sequence element).
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

/// One lexical-match record from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YamlMatch {
    /// The dotted path (starting `docN`) to the matching node.
    pub path: String,
    /// Whether the match is a mapping key or a scalar value.
    pub role: MatchRole,
    /// The exact source span of the matching scalar token.
    pub start: u64,
    /// One past the matching scalar token.
    pub end: u64,
    /// The decoded text of the matching scalar.
    pub text: String,
}

/// A bounded, case-sensitive lexical search over mapping keys and scalar values.
/// Returns matches in document order.
pub fn find(
    model: &YamlModel,
    source: &[u8],
    pattern: &str,
    limits: Limits,
) -> Result<Vec<YamlMatch>> {
    let mut out = Vec::new();
    for (d, doc) in model.docs.iter().enumerate() {
        let mut path = vec![format!("doc{d}")];
        walk_find(
            model, source, doc.root, true, &mut path, pattern, &mut out, limits, 0,
        )?;
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn walk_find(
    model: &YamlModel,
    source: &[u8],
    index: u32,
    is_value: bool,
    path: &mut Vec<String>,
    pattern: &str,
    out: &mut Vec<YamlMatch>,
    limits: Limits,
    depth: u32,
) -> Result<()> {
    if depth > limits.max_yaml_depth {
        return Err(Error::resource_limit(
            "YAML find exceeded the nesting-depth cap",
        ));
    }
    let node = model
        .node(index)
        .ok_or_else(|| corrupt("find hit an out-of-range node"))?;
    match node.kind {
        K_MAP => {
            let mut i = 0usize;
            while i + 1 < node.children.len() {
                let key_idx = node.children[i];
                let val_idx = node.children[i + 1];
                i += 2;
                let key_node = model
                    .node(key_idx)
                    .ok_or_else(|| corrupt("map key index is out of range"))?;
                let key = decode_scalar_value(source, key_node)?;
                if key.contains(pattern) {
                    out.push(YamlMatch {
                        path: path.join("."),
                        role: MatchRole::Key,
                        start: key_node.start,
                        end: key_node.end,
                        text: key.clone(),
                    });
                }
                path.push(key);
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
        K_SEQ => {
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
        K_SCALAR if is_value => {
            let text = decode_scalar_value(source, node)?;
            if text.contains(pattern) {
                out.push(YamlMatch {
                    path: path.join("."),
                    role: MatchRole::Value,
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

// ---------------------------------------------------------------------------
// Scalar decoding
// ---------------------------------------------------------------------------

fn decode_single(tok: &[u8]) -> Result<String> {
    if tok.len() < 2 || tok[0] != b'\'' || tok[tok.len() - 1] != b'\'' {
        return Err(corrupt("node is not a well-formed single-quoted scalar"));
    }
    let inner = &tok[1..tok.len() - 1];
    let mut out: Vec<u8> = Vec::with_capacity(inner.len());
    let mut i = 0usize;
    while i < inner.len() {
        if inner[i] == b'\'' {
            if inner.get(i + 1) == Some(&b'\'') {
                out.push(b'\'');
                i += 2;
                continue;
            }
            return Err(corrupt("stray quote in single-quoted scalar"));
        }
        out.push(inner[i]);
        i += 1;
    }
    String::from_utf8(out).map_err(|_| corrupt("scalar is not valid UTF-8"))
}

fn decode_double(tok: &[u8]) -> Result<String> {
    if tok.len() < 2 || tok[0] != b'"' || tok[tok.len() - 1] != b'"' {
        return Err(corrupt("node is not a well-formed double-quoted scalar"));
    }
    let inner = &tok[1..tok.len() - 1];
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
            .ok_or_else(|| corrupt("scalar ends inside an escape"))?;
        i += 1;
        match e {
            b'0' => out.push(0),
            b'a' => out.push(0x07),
            b'b' => out.push(0x08),
            b't' => out.push(b'\t'),
            b'n' => out.push(b'\n'),
            b'v' => out.push(0x0B),
            b'f' => out.push(0x0C),
            b'r' => out.push(b'\r'),
            b'e' => out.push(0x1B),
            b' ' => out.push(b' '),
            b'"' => out.push(b'"'),
            b'/' => out.push(b'/'),
            b'\\' => out.push(b'\\'),
            b'N' => push_cp(&mut out, 0x85)?,
            b'_' => push_cp(&mut out, 0xA0)?,
            b'L' => push_cp(&mut out, 0x2028)?,
            b'P' => push_cp(&mut out, 0x2029)?,
            b'x' => {
                let v = read_hex(inner, &mut i, 2)?;
                push_cp(&mut out, v)?;
            }
            b'u' => {
                let v = read_hex(inner, &mut i, 4)?;
                push_cp(&mut out, v)?;
            }
            b'U' => {
                let v = read_hex(inner, &mut i, 8)?;
                push_cp(&mut out, v)?;
            }
            _ => return Err(corrupt("invalid escape in double-quoted scalar")),
        }
    }
    String::from_utf8(out).map_err(|_| corrupt("scalar is not valid UTF-8"))
}

fn push_cp(out: &mut Vec<u8>, cp: u32) -> Result<()> {
    let ch = char::from_u32(cp).ok_or_else(|| corrupt("invalid code point in escape"))?;
    let mut buf = [0u8; 4];
    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
    Ok(())
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

/// Decode a block scalar token (its header line plus content). A deliberately
/// simple, exact decoder: it strips the block indentation and applies literal
/// (keep newlines) or folded (join adjacent non-blank lines with a space) folding.
fn decode_block(tok: &[u8], folded: bool) -> Result<String> {
    // Split off the header line.
    let nl = tok
        .iter()
        .position(|&b| b == b'\n')
        .ok_or_else(|| corrupt("block scalar has no header line"))?;
    let body = &tok[nl + 1..];
    let mut lines: Vec<&[u8]> = Vec::new();
    let mut start = 0usize;
    for (i, &b) in body.iter().enumerate() {
        if b == b'\n' {
            lines.push(&body[start..i]);
            start = i + 1;
        }
    }
    if start < body.len() {
        lines.push(&body[start..]);
    }
    // Strip the common indentation of the non-blank lines.
    let indent = lines
        .iter()
        .filter(|l| !l.iter().all(|&b| b == b' ' || b == b'\t'))
        .map(|l| l.iter().take_while(|&&b| b == b' ').count())
        .min()
        .unwrap_or(0);
    let stripped: Vec<&[u8]> = lines
        .iter()
        .map(|l| {
            let n = l.iter().take_while(|&&b| b == b' ').count().min(indent);
            &l[n..]
        })
        .collect();
    let mut out: Vec<u8> = Vec::new();
    if folded {
        let mut prev_nonblank = false;
        for l in &stripped {
            let blank = l.is_empty();
            if !blank && prev_nonblank {
                out.push(b' ');
            }
            out.extend_from_slice(l);
            out.push(b'\n');
            prev_nonblank = !blank;
        }
    } else {
        for l in &stripped {
            out.extend_from_slice(l);
            out.push(b'\n');
        }
    }
    String::from_utf8(out).map_err(|_| corrupt("block scalar is not valid UTF-8"))
}

// ---------------------------------------------------------------------------
// The bounded parser
// ---------------------------------------------------------------------------

struct Parser<'a> {
    b: &'a [u8],
    at: usize,
    limits: Limits,
    build: bool,
    nodes: Vec<YNode>,
    docs: Vec<YDoc>,
    comments: Vec<(u64, u64)>,
    count: u64,
    scalar_count: u64,
    scalar_bytes: u64,
    anchor_count: u64,
    doc_count: u64,
    max_depth_seen: u32,
    last_end: u64,
}

struct LineInfo {
    indent: usize,
    tab_in_indent: bool,
    blank: bool,
    content: usize,
    eol: usize,
    end: usize,
}

impl<'a> Parser<'a> {
    fn new(b: &'a [u8], limits: Limits, build: bool) -> Self {
        Parser {
            b,
            at: 0,
            limits,
            build,
            nodes: Vec::new(),
            docs: Vec::new(),
            comments: Vec::new(),
            count: 0,
            scalar_count: 0,
            scalar_bytes: 0,
            anchor_count: 0,
            doc_count: 0,
            max_depth_seen: 0,
            last_end: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.at).copied()
    }

    fn skip_spaces(&mut self) {
        while let Some(&c) = self.b.get(self.at) {
            if c == b' ' || c == b'\t' {
                self.at += 1;
            } else {
                break;
            }
        }
    }

    fn line_start(&self) -> usize {
        let mut i = self.at;
        while i > 0 && self.b[i - 1] != b'\n' && self.b[i - 1] != b'\r' {
            i -= 1;
        }
        i
    }

    fn col(&self) -> usize {
        self.at - self.line_start()
    }

    /// Consume the end of the current entry's line, unless the position is already
    /// at the first content character of a following line (the nested-block case,
    /// where the value parser already advanced past the newline).
    fn end_entry(&mut self) -> Result<()> {
        self.skip_spaces();
        match self.peek() {
            None | Some(b'#') | Some(b'\n') | Some(b'\r') => self.consume_line_end(),
            Some(_) => Ok(()),
        }
    }

    fn line_info_at(&self, start: usize) -> LineInfo {
        let b = self.b;
        let n = b.len();
        let mut i = start;
        let mut indent = 0usize;
        let mut tab = false;
        while i < n && (b[i] == b' ' || b[i] == b'\t') {
            if b[i] == b'\t' {
                tab = true;
            }
            indent += 1;
            i += 1;
        }
        let content = i;
        let mut j = i;
        while j < n && b[j] != b'\n' && b[j] != b'\r' {
            j += 1;
        }
        let eol = j;
        let mut end = j;
        if end < n && b[end] == b'\r' {
            end += 1;
        }
        if end < n && b[end] == b'\n' {
            end += 1;
        }
        LineInfo {
            indent,
            tab_in_indent: tab,
            blank: j == content,
            content,
            eol,
            end,
        }
    }

    fn at_line_marker(&self, m: &[u8]) -> bool {
        if self.col() != 0 {
            return false;
        }
        let end = self.at + m.len();
        if end > self.b.len() || &self.b[self.at..end] != m {
            return false;
        }
        matches!(
            self.b.get(end),
            None | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') | Some(b'#')
        )
    }

    fn record_comment(&mut self, start: usize, end: usize) {
        if !self.build {
            return;
        }
        if self.comments.len() as u64 >= self.limits.max_yaml_nodes as u64 {
            return;
        }
        self.comments.push((start as u64, end as u64));
    }

    fn consume_newline(&mut self) -> Result<()> {
        match self.peek() {
            Some(b'\n') => {
                self.at += 1;
                Ok(())
            }
            Some(b'\r') => {
                self.at += 1;
                if self.peek() == Some(b'\n') {
                    self.at += 1;
                }
                Ok(())
            }
            _ => Err(corrupt("expected end of line")),
        }
    }

    fn consume_line_end(&mut self) -> Result<()> {
        self.skip_spaces();
        if self.peek() == Some(b'#') {
            let s = self.at;
            while matches!(self.peek(), Some(c) if c != b'\n' && c != b'\r') {
                self.at += 1;
            }
            self.record_comment(s, self.at);
        }
        match self.peek() {
            None => Ok(()),
            Some(b'\n') | Some(b'\r') => self.consume_newline(),
            Some(_) => Err(corrupt("unexpected content after the node on this line")),
        }
    }

    /// Skip blank lines and full-line comments, recording comment spans. Assumes
    /// `self.at` is at the start of a line.
    fn skip_blank_comment_lines(&mut self) -> Result<()> {
        while self.at < self.b.len() {
            let li = self.line_info_at(self.at);
            if li.blank {
                self.at = li.end;
                continue;
            }
            if self.b[li.content] == b'#' {
                self.record_comment(li.content, li.eol);
                self.at = li.end;
                continue;
            }
            if li.tab_in_indent {
                return Err(corrupt("tab used for indentation"));
            }
            // Leave the cursor at the first content byte so `col()` reports the
            // line's indentation (never the line start).
            self.at = li.content;
            break;
        }
        Ok(())
    }

    fn new_node(&mut self, kind: u8, style: u8, start: u64) -> Result<u32> {
        self.count = self.count.saturating_add(1);
        if self.count > self.limits.max_yaml_nodes as u64 {
            return Err(Error::resource_limit(format!(
                "YAML document exceeds the {}-node cap",
                self.limits.max_yaml_nodes
            )));
        }
        if self.build {
            let idx = self.nodes.len() as u32;
            self.nodes.push(YNode {
                kind,
                style,
                anchor: None,
                tag: None,
                alias: None,
                start,
                end: start,
                children: Vec::new(),
            });
            Ok(idx)
        } else {
            Ok(0)
        }
    }

    fn finish_node(&mut self, idx: u32, end: u64, children: Vec<u32>) {
        self.last_end = end;
        if self.build
            && let Some(n) = self.nodes.get_mut(idx as usize)
        {
            n.end = end;
            n.children = children;
        }
    }

    fn charge_scalar(&mut self, bytes: usize) -> Result<()> {
        self.scalar_count = self.scalar_count.saturating_add(1);
        if self.scalar_count > self.limits.max_yaml_scalars as u64 {
            return Err(Error::resource_limit(format!(
                "YAML document exceeds the {}-scalar cap",
                self.limits.max_yaml_scalars
            )));
        }
        self.scalar_bytes = self.scalar_bytes.saturating_add(bytes as u64);
        if self.scalar_bytes > self.limits.max_yaml_string_bytes {
            return Err(Error::resource_limit(format!(
                "YAML scalar bytes exceed the {}-byte cap",
                self.limits.max_yaml_string_bytes
            )));
        }
        Ok(())
    }

    fn bump_depth(&mut self, depth: u32) -> Result<()> {
        if depth > self.limits.max_yaml_depth {
            return Err(Error::resource_limit(format!(
                "YAML nesting exceeds the {}-level cap",
                self.limits.max_yaml_depth
            )));
        }
        self.max_depth_seen = self.max_depth_seen.max(depth);
        Ok(())
    }

    fn empty_node(&mut self, pos: u64) -> Result<u32> {
        let idx = self.new_node(K_EMPTY, S_NONE, pos)?;
        self.finish_node(idx, pos, Vec::new());
        Ok(idx)
    }

    // -- stream ---------------------------------------------------------------

    fn parse_stream(&mut self) -> Result<()> {
        self.skip_blank_comment_lines()?;
        if self.at >= self.b.len() {
            return Ok(());
        }
        if self.peek() == Some(b'%') {
            return Err(Error::unsupported_feature(
                "YAML directives (`%YAML`/`%TAG`) are not supported",
            ));
        }
        loop {
            let doc_start;
            let explicit;
            if self.at_line_marker(b"---") {
                doc_start = self.at as u64;
                explicit = true;
                self.at += 3;
                self.consume_line_end()?;
                self.skip_blank_comment_lines()?;
            } else if self.at_line_marker(b"...") {
                return Err(corrupt("document end marker without a start"));
            } else {
                doc_start = self.at as u64;
                explicit = false;
            }
            let (root, kind) = if self.at >= self.b.len()
                || self.at_line_marker(b"---")
                || self.at_line_marker(b"...")
            {
                (self.empty_node(self.at as u64)?, K_EMPTY)
            } else {
                self.parse_block_node(1)?
            };
            let doc_end = self.last_end;
            self.doc_count = self.doc_count.saturating_add(1);
            if self.doc_count > self.limits.max_yaml_documents as u64 {
                return Err(Error::resource_limit(format!(
                    "YAML stream exceeds the {}-document cap",
                    self.limits.max_yaml_documents
                )));
            }
            self.docs.push(YDoc {
                root,
                root_kind: kind,
                start: doc_start,
                end: doc_end,
                explicit,
            });
            self.end_entry()?;
            self.skip_blank_comment_lines()?;
            if self.at >= self.b.len() {
                break;
            }
            if self.at_line_marker(b"...") {
                self.at += 3;
                self.consume_line_end()?;
                self.skip_blank_comment_lines()?;
                if self.at >= self.b.len() {
                    break;
                }
                if !self.at_line_marker(b"---") {
                    return Err(corrupt(
                        "content after a document end marker without a start marker",
                    ));
                }
                continue;
            }
            if self.at_line_marker(b"---") {
                continue;
            }
            return Err(corrupt("unexpected trailing content after a YAML document"));
        }
        Ok(())
    }

    // -- block nodes ----------------------------------------------------------

    /// Parse the properties (`&anchor` / `!tag`) preceding a node.
    fn parse_properties(&mut self) -> Result<(Option<String>, Option<String>)> {
        let mut anchor = None;
        let mut tag = None;
        loop {
            match self.peek() {
                Some(b'&') => {
                    if anchor.is_some() {
                        return Err(corrupt("duplicate anchor property"));
                    }
                    self.at += 1;
                    let name = self.scan_name()?;
                    self.anchor_count = self.anchor_count.saturating_add(1);
                    if self.anchor_count > self.limits.max_yaml_anchors as u64 {
                        return Err(Error::resource_limit(format!(
                            "YAML document exceeds the {}-anchor cap",
                            self.limits.max_yaml_anchors
                        )));
                    }
                    anchor = Some(name);
                    self.skip_spaces();
                }
                Some(b'!') => {
                    if tag.is_some() {
                        return Err(corrupt("duplicate tag property"));
                    }
                    tag = Some(self.scan_tag()?);
                    self.skip_spaces();
                }
                _ => break,
            }
        }
        Ok((anchor, tag))
    }

    fn scan_name(&mut self) -> Result<String> {
        let start = self.at;
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() || matches!(c, b',' | b'[' | b']' | b'{' | b'}' | b'#') {
                break;
            }
            self.at += 1;
        }
        if self.at == start {
            return Err(corrupt("empty anchor/alias name"));
        }
        core::str::from_utf8(&self.b[start..self.at])
            .map(str::to_string)
            .map_err(|_| corrupt("anchor/alias name is not valid UTF-8"))
    }

    fn scan_tag(&mut self) -> Result<String> {
        let start = self.at;
        self.at += 1; // leading '!'
        if self.peek() == Some(b'<') {
            self.at += 1;
            loop {
                match self.peek() {
                    Some(b'>') => {
                        self.at += 1;
                        break;
                    }
                    Some(b'\n') | Some(b'\r') | None => {
                        return Err(corrupt("unterminated verbatim tag"));
                    }
                    Some(_) => self.at += 1,
                }
            }
        } else {
            while let Some(c) = self.peek() {
                if c.is_ascii_whitespace() || matches!(c, b',' | b'[' | b']' | b'{' | b'}') {
                    break;
                }
                self.at += 1;
            }
        }
        core::str::from_utf8(&self.b[start..self.at])
            .map(str::to_string)
            .map_err(|_| corrupt("tag is not valid UTF-8"))
    }

    fn at_dash(&self) -> bool {
        self.peek() == Some(b'-')
            && matches!(
                self.b.get(self.at + 1),
                None | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')
            )
    }

    fn followed_by_space_or_eol(&self) -> bool {
        matches!(
            self.b.get(self.at + 1),
            None | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')
        )
    }

    /// Parse a node in block context at the current position. Returns its index
    /// (0 when not building) and its kind.
    fn parse_block_node(&mut self, depth: u32) -> Result<(u32, u8)> {
        if depth > self.limits.max_yaml_depth {
            return Err(Error::resource_limit(format!(
                "YAML nesting exceeds the {}-level cap",
                self.limits.max_yaml_depth
            )));
        }
        self.max_depth_seen = self.max_depth_seen.max(depth);
        self.skip_spaces();
        let (anchor, tag) = self.parse_properties()?;
        self.skip_spaces();
        let c = self
            .peek()
            .ok_or_else(|| corrupt("unexpected end of input"))?;
        let (idx, kind) = if c == b'-' && self.at_dash() {
            self.parse_block_sequence(depth)?
        } else if c == b'?' && self.followed_by_space_or_eol() {
            return Err(Error::unsupported_feature(
                "YAML explicit keys (`?`) are not supported",
            ));
        } else if self.find_key_colon().is_some() {
            self.parse_block_mapping(depth)?
        } else {
            self.inline_value(0, depth)?
        };
        if self.build
            && let Some(n) = self.nodes.get_mut(idx as usize)
        {
            n.anchor = anchor;
            n.tag = tag;
        }
        Ok((idx, kind))
    }

    fn parse_block_sequence(&mut self, depth: u32) -> Result<(u32, u8)> {
        self.bump_depth(depth)?;
        let indent = self.col();
        let start = self.at as u64;
        let idx = self.new_node(K_SEQ, C_BLOCK, start)?;
        let mut children: Vec<u32> = Vec::new();
        let mut last = start;
        loop {
            let col = self.col();
            if col < indent {
                break;
            }
            if col > indent {
                return Err(corrupt("bad indentation in a block sequence"));
            }
            if !self.at_dash() {
                break;
            }
            self.at += 1; // consume '-'
            let elem = self.parse_seq_element(indent, depth + 1)?;
            if self.build {
                children.push(elem);
            }
            last = self.last_end;
            self.end_entry()?;
            self.skip_blank_comment_lines()?;
            if self.at >= self.b.len() || self.at_line_marker(b"---") || self.at_line_marker(b"...")
            {
                break;
            }
        }
        self.finish_node(idx, last, children);
        Ok((idx, K_SEQ))
    }

    fn parse_seq_element(&mut self, seq_indent: usize, depth: u32) -> Result<u32> {
        self.bump_depth(depth)?;
        self.skip_spaces();
        match self.peek() {
            None | Some(b'#') => return self.empty_node(self.at as u64),
            Some(b'\n') | Some(b'\r') => {
                self.consume_newline()?;
                self.skip_blank_comment_lines()?;
                if self.at >= self.b.len()
                    || self.at_line_marker(b"---")
                    || self.at_line_marker(b"...")
                {
                    return self.empty_node(self.at as u64);
                }
                let col = self.col();
                if col > seq_indent {
                    return Ok(self.parse_block_node(depth)?.0);
                }
                return self.empty_node(self.at as u64);
            }
            _ => {}
        }
        let (anchor, tag) = self.parse_properties()?;
        self.skip_spaces();
        let c = self
            .peek()
            .ok_or_else(|| corrupt("unexpected end of input"))?;
        let idx = if c == b'-' && self.at_dash() {
            self.parse_block_sequence(depth)?.0
        } else if self.find_key_colon().is_some() {
            self.parse_block_mapping(depth)?.0
        } else {
            self.inline_value(seq_indent, depth)?.0
        };
        if self.build
            && let Some(n) = self.nodes.get_mut(idx as usize)
        {
            n.anchor = anchor;
            n.tag = tag;
        }
        Ok(idx)
    }

    fn parse_block_mapping(&mut self, depth: u32) -> Result<(u32, u8)> {
        self.bump_depth(depth)?;
        let indent = self.col();
        let start = self.at as u64;
        let idx = self.new_node(K_MAP, C_BLOCK, start)?;
        let mut children: Vec<u32> = Vec::new();
        let mut last = start;
        loop {
            let col = self.col();
            if col < indent {
                break;
            }
            if col > indent {
                return Err(corrupt("bad indentation in a block mapping"));
            }
            let key = self.parse_key_node(depth)?;
            self.skip_spaces();
            if self.peek() != Some(b':') {
                return Err(corrupt("expected ':' after a mapping key"));
            }
            self.at += 1; // consume ':'
            let val = self.parse_block_value(indent, depth + 1)?;
            if self.build {
                children.push(key);
                children.push(val);
            }
            last = self.last_end;
            self.end_entry()?;
            self.skip_blank_comment_lines()?;
            if self.at >= self.b.len() || self.at_line_marker(b"---") || self.at_line_marker(b"...")
            {
                break;
            }
        }
        self.finish_node(idx, last, children);
        Ok((idx, K_MAP))
    }

    /// Parse a mapping value (the part after `key:`). `key_indent` is the column of
    /// the key that owns this value, used to decide whether a following line is a
    /// nested node.
    fn parse_block_value(&mut self, key_indent: usize, depth: u32) -> Result<u32> {
        self.bump_depth(depth)?;
        self.skip_spaces();
        let props_start = self.at;
        let (anchor, tag) = self.parse_properties()?;
        self.skip_spaces();
        // A trailing comment on the key's line (`key:  # note`) does not end the
        // value: consume the comment up to its line ending so the following line
        // is still examined below. Without this, `on:  # yamllint ...` followed by
        // an indented block was read as an empty value and the next line then
        // tripped the "bad indentation" guard.
        if self.peek() == Some(b'#') {
            let s = self.at;
            while matches!(self.peek(), Some(c) if c != b'\n' && c != b'\r') {
                self.at += 1;
            }
            self.record_comment(s, self.at);
        }
        match self.peek() {
            None => {
                let idx = self.new_node(K_EMPTY, S_NONE, props_start as u64)?;
                self.finish_node(idx, props_start as u64, Vec::new());
                if self.build
                    && let Some(n) = self.nodes.get_mut(idx as usize)
                {
                    n.anchor = anchor;
                    n.tag = tag;
                }
                Ok(idx)
            }
            Some(b'\n') | Some(b'\r') => {
                self.consume_newline()?;
                self.skip_blank_comment_lines()?;
                let empty = self.at >= self.b.len()
                    || self.at_line_marker(b"---")
                    || self.at_line_marker(b"...");
                let idx = if empty {
                    let i = self.new_node(K_EMPTY, S_NONE, props_start as u64)?;
                    self.finish_node(i, props_start as u64, Vec::new());
                    i
                } else {
                    let col = self.col();
                    if col > key_indent {
                        self.parse_block_node(depth)?.0
                    } else if col == key_indent && self.at_dash() {
                        self.parse_block_sequence(depth)?.0
                    } else {
                        let i = self.new_node(K_EMPTY, S_NONE, props_start as u64)?;
                        self.finish_node(i, props_start as u64, Vec::new());
                        i
                    }
                };
                if self.build
                    && let Some(n) = self.nodes.get_mut(idx as usize)
                {
                    n.anchor = anchor;
                    n.tag = tag;
                }
                Ok(idx)
            }
            _ => {
                let (idx, _) = self.inline_value(key_indent, depth)?;
                if self.build
                    && let Some(n) = self.nodes.get_mut(idx as usize)
                {
                    n.anchor = anchor;
                    n.tag = tag;
                }
                Ok(idx)
            }
        }
    }

    /// An inline (non-block-collection) node: an alias, a flow collection, a
    /// quoted/plain scalar, or a block scalar.
    fn inline_value(&mut self, parent_indent: usize, depth: u32) -> Result<(u32, u8)> {
        let c = self
            .peek()
            .ok_or_else(|| corrupt("unexpected end of input"))?;
        match c {
            b'*' => self.parse_alias(),
            b'[' => self.parse_flow_seq(depth),
            b'{' => self.parse_flow_map(depth),
            b'\'' => self.parse_single_quoted(),
            b'"' => self.parse_double_quoted(),
            b'|' | b'>' => self.parse_block_scalar(parent_indent, depth),
            _ => self.parse_plain_scalar_block(),
        }
    }

    fn parse_key_node(&mut self, depth: u32) -> Result<u32> {
        if depth > self.limits.max_yaml_depth {
            return Err(Error::resource_limit(
                "YAML nesting exceeds the configured depth cap",
            ));
        }
        let c = self
            .peek()
            .ok_or_else(|| corrupt("expected a mapping key"))?;
        match c {
            b'\'' => Ok(self.parse_single_quoted()?.0),
            b'"' => Ok(self.parse_double_quoted()?.0),
            b'[' | b'{' => Err(Error::unsupported_feature(
                "YAML flow-collection keys are not supported",
            )),
            b'&' | b'!' | b'*' => Err(Error::unsupported_feature(
                "YAML anchored/tagged/aliased mapping keys are not supported",
            )),
            b'?' if self.followed_by_space_or_eol() => Err(Error::unsupported_feature(
                "YAML explicit keys (`?`) are not supported",
            )),
            _ => self.parse_plain_key(),
        }
    }

    fn parse_plain_key(&mut self) -> Result<u32> {
        let start = self.at;
        let colon = self
            .find_key_colon()
            .ok_or_else(|| corrupt("expected ':' after a mapping key"))?;
        let mut e = colon;
        while e > start && matches!(self.b[e - 1], b' ' | b'\t') {
            e -= 1;
        }
        if e == start {
            return Err(corrupt("empty mapping key"));
        }
        let idx = self.new_node(K_SCALAR, S_PLAIN, start as u64)?;
        self.charge_scalar(e - start)?;
        self.at = e;
        self.finish_node(idx, e as u64, Vec::new());
        Ok(idx)
    }

    /// The offset of the `:` that terminates a block mapping key at the current
    /// position, if the current line begins a mapping entry. A colon only counts
    /// when followed by a space, a tab, or the end of the line.
    fn find_key_colon(&self) -> Option<usize> {
        let b = self.b;
        let n = b.len();
        let mut i = self.at;
        if i >= n {
            return None;
        }
        if b[i] == b'\'' || b[i] == b'"' {
            i = self.skip_quoted_at(i)?;
            while i < n && (b[i] == b' ' || b[i] == b'\t') {
                i += 1;
            }
            if b.get(i) == Some(&b':') && self.colon_ok(i) {
                return Some(i);
            }
            return None;
        }
        while i < n {
            match b[i] {
                b'\n' | b'\r' => return None,
                b':' if self.colon_ok(i) => return Some(i),
                b'#' if i > self.at && matches!(b[i - 1], b' ' | b'\t') => return None,
                _ => i += 1,
            }
        }
        None
    }

    fn colon_ok(&self, i: usize) -> bool {
        matches!(
            self.b.get(i + 1),
            None | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')
        )
    }

    fn skip_quoted_at(&self, start: usize) -> Option<usize> {
        let b = self.b;
        let quote = *b.get(start)?;
        let mut i = start + 1;
        while i < b.len() {
            let c = b[i];
            if c == quote {
                if quote == b'\'' && b.get(i + 1) == Some(&b'\'') {
                    i += 2;
                    continue;
                }
                return Some(i + 1);
            }
            if c == b'\\' && quote == b'"' {
                i += 2;
                continue;
            }
            if c == b'\n' || c == b'\r' {
                return None;
            }
            i += 1;
        }
        None
    }

    fn parse_single_quoted(&mut self) -> Result<(u32, u8)> {
        let start = self.at;
        self.at += 1;
        loop {
            let c = self
                .peek()
                .ok_or_else(|| corrupt("unterminated single-quoted scalar"))?;
            if c == b'\'' {
                if self.b.get(self.at + 1) == Some(&b'\'') {
                    self.at += 2;
                    continue;
                }
                self.at += 1;
                break;
            }
            if c == b'\n' || c == b'\r' {
                return Err(corrupt("a single-quoted scalar may not span lines"));
            }
            self.at += 1;
        }
        let idx = self.new_node(K_SCALAR, S_SINGLE, start as u64)?;
        self.charge_scalar(self.at - start)?;
        self.finish_node(idx, self.at as u64, Vec::new());
        Ok((idx, K_SCALAR))
    }

    fn parse_double_quoted(&mut self) -> Result<(u32, u8)> {
        let start = self.at;
        self.at += 1;
        loop {
            let c = self
                .peek()
                .ok_or_else(|| corrupt("unterminated double-quoted scalar"))?;
            match c {
                b'"' => {
                    self.at += 1;
                    break;
                }
                b'\\' => {
                    self.at += 1;
                    match self.peek() {
                        None | Some(b'\n') | Some(b'\r') => {
                            return Err(corrupt("a double-quoted scalar may not span lines"));
                        }
                        Some(_) => self.at += 1,
                    }
                }
                b'\n' | b'\r' => {
                    return Err(corrupt("a double-quoted scalar may not span lines"));
                }
                _ => self.at += 1,
            }
        }
        let idx = self.new_node(K_SCALAR, S_DOUBLE, start as u64)?;
        self.charge_scalar(self.at - start)?;
        self.finish_node(idx, self.at as u64, Vec::new());
        Ok((idx, K_SCALAR))
    }

    fn parse_plain_scalar_block(&mut self) -> Result<(u32, u8)> {
        let start = self.at;
        let mut end = start;
        while let Some(c) = self.peek() {
            match c {
                b'\n' | b'\r' => break,
                b'#' if end > start && matches!(self.b[end - 1], b' ' | b'\t') => break,
                b':' if self.colon_ok(self.at) => {
                    return Err(corrupt("unexpected ':' in a plain scalar value"));
                }
                _ => {
                    self.at += 1;
                    end = self.at;
                }
            }
        }
        let mut e = end;
        while e > start && matches!(self.b[e - 1], b' ' | b'\t') {
            e -= 1;
        }
        if e == start {
            return Err(corrupt("empty plain scalar"));
        }
        let idx = self.new_node(K_SCALAR, S_PLAIN, start as u64)?;
        self.charge_scalar(e - start)?;
        self.finish_node(idx, e as u64, Vec::new());
        Ok((idx, K_SCALAR))
    }

    fn parse_alias(&mut self) -> Result<(u32, u8)> {
        let start = self.at;
        self.at += 1; // consume '*'
        let name = self.scan_name()?;
        let idx = self.new_node(K_ALIAS, S_NONE, start as u64)?;
        if self.build
            && let Some(n) = self.nodes.get_mut(idx as usize)
        {
            n.alias = Some(name);
        }
        self.finish_node(idx, self.at as u64, Vec::new());
        Ok((idx, K_ALIAS))
    }

    /// Parse a block scalar (`|` / `>`), with optional chomping/indentation
    /// indicators. `parent_indent` is the indentation of the owning key/element.
    fn parse_block_scalar(&mut self, parent_indent: usize, depth: u32) -> Result<(u32, u8)> {
        if depth > self.limits.max_yaml_depth {
            return Err(Error::resource_limit(
                "YAML nesting exceeds the configured depth cap",
            ));
        }
        self.max_depth_seen = self.max_depth_seen.max(depth);
        let start = self.at;
        let style = if self.peek() == Some(b'|') {
            S_LITERAL
        } else {
            S_FOLDED
        };
        self.at += 1;
        let mut explicit: Option<usize> = None;
        loop {
            match self.peek() {
                Some(b'+') | Some(b'-') => self.at += 1,
                Some(d @ b'1'..=b'9') => {
                    explicit = Some((d - b'0') as usize);
                    self.at += 1;
                }
                _ => break,
            }
        }
        self.skip_spaces();
        if self.peek() == Some(b'#') {
            let s = self.at;
            while matches!(self.peek(), Some(c) if c != b'\n' && c != b'\r') {
                self.at += 1;
            }
            self.record_comment(s, self.at);
        }
        match self.peek() {
            Some(b'\n') | Some(b'\r') => self.consume_newline()?,
            None => {}
            Some(_) => return Err(corrupt("unexpected content after a block scalar header")),
        }
        let content_indent = match explicit {
            Some(d) => parent_indent.saturating_add(d),
            None => {
                let mut p = self.at;
                let mut ci = parent_indent + 1;
                loop {
                    if p >= self.b.len() {
                        break;
                    }
                    let li = self.line_info_at(p);
                    if li.blank {
                        p = li.end;
                        continue;
                    }
                    if li.tab_in_indent {
                        return Err(corrupt("tab in block scalar indentation"));
                    }
                    if li.indent <= parent_indent {
                        break;
                    }
                    ci = li.indent;
                    break;
                }
                ci
            }
        };
        let mut end = self.at;
        while end < self.b.len() {
            let li = self.line_info_at(end);
            if li.blank {
                end = li.end;
                continue;
            }
            if li.tab_in_indent {
                return Err(corrupt("tab in block scalar indentation"));
            }
            if li.indent >= content_indent {
                end = li.end;
                continue;
            }
            break;
        }
        self.at = end;
        let idx = self.new_node(K_SCALAR, style, start as u64)?;
        self.charge_scalar(end - start)?;
        self.finish_node(idx, end as u64, Vec::new());
        Ok((idx, K_SCALAR))
    }

    // -- flow -----------------------------------------------------------------

    fn skip_flow_ws(&mut self) -> Result<()> {
        loop {
            match self.peek() {
                Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') => self.at += 1,
                Some(b'#') => {
                    let s = self.at;
                    while matches!(self.peek(), Some(c) if c != b'\n' && c != b'\r') {
                        self.at += 1;
                    }
                    self.record_comment(s, self.at);
                }
                _ => break,
            }
        }
        Ok(())
    }

    fn parse_flow_node(&mut self, depth: u32) -> Result<u32> {
        if depth > self.limits.max_yaml_depth {
            return Err(Error::resource_limit(
                "YAML nesting exceeds the configured depth cap",
            ));
        }
        self.max_depth_seen = self.max_depth_seen.max(depth);
        self.skip_flow_ws()?;
        let (anchor, tag) = self.parse_properties()?;
        self.skip_flow_ws()?;
        let c = self
            .peek()
            .ok_or_else(|| corrupt("unexpected end of a flow collection"))?;
        let idx = match c {
            b'*' => self.parse_alias()?.0,
            b'[' => self.parse_flow_seq(depth)?.0,
            b'{' => self.parse_flow_map(depth)?.0,
            b'\'' => self.parse_single_quoted()?.0,
            b'"' => self.parse_double_quoted()?.0,
            b'|' | b'>' => {
                return Err(Error::unsupported_feature(
                    "a block scalar is not allowed in flow context",
                ));
            }
            b'?' if self.followed_by_space_or_eol() => {
                return Err(Error::unsupported_feature(
                    "YAML explicit keys (`?`) are not supported",
                ));
            }
            b':' => return Err(corrupt("unexpected ':' in a flow node")),
            _ => self.parse_plain_scalar_flow()?.0,
        };
        if self.build
            && let Some(n) = self.nodes.get_mut(idx as usize)
        {
            n.anchor = anchor;
            n.tag = tag;
        }
        Ok(idx)
    }

    fn parse_flow_seq(&mut self, depth: u32) -> Result<(u32, u8)> {
        self.bump_depth(depth)?;
        let start = self.at;
        self.at += 1; // '['
        let idx = self.new_node(K_SEQ, C_FLOW, start as u64)?;
        let mut children: Vec<u32> = Vec::new();
        self.skip_flow_ws()?;
        if self.peek() == Some(b']') {
            self.at += 1;
            self.finish_node(idx, self.at as u64, children);
            return Ok((idx, K_SEQ));
        }
        loop {
            let elem = self.parse_flow_node(depth + 1)?;
            if self.build {
                children.push(elem);
            }
            self.skip_flow_ws()?;
            match self.peek() {
                Some(b',') => {
                    self.at += 1;
                    self.skip_flow_ws()?;
                    if self.peek() == Some(b']') {
                        self.at += 1;
                        break;
                    }
                }
                Some(b']') => {
                    self.at += 1;
                    break;
                }
                _ => return Err(corrupt("expected ',' or ']' in a flow sequence")),
            }
        }
        self.finish_node(idx, self.at as u64, children);
        Ok((idx, K_SEQ))
    }

    fn parse_flow_map(&mut self, depth: u32) -> Result<(u32, u8)> {
        self.bump_depth(depth)?;
        let start = self.at;
        self.at += 1; // '{'
        let idx = self.new_node(K_MAP, C_FLOW, start as u64)?;
        let mut children: Vec<u32> = Vec::new();
        self.skip_flow_ws()?;
        if self.peek() == Some(b'}') {
            self.at += 1;
            self.finish_node(idx, self.at as u64, children);
            return Ok((idx, K_MAP));
        }
        loop {
            let key = self.parse_flow_node(depth + 1)?;
            self.skip_flow_ws()?;
            if self.peek() != Some(b':') {
                return Err(corrupt("expected ':' in a flow mapping"));
            }
            self.at += 1;
            self.skip_flow_ws()?;
            let val = if matches!(self.peek(), Some(b',') | Some(b'}')) {
                self.empty_node(self.at as u64)?
            } else {
                self.parse_flow_node(depth + 1)?
            };
            if self.build {
                children.push(key);
                children.push(val);
            }
            self.skip_flow_ws()?;
            match self.peek() {
                Some(b',') => {
                    self.at += 1;
                    self.skip_flow_ws()?;
                    if self.peek() == Some(b'}') {
                        self.at += 1;
                        break;
                    }
                }
                Some(b'}') => {
                    self.at += 1;
                    break;
                }
                _ => return Err(corrupt("expected ',' or '}' in a flow mapping")),
            }
        }
        self.finish_node(idx, self.at as u64, children);
        Ok((idx, K_MAP))
    }

    fn parse_plain_scalar_flow(&mut self) -> Result<(u32, u8)> {
        if self.followed_by_colon_or_flow() {
            return Err(corrupt("empty plain scalar in flow"));
        }
        let start = self.at;
        let mut end = start;
        while let Some(c) = self.peek() {
            match c {
                b',' | b'[' | b']' | b'{' | b'}' => break,
                b'\n' | b'\r' => break,
                b'#' if end > start && matches!(self.b[end - 1], b' ' | b'\t') => break,
                b':' if matches!(
                    self.b.get(self.at + 1),
                    None | Some(b' ') | Some(b'\t') | Some(b',') | Some(b']') | Some(b'}')
                ) =>
                {
                    break;
                }
                _ => {
                    self.at += 1;
                    end = self.at;
                }
            }
        }
        let mut e = end;
        while e > start && matches!(self.b[e - 1], b' ' | b'\t') {
            e -= 1;
        }
        if e == start {
            return Err(corrupt("empty plain scalar in flow"));
        }
        let idx = self.new_node(K_SCALAR, S_PLAIN, start as u64)?;
        self.charge_scalar(e - start)?;
        self.finish_node(idx, e as u64, Vec::new());
        Ok((idx, K_SCALAR))
    }

    fn followed_by_colon_or_flow(&self) -> bool {
        matches!(
            self.peek(),
            Some(b':') | Some(b',') | Some(b'[') | Some(b']') | Some(b'{') | Some(b'}')
        )
    }
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn corrupt(msg: &str) -> Error {
    Error::invalid_yaml_structure(format!("malformed YAML: {msg}"))
}

fn put_opt(out: &mut Vec<u8>, s: Option<&str>) {
    match s {
        Some(x) => {
            out.push(1);
            out.extend_from_slice(&(x.len() as u32).to_le_bytes());
            out.extend_from_slice(x.as_bytes());
        }
        None => out.push(0),
    }
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

    fn opt_str(&mut self) -> Result<Option<String>> {
        match self.u8()? {
            0 => Ok(None),
            1 => {
                let len = self.u32()? as usize;
                let s = self.bytes(len)?;
                core::str::from_utf8(s)
                    .map(|x| Some(x.to_string()))
                    .map_err(|_| corrupt("model string is not valid UTF-8"))
            }
            _ => Err(corrupt("invalid optional-string tag")),
        }
    }

    fn at_end(&self) -> bool {
        self.at == self.b.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(src: &[u8]) -> YamlModel {
        parse(src, Limits::DEFAULT, true).unwrap()
    }

    #[test]
    fn detects_mappings_and_sequences_but_not_scalars() {
        assert!(detect(b"a: 1\nb: 2\n", Limits::DEFAULT));
        assert!(detect(b"- 1\n- 2\n", Limits::DEFAULT));
        // A bare scalar or plain-text blob is not admitted.
        assert!(!detect(b"42\n", Limits::DEFAULT));
        assert!(!detect(b"just some text\n", Limits::DEFAULT));
        // Malformed YAML is not admitted (and never panics).
        assert!(!detect(b"", Limits::DEFAULT));
        assert!(!detect(b"a: [1, 2\n", Limits::DEFAULT));
        assert!(!detect(b"\t- x\n", Limits::DEFAULT));
    }

    #[test]
    fn preserves_anchors_aliases_tags_and_order() {
        let src = b"a: &x 1\nb: *x\nc: !!str 2\n";
        let m = model(src);
        assert_eq!(m.top_type(), K_MAP);
        // The anchor is preserved on `a`; the alias on `b` targets `x`.
        let anchors: Vec<&str> = m.nodes.iter().filter_map(|n| n.anchor.as_deref()).collect();
        assert_eq!(anchors, vec!["x"]);
        let aliases: Vec<&str> = m.nodes.iter().filter_map(|n| n.alias.as_deref()).collect();
        assert_eq!(aliases, vec!["x"]);
        let tags: Vec<&str> = m.nodes.iter().filter_map(|n| n.tag.as_deref()).collect();
        assert_eq!(tags, vec!["!!str"]);
    }

    #[test]
    fn preserves_scalar_styles() {
        let src = b"a: plain\nb: 'single'\nc: \"double\"\nd: |\n  lit\ne: >\n  fold\n";
        let m = model(src);
        let mut styles: Vec<&str> = Vec::new();
        for n in &m.nodes {
            if n.kind == K_SCALAR {
                styles.push(style_name(n.kind, n.style));
            }
        }
        for want in ["plain", "single", "double", "literal", "folded"] {
            assert!(styles.contains(&want), "missing {want}: {styles:?}");
        }
    }

    #[test]
    fn multiple_documents_are_ordered() {
        let src = b"---\na: 1\n---\nb: 2\n...\n";
        let m = model(src);
        assert_eq!(m.docs.len(), 2);
        assert!(m.docs[0].explicit && m.docs[1].explicit);
    }

    #[test]
    fn merge_keys_and_comments_are_surfaced() {
        let src = b"base: &b {x: 1}\nderived:\n  <<: *b  # merge\n  y: 2\n";
        let m = model(src);
        assert!(!m.comments.is_empty());
        let keys: Vec<String> = m
            .nodes
            .iter()
            .filter(|n| n.kind == K_SCALAR && n.style == S_PLAIN)
            .map(|n| String::from_utf8_lossy(token_bytes(src, n).unwrap()).into_owned())
            .collect();
        assert!(keys.iter().any(|k| k == "<<"), "{keys:?}");
    }

    #[test]
    fn path_resolution_reports_duplicates() {
        let src = b"a: 1\na: 2\nb: 3\n";
        let m = model(src);
        let r = resolve_path(&m, src, "a").unwrap();
        assert_eq!(r.matches, 2);
        assert_eq!(token_bytes(src, m.node(r.index).unwrap()).unwrap(), b"1");
    }

    #[test]
    fn model_roundtrips() {
        let src = b"a: &x [1, 2]\nb: *x\nc: {d: 'e'}\n";
        let m = model(src);
        let enc = m.encode();
        let dec = YamlModel::decode(&enc).unwrap();
        assert_eq!(dec, m);
    }

    #[test]
    fn deep_nesting_declines_typed() {
        let mut s = String::new();
        for _ in 0..200 {
            s.push_str("- ");
        }
        s.push('0');
        s.push('\n');
        let e = parse(s.as_bytes(), Limits::STRICT, true).unwrap_err();
        assert_eq!(e.class(), crate::error::ErrorClass::ResourceLimit);
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
            if let Ok(m) = parse(&buf, Limits::STRICT, true) {
                let _ = canonical_text(&m, &buf);
                let _ = build_yaml_model(&buf, Limits::STRICT);
            }
        }
    }
}
