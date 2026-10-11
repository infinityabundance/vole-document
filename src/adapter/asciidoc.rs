//! Bounded, representation-preserving AsciiDoc adapter (Phase 21.26.2).
//!
//! AsciiDoc is the third **prose** Wave-2 format (after Markdown and
//! reStructuredText), the human-readable input language of Asciidoctor. Like
//! Markdown/JSON/YAML/CSV it is *not* a package: there is no OPC/ZIP layer, no
//! `mimetype`, and no relationship graph. The exact leaf is the **whole source**
//! (a `DocumentExact`, a RAW-like authority), and everything this module produces
//! is a bounded, deterministic (`Q_gen`) projection that never sits on the
//! exactness path.
//!
//! ## Why a bespoke parser, and what "representation-preserving" means
//!
//! A conventional "AsciiDoc → HTML" pipeline drops the source spelling: it expands
//! attribute references, resolves and rewrites macros and includes, strips the
//! formatting markers, normalizes whitespace, and forgets every source offset.
//! This adapter instead records, for every block and every inline span, its exact
//! **byte span** in the source, and never rewrites the bytes:
//!
//! * **document title** (`= Title`) and **section levels** (`==`, `===`, …) with
//!   their exact heading span, their exact `=` marker, and a recorded level (the
//!   document title is level 0; `==` is level 1; `===` is level 2; …);
//! * **document attributes** (`:name: value`, `:name!:` unset) — preserved, and
//!   **attribute references** (`{name}`) surfaced **literally, never expanded**;
//! * **delimited blocks** with their exact delimiter and verbatim content: listing
//!   `----`, literal `....`, example `====`, sidebar `****`, quote `____`, open
//!   `--`, passthrough `++++`;
//! * **block attribute lines** (`[source,rust]`, `[quote, Author]`) preserved as
//!   their own block **and attached** to the following block (its `attrs`);
//! * **lists**: unordered `*`/`-`, ordered `.`, description lists (`term:: def`),
//!   with nesting depth and attached attributes;
//! * **tables** (`|===` with `|` cell markers), exposing their exact cell spans;
//! * **inline** markup: strong `*strong*`, emphasis `_emphasis_`, mono `` `mono` ``,
//!   passthrough `+passthrough+`, superscript `^super^`, subscript `~sub~`, mark
//!   `#mark#`/`##mark##`, and **macros preserved verbatim** (`link:url[text]`,
//!   `image::path[attrs]`, `include::file[]`, `xref:id[text]`, `http://url[text]`);
//! * **admonitions** (`NOTE:`/`TIP:`/`IMPORTANT:`/`WARNING:`/`CAUTION:`) and
//!   **block admonitions** (`[NOTE]`).
//!
//! The source is *never* re-flowed or normalized: a block's exact bytes are
//! literally `source[span]`, and the canonical text projection is the source
//! itself (lossily decoded), not a rendered or re-wrapped derivative.
//!
//! ## The supported subset (and what is DECLINED, typed)
//!
//! This is a **bounded AsciiDoc subset**, deliberately conservative. It supports
//! the constructs above at the top level (indent 0), plus nested list items. It does
//! **not** attempt nested sections inside delimited blocks, include/link resolution,
//! attribute expansion, the full cell/row spanning table grammar, `[#id]` block
//! anchors beyond the plain attribute list, CSV/DSV table bodies, or multi-line
//! section titles; those constructs are left as literal text inside their enclosing
//! block rather than being guessed at. Delimited-block content is preserved
//! **verbatim** (no inline extraction inside a delimited block), because that is
//! what "representation-preserving" requires for a passthrough/listing body.
//!
//! ## Detection (conservative; no magic bytes) and the reST/Markdown boundaries
//!
//! Plain prose has no AsciiDoc structural mark and stays
//! [`Opaque`](crate::field::document_format::DocumentFormat::Opaque). Detection
//! requires an **AsciiDoc-specific** signal: a level-0 document title (`= ` on the
//! first non-blank line) **followed by at least one further block**; or a `==`+
//! section; or a `|===` table; or a complete delimited block (`----`/`....`/`====`/
//! `****`/`____`/`--`/`++++`); or a block attribute line `[...]` followed by a
//! block.
//!
//! Two boundaries are *load-bearing and honest*, because the dispatcher tries
//! Markdown and reStructuredText **before** AsciiDoc and the config/INI family
//! before both:
//!
//! * An example (`====`), passthrough (`++++`), or literal (`....`) block whose
//!   delimiter character is `=`, `+`, or `.` and whose body is a **single**
//!   non-blank line is indistinguishable from a reST **overline section title**
//!   (`====\nTitle\n====`). reST is tried first, so such a source is admitted as
//!   reStructuredText (or stays `Opaque`), and is never reclassified as AsciiDoc.
//!   A `====`/`++++`/`....` block is claimed only when its body spans two or more
//!   lines, or when the delimiter character is `-`/`*`/`_` (which reST's
//!   *detection* set excludes).
//! * A `:name: value` attribute line is shaped like a YAML/config mapping entry, so
//!   a document that is *only* attribute lines is claimed earlier (YAML/config);
//!   the attribute-line construct is admitted here only inside a document that
//!   already carries a distinct AsciiDoc signal.
//! * A **bare** `|===` table whose lines are a majority pipe-delimited is shaped like
//!   a pipe-separated (PSV) table, so it is claimed earlier by the CSV/PSV adapter
//!   (which precedes AsciiDoc in the dispatcher order); the `|===` construct is
//!   admitted here only inside a document that also carries a non-tabular signal (a
//!   title, prose, a section, or a delimited block).
//!
//! A document that is valid Markdown is tried first, so a Markdown document is
//! never stolen; likewise a valid reST document is claimed by reST first.
//!
//! ## Bounds and no panics
//!
//! Untrusted input can only ever yield a typed decline, never a panic or an
//! unbounded allocation: the block count is capped by [`Limits::max_asciidoc_blocks`];
//! the inline-span count by [`Limits::max_asciidoc_inline_spans`]; the total node
//! count by [`Limits::max_asciidoc_nodes`]; the title/list nesting depth by
//! [`Limits::max_asciidoc_depth`]; a single physical line's bytes by
//! [`Limits::max_asciidoc_line_bytes`]; and the source length by
//! [`Limits::max_asciidoc_document_bytes`]. Inline extraction additionally runs under
//! a fixed per-document **probe budget** so a pathological delimiter pattern cannot
//! drive super-linear work; when the budget is exhausted the remaining text is left
//! as literal block content (the block structure and every span captured so far are
//! unaffected, and exactness is untouched).

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model blocks (defends the decoder against a hostile blob).
pub const MAX_MODEL_BLOCKS: u32 = 1 << 22;
/// Hard cap on decoded model inline spans (defends the decoder against a hostile blob).
pub const MAX_MODEL_INLINES: u64 = 1 << 24;

