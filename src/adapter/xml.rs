//! Bounded, representation-preserving XML adapter (Phase 21.9).
//!
//! XML is the next **structured-tree** Wave-2 format after JSON/YAML/CSV/Markdown.
//! Like them it is *not* a package: there is no OPC/ZIP layer, no `mimetype`, and
//! no relationship graph. The exact leaf is therefore the **whole source** (a
//! `DocumentExact`, a RAW-like authority), and everything this module produces is
//! a bounded, deterministic (`Q_gen`) projection that never sits on the exactness
//! path.
//!
//! ## Why a bespoke, span-preserving tree
//!
//! A conventional XML load (`ElementTree`, a DOM, etc.) keeps *values* and drops
//! the *representation*: it discards comments, processing instructions, CDATA
//! boundaries, namespace-declaration spelling, attribute quoting, and every
//! source offset, and it expands entity references. This adapter does the
//! opposite. For every construct it records the exact **byte span** in the source,
//! so it preserves and can report:
//!
//! * every **element** (its qualified name span, start-tag span, end-tag span, and
//!   full span), in document order;
//! * every **attribute** (its name span, quoted value span, inner value span, and
//!   full span), in source order;
//! * every **text** run, **CDATA** section, **comment**, **processing
//!   instruction**, and **DOCTYPE**;
//! * every **namespace declaration** (`xmlns`/`xmlns:prefix`) as an ordinary
//!   attribute with its exact span, so the in-scope bindings are recoverable;
//! * **entity references are never expanded** — the raw `&name;`/`&#…;` bytes
//!   remain in the reported text/content spans, surfaced literally;
//! * **no DTD internal subset** is ever processed, so neither external entities
//!   (XXE) nor entity expansion (billion-laughs) can occur. A benign `<!DOCTYPE…>`
//!   (bare or with a PUBLIC/SYSTEM identifier) is accepted and ignored, exactly as
//!   in the shared [bounded XML policy](crate::adapter::xml_policy); a declaration
//!   with an internal subset (`[…]`) is **declined typed** (the whole document is
//!   then not detected as XML and stays `Opaque`).
//!
//! ## Bounds
//!
//! The parser is a bounded single-pass scanner. It reuses the shared
//! [`crate::adapter::xml_policy::XmlState`] budget for events, element nodes,
//! nesting depth, and accumulated text bytes, and additionally charges the parser
//! against the source length by [`Limits::max_xml_document_bytes`]. Recursion is
//! capped by [`Limits::max_xml_depth`]; the model's node arena is additionally
//! bounded by [`MAX_MODEL_NODES`] at decode. Untrusted input can only ever yield a
//! typed decline, never a panic or an unbounded allocation. There is no `unwrap`,
//! `expect`, `panic!`, or indexing that is not span-checked.
//!
//! ## Detection
//!
//! [`detect`] is deliberately conservative: the source must begin with `<` (an XML
//! prolog `<?xml` or a root element / comment / DOCTYPE start) **and** the whole
//! source must parse as well-formed XML with exactly one root element, within every
//! cap. A `<-prefixed` byte string that is not well-formed XML (and plain prose)
//! stays [`DocumentFormat::Opaque`](crate::field::document_format::DocumentFormat::Opaque).
//!
//! ## Explicitly declined
//!
//! This adapter does **not** support: typed what it does not: DTD internal subsets
//! (entities), external entity resolution, character-encoding switching beyond
//! UTF-8, XInclude, XSLT, schema validation, XPath (beyond the simple
//! `name[N]` element-path selector this module implements), or XML canonicalization
//! (C14N). Each is either a typed decline or simply out of scope; none is guessed.

use crate::adapter::xml_policy::{XmlState, accept_doctype, doctype_declined, harden_xml};
use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model nodes (defends the decoder against a hostile blob).
pub const MAX_MODEL_NODES: u32 = 1 << 24;
/// Hard cap on decoded model attributes (defends the decoder against a hostile blob).
pub const MAX_MODEL_ATTRS: u32 = 1 << 26;

/// XML node kind: an element (`<name …>…</name>` or `<name …/>`).
pub const K_ELEMENT: u8 = 0;
/// XML node kind: a character-data run.
pub const K_TEXT: u8 = 1;
/// XML node kind: a CDATA section (`<![CDATA[…]]>`).
pub const K_CDATA: u8 = 2;
/// XML node kind: a comment (`<!--…-->`).
pub const K_COMMENT: u8 = 3;
/// XML node kind: a processing instruction or the XML declaration (`<?…?>`).
pub const K_PI: u8 = 4;
/// XML node kind: a document type declaration (`<!DOCTYPE…>`).
pub const K_DOCTYPE: u8 = 5;

/// Stable lower-case kind name for reports and JSON output.
pub const fn kind_name(kind: u8) -> &'static str {
    match kind {
        K_ELEMENT => "element",
        K_TEXT => "text",
        K_CDATA => "cdata",
        K_COMMENT => "comment",
        K_PI => "pi",
        K_DOCTYPE => "doctype",
        _ => "unknown",
    }
}

/// Whether `kind` is character data (text or CDATA).
pub const fn is_chardata(kind: u8) -> bool {
    matches!(kind, K_TEXT | K_CDATA)
}

/// One parsed XML node.
///
/// * For an **element**, `[start, end)` is the whole element (`<a…>…</a>` or the
///   self-closing `<a…/>`); `[name_start, name_end)` is the qualified name;
///   `open_end` is one past the start tag's `>`; `close_start` is the `<` of the
///   end tag (`0` for a self-closing element, where `end == open_end`); `children`
///   are the child node indices in document order; `attrs` index [`XAttr`]s.
/// * For **text/cdata/comment/pi/doctype**, `[start, end)` is the whole construct
///   (delimiters included) and `[name_start, name_end)` is the *content* span
///   (for a PI, the target name; the remainder is derivable from `name_end..end`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XNode {
    /// One of the `K_*` tags.
    pub kind: u8,
    /// The construct's first source byte.
    pub start: u64,
    /// One past the construct's last source byte.
    pub end: u64,
    /// Element: qualified-name start; others: content start.
    pub name_start: u64,
    /// Element: qualified-name end; others: content end.
    pub name_end: u64,
    /// Element: one past the start tag's `>`; others: `0`.
    pub open_end: u64,
    /// Element: the `<` of the end tag (`0` when self-closing); others: `0`.
    pub close_start: u64,
    /// Element: whether it declares a namespace (`xmlns`/`xmlns:…`).
    pub ns_decl: bool,
    /// Child node indices in document order.
    pub children: Vec<u32>,
    /// Attribute indices into [`XmlModel::attrs`].
    pub attrs: Vec<u32>,
}

