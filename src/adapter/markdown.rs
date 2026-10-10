//! Bounded, representation-preserving Markdown adapter (Phase 21.8).
//!
//! Markdown is the first **prose** Wave-2 format. Like JSON/YAML/CSV it is *not*
//! a package: there is no OPC/ZIP layer, no `mimetype`, and no relationship graph.
//! The exact leaf is the **whole source** (a `DocumentExact`, a RAW-like
//! authority), and everything this module produces is a bounded, deterministic
//! (`Q_gen`) projection that never sits on the exactness path.
//!
//! ## Why a bespoke parser, and what "representation-preserving" means
//!
//! The point of a Markdown adapter is to preserve the **representation**, not a
//! rendered value. A conventional "Markdown → HTML" pipeline drops the source
//! spelling: it re-flows paragraphs, resolves and *rewrites* links, strips
//! markers, normalizes whitespace, and forgets every source offset. This adapter
//! instead records, for every block and every inline span, its exact **byte span**
//! in the source, and never rewrites the bytes:
//!
//! * **block structure** — front matter, ATX headings (level 1..=6), paragraphs,
//!   list items (ordered/unordered, nesting depth), fenced code (with its info
//!   string / language tag), indented code, blockquotes, tables, reference
//!   definitions, footnote definitions, and thematic breaks;
//! * **inline spans** — code spans, emphasis/strong, links and images (destination
//!   + optional title), reference links, footnote references, and table cells;
//! * **heading levels** and a heading tree derivable from them;
//! * **front matter** — a leading `---`/`...` (YAML) or `+++` (TOML) fenced block,
//!   surfaced as a front-matter block with its exact delimiters and inner span.
//!
//! The source is *never* re-flowed or normalized: a block's exact bytes are
//! literally `source[span]`, and the canonical text projection is the source
//! itself (lossily decoded), not a rendered or re-wrapped derivative.
//!
//! ## The supported subset (and what is DECLINED, typed)
//!
//! This is a **bounded CommonMark subset**, deliberately conservative. It supports
//! the constructs listed above. It does **not** attempt setext headings, HTML
//! blocks, nested inline emphasis inside link text, reference-image titles across
//! lines, or the full container-continuation algorithm; those constructs are left
//! as literal text inside their enclosing block rather than being guessed at.
//! Detection additionally requires a **structural** mark (a heading, a fenced code
//! block, front matter, a table, a reference definition, or a footnote
//! definition), so plain prose — which is a valid Markdown paragraph — stays
//! [`Opaque`](crate::field::document_format::DocumentFormat::Opaque) rather than
//! being admitted as Markdown on the strength of being text.
//!
//! Untrusted input can only ever yield a typed decline, never a panic or an
//! unbounded allocation: the block count is capped by [`Limits::max_markdown_blocks`];
//! the inline-span count by [`Limits::max_markdown_inline_spans`]; the container
//! nesting depth by [`Limits::max_markdown_depth`]; the code content by
//! [`Limits::max_markdown_code_bytes`]; the total node count by
//! [`Limits::max_markdown_nodes`]; and the source length by
//! [`Limits::max_markdown_document_bytes`]. Inline extraction additionally runs
//! under a fixed per-document **probe budget** so a pathological delimiter pattern
//! cannot drive super-linear work; when the budget is exhausted the remaining text
//! is simply left as literal block content (the block structure and every span
//! captured so far are unaffected, and exactness is untouched).

use std::collections::HashMap;

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model blocks (defends the decoder against a hostile blob).
pub const MAX_MODEL_BLOCKS: u32 = 1 << 22;
/// Hard cap on decoded model inline spans (defends the decoder against a hostile blob).
pub const MAX_MODEL_INLINES: u64 = 1 << 24;

/// Block kind: leading front matter (YAML `---`/`...` or TOML `+++`).
pub const B_FRONT_MATTER: u8 = 0;
/// Block kind: an ATX heading (`#`..`######`).
pub const B_HEADING: u8 = 1;
/// Block kind: a paragraph (a run of non-blank lines).
pub const B_PARAGRAPH: u8 = 2;
/// Block kind: one list item (ordered or unordered).
pub const B_LIST_ITEM: u8 = 3;
/// Block kind: a fenced code block (``` or ~~~).
pub const B_FENCE: u8 = 4;
/// Block kind: an indented code block (four-space indent).
pub const B_INDENTED_CODE: u8 = 5;
/// Block kind: a blockquote run.
pub const B_BLOCKQUOTE: u8 = 6;
/// Block kind: a GFM table (a header row plus a delimiter row plus body rows).
pub const B_TABLE: u8 = 7;
/// Block kind: a link reference definition (`[label]: dest "title"`).
pub const B_REF_DEF: u8 = 8;
/// Block kind: a footnote definition (`[^label]: ...`).
pub const B_FOOTNOTE_DEF: u8 = 9;
/// Block kind: a thematic break (`---`, `***`, `___`).
pub const B_THEMATIC_BREAK: u8 = 10;
/// The highest valid block kind.
pub const B_LAST: u8 = B_THEMATIC_BREAK;

/// Inline kind: a code span (`` `…` ``).
pub const I_CODE: u8 = 0;
/// Inline kind: emphasis (`*…*`/`_…_`).
pub const I_EMPHASIS: u8 = 1;
/// Inline kind: strong (`**…**`/`__…__`).
pub const I_STRONG: u8 = 2;
/// Inline kind: an inline link `[text](dest "title")`.
pub const I_LINK: u8 = 3;
/// Inline kind: an image `![alt](dest "title")`.
pub const I_IMAGE: u8 = 4;
/// Inline kind: a reference link `[text][label]` (target resolved from a definition).
pub const I_REF_LINK: u8 = 5;
/// Inline kind: a footnote reference `[^label]`.
pub const I_FOOTNOTE_REF: u8 = 6;
/// Inline kind: a table cell (its trimmed content span).
pub const I_TABLE_CELL: u8 = 7;
/// The highest valid inline kind.
pub const I_LAST: u8 = I_TABLE_CELL;

/// Block flag bit: an ordered list item.
pub const F_ORDERED: u8 = 1;
/// Block flag bit: a loose (blank-line-separated) list item.
pub const F_LOOSE: u8 = 2;

/// Stable lower-case block-kind name for reports and JSON output.
pub const fn block_kind_name(kind: u8) -> &'static str {
    match kind {
        B_FRONT_MATTER => "front-matter",
        B_HEADING => "heading",
        B_PARAGRAPH => "paragraph",
        B_LIST_ITEM => "list-item",
        B_FENCE => "fence",
        B_INDENTED_CODE => "indented-code",
        B_BLOCKQUOTE => "blockquote",
        B_TABLE => "table",
        B_REF_DEF => "ref-def",
        B_FOOTNOTE_DEF => "footnote-def",
        B_THEMATIC_BREAK => "thematic-break",
        _ => "unknown",
    }
}

/// Stable lower-case inline-kind name for reports and JSON output.
pub const fn inline_kind_name(kind: u8) -> &'static str {
    match kind {
        I_CODE => "code",
        I_EMPHASIS => "emphasis",
        I_STRONG => "strong",
        I_LINK => "link",
        I_IMAGE => "image",
        I_REF_LINK => "ref-link",
        I_FOOTNOTE_REF => "footnote-ref",
        I_TABLE_CELL => "table-cell",
        _ => "unknown",
    }
}

/// Whether a block kind is a code block (fenced or indented).
pub const fn is_code_block(kind: u8) -> bool {
    matches!(kind, B_FENCE | B_INDENTED_CODE)
}

