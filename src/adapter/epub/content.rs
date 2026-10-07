//! Bounded XHTML content observations for one EPUB spine item (Phase 12.6,
//! ADR-0033).
//!
//! A spine item's content document is XHTML5 (HTML5 in XML serialisation). This
//! module parses the **exact decoded member bytes** with the shared bounded-XML
//! policy into a canonical, derived ([`ContentModel`]) serialization: headings,
//! paragraphs, lists, tables (with `colspan`/`rowspan`), links (fragment /
//! intra-container / external-inert), images/resources, fragment ids, and semantic
//! sections, all in document order.
//!
//! Everything here is **derived (`Q_gen`) state**: exact authority remains the
//! ZIP member raw span, and a hostile or malformed document is still an exact
//! archival object — only the *derived* observation declines, typed.
//!
//! ## Security (research D §8, plan §12)
//!
//! No script execution, no remote fetch, no CSS/SVG engine. `script`/`style`
//! bodies are treated as data and never contribute reading text; an external
//! target is an inert string. `<!DOCTYPE` is refused outright (no DTD, no entity
//! expansion), and depth / node / attribute / text counts are bounded by
//! [`crate::limits::Limits`].
//!
//! ## Honest limits
//!
//! This is a **structured-line** view, not a browser render and not a page image.
//! There is no layout, no visual fidelity, and no computed list numbering. A cell
//! is addressed by its **physical** `(row, cell)` position within its `<tr>`; a
//! logical grid under `colspan`/`rowspan` is an author-dependent projection and is
//! not synthesised.

use quick_xml::Reader;
use quick_xml::events::{BytesRef, Event};

use crate::error::{Error, Result};
use crate::limits::Limits;

use crate::adapter::package::xml::{XmlState, read_attrs_qualified};

use super::{
    BinReader, EpubExtractProfile, accept_doctype, classify_href, corrupt, entity_ref_text,
    harden_xml, put_opt_str, put_str, put_u32, read_strs, xml_err,
};

const CONTENT_MAGIC: &[u8; 4] = b"EPCT";
const CONTENT_VERSION: u8 = 1;

/// Maximum table nesting the parser will descend.
const MAX_TABLE_NESTING: usize = 8;
/// Maximum nested inline link depth the parser will capture.
const MAX_LINK_NESTING: usize = 8;

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

/// One authored table cell (its **physical** position within its row's cell list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// The cell element is a header (`th`).
    pub header: bool,
    /// The cell's normalized text.
    pub text: String,
    /// The declared `colspan` (clamped to `>= 1`).
    pub colspan: u32,
    /// The declared `rowspan` (clamped to `>= 1`).
    pub rowspan: u32,
}

/// One table row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRow {
    /// The row's cells, in document order.
    pub cells: Vec<Cell>,
}

/// A block in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// A heading `h1`–`h6`.
    Heading {
        /// The heading level (1–6).
        level: u8,
        /// The heading's `id`, when present.
        id: Option<String>,
        /// The `epub:type` semantic, when present.
        epub_type: Option<String>,
        /// The heading text.
        text: String,
    },
    /// A paragraph-like block (`p`, `blockquote`, `pre`, `figcaption`).
    Paragraph {
        /// The paragraph text.
        text: String,
    },
    /// An ordered/unordered list (`ul`/`ol`).
    List {
        /// Whether the list is ordered (`ol`).
        ordered: bool,
        /// The list items, in document order.
        items: Vec<String>,
    },
    /// A table (`table`).
    Table {
        /// The table rows.
        rows: Vec<TableRow>,
    },
}

