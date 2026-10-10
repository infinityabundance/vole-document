//! Bounded, representation-preserving MessagePack adapter (Phase 21.19).
//!
//! MessagePack (<https://github.com/msgpack/msgpack/blob/master/spec.md>) is a
//! **binary** structured-tree Wave-2 format, the sibling of CBOR (RFC 8949). Like
//! the other Wave-2 formats it is *not* an office package: there is no OPC/ZIP
//! layer, no `mimetype`, and no relationship graph. The exact leaf is therefore the
//! **whole source** (a `DocumentExact`, a RAW-like authority), and everything this
//! module produces is a bounded, deterministic (`Q_gen`) projection that never sits
//! on the exactness path.
//!
//! ## Same structured-tree observation vocabulary
//!
//! Like the JSON/JSON5/CBOR adapters ([`crate::adapter::json`],
//! [`crate::adapter::json5`], [`crate::adapter::cbor`]) this adapter produces a node
//! arena with a **stable kind tag**, an **exact `[start, end)` source span**, and an
//! **ordered child list** (map children are the interleaved `key0, value0, key1,
//! value1, …` sequence, so member order and duplicate keys are preserved verbatim;
//! an array's children are its elements). On top of the shared vocabulary it records
//! the MessagePack-specific representation facts that have no JSON analogue.
//!
//! ## What is preserved (exactly, for every token)
//!
//! * the **exact format byte** ([`MsgpackNode::head`]) that introduced every item, so
//!   the encoding width **and signedness actually used** are preserved: `0x17`
//!   (`positive fixint` 23) is distinguishable from `0xcc 0x17` (`uint8` 23), and a
//!   value encoded as a signed integer (`0xd0 0x7f`) is distinguishable from the
//!   same value encoded unsigned (`0xcc 0x7f`); both become a [`K_UINT`] node but with
//!   a different `head`;
//! * every kind: positive fixint / `uint8/16/32/64`, negative fixint /
//!   `int8/16/32/64`, `fixmap`/`map16`/`map32`, `fixarray`/`array16`/`array32`,
//!   `fixstr`/`str8/16/32`, `bin8/16/32`, `nil`/`false`/`true`, `float32`/`float64`,
//!   and `ext8/16/32` + `fixext1/2/4/8/16`;
//! * **str vs bin as distinct kinds** ([`K_STR`] vs [`K_BIN`], never conflated — the
//!   defining MessagePack byte-vs-text distinction), and a `str` payload must be
//!   well-formed UTF-8;
//! * **map key order** and **duplicate keys** (kept as distinct children, never
//!   overwritten);
//! * **float width** — `float32` (`0xca`, 32 bits) vs `float64` (`0xcb`, 64 bits) —
//!   with the IEEE-754 bit pattern recorded verbatim in [`MsgpackNode::arg`];
//! * **extension type numbers and payload length** ([`MsgpackNode::ext_type`],
//!   [`MsgpackNode::arg`]) are preserved verbatim and are **never** interpreted or
//!   expanded into the extension's semantics (a `timestamp` extension is not turned
//!   into a date, an `ext(-1)` timestamp is not decoded).
//!
//! MessagePack has **no** indefinite-length form (unlike CBOR's break-stop `0xff`),
//! so every item is definite: there is no `indefinite` flag.
//!
//! ## Detection (the honest boundary)
//!
//! MessagePack has **no magic bytes**, so a universal detector is impossible and any
//! detector is inherently heuristic. [`detect`] is therefore deliberately
//! conservative and claims an input only on a strong signal:
//!
//! 1. a **full-input, well-formed** parse of exactly one item whose root is a
//!    **container** (array or map) that reaches at least [`MIN_DETECT_NODES`] items
//!    and whose source reaches at least [`MIN_DETECT_BYTES`] bytes; **or**
//! 2. the same, with a root head byte that is **unambiguous MessagePack-only** —
//!    `map16`/`map32`/`array16`/`array32` (`0xdc..=0xdf`), whose byte value CBOR's
//!    own grammar rejects (reserved additional information `28..=30`, or an
//!    indefinite-length tag `31`) — so no length gate is required.
//!
//! Every other input stays `Opaque`. A lone scalar, an empty container, and any
//! structurally trivial input stay `Opaque`; a pure-ASCII document can never begin
//! with a container head byte (`>= 0x80`), so JSON/YAML/TOML/XML/HTML/prose sources
//! are never claimed.
//!
//! ### What it cannot distinguish (recorded negative)
//!
//! MessagePack and CBOR overlap in the single-byte representation of the whole
//! numbers `0..=23` (MessagePack `fixint` is CBOR's inline `uint`) and in their
//! short-container prefixes (`fixarray`/`fixmap` low nibble is a MessagePack element
//! **count** but CBOR's additional-information **selector**), so a small/trivial
//! source in that range is genuinely ambiguous. [`MIN_DETECT_BYTES`] is deliberately
//! larger than the trivially small overlap fixtures (the CBOR adapter's 4-byte
//! MessagePack `fixarray(3)` and 5-byte `fixmap(2)` controls), so those stay
//! `Opaque` here too. The two encodings also cannot always be told apart for a
//! well-formed source that satisfies **both** grammars; because CBOR's detector is
//! tried **before** this one (its self-described tag is the strongest binary signal),
//! such an input is honestly classified `Cbor`, never guessed to be MessagePack.
//! If a source is well-formed MessagePack but **also** a complete well-formed CBOR
//! container, CBOR wins; if it is well-formed MessagePack that CBOR's grammar
//! rejects, MessagePack wins.
//!
//! ## Bounds and no panics
//!
//! Untrusted input can only ever yield a typed decline, never a panic or an
//! unbounded allocation: the nesting depth by [`Limits::max_msgpack_depth`], the
//! node count by [`Limits::max_msgpack_nodes`], the raw `str` bytes by
//! [`Limits::max_msgpack_str_bytes`], the raw `bin` bytes by
//! [`Limits::max_msgpack_bin_bytes`], the raw extension payload bytes by
//! [`Limits::max_msgpack_ext_bytes`], and the source length by
//! [`Limits::max_msgpack_document_bytes`]. All length arithmetic is checked.

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model nodes (defends the decoder against a hostile blob).
pub const MAX_MODEL_NODES: u32 = 1 << 24;

