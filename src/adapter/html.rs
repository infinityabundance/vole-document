//! Bounded, representation-preserving, **error-recovering** HTML adapter (Phase 21.10).
//!
//! HTML is the next Wave-2 markup format after XML. Like JSON/YAML/CSV/Markdown/XML
//! it is *not* a package: there is no OPC/ZIP layer, no `mimetype`, and no
//! relationship graph. The exact leaf is therefore the **whole source** (a
//! `DocumentExact`, a RAW-like authority), and everything this module produces is a
//! bounded, deterministic (`Q_gen`) projection that never sits on the exactness path.
//!
//! ## Why a bespoke, span-preserving, error-recovering scanner
//!
//! A conventional HTML load (`lxml`, `BeautifulSoup`, a DOM, `html5lib`) keeps
//! *values* and drops the *representation*: it discards comments, attribute quoting,
//! unquoted-attribute spelling, and every source offset, and it expands entity
//! references. This adapter does the opposite. For every construct it records the
//! exact **byte span** in the source, so it preserves and can report:
//!
//! * every **element** (its name span, start-tag span, end-tag span, and full
//!   span), in document order;
//! * every **attribute** (its name span, its value span **with and without** the
//!   quotes, and its full span), in source order, distinguishing a double-quoted,
//!   single-quoted, **unquoted**, or **boolean** value;
//! * every **text** run, **comment**, **DOCTYPE**, and the raw content of every
//!   raw-text element (`<script>`/`<style>`);
//! * **entity references are never expanded** — the raw `&name;`/`&#…;` bytes
//!   remain in the reported text/content spans, surfaced literally;
//! * `<script>`/`<style>` content is captured as **raw bytes and never executed**
//!   (nor parsed as markup).
//!
//! Unlike XML, HTML is parsed **error-recovering** (the HTML parsing spec is a
//! recovery algorithm, not a well-formedness grammar):
//!
//! * **void elements** (`area`/`base`/`br`/`col`/`embed`/`hr`/`img`/`input`/`link`/
//!   `meta`/`param`/`source`/`track`/`wbr`) never take an end tag;
//! * a small set of **implicit tag-closing** rules apply (a new `<li>` closes a
//!   previous open `<li>`; a block start tag closes an open `<p>`; a new `<td>`/
//!   `<th>`/`<tr>` closes the previous cell/row; a heading closes a heading);
//! * a **stray** end tag with no matching open element is ignored;
//! * an **unterminated** tag, attribute value, comment, or raw-text run is closed
//!   at end of input.
//!
//! Malformed input is recovered, **never a panic**. There is no `unwrap`, `expect`,
//! `panic!`, or indexing that is not span-checked on the parse path.
//!
//! ## Encoding policy (explicit)
//!
//! HTML has **no `encoding_rs`** here: only **UTF-8** is accepted. A UTF-16 BOM, a
//! NUL byte, or any non-UTF-8 byte string is a typed decline (and is then not
//! detected as HTML, staying `Opaque`). Declared `charset=` values are not
//! honoured — this is stated, not guessed.
//!
//! ## Dependency policy
//!
//! This module is **dependency-free** (the non-default `html` feature adds no
//! crate). The scanner is bespoke; it does **not** use `quick-xml` and does not
//! require the `xml` feature, because HTML's error-recovery tokenizer is not a
//! well-formedness parser, and pulling a heavyweight HTML crate would be neither
//! needed nor policy-free.
//!
//! ## Detection
//!
//! [`detect`] is deliberately conservative. XML is tried **before** HTML in
//! `detect_document_format`, so a well-formed XML/XHTML source stays XML; HTML only
//! claims the `<`-bearing sources XML declines. A source qualifies as HTML only if
//! it carries clear HTML structure — a `<!doctype html>`, an `<html`/`<head`/
//! `<body` tag, or a preponderance of known HTML tags — and then parses within every
//! cap. Plain prose and non-HTML `<`-junk stay
//! [`DocumentFormat::Opaque`](crate::field::document_format::DocumentFormat::Opaque).
//!
//! ## Explicitly declined
//!
//! This adapter does **not** support: a DOCTYPE with an internal subset (declined,
//! so no entity expansion surface), character encodings beyond UTF-8, the HTML
//! tree-construction algorithm's full adoption-agency/foster-parenting recovery
//! (only the bounded subset above), CSS/JS execution or interpretation, or SVG/
//! MathML foreign-content tree adjustments.

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model nodes (defends the decoder against a hostile blob).
pub const MAX_MODEL_NODES: u32 = 1 << 24;
/// Hard cap on decoded model attributes (defends the decoder against a hostile blob).
pub const MAX_MODEL_ATTRS: u32 = 1 << 26;

/// HTML node kind: an element (`<name …>…</name>`, `<name …>`, or `<name …/>`).
pub const K_ELEMENT: u8 = 0;
/// HTML node kind: a character-data run.
pub const K_TEXT: u8 = 1;
/// HTML node kind: a comment (or a bogus comment from `<!`/`<?`).
pub const K_COMMENT: u8 = 2;
/// HTML node kind: a document type declaration (`<!DOCTYPE…>`).
pub const K_DOCTYPE: u8 = 3;
/// HTML node kind: the **raw** content of a `<script>`/`<style>` element.
pub const K_RAW_TEXT: u8 = 4;

/// Stable lower-case kind name for reports and JSON output.
pub const fn kind_name(kind: u8) -> &'static str {
    match kind {
        K_ELEMENT => "element",
        K_TEXT => "text",
        K_COMMENT => "comment",
        K_DOCTYPE => "doctype",
        K_RAW_TEXT => "raw-text",
        _ => "unknown",
    }
}

/// Whether `kind` is a *visible* character-data run (raw `script`/`style` content is
/// **not** visible reading text). Used by the reading-text projection.
pub const fn is_text(kind: u8) -> bool {
    kind == K_TEXT
}

/// Attribute value quoting, as spelled in the source.
pub const Q_NONE: u8 = 0;
/// A single-quoted attribute value.
pub const Q_SINGLE: u8 = 1;
/// A double-quoted attribute value.
pub const Q_DOUBLE: u8 = 2;
/// An unquoted attribute value.
pub const Q_UNQUOTED: u8 = 3;

/// One parsed HTML node.
///
/// * For an **element**, `[start, end)` is the whole element; `[name_start,
///   name_end)` is its (source-spelled) name; `open_end` is one past the start
///   tag's `>`; `close_start` is the `<` of the end tag (`0` when there is no
///   explicit end tag — a void element, a self-closing tag, an implicitly closed
///   element, or an element closed at EOF); `children` are the child node indices;
///   `attrs` index [`HAttr`]s.
/// * For **text/comment/doctype/raw-text**, `[start, end)` is the whole construct
///   (delimiters included) and `[name_start, name_end)` is the *content* span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HNode {
    /// One of the `K_*` tags.
    pub kind: u8,
    /// The construct's first source byte.
    pub start: u64,
    /// One past the construct's last source byte.
    pub end: u64,
    /// Element: name start; others: content start.
    pub name_start: u64,
    /// Element: name end; others: content end.
    pub name_end: u64,
    /// Element: one past the start tag's `>`; others: `0`.
    pub open_end: u64,
    /// Element: the `<` of the end tag (`0` when there is none); others: `0`.
    pub close_start: u64,
    /// Child node indices in document order.
    pub children: Vec<u32>,
    /// Attribute indices into [`HtmlModel::attrs`].
    pub attrs: Vec<u32>,
}