impl Block {
    /// Stable lower-case kind name.
    pub const fn kind(&self) -> &'static str {
        match self {
            Block::Heading { .. } => "heading",
            Block::Paragraph { .. } => "paragraph",
            Block::List { .. } => "list",
            Block::Table { .. } => "table",
        }
    }

    /// The block's reading text (cells joined by a tab, rows by a newline).
    pub fn text(&self) -> String {
        match self {
            Block::Heading { text, .. } | Block::Paragraph { text } => text.clone(),
            Block::List { items, .. } => items.join("\n"),
            Block::Table { rows } => rows
                .iter()
                .map(|r| {
                    r.cells
                        .iter()
                        .map(|c| c.text.as_str())
                        .collect::<Vec<_>>()
                        .join("\t")
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

/// A link (`a/@href`) classified against the container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// `href` exactly as written.
    pub href: String,
    /// The anchor's text.
    pub text: String,
    /// The fragment (after `#`), when present.
    pub fragment: Option<String>,
    /// The resolved internal container member (empty for a pure fragment).
    pub member: Option<String>,
    /// The href is an absolute URI: inert, never fetched.
    pub external: bool,
    /// The anchor's `epub:type`, when present.
    pub epub_type: Option<String>,
}

/// A resource reference (`img`, SVG `image`/`use`, `object`, `source`, media).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    /// The element-local resource class (`img`, `image`, `object`, …).
    pub kind: String,
    /// The attribute the reference came from (`src`, `href`, `data`, `srcset`).
    pub attr: String,
    /// The reference value exactly as written.
    pub value: String,
    /// The resolved internal container member, when internal.
    pub member: Option<String>,
    /// The target is an absolute URI: inert, never fetched.
    pub external: bool,
}

/// A semantic section (`section`/`article`/`aside`/`nav`/`figure`/…).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// The element local name.
    pub local: String,
    /// The `epub:type` semantic, when present.
    pub epub_type: Option<String>,
    /// Element depth where the section started (1-based).
    pub depth: u32,
}

/// The canonical, derived content model of one XHTML content document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentModel {
    /// The root element's local name (usually `html`).
    pub root_local: String,
    /// A `<body>` element was seen.
    pub body_seen: bool,
    /// A `<script>` element was seen; scripted content is never executed.
    pub scripted: bool,
    /// Blocks in document order.
    pub blocks: Vec<Block>,
    /// Links in document order.
    pub links: Vec<Link>,
    /// Resource references in document order.
    pub resources: Vec<Resource>,
    /// Fragment ids (`id`/`name` attributes) in document order.
    pub fragments: Vec<String>,
    /// Semantic sections in document order.
    pub sections: Vec<Section>,
    /// Element nodes scanned while building this model (the bounded work counter).
    pub xhtml_nodes: u64,
}