/// One parsed XML attribute (or namespace declaration).
///
/// `[name_start, name_end)` is the qualified attribute name.
/// `[value_start, value_end)` is the value *without* its quotes.
/// `[span_start, span_end)` is the whole attribute (`name="value"`), so the
/// original quoting is recoverable from the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XAttr {
    /// The qualified attribute name start.
    pub name_start: u64,
    /// The qualified attribute name end.
    pub name_end: u64,
    /// The attribute value start (past the opening quote).
    pub value_start: u64,
    /// The attribute value end (at the closing quote).
    pub value_end: u64,
    /// The whole attribute span start.
    pub span_start: u64,
    /// The whole attribute span end (past the closing quote).
    pub span_end: u64,
    /// `0` = ordinary, `1` = `xmlns`, `2` = `xmlns:prefix`.
    pub is_ns: u8,
}

/// The canonical derived XML model (the materialization of an `XmlModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlModel {
    /// Index of the root element node.
    pub root: u32,
    /// The observed maximum element nesting depth (root = 1).
    pub max_depth: u32,
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// The node arena, in document order.
    pub nodes: Vec<XNode>,
    /// The attribute arena, in document order.
    pub attrs: Vec<XAttr>,
}

impl XmlModel {
    /// The node at `index`, if present.
    pub fn node(&self, index: u32) -> Option<&XNode> {
        self.nodes.get(index as usize)
    }

    /// The attribute at `index`, if present.
    pub fn attr(&self, index: u32) -> Option<&XAttr> {
        self.attrs.get(index as usize)
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32 + self.nodes.len() * 72 + self.attrs.len() * 49);
        out.extend_from_slice(b"XMLM");
        out.push(MODEL_VERSION);
        out.extend_from_slice(&self.root.to_le_bytes());
        out.extend_from_slice(&self.max_depth.to_le_bytes());
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&(self.nodes.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.attrs.len() as u32).to_le_bytes());
        for n in &self.nodes {
            out.push(n.kind);
            out.push(u8::from(n.ns_decl));
            out.extend_from_slice(&n.start.to_le_bytes());
            out.extend_from_slice(&n.end.to_le_bytes());
            out.extend_from_slice(&n.name_start.to_le_bytes());
            out.extend_from_slice(&n.name_end.to_le_bytes());
            out.extend_from_slice(&n.open_end.to_le_bytes());
            out.extend_from_slice(&n.close_start.to_le_bytes());
            out.extend_from_slice(&(n.children.len() as u32).to_le_bytes());
            for c in &n.children {
                out.extend_from_slice(&c.to_le_bytes());
            }
            out.extend_from_slice(&(n.attrs.len() as u32).to_le_bytes());
            for a in &n.attrs {
                out.extend_from_slice(&a.to_le_bytes());
            }
        }
        for a in &self.attrs {
            out.push(a.is_ns);
            out.extend_from_slice(&a.name_start.to_le_bytes());
            out.extend_from_slice(&a.name_end.to_le_bytes());
            out.extend_from_slice(&a.value_start.to_le_bytes());
            out.extend_from_slice(&a.value_end.to_le_bytes());
            out.extend_from_slice(&a.span_start.to_le_bytes());
            out.extend_from_slice(&a.span_end.to_le_bytes());
        }
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<XmlModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"XMLM" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let root = r.u32()?;
        let max_depth = r.u32()?;
        let doc_len = r.u64()?;
        let node_count = r.u32()?;
        let attr_count = r.u32()?;
        if node_count > MAX_MODEL_NODES {
            return Err(corrupt("model node count is implausible"));
        }
        if attr_count > MAX_MODEL_ATTRS {
            return Err(corrupt("model attribute count is implausible"));
        }
        let mut nodes = Vec::with_capacity(node_count as usize);
        for _ in 0..node_count {
            let kind = r.u8()?;
            if kind > K_DOCTYPE {
                return Err(corrupt("unknown node kind"));
            }
            let ns_decl = match r.u8()? {
                0 => false,
                1 => true,
                _ => return Err(corrupt("invalid node flag")),
            };
            let start = r.u64()?;
            let end = r.u64()?;
            let name_start = r.u64()?;
            let name_end = r.u64()?;
            let open_end = r.u64()?;
            let close_start = r.u64()?;
            for (lo, hi) in [
                (start, end),
                (name_start, name_end),
                (0, open_end),
                (0, close_start),
            ] {
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
            let an = r.u32()?;
            if an as u64 > attr_count as u64 {
                return Err(corrupt("node attribute count is implausible"));
            }
            let mut attrs = Vec::with_capacity(an as usize);
            for _ in 0..an {
                let a = r.u32()?;
                if a >= attr_count {
                    return Err(corrupt("attribute index is out of range"));
                }
                attrs.push(a);
            }
            nodes.push(XNode {
                kind,
                start,
                end,
                name_start,
                name_end,
                open_end,
                close_start,
                ns_decl,
                children,
                attrs,
            });
        }
        let mut attrs = Vec::with_capacity(attr_count as usize);
        for _ in 0..attr_count {
            let is_ns = r.u8()?;
            if is_ns > 2 {
                return Err(corrupt("invalid namespace flag"));
            }
            let name_start = r.u64()?;
            let name_end = r.u64()?;
            let value_start = r.u64()?;
            let value_end = r.u64()?;
            let span_start = r.u64()?;
            let span_end = r.u64()?;
            for (lo, hi) in [
                (name_start, name_end),
                (value_start, value_end),
                (span_start, span_end),
            ] {
                if lo > hi || hi > doc_len {
                    return Err(corrupt("attribute span is outside the document"));
                }
            }
            attrs.push(XAttr {
                name_start,
                name_end,
                value_start,
                value_end,
                span_start,
                span_end,
                is_ns,
            });
        }
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        if root as usize >= nodes.len() {
            return Err(corrupt("root index is out of range"));
        }
        Ok(XmlModel {
            root,
            max_depth,
            doc_len,
            nodes,
            attrs,
        })
    }
}

