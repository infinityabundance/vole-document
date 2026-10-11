//! Bounded, representation-preserving reStructuredText adapter (Phase 21.26.1).
//!
//! reStructuredText (reST) is the next **prose** Wave-2 format, the Docutils input
//! language. Like Markdown/JSON/YAML/CSV it is *not* a package: there is no OPC/ZIP
//! layer, no `mimetype`, and no relationship graph. The exact leaf is the **whole
//! source** (a `DocumentExact`, a RAW-like authority), and everything this module
//! produces is a bounded, deterministic (`Q_gen`) projection that never sits on the
//! exactness path.
//!
//! ## Why a bespoke parser, and what "representation-preserving" means
//!
//! The point of a reST adapter is to preserve the **representation**, not a
//! rendered value. A conventional "reST → HTML" pipeline drops the source spelling:
//! it resolves and *rewrites* hyperlink targets, executes directives, expands
//! substitutions, strips adornment markers, normalizes whitespace, and forgets
//! every source offset. This adapter instead records, for every block and every
//! inline span, its exact **byte span** in the source, and never rewrites the bytes:
//!
//! * **section titles** and their **underline/overline adornment** — the exact
//!   adornment char and length are preserved (as the block's `info`), the overline
//!   fact is a flag, and the **adornment hierarchy** is recorded as a per-title
//!   `level` in first-seen order;
//! * **paragraphs**, **explicit markup starts** (`.. ` comments, `.. directive::`
//!   directives with their options/arguments, substitution definitions, footnotes
//!   and citations like `.. [1]` / `.. [name]`, and hyperlink targets `.. _name:`);
//!   directives are preserved verbatim and never executed or resolved;
//! * **field lists** (`:name: value`), **option lists**, **definition lists**,
//!   **literal blocks** (`::`), and **doctest blocks** (`>>> `);
//! * **bullet** (`*`/`-`/`+`) and **enumerated** (`1.`/`#.`/`a.`) lists with
//!   nesting depth;
//! * **inline** markup: strong `**…**`, emphasis `*…*`, literal ``` ``…`` ```,
//!   interpreted text `` `text`:role: ``, substitution references `|x|`, footnote
//!   references `[1]_`/`[name]_`, hyperlink references `` `name`_ `` / `name_`,
//!   and anonymous references `` `name`__ `` / `name__`;
//! * **grid tables** (`+`/`-`/`|` borders) and **simple tables** (`=` rules), each
//!   exposing its exact border spans and per-cell spans.
//!
//! The source is *never* re-flowed or normalized: a block's exact bytes are
//! literally `source[span]`, and the canonical text projection is the source
//! itself (lossily decoded), not a rendered or re-wrapped derivative.
//!
//! ## The supported subset (and what is DECLINED, typed)
//!
//! This is a **bounded Docutils subset**, deliberately conservative. It supports
//! the constructs listed above at the top level (indent 0). It does **not** attempt
//! nested sections inside directives/lists, the full directive option/argument
//! grammar, multi-line section titles, quoted-literal or "field-name with
//! arguments" edge cases, or the complete enumerated-list marker space; those
//! constructs are left as literal text inside their enclosing block rather than
//! being guessed at.
//!
//! ## Detection (conservative; no magic bytes) and the Markdown boundary
//!
//! Plain prose has no reST structural mark and stays
//! [`Opaque`](crate::field::document_format::DocumentFormat::Opaque). Detection
//! requires a **reST-specific** mark: an **explicit markup start** (`.. ` directive,
//! comment, target, footnote/citation, or substitution definition) — which Markdown,
//! CSV, and the other text formats do not claim; a **grid table** or a **simple
//! table**; a **field list** (`:name:`); or a **section adornment whose char is not
//! a Markdown construct**. The last clause is load-bearing and *honest*: an
//! adornment of `-`, `*`, or `_` (a run of length ≥ 3) is a Markdown thematic break,
//! an adornment of `~` (≥ 3) is a Markdown fence opener, and an adornment of `#` is
//! a Markdown ATX heading. Those documents are therefore admitted as **Markdown**,
//! or stay `Opaque`, and are never reclassified as reST. Only the adornment chars
//! `= + ^ " ' : . < >` are accepted as a *standalone detection signal* here. A
//! document that is valid Markdown is always tried first in the dispatcher's order,
//! so a Markdown document is never stolen.
//!
//! ## Bounds and no panics
//!
//! Untrusted input can only ever yield a typed decline, never a panic or an
//! unbounded allocation: the block count is capped by [`Limits::max_rst_blocks`];
//! the inline-span count by [`Limits::max_rst_inline_spans`]; the total node count
//! by [`Limits::max_rst_nodes`]; the title/list nesting depth by
//! [`Limits::max_rst_depth`]; a single physical line's bytes by
//! [`Limits::max_rst_line_bytes`]; and the source length by
//! [`Limits::max_rst_document_bytes`]. Inline extraction additionally runs under a
//! fixed per-document **probe budget** so a pathological delimiter pattern cannot
//! drive super-linear work; when the budget is exhausted the remaining text is
//! simply left as literal block content (the block structure and every span
//! captured so far are unaffected, and exactness is untouched).

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model blocks (defends the decoder against a hostile blob).
pub const MAX_MODEL_BLOCKS: u32 = 1 << 22;
/// Hard cap on decoded model inline spans (defends the decoder against a hostile blob).
pub const MAX_MODEL_INLINES: u64 = 1 << 24;

/// Block kind: a section title (with its underline/overline adornment).
pub const B_TITLE: u8 = 0;
/// Block kind: a paragraph (a run of non-blank lines at one indent).
pub const B_PARAGRAPH: u8 = 1;
/// Block kind: one bullet or enumerated list item.
pub const B_LIST_ITEM: u8 = 2;
/// Block kind: a literal block (the indented body introduced by `::`).
pub const B_LITERAL_BLOCK: u8 = 3;
/// Block kind: a doctest block (`>>> ` plus its output).
pub const B_DOCTEST: u8 = 4;
/// Block kind: an explicit-markup comment (`.. text`).
pub const B_COMMENT: u8 = 5;
/// Block kind: an explicit-markup directive (`.. name:: args`), preserved verbatim.
pub const B_DIRECTIVE: u8 = 6;
/// Block kind: a substitution definition (`.. |name| directive::`).
pub const B_SUBSTITUTION_DEF: u8 = 7;
/// Block kind: a footnote or citation definition (`.. [label] body`).
pub const B_FOOTNOTE_DEF: u8 = 8;
/// Block kind: a hyperlink target (`.. _name: uri`).
pub const B_HYPERLINK_TARGET: u8 = 9;
/// Block kind: a field list entry (`:name: value`).
pub const B_FIELD: u8 = 10;
/// Block kind: an option list entry (`-a, --all  description`).
pub const B_OPTION: u8 = 11;
/// Block kind: a definition list entry (`term` plus its indented definition).
pub const B_DEFINITION: u8 = 12;
/// Block kind: a grid table (`+`/`-`/`|`/`=` borders).
pub const B_GRID_TABLE: u8 = 13;
/// Block kind: a simple table (`=` rules).
pub const B_SIMPLE_TABLE: u8 = 14;
/// The highest valid block kind.
pub const B_LAST: u8 = B_SIMPLE_TABLE;

