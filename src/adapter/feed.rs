//! Bounded, representation-preserving RSS 2.0 / Atom 1.0 feed adapter (Phase 21.21).
//!
//! A syndication feed is XML, so the physical bytes are shared with the
//! standalone [`XML adapter`](crate::adapter::xml). This adapter therefore
//! **reuses the shared bounded XML parser and policy** ([`crate::adapter::xml`])
//! to obtain a span-preserving element/attribute tree, and layers a small,
//! representation-preserving *feed* projection on top of it. The exact authority
//! is still the whole source (a `DocumentExact`, a RAW-like authority): everything
//! this module produces is a bounded, deterministic (`Q_gen`) projection that never
//! sits on the exactness path.
//!
//! ## Why a separate dialect
//!
//! Two feeds that are byte-for-byte different documents can share every XML
//! construct, so the *semantic* dialect matters: RSS is `<rss><channel>…` with
//! `<item>` records, Atom is `<feed xmlns="http://www.w3.org/2005/Atom">` with
//! `<entry>` records, and their field vocabularies differ (`item`/`pubDate`/`guid`
//! vs `entry`/`updated`/`summary`). The dialect is recorded in the model exactly
//! as the CSV/TSV adapter records its delimiter/terminator dialect and the config
//! adapter records its INI/`.env`/`properties` dialect.
//!
//! ## Representation preservation
//!
//! Because the model embeds the full [`XmlModel`], it preserves, for the whole
//! document and for every recognized feed field:
//!
//! * every element's qualified name, start-tag, end-tag, and full **byte span**;
//! * every attribute's name/quoted-value/inner-value/full span, including the
//!   Atom `<link href="…" rel="…"/>` spelling and the RSS `<guid isPermaLink="…">`
//!   spelling, in source order;
//! * every element's **document order** (the `<item>`/`<entry>` arena is ordered);
//! * the **namespace declaration** on an Atom `<feed>` (kept as an ordinary
//!   attribute with its exact span);
//! * entity references are **never expanded** and no DTD internal subset is
//!   processed (the shared XML policy declines it).
//!
//! ## Detection (semantic sub-detection — the critical part)
//!
//! The physical bytes are XML, so a feed must be distinguished from a plain XML
//! document by a **bounded semantic test** run *before* the generic XML detector:
//!
//! * **RSS** — the root element is `<rss>` (its local name) with a `<channel>`
//!   child element;
//! * **Atom** — the root element is `<feed>` whose namespace prefix is bound to
//!   the Atom namespace URI `http://www.w3.org/2005/Atom`, with at least one
//!   `<entry>` child element.
//!
//! Everything else declines: a plain XML document stays
//! [`Xml`](crate::field::document_format::DocumentFormat::Xml), an HTML document
//! (already claimed earlier) stays `Html`, and an `<rss>`-shaped-but-invalid
//! document (no `<channel>`), an Atom `<feed>` **without** the Atom namespace or
//! **without** an `<entry>`, and a non-feed root all decline and stay on the
//! XML/opaque path.
//!
//! ### What the detector cannot distinguish (recorded honestly)
//!
//! * **RSS 1.0 (RDF)** — its root is `<rdf:RDF>` (an RDF graph of `<channel>` and
//!   `<item>` resources), **not** `<rss>`, so it is not recognized here and stays
//!   `Xml`. Only RSS 2.0's `<rss><channel>` shape is claimed.
//! * **An Atom 0.3 document** — its namespace URI is `http://purl.org/atom/ns#`,
//!   not the Atom 1.0 URI, so it stays `Xml`.
//! * **A feed with no records** — an Atom `<feed>` with no `<entry>` declines (the
//!   `<entry>`-bearing test is the Atom-only signal); RSS is admitted on
//!   `<rss><channel>` alone, so an empty-but-well-formed RSS channel is a feed.
//!
//! ## Bounds
//!
//! Parsing reuses the shared bounded XML parser (depth/nodes/events/text caps,
//! NaN-free, no `unwrap`/`panic`) and additionally charges the recognized-field
//! and entry arenas against [`Limits::max_feed_entries`],
//! [`Limits::max_feed_fields`], and [`Limits::max_feed_field_bytes`], and the
//! source against [`Limits::max_feed_document_bytes`]. Untrusted input can only
//! ever yield a typed decline.