/// One parsed HTML attribute.
///
/// `[name_start, name_end)` is the attribute name. `[value_start, value_end)` is the
/// value *without* its quotes (empty for a boolean attribute). `[span_start,
/// span_end)` is the whole attribute (`name="value"`, `name`, …). `quote` records
/// how the value was spelled in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HAttr {
    /// The attribute name start.
    pub name_start: u64,
    /// The attribute name end.
    pub name_end: u64,
    /// The attribute value start (past the opening quote).
    pub value_start: u64,
    /// The attribute value end (at the closing quote).
    pub value_end: u64,
    /// The whole attribute span start.
    pub span_start: u64,
    /// The whole attribute span end.
    pub span_end: u64,
    /// One of the `Q_*` quoting tags.
    pub quote: u8,
}

/// The canonical derived HTML model (the materialization of an `HtmlModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlModel {
    /// Index of the root element, or `u32::MAX` when the document has none.
    pub root: u32,
    /// The observed maximum element nesting depth (root = 1).
    pub max_depth: u32,
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// The node arena, in document order.
    pub nodes: Vec<HNode>,
    /// The attribute arena, in document order.
    pub attrs: Vec<HAttr>,
    /// Top-level node indices (doctype/comments/text/elements) in document order.
    pub top: Vec<u32>,
}

impl HtmlModel {
    /// The node at `index`, if present.
    pub fn node(&self, index: u32) -> Option<&HNode> {
        self.nodes.get(index as usize)
    }

    /// The attribute at `index`, if present.
    pub fn attr(&self, index: u32) -> Option<&HAttr> {
        self.attrs.get(index as usize)
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64 + self.nodes.len() * 72 + self.attrs.len() * 49);
        out.extend_from_slice(b"HTML");
        out.push(MODEL_VERSION);
        out.extend_from_slice(&self.root.to_le_bytes());
        out.extend_from_slice(&self.max_depth.to_le_bytes());
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&(self.nodes.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.attrs.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.top.len() as u32).to_le_bytes());
        for t in &self.top {
            out.extend_from_slice(&t.to_le_bytes());
        }
        for n in &self.nodes {
            out.push(n.kind);
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
            out.push(a.quote);
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
    pub fn decode(bytes: &[u8]) -> Result<HtmlModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"HTML" {
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
        let top_count = r.u32()?;
        if node_count > MAX_MODEL_NODES {
            return Err(corrupt("model node count is implausible"));
        }
        if attr_count > MAX_MODEL_ATTRS {
            return Err(corrupt("model attribute count is implausible"));
        }
        if top_count > node_count {
            return Err(corrupt("model top-level count is implausible"));
        }
        let mut top = Vec::with_capacity(top_count as usize);
        for _ in 0..top_count {
            let t = r.u32()?;
            if t >= node_count {
                return Err(corrupt("top-level index is out of range"));
            }
            top.push(t);
        }
        let mut nodes = Vec::with_capacity(node_count as usize);
        for _ in 0..node_count {
            let kind = r.u8()?;
            if kind > K_RAW_TEXT {
                return Err(corrupt("unknown node kind"));
            }
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
            nodes.push(HNode {
                kind,
                start,
                end,
                name_start,
                name_end,
                open_end,
                close_start,
                children,
                attrs,
            });
        }
        let mut attrs = Vec::with_capacity(attr_count as usize);
        for _ in 0..attr_count {
            let quote = r.u8()?;
            if quote > Q_UNQUOTED {
                return Err(corrupt("invalid attribute quoting tag"));
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
            attrs.push(HAttr {
                name_start,
                name_end,
                value_start,
                value_end,
                span_start,
                span_end,
                quote,
            });
        }
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        if root != u32::MAX && root as usize >= nodes.len() {
            return Err(corrupt("root index is out of range"));
        }
        Ok(HtmlModel {
            root,
            max_depth,
            doc_len,
            nodes,
            attrs,
            top,
        })
    }
}

/// A resolved element-path query: the node index and how many siblings matched the
/// final step's name (so an ambiguous name is reported, never hidden).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolved {
    /// The resolved element node index.
    pub index: u32,
    /// The number of child elements with the final step's name.
    pub matches: u32,
}

/// A resolved attribute query: the global attribute index and how many attributes
/// matched the name on the element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedAttr {
    /// The resolved global attribute index.
    pub index: u32,
    /// The number of attributes with the requested name on the element.
    pub matches: u32,
}

/// One lexical-match record from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlMatch {
    /// A canonical-ish element path to the containing node (e.g. `/html/body[1]/p`).
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
    /// An element's name.
    Element,
    /// An attribute's name.
    AttrName,
    /// An attribute value.
    AttrValue,
    /// A character-data run.
    Text,
}

impl MatchRole {
    /// Stable lower-case name.
    pub const fn name(self) -> &'static str {
        match self {
            MatchRole::Element => "element",
            MatchRole::AttrName => "attr-name",
            MatchRole::AttrValue => "attr-value",
            MatchRole::Text => "text",
        }
    }
}

/// One raw `<script>`/`<style>` entry found in the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawText {
    /// The element name (`script` or `style`), lower-cased.
    pub name: String,
    /// The 0-based ordinal of this raw element among all raw elements.
    pub ordinal: u32,
    /// The element node index.
    pub element: u32,
    /// The raw content span start.
    pub start: u64,
    /// One past the raw content span.
    pub end: u64,
    /// The whole element span start.
    pub element_start: u64,
    /// One past the whole element span.
    pub element_end: u64,
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Byte-based, conservative HTML detector. See the module docs.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_html_document_bytes {
        return false;
    }
    if !has_html_structure(source) {
        return false;
    }
    parse(source, limits, false).is_ok()
}

/// Whether `source` carries clear HTML structure (a `<!doctype html>`, an
/// `<html`/`<head`/`<body` tag, or a preponderance of known HTML tags). This is the
/// conservative gate detection applies *in addition to* a successful bounded parse.
pub fn has_html_structure(source: &[u8]) -> bool {
    if find_ci(source, b"<!doctype html").is_some() {
        return true;
    }
    for tag in [b"html".as_slice(), b"head", b"body"] {
        if has_tag(source, tag) {
            return true;
        }
    }
    let mut known = 0u32;
    let mut total = 0u32;
    scan_tag_names(source, |name| {
        total = total.saturating_add(1);
        if is_known_html_tag(name) {
            known = known.saturating_add(1);
        }
    });
    // A strong preponderance of known HTML tags: at least three, and at least
    // three quarters of all tags. This fallback exists for tag *fragments* that
    // carry no `<html>`/`<head>`/`<body>` and no `<!doctype html>`; requiring a
    // clear majority keeps plain prose that merely mentions a tag or two Opaque.
    known >= 3 && known.saturating_mul(4) >= total.saturating_mul(3)
}