/// A resolved element-path query: the node index and how many siblings matched
/// the final step's name (so an ambiguous name is reported, never hidden).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolved {
    /// The resolved element node index.
    pub index: u32,
    /// The number of child elements with the final step's name.
    pub matches: u32,
}

/// A resolved attribute query: the global attribute index and how many
/// attributes matched the name on the element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedAttr {
    /// The resolved global attribute index.
    pub index: u32,
    /// The number of attributes with the requested name on the element.
    pub matches: u32,
}

/// One lexical-match record from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlMatch {
    /// A canonical-ish element path to the containing node (e.g. `/a/b[2]`).
    pub path: String,
    /// What matched.
    pub role: MatchRole,
    /// The exact source span of the matching construct.
    pub start: u64,
    /// One past the matching construct.
    pub end: u64,
    /// The matched text (raw; entity references unexpanded).
    pub text: String,
}

/// What a lexical match is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchRole {
    /// An element's qualified name.
    Element,
    /// An attribute's qualified name.
    AttrName,
    /// An attribute value.
    AttrValue,
    /// A character-data run.
    Text,
    /// A CDATA section's content.
    Cdata,
}

impl MatchRole {
    /// Stable lower-case name.
    pub const fn name(self) -> &'static str {
        match self {
            MatchRole::Element => "element",
            MatchRole::AttrName => "attr-name",
            MatchRole::AttrValue => "attr-value",
            MatchRole::Text => "text",
            MatchRole::Cdata => "cdata",
        }
    }
}

/// One namespace declaration found in the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NsDecl {
    /// The element that carries the declaration (model node index).
    pub element: u32,
    /// The prefix (empty string for a default `xmlns` declaration).
    pub prefix: String,
    /// The declared URI (raw; entity references unexpanded).
    pub uri: String,
    /// The exact span of the whole declaration attribute.
    pub start: u64,
    /// One past the declaration attribute.
    pub end: u64,
}

/// Byte-based, conservative XML detector. See the module docs.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_xml_document_bytes {
        return false;
    }
    // Must begin with `<` (prolog, root element, comment, or DOCTYPE start).
    if source.first() != Some(&b'<') {
        return false;
    }
    parse(source, limits, false).is_ok()
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of an `XmlModel` node).
pub fn build_xml_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits, true)?.encode())
}

/// Parse `source` into an [`XmlModel`]. `build` selects whether the node/attribute
/// arenas are populated (detection runs with `build = false` to stay O(1) in extra
/// memory).
pub fn parse(source: &[u8], limits: Limits, build: bool) -> Result<XmlModel> {
    if source.len() as u64 > limits.max_xml_document_bytes {
        return Err(Error::resource_limit(format!(
            "XML source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_xml_document_bytes
        )));
    }
    harden_xml(source, limits)?;
    let mut p = Parser::new(source, limits, build);
    let root = p.parse_document()?;
    Ok(XmlModel {
        root,
        max_depth: p.max_depth_seen,
        doc_len: source.len() as u64,
        nodes: p.nodes,
        attrs: p.attrs,
    })
}

/// The exact source bytes of a node's whole span (`[start, end)`).
pub fn token_bytes<'a>(source: &'a [u8], node: &XNode) -> Result<&'a [u8]> {
    slice(source, node.start, node.end).ok_or_else(|| corrupt("node span is outside the source"))
}

/// The exact source bytes of a node's content span (`[name_start, name_end)`).
pub fn content_bytes<'a>(source: &'a [u8], node: &XNode) -> Result<&'a [u8]> {
    slice(source, node.name_start, node.name_end)
        .ok_or_else(|| corrupt("content span is outside the source"))
}

/// The exact source bytes of an attribute's quoted value (`[value_start, value_end)`).
pub fn attr_value_bytes<'a>(source: &'a [u8], attr: &XAttr) -> Result<&'a [u8]> {
    slice(source, attr.value_start, attr.value_end)
        .ok_or_else(|| corrupt("attribute value span is outside the source"))
}

/// An element's qualified name (raw source spelling, prefix intact).
pub fn element_name<'a>(source: &'a [u8], node: &XNode) -> Result<&'a str> {
    if node.kind != K_ELEMENT {
        return Err(corrupt("node is not an element"));
    }
    let bytes = slice(source, node.name_start, node.name_end)
        .ok_or_else(|| corrupt("element name span is outside the source"))?;
    core::str::from_utf8(bytes).map_err(|_| corrupt("element name is not valid UTF-8"))
}

/// An attribute's qualified name (raw source spelling).
pub fn attr_name<'a>(source: &'a [u8], attr: &XAttr) -> Result<&'a str> {
    let bytes = slice(source, attr.name_start, attr.name_end)
        .ok_or_else(|| corrupt("attribute name span is outside the source"))?;
    core::str::from_utf8(bytes).map_err(|_| corrupt("attribute name is not valid UTF-8"))
}

fn slice(source: &[u8], lo: u64, hi: u64) -> Option<&[u8]> {
    let s = usize::try_from(lo).ok()?;
    let e = usize::try_from(hi).ok()?;
    source.get(s..e)
}

