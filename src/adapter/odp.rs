//! OpenDocument Presentation (ODP) adapter (Phase 21.4.1).
//!
//! An `.odp` is an **OpenDocument (ODF) package**: a ZIP archive whose first member
//! is the mandatory `stored` `mimetype`
//! (`application/vnd.oasis.opendocument.presentation`), whose
//! `META-INF/manifest.xml` enumerates the package's files with their media types,
//! and whose main document part is the OpenDocument content stream (`content.xml`,
//! `office:document-content` → `office:body` → `office:presentation`).
//!
//! ODF is *not* OPC: it has no `[Content_Types].xml` and no `_rels/.rels`
//! `officeDocument` relationship. Like ODT/ODS/EPUB, ODP therefore reuses the
//! byte-authoritative ZIP layer (ADR-0030) and the shared bounded-XML policy, but
//! discovers its main part **semantically** from the ODF manifest's declared media
//! type — never from a hardcoded `content.xml`. It shares the OPC *helpers* but does
//! not route through the OPC graph.
//!
//! Everything here is **derived (`Q_gen`) state**: exact authority remains the
//! Phase-12.2 ZIP member raw spans, so a hostile or malformed ODP is still an exact
//! archival object — only the *derived* observation declines, typed, and
//! `materialize(descriptor) == original_bytes` is untouched.
//!
//! The **slide order is the `draw:page` document order**, never a file/member
//! name/order: every slide lives in the *same* `content.xml` part, so the only
//! faithful order is the order the pages appear in the presentation stream. Slide
//! `draw:name`, the slide's own text, a shape's text, an embedded table's cells, a
//! referenced media part, and the notes text are distinct facets, never conflated.
//!
//! ODF does **not** standardize a hidden-slide flag. This adapter reads a declared
//! `presentation:visibility` attribute on `draw:page` (value `hidden`/`false`) as
//! the hide signal and treats an absent attribute as visible; the interpretation is
//! stated here so it is auditable and never silently assumed. The declared profile
//! (`include_hidden`/`include_notes`) governs only whole-deck projections; hidden
//! slides and notes stay addressable by their own selectors.

use std::collections::BTreeMap;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::adapter::package::xml::{
    XmlState, accept_doctype, attr_of, harden_xml, read_attrs, xml_err,
};
use crate::adapter::package::zip::{ZipMember, ZipPhysical, scan};
use crate::error::{Error, Result};
use crate::limits::Limits;

/// The mandatory ODF presentation `mimetype` payload.
pub const ODP_MIMETYPE: &str = "application/vnd.oasis.opendocument.presentation";
/// The additional accepted OpenDocument *presentation* media type.
pub const ODP_MIMETYPE_TEMPLATE: &str = "application/vnd.oasis.opendocument.presentation-template";
/// The ODF package manifest member.
pub const MANIFEST_MEMBER: &str = "META-INF/manifest.xml";
/// The conventional main content part (advisory; the manifest is authority).
pub const CONTENT_MEMBER: &str = "content.xml";
/// The conventional styles part.
pub const STYLES_MEMBER: &str = "styles.xml";
/// The conventional metadata part.
pub const META_MEMBER: &str = "meta.xml";

/// Sentinel ordinal meaning "not a member of this package".
const NO_MEMBER: u32 = u32::MAX;

/// Version of the ODP extraction profile semantics.
pub const ODP_EXTRACT_PROFILE_VERSION: u32 = 1;

/// A hard cap on shape-tree recursion while decoding a canonical content model.
/// The model is our own canonical output, but decode is still bounded so a
/// truncated/corrupt model can never drive unbounded recursion.
const MAX_SHAPE_DECODE_DEPTH: u32 = 64;

/// Whether a media type is an OpenDocument *presentation* document type.
pub fn is_odp_presentation_media_type(ct: &str) -> bool {
    matches!(ct, ODP_MIMETYPE | ODP_MIMETYPE_TEMPLATE)
}

// ---------------------------------------------------------------------------
// Extraction profile
// ---------------------------------------------------------------------------

/// A versioned, explicit OpenDocument presentation-extraction profile. The profile
/// identity is recorded in every answer and hashed into the canonical selector, so
/// a projection is never hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OdpExtractProfile {
    /// Profile semantics version; must equal [`ODP_EXTRACT_PROFILE_VERSION`].
    pub version: u32,
    /// Include notes text in a whole-deck projection. Notes stay addressable by
    /// their own selector regardless.
    pub include_notes: bool,
    /// Include hidden (`presentation:visibility`) slides in a whole-deck
    /// projection. Hidden slides stay addressable by slide index regardless.
    pub include_hidden: bool,
}

impl OdpExtractProfile {
    /// The declared default: notes and hidden slides excluded from whole-deck
    /// projections (both are still addressable by their own selectors).
    pub const DEFAULT: OdpExtractProfile = OdpExtractProfile {
        version: ODP_EXTRACT_PROFILE_VERSION,
        include_notes: false,
        include_hidden: false,
    };

    /// A stable, human-readable profile fingerprint used in canonical selectors.
    pub fn fingerprint(&self) -> String {
        format!(
            "v{}-n{}-h{}",
            self.version, self.include_notes as u8, self.include_hidden as u8
        )
    }

    /// The canonical 3-byte profile block.
    pub fn encode(&self) -> [u8; 3] {
        [
            self.version as u8,
            self.include_notes as u8,
            self.include_hidden as u8,
        ]
    }

    /// Decode a profile block produced by [`Self::encode`].
    pub fn decode(b: &[u8]) -> Result<OdpExtractProfile> {
        if b.len() != 3 {
            return Err(corrupt("ODP profile must be 3 bytes"));
        }
        if b[0] as u32 != ODP_EXTRACT_PROFILE_VERSION {
            return Err(Error::unsupported_version(format!(
                "ODP extraction profile version {} is not supported",
                b[0]
            )));
        }
        Ok(OdpExtractProfile {
            version: b[0] as u32,
            include_notes: b[1] != 0,
            include_hidden: b[2] != 0,
        })
    }
}

impl Default for OdpExtractProfile {
    fn default() -> Self {
        OdpExtractProfile::DEFAULT
    }
}

// ---------------------------------------------------------------------------
// Discovery model
// ---------------------------------------------------------------------------

/// The independent ODF `mimetype` conformance facts. None of these gate exact
/// preservation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MimetypeFacts {
    /// A member named exactly `mimetype` exists.
    pub present: bool,
    /// It is the first member in the archive (ordinal 0).
    pub first: bool,
    /// It is `stored` (method 0), not DEFLATE-compressed.
    pub stored: bool,
    /// Its local file header carries no extra field.
    pub no_extra: bool,
    /// Its decoded payload is an OpenDocument presentation media type with no extra
    /// bytes.
    pub exact_bytes: bool,
    /// Its decoded payload, when decodable.
    pub media_type: Option<String>,
    /// Its physical ordinal ([`NO_MEMBER`] when absent).
    pub ordinal: u32,
    /// All of the storage conformance facts hold.
    pub conformant: bool,
}

/// One `META-INF/manifest.xml` file entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    /// `manifest:full-path` exactly as written.
    pub full_path: String,
    /// `manifest:media-type`.
    pub media_type: String,
    /// `manifest:version`, when present.
    pub version: Option<String>,
    /// The resolved member ordinal ([`NO_MEMBER`] when unresolved).
    pub ordinal: u32,
}

/// A reference to a package part discovered semantically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartRef {
    /// The absolute part name.
    pub name: String,
    /// The physical member ordinal.
    pub ordinal: u32,
    /// The declared media type, when known.
    pub media_type: Option<String>,
}

/// The canonical ODP discovery model (the derived state of the `OdpModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OdpModel {
    /// The `mimetype` conformance facts.
    pub mimetype: MimetypeFacts,
    /// The parsed ODF manifest file entries, in document order.
    pub manifest: Vec<ManifestEntry>,
    /// The main OpenDocument content part, discovered semantically.
    pub content: Option<PartRef>,
    /// The styles part, when present.
    pub styles: Option<PartRef>,
    /// The metadata part, when present.
    pub meta: Option<PartRef>,
    /// The media parts (`Pictures/*` image entries), name-sorted.
    pub media: Vec<PartRef>,
    /// Non-fatal structural observations.
    pub issues: Vec<String>,
}

