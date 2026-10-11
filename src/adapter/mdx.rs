//! Bounded, representation-preserving MDX adapter (Phase 21.26.3).
//!
//! MDX is Markdown with JSX and ESM (ECMAScript module) statements layered on top.
//! It is a **superset** of Markdown: rather than writing a second prose parser this
//! adapter **reuses the Markdown parser and model** ([`crate::adapter::markdown`])
//! for the prose/block structure and adds the MDX-specific constructs on top, each
//! with an exact source span. Like JSON5 over JSON (Phase 21.17.1) or JSONL over the
//! per-line JSON parser, the base grammar is shared and never re-implemented.
//!
//! MDX is *not* a package: there is no OPC/ZIP layer, no `mimetype`, and no
//! relationship graph. The exact leaf is the **whole source** (a `DocumentExact`, a
//! RAW-like authority), and everything this module produces is a bounded,
//! deterministic (`Q_gen`) projection that never sits on the exactness path: a
//! block's or element's exact bytes are literally `source[span]`.
//!
//! ## What is preserved
//!
//! * the full **Markdown surface** (front matter, ATX headings, paragraphs, lists,
//!   fenced/indented code, blockquotes, tables, reference definitions, footnotes,
//!   inline spans) — via the reused [`MarkdownModel`];
//! * **ESM statements**: top-level `import … from '…'` (default, named, namespace,
//!   bare, and multi-line) and `export …` (`default`, declarations, `{…}` lists,
//!   `export * from '…'`), preserved verbatim with their exact spans;
//! * **JSX blocks**: elements `<Component …>` / `<Component … />`, fragments
//!   `<>…</>`, with their **attributes** and **nested children** (children are
//!   themselves MDX/Markdown), each with an exact span and a recorded nesting depth.
//!   JSX is **never executed and never parsed as JavaScript**: only its extent,
//!   name, attribute count, and child count are recorded;
//! * **MDX expressions**: `{ … }` inline and block (e.g. `{frontmatter.title}`,
//!   `{/* comment */}`), preserved verbatim. Braces are balanced with string
//!   literals (`"…"`, `'…'`, `` `…` ``), escapes, `//` line comments, and `/* … */`
//!   block comments respected, so a brace inside a string does not close the
//!   expression.
//!
//! ## Detection (conservative; no magic bytes)
//!
//! MDX has no magic bytes and its bytes are Markdown-plus-JSX, so it is detected
//! **before** generic Markdown and **before** HTML, and only claims a source that
//! (a) parses under the Markdown model **and** (b) carries an **MDX-specific**
//! signal — one of:
//!
//! * a top-level (column-0) ESM `import`/`export` statement;
//! * a JSX element whose name is a **component** (an ASCII uppercase letter appears
//!   in the name, e.g. `<Foo>`, `<Foo.Bar>`, `<svg:linearGradient>`);
//! * a JSX **fragment** `<>`;
//! * a **JSX attribute** — a brace-valued attribute (`prop={…}`) or a spread
//!   (`{…props}`), which HTML has no syntax for;
//! * a **block expression** — a physical line that is entirely a `{ … }`
//!   expression (e.g. `{frontmatter.title}` or `{/* comment */}` on its own line).
//!
//! A plain Markdown document stays [`DocumentFormat::Markdown`], a plain HTML
//! document stays [`DocumentFormat::Html`], and plain prose stays
//! [`DocumentFormat::Opaque`](crate::field::document_format::DocumentFormat::Opaque).
//! The MDX scanner **skips fenced/indented code, front matter, and inline code
//! spans** (taken from the reused Markdown model), so a construct inside code never
//! makes a Markdown document MDX.
//!
//! ## The boundary it cannot cross (honest)
//!
//! * A **plain HTML element** with only lowercase tags and quoted attributes
//!   (`<div class="x">…</div>`) is **indistinguishable** from the same JSX: it
//!   carries no component name, no brace-valued attribute, and no fragment, so MDX
//!   declines it and it stays `Html`. Only a capitalized/namespaced name or a
//!   JSX-specific attribute distinguishes JSX from HTML.
//! * An **inline** `{…}` expression alone does **not** admit a document as MDX:
//!   plain prose is full of balanced braces. Only a whole-line block expression (or
//!   a component/ESM/JSX-attribute signal) admits it, so a Markdown document with an
//!   inline `{x}` stays `Markdown`.
//! * A document that also carries an RST/AsciiDoc/JSON/YAML/CSV/HTML/XML-specific
//!   signal is declined here (defence in depth), so MDX never steals those formats.
//!
//! ## Bounds and no panics
//!
//! Untrusted input can only ever yield a typed decline, never a panic or an
//! unbounded allocation: the ESM-statement count by
//! [`Limits::max_mdx_esm_statements`], the JSX-element count by
//! [`Limits::max_mdx_jsx_blocks`], the expression count by
//! [`Limits::max_mdx_expressions`], the total node count by [`Limits::max_mdx_nodes`],
//! the JSX nesting depth by [`Limits::max_mdx_depth`], a single physical line's bytes
//! by [`Limits::max_mdx_line_bytes`], and the source length by
//! [`Limits::max_mdx_document_bytes`]. An unbalanced or ambiguous brace in a
//! non-code position is a **typed decline** (so the source stays Opaque/Markdown).

use crate::adapter::markdown::{self, MarkdownModel};
use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model ESM statements (defends the decoder against a hostile blob).
pub const MAX_MODEL_ESM: u32 = 1 << 22;
/// Hard cap on decoded model JSX elements (defends the decoder against a hostile blob).
pub const MAX_MODEL_JSX: u32 = 1 << 22;
/// Hard cap on decoded model expressions (defends the decoder against a hostile blob).
pub const MAX_MODEL_EXPRESSIONS: u32 = 1 << 24;

/// ESM statement kind: an `import …` statement.
pub const E_IMPORT: u8 = 0;
/// ESM statement kind: an `export …` statement.
pub const E_EXPORT: u8 = 1;