/// Parse an element path (`/a/b[2]/c`; `""` is the root element).
fn parse_path(path: &str) -> Result<Vec<Step>> {
    if path.is_empty() {
        return Ok(Vec::new());
    }
    if !path.starts_with('/') {
        return Err(Error::usage(format!(
            "XML path {path:?} must be empty or start with '/'"
        )));
    }
    let mut out = Vec::new();
    for raw in path.split('/').skip(1) {
        if raw.is_empty() {
            return Err(Error::usage(format!("XML path {path:?} has an empty step")));
        }
        let (name, index) = match raw.split_once('[') {
            Some((name, rest)) => {
                let close = rest.strip_suffix(']').ok_or_else(|| {
                    Error::usage(format!("XML path step {raw:?} has an unclosed '['"))
                })?;
                let n: u32 = close
                    .parse()
                    .map_err(|_| Error::usage(format!("XML path step {raw:?} has a bad index")))?;
                if n == 0 {
                    return Err(Error::usage(format!(
                        "XML path step {raw:?} uses a 0 index (indices are 1-based)"
                    )));
                }
                (name, n)
            }
            None => (raw, 1),
        };
        if name.is_empty() {
            return Err(Error::usage(format!(
                "XML path step {raw:?} has an empty name"
            )));
        }
        out.push(Step {
            name: name.to_string(),
            index,
        });
    }
    Ok(out)
}

struct Step {
    name: String,
    index: u32,
}

/// Resolve an element path against a parsed model. A missing element or an
/// out-of-range positional index is a typed decline (never a silent empty answer).
pub fn resolve_path(model: &XmlModel, source: &[u8], path: &str) -> Result<Resolved> {
    let steps = parse_path(path)?;
    // The empty path is the root element. A non-empty path's first step must name
    // the root element itself (`/a/b` selects `b` inside a root named `a`), mirroring
    // how an absolute XML path is rooted at the document element.
    let root_node = model
        .node(model.root)
        .ok_or_else(|| corrupt("model has no root node"))?;
    if root_node.kind != K_ELEMENT {
        return Err(corrupt("the model root is not an element"));
    }
    if steps.is_empty() {
        return Ok(Resolved {
            index: model.root,
            matches: 1,
        });
    }
    let root_name = element_name(source, root_node)?;
    if steps[0].index != 1 || steps[0].name != root_name {
        return Err(Error::unsupported_feature(format!(
            "XML root element is {root_name:?}, not {:?}[{}]",
            steps[0].name, steps[0].index
        )));
    }
    let mut index = model.root;
    let mut matches = 1u32;
    for step in &steps[1..] {
        let node = model
            .node(index)
            .ok_or_else(|| corrupt("path traversal left the model"))?;
        if node.kind != K_ELEMENT {
            return Err(corrupt("path traversal reached a non-element node"));
        }
        let mut found = 0u32;
        let mut selected = None;
        for &c in &node.children {
            let cn = model
                .node(c)
                .ok_or_else(|| corrupt("child index is out of range"))?;
            if cn.kind != K_ELEMENT {
                continue;
            }
            if element_name(source, cn)? == step.name {
                found = found.saturating_add(1);
                if found == step.index {
                    selected = Some(c);
                }
            }
        }
        index = selected.ok_or_else(|| {
            Error::unsupported_feature(format!(
                "XML has no element {name:?}[{idx}] at this path",
                name = step.name,
                idx = step.index
            ))
        })?;
        matches = found;
    }
    Ok(Resolved { index, matches })
}

/// Resolve an attribute reference `PATH@NAME` (e.g. `/a/b@id`, or `@id` for the
/// root). A missing element or attribute is a typed decline.
pub fn resolve_attr(model: &XmlModel, source: &[u8], spec: &str) -> Result<ResolvedAttr> {
    let (path, name) = spec
        .rsplit_once('@')
        .ok_or_else(|| Error::usage(format!("XML attribute spec {spec:?} must be PATH@NAME")))?;
    if name.is_empty() {
        return Err(Error::usage(format!(
            "XML attribute spec {spec:?} has an empty name"
        )));
    }
    let r = resolve_path(model, source, path)?;
    let node = model
        .node(r.index)
        .ok_or_else(|| corrupt("attribute element resolved out of range"))?;
    let mut found = 0u32;
    let mut first = None;
    for &a in &node.attrs {
        let attr = model
            .attr(a)
            .ok_or_else(|| corrupt("attribute index is out of range"))?;
        if attr_name(source, attr)? == name {
            found = found.saturating_add(1);
            if first.is_none() {
                first = Some(a);
            }
        }
    }
    let index = first.ok_or_else(|| {
        Error::unsupported_feature(format!(
            "XML element at path {path:?} has no attribute {name:?}"
        ))
    })?;
    Ok(ResolvedAttr {
        index,
        matches: found,
    })
}

/// All namespace declarations (`xmlns`/`xmlns:prefix`) in document order.
pub fn namespaces(model: &XmlModel, source: &[u8]) -> Result<Vec<NsDecl>> {
    let mut out = Vec::new();
    for (i, node) in model.nodes.iter().enumerate() {
        if node.kind != K_ELEMENT || !node.ns_decl {
            continue;
        }
        for &a in &node.attrs {
            let attr = model
                .attr(a)
                .ok_or_else(|| corrupt("attribute index is out of range"))?;
            if attr.is_ns == 0 {
                continue;
            }
            let full = attr_name(source, attr)?;
            let prefix = match full.strip_prefix("xmlns:") {
                Some(p) => p.to_string(),
                None if full == "xmlns" => String::new(),
                None => continue,
            };
            let uri = core::str::from_utf8(attr_value_bytes(source, attr)?)
                .map_err(|_| corrupt("namespace URI is not valid UTF-8"))?
                .to_string();
            out.push(NsDecl {
                element: i as u32,
                prefix,
                uri,
                start: attr.span_start,
                end: attr.span_end,
            });
        }
    }
    Ok(out)
}

/// The concatenation of all character data (text + CDATA content) in the subtree
/// rooted at `index`, in document order. Entity references are surfaced
/// **literally** (never expanded).
pub fn subtree_text(model: &XmlModel, source: &[u8], index: u32) -> Result<String> {
    let mut out = String::new();
    collect_text(model, source, index, &mut out)?;
    Ok(out)
}

fn collect_text(model: &XmlModel, source: &[u8], index: u32, out: &mut String) -> Result<()> {
    let node = model
        .node(index)
        .ok_or_else(|| corrupt("text walk hit an out-of-range node"))?;
    if is_chardata(node.kind) {
        out.push_str(
            core::str::from_utf8(content_bytes(source, node)?)
                .map_err(|_| corrupt("character data is not valid UTF-8"))?,
        );
        return Ok(());
    }
    for &c in &node.children {
        collect_text(model, source, c, out)?;
    }
    Ok(())
}