impl ContentModel {
    /// The full reading text of the document, one block per line.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for b in &self.blocks {
            let t = b.text();
            if !t.is_empty() {
                out.push_str(&t);
                out.push('\n');
            }
        }
        out
    }

    /// The `n`-th table (`Block::Table`) in document order.
    pub fn table(&self, n: u32) -> Option<&Block> {
        self.blocks
            .iter()
            .filter(|b| matches!(b, Block::Table { .. }))
            .nth(n as usize)
    }

    /// A canonical, honest structured preview. It is a **line-oriented structural
    /// summary**, never a page render: no layout and no visual fidelity are
    /// claimed.
    pub fn preview_text(&self, index: u32, part: &str, profile: &str) -> String {
        let mut out = String::new();
        out.push_str("VOLE-EPUB-PREVIEW v1\n");
        out.push_str(&format!("spine {index}\n"));
        out.push_str(&format!("part {part}\n"));
        out.push_str(&format!("profile {profile}\n"));
        out.push_str("basis structured-line-preview; not a page render\n");
        out.push_str("--\n");
        for (i, b) in self.blocks.iter().enumerate() {
            match b {
                Block::Heading { level, text, .. } => {
                    out.push_str(&format!("H{level} [{i}] {text}\n"));
                }
                Block::Paragraph { text } => {
                    out.push_str(&format!("P [{i}] {text}\n"));
                }
                Block::List { ordered, items } => {
                    let mark = if *ordered { "OL" } else { "UL" };
                    out.push_str(&format!(
                        "{mark} [{i}] items={} {}\n",
                        items.len(),
                        items.join(" | ")
                    ));
                }
                Block::Table { rows } => {
                    let cols = rows.first().map_or(0, |r| r.cells.len());
                    out.push_str(&format!("TABLE [{i}] {}x{cols}\n", rows.len()));
                }
            }
        }
        for (i, r) in self.resources.iter().enumerate() {
            let loc = if r.external {
                "external-inert".to_string()
            } else {
                r.member.clone().unwrap_or_default()
            };
            out.push_str(&format!("RES [{i}] {} {loc}\n", r.kind));
        }
        for (i, l) in self.links.iter().enumerate() {
            let loc = if l.external {
                "external-inert".to_string()
            } else if let Some(f) = &l.fragment {
                format!("fragment={f}")
            } else {
                l.member.clone().unwrap_or_default()
            };
            out.push_str(&format!("LINK [{i}] {loc}\n"));
        }
        out
    }

    /// Deterministically encode the model (length-prefixed little-endian).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(CONTENT_MAGIC);
        out.push(CONTENT_VERSION);
        put_str(&mut out, &self.root_local);
        out.push(self.body_seen as u8);
        out.push(self.scripted as u8);

        put_u32(&mut out, self.blocks.len() as u32);
        for b in &self.blocks {
            match b {
                Block::Heading {
                    level,
                    id,
                    epub_type,
                    text,
                } => {
                    out.push(0);
                    out.push(*level);
                    put_opt_str(&mut out, id.as_deref());
                    put_opt_str(&mut out, epub_type.as_deref());
                    put_str(&mut out, text);
                }
                Block::Paragraph { text } => {
                    out.push(1);
                    put_str(&mut out, text);
                }
                Block::List { ordered, items } => {
                    out.push(2);
                    out.push(*ordered as u8);
                    put_u32(&mut out, items.len() as u32);
                    for it in items {
                        put_str(&mut out, it);
                    }
                }
                Block::Table { rows } => {
                    out.push(3);
                    put_u32(&mut out, rows.len() as u32);
                    for r in rows {
                        put_u32(&mut out, r.cells.len() as u32);
                        for c in &r.cells {
                            out.push(c.header as u8);
                            put_u32(&mut out, c.colspan);
                            put_u32(&mut out, c.rowspan);
                            put_str(&mut out, &c.text);
                        }
                    }
                }
            }
        }

        put_u32(&mut out, self.links.len() as u32);
        for l in &self.links {
            put_str(&mut out, &l.href);
            put_str(&mut out, &l.text);
            put_opt_str(&mut out, l.fragment.as_deref());
            put_opt_str(&mut out, l.member.as_deref());
            out.push(l.external as u8);
            put_opt_str(&mut out, l.epub_type.as_deref());
        }

        put_u32(&mut out, self.resources.len() as u32);
        for r in &self.resources {
            put_str(&mut out, &r.kind);
            put_str(&mut out, &r.attr);
            put_str(&mut out, &r.value);
            put_opt_str(&mut out, r.member.as_deref());
            out.push(r.external as u8);
        }

        put_strs(&mut out, &self.fragments);

        put_u32(&mut out, self.sections.len() as u32);
        for s in &self.sections {
            put_str(&mut out, &s.local);
            put_opt_str(&mut out, s.epub_type.as_deref());
            put_u32(&mut out, s.depth);
        }

        out.extend_from_slice(&self.xhtml_nodes.to_le_bytes());
        out
    }

    /// Decode a model produced by [`Self::encode`].
    pub fn decode(bytes: &[u8]) -> Result<ContentModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != CONTENT_MAGIC {
            return Err(corrupt("bad EPUB content model magic"));
        }
        if r.u8()? != CONTENT_VERSION {
            return Err(corrupt("unsupported EPUB content model version"));
        }
        let root_local = r.string()?;
        let body_seen = r.u8()? != 0;
        let scripted = r.u8()? != 0;

        let n = bounded(&mut r, "content block")?;
        let mut blocks = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let tag = r.u8()?;
            blocks.push(match tag {
                0 => {
                    let level = r.u8()?;
                    if !(1..=6).contains(&level) {
                        return Err(corrupt("heading level is out of range"));
                    }
                    Block::Heading {
                        level,
                        id: r.opt_string()?,
                        epub_type: r.opt_string()?,
                        text: r.string()?,
                    }
                }
                1 => Block::Paragraph { text: r.string()? },
                2 => {
                    let ordered = r.u8()? != 0;
                    let m = bounded(&mut r, "list item")?;
                    let mut items = Vec::with_capacity(m as usize);
                    for _ in 0..m {
                        items.push(r.string()?);
                    }
                    Block::List { ordered, items }
                }
                3 => {
                    let rows_n = bounded(&mut r, "table row")?;
                    let mut rows = Vec::with_capacity(rows_n as usize);
                    for _ in 0..rows_n {
                        let cells_n = bounded(&mut r, "table cell")?;
                        let mut cells = Vec::with_capacity(cells_n as usize);
                        for _ in 0..cells_n {
                            let header = r.u8()? != 0;
                            let colspan = r.u32()?;
                            let rowspan = r.u32()?;
                            let text = r.string()?;
                            cells.push(Cell {
                                header,
                                text,
                                colspan,
                                rowspan,
                            });
                        }
                        rows.push(TableRow { cells });
                    }
                    Block::Table { rows }
                }
                _ => return Err(corrupt("unknown EPUB content block tag")),
            });
        }

        let n = bounded(&mut r, "content link")?;
        let mut links = Vec::with_capacity(n as usize);
        for _ in 0..n {
            links.push(Link {
                href: r.string()?,
                text: r.string()?,
                fragment: r.opt_string()?,
                member: r.opt_string()?,
                external: r.u8()? != 0,
                epub_type: r.opt_string()?,
            });
        }

        let n = bounded(&mut r, "content resource")?;
        let mut resources = Vec::with_capacity(n as usize);
        for _ in 0..n {
            resources.push(Resource {
                kind: r.string()?,
                attr: r.string()?,
                value: r.string()?,
                member: r.opt_string()?,
                external: r.u8()? != 0,
            });
        }

        let fragments = read_strs(&mut r)?;

        let n = bounded(&mut r, "content section")?;
        let mut sections = Vec::with_capacity(n as usize);
        for _ in 0..n {
            sections.push(Section {
                local: r.string()?,
                epub_type: r.opt_string()?,
                depth: r.u32()?,
            });
        }

        let xhtml_nodes = u64::from_le_bytes(
            r.bytes(8)?
                .try_into()
                .map_err(|_| corrupt("content node counter"))?,
        );

        if !r.at_end() {
            return Err(corrupt("EPUB content model has trailing bytes"));
        }
        Ok(ContentModel {
            root_local,
            body_seen,
            scripted,
            blocks,
            links,
            resources,
            fragments,
            sections,
            xhtml_nodes,
        })
    }
}