/// Stable lower-case ESM-kind name.
pub const fn esm_kind_name(kind: u8) -> &'static str {
    match kind {
        E_IMPORT => "import",
        E_EXPORT => "export",
        _ => "unknown",
    }
}

/// JSX element kind: an element (`<Name …>` or `<Name … />`).
pub const J_ELEMENT: u8 = 0;
/// JSX element kind: a fragment (`<>…</>`).
pub const J_FRAGMENT: u8 = 1;

/// Stable lower-case JSX-kind name.
pub const fn jsx_kind_name(kind: u8) -> &'static str {
    match kind {
        J_ELEMENT => "element",
        J_FRAGMENT => "fragment",
        _ => "unknown",
    }
}

/// JSX flag bit: a self-closing element (`<Name … />`).
pub const F_SELF_CLOSING: u8 = 1;
/// JSX flag bit: the element name is a **component** (an uppercase letter appears).
pub const F_COMPONENT: u8 = 2;
/// JSX flag bit: the element start tag carries at least one attribute.
pub const F_HAS_ATTRS: u8 = 4;
/// JSX flag bit: the element carries a JSX-specific attribute (brace value or spread).
pub const F_JSX_ATTR: u8 = 8;
/// JSX flag bit: the element name is namespaced (`a.b` or `a:b`).
pub const F_NAMESPACED: u8 = 16;
/// JSX flag bit: the element carries a spread attribute (`{…props}`).
pub const F_HAS_SPREAD: u8 = 32;

/// Expression flag bit: the expression occupies a whole physical line (a block position).
pub const X_BLOCK: u8 = 1;
/// Expression flag bit: the expression is a `{/* … */}` comment.
pub const X_COMMENT: u8 = 2;

/// MDX signal bit: a top-level ESM statement was seen.
pub const S_ESM: u8 = 1;
/// MDX signal bit: a JSX component element was seen.
pub const S_COMPONENT: u8 = 2;
/// MDX signal bit: a JSX fragment was seen.
pub const S_FRAGMENT: u8 = 4;
/// MDX signal bit: a JSX-specific attribute was seen.
pub const S_JSX_ATTR: u8 = 8;
/// MDX signal bit: a whole-line block expression was seen.
pub const S_BLOCK_EXPR: u8 = 16;

/// One ESM statement: its kind and exact source span (terminator excluded).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdxEsm {
    /// `E_IMPORT` or `E_EXPORT`.
    pub kind: u8,
    /// The statement's first source byte.
    pub start: u64,
    /// One past the statement's last source byte (its line terminator excluded).
    pub end: u64,
}

/// One JSX element or fragment: its kind, exact source span, nesting depth, and
/// structural facts (name, attribute count, direct-child count). Never executed and
/// never parsed as JavaScript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdxJsx {
    /// `J_ELEMENT` or `J_FRAGMENT`.
    pub kind: u8,
    /// `F_*` flag bits.
    pub flags: u8,
    /// The nesting depth (a top-level element is depth 1).
    pub depth: u32,
    /// The element's first source byte (its `<`).
    pub start: u64,
    /// One past the element's last source byte.
    pub end: u64,
    /// The element name (`None` for a fragment).
    pub name: Option<String>,
    /// The number of attributes on the element's start tag.
    pub attrs: u32,
    /// The number of direct child JSX elements/fragments.
    pub children: u32,
}

/// One MDX expression `{ … }`: its flags, nesting depth, and exact span (braces
/// included).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdxExpr {
    /// `X_*` flag bits.
    pub flags: u8,
    /// The nesting depth.
    pub depth: u32,
    /// The expression's first source byte (its `{`).
    pub start: u64,
    /// One past the expression's last source byte (just past its `}`).
    pub end: u64,
}

/// The canonical derived MDX model (the materialization of an `MdxModel` node): the
/// reused [`MarkdownModel`] plus the MDX-specific arenas and the bit-set of MDX
/// signals observed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdxModel {
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// `S_*` signal bits observed by the scanner.
    pub signals: u8,
    /// The reused, complete Markdown model of the same source.
    pub md: MarkdownModel,
    /// ESM statements, in document order.
    pub esm: Vec<MdxEsm>,
    /// JSX elements and fragments, in document order (outer before inner).
    pub jsx: Vec<MdxJsx>,
    /// MDX expressions, in document order.
    pub exprs: Vec<MdxExpr>,
}

impl MdxModel {
    /// The ESM statement at `index`, if present.
    pub fn esm(&self, index: u32) -> Option<&MdxEsm> {
        self.esm.get(index as usize)
    }

    /// The JSX element at `index`, if present.
    pub fn jsx(&self, index: u32) -> Option<&MdxJsx> {
        self.jsx.get(index as usize)
    }

    /// The expression at `index`, if present.
    pub fn expr(&self, index: u32) -> Option<&MdxExpr> {
        self.exprs.get(index as usize)
    }

    /// Whether the source carries an MDX-specific signal that admits it as MDX.
    ///
    /// A **strong** signal (a top-level ESM statement, a JSX component/fragment, or a
    /// JSX-specific attribute) admits the source on its own. A **block expression**
    /// `{ … }` is weaker — plain prose and JSON-shaped blobs contain balanced braces —
    /// so it admits the source only when the reused Markdown model also carries a
    /// structural mark (a heading, a fence, front matter, a table, a reference
    /// definition, or a footnote definition). This is the documented boundary that
    /// keeps plain prose, a brace-bearing text blob, and a plain Markdown document out.
    pub fn has_mdx_signal(&self) -> bool {
        let strong = self.signals & (S_ESM | S_COMPONENT | S_FRAGMENT | S_JSX_ATTR) != 0;
        let block_expr = self.signals & S_BLOCK_EXPR != 0;
        strong || (block_expr && self.md.has_structural_signal())
    }