impl OdpModel {
    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"ODPM");
        out.push(1);

        let m = &self.mimetype;
        for b in [
            m.present,
            m.first,
            m.stored,
            m.no_extra,
            m.exact_bytes,
            m.conformant,
        ] {
            out.push(b as u8);
        }
        put_opt_str(&mut out, m.media_type.as_deref());
        put_u32(&mut out, m.ordinal);

        put_u32(&mut out, self.manifest.len() as u32);
        for e in &self.manifest {
            put_str(&mut out, &e.full_path);
            put_str(&mut out, &e.media_type);
            put_opt_str(&mut out, e.version.as_deref());
            put_u32(&mut out, e.ordinal);
        }
        put_opt_part(&mut out, self.content.as_ref());
        put_opt_part(&mut out, self.styles.as_ref());
        put_opt_part(&mut out, self.meta.as_ref());
        put_parts(&mut out, &self.media);
        put_strs(&mut out, &self.issues);
        out
    }

    /// Decode a model produced by [`Self::encode`].
    pub fn decode(bytes: &[u8]) -> Result<OdpModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"ODPM" {
            return Err(corrupt("bad ODP model magic"));
        }
        if r.u8()? != 1 {
            return Err(corrupt("unsupported ODP model version"));
        }
        let mimetype = MimetypeFacts {
            present: r.u8()? != 0,
            first: r.u8()? != 0,
            stored: r.u8()? != 0,
            no_extra: r.u8()? != 0,
            exact_bytes: r.u8()? != 0,
            conformant: r.u8()? != 0,
            media_type: r.opt_string()?,
            ordinal: r.u32()?,
        };
        let n = bounded(&mut r, "manifest entry")?;
        let mut manifest = Vec::with_capacity(n as usize);
        for _ in 0..n {
            manifest.push(ManifestEntry {
                full_path: r.string()?,
                media_type: r.string()?,
                version: r.opt_string()?,
                ordinal: r.u32()?,
            });
        }
        let content = read_opt_part(&mut r)?;
        let styles = read_opt_part(&mut r)?;
        let meta = read_opt_part(&mut r)?;
        let media = read_parts(&mut r)?;
        let issues = read_strs(&mut r)?;
        if !r.at_end() {
            return Err(corrupt("ODP model has trailing bytes"));
        }
        Ok(OdpModel {
            mimetype,
            manifest,
            content,
            styles,
            meta,
            media,
            issues,
        })
    }
}

fn put_part(out: &mut Vec<u8>, p: &PartRef) {
    put_str(out, &p.name);
    put_u32(out, p.ordinal);
    put_opt_str(out, p.media_type.as_deref());
}

fn put_opt_part(out: &mut Vec<u8>, p: Option<&PartRef>) {
    match p {
        None => out.push(0),
        Some(p) => {
            out.push(1);
            put_part(out, p);
        }
    }
}

fn put_parts(out: &mut Vec<u8>, ps: &[PartRef]) {
    put_u32(out, ps.len() as u32);
    for p in ps {
        put_part(out, p);
    }
}

fn read_part(r: &mut BinReader<'_>) -> Result<PartRef> {
    Ok(PartRef {
        name: r.string()?,
        ordinal: r.u32()?,
        media_type: r.opt_string()?,
    })
}

fn read_opt_part(r: &mut BinReader<'_>) -> Result<Option<PartRef>> {
    Ok(match r.u8()? {
        0 => None,
        1 => Some(read_part(r)?),
        _ => return Err(corrupt("bad ODP part tag")),
    })
}

fn read_parts(r: &mut BinReader<'_>) -> Result<Vec<PartRef>> {
    let n = bounded(r, "part")?;
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        out.push(read_part(r)?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Content model
// ---------------------------------------------------------------------------

/// The kind of a presentation shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeKind {
    /// A frame carrying a `draw:text-box` (text).
    Text,
    /// A frame carrying a `draw:image` (picture).
    Picture,
    /// A `draw:custom-shape`.
    CustomShape,
    /// A `draw:g` group.
    Group,
    /// A frame carrying an embedded table (`table:table`).
    Table,
    /// Any other frame / unrecognized shape.
    Other,
}

impl ShapeKind {
    /// Stable lower-case name.
    pub const fn name(self) -> &'static str {
        match self {
            ShapeKind::Text => "text",
            ShapeKind::Picture => "picture",
            ShapeKind::CustomShape => "custom-shape",
            ShapeKind::Group => "group",
            ShapeKind::Table => "table",
            ShapeKind::Other => "other",
        }
    }

    fn tag(self) -> u8 {
        match self {
            ShapeKind::Text => 0,
            ShapeKind::Picture => 1,
            ShapeKind::CustomShape => 2,
            ShapeKind::Group => 3,
            ShapeKind::Table => 4,
            ShapeKind::Other => 5,
        }
    }

    fn from_tag(b: u8) -> Result<ShapeKind> {
        Ok(match b {
            0 => ShapeKind::Text,
            1 => ShapeKind::Picture,
            2 => ShapeKind::CustomShape,
            3 => ShapeKind::Group,
            4 => ShapeKind::Table,
            5 => ShapeKind::Other,
            _ => return Err(corrupt("bad shape-kind tag")),
        })
    }
}

/// One cell of an embedded table (`table:table-cell` / `table:covered-table-cell`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OdpTableCell {
    /// The cell text (paragraphs joined by `\n`).
    pub text: String,
    /// `table:number-columns-spanned` (>= 1).
    pub col_span: u32,
    /// `table:number-rows-spanned` (>= 1).
    pub row_span: u32,
    /// The cell is a `table:covered-table-cell` (a span continuation).
    pub covered: bool,
}

/// One row of an embedded table (`table:table-row`), after repeat expansion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OdpTableRow {
    /// Cells, in physical order.
    pub cells: Vec<OdpTableCell>,
}

/// An embedded table (`table:table`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OdpTable {
    /// Rows, in document order.
    pub rows: Vec<OdpTableRow>,
}

impl OdpTable {
    /// Total number of cells.
    pub fn cell_count(&self) -> u64 {
        self.rows.iter().map(|r| r.cells.len() as u64).sum()
    }

    /// The table rendered as TSV: rows joined by `\n`, cells joined by `\t`.
    pub fn text(&self) -> String {
        self.rows
            .iter()
            .map(|r| {
                r.cells
                    .iter()
                    .map(|c| c.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// One shape in a slide's shape tree. `index` is the shape's position within its
/// container (the slide's shape list, or a group's nested list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OdpShape {
    /// Position within the containing shape list.
    pub index: u32,
    /// The shape kind.
    pub kind: ShapeKind,
    /// The `draw:name`, when present.
    pub name: Option<String>,
    /// The placeholder object type (`presentation:object`), when present.
    pub placeholder: Option<String>,
    /// The shape's text (paragraphs joined by `\n`).
    pub text: String,
    /// The `xlink:href` of the referenced media part, for pictures.
    pub media_href: Option<String>,
    /// The embedded table, for table frames.
    pub table: Option<OdpTable>,
    /// Nested shapes, for group shapes.
    pub children: Vec<OdpShape>,
}

impl OdpShape {
    /// The shape's own searchable text: its run text plus, for a table frame, the
    /// embedded table's text. **Not** its children's text.
    pub fn own_text(&self) -> String {
        match &self.table {
            Some(t) if !self.text.is_empty() => format!("{}\n{}", self.text, t.text()),
            Some(t) => t.text(),
            None => self.text.clone(),
        }
    }

    /// The shape's text joined with its descendants' text (pre-order).
    pub fn text_deep(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        self.push_text_deep(&mut parts);
        parts.join("\n")
    }

    fn push_text_deep(&self, out: &mut Vec<String>) {
        let own = self.own_text();
        if !own.is_empty() {
            out.push(own);
        }
        for c in &self.children {
            c.push_text_deep(out);
        }
    }

    fn collect_tables(&self, out: &mut Vec<OdpTable>) {
        if let Some(t) = &self.table {
            out.push(t.clone());
        }
        for c in &self.children {
            c.collect_tables(out);
        }
    }
}

/// One `draw:page`, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slide {
    /// 0-based document-order index among the pages.
    pub index: u32,
    /// `draw:name` (never locale-normalized).
    pub name: String,
    /// The declared hidden flag (`presentation:visibility`).
    pub hidden: bool,
    /// `draw:master-page-name`, when present.
    pub master_page: Option<String>,
    /// Top-level shapes, in document order.
    pub shapes: Vec<OdpShape>,
    /// All embedded tables, flattened in pre-order across shapes (incl. groups).
    pub tables: Vec<OdpTable>,
    /// The notes text (`presentation:notes`), when the page carries a notes page.
    pub notes: Option<String>,
    /// The number of shapes parsed inside the notes page.
    pub notes_shapes: u32,
}

impl Slide {
    /// Total number of shapes (including nested group children).
    pub fn shape_count(&self) -> u64 {
        fn walk(s: &OdpShape) -> u64 {
            1 + s.children.iter().map(walk).sum::<u64>()
        }
        self.shapes.iter().map(walk).sum()
    }

    /// The number of top-level shapes.
    pub fn top_level_count(&self) -> usize {
        self.shapes.len()
    }

    /// A shape by its flattened pre-order index (groups counted as one node each,
    /// in pre-order before their children). `None` when out of range.
    pub fn shape_by_flat_index(&self, index: u32) -> Option<&OdpShape> {
        fn walk<'a>(s: &'a OdpShape, want: u32, seen: &mut u32) -> Option<&'a OdpShape> {
            if *seen == want {
                return Some(s);
            }
            *seen += 1;
            for c in &s.children {
                if let Some(f) = walk(c, want, seen) {
                    return Some(f);
                }
            }
            None
        }
        let mut seen = 0u32;
        self.shapes.iter().find_map(|s| walk(s, index, &mut seen))
    }

    /// The slide's whole text: every shape's (and descendant's) text, in pre-order,
    /// non-empty lines joined by `\n`.
    pub fn text(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        for s in &self.shapes {
            s.push_text_deep(&mut parts);
        }
        parts.join("\n")
    }

    /// The title text: the first title/centered-title placeholder with text, else
    /// the first non-empty shape text.
    pub fn title(&self) -> Option<String> {
        let mut first: Option<String> = None;
        for s in &self.shapes {
            if let Some(t) = title_of(s) {
                return Some(t);
            }
            if first.is_none() {
                let t = s.text_deep();
                if !t.is_empty() {
                    first = Some(t);
                }
            }
        }
        first
    }
}

fn title_of(s: &OdpShape) -> Option<String> {
    if matches!(s.placeholder.as_deref(), Some("title") | Some("ctrTitle")) {
        let t = s.text_deep();
        if !t.is_empty() {
            return Some(t);
        }
    }
    for c in &s.children {
        if let Some(t) = title_of(c) {
            return Some(t);
        }
    }
    None
}

/// One `style:style` record (a named or automatic style), kept distinct from the
/// content it styles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Style {
    /// `style:name`.
    pub name: String,
    /// `style:family`.
    pub family: String,
    /// `style:parent-style-name`, when present.
    pub parent: Option<String>,
}