// ---------------------------------------------------------------------------
// Node parameters
// ---------------------------------------------------------------------------

/// Canonical parameters for an [`crate::field::node::NodeKind::EpubContent`] node:
/// `version(1) · spine index(4) · member ordinal(4) · profile(5) · len-prefixed base dir`.
pub fn content_params(
    spine_index: u32,
    ordinal: u32,
    base_dir: &str,
    profile: &EpubExtractProfile,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + base_dir.len());
    out.push(1);
    out.extend_from_slice(&spine_index.to_le_bytes());
    out.extend_from_slice(&ordinal.to_le_bytes());
    out.extend_from_slice(&profile.encode());
    let b = base_dir.as_bytes();
    out.extend_from_slice(&(b.len() as u32).to_le_bytes());
    out.extend_from_slice(b);
    out
}

/// Decode parameters produced by [`content_params`].
pub fn read_content_params(params: &[u8]) -> Result<(u32, u32, String, EpubExtractProfile)> {
    let mut r = BinReader::new(params);
    if r.u8()? != 1 {
        return Err(corrupt("unsupported EPUB content params version"));
    }
    let spine_index = r.u32()?;
    let ordinal = r.u32()?;
    let profile = EpubExtractProfile::decode(r.bytes(5)?)?;
    let base_dir = r.string()?;
    if !r.at_end() {
        return Err(corrupt("EPUB content params have trailing bytes"));
    }
    Ok((spine_index, ordinal, base_dir, profile))
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

fn bounded(r: &mut BinReader<'_>, what: &str) -> Result<u32> {
    let n = r.u32()?;
    if n as u64 > 1 << 24 {
        return Err(corrupt(&format!("EPUB {what} count is implausible")));
    }
    Ok(n)
}

fn put_strs(out: &mut Vec<u8>, items: &[String]) {
    put_u32(out, items.len() as u32);
    for s in items {
        put_str(out, s);
    }
}

/// Collapse runs of ASCII whitespace and trim (a deterministic reading text).
fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_u32(v: &str) -> Option<u32> {
    let t = v.trim();
    if t.is_empty() || !t.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    t.parse().ok()
}

fn local_part(qname: &str) -> &str {
    qname.rsplit(':').next().unwrap_or(qname)
}

fn attr<'a>(attrs: &'a [(String, String)], local: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(q, _)| local_part(q) == local)
        .map(|(_, v)| v.as_str())
}

fn qattr<'a>(attrs: &'a [(String, String)], qname: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(q, _)| q == qname)
        .map(|(_, v)| v.as_str())
}

/// The first URL token of a `srcset` value (comma/whitespace separated candidates).
fn first_srcset(v: &str) -> String {
    v.split(',')
        .next()
        .unwrap_or("")
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string()
}

/// The resource class and reference attribute for an element, when it is one.
fn resource_of(
    local: &str,
    attrs: &[(String, String)],
) -> Option<(&'static str, &'static str, String)> {
    match local {
        "img" => {
            if let Some(v) = attr(attrs, "src") {
                Some(("img", "src", v.to_string()))
            } else {
                attr(attrs, "srcset").map(|v| ("img", "srcset", first_srcset(v)))
            }
        }
        "image" | "use" => attr(attrs, "href")
            .or_else(|| attr(attrs, "src"))
            .map(|v| ("image", "href", v.to_string())),
        "object" => attr(attrs, "data").map(|v| ("object", "data", v.to_string())),
        "source" => {
            if let Some(v) = attr(attrs, "src") {
                Some(("source", "src", v.to_string()))
            } else {
                attr(attrs, "srcset").map(|v| ("source", "srcset", first_srcset(v)))
            }
        }
        "video" | "audio" | "iframe" | "embed" | "track" => {
            attr(attrs, "src").map(|v| ("media", "src", v.to_string()))
        }
        "link" => attr(attrs, "href").map(|v| ("link", "href", v.to_string())),
        _ => None,
    }
}