/// The whole document's character data: every text and CDATA content span in
/// document order (including prolog/epilog text is impossible, but comments/PIs
/// carry none). Entity references are surfaced literally.
pub fn canonical_text(model: &XmlModel, source: &[u8]) -> Result<String> {
    let mut out = String::new();
    for node in &model.nodes {
        if is_chardata(node.kind) {
            out.push_str(
                core::str::from_utf8(content_bytes(source, node)?)
                    .map_err(|_| corrupt("character data is not valid UTF-8"))?,
            );
        }
    }
    Ok(out)
}

/// The index of a node's parent, if any (the root and top-level nodes have none).
pub fn find_parent(model: &XmlModel, index: u32) -> Option<u32> {
    model
        .nodes
        .iter()
        .position(|n| n.children.contains(&index))
        .map(|i| i as u32)
}

/// A bounded, case-sensitive lexical search over element names, attribute names
/// and values, and character data. Returns matches in document order.
pub fn find(
    model: &XmlModel,
    source: &[u8],
    pattern: &str,
    limits: Limits,
) -> Result<Vec<XmlMatch>> {
    let mut out = Vec::new();
    let mut path: Vec<String> = Vec::new();
    walk_find(
        model, source, model.root, &mut path, pattern, &mut out, limits, 0,
    )?;
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn walk_find(
    model: &XmlModel,
    source: &[u8],
    index: u32,
    path: &mut Vec<String>,
    pattern: &str,
    out: &mut Vec<XmlMatch>,
    limits: Limits,
    depth: u32,
) -> Result<()> {
    if depth > limits.max_xml_depth {
        return Err(Error::resource_limit(
            "XML find exceeded the nesting-depth cap",
        ));
    }
    let node = model
        .node(index)
        .ok_or_else(|| corrupt("find hit an out-of-range node"))?;
    let here = pointer_of(path);
    if node.kind != K_ELEMENT {
        return Ok(());
    }
    let name = element_name(source, node)?;
    if name.contains(pattern) {
        out.push(XmlMatch {
            path: here.clone(),
            role: MatchRole::Element,
            start: node.name_start,
            end: node.name_end,
            text: name.to_string(),
        });
    }
    for &a in &node.attrs {
        let attr = model
            .attr(a)
            .ok_or_else(|| corrupt("attribute index is out of range"))?;
        let an = attr_name(source, attr)?;
        if an.contains(pattern) {
            out.push(XmlMatch {
                path: format!("{here}@{an}"),
                role: MatchRole::AttrName,
                start: attr.name_start,
                end: attr.name_end,
                text: an.to_string(),
            });
        }
        let av = core::str::from_utf8(attr_value_bytes(source, attr)?)
            .map_err(|_| corrupt("attribute value is not valid UTF-8"))?;
        if av.contains(pattern) {
            out.push(XmlMatch {
                path: format!("{here}@{an}"),
                role: MatchRole::AttrValue,
                start: attr.value_start,
                end: attr.value_end,
                text: av.to_string(),
            });
        }
    }
    for &c in &node.children {
        let cn = model
            .node(c)
            .ok_or_else(|| corrupt("child index is out of range"))?;
        if cn.kind == K_ELEMENT {
            let cname = element_name(source, cn)?.to_string();
            // 1-based ordinal among preceding element siblings with this name.
            let mut ordinal = 1u32;
            for &x in node.children.iter() {
                if x == c {
                    break;
                }
                let xn = model
                    .node(x)
                    .ok_or_else(|| corrupt("child index is out of range"))?;
                if xn.kind == K_ELEMENT && element_name(source, xn)? == cname {
                    ordinal += 1;
                }
            }
            path.push(format!("{cname}[{ordinal}]"));
            walk_find(model, source, c, path, pattern, out, limits, depth + 1)?;
            path.pop();
        } else if is_chardata(cn.kind) {
            let content = core::str::from_utf8(content_bytes(source, cn)?)
                .map_err(|_| corrupt("character data is not valid UTF-8"))?;
            if content.contains(pattern) {
                out.push(XmlMatch {
                    path: here.clone(),
                    role: if cn.kind == K_CDATA {
                        MatchRole::Cdata
                    } else {
                        MatchRole::Text
                    },
                    start: cn.name_start,
                    end: cn.name_end,
                    text: content.to_string(),
                });
            }
        }
    }
    Ok(())
}

fn pointer_of(path: &[String]) -> String {
    let mut s = String::new();
    for seg in path {
        s.push('/');
        s.push_str(seg);
    }
    if s.is_empty() {
        s.push('/');
    }
    s
}

// ---------------------------------------------------------------------------
// The bounded scanner
// ---------------------------------------------------------------------------

fn corrupt(msg: &str) -> Error {
    Error::invalid_xml_structure(format!("malformed XML: {msg}"))
}

fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n')
}

fn is_name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b':' || c >= 0x80
}

fn is_name_char(c: u8) -> bool {
    is_name_start(c) || c.is_ascii_digit() || c == b'-' || c == b'.'
}

struct Parser<'a> {
    b: &'a [u8],
    at: usize,
    limits: Limits,
    build: bool,
    st: XmlState,
    nodes: Vec<XNode>,
    attrs: Vec<XAttr>,
    max_depth_seen: u32,
}

impl<'a> Parser<'a> {
    fn new(b: &'a [u8], limits: Limits, build: bool) -> Self {
        Parser {
            b,
            at: 0,
            limits,
            build,
            st: XmlState::new(),
            nodes: Vec::new(),
            attrs: Vec::new(),
            max_depth_seen: 0,
        }
    }

    fn byte(&self) -> Option<u8> {
        self.b.get(self.at).copied()
    }

    fn starts_with(&self, pat: &[u8]) -> bool {
        self.b.get(self.at..).is_some_and(|s| s.starts_with(pat))
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.byte() {
            if is_ws(c) {
                self.at += 1;
            } else {
                break;
            }
        }
    }