/// One `style:master-page` (a master page), from the styles part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MasterPage {
    /// `style:name`.
    pub name: String,
    /// `style:page-layout-name`, when present.
    pub page_layout: Option<String>,
}

/// One `draw:image` reference to a media part (`xlink:href`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRef {
    /// The `xlink:href` as written (resolved to a member path is a discovery fact).
    pub href: String,
}

/// The canonical, derived content model of the OpenDocument presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentModel {
    /// The absolute part name the model was parsed from.
    pub part_name: String,
    /// The verified root element local name.
    pub root_local: String,
    /// Slides (`draw:page`), in document order.
    pub slides: Vec<Slide>,
    /// `style:style` records found in the content part.
    pub styles: Vec<Style>,
    /// `style:master-page` records found in the content part (usually empty).
    pub master_pages: Vec<MasterPage>,
    /// `draw:image` references found in the content part, in document order.
    pub images: Vec<ImageRef>,
    /// Non-fatal parse observations.
    pub issues: Vec<String>,
    /// Element nodes scanned while building this model.
    pub nodes: u64,
}

impl ContentModel {
    /// The slide at a 0-based index, if present.
    pub fn slide(&self, index: u32) -> Option<&Slide> {
        self.slides.get(index as usize)
    }

    /// Total shapes across all slides (including nested group children).
    pub fn shape_count(&self) -> u64 {
        self.slides.iter().map(|s| s.shape_count()).sum()
    }

    /// Total embedded tables across all slides.
    pub fn table_count(&self) -> u64 {
        self.slides.iter().map(|s| s.tables.len() as u64).sum()
    }

    /// The whole-deck text: projected slides joined by `\n`. Hidden slides are
    /// skipped unless `include_hidden`; notes are appended per slide when
    /// `include_notes`.
    pub fn text(&self, include_notes: bool, include_hidden: bool) -> String {
        let mut out = String::new();
        for s in &self.slides {
            if s.hidden && !include_hidden {
                continue;
            }
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&s.text());
            if include_notes
                && let Some(n) = &s.notes
                && !n.is_empty()
            {
                out.push('\n');
                out.push_str(n);
            }
        }
        out
    }

    /// Deterministically encode the model.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"ODPC");
        out.push(1);
        put_str(&mut out, &self.part_name);
        put_str(&mut out, &self.root_local);

        put_u32(&mut out, self.slides.len() as u32);
        for s in &self.slides {
            put_u32(&mut out, s.index);
            put_str(&mut out, &s.name);
            out.push(s.hidden as u8);
            put_opt_str(&mut out, s.master_page.as_deref());
            put_opt_str(&mut out, s.notes.as_deref());
            put_u32(&mut out, s.notes_shapes);
            put_u32(&mut out, s.shapes.len() as u32);
            for sh in &s.shapes {
                put_shape(&mut out, sh);
            }
            put_u32(&mut out, s.tables.len() as u32);
            for t in &s.tables {
                put_table(&mut out, t);
            }
        }

        put_u32(&mut out, self.styles.len() as u32);
        for st in &self.styles {
            put_str(&mut out, &st.name);
            put_str(&mut out, &st.family);
            put_opt_str(&mut out, st.parent.as_deref());
        }

        put_u32(&mut out, self.master_pages.len() as u32);
        for m in &self.master_pages {
            put_str(&mut out, &m.name);
            put_opt_str(&mut out, m.page_layout.as_deref());
        }

        put_u32(&mut out, self.images.len() as u32);
        for im in &self.images {
            put_str(&mut out, &im.href);
        }

        put_strs(&mut out, &self.issues);
        out.extend_from_slice(&self.nodes.to_le_bytes());
        out
    }

    /// Decode a model produced by [`Self::encode`].
    pub fn decode(bytes: &[u8]) -> Result<ContentModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"ODPC" {
            return Err(corrupt("bad ODP content model magic"));
        }
        if r.u8()? != 1 {
            return Err(corrupt("unsupported ODP content model version"));
        }
        let part_name = r.string()?;
        let root_local = r.string()?;

        let ns = bounded(&mut r, "slide")?;
        let mut slides = Vec::with_capacity(ns as usize);
        for _ in 0..ns {
            let index = r.u32()?;
            let name = r.string()?;
            let hidden = r.u8()? != 0;
            let master_page = r.opt_string()?;
            let notes = r.opt_string()?;
            let notes_shapes = r.u32()?;
            let nsh = bounded(&mut r, "slide shape")?;
            let mut shapes = Vec::with_capacity(nsh as usize);
            for _ in 0..nsh {
                shapes.push(read_shape(&mut r, 0)?);
            }
            let nt = bounded(&mut r, "slide table")?;
            let mut tables = Vec::with_capacity(nt as usize);
            for _ in 0..nt {
                tables.push(read_table(&mut r)?);
            }
            slides.push(Slide {
                index,
                name,
                hidden,
                master_page,
                shapes,
                tables,
                notes,
                notes_shapes,
            });
        }

        let nst = bounded(&mut r, "style")?;
        let mut styles = Vec::with_capacity(nst as usize);
        for _ in 0..nst {
            styles.push(Style {
                name: r.string()?,
                family: r.string()?,
                parent: r.opt_string()?,
            });
        }

        let nm = bounded(&mut r, "master page")?;
        let mut master_pages = Vec::with_capacity(nm as usize);
        for _ in 0..nm {
            master_pages.push(MasterPage {
                name: r.string()?,
                page_layout: r.opt_string()?,
            });
        }

        let ni = bounded(&mut r, "image")?;
        let mut images = Vec::with_capacity(ni as usize);
        for _ in 0..ni {
            images.push(ImageRef { href: r.string()? });
        }

        let issues = read_strs(&mut r)?;
        let nodes = u64::from_le_bytes(
            r.bytes(8)?
                .try_into()
                .map_err(|_| corrupt("content node counter"))?,
        );
        if !r.at_end() {
            return Err(corrupt("ODP content model has trailing bytes"));
        }
        Ok(ContentModel {
            part_name,
            root_local,
            slides,
            styles,
            master_pages,
            images,
            issues,
            nodes,
        })
    }
}

/// The canonical, derived model of one OpenDocument styles part (`styles.xml`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StylesModel {
    /// The absolute part name the model was parsed from.
    pub part_name: String,
    /// The verified root element local name.
    pub root_local: String,
    /// Named/automatic `style:style` records.
    pub styles: Vec<Style>,
    /// `style:master-page` records.
    pub master_pages: Vec<MasterPage>,
    /// Non-fatal parse observations.
    pub issues: Vec<String>,
    /// Element nodes scanned while building this model.
    pub nodes: u64,
}

impl StylesModel {
    /// Deterministically encode the model.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"ODPS");
        out.push(1);
        put_str(&mut out, &self.part_name);
        put_str(&mut out, &self.root_local);
        put_u32(&mut out, self.styles.len() as u32);
        for st in &self.styles {
            put_str(&mut out, &st.name);
            put_str(&mut out, &st.family);
            put_opt_str(&mut out, st.parent.as_deref());
        }
        put_u32(&mut out, self.master_pages.len() as u32);
        for m in &self.master_pages {
            put_str(&mut out, &m.name);
            put_opt_str(&mut out, m.page_layout.as_deref());
        }
        put_strs(&mut out, &self.issues);
        out.extend_from_slice(&self.nodes.to_le_bytes());
        out
    }

    /// Decode a model produced by [`Self::encode`].
    pub fn decode(bytes: &[u8]) -> Result<StylesModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"ODPS" {
            return Err(corrupt("bad ODP styles model magic"));
        }
        if r.u8()? != 1 {
            return Err(corrupt("unsupported ODP styles model version"));
        }
        let part_name = r.string()?;
        let root_local = r.string()?;
        let nst = bounded(&mut r, "style")?;
        let mut styles = Vec::with_capacity(nst as usize);
        for _ in 0..nst {
            styles.push(Style {
                name: r.string()?,
                family: r.string()?,
                parent: r.opt_string()?,
            });
        }
        let nm = bounded(&mut r, "master page")?;
        let mut master_pages = Vec::with_capacity(nm as usize);
        for _ in 0..nm {
            master_pages.push(MasterPage {
                name: r.string()?,
                page_layout: r.opt_string()?,
            });
        }
        let issues = read_strs(&mut r)?;
        let nodes = u64::from_le_bytes(
            r.bytes(8)?
                .try_into()
                .map_err(|_| corrupt("styles node counter"))?,
        );
        if !r.at_end() {
            return Err(corrupt("ODP styles model has trailing bytes"));
        }
        Ok(StylesModel {
            part_name,
            root_local,
            styles,
            master_pages,
            issues,
            nodes,
        })
    }
}