enum BlockCap {
    Heading {
        level: u8,
        id: Option<String>,
        epub_type: Option<String>,
        buf: String,
    },
    Paragraph {
        buf: String,
    },
    ListItem {
        buf: String,
    },
    Cell {
        header: bool,
        colspan: u32,
        rowspan: u32,
        buf: String,
    },
}

impl BlockCap {
    fn push(&mut self, s: &str) {
        match self {
            BlockCap::Heading { buf, .. }
            | BlockCap::Paragraph { buf }
            | BlockCap::ListItem { buf }
            | BlockCap::Cell { buf, .. } => buf.push_str(s),
        }
    }
}

struct LinkCap {
    href: String,
    epub_type: Option<String>,
    buf: String,
}

struct ListCtx {
    ordered: bool,
    items: Vec<String>,
}

struct TableCtx {
    rows: Vec<TableRow>,
    current_row: Option<Vec<Cell>>,
}

struct ContentParser<'a> {
    limits: Limits,
    base_dir: &'a str,
    st: XmlState,
    nodes: u64,
    elem_depth: u32,
    head_depth: u32,
    skip_depth: u32,
    root_local: String,
    body_seen: bool,
    scripted: bool,
    blocks: Vec<Block>,
    links: Vec<Link>,
    resources: Vec<Resource>,
    fragments: Vec<String>,
    sections: Vec<Section>,
    block_caps: Vec<BlockCap>,
    link_caps: Vec<LinkCap>,
    list_stack: Vec<ListCtx>,
    table_stack: Vec<TableCtx>,
}

impl<'a> ContentParser<'a> {
    fn new(limits: Limits, base_dir: &'a str) -> Self {
        ContentParser {
            limits,
            base_dir,
            st: XmlState::new(),
            nodes: 0,
            elem_depth: 0,
            head_depth: 0,
            skip_depth: 0,
            root_local: String::new(),
            body_seen: false,
            scripted: false,
            blocks: Vec::new(),
            links: Vec::new(),
            resources: Vec::new(),
            fragments: Vec::new(),
            sections: Vec::new(),
            block_caps: Vec::new(),
            link_caps: Vec::new(),
            list_stack: Vec::new(),
            table_stack: Vec::new(),
        }
    }

    fn bump_node(&mut self) -> Result<()> {
        self.nodes = self.nodes.saturating_add(1);
        if self.nodes > u64::from(self.limits.max_xhtml_nodes) {
            return Err(Error::resource_limit("XHTML exceeds max_xhtml_nodes"));
        }
        Ok(())
    }

    fn item_bound(&self) -> u64 {
        u64::from(self.limits.max_xhtml_nodes)
    }