    /// The observed maximum JSX nesting depth (0 if there is no JSX).
    pub fn max_jsx_depth(&self) -> u32 {
        self.jsx.iter().map(|j| j.depth).max().unwrap_or(0)
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let md = self.md.encode();
        let mut out = Vec::with_capacity(64 + md.len() + self.esm.len() * 24);
        out.extend_from_slice(b"MDX1");
        out.push(MODEL_VERSION);
        out.push(self.signals);
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&(md.len() as u32).to_le_bytes());
        out.extend_from_slice(&md);
        out.extend_from_slice(&(self.esm.len() as u32).to_le_bytes());
        for e in &self.esm {
            out.push(e.kind);
            out.extend_from_slice(&e.start.to_le_bytes());
            out.extend_from_slice(&e.end.to_le_bytes());
        }
        out.extend_from_slice(&(self.jsx.len() as u32).to_le_bytes());
        for j in &self.jsx {
            out.push(j.kind);
            out.push(j.flags);
            out.extend_from_slice(&j.depth.to_le_bytes());
            out.extend_from_slice(&j.start.to_le_bytes());
            out.extend_from_slice(&j.end.to_le_bytes());
            put_opt(&mut out, j.name.as_deref());
            out.extend_from_slice(&j.attrs.to_le_bytes());
            out.extend_from_slice(&j.children.to_le_bytes());
        }
        out.extend_from_slice(&(self.exprs.len() as u32).to_le_bytes());
        for x in &self.exprs {
            out.push(x.flags);
            out.extend_from_slice(&x.depth.to_le_bytes());
            out.extend_from_slice(&x.start.to_le_bytes());
            out.extend_from_slice(&x.end.to_le_bytes());
        }
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<MdxModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"MDX1" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let signals = r.u8()?;
        let doc_len = r.u64()?;
        let md_len = r.u32()? as usize;
        let md_bytes = r.bytes(md_len)?;
        let md = MarkdownModel::decode(md_bytes)?;
        if md.doc_len != doc_len {
            return Err(corrupt("embedded Markdown model length mismatch"));
        }
        let esm_n = r.u32()?;
        if esm_n > MAX_MODEL_ESM {
            return Err(corrupt("model ESM count is implausible"));
        }
        let mut esm = Vec::with_capacity(esm_n as usize);
        for _ in 0..esm_n {
            let kind = r.u8()?;
            if kind > E_EXPORT {
                return Err(corrupt("unknown ESM kind"));
            }
            let start = r.u64()?;
            let end = r.u64()?;
            if start > end || end > doc_len {
                return Err(corrupt("ESM span is outside the document"));
            }
            esm.push(MdxEsm { kind, start, end });
        }
        let jsx_n = r.u32()?;
        if jsx_n > MAX_MODEL_JSX {
            return Err(corrupt("model JSX count is implausible"));
        }
        let mut jsx = Vec::with_capacity(jsx_n as usize);
        for _ in 0..jsx_n {
            let kind = r.u8()?;
            if kind > J_FRAGMENT {
                return Err(corrupt("unknown JSX kind"));
            }
            let flags = r.u8()?;
            let depth = r.u32()?;
            let start = r.u64()?;
            let end = r.u64()?;
            if start > end || end > doc_len {
                return Err(corrupt("JSX span is outside the document"));
            }
            let name = r.opt_str()?;
            let attrs = r.u32()?;
            let children = r.u32()?;
            jsx.push(MdxJsx {
                kind,
                flags,
                depth,
                start,
                end,
                name,
                attrs,
                children,
            });
        }
        let expr_n = r.u32()?;
        if expr_n > MAX_MODEL_EXPRESSIONS {
            return Err(corrupt("model expression count is implausible"));
        }
        let mut exprs = Vec::with_capacity(expr_n as usize);
        for _ in 0..expr_n {
            let flags = r.u8()?;
            let depth = r.u32()?;
            let start = r.u64()?;
            let end = r.u64()?;
            if start > end || end > doc_len {
                return Err(corrupt("expression span is outside the document"));
            }
            exprs.push(MdxExpr {
                flags,
                depth,
                start,
                end,
            });
        }
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        Ok(MdxModel {
            doc_len,
            signals,
            md,
            esm,
            jsx,
            exprs,
        })
    }
}

/// Byte-based MDX detector. Claims a source that parses under the Markdown model and
/// carries an MDX-specific signal, and that is not claimed by a sibling family
/// (JSON/YAML/CSV/HTML/XML/RST/AsciiDoc). See the module docs for the boundaries.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.is_empty() {
        return false;
    }
    if source.len() as u64 > limits.max_mdx_document_bytes {
        return false;
    }
    if looks_like_other(source, limits) {
        return false;
    }
    match parse(source, limits, false) {
        Ok(m) => m.has_mdx_signal(),
        Err(_) => false,
    }
}

/// Whether `source` belongs to a family that is *not* MDX (defence in depth; the
/// dispatcher also orders the strong formats ahead of MDX).
fn looks_like_other(source: &[u8], limits: Limits) -> bool {
    let _ = &limits;
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
    if looks_like_markup_document(source) {
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
    #[cfg(feature = "rst")]
    if crate::adapter::rst::detect(source, limits) {
        return true;
    }
    #[cfg(feature = "asciidoc")]
    if crate::adapter::asciidoc::detect(source, limits) {
        return true;
    }
    // A well-formed XML/feed/GIS document must never be claimed by MDX on the strength
    // of a capitalized element name (`<Document>`, `<Placemark>`) or a lowercase
    // element; the XML families would otherwise be read as JSX components.
    #[cfg(feature = "xml")]
    if crate::adapter::xml::detect(source, limits) {
        return true;
    }
    false
}

/// Whether the source begins (after a BOM and whitespace) with an XML/HTML
/// document marker. A document that is genuinely markup must never be claimed by
/// MDX on the strength of a `<div>`-only element or a `{…}` inside a script.
fn looks_like_markup_document(source: &[u8]) -> bool {
    let mut i = 0usize;
    if source.starts_with(&[0xEF, 0xBB, 0xBF]) {
        i = 3;
    }
    while i < source.len() && matches!(source[i], b' ' | b'\t' | b'\r' | b'\n') {
        i += 1;
    }
    let rest = &source[i..];
    starts_with_ci(rest, b"<!doctype html")
        || starts_with_ci(rest, b"<html")
        || starts_with_ci(rest, b"<?xml")
}

/// ASCII case-insensitive `starts_with`.
fn starts_with_ci(hay: &[u8], needle: &[u8]) -> bool {
    hay.len() >= needle.len()
        && hay[..needle.len()]
            .iter()
            .zip(needle)
            .all(|(a, b)| a.to_ascii_lowercase() == *b)
}

/// Parse `source` and return its canonical derived model, encoded (the
/// materialization of an `MdxModel` node).
pub fn build_mdx_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits, true)?.encode())
}