/// One parsed block: its kind, its exact source span, its content span (the bytes
/// a reader sees), and any kind-specific fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdBlock {
    /// One of the `B_*` tags.
    pub kind: u8,
    /// The block's first source byte.
    pub start: u64,
    /// One past the block's last source byte (its line terminator excluded).
    pub end: u64,
    /// The first byte of the block's *content* (never re-flowed).
    pub content_start: u64,
    /// One past the content's last byte.
    pub content_end: u64,
    /// Heading level (1..=6); list nesting depth; blockquote marker depth; else 0.
    pub level: u8,
    /// `F_*` flag bits.
    pub flags: u8,
    /// A table's row count (including the header row); else 0.
    pub rows: u32,
    /// A table's column count; else 0.
    pub cols: u32,
    /// A fenced code block's info string; a front-matter flavour (`yaml`/`toml`); a
    /// reference/footnote definition's label; else `None`.
    pub info: Option<String>,
    /// A reference definition's destination; else `None`.
    pub target: Option<String>,
    /// A reference definition's title; else `None`.
    pub title: Option<String>,
    /// Indices into the inline arena belonging to this block, in order.
    pub inlines: Vec<u32>,
}

/// One parsed inline span: its kind, its exact span and inner (text) span, and any
/// link/image fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdInline {
    /// One of the `I_*` tags.
    pub kind: u8,
    /// The inline's first source byte (including its markers).
    pub start: u64,
    /// One past the inline's last source byte.
    pub end: u64,
    /// The first byte of the inline's inner text span.
    pub inner_start: u64,
    /// One past the inner text span.
    pub inner_end: u64,
    /// A link/image destination, if any.
    pub target: Option<String>,
    /// A link/image title, if any.
    pub title: Option<String>,
    /// The index of the owning block.
    pub block: u32,
}

/// The canonical derived Markdown model (the materialization of a `MarkdownModel`
/// node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownModel {
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// The block arena, in document order.
    pub blocks: Vec<MdBlock>,
    /// The inline-span arena, in document order.
    pub inlines: Vec<MdInline>,
}

/// The structural signals detection requires. Plain prose is a valid Markdown
/// paragraph, so detection needs at least one *structural* mark, which this
/// returns.
impl MarkdownModel {
    /// The block at `index`, if present.
    pub fn block(&self, index: u32) -> Option<&MdBlock> {
        self.blocks.get(index as usize)
    }

    /// The inline at `index`, if present.
    pub fn inline(&self, index: u32) -> Option<&MdInline> {
        self.inlines.get(index as usize)
    }

    /// The indices of blocks of `kind`, in document order.
    pub fn blocks_of_kind(&self, kind: u8) -> Vec<u32> {
        self.blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| b.kind == kind)
            .map(|(i, _)| i as u32)
            .collect()
    }

    /// The indices of inlines of `kind`, in document order.
    pub fn inlines_of_kind(&self, kind: u8) -> Vec<u32> {
        self.inlines
            .iter()
            .enumerate()
            .filter(|(_, x)| x.kind == kind)
            .map(|(i, _)| i as u32)
            .collect()
    }

    /// The document's maximum heading level observed (0 if there are no headings).
    pub fn max_heading_level(&self) -> u8 {
        self.blocks
            .iter()
            .filter(|b| b.kind == B_HEADING)
            .map(|b| b.level)
            .max()
            .unwrap_or(0)
    }

    /// Whether the source carries a structural mark that admits it as Markdown.
    pub fn has_structural_signal(&self) -> bool {
        self.blocks.iter().any(|b| {
            matches!(
                b.kind,
                B_FRONT_MATTER | B_HEADING | B_FENCE | B_TABLE | B_REF_DEF | B_FOOTNOTE_DEF
            )
        })
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64 + self.blocks.len() * 48);
        out.extend_from_slice(b"MDOC");
        out.push(MODEL_VERSION);
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&(self.blocks.len() as u32).to_le_bytes());
        for b in &self.blocks {
            out.push(b.kind);
            out.push(b.level);
            out.push(b.flags);
            out.extend_from_slice(&b.start.to_le_bytes());
            out.extend_from_slice(&b.end.to_le_bytes());
            out.extend_from_slice(&b.content_start.to_le_bytes());
            out.extend_from_slice(&b.content_end.to_le_bytes());
            out.extend_from_slice(&b.rows.to_le_bytes());
            out.extend_from_slice(&b.cols.to_le_bytes());
            put_opt(&mut out, b.info.as_deref());
            put_opt(&mut out, b.target.as_deref());
            put_opt(&mut out, b.title.as_deref());
            out.extend_from_slice(&(b.inlines.len() as u32).to_le_bytes());
            for i in &b.inlines {
                out.extend_from_slice(&i.to_le_bytes());
            }
        }
        out.extend_from_slice(&(self.inlines.len() as u32).to_le_bytes());
        for x in &self.inlines {
            out.push(x.kind);
            out.extend_from_slice(&x.start.to_le_bytes());
            out.extend_from_slice(&x.end.to_le_bytes());
            out.extend_from_slice(&x.inner_start.to_le_bytes());
            out.extend_from_slice(&x.inner_end.to_le_bytes());
            put_opt(&mut out, x.target.as_deref());
            put_opt(&mut out, x.title.as_deref());
            out.extend_from_slice(&x.block.to_le_bytes());
        }
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<MarkdownModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"MDOC" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let doc_len = r.u64()?;
        let bc = r.u32()?;
        if bc > MAX_MODEL_BLOCKS {
            return Err(corrupt("model block count is implausible"));
        }
        let mut total_inlines = 0u64;
        let mut blocks = Vec::with_capacity(bc as usize);
        for _ in 0..bc {
            let kind = r.u8()?;
            if kind > B_LAST {
                return Err(corrupt("unknown block kind"));
            }
            let level = r.u8()?;
            let flags = r.u8()?;
            if flags & !(F_ORDERED | F_LOOSE) != 0 {
                return Err(corrupt("unknown block flags"));
            }
            let start = r.u64()?;
            let end = r.u64()?;
            let cs = r.u64()?;
            let ce = r.u64()?;
            if start > end || end > doc_len || cs > ce || ce > end || cs < start {
                return Err(corrupt("block span is outside the source"));
            }
            let rows = r.u32()?;
            let cols = r.u32()?;
            let info = r.opt_str()?;
            let target = r.opt_str()?;
            let title = r.opt_str()?;
            let n = r.u32()?;
            total_inlines = total_inlines.saturating_add(n as u64);
            if total_inlines > MAX_MODEL_INLINES {
                return Err(corrupt("model inline count is implausible"));
            }
            let mut inlines = Vec::with_capacity(n as usize);
            for _ in 0..n {
                inlines.push(r.u32()?);
            }
            blocks.push(MdBlock {
                kind,
                start,
                end,
                content_start: cs,
                content_end: ce,
                level,
                flags,
                rows,
                cols,
                info,
                target,
                title,
                inlines,
            });
        }
        let ic = r.u32()?;
        if ic as u64 > MAX_MODEL_INLINES {
            return Err(corrupt("model inline count is implausible"));
        }
        let mut inlines = Vec::with_capacity(ic as usize);
        for _ in 0..ic {
            let kind = r.u8()?;
            if kind > I_LAST {
                return Err(corrupt("unknown inline kind"));
            }
            let start = r.u64()?;
            let end = r.u64()?;
            let is = r.u64()?;
            let ie = r.u64()?;
            if start > end || end > doc_len || is > ie || ie > end || is < start {
                return Err(corrupt("inline span is outside the source"));
            }
            let target = r.opt_str()?;
            let title = r.opt_str()?;
            let block = r.u32()?;
            inlines.push(MdInline {
                kind,
                start,
                end,
                inner_start: is,
                inner_end: ie,
                target,
                title,
                block,
            });
        }
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        let block_count = blocks.len() as u32;
        for b in &blocks {
            for &i in &b.inlines {
                if i >= inlines.len() as u32 {
                    return Err(corrupt("block inline index is out of range"));
                }
            }
        }
        for x in &inlines {
            if x.block >= block_count {
                return Err(corrupt("inline block index is out of range"));
            }
        }
        Ok(MarkdownModel {
            doc_len,
            blocks,
            inlines,
        })
    }
}