// ---------------------------------------------------------------------------
// Node parameters
// ---------------------------------------------------------------------------

/// Canonical parameters for an [`crate::field::node::NodeKind::OdpContent`] node:
/// `version(1) · member ordinal(4) · profile(3) · len-prefixed part name`.
pub fn content_params(ordinal: u32, part_name: &str, profile: &OdpExtractProfile) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + part_name.len());
    out.push(1);
    out.extend_from_slice(&ordinal.to_le_bytes());
    out.extend_from_slice(&profile.encode());
    put_str(&mut out, part_name);
    out
}

/// Decode parameters produced by [`content_params`].
pub fn read_content_params(params: &[u8]) -> Result<(u32, String, OdpExtractProfile)> {
    let mut r = BinReader::new(params);
    if r.u8()? != 1 {
        return Err(corrupt("unsupported ODP content params version"));
    }
    let ordinal = r.u32()?;
    let profile = OdpExtractProfile::decode(r.bytes(3)?)?;
    let part_name = r.string()?;
    if !r.at_end() {
        return Err(corrupt("ODP content params have trailing bytes"));
    }
    Ok((ordinal, part_name, profile))
}

/// Canonical parameters for an [`crate::field::node::NodeKind::OdpStyles`] node:
/// `version(1) · member ordinal(4) · len-prefixed part name`.
pub fn styles_params(ordinal: u32, part_name: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(9 + part_name.len());
    out.push(1);
    out.extend_from_slice(&ordinal.to_le_bytes());
    put_str(&mut out, part_name);
    out
}

/// Decode parameters produced by [`styles_params`].
pub fn read_styles_params(params: &[u8]) -> Result<(u32, String)> {
    let mut r = BinReader::new(params);
    if r.u8()? != 1 {
        return Err(corrupt("unsupported ODP styles params version"));
    }
    let ordinal = r.u32()?;
    let part_name = r.string()?;
    if !r.at_end() {
        return Err(corrupt("ODP styles params have trailing bytes"));
    }
    Ok((ordinal, part_name))
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Build the canonical ODP discovery model from an exact ODF/ZIP source (the
/// derived [`crate::field::node::NodeKind::OdpModel`] computation).
///
/// Fails closed with a typed error when the package has no decodable ODF manifest
/// (an `.odp` always has one). The exact bytes remain recoverable regardless.
pub fn build_odp_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    let physical = scan(source, limits)?;
    physical.validate(source.len() as u64)?;
    let model = discover(source, &physical, limits)?;
    Ok(model.encode())
}

fn discover(source: &[u8], physical: &ZipPhysical, limits: Limits) -> Result<OdpModel> {
    let mut issues: Vec<String> = Vec::new();

    let mut lookup: BTreeMap<String, u32> = BTreeMap::new();
    for member in &physical.members {
        match core::str::from_utf8(&member.name) {
            Ok(name) => {
                if lookup.insert(name.to_string(), member.id.ordinal).is_some() {
                    issues.push(format!("duplicate package member name {name:?}"));
                }
            }
            Err(_) => issues
                .push("package has a non-UTF-8 member name (preserved, uninterpreted)".to_string()),
        }
    }

    let mimetype = mimetype_facts(source, physical, limits);

    // The ODF manifest is mandatory for an ODP package.
    let manifest_member = physical
        .members
        .iter()
        .find(|m| m.name.as_slice() == MANIFEST_MEMBER.as_bytes())
        .ok_or_else(|| {
            Error::invalid_package_structure("ODF package has no META-INF/manifest.xml manifest")
        })?;
    let manifest_bytes = decode_member(source, manifest_member, limits).ok_or_else(|| {
        Error::invalid_package_structure(
            "META-INF/manifest.xml could not be decoded (encrypted or oversized)",
        )
    })?;
    let manifest = parse_manifest(&manifest_bytes, limits, &lookup, &mut issues)?;

    // Semantic main-part discovery: the manifest's `content.xml` entry (the ODF
    // main content stream), else a non-root entry declaring an OpenDocument
    // *presentation* media type. The extension is never the authority.
    let content = find_part(&manifest, &lookup, |e| e.full_path == CONTENT_MEMBER).or_else(|| {
        find_part(&manifest, &lookup, |e| {
            e.full_path != "/" && is_odp_presentation_media_type(&e.media_type)
        })
    });
    if content.is_none() {
        issues.push(
            "ODF manifest declares no OpenDocument presentation content part and no content.xml"
                .to_string(),
        );
    }
    let styles = find_part(&manifest, &lookup, |e| e.full_path == STYLES_MEMBER);
    let meta = find_part(&manifest, &lookup, |e| e.full_path == META_MEMBER);

    // Media parts (`Pictures/*`): manifest entries declaring an `image/*` media
    // type, or living under `Pictures/`. Name-sorted; the extension is never the
    // authority for the *content part*, but the media list is a declared inventory.
    let mut media: Vec<PartRef> = Vec::new();
    for e in manifest.iter().filter(|e| {
        e.full_path != "/"
            && (e.media_type.starts_with("image/") || e.full_path.starts_with("Pictures/"))
    }) {
        if media.len() as u64 >= u64::from(limits.max_odp_media) {
            return Err(Error::resource_limit(
                "package declares more media parts than max_odp_media",
            ));
        }
        if !media.iter().any(|m| m.name == e.full_path) {
            media.push(PartRef {
                name: e.full_path.clone(),
                ordinal: e.ordinal,
                media_type: Some(e.media_type.clone()).filter(|m| !m.is_empty()),
            });
        }
    }
    media.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });

    if mimetype.present && !mimetype.exact_bytes && !mimetype.conformant {
        issues.push(
            "mimetype member is present but not a conformant OpenDocument presentation declaration"
                .to_string(),
        );
    }

    Ok(OdpModel {
        mimetype,
        manifest,
        content,
        styles,
        meta,
        media,
        issues,
    })
}

fn find_part<F: Fn(&ManifestEntry) -> bool>(
    manifest: &[ManifestEntry],
    lookup: &BTreeMap<String, u32>,
    pred: F,
) -> Option<PartRef> {
    manifest.iter().find(|e| pred(e)).map(|e| PartRef {
        name: e.full_path.clone(),
        ordinal: lookup.get(&e.full_path).copied().unwrap_or(NO_MEMBER),
        media_type: Some(e.media_type.clone()).filter(|m| !m.is_empty()),
    })
}

fn mimetype_facts(source: &[u8], physical: &ZipPhysical, limits: Limits) -> MimetypeFacts {
    let member = physical
        .members
        .iter()
        .find(|m| m.name.as_slice() == b"mimetype");
    let Some(m) = member else {
        return MimetypeFacts {
            present: false,
            first: false,
            stored: false,
            no_extra: false,
            exact_bytes: false,
            media_type: None,
            ordinal: NO_MEMBER,
            conformant: false,
        };
    };
    let first = physical
        .members
        .first()
        .is_some_and(|x| x.name.as_slice() == b"mimetype");
    let stored = m.method == 0;
    let local_extra = m.local_header.1.saturating_sub(30 + m.name.len() as u64);
    let no_extra = local_extra == 0;
    let decoded = decode_member(source, m, limits);
    let media_type = decoded
        .as_deref()
        .and_then(|b| core::str::from_utf8(b).ok())
        .map(str::to_string);
    let exact_bytes = media_type
        .as_deref()
        .is_some_and(is_odp_presentation_media_type);
    MimetypeFacts {
        present: true,
        first,
        stored,
        no_extra,
        exact_bytes,
        media_type,
        ordinal: m.id.ordinal,
        conformant: first && stored && no_extra && exact_bytes,
    }
}

