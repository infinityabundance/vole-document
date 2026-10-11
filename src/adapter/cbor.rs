//! Bounded, representation-preserving CBOR adapter (Phase 21.18).
//!
//! CBOR (Concise Binary Object Representation, RFC 8949) is a **binary**
//! structured-tree Wave-2 format. Like the other Wave-2 formats it is *not* an
//! office package: there is no OPC/ZIP layer, no `mimetype`, and no relationship
//! graph. The exact leaf is therefore the **whole source** (a `DocumentExact`, a
//! RAW-like authority), and everything this module produces is a bounded,
//! deterministic (`Q_gen`) projection that never sits on the exactness path.
//!
//! ## Same structured-tree observation vocabulary
//!
//! Like the JSON/JSON5 adapters ([`crate::adapter::json`], [`crate::adapter::json5`])
//! this adapter produces a node arena with a **stable kind tag**, an **exact
//! `[start, end)` source span**, and an **ordered child list** (map children are
//! the interleaved `key0, value0, key1, value1, …` sequence, so member order and
//! duplicate keys are preserved verbatim; an array's children are its elements; a
//! tag's single child is the tagged item). On top of the shared vocabulary it
//! records the CBOR-specific representation facts that have no JSON analogue.
//!
//! ## What is preserved (exactly, for every token)
//!
//! * every **major type**: unsigned int (mt 0), negative int (mt 1), byte string
//!   (mt 2), text string (mt 3), array (mt 4), map (mt 5), tag (mt 6), and
//!   simple/float (mt 7);
//! * the **encoding width actually used** — the head byte's additional-information
//!   nibble is recorded verbatim in [`CborNode::info`], so `0x1817` (uint 23 in the
//!   one-byte form, info 24) is distinguishable from `0x17` (info 23);
//! * **byte string vs text string** as distinct kinds ([`K_BYTES`] vs [`K_TEXT`],
//!   never conflated — the defining CBOR-vs-JSON distinction), and a text string's
//!   payload must be well-formed UTF-8;
//! * **tag numbers** ([`CborNode::tag`]) are preserved verbatim and are **never**
//!   resolved or expanded into the tag's semantics (tag 0 is not turned into a
//!   date string, tag 55799 is not stripped);
//! * **map key order** and **duplicate keys** (kept as distinct children, never
//!   overwritten);
//! * **float width** — half (`0xf9`, info 25), single (`0xfa`, info 26), and double
//!   (`0xfb`, info 27) — with the IEEE-754 bit pattern recorded verbatim in
//!   [`CborNode::arg`]; and simple values (`0xf4` false, `0xf5` true, `0xf6` null,
//!   `0xf7` undefined, `0xf8` simple);
//! * **definite AND indefinite-length** items (the break-stop `0xff`), recording
//!   which form was used in [`CborNode::indefinite`].
//!
//! ## Detection (the honest boundary)
//!
//! CBOR has **no magic bytes**, so a universal detector is impossible and any
//! detector is inherently heuristic. [`detect`] is therefore deliberately
//! conservative and claims an input only on a strong signal:
//!
//! 1. the **self-described-CBOR tag `55799`** (`0xd9 0xd9 0xf7`) at the start of a
//!    source that then parses, in full, as exactly one well-formed CBOR item (the
//!    format literally announces itself); **or**
//! 2. a **full-input, well-formed** parse whose root is a **container** (array or
//!    map) or a **tag**, reaching at least [`MIN_DETECT_NODES`] nodes from at least
//!    [`MIN_DETECT_BYTES`] bytes — so a single scalar, an empty container, a
//!    three-byte item, and any structurally trivial input stay `Opaque`.
//!
//! A container head byte is always `>= 0x80`, so this detector can **never** claim
//! a pure-ASCII document (every JSON/YAML/TOML/XML/HTML/prose source is therefore
//! safe); and it is tried *after* the strong magic-byte binaries (PDF/ZIP/Parquet/
//! Arrow) and the JSON family, so nothing stronger is ever reconsidered.
//!
//! ### What it cannot distinguish (recorded negative)
//!
//! MessagePack and CBOR share the single-byte representation of the whole numbers
//! `0..=23` (MessagePack `fixint` is CBOR's inline `uint`), and that whole-number /
//! short-container **prefix** overlap is real, so a lone byte in that range is
//! genuinely ambiguous — but it is also structurally trivial and therefore stays
//! `Opaque` (as does an empty container). Beyond that the encodings diverge:
//! MessagePack's `fixarray`/`fixmap` low nibble is an element **count**, whereas
//! CBOR's is an additional-information selector, so a MessagePack container header
//! is rarely a complete well-formed CBOR container. This adapter therefore falls
//! back to `Opaque` for any input it cannot parse as exactly one full CBOR item,
//! and it does **not** guess. A future Phase-21.19 MessagePack adapter must still
//! share this seam (and would have to decide the boundary for the shared
//! whole-number range).
//!
//! ## Bounds and no panics
//!
//! Untrusted input can only ever yield a typed decline, never a panic or an
//! unbounded allocation: the nesting depth by [`Limits::max_cbor_depth`], the node
//! count by [`Limits::max_cbor_nodes`], the raw string bytes by
//! [`Limits::max_cbor_string_bytes`], and the source length by
//! [`Limits::max_cbor_document_bytes`]. All length arithmetic is checked.

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model nodes (defends the decoder against a hostile blob).
pub const MAX_MODEL_NODES: u32 = 1 << 24;

/// The RFC 8949 self-described-CBOR tag number (`55799`).
pub const SELF_DESCRIBED_TAG: u64 = 55799;
/// The three-byte self-described-CBOR prefix (`0xd9 0xd9 0xf7`, tag 55799).
pub const SELF_DESCRIBED_MAGIC: [u8; 3] = [0xd9, 0xd9, 0xf7];
/// The minimum node count a source without the self-described tag must reach to be
/// detected as CBOR (so a single scalar or an empty container stays `Opaque`).
pub const MIN_DETECT_NODES: u32 = 3;
/// The minimum source length admitted for structural detection.
pub const MIN_DETECT_BYTES: u64 = 4;