use crate::adapter::xml::{
    K_ELEMENT, XmlModel, attr_name, attr_value_bytes, element_name, parse as xml_parse,
    subtree_text, token_bytes,
};
use crate::error::{Error, ErrorClass, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model entries (defends the decoder against a hostile blob).
pub const MAX_MODEL_ENTRIES: u32 = 1 << 20;

/// Dialect tag: RSS 2.0 (`<rss><channel>…<item>…`).
pub const DIALECT_RSS: u8 = 0;
/// Dialect tag: Atom 1.0 (`<feed xmlns="…/Atom">…<entry>…`).
pub const DIALECT_ATOM: u8 = 1;

/// The Atom 1.0 namespace URI.
pub const ATOM_NS: &str = "http://www.w3.org/2005/Atom";

/// Stable name for a dialect tag.
pub const fn dialect_name(d: u8) -> &'static str {
    match d {
        DIALECT_ATOM => "atom",
        _ => "rss",
    }
}

/// The recognized channel/feed-level field element local names, by dialect.
pub const fn channel_fields(dialect: u8) -> &'static [&'static str] {
    match dialect {
        DIALECT_ATOM => &["id", "title", "updated", "link"],
        _ => &["title", "link", "description", "language"],
    }
}

/// The recognized entry/item-level field element local names, by dialect.
pub const fn entry_fields(dialect: u8) -> &'static [&'static str] {
    match dialect {
        DIALECT_ATOM => &["id", "title", "link", "updated", "summary", "content"],
        _ => &[
            "title",
            "link",
            "description",
            "pubDate",
            "guid",
            "category",
        ],
    }
}

/// The part of a qualified name after the last `:`, with its prefix.
pub fn split_qname(name: &str) -> (&str, &str) {
    match name.rsplit_once(':') {
        Some((prefix, local)) => (prefix, local),
        None => ("", name),
    }
}

/// One attribute of a field element: its qualified name (spelling preserved),
/// its decoded value, and its exact span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldAttr {
    /// The qualified attribute name (prefix intact).
    pub name: String,
    /// The decoded attribute value.
    pub value: String,
    /// The exact span of the whole attribute.
    pub start: u64,
    /// One past the attribute.
    pub end: u64,
}

/// One lexical match from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedMatch {
    /// The 0-based entry ordinal the field belongs to, or `None` for a
    /// channel/feed-level field.
    pub entry: Option<u32>,
    /// The matched field's local name.
    pub name: String,
    /// The exact source span of the matching field element.
    pub start: u64,
    /// One past the matching field element.
    pub end: u64,
    /// The decoded field text.
    pub text: String,
}

/// The canonical derived feed model (the materialization of a `FeedModel` node).
///
/// The embedded [`XmlModel`] carries the whole span-preserving tree; `root`,
/// `channel`, and `entries` are indices into that tree. `channel` is the RSS
/// `<channel>` element or, for Atom, the `<feed>` root element itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedModel {
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// The recorded dialect (`DIALECT_*`).
    pub dialect: u8,
    /// The XML root element node index (`<rss>` or `<feed>`).
    pub root: u32,
    /// The container node index: the RSS `<channel>`, or the Atom `<feed>` root.
    pub channel: u32,
    /// The `<item>` / `<entry>` element node indices, in document order.
    pub entries: Vec<u32>,
    /// The span-preserving XML tree the indices refer to.
    pub xml: XmlModel,
}

impl FeedModel {
    /// The XML node at `index`, if present.
    pub fn node(&self, index: u32) -> Option<&crate::adapter::xml::XNode> {
        self.xml.node(index)
    }