    fn open(&mut self, local: &str, attrs: &[(String, String)]) -> Result<()> {
        self.elem_depth = self.elem_depth.saturating_add(1);
        if self.root_local.is_empty() {
            self.root_local = local.to_string();
        }
        if local == "body" {
            self.body_seen = true;
        }
        // Fragment ids: every `id`, and `a/@name` (legacy anchors).
        if let Some(id) = attr(attrs, "id") {
            if !id.is_empty() && (self.fragments.len() as u64) < self.item_bound() {
                self.fragments.push(id.to_string());
            }
        } else if local == "a"
            && let Some(name) = attr(attrs, "name")
            && !name.is_empty()
            && (self.fragments.len() as u64) < self.item_bound()
        {
            self.fragments.push(name.to_string());
        }

        match local {
            "script" => {
                self.scripted = true;
                self.skip_depth = self.skip_depth.saturating_add(1);
            }
            "style" => {
                self.skip_depth = self.skip_depth.saturating_add(1);
            }
            "head" => {
                self.head_depth = self.head_depth.saturating_add(1);
            }
            _ => {}
        }

        if self.skip_depth > 0 {
            return Ok(());
        }

        if let Some((kind, ra, value)) = resource_of(local, attrs)
            && !value.is_empty()
            && (self.resources.len() as u64) < self.item_bound()
        {
            let (member, external) = classify_href(self.base_dir, &value, self.limits);
            self.resources.push(Resource {
                kind: kind.to_string(),
                attr: ra.to_string(),
                value,
                member,
                external,
            });
        }

        match local {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = local.as_bytes()[1] - b'0';
                self.block_caps.push(BlockCap::Heading {
                    level,
                    id: attr(attrs, "id").map(str::to_string),
                    epub_type: qattr(attrs, "epub:type").map(str::to_string),
                    buf: String::new(),
                });
            }
            "p" | "blockquote" | "pre" | "figcaption" => {
                if self.table_stack.is_empty() {
                    self.block_caps
                        .push(BlockCap::Paragraph { buf: String::new() });
                }
            }
            "ul" | "ol" => {
                if self.table_stack.is_empty() {
                    self.list_stack.push(ListCtx {
                        ordered: local == "ol",
                        items: Vec::new(),
                    });
                }
            }
            "li" => {
                if self.table_stack.is_empty() && !self.list_stack.is_empty() {
                    self.block_caps
                        .push(BlockCap::ListItem { buf: String::new() });
                }
            }
            "table" => {
                if self.table_stack.len() >= MAX_TABLE_NESTING {
                    return Err(Error::resource_limit(
                        "XHTML table nesting exceeds the bound",
                    ));
                }
                self.table_stack.push(TableCtx {
                    rows: Vec::new(),
                    current_row: None,
                });
            }
            "tr" => {
                if let Some(t) = self.table_stack.last_mut() {
                    t.current_row = Some(Vec::new());
                }
            }
            "td" | "th" => {
                if self.table_stack.last().is_some() {
                    let colspan = attr(attrs, "colspan")
                        .and_then(parse_u32)
                        .unwrap_or(1)
                        .clamp(1, 4096);
                    let rowspan = attr(attrs, "rowspan")
                        .and_then(parse_u32)
                        .unwrap_or(1)
                        .clamp(1, 4096);
                    self.block_caps.push(BlockCap::Cell {
                        header: local == "th",
                        colspan,
                        rowspan,
                        buf: String::new(),
                    });
                }
            }
            "a" => {
                if let Some(href) = attr(attrs, "href")
                    && !href.is_empty()
                    && self.link_caps.len() < MAX_LINK_NESTING
                {
                    self.link_caps.push(LinkCap {
                        href: href.to_string(),
                        epub_type: qattr(attrs, "epub:type").map(str::to_string),
                        buf: String::new(),
                    });
                }
            }
            "section" | "article" | "aside" | "nav" | "figure" | "main" | "header" | "footer"
                if (self.sections.len() as u64) < self.item_bound() =>
            {
                self.sections.push(Section {
                    local: local.to_string(),
                    epub_type: qattr(attrs, "epub:type").map(str::to_string),
                    depth: self.elem_depth,
                });
            }
            _ => {}
        }
        Ok(())
    }

    fn close(&mut self, local: &str) {
        self.elem_depth = self.elem_depth.saturating_sub(1);
        match local {
            "script" | "style" => {
                self.skip_depth = self.skip_depth.saturating_sub(1);
            }
            "head" => {
                self.head_depth = self.head_depth.saturating_sub(1);
            }
            _ => {}
        }

        match local {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                if let Some(BlockCap::Heading {
                    level,
                    id,
                    epub_type,
                    buf,
                }) = pop_if(&mut self.block_caps, |c| {
                    matches!(c, BlockCap::Heading { .. })
                }) {
                    self.blocks.push(Block::Heading {
                        level,
                        id,
                        epub_type,
                        text: normalize(&buf),
                    });
                }
            }
            "p" | "blockquote" | "pre" | "figcaption" => {
                if let Some(BlockCap::Paragraph { buf }) = pop_if(&mut self.block_caps, |c| {
                    matches!(c, BlockCap::Paragraph { .. })
                }) {
                    self.blocks.push(Block::Paragraph {
                        text: normalize(&buf),
                    });
                }
            }
            "li" => {
                if let Some(BlockCap::ListItem { buf }) = pop_if(&mut self.block_caps, |c| {
                    matches!(c, BlockCap::ListItem { .. })
                }) {
                    let text = normalize(&buf);
                    if let Some(list) = self.list_stack.last_mut() {
                        list.items.push(text);
                    }
                }
            }
            "ul" | "ol" => {
                if let Some(ctx) = self.list_stack.pop() {
                    self.blocks.push(Block::List {
                        ordered: ctx.ordered,
                        items: ctx.items,
                    });
                }
            }
            "td" | "th" => {
                if let Some(BlockCap::Cell {
                    header,
                    colspan,
                    rowspan,
                    buf,
                }) = pop_if(&mut self.block_caps, |c| matches!(c, BlockCap::Cell { .. }))
                {
                    let cell = Cell {
                        header,
                        text: normalize(&buf),
                        colspan,
                        rowspan,
                    };
                    if let Some(t) = self.table_stack.last_mut()
                        && let Some(row) = t.current_row.as_mut()
                    {
                        row.push(cell);
                    }
                }
            }
            "tr" => {
                if let Some(t) = self.table_stack.last_mut()
                    && let Some(row) = t.current_row.take()
                {
                    t.rows.push(TableRow { cells: row });
                }
            }
            "table" => {
                if let Some(t) = self.table_stack.pop() {
                    self.blocks.push(Block::Table { rows: t.rows });
                }
            }
            "a" => {
                if let Some(link) = pop_if(&mut self.link_caps, |_| true) {
                    let fragment = link
                        .href
                        .split_once('#')
                        .map(|(_, f)| f.to_string())
                        .filter(|f| !f.is_empty());
                    let (member, external) = classify_href(self.base_dir, &link.href, self.limits);
                    self.links.push(Link {
                        href: link.href,
                        text: normalize(&link.buf),
                        fragment,
                        member,
                        external,
                        epub_type: link.epub_type,
                    });
                }
            }
            _ => {}
        }
    }

    fn text(&mut self, s: &str) -> Result<()> {
        self.st.text(s.len(), self.limits)?;
        if self.head_depth > 0 || self.skip_depth > 0 {
            return Ok(());
        }
        if let Some(cap) = self.block_caps.last_mut() {
            cap.push(s);
        }
        if let Some(l) = self.link_caps.last_mut() {
            l.buf.push_str(s);
        }
        Ok(())
    }

    fn finish(self) -> Result<ContentModel> {
        if self.root_local.is_empty() {
            return Err(Error::invalid_xml_structure(
                "XHTML content document is empty",
            ));
        }
        // Drop blocks that carry no authored text; tables are kept as structure.
        let blocks: Vec<Block> = self
            .blocks
            .into_iter()
            .filter(|b| match b {
                Block::Heading { text, .. } | Block::Paragraph { text } => !text.is_empty(),
                Block::List { items, .. } => !items.is_empty(),
                Block::Table { .. } => true,
            })
            .collect();
        Ok(ContentModel {
            root_local: self.root_local,
            body_seen: self.body_seen,
            scripted: self.scripted,
            blocks,
            links: self.links,
            resources: self.resources,
            fragments: self.fragments,
            sections: self.sections,
            xhtml_nodes: self.nodes,
        })
    }
}