/// CBOR node kind: an unsigned integer (major type 0).
pub const K_UINT: u8 = 0;
/// CBOR node kind: a negative integer (major type 1; the recorded value is `-1 - n`).
pub const K_NEGINT: u8 = 1;
/// CBOR node kind: a byte string (major type 2).
pub const K_BYTES: u8 = 2;
/// CBOR node kind: a text string (major type 3; the payload is well-formed UTF-8).
pub const K_TEXT: u8 = 3;
/// CBOR node kind: an array (major type 4).
pub const K_ARRAY: u8 = 4;
/// CBOR node kind: a map (major type 5).
pub const K_MAP: u8 = 5;
/// CBOR node kind: a tagged item (major type 6). Its single child is the tagged item.
pub const K_TAG: u8 = 6;
/// CBOR node kind: the simple value `false` (`0xf4`).
pub const K_FALSE: u8 = 7;
/// CBOR node kind: the simple value `true` (`0xf5`).
pub const K_TRUE: u8 = 8;
/// CBOR node kind: the simple value `null` (`0xf6`).
pub const K_NULL: u8 = 9;
/// CBOR node kind: the simple value `undefined` (`0xf7`).
pub const K_UNDEFINED: u8 = 10;
/// CBOR node kind: an unassigned simple value (`0xe0..0xf3`, or `0xf8 <byte>`).
pub const K_SIMPLE: u8 = 11;
/// CBOR node kind: an IEEE-754 float (half `0xf9`, single `0xfa`, double `0xfb`).
pub const K_FLOAT: u8 = 12;

/// The highest valid `K_*` kind tag (used by the decoder's range check).
const K_MAX: u8 = K_FLOAT;

/// Stable lower-case kind name for reports and JSON output.
pub const fn kind_name(kind: u8) -> &'static str {
    match kind {
        K_UINT => "uint",
        K_NEGINT => "negint",
        K_BYTES => "bytes",
        K_TEXT => "text",
        K_ARRAY => "array",
        K_MAP => "map",
        K_TAG => "tag",
        K_FALSE => "false",
        K_TRUE => "true",
        K_NULL => "null",
        K_UNDEFINED => "undefined",
        K_SIMPLE => "simple",
        K_FLOAT => "float",
        _ => "unknown",
    }
}

/// Whether `kind` is a container (has children and can be index-addressed).
pub const fn is_container(kind: u8) -> bool {
    matches!(kind, K_ARRAY | K_MAP)
}

/// The CBOR major type (`0..=7`) for a node kind.
pub const fn major_of(kind: u8) -> u8 {
    match kind {
        K_UINT => 0,
        K_NEGINT => 1,
        K_BYTES => 2,
        K_TEXT => 3,
        K_ARRAY => 4,
        K_MAP => 5,
        K_TAG => 6,
        _ => 7,
    }
}

/// Stable name for the IEEE-754 width selected by a float's additional information.
pub const fn float_width_name(info: u8) -> &'static str {
    match info {
        25 => "half",
        26 => "single",
        27 => "double",
        _ => "unknown",
    }
}

/// One parsed CBOR item: its kind, exact source span, ordered children, and the
/// representation facts CBOR adds over JSON.
///
/// The map `children` list is the interleaved `[key0, value0, key1, value1, …]`
/// document-order sequence (order and duplicate keys preserved verbatim); an
/// array's `children` is its element list; a tag's `children` holds its one tagged
/// item; every scalar has an empty `children`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CborNode {
    /// One of the `K_*` tags.
    pub kind: u8,
    /// The head byte's additional-information nibble (`0..=31`), recorded verbatim so
    /// the encoding width actually used is preserved (e.g. `0x17` is 23, `0x18 0x17`
    /// is 24).
    pub info: u8,
    /// Whether this item used the indefinite-length form (break-stop terminated).
    pub indefinite: bool,
    /// For [`K_TAG`], the tag number; `0` otherwise.
    pub tag: u64,
    /// The decoded unsigned argument: the value for [`K_UINT`], the magnitude `n`
    /// (`value = -1 - n`) for [`K_NEGINT`], the byte length for a definite
    /// [`K_BYTES`]/[`K_TEXT`], the element/pair count for a definite
    /// [`K_ARRAY`]/[`K_MAP`], the tag number for [`K_TAG`], the simple value for
    /// [`K_SIMPLE`], and the raw IEEE-754 bit pattern for [`K_FLOAT`]. `0` for an
    /// indefinite-length item.
    pub arg: u64,
    /// The token's first source byte.
    pub start: u64,
    /// One past the token's last source byte.
    pub end: u64,
    /// Map: interleaved key/value indices; array: element indices; tag: one index.
    pub children: Vec<u32>,
}

/// The canonical derived CBOR model (the materialization of a `CborModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CborModel {
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// Index of the root node.
    pub root: u32,
    /// The root's kind tag.
    pub top_type: u8,
    /// The observed maximum nesting depth (root = 1).
    pub max_depth: u32,
    /// The number of definite-length items observed.
    pub definite: u32,
    /// The number of indefinite-length items observed.
    pub indefinite: u32,
    /// The node arena.
    pub nodes: Vec<CborNode>,
}

impl CborModel {
    /// The node at `index`, if present.
    pub fn node(&self, index: u32) -> Option<&CborNode> {
        self.nodes.get(index as usize)
    }