/// Decode one member (stored or raw-DEFLATE, unencrypted, within `max_xml_part_bytes`).
fn decode_member(source: &[u8], member: &ZipMember, limits: Limits) -> Option<Vec<u8>> {
    const FLAG_ENCRYPTED: u16 = 0x0001;
    if member.flags & FLAG_ENCRYPTED != 0 {
        return None;
    }
    if member.uncompressed_size > limits.max_xml_part_bytes {
        return None;
    }
    let off = usize::try_from(member.data.0).ok()?;
    let len = usize::try_from(member.data.1).ok()?;
    let raw = source.get(off..off.checked_add(len)?)?;
    match member.method {
        0 => Some(raw.to_vec()),
        8 => crate::field::derive::inflate_raw_deflate(raw, member.uncompressed_size, limits).ok(),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// META-INF/manifest.xml
// ---------------------------------------------------------------------------

fn parse_manifest(
    xml: &[u8],
    limits: Limits,
    lookup: &BTreeMap<String, u32>,
    issues: &mut Vec<String>,
) -> Result<Vec<ManifestEntry>> {
    harden_xml(xml, limits)?;
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut st = XmlState::new();
    let mut out: Vec<ManifestEntry> = Vec::new();
    let mut saw_root = false;
    loop {
        let ev = reader.read_event().map_err(xml_err)?;
        st.event(limits)?;
        match ev {
            Event::Eof => break,
            Event::DocType(d) => accept_doctype(&d.into_inner())?,
            Event::Start(e) => {
                st.open(limits)?;
                manifest_element(&e, limits, lookup, issues, &mut saw_root, &mut out)?;
            }
            Event::Empty(e) => {
                st.leaf(limits)?;
                manifest_element(&e, limits, lookup, issues, &mut saw_root, &mut out)?;
            }
            Event::End(_) => st.close(),
            Event::Text(t) => st.text(t.len(), limits)?,
            _ => {}
        }
    }
    if !saw_root {
        return Err(Error::invalid_xml_structure(
            "META-INF/manifest.xml is empty",
        ));
    }
    if out.len() as u64 > u64::from(limits.max_odt_manifest_entries) {
        return Err(Error::resource_limit(
            "manifest exceeds max_odt_manifest_entries",
        ));
    }
    Ok(out)
}

/// Handle one `<manifest>` / `<file-entry>` element in `META-INF/manifest.xml`.
fn manifest_element(
    e: &BytesStart<'_>,
    limits: Limits,
    lookup: &BTreeMap<String, u32>,
    issues: &mut Vec<String>,
    saw_root: &mut bool,
    out: &mut Vec<ManifestEntry>,
) -> Result<()> {
    let local = e.name().local_name().as_ref().to_string();
    if !*saw_root {
        if local != "manifest" {
            return Err(Error::invalid_package_structure(
                "META-INF/manifest.xml root element is not <manifest>",
            ));
        }
        *saw_root = true;
    } else if local == "file-entry" {
        push_file_entry(e, limits, lookup, issues, out)?;
    }
    Ok(())
}

fn push_file_entry(
    e: &BytesStart<'_>,
    limits: Limits,
    lookup: &BTreeMap<String, u32>,
    issues: &mut Vec<String>,
    out: &mut Vec<ManifestEntry>,
) -> Result<()> {
    let attrs = read_attrs(e, limits)?;
    let full = attr_of(&attrs, "full-path")
        .ok_or_else(|| Error::invalid_package_structure("manifest file-entry lacks full-path"))?;
    if full.is_empty() {
        return Err(Error::invalid_package_structure(
            "manifest file-entry full-path is empty",
        ));
    }
    if full.len() as u64 > u64::from(limits.max_opc_part_name_bytes) {
        return Err(Error::resource_limit(
            "manifest full-path exceeds max_opc_part_name_bytes",
        ));
    }
    let media_type = attr_of(&attrs, "media-type").unwrap_or("").to_string();
    let version = attr_of(&attrs, "version").map(str::to_string);
    let ordinal = lookup.get(full).copied().unwrap_or(NO_MEMBER);
    if ordinal == NO_MEMBER && full != "/" {
        issues.push(format!(
            "manifest file-entry {full:?} does not resolve to a package member"
        ));
    }
    out.push(ManifestEntry {
        full_path: full.to_string(),
        media_type,
        version,
        ordinal,
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// content.xml / styles.xml parsing
// ---------------------------------------------------------------------------

/// Parse the OpenDocument presentation main part into a canonical [`ContentModel`],
/// honoring `profile`. Only `content.xml` is ever parsed here; exact member bytes
/// stay authoritative.
pub fn parse_content(
    bytes: &[u8],
    part_name: &str,
    _profile: &OdpExtractProfile,
    limits: Limits,
) -> Result<ContentModel> {
    let p = parse_document(bytes, part_name, limits, Mode::Content)?;
    Ok(p.content)
}

/// Parse an OpenDocument styles part (`styles.xml`) into a canonical
/// [`StylesModel`].
pub fn parse_styles(bytes: &[u8], part_name: &str, limits: Limits) -> Result<StylesModel> {
    let p = parse_document(bytes, part_name, limits, Mode::Styles)?;
    Ok(StylesModel {
        part_name: p.content.part_name,
        root_local: p.content.root_local,
        styles: p.content.styles,
        master_pages: p.content.master_pages,
        issues: p.content.issues,
        nodes: p.content.nodes,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Content,
    Styles,
}

struct Parsed {
    content: ContentModel,
}

fn parse_document(bytes: &[u8], part_name: &str, limits: Limits, mode: Mode) -> Result<Parsed> {
    harden_xml(bytes, limits)?;
    let mut p = DocParser::new(mode);
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut saw_root = false;
    loop {
        let ev = reader.read_event().map_err(xml_err)?;
        p.nodes = p.nodes.saturating_add(1);
        if p.nodes > limits.max_xml_nodes {
            return Err(Error::resource_limit("ODP content exceeds max_xml_nodes"));
        }
        match ev {
            Event::Eof => break,
            Event::DocType(d) => accept_doctype(&d.into_inner())?,
            Event::Start(e) => {
                if !saw_root {
                    p.check_root(&e)?;
                    saw_root = true;
                }
                p.on_open(&e, limits, false)?;
            }
            Event::Empty(e) => {
                if !saw_root {
                    p.check_root(&e)?;
                    saw_root = true;
                }
                p.on_open(&e, limits, true)?;
            }
            Event::End(e) => {
                let local = e.name().local_name().as_ref().to_string();
                p.on_close(local.as_bytes(), limits)?;
            }
            Event::Text(t) => {
                p.on_text(t.into_inner().as_ref(), limits)?;
            }
            Event::GeneralRef(r) => {
                p.on_text(&entity_ref_text(r), limits)?;
            }
            _ => {}
        }
    }
    if !saw_root {
        return Err(Error::invalid_xml_structure("ODP content.xml is empty"));
    }
    if !p.stack_closed() {
        return Err(Error::invalid_xml_structure(
            "ODP content.xml is not well-formed (unclosed elements)",
        ));
    }
    Ok(Parsed {
        content: p.finish(part_name),
    })
}

/// Decode a general entity/character reference without any DTD or external lookup.
fn entity_ref_text(r: quick_xml::events::BytesRef<'_>) -> String {
    if r.is_char_ref() {
        match r.resolve_char_ref() {
            Ok(Some(c)) => c.to_string(),
            _ => String::new(),
        }
    } else {
        match r.into_inner().as_ref() {
            "amp" => "&".to_string(),
            "lt" => "<".to_string(),
            "gt" => ">".to_string(),
            "quot" => "\"".to_string(),
            "apos" => "'".to_string(),
            _ => String::new(),
        }
    }
}

enum Cont {
    Body,
    Presentation,
    Page(PageBuilder),
    Notes(NotesBuilder),
    Shape(ShapeB),
    Table(TableB),
    Row(RowB),
    Cell(CellB),
    Style(StyleBuilder),
    Master(MasterBuilder),
    Other,
}

struct PageBuilder {
    name: String,
    hidden: bool,
    master_page: Option<String>,
    shapes: Vec<OdpShape>,
    notes: Option<String>,
    notes_shapes: u32,
}

struct NotesBuilder {
    text: String,
    shapes: u32,
}

struct ShapeB {
    kind: ShapeKind,
    name: Option<String>,
    placeholder: Option<String>,
    text: String,
    media_href: Option<String>,
    table: Option<TableB>,
    children: Vec<OdpShape>,
}

impl ShapeB {
    fn new(kind: ShapeKind) -> Self {
        ShapeB {
            kind,
            name: None,
            placeholder: None,
            text: String::new(),
            media_href: None,
            table: None,
            children: Vec::new(),
        }
    }

    fn into_shape(self, index: u32) -> OdpShape {
        let mut kind = self.kind;
        if self.table.is_some() {
            kind = ShapeKind::Table;
        } else if self.kind == ShapeKind::Other && self.media_href.is_some() {
            kind = ShapeKind::Picture;
        }
        OdpShape {
            index,
            kind,
            name: self.name,
            placeholder: self.placeholder,
            text: self.text,
            media_href: self.media_href,
            table: self.table.map(TableB::finish),
            children: self.children,
        }
    }
}

struct CellB {
    repeat: u32,
    col_span: u32,
    row_span: u32,
    covered: bool,
    paragraphs: Vec<String>,
    raw: String,
}

struct RowB {
    repeat: u32,
    cells: Vec<OdpTableCell>,
}

struct TableB {
    rows: Vec<OdpTableRow>,
}

impl TableB {
    fn finish(self) -> OdpTable {
        OdpTable { rows: self.rows }
    }
}

struct StyleBuilder {
    name: String,
    family: String,
    parent: Option<String>,
}

struct MasterBuilder {
    name: String,
    page_layout: Option<String>,
}

struct DocParser {
    mode: Mode,
    stack: Vec<Cont>,
    slides: Vec<Slide>,
    styles: Vec<Style>,
    master_pages: Vec<MasterPage>,
    images: Vec<ImageRef>,
    issues: Vec<String>,
    root_local: String,
    nodes: u64,
    page_shapes: u64,
    page_text_runs: u64,
    table_cells: u64,
    text_bytes: u64,
}

impl DocParser {
    fn new(mode: Mode) -> Self {
        DocParser {
            mode,
            stack: vec![Cont::Body],
            slides: Vec::new(),
            styles: Vec::new(),
            master_pages: Vec::new(),
            images: Vec::new(),
            issues: Vec::new(),
            root_local: String::new(),
            nodes: 0,
            page_shapes: 0,
            page_text_runs: 0,
            table_cells: 0,
            text_bytes: 0,
        }
    }

    fn stack_closed(&self) -> bool {
        self.stack.len() == 1
    }

    fn check_root(&mut self, e: &BytesStart<'_>) -> Result<()> {
        let local = e.name().local_name().as_ref().to_string();
        let ok = match self.mode {
            Mode::Content => local == "document-content" || local == "document",
            Mode::Styles => local == "document-styles" || local == "document",
        };
        if !ok {
            let expect = match self.mode {
                Mode::Content => "office:document-content",
                Mode::Styles => "office:document-styles",
            };
            return Err(Error::invalid_package_structure(format!(
                "ODP content root element <{local}> is not <{expect}>"
            )));
        }
        self.root_local = local;
        Ok(())
    }

    fn in_presentation(&self) -> bool {
        self.stack.iter().any(|c| matches!(c, Cont::Presentation))
    }

    fn in_page(&self) -> bool {
        self.stack.iter().any(|c| matches!(c, Cont::Page(_)))
    }

    fn in_notes(&self) -> bool {
        self.stack.iter().any(|c| matches!(c, Cont::Notes(_)))
    }

    fn repeated_count(attrs: &[(String, String)], name: &str, limits: Limits) -> Result<u32> {
        let raw = attr_of(attrs, name);
        let n = raw.and_then(|v| v.parse::<u32>().ok()).unwrap_or(1).max(1);
        // A single declaration is bounded by `max_odp_table_cells`: the declared
        // expansion can never exceed the total cell bound, so a hostile repeat
        // declines typed before any allocation.
        if n > limits.max_odp_table_cells {
            return Err(Error::resource_limit(format!(
                "{name} exceeds max_odp_table_cells"
            )));
        }
        Ok(n)
    }

    fn on_text(&mut self, s: &str, limits: Limits) -> Result<()> {
        if s.is_empty() || s.chars().all(char::is_whitespace) {
            return Ok(());
        }
        self.text_bytes = self.text_bytes.saturating_add(s.len() as u64);
        if self.text_bytes > limits.max_xml_text_bytes {
            return Err(Error::resource_limit("ODP text exceeds max_xml_text_bytes"));
        }
        self.push_text(s);
        Ok(())
    }

    /// Append text to the nearest text-bearing container (Notes > Cell > Shape).
    fn push_text(&mut self, s: &str) {
        for c in self.stack.iter_mut().rev() {
            match c {
                Cont::Notes(n) => {
                    n.text.push_str(s);
                    return;
                }
                Cont::Cell(cell) => {
                    if let Some(p) = cell.paragraphs.last_mut() {
                        p.push_str(s);
                    } else {
                        cell.raw.push_str(s);
                    }
                    return;
                }
                Cont::Shape(sh) => {
                    sh.text.push_str(s);
                    return;
                }
                _ => {}
            }
        }
    }

    /// Start a new text paragraph in the nearest text-bearing container.
    fn new_paragraph(&mut self) {
        for c in self.stack.iter_mut().rev() {
            match c {
                Cont::Notes(n) => {
                    if !n.text.is_empty() {
                        n.text.push('\n');
                    }
                    return;
                }
                Cont::Cell(cell) => {
                    let prev = cell
                        .paragraphs
                        .last()
                        .map(String::as_str)
                        .unwrap_or(&cell.raw);
                    if !prev.is_empty() {
                        cell.paragraphs.push(String::new());
                    }
                    return;
                }
                Cont::Shape(sh) => {
                    if !sh.text.is_empty() {
                        sh.text.push('\n');
                    }
                    return;
                }
                _ => {}
            }
        }
    }

    fn nearest_shape_mut(&mut self) -> Option<&mut ShapeB> {
        self.stack.iter_mut().rev().find_map(|c| match c {
            Cont::Shape(s) => Some(s),
            _ => None,
        })
    }

    fn push_shape(&mut self, kind: ShapeKind, limits: Limits) -> Result<()> {
        self.page_shapes = self.page_shapes.saturating_add(1);
        if self.page_shapes > u64::from(limits.max_odp_shapes_per_slide) {
            return Err(Error::resource_limit(
                "slide has more shapes than max_odp_shapes_per_slide",
            ));
        }
        if kind == ShapeKind::Group {
            let groups = self
                .stack
                .iter()
                .filter(|c| matches!(c, Cont::Shape(s) if s.kind == ShapeKind::Group))
                .count() as u64;
            if groups + 1 > u64::from(limits.max_odp_group_depth) {
                return Err(Error::resource_limit(
                    "slide group nesting exceeds max_odp_group_depth",
                ));
            }
        }
        self.stack.push(Cont::Shape(ShapeB::new(kind)));
        Ok(())
    }

    fn on_open(&mut self, e: &BytesStart<'_>, limits: Limits, empty: bool) -> Result<()> {
        let local = e.name().local_name().as_ref().to_string();
        match local.as_str() {
            // -- styles (both modes) ------------------------------------------
            "style" => {
                let attrs = read_attrs(e, limits)?;
                if self.styles.len() as u64 >= u64::from(limits.max_odp_masters) {
                    return Err(Error::resource_limit("styles exceed max_odp_masters"));
                }
                self.stack.push(Cont::Style(StyleBuilder {
                    name: attr_of(&attrs, "name").unwrap_or("").to_string(),
                    family: attr_of(&attrs, "family").unwrap_or("").to_string(),
                    parent: attr_of(&attrs, "parent-style-name").map(str::to_string),
                }));
            }
            "master-page" => {
                let attrs = read_attrs(e, limits)?;
                if self.master_pages.len() as u64 >= u64::from(limits.max_odp_masters) {
                    return Err(Error::resource_limit("master pages exceed max_odp_masters"));
                }
                self.stack.push(Cont::Master(MasterBuilder {
                    name: attr_of(&attrs, "name").unwrap_or("").to_string(),
                    page_layout: attr_of(&attrs, "page-layout-name").map(str::to_string),
                }));
            }
            // -- presentation structure (content mode) ------------------------
            "presentation" if self.mode == Mode::Content => self.stack.push(Cont::Presentation),
            "page" if self.mode == Mode::Content && self.in_presentation() => {
                if self.slides.len() as u64 >= u64::from(limits.max_odp_slides) {
                    return Err(Error::resource_limit("presentation exceeds max_odp_slides"));
                }
                let attrs = read_attrs(e, limits)?;
                self.page_shapes = 0;
                self.page_text_runs = 0;
                self.stack.push(Cont::Page(PageBuilder {
                    name: attr_of(&attrs, "name").unwrap_or("").to_string(),
                    hidden: is_hidden_visibility(attr_of(&attrs, "visibility")),
                    master_page: attr_of(&attrs, "master-page-name").map(str::to_string),
                    shapes: Vec::new(),
                    notes: None,
                    notes_shapes: 0,
                }));
            }
            "notes" if self.mode == Mode::Content && self.in_page() && !self.in_notes() => {
                self.stack.push(Cont::Notes(NotesBuilder {
                    text: String::new(),
                    shapes: 0,
                }));
            }
            "frame" | "custom-shape" | "g"
                if self.mode == Mode::Content && self.in_page() && !self.in_notes() =>
            {
                let kind = match local.as_str() {
                    "g" => ShapeKind::Group,
                    "custom-shape" => ShapeKind::CustomShape,
                    _ => ShapeKind::Other,
                };
                let attrs = read_attrs(e, limits)?;
                self.push_shape(kind, limits)?;
                if let Some(sh) = self.nearest_shape_mut() {
                    sh.name = attr_of(&attrs, "name").map(str::to_string);
                }
            }
            // A shape inside a notes page: counted, but its text routes to the
            // notes text (the notes page is a plain text projection here).
            "frame" | "custom-shape" | "g" if self.mode == Mode::Content && self.in_notes() => {
                if let Some(Cont::Notes(n)) = self
                    .stack
                    .iter_mut()
                    .rev()
                    .find(|c| matches!(c, Cont::Notes(_)))
                {
                    n.shapes = n.shapes.saturating_add(1);
                }
                self.stack.push(Cont::Other);
            }
            "text-box" if self.mode == Mode::Content && !self.in_notes() => {
                if let Some(sh) = self.nearest_shape_mut()
                    && sh.kind == ShapeKind::Other
                {
                    sh.kind = ShapeKind::Text;
                }
                self.stack.push(Cont::Other);
            }
            "image" if self.mode == Mode::Content && !self.in_notes() => {
                let attrs = read_attrs(e, limits)?;
                let href = attr_of(&attrs, "href").unwrap_or("").to_string();
                if !href.is_empty() {
                    self.images.push(ImageRef { href: href.clone() });
                }
                if let Some(sh) = self.nearest_shape_mut() {
                    if sh.kind == ShapeKind::Other {
                        sh.kind = ShapeKind::Picture;
                    }
                    sh.media_href = Some(href);
                }
                self.stack.push(Cont::Other);
            }
            "placeholder" if self.mode == Mode::Content && !self.in_notes() => {
                let attrs = read_attrs(e, limits)?;
                if let Some(sh) = self.nearest_shape_mut() {
                    sh.placeholder = attr_of(&attrs, "object")
                        .or_else(|| attr_of(&attrs, "placeholder"))
                        .map(str::to_string);
                }
                self.stack.push(Cont::Other);
            }
            // Embedded table (ODF's `table:table`, inside a frame).
            "table" if self.mode == Mode::Content && self.in_page() && !self.in_notes() => {
                let tables = self.count_open_tables();
                if tables >= u64::from(limits.max_odp_tables) {
                    return Err(Error::resource_limit(
                        "slide has more tables than max_odp_tables",
                    ));
                }
                self.stack.push(Cont::Table(TableB { rows: Vec::new() }));
            }
            "table-row" => {
                if let Some(Cont::Table(_)) = self.stack.last() {
                    let attrs = read_attrs(e, limits)?;
                    let repeat = Self::repeated_count(&attrs, "number-rows-repeated", limits)?;
                    self.stack.push(Cont::Row(RowB {
                        repeat,
                        cells: Vec::new(),
                    }));
                } else {
                    self.stack.push(Cont::Other);
                }
            }
            "table-cell" | "covered-table-cell" => {
                if let Some(Cont::Row(_)) = self.stack.last() {
                    let attrs = read_attrs(e, limits)?;
                    let repeat = Self::repeated_count(&attrs, "number-columns-repeated", limits)?;
                    let col_span = attr_of(&attrs, "number-columns-spanned")
                        .and_then(|v| v.parse::<u32>().ok())
                        .unwrap_or(1)
                        .max(1);
                    let row_span = attr_of(&attrs, "number-rows-spanned")
                        .and_then(|v| v.parse::<u32>().ok())
                        .unwrap_or(1)
                        .max(1);
                    self.stack.push(Cont::Cell(CellB {
                        repeat,
                        col_span,
                        row_span,
                        covered: local == "covered-table-cell",
                        paragraphs: Vec::new(),
                        raw: String::new(),
                    }));
                } else {
                    self.stack.push(Cont::Other);
                }
            }
            // -- text (content mode) ------------------------------------------
            "p" | "h" if self.mode == Mode::Content && self.in_page() => {
                self.new_paragraph();
                self.stack.push(Cont::Other);
            }
            "span" if self.mode == Mode::Content && self.in_page() => {
                self.page_text_runs = self.page_text_runs.saturating_add(1);
                if self.page_text_runs > u64::from(limits.max_odp_text_runs) {
                    return Err(Error::resource_limit(
                        "slide has more text runs than max_odp_text_runs",
                    ));
                }
                self.stack.push(Cont::Other);
            }
            "s" if self.mode == Mode::Content && self.in_page() => {
                let attrs = read_attrs(e, limits)?;
                let c = attr_of(&attrs, "c")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(1)
                    .min(1 << 16);
                let spaces: String = core::iter::repeat_n(' ', c as usize).collect();
                self.push_text(&spaces);
                self.stack.push(Cont::Other);
            }
            "tab" if self.mode == Mode::Content && self.in_page() => {
                self.push_text("\t");
                self.stack.push(Cont::Other);
            }
            "line-break" if self.mode == Mode::Content && self.in_page() => {
                self.push_text("\n");
                self.stack.push(Cont::Other);
            }
            _ => self.stack.push(Cont::Other),
        }
        if empty {
            self.close_element(local.as_bytes(), limits)?;
        }
        Ok(())
    }

    /// The number of `table:table` containers currently open on the stack.
    fn count_open_tables(&self) -> u64 {
        self.stack
            .iter()
            .filter(|c| matches!(c, Cont::Table(_)))
            .count() as u64
    }

    fn on_close(&mut self, local: &[u8], limits: Limits) -> Result<()> {
        self.close_element(local, limits)
    }

    fn close_element(&mut self, local: &[u8], limits: Limits) -> Result<()> {
        match local {
            b"page" => {
                if let Some(Cont::Page(pb)) = self.stack.pop() {
                    let index = self.slides.len() as u32;
                    let mut tables: Vec<OdpTable> = Vec::new();
                    for s in &pb.shapes {
                        s.collect_tables(&mut tables);
                    }
                    self.slides.push(Slide {
                        index,
                        name: pb.name,
                        hidden: pb.hidden,
                        master_page: pb.master_page,
                        shapes: pb.shapes,
                        tables,
                        notes: pb.notes,
                        notes_shapes: pb.notes_shapes,
                    });
                }
            }
            b"notes" => {
                if let Some(Cont::Notes(nb)) = self.stack.pop()
                    && let Some(Cont::Page(pb)) = self
                        .stack
                        .iter_mut()
                        .rev()
                        .find(|c| matches!(c, Cont::Page(_)))
                {
                    pb.notes = Some(nb.text);
                    pb.notes_shapes = nb.shapes;
                }
            }
            b"frame" | b"custom-shape" | b"g" => {
                if let Some(Cont::Shape(sb)) = self.stack.pop() {
                    let index = self
                        .stack
                        .iter()
                        .rev()
                        .find_map(|c| match c {
                            Cont::Shape(p) => Some(p.children.len()),
                            Cont::Page(p) => Some(p.shapes.len()),
                            _ => None,
                        })
                        .unwrap_or(0) as u32;
                    let shape = sb.into_shape(index);
                    if let Some(Cont::Shape(parent)) = self
                        .stack
                        .iter_mut()
                        .rev()
                        .find(|c| matches!(c, Cont::Shape(_)))
                    {
                        parent.children.push(shape);
                    } else if let Some(Cont::Page(pb)) = self
                        .stack
                        .iter_mut()
                        .rev()
                        .find(|c| matches!(c, Cont::Page(_)))
                    {
                        pb.shapes.push(shape);
                    }
                }
            }
            b"table" => {
                if let Some(Cont::Table(tb)) = self.stack.pop()
                    && let Some(sh) = self.nearest_shape_mut()
                {
                    sh.table = Some(tb);
                }
            }
            b"table-row" => {
                if let Some(Cont::Row(rb)) = self.stack.pop() {
                    let extra =
                        (rb.repeat as u64 - 1).saturating_mul((rb.cells.len() as u64).max(1));
                    let new_cells = self.table_cells.saturating_add(extra);
                    if new_cells > u64::from(limits.max_odp_table_cells) {
                        return Err(Error::resource_limit(
                            "table exceeds max_odp_table_cells after repeated-row expansion",
                        ));
                    }
                    self.table_cells = new_cells;
                    if let Some(Cont::Table(tb)) = self
                        .stack
                        .iter_mut()
                        .rev()
                        .find(|c| matches!(c, Cont::Table(_)))
                    {
                        for _ in 0..rb.repeat {
                            tb.rows.push(OdpTableRow {
                                cells: rb.cells.clone(),
                            });
                        }
                    }
                }
            }
            b"table-cell" | b"covered-table-cell" => {
                if let Some(Cont::Cell(cb)) = self.stack.pop() {
                    let new_cells = self.table_cells.saturating_add(cb.repeat as u64);
                    if new_cells > u64::from(limits.max_odp_table_cells) {
                        return Err(Error::resource_limit(
                            "table exceeds max_odp_table_cells after repeated-cell expansion",
                        ));
                    }
                    self.table_cells = new_cells;
                    let text = if cb.paragraphs.is_empty() {
                        cb.raw.clone()
                    } else {
                        cb.paragraphs.join("\n")
                    };
                    if let Some(Cont::Row(rb)) = self
                        .stack
                        .iter_mut()
                        .rev()
                        .find(|c| matches!(c, Cont::Row(_)))
                    {
                        for _ in 0..cb.repeat {
                            rb.cells.push(OdpTableCell {
                                text: text.clone(),
                                col_span: cb.col_span,
                                row_span: cb.row_span,
                                covered: cb.covered,
                            });
                        }
                    }
                }
            }
            b"style" => {
                if let Some(Cont::Style(sb)) = self.stack.pop() {
                    self.styles.push(Style {
                        name: sb.name,
                        family: sb.family,
                        parent: sb.parent,
                    });
                }
            }
            b"master-page" => {
                if let Some(Cont::Master(mb)) = self.stack.pop() {
                    self.master_pages.push(MasterPage {
                        name: mb.name,
                        page_layout: mb.page_layout,
                    });
                }
            }
            _ => {
                self.stack.pop();
            }
        }
        Ok(())
    }

    fn finish(mut self, part_name: &str) -> ContentModel {
        let issues = core::mem::take(&mut self.issues);
        ContentModel {
            part_name: part_name.to_string(),
            root_local: if self.root_local.is_empty() {
                match self.mode {
                    Mode::Content => "document-content".to_string(),
                    Mode::Styles => "document-styles".to_string(),
                }
            } else {
                self.root_local
            },
            slides: self.slides,
            styles: self.styles,
            master_pages: self.master_pages,
            images: self.images,
            issues,
            nodes: self.nodes,
        }
    }
}

/// Whether a declared `presentation:visibility` value marks a slide hidden. ODF
/// does not standardize slide hidden state; this adapter reads the declared
/// attribute (`hidden`/`false`) and treats an absent value as visible.
fn is_hidden_visibility(v: Option<&str>) -> bool {
    matches!(v, Some("hidden") | Some("false") | Some("0"))
}

// ---------------------------------------------------------------------------
// Codec helpers
// ---------------------------------------------------------------------------

fn corrupt(msg: &str) -> Error {
    Error::invalid_package_structure(format!("corrupt ODP model: {msg}"))
}

fn bounded(r: &mut BinReader<'_>, what: &str) -> Result<u32> {
    let n = r.u32()?;
    if n as u64 > 1 << 24 {
        return Err(corrupt(&format!("ODP {what} count is implausible")));
    }
    Ok(n)
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    put_u32(out, s.len() as u32);
    out.extend_from_slice(s.as_bytes());
}

fn put_opt_str(out: &mut Vec<u8>, s: Option<&str>) {
    match s {
        Some(s) => {
            out.push(1);
            put_str(out, s);
        }
        None => out.push(0),
    }
}

fn put_strs(out: &mut Vec<u8>, items: &[String]) {
    put_u32(out, items.len() as u32);
    for s in items {
        put_str(out, s);
    }
}

fn put_table(out: &mut Vec<u8>, t: &OdpTable) {
    put_u32(out, t.rows.len() as u32);
    for row in &t.rows {
        put_u32(out, row.cells.len() as u32);
        for c in &row.cells {
            put_str(out, &c.text);
            put_u32(out, c.col_span);
            put_u32(out, c.row_span);
            out.push(c.covered as u8);
        }
    }
}

fn read_table(r: &mut BinReader<'_>) -> Result<OdpTable> {
    let nr = bounded(r, "table row")?;
    let mut rows = Vec::with_capacity(nr as usize);
    for _ in 0..nr {
        let nc = bounded(r, "table cell")?;
        let mut cells = Vec::with_capacity(nc as usize);
        for _ in 0..nc {
            cells.push(OdpTableCell {
                text: r.string()?,
                col_span: r.u32()?,
                row_span: r.u32()?,
                covered: r.u8()? != 0,
            });
        }
        rows.push(OdpTableRow { cells });
    }
    Ok(OdpTable { rows })
}

fn put_shape(out: &mut Vec<u8>, s: &OdpShape) {
    put_u32(out, s.index);
    out.push(s.kind.tag());
    put_opt_str(out, s.name.as_deref());
    put_opt_str(out, s.placeholder.as_deref());
    put_str(out, &s.text);
    put_opt_str(out, s.media_href.as_deref());
    match &s.table {
        Some(t) => {
            out.push(1);
            put_table(out, t);
        }
        None => out.push(0),
    }
    put_u32(out, s.children.len() as u32);
    for c in &s.children {
        put_shape(out, c);
    }
}

fn read_shape(r: &mut BinReader<'_>, depth: u32) -> Result<OdpShape> {
    if depth > MAX_SHAPE_DECODE_DEPTH {
        return Err(corrupt("shape nesting exceeds the decode bound"));
    }
    let index = r.u32()?;
    let kind = ShapeKind::from_tag(r.u8()?)?;
    let name = r.opt_string()?;
    let placeholder = r.opt_string()?;
    let text = r.string()?;
    let media_href = r.opt_string()?;
    let table = match r.u8()? {
        0 => None,
        1 => Some(read_table(r)?),
        _ => return Err(corrupt("bad shape table tag")),
    };
    let n = bounded(r, "shape child")?;
    let mut children = Vec::with_capacity(n as usize);
    for _ in 0..n {
        children.push(read_shape(r, depth + 1)?);
    }
    Ok(OdpShape {
        index,
        kind,
        name,
        placeholder,
        text,
        media_href,
        table,
        children,
    })
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
            .ok_or_else(|| corrupt("length overflow"))?;
        let s = self
            .b
            .get(self.at..end)
            .ok_or_else(|| corrupt("truncated"))?;
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

    fn string(&mut self) -> Result<String> {
        let n = self.u32()? as usize;
        let bytes = self.bytes(n)?;
        core::str::from_utf8(bytes)
            .map(str::to_string)
            .map_err(|_| corrupt("string is not UTF-8"))
    }

    fn opt_string(&mut self) -> Result<Option<String>> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.string()?)),
            _ => Err(corrupt("bad optional-string tag")),
        }
    }

    fn at_end(&self) -> bool {
        self.at == self.b.len()
    }
}

fn read_strs(r: &mut BinReader<'_>) -> Result<Vec<String>> {
    let n = bounded(r, "string list")?;
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        out.push(r.string()?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_roundtrips_and_fingerprints() {
        let p = OdpExtractProfile::DEFAULT;
        assert_eq!(OdpExtractProfile::decode(&p.encode()).unwrap(), p);
        assert!(p.fingerprint().contains("n0"));
        assert!(p.fingerprint().contains("h0"));
        let mut q = p;
        q.include_notes = true;
        q.include_hidden = true;
        assert_ne!(q.encode(), p.encode());
        assert_eq!(OdpExtractProfile::decode(&q.encode()).unwrap(), q);
    }

    #[test]
    fn params_roundtrip() {
        let p = OdpExtractProfile::DEFAULT;
        let b = content_params(7, "content.xml", &p);
        assert_eq!(
            read_content_params(&b).unwrap(),
            (7, "content.xml".to_string(), p)
        );
        let b = styles_params(3, "styles.xml");
        assert_eq!(
            read_styles_params(&b).unwrap(),
            (3, "styles.xml".to_string())
        );
    }

    #[test]
    fn content_parses_pages_shapes_and_notes() {
        let xml = br#"<?xml version="1.0"?>
<office:document-content xmlns:office="o" xmlns:draw="d" xmlns:text="t" xmlns:presentation="p" xmlns:xlink="x">
<office:body><office:presentation>
<draw:page draw:name="SlideA" draw:master-page-name="Default">
  <draw:frame draw:name="Title"><draw:text-box><text:p>Hello <text:span>World</text:span></text:p></draw:text-box></draw:frame>
  <draw:frame draw:name="Pic"><draw:image xlink:href="Pictures/image1.png"/></draw:frame>
  <draw:page-notes/>
  <presentation:notes><draw:frame><draw:text-box><text:p>note text</text:p></draw:text-box></draw:frame></presentation:notes>
</draw:page>
<draw:page draw:name="SlideB" presentation:visibility="hidden">
  <draw:frame><draw:text-box><text:p>Second</text:p></draw:text-box></draw:frame>
</draw:page>
</office:presentation></office:body></office:document-content>"#;
        let m = parse_content(
            xml,
            "content.xml",
            &OdpExtractProfile::DEFAULT,
            Limits::DEFAULT,
        )
        .unwrap();
        assert_eq!(m.slides.len(), 2);
        assert_eq!(m.slides[0].name, "SlideA");
        assert_eq!(m.slides[0].master_page.as_deref(), Some("Default"));
        assert_eq!(m.slides[0].text(), "Hello World");
        assert_eq!(m.slides[0].shapes[1].kind, ShapeKind::Picture);
        assert_eq!(
            m.slides[0].shapes[1].media_href.as_deref(),
            Some("Pictures/image1.png")
        );
        assert_eq!(m.slides[0].notes.as_deref(), Some("note text"));
        assert!(m.slides[1].hidden);
        assert_eq!(m.text(false, false), "Hello World");
        assert_eq!(m.text(false, true), "Hello World\nSecond");
        assert_eq!(ContentModel::decode(&m.encode()).unwrap(), m);
    }

    #[test]
    fn group_shapes_are_nested_and_flattened() {
        let xml = br#"<office:document-content xmlns:office="o" xmlns:draw="d" xmlns:text="t"><office:body><office:presentation>
<draw:page draw:name="S"><draw:g draw:name="G"><draw:frame><draw:text-box><text:p>inner</text:p></draw:text-box></draw:frame></draw:g></draw:page>
</office:presentation></office:body></office:document-content>"#;
        let m = parse_content(
            xml,
            "content.xml",
            &OdpExtractProfile::DEFAULT,
            Limits::DEFAULT,
        )
        .unwrap();
        assert_eq!(m.slides[0].shapes.len(), 1);
        assert_eq!(m.slides[0].shapes[0].kind, ShapeKind::Group);
        assert_eq!(m.slides[0].shapes[0].children.len(), 1);
        assert_eq!(m.slides[0].shapes[0].text_deep(), "inner");
        assert_eq!(m.slides[0].shape_count(), 2);
        assert_eq!(ContentModel::decode(&m.encode()).unwrap(), m);
    }

    #[test]
    fn wrong_root_is_declined() {
        assert!(
            parse_content(
                b"<html></html>",
                "content.xml",
                &OdpExtractProfile::DEFAULT,
                Limits::DEFAULT
            )
            .is_err()
        );
    }
}