/// Pop the top capture iff it matches `pred`, returning it.
fn pop_if<T>(stack: &mut Vec<T>, pred: impl FnOnce(&T) -> bool) -> Option<T> {
    if stack.last().is_some_and(pred) {
        stack.pop()
    } else {
        None
    }
}

/// Parse one XHTML content document into its bounded content model. `base_dir` is
/// the container directory the document resolves references against. Never
/// executes scripts and never fetches an external target.
pub fn parse_content(xml: &[u8], base_dir: &str, limits: Limits) -> Result<ContentModel> {
    harden_xml(xml, limits)?;
    let mut reader = Reader::from_reader(xml);
    let mut p = ContentParser::new(limits, base_dir);
    loop {
        let ev = reader.read_event().map_err(xml_err)?;
        p.st.event(limits)?;
        match ev {
            Event::Eof => break,
            Event::DocType(d) => accept_doctype(&d.into_inner())?,
            Event::Start(e) => {
                p.st.open(limits)?;
                p.bump_node()?;
                let attrs = read_attrs_qualified(&e, limits)?;
                let local = e.name().local_name().as_ref().to_string();
                p.open(&local, &attrs)?;
            }
            Event::Empty(e) => {
                p.st.leaf(limits)?;
                p.bump_node()?;
                let attrs = read_attrs_qualified(&e, limits)?;
                let local = e.name().local_name().as_ref().to_string();
                p.open(&local, &attrs)?;
                p.close(&local);
            }
            Event::End(e) => {
                p.st.close();
                let local = e.name().local_name().as_ref().to_string();
                p.close(&local);
            }
            Event::Text(t) => {
                let s = t.into_inner().into_owned();
                p.text(&s)?;
            }
            Event::CData(t) => {
                let s = t.into_inner().into_owned();
                p.text(&s)?;
            }
            Event::GeneralRef(r) => {
                let s = entity_ref(r);
                p.text(&s)?;
            }
            _ => {}
        }
    }
    p.finish()
}