/// Inline kind: strong emphasis (`**…**`).
pub const I_STRONG: u8 = 0;
/// Inline kind: emphasis (`*…*`).
pub const I_EMPHASIS: u8 = 1;
/// Inline kind: inline literal (``` ``…`` ```).
pub const I_LITERAL: u8 = 2;
/// Inline kind: interpreted text (`` `text`:role: ``), the role name kept verbatim.
pub const I_INTERPRETED: u8 = 3;
/// Inline kind: a substitution reference (`|name|`).
pub const I_SUBSTITUTION_REF: u8 = 4;
/// Inline kind: a footnote/citation reference (`[label]_`).
pub const I_FOOTNOTE_REF: u8 = 5;
/// Inline kind: a hyperlink reference (`` `name`_ `` or `name_`).
pub const I_HYPERLINK_REF: u8 = 6;
/// Inline kind: an anonymous reference (`` `name`__ `` or `name__`).
pub const I_ANON_REF: u8 = 7;
/// Inline kind: a table cell (its exact content span).
pub const I_TABLE_CELL: u8 = 8;
/// The highest valid inline kind.
pub const I_LAST: u8 = I_TABLE_CELL;

/// Block flag bit: an enumerated list item.
pub const F_ORDERED: u8 = 1;
/// Block flag bit: a title with an overline as well as an underline.
pub const F_OVERLINE: u8 = 2;

/// Stable lower-case block-kind name for reports and JSON output.
pub const fn block_kind_name(kind: u8) -> &'static str {
    match kind {
        B_TITLE => "title",
        B_PARAGRAPH => "paragraph",
        B_LIST_ITEM => "list-item",
        B_LITERAL_BLOCK => "literal-block",
        B_DOCTEST => "doctest",
        B_COMMENT => "comment",
        B_DIRECTIVE => "directive",
        B_SUBSTITUTION_DEF => "substitution-def",
        B_FOOTNOTE_DEF => "footnote-def",
        B_HYPERLINK_TARGET => "hyperlink-target",
        B_FIELD => "field",
        B_OPTION => "option",
        B_DEFINITION => "definition",
        B_GRID_TABLE => "grid-table",
        B_SIMPLE_TABLE => "simple-table",
        _ => "unknown",
    }
}

/// Stable lower-case inline-kind name for reports and JSON output.
pub const fn inline_kind_name(kind: u8) -> &'static str {
    match kind {
        I_STRONG => "strong",
        I_EMPHASIS => "emphasis",
        I_LITERAL => "literal",
        I_INTERPRETED => "interpreted",
        I_SUBSTITUTION_REF => "substitution-ref",
        I_FOOTNOTE_REF => "footnote-ref",
        I_HYPERLINK_REF => "hyperlink-ref",
        I_ANON_REF => "anonymous-ref",
        I_TABLE_CELL => "table-cell",
        _ => "unknown",
    }
}

/// Whether a block kind is a table (grid or simple).
pub const fn is_table_block(kind: u8) -> bool {
    matches!(kind, B_GRID_TABLE | B_SIMPLE_TABLE)
}

/// Whether a block kind is an explicit-markup start.
pub const fn is_explicit_markup_block(kind: u8) -> bool {
    matches!(
        kind,
        B_COMMENT | B_DIRECTIVE | B_SUBSTITUTION_DEF | B_FOOTNOTE_DEF | B_HYPERLINK_TARGET
    )
}

/// One parsed block: its kind, its exact source span, its content span (the bytes
/// a reader sees), and any kind-specific fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RstBlock {
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
    /// Title hierarchy level (1-based, first-seen order); list nesting depth; else 0.
    pub level: u8,
    /// `F_*` flag bits.
    pub flags: u8,
    /// A grid/simple table's row count; else 0.
    pub rows: u32,
    /// A grid/simple table's column count; else 0.
    pub cols: u32,
    /// The block's exact adornment (title), directive/substitution name, footnote
    /// label, field/option/definition name, list marker, or `::` marker; else `None`.
    pub info: Option<String>,
    /// A hyperlink target's URI; an interpreted inline's role name; a list item's
    /// enumerated value; else `None`.
    pub target: Option<String>,
    /// A directive argument, substitution directive name, field/option description,
    /// or list item text; else `None`.
    pub title: Option<String>,
    /// Indices into the inline arena belonging to this block, in order.
    pub inlines: Vec<u32>,
}

/// One parsed inline span: its kind, its exact span and inner (text) span, and any
/// role/target fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RstInline {
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
    /// An interpreted role name or a hyperlink target name; else `None`.
    pub target: Option<String>,
    /// A hyperlink/anonymous reference's body text; else `None`.
    pub title: Option<String>,
    /// The index of the owning block.
    pub block: u32,
}

/// The canonical derived reStructuredText model (the materialization of an
/// `RstModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RstModel {
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// The block arena, in document order.
    pub blocks: Vec<RstBlock>,
    /// The inline-span arena, in document order.
    pub inlines: Vec<RstInline>,
}

impl RstModel {
    /// The block at `index`, if present.
    pub fn block(&self, index: u32) -> Option<&RstBlock> {
        self.blocks.get(index as usize)
    }

    /// The inline at `index`, if present.
    pub fn inline(&self, index: u32) -> Option<&RstInline> {
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

    /// The document's maximum title hierarchy level observed (0 if there are none).
    pub fn max_title_level(&self) -> u8 {
        self.blocks
            .iter()
            .filter(|b| b.kind == B_TITLE)
            .map(|b| b.level)
            .max()
            .unwrap_or(0)
    }

    /// Whether the source carries a **reST-specific** structural mark that admits it
    /// as reStructuredText. See the module docs for the Markdown boundary.
    pub fn has_structural_signal(&self) -> bool {
        self.blocks.iter().any(|b| match b.kind {
            B_COMMENT | B_DIRECTIVE | B_SUBSTITUTION_DEF | B_FOOTNOTE_DEF | B_HYPERLINK_TARGET
            | B_FIELD | B_GRID_TABLE | B_SIMPLE_TABLE => true,
            B_TITLE => b
                .info
                .as_deref()
                .and_then(|a| a.bytes().next())
                .map(detection_adornment)
                .unwrap_or(false),
            _ => false,
        })
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64 + self.blocks.len() * 48);
        out.extend_from_slice(b"RST1");
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
    pub fn decode(bytes: &[u8]) -> Result<RstModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"RST1" {
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
            if flags & !(F_ORDERED | F_OVERLINE) != 0 {
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
            blocks.push(RstBlock {
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
            inlines.push(RstInline {
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
        Ok(RstModel {
            doc_len,
            blocks,
            inlines,
        })
    }
}

/// Byte-based reStructuredText detector. See the module docs for the heuristic and
/// the Markdown boundary it cannot cross.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.is_empty() {
        return false;
    }
    if source.len() as u64 > limits.max_rst_document_bytes {
        return false;
    }
    if looks_like_other(source, limits) {
        return false;
    }
    match parse(source, limits, false) {
        Ok(m) => m.has_structural_signal(),
        Err(_) => false,
    }
}

/// Whether `source` belongs to a family that is *not* reStructuredText (defence in
/// depth; the dispatcher also orders PDF/ZIP/JSON/YAML/CSV/Markdown ahead of reST).
fn looks_like_other(source: &[u8], limits: Limits) -> bool {
    let _ = limits;
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
    // A document that Markdown already admits must never be stolen (defence in depth;
    // the dispatcher tries Markdown first, so this is belt and suspenders).
    #[cfg(feature = "markdown")]
    if crate::adapter::markdown::detect(source, limits) {
        return true;
    }
    false
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of an `RstModel` node).
pub fn build_rst_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits, true)?.encode())
}

/// Parse `source` into an [`RstModel`]. `build` selects whether the inline arena is
/// populated (detection runs with `build = false`, so a detection-style call never
/// scans inline spans).
pub fn parse(source: &[u8], limits: Limits, build: bool) -> Result<RstModel> {
    if source.len() as u64 > limits.max_rst_document_bytes {
        return Err(Error::resource_limit(format!(
            "reStructuredText source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_rst_document_bytes
        )));
    }
    let mut p = Parser::new(source, limits, build);
    p.run()?;
    Ok(RstModel {
        doc_len: source.len() as u64,
        blocks: p.blocks,
        inlines: p.inlines,
    })
}

/// The exact source bytes of a block's whole span (`[start, end)`).
pub fn block_bytes<'a>(source: &'a [u8], block: &RstBlock) -> Result<&'a [u8]> {
    slice(
        source,
        block.start,
        block.end,
        "block span is outside the source",
    )
}