/// The minimum item count a source must reach to be detected as MessagePack (so a
/// single scalar or an empty container stays `Opaque`).
pub const MIN_DETECT_NODES: u32 = 3;
/// The minimum source length admitted for structural detection unless the root head
/// byte is unambiguous. Deliberately larger than the 4/5-byte MessagePack fixtures
/// the CBOR adapter records as Opaque, so the shared small-int/short-container
/// prefix overlap is never claimed.
pub const MIN_DETECT_BYTES: u64 = 8;

/// MessagePack node kind: a non-negative integer (`positive fixint` or a positive
/// `int8/16/32/64` value, or `uint8/16/32/64`).
pub const K_UINT: u8 = 0;
/// MessagePack node kind: a negative integer (`negative fixint`, or a negative
/// `int8/16/32/64` value; the recorded value is `-1 - n`).
pub const K_NEGINT: u8 = 1;
/// MessagePack node kind: a byte string (`bin8/16/32`).
pub const K_BIN: u8 = 2;
/// MessagePack node kind: a text string (`fixstr`/`str8/16/32`; the payload is
/// well-formed UTF-8).
pub const K_STR: u8 = 3;
/// MessagePack node kind: an array (`fixarray`/`array16`/`array32`).
pub const K_ARRAY: u8 = 4;
/// MessagePack node kind: a map (`fixmap`/`map16`/`map32`).
pub const K_MAP: u8 = 5;
/// MessagePack node kind: an extension (`ext8/16/32`, `fixext1/2/4/8/16`). Its type
/// number and payload length are preserved, never interpreted.
pub const K_EXT: u8 = 6;
/// MessagePack node kind: `false` (`0xc2`).
pub const K_FALSE: u8 = 7;
/// MessagePack node kind: `true` (`0xc3`).
pub const K_TRUE: u8 = 8;
/// MessagePack node kind: `nil` (`0xc0`).
pub const K_NIL: u8 = 9;
/// MessagePack node kind: an IEEE-754 float (`float32` `0xca`, `float64` `0xcb`).
pub const K_FLOAT: u8 = 10;

/// The highest valid `K_*` kind tag (used by the decoder's range check).
const K_MAX: u8 = K_FLOAT;

/// The MessagePack-only root head bytes (`map16`/`map32`/`array16`/`array32`,
/// `0xdc..=0xdf`) whose byte value CBOR's own grammar rejects (reserved
/// additional-information nibbles `28..=30`, or an indefinite-length tag `31`).
pub const UNAMBIGUOUS_HEADS: [u8; 4] = [0xdc, 0xdd, 0xde, 0xdf];

/// Whether `head` is an unambiguous MessagePack-only format byte (see
/// [`UNAMBIGUOUS_HEADS`]).
pub const fn is_unambiguous_head(head: u8) -> bool {
    matches!(head, 0xdc..=0xdf)
}

/// Stable lower-case kind name for reports and JSON output.
pub const fn kind_name(kind: u8) -> &'static str {
    match kind {
        K_UINT => "uint",
        K_NEGINT => "negint",
        K_BIN => "bin",
        K_STR => "str",
        K_ARRAY => "array",
        K_MAP => "map",
        K_EXT => "ext",
        K_FALSE => "false",
        K_TRUE => "true",
        K_NIL => "nil",
        K_FLOAT => "float",
        _ => "unknown",
    }
}

/// Whether `kind` is a container (has children and can be index-addressed).
pub const fn is_container(kind: u8) -> bool {
    matches!(kind, K_ARRAY | K_MAP)
}

/// Stable name for the IEEE-754 width selected by a float's format byte.
pub const fn float_width_name(head: u8) -> &'static str {
    match head {
        0xca => "single",
        0xcb => "double",
        _ => "unknown",
    }
}