    fn find(&self, from: usize, pat: &[u8]) -> Option<usize> {
        if pat.is_empty() {
            return Some(from);
        }
        let hay = self.b.get(from..)?;
        if pat.len() > hay.len() {
            return None;
        }
        for i in 0..=(hay.len() - pat.len()) {
            if &hay[i..i + pat.len()] == pat {
                return Some(from + i);
            }
        }
        None
    }

    fn parse_name(&mut self) -> Result<()> {
        let start = self.at;
        match self.byte() {
            Some(c) if is_name_start(c) => self.at += 1,
            _ => return Err(corrupt("expected an XML name")),
        }
        while let Some(c) = self.byte() {
            if is_name_char(c) {
                self.at += 1;
            } else {
                break;
            }
        }
        if self.at == start {
            return Err(corrupt("empty XML name"));
        }
        Ok(())
    }

    fn push_node(&mut self, node: XNode) -> Result<u32> {
        if !self.build {
            return Ok(0);
        }
        if self.nodes.len() as u64 >= u64::from(MAX_MODEL_NODES) {
            return Err(Error::resource_limit("XML model node arena is full"));
        }
        let idx = self.nodes.len() as u32;
        self.nodes.push(node);
        Ok(idx)
    }

    fn push_attr(&mut self, attr: XAttr) -> Result<u32> {
        if !self.build {
            return Ok(0);
        }
        if self.attrs.len() as u64 >= u64::from(MAX_MODEL_ATTRS) {
            return Err(Error::resource_limit("XML model attribute arena is full"));
        }
        let idx = self.attrs.len() as u32;
        self.attrs.push(attr);
        Ok(idx)
    }

    fn parse_document(&mut self) -> Result<u32> {
        let mut root: Option<u32> = None;
        loop {
            self.skip_ws();
            if self.at >= self.b.len() {
                break;
            }
            if self.byte() != Some(b'<') {
                return Err(corrupt("character data outside the root element"));
            }
            if self.starts_with(b"<?") {
                self.parse_pi()?;
            } else if self.starts_with(b"<!--") {
                self.parse_comment()?;
            } else if self.starts_with(b"<!DOCTYPE") {
                self.parse_doctype()?;
            } else if self.byte() == Some(b'<') && self.b.get(self.at + 1) == Some(&b'!') {
                return Err(corrupt("unexpected declaration at the document level"));
            } else {
                if root.is_some() {
                    return Err(corrupt("multiple root elements"));
                }
                root = Some(self.parse_element(1)?);
            }
        }
        root.ok_or_else(|| corrupt("no root element"))
    }

    fn parse_element(&mut self, depth: u32) -> Result<u32> {
        if depth > self.limits.max_xml_depth {
            return Err(Error::resource_limit(format!(
                "XML nesting exceeds the {}-level cap",
                self.limits.max_xml_depth
            )));
        }
        self.max_depth_seen = self.max_depth_seen.max(depth);
        self.st.event(self.limits)?;
        let start = self.at as u64;
        self.at += 1; // '<'
        let name_start = self.at as u64;
        self.parse_name()?;
        let name_end = self.at as u64;
        let mut attr_entries: Vec<XAttr> = Vec::new();
        let self_closing;
        let open_end;
        loop {
            self.skip_ws();
            match self.byte() {
                Some(b'>') => {
                    self.at += 1;
                    open_end = self.at as u64;
                    self.st.open(self.limits)?;
                    self_closing = false;
                    break;
                }
                Some(b'/') => {
                    if self.b.get(self.at + 1) != Some(&b'>') {
                        return Err(corrupt("expected '>' after '/' in a start tag"));
                    }
                    self.at += 2;
                    open_end = self.at as u64;
                    self.st.leaf(self.limits)?;
                    self_closing = true;
                    break;
                }
                Some(_) => {
                    if attr_entries.len() as u64 >= u64::from(self.limits.max_xml_attrs_per_element)
                    {
                        return Err(Error::resource_limit("XML element has too many attributes"));
                    }
                    attr_entries.push(self.parse_attribute()?);
                }
                None => return Err(corrupt("unexpected end of input inside a start tag")),
            }
        }
        let ns_decl = attr_entries.iter().any(|a| a.is_ns != 0);
        let mut attrs = Vec::with_capacity(attr_entries.len());
        for a in attr_entries {
            attrs.push(self.push_attr(a)?);
        }
        let idx = self.push_node(XNode {
            kind: K_ELEMENT,
            start,
            end: open_end,
            name_start,
            name_end,
            open_end,
            close_start: 0,
            ns_decl,
            children: Vec::new(),
            attrs,
        })?;
        if self_closing {
            return Ok(idx);
        }
        let mut children: Vec<u32> = Vec::new();
        loop {
            if self.at >= self.b.len() {
                return Err(corrupt("unclosed element"));
            }
            if self.byte() == Some(b'<') {
                if self.starts_with(b"</") {
                    let close_start = self.at as u64;
                    self.st.event(self.limits)?;
                    self.at += 2;
                    let en_start = self.at;
                    self.parse_name()?;
                    let en_end = self.at;
                    if self.b.get(name_start as usize..name_end as usize)
                        != self.b.get(en_start..en_end)
                    {
                        return Err(corrupt("mismatched end tag"));
                    }
                    self.skip_ws();
                    if self.byte() != Some(b'>') {
                        return Err(corrupt("expected '>' at the end of an end tag"));
                    }
                    self.at += 1;
                    self.st.close();
                    if self.build
                        && let Some(n) = self.nodes.get_mut(idx as usize)
                    {
                        n.children = children;
                        n.close_start = close_start;
                        n.end = self.at as u64;
                    }
                    return Ok(idx);
                } else if self.starts_with(b"<!--") {
                    children.push(self.parse_comment()?);
                } else if self.starts_with(b"<![CDATA[") {
                    children.push(self.parse_cdata()?);
                } else if self.starts_with(b"<?") {
                    children.push(self.parse_pi()?);
                } else if self.b.get(self.at + 1) == Some(&b'!') {
                    return Err(corrupt("declaration inside element content"));
                } else {
                    children.push(self.parse_element(depth + 1)?);
                }
            } else {
                children.push(self.parse_text()?);
            }
        }
    }