/// Whether `source` contains a start tag named `name` (case-insensitive) followed by
/// whitespace, `/`, or `>`.
fn has_tag(source: &[u8], name: &[u8]) -> bool {
    let mut i = 0usize;
    while i + 1 < source.len() {
        if source[i] == b'<' && source[i + 1].is_ascii_alphabetic() {
            let start = i + 1;
            let mut j = start;
            while j < source.len() && is_tag_name_char(source[j]) {
                j += 1;
            }
            if j - start == name.len() && source[start..j].eq_ignore_ascii_case(name) {
                match source.get(j) {
                    Some(&c) if c == b'>' || c == b'/' || is_ws(c) => return true,
                    None => return true,
                    _ => {}
                }
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    false
}

/// Invoke `f` with the (raw) name of every start-tag or end-tag found in `source`.
fn scan_tag_names(source: &[u8], mut f: impl FnMut(&[u8])) {
    let mut i = 0usize;
    while i + 1 < source.len() {
        if source[i] == b'<' {
            let mut j = i + 1;
            if source[j] == b'/' {
                j += 1;
            }
            if j < source.len() && source[j].is_ascii_alphabetic() {
                let start = j;
                while j < source.len() && is_tag_name_char(source[j]) {
                    j += 1;
                }
                f(&source[start..j]);
                i = j;
                continue;
            }
        }
        i += 1;
    }
}

/// The set of HTML element names used for the "preponderance" heuristic. It is a
/// conservative, common subset — deliberately not exhaustive, because an incomplete
/// list only makes detection *stricter*.
fn is_known_html_tag(name: &[u8]) -> bool {
    const KNOWN: &[&[u8]] = &[
        b"a",
        b"abbr",
        b"address",
        b"article",
        b"aside",
        b"b",
        b"blockquote",
        b"body",
        b"br",
        b"button",
        b"caption",
        b"code",
        b"col",
        b"colgroup",
        b"dd",
        b"del",
        b"details",
        b"div",
        b"dl",
        b"dt",
        b"em",
        b"fieldset",
        b"figcaption",
        b"figure",
        b"footer",
        b"form",
        b"h1",
        b"h2",
        b"h3",
        b"h4",
        b"h5",
        b"h6",
        b"head",
        b"header",
        b"hr",
        b"html",
        b"i",
        b"iframe",
        b"img",
        b"input",
        b"ins",
        b"label",
        b"legend",
        b"li",
        b"link",
        b"main",
        b"meta",
        b"nav",
        b"ol",
        b"option",
        b"optgroup",
        b"p",
        b"pre",
        b"script",
        b"section",
        b"select",
        b"small",
        b"span",
        b"strong",
        b"style",
        b"sub",
        b"summary",
        b"sup",
        b"table",
        b"tbody",
        b"td",
        b"textarea",
        b"tfoot",
        b"th",
        b"thead",
        b"title",
        b"tr",
        b"u",
        b"ul",
        b"video",
        b"audio",
        b"canvas",
        b"picture",
        b"source",
        b"template",
        b"figure",
        b"time",
        b"mark",
    ];
    let lower = to_ascii_lowercase(name);
    KNOWN.contains(&lower.as_slice())
}

/// The void elements: start tags with no content and no end tag.
fn is_void_element(name: &[u8]) -> bool {
    const VOID: &[&[u8]] = &[
        b"area", b"base", b"br", b"col", b"embed", b"hr", b"img", b"input", b"link", b"meta",
        b"param", b"source", b"track", b"wbr",
    ];
    VOID.contains(&name)
}

/// Whether `top` must be implicitly closed when a `new` start tag begins.
fn auto_closes(top: &[u8], new: &[u8]) -> bool {
    fn is(n: &[u8], s: &str) -> bool {
        n == s.as_bytes()
    }
    fn heading(n: &[u8]) -> bool {
        ["h1", "h2", "h3", "h4", "h5", "h6"]
            .iter()
            .any(|s| is(n, s))
    }
    fn block(n: &[u8]) -> bool {
        [
            "address",
            "article",
            "aside",
            "blockquote",
            "details",
            "div",
            "dl",
            "fieldset",
            "figcaption",
            "figure",
            "footer",
            "form",
            "h1",
            "h2",
            "h3",
            "h4",
            "h5",
            "h6",
            "header",
            "hr",
            "main",
            "menu",
            "nav",
            "ol",
            "p",
            "pre",
            "section",
            "table",
            "ul",
        ]
        .iter()
        .any(|s| is(n, s))
    }
    if is(top, "p") && block(new) {
        return true;
    }
    if heading(top) && (heading(new) || block(new)) {
        return true;
    }
    if is(top, "li") && is(new, "li") {
        return true;
    }
    if (is(top, "dt") || is(top, "dd")) && (is(new, "dt") || is(new, "dd")) {
        return true;
    }
    if is(top, "tr") && is(new, "tr") {
        return true;
    }
    if (is(top, "td") || is(top, "th")) && (is(new, "td") || is(new, "th") || is(new, "tr")) {
        return true;
    }
    if is(top, "thead") && (is(new, "tbody") || is(new, "tfoot")) {
        return true;
    }
    if is(top, "tbody") && is(new, "tfoot") {
        return true;
    }
    if is(top, "option") && (is(new, "option") || is(new, "optgroup")) {
        return true;
    }
    if is(top, "optgroup") && is(new, "optgroup") {
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// Public parse entry points
// ---------------------------------------------------------------------------

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of an `HtmlModel` node).
pub fn build_html_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits, true)?.encode())
}

/// Parse `source` into an [`HtmlModel`]. `build` selects whether the node/attribute
/// arenas are populated (detection runs with `build = false` to stay O(1) in extra
/// memory while still tracking the open-element stack for depth/declines).
pub fn parse(source: &[u8], limits: Limits, build: bool) -> Result<HtmlModel> {
    if source.len() as u64 > limits.max_html_document_bytes {
        return Err(Error::resource_limit(format!(
            "HTML source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_html_document_bytes
        )));
    }
    harden_html(source, limits)?;
    let mut p = Parser::new(source, limits, build);
    p.parse_document()?;
    let root = p
        .nodes
        .iter()
        .position(|n| n.kind == K_ELEMENT)
        .map_or(u32::MAX, |i| i as u32);
    Ok(HtmlModel {
        root,
        max_depth: p.max_depth_seen,
        doc_len: source.len() as u64,
        nodes: p.nodes,
        attrs: p.attrs,
        top: p.top,
    })
}

/// Reject constructs that must never reach the parse path: over-large inputs,
/// UTF-16 encodings, NUL bytes, and non-UTF-8 bytes (HTML here is UTF-8 only).
pub fn harden_html(bytes: &[u8], limits: Limits) -> Result<()> {
    if bytes.len() as u64 > limits.max_html_document_bytes {
        return Err(Error::resource_limit(
            "HTML source exceeds max_html_document_bytes",
        ));
    }
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return Err(Error::invalid_html_structure(
            "UTF-16 HTML is not supported (UTF-8 only)",
        ));
    }
    if bytes.contains(&0) {
        return Err(Error::invalid_html_structure("HTML contains a NUL byte"));
    }
    if core::str::from_utf8(bytes).is_err() {
        return Err(Error::invalid_html_structure("HTML is not valid UTF-8"));
    }
    Ok(())
}

/// The exact source bytes of a node's whole span (`[start, end)`).
pub fn token_bytes<'a>(source: &'a [u8], node: &HNode) -> Result<&'a [u8]> {
    slice(source, node.start, node.end).ok_or_else(|| corrupt("node span is outside the source"))
}

/// The exact source bytes of a node's content span (`[name_start, name_end)`): an
/// element's name, or another node's content.
pub fn content_bytes<'a>(source: &'a [u8], node: &HNode) -> Result<&'a [u8]> {
    slice(source, node.name_start, node.name_end)
        .ok_or_else(|| corrupt("content span is outside the source"))
}

/// The exact source bytes of an attribute's value (`[value_start, value_end)`).
pub fn attr_value_bytes<'a>(source: &'a [u8], attr: &HAttr) -> Result<&'a [u8]> {
    slice(source, attr.value_start, attr.value_end)
        .ok_or_else(|| corrupt("attribute value span is outside the source"))
}