/// Block kind: the document title (`= Title`, level 0).
pub const B_DOC_TITLE: u8 = 0;
/// Block kind: a section heading (`== Title`, level = marker run length − 1).
pub const B_SECTION: u8 = 1;
/// Block kind: a paragraph (a run of non-blank lines).
pub const B_PARAGRAPH: u8 = 2;
/// Block kind: a document attribute (`:name: value`, `:name!:`).
pub const B_ATTRIBUTE: u8 = 3;
/// Block kind: a block attribute line (`[source,rust]`).
pub const B_BLOCK_ATTR: u8 = 4;
/// Block kind: a listing block (`----`, content verbatim).
pub const B_LISTING: u8 = 5;
/// Block kind: a literal block (`....`, content verbatim).
pub const B_LITERAL: u8 = 6;
/// Block kind: an example block (`====`).
pub const B_EXAMPLE: u8 = 7;
/// Block kind: a sidebar block (`****`).
pub const B_SIDEBAR: u8 = 8;
/// Block kind: a quote block (`____`).
pub const B_QUOTE: u8 = 9;
/// Block kind: an open block (`--`).
pub const B_OPEN: u8 = 10;
/// Block kind: a passthrough block (`++++`, content verbatim).
pub const B_PASSTHROUGH: u8 = 11;
/// Block kind: one unordered/ordered/description list item.
pub const B_LIST_ITEM: u8 = 12;
/// Block kind: a table (`|===` with `|` cell markers).
pub const B_TABLE: u8 = 13;
/// Block kind: an admonition (`NOTE:` paragraph or `[NOTE]` block).
pub const B_ADMONITION: u8 = 14;
/// The highest valid block kind.
pub const B_LAST: u8 = B_ADMONITION;

/// Inline kind: strong (`*strong*` / `**strong**`).
pub const I_STRONG: u8 = 0;
/// Inline kind: emphasis (`_emphasis_` / `__emphasis__`).
pub const I_EMPHASIS: u8 = 1;
/// Inline kind: mono (`` `mono` ``).
pub const I_MONO: u8 = 2;
/// Inline kind: passthrough (`+passthrough+`).
pub const I_PASSTHROUGH: u8 = 3;
/// Inline kind: superscript (`^super^`).
pub const I_SUPERSCRIPT: u8 = 4;
/// Inline kind: subscript (`~sub~`).
pub const I_SUBSCRIPT: u8 = 5;
/// Inline kind: mark (`#mark#`).
pub const I_MARK: u8 = 6;
/// Inline kind: unconstrained mark (`##mark##`).
pub const I_MARK_DOUBLE: u8 = 7;
/// Inline kind: a link macro (`link:url[text]`).
pub const I_LINK: u8 = 8;
/// Inline kind: an image macro (`image:path[attrs]` / `image::path[attrs]`).
pub const I_IMAGE: u8 = 9;
/// Inline kind: an include macro (`include::file[]`).
pub const I_INCLUDE: u8 = 10;
/// Inline kind: a cross-reference macro (`xref:id[text]`).
pub const I_XREF: u8 = 11;
/// Inline kind: a bare URL macro (`http://url[text]`).
pub const I_URL: u8 = 12;
/// Inline kind: an attribute reference (`{name}`), surfaced literally.
pub const I_ATTR_REF: u8 = 13;
/// Inline kind: a table cell (its exact content span).
pub const I_TABLE_CELL: u8 = 14;
/// The highest valid inline kind.
pub const I_LAST: u8 = I_TABLE_CELL;

/// Block flag bit: an ordered (`.`) list item.
pub const F_ORDERED: u8 = 1;
/// Block flag bit: a description (`term:: def`) list item.
pub const F_DESCRIPTION: u8 = 2;
/// Block flag bit: an unset document attribute (`:name!:`).
pub const F_UNSET: u8 = 4;
/// Block flag bit: a block carrying attached block attributes from a `[...]` line.
pub const F_ATTRS: u8 = 8;
/// Block flag bit: a **block** admonition (`[NOTE]`) rather than a paragraph one.
pub const F_BLOCK: u8 = 16;
/// The valid block flag bits.
const FLAGS_MASK: u8 = F_ORDERED | F_DESCRIPTION | F_UNSET | F_ATTRS | F_BLOCK;

/// Stable lower-case block-kind name for reports and JSON output.
pub const fn block_kind_name(kind: u8) -> &'static str {
    match kind {
        B_DOC_TITLE => "doc-title",
        B_SECTION => "section",
        B_PARAGRAPH => "paragraph",
        B_ATTRIBUTE => "attribute",
        B_BLOCK_ATTR => "block-attr",
        B_LISTING => "listing",
        B_LITERAL => "literal",
        B_EXAMPLE => "example",
        B_SIDEBAR => "sidebar",
        B_QUOTE => "quote",
        B_OPEN => "open",
        B_PASSTHROUGH => "passthrough",
        B_LIST_ITEM => "list-item",
        B_TABLE => "table",
        B_ADMONITION => "admonition",
        _ => "unknown",
    }
}

/// Stable lower-case inline-kind name for reports and JSON output.
pub const fn inline_kind_name(kind: u8) -> &'static str {
    match kind {
        I_STRONG => "strong",
        I_EMPHASIS => "emphasis",
        I_MONO => "mono",
        I_PASSTHROUGH => "passthrough",
        I_SUPERSCRIPT => "superscript",
        I_SUBSCRIPT => "subscript",
        I_MARK => "mark",
        I_MARK_DOUBLE => "mark-double",
        I_LINK => "link",
        I_IMAGE => "image",
        I_INCLUDE => "include",
        I_XREF => "xref",
        I_URL => "url",
        I_ATTR_REF => "attribute-ref",
        I_TABLE_CELL => "table-cell",
        _ => "unknown",
    }
}

/// Whether a block kind is a delimited block (content preserved verbatim).
pub const fn is_delimited_block(kind: u8) -> bool {
    matches!(
        kind,
        B_LISTING | B_LITERAL | B_EXAMPLE | B_SIDEBAR | B_QUOTE | B_OPEN | B_PASSTHROUGH
    )
}

/// One parsed block: its kind, its exact source span, its content span (the bytes a
/// reader sees), the block attributes attached from a preceding `[...]` line, and
/// any kind-specific fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdocBlock {
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
    /// Section/list nesting level; else 0.
    pub level: u8,
    /// `F_*` flag bits.
    pub flags: u8,
    /// A table's row count; else 0.
    pub rows: u32,
    /// A table's column count; else 0.
    pub cols: u32,
    /// The block's exact `=` marker (section/doc title), delimiter (delimited block),
    /// attribute name, list marker, or admonition label; else `None`.
    pub info: Option<String>,
    /// A description-list term; else `None`.
    pub target: Option<String>,
    /// A document attribute's value; a list item's text; else `None`.
    pub title: Option<String>,
    /// The exact text of an attached block attribute line (`[source,rust]` → the
    /// `source,rust`), when this block directly follows one; else `None`.
    pub attrs: Option<String>,
    /// Indices into the inline arena belonging to this block, in order.
    pub inlines: Vec<u32>,
}

/// One parsed inline span: its kind, its exact span and inner (text) span, and any
/// target field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdocInline {
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
    /// A macro target (the URL/path/id) or an attribute-reference name; else `None`.
    pub target: Option<String>,
    /// Reserved (currently always `None`).
    pub title: Option<String>,
    /// The index of the owning block.
    pub block: u32,
}

/// The canonical derived AsciiDoc model (the materialization of an `AsciidocModel`
/// node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdocModel {
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// The block arena, in document order.
    pub blocks: Vec<AdocBlock>,
    /// The inline-span arena, in document order.
    pub inlines: Vec<AdocInline>,
}