/// One parsed MessagePack item: its kind, exact source span, ordered children, and
/// the representation facts MessagePack adds over JSON.
///
/// The map `children` list is the interleaved `[key0, value0, key1, value1, …]`
/// document-order sequence (order and duplicate keys preserved verbatim); an
/// array's `children` is its element list; every scalar has an empty `children`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsgpackNode {
    /// One of the `K_*` tags.
    pub kind: u8,
    /// The exact format byte that introduced this item (`0x00..=0xff`), recorded
    /// verbatim so the encoding width **and signedness** actually used are preserved
    /// (e.g. `0x17` is `positive fixint` 23, `0xcc 0x17` is `uint8` 23, `0xd0 0x17`
    /// is `int8` 23).
    pub head: u8,
    /// For [`K_EXT`], the raw type-number byte (a signed 8-bit integer); `0`
    /// otherwise. The type is preserved verbatim and **never** interpreted.
    pub ext_type: u8,
    /// The decoded unsigned argument: the value for [`K_UINT`], the magnitude `n`
    /// (`value = -1 - n`) for [`K_NEGINT`], the byte length for a [`K_STR`]/[`K_BIN`],
    /// the element/pair count for an [`K_ARRAY`]/[`K_MAP`], the payload length for a
    /// [`K_EXT`], the raw IEEE-754 bit pattern for a [`K_FLOAT`], and `0` for
    /// [`K_NIL`]/[`K_FALSE`]/[`K_TRUE`].
    pub arg: u64,
    /// The token's first source byte.
    pub start: u64,
    /// One past the token's last source byte.
    pub end: u64,
    /// Map: interleaved key/value indices; array: element indices.
    pub children: Vec<u32>,
}

/// The canonical derived MessagePack model (the materialization of a
/// `MsgpackModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsgpackModel {
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// Index of the root node.
    pub root: u32,
    /// The root's kind tag.
    pub top_type: u8,
    /// The root's exact format byte.
    pub root_head: u8,
    /// The observed maximum nesting depth (root = 1).
    pub max_depth: u32,
    /// The total number of items observed (equals `nodes.len()` for a built model;
    /// also correct for a detection-only parse, where the arena is not populated).
    pub items: u32,
    /// The node arena.
    pub nodes: Vec<MsgpackNode>,
}

impl MsgpackModel {
    /// The node at `index`, if present.
    pub fn node(&self, index: u32) -> Option<&MsgpackNode> {
        self.nodes.get(index as usize)
    }

    /// The total number of items observed.
    pub fn item_count(&self) -> u32 {
        self.items
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(48 + self.nodes.len() * 40);
        out.extend_from_slice(b"MPKM");
        out.push(MODEL_VERSION);
        out.push(self.top_type);
        out.push(self.root_head);
        out.extend_from_slice(&self.root.to_le_bytes());
        out.extend_from_slice(&self.max_depth.to_le_bytes());
        out.extend_from_slice(&self.items.to_le_bytes());
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&(self.nodes.len() as u32).to_le_bytes());
        for n in &self.nodes {
            out.push(n.kind);
            out.push(n.head);
            out.push(n.ext_type);
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
    pub fn decode(bytes: &[u8]) -> Result<MsgpackModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"MPKM" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let top_type = r.u8()?;
        if top_type > K_MAX {
            return Err(corrupt("unknown top-level kind"));
        }
        let root_head = r.u8()?;
        let root = r.u32()?;
        let max_depth = r.u32()?;
        let items = r.u32()?;
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
            let head = r.u8()?;
            let ext_type = r.u8()?;
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
            nodes.push(MsgpackNode {
                kind,
                head,
                ext_type,
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
        Ok(MsgpackModel {
            doc_len,
            root,
            top_type,
            root_head,
            max_depth,
            items,
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
    /// index or the root; `>1` witnesses duplicate keys).
    pub matches: u32,
}

/// One lexical-match record from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsgpackMatch {
    /// Canonical RFC 6901 pointer to the matching node.
    pub pointer: String,
    /// Whether the match is a map key or a text value.
    pub role: crate::adapter::json::MatchRole,
    /// The matching node's kind (always [`K_STR`]).
    pub kind: u8,
    /// The exact source span of the matching text token (delimiters included).
    pub start: u64,
    /// One past the matching text token.
    pub end: u64,
    /// The decoded UTF-8 text of the matching token.
    pub text: String,
}

/// Fast-fail MessagePack detector (see the module docs for the exact boundary).
///
/// True iff `source` is within the caps and parses, in full, as exactly one
/// MessagePack item whose root is a container (array/map) reaching at least
/// [`MIN_DETECT_NODES`] items, and either its root head byte is an unambiguous
/// MessagePack-only head ([`is_unambiguous_head`]) or the source reaches at least
/// [`MIN_DETECT_BYTES`] bytes. Every other input stays `Opaque`.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_msgpack_document_bytes {
        return false;
    }
    let m = match parse(source, limits, false) {
        Ok(m) => m,
        Err(_) => return false,
    };
    if !is_container(m.top_type) {
        return false;
    }
    if m.item_count() < MIN_DETECT_NODES {
        return false;
    }
    // A `map16`/`map32`/`array16`/`array32` head byte is rejected by CBOR's grammar,
    // so it is an unambiguous MessagePack signal and needs no length gate.
    if is_unambiguous_head(m.root_head) {
        return true;
    }
    // Otherwise require the source to be structurally non-trivial: the small
    // whole-number/short-container prefix overlap with CBOR is genuinely ambiguous.
    (source.len() as u64) >= MIN_DETECT_BYTES
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of a `MsgpackModel` node).
pub fn build_msgpack_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits, true)?.encode())
}