/// An element's name (raw source spelling).
pub fn element_name<'a>(source: &'a [u8], node: &HNode) -> Result<&'a str> {
    if node.kind != K_ELEMENT {
        return Err(corrupt("node is not an element"));
    }
    let bytes = slice(source, node.name_start, node.name_end)
        .ok_or_else(|| corrupt("element name span is outside the source"))?;
    core::str::from_utf8(bytes).map_err(|_| corrupt("element name is not valid UTF-8"))
}

/// An attribute's name (raw source spelling).
pub fn attr_name<'a>(source: &'a [u8], attr: &HAttr) -> Result<&'a str> {
    let bytes = slice(source, attr.name_start, attr.name_end)
        .ok_or_else(|| corrupt("attribute name span is outside the source"))?;
    core::str::from_utf8(bytes).map_err(|_| corrupt("attribute name is not valid UTF-8"))
}

/// The element-node indices of every heading element (`h1`..`h6`), in document
/// order, together with their level.
pub fn headings(model: &HtmlModel, source: &[u8]) -> Result<Vec<(u32, u8)>> {
    let mut out = Vec::new();
    for (i, n) in model.nodes.iter().enumerate() {
        if n.kind != K_ELEMENT {
            continue;
        }
        let name = element_name(source, n)?;
        if name.len() == 2
            && (name.as_bytes()[0] | 0x20) == b'h'
            && (b'1'..=b'6').contains(&name.as_bytes()[1])
        {
            out.push((i as u32, name.as_bytes()[1] - b'0'));
        }
    }
    Ok(out)
}

/// The element-node indices of every anchor (`<a>`) with an `href` attribute, in
/// document order.
pub fn anchors(model: &HtmlModel, source: &[u8]) -> Result<Vec<u32>> {
    let mut out = Vec::new();
    for (i, n) in model.nodes.iter().enumerate() {
        if n.kind != K_ELEMENT || !element_name(source, n)?.eq_ignore_ascii_case("a") {
            continue;
        }
        if has_attr(model, source, n, "href")? {
            out.push(i as u32);
        }
    }
    Ok(out)
}