    fn parse_attribute(&mut self) -> Result<XAttr> {
        let span_start = self.at as u64;
        let name_start = self.at as u64;
        self.parse_name()?;
        let name_end = self.at as u64;
        self.skip_ws();
        if self.byte() != Some(b'=') {
            return Err(corrupt("expected '=' after an attribute name"));
        }
        self.at += 1;
        self.skip_ws();
        let quote = match self.byte() {
            Some(q @ (b'"' | b'\'')) => q,
            _ => return Err(corrupt("attribute value must be quoted")),
        };
        self.at += 1;
        let value_start = self.at as u64;
        loop {
            match self.byte() {
                None => return Err(corrupt("unterminated attribute value")),
                Some(c) if c == quote => {
                    let value_end = self.at as u64;
                    self.at += 1;
                    let span_end = self.at as u64;
                    let name_bytes = self
                        .b
                        .get(name_start as usize..name_end as usize)
                        .ok_or_else(|| corrupt("attribute name span overflow"))?;
                    let is_ns = if name_bytes == b"xmlns" {
                        1
                    } else if name_bytes.starts_with(b"xmlns:") {
                        2
                    } else {
                        0
                    };
                    return Ok(XAttr {
                        name_start,
                        name_end,
                        value_start,
                        value_end,
                        span_start,
                        span_end,
                        is_ns,
                    });
                }
                Some(b'<') => return Err(corrupt("'<' is not allowed in an attribute value")),
                Some(_) => self.at += 1,
            }
        }
    }

    fn parse_text(&mut self) -> Result<u32> {
        let start = self.at;
        while let Some(c) = self.byte() {
            if c == b'<' {
                break;
            }
            self.at += 1;
        }
        let end = self.at;
        let raw = self
            .b
            .get(start..end)
            .ok_or_else(|| corrupt("text span overflow"))?;
        if raw.windows(3).any(|w| w == b"]]>") {
            return Err(corrupt("']]>' is not allowed in character data"));
        }
        self.st.event(self.limits)?;
        self.st.text(end - start, self.limits)?;
        self.push_node(XNode {
            kind: K_TEXT,
            start: start as u64,
            end: end as u64,
            name_start: start as u64,
            name_end: end as u64,
            open_end: 0,
            close_start: 0,
            ns_decl: false,
            children: Vec::new(),
            attrs: Vec::new(),
        })
    }

    fn parse_cdata(&mut self) -> Result<u32> {
        self.st.event(self.limits)?;
        let start = self.at;
        self.at += 9; // <![CDATA[
        let content_start = self.at;
        let close = self
            .find(self.at, b"]]>")
            .ok_or_else(|| corrupt("unterminated CDATA section"))?;
        let content_end = close;
        self.at = close + 3;
        let end = self.at;
        self.st.text(content_end - content_start, self.limits)?;
        self.push_node(XNode {
            kind: K_CDATA,
            start: start as u64,
            end: end as u64,
            name_start: content_start as u64,
            name_end: content_end as u64,
            open_end: 0,
            close_start: 0,
            ns_decl: false,
            children: Vec::new(),
            attrs: Vec::new(),
        })
    }

    fn parse_comment(&mut self) -> Result<u32> {
        self.st.event(self.limits)?;
        let start = self.at;
        self.at += 4; // <!--
        let content_start = self.at;
        let close = self
            .find(self.at, b"-->")
            .ok_or_else(|| corrupt("unterminated comment"))?;
        let content_end = close;
        // XML forbids '--' inside a comment.
        if self
            .b
            .get(content_start..content_end)
            .is_some_and(|c| c.windows(2).any(|w| w == b"--"))
        {
            return Err(corrupt("'--' is not allowed inside a comment"));
        }
        self.at = close + 3;
        let end = self.at;
        self.push_node(XNode {
            kind: K_COMMENT,
            start: start as u64,
            end: end as u64,
            name_start: content_start as u64,
            name_end: content_end as u64,
            open_end: 0,
            close_start: 0,
            ns_decl: false,
            children: Vec::new(),
            attrs: Vec::new(),
        })
    }

    fn parse_pi(&mut self) -> Result<u32> {
        self.st.event(self.limits)?;
        let start = self.at;
        self.at += 2; // <?
        let name_start = self.at;
        self.parse_name()?;
        let name_end = self.at;
        let close = self
            .find(self.at, b"?>")
            .ok_or_else(|| corrupt("unterminated processing instruction"))?;
        self.at = close + 2;
        let end = self.at;
        self.push_node(XNode {
            kind: K_PI,
            start: start as u64,
            end: end as u64,
            name_start: name_start as u64,
            name_end: name_end as u64,
            open_end: 0,
            close_start: 0,
            ns_decl: false,
            children: Vec::new(),
            attrs: Vec::new(),
        })
    }