    /// The number of records (`<item>`/`<entry>`) in the model.
    pub fn entry_count(&self) -> u32 {
        self.entries.len() as u32
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let xml_bytes = self.xml.encode();
        let mut out = Vec::with_capacity(32 + xml_bytes.len() + self.entries.len() * 4);
        out.extend_from_slice(b"FEED");
        out.push(MODEL_VERSION);
        out.push(self.dialect);
        out.push(0); // reserved
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&self.root.to_le_bytes());
        out.extend_from_slice(&self.channel.to_le_bytes());
        out.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        for e in &self.entries {
            out.extend_from_slice(&e.to_le_bytes());
        }
        out.extend_from_slice(&(xml_bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&xml_bytes);
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<FeedModel> {
        let mut r = Reader::new(bytes);
        if r.bytes(4)? != b"FEED" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let dialect = r.u8()?;
        if dialect > DIALECT_ATOM {
            return Err(corrupt("unknown dialect"));
        }
        if r.u8()? != 0 {
            return Err(corrupt("unknown model flags"));
        }
        let doc_len = r.u64()?;
        let root = r.u32()?;
        let channel = r.u32()?;
        let count = r.u32()?;
        if count > MAX_MODEL_ENTRIES {
            return Err(corrupt("model entry count is implausible"));
        }
        let mut entries = Vec::with_capacity(count as usize);
        for _ in 0..count {
            entries.push(r.u32()?);
        }
        let xml_len = r.u32()?;
        let xml_bytes = r.bytes(xml_len as usize)?;
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        let xml = XmlModel::decode(xml_bytes)?;
        if xml.doc_len != doc_len {
            return Err(corrupt("model doc_len disagrees with the XML model"));
        }
        if root as usize >= xml.nodes.len() {
            return Err(corrupt("root index is out of range"));
        }
        if channel as usize >= xml.nodes.len() {
            return Err(corrupt("channel index is out of range"));
        }
        for &e in &entries {
            if e as usize >= xml.nodes.len() {
                return Err(corrupt("entry index is out of range"));
            }
        }
        Ok(FeedModel {
            doc_len,
            dialect,
            root,
            channel,
            entries,
            xml,
        })
    }
}

/// Byte-based, conservative RSS/Atom detector. See the module docs for the exact
/// semantic boundary.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    classify(source, limits).is_ok()
}

/// Classify `source` as one feed dialect, or decline typed when it is not an
/// RSS 2.0 / Atom 1.0 feed under the semantic test.
pub fn classify(source: &[u8], limits: Limits) -> Result<u8> {
    Ok(parse(source, limits, false)?.dialect)
}

/// Parse `source` into a [`FeedModel`]. `build` selects whether the entry arena is
/// populated (detection runs with `build = false`); the XML tree is always built
/// because the semantic test walks it.
pub fn parse(source: &[u8], limits: Limits, build: bool) -> Result<FeedModel> {
    if source.len() as u64 > limits.max_feed_document_bytes {
        return Err(Error::resource_limit(format!(
            "feed source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_feed_document_bytes
        )));
    }
    let xml = xml_parse(source, limits, true).map_err(|e| match e.class() {
        ErrorClass::ResourceLimit => e,
        _ => invalid_feed(format!(
            "feed source is not well-formed XML: {}",
            e.message()
        )),
    })?;
    let (dialect, channel, entries_all) = analyze(&xml, source, limits)?;
    let entries = if build { entries_all } else { Vec::new() };
    Ok(FeedModel {
        doc_len: source.len() as u64,
        dialect,
        root: xml.root,
        channel,
        entries,
        xml,
    })
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of a `FeedModel` node).
pub fn build_feed_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits, true)?.encode())
}

/// The recognized channel/feed-level field node indices, in document order.
pub fn channel_field_nodes(model: &FeedModel, source: &[u8]) -> Result<Vec<u32>> {
    recognized_children(
        &model.xml,
        source,
        model.channel,
        channel_fields(model.dialect),
    )
}

/// The recognized field node indices of the `index`-th entry/item, in document
/// order.
pub fn entry_field_nodes(model: &FeedModel, source: &[u8], index: u32) -> Result<Vec<u32>> {
    let node = *model
        .entries
        .get(index as usize)
        .ok_or_else(|| Error::unsupported_feature(format!("feed has no entry {index}")))?;
    recognized_children(&model.xml, source, node, entry_fields(model.dialect))
}

/// A field element's decoded text: for an Atom `<link>` the `href` attribute
/// value is returned (the Atom link value lives in the attribute); otherwise the
/// element's character data. Entity references are surfaced literally.
pub fn field_text(model: &FeedModel, source: &[u8], node: u32) -> Result<String> {
    let n = model
        .xml
        .node(node)
        .ok_or_else(|| corrupt("feed field node is out of range"))?;
    let local = split_qname(element_name(source, n)?).1;
    if model.dialect == DIALECT_ATOM
        && local == "link"
        && let Some(href) = attr_value_by_local(&model.xml, source, node, "href")?
    {
        return Ok(href);
    }
    subtree_text(&model.xml, source, node)
}