/// Whether element `node` carries an attribute named `name` (case-insensitive).
pub fn has_attr(model: &HtmlModel, source: &[u8], node: &HNode, name: &str) -> Result<bool> {
    for &a in &node.attrs {
        let attr = model
            .attr(a)
            .ok_or_else(|| corrupt("attribute index is out of range"))?;
        if attr_name(source, attr)?.eq_ignore_ascii_case(name) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Every raw `<script>`/`<style>` element (its raw content span and full element
/// span), in document order.
pub fn raw_texts(model: &HtmlModel, source: &[u8]) -> Result<Vec<RawText>> {
    let mut out = Vec::new();
    let mut ordinal = 0u32;
    for (i, n) in model.nodes.iter().enumerate() {
        if n.kind != K_ELEMENT {
            continue;
        }
        let name = element_name(source, n)?;
        if !name.eq_ignore_ascii_case("script") && !name.eq_ignore_ascii_case("style") {
            continue;
        }
        // The raw content is the element's first raw-text child, if any.
        let (start, end) = n
            .children
            .iter()
            .find_map(|&c| {
                model
                    .node(c)
                    .filter(|cn| cn.kind == K_RAW_TEXT)
                    .map(|cn| (cn.start, cn.end))
            })
            .unwrap_or((n.open_end, n.open_end));
        out.push(RawText {
            name: to_ascii_lowercase(name.as_bytes())
                .iter()
                .map(|b| *b as char)
                .collect(),
            ordinal,
            element: i as u32,
            start,
            end,
            element_start: n.start,
            element_end: n.end,
        });
        ordinal = ordinal.saturating_add(1);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Path / attribute resolution
// ---------------------------------------------------------------------------

fn slice(source: &[u8], lo: u64, hi: u64) -> Option<&[u8]> {
    let s = usize::try_from(lo).ok()?;
    let e = usize::try_from(hi).ok()?;
    source.get(s..e)
}

struct Step {
    name: String,
    index: u32,
}

/// Parse an element path (`/html/body[2]/p`; `""` is the root element).
fn parse_path(path: &str) -> Result<Vec<Step>> {
    if path.is_empty() {
        return Ok(Vec::new());
    }
    if !path.starts_with('/') {
        return Err(Error::usage(format!(
            "HTML path {path:?} must be empty or start with '/'"
        )));
    }
    let mut out = Vec::new();
    for raw in path.split('/').skip(1) {
        if raw.is_empty() {
            return Err(Error::usage(format!(
                "HTML path {path:?} has an empty step"
            )));
        }
        let (name, index) = match raw.split_once('[') {
            Some((name, rest)) => {
                let close = rest.strip_suffix(']').ok_or_else(|| {
                    Error::usage(format!("HTML path step {raw:?} has an unclosed '['"))
                })?;
                let n: u32 = close
                    .parse()
                    .map_err(|_| Error::usage(format!("HTML path step {raw:?} has a bad index")))?;
                if n == 0 {
                    return Err(Error::usage(format!(
                        "HTML path step {raw:?} uses a 0 index (indices are 1-based)"
                    )));
                }
                (name, n)
            }
            None => (raw, 1),
        };
        if name.is_empty() {
            return Err(Error::usage(format!(
                "HTML path step {raw:?} has an empty name"
            )));
        }
        out.push(Step {
            name: name.to_string(),
            index,
        });
    }
    Ok(out)
}

/// Resolve an element path against a parsed model. A missing element or an
/// out-of-range positional index is a typed decline (never a silent empty answer).
/// Name comparisons are ASCII case-insensitive (HTML tag names are case-insensitive).
pub fn resolve_path(model: &HtmlModel, source: &[u8], path: &str) -> Result<Resolved> {
    if model.root == u32::MAX {
        return Err(Error::unsupported_feature(
            "HTML document has no root element",
        ));
    }
    let steps = parse_path(path)?;
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
    if steps[0].index != 1 || !root_name.eq_ignore_ascii_case(&steps[0].name) {
        return Err(Error::unsupported_feature(format!(
            "HTML root element is {root_name:?}, not {:?}[{}]",
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
            if element_name(source, cn)?.eq_ignore_ascii_case(&step.name) {
                found = found.saturating_add(1);
                if found == step.index {
                    selected = Some(c);
                }
            }
        }
        index = selected.ok_or_else(|| {
            Error::unsupported_feature(format!(
                "HTML has no element {name:?}[{idx}] at this path",
                name = step.name,
                idx = step.index
            ))
        })?;
        matches = found;
    }
    Ok(Resolved { index, matches })
}

/// Resolve an attribute reference `PATH@NAME` (e.g. `/html/body@id`, or `@id` for
/// the root). A missing element or attribute is a typed decline. Name comparisons
/// are ASCII case-insensitive.
pub fn resolve_attr(model: &HtmlModel, source: &[u8], spec: &str) -> Result<ResolvedAttr> {
    let (path, name) = spec
        .rsplit_once('@')
        .ok_or_else(|| Error::usage(format!("HTML attribute spec {spec:?} must be PATH@NAME")))?;
    if name.is_empty() {
        return Err(Error::usage(format!(
            "HTML attribute spec {spec:?} has an empty name"
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
        if attr_name(source, attr)?.eq_ignore_ascii_case(name) {
            found = found.saturating_add(1);
            if first.is_none() {
                first = Some(a);
            }
        }
    }
    let index = first.ok_or_else(|| {
        Error::unsupported_feature(format!(
            "HTML element at path {path:?} has no attribute {name:?}"
        ))
    })?;
    Ok(ResolvedAttr {
        index,
        matches: found,
    })
}

/// The concatenation of all visible character data (text runs) in the subtree rooted
/// at `index`, in document order. Entity references are surfaced **literally**
/// (never expanded); raw `script`/`style` content is excluded.
pub fn subtree_text(model: &HtmlModel, source: &[u8], index: u32) -> Result<String> {
    let mut out = String::new();
    collect_text(model, source, index, &mut out)?;
    Ok(out)
}

fn collect_text(model: &HtmlModel, source: &[u8], index: u32, out: &mut String) -> Result<()> {
    let node = model
        .node(index)
        .ok_or_else(|| corrupt("text walk hit an out-of-range node"))?;
    if is_text(node.kind) {
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

/// The whole document's visible character data: every text run in document order.
/// Entity references are surfaced literally; raw `script`/`style` content is
/// excluded.
pub fn canonical_text(model: &HtmlModel, source: &[u8]) -> Result<String> {
    let mut out = String::new();
    for node in &model.nodes {
        if is_text(node.kind) {
            out.push_str(
                core::str::from_utf8(content_bytes(source, node)?)
                    .map_err(|_| corrupt("character data is not valid UTF-8"))?,
            );
        }
    }
    Ok(out)
}

/// A bounded, case-sensitive lexical search over element names, attribute names and
/// values, and text runs. Returns matches in document order.
pub fn find(
    model: &HtmlModel,
    source: &[u8],
    pattern: &str,
    limits: Limits,
) -> Result<Vec<HtmlMatch>> {
    let mut out = Vec::new();
    let mut path: Vec<String> = Vec::new();
    for &t in &model.top {
        walk_find(model, source, t, &mut path, pattern, &mut out, limits, 0)?;
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn walk_find(
    model: &HtmlModel,
    source: &[u8],
    index: u32,
    path: &mut Vec<String>,
    pattern: &str,
    out: &mut Vec<HtmlMatch>,
    limits: Limits,
    depth: u32,
) -> Result<()> {
    if depth > limits.max_html_depth {
        return Err(Error::resource_limit(
            "HTML find exceeded the nesting-depth cap",
        ));
    }
    let node = model
        .node(index)
        .ok_or_else(|| corrupt("find hit an out-of-range node"))?;
    let here = pointer_of(path);
    if node.kind != K_ELEMENT {
        if is_text(node.kind) {
            let content = core::str::from_utf8(content_bytes(source, node)?)
                .map_err(|_| corrupt("character data is not valid UTF-8"))?;
            if content.contains(pattern) {
                out.push(HtmlMatch {
                    path: here.clone(),
                    role: MatchRole::Text,
                    start: node.name_start,
                    end: node.name_end,
                    text: content.to_string(),
                });
            }
        }
        return Ok(());
    }
    let name = element_name(source, node)?;
    if name.contains(pattern) {
        out.push(HtmlMatch {
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
            out.push(HtmlMatch {
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
            out.push(HtmlMatch {
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
            let cname = element_name(source, cn)?;
            // 1-based ordinal among preceding element siblings with this name.
            let mut ordinal = 1u32;
            for &x in node.children.iter() {
                if x == c {
                    break;
                }
                let xn = model
                    .node(x)
                    .ok_or_else(|| corrupt("child index is out of range"))?;
                if xn.kind == K_ELEMENT && element_name(source, xn)?.eq_ignore_ascii_case(cname) {
                    ordinal += 1;
                }
            }
            path.push(format!("{cname}[{ordinal}]"));
            walk_find(model, source, c, path, pattern, out, limits, depth + 1)?;
            path.pop();
        } else {
            walk_find(model, source, c, path, pattern, out, limits, depth + 1)?;
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
// The bounded, error-recovering scanner
// ---------------------------------------------------------------------------

fn corrupt(msg: &str) -> Error {
    Error::invalid_html_structure(format!("malformed HTML: {msg}"))
}

fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n' | 0x0C)
}

fn is_tag_name_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b':' | b'.')
}

fn is_attr_name_start(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b':' | b'.' | b'@')
}

fn is_attr_name_char(c: u8) -> bool {
    !is_ws(c) && !matches!(c, b'=' | b'>' | b'/' | b'<' | b'"' | b'\'')
}

fn to_ascii_lowercase(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().map(|b| b.to_ascii_lowercase()).collect()
}

fn find_ci(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > hay.len() {
        return None;
    }
    for i in 0..=(hay.len() - needle.len()) {
        if hay[i..i + needle.len()].eq_ignore_ascii_case(needle) {
            return Some(i);
        }
    }
    None
}

struct Open {
    node: u32,
    name_start: u64,
    name_end: u64,
}

struct Parser<'a> {
    b: &'a [u8],
    at: usize,
    limits: Limits,
    build: bool,
    nodes: Vec<HNode>,
    attrs: Vec<HAttr>,
    top: Vec<u32>,
    nodes_count: u64,
    attrs_count: u64,
    text_bytes: u64,
    script_bytes: u64,
    max_depth_seen: u32,
}

impl<'a> Parser<'a> {
    fn new(b: &'a [u8], limits: Limits, build: bool) -> Self {
        Parser {
            b,
            at: 0,
            limits,
            build,
            nodes: Vec::new(),
            attrs: Vec::new(),
            top: Vec::new(),
            nodes_count: 0,
            attrs_count: 0,
            text_bytes: 0,
            script_bytes: 0,
            max_depth_seen: 0,
        }
    }

    fn byte(&self) -> Option<u8> {
        self.b.get(self.at).copied()
    }

    fn starts_with(&self, pat: &[u8]) -> bool {
        self.b.get(self.at..).is_some_and(|s| s.starts_with(pat))
    }

    fn starts_with_ci(&self, pat: &[u8]) -> bool {
        self.b
            .get(self.at..)
            .is_some_and(|s| s.len() >= pat.len() && s[..pat.len()].eq_ignore_ascii_case(pat))
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

    fn name_eq(&self, open: &Open, lower: &[u8]) -> bool {
        self.b
            .get(open.name_start as usize..open.name_end as usize)
            .is_some_and(|n| n.eq_ignore_ascii_case(lower))
    }

    fn name_lower(&self, open: &Open) -> Vec<u8> {
        to_ascii_lowercase(
            self.b
                .get(open.name_start as usize..open.name_end as usize)
                .unwrap_or(&[]),
        )
    }

    fn push_node(&mut self, node: HNode) -> Result<u32> {
        // The node budget is charged even when the arenas are not built, so
        // detection declines an over-large document identically.
        self.nodes_count = self.nodes_count.saturating_add(1);
        if self.nodes_count > self.limits.max_html_nodes as u64 {
            return Err(Error::resource_limit("HTML node bound exceeded"));
        }
        if !self.build {
            return Ok(0);
        }
        if self.nodes.len() as u64 >= u64::from(MAX_MODEL_NODES) {
            return Err(Error::resource_limit("HTML model node arena is full"));
        }
        let idx = self.nodes.len() as u32;
        self.nodes.push(node);
        Ok(idx)
    }

    fn push_attr(&mut self, attr: HAttr) -> Result<u32> {
        self.attrs_count = self.attrs_count.saturating_add(1);
        if self.attrs_count > self.limits.max_html_attrs {
            return Err(Error::resource_limit(
                "HTML document has too many attributes",
            ));
        }
        if !self.build {
            return Ok(0);
        }
        if self.attrs.len() as u64 >= u64::from(MAX_MODEL_ATTRS) {
            return Err(Error::resource_limit("HTML model attribute arena is full"));
        }
        let idx = self.attrs.len() as u32;
        self.attrs.push(attr);
        Ok(idx)
    }

    fn attach(&mut self, stack: &[Open], child: u32) {
        if !self.build {
            return;
        }
        if let Some(top) = stack.last() {
            if let Some(n) = self.nodes.get_mut(top.node as usize) {
                n.children.push(child);
            }
        } else {
            self.top.push(child);
        }
    }

    fn finalize(&mut self, node: u32, end: u64, close_start: u64) {
        if !self.build {
            return;
        }
        if let Some(n) = self.nodes.get_mut(node as usize) {
            n.end = end;
            n.close_start = close_start;
        }
    }

    fn auto_close(&mut self, stack: &mut Vec<Open>, tag_start: u64, new_lower: &[u8]) {
        while let Some(open) = stack.last() {
            let top_lower = self.name_lower(open);
            if auto_closes(&top_lower, new_lower) {
                let node = open.node;
                self.finalize(node, tag_start, 0);
                stack.pop();
            } else {
                break;
            }
        }
    }

    fn parse_document(&mut self) -> Result<()> {
        let mut stack: Vec<Open> = Vec::new();
        loop {
            if self.at >= self.b.len() {
                break;
            }
            if self.byte() == Some(b'<') {
                if self.starts_with(b"<!--") {
                    let n = self.parse_comment()?;
                    self.attach(&stack, n);
                } else if self.starts_with_ci(b"<!doctype") {
                    let n = self.parse_doctype()?;
                    self.attach(&stack, n);
                } else if self.starts_with(b"<!") || self.starts_with(b"<?") {
                    let n = self.parse_bogus_comment()?;
                    self.attach(&stack, n);
                } else if self.starts_with(b"</") {
                    self.parse_end_tag(&mut stack)?;
                } else if self
                    .b
                    .get(self.at + 1)
                    .is_some_and(|c| c.is_ascii_alphabetic())
                {
                    self.parse_start_tag(&mut stack)?;
                } else {
                    let n = self.parse_text()?;
                    self.attach(&stack, n);
                }
            } else {
                let n = self.parse_text()?;
                self.attach(&stack, n);
            }
        }
        // Close every remaining open element at end of input (recovery).
        let end = self.b.len() as u64;
        for open in stack.iter() {
            let node = open.node;
            self.finalize(node, end, 0);
        }
        Ok(())
    }

    fn parse_text(&mut self) -> Result<u32> {
        let start = self.at;
        if self.byte() == Some(b'<') {
            self.at += 1;
        }
        while let Some(c) = self.byte() {
            if c == b'<' {
                break;
            }
            self.at += 1;
        }
        let end = self.at;
        self.text_bytes = self.text_bytes.saturating_add((end - start) as u64);
        if self.text_bytes > self.limits.max_html_text_bytes {
            return Err(Error::resource_limit("HTML text bound exceeded"));
        }
        self.push_node(HNode {
            kind: K_TEXT,
            start: start as u64,
            end: end as u64,
            name_start: start as u64,
            name_end: end as u64,
            open_end: 0,
            close_start: 0,
            children: Vec::new(),
            attrs: Vec::new(),
        })
    }

    fn parse_comment(&mut self) -> Result<u32> {
        let start = self.at;
        self.at += 4; // <!--
        let content_start = self.at;
        let (content_end, end) = match self.find(self.at, b"-->") {
            Some(i) => {
                self.at = i + 3;
                (i, self.at)
            }
            None => {
                self.at = self.b.len();
                (self.b.len(), self.b.len())
            }
        };
        self.push_node(HNode {
            kind: K_COMMENT,
            start: start as u64,
            end: end as u64,
            name_start: content_start as u64,
            name_end: content_end as u64,
            open_end: 0,
            close_start: 0,
            children: Vec::new(),
            attrs: Vec::new(),
        })
    }

    fn parse_bogus_comment(&mut self) -> Result<u32> {
        let start = self.at;
        self.at += 2; // <! or <?
        let content_start = self.at;
        let (content_end, end) = match self.find(self.at, b">") {
            Some(i) => {
                self.at = i + 1;
                (i, self.at)
            }
            None => {
                self.at = self.b.len();
                (self.b.len(), self.b.len())
            }
        };
        self.push_node(HNode {
            kind: K_COMMENT,
            start: start as u64,
            end: end as u64,
            name_start: content_start as u64,
            name_end: content_end as u64,
            open_end: 0,
            close_start: 0,
            children: Vec::new(),
            attrs: Vec::new(),
        })
    }

    fn parse_doctype(&mut self) -> Result<u32> {
        let start = self.at;
        self.at += 9; // <!DOCTYPE
        let content_start = self.at;
        let mut internal_subset = false;
        let mut endpos = self.b.len();
        while let Some(c) = self.byte() {
            if c == b'[' {
                // An internal subset can declare entities: refuse it (no expansion
                // surface), mirroring the shared bounded-XML policy.
                internal_subset = true;
            }
            if c == b'>' {
                self.at += 1;
                endpos = self.at;
                break;
            }
            self.at += 1;
        }
        if internal_subset {
            return Err(Error::invalid_html_structure(
                "DOCTYPE with an internal subset is forbidden",
            ));
        }
        self.push_node(HNode {
            kind: K_DOCTYPE,
            start: start as u64,
            end: endpos as u64,
            name_start: content_start as u64,
            name_end: endpos as u64,
            open_end: 0,
            close_start: 0,
            children: Vec::new(),
            attrs: Vec::new(),
        })
    }

    fn parse_start_tag(&mut self, stack: &mut Vec<Open>) -> Result<()> {
        let tag_start = self.at;
        self.at += 1; // '<'
        let name_start = self.at;
        self.parse_tag_name();
        let name_end = self.at;
        let name_lower = to_ascii_lowercase(&self.b[name_start..name_end]);
        let mut attr_entries: Vec<HAttr> = Vec::new();
        let self_closing;
        let open_end;
        loop {
            self.skip_ws();
            match self.byte() {
                Some(b'>') => {
                    self.at += 1;
                    open_end = self.at as u64;
                    self_closing = false;
                    break;
                }
                Some(b'/') => {
                    if self.b.get(self.at + 1) == Some(&b'>') {
                        self.at += 2;
                        open_end = self.at as u64;
                        self_closing = true;
                        break;
                    }
                    self.at += 1; // stray '/'
                }
                None => {
                    // EOF inside a start tag: recover as self-closing.
                    open_end = self.b.len() as u64;
                    self_closing = true;
                    break;
                }
                Some(c) if is_attr_name_start(c) => {
                    if self.attrs_len_plus(attr_entries.len()) {
                        return Err(Error::resource_limit(
                            "HTML document has too many attributes",
                        ));
                    }
                    attr_entries.push(self.parse_attribute());
                }
                Some(_) => {
                    self.at += 1; // junk byte inside a start tag
                }
            }
        }
        let mut attrs = Vec::with_capacity(attr_entries.len());
        for a in attr_entries {
            attrs.push(self.push_attr(a)?);
        }
        let idx = self.push_node(HNode {
            kind: K_ELEMENT,
            start: tag_start as u64,
            end: open_end,
            name_start: name_start as u64,
            name_end: name_end as u64,
            open_end,
            close_start: 0,
            children: Vec::new(),
            attrs,
        })?;
        if self_closing || is_void_element(&name_lower) {
            self.attach(stack, idx);
            return Ok(());
        }
        // Implicitly close any open element this start tag closes.
        self.auto_close(stack, tag_start as u64, &name_lower);
        let depth = (stack.len() as u32).saturating_add(1);
        if depth > self.limits.max_html_depth {
            return Err(Error::resource_limit(format!(
                "HTML nesting exceeds the {}-level cap",
                self.limits.max_html_depth
            )));
        }
        self.max_depth_seen = self.max_depth_seen.max(depth);
        self.attach(stack, idx);
        stack.push(Open {
            node: idx,
            name_start: name_start as u64,
            name_end: name_end as u64,
        });
        // Raw-text elements: capture their content as raw bytes, never parsed.
        if name_lower == b"script" || name_lower == b"style" {
            let raw = self.parse_raw_text(&name_lower)?;
            if let Some(n) = self.nodes.get_mut(idx as usize) {
                n.children.push(raw);
            }
        }
        Ok(())
    }

    fn attrs_len_plus(&self, pending: usize) -> bool {
        let total = self.attrs_count.saturating_add(pending as u64);
        total >= self.limits.max_html_attrs
    }

    fn parse_raw_text(&mut self, name_lower: &[u8]) -> Result<u32> {
        let start = self.at;
        let content_end = match self.find_close_tag(name_lower, self.at) {
            Some(i) => i,
            None => self.b.len(),
        };
        self.script_bytes = self
            .script_bytes
            .saturating_add((content_end - start) as u64);
        if self.script_bytes > self.limits.max_html_script_bytes {
            return Err(Error::resource_limit(
                "HTML raw script/style content bound exceeded",
            ));
        }
        self.at = content_end;
        self.push_node(HNode {
            kind: K_RAW_TEXT,
            start: start as u64,
            end: content_end as u64,
            name_start: start as u64,
            name_end: content_end as u64,
            open_end: 0,
            close_start: 0,
            children: Vec::new(),
            attrs: Vec::new(),
        })
    }

    /// Find the next `</name` (case-insensitive) followed by whitespace, `/`, or `>`
    /// from `from`.
    fn find_close_tag(&self, name_lower: &[u8], from: usize) -> Option<usize> {
        let hay = self.b.get(from..)?;
        let mut i = 0usize;
        while i + 2 + name_lower.len() <= hay.len() {
            if hay[i] == b'<' && hay[i + 1] == b'/' {
                let s = i + 2;
                let e = s + name_lower.len();
                if hay[s..e].eq_ignore_ascii_case(name_lower) {
                    match hay.get(e) {
                        Some(&c) if c == b'>' || c == b'/' || is_ws(c) => return Some(from + i),
                        None => return Some(from + i),
                        _ => {}
                    }
                }
            }
            i += 1;
        }
        None
    }

    fn parse_end_tag(&mut self, stack: &mut Vec<Open>) -> Result<()> {
        let tag_start = self.at;
        self.at += 2; // '</'
        self.skip_ws();
        let name_start = self.at;
        self.parse_tag_name();
        let name_end = self.at;
        let end = match self.find(self.at, b">") {
            Some(i) => {
                self.at = i + 1;
                self.at as u64
            }
            None => {
                self.at = self.b.len();
                self.b.len() as u64
            }
        };
        if name_end <= name_start {
            return Ok(()); // junk end tag: ignore
        }
        let name_lower = to_ascii_lowercase(&self.b[name_start..name_end]);
        // Find the nearest matching open element.
        let mut matched = None;
        for j in (0..stack.len()).rev() {
            if self.name_eq(&stack[j], &name_lower) {
                matched = Some(j);
                break;
            }
        }
        if let Some(k) = matched {
            // Implicitly close everything deeper.
            let deeper: Vec<u32> = stack[k + 1..].iter().map(|o| o.node).collect();
            for node in deeper {
                self.finalize(node, tag_start as u64, 0);
            }
            let node = stack[k].node;
            self.finalize(node, end, tag_start as u64);
            stack.truncate(k);
        }
        // A stray end tag with no matching open element is ignored (recovery).
        Ok(())
    }

    fn parse_tag_name(&mut self) {
        while let Some(c) = self.byte() {
            if is_tag_name_char(c) {
                self.at += 1;
            } else {
                break;
            }
        }
    }

    fn parse_attribute(&mut self) -> HAttr {
        let span_start = self.at as u64;
        let name_start = self.at as u64;
        while let Some(c) = self.byte() {
            if is_attr_name_char(c) {
                self.at += 1;
            } else {
                break;
            }
        }
        let name_end = self.at as u64;
        self.skip_ws();
        if self.byte() == Some(b'=') {
            self.at += 1;
            self.skip_ws();
            match self.byte() {
                Some(q @ (b'"' | b'\'')) => {
                    self.at += 1;
                    let value_start = self.at as u64;
                    while let Some(c) = self.byte() {
                        if c == q {
                            break;
                        }
                        self.at += 1;
                    }
                    let value_end = self.at as u64;
                    if self.byte() == Some(q) {
                        self.at += 1;
                    }
                    HAttr {
                        name_start,
                        name_end,
                        value_start,
                        value_end,
                        span_start,
                        span_end: self.at as u64,
                        quote: if q == b'"' { Q_DOUBLE } else { Q_SINGLE },
                    }
                }
                Some(b'>') | None => {
                    // '=' with no value: recover as an empty value.
                    let value_start = self.at as u64;
                    HAttr {
                        name_start,
                        name_end,
                        value_start,
                        value_end: value_start,
                        span_start,
                        span_end: self.at as u64,
                        quote: Q_UNQUOTED,
                    }
                }
                Some(_) => {
                    let value_start = self.at as u64;
                    while let Some(c) = self.byte() {
                        if is_ws(c) || c == b'>' || c == b'<' {
                            break;
                        }
                        self.at += 1;
                    }
                    let value_end = self.at as u64;
                    HAttr {
                        name_start,
                        name_end,
                        value_start,
                        value_end,
                        span_start,
                        span_end: value_end,
                        quote: Q_UNQUOTED,
                    }
                }
            }
        } else {
            // A boolean attribute (no '=').
            HAttr {
                name_start,
                name_end,
                value_start: name_end,
                value_end: name_end,
                span_start,
                span_end: name_end,
                quote: Q_NONE,
            }
        }
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

    fn model(src: &[u8]) -> HtmlModel {
        parse(src, Limits::DEFAULT, true).unwrap()
    }

    #[test]
    fn detects_html_and_rejects_junk() {
        assert!(detect(
            b"<!DOCTYPE html><html><body><br></body></html>",
            Limits::DEFAULT
        ));
        assert!(detect(
            b"<html><body><p>x</p></body></html>",
            Limits::DEFAULT
        ));
        assert!(detect(
            b"<div><span>a</span><p>b</p></div>",
            Limits::DEFAULT
        ));
        assert!(!detect(b"plain prose paragraph", Limits::DEFAULT));
        assert!(!detect(b"<<< not html >>> <a <b> <<", Limits::DEFAULT));
        assert!(!detect(
            b"#include <stdio.h>\nint main(){}\n",
            Limits::DEFAULT
        ));
        // Plain text with one known tag is not enough (preponderance).
        assert!(!detect(b"just text mentioning <p> once", Limits::DEFAULT));
        // A DOCTYPE internal subset is declined -> not HTML.
        assert!(!detect(
            b"<!DOCTYPE html [<!ENTITY x \"y\">]><html><body></body></html>",
            Limits::DEFAULT
        ));
    }

    #[test]
    fn preserves_spans_attributes_and_raw_text() {
        let src: &[u8] =
            b"<!DOCTYPE html><html><head><title>T</title><style>a{color:red}</style></head>\
<body id=\"main\" data-x='1' hidden><p>hi<br>there</p><!-- c -->\
<script>var x = 1 < 2 && 3 > 2;</script></body></html>";
        let m = model(src);
        assert_eq!(element_name(src, m.node(m.root).unwrap()).unwrap(), "html");
        // Attribute quoting styles are preserved and recoverable.
        let ra = resolve_attr(&m, src, "/html/body@data-x").unwrap();
        let a = m.attr(ra.index).unwrap();
        assert_eq!(a.quote, Q_SINGLE);
        assert_eq!(
            &src[a.span_start as usize..a.span_end as usize],
            b"data-x='1'"
        );
        let rb = resolve_attr(&m, src, "/html/body@hidden").unwrap();
        assert_eq!(m.attr(rb.index).unwrap().quote, Q_NONE);
        // A void element (`<br>`) has no end tag.
        let br = m
            .nodes
            .iter()
            .find(|n| n.kind == K_ELEMENT && element_name(src, n).unwrap() == "br")
            .unwrap();
        assert_eq!(br.close_start, 0);
        assert_eq!(&src[br.start as usize..br.end as usize], b"<br>");
        // Raw script/style content is captured verbatim (never parsed/executed).
        let raws = raw_texts(&m, src).unwrap();
        assert_eq!(raws.len(), 2);
        assert_eq!(raws[0].name, "style");
        assert_eq!(
            &src[raws[0].start as usize..raws[0].end as usize],
            b"a{color:red}"
        );
        assert_eq!(raws[1].name, "script");
        assert_eq!(
            &src[raws[1].start as usize..raws[1].end as usize],
            b"var x = 1 < 2 && 3 > 2;"
        );
        // The comment keeps its exact delimited span.
        let comment = m.nodes.iter().find(|n| n.kind == K_COMMENT).unwrap();
        assert_eq!(token_bytes(src, comment).unwrap(), b"<!-- c -->");
        // Visible text excludes raw script/style content.
        let text = canonical_text(&m, src).unwrap();
        assert!(text.contains("hi"));
        assert!(text.contains("there"));
        assert!(!text.contains("color"));
    }

    #[test]
    fn error_recovery_implicit_close_and_stray_end_tags() {
        // Implicit `<li>` closing.
        let src: &[u8] = b"<ul><li>a<li>b<li>c</ul>";
        let m = model(src);
        assert_eq!(resolve_path(&m, src, "/ul/li[1]").unwrap().matches, 3);
        assert_eq!(
            subtree_text(&m, src, resolve_path(&m, src, "/ul/li[2]").unwrap().index).unwrap(),
            "b"
        );
        // A stray end tag is ignored; unclosed elements recover.
        let src2: &[u8] = b"<div><p>x</span>y";
        let m2 = model(src2);
        assert_eq!(subtree_text(&m2, src2, m2.root).unwrap(), "xy");
        // Unquoted and malformed attribute values do not panic.
        let src3: &[u8] = b"<a href=http://x/?a=1&b=2 disabled>x</a>";
        let m3 = model(src3);
        let r = resolve_attr(&m3, src3, "/a@href").unwrap();
        assert_eq!(
            attr_value_bytes(src3, m3.attr(r.index).unwrap()).unwrap(),
            b"http://x/?a=1&b=2"
        );
    }

    #[test]
    fn entities_are_surfaced_literally() {
        let src: &[u8] = b"<html><body><p>a &amp; b &nbsp; c</p></body></html>";
        let m = model(src);
        let text = canonical_text(&m, src).unwrap();
        assert_eq!(text, "a &amp; b &nbsp; c");
    }

    #[test]
    fn heading_and_anchor_indexing() {
        let src: &[u8] =
            b"<html><body><h1>x</h1><h2>y</h2><a href=\"http://a\">one</a><a>two</a></body></html>";
        let m = model(src);
        let hs = headings(&m, src).unwrap();
        assert_eq!(hs.len(), 2);
        assert_eq!(hs[0].1, 1);
        assert_eq!(hs[1].1, 2);
        // Only anchors with an href are links.
        assert_eq!(anchors(&m, src).unwrap().len(), 1);
    }

    #[test]
    fn deep_nesting_declines_typed() {
        let mut src = Vec::new();
        src.extend_from_slice(b"<html>");
        for _ in 0..200 {
            src.extend_from_slice(b"<div>");
        }
        src.extend_from_slice(b"x");
        for _ in 0..200 {
            src.extend_from_slice(b"</div>");
        }
        src.extend_from_slice(b"</html>");
        assert!(!detect(&src, Limits::STRICT));
        let e = parse(&src, Limits::STRICT, true).unwrap_err();
        assert_eq!(e.class(), crate::error::ErrorClass::ResourceLimit);
    }

    #[test]
    fn doctype_internal_subset_declines_typed() {
        let src: &[u8] = b"<!DOCTYPE html [<!ENTITY x \"y\">]><html></html>";
        let e = parse(src, Limits::DEFAULT, true).unwrap_err();
        assert_eq!(e.class(), crate::error::ErrorClass::InvalidHtmlStructure);
    }

    #[test]
    fn resource_bounds_decline_typed() {
        let l_attrs = Limits {
            max_html_attrs: 1,
            ..Limits::DEFAULT
        };
        let e = parse(b"<html><body a=\"1\" b=\"2\"></body></html>", l_attrs, true).unwrap_err();
        assert_eq!(e.class(), crate::error::ErrorClass::ResourceLimit);

        let l_text = Limits {
            max_html_text_bytes: 2,
            ..Limits::DEFAULT
        };
        let e = parse(b"<html><body>hello</body></html>", l_text, true).unwrap_err();
        assert_eq!(e.class(), crate::error::ErrorClass::ResourceLimit);

        let l_script = Limits {
            max_html_script_bytes: 2,
            ..Limits::DEFAULT
        };
        let e = parse(
            b"<html><body><script>abcdef</script></body></html>",
            l_script,
            true,
        )
        .unwrap_err();
        assert_eq!(e.class(), crate::error::ErrorClass::ResourceLimit);

        let l_nodes = Limits {
            max_html_nodes: 1,
            ..Limits::DEFAULT
        };
        let e = parse(b"<html><body>x</body></html>", l_nodes, true).unwrap_err();
        assert_eq!(e.class(), crate::error::ErrorClass::ResourceLimit);
        // The node bound is charged even in detection mode.
        assert!(!detect(b"<html><body>x</body></html>", l_nodes));
    }

    #[test]
    fn model_roundtrips() {
        let src: &[u8] = b"<html a=\"1\" b='2' c=3 d><body><br><p>x</p></body></html>";
        let m = model(src);
        let bytes = m.encode();
        assert_eq!(HtmlModel::decode(&bytes).unwrap(), m);
        for cut in 0..bytes.len() {
            let _ = HtmlModel::decode(&bytes[..cut]);
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
                let _ = raw_texts(&m, &buf);
                let _ = headings(&m, &buf);
                let _ = anchors(&m, &buf);
            }
        }
    }
}