/// Parse `source` into an [`MdxModel`]. The reused Markdown model is always built in
/// full (so inline code spans can be protected); `build` selects whether the
/// MDX-specific arenas are recorded (detection runs with `build = false`).
pub fn parse(source: &[u8], limits: Limits, build: bool) -> Result<MdxModel> {
    if source.len() as u64 > limits.max_mdx_document_bytes {
        return Err(Error::resource_limit(format!(
            "MDX source is {} bytes, above the {}-byte document cap",
            source.len(),
            limits.max_mdx_document_bytes
        )));
    }
    check_lines(source, limits)?;
    let md = markdown::parse(source, limits, true)?;
    let mut scan = Scan::new(source, &md, limits, build);
    scan.run()?;
    Ok(MdxModel {
        doc_len: source.len() as u64,
        signals: scan.signals,
        md,
        esm: scan.esm,
        jsx: scan.jsx,
        exprs: scan.exprs,
    })
}

/// A deterministic canonical text projection: the exact source bytes, lossily
/// decoded. MDX is **not** rendered, so the canonical text is the source itself.
pub fn canonical_text(source: &[u8], limits: Limits, max_out: u64) -> Result<String> {
    markdown::canonical_text(source, limits, max_out)
}

/// A bounded lexical search over the Markdown block content (reusing the Markdown
/// finder over the embedded [`MarkdownModel`]).
pub fn find(
    source: &[u8],
    model: &MdxModel,
    pattern: &str,
    max_out: u64,
) -> Result<Vec<markdown::MdMatch>> {
    markdown::find(source, &model.md, pattern, max_out)
}