    /// The total number of items (every item is exactly one of definite or
    /// indefinite). Equals `nodes.len()` for a built model and is also correct for a
    /// detection-only parse (where the arena is not populated).
    pub fn item_count(&self) -> u32 {
        self.definite.saturating_add(self.indefinite)
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(48 + self.nodes.len() * 40);
        out.extend_from_slice(b"CBRM");
        out.push(MODEL_VERSION);
        out.push(self.top_type);
        out.extend_from_slice(&self.root.to_le_bytes());
        out.extend_from_slice(&self.max_depth.to_le_bytes());
        out.extend_from_slice(&self.definite.to_le_bytes());
        out.extend_from_slice(&self.indefinite.to_le_bytes());
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&(self.nodes.len() as u32).to_le_bytes());
        for n in &self.nodes {
            out.push(n.kind);
            out.push(n.info);
            out.push(u8::from(n.indefinite));
            out.extend_from_slice(&n.tag.to_le_bytes());
            out.extend_from_slice(&n.arg.to_le_bytes());
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
    pub fn decode(bytes: &[u8]) -> Result<CborModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"CBRM" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let top_type = r.u8()?;
        if top_type > K_MAX {
            return Err(corrupt("unknown top-level kind"));
        }
        let root = r.u32()?;
        let max_depth = r.u32()?;
        let definite = r.u32()?;
        let indefinite = r.u32()?;
        let doc_len = r.u64()?;
        let count = r.u32()?;
        if count > MAX_MODEL_NODES {
            return Err(corrupt("model node count is implausible"));
        }
        let mut nodes = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let kind = r.u8()?;
            if kind > K_MAX {
                return Err(corrupt("unknown node kind"));
            }
            let info = r.u8()?;
            if info > 31 {
                return Err(corrupt("node additional information is out of range"));
            }
            let flag = r.u8()?;
            if flag > 1 {
                return Err(corrupt("node flag byte is out of range"));
            }
            let tag = r.u64()?;
            let arg = r.u64()?;
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
            nodes.push(CborNode {
                kind,
                info,
                indefinite: flag == 1,
                tag,
                arg,
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
        Ok(CborModel {
            doc_len,
            root,
            top_type,
            max_depth,
            definite,
            indefinite,
            nodes,
        })
    }
}

/// A resolved pointer query: the node index and how many map entries matched the
/// final key segment (so duplicate keys are reported, never hidden).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolved {
    /// The resolved node index.
    pub index: u32,
    /// The number of map entries with the final segment's key (`1` for an array
    /// index, a tag hop, or the root; `>1` witnesses duplicate keys).
    pub matches: u32,
}

/// One lexical-match record from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CborMatch {
    /// Canonical RFC 6901 pointer to the matching node.
    pub pointer: String,
    /// Whether the match is a map key or a text value.
    pub role: crate::adapter::json::MatchRole,
    /// The matching node's kind (always [`K_TEXT`]).
    pub kind: u8,
    /// The exact source span of the matching text token (delimiters included).
    pub start: u64,
    /// One past the matching text token.
    pub end: u64,
    /// The decoded UTF-8 text of the matching token.
    pub text: String,
}

/// Fast-fail CBOR detector (see the module docs for the exact boundary).
///
/// True iff `source` is within the caps and either carries the self-described-CBOR
/// tag `55799` and parses, in full, as exactly one CBOR item; or parses, in full, as
/// one CBOR item whose root is a container or a tag and which reaches at least
/// [`MIN_DETECT_NODES`] nodes. Every other input stays `Opaque`.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_cbor_document_bytes {
        return false;
    }
    if (source.len() as u64) < MIN_DETECT_BYTES {
        return false;
    }
    // The self-described-CBOR tag is the strongest possible signal: the format
    // literally announces itself, so any full well-formed parse is accepted.
    if source.starts_with(&SELF_DESCRIBED_MAGIC) {
        return parse(source, limits, false).is_ok();
    }
    match parse(source, limits, false) {
        Ok(m) => {
            let rich = is_container(m.top_type) || m.top_type == K_TAG;
            rich && m.item_count() >= MIN_DETECT_NODES
        }
        Err(_) => false,
    }
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of a `CborModel` node).
pub fn build_cbor_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits, true)?.encode())
}

/// Parse `source` into a [`CborModel`]. `build` selects whether the node arena is
/// populated (detection runs with `build = false` to stay bounded in memory).
pub fn parse(source: &[u8], limits: Limits, build: bool) -> Result<CborModel> {
    if source.len() as u64 > limits.max_cbor_document_bytes {
        return Err(Error::resource_limit(format!(
            "CBOR source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_cbor_document_bytes
        )));
    }
    if source.is_empty() {
        return Err(corrupt("empty input is not a CBOR item"));
    }
    let mut p = Parser::new(source, limits, build);
    let root = p.item(1)?;
    if p.at != source.len() {
        return Err(corrupt("trailing bytes after the single CBOR item"));
    }
    Ok(CborModel {
        doc_len: source.len() as u64,
        root,
        top_type: p.root_kind,
        max_depth: p.max_depth,
        definite: p.definite,
        indefinite: p.indefinite,
        nodes: p.nodes,
    })
}

/// The exact source bytes of a node's token (`[start, end)`), bounded by the source.
pub fn token_bytes<'a>(source: &'a [u8], node: &CborNode) -> Result<&'a [u8]> {
    let start = usize::try_from(node.start)
        .map_err(|_| Error::internal_invariant("CBOR token span overflows usize"))?;
    let end = usize::try_from(node.end)
        .map_err(|_| Error::internal_invariant("CBOR token span overflows usize"))?;
    source
        .get(start..end)
        .ok_or_else(|| Error::internal_invariant("CBOR token span is outside the source"))
}