/// The exact source bytes of a field element's whole span.
pub fn field_bytes<'a>(model: &FeedModel, source: &'a [u8], node: u32) -> Result<&'a [u8]> {
    let n = model
        .xml
        .node(node)
        .ok_or_else(|| corrupt("feed field node is out of range"))?;
    token_bytes(source, n)
}

/// The attributes of a field element, in source order, with the qualified
/// spelling preserved.
pub fn field_attrs(model: &FeedModel, source: &[u8], node: u32) -> Result<Vec<FieldAttr>> {
    let n = model
        .xml
        .node(node)
        .ok_or_else(|| corrupt("feed field node is out of range"))?;
    let mut out = Vec::new();
    for &a in &n.attrs {
        let attr = model
            .xml
            .attr(a)
            .ok_or_else(|| corrupt("feed attribute index is out of range"))?;
        out.push(FieldAttr {
            name: attr_name(source, attr)?.to_string(),
            value: String::from_utf8_lossy(attr_value_bytes(source, attr)?).into_owned(),
            start: attr.span_start,
            end: attr.span_end,
        });
    }
    Ok(out)
}

/// A bounded, case-sensitive lexical search over recognized field values, in
/// document order (channel/feed fields first, then entries).
pub fn find(
    model: &FeedModel,
    source: &[u8],
    pattern: &str,
    max_out: u64,
) -> Result<Vec<FeedMatch>> {
    let mut out: Vec<FeedMatch> = Vec::new();
    for f in channel_field_nodes(model, source)? {
        push_match(model, source, f, None, pattern, max_out, &mut out)?;
    }
    for (i, &entry) in model.entries.iter().enumerate() {
        let fields = recognized_children(&model.xml, source, entry, entry_fields(model.dialect))?;
        for f in fields {
            push_match(model, source, f, Some(i as u32), pattern, max_out, &mut out)?;
        }
    }
    Ok(out)
}

fn push_match(
    model: &FeedModel,
    source: &[u8],
    node: u32,
    entry: Option<u32>,
    pattern: &str,
    max_out: u64,
    out: &mut Vec<FeedMatch>,
) -> Result<()> {
    let text = field_text(model, source, node)?;
    if !text.contains(pattern) {
        return Ok(());
    }
    let n = model
        .xml
        .node(node)
        .ok_or_else(|| corrupt("feed field node is out of range"))?;
    out.push(FeedMatch {
        entry,
        name: split_qname(element_name(source, n)?).1.to_string(),
        start: n.start,
        end: n.end,
        text,
    });
    if out.len() as u64 > max_out {
        return Err(Error::resource_limit(format!(
            "feed find exceeded the {max_out}-match budget"
        )));
    }
    Ok(())
}