/// The exact source bytes of a block's content span (`[content_start, content_end)`).
pub fn content_bytes<'a>(source: &'a [u8], block: &RstBlock) -> Result<&'a [u8]> {
    slice(
        source,
        block.content_start,
        block.content_end,
        "block content span is outside the source",
    )
}

/// The exact source bytes of an inline's whole span (`[start, end)`).
pub fn inline_bytes<'a>(source: &'a [u8], inline: &RstInline) -> Result<&'a [u8]> {
    slice(
        source,
        inline.start,
        inline.end,
        "inline span is outside the source",
    )
}

/// The exact source bytes of an inline's inner text span.
pub fn inline_text_bytes<'a>(source: &'a [u8], inline: &RstInline) -> Result<&'a [u8]> {
    slice(
        source,
        inline.inner_start,
        inline.inner_end,
        "inline text span is outside the source",
    )
}

/// A deterministic canonical text projection: the exact source bytes, lossily
/// decoded. reStructuredText is **not** rendered, so the canonical text is the
/// source itself. Declines typed if it would exceed `max_out`.
pub fn canonical_text(source: &[u8], _limits: Limits, max_out: u64) -> Result<String> {
    if source.len() as u64 > max_out {
        return Err(Error::resource_limit(format!(
            "reStructuredText text projection exceeds the {max_out}-byte budget"
        )));
    }
    Ok(String::from_utf8_lossy(source).into_owned())
}