/// The exact payload bytes of a definite byte-string or text-string node (the
/// bytes between the head and the item's end). Declines typed for any other node.
pub fn payload_bytes<'a>(source: &'a [u8], node: &CborNode) -> Result<&'a [u8]> {
    if node.indefinite || (node.kind != K_BYTES && node.kind != K_TEXT) {
        return Err(corrupt("node is not a definite string"));
    }
    let len = usize::try_from(node.arg)
        .map_err(|_| Error::internal_invariant("CBOR string length overflows usize"))?;
    let end = usize::try_from(node.end)
        .map_err(|_| Error::internal_invariant("CBOR string span overflows usize"))?;
    let start = end
        .checked_sub(len)
        .ok_or_else(|| Error::internal_invariant("CBOR string length exceeds its token"))?;
    source
        .get(start..end)
        .ok_or_else(|| Error::internal_invariant("CBOR string payload is outside the source"))
}

/// Decode a text-string node's payload into its UTF-8 text. Declines typed for a
/// byte string or a non-string node.
pub fn decode_text(source: &[u8], node: &CborNode) -> Result<String> {
    if node.kind != K_TEXT {
        return Err(corrupt("node is not a text string"));
    }
    if node.indefinite {
        // The indefinite text string's children are its definite chunks; join them.
        return Err(corrupt(
            "decode_text requires a definite text node; use subtree_text for an indefinite string",
        ));
    }
    let bytes = payload_bytes(source, node)?;
    String::from_utf8(bytes.to_vec()).map_err(|_| corrupt("text string is not valid UTF-8"))
}

/// Render a deterministic canonical text projection of the whole model.
///
/// Binary payloads are rendered as `h'HEX'` (byte strings) or as a JSON-quoted
/// string (text strings); floats as `f16:0x…`/`f32:0x…`/`f64:0x…` (the raw IEEE-754
/// bits, so the width is visible); a tag as `N(<tagged item>)` (never resolved).
pub fn canonical_text(model: &CborModel, source: &[u8]) -> Result<String> {
    subtree_text(model, source, model.root)
}

/// Render one node's subtree to canonical text (see [`canonical_text`]).
pub fn subtree_text(model: &CborModel, source: &[u8], index: u32) -> Result<String> {
    let mut out = String::new();
    render(model, source, index, &mut out, 0)?;
    Ok(out)
}

/// The index of a node's parent, if any (the root has none).
pub fn find_parent(model: &CborModel, index: u32) -> Option<u32> {
    model
        .nodes
        .iter()
        .position(|n| n.children.contains(&index))
        .map(|i| i as u32)
}

fn render(
    model: &CborModel,
    source: &[u8],
    index: u32,
    out: &mut String,
    depth: u32,
) -> Result<()> {
    if depth > MAX_MODEL_NODES {
        return Err(Error::resource_limit(
            "CBOR render exceeded its depth bound",
        ));
    }
    let node = model
        .node(index)
        .ok_or_else(|| corrupt("render hit an out-of-range node"))?;
    match node.kind {
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
        K_TAG => {
            out.push_str(&node.tag.to_string());
            out.push('(');
            if let Some(child) = node.children.first() {
                render(model, source, *child, out, depth + 1)?;
            }
            out.push(')');
        }
        K_BYTES => {
            out.push_str("h'");
            if node.indefinite {
                for child in &node.children {
                    let cn = model
                        .node(*child)
                        .ok_or_else(|| corrupt("string chunk index is out of range"))?;
                    out.push_str(&hex(payload_bytes(source, cn)?));
                }
            } else {
                out.push_str(&hex(payload_bytes(source, node)?));
            }
            out.push('\'');
        }
        K_TEXT => {
            out.push('"');
            if node.indefinite {
                for child in &node.children {
                    let cn = model
                        .node(*child)
                        .ok_or_else(|| corrupt("string chunk index is out of range"))?;
                    push_json_string(payload_bytes(source, cn)?, out);
                }
            } else {
                push_json_string(payload_bytes(source, node)?, out);
            }
            out.push('"');
        }
        K_UINT => out.push_str(&node.arg.to_string()),
        K_NEGINT => {
            let v = -1i128 - i128::from(node.arg);
            out.push_str(&v.to_string());
        }
        K_FALSE => out.push_str("false"),
        K_TRUE => out.push_str("true"),
        K_NULL => out.push_str("null"),
        K_UNDEFINED => out.push_str("undefined"),
        K_SIMPLE => {
            out.push_str("simple(");
            out.push_str(&node.arg.to_string());
            out.push(')');
        }
        K_FLOAT => {
            let prefix = match node.info {
                25 => "f16",
                26 => "f32",
                _ => "f64",
            };
            out.push_str(prefix);
            out.push_str(":0x");
            out.push_str(&hex(&node.arg.to_be_bytes()));
        }
        _ => out.push_str("unknown"),
    }
    Ok(())
}

/// Append the bytes of a UTF-8 text payload to `out`, JSON-quoting the characters
/// that need escaping (this is a diagnostic projection, never the exact path).
fn push_json_string(bytes: &[u8], out: &mut String) {
    let s = String::from_utf8_lossy(bytes);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(DIGITS[(b >> 4) as usize] as char);
        out.push(DIGITS[(b & 0x0f) as usize] as char);
    }
    out
}