/// The whole feed's canonical text: every recognized field value in document
/// order (channel/feed fields, then each entry's fields), newline-separated.
pub fn canonical_text(model: &FeedModel, source: &[u8], max_out: u64) -> Result<String> {
    let mut out = String::new();
    for f in channel_field_nodes(model, source)? {
        out.push_str(&field_text(model, source, f)?);
        out.push('\n');
        if out.len() as u64 > max_out {
            return Err(Error::resource_limit(format!(
                "feed text projection exceeds the {max_out}-byte budget"
            )));
        }
    }
    for i in 0..model.entries.len() as u32 {
        for f in entry_field_nodes(model, source, i)? {
            out.push_str(&field_text(model, source, f)?);
            out.push('\n');
            if out.len() as u64 > max_out {
                return Err(Error::resource_limit(format!(
                    "feed text projection exceeds the {max_out}-byte budget"
                )));
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// Resolve the dialect, the container node, and the entry nodes from a parsed XML
/// tree, enforcing the feed caps.
fn analyze(xml: &XmlModel, source: &[u8], limits: Limits) -> Result<(u8, u32, Vec<u32>)> {
    let root_node = xml
        .node(xml.root)
        .ok_or_else(|| corrupt("model has no root node"))?;
    if root_node.kind != K_ELEMENT {
        return Err(corrupt("the feed root is not an element"));
    }
    let full = element_name(source, root_node)?;
    let (prefix, local) = split_qname(full);
    match local {
        "rss" => {
            let channel = child_element(xml, source, xml.root, "channel")?
                .ok_or_else(|| invalid_feed("RSS `<rss>` has no `<channel>` child element"))?;
            let entries = child_elements(xml, source, channel, "item")?;
            check_caps(xml, source, channel, &entries, DIALECT_RSS, limits)?;
            Ok((DIALECT_RSS, channel, entries))
        }
        "feed" => {
            if !atom_ns_in_force(xml, source, xml.root, prefix)? {
                return Err(invalid_feed(
                    "`<feed>` is not in the Atom namespace (http://www.w3.org/2005/Atom)",
                ));
            }
            let entries = child_elements(xml, source, xml.root, "entry")?;
            if entries.is_empty() {
                return Err(invalid_feed("Atom `<feed>` has no `<entry>` child element"));
            }
            check_caps(xml, source, xml.root, &entries, DIALECT_ATOM, limits)?;
            Ok((DIALECT_ATOM, xml.root, entries))
        }
        other => Err(invalid_feed(format!(
            "root element {other:?} is not an RSS/Atom feed"
        ))),
    }
}

fn check_caps(
    xml: &XmlModel,
    source: &[u8],
    channel: u32,
    entries: &[u32],
    dialect: u8,
    limits: Limits,
) -> Result<()> {
    if entries.len() as u64 > u64::from(limits.max_feed_entries) {
        return Err(Error::resource_limit(format!(
            "feed exceeds the {}-entry cap",
            limits.max_feed_entries
        )));
    }
    let mut fields: u64 = 0;
    let mut charge = |nodes: Vec<u32>| -> Result<()> {
        for n in nodes {
            let node = xml
                .node(n)
                .ok_or_else(|| corrupt("feed field node is out of range"))?;
            if node.end.saturating_sub(node.start) > limits.max_feed_field_bytes {
                return Err(Error::resource_limit(format!(
                    "feed field exceeds the {}-byte cap",
                    limits.max_feed_field_bytes
                )));
            }
            fields = fields.saturating_add(1);
            if fields > u64::from(limits.max_feed_fields) {
                return Err(Error::resource_limit(format!(
                    "feed exceeds the {}-field cap",
                    limits.max_feed_fields
                )));
            }
        }
        Ok(())
    };
    charge(recognized_children(
        xml,
        source,
        channel,
        channel_fields(dialect),
    )?)?;
    for &e in entries {
        charge(recognized_children(xml, source, e, entry_fields(dialect))?)?;
    }
    Ok(())
}

/// The first direct child element of `parent` whose local name is `name`.
fn child_element(xml: &XmlModel, source: &[u8], parent: u32, name: &str) -> Result<Option<u32>> {
    let node = xml
        .node(parent)
        .ok_or_else(|| corrupt("feed parent node is out of range"))?;
    for &c in &node.children {
        let cn = xml
            .node(c)
            .ok_or_else(|| corrupt("feed child index is out of range"))?;
        if cn.kind != K_ELEMENT {
            continue;
        }
        if split_qname(element_name(source, cn)?).1 == name {
            return Ok(Some(c));
        }
    }
    Ok(None)
}

/// Every direct child element of `parent` whose local name is `name`, in document
/// order.
fn child_elements(xml: &XmlModel, source: &[u8], parent: u32, name: &str) -> Result<Vec<u32>> {
    recognized_children(xml, source, parent, &[name])
}

/// Every direct child element of `parent` whose local name is one of `names`, in
/// document order.
fn recognized_children(
    xml: &XmlModel,
    source: &[u8],
    parent: u32,
    names: &[&str],
) -> Result<Vec<u32>> {
    let node = xml
        .node(parent)
        .ok_or_else(|| corrupt("feed parent node is out of range"))?;
    let mut out = Vec::new();
    for &c in &node.children {
        let cn = xml
            .node(c)
            .ok_or_else(|| corrupt("feed child index is out of range"))?;
        if cn.kind != K_ELEMENT {
            continue;
        }
        let local = split_qname(element_name(source, cn)?).1;
        if names.contains(&local) {
            out.push(c);
        }
    }
    Ok(out)
}

/// Whether the element `node` puts the namespace `prefix` (empty for the default
/// namespace) in force, bound to the Atom namespace URI. Only the element's own
/// declarations are consulted — the feed root is the document element, so there
/// is no enclosing scope.
fn atom_ns_in_force(xml: &XmlModel, source: &[u8], node: u32, prefix: &str) -> Result<bool> {
    let n = xml
        .node(node)
        .ok_or_else(|| corrupt("feed root node is out of range"))?;
    for &a in &n.attrs {
        let attr = xml
            .attr(a)
            .ok_or_else(|| corrupt("feed attribute index is out of range"))?;
        if attr.is_ns == 0 {
            continue;
        }
        let name = attr_name(source, attr)?;
        let matches = if prefix.is_empty() {
            name == "xmlns"
        } else {
            name.strip_prefix("xmlns:") == Some(prefix)
        };
        if matches {
            return Ok(attr_value_bytes(source, attr)? == ATOM_NS.as_bytes());
        }
    }
    Ok(false)
}

/// The value of the first attribute of `node` whose local name is `name`
/// (namespace-prefix stripped; namespace declarations are skipped).
fn attr_value_by_local(
    xml: &XmlModel,
    source: &[u8],
    node: u32,
    name: &str,
) -> Result<Option<String>> {
    let n = xml
        .node(node)
        .ok_or_else(|| corrupt("feed field node is out of range"))?;
    for &a in &n.attrs {
        let attr = xml
            .attr(a)
            .ok_or_else(|| corrupt("feed attribute index is out of range"))?;
        if attr.is_ns != 0 {
            continue;
        }
        if split_qname(attr_name(source, attr)?).1 == name {
            return Ok(Some(
                String::from_utf8_lossy(attr_value_bytes(source, attr)?).into_owned(),
            ));
        }
    }
    Ok(None)
}

fn corrupt(msg: &str) -> Error {
    Error::invalid_feed_structure(msg)
}

fn invalid_feed(msg: impl Into<String>) -> Error {
    Error::invalid_feed_structure(msg)
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(b: &'a [u8]) -> Self {
        Reader { b, at: 0 }
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

    fn at_end(&self) -> bool {
        self.at >= self.b.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSS: &[u8] = b"<?xml version=\"1.0\"?>\n<rss version=\"2.0\">\n<channel>\n\
<title>Example Feed</title>\n<link>https://example.com/</link>\n\
<description>An example</description>\n<language>en</language>\n\
<item>\n<title>First</title>\n<link>https://example.com/1</link>\n\
<description>one</description>\n<pubDate>Mon, 01 Jan 2024 00:00:00 GMT</pubDate>\n\
<guid isPermaLink=\"false\">urn:1</guid>\n<category>news</category>\n</item>\n\
<item>\n<title>Second</title>\n<guid>urn:2</guid>\n</item>\n</channel>\n</rss>\n";

    const ATOM: &[u8] = b"<?xml version=\"1.0\"?>\n\
<feed xmlns=\"http://www.w3.org/2005/Atom\">\n<id>urn:feed:1</id>\n\
<title>Example</title>\n<updated>2024-01-01T00:00:00Z</updated>\n\
<link href=\"https://example.com/\" rel=\"alternate\"/>\n\
<entry>\n<id>urn:entry:1</id>\n<title>First</title>\n\
<link href=\"https://example.com/1\"/>\n<updated>2024-01-01T00:00:00Z</updated>\n\
<summary>one</summary>\n<content type=\"html\">&lt;b&gt;one&lt;/b&gt;</content>\n</entry>\n\
<entry>\n<id>urn:entry:2</id>\n<title>Second</title>\n\
<link href=\"https://example.com/2\"/>\n</entry>\n</feed>\n";

    fn model(src: &[u8]) -> FeedModel {
        parse(src, Limits::DEFAULT, true).unwrap()
    }

    #[test]
    fn detects_rss_and_atom_and_rejects_junk() {
        assert_eq!(classify(RSS, Limits::DEFAULT).unwrap(), DIALECT_RSS);
        assert_eq!(classify(ATOM, Limits::DEFAULT).unwrap(), DIALECT_ATOM);
        // A plain XML document is not a feed.
        assert!(classify(b"<a><b>text</b></a>", Limits::DEFAULT).is_err());
        // `<rss>` without a `<channel>` declines.
        assert!(classify(b"<rss version=\"2.0\"></rss>", Limits::DEFAULT).is_err());
        // `<feed>` with no Atom namespace declines.
        assert!(classify(b"<feed><entry/></feed>", Limits::DEFAULT).is_err());
        // Atom `<feed>` with no `<entry>` declines.
        assert!(
            classify(
                b"<feed xmlns=\"http://www.w3.org/2005/Atom\"><id>x</id></feed>",
                Limits::DEFAULT
            )
            .is_err()
        );
        // Prose and junk decline.
        assert!(classify(b"not xml at all", Limits::DEFAULT).is_err());
    }

    #[test]
    fn rss_preserves_spans_order_and_attributes() {
        let m = model(RSS);
        assert_eq!(m.dialect, DIALECT_RSS);
        assert_eq!(m.entry_count(), 2);
        let ch = channel_field_nodes(&m, RSS).unwrap();
        assert_eq!(ch.len(), 4);
        // The channel title span is the exact source bytes.
        assert_eq!(
            field_bytes(&m, RSS, ch[0]).unwrap(),
            b"<title>Example Feed</title>"
        );
        // The entries are in document order.
        let e0 = entry_field_nodes(&m, RSS, 0).unwrap();
        let e1 = entry_field_nodes(&m, RSS, 1).unwrap();
        assert_eq!(e0.len(), 6);
        assert_eq!(e1.len(), 2);
        assert!(m.entries[0] < m.entries[1]);
        // `guid isPermaLink="false"` survives as an attribute with its span.
        let guid = e0[4];
        let attrs = field_attrs(&m, RSS, guid).unwrap();
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0].name, "isPermaLink");
        assert_eq!(attrs[0].value, "false");
        assert_eq!(field_text(&m, RSS, guid).unwrap(), "urn:1");
    }

    #[test]
    fn atom_preserves_namespace_and_link_href() {
        let m = model(ATOM);
        assert_eq!(m.dialect, DIALECT_ATOM);
        assert_eq!(m.entry_count(), 2);
        // The namespace declaration is preserved as an attribute on the root.
        let root = m.node(m.root).unwrap();
        let ns = m.xml.attr(root.attrs[0]).unwrap();
        assert_eq!(ns.is_ns, 1);
        assert_eq!(attr_name(ATOM, ns).unwrap(), "xmlns");
        assert_eq!(attr_value_bytes(ATOM, ns).unwrap(), ATOM_NS.as_bytes());
        // The channel/feed `link` is a void element; its text is the href.
        let fields = channel_field_nodes(&m, ATOM).unwrap();
        let link = fields[3];
        assert_eq!(field_text(&m, ATOM, link).unwrap(), "https://example.com/");
        let attrs = field_attrs(&m, ATOM, link).unwrap();
        assert!(
            attrs
                .iter()
                .any(|a| a.name == "href" && a.value == "https://example.com/")
        );
        assert!(attrs.iter().any(|a| a.name == "rel"));
        // Entry content retains an entity reference literally (never expanded).
        let e0 = entry_field_nodes(&m, ATOM, 0).unwrap();
        let content = e0[5];
        assert_eq!(
            field_text(&m, ATOM, content).unwrap(),
            "&lt;b&gt;one&lt;/b&gt;"
        );
    }

    #[test]
    fn model_roundtrips() {
        for src in [RSS, ATOM] {
            let m = model(src);
            let enc = m.encode();
            assert_eq!(FeedModel::decode(&enc).unwrap(), m);
            let mut bad = enc.clone();
            bad[0] = b'X';
            assert!(FeedModel::decode(&bad).is_err());
            let mut truncated = enc;
            truncated.truncate(truncated.len() - 1);
            assert!(FeedModel::decode(&truncated).is_err());
        }
    }

    #[test]
    fn canonical_text_and_find() {
        let m = model(RSS);
        let text = canonical_text(&m, RSS, 1 << 20).unwrap();
        assert!(text.contains("Example Feed"));
        assert!(text.contains("First"));
        let hits = find(&m, RSS, "First", 1 << 20).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].entry, Some(0));
        assert_eq!(hits[0].name, "title");
    }

    #[test]
    fn caps_decline_typed() {
        // A feed over the STRICT entry cap (4096).
        let mut src = String::from("<rss version=\"2.0\"><channel>");
        for _ in 0..5000 {
            src.push_str("<item><title>t</title></item>");
        }
        src.push_str("</channel></rss>");
        let e = parse(src.as_bytes(), Limits::STRICT, true).unwrap_err();
        assert_eq!(e.class(), ErrorClass::ResourceLimit);
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0xF33D_2020_21DE_AD00;
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
                let _ = canonical_text(&m, &buf, 1 << 20);
                let _ = build_feed_model(&buf, Limits::STRICT);
            }
        }
    }
}