/// Byte-based Markdown detector. See the module docs for the exact heuristic.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if looks_like_other(source, limits) {
        return false;
    }
    match parse(source, limits, false) {
        Ok(m) => m.has_structural_signal(),
        Err(_) => false,
    }
}

/// Whether `source` belongs to a family that is *not* Markdown (defence in depth;
/// the dispatcher also orders PDF/ZIP/JSON/YAML/CSV ahead of Markdown).
fn looks_like_other(source: &[u8], limits: Limits) -> bool {
    if source.is_empty() {
        return true;
    }
    if source.starts_with(b"%PDF-") {
        return true;
    }
    if source.starts_with(b"PK\x03\x04")
        || source.starts_with(b"PK\x05\x06")
        || source.starts_with(b"PK\x07\x08")
    {
        return true;
    }
    #[cfg(feature = "json")]
    if crate::adapter::json::detect(source, limits) {
        return true;
    }
    #[cfg(feature = "yaml")]
    if crate::adapter::yaml::detect(source, limits) {
        return true;
    }
    #[cfg(feature = "csv")]
    if crate::adapter::csv::detect(source, limits) {
        return true;
    }
    false
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of a `MarkdownModel` node).
pub fn build_markdown_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits, true)?.encode())
}

/// Parse `source` into a [`MarkdownModel`]. `build` selects whether the inline
/// arena is populated (detection runs with `build = false`, so a detection-style
/// call never scans inline spans).
pub fn parse(source: &[u8], limits: Limits, build: bool) -> Result<MarkdownModel> {
    if source.len() as u64 > limits.max_markdown_document_bytes {
        return Err(Error::resource_limit(format!(
            "Markdown source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_markdown_document_bytes
        )));
    }
    let mut p = Parser::new(source, limits, build);
    p.run()?;
    Ok(MarkdownModel {
        doc_len: source.len() as u64,
        blocks: p.blocks,
        inlines: p.inlines,
    })
}

/// The exact source bytes of a block's whole span (`[start, end)`).
pub fn block_bytes<'a>(source: &'a [u8], block: &MdBlock) -> Result<&'a [u8]> {
    slice(
        source,
        block.start,
        block.end,
        "block span is outside the source",
    )
}

/// The exact source bytes of a block's content span (`[content_start, content_end)`).
pub fn content_bytes<'a>(source: &'a [u8], block: &MdBlock) -> Result<&'a [u8]> {
    slice(
        source,
        block.content_start,
        block.content_end,
        "block content span is outside the source",
    )
}

/// The exact source bytes of an inline's whole span (`[start, end)`).
pub fn inline_bytes<'a>(source: &'a [u8], inline: &MdInline) -> Result<&'a [u8]> {
    slice(
        source,
        inline.start,
        inline.end,
        "inline span is outside the source",
    )
}

/// The exact source bytes of an inline's inner text span.
pub fn inline_text_bytes<'a>(source: &'a [u8], inline: &MdInline) -> Result<&'a [u8]> {
    slice(
        source,
        inline.inner_start,
        inline.inner_end,
        "inline text span is outside the source",
    )
}

/// The language tag of a fenced code block: the first whitespace-delimited token
/// of its info string, or `None`.
pub fn fence_language(block: &MdBlock) -> Option<String> {
    let info = block.info.as_deref()?;
    let lang = info.split_whitespace().next()?;
    if lang.is_empty() {
        None
    } else {
        Some(lang.to_string())
    }
}

/// A deterministic canonical text projection: the exact source bytes, lossily
/// decoded. Markdown is **not** re-flowed or rendered, so the canonical text is
/// the source itself. Declines typed if it would exceed `max_out`.
pub fn canonical_text(source: &[u8], _limits: Limits, max_out: u64) -> Result<String> {
    if source.len() as u64 > max_out {
        return Err(Error::resource_limit(format!(
            "Markdown text projection exceeds the {max_out}-byte budget"
        )));
    }
    Ok(String::from_utf8_lossy(source).into_owned())
}

/// One lexical-match record from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdMatch {
    /// The 0-based block index the match lies in.
    pub block: u32,
    /// The block's kind tag.
    pub kind: u8,
    /// The exact source span of the whole block the match lies in.
    pub start: u64,
    /// One past the block's span.
    pub end: u64,
    /// The block's exact content text.
    pub text: String,
}

/// A bounded, case-sensitive lexical search over each block's content span.
/// Returns matches in document order. Declines typed if the match list would
/// exceed `max_out` bytes (an approximate bound).
pub fn find(
    source: &[u8],
    model: &MarkdownModel,
    pattern: &str,
    max_out: u64,
) -> Result<Vec<MdMatch>> {
    if pattern.is_empty() {
        return Err(Error::usage("Markdown find pattern must not be empty"));
    }
    let needle = pattern.as_bytes();
    let mut out: Vec<MdMatch> = Vec::new();
    let mut estimated: u64 = 0;
    for (i, b) in model.blocks.iter().enumerate() {
        let content = content_bytes(source, b)?;
        if contains(content, needle) {
            estimated = estimated.saturating_add(32 + content.len() as u64);
            if estimated > max_out {
                return Err(Error::resource_limit(format!(
                    "Markdown find exceeded the {max_out}-byte budget"
                )));
            }
            out.push(MdMatch {
                block: i as u32,
                kind: b.kind,
                start: b.start,
                end: b.end,
                text: String::from_utf8_lossy(content).into_owned(),
            });
        }
    }
    Ok(out)
}

fn slice<'a>(source: &'a [u8], s: u64, e: u64, msg: &str) -> Result<&'a [u8]> {
    let s = usize::try_from(s).map_err(|_| corrupt("span overflow"))?;
    let e = usize::try_from(e).map_err(|_| corrupt("span overflow"))?;
    source.get(s..e).ok_or_else(|| corrupt(msg))
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || needle.len() > hay.len() {
        return false;
    }
    hay.windows(needle.len()).any(|w| w == needle)
}

// ---------------------------------------------------------------------------
// The parser
// ---------------------------------------------------------------------------

struct Line {
    start: usize,
    /// One past the last content byte (a trailing `\r` is excluded).
    content_end: usize,
    /// One past the line terminator (or the end of the source).
    end: usize,
}

type RefMap = HashMap<String, (String, String)>;

struct Parser<'a> {
    b: &'a [u8],
    lines: Vec<Line>,
    limits: Limits,
    build: bool,
    blocks: Vec<MdBlock>,
    inlines: Vec<MdInline>,
    node_count: u64,
    inline_count: u64,
    code_bytes: u64,
    max_depth: u32,
    probe: u64,
    refs: RefMap,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Atx {
        level: u8,
        content_off: usize,
    },
    Fence {
        ch: u8,
        run: usize,
        info_off: usize,
    },
    Thematic,
    Blockquote {
        depth: u32,
    },
    List {
        ordered: bool,
        number: u64,
        marker_indent: usize,
        content_off: usize,
    },
    Table,
    RefDef,
    FootnoteDef,
    IndentedCode,
    None,
}

impl<'a> Parser<'a> {
    fn new(b: &'a [u8], limits: Limits, build: bool) -> Self {
        // A fixed per-document probe budget bounds inline delimiter searches so a
        // pathological pattern cannot drive super-linear work.
        let probe = (b.len() as u64).saturating_mul(2).saturating_add(4096);
        Parser {
            b,
            lines: split_lines(b),
            limits,
            build,
            blocks: Vec::new(),
            inlines: Vec::new(),
            node_count: 0,
            inline_count: 0,
            code_bytes: 0,
            max_depth: 0,
            probe,
            refs: HashMap::new(),
        }
    }