impl AdocModel {
    /// The block at `index`, if present.
    pub fn block(&self, index: u32) -> Option<&AdocBlock> {
        self.blocks.get(index as usize)
    }

    /// The inline at `index`, if present.
    pub fn inline(&self, index: u32) -> Option<&AdocInline> {
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

    /// The indices of **heading** blocks (the document title, then every section), in
    /// document order.
    pub fn headings(&self) -> Vec<u32> {
        self.blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| matches!(b.kind, B_DOC_TITLE | B_SECTION))
            .map(|(i, _)| i as u32)
            .collect()
    }

    /// The document's maximum section level observed (0 if there are none).
    pub fn max_section_level(&self) -> u8 {
        self.blocks
            .iter()
            .filter(|b| b.kind == B_SECTION)
            .map(|b| b.level)
            .max()
            .unwrap_or(0)
    }

    /// Whether the source carries an **AsciiDoc-specific** structural mark that
    /// admits it as AsciiDoc. See the module docs for the Markdown/reST boundaries.
    pub fn has_structural_signal(&self) -> bool {
        let mut has_doc_title = false;
        let mut has_section = false;
        let mut has_table = false;
        let mut has_delimited = false;
        let mut block_attr_then_block = false;
        let mut prev_is_block_attr = false;
        let mut other_blocks: u64 = 0;
        for b in &self.blocks {
            match b.kind {
                B_DOC_TITLE => {
                    has_doc_title = true;
                    prev_is_block_attr = false;
                    continue;
                }
                B_BLOCK_ATTR => {
                    prev_is_block_attr = true;
                    continue;
                }
                B_SECTION => has_section = true,
                B_TABLE => has_table = true,
                k if is_delimited_block(k) => has_delimited = true,
                _ => {}
            }
            if prev_is_block_attr {
                block_attr_then_block = true;
            }
            prev_is_block_attr = false;
            other_blocks += 1;
        }
        if has_section || has_table || has_delimited || block_attr_then_block {
            return true;
        }
        // A document title alone is a weak signal: require it to be followed by at
        // least one further block (so a lone `= text` line stays Opaque).
        has_doc_title && other_blocks >= 1
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64 + self.blocks.len() * 56);
        out.extend_from_slice(b"ADC1");
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
            put_opt(&mut out, b.attrs.as_deref());
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
    pub fn decode(bytes: &[u8]) -> Result<AdocModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"ADC1" {
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
            if flags & !FLAGS_MASK != 0 {
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
            let attrs = r.opt_str()?;
            let n = r.u32()?;
            total_inlines = total_inlines.saturating_add(n as u64);
            if total_inlines > MAX_MODEL_INLINES {
                return Err(corrupt("model inline count is implausible"));
            }
            let mut inlines = Vec::with_capacity(n as usize);
            for _ in 0..n {
                inlines.push(r.u32()?);
            }
            blocks.push(AdocBlock {
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
                attrs,
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
            inlines.push(AdocInline {
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
        Ok(AdocModel {
            doc_len,
            blocks,
            inlines,
        })
    }
}

/// Byte-based AsciiDoc detector. See the module docs for the heuristic and the
/// Markdown/reST boundaries it cannot cross.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.is_empty() {
        return false;
    }
    if source.len() as u64 > limits.max_asciidoc_document_bytes {
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

/// Whether `source` belongs to a family that is *not* AsciiDoc (defence in depth;
/// the dispatcher also orders PDF/ZIP/JSON/YAML/CSV/Markdown/reST ahead of AsciiDoc).
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
    #[cfg(feature = "markdown")]
    if crate::adapter::markdown::detect(source, limits) {
        return true;
    }
    // A document that reStructuredText already admits must never be stolen (defence
    // in depth; the dispatcher tries reST first, so this is belt and suspenders).
    #[cfg(feature = "rst")]
    if crate::adapter::rst::detect(source, limits) {
        return true;
    }
    false
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of an `AsciidocModel` node).
pub fn build_asciidoc_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits, true)?.encode())
}

/// Parse `source` into an [`AdocModel`]. `build` selects whether the inline arena is
/// populated (detection runs with `build = false`, so a detection-style call never
/// scans inline spans).
pub fn parse(source: &[u8], limits: Limits, build: bool) -> Result<AdocModel> {
    if source.len() as u64 > limits.max_asciidoc_document_bytes {
        return Err(Error::resource_limit(format!(
            "AsciiDoc source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_asciidoc_document_bytes
        )));
    }
    let mut p = Parser::new(source, limits, build);
    p.run()?;
    Ok(AdocModel {
        doc_len: source.len() as u64,
        blocks: p.blocks,
        inlines: p.inlines,
    })
}

/// The exact source bytes of a block's whole span (`[start, end)`).
pub fn block_bytes<'a>(source: &'a [u8], block: &AdocBlock) -> Result<&'a [u8]> {
    slice(
        source,
        block.start,
        block.end,
        "block span is outside the source",
    )
}

/// The exact source bytes of a block's content span (`[content_start, content_end)`).
pub fn content_bytes<'a>(source: &'a [u8], block: &AdocBlock) -> Result<&'a [u8]> {
    slice(
        source,
        block.content_start,
        block.content_end,
        "block content span is outside the source",
    )
}

/// The exact source bytes of an inline's whole span (`[start, end)`).
pub fn inline_bytes<'a>(source: &'a [u8], inline: &AdocInline) -> Result<&'a [u8]> {
    slice(
        source,
        inline.start,
        inline.end,
        "inline span is outside the source",
    )
}

/// The exact source bytes of an inline's inner text span.
pub fn inline_text_bytes<'a>(source: &'a [u8], inline: &AdocInline) -> Result<&'a [u8]> {
    slice(
        source,
        inline.inner_start,
        inline.inner_end,
        "inline text span is outside the source",
    )
}

/// A deterministic canonical text projection: the exact source bytes, lossily
/// decoded. AsciiDoc is **not** rendered, so the canonical text is the source
/// itself. Declines typed if it would exceed `max_out`.
pub fn canonical_text(source: &[u8], _limits: Limits, max_out: u64) -> Result<String> {
    if source.len() as u64 > max_out {
        return Err(Error::resource_limit(format!(
            "AsciiDoc text projection exceeds the {max_out}-byte budget"
        )));
    }
    Ok(String::from_utf8_lossy(source).into_owned())
}

/// One lexical-match record from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdocMatch {
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