/// One lexical-match record from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RstMatch {
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
pub fn find(source: &[u8], model: &RstModel, pattern: &str, max_out: u64) -> Result<Vec<RstMatch>> {
    if pattern.is_empty() {
        return Err(Error::usage(
            "reStructuredText find pattern must not be empty",
        ));
    }
    let needle = pattern.as_bytes();
    let mut out: Vec<RstMatch> = Vec::new();
    let mut estimated: u64 = 0;
    for (i, b) in model.blocks.iter().enumerate() {
        let content = content_bytes(source, b)?;
        if contains(content, needle) {
            estimated = estimated.saturating_add(32 + content.len() as u64);
            if estimated > max_out {
                return Err(Error::resource_limit(format!(
                    "reStructuredText find exceeded the {max_out}-byte budget"
                )));
            }
            out.push(RstMatch {
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
}

struct Parser<'a> {
    b: &'a [u8],
    lines: Vec<Line>,
    limits: Limits,
    build: bool,
    blocks: Vec<RstBlock>,
    inlines: Vec<RstInline>,
    node_count: u64,
    inline_count: u64,
    probe: u64,
    /// Adornment signatures in first-seen order (the title hierarchy).
    title_styles: Vec<String>,
}

/// A detected section title.
struct TitleInfo {
    /// Index of the first line after the title block.
    after: usize,
    /// Whether the title carries an overline.
    overline: bool,
    /// The exact underline adornment string.
    adornment: String,
}

/// A detected grid/simple table.
struct TableInfo {
    /// Index of the first line after the table.
    after: usize,
    /// The number of content rows (borders/separators excluded).
    rows: u32,
    /// The number of columns.
    cols: u32,
}

/// A detected field-list entry.
struct FieldInfo {
    /// The field name (between the colons).
    name: String,
    /// The offset (within the line) at which the value starts.
    value_off: usize,
}

/// A detected option-list entry.
struct OptionInfo {
    /// The option spec (e.g. `-a, --all`).
    spec: String,
    /// The offset (within the line) at which the description starts.
    desc_off: usize,
}

/// A detected list marker.
struct ListMarker {
    ordered: bool,
    /// The marker spelling (e.g. `*`, `1.`, `#.`).
    marker: String,
    /// The leading indent.
    indent: usize,
    /// The offset (within the line) at which the item content starts.
    content_off: usize,
}

impl<'a> Parser<'a> {
    fn new(b: &'a [u8], limits: Limits, build: bool) -> Self {
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
            probe,
            title_styles: Vec::new(),
        }
    }

    fn lb(&self, i: usize) -> &'a [u8] {
        let l = &self.lines[i];
        &self.b[l.start..l.content_end]
    }

    fn run(&mut self) -> Result<()> {
        for i in 0..self.lines.len() {
            let l = &self.lines[i];
            if (l.content_end - l.start) as u64 > self.limits.max_rst_line_bytes {
                return Err(Error::resource_limit(format!(
                    "reStructuredText line {} exceeds the {}-byte line cap",
                    i, self.limits.max_rst_line_bytes
                )));
            }
        }
        let mut i = 0usize;
        while i < self.lines.len() {
            if is_blank(self.lb(i)) {
                i += 1;
                continue;
            }
            if (i == 0 || is_blank(self.lb(i - 1)))
                && let Some(t) = self.section_title(i)
            {
                self.emit_title(i, &t)?;
                i = t.after;
                continue;
            }
            if let Some(t) = self.grid_table(i) {
                self.emit_table(B_GRID_TABLE, i, &t, true)?;
                i = t.after;
                continue;
            }
            if let Some(t) = self.simple_table(i) {
                self.emit_table(B_SIMPLE_TABLE, i, &t, false)?;
                i = t.after;
                continue;
            }
            if self.is_explicit_markup(i) {
                i = self.emit_explicit(i)?;
                continue;
            }
            if self.is_doctest(i) {
                i = self.emit_doctest(i)?;
                continue;
            }
            if let Some(f) = self.field_line(i) {
                i = self.emit_field(i, &f)?;
                continue;
            }
            if let Some(o) = self.option_line(i) {
                i = self.emit_option(i, &o)?;
                continue;
            }
            if let Some(m) = parse_list_marker(self.lb(i)) {
                i = self.emit_list_item(i, &m)?;
                continue;
            }
            i = self.emit_paragraph(i)?;
        }
        Ok(())
    }

    // -- section titles -----------------------------------------------------

    /// Detect a section title at line `i` (already known to be at the start of a
    /// text block: indent 0 and preceded by a blank line or the document start).
    fn section_title(&self, i: usize) -> Option<TitleInfo> {
        let lb = self.lb(i);
        if is_blank(lb) || leading_spaces(lb) != 0 {
            return None;
        }
        // Overline form: adornment, non-blank title, same adornment.
        if lb.len() >= 2
            && let Some(ch) = adornment_run(lb)
            && i + 2 < self.lines.len()
        {
            let mid = self.lb(i + 1);
            let under = self.lb(i + 2);
            if !is_blank(mid)
                && leading_spaces(mid) == 0
                && adornment_run(mid).is_none()
                && adornment_run(under) == Some(ch)
                && under.len() == lb.len()
            {
                return Some(TitleInfo {
                    after: i + 3,
                    overline: true,
                    adornment: String::from_utf8_lossy(lb).into_owned(),
                });
            }
        }
        // Underline form: title line, then a pure adornment at least as long.
        if adornment_run(lb).is_some() || lb.is_empty() {
            return None;
        }
        if i + 1 >= self.lines.len() {
            return None;
        }
        let under = self.lb(i + 1);
        let ch = adornment_run(under)?;
        if under.len() < lb.len() {
            return None;
        }
        // The title text must not itself begin with the adornment run char repeated
        // (that would be a decorative separator, not a title).
        if lb.iter().all(|&c| c == ch) {
            return None;
        }
        Some(TitleInfo {
            after: i + 2,
            overline: false,
            adornment: String::from_utf8_lossy(under).into_owned(),
        })
    }

    fn emit_title(&mut self, i: usize, t: &TitleInfo) -> Result<()> {
        let l = &self.lines[i];
        let start = l.start as u64;
        let last = if t.overline { i + 2 } else { i + 1 };
        let end = self.lines[last].content_end as u64;
        let (cs, ce) = if t.overline {
            let tl = &self.lines[i + 1];
            (tl.start as u64, tl.content_end as u64)
        } else {
            (l.start as u64, l.content_end as u64)
        };
        let sig = format!(
            "{}{}",
            t.adornment.as_bytes().first().copied().unwrap_or(0) as char,
            if t.overline { "O" } else { "U" }
        );
        let level = match self.title_styles.iter().position(|s| s == &sig) {
            Some(p) => p + 1,
            None => {
                self.title_styles.push(sig);
                self.title_styles.len()
            }
        };
        let level = (level as u32).min(self.limits.max_rst_depth);
        self.note_depth(level)?;
        let mut flags = 0u8;
        if t.overline {
            flags |= F_OVERLINE;
        }
        let idx = self.push_block(
            B_TITLE,
            start,
            end,
            cs,
            ce,
            level.min(255) as u8,
            flags,
            Some(t.adornment.clone()),
            None,
            None,
        )?;
        self.scan_block_inlines(idx)?;
        Ok(())
    }

    // -- tables -------------------------------------------------------------

    /// Detect a grid table at line `i` (a `+`/`-`/`=` border followed by
    /// `|`-delimited rows and a closing border).
    fn grid_table(&self, i: usize) -> Option<TableInfo> {
        if !looks_like_grid_border(self.lb(i)) {
            return None;
        }
        let top = self.lb(i);
        let cols = top.iter().filter(|&&c| c == b'+').count().saturating_sub(1) as u32;
        if cols == 0 {
            return None;
        }
        let mut j = i + 1;
        let mut rows = 0u32;
        let mut closed = false;
        while j < self.lines.len() {
            let lb = self.lb(j);
            if looks_like_grid_border(lb) && !lb.is_empty() && lb[0] == b'+' {
                j += 1;
                closed = true;
                continue;
            }
            if lb.first() == Some(&b'|') {
                rows += 1;
                j += 1;
                continue;
            }
            break;
        }
        if !closed || rows == 0 {
            return None;
        }
        Some(TableInfo {
            after: j,
            rows,
            cols,
        })
    }

    /// Detect a simple table at line `i` (an `=`-run rule with two or more runs).
    fn simple_table(&self, i: usize) -> Option<TableInfo> {
        let top = self.lb(i);
        let runs = simple_rule_runs(top)?;
        if runs.len() < 2 {
            return None;
        }
        // Gather the whole non-blank run; the last line must be a closing rule equal
        // to the top rule, and every interior line must be a rule or a content row.
        let mut j = i + 1;
        while j < self.lines.len() && !is_blank(self.lb(j)) {
            j += 1;
        }
        if j <= i + 1 {
            return None;
        }
        let last = j - 1;
        if last == i || self.lb(last) != top {
            return None;
        }
        let mut rows = 0u32;
        for k in i + 1..last {
            let lb = self.lb(k);
            if lb == top {
                continue;
            }
            if leading_spaces(lb) != 0 {
                return None;
            }
            rows += 1;
        }
        if rows == 0 {
            return None;
        }
        Some(TableInfo {
            after: j,
            rows,
            cols: runs.len() as u32,
        })
    }

    fn emit_table(&mut self, kind: u8, i: usize, t: &TableInfo, grid: bool) -> Result<()> {
        let start = self.lines[i].start as u64;
        let end = self.lines[t.after - 1].content_end as u64;
        let idx = self.push_block(kind, start, end, start, end, 0, 0, None, None, None)?;
        self.blocks[idx].rows = t.rows;
        self.blocks[idx].cols = t.cols;
        if self.build {
            self.scan_table_cells(idx, i, t.after, grid)?;
        }
        Ok(())
    }

    // -- explicit markup ----------------------------------------------------

    /// Whether line `i` starts an explicit-markup construct (`.. ` at indent ≤ 3).
    fn is_explicit_markup(&self, i: usize) -> bool {
        let lb = self.lb(i);
        if leading_spaces(lb) > 3 {
            return false;
        }
        let lb = &lb[leading_spaces(lb)..];
        lb == b".." || lb.starts_with(b".. ")
    }

    fn emit_explicit(&mut self, i: usize) -> Result<usize> {
        let base_indent = leading_spaces(self.lb(i));
        let start = self.lines[i].start;
        let lb = self.lb(i);
        let after_dots = leading_spaces(lb) + 3;
        let rest = if lb.len() > after_dots {
            &lb[after_dots..]
        } else {
            &[]
        };
        let (kind, info, target, title, content_off) = if let Some(t) = rest.strip_prefix(b"_") {
            // Hyperlink target: `_name:` optionally followed by a URI.
            let colon = t.iter().position(|&c| c == b':');
            match colon {
                Some(c) => {
                    let name = String::from_utf8_lossy(&t[..c]).into_owned();
                    let uri_off = leading_spaces(&t[c + 1..]);
                    let uri_bytes = &t[c + 1 + uri_off..];
                    let uri = String::from_utf8_lossy(uri_bytes).trim().to_string();
                    let target = if uri.is_empty() { None } else { Some(uri) };
                    (B_HYPERLINK_TARGET, name, target, None, after_dots + c + 1)
                }
                None => (B_COMMENT, "target".to_string(), None, None, after_dots),
            }
        } else if let Some(t) = rest.strip_prefix(b"[") {
            // Footnote/citation definition: `[label] body`.
            match t.iter().position(|&c| c == b']') {
                Some(c) => {
                    let label = String::from_utf8_lossy(&t[..c]).into_owned();
                    let mut off = after_dots + 1 + c + 1;
                    if lb.get(off) == Some(&b' ') {
                        off += 1;
                    }
                    (B_FOOTNOTE_DEF, label, None, None, off)
                }
                None => (B_COMMENT, "footnote".to_string(), None, None, after_dots),
            }
        } else if let Some(t) = rest.strip_prefix(b"|") {
            // Substitution definition: `|name| directive::`.
            match t.iter().position(|&c| c == b'|') {
                Some(c) => {
                    let name = String::from_utf8_lossy(&t[..c]).into_owned();
                    let tail = String::from_utf8_lossy(&t[c + 1..]);
                    let directive = tail.split("::").next().unwrap_or("").trim().to_string();
                    (B_SUBSTITUTION_DEF, name, None, Some(directive), after_dots)
                }
                None => (
                    B_COMMENT,
                    "substitution".to_string(),
                    None,
                    None,
                    after_dots,
                ),
            }
        } else if let Some((name, arg)) = parse_directive(rest) {
            (B_DIRECTIVE, name, None, arg, after_dots)
        } else {
            (B_COMMENT, String::new(), None, None, after_dots)
        };

        let (end_idx, end) = self.gather_indented(i, base_indent);
        let content_start = (start + content_off).min(end as usize) as u64;
        let info = if info.is_empty() { None } else { Some(info) };
        self.push_block(
            kind,
            start as u64,
            end,
            content_start,
            end,
            0,
            0,
            info,
            target,
            title,
        )?;
        Ok(end_idx)
    }

    /// Gather the block lines that continue `i`: blank lines and lines indented past
    /// `base_indent`. Returns `(next_line, last_content_end)`.
    fn gather_indented(&self, i: usize, base_indent: usize) -> (usize, u64) {
        let mut j = i + 1;
        let mut last_end = self.lines[i].content_end as u64;
        while j < self.lines.len() {
            let lb = self.lb(j);
            if is_blank(lb) {
                j += 1;
                continue;
            }
            if leading_spaces(lb) > base_indent {
                last_end = self.lines[j].content_end as u64;
                j += 1;
            } else {
                break;
            }
        }
        (j, last_end)
    }

    // -- doctest ------------------------------------------------------------

    fn is_doctest(&self, i: usize) -> bool {
        let lb = self.lb(i);
        leading_spaces(lb) == 0 && (lb == b">>>" || lb.starts_with(b">>> "))
    }

    fn emit_doctest(&mut self, i: usize) -> Result<usize> {
        let start = self.lines[i].start as u64;
        let mut j = i + 1;
        let mut last_end = self.lines[i].content_end as u64;
        while j < self.lines.len() && !is_blank(self.lb(j)) {
            last_end = self.lines[j].content_end as u64;
            j += 1;
        }
        self.push_block(
            B_DOCTEST, start, last_end, start, last_end, 0, 0, None, None, None,
        )?;
        Ok(j)
    }

    // -- field lists --------------------------------------------------------

    fn field_line(&self, i: usize) -> Option<FieldInfo> {
        let lb = self.lb(i);
        if leading_spaces(lb) > 3 {
            return None;
        }
        let lb = &lb[leading_spaces(lb)..];
        if lb.first() != Some(&b':') {
            return None;
        }
        let close = lb[1..].iter().position(|&c| c == b':')? + 1;
        let name = &lb[1..close];
        if name.is_empty() || name.iter().any(|&c| c == b' ' || c == b'\t') {
            return None;
        }
        let after = close + 1;
        match lb.get(after) {
            None => Some(FieldInfo {
                name: String::from_utf8_lossy(name).into_owned(),
                value_off: after,
            }),
            Some(&b' ') | Some(&b'\t') => {
                let mut off = after;
                while lb.get(off) == Some(&b' ') {
                    off += 1;
                }
                Some(FieldInfo {
                    name: String::from_utf8_lossy(name).into_owned(),
                    value_off: off,
                })
            }
            _ => None,
        }
    }

    fn emit_field(&mut self, i: usize, f: &FieldInfo) -> Result<usize> {
        let base_indent = leading_spaces(self.lb(i));
        let start = self.lines[i].start;
        let cs = start + f.value_off;
        let (j, end) = self.gather_indented(i, base_indent);
        let idx = self.push_block(
            B_FIELD,
            start as u64,
            end,
            cs.min(end as usize) as u64,
            end,
            0,
            0,
            Some(f.name.clone()),
            None,
            None,
        )?;
        self.scan_block_inlines(idx)?;
        Ok(j)
    }

    // -- option lists -------------------------------------------------------

    fn option_line(&self, i: usize) -> Option<OptionInfo> {
        let lb = self.lb(i);
        if leading_spaces(lb) > 3 {
            return None;
        }
        let lb = &lb[leading_spaces(lb)..];
        let first = *lb.first()?;
        if !matches!(first, b'-' | b'+' | b'/') {
            return None;
        }
        let second = *lb.get(1)?;
        let looks_option = (first == b'-'
            && second == b'-'
            && lb.get(2).is_some_and(|c| c.is_ascii_alphanumeric()))
            || (second.is_ascii_alphabetic() && (first == b'-' || first == b'+' || first == b'/'));
        if !looks_option {
            return None;
        }
        // The description starts after a run of two or more spaces (or at EOL).
        let mut off = lb.len();
        let mut k = 0usize;
        while k + 1 < lb.len() {
            if lb[k] == b' ' && lb[k + 1] == b' ' {
                off = k;
                break;
            }
            k += 1;
        }
        let spec = String::from_utf8_lossy(&lb[..off]).trim().to_string();
        let mut desc_off = off;
        while lb.get(desc_off) == Some(&b' ') {
            desc_off += 1;
        }
        Some(OptionInfo { spec, desc_off })
    }

    fn emit_option(&mut self, i: usize, o: &OptionInfo) -> Result<usize> {
        let base_indent = leading_spaces(self.lb(i));
        let start = self.lines[i].start;
        let lb = self.lb(i);
        let desc = String::from_utf8_lossy(&lb[o.desc_off.min(lb.len())..])
            .trim()
            .to_string();
        let (j, end) = self.gather_indented(i, base_indent);
        let cs = (start + o.desc_off) as u64;
        let title = if desc.is_empty() { None } else { Some(desc) };
        let idx = self.push_block(
            B_OPTION,
            start as u64,
            end,
            cs.min(end),
            end,
            0,
            0,
            Some(o.spec.clone()),
            None,
            title,
        )?;
        self.scan_block_inlines(idx)?;
        Ok(j)
    }

    // -- list items ---------------------------------------------------------

    fn emit_list_item(&mut self, i: usize, m: &ListMarker) -> Result<usize> {
        let start = self.lines[i].start;
        let mut j = i + 1;
        let mut last_end = self.lines[i].content_end;
        while j < self.lines.len() {
            let lb = self.lb(j);
            if is_blank(lb) {
                j += 1;
                continue;
            }
            if leading_spaces(lb) > m.indent {
                last_end = self.lines[j].content_end;
                j += 1;
            } else {
                break;
            }
        }
        let cs = (start + m.content_off) as u64;
        let ce = last_end as u64;
        let mut flags = 0u8;
        if m.ordered {
            flags |= F_ORDERED;
        }
        let raw_level = (m.indent / 2 + 1) as u32;
        self.note_depth(raw_level)?;
        let level = raw_level.min(self.limits.max_rst_depth);
        let idx = self.push_block(
            B_LIST_ITEM,
            start as u64,
            ce,
            cs.min(ce),
            ce,
            level.min(255) as u8,
            flags,
            Some(m.marker.clone()),
            None,
            None,
        )?;
        self.scan_block_inlines(idx)?;
        Ok(j)
    }

    // -- paragraphs (and literal blocks / definition lists) -----------------

    fn emit_paragraph(&mut self, i: usize) -> Result<usize> {
        let indent0 = leading_spaces(self.lb(i));
        let start = self.lines[i].start;
        let mut j = i + 1;
        let mut last_end = self.lines[i].content_end;
        while j < self.lines.len() && !is_blank(self.lb(j)) {
            if leading_spaces(self.lb(j)) > indent0 || self.interrupts_paragraph(j) {
                break;
            }
            last_end = self.lines[j].content_end;
            j += 1;
        }

        // A literal block: the paragraph ends with `::`, and an indented block
        // follows (blank-separated). The `::` paragraph is preserved as-is.
        let text = &self.b[start..last_end];
        let ends_double_colon = trim_end(text).ends_with(b"::");
        if ends_double_colon {
            let mut k = j;
            while k < self.lines.len() && is_blank(self.lb(k)) {
                k += 1;
            }
            if k < self.lines.len() && leading_spaces(self.lb(k)) > indent0 {
                // Emit the `::` paragraph first (the marker lives in the paragraph).
                let pidx = self.push_block(
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
                self.scan_block_inlines(pidx)?;
                let (after, lit_end) = self.gather_indented(k - 1, indent0);
                let lit_start = self.lines[k].start;
                self.push_block(
                    B_LITERAL_BLOCK,
                    lit_start as u64,
                    lit_end,
                    lit_start as u64,
                    lit_end,
                    0,
                    0,
                    Some("::".to_string()),
                    None,
                    None,
                )?;
                return Ok(after);
            }
        }

        // A definition list: a single-line term at indent 0 followed directly by a
        // more-indented definition (and the term does not end with `::`).
        if j == i + 1
            && j < self.lines.len()
            && !is_blank(self.lb(j))
            && leading_spaces(self.lb(j)) > indent0
            && !ends_double_colon
        {
            let term = String::from_utf8_lossy(self.lb(i)).trim().to_string();
            let def_start = self.lines[j].start;
            let (after, def_end) = self.gather_indented(j - 1, indent0);
            let idx = self.push_block(
                B_DEFINITION,
                start as u64,
                def_end,
                def_start as u64,
                def_end,
                0,
                0,
                Some(term),
                None,
                None,
            )?;
            self.scan_block_inlines(idx)?;
            return Ok(after);
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
        Ok(j)
    }

    /// Whether line `j` (non-blank, at indent 0) begins a new block that interrupts
    /// an open paragraph.
    fn interrupts_paragraph(&self, j: usize) -> bool {
        let lb = self.lb(j);
        if leading_spaces(lb) > 0 {
            return true;
        }
        self.is_explicit_markup(j)
            || self.is_doctest(j)
            || self.field_line(j).is_some()
            || self.option_line(j).is_some()
            || parse_list_marker(lb).is_some()
            || looks_like_grid_border(lb)
            || looks_like_simple_rule(lb)
    }

    // -- block / inline arena -----------------------------------------------

    fn note_depth(&mut self, depth: u32) -> Result<()> {
        if depth > self.limits.max_rst_depth {
            return Err(Error::resource_limit(format!(
                "reStructuredText nesting depth {depth} exceeds the {}-deep cap",
                self.limits.max_rst_depth
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
        if self.blocks.len() as u64 >= self.limits.max_rst_blocks as u64 {
            return Err(Error::resource_limit(format!(
                "reStructuredText document exceeds the {}-block cap",
                self.limits.max_rst_blocks
            )));
        }
        self.node_count = self.node_count.saturating_add(1);
        if self.node_count > self.limits.max_rst_nodes as u64 {
            return Err(Error::resource_limit(format!(
                "reStructuredText document exceeds the {}-node cap",
                self.limits.max_rst_nodes
            )));
        }
        let idx = self.blocks.len();
        self.blocks.push(RstBlock {
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
        if self.inline_count > self.limits.max_rst_inline_spans as u64 {
            return Err(Error::resource_limit(format!(
                "reStructuredText document exceeds the {}-inline-span cap",
                self.limits.max_rst_inline_spans
            )));
        }
        self.node_count = self.node_count.saturating_add(1);
        if self.node_count > self.limits.max_rst_nodes as u64 {
            return Err(Error::resource_limit(format!(
                "reStructuredText document exceeds the {}-node cap",
                self.limits.max_rst_nodes
            )));
        }
        let idx = self.inlines.len() as u32;
        self.blocks[block].inlines.push(idx);
        self.inlines.push(RstInline {
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

    fn scan_table_cells(
        &mut self,
        block: usize,
        first: usize,
        last: usize,
        grid: bool,
    ) -> Result<()> {
        if grid {
            for row in first..last {
                let lb = self.lb(row);
                if lb.first() != Some(&b'|') {
                    continue;
                }
                let base = self.lines[row].start;
                for (cs, ce) in grid_cell_spans(lb) {
                    self.push_inline(
                        block,
                        I_TABLE_CELL,
                        base + cs,
                        base + ce,
                        base + cs,
                        base + ce,
                        None,
                        None,
                    )?;
                }
            }
        } else {
            let top = self.lb(first);
            let runs = simple_rule_runs(top).unwrap_or_default();
            for row in first + 1..last {
                let l = &self.lines[row];
                if &self.b[l.start..l.content_end] == top {
                    continue;
                }
                let base = l.start;
                let len = l.content_end - l.start;
                for &(rs, re) in &runs {
                    let s = rs.min(len);
                    let e = re.min(len);
                    if s < e {
                        self.push_inline(
                            block,
                            I_TABLE_CELL,
                            base + s,
                            base + e,
                            base + s,
                            base + e,
                            None,
                            None,
                        )?;
                    }
                }
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
                b'*' => {
                    if let Some((k, is, ie, we)) = self.emphasis(i, end) {
                        self.push_inline(block, k, i, we, is, ie, None, None)?;
                        i = we;
                        continue;
                    }
                }
                b'`' => {
                    if let Some((k, is, ie, we, role, name)) = self.backtick(i, end) {
                        self.push_inline(block, k, i, we, is, ie, role, name)?;
                        i = we;
                        continue;
                    }
                }
                b'|' => {
                    if let Some((is, ie, we)) = self.substitution(i, end) {
                        self.push_inline(block, I_SUBSTITUTION_REF, i, we, is, ie, None, None)?;
                        i = we;
                        continue;
                    }
                }
                b'[' => {
                    if let Some((is, ie, we)) = self.footnote_ref(i, end) {
                        self.push_inline(block, I_FOOTNOTE_REF, i, we, is, ie, None, None)?;
                        i = we;
                        continue;
                    }
                }
                c if c.is_ascii_alphanumeric() => {
                    if let Some((k, is, ie, we)) = self.bare_ref(i, end, start) {
                        let name = String::from_utf8_lossy(&self.b[is..ie]).into_owned();
                        self.push_inline(block, k, i, we, is, ie, None, Some(name))?;
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

    /// Try strong/emphasis at `i`. Returns `(kind, inner_start, inner_end, whole_end)`.
    fn emphasis(&mut self, i: usize, end: usize) -> Option<(u8, usize, usize, usize)> {
        let strong = i + 1 < end && self.b[i + 1] == b'*';
        let open_len = if strong { 2 } else { 1 };
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
            if self.b[j] == b'*' {
                let mut rr = 0;
                while j + rr < end && self.b[j + rr] == b'*' {
                    rr += 1;
                }
                let need = if strong { 2 } else { 1 };
                if rr >= need && self.b[j - 1] != b' ' {
                    let kind = if strong { I_STRONG } else { I_EMPHASIS };
                    return Some((kind, inner0, j, j + need));
                }
                j += rr.max(1);
            } else {
                j += 1;
            }
        }
        None
    }

    /// Try the backtick family at `i`: inline literal (``` ``…`` ```), an interpreted
    /// text (`` `…`:role: ``), or a hyperlink/anonymous reference (`` `name`_ `` /
    /// `` `name`__ ``). Returns `(kind, inner_start, inner_end, whole_end, target,
    /// title)`.
    #[allow(clippy::type_complexity)]
    fn backtick(
        &mut self,
        i: usize,
        end: usize,
    ) -> Option<(u8, usize, usize, usize, Option<String>, Option<String>)> {
        if i + 1 < end && self.b[i + 1] == b'`' {
            // Inline literal: `` … ``.
            let inner0 = i + 2;
            if inner0 >= end {
                return None;
            }
            let mut j = inner0;
            while j + 1 < end {
                if self.probe == 0 {
                    return None;
                }
                self.probe -= 1;
                if self.b[j] == b'`' && self.b[j + 1] == b'`' {
                    return Some((I_LITERAL, inner0, j, j + 2, None, None));
                }
                j += 1;
            }
            return None;
        }
        // Single-backtick phrase: interpreted text or a reference.
        let inner0 = i + 1;
        if inner0 >= end || self.b[inner0].is_ascii_whitespace() {
            return None;
        }
        let mut j = inner0;
        while j < end {
            if self.probe == 0 {
                return None;
            }
            self.probe -= 1;
            if self.b[j] == b'`' {
                break;
            }
            j += 1;
        }
        if j >= end || self.b[j] != b'`' {
            return None;
        }
        // Reference: `` `name`_ `` or `` `name`__ ``.
        if self.b.get(j + 1) == Some(&b'_') {
            let name = String::from_utf8_lossy(&self.b[inner0..j]).into_owned();
            if self.b.get(j + 2) == Some(&b'_') {
                return Some((I_ANON_REF, inner0, j, j + 3, None, Some(name)));
            }
            return Some((I_HYPERLINK_REF, inner0, j, j + 2, None, Some(name)));
        }
        // Interpreted text: `` `text`:role: ``.
        if self.b.get(j + 1) == Some(&b':') {
            let mut k = j + 2;
            while k < end && self.b[k] != b':' && self.b[k] != b' ' && self.b[k] != b'\n' {
                k += 1;
            }
            if k < end && self.b[k] == b':' {
                let role = String::from_utf8_lossy(&self.b[j + 2..k]).into_owned();
                return Some((I_INTERPRETED, inner0, j, k + 1, Some(role), None));
            }
        }
        // Interpreted text with the default role.
        Some((I_INTERPRETED, inner0, j, j + 1, None, None))
    }

    /// Try a substitution reference `|name|` at `i`. Returns
    /// `(inner_start, inner_end, whole_end)`.
    fn substitution(&mut self, i: usize, end: usize) -> Option<(usize, usize, usize)> {
        let inner0 = i + 1;
        if inner0 >= end || !self.b[inner0].is_ascii_alphanumeric() {
            return None;
        }
        let mut j = inner0;
        while j < end {
            if self.probe == 0 {
                return None;
            }
            self.probe -= 1;
            if self.b[j] == b'|' && self.b[j - 1] != b' ' {
                return Some((inner0, j, j + 1));
            }
            if self.b[j] == b'\n' {
                return None;
            }
            j += 1;
        }
        None
    }

    /// Try a footnote/citation reference `[label]_` at `i`. Returns
    /// `(inner_start, inner_end, whole_end)`.
    fn footnote_ref(&mut self, i: usize, end: usize) -> Option<(usize, usize, usize)> {
        let inner0 = i + 1;
        let mut j = inner0;
        while j < end {
            if self.probe == 0 {
                return None;
            }
            self.probe -= 1;
            if self.b[j] == b']' {
                if j > inner0 && self.b.get(j + 1) == Some(&b'_') {
                    return Some((inner0, j, j + 2));
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

    /// Try a bare hyperlink/anonymous reference `name_` / `name__` at `i`. Returns
    /// `(kind, inner_start, inner_end, whole_end)`.
    fn bare_ref(
        &mut self,
        i: usize,
        end: usize,
        region_start: usize,
    ) -> Option<(u8, usize, usize, usize)> {
        if i > region_start {
            let prev = self.b[i - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'-' {
                return None;
            }
        }
        let mut j = i;
        while j < end {
            if self.probe == 0 {
                return None;
            }
            self.probe -= 1;
            if self.b[j].is_ascii_alphanumeric() || self.b[j] == b'-' || self.b[j] == b'.' {
                j += 1;
            } else {
                break;
            }
        }
        if j == i || j >= end || self.b[j] != b'_' {
            return None;
        }
        if self.b.get(j + 1) == Some(&b'_') {
            return Some((I_ANON_REF, i, j, j + 2));
        }
        Some((I_HYPERLINK_REF, i, j, j + 1))
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
        out.push(Line { start, content_end });
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

fn trim_end(lb: &[u8]) -> &[u8] {
    let mut e = lb.len();
    while e > 0 && (lb[e - 1] == b' ' || lb[e - 1] == b'\t') {
        e -= 1;
    }
    &lb[..e]
}

/// Whether `c` is a reStructuredText section-adornment character.
fn adornment_char(c: u8) -> bool {
    matches!(
        c,
        b'=' | b'-'
            | b'`'
            | b':'
            | b'.'
            | b'\''
            | b'"'
            | b'~'
            | b'^'
            | b'_'
            | b'*'
            | b'+'
            | b'#'
            | b'<'
            | b'>'
    )
}

/// Whether `c` is accepted as a **standalone** reST detection signal (see module
/// docs: `- * _ ~ #` are Markdown-claimed and deliberately excluded).
fn detection_adornment(c: u8) -> bool {
    matches!(
        c,
        b'=' | b'+' | b'^' | b'"' | b'\'' | b':' | b'.' | b'<' | b'>'
    )
}

/// If `lb` is a pure adornment line (one repeated adornment char, length ≥ 2),
/// return the char.
fn adornment_run(lb: &[u8]) -> Option<u8> {
    let first = *lb.first()?;
    if lb.len() < 2 || !adornment_char(first) {
        return None;
    }
    if lb.iter().all(|&c| c == first) {
        Some(first)
    } else {
        None
    }
}

/// Whether `lb` looks like a grid-table border (`+`-led, all `+-=`).
fn looks_like_grid_border(lb: &[u8]) -> bool {
    if lb.len() < 2 || lb[0] != b'+' {
        return false;
    }
    let mut has_dash = false;
    for &c in lb {
        match c {
            b'-' | b'=' => has_dash = true,
            b'+' => {}
            _ => return false,
        }
    }
    has_dash
}

/// Whether `lb` looks like a simple-table top rule (`=`-led, `=` and spaces, with
/// at least one space separating runs).
fn looks_like_simple_rule(lb: &[u8]) -> bool {
    if lb.is_empty() || lb[0] != b'=' {
        return false;
    }
    let mut has_space = false;
    for &c in lb {
        match c {
            b'=' => {}
            b' ' => has_space = true,
            _ => return false,
        }
    }
    has_space
}

/// The `=`-run spans of a simple-table rule, if `lb` is one. Returns `(start, end)`
/// offsets within `lb`.
fn simple_rule_runs(lb: &[u8]) -> Option<Vec<(usize, usize)>> {
    if !looks_like_simple_rule(lb) {
        return None;
    }
    let mut runs = Vec::new();
    let mut i = 0usize;
    while i < lb.len() {
        if lb[i] == b'=' {
            let s = i;
            while i < lb.len() && lb[i] == b'=' {
                i += 1;
            }
            runs.push((s, i));
        } else {
            i += 1;
        }
    }
    if runs.is_empty() { None } else { Some(runs) }
}

/// The trimmed cell spans of a grid-table row (offsets within `lb`).
fn grid_cell_spans(lb: &[u8]) -> Vec<(usize, usize)> {
    let mut bars = Vec::new();
    for (idx, &c) in lb.iter().enumerate() {
        if c == b'|' {
            bars.push(idx);
        }
    }
    let mut out = Vec::new();
    for w in bars.windows(2) {
        let mut a = w[0] + 1;
        let mut b = w[1];
        while a < b && (lb[a] == b' ' || lb[a] == b'\t') {
            a += 1;
        }
        while b > a && (lb[b - 1] == b' ' || lb[b - 1] == b'\t') {
            b -= 1;
        }
        out.push((a, b));
    }
    out
}

/// Parse a directive start `name:: rest`. Returns `(name, argument)`.
fn parse_directive(rest: &[u8]) -> Option<(String, Option<String>)> {
    // The directive name ends at the first `::`; the name itself may contain a
    // single `:` (a domain prefix such as `py:function`).
    let mut pos = None;
    let mut i = 0usize;
    while i + 1 < rest.len() {
        if rest[i] == b':' && rest[i + 1] == b':' {
            pos = Some(i);
            break;
        }
        i += 1;
    }
    let k = pos?;
    if k == 0 {
        return None;
    }
    let name_bytes = &rest[..k];
    if !name_bytes
        .iter()
        .all(|&c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b':'))
    {
        return None;
    }
    let name = String::from_utf8_lossy(name_bytes).into_owned();
    let arg = String::from_utf8_lossy(&rest[k + 2..]).trim().to_string();
    Some((name, if arg.is_empty() { None } else { Some(arg) }))
}

/// Parse a bullet or enumerated list marker. Returns `None` if the line is not one.
fn parse_list_marker(lb: &[u8]) -> Option<ListMarker> {
    let indent = leading_spaces(lb);
    let rest = &lb[indent..];
    let first = *rest.first()?;
    // Bullet.
    if matches!(first, b'-' | b'*' | b'+') && matches!(rest.get(1), Some(&b' ') | Some(&b'\t')) {
        let mut off = indent + 1;
        while lb.get(off) == Some(&b' ') {
            off += 1;
        }
        return Some(ListMarker {
            ordered: false,
            marker: (first as char).to_string(),
            indent,
            content_off: off,
        });
    }
    // Enumerated: `1.`/`1)`/`#.`/`#)`/`a.`/`a)`, or `(1)`/`(a)`.
    let (marker_len, _) = enum_marker(rest)?;
    let off = indent + marker_len;
    if !matches!(lb.get(off), Some(&b' ') | Some(&b'\t')) {
        return None;
    }
    let mut content_off = off;
    while lb.get(content_off) == Some(&b' ') {
        content_off += 1;
    }
    Some(ListMarker {
        ordered: true,
        marker: String::from_utf8_lossy(&rest[..marker_len]).into_owned(),
        indent,
        content_off,
    })
}

/// The byte length of an enumerated marker at the start of `rest` (including a
/// trailing `.`/`)`), or `None` if there is none.
fn enum_marker(rest: &[u8]) -> Option<(usize, bool)> {
    // Parenthesized: `(1)` / `(a)`.
    if rest.first() == Some(&b'(') {
        let close = rest.iter().position(|&c| c == b')')?;
        if (2..=3).contains(&close) {
            let body = &rest[1..close];
            if enum_body(body) {
                return Some((close + 1, true));
            }
        }
        return None;
    }
    // `1.` / `#.` / `a.` / `i.` with `.` or `)`.
    let mut k = 0usize;
    while k < rest.len() && (rest[k].is_ascii_alphanumeric() || rest[k] == b'#') {
        k += 1;
    }
    if k == 0 || k > 3 {
        return None;
    }
    if !matches!(rest.get(k), Some(&b'.') | Some(&b')')) {
        return None;
    }
    if enum_body(&rest[..k]) {
        Some((k + 1, true))
    } else {
        None
    }
}

fn enum_body(body: &[u8]) -> bool {
    if body.is_empty() {
        return false;
    }
    if body.iter().all(|c| c.is_ascii_digit()) {
        return true;
    }
    if body == b"#" {
        return true;
    }
    // A single Latin letter (alpha) or a short roman numeral.
    let s = body;
    (s.len() == 1 && s[0].is_ascii_alphabetic())
        || s.iter()
            .all(|&c| matches!(c, b'i' | b'v' | b'x' | b'l' | b'c' | b'd' | b'm'))
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn corrupt(msg: &str) -> Error {
    Error::invalid_rst_structure(format!("malformed reStructuredText: {msg}"))
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

    fn model(src: &str) -> RstModel {
        parse(src.as_bytes(), Limits::DEFAULT, true).unwrap()
    }

    #[test]
    fn detects_structure_but_not_prose() {
        let l = Limits::DEFAULT;
        assert!(detect(b"Title\n=====\n\nbody\n", l));
        assert!(detect(b".. a comment\n\nbody\n", l));
        assert!(detect(
            b"+---+---+\n| a | b |\n+---+---+\n| 1 | 2 |\n+---+---+\n",
            l
        ));
        // Plain prose has no reST structural mark and stays Opaque.
        assert!(!detect(b"just some prose\nwith more lines\n", l));
        assert!(!detect(b"", l));
        // A Markdown-shaped document is not a reST signal by itself.
        assert!(!detect(b"# Heading\n\nbody\n", l));
        // The structural-signal predicate itself recognizes an explicit-markup
        // directive and a field list directly (the cross-format `looks_like_other`
        // guard is deliberately bypassed here, since a tiny `.. name:: body` or
        // `:name: value` source is claimed by YAML first in the dispatcher).
        assert!(
            parse(b".. note:: body\n", l, false)
                .unwrap()
                .has_structural_signal()
        );
        assert!(
            parse(b":author: me\n", l, false)
                .unwrap()
                .has_structural_signal()
        );
    }

    #[test]
    fn titles_lists_and_directives_are_exact() {
        let m = model(
            "Top Title\n=========\n\nSub Title\n---------\n\n- item one\n- item two\n\n.. note:: hi\n",
        );
        let titles = m.blocks_of_kind(B_TITLE);
        assert_eq!(titles.len(), 2);
        assert_eq!(m.blocks[titles[0] as usize].level, 1);
        assert_eq!(m.blocks[titles[1] as usize].level, 2);
        assert_eq!(
            m.blocks[titles[0] as usize].info.as_deref(),
            Some("=========")
        );
        let items = m.blocks_of_kind(B_LIST_ITEM);
        assert_eq!(items.len(), 2);
        let dirs = m.blocks_of_kind(B_DIRECTIVE);
        assert_eq!(dirs.len(), 1);
        assert_eq!(m.blocks[dirs[0] as usize].info.as_deref(), Some("note"));
    }

    #[test]
    fn inline_spans_and_model_roundtrip() {
        let src = "see **strong** and *em* and ``code`` and |sub| and [1]_\n";
        let m = model(src);
        let bytes = m.encode();
        let back = RstModel::decode(&bytes).unwrap();
        assert_eq!(m, back);
        assert_eq!(m.inlines_of_kind(I_STRONG).len(), 1);
        assert_eq!(m.inlines_of_kind(I_EMPHASIS).len(), 1);
        assert_eq!(m.inlines_of_kind(I_LITERAL).len(), 1);
        assert_eq!(m.inlines_of_kind(I_SUBSTITUTION_REF).len(), 1);
        assert_eq!(m.inlines_of_kind(I_FOOTNOTE_REF).len(), 1);
    }

    #[test]
    fn grid_and_simple_tables_are_exact() {
        let m = model("+---+---+\n| a | b |\n+---+---+\n| 1 | 2 |\n+---+---+\n");
        let t = m.blocks_of_kind(B_GRID_TABLE);
        assert_eq!(t.len(), 1);
        assert_eq!(m.blocks[t[0] as usize].cols, 2);
        assert_eq!(m.blocks[t[0] as usize].rows, 2);
        let m = model("=====  =====\na      b\n=====  =====\n1      2\n=====  =====\n");
        let t = m.blocks_of_kind(B_SIMPLE_TABLE);
        assert_eq!(t.len(), 1);
        assert_eq!(m.blocks[t[0] as usize].cols, 2);
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0x5201_9abc_def0_1234;
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