    fn parse_doctype(&mut self) -> Result<u32> {
        self.st.event(self.limits)?;
        let start = self.at;
        self.at += 9; // <!DOCTYPE
        loop {
            match self.byte() {
                None => return Err(corrupt("unterminated DOCTYPE")),
                // An internal subset declares entities: refuse it (no XXE, no
                // billion-laughs). This mirrors the shared bounded-XML policy.
                Some(b'[') => return Err(doctype_declined()),
                Some(b'>') => {
                    self.at += 1;
                    break;
                }
                Some(_) => self.at += 1,
            }
        }
        let end = self.at;
        let raw = self
            .b
            .get(start..end)
            .ok_or_else(|| corrupt("DOCTYPE span overflow"))?;
        let text = core::str::from_utf8(raw).map_err(|_| corrupt("DOCTYPE is not valid UTF-8"))?;
        accept_doctype(text)?;
        self.push_node(XNode {
            kind: K_DOCTYPE,
            start: start as u64,
            end: end as u64,
            name_start: (start + 9).min(end) as u64,
            name_end: end as u64,
            open_end: 0,
            close_start: 0,
            ns_decl: false,
            children: Vec::new(),
            attrs: Vec::new(),
        })
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

    fn at_end(&self) -> bool {
        self.at == self.b.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(src: &[u8]) -> XmlModel {
        parse(src, Limits::DEFAULT, true).unwrap()
    }

    #[test]
    fn detects_xml_and_rejects_junk() {
        assert!(detect(b"<?xml version=\"1.0\"?><a/>", Limits::DEFAULT));
        assert!(detect(b"<a><b>x</b></a>", Limits::DEFAULT));
        assert!(detect(b"<!-- c --><a/>", Limits::DEFAULT));
        assert!(detect(b"<!DOCTYPE a><a/>", Limits::DEFAULT));
        assert!(!detect(b"", Limits::DEFAULT));
        assert!(!detect(b"plain prose", Limits::DEFAULT));
        assert!(!detect(b"<<<", Limits::DEFAULT));
        assert!(!detect(b"<3", Limits::DEFAULT));
        assert!(!detect(b"<a>", Limits::DEFAULT));
        assert!(!detect(b"<a></b>", Limits::DEFAULT));
        // An internal-subset DOCTYPE (billion-laughs) is refused -> Opaque.
        assert!(!detect(
            b"<!DOCTYPE a [<!ENTITY x \"y\">]><a/>",
            Limits::DEFAULT
        ));
    }

    #[test]
    fn preserves_spans_comments_cdata_pi_and_namespaces() {
        let src: &[u8] =
            b"<?xml version=\"1.0\"?><!-- hi --><a xmlns:p=\"u\" id='7'>t<![CDATA[x<y]]><p:b/></a>";
        let m = model(src);
        // The root element's name and start tag.
        let root = m.node(m.root).unwrap();
        assert_eq!(element_name(src, root).unwrap(), "a");
        assert_eq!(&src[root.start as usize..root.name_end as usize], b"<a");
        // Namespace declaration is recorded.
        let ns = namespaces(&m, src).unwrap();
        assert_eq!(ns.len(), 1);
        assert_eq!(ns[0].prefix, "p");
        assert_eq!(ns[0].uri, "u");
        assert_eq!(
            &src[ns[0].start as usize..ns[0].end as usize],
            b"xmlns:p=\"u\""
        );
        // The comment and CDATA keep their exact spans.
        let comment = m.nodes.iter().find(|n| n.kind == K_COMMENT).unwrap();
        assert_eq!(token_bytes(src, comment).unwrap(), b"<!-- hi -->");
        assert_eq!(content_bytes(src, comment).unwrap(), b" hi ");
        let cdata = m.nodes.iter().find(|n| n.kind == K_CDATA).unwrap();
        assert_eq!(token_bytes(src, cdata).unwrap(), b"<![CDATA[x<y]]>");
        assert_eq!(content_bytes(src, cdata).unwrap(), b"x<y");
        // The element text is the raw character data (entity refs unexpanded).
        assert_eq!(subtree_text(&m, src, m.root).unwrap(), "tx<y");
        // Attribute exact span preserves the single quotes.
        let ra = resolve_attr(&m, src, "/a@id").unwrap();
        let attr = m.attr(ra.index).unwrap();
        assert_eq!(
            &src[attr.span_start as usize..attr.span_end as usize],
            b"id='7'"
        );
        assert_eq!(attr_value_bytes(src, attr).unwrap(), b"7");
    }

    #[test]
    fn path_resolution_handles_positions_and_errors() {
        let src: &[u8] = b"<r><a>1</a><a>2</a><b><a>3</a></b></r>";
        let m = model(src);
        let r = resolve_path(&m, src, "/r/a[2]").unwrap();
        assert_eq!(subtree_text(&m, src, r.index).unwrap(), "2");
        let r = resolve_path(&m, src, "/r/b/a").unwrap();
        assert_eq!(subtree_text(&m, src, r.index).unwrap(), "3");
        assert!(resolve_path(&m, src, "/r/nope").is_err());
        assert!(resolve_path(&m, src, "/r/a[3]").is_err());
        assert!(resolve_path(&m, src, "no-slash").is_err());
        assert!(resolve_path(&m, src, "/r/a[0]").is_err());
        // A path whose first step is not the root element is a typed decline.
        assert!(resolve_path(&m, src, "/a").is_err());
    }

    #[test]
    fn entity_references_are_surfaced_literally() {
        // A predefined entity is NOT expanded in the reported text span.
        let src: &[u8] = b"<a>a&amp;b</a>";
        let m = model(src);
        assert_eq!(subtree_text(&m, src, m.root).unwrap(), "a&amp;b");
        // The whole-source exactness is independent of the derived projection.
        assert!(!build_xml_model(src, Limits::DEFAULT).unwrap().is_empty());
    }

    #[test]
    fn billion_laughs_declines_typed() {
        let src: &[u8] = b"<!DOCTYPE lolz [<!ENTITY lol \"lol\">]><lolz>&lol;</lolz>";
        assert!(!detect(src, Limits::DEFAULT));
        let e = parse(src, Limits::DEFAULT, true).unwrap_err();
        assert_eq!(e.class(), crate::error::ErrorClass::InvalidXmlStructure);
    }

    #[test]
    fn deep_nesting_declines_typed() {
        let mut src = Vec::new();
        for _ in 0..200 {
            src.extend_from_slice(b"<a>");
        }
        src.extend_from_slice(b"x");
        for _ in 0..200 {
            src.extend_from_slice(b"</a>");
        }
        assert!(!detect(&src, Limits::STRICT));
        let e = parse(&src, Limits::STRICT, true).unwrap_err();
        assert_eq!(e.class(), crate::error::ErrorClass::ResourceLimit);
    }

    #[test]
    fn model_roundtrips() {
        let src: &[u8] = b"<r a=\"1\" b='2'><c/>t</r>";
        let m = model(src);
        let bytes = m.encode();
        assert_eq!(XmlModel::decode(&bytes).unwrap(), m);
        for cut in 0..bytes.len() {
            let _ = XmlModel::decode(&bytes[..cut]);
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