fn entity_ref(r: BytesRef<'_>) -> String {
    entity_ref_text(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head><title>ignored</title></head>
<body>
<section epub:type="chapter" id="sec1">
<h2 id="h1">Heading &amp; more</h2>
<p>Hello <em>world</em>!</p>
<ul><li>alpha</li><li>beta</li></ul>
<table><tr><th>K</th><th>V</th></tr><tr><td>answer</td><td>42</td></tr></table>
<p>See <a href="ch1.xhtml#frag">one</a> and <a href="https://example.com/x">ext</a>.</p>
<img src="images/p.png" alt="p"/>
</section>
</body>
</html>"#;

    #[test]
    fn parses_blocks_links_resources() {
        let m = parse_content(DOC, "OEBPS/", Limits::DEFAULT).unwrap();
        assert_eq!(m.root_local, "html");
        assert!(m.body_seen);
        assert!(!m.scripted);
        // heading, paragraph, list, table, paragraph
        assert_eq!(m.blocks.len(), 5, "{:?}", m.blocks);
        match &m.blocks[0] {
            Block::Heading {
                level, text, id, ..
            } => {
                assert_eq!(*level, 2);
                assert_eq!(text, "Heading & more");
                assert_eq!(id.as_deref(), Some("h1"));
            }
            other => panic!("expected heading, got {other:?}"),
        }
        match &m.blocks[2] {
            Block::List { ordered, items } => {
                assert!(!*ordered);
                assert_eq!(items, &["alpha".to_string(), "beta".to_string()]);
            }
            other => panic!("expected list, got {other:?}"),
        }
        match &m.blocks[3] {
            Block::Table { rows } => {
                assert_eq!(rows.len(), 2);
                assert!(rows[0].cells[0].header);
                assert_eq!(rows[1].cells[1].text, "42");
            }
            other => panic!("expected table, got {other:?}"),
        }
        assert_eq!(m.links.len(), 2);
        assert_eq!(m.links[0].member.as_deref(), Some("OEBPS/ch1.xhtml"));
        assert_eq!(m.links[0].fragment.as_deref(), Some("frag"));
        assert!(m.links[1].external);
        assert_eq!(m.resources.len(), 1);
        assert_eq!(m.resources[0].kind, "img");
        assert_eq!(m.resources[0].member.as_deref(), Some("OEBPS/images/p.png"));
        assert!(m.fragments.contains(&"sec1".to_string()));
        assert_eq!(m.sections.len(), 1);
        assert_eq!(m.sections[0].epub_type.as_deref(), Some("chapter"));
        assert!(m.text().contains("Hello world!"));
        assert!(m.text().contains("answer\t42"));
    }

    #[test]
    fn model_roundtrips() {
        let m = parse_content(DOC, "OEBPS/", Limits::DEFAULT).unwrap();
        let bytes = m.encode();
        let back = ContentModel::decode(&bytes).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn params_roundtrip() {
        let p = EpubExtractProfile::DEFAULT;
        let params = content_params(3, 7, "OEBPS/text/", &p);
        let (i, o, b, prof) = read_content_params(&params).unwrap();
        assert_eq!((i, o), (3, 7));
        assert_eq!(b, "OEBPS/text/");
        assert_eq!(prof, p);
    }

    #[test]
    fn doctype_policy() {
        // A benign XHTML declaration is accepted (real EPUB content documents carry one).
        let ok = parse_content(
            b"<!DOCTYPE html><html><body><p>x</p></body></html>",
            "",
            Limits::DEFAULT,
        );
        assert!(ok.is_ok(), "benign DOCTYPE must parse");
        // A declaration with an internal subset can declare entities, so it is refused.
        let bad = b"<!DOCTYPE html [<!ENTITY e \"x\">]><html><body><p>x</p></body></html>";
        assert_eq!(
            parse_content(bad, "", Limits::DEFAULT).unwrap_err().class(),
            crate::ErrorClass::InvalidXmlStructure
        );
    }

    #[test]
    fn deep_nesting_declines_not_panics() {
        let limits = Limits::STRICT;
        let mut s = String::from("<html><body>");
        for _ in 0..(limits.max_xml_depth + 8) {
            s.push_str("<div>");
        }
        s.push_str("x</body></html>");
        assert_eq!(
            parse_content(s.as_bytes(), "", limits).unwrap_err().class(),
            crate::ErrorClass::InvalidXmlStructure
        );
    }

    #[test]
    fn node_bound_declines() {
        let limits = Limits::STRICT;
        let mut s = String::from("<html><body>");
        for _ in 0..(limits.max_xhtml_nodes + 4) {
            s.push_str("<br/>");
        }
        s.push_str("</body></html>");
        assert_eq!(
            parse_content(s.as_bytes(), "", limits).unwrap_err().class(),
            crate::ErrorClass::ResourceLimit
        );
    }
}