    fn lb(&self, i: usize) -> &'a [u8] {
        let l = &self.lines[i];
        &self.b[l.start..l.content_end]
    }

    fn run(&mut self) -> Result<()> {
        if self.lines.is_empty() {
            return Ok(());
        }
        // Collect reference definitions first, so a reference link resolves
        // regardless of whether its definition appears before or after the use.
        self.collect_refs();
        let mut i = 0usize;
        // Leading front matter (only at the very start of the document).
        if let Some((fm_end, flavour, inner_start, inner_end)) = self.front_matter() {
            self.push_block(
                B_FRONT_MATTER,
                0,
                self.lines[fm_end - 1].content_end as u64,
                inner_start as u64,
                inner_end as u64,
                0,
                0,
                Some(flavour),
                None,
                None,
            )?;
            i = fm_end;
        }
        while i < self.lines.len() {
            if is_blank(self.lb(i)) {
                i += 1;
                continue;
            }
            match self.classify(i) {
                Kind::Atx { level, content_off } => {
                    let l = &self.lines[i];
                    let start = l.start as u64;
                    let end = l.content_end as u64;
                    // Strip trailing closing hashes (and the spaces before them).
                    let lb = self.lb(i);
                    let ce_off = trim_trailing_atx(lb, content_off);
                    let cs = (l.start + content_off) as u64;
                    let ce = (l.start + ce_off) as u64;
                    let idx =
                        self.push_block(B_HEADING, start, end, cs, ce, level, 0, None, None, None)?;
                    self.scan_block_inlines(idx)?;
                    i += 1;
                }
                Kind::Fence { ch, run, info_off } => {
                    let l = &self.lines[i];
                    let start = l.start as u64;
                    let info = String::from_utf8_lossy(&self.lb(i)[info_off..]).into_owned();
                    // Find the closing fence.
                    let mut j = i + 1;
                    let mut close: Option<usize> = None;
                    while j < self.lines.len() {
                        if is_closing_fence(self.lb(j), ch, run) {
                            close = Some(j);
                            break;
                        }
                        j += 1;
                    }
                    let (end, content_end) = match close {
                        Some(c) => (self.lines[c].content_end as u64, self.lines[c].start as u64),
                        None => (
                            self.lines[self.lines.len() - 1].end as u64,
                            self.b.len() as u64,
                        ),
                    };
                    let cs = l.end.min(content_end as usize) as u64;
                    self.charge_code(content_end.saturating_sub(cs))?;
                    let info = if info.trim().is_empty() {
                        None
                    } else {
                        Some(info.trim().to_string())
                    };
                    self.push_block(B_FENCE, start, end, cs, content_end, 0, 0, info, None, None)?;
                    i = match close {
                        Some(c) => c + 1,
                        None => self.lines.len(),
                    };
                }
                Kind::Thematic => {
                    let l = &self.lines[i];
                    self.push_block(
                        B_THEMATIC_BREAK,
                        l.start as u64,
                        l.content_end as u64,
                        l.start as u64,
                        l.content_end as u64,
                        0,
                        0,
                        None,
                        None,
                        None,
                    )?;
                    i += 1;
                }
                Kind::Blockquote { depth } => {
                    let start = self.lines[i].start;
                    let mut j = i;
                    let mut last_end = self.lines[i].content_end;
                    let mut max_depth = depth;
                    while j < self.lines.len() && !is_blank(self.lb(j)) {
                        if let Kind::Blockquote { depth: d } = self.classify(j) {
                            last_end = self.lines[j].content_end;
                            max_depth = max_depth.max(d);
                            j += 1;
                        } else {
                            break;
                        }
                    }
                    let idx = self.push_block(
                        B_BLOCKQUOTE,
                        start as u64,
                        last_end as u64,
                        start as u64,
                        last_end as u64,
                        max_depth.min(255) as u8,
                        0,
                        None,
                        None,
                        None,
                    )?;
                    self.note_depth(max_depth)?;
                    self.scan_block_inlines(idx)?;
                    i = j;
                }
                Kind::List {
                    ordered,
                    number,
                    marker_indent,
                    content_off,
                } => {
                    let start = self.lines[i].start;
                    // Gather continuation lines: blanks or lines indented past the
                    // marker, stopping at a non-blank less-indented line.
                    let mut j = i + 1;
                    let mut last_end = self.lines[i].content_end;
                    while j < self.lines.len() {
                        let lb = self.lb(j);
                        if is_blank(lb) {
                            j += 1;
                            continue;
                        }
                        let ind = leading_spaces(lb);
                        if ind > marker_indent {
                            last_end = self.lines[j].content_end;
                            j += 1;
                        } else {
                            break;
                        }
                    }
                    let cs = (self.lines[i].start + content_off) as u64;
                    let ce = last_end as u64;
                    let mut flags = 0u8;
                    if ordered {
                        flags |= F_ORDERED;
                    }
                    let raw_level = (marker_indent / 2 + 1) as u32;
                    self.note_depth(raw_level)?;
                    let level = raw_level.min(self.limits.max_markdown_depth) as u8;
                    let item_info = if ordered {
                        Some(number.to_string())
                    } else {
                        None
                    };
                    let idx = self.push_block(
                        B_LIST_ITEM,
                        start as u64,
                        last_end as u64,
                        cs.min(ce),
                        ce,
                        level,
                        flags,
                        item_info,
                        None,
                        None,
                    )?;
                    self.scan_block_inlines(idx)?;
                    i = j;
                }
                Kind::Table => {
                    let start = self.lines[i].start;
                    let delim = self.lb(i + 1);
                    let cols = table_cell_spans(delim).len().max(1) as u32;
                    let mut j = i + 2;
                    let mut last_end = self.lines[i + 1].content_end;
                    while j < self.lines.len()
                        && !is_blank(self.lb(j))
                        && self.lb(j).contains(&b'|')
                    {
                        last_end = self.lines[j].content_end;
                        j += 1;
                    }
                    let idx = self.push_block(
                        B_TABLE,
                        start as u64,
                        last_end as u64,
                        start as u64,
                        last_end as u64,
                        0,
                        0,
                        Some(String::from_utf8_lossy(delim).into_owned()),
                        None,
                        None,
                    )?;
                    if self.build {
                        self.scan_table_cells(idx, i, j)?;
                    }
                    // `rows` counts content rows (the delimiter row is not a row).
                    self.blocks[idx].rows = (j - i - 1) as u32;
                    self.blocks[idx].cols = cols;
                    i = j;
                }
                Kind::RefDef => {
                    let l = &self.lines[i];
                    let lb = self.lb(i);
                    let (label, dest, title) =
                        parse_refdef(lb).ok_or_else(|| corrupt("bad ref definition"))?;
                    let meta = l.start;
                    let after = lb
                        .iter()
                        .position(|&c| c == b':')
                        .map(|p| p + 1)
                        .unwrap_or(0);
                    let cs = (meta + after) as u64;
                    self.push_block(
                        B_REF_DEF,
                        l.start as u64,
                        l.content_end as u64,
                        cs,
                        l.content_end as u64,
                        0,
                        0,
                        Some(label),
                        Some(dest),
                        title,
                    )?;
                    i += 1;
                }
                Kind::FootnoteDef => {
                    let l = &self.lines[i];
                    let lb = self.lb(i);
                    let label = parse_footnote_label(lb)
                        .ok_or_else(|| corrupt("bad footnote definition"))?;
                    let after = lb
                        .iter()
                        .position(|&c| c == b':')
                        .map(|p| p + 1)
                        .unwrap_or(lb.len());
                    // Gather indented continuation lines.
                    let mut j = i + 1;
                    let mut last_end = l.content_end;
                    while j < self.lines.len() {
                        let l2 = self.lb(j);
                        if is_blank(l2) {
                            j += 1;
                            continue;
                        }
                        if leading_spaces(l2) > 0 {
                            last_end = self.lines[j].content_end;
                            j += 1;
                        } else {
                            break;
                        }
                    }
                    let idx = self.push_block(
                        B_FOOTNOTE_DEF,
                        l.start as u64,
                        last_end as u64,
                        (l.start + after) as u64,
                        last_end as u64,
                        0,
                        0,
                        Some(label),
                        None,
                        None,
                    )?;
                    self.scan_block_inlines(idx)?;
                    i = j;
                }
                Kind::IndentedCode => {
                    let start = self.lines[i].start;
                    let mut j = i;
                    let mut last_end = self.lines[i].content_end;
                    while j < self.lines.len() {
                        let lb = self.lb(j);
                        if is_blank(lb) {
                            j += 1;
                            continue;
                        }
                        if leading_spaces(lb) >= 4 {
                            last_end = self.lines[j].content_end;
                            j += 1;
                        } else {
                            break;
                        }
                    }
                    let end = last_end as u64;
                    self.charge_code((last_end - start) as u64)?;
                    self.push_block(
                        B_INDENTED_CODE,
                        start as u64,
                        end,
                        start as u64,
                        end,
                        0,
                        0,
                        None,
                        None,
                        None,
                    )?;
                    i = j;
                }
                Kind::None => {
                    let start = self.lines[i].start;
                    let mut j = i + 1;
                    let mut last_end = self.lines[i].content_end;
                    while j < self.lines.len() && !is_blank(self.lb(j)) {
                        if self.interrupts_paragraph(j) {
                            break;
                        }
                        last_end = self.lines[j].content_end;
                        j += 1;
                    }
                    let idx = self.push_block(
                        B_PARAGRAPH,
                        start as u64,
                        last_end as u64,
                        start as u64,
                        last_end as u64,
                        0,
                        0,
                        None,
                        None,
                        None,
                    )?;
                    self.scan_block_inlines(idx)?;
                    i = j;
                }
            }
        }
        Ok(())
    }

    /// Whether the line at `j` starts a block that can interrupt an open
    /// paragraph. Indented lines and plain text do **not** interrupt (they are
    /// lazy continuation lines).
    fn interrupts_paragraph(&self, j: usize) -> bool {
        matches!(
            self.classify(j),
            Kind::Atx { .. }
                | Kind::Fence { .. }
                | Kind::Thematic
                | Kind::Blockquote { .. }
                | Kind::List { .. }
                | Kind::Table
                | Kind::RefDef
                | Kind::FootnoteDef
        )
    }

    fn classify(&self, i: usize) -> Kind {
        let lb = self.lb(i);
        if let Some((level, content_off)) = parse_atx(lb) {
            return Kind::Atx { level, content_off };
        }
        if let Some((ch, run, info_off)) = parse_fence(lb) {
            return Kind::Fence { ch, run, info_off };
        }
        if is_thematic(lb) {
            return Kind::Thematic;
        }
        if let Some(depth) = parse_blockquote(lb) {
            return Kind::Blockquote { depth };
        }
        if leading_spaces(lb) <= 3 && parse_footnote_label(lb).is_some() {
            return Kind::FootnoteDef;
        }
        if leading_spaces(lb) <= 3 && parse_refdef(lb).is_some() {
            return Kind::RefDef;
        }
        if leading_spaces(lb) <= 3 && self.is_table(i) {
            return Kind::Table;
        }
        if let Some(m) = parse_list_marker(lb) {
            return Kind::List {
                ordered: m.0,
                number: m.1,
                marker_indent: m.2,
                content_off: m.3,
            };
        }
        if leading_spaces(lb) >= 4 {
            return Kind::IndentedCode;
        }
        Kind::None
    }

    fn is_table(&self, i: usize) -> bool {
        if !self.lb(i).contains(&b'|') {
            return false;
        }
        if i + 1 >= self.lines.len() {
            return false;
        }
        is_delimiter_row(self.lb(i + 1))
    }

    /// Detect leading front matter. Returns `(line_after, flavour, inner_start,
    /// inner_end)` on success.
    fn front_matter(&self) -> Option<(usize, String, usize, usize)> {
        let first = self.lines.first()?;
        let lb0 = &self.b[first.start..first.content_end];
        if lb0 != b"---" && lb0 != b"+++" {
            return None;
        }
        let (flavour, closes) = if lb0 == b"---" {
            ("yaml", [&b"---"[..], &b"..."[..]])
        } else {
            ("toml", [&b"+++"[..], &b"+++"[..]])
        };
        let mut j = 1;
        while j < self.lines.len() {
            let lb = self.lb(j);
            if lb == closes[0] || lb == closes[1] {
                let inner_start = first.end;
                let inner_end = self.lines[j].start;
                return Some((j + 1, flavour.to_string(), inner_start, inner_end));
            }
            j += 1;
        }
        None
    }

    fn collect_refs(&mut self) {
        for i in 1..self.lines.len() {
            let lb = self.lb(i);
            if is_blank(lb) || leading_spaces(lb) > 3 {
                continue;
            }
            if let Some((label, dest, title)) = parse_refdef(lb) {
                self.refs
                    .entry(label)
                    .or_insert((dest, title.unwrap_or_default()));
            }
        }
    }

    fn note_depth(&mut self, depth: u32) -> Result<()> {
        if depth > self.limits.max_markdown_depth {
            return Err(Error::resource_limit(format!(
                "Markdown nesting depth {depth} exceeds the {}-deep cap",
                self.limits.max_markdown_depth
            )));
        }
        self.max_depth = self.max_depth.max(depth);
        Ok(())
    }

    fn charge_code(&mut self, bytes: u64) -> Result<()> {
        self.code_bytes = self.code_bytes.saturating_add(bytes);
        if self.code_bytes > self.limits.max_markdown_code_bytes {
            return Err(Error::resource_limit(format!(
                "Markdown code content exceeds the {}-byte cap",
                self.limits.max_markdown_code_bytes
            )));
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn push_block(
        &mut self,
        kind: u8,
        start: u64,
        end: u64,
        content_start: u64,
        content_end: u64,
        level: u8,
        flags: u8,
        info: Option<String>,
        target: Option<String>,
        title: Option<String>,
    ) -> Result<usize> {
        if self.blocks.len() as u64 >= self.limits.max_markdown_blocks as u64 {
            return Err(Error::resource_limit(format!(
                "Markdown document exceeds the {}-block cap",
                self.limits.max_markdown_blocks
            )));
        }
        self.node_count = self.node_count.saturating_add(1);
        if self.node_count > self.limits.max_markdown_nodes as u64 {
            return Err(Error::resource_limit(format!(
                "Markdown document exceeds the {}-node cap",
                self.limits.max_markdown_nodes
            )));
        }
        let idx = self.blocks.len();
        self.blocks.push(MdBlock {
            kind,
            start,
            end,
            content_start: content_start.min(content_end),
            content_end,
            level,
            flags,
            rows: 0,
            cols: 0,
            info,
            target,
            title,
            inlines: Vec::new(),
        });
        Ok(idx)
    }

    #[allow(clippy::too_many_arguments)]
    fn push_inline(
        &mut self,
        block: usize,
        kind: u8,
        start: usize,
        end: usize,
        inner_start: usize,
        inner_end: usize,
        target: Option<String>,
        title: Option<String>,
    ) -> Result<()> {
        self.inline_count = self.inline_count.saturating_add(1);
        if self.inline_count > self.limits.max_markdown_inline_spans as u64 {
            return Err(Error::resource_limit(format!(
                "Markdown document exceeds the {}-inline-span cap",
                self.limits.max_markdown_inline_spans
            )));
        }
        self.node_count = self.node_count.saturating_add(1);
        if self.node_count > self.limits.max_markdown_nodes as u64 {
            return Err(Error::resource_limit(format!(
                "Markdown document exceeds the {}-node cap",
                self.limits.max_markdown_nodes
            )));
        }
        let idx = self.inlines.len() as u32;
        self.blocks[block].inlines.push(idx);
        self.inlines.push(MdInline {
            kind,
            start: start as u64,
            end: end as u64,
            inner_start: inner_start as u64,
            inner_end: inner_end as u64,
            target,
            title,
            block: block as u32,
        });
        Ok(())
    }

    fn scan_block_inlines(&mut self, block: usize) -> Result<()> {
        if !self.build {
            return Ok(());
        }
        let (cs, ce) = {
            let b = &self.blocks[block];
            (b.content_start as usize, b.content_end as usize)
        };
        self.scan_region(block, cs, ce)
    }

    fn scan_table_cells(&mut self, block: usize, first: usize, last: usize) -> Result<()> {
        for row in first..last {
            // The delimiter row (the second line) declares alignment, not a row.
            if row == first + 1 {
                continue;
            }
            let (cells, base) = {
                let l = &self.lines[row];
                let lb = &self.b[l.start..l.content_end];
                (table_cell_spans(lb), l.start)
            };
            for (cs, ce) in cells {
                let abs_cs = base + cs;
                let abs_ce = base + ce;
                self.push_inline(
                    block,
                    I_TABLE_CELL,
                    abs_cs,
                    abs_ce,
                    abs_cs,
                    abs_ce,
                    None,
                    None,
                )?;
            }
        }
        Ok(())
    }

    fn scan_region(&mut self, block: usize, start: usize, end: usize) -> Result<()> {
        let mut i = start;
        while i < end {
            if self.probe == 0 {
                break;
            }
            let c = self.b[i];
            match c {
                b'`' => {
                    if let Some((is, ie, we)) = self.code_span(i, end) {
                        self.push_inline(block, I_CODE, i, we, is, ie, None, None)?;
                        i = we;
                        continue;
                    }
                }
                b'!' if i + 1 < end && self.b[i + 1] == b'[' => {
                    if let Some((is, ie, t, we, _)) = self.link(i, end, true) {
                        self.push_inline(block, I_IMAGE, i, we, is, ie, t.0, t.1)?;
                        i = we;
                        continue;
                    }
                }
                b'[' => {
                    if i + 1 < end
                        && self.b[i + 1] == b'^'
                        && let Some((ls, we)) = self.footnote_ref(i, end)
                    {
                        // inner span is the label text (between `[^` and `]`).
                        self.push_inline(block, I_FOOTNOTE_REF, i, we, ls, we - 1, None, None)?;
                        i = we;
                        continue;
                    }
                    if let Some((is, ie, t, we, is_ref)) = self.link(i, end, false) {
                        let kind = if is_ref { I_REF_LINK } else { I_LINK };
                        self.push_inline(block, kind, i, we, is, ie, t.0, t.1)?;
                        i = we;
                        continue;
                    }
                }
                b'*' | b'_' => {
                    if let Some((k, is, ie, we)) = self.emphasis(i, end) {
                        self.push_inline(block, k, i, we, is, ie, None, None)?;
                        i = we;
                        continue;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        Ok(())
    }

    /// Try a code span at `i`. Returns `(inner_start, inner_end, whole_end)`.
    fn code_span(&mut self, i: usize, end: usize) -> Option<(usize, usize, usize)> {
        let c = b'`';
        let mut r = 0;
        while i + r < end && self.b[i + r] == c {
            r += 1;
        }
        if r == 0 {
            return None;
        }
        let mut j = i + r;
        while j < end {
            if self.probe == 0 {
                return None;
            }
            self.probe -= 1;
            if self.b[j] == c {
                let mut rr = 0;
                while j + rr < end && self.b[j + rr] == c {
                    rr += 1;
                }
                if rr == r {
                    return Some((i + r, j, j + r));
                }
                j += rr.max(1);
            } else {
                j += 1;
            }
        }
        None
    }

    /// Try a footnote reference `[^label]` at `i`. Returns `(label_start, whole_end)`.
    fn footnote_ref(&mut self, i: usize, end: usize) -> Option<(usize, usize)> {
        let mut j = i + 2;
        while j < end {
            if self.probe == 0 {
                return None;
            }
            self.probe -= 1;
            if self.b[j] == b']' {
                if j > i + 2 {
                    return Some((i + 2, j + 1));
                }
                return None;
            }
            if self.b[j] == b'\n' {
                return None;
            }
            j += 1;
        }
        None
    }

    /// Try emphasis/strong at `i`. Returns `(kind, inner_start, inner_end,
    /// whole_end)`.
    fn emphasis(&mut self, i: usize, end: usize) -> Option<(u8, usize, usize, usize)> {
        let c = self.b[i];
        let mut r = 0;
        while i + r < end && self.b[i + r] == c && r < 2 {
            r += 1;
        }
        let strong = r >= 2;
        let open_len = if strong { 2 } else { 1 };
        // The opening delimiter must be followed by non-whitespace.
        let inner0 = i + open_len;
        if inner0 >= end || self.b[inner0].is_ascii_whitespace() {
            return None;
        }
        let mut j = inner0;
        while j < end {
            if self.probe == 0 {
                return None;
            }
            self.probe -= 1;
            if self.b[j] == c {
                let mut rr = 0;
                while j + rr < end && self.b[j + rr] == c {
                    rr += 1;
                }
                if strong && rr >= 2 {
                    let before = if j > inner0 { self.b[j - 1] } else { b' ' };
                    if !before.is_ascii_whitespace() {
                        return Some((I_STRONG, inner0, j, j + 2));
                    }
                } else if !strong && rr == 1 {
                    let before = if j > inner0 { self.b[j - 1] } else { b' ' };
                    if !before.is_ascii_whitespace() {
                        return Some((I_EMPHASIS, inner0, j, j + 1));
                    }
                }
                j += rr.max(1);
            } else {
                j += 1;
            }
        }
        None
    }

    /// Try a link or image at `i`. Returns `(inner_start, inner_end, (target,
    /// title), whole_end, is_ref)`, where `is_ref` marks a reference link.
    #[allow(clippy::type_complexity)]
    fn link(
        &mut self,
        i: usize,
        end: usize,
        is_image: bool,
    ) -> Option<(usize, usize, (Option<String>, Option<String>), usize, bool)> {
        let text_open = if is_image { i + 1 } else { i };
        if self.b.get(text_open) != Some(&b'[') {
            return None;
        }
        let text_start = text_open + 1;
        // Find the matching `]` with bracket nesting.
        let mut depth = 0usize;
        let mut j = text_start;
        let mut close = None;
        while j < end {
            if self.probe == 0 {
                return None;
            }
            self.probe -= 1;
            match self.b[j] {
                b'[' => depth += 1,
                b']' => {
                    if depth == 0 {
                        close = Some(j);
                        break;
                    }
                    depth -= 1;
                }
                b'\n' => {}
                _ => {}
            }
            j += 1;
        }
        let close = close?;
        if close == text_start {
            return None; // empty text is not a link
        }
        let after = close + 1;
        if after < end && self.b[after] == b'(' {
            // Inline link: [text](dest "title").
            let paren_close = self.find_paren_close(after, end)?;
            let region = &self.b[after + 1..paren_close];
            let (dest, title) = parse_destination(region);
            return Some((text_start, close, (dest, title), paren_close + 1, false));
        }
        if after < end && self.b[after] == b'[' {
            // Full/collapsed reference: [text][label].
            let label_close = self.find_bracket_close(after, end)?;
            let label_region = &self.b[after + 1..label_close];
            let label = if label_region.is_empty() {
                &self.b[text_start..close]
            } else {
                label_region
            };
            let key = normalize_label(label);
            let (target, title) = self.refs.get(&key).cloned()?;
            let title = if title.is_empty() { None } else { Some(title) };
            return Some((
                text_start,
                close,
                (Some(target), title),
                label_close + 1,
                true,
            ));
        }
        // Shortcut reference: [text].
        let key = normalize_label(&self.b[text_start..close]);
        let (target, title) = self.refs.get(&key).cloned()?;
        let title = if title.is_empty() { None } else { Some(title) };
        Some((text_start, close, (Some(target), title), close + 1, true))
    }

    fn find_paren_close(&mut self, open: usize, end: usize) -> Option<usize> {
        let mut j = open + 1;
        while j < end {
            if self.probe == 0 {
                return None;
            }
            self.probe -= 1;
            if self.b[j] == b')' {
                return Some(j);
            }
            if self.b[j] == b'\n' {
                return None;
            }
            j += 1;
        }
        None
    }

    fn find_bracket_close(&mut self, open: usize, end: usize) -> Option<usize> {
        let mut j = open + 1;
        while j < end {
            if self.probe == 0 {
                return None;
            }
            self.probe -= 1;
            if self.b[j] == b']' {
                return Some(j);
            }
            if self.b[j] == b'\n' {
                return None;
            }
            j += 1;
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Line helpers
// ---------------------------------------------------------------------------

fn split_lines(b: &[u8]) -> Vec<Line> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        let start = i;
        let mut j = i;
        while j < b.len() && b[j] != b'\n' {
            j += 1;
        }
        let mut content_end = j;
        if content_end > start && b[content_end - 1] == b'\r' {
            content_end -= 1;
        }
        let end = if j < b.len() { j + 1 } else { j };
        out.push(Line {
            start,
            content_end,
            end,
        });
        i = end;
    }
    out
}

fn is_blank(lb: &[u8]) -> bool {
    lb.iter().all(|&c| c == b' ' || c == b'\t')
}

fn leading_spaces(lb: &[u8]) -> usize {
    lb.iter().take_while(|&&c| c == b' ').count()
}

fn parse_atx(lb: &[u8]) -> Option<(u8, usize)> {
    let ind = leading_spaces(lb);
    if ind > 3 {
        return None;
    }
    let rest = &lb[ind..];
    let mut n = 0;
    while n < rest.len() && rest[n] == b'#' {
        n += 1;
    }
    if n == 0 || n > 6 {
        return None;
    }
    if n < rest.len() && rest[n] != b' ' && rest[n] != b'\t' {
        return None;
    }
    // Skip the marker and any following spaces/tabs.
    let mut off = ind + n;
    while off < lb.len() && (lb[off] == b' ' || lb[off] == b'\t') {
        off += 1;
    }
    Some((n as u8, off))
}

/// Given the heading line and its content offset, strip trailing spaces and a
/// trailing closing-hash run. Returns the content end offset.
fn trim_trailing_atx(lb: &[u8], content_off: usize) -> usize {
    let mut e = lb.len();
    while e > content_off && (lb[e - 1] == b' ' || lb[e - 1] == b'\t') {
        e -= 1;
    }
    // A closing run of `#` requires a preceding space (or empty content).
    let mut h = e;
    while h > content_off && lb[h - 1] == b'#' {
        h -= 1;
    }
    if h < e && (h == content_off || lb[h - 1] == b' ' || lb[h - 1] == b'\t') {
        e = h;
        while e > content_off && (lb[e - 1] == b' ' || lb[e - 1] == b'\t') {
            e -= 1;
        }
    }
    e
}

fn parse_fence(lb: &[u8]) -> Option<(u8, usize, usize)> {
    let ind = leading_spaces(lb);
    if ind > 3 {
        return None;
    }
    let rest = &lb[ind..];
    let ch = *rest.first()?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let mut run = 0;
    while run < rest.len() && rest[run] == ch {
        run += 1;
    }
    if run < 3 {
        return None;
    }
    let info_off = ind + run;
    if ch == b'`' && lb[info_off..].contains(&b'`') {
        return None;
    }
    Some((ch, run, info_off))
}

fn is_closing_fence(lb: &[u8], ch: u8, open_run: usize) -> bool {
    let ind = leading_spaces(lb);
    if ind > 3 {
        return false;
    }
    let rest = &lb[ind..];
    let mut run = 0;
    while run < rest.len() && rest[run] == ch {
        run += 1;
    }
    if run < open_run {
        return false;
    }
    rest[run..].iter().all(|&c| c == b' ' || c == b'\t')
}

fn is_thematic(lb: &[u8]) -> bool {
    let ind = leading_spaces(lb);
    if ind > 3 {
        return false;
    }
    let rest = &lb[ind..];
    let mut ch = None;
    let mut count = 0;
    for &c in rest {
        match c {
            b'-' | b'*' | b'_' => {
                if ch.is_none() {
                    ch = Some(c);
                }
                if ch != Some(c) {
                    return false;
                }
                count += 1;
            }
            b' ' | b'\t' => {}
            _ => return false,
        }
    }
    count >= 3
}

/// Parse a blockquote line. Returns the marker depth (`>` count).
fn parse_blockquote(lb: &[u8]) -> Option<u32> {
    let ind = leading_spaces(lb);
    if ind > 3 {
        return None;
    }
    let rest = &lb[ind..];
    if rest.first() != Some(&b'>') {
        return None;
    }
    let mut depth = 0u32;
    let mut k = 0usize;
    loop {
        if rest.get(k) != Some(&b'>') {
            break;
        }
        depth += 1;
        k += 1;
        // An optional single space after each marker.
        if rest.get(k) == Some(&b' ') {
            k += 1;
        }
        // Allow nested `>` markers separated by spaces.
        let mut kk = k;
        while rest.get(kk) == Some(&b' ') {
            kk += 1;
        }
        if rest.get(kk) == Some(&b'>') {
            k = kk;
            continue;
        }
        break;
    }
    Some(depth)
}

/// Parse a list marker. Returns `(ordered, number, marker_indent, content_off)`.
fn parse_list_marker(lb: &[u8]) -> Option<(bool, u64, usize, usize)> {
    let ind = leading_spaces(lb);
    let rest = &lb[ind..];
    let first = *rest.first()?;
    if first == b'-' || first == b'*' || first == b'+' {
        if rest.get(1) != Some(&b' ') && rest.get(1) != Some(&b'\t') {
            return None;
        }
        // A thematic break is not a list marker.
        if is_thematic(lb) {
            return None;
        }
        return Some((false, 0, ind, skip_marker_space(lb, ind + 1)));
    }
    if first.is_ascii_digit() {
        let mut n = 0usize;
        let mut num = 0u64;
        while ind + n < lb.len() && lb[ind + n].is_ascii_digit() && n < 9 {
            num = num * 10 + (lb[ind + n] - b'0') as u64;
            n += 1;
        }
        if n == 0 {
            return None;
        }
        let d = *lb.get(ind + n)?;
        if d != b'.' && d != b')' {
            return None;
        }
        if lb.get(ind + n + 1) != Some(&b' ') && lb.get(ind + n + 1) != Some(&b'\t') {
            return None;
        }
        return Some((true, num, ind, skip_marker_space(lb, ind + n + 1)));
    }
    None
}

fn skip_marker_space(lb: &[u8], mut off: usize) -> usize {
    // Skip the single required space plus additional indentation.
    while off < lb.len() && (lb[off] == b' ' || lb[off] == b'\t') {
        off += 1;
    }
    off
}

fn is_delimiter_row(lb: &[u8]) -> bool {
    let mut s = 0usize;
    let mut e = lb.len();
    while s < e && (lb[s] == b' ' || lb[s] == b'\t') {
        s += 1;
    }
    while e > s && (lb[e - 1] == b' ' || lb[e - 1] == b'\t') {
        e -= 1;
    }
    let row = &lb[s..e];
    if row.is_empty() {
        return false;
    }
    let mut has_pipe = false;
    let mut has_dash = false;
    for &c in row {
        match c {
            b'|' => has_pipe = true,
            b'-' => has_dash = true,
            b':' | b' ' | b'\t' => {}
            _ => return false,
        }
    }
    has_pipe && has_dash
}

/// Parse a reference definition `[label]: dest "title"`. Returns
/// `(normalized_label, dest, title)`.
fn parse_refdef(lb: &[u8]) -> Option<(String, String, Option<String>)> {
    let ind = leading_spaces(lb);
    if ind > 3 {
        return None;
    }
    let rest = &lb[ind..];
    if rest.first() != Some(&b'[') || rest.get(1) == Some(&b'^') {
        return None;
    }
    let close = rest.iter().position(|&c| c == b']')?;
    if close < 2 {
        return None;
    }
    if rest.get(close + 1) != Some(&b':') {
        return None;
    }
    let label = &rest[1..close];
    let after = close + 2;
    let tail = rest.get(after..)?;
    if tail.iter().all(|&c| c == b' ' || c == b'\t') {
        return None; // a destination is required
    }
    let (dest, title) = parse_destination(tail);
    let dest = dest?;
    Some((normalize_label(label), dest, title))
}

/// Parse a footnote definition label `[^label]:`. Returns the label.
fn parse_footnote_label(lb: &[u8]) -> Option<String> {
    let ind = leading_spaces(lb);
    if ind > 3 {
        return None;
    }
    let rest = &lb[ind..];
    if rest.first() != Some(&b'[') || rest.get(1) != Some(&b'^') {
        return None;
    }
    let close = rest.iter().position(|&c| c == b']')?;
    if close < 3 {
        return None;
    }
    if rest.get(close + 1) != Some(&b':') {
        return None;
    }
    Some(normalize_label(&rest[2..close]))
}

/// Parse a link destination and optional title from a `(…)` region or a ref-def
/// tail. The destination may be `<…>` or a bare token; the title may be `"…"`,
/// `'…'`, or `(…)`.
fn parse_destination(region: &[u8]) -> (Option<String>, Option<String>) {
    let mut s = 0usize;
    while s < region.len() && (region[s] == b' ' || region[s] == b'\t') {
        s += 1;
    }
    let rest = &region[s..];
    let dest = if rest.first() == Some(&b'<') {
        match rest.iter().position(|&c| c == b'>') {
            Some(g) => region[s + 1..s + g].to_vec(),
            None => Vec::new(),
        }
    } else {
        let mut e = 0usize;
        while e < rest.len() && rest[e] != b' ' && rest[e] != b'\t' {
            e += 1;
        }
        rest[..e].to_vec()
    };
    if dest.is_empty() {
        return (None, None);
    }
    // Optional title.
    let mut t = 0usize;
    // Re-find the position after the destination.
    if rest.first() == Some(&b'<') {
        if let Some(g) = rest.iter().position(|&c| c == b'>') {
            t = s + g + 1;
        }
    } else {
        let mut e = 0usize;
        while e < rest.len() && rest[e] != b' ' && rest[e] != b'\t' {
            e += 1;
        }
        t = s + e;
    }
    while t < region.len() && (region[t] == b' ' || region[t] == b'\t') {
        t += 1;
    }
    let mut title = None;
    if t < region.len() {
        let closer = match region[t] {
            b'"' => b'"',
            b'\'' => b'\'',
            b'(' => b')',
            _ => 0,
        };
        if closer != 0
            && let Some(p) = region[t + 1..].iter().position(|&c| c == closer)
        {
            title = Some(String::from_utf8_lossy(&region[t + 1..t + 1 + p]).into_owned());
        }
    }
    (Some(String::from_utf8_lossy(&dest).into_owned()), title)
}

/// The trimmed content spans of a table row's cells (offsets within `lb`).
fn table_cell_spans(lb: &[u8]) -> Vec<(usize, usize)> {
    let mut s = 0usize;
    let mut e = lb.len();
    while s < e && (lb[s] == b' ' || lb[s] == b'\t') {
        s += 1;
    }
    while e > s && (lb[e - 1] == b' ' || lb[e - 1] == b'\t') {
        e -= 1;
    }
    if s < e && lb[s] == b'|' {
        s += 1;
    }
    if e > s && lb[e - 1] == b'|' {
        e -= 1;
    }
    let row = &lb[s..e];
    let mut out = Vec::new();
    let mut cell_start = 0usize;
    for (idx, &c) in row.iter().enumerate() {
        if c == b'|' {
            push_cell(&mut out, row, cell_start, idx, s);
            cell_start = idx + 1;
        }
    }
    push_cell(&mut out, row, cell_start, row.len(), s);
    out
}

fn push_cell(out: &mut Vec<(usize, usize)>, row: &[u8], mut a: usize, mut b: usize, base: usize) {
    while a < b && (row[a] == b' ' || row[a] == b'\t') {
        a += 1;
    }
    while b > a && (row[b - 1] == b' ' || row[b - 1] == b'\t') {
        b -= 1;
    }
    out.push((base + a, base + b));
}

/// Normalize a reference label: case-folded ASCII, whitespace-collapsed, trimmed.
fn normalize_label(label: &[u8]) -> String {
    let s = String::from_utf8_lossy(label);
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
            }
            prev_space = true;
        } else {
            prev_space = false;
            out.extend(c.to_lowercase());
        }
    }
    while out.ends_with(' ') {
        out.pop();
    }
    out
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn corrupt(msg: &str) -> Error {
    Error::invalid_markdown_structure(format!("malformed Markdown: {msg}"))
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

    fn model(src: &str) -> MarkdownModel {
        parse(src.as_bytes(), Limits::DEFAULT, true).unwrap()
    }

    #[test]
    fn detects_structure_but_not_prose() {
        let l = Limits::DEFAULT;
        assert!(detect(b"# Title\n\nbody\n", l));
        assert!(detect(b"```rust\nfn main() {}\n```\n", l));
        assert!(detect(b"---\ntitle: x\n---\n\n# H\n", l));
        assert!(detect(b"| a | b |\n| - | - |\n| 1 | 2 |\n", l));
        // Plain prose has no structural mark and stays Opaque.
        assert!(!detect(b"just some prose\nwith more lines\n", l));
        assert!(!detect(b"", l));
    }

    #[test]
    fn headings_lists_and_code_are_exact() {
        let m = model("# One\n\n## Two\n\n- a\n- b\n\n```rust\nlet x = 1;\n```\n");
        let heads = m.blocks_of_kind(B_HEADING);
        assert_eq!(heads.len(), 2);
        assert_eq!(m.blocks[heads[0] as usize].level, 1);
        assert_eq!(m.blocks[heads[1] as usize].level, 2);
        let items = m.blocks_of_kind(B_LIST_ITEM);
        assert_eq!(items.len(), 2);
        let fences = m.blocks_of_kind(B_FENCE);
        assert_eq!(fences.len(), 1);
        assert_eq!(
            fence_language(&m.blocks[fences[0] as usize]).as_deref(),
            Some("rust")
        );
    }

    #[test]
    fn inline_spans_and_model_roundtrip() {
        let src = "see [link](http://x \"t\") and *em* and `code`\n";
        let m = model(src);
        let bytes = m.encode();
        let back = MarkdownModel::decode(&bytes).unwrap();
        assert_eq!(m, back);
        let links = m.inlines_of_kind(I_LINK);
        assert_eq!(links.len(), 1);
        let l = &m.inlines[links[0] as usize];
        assert_eq!(l.target.as_deref(), Some("http://x"));
        assert_eq!(l.title.as_deref(), Some("t"));
        let em = m.inlines_of_kind(I_EMPHASIS);
        assert_eq!(em.len(), 1);
        let code = m.inlines_of_kind(I_CODE);
        assert_eq!(code.len(), 1);
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0x1234_5678_9abc_def0;
        for _ in 0..128 {
            let mut buf = vec![0u8; 512];
            for b in buf.iter_mut() {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                *b = (x & 0xFF) as u8;
            }
            let _ = detect(&buf, Limits::STRICT);
            if let Ok(m) = parse(&buf, Limits::STRICT, true) {
                let _ = m.encode();
                let _ = find(&buf, &m, "a", 1 << 20);
            }
        }
    }
}