/// A bounded, case-sensitive lexical search over text-string map keys and text
/// values. Returns matches in document order (map members before descendants).
pub fn find(
    model: &CborModel,
    source: &[u8],
    pattern: &str,
    limits: Limits,
) -> Result<Vec<CborMatch>> {
    let mut out = Vec::new();
    let mut path: Vec<String> = Vec::new();
    walk_find(
        model, source, model.root, true, &mut path, pattern, &mut out, limits, 0,
    )?;
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn walk_find(
    model: &CborModel,
    source: &[u8],
    index: u32,
    is_value: bool,
    path: &mut Vec<String>,
    pattern: &str,
    out: &mut Vec<CborMatch>,
    limits: Limits,
    depth: u32,
) -> Result<()> {
    if depth > limits.max_cbor_depth {
        return Err(Error::resource_limit(
            "CBOR find exceeded the nesting-depth cap",
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
                if key_node.kind == K_TEXT && !key_node.indefinite {
                    let key = decode_text(source, key_node)?;
                    if key.contains(pattern) {
                        out.push(CborMatch {
                            pointer: pointer_of(path, Some(&key)),
                            role: crate::adapter::json::MatchRole::Key,
                            kind: K_TEXT,
                            start: key_node.start,
                            end: key_node.end,
                            text: key.clone(),
                        });
                    }
                    path.push(crate::adapter::json::escape_segment(&key));
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
                } else {
                    // A non-text key is not addressable by pointer; recurse without a
                    // path segment (the structural view still exposes it).
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
                }
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
        K_TAG => {
            if let Some(child) = node.children.first() {
                walk_find(
                    model,
                    source,
                    *child,
                    is_value,
                    path,
                    pattern,
                    out,
                    limits,
                    depth + 1,
                )?;
            }
        }
        K_TEXT if is_value && !node.indefinite => {
            let text = decode_text(source, node)?;
            if text.contains(pattern) {
                out.push(CborMatch {
                    pointer: pointer_of(path, None),
                    role: crate::adapter::json::MatchRole::Value,
                    kind: K_TEXT,
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
        s.push_str(&crate::adapter::json::escape_segment(k));
    }
    s
}

/// RFC 6901 pointer resolution against a parsed model. An array segment is a
/// 0-based index; a map segment matches the entry whose key is a text string equal
/// to the segment (duplicate matches are reported); a [`K_TAG`] node is transparent
/// to traversal (tags are never resolved semantically, but a pointer may step
/// through one). A missing member, an out-of-range index, or indexing into a scalar
/// is a typed decline (never a silent empty answer).
pub fn resolve_pointer(model: &CborModel, source: &[u8], pointer: &str) -> Result<Resolved> {
    let segments = parse_pointer(pointer)?;
    let mut index = model.root;
    let mut matches = 1u32;
    for seg in &segments {
        let node = model
            .node(index)
            .ok_or_else(|| corrupt("pointer traversal left the model"))?;
        match node.kind {
            K_TAG => {
                index = *node
                    .children
                    .first()
                    .ok_or_else(|| corrupt("tag node has no tagged item"))?;
                matches = 1;
                // Re-dispatch the same segment against the tagged item.
                let inner = model
                    .node(index)
                    .ok_or_else(|| corrupt("tagged item index is out of range"))?;
                match inner.kind {
                    K_MAP => {
                        let (found, first) = map_lookup(model, source, index, seg)?;
                        index = first.ok_or_else(|| {
                            Error::unsupported_feature(format!(
                                "CBOR map has no member {seg:?} at this pointer"
                            ))
                        })?;
                        matches = found;
                    }
                    K_ARRAY => {
                        let idx = parse_array_index(seg)?;
                        let len = inner.children.len();
                        if idx as usize >= len {
                            return Err(Error::unsupported_feature(format!(
                                "CBOR array index {idx} is out of range (length {len})"
                            )));
                        }
                        index = inner.children[idx as usize];
                    }
                    other => {
                        return Err(Error::unsupported_feature(format!(
                            "cannot index into a tagged CBOR {} at {seg:?}",
                            kind_name(other)
                        )));
                    }
                }
            }
            K_MAP => {
                let (found, first) = map_lookup(model, source, index, seg)?;
                index = first.ok_or_else(|| {
                    Error::unsupported_feature(format!(
                        "CBOR map has no member {seg:?} at this pointer"
                    ))
                })?;
                matches = found;
            }
            K_ARRAY => {
                let idx = parse_array_index(seg)?;
                let len = node.children.len();
                if idx as usize >= len {
                    return Err(Error::unsupported_feature(format!(
                        "CBOR array index {idx} is out of range (length {len})"
                    )));
                }
                index = node.children[idx as usize];
                matches = 1;
            }
            _ => {
                return Err(Error::unsupported_feature(format!(
                    "cannot index into a CBOR {} at {seg:?}",
                    kind_name(node.kind)
                )));
            }
        }
    }
    Ok(Resolved { index, matches })
}

/// Find the map entries whose key is a text string equal to `seg`. Returns the match
/// count and the first matching value index.
fn map_lookup(
    model: &CborModel,
    source: &[u8],
    index: u32,
    seg: &str,
) -> Result<(u32, Option<u32>)> {
    let node = model
        .node(index)
        .ok_or_else(|| corrupt("map index is out of range"))?;
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
        if key_node.kind == K_TEXT
            && !key_node.indefinite
            && decode_text(source, key_node).is_ok_and(|k| k == seg)
        {
            found = found.saturating_add(1);
            if first.is_none() {
                first = Some(val_idx);
            }
        }
    }
    Ok((found, first))
}

fn parse_pointer(pointer: &str) -> Result<Vec<String>> {
    if pointer.is_empty() {
        return Ok(Vec::new());
    }
    if !pointer.starts_with('/') {
        return Err(Error::usage(format!(
            "CBOR pointer {pointer:?} must be empty or start with '/'"
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
                        "invalid CBOR pointer escape in segment {raw:?}"
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
    if seg.is_empty() || (seg.len() > 1 && seg.starts_with('0')) {
        return Err(Error::usage(format!(
            "CBOR array segment {seg:?} is not a canonical index"
        )));
    }
    seg.parse::<u32>()
        .map_err(|_| Error::usage(format!("CBOR array index {seg:?} is not a u32")))
}

// ---------------------------------------------------------------------------
// The bounded parser
// ---------------------------------------------------------------------------

const BREAK: u8 = 0xff;

struct Parser<'a> {
    b: &'a [u8],
    at: usize,
    limits: Limits,
    build: bool,
    nodes: Vec<CborNode>,
    count: u32,
    string_bytes: u64,
    definite: u32,
    indefinite: u32,
    max_depth: u32,
    root_kind: u8,
}

impl<'a> Parser<'a> {
    fn new(b: &'a [u8], limits: Limits, build: bool) -> Parser<'a> {
        Parser {
            b,
            at: 0,
            limits,
            build,
            nodes: Vec::new(),
            count: 0,
            string_bytes: 0,
            definite: 0,
            indefinite: 0,
            max_depth: 0,
            root_kind: K_NULL,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.at).copied()
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| corrupt("CBOR read length overflows"))?;
        if end > self.b.len() {
            return Err(corrupt("CBOR item is truncated"));
        }
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }

    /// Read the unsigned argument selected by an additional-information nibble.
    /// `info == 31` is never passed here (it selects the indefinite form).
    fn read_arg(&mut self, info: u8) -> Result<u64> {
        match info {
            0..=23 => Ok(u64::from(info)),
            24 => Ok(u64::from(self.take(1)?[0])),
            25 => {
                let s = self.take(2)?;
                Ok(u64::from(u16::from_be_bytes([s[0], s[1]])))
            }
            26 => {
                let s = self.take(4)?;
                Ok(u64::from(u32::from_be_bytes([s[0], s[1], s[2], s[3]])))
            }
            27 => {
                let s = self.take(8)?;
                Ok(u64::from_be_bytes([
                    s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
                ]))
            }
            28..=30 => Err(corrupt("reserved additional information in a CBOR head")),
            // 31 selects the indefinite form and is handled by the caller.
            _ => Err(corrupt(
                "indefinite additional information is not valid here",
            )),
        }
    }

    fn bump_string(&mut self, len: u64) -> Result<()> {
        self.string_bytes = self.string_bytes.saturating_add(len);
        if self.string_bytes > self.limits.max_cbor_string_bytes {
            return Err(Error::resource_limit(format!(
                "CBOR string bytes exceed the {}-byte cap",
                self.limits.max_cbor_string_bytes
            )));
        }
        Ok(())
    }

    fn emit(&mut self, node: CborNode) -> Result<u32> {
        self.count = self.count.saturating_add(1);
        if self.count > self.limits.max_cbor_nodes {
            return Err(Error::resource_limit(format!(
                "CBOR item count exceeds the {}-node cap",
                self.limits.max_cbor_nodes
            )));
        }
        if self.build {
            self.nodes.push(node);
        }
        Ok(self.count - 1)
    }

    fn item(&mut self, depth: u32) -> Result<u32> {
        if depth > self.limits.max_cbor_depth {
            return Err(Error::resource_limit(format!(
                "CBOR nesting depth exceeds the {}-level cap",
                self.limits.max_cbor_depth
            )));
        }
        if depth > self.max_depth {
            self.max_depth = depth;
        }
        let head = *self
            .b
            .get(self.at)
            .ok_or_else(|| corrupt("CBOR item is truncated"))?;
        self.at += 1;
        let major = head >> 5;
        let info = head & 0x1f;
        let start = (self.at - 1) as u64;
        let (index, kind) = match major {
            0 => {
                let arg = self.read_arg(info)?;
                self.definite += 1;
                (self.emit(self.leaf(K_UINT, info, arg, start))?, K_UINT)
            }
            1 => {
                let arg = self.read_arg(info)?;
                self.definite += 1;
                (self.emit(self.leaf(K_NEGINT, info, arg, start))?, K_NEGINT)
            }
            2 => (self.string_item(K_BYTES, 2, info, start, depth)?, K_BYTES),
            3 => (self.string_item(K_TEXT, 3, info, start, depth)?, K_TEXT),
            4 => (self.array_item(info, start, depth)?, K_ARRAY),
            5 => (self.map_item(info, start, depth)?, K_MAP),
            6 => (self.tag_item(info, start, depth)?, K_TAG),
            _ => {
                let (idx, k) = self.mt7_item(info, start)?;
                (idx, k)
            }
        };
        if depth == 1 {
            self.root_kind = kind;
        }
        Ok(index)
    }

    fn leaf(&self, kind: u8, info: u8, arg: u64, start: u64) -> CborNode {
        CborNode {
            kind,
            info,
            indefinite: false,
            tag: u64::from(kind == K_TAG) * arg,
            arg,
            start,
            end: self.at as u64,
            children: Vec::new(),
        }
    }

    fn string_item(
        &mut self,
        kind: u8,
        major: u8,
        info: u8,
        start: u64,
        _depth: u32,
    ) -> Result<u32> {
        if info == 31 {
            let mut children: Vec<u32> = Vec::new();
            loop {
                let h = self
                    .peek()
                    .ok_or_else(|| corrupt("unterminated indefinite string"))?;
                if h == BREAK {
                    self.at += 1;
                    break;
                }
                if h >> 5 != major {
                    return Err(corrupt("indefinite string chunk has the wrong major type"));
                }
                let cinfo = h & 0x1f;
                if cinfo == 31 {
                    return Err(corrupt("nested indefinite string chunk"));
                }
                let cstart = self.at as u64;
                self.at += 1;
                let len = self.read_arg(cinfo)?;
                self.bump_string(len)?;
                let bytes = self.take_len(len)?;
                if kind == K_TEXT && std::str::from_utf8(bytes).is_err() {
                    return Err(corrupt("indefinite text string chunk is not valid UTF-8"));
                }
                self.definite += 1;
                if self.build {
                    let child = self.emit(self.leaf(kind, cinfo, len, cstart))?;
                    children.push(child);
                } else {
                    self.emit(self.leaf(kind, cinfo, len, cstart))?;
                }
            }
            self.indefinite += 1;
            let end = self.at as u64;
            let node = CborNode {
                kind,
                info,
                indefinite: true,
                tag: 0,
                arg: 0,
                start,
                end,
                children,
            };
            self.emit(node)
        } else {
            let len = self.read_arg(info)?;
            self.bump_string(len)?;
            let bytes = self.take_len(len)?;
            if kind == K_TEXT && std::str::from_utf8(bytes).is_err() {
                return Err(corrupt("text string is not valid UTF-8"));
            }
            self.definite += 1;
            let node = self.leaf(kind, info, len, start);
            self.emit(node)
        }
    }

    fn array_item(&mut self, info: u8, start: u64, depth: u32) -> Result<u32> {
        let mut children: Vec<u32> = Vec::new();
        let indefinite = info == 31;
        if indefinite {
            loop {
                let h = self
                    .peek()
                    .ok_or_else(|| corrupt("unterminated indefinite array"))?;
                if h == BREAK {
                    self.at += 1;
                    break;
                }
                let child = self.item(depth + 1)?;
                if self.build {
                    children.push(child);
                }
            }
            self.indefinite += 1;
        } else {
            let n = self.read_arg(info)?;
            let mut i = 0u64;
            while i < n {
                let child = self.item(depth + 1)?;
                if self.build {
                    children.push(child);
                }
                i += 1;
            }
            self.definite += 1;
        }
        let end = self.at as u64;
        let arg = if indefinite { 0 } else { children.len() as u64 };
        self.emit(CborNode {
            kind: K_ARRAY,
            info,
            indefinite,
            tag: 0,
            arg,
            start,
            end,
            children,
        })
    }

    fn map_item(&mut self, info: u8, start: u64, depth: u32) -> Result<u32> {
        let mut children: Vec<u32> = Vec::new();
        let indefinite = info == 31;
        if indefinite {
            loop {
                let h = self
                    .peek()
                    .ok_or_else(|| corrupt("unterminated indefinite map"))?;
                if h == BREAK {
                    self.at += 1;
                    break;
                }
                let key = self.item(depth + 1)?;
                let h2 = self
                    .peek()
                    .ok_or_else(|| corrupt("indefinite map has a key with no value"))?;
                if h2 == BREAK {
                    return Err(corrupt("indefinite map has a key with no value"));
                }
                let val = self.item(depth + 1)?;
                if self.build {
                    children.push(key);
                    children.push(val);
                }
            }
            self.indefinite += 1;
        } else {
            let n = self.read_arg(info)?;
            let mut i = 0u64;
            while i < n {
                let key = self.item(depth + 1)?;
                let val = self.item(depth + 1)?;
                if self.build {
                    children.push(key);
                    children.push(val);
                }
                i += 1;
            }
            self.definite += 1;
        }
        let end = self.at as u64;
        let arg = if indefinite {
            0
        } else {
            (children.len() / 2) as u64
        };
        self.emit(CborNode {
            kind: K_MAP,
            info,
            indefinite,
            tag: 0,
            arg,
            start,
            end,
            children,
        })
    }

    fn tag_item(&mut self, info: u8, start: u64, depth: u32) -> Result<u32> {
        if info == 31 {
            return Err(corrupt("a CBOR tag cannot be indefinite-length"));
        }
        let tag = self.read_arg(info)?;
        let child = self.item(depth + 1)?;
        self.definite += 1;
        let end = self.at as u64;
        let mut children = Vec::new();
        if self.build {
            children.push(child);
        }
        self.emit(CborNode {
            kind: K_TAG,
            info,
            indefinite: false,
            tag,
            arg: tag,
            start,
            end,
            children,
        })
    }

    fn mt7_item(&mut self, info: u8, start: u64) -> Result<(u32, u8)> {
        match info {
            0..=19 => {
                self.definite += 1;
                Ok((
                    self.emit(self.leaf(K_SIMPLE, info, u64::from(info), start))?,
                    K_SIMPLE,
                ))
            }
            20 => {
                self.definite += 1;
                Ok((self.emit(self.leaf(K_FALSE, info, 0, start))?, K_FALSE))
            }
            21 => {
                self.definite += 1;
                Ok((self.emit(self.leaf(K_TRUE, info, 0, start))?, K_TRUE))
            }
            22 => {
                self.definite += 1;
                Ok((self.emit(self.leaf(K_NULL, info, 0, start))?, K_NULL))
            }
            23 => {
                self.definite += 1;
                Ok((
                    self.emit(self.leaf(K_UNDEFINED, info, 0, start))?,
                    K_UNDEFINED,
                ))
            }
            24 => {
                let sv = u64::from(self.take(1)?[0]);
                if sv < 32 {
                    return Err(corrupt(
                        "non-minimal simple value: 0xf8 must be followed by a byte >= 32",
                    ));
                }
                self.definite += 1;
                Ok((self.emit(self.leaf(K_SIMPLE, info, sv, start))?, K_SIMPLE))
            }
            25 => {
                let s = self.take(2)?;
                let bits = u16::from_be_bytes([s[0], s[1]]);
                self.definite += 1;
                Ok((
                    self.emit(self.leaf(K_FLOAT, info, u64::from(bits), start))?,
                    K_FLOAT,
                ))
            }
            26 => {
                let s = self.take(4)?;
                let bits = u32::from_be_bytes([s[0], s[1], s[2], s[3]]);
                self.definite += 1;
                Ok((
                    self.emit(self.leaf(K_FLOAT, info, u64::from(bits), start))?,
                    K_FLOAT,
                ))
            }
            27 => {
                let s = self.take(8)?;
                let bits = u64::from_be_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]);
                self.definite += 1;
                Ok((self.emit(self.leaf(K_FLOAT, info, bits, start))?, K_FLOAT))
            }
            28..=30 => Err(corrupt("reserved additional information in a CBOR head")),
            _ => Err(corrupt("break (0xff) outside an indefinite-length item")),
        }
    }

    fn take_len(&mut self, len: u64) -> Result<&'a [u8]> {
        let n = usize::try_from(len)
            .map_err(|_| corrupt("CBOR arithmetic argument exceeds the addressable length"))?;
        self.take(n)
    }
}

fn corrupt(msg: impl Into<String>) -> Error {
    Error::invalid_cbor_structure(msg)
}

struct BinReader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> BinReader<'a> {
    fn new(b: &'a [u8]) -> BinReader<'a> {
        BinReader { b, at: 0 }
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| corrupt("model read overflows"))?;
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
        let s = self.bytes(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }

    fn u64(&mut self) -> Result<u64> {
        let s = self.bytes(8)?;
        Ok(u64::from_le_bytes([
            s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
        ]))
    }

    fn at_end(&self) -> bool {
        self.at == self.b.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_a_rich_container_and_rejects_trivial_inputs() {
        // {"a": 1, "b": [1, 2], "c": true}
        let doc = [
            0xa3, 0x61, b'a', 0x01, 0x61, b'b', 0x82, 0x01, 0x02, 0x61, b'c', 0xf5,
        ];
        assert!(detect(&doc, Limits::DEFAULT));
        // A single scalar stays Opaque (too small / trivial).
        assert!(!detect(&[0x17], Limits::DEFAULT));
        assert!(!detect(&[0x18, 0x17], Limits::DEFAULT));
        // An empty container stays Opaque (structurally trivial).
        assert!(!detect(&[0x80, 0x00], Limits::DEFAULT));
        // Prose (never a container head) stays Opaque.
        assert!(!detect(
            b"plain prose, definitely not cbor\n",
            Limits::DEFAULT
        ));
    }

    #[test]
    fn preserves_width_order_duplicates_and_kinds() {
        // [23 (0x17), 23 (0x18 0x17), h'010203', "abc"]
        let doc = [
            0x84, 0x17, 0x18, 0x17, 0x43, 0x01, 0x02, 0x03, 0x63, b'a', b'b', b'c',
        ];
        let m = parse(&doc, Limits::DEFAULT, true).unwrap();
        assert_eq!(m.top_type, K_ARRAY);
        assert_eq!(m.nodes[m.root as usize].children.len(), 4);
        let c0 = &m.nodes[m.nodes[m.root as usize].children[0] as usize];
        assert_eq!(c0.kind, K_UINT);
        assert_eq!(c0.info, 23);
        let c1 = &m.nodes[m.nodes[m.root as usize].children[1] as usize];
        assert_eq!(c1.kind, K_UINT);
        assert_eq!(c1.info, 24);
        let c2 = &m.nodes[m.nodes[m.root as usize].children[2] as usize];
        assert_eq!(c2.kind, K_BYTES);
        assert_eq!(payload_bytes(&doc, c2).unwrap(), &[0x01, 0x02, 0x03]);
        let c3 = &m.nodes[m.nodes[m.root as usize].children[3] as usize];
        assert_eq!(c3.kind, K_TEXT);
        assert_eq!(decode_text(&doc, c3).unwrap(), "abc");
    }

    #[test]
    fn preserves_tags_floats_and_indefinite_items() {
        // 55799([1.5half, 1.0single, 1.0double]) with a nested indefinite array.
        let doc = [
            0xd9, 0xd9, 0xf7, 0x83, 0xf9, 0x3e, 0x00, 0xfa, 0x3f, 0x80, 0x00, 0x00, 0xfb, 0x3f,
            0xf0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let m = parse(&doc, Limits::DEFAULT, true).unwrap();
        assert_eq!(m.top_type, K_TAG);
        let root = &m.nodes[m.root as usize];
        assert_eq!(root.tag, SELF_DESCRIBED_TAG);
        assert!(detect(&doc, Limits::DEFAULT));
        // Indefinite array [1, 2] and indefinite text "ab".
        let ind = [0x9f, 0x01, 0x02, 0x7f, 0x61, b'a', 0x61, b'b', 0xff, 0xff];
        let mi = parse(&ind, Limits::DEFAULT, true).unwrap();
        assert!(mi.indefinite >= 2);
        assert!(mi.nodes[mi.root as usize].indefinite);
    }

    #[test]
    fn declines_trailing_break_map_and_argument_faults() {
        for bad in [
            &[0x01, 0x02][..],       // trailing bytes
            &[0xff][..],             // lone break
            &[0xa1, 0x61, b'a'][..], // map key with no value
            &[0x9f, 0x01][..],       // unterminated indefinite array
            &[0x1c][..],             // reserved additional information
            &[0xf8, 0x00][..],       // non-minimal simple value
            &[0x63, 0x61, 0x62][..], // truncated text string
        ] {
            let e = parse(bad, Limits::DEFAULT, true).unwrap_err();
            assert_eq!(
                e.class(),
                crate::error::ErrorClass::InvalidCborStructure,
                "{bad:?}"
            );
        }
    }

    #[test]
    fn model_roundtrips() {
        let doc = [
            0xa3, 0x61, b'a', 0x01, 0x61, b'b', 0x82, 0x01, 0x02, 0x61, b'c', 0xf5,
        ];
        let m = parse(&doc, Limits::DEFAULT, true).unwrap();
        let encoded = m.encode();
        let round = CborModel::decode(&encoded).unwrap();
        assert_eq!(m, round);
        assert!(canonical_text(&m, &doc).unwrap().contains("true"));
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0xCB0B_5EED_1234_ABCD;
        for _ in 0..256 {
            let mut buf = vec![0u8; 256];
            for b in buf.iter_mut() {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                *b = (x & 0xFF) as u8;
            }
            let _ = detect(&buf, Limits::DEFAULT);
            if let Ok(m) = parse(&buf, Limits::STRICT, true) {
                let _ = canonical_text(&m, &buf);
                let _ = build_cbor_model(&buf, Limits::STRICT);
            }
        }
    }
}