/// Decline if any physical line's content (terminator excluded) exceeds the cap.
fn check_lines(source: &[u8], limits: Limits) -> Result<()> {
    let cap = limits.max_mdx_line_bytes;
    let mut i = 0usize;
    while i < source.len() {
        let start = i;
        while i < source.len() && source[i] != b'\n' {
            i += 1;
        }
        let mut end = i;
        if end > start && source[end - 1] == b'\r' {
            end -= 1;
        }
        if (end - start) as u64 > cap {
            return Err(Error::resource_limit(format!(
                "MDX line exceeds the {cap}-byte line cap"
            )));
        }
        if i < source.len() {
            i += 1;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The scanner
// ---------------------------------------------------------------------------

struct Scan<'a> {
    b: &'a [u8],
    limits: Limits,
    build: bool,
    /// Sorted, non-overlapping `[start, end)` regions where MDX constructs never
    /// apply (fenced/indented code, front matter, inline code spans).
    protected: Vec<(u64, u64)>,
    esm: Vec<MdxEsm>,
    jsx: Vec<MdxJsx>,
    exprs: Vec<MdxExpr>,
    signals: u8,
    node_count: u64,
}

impl<'a> Scan<'a> {
    fn new(b: &'a [u8], md: &MarkdownModel, limits: Limits, build: bool) -> Scan<'a> {
        use crate::adapter::markdown::{B_FENCE, B_FRONT_MATTER, B_INDENTED_CODE, I_CODE};
        let mut regions: Vec<(u64, u64)> = Vec::new();
        for blk in &md.blocks {
            if matches!(blk.kind, B_FENCE | B_INDENTED_CODE | B_FRONT_MATTER) {
                regions.push((blk.start, blk.end));
            }
        }
        for inl in &md.inlines {
            if inl.kind == I_CODE {
                regions.push((inl.start, inl.end));
            }
        }
        regions.sort_unstable();
        let mut protected: Vec<(u64, u64)> = Vec::with_capacity(regions.len());
        for (s, e) in regions {
            if let Some(last) = protected.last_mut()
                && s <= last.1
            {
                last.1 = last.1.max(e);
                continue;
            }
            protected.push((s, e));
        }
        Scan {
            b,
            limits,
            build,
            protected,
            esm: Vec::new(),
            jsx: Vec::new(),
            exprs: Vec::new(),
            signals: 0,
            node_count: 0,
        }
    }

    fn run(&mut self) -> Result<()> {
        let end = self.b.len();
        let mut i = 0usize;
        while i < end {
            if let Some(pe) = self.protected_end(i) {
                i = pe;
                continue;
            }
            let c = self.b[i];
            if c == b'{' {
                i = self.scan_expression(i, end, 0)?;
                continue;
            }
            if c == b'<' {
                if let Some(n) = self.scan_jsx_at(i, end, 1)? {
                    i = n;
                    continue;
                }
                i += 1;
                continue;
            }
            if (i == 0 || self.b[i - 1] == b'\n')
                && (c == b'i' || c == b'e')
                && let Some(n) = self.try_esm(i, end)?
            {
                i = n;
                continue;
            }
            i += 1;
        }
        Ok(())
    }

    /// The end of the protected region containing `pos`, if any.
    fn protected_end(&self, pos: usize) -> Option<usize> {
        let mut lo = 0usize;
        let mut hi = self.protected.len();
        while lo < hi {
            let mid = (lo + hi) / 2;
            if (self.protected[mid].0 as usize) <= pos {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        if lo == 0 {
            return None;
        }
        let (s, e) = self.protected[lo - 1];
        if pos >= s as usize && pos < e as usize {
            Some(e as usize)
        } else {
            None
        }
    }

    fn is_line_start(&self, pos: usize) -> bool {
        let mut i = pos;
        while i > 0 {
            let c = self.b[i - 1];
            if c == b'\n' {
                return true;
            }
            if c != b' ' && c != b'\t' {
                return false;
            }
            i -= 1;
        }
        true
    }

    fn is_line_end(&self, pos: usize) -> bool {
        let mut i = pos;
        while i < self.b.len() {
            let c = self.b[i];
            if c == b'\n' {
                return true;
            }
            if c != b' ' && c != b'\t' && c != b'\r' {
                return false;
            }
            i += 1;
        }
        true
    }

    // -- expressions ----------------------------------------------------------

    /// Scan the brace-balanced expression starting at `start` (a `{`); record it and
    /// return the index just past its matching `}`.
    fn scan_expression(&mut self, start: usize, end: usize, depth: u32) -> Result<usize> {
        let (e, is_comment) = self.scan_balance(start, end, b'{', b'}')?;
        let block = self.is_line_start(start) && self.is_line_end(e);
        let mut flags = 0u8;
        if block {
            flags |= X_BLOCK;
        }
        if is_comment {
            flags |= X_COMMENT;
        }
        if block {
            self.signals |= S_BLOCK_EXPR;
        }
        self.push_node()?;
        if self.build {
            if self.exprs.len() as u64 >= self.limits.max_mdx_expressions as u64 {
                return Err(Error::resource_limit(format!(
                    "MDX document exceeds the {}-expression cap",
                    self.limits.max_mdx_expressions
                )));
            }
            self.exprs.push(MdxExpr {
                flags,
                depth,
                start: start as u64,
                end: e as u64,
            });
        }
        Ok(e)
    }

    /// Scan a balanced `open … close` region (with string literals, escapes, line
    /// comments, and block comments respected). Returns the index just past the
    /// matching `close` and whether the content is a `/* … */` comment.
    fn scan_balance(&self, start: usize, end: usize, open: u8, close: u8) -> Result<(usize, bool)> {
        let mut i = start + 1;
        let mut depth = 1i32;
        let mut quote = 0u8;
        let mut esc = false;
        let mut line_comment = false;
        let mut block_comment = 0i32;
        let content_start = start + 1;
        while i < end {
            let c = self.b[i];
            if line_comment {
                if c == b'\n' {
                    line_comment = false;
                }
                i += 1;
                continue;
            }
            if block_comment > 0 {
                if c == b'*' && self.b.get(i + 1) == Some(&b'/') {
                    block_comment -= 1;
                    i += 2;
                    continue;
                }
                i += 1;
                continue;
            }
            if quote != 0 {
                if esc {
                    esc = false;
                } else if c == b'\\' {
                    esc = true;
                } else if c == quote {
                    quote = 0;
                }
                i += 1;
                continue;
            }
            match c {
                b'"' | b'\'' | b'`' => quote = c,
                b'/' if self.b.get(i + 1) == Some(&b'/') => {
                    line_comment = true;
                    i += 2;
                    continue;
                }
                b'/' if self.b.get(i + 1) == Some(&b'*') => {
                    block_comment += 1;
                    i += 2;
                    continue;
                }
                _ => {
                    if c == open {
                        depth += 1;
                    } else if c == close {
                        depth -= 1;
                        if depth == 0 {
                            let content = &self.b[content_start..i];
                            return Ok((i + 1, is_block_comment(content)));
                        }
                    }
                }
            }
            i += 1;
        }
        Err(corrupt("unbalanced braces in an MDX expression"))
    }

    // -- JSX ------------------------------------------------------------------

    /// Try to scan a JSX element or fragment starting at `i` (a `<`). Returns the
    /// index just past the element, or `None` if `i` is not an element start.
    fn scan_jsx_at(&mut self, i: usize, end: usize, depth: u32) -> Result<Option<usize>> {
        if i + 1 >= end {
            return Ok(None);
        }
        if self.b[i + 1] == b'/' {
            return Ok(None);
        }
        if self.b[i + 1] == b'>' {
            // A fragment `<> … </>`.
            self.signals |= S_FRAGMENT;
            let slot = self.reserve_jsx(depth)?;
            let (next, children, _closed) = self.scan_jsx_body(i + 2, end, depth, None)?;
            self.finish_jsx(slot, J_FRAGMENT, 0, depth, i, next, None, 0, children);
            return Ok(Some(next));
        }
        let name_start = i + 1;
        if !is_tag_name_start(self.b.get(name_start).copied().unwrap_or(0)) {
            return Ok(None);
        }
        let mut j = name_start;
        while j < end && is_tag_name_continue(self.b[j]) {
            j += 1;
        }
        let raw_name = &self.b[name_start..j];
        let name_str = String::from_utf8_lossy(raw_name).into_owned();
        let component = raw_name.iter().any(|c| c.is_ascii_uppercase());
        let namespaced = raw_name.contains(&b'.') || raw_name.contains(&b':');

        // Parse the start tag (attributes) up to `>` or `/>`.
        let mut k = j;
        let mut attrs: u32 = 0;
        let mut jsx_attr = false;
        let mut has_spread = false;
        let (tag_end, self_closing) = loop {
            while k < end && is_ws(self.b[k]) {
                k += 1;
            }
            if k >= end {
                return Err(corrupt("unterminated JSX start tag"));
            }
            let c = self.b[k];
            if c == b'>' {
                break (k + 1, false);
            }
            if c == b'/' && self.b.get(k + 1) == Some(&b'>') {
                break (k + 2, true);
            }
            if c == b'{' {
                let (ne, _) = self.scan_balance(k, end, b'{', b'}')?;
                attrs = attrs.saturating_add(1);
                jsx_attr = true;
                has_spread = true;
                k = ne;
                continue;
            }
            if is_attr_name_start(c) {
                k += 1;
                while k < end && is_attr_name_continue(self.b[k]) {
                    k += 1;
                }
                attrs = attrs.saturating_add(1);
                let mut m = k;
                while m < end && is_ws(self.b[m]) {
                    m += 1;
                }
                if m < end && self.b[m] == b'=' {
                    m += 1;
                    while m < end && is_ws(self.b[m]) {
                        m += 1;
                    }
                    if m >= end {
                        return Err(corrupt("unterminated JSX attribute value"));
                    }
                    match self.b[m] {
                        b'"' | b'\'' => {
                            k = self.scan_quoted(m, end)?;
                        }
                        b'{' => {
                            let (ne, _) = self.scan_balance(m, end, b'{', b'}')?;
                            jsx_attr = true;
                            k = ne;
                        }
                        _ => {
                            let mut t = m;
                            while t < end
                                && !is_ws(self.b[t])
                                && self.b[t] != b'>'
                                && !(self.b[t] == b'/' && self.b.get(t + 1) == Some(&b'>'))
                            {
                                t += 1;
                            }
                            k = t;
                        }
                    }
                }
                continue;
            }
            // An unknown byte in the start tag: consume to the next `>` as a leaf.
            let mut t = k;
            while t < end && self.b[t] != b'>' {
                t += 1;
            }
            if t >= end {
                return Err(corrupt("unterminated JSX start tag"));
            }
            break (t + 1, false);
        };

        let mut flags = 0u8;
        if self_closing {
            flags |= F_SELF_CLOSING;
        }
        if component {
            flags |= F_COMPONENT;
        }
        if namespaced {
            flags |= F_NAMESPACED;
        }
        if attrs > 0 {
            flags |= F_HAS_ATTRS;
        }
        if jsx_attr {
            flags |= F_JSX_ATTR;
        }
        if has_spread {
            flags |= F_HAS_SPREAD;
        }
        if component {
            self.signals |= S_COMPONENT;
        }
        if jsx_attr {
            self.signals |= S_JSX_ATTR;
        }

        let slot = self.reserve_jsx(depth)?;
        let (next, children) = if self_closing {
            (tag_end, 0u32)
        } else {
            let (n, ch, closed) = self.scan_jsx_body(tag_end, end, depth, Some(&name_str))?;
            if closed {
                (n, ch)
            } else if component {
                return Err(corrupt(
                    "unterminated JSX component element (no matching close tag)",
                ));
            } else {
                // A lowercase, HTML-looking element with no close: a leaf.
                (tag_end, 0)
            }
        };
        self.finish_jsx(
            slot,
            J_ELEMENT,
            flags,
            depth,
            i,
            next,
            Some(name_str),
            attrs,
            children,
        );
        Ok(Some(next))
    }

    /// Scan an element's children until its matching close tag. Returns
    /// `(after_close_or_end, child_count, closed)`.
    fn scan_jsx_body(
        &mut self,
        i: usize,
        end: usize,
        depth: u32,
        close_name: Option<&str>,
    ) -> Result<(usize, u32, bool)> {
        if depth > self.limits.max_mdx_depth {
            return Err(Error::resource_limit(format!(
                "MDX JSX nesting depth {depth} exceeds the {}-deep cap",
                self.limits.max_mdx_depth
            )));
        }
        let mut pos = i;
        let mut children = 0u32;
        while pos < end {
            if let Some(pe) = self.protected_end(pos) {
                pos = pe;
                continue;
            }
            let c = self.b[pos];
            if c == b'<' && self.b.get(pos + 1) == Some(&b'/') {
                if let Some((cname, after)) = self.parse_close_tag(pos, end) {
                    let matches = match close_name {
                        None => cname.is_none(),
                        Some(n) => cname.as_deref() == Some(n),
                    };
                    if matches {
                        return Ok((after, children, true));
                    }
                    pos = after;
                    continue;
                }
                pos += 1;
                continue;
            }
            if c == b'<' {
                if let Some(next) = self.scan_jsx_at(pos, end, depth + 1)? {
                    children = children.saturating_add(1);
                    pos = next;
                    continue;
                }
                pos += 1;
                continue;
            }
            if c == b'{' {
                pos = self.scan_expression(pos, end, depth)?;
                continue;
            }
            pos += 1;
        }
        Ok((end, children, false))
    }

    /// Parse a close tag at `pos` (a `<`). Returns `(name, after)`, where `name` is
    /// `None` for a fragment close (`</>`).
    fn parse_close_tag(&self, pos: usize, end: usize) -> Option<(Option<String>, usize)> {
        let mut k = pos + 2;
        if k < end && self.b[k] == b'>' {
            return Some((None, k + 1));
        }
        if !is_tag_name_start(self.b.get(k).copied().unwrap_or(0)) {
            return None;
        }
        let name_start = k;
        while k < end && is_tag_name_continue(self.b[k]) {
            k += 1;
        }
        let name = String::from_utf8_lossy(&self.b[name_start..k]).into_owned();
        while k < end && is_ws(self.b[k]) {
            k += 1;
        }
        if k < end && self.b[k] == b'>' {
            Some((Some(name), k + 1))
        } else {
            None
        }
    }

    /// Scan a quoted string starting at `start` (a `"` or `'`), returning the index
    /// just past its closing quote.
    fn scan_quoted(&self, start: usize, end: usize) -> Result<usize> {
        let q = self.b[start];
        let mut i = start + 1;
        let mut esc = false;
        while i < end {
            let c = self.b[i];
            if esc {
                esc = false;
            } else if c == b'\\' {
                esc = true;
            } else if c == q {
                return Ok(i + 1);
            }
            i += 1;
        }
        Err(corrupt("unterminated JSX attribute string"))
    }

    // -- ESM ------------------------------------------------------------------

    /// Try to scan a top-level ESM statement at `i` (a line start). Returns the index
    /// just past the statement, or `None` if `i` is not an ESM statement.
    fn try_esm(&mut self, i: usize, end: usize) -> Result<Option<usize>> {
        let rest = &self.b[i..end];
        let kind = if starts_with_kw(rest, b"import") {
            E_IMPORT
        } else if starts_with_kw(rest, b"export") {
            E_EXPORT
        } else {
            return Ok(None);
        };
        let after = i + "import".len();
        if after >= end || !is_ws(self.b[after]) {
            return Ok(None);
        }
        let stmt_end = self.esm_end(i, end)?;
        let stmt = &self.b[i..stmt_end];
        let ok = match kind {
            E_IMPORT => has_quoted_specifier(stmt),
            _ => export_form_ok(stmt),
        };
        if !ok {
            return Ok(None);
        }
        self.push_node()?;
        if self.build {
            if self.esm.len() as u64 >= self.limits.max_mdx_esm_statements as u64 {
                return Err(Error::resource_limit(format!(
                    "MDX document exceeds the {}-ESM-statement cap",
                    self.limits.max_mdx_esm_statements
                )));
            }
            self.esm.push(MdxEsm {
                kind,
                start: i as u64,
                end: stmt_end as u64,
            });
        }
        self.signals |= S_ESM;
        Ok(Some(stmt_end))
    }

    /// The end of an ESM statement beginning at `start`: the end of the physical line
    /// on which bracket depth returns to zero and which does not end with a
    /// continuation token.
    fn esm_end(&self, start: usize, end: usize) -> Result<usize> {
        let mut i = start;
        let mut line_start = start;
        let mut depth = 0i32;
        let mut quote = 0u8;
        let mut esc = false;
        let mut line_comment = false;
        let mut block_comment = 0i32;
        while i < end {
            if (i - start) as u64 > self.limits.max_mdx_line_bytes {
                return Err(Error::resource_limit(format!(
                    "MDX ESM statement exceeds the {}-byte line cap",
                    self.limits.max_mdx_line_bytes
                )));
            }
            let c = self.b[i];
            if line_comment {
                if c == b'\n' {
                    line_comment = false;
                }
                i += 1;
                continue;
            }
            if block_comment > 0 {
                if c == b'*' && self.b.get(i + 1) == Some(&b'/') {
                    block_comment -= 1;
                    i += 2;
                    continue;
                }
                i += 1;
                continue;
            }
            if quote != 0 {
                if esc {
                    esc = false;
                } else if c == b'\\' {
                    esc = true;
                } else if c == quote {
                    quote = 0;
                }
                i += 1;
                continue;
            }
            if c == b'\n' {
                let line = trim_ascii(&self.b[line_start..i]);
                if depth <= 0 && !ends_with_continuation(line) {
                    return Ok(i);
                }
                line_start = i + 1;
                i += 1;
                continue;
            }
            match c {
                b'"' | b'\'' | b'`' => quote = c,
                b'/' if self.b.get(i + 1) == Some(&b'/') => {
                    line_comment = true;
                    i += 2;
                    continue;
                }
                b'/' if self.b.get(i + 1) == Some(&b'*') => {
                    block_comment += 1;
                    i += 2;
                    continue;
                }
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                _ => {}
            }
            i += 1;
        }
        Ok(end)
    }

    // -- arena pushes ---------------------------------------------------------

    fn push_node(&mut self) -> Result<()> {
        self.node_count = self.node_count.saturating_add(1);
        if self.node_count > self.limits.max_mdx_nodes as u64 {
            return Err(Error::resource_limit(format!(
                "MDX document exceeds the {}-node cap",
                self.limits.max_mdx_nodes
            )));
        }
        Ok(())
    }

    /// Reserve an arena slot for a JSX element (so a parent precedes its children in
    /// document order). Returns the slot index (unused when `!build`).
    fn reserve_jsx(&mut self, depth: u32) -> Result<usize> {
        if depth > self.limits.max_mdx_depth {
            return Err(Error::resource_limit(format!(
                "MDX JSX nesting depth {depth} exceeds the {}-deep cap",
                self.limits.max_mdx_depth
            )));
        }
        self.push_node()?;
        if self.build {
            if self.jsx.len() as u64 >= self.limits.max_mdx_jsx_blocks as u64 {
                return Err(Error::resource_limit(format!(
                    "MDX document exceeds the {}-JSX-block cap",
                    self.limits.max_mdx_jsx_blocks
                )));
            }
            self.jsx.push(MdxJsx {
                kind: 0,
                flags: 0,
                depth,
                start: 0,
                end: 0,
                name: None,
                attrs: 0,
                children: 0,
            });
            Ok(self.jsx.len() - 1)
        } else {
            Ok(usize::MAX)
        }
    }

    /// Fill in a reserved JSX slot (a no-op when `!build`).
    #[allow(clippy::too_many_arguments)]
    fn finish_jsx(
        &mut self,
        slot: usize,
        kind: u8,
        flags: u8,
        depth: u32,
        start: usize,
        end: usize,
        name: Option<String>,
        attrs: u32,
        children: u32,
    ) {
        if !self.build {
            return;
        }
        let j = &mut self.jsx[slot];
        j.kind = kind;
        j.flags = flags;
        j.depth = depth;
        j.start = start as u64;
        j.end = end as u64;
        j.name = name;
        j.attrs = attrs;
        j.children = children;
    }
}

// ---------------------------------------------------------------------------
// Lexical helpers
// ---------------------------------------------------------------------------

fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n')
}

fn is_tag_name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_tag_name_continue(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b':')
}

fn is_attr_name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b'$'
}

fn is_attr_name_continue(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'$' | b'-' | b'.' | b':')
}

fn is_block_comment(content: &[u8]) -> bool {
    let t = trim_ascii(content);
    t.len() >= 4 && t.starts_with(b"/*") && t.ends_with(b"*/")
}

fn trim_ascii(mut s: &[u8]) -> &[u8] {
    while let Some((f, rest)) = s.split_first() {
        if f.is_ascii_whitespace() {
            s = rest;
        } else {
            break;
        }
    }
    while let Some((l, rest)) = s.split_last() {
        if l.is_ascii_whitespace() {
            s = rest;
        } else {
            break;
        }
    }
    s
}

fn ends_with_continuation(line: &[u8]) -> bool {
    if line.is_empty() {
        return false;
    }
    let last = line[line.len() - 1];
    if matches!(
        last,
        b',' | b'(' | b'[' | b'{' | b'\\' | b'=' | b'+' | b'.' | b'*' | b'&' | b'|'
    ) {
        return true;
    }
    line.ends_with(b"from")
        || line.ends_with(b"as")
        || line.ends_with(b"=>")
        || line.ends_with(b"&&")
        || line.ends_with(b"||")
}

/// Whether `rest` starts with the ASCII keyword `kw` (not followed by an ident byte).
fn starts_with_kw(rest: &[u8], kw: &[u8]) -> bool {
    rest.len() >= kw.len()
        && rest[..kw.len()] == *kw
        && !rest
            .get(kw.len())
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
}

/// Whether a statement contains a quoted module specifier.
fn has_quoted_specifier(stmt: &[u8]) -> bool {
    stmt.contains(&b'"') || stmt.contains(&b'\'')
}

/// Whether an `export …` statement carries a recognized export form.
fn export_form_ok(stmt: &[u8]) -> bool {
    let mut i = "export".len();
    while i < stmt.len() && is_ws(stmt[i]) {
        i += 1;
    }
    let rest = &stmt[i..];
    for form in [
        b"default".as_slice(),
        b"const",
        b"let",
        b"var",
        b"function",
        b"class",
        b"async",
    ] {
        if starts_with_kw(rest, form) {
            return true;
        }
    }
    rest.starts_with(b"{") || rest.starts_with(b"*")
}

// ---------------------------------------------------------------------------
// Binary encoding helpers
// ---------------------------------------------------------------------------

fn corrupt(msg: &str) -> Error {
    Error::invalid_mdx_structure(format!("malformed MDX: {msg}"))
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

    fn model(src: &str) -> MdxModel {
        parse(src.as_bytes(), Limits::DEFAULT, true).unwrap()
    }

    #[test]
    fn detects_mdx_but_not_markdown_or_prose() {
        let l = Limits::DEFAULT;
        assert!(detect(b"import A from './a'\n\n# Title\n\n<A />\n", l));
        assert!(detect(b"# Title\n\n<Foo bar />\n", l));
        assert!(detect(b"<>a</>\n", l));
        assert!(detect(b"# T\n\n{frontmatter.title}\n", l));
        assert!(detect(b"# T\n\n<div className={x}>y</div>\n", l));
        // Plain Markdown stays Markdown; plain prose stays Opaque.
        assert!(!detect(b"# Title\n\nplain paragraph\n", l));
        assert!(!detect(b"just some prose\nwith more lines\n", l));
        // A `{x}` inline expression alone does not admit MDX.
        assert!(!detect(b"# T\n\nvalue is {x} here\n", l));
        // A brace-bearing JSON-shaped blob with no Markdown structural mark stays out.
        assert!(!detect(b"{\"a\":1}\n{oops}\n", l));
        // A JSX-looking tag inside inline code is not a signal.
        assert!(!detect(b"# T\n\nuse `<Foo />` here\n", l));
    }

    #[test]
    fn esm_jsx_and_expressions_are_exact() {
        let src = "import A, { B } from './a'\nexport default function App() {\n  return <A>{B}</A>\n}\n\n# T\n\n<A b={x} c=\"y\" {...rest}>\n  <B.Child />\n</A>\n\n{frontmatter.title}\n";
        let m = model(src);
        assert_eq!(m.esm.len(), 2);
        assert_eq!(m.esm[0].kind, E_IMPORT);
        assert_eq!(
            &src[m.esm[0].start as usize..m.esm[0].end as usize],
            "import A, { B } from './a'"
        );
        assert_eq!(m.esm[1].kind, E_EXPORT);
        // The JSX component element with children and a fragment-free nesting.
        let comp = m
            .jsx
            .iter()
            .find(|j| j.name.as_deref() == Some("A"))
            .unwrap();
        assert!(comp.flags & F_COMPONENT != 0);
        assert!(comp.flags & F_JSX_ATTR != 0);
        assert!(comp.flags & F_HAS_SPREAD != 0);
        assert!(comp.children >= 1);
        assert!(comp.depth >= 1);
        // A block expression is flagged.
        assert!(m.exprs.iter().any(|x| x.flags & X_BLOCK != 0));
        assert!(m.has_mdx_signal());
    }

    #[test]
    fn braces_inside_strings_do_not_close_the_expression() {
        let m = model("{ \"a\": \"}\", /* } */ b: 2 }");
        assert_eq!(m.exprs.len(), 1);
        assert_eq!(m.exprs[0].start, 0);
        assert_eq!(
            m.exprs[0].end as usize,
            "{ \"a\": \"}\", /* } */ b: 2 }".len()
        );
    }

    #[test]
    fn unbalanced_braces_decline_typed() {
        assert!(parse(b"# T\n\n{ unclosed\n", Limits::DEFAULT, true).is_err());
    }

    #[test]
    fn model_roundtrips() {
        let m = model("import A from './a'\n\n# T\n\n<A>{x}</A>\n");
        let bytes = m.encode();
        let back = MdxModel::decode(&bytes).unwrap();
        assert_eq!(m, back);
        let mut bad = bytes.clone();
        bad[0] = b'X';
        assert!(MdxModel::decode(&bad).is_err());
        let mut truncated = bytes;
        truncated.truncate(truncated.len() - 1);
        assert!(MdxModel::decode(&truncated).is_err());
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut x: u64 = 0x51ED_2703_9A5C_1F0B;
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
                let _ = m.encode();
                let _ = find(&buf, &m, "a", 1 << 20);
            }
        }
    }
}