/// Parse `source` into a [`MsgpackModel`]. `build` selects whether the node arena is
/// populated (detection runs with `build = false` to stay bounded in memory).
pub fn parse(source: &[u8], limits: Limits, build: bool) -> Result<MsgpackModel> {
    if source.len() as u64 > limits.max_msgpack_document_bytes {
        return Err(Error::resource_limit(format!(
            "MessagePack source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_msgpack_document_bytes
        )));
    }
    if source.is_empty() {
        return Err(corrupt("empty input is not a MessagePack item"));
    }
    let mut p = Parser::new(source, limits, build);
    let root = p.item(1)?;
    if p.at != source.len() {
        return Err(corrupt("trailing bytes after the single MessagePack item"));
    }
    Ok(MsgpackModel {
        doc_len: source.len() as u64,
        root,
        top_type: p.root_kind,
        root_head: p.root_head,
        max_depth: p.max_depth,
        items: p.count,
        nodes: p.nodes,
    })
}

/// The exact source bytes of a node's token (`[start, end)`), bounded by the source.
pub fn token_bytes<'a>(source: &'a [u8], node: &MsgpackNode) -> Result<&'a [u8]> {
    let start = usize::try_from(node.start)
        .map_err(|_| Error::internal_invariant("MessagePack token span overflows usize"))?;
    let end = usize::try_from(node.end)
        .map_err(|_| Error::internal_invariant("MessagePack token span overflows usize"))?;
    source
        .get(start..end)
        .ok_or_else(|| Error::internal_invariant("MessagePack token span is outside the source"))
}

/// The exact payload bytes of a `str`, `bin`, or `ext` node (the bytes between the
/// head/type and the item's end). Declines typed for any other node.
pub fn payload_bytes<'a>(source: &'a [u8], node: &MsgpackNode) -> Result<&'a [u8]> {
    if node.kind != K_STR && node.kind != K_BIN && node.kind != K_EXT {
        return Err(corrupt("node is not a string, binary, or extension"));
    }
    let len = usize::try_from(node.arg)
        .map_err(|_| Error::internal_invariant("MessagePack string length overflows usize"))?;
    let end = usize::try_from(node.end)
        .map_err(|_| Error::internal_invariant("MessagePack string span overflows usize"))?;
    let start = end
        .checked_sub(len)
        .ok_or_else(|| Error::internal_invariant("MessagePack string length exceeds its token"))?;
    source.get(start..end).ok_or_else(|| {
        Error::internal_invariant("MessagePack string payload is outside the source")
    })
}

/// Decode a `str` node's payload into its UTF-8 text. Declines typed for a byte
/// string, an extension, or a non-string node.
pub fn decode_text(source: &[u8], node: &MsgpackNode) -> Result<String> {
    if node.kind != K_STR {
        return Err(corrupt("node is not a text string"));
    }
    let bytes = payload_bytes(source, node)?;
    String::from_utf8(bytes.to_vec()).map_err(|_| corrupt("text string is not valid UTF-8"))
}

/// Render a deterministic canonical text projection of the whole model.
///
/// Binary payloads are rendered as `h'HEX'` (byte strings); an extension as
/// `ext(TYPE,0xHEX)` (type number and payload, never interpreted); text strings as a
/// JSON-quoted string; floats as `f32:0x…`/`f64:0x…` (the raw IEEE-754 bits, so the
/// width is visible).
pub fn canonical_text(model: &MsgpackModel, source: &[u8]) -> Result<String> {
    subtree_text(model, source, model.root)
}

/// Render one node's subtree to canonical text (see [`canonical_text`]).
pub fn subtree_text(model: &MsgpackModel, source: &[u8], index: u32) -> Result<String> {
    let mut out = String::new();
    render(model, source, index, &mut out, 0)?;
    Ok(out)
}

/// The index of a node's parent, if any (the root has none).
pub fn find_parent(model: &MsgpackModel, index: u32) -> Option<u32> {
    model
        .nodes
        .iter()
        .position(|n| n.children.contains(&index))
        .map(|i| i as u32)
}