/// A bounded, case-sensitive lexical search over each block's content span. Returns
/// matches in document order. Declines typed if the match list would exceed `max_out`
/// bytes (an approximate bound).
pub fn find(
    source: &[u8],
    model: &AdocModel,
    pattern: &str,
    max_out: u64,
) -> Result<Vec<AdocMatch>> {
    if pattern.is_empty() {
        return Err(Error::usage("AsciiDoc find pattern must not be empty"));
    }
    let needle = pattern.as_bytes();
    let mut out: Vec<AdocMatch> = Vec::new();
    let mut estimated: u64 = 0;
    for (i, b) in model.blocks.iter().enumerate() {
        let content = content_bytes(source, b)?;
        if contains(content, needle) {
            estimated = estimated.saturating_add(32 + content.len() as u64);
            if estimated > max_out {
                return Err(Error::resource_limit(format!(
                    "AsciiDoc find exceeded the {max_out}-byte budget"
                )));
            }
            out.push(AdocMatch {
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
    blocks: Vec<AdocBlock>,
    inlines: Vec<AdocInline>,
    node_count: u64,
    inline_count: u64,
    probe: u64,
    /// Whether a block has been emitted yet (guards the document title).
    emitted_any: bool,
    /// The block attribute text pending attachment to the next block.
    pending_attrs: Option<String>,
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
            emitted_any: false,
            pending_attrs: None,
        }
    }

    fn lb(&self, i: usize) -> &'a [u8] {
        let l = &self.lines[i];
        &self.b[l.start..l.content_end]
    }

    fn run(&mut self) -> Result<()> {
        for i in 0..self.lines.len() {
            let l = &self.lines[i];
            if (l.content_end - l.start) as u64 > self.limits.max_asciidoc_line_bytes {
                return Err(Error::resource_limit(format!(
                    "AsciiDoc line {} exceeds the {}-byte line cap",
                    i, self.limits.max_asciidoc_line_bytes
                )));
            }
        }
        let mut i = 0usize;
        while i < self.lines.len() {
            let lb = self.lb(i);
            if is_blank(lb) {
                self.pending_attrs = None;
                i += 1;
                continue;
            }
            // The document title must be the very first block.
            if !self.emitted_any && is_doc_title_line(lb) {
                self.emit_doc_title(i)?;
                i += 1;
                continue;
            }
            if let Some((level, marker)) = section_line(lb) {
                self.emit_section(i, level, &marker)?;
                i += 1;
                continue;
            }
            if let Some(a) = attribute_entry(lb) {
                self.emit_attribute(i, &a)?;
                i += 1;
                continue;
            }
            if let Some(inner) = block_attr_line(lb) {
                if let Some(label) = admonition_block(inner) {
                    self.emit_block_admonition(i, label, inner)?;
                } else {
                    self.emit_block_attr(i, inner)?;
                }
                i += 1;
                continue;
            }
            if lb == b"|==="
                && let Some(t) = self.table(i)
            {
                self.emit_table(i, &t)?;
                i = t.after;
                continue;
            }
            if let Some(kind) = delimiter_kind(lb)
                && let Some(t) = self.delimited_block(i, kind)
            {
                self.emit_delimited(i, kind, &t)?;
                i = t.after;
                continue;
            }
            if let Some(label) = admonition_paragraph(lb) {
                i = self.emit_admonition(i, label)?;
                continue;
            }
            if let Some(m) = parse_list_marker(lb) {
                i = self.emit_list_item(i, &m)?;
                continue;
            }
            if let Some((term, def_off)) = description_line(lb) {
                i = self.emit_description(i, &term, def_off)?;
                continue;
            }
            i = self.emit_paragraph(i)?;
        }
        Ok(())
    }

    // -- headings -----------------------------------------------------------

    fn emit_doc_title(&mut self, i: usize) -> Result<()> {
        let l = &self.lines[i];
        let lb = self.lb(i);
        let cs = l.start + doc_title_content_off(lb);
        let (end, ce) = (l.content_end as u64, l.content_end as u64);
        let idx = self.push_block(
            B_DOC_TITLE,
            l.start as u64,
            end,
            cs as u64,
            ce,
            0,
            0,
            None,
            None,
            None,
        )?;
        self.scan_block_inlines(idx)?;
        Ok(())
    }

    fn emit_section(&mut self, i: usize, level: u8, marker: &str) -> Result<()> {
        let (start, content_end) = {
            let l = &self.lines[i];
            (l.start, l.content_end)
        };
        let cs = start + section_content_off(self.lb(i));
        let level = (level as u32).min(self.limits.max_asciidoc_depth);
        self.note_depth(level)?;
        let idx = self.push_block(
            B_SECTION,
            start as u64,
            content_end as u64,
            cs as u64,
            content_end as u64,
            level.min(255) as u8,
            0,
            Some(marker.to_string()),
            None,
            None,
        )?;
        self.scan_block_inlines(idx)?;
        Ok(())
    }

    // -- document attributes ------------------------------------------------

    fn emit_attribute(&mut self, i: usize, a: &Attribute) -> Result<()> {
        let l = &self.lines[i];
        let cs = l.start + a.value_off;
        let mut flags = 0u8;
        if a.unset {
            flags |= F_UNSET;
        }
        let value = if a.unset {
            None
        } else if a.value_off < l.content_end - l.start {
            Some(String::from_utf8_lossy(&self.lb(i)[a.value_off..]).into_owned())
        } else {
            None
        };
        let idx = self.push_block(
            B_ATTRIBUTE,
            l.start as u64,
            l.content_end as u64,
            cs.min(l.content_end) as u64,
            l.content_end as u64,
            0,
            flags,
            Some(a.name.clone()),
            None,
            value,
        )?;
        self.scan_block_inlines(idx)?;
        Ok(())
    }

    // -- block attribute lines ----------------------------------------------

    fn emit_block_attr(&mut self, i: usize, inner: &[u8]) -> Result<()> {
        let l = &self.lines[i];
        let lb = self.lb(i);
        // The inner span is between the leading `[` and the trailing `]`.
        let (is, ie) = (l.start + 1, l.start + lb.len() - 1);
        let text = String::from_utf8_lossy(inner).into_owned();
        self.push_block(
            B_BLOCK_ATTR,
            l.start as u64,
            l.content_end as u64,
            is as u64,
            ie as u64,
            0,
            0,
            Some(text.clone()),
            None,
            None,
        )?;
        match &mut self.pending_attrs {
            Some(s) => {
                s.push('\n');
                s.push_str(&text);
            }
            None => self.pending_attrs = Some(text),
        }
        Ok(())
    }

    // -- admonitions --------------------------------------------------------

    fn emit_block_admonition(&mut self, i: usize, label: &str, inner: &[u8]) -> Result<()> {
        let l = &self.lines[i];
        let lb = self.lb(i);
        let (is, ie) = (l.start + 1, l.start + lb.len() - 1);
        let _ = inner;
        self.push_block(
            B_ADMONITION,
            l.start as u64,
            l.content_end as u64,
            is as u64,
            ie as u64,
            0,
            F_BLOCK,
            Some(label.to_string()),
            None,
            None,
        )?;
        Ok(())
    }

    fn emit_admonition(&mut self, i: usize, label: &str) -> Result<usize> {
        let start = self.lines[i].start;
        let mut j = i + 1;
        let mut last_end = self.lines[i].content_end;
        while j < self.lines.len() && !is_blank(self.lb(j)) {
            last_end = self.lines[j].content_end;
            j += 1;
        }
        let idx = self.push_block(
            B_ADMONITION,
            start as u64,
            last_end as u64,
            start as u64,
            last_end as u64,
            0,
            0,
            Some(label.to_string()),
            None,
            None,
        )?;
        self.scan_block_inlines(idx)?;
        Ok(j)
    }

    // -- lists --------------------------------------------------------------

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
        let raw_level = m.level as u32;
        self.note_depth(raw_level)?;
        let level = raw_level.min(self.limits.max_asciidoc_depth);
        let mut flags = 0u8;
        if m.ordered {
            flags |= F_ORDERED;
        }
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

    fn emit_description(&mut self, i: usize, term: &str, def_off: usize) -> Result<usize> {
        let start = self.lines[i].start;
        let mut j = i + 1;
        let mut last_end = self.lines[i].content_end;
        while j < self.lines.len() {
            let lb = self.lb(j);
            if is_blank(lb) {
                j += 1;
                continue;
            }
            if leading_spaces(lb) > 0 {
                last_end = self.lines[j].content_end;
                j += 1;
            } else {
                break;
            }
        }
        let cs = (start + def_off) as u64;
        let ce = last_end as u64;
        self.note_depth(1)?;
        let idx = self.push_block(
            B_LIST_ITEM,
            start as u64,
            ce,
            cs.min(ce),
            ce,
            1,
            F_DESCRIPTION,
            Some("::".to_string()),
            Some(term.to_string()),
            None,
        )?;
        self.scan_block_inlines(idx)?;
        Ok(j)
    }

    // -- tables -------------------------------------------------------------

    fn table(&self, i: usize) -> Option<TableInfo> {
        if self.lb(i) != b"|===" {
            return None;
        }
        let mut j = i + 1;
        while j < self.lines.len() {
            if self.lb(j) == b"|===" {
                let mut rows = 0u32;
                let mut cols = 0u32;
                for k in i + 1..j {
                    let lb = self.lb(k);
                    if lb.contains(&b'|') {
                        rows += 1;
                        let c = cell_spans(lb).len() as u32;
                        cols = cols.max(c);
                    }
                }
                return Some(TableInfo {
                    after: j + 1,
                    rows,
                    cols,
                });
            }
            j += 1;
        }
        None
    }

    fn emit_table(&mut self, i: usize, t: &TableInfo) -> Result<()> {
        let start = self.lines[i].start as u64;
        let end = self.lines[t.after - 1].content_end as u64;
        let idx = self.push_block(B_TABLE, start, end, start, end, 0, 0, None, None, None)?;
        self.blocks[idx].rows = t.rows;
        self.blocks[idx].cols = t.cols;
        if self.build {
            for k in i + 1..t.after - 1 {
                let lb = self.lb(k);
                if !lb.contains(&b'|') {
                    continue;
                }
                let base = self.lines[k].start;
                for (cs, ce) in cell_spans(lb) {
                    self.push_inline(
                        idx,
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
        }
        Ok(())
    }

    // -- delimited blocks ---------------------------------------------------

    fn delimited_block(&self, i: usize, kind: u8) -> Option<DelimInfo> {
        let opener = self.lb(i);
        let mut j = i + 1;
        while j < self.lines.len() {
            if self.lb(j) == opener {
                let content_start = if i + 1 < j {
                    self.lines[i + 1].start as u64
                } else {
                    self.lines[i].content_end as u64
                };
                let content_end = if i + 1 < j {
                    self.lines[j - 1].content_end as u64
                } else {
                    self.lines[i].content_end as u64
                };
                let _ = kind;
                return Some(DelimInfo {
                    after: j + 1,
                    content_start,
                    content_end,
                    end: self.lines[j].content_end as u64,
                    delim: String::from_utf8_lossy(opener).into_owned(),
                });
            }
            j += 1;
        }
        None
    }

    fn emit_delimited(&mut self, i: usize, kind: u8, t: &DelimInfo) -> Result<()> {
        let start = self.lines[i].start as u64;
        let idx = self.push_block(
            kind,
            start,
            t.end,
            t.content_start,
            t.content_end,
            0,
            0,
            Some(t.delim.clone()),
            None,
            None,
        )?;
        let _ = idx;
        Ok(())
    }

    // -- paragraphs ---------------------------------------------------------

    fn emit_paragraph(&mut self, i: usize) -> Result<usize> {
        let indent0 = leading_spaces(self.lb(i));
        let start = self.lines[i].start;
        let mut j = i + 1;
        let mut last_end = self.lines[i].content_end;
        while j < self.lines.len() && !is_blank(self.lb(j)) {
            if self.interrupts_paragraph(j, indent0) {
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
        Ok(j)
    }

    /// Whether line `j` (non-blank) begins a new block at an indent no deeper than
    /// `indent0`, interrupting an open paragraph.
    fn interrupts_paragraph(&self, j: usize, indent0: usize) -> bool {
        let lb = self.lb(j);
        if leading_spaces(lb) > indent0 {
            return true;
        }
        if leading_spaces(lb) != 0 {
            return false;
        }
        section_line(lb).is_some()
            || attribute_entry(lb).is_some()
            || block_attr_line(lb).is_some()
            || lb == b"|==="
            || delimiter_kind(lb).is_some()
            || admonition_paragraph(lb).is_some()
            || parse_list_marker(lb).is_some()
            || description_line(lb).is_some()
    }

    // -- block / inline arena -----------------------------------------------

    fn note_depth(&mut self, depth: u32) -> Result<()> {
        if depth > self.limits.max_asciidoc_depth {
            return Err(Error::resource_limit(format!(
                "AsciiDoc nesting depth {depth} exceeds the {}-deep cap",
                self.limits.max_asciidoc_depth
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
        if self.blocks.len() as u64 >= self.limits.max_asciidoc_blocks as u64 {
            return Err(Error::resource_limit(format!(
                "AsciiDoc document exceeds the {}-block cap",
                self.limits.max_asciidoc_blocks
            )));
        }
        self.node_count = self.node_count.saturating_add(1);
        if self.node_count > self.limits.max_asciidoc_nodes as u64 {
            return Err(Error::resource_limit(format!(
                "AsciiDoc document exceeds the {}-node cap",
                self.limits.max_asciidoc_nodes
            )));
        }
        // A block attribute line never consumes a preceding one (consecutive
        // attribute lines all attach to the block that follows them).
        let attrs = if kind == B_BLOCK_ATTR {
            None
        } else {
            self.pending_attrs.take()
        };
        let flags = if attrs.is_some() {
            flags | F_ATTRS
        } else {
            flags
        };
        self.emitted_any = true;
        let idx = self.blocks.len();
        self.blocks.push(AdocBlock {
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
            attrs,
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
        if self.inline_count > self.limits.max_asciidoc_inline_spans as u64 {
            return Err(Error::resource_limit(format!(
                "AsciiDoc document exceeds the {}-inline-span cap",
                self.limits.max_asciidoc_inline_spans
            )));
        }
        self.node_count = self.node_count.saturating_add(1);
        if self.node_count > self.limits.max_asciidoc_nodes as u64 {
            return Err(Error::resource_limit(format!(
                "AsciiDoc document exceeds the {}-node cap",
                self.limits.max_asciidoc_nodes
            )));
        }
        let idx = self.inlines.len() as u32;
        self.blocks[block].inlines.push(idx);
        self.inlines.push(AdocInline {
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
        let kind = self.blocks[block].kind;
        // Delimited-block content and attribute lines are preserved verbatim.
        if is_delimited_block(kind) || kind == B_BLOCK_ATTR {
            return Ok(());
        }
        let (cs, ce) = {
            let b = &self.blocks[block];
            (b.content_start as usize, b.content_end as usize)
        };
        self.scan_region(block, cs, ce)
    }

    fn scan_region(&mut self, block: usize, start: usize, end: usize) -> Result<()> {
        let mut i = start;
        while i < end {
            if self.probe == 0 {
                break;
            }
            let c = self.b[i];
            if c == b'\\' {
                i = (i + 2).min(end);
                continue;
            }
            if let Some(next) = self.try_inline(block, i, end, start)? {
                i = next;
                continue;
            }
            i += 1;
        }
        Ok(())
    }

    /// Try every inline construct at `i`. Returns `Some(whole_end)` (and pushes the
    /// span) on a match.
    fn try_inline(
        &mut self,
        block: usize,
        i: usize,
        end: usize,
        region_start: usize,
    ) -> Result<Option<usize>> {
        // Macro family (checked first: `link:`/`image:`/`include::`/`xref:`/`http(s):`).
        if let Some((kind, is, ie, we, target)) = self.try_macro(i, end, region_start) {
            self.push_inline(block, kind, i, we, is, ie, Some(target), None)?;
            return Ok(Some(we));
        }
        let c = self.b[i];
        let fmt = match c {
            b'*' => self.fmt(i, end, b'*', I_STRONG, I_STRONG, region_start),
            b'_' => self.fmt(i, end, b'_', I_EMPHASIS, I_EMPHASIS, region_start),
            b'`' => self.fmt(i, end, b'`', I_MONO, I_MONO, region_start),
            b'+' => self.fmt(i, end, b'+', I_PASSTHROUGH, I_PASSTHROUGH, region_start),
            b'^' => self.fmt(i, end, b'^', I_SUPERSCRIPT, I_SUPERSCRIPT, region_start),
            b'~' => self.fmt(i, end, b'~', I_SUBSCRIPT, I_SUBSCRIPT, region_start),
            b'#' => self.fmt(i, end, b'#', I_MARK, I_MARK_DOUBLE, region_start),
            b'{' => self.attr_ref(i, end),
            _ => None,
        };
        if let Some((kind, is, ie, we)) = fmt {
            self.push_inline(block, kind, i, we, is, ie, None, None)?;
            return Ok(Some(we));
        }
        Ok(None)
    }

    /// Try a delimited inline format at `i`. Returns
    /// `(kind, inner_start, inner_end, whole_end)`.
    fn fmt(
        &mut self,
        i: usize,
        end: usize,
        ch: u8,
        single_kind: u8,
        double_kind: u8,
        region_start: usize,
    ) -> Option<(u8, usize, usize, usize)> {
        let dbl = i + 1 < end && self.b[i + 1] == ch;
        let open_len = if dbl { 2 } else { 1 };
        let inner0 = i + open_len;
        if inner0 >= end || self.b[inner0].is_ascii_whitespace() {
            return None;
        }
        if open_len == 1 && i > region_start && is_word_byte(self.b[i - 1]) {
            return None;
        }
        let mut j = inner0;
        while j < end {
            if self.probe == 0 {
                return None;
            }
            self.probe -= 1;
            if self.b[j] == ch && self.b[j - 1] != b' ' && self.b[j - 1] != b'\t' {
                if open_len == 2 && !(j + 1 < end && self.b[j + 1] == ch) {
                    j += 1;
                    continue;
                }
                let we = j + open_len;
                if open_len == 1 && we < end && is_word_byte(self.b[we]) {
                    j += 1;
                    continue;
                }
                let kind = if open_len == 2 {
                    double_kind
                } else {
                    single_kind
                };
                return Some((kind, inner0, j, we));
            }
            j += 1;
        }
        None
    }

    /// Try an attribute reference `{name}` at `i`. Returns
    /// `(I_ATTR_REF, inner_start, inner_end, whole_end)`.
    fn attr_ref(&mut self, i: usize, end: usize) -> Option<(u8, usize, usize, usize)> {
        let inner0 = i + 1;
        if inner0 >= end {
            return None;
        }
        let mut j = inner0;
        while j < end {
            if self.probe == 0 {
                return None;
            }
            self.probe -= 1;
            let c = self.b[j];
            if c == b'}' {
                if j > inner0 {
                    return Some((I_ATTR_REF, inner0, j, j + 1));
                }
                return None;
            }
            if !(c.is_ascii_alphanumeric() || c == b'_' || c == b'-') {
                return None;
            }
            j += 1;
        }
        None
    }

    /// Try a macro at `i`. Returns `(kind, inner_start, inner_end, whole_end, target)`.
    #[allow(clippy::type_complexity)]
    fn try_macro(
        &mut self,
        i: usize,
        end: usize,
        region_start: usize,
    ) -> Option<(u8, usize, usize, usize, String)> {
        if i > region_start && is_word_byte(self.b[i - 1]) {
            return None;
        }
        let rest = &self.b[i..end];
        let (kind, prefix_len, target_start) = if rest.starts_with(b"include::") {
            (I_INCLUDE, 9usize, i + 9)
        } else if rest.starts_with(b"image::") {
            (I_IMAGE, 7, i + 7)
        } else if rest.starts_with(b"image:") {
            (I_IMAGE, 6, i + 6)
        } else if rest.starts_with(b"link:") {
            (I_LINK, 5, i + 5)
        } else if rest.starts_with(b"xref:") {
            (I_XREF, 5, i + 5)
        } else if rest.starts_with(b"https://") {
            (I_URL, 8, i)
        } else if rest.starts_with(b"http://") {
            (I_URL, 7, i)
        } else {
            return None;
        };
        let _ = prefix_len;
        // Find `[` (bounded, same line) then `]`.
        let mut k = i + prefix_len;
        while k < end && self.b[k] != b'[' {
            if self.b[k] == b'\n' {
                return None;
            }
            self.probe = self.probe.saturating_sub(1);
            k += 1;
        }
        if k >= end || self.b[k] != b'[' {
            return None;
        }
        let open = k;
        let mut m = open + 1;
        while m < end && self.b[m] != b']' {
            if self.b[m] == b'\n' {
                return None;
            }
            self.probe = self.probe.saturating_sub(1);
            m += 1;
        }
        if m >= end || self.b[m] != b']' {
            return None;
        }
        if open <= target_start {
            return None;
        }
        let target = String::from_utf8_lossy(&self.b[target_start..open]).into_owned();
        if target.is_empty() || target.bytes().next() == Some(b' ') {
            return None;
        }
        Some((kind, open + 1, m, m + 1, target))
    }
}

/// A detected table.
struct TableInfo {
    /// Index of the first line after the table.
    after: usize,
    /// The number of `|`-bearing content rows.
    rows: u32,
    /// The maximum number of cells on any content row.
    cols: u32,
}

/// A detected delimited block.
struct DelimInfo {
    /// Index of the first line after the block.
    after: usize,
    /// The first byte of the content (between the delimiters).
    content_start: u64,
    /// One past the last content byte.
    content_end: u64,
    /// One past the closing delimiter line.
    end: u64,
    /// The exact delimiter string.
    delim: String,
}

/// A detected document attribute.
struct Attribute {
    name: String,
    /// Offset (within the line) at which the value starts.
    value_off: usize,
    unset: bool,
}

/// A detected list marker.
struct ListMarker {
    ordered: bool,
    /// The marker spelling (`*`, `-`, `.`, `..`, …).
    marker: String,
    /// The leading indent.
    indent: usize,
    /// The offset (within the line) at which the item content starts.
    content_off: usize,
    /// The nesting level (1-based).
    level: u8,
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

fn is_word_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric()
}

/// Whether `lb` is a document title (`= ` at column 0).
fn is_doc_title_line(lb: &[u8]) -> bool {
    lb.len() >= 3 && lb[0] == b'=' && lb[1] == b' ' && lb[2] != b' '
}

fn doc_title_content_off(lb: &[u8]) -> usize {
    let mut k = 1;
    while lb.get(k) == Some(&b' ') {
        k += 1;
    }
    k
}

/// If `lb` is a section heading (`==`+ then a space), return `(level, marker)`.
fn section_line(lb: &[u8]) -> Option<(u8, String)> {
    if leading_spaces(lb) != 0 || lb.first() != Some(&b'=') {
        return None;
    }
    let mut n = 0usize;
    while lb.get(n) == Some(&b'=') {
        n += 1;
    }
    if n < 2 || lb.get(n) != Some(&b' ') {
        return None;
    }
    let level = n.saturating_sub(1).min(255) as u8;
    Some((level, String::from_utf8_lossy(&lb[..n]).into_owned()))
}

fn section_content_off(lb: &[u8]) -> usize {
    let mut n = 0usize;
    while lb.get(n) == Some(&b'=') {
        n += 1;
    }
    let mut k = n;
    while lb.get(k) == Some(&b' ') {
        k += 1;
    }
    k
}

/// If `lb` is a document attribute entry (`:name: value`, `:name!:`), return it.
fn attribute_entry(lb: &[u8]) -> Option<Attribute> {
    if leading_spaces(lb) != 0 || lb.first() != Some(&b':') {
        return None;
    }
    let mut k = 1usize;
    while let Some(&c) = lb.get(k) {
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' {
            k += 1;
        } else {
            break;
        }
    }
    if k == 1 {
        return None;
    }
    let name = String::from_utf8_lossy(&lb[1..k]).into_owned();
    match lb.get(k) {
        Some(&b'!') => {
            if lb.get(k + 1) == Some(&b':') {
                Some(Attribute {
                    name,
                    value_off: lb.len(),
                    unset: true,
                })
            } else {
                None
            }
        }
        Some(&b':') => {
            let mut off = k + 1;
            while lb.get(off) == Some(&b' ') {
                off += 1;
            }
            Some(Attribute {
                name,
                value_off: off,
                unset: false,
            })
        }
        _ => None,
    }
}

/// If `lb` is a block attribute line `[...]`, return its inner text.
fn block_attr_line(lb: &[u8]) -> Option<&[u8]> {
    if leading_spaces(lb) != 0 || lb.first() != Some(&b'[') {
        return None;
    }
    let n = lb.len();
    if n < 3 || lb[n - 1] != b']' {
        return None;
    }
    let inner = &lb[1..n - 1];
    if inner.contains(&b'[') || inner.contains(&b']') {
        return None;
    }
    Some(inner)
}

/// The recognized admonition labels.
const ADMONITION_LABELS: [&str; 5] = ["NOTE", "TIP", "IMPORTANT", "WARNING", "CAUTION"];

/// If `inner` is exactly an admonition label, return it.
fn admonition_block(inner: &[u8]) -> Option<&'static str> {
    let s = trim_ascii(inner);
    ADMONITION_LABELS
        .iter()
        .copied()
        .find(|&l| s == l.as_bytes())
}

/// If `lb` begins with an admonition paragraph (`NOTE: …`), return the label.
fn admonition_paragraph(lb: &[u8]) -> Option<&'static str> {
    if leading_spaces(lb) != 0 {
        return None;
    }
    ADMONITION_LABELS.iter().copied().find(|&l| {
        let b = l.as_bytes();
        lb.len() > b.len()
            && &lb[..b.len()] == b
            && lb[b.len()] == b':'
            && (lb.len() == b.len() + 1 || lb[b.len() + 1] == b' ')
    })
}

/// If `lb` is a complete delimited-block delimiter, return its block kind.
fn delimiter_kind(lb: &[u8]) -> Option<u8> {
    if lb.is_empty() {
        return None;
    }
    let c = lb[0];
    if !lb.iter().all(|&x| x == c) {
        return None;
    }
    let n = lb.len();
    if c == b'-' && n == 2 {
        return Some(B_OPEN);
    }
    if n < 4 {
        return None;
    }
    match c {
        b'-' => Some(B_LISTING),
        b'.' => Some(B_LITERAL),
        b'=' => Some(B_EXAMPLE),
        b'*' => Some(B_SIDEBAR),
        b'_' => Some(B_QUOTE),
        b'+' => Some(B_PASSTHROUGH),
        _ => None,
    }
}

/// The trimmed cell spans of a table row (offsets within `lb`). Each cell begins at a
/// `|` and runs to the next `|` or the end of the line (an AsciiDoc row need not end
/// with a trailing `|`).
fn cell_spans(lb: &[u8]) -> Vec<(usize, usize)> {
    let mut bars = Vec::new();
    for (idx, &c) in lb.iter().enumerate() {
        if c == b'|' {
            bars.push(idx);
        }
    }
    let mut out = Vec::new();
    for (n, &b) in bars.iter().enumerate() {
        let mut a = b + 1;
        let mut e = bars.get(n + 1).copied().unwrap_or(lb.len());
        while a < e && (lb[a] == b' ' || lb[a] == b'\t') {
            a += 1;
        }
        while e > a && (lb[e - 1] == b' ' || lb[e - 1] == b'\t') {
            e -= 1;
        }
        out.push((a, e));
    }
    out
}

/// Parse a bullet or ordered list marker. Returns `None` if the line is not one.
fn parse_list_marker(lb: &[u8]) -> Option<ListMarker> {
    let indent = leading_spaces(lb);
    let rest = &lb[indent..];
    let first = *rest.first()?;
    if first == b'*' || first == b'.' {
        // A run of the marker character sets the nesting level.
        let mut run = 0usize;
        while rest.get(run) == Some(&first) {
            run += 1;
        }
        if !matches!(rest.get(run), Some(&b' ') | Some(&b'\t')) {
            return None;
        }
        let mut off = indent + run;
        while lb.get(off) == Some(&b' ') {
            off += 1;
        }
        let level = (run as u32).max(indent as u32 / 2 + 1).min(255) as u8;
        return Some(ListMarker {
            ordered: first == b'.',
            marker: (first as char).to_string().repeat(run),
            indent,
            content_off: off,
            level,
        });
    }
    if first == b'-' && matches!(rest.get(1), Some(&b' ') | Some(&b'\t')) {
        let mut off = indent + 1;
        while lb.get(off) == Some(&b' ') {
            off += 1;
        }
        let level = ((indent / 2 + 1).min(255)) as u8;
        return Some(ListMarker {
            ordered: false,
            marker: "-".to_string(),
            indent,
            content_off: off,
            level,
        });
    }
    None
}

/// If `lb` is a description-list entry (`term:: def`), return `(term, def_off)`.
/// A space (or end of line) must follow `::` so a block macro (`image::path[]`) is
/// never mistaken for a term.
fn description_line(lb: &[u8]) -> Option<(String, usize)> {
    if leading_spaces(lb) != 0 {
        return None;
    }
    let mut k = 0usize;
    while k + 1 < lb.len() {
        if lb[k] == b':' && lb[k + 1] == b':' {
            let after = lb.get(k + 2);
            if matches!(after, None | Some(&b' ')) {
                let term = trim_ascii(&lb[..k]);
                if term.is_empty() {
                    return None;
                }
                let mut off = k + 2;
                while lb.get(off) == Some(&b' ') {
                    off += 1;
                }
                return Some((String::from_utf8_lossy(term).into_owned(), off));
            }
        }
        k += 1;
    }
    None
}

fn trim_ascii(s: &[u8]) -> &[u8] {
    let mut a = 0usize;
    let mut b = s.len();
    while a < b && (s[a] == b' ' || s[a] == b'\t') {
        a += 1;
    }
    while b > a && (s[b - 1] == b' ' || s[b - 1] == b'\t') {
        b -= 1;
    }
    &s[a..b]
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn corrupt(msg: &str) -> Error {
    Error::invalid_asciidoc_structure(format!("malformed AsciiDoc: {msg}"))
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

    fn model(src: &str) -> AdocModel {
        parse(src.as_bytes(), Limits::DEFAULT, true).unwrap()
    }

    #[test]
    fn detects_structure_but_not_prose() {
        let l = Limits::DEFAULT;
        assert!(detect(b"= Doc Title\n\nA paragraph.\n", l));
        assert!(detect(b"== Section\n\nbody\n", l));
        // A `|===` table inside a document carrying a title is an AsciiDoc mark. (A
        // *bare* `|===` table whose lines are a majority pipe-delimited is claimed by
        // the CSV/PSV adapter first; the dispatcher tries CSV before AsciiDoc.)
        assert!(detect(b"= T\n\n|===\n| a | b\n| 1 | 2\n|===\n", l));
        assert!(detect(b"----\nlisting body\n----\n", l));
        // Plain prose has no AsciiDoc structural mark and stays Opaque.
        assert!(!detect(b"just some prose\nwith more lines\n", l));
        assert!(!detect(b"", l));
        // A lone `= text` line (no further block) is too weak to claim.
        assert!(!detect(b"= lonely\n", l));
    }

    #[test]
    fn title_and_sections_have_exact_levels() {
        let m = model("= Doc\n\n== One\n\n=== Two\n\nbody\n");
        let titles = m.blocks_of_kind(B_DOC_TITLE);
        assert_eq!(titles.len(), 1);
        assert_eq!(m.blocks[titles[0] as usize].level, 0);
        let secs = m.blocks_of_kind(B_SECTION);
        assert_eq!(secs.len(), 2);
        assert_eq!(m.blocks[secs[0] as usize].level, 1);
        assert_eq!(m.blocks[secs[1] as usize].level, 2);
        assert_eq!(m.blocks[secs[0] as usize].info.as_deref(), Some("=="));
    }

    #[test]
    fn attributes_and_refs_are_literal() {
        let m = model("= Doc\n\n:author: Jane\n:off!:\n\nSee {author}.\n");
        let attrs = m.blocks_of_kind(B_ATTRIBUTE);
        assert_eq!(attrs.len(), 2);
        assert_eq!(m.blocks[attrs[0] as usize].info.as_deref(), Some("author"));
        assert_eq!(m.blocks[attrs[0] as usize].title.as_deref(), Some("Jane"));
        assert_ne!(m.blocks[attrs[1] as usize].flags & F_UNSET, 0);
        assert_eq!(m.inlines_of_kind(I_ATTR_REF).len(), 1);
    }

    #[test]
    fn delimited_blocks_are_verbatim() {
        let m = model("= Doc\n\n----\ncode `x`\n----\n\n****\nsidebar\n****\n");
        assert_eq!(m.blocks_of_kind(B_LISTING).len(), 1);
        assert_eq!(m.blocks_of_kind(B_SIDEBAR).len(), 1);
        // No inline extraction inside a delimited block.
        assert_eq!(m.inlines.len(), 0);
        let l = m.blocks_of_kind(B_LISTING)[0] as usize;
        assert_eq!(m.blocks[l].info.as_deref(), Some("----"));
    }

    #[test]
    fn block_attrs_attach_to_the_next_block() {
        let m = model("= Doc\n\n[source,rust]\n----\nfn main() {}\n----\n");
        let attrs = m.blocks_of_kind(B_BLOCK_ATTR);
        assert_eq!(attrs.len(), 1);
        assert_eq!(
            m.blocks[attrs[0] as usize].info.as_deref(),
            Some("source,rust")
        );
        let listing = m.blocks_of_kind(B_LISTING)[0] as usize;
        assert_eq!(m.blocks[listing].attrs.as_deref(), Some("source,rust"));
        assert_ne!(m.blocks[listing].flags & F_ATTRS, 0);
    }

    #[test]
    fn inline_spans_and_model_roundtrip() {
        let src = "Text with *strong*, _em_, `mono`, +pass+, ^sup^, ~sub~, #mark#, ##mark##.\n";
        let m = model(src);
        let bytes = m.encode();
        let back = AdocModel::decode(&bytes).unwrap();
        assert_eq!(m, back);
        assert_eq!(m.inlines_of_kind(I_STRONG).len(), 1);
        assert_eq!(m.inlines_of_kind(I_EMPHASIS).len(), 1);
        assert_eq!(m.inlines_of_kind(I_MONO).len(), 1);
        assert_eq!(m.inlines_of_kind(I_PASSTHROUGH).len(), 1);
        assert_eq!(m.inlines_of_kind(I_SUPERSCRIPT).len(), 1);
        assert_eq!(m.inlines_of_kind(I_SUBSCRIPT).len(), 1);
        assert_eq!(m.inlines_of_kind(I_MARK).len(), 1);
        assert_eq!(m.inlines_of_kind(I_MARK_DOUBLE).len(), 1);
    }

    #[test]
    fn macros_are_preserved_verbatim() {
        let src = "See link:https://x.example[A] and image:logo.png[Logo] and include::ch.adoc[] and xref:s1[One] and https://y.example[B].\n";
        let m = model(&format!("= Doc\n\n{src}"));
        assert_eq!(m.inlines_of_kind(I_LINK).len(), 1);
        assert_eq!(m.inlines_of_kind(I_IMAGE).len(), 1);
        assert_eq!(m.inlines_of_kind(I_INCLUDE).len(), 1);
        assert_eq!(m.inlines_of_kind(I_XREF).len(), 1);
        assert_eq!(m.inlines_of_kind(I_URL).len(), 1);
    }

    #[test]
    fn stacked_block_attrs_attach_and_mid_doc_equals_is_a_paragraph() {
        let m = model("= Doc\n\n[a]\n[b, c]\n----\ncode\n----\n\n= not a title\n");
        assert_eq!(m.blocks_of_kind(B_BLOCK_ATTR).len(), 2);
        let listing = m.blocks_of_kind(B_LISTING)[0] as usize;
        assert_eq!(m.blocks[listing].attrs.as_deref(), Some("a\nb, c"));
        // The document title is the *first* block only; a later `= x` line is a
        // paragraph.
        assert_eq!(m.blocks_of_kind(B_DOC_TITLE).len(), 1);
    }

    #[test]
    fn fails_closed_on_corruption() {
        let m = model("= Doc\n\n== S\n\nbody\n");
        let enc = m.encode();
        assert_eq!(AdocModel::decode(&enc).unwrap(), m);
        let mut bad = enc.clone();
        bad[0] = b'X';
        assert!(AdocModel::decode(&bad).is_err());
        let mut truncated = enc;
        truncated.truncate(truncated.len() - 1);
        assert!(AdocModel::decode(&truncated).is_err());
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0x0adc_1234_5678_9abc;
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