fn render(
    model: &MsgpackModel,
    source: &[u8],
    index: u32,
    out: &mut String,
    depth: u32,
) -> Result<()> {
    if depth > MAX_MODEL_NODES {
        return Err(Error::resource_limit(
            "MessagePack render exceeded its depth bound",
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
        K_STR => {
            out.push('"');
            push_json_string(payload_bytes(source, node)?, out);
            out.push('"');
        }
        K_BIN => {
            out.push_str("h'");
            out.push_str(&hex(payload_bytes(source, node)?));
            out.push('\'');
        }
        K_EXT => {
            out.push_str("ext(");
            out.push_str(&(node.ext_type as i8).to_string());
            out.push_str(",0x");
            out.push_str(&hex(payload_bytes(source, node)?));
            out.push(')');
        }
        K_UINT => out.push_str(&node.arg.to_string()),
        K_NEGINT => {
            let v = -1i128 - i128::from(node.arg);
            out.push_str(&v.to_string());
        }
        K_FALSE => out.push_str("false"),
        K_TRUE => out.push_str("true"),
        K_NIL => out.push_str("nil"),
        K_FLOAT => {
            let prefix = match node.head {
                0xca => "f32",
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
    model: &MsgpackModel,
    source: &[u8],
    pattern: &str,
    limits: Limits,
) -> Result<Vec<MsgpackMatch>> {
    let mut out = Vec::new();
    let mut path: Vec<String> = Vec::new();
    walk_find(
        model, source, model.root, true, &mut path, pattern, &mut out, limits, 0,
    )?;
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn walk_find(
    model: &MsgpackModel,
    source: &[u8],
    index: u32,
    is_value: bool,
    path: &mut Vec<String>,
    pattern: &str,
    out: &mut Vec<MsgpackMatch>,
    limits: Limits,
    depth: u32,
) -> Result<()> {
    if depth > limits.max_msgpack_depth {
        return Err(Error::resource_limit(
            "MessagePack find exceeded the nesting-depth cap",
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
                if key_node.kind == K_STR {
                    let key = decode_text(source, key_node)?;
                    if key.contains(pattern) {
                        out.push(MsgpackMatch {
                            pointer: pointer_of(path, Some(&key)),
                            role: crate::adapter::json::MatchRole::Key,
                            kind: K_STR,
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
        K_STR if is_value => {
            let text = decode_text(source, node)?;
            if text.contains(pattern) {
                out.push(MsgpackMatch {
                    pointer: pointer_of(path, None),
                    role: crate::adapter::json::MatchRole::Value,
                    kind: K_STR,
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
/// to the segment (duplicate matches are reported). A missing member, an
/// out-of-range index, or indexing into a scalar is a typed decline (never a silent
/// empty answer).
pub fn resolve_pointer(model: &MsgpackModel, source: &[u8], pointer: &str) -> Result<Resolved> {
    let segments = parse_pointer(pointer)?;
    let mut index = model.root;
    let mut matches = 1u32;
    for seg in &segments {
        let node = model
            .node(index)
            .ok_or_else(|| corrupt("pointer traversal left the model"))?;
        match node.kind {
            K_MAP => {
                let (found, first) = map_lookup(model, source, index, seg)?;
                index = first.ok_or_else(|| {
                    Error::unsupported_feature(format!(
                        "MessagePack map has no member {seg:?} at this pointer"
                    ))
                })?;
                matches = found;
            }
            K_ARRAY => {
                let idx = parse_array_index(seg)?;
                let len = node.children.len();
                if idx as usize >= len {
                    return Err(Error::unsupported_feature(format!(
                        "MessagePack array index {idx} is out of range (length {len})"
                    )));
                }
                index = node.children[idx as usize];
                matches = 1;
            }
            _ => {
                return Err(Error::unsupported_feature(format!(
                    "cannot index into a MessagePack {} at {seg:?}",
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
    model: &MsgpackModel,
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
        if key_node.kind == K_STR && decode_text(source, key_node).is_ok_and(|k| k == seg) {
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
            "MessagePack pointer {pointer:?} must be empty or start with '/'"
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
                        "invalid MessagePack pointer escape in segment {raw:?}"
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
            "MessagePack array segment {seg:?} is not a canonical index"
        )));
    }
    seg.parse::<u32>()
        .map_err(|_| Error::usage(format!("MessagePack array index {seg:?} is not a u32")))
}

// ---------------------------------------------------------------------------
// The bounded parser
// ---------------------------------------------------------------------------

struct Parser<'a> {
    b: &'a [u8],
    at: usize,
    limits: Limits,
    build: bool,
    nodes: Vec<MsgpackNode>,
    count: u32,
    str_bytes: u64,
    bin_bytes: u64,
    ext_bytes: u64,
    max_depth: u32,
    root_kind: u8,
    root_head: u8,
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
            str_bytes: 0,
            bin_bytes: 0,
            ext_bytes: 0,
            max_depth: 0,
            root_kind: K_NIL,
            root_head: 0,
        }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| corrupt("MessagePack read length overflows"))?;
        if end > self.b.len() {
            return Err(corrupt("MessagePack item is truncated"));
        }
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }

    fn take_len(&mut self, len: u64) -> Result<&'a [u8]> {
        let n = usize::try_from(len)
            .map_err(|_| corrupt("MessagePack length exceeds the addressable range"))?;
        self.take(n)
    }

    fn read_u32_len(&mut self) -> Result<u64> {
        let s = self.take(4)?;
        Ok(u64::from(u32::from_be_bytes([s[0], s[1], s[2], s[3]])))
    }

    fn read_u32_raw(&mut self) -> Result<u32> {
        let s = self.take(4)?;
        Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    }

    fn read_u64_raw(&mut self) -> Result<u64> {
        let s = self.take(8)?;
        Ok(u64::from_be_bytes([
            s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
        ]))
    }

    fn emit(&mut self, node: MsgpackNode) -> Result<u32> {
        self.count = self.count.saturating_add(1);
        if self.count > self.limits.max_msgpack_nodes {
            return Err(Error::resource_limit(format!(
                "MessagePack item count exceeds the {}-node cap",
                self.limits.max_msgpack_nodes
            )));
        }
        if self.build {
            self.nodes.push(node);
        }
        Ok(self.count - 1)
    }

    fn emit_leaf(&mut self, kind: u8, head: u8, arg: u64, start: u64) -> Result<u32> {
        self.emit(MsgpackNode {
            kind,
            head,
            ext_type: 0,
            arg,
            start,
            end: self.at as u64,
            children: Vec::new(),
        })
    }

    /// Decode a signed integer, classifying it by **value** (so a positive value
    /// encoded signed is still a [`K_UINT`]); the exact `head` preserves the width
    /// and signedness actually used.
    fn int_item(&mut self, head: u8, value: i64, start: u64) -> Result<(u32, u8)> {
        if value >= 0 {
            let idx = self.emit_leaf(K_UINT, head, value as u64, start)?;
            Ok((idx, K_UINT))
        } else {
            let arg = value.unsigned_abs() - 1;
            let idx = self.emit_leaf(K_NEGINT, head, arg, start)?;
            Ok((idx, K_NEGINT))
        }
    }

    fn container(&mut self, kind: u8, head: u8, count: u64, start: u64, depth: u32) -> Result<u32> {
        let mut children: Vec<u32> = Vec::new();
        if kind == K_MAP {
            let mut i = 0u64;
            while i < count {
                let key = self.item(depth + 1)?;
                let val = self.item(depth + 1)?;
                if self.build {
                    children.push(key);
                    children.push(val);
                }
                i += 1;
            }
        } else {
            let mut i = 0u64;
            while i < count {
                let child = self.item(depth + 1)?;
                if self.build {
                    children.push(child);
                }
                i += 1;
            }
        }
        let end = self.at as u64;
        self.emit(MsgpackNode {
            kind,
            head,
            ext_type: 0,
            arg: count,
            start,
            end,
            children,
        })
    }

    fn str_item(&mut self, head: u8, len: u64, start: u64) -> Result<u32> {
        self.str_bytes = self.str_bytes.saturating_add(len);
        if self.str_bytes > self.limits.max_msgpack_str_bytes {
            return Err(Error::resource_limit(format!(
                "MessagePack string bytes exceed the {}-byte cap",
                self.limits.max_msgpack_str_bytes
            )));
        }
        let bytes = self.take_len(len)?;
        if std::str::from_utf8(bytes).is_err() {
            return Err(corrupt("text string is not valid UTF-8"));
        }
        self.emit_leaf(K_STR, head, len, start)
    }

    fn bin_item(&mut self, head: u8, len: u64, start: u64) -> Result<u32> {
        self.bin_bytes = self.bin_bytes.saturating_add(len);
        if self.bin_bytes > self.limits.max_msgpack_bin_bytes {
            return Err(Error::resource_limit(format!(
                "MessagePack binary bytes exceed the {}-byte cap",
                self.limits.max_msgpack_bin_bytes
            )));
        }
        let _ = self.take_len(len)?;
        self.emit_leaf(K_BIN, head, len, start)
    }

    fn ext_item(&mut self, head: u8, len: u64, start: u64) -> Result<u32> {
        self.ext_bytes = self.ext_bytes.saturating_add(len);
        if self.ext_bytes > self.limits.max_msgpack_ext_bytes {
            return Err(Error::resource_limit(format!(
                "MessagePack extension bytes exceed the {}-byte cap",
                self.limits.max_msgpack_ext_bytes
            )));
        }
        let ext_type = self.take(1)?[0];
        let _ = self.take_len(len)?;
        self.emit(MsgpackNode {
            kind: K_EXT,
            head,
            ext_type,
            arg: len,
            start,
            end: self.at as u64,
            children: Vec::new(),
        })
    }

    fn item(&mut self, depth: u32) -> Result<u32> {
        if depth > self.limits.max_msgpack_depth {
            return Err(Error::resource_limit(format!(
                "MessagePack nesting depth exceeds the {}-level cap",
                self.limits.max_msgpack_depth
            )));
        }
        if depth > self.max_depth {
            self.max_depth = depth;
        }
        let head = *self
            .b
            .get(self.at)
            .ok_or_else(|| corrupt("MessagePack item is truncated"))?;
        self.at += 1;
        let start = (self.at - 1) as u64;
        let (index, kind) = match head {
            0x00..=0x7f => (
                self.emit_leaf(K_UINT, head, u64::from(head), start)?,
                K_UINT,
            ),
            0x80..=0x8f => (
                self.container(K_MAP, head, u64::from(head & 0x0f), start, depth)?,
                K_MAP,
            ),
            0x90..=0x9f => (
                self.container(K_ARRAY, head, u64::from(head & 0x0f), start, depth)?,
                K_ARRAY,
            ),
            0xa0..=0xbf => (self.str_item(head, u64::from(head & 0x1f), start)?, K_STR),
            0xc0 => (self.emit_leaf(K_NIL, head, 0, start)?, K_NIL),
            0xc1 => return Err(corrupt("0xc1 is never a valid MessagePack head byte")),
            0xc2 => (self.emit_leaf(K_FALSE, head, 0, start)?, K_FALSE),
            0xc3 => (self.emit_leaf(K_TRUE, head, 0, start)?, K_TRUE),
            0xc4 => {
                let n = u64::from(self.take(1)?[0]);
                (self.bin_item(head, n, start)?, K_BIN)
            }
            0xc5 => {
                let s = self.take(2)?;
                let n = u64::from(u16::from_be_bytes([s[0], s[1]]));
                (self.bin_item(head, n, start)?, K_BIN)
            }
            0xc6 => {
                let n = self.read_u32_len()?;
                (self.bin_item(head, n, start)?, K_BIN)
            }
            0xc7 => {
                let n = u64::from(self.take(1)?[0]);
                (self.ext_item(head, n, start)?, K_EXT)
            }
            0xc8 => {
                let s = self.take(2)?;
                let n = u64::from(u16::from_be_bytes([s[0], s[1]]));
                (self.ext_item(head, n, start)?, K_EXT)
            }
            0xc9 => {
                let n = self.read_u32_len()?;
                (self.ext_item(head, n, start)?, K_EXT)
            }
            0xca => {
                let bits = self.read_u32_raw()?;
                (
                    self.emit_leaf(K_FLOAT, head, u64::from(bits), start)?,
                    K_FLOAT,
                )
            }
            0xcb => {
                let bits = self.read_u64_raw()?;
                (self.emit_leaf(K_FLOAT, head, bits, start)?, K_FLOAT)
            }
            0xcc => {
                let v = u64::from(self.take(1)?[0]);
                (self.emit_leaf(K_UINT, head, v, start)?, K_UINT)
            }
            0xcd => {
                let s = self.take(2)?;
                let v = u64::from(u16::from_be_bytes([s[0], s[1]]));
                (self.emit_leaf(K_UINT, head, v, start)?, K_UINT)
            }
            0xce => {
                let v = u64::from(self.read_u32_raw()?);
                (self.emit_leaf(K_UINT, head, v, start)?, K_UINT)
            }
            0xcf => {
                let v = self.read_u64_raw()?;
                (self.emit_leaf(K_UINT, head, v, start)?, K_UINT)
            }
            0xd0 => {
                let v = i64::from(self.take(1)?[0] as i8);
                self.int_item(head, v, start)?
            }
            0xd1 => {
                let s = self.take(2)?;
                let v = i64::from(i16::from_be_bytes([s[0], s[1]]));
                self.int_item(head, v, start)?
            }
            0xd2 => {
                let s = self.take(4)?;
                let v = i64::from(i32::from_be_bytes([s[0], s[1], s[2], s[3]]));
                self.int_item(head, v, start)?
            }
            0xd3 => {
                let s = self.take(8)?;
                let v = i64::from_be_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]);
                self.int_item(head, v, start)?
            }
            0xd4 => (self.ext_item(head, 1, start)?, K_EXT),
            0xd5 => (self.ext_item(head, 2, start)?, K_EXT),
            0xd6 => (self.ext_item(head, 4, start)?, K_EXT),
            0xd7 => (self.ext_item(head, 8, start)?, K_EXT),
            0xd8 => (self.ext_item(head, 16, start)?, K_EXT),
            0xd9 => {
                let n = u64::from(self.take(1)?[0]);
                (self.str_item(head, n, start)?, K_STR)
            }
            0xda => {
                let s = self.take(2)?;
                let n = u64::from(u16::from_be_bytes([s[0], s[1]]));
                (self.str_item(head, n, start)?, K_STR)
            }
            0xdb => {
                let n = self.read_u32_len()?;
                (self.str_item(head, n, start)?, K_STR)
            }
            0xdc => {
                let s = self.take(2)?;
                let n = u64::from(u16::from_be_bytes([s[0], s[1]]));
                (self.container(K_ARRAY, head, n, start, depth)?, K_ARRAY)
            }
            0xdd => {
                let n = self.read_u32_len()?;
                (self.container(K_ARRAY, head, n, start, depth)?, K_ARRAY)
            }
            0xde => {
                let s = self.take(2)?;
                let n = u64::from(u16::from_be_bytes([s[0], s[1]]));
                (self.container(K_MAP, head, n, start, depth)?, K_MAP)
            }
            0xdf => {
                let n = self.read_u32_len()?;
                (self.container(K_MAP, head, n, start, depth)?, K_MAP)
            }
            0xe0..=0xff => {
                let value = i64::from(head as i8);
                let arg = value.unsigned_abs() - 1;
                (self.emit_leaf(K_NEGINT, head, arg, start)?, K_NEGINT)
            }
        };
        if depth == 1 {
            self.root_kind = kind;
            self.root_head = head;
        }
        Ok(index)
    }
}

fn corrupt(msg: impl Into<String>) -> Error {
    Error::invalid_msgpack_structure(msg)
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
        // {"a": [1, 2, 3], "b": true}
        let doc = [0x82, 0xa1, b'a', 0x93, 0x01, 0x02, 0x03, 0xa1, b'b', 0xc3];
        assert!(detect(&doc, Limits::DEFAULT));
        // A single scalar stays Opaque (too small / trivial).
        assert!(!detect(&[0x17], Limits::DEFAULT));
        assert!(!detect(&[0xcc, 0x17], Limits::DEFAULT));
        // An empty container stays Opaque (structurally trivial).
        assert!(!detect(&[0x90], Limits::DEFAULT));
        // The ambiguous MessagePack fixarray(3)/fixmap(2) overlap fixtures stay
        // Opaque (they are below the byte threshold).
        assert!(!detect(&[0x93, 0x01, 0x02, 0x03], Limits::DEFAULT));
        assert!(!detect(&[0x82, 0x01, 0x02, 0x03, 0x04], Limits::DEFAULT));
        // Prose (never a container head byte) stays Opaque.
        assert!(!detect(
            b"plain prose, definitely not msgpack\n",
            Limits::DEFAULT
        ));
    }

    #[test]
    fn preserves_width_order_duplicates_and_kinds() {
        // [23 (0x17), 23 (0xcc 0x17), h'010203', "abc"]
        let doc = [
            0x94, 0x17, 0xcc, 0x17, 0xc4, 0x03, 0x01, 0x02, 0x03, 0xa3, b'a', b'b', b'c',
        ];
        let m = parse(&doc, Limits::DEFAULT, true).unwrap();
        assert_eq!(m.top_type, K_ARRAY);
        assert_eq!(m.nodes[m.root as usize].children.len(), 4);
        let c0 = &m.nodes[m.nodes[m.root as usize].children[0] as usize];
        assert_eq!(c0.kind, K_UINT);
        assert_eq!(c0.head, 0x17);
        assert_eq!(c0.arg, 23);
        let c1 = &m.nodes[m.nodes[m.root as usize].children[1] as usize];
        assert_eq!(c1.kind, K_UINT);
        assert_eq!(c1.head, 0xcc);
        assert_eq!(c1.arg, 23);
        assert_ne!(c0.head, c1.head);
        let c2 = &m.nodes[m.nodes[m.root as usize].children[2] as usize];
        assert_eq!(c2.kind, K_BIN);
        assert_eq!(payload_bytes(&doc, c2).unwrap(), &[0x01, 0x02, 0x03]);
        let c3 = &m.nodes[m.nodes[m.root as usize].children[3] as usize];
        assert_eq!(c3.kind, K_STR);
        assert_eq!(decode_text(&doc, c3).unwrap(), "abc");
    }

    #[test]
    fn preserves_signedness_floats_ext_and_map_order() {
        // [int8 127 (positive), int8 -1, f32 1.0, f64 1.0, fixext1(-1, 0x2a)]
        let doc = [
            0x95, 0xd0, 0x7f, 0xd0, 0xff, 0xca, 0x3f, 0x80, 0x00, 0x00, 0xcb, 0x3f, 0xf0, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0xd4, 0xff, 0x2a,
        ];
        let m = parse(&doc, Limits::DEFAULT, true).unwrap();
        assert_eq!(m.top_type, K_ARRAY);
        let kids: Vec<&MsgpackNode> = m.nodes[m.root as usize]
            .children
            .iter()
            .map(|c| &m.nodes[*c as usize])
            .collect();
        // A positive value encoded signed is a K_UINT but keeps the signed head.
        assert_eq!(kids[0].kind, K_UINT);
        assert_eq!(kids[0].head, 0xd0);
        assert_eq!(kids[0].arg, 127);
        assert_eq!(kids[1].kind, K_NEGINT);
        assert_eq!(kids[1].arg, 0);
        assert_eq!(kids[2].kind, K_FLOAT);
        assert_eq!(kids[2].head, 0xca);
        assert_eq!(kids[3].kind, K_FLOAT);
        assert_eq!(kids[3].head, 0xcb);
        assert_eq!(kids[4].kind, K_EXT);
        assert_eq!(kids[4].head, 0xd4);
        assert_eq!(kids[4].ext_type, 0xff);
        assert_eq!(kids[4].arg, 1);
        assert_eq!(payload_bytes(&doc, kids[4]).unwrap(), &[0x2a]);
    }

    #[test]
    fn declines_trailing_c1_map_and_length_faults() {
        for bad in [
            &[0x01, 0x02][..],       // trailing bytes
            &[0xc1][..],             // the never-used 0xc1 byte
            &[0x81, 0xa1, b'a'][..], // map key with no value
            &[0x93, 0x01][..],       // truncated array
            &[0xc4, 0x10, 0x00][..], // over-long declared bin length
            &[0xa3, b'a', b'b'][..], // truncated fixstr
            &[0xcc][..],             // truncated uint8
        ] {
            let e = parse(bad, Limits::DEFAULT, true).unwrap_err();
            assert_eq!(
                e.class(),
                crate::error::ErrorClass::InvalidMsgpackStructure,
                "{bad:?}"
            );
        }
    }

    #[test]
    fn model_roundtrips() {
        let doc = [0x82, 0xa1, b'a', 0x93, 0x01, 0x02, 0x03, 0xa1, b'b', 0xc3];
        let m = parse(&doc, Limits::DEFAULT, true).unwrap();
        let encoded = m.encode();
        let round = MsgpackModel::decode(&encoded).unwrap();
        assert_eq!(m, round);
        assert!(canonical_text(&m, &doc).unwrap().contains("true"));
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0x4D50_0B5E_ED12_34AB;
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
                let _ = build_msgpack_model(&buf, Limits::STRICT);
            }
        }
    }
}
