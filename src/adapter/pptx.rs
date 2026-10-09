//! PresentationML (PPTX) adapter (Phase 21.2.1, the format programme's second
//! subphase — the analogue of 21.1.1 for XLSX).
//!
//! A `.pptx` is an **OPC package** on the ZIP layer (Phase 12.2/12.3): the
//! presentation is identified **semantically** by the package `officeDocument`
//! relationship and its PresentationML content type — never by a hardcoded
//! `/ppt/presentation.xml`. This module adds the PresentationML semantics on top
//! of the generic OPC core:
//!
//! * **presentation discovery** (relationship → content type → later root element),
//! * a **declared versioned extraction profile** (whether notes slides are folded
//!   into a whole-deck projection),
//! * **slide discovery** by the presentation relationship (`r:id` → slide part),
//!   with the **slide order** taken from `p:sldIdLst` (never from `slideN.xml`
//!   file names), and
//! * a bounded PresentationML subset parsed into canonical, derived
//!   ([`SlideModel`]/[`PresentationModel`]) serializations.
//!
//! Everything here is **derived (`Q_gen`) state**: exact authority stays with the
//! Phase-12.2 ZIP member raw spans, and the exact original package still
//! materializes byte-identically regardless of any decline. Text runs (`a:t`) are
//! the atomic text and paragraphs are `<a:p>`; a run's text and the slide's XML
//! span are distinct observations and are never conflated.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::adapter::package::opc::{
    OFFICE_DOCUMENT_REL, OFFICE_DOCUMENT_REL_STRICT, OpcModel, OpcPart,
};
use crate::adapter::package::xml::{
    XmlState, accept_doctype, attr_of, harden_xml, read_attrs, read_attrs_qualified, xml_err,
};
use crate::error::{Error, Result};
use crate::limits::Limits;

/// Version of the PPTX extraction profile semantics.
pub const PPTX_EXTRACT_PROFILE_VERSION: u32 = 1;

/// A hard cap on shape-tree recursion while decoding a canonical slide model.
/// The model is our own canonical output, but decode is still bounded so a
/// truncated/corrupt model can never drive unbounded recursion.
const MAX_SHAPE_DECODE_DEPTH: u32 = 64;

/// The PresentationML presentation main content types (transitional and macro
/// forms).
fn is_presentation_content_type(ct: &str) -> bool {
    matches!(
        ct,
        "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"
            | "application/vnd.openxmlformats-officedocument.presentationml.template.main+xml"
            | "application/vnd.openxmlformats-officedocument.presentationml.slideshow.main+xml"
            | "application/vnd.ms-powerpoint.presentation.macroEnabled.main+xml"
            | "application/vnd.ms-powerpoint.template.macroEnabled.main+xml"
            | "application/vnd.ms-powerpoint.slideshow.macroEnabled.main+xml"
    )
}

/// The relationship type suffix (`…/<suffix>`) of a PresentationML part.
fn rel_matches(rel_type: &str, suffix: &str) -> bool {
    rel_type == suffix || rel_type.ends_with(&format!("/{suffix}"))
}

fn presentation_relationship_types() -> [&'static str; 2] {
    [OFFICE_DOCUMENT_REL, OFFICE_DOCUMENT_REL_STRICT]
}

// ---------------------------------------------------------------------------
// Extraction profile
// ---------------------------------------------------------------------------

/// A versioned, explicit presentation-projection profile. The profile identity is
/// recorded in every answer (and hashed into the canonical selector), so a
/// projection is never hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PptxExtractProfile {
    /// Profile semantics version; must equal [`PPTX_EXTRACT_PROFILE_VERSION`].
    pub version: u32,
    /// Include notes-slide text in a whole-deck projection.
    pub include_notes: bool,
    /// Include hidden slides in a whole-deck projection.
    pub include_hidden: bool,
}

impl PptxExtractProfile {
    /// The declared default profile: notes excluded from whole-deck projections
    /// (notes are still addressable by their own selector) and hidden slides
    /// excluded (but still addressable by slide index).
    pub const DEFAULT: PptxExtractProfile = PptxExtractProfile {
        version: PPTX_EXTRACT_PROFILE_VERSION,
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
    pub fn decode(b: &[u8]) -> Result<PptxExtractProfile> {
        if b.len() != 3 {
            return Err(corrupt("PPTX profile must be 3 bytes"));
        }
        if b[0] as u32 != PPTX_EXTRACT_PROFILE_VERSION {
            return Err(Error::unsupported_version(format!(
                "PPTX extraction profile version {} is not supported",
                b[0]
            )));
        }
        Ok(PptxExtractProfile {
            version: b[0] as u32,
            include_notes: b[1] != 0,
            include_hidden: b[2] != 0,
        })
    }
}

impl Default for PptxExtractProfile {
    fn default() -> Self {
        PptxExtractProfile::DEFAULT
    }
}

// ---------------------------------------------------------------------------
// Discovery model
// ---------------------------------------------------------------------------

/// A part that backs a presentation object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PptxPartRef {
    /// Absolute OPC part name.
    pub name: String,
    /// Physical member ordinal.
    pub ordinal: u32,
    /// Content type, when known.
    pub content_type: Option<String>,
}

/// One slide part discovered through the presentation relationships.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PptxSlideRef {
    /// The presentation relationship id (`r:id`) when discovered by relationship.
    pub rel_id: Option<String>,
    /// Deterministic discovery order (relationship-id order, or part-name order).
    pub order: u32,
    /// The backing slide part.
    pub part: PptxPartRef,
}

/// The canonical PPTX discovery model: the main presentation part, the notes and
/// slide masters, the layouts, themes and media parts, and the discovered slides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PptxModel {
    /// The main presentation part (resolved via the `officeDocument` relationship).
    pub presentation: PptxPartRef,
    /// The notes-master part, when present.
    pub notes_master: Option<PptxPartRef>,
    /// The slide-master parts, in relationship-id order.
    pub slide_masters: Vec<PptxPartRef>,
    /// The slide-layout parts, in deterministic part-name order.
    pub layouts: Vec<PptxPartRef>,
    /// The theme parts, in deterministic part-name order.
    pub themes: Vec<PptxPartRef>,
    /// The media parts (images/audio/video), in deterministic part-name order.
    pub media: Vec<PptxPartRef>,
    /// Discovered slide parts, sorted by `(order, part name)`.
    pub slides: Vec<PptxSlideRef>,
}

impl PptxModel {
    /// Encode the discovery model canonically.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"PPTM");
        out.push(1);
        put_part(&mut out, &self.presentation);
        put_opt_part(&mut out, self.notes_master.as_ref());
        put_parts(&mut out, &self.slide_masters);
        put_parts(&mut out, &self.layouts);
        put_parts(&mut out, &self.themes);
        put_parts(&mut out, &self.media);
        put_u32(&mut out, self.slides.len() as u32);
        for s in &self.slides {
            put_opt_str(&mut out, s.rel_id.as_deref());
            put_u32(&mut out, s.order);
            put_part(&mut out, &s.part);
        }
        out
    }

    /// Decode a model produced by [`PptxModel::encode`].
    pub fn decode(bytes: &[u8]) -> Result<PptxModel> {
        let mut r = ByteReader::new(bytes);
        if r.bytes(4)? != b"PPTM" {
            return Err(corrupt("bad PPTX model magic"));
        }
        if r.u8()? != 1 {
            return Err(corrupt("unsupported PPTX model version"));
        }
        let presentation = read_part(&mut r)?;
        let notes_master = read_opt_part(&mut r)?;
        let slide_masters = read_parts(&mut r)?;
        let layouts = read_parts(&mut r)?;
        let themes = read_parts(&mut r)?;
        let media = read_parts(&mut r)?;
        let n = r.u32()?;
        let mut slides = Vec::new();
        for _ in 0..n {
            let rel_id = r.opt_string()?;
            let order = r.u32()?;
            let part = read_part(&mut r)?;
            slides.push(PptxSlideRef {
                rel_id,
                order,
                part,
            });
        }
        if !r.at_end() {
            return Err(corrupt("PPTX model has trailing bytes"));
        }
        Ok(PptxModel {
            presentation,
            notes_master,
            slide_masters,
            layouts,
            themes,
            media,
            slides,
        })
    }
}

// ---------------------------------------------------------------------------
// Presentation (slide-order) model
// ---------------------------------------------------------------------------

/// One `<p:sldId>` declared in `ppt/presentation.xml`, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationSlide {
    /// The numeric `id` attribute, when present.
    pub id: Option<u32>,
    /// `r:id` → the presentation relationship naming the slide part.
    pub rel_id: Option<String>,
    /// Whether the slide is hidden (`show="0"` on the slide part, resolved later;
    /// recorded here when the presentation declares it).
    pub hidden: bool,
}

/// The parsed presentation inventory: the slide size and the ordered slide list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationModel {
    /// The `p:sldSz` `(cx, cy)` in EMU, when declared.
    pub slide_size: Option<(u64, u64)>,
    /// The declared slides, in `p:sldIdLst` document order.
    pub slides: Vec<PresentationSlide>,
}

impl PresentationModel {
    /// Encode canonically.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"PPTP");
        out.push(1);
        match self.slide_size {
            Some((cx, cy)) => {
                out.push(1);
                put_u64(&mut out, cx);
                put_u64(&mut out, cy);
            }
            None => out.push(0),
        }
        put_u32(&mut out, self.slides.len() as u32);
        for s in &self.slides {
            put_opt_u32(&mut out, s.id);
            put_opt_str(&mut out, s.rel_id.as_deref());
            out.push(s.hidden as u8);
        }
        out
    }

    /// Decode a model produced by [`Self::encode`].
    pub fn decode(bytes: &[u8]) -> Result<PresentationModel> {
        let mut r = ByteReader::new(bytes);
        if r.bytes(4)? != b"PPTP" {
            return Err(corrupt("bad PPTX presentation magic"));
        }
        if r.u8()? != 1 {
            return Err(corrupt("unsupported PPTX presentation version"));
        }
        let slide_size = match r.u8()? {
            0 => None,
            1 => Some((r.u64()?, r.u64()?)),
            _ => return Err(corrupt("bad slide-size tag")),
        };
        let n = r.u32()?;
        let mut slides = Vec::new();
        for _ in 0..n {
            slides.push(PresentationSlide {
                id: r.opt_u32()?,
                rel_id: r.opt_string()?,
                hidden: r.u8()? != 0,
            });
        }
        if !r.at_end() {
            return Err(corrupt("PPTX presentation has trailing bytes"));
        }
        Ok(PresentationModel { slide_size, slides })
    }
}

// ---------------------------------------------------------------------------
// Slide (shape) model
// ---------------------------------------------------------------------------

/// The kind of a presentation shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeKind {
    /// A text shape (`p:sp`).
    Text,
    /// A picture (`p:pic`).
    Picture,
    /// A graphic frame carrying an embedded table (`p:graphicFrame` → `a:tbl`).
    Table,
    /// A graphic frame carrying a chart reference (`p:graphicFrame` → `c:chart`).
    Chart,
    /// A group shape (`p:grpSp`).
    Group,
    /// A connector (`p:cxnSp`).
    Connector,
    /// Any other graphic frame / unrecognized shape.
    Other,
}

impl ShapeKind {
    /// Stable lower-case name.
    pub const fn name(self) -> &'static str {
        match self {
            ShapeKind::Text => "text",
            ShapeKind::Picture => "picture",
            ShapeKind::Table => "table",
            ShapeKind::Chart => "chart",
            ShapeKind::Group => "group",
            ShapeKind::Connector => "connector",
            ShapeKind::Other => "other",
        }
    }

    fn tag(self) -> u8 {
        match self {
            ShapeKind::Text => 0,
            ShapeKind::Picture => 1,
            ShapeKind::Table => 2,
            ShapeKind::Chart => 3,
            ShapeKind::Group => 4,
            ShapeKind::Connector => 5,
            ShapeKind::Other => 6,
        }
    }

    fn from_tag(b: u8) -> Result<ShapeKind> {
        Ok(match b {
            0 => ShapeKind::Text,
            1 => ShapeKind::Picture,
            2 => ShapeKind::Table,
            3 => ShapeKind::Chart,
            4 => ShapeKind::Group,
            5 => ShapeKind::Connector,
            6 => ShapeKind::Other,
            _ => return Err(corrupt("bad shape-kind tag")),
        })
    }
}

/// One cell of an embedded table (`a:tc`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PptxTableCell {
    /// The cell text (the concatenated `a:t` runs, paragraphs joined by `\n`).
    pub text: String,
    /// `gridSpan`, defaulting to 1.
    pub grid_span: u32,
    /// `rowSpan`, defaulting to 1.
    pub row_span: u32,
    /// `hMerge="1"`.
    pub h_merge: bool,
    /// `vMerge="1"`.
    pub v_merge: bool,
}

/// One row of an embedded table (`a:tr`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PptxTableRow {
    /// Cells present in the row, in document order.
    pub cells: Vec<PptxTableCell>,
}

/// An embedded table (`a:tbl`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PptxTable {
    /// Rows, in document order.
    pub rows: Vec<PptxTableRow>,
}

impl PptxTable {
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
/// container (the shape tree, or a group's nested tree).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PptxShape {
    /// Position within the containing shape tree.
    pub index: u32,
    /// The shape kind.
    pub kind: ShapeKind,
    /// The `p:cNvPr/@name`, when present.
    pub name: Option<String>,
    /// The `p:cNvPr/@id`, when present.
    pub shape_id: Option<u32>,
    /// The placeholder type (`p:ph/@type`), when present.
    pub placeholder: Option<String>,
    /// The shape's text (paragraphs joined by `\n`).
    pub text: String,
    /// The `a:blip/@r:embed` media relationship id, for pictures.
    pub media_rel_id: Option<String>,
    /// The `c:chart/@r:id` chart relationship id, for chart frames.
    pub chart_rel_id: Option<String>,
    /// The embedded table, for table frames.
    pub table: Option<PptxTable>,
    /// Nested shapes, for group shapes.
    pub children: Vec<PptxShape>,
}

impl PptxShape {
    /// The shape's own searchable text: its run text plus, for a table frame, the
    /// embedded table's text. **Not** its children's text. This is the atomic
    /// content of one shape, so a table shape contributes its cells (matching the
    /// DOCX/drawing distinction where a table block's text is part of the body).
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

    fn collect_tables(&self, out: &mut Vec<PptxTable>) {
        if let Some(t) = &self.table {
            out.push(t.clone());
        }
        for c in &self.children {
            c.collect_tables(out);
        }
    }
}

/// The parsed bounded view of one slide (or notes-slide) part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlideModel {
    /// Absolute OPC part name.
    pub part_name: String,
    /// Whether the slide is hidden (`show="0"` on the slide root).
    pub hidden: bool,
    /// Top-level shapes, in document order.
    pub shapes: Vec<PptxShape>,
    /// All embedded tables, flattened in pre-order across shapes (incl. groups).
    pub tables: Vec<PptxTable>,
}

impl SlideModel {
    /// Total number of shapes (including nested group children).
    pub fn shape_count(&self) -> u64 {
        fn walk(s: &PptxShape) -> u64 {
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
    pub fn shape_by_flat_index(&self, index: u32) -> Option<&PptxShape> {
        fn walk<'a>(s: &'a PptxShape, want: u32, seen: &mut u32) -> Option<&'a PptxShape> {
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
            collect_shape_text(s, &mut parts);
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

fn collect_shape_text(s: &PptxShape, out: &mut Vec<String>) {
    let own = s.own_text();
    if !own.is_empty() {
        out.push(own);
    }
    for c in &s.children {
        collect_shape_text(c, out);
    }
}

fn title_of(s: &PptxShape) -> Option<String> {
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

impl SlideModel {
    /// Encode canonically.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"PPTS");
        out.push(1);
        put_str(&mut out, &self.part_name);
        out.push(self.hidden as u8);
        put_u32(&mut out, self.shapes.len() as u32);
        for s in &self.shapes {
            put_shape(&mut out, s);
        }
        put_u32(&mut out, self.tables.len() as u32);
        for t in &self.tables {
            put_table(&mut out, t);
        }
        out
    }

    /// Decode a model produced by [`Self::encode`].
    pub fn decode(bytes: &[u8]) -> Result<SlideModel> {
        let mut r = ByteReader::new(bytes);
        if r.bytes(4)? != b"PPTS" {
            return Err(corrupt("bad PPTX slide magic"));
        }
        if r.u8()? != 1 {
            return Err(corrupt("unsupported PPTX slide version"));
        }
        let part_name = r.string()?;
        let hidden = r.u8()? != 0;
        let n = r.u32()?;
        let mut shapes = Vec::new();
        for _ in 0..n {
            shapes.push(read_shape(&mut r, 0)?);
        }
        let nt = r.u32()?;
        let mut tables = Vec::new();
        for _ in 0..nt {
            tables.push(read_table(&mut r)?);
        }
        if !r.at_end() {
            return Err(corrupt("PPTX slide has trailing bytes"));
        }
        Ok(SlideModel {
            part_name,
            hidden,
            shapes,
            tables,
        })
    }
}

/// The parsed bounded view of one notes-slide part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotesModel {
    /// Absolute OPC part name.
    pub part_name: String,
    /// The notes text (all shape text, pre-order).
    pub text: String,
    /// The number of shapes parsed.
    pub shapes: u32,
}

impl NotesModel {
    /// Encode canonically.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"PPTN");
        out.push(1);
        put_str(&mut out, &self.part_name);
        put_str(&mut out, &self.text);
        put_u32(&mut out, self.shapes);
        out
    }

    /// Decode a model produced by [`Self::encode`].
    pub fn decode(bytes: &[u8]) -> Result<NotesModel> {
        let mut r = ByteReader::new(bytes);
        if r.bytes(4)? != b"PPTN" {
            return Err(corrupt("bad PPTX notes magic"));
        }
        if r.u8()? != 1 {
            return Err(corrupt("unsupported PPTX notes version"));
        }
        let part_name = r.string()?;
        let text = r.string()?;
        let shapes = r.u32()?;
        if !r.at_end() {
            return Err(corrupt("PPTX notes has trailing bytes"));
        }
        Ok(NotesModel {
            part_name,
            text,
            shapes,
        })
    }
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Discover the main presentation part by relationship (package `_rels/.rels`),
/// failing closed on zero, ambiguous, external, or non-part targets.
fn find_presentation(model: &OpcModel) -> Result<&OpcPart> {
    let types = presentation_relationship_types();
    let mut matched: u32 = 0;
    let mut target: Option<&OpcPart> = None;
    for r in &model.package_rels {
        if !types.contains(&r.rel_type.as_str()) {
            continue;
        }
        matched += 1;
        let resolved = r.resolved.as_deref().ok_or_else(|| {
            Error::invalid_package_structure(format!(
                "officeDocument relationship {:?} has an external/unresolved target",
                r.id
            ))
        })?;
        let part = model.part_by_name(resolved).ok_or_else(|| {
            Error::invalid_package_structure(format!(
                "officeDocument relationship {:?} targets {resolved:?}, which is not a package part",
                r.id
            ))
        })?;
        if let Some(prev) = target
            && !prev.name.eq_ignore_ascii_case(&part.name)
        {
            return Err(Error::invalid_package_structure(
                "more than one distinct officeDocument target part is ambiguous",
            ));
        }
        target = Some(part);
    }
    if matched == 0 {
        return Err(Error::invalid_package_structure(
            "package has no officeDocument relationship (not a PPTX presentation)",
        ));
    }
    let part = target.ok_or_else(|| {
        Error::invalid_package_structure("officeDocument relationship has no internal target")
    })?;
    let ct = part.content_type.as_deref().unwrap_or("");
    if !is_presentation_content_type(ct)
        && !part
            .name
            .to_ascii_lowercase()
            .ends_with("/presentation.xml")
    {
        return Err(Error::invalid_package_structure(format!(
            "officeDocument target {:?} has content type {:?}, not a PresentationML presentation",
            part.name, ct
        )));
    }
    Ok(part)
}

/// The `owner`'s relationships whose type matches one of `suffixes`, resolved to
/// a package part, returned as `(rel_id, part)` in relationship-id order.
fn related(model: &OpcModel, owner_ordinal: u32, suffixes: &[&str]) -> Vec<(String, OpcPart)> {
    let mut out: Vec<(String, OpcPart)> = Vec::new();
    let Some((_, rels)) = model.part_rels.iter().find(|(o, _)| *o == owner_ordinal) else {
        return out;
    };
    for r in rels {
        if !suffixes.iter().any(|s| rel_matches(&r.rel_type, s)) {
            continue;
        }
        let Some(resolved) = r.resolved.as_deref() else {
            continue;
        };
        if let Some(p) = model.part_by_name(resolved)
            && !out
                .iter()
                .any(|(_, q)| q.name.eq_ignore_ascii_case(&p.name))
        {
            out.push((r.id.clone(), p.clone()));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn parts_by_content_type(model: &OpcModel, pred: impl Fn(&str) -> bool) -> Vec<OpcPart> {
    let mut out: Vec<OpcPart> = model
        .parts
        .iter()
        .filter(|p| p.content_type.as_deref().is_some_and(&pred))
        .cloned()
        .collect();
    out.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });
    out
}

fn to_ref(p: &OpcPart) -> PptxPartRef {
    PptxPartRef {
        name: p.name.clone(),
        ordinal: p.ordinal,
        content_type: p.content_type.clone(),
    }
}

fn push_capped(out: &mut Vec<PptxPartRef>, p: &OpcPart, cap: u32, what: &str) -> Result<()> {
    if out.len() as u64 >= u64::from(cap) {
        return Err(Error::resource_limit(format!(
            "package declares more {what} than the configured cap"
        )));
    }
    if !out.iter().any(|q| q.name.eq_ignore_ascii_case(&p.name)) {
        out.push(to_ref(p));
    }
    Ok(())
}

fn discover(model: &OpcModel, limits: Limits) -> Result<PptxModel> {
    let presentation = find_presentation(model)?;
    let p_ord = presentation.ordinal;

    let notes_master = related(model, p_ord, &["notesMaster"])
        .into_iter()
        .next()
        .map(|(_, p)| to_ref(&p));

    let master_parts = related(model, p_ord, &["slideMaster"]);
    if master_parts.len() as u64 > u64::from(limits.max_pptx_masters) {
        return Err(Error::resource_limit(
            "package declares more slide masters than max_pptx_masters",
        ));
    }
    let slide_masters: Vec<PptxPartRef> = master_parts.iter().map(|(_, p)| to_ref(p)).collect();

    // Layouts: resolve through the slide masters' relationships first; fall back
    // to a content-type scan when a master declares none.
    let mut layouts: Vec<PptxPartRef> = Vec::new();
    for (_, master) in &master_parts {
        for (_, p) in related(model, master.ordinal, &["slideLayout"]) {
            push_capped(&mut layouts, &p, limits.max_pptx_layouts, "slide layouts")?;
        }
    }
    if layouts.is_empty() {
        for p in parts_by_content_type(model, |ct| ct.ends_with("presentationml.slideLayout+xml")) {
            push_capped(&mut layouts, &p, limits.max_pptx_layouts, "slide layouts")?;
        }
    }

    let mut themes: Vec<PptxPartRef> = Vec::new();
    for p in parts_by_content_type(model, |ct| ct.ends_with("officedocument.theme+xml")) {
        push_capped(&mut themes, &p, limits.max_pptx_masters, "themes")?;
    }

    let mut media: Vec<PptxPartRef> = Vec::new();
    for p in parts_by_content_type(model, |ct| {
        ct.starts_with("image/") || ct.starts_with("audio/") || ct.starts_with("video/")
    }) {
        push_capped(&mut media, &p, limits.max_pptx_media, "media parts")?;
    }

    // Slides: by presentation relationship (never a hardcoded `slideN.xml`); the
    // *order* is decided later from `p:sldIdLst`, so this is a set keyed by r:id.
    let mut slides: Vec<PptxSlideRef> = Vec::new();
    let related_slides = related(model, p_ord, &["slide"]);
    if !related_slides.is_empty() {
        for (order, (rel_id, p)) in related_slides.iter().enumerate() {
            if slides.len() as u64 >= u64::from(limits.max_pptx_slides) {
                return Err(Error::resource_limit(
                    "presentation declares more slides than max_pptx_slides",
                ));
            }
            slides.push(PptxSlideRef {
                rel_id: Some(rel_id.clone()),
                order: order as u32,
                part: to_ref(p),
            });
        }
    } else {
        for (order, p) in
            parts_by_content_type(model, |ct| ct.ends_with("presentationml.slide+xml"))
                .into_iter()
                .enumerate()
        {
            if slides.len() as u64 >= u64::from(limits.max_pptx_slides) {
                return Err(Error::resource_limit(
                    "presentation declares more slides than max_pptx_slides",
                ));
            }
            slides.push(PptxSlideRef {
                rel_id: None,
                order: order as u32,
                part: to_ref(&p),
            });
        }
    }

    Ok(PptxModel {
        presentation: to_ref(presentation),
        notes_master,
        slide_masters,
        layouts,
        themes,
        media,
        slides,
    })
}

/// Build the canonical PPTX discovery model from a canonical OPC model (the
/// derived [`crate::field::node::NodeKind::PptxModel`] computation).
pub fn build_pptx_model(opc_bytes: &[u8], limits: Limits) -> Result<Vec<u8>> {
    let model = OpcModel::decode(opc_bytes)?;
    let pptx = discover(&model, limits)?;
    Ok(pptx.encode())
}

// ---------------------------------------------------------------------------
// Node parameters
// ---------------------------------------------------------------------------

/// Canonical parameters for a [`crate::field::node::NodeKind::PptxPresentation`]
/// node: `version(1) · ordinal(4) · len-prefixed part name`.
pub fn presentation_params(ordinal: u32, part_name: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(9 + part_name.len());
    out.push(1);
    put_u32(&mut out, ordinal);
    put_str(&mut out, part_name);
    out
}

/// Decode parameters produced by [`presentation_params`].
pub fn read_presentation_params(params: &[u8]) -> Result<(u32, String)> {
    let mut r = ByteReader::new(params);
    if r.u8()? != 1 {
        return Err(corrupt("unsupported PPTX presentation params version"));
    }
    let ordinal = r.u32()?;
    let name = r.string()?;
    if !r.at_end() {
        return Err(corrupt("PPTX presentation params have trailing bytes"));
    }
    Ok((ordinal, name))
}

/// Canonical parameters for a [`crate::field::node::NodeKind::PptxSlide`] node:
/// `version(1) · ordinal(4) · profile(3) · len-prefixed part name`.
pub fn slide_params(ordinal: u32, part_name: &str, profile: &PptxExtractProfile) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + part_name.len());
    out.push(1);
    put_u32(&mut out, ordinal);
    out.extend_from_slice(&profile.encode());
    put_str(&mut out, part_name);
    out
}

/// Decode parameters produced by [`slide_params`].
pub fn read_slide_params(params: &[u8]) -> Result<(u32, PptxExtractProfile, String)> {
    let mut r = ByteReader::new(params);
    if r.u8()? != 1 {
        return Err(corrupt("unsupported PPTX slide params version"));
    }
    let ordinal = r.u32()?;
    let profile = PptxExtractProfile::decode(r.bytes(3)?)?;
    let part_name = r.string()?;
    if !r.at_end() {
        return Err(corrupt("PPTX slide params have trailing bytes"));
    }
    Ok((ordinal, profile, part_name))
}

/// Canonical parameters for a [`crate::field::node::NodeKind::PptxNotes`] node:
/// `version(1) · ordinal(4) · profile(3) · len-prefixed part name`.
pub fn notes_params(ordinal: u32, part_name: &str, profile: &PptxExtractProfile) -> Vec<u8> {
    slide_params(ordinal, part_name, profile)
}

/// Decode parameters produced by [`notes_params`].
pub fn read_notes_params(params: &[u8]) -> Result<(u32, PptxExtractProfile, String)> {
    read_slide_params(params)
}

// ---------------------------------------------------------------------------
// Presentation parsing
// ---------------------------------------------------------------------------

/// Parse and harden `ppt/presentation.xml` into its slide size and ordered slide
/// list (`p:sldIdLst` → `p:sldId/@r:id`).
pub fn parse_presentation(bytes: &[u8], limits: Limits) -> Result<PresentationModel> {
    harden_xml(bytes, limits)?;
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut st = XmlState::new();
    let mut slide_size: Option<(u64, u64)> = None;
    let mut slides: Vec<PresentationSlide> = Vec::new();
    let mut saw_root = false;
    loop {
        let ev = reader.read_event().map_err(xml_err)?;
        st.event(limits)?;
        match ev {
            Event::Eof => break,
            Event::DocType(d) => accept_doctype(&d.into_inner())?,
            Event::Start(e) => {
                st.open(limits)?;
                if !saw_root {
                    check_root(&e, "presentation")?;
                    saw_root = true;
                }
                presentation_element(&e, limits, &mut slide_size, &mut slides)?;
            }
            Event::Empty(e) => {
                st.leaf(limits)?;
                if !saw_root {
                    check_root(&e, "presentation")?;
                    saw_root = true;
                }
                presentation_element(&e, limits, &mut slide_size, &mut slides)?;
            }
            Event::End(_) => st.close(),
            Event::Text(t) => st.text(t.len(), limits)?,
            _ => {}
        }
    }
    if !saw_root {
        return Err(Error::invalid_xml_structure(
            "presentation XML part is empty",
        ));
    }
    Ok(PresentationModel { slide_size, slides })
}

fn presentation_element(
    e: &BytesStart<'_>,
    limits: Limits,
    slide_size: &mut Option<(u64, u64)>,
    slides: &mut Vec<PresentationSlide>,
) -> Result<()> {
    match e.name().local_name().as_ref() {
        "sldSz" => {
            let attrs = read_attrs(e, limits)?;
            let cx = parse_u64_opt(&attrs, "cx");
            let cy = parse_u64_opt(&attrs, "cy");
            if let (Some(cx), Some(cy)) = (cx, cy) {
                *slide_size = Some((cx, cy));
            }
        }
        "sldId" => {
            if slides.len() as u64 >= u64::from(limits.max_pptx_slides) {
                return Err(Error::resource_limit(
                    "presentation declares more slides than max_pptx_slides",
                ));
            }
            let attrs = read_attrs_qualified(e, limits)?;
            let id = match attr_of(&attrs, "id") {
                Some(v) => Some(
                    v.parse::<u32>()
                        .map_err(|_| Error::invalid_package_structure("sldId id is not a u32"))?,
                ),
                None => None,
            };
            let rel_id = ns_attr(&attrs, "id").map(str::to_string);
            slides.push(PresentationSlide {
                id,
                rel_id,
                hidden: false,
            });
        }
        _ => {}
    }
    Ok(())
}

fn parse_u64_opt(attrs: &[(String, String)], name: &str) -> Option<u64> {
    attr_of(attrs, name).and_then(|v| v.parse::<u64>().ok())
}

/// The value of the first attribute whose qualified name carries a namespace
/// prefix (`…:local`) and whose local part is `local`. Used for the
/// PresentationML attributes that live in the relationships namespace (`r:id`,
/// `r:embed`), whose prefix is writer-chosen. Requires a prefix so it never
/// collides with a bare unprefixed attribute of the same local name.
fn ns_attr<'a>(attrs: &'a [(String, String)], local: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k.contains(':') && k.rsplit(':').next() == Some(local))
        .map(|(_, v)| v.as_str())
}

// ---------------------------------------------------------------------------
// Slide parsing
// ---------------------------------------------------------------------------

/// One cell being assembled.
struct CellB {
    text: String,
    grid_span: u32,
    row_span: u32,
    h_merge: bool,
    v_merge: bool,
}

/// A table being assembled.
struct TableB {
    rows: Vec<Vec<PptxTableCell>>,
    cur_row: Option<Vec<PptxTableCell>>,
    cur_cell: Option<CellB>,
}

impl TableB {
    fn new() -> Self {
        TableB {
            rows: Vec::new(),
            cur_row: None,
            cur_cell: None,
        }
    }

    fn finish(self) -> PptxTable {
        PptxTable {
            rows: self
                .rows
                .into_iter()
                .map(|cells| PptxTableRow { cells })
                .collect(),
        }
    }
}

/// A shape being assembled.
struct ShapeB {
    kind: ShapeKind,
    name: Option<String>,
    shape_id: Option<u32>,
    placeholder: Option<String>,
    text: String,
    media_rel_id: Option<String>,
    chart_rel_id: Option<String>,
    table: Option<TableB>,
    children: Vec<PptxShape>,
}

impl ShapeB {
    fn new(kind: ShapeKind) -> Self {
        ShapeB {
            kind,
            name: None,
            shape_id: None,
            placeholder: None,
            text: String::new(),
            media_rel_id: None,
            chart_rel_id: None,
            table: None,
            children: Vec::new(),
        }
    }

    fn into_shape(self, index: u32) -> PptxShape {
        let mut kind = self.kind;
        if self.table.is_some() {
            kind = ShapeKind::Table;
        } else if kind == ShapeKind::Other && self.chart_rel_id.is_some() {
            kind = ShapeKind::Chart;
        }
        PptxShape {
            index,
            kind,
            name: self.name,
            shape_id: self.shape_id,
            placeholder: self.placeholder,
            text: self.text,
            media_rel_id: self.media_rel_id,
            chart_rel_id: self.chart_rel_id,
            table: self.table.map(TableB::finish),
            children: self.children,
        }
    }
}

/// Where the current text is routed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TxtTarget {
    None,
    Shape,
    Cell,
}

/// A bounded streaming parser for a slide (or notes-slide) shape tree.
struct SlideParser {
    limits: Limits,
    stack: Vec<ShapeB>,
    root: Vec<PptxShape>,
    total_shapes: u64,
    total_runs: u64,
    table_count: u64,
    table_cells: u64,
    txt: TxtTarget,
    root_hidden: bool,
}

impl SlideParser {
    fn new(limits: Limits) -> Self {
        SlideParser {
            limits,
            stack: Vec::new(),
            root: Vec::new(),
            total_shapes: 0,
            total_runs: 0,
            table_count: 0,
            table_cells: 0,
            txt: TxtTarget::None,
            root_hidden: false,
        }
    }

    fn set_root_hidden(&mut self, e: &BytesStart<'_>, limits: Limits) -> Result<()> {
        let attrs = read_attrs(e, limits)?;
        self.root_hidden = attr_of(&attrs, "show") == Some("0");
        Ok(())
    }

    fn push_shape(&mut self, kind: ShapeKind) -> Result<()> {
        self.total_shapes = self.total_shapes.saturating_add(1);
        if self.total_shapes > u64::from(self.limits.max_pptx_shapes_per_slide) {
            return Err(Error::resource_limit(
                "slide has more shapes than max_pptx_shapes_per_slide",
            ));
        }
        if kind == ShapeKind::Group {
            let groups = self
                .stack
                .iter()
                .filter(|s| s.kind == ShapeKind::Group)
                .count() as u64;
            if groups + 1 > u64::from(self.limits.max_pptx_group_depth) {
                return Err(Error::resource_limit(
                    "slide group nesting exceeds max_pptx_group_depth",
                ));
            }
        }
        self.stack.push(ShapeB::new(kind));
        Ok(())
    }

    fn element(&mut self, e: &BytesStart<'_>) -> Result<()> {
        let limits = self.limits;
        let local = e.name().local_name().as_ref().as_bytes().to_vec();
        match local.as_slice() {
            b"sp" => self.push_shape(ShapeKind::Text)?,
            b"pic" => self.push_shape(ShapeKind::Picture)?,
            b"graphicFrame" => self.push_shape(ShapeKind::Other)?,
            b"grpSp" => self.push_shape(ShapeKind::Group)?,
            b"cxnSp" => self.push_shape(ShapeKind::Connector)?,
            b"cNvPr" => {
                let attrs = read_attrs(e, limits)?;
                if let Some(top) = self.stack.last_mut() {
                    top.name = attr_of(&attrs, "name").map(str::to_string);
                    top.shape_id = match attr_of(&attrs, "id") {
                        Some(v) => Some(v.parse::<u32>().map_err(|_| {
                            Error::invalid_package_structure("cNvPr id is not a u32")
                        })?),
                        None => None,
                    };
                }
            }
            b"ph" => {
                let attrs = read_attrs(e, limits)?;
                if let Some(top) = self.stack.last_mut() {
                    top.placeholder = attr_of(&attrs, "type").map(str::to_string);
                }
            }
            b"blip" => {
                let attrs = read_attrs_qualified(e, limits)?;
                if let Some(top) = self.stack.last_mut() {
                    top.media_rel_id = ns_attr(&attrs, "embed").map(str::to_string);
                }
            }
            b"chart" => {
                let attrs = read_attrs_qualified(e, limits)?;
                if let Some(top) = self.stack.last_mut() {
                    top.chart_rel_id = ns_attr(&attrs, "id").map(str::to_string);
                }
            }
            b"tbl" => {
                self.table_count = self.table_count.saturating_add(1);
                if self.table_count > u64::from(self.limits.max_pptx_tables) {
                    return Err(Error::resource_limit(
                        "slide has more tables than max_pptx_tables",
                    ));
                }
                if let Some(top) = self.stack.last_mut() {
                    top.table = Some(TableB::new());
                }
            }
            b"tr" => {
                if let Some(top) = self.stack.last_mut()
                    && let Some(t) = top.table.as_mut()
                {
                    t.cur_row = Some(Vec::new());
                }
            }
            b"tc" => {
                self.table_cells = self.table_cells.saturating_add(1);
                if self.table_cells > u64::from(self.limits.max_pptx_table_cells) {
                    return Err(Error::resource_limit(
                        "slide tables exceed max_pptx_table_cells",
                    ));
                }
                let attrs = read_attrs(e, limits)?;
                if let Some(top) = self.stack.last_mut()
                    && let Some(t) = top.table.as_mut()
                {
                    t.cur_cell = Some(CellB {
                        text: String::new(),
                        grid_span: parse_u32_attr(&attrs, "gridSpan", 1)?,
                        row_span: parse_u32_attr(&attrs, "rowSpan", 1)?,
                        h_merge: xml_flag(attr_of(&attrs, "hMerge")),
                        v_merge: xml_flag(attr_of(&attrs, "vMerge")),
                    });
                }
            }
            b"t" => {
                self.total_runs = self.total_runs.saturating_add(1);
                if self.total_runs > u64::from(self.limits.max_pptx_text_runs) {
                    return Err(Error::resource_limit(
                        "slide has more text runs than max_pptx_text_runs",
                    ));
                }
            }
            b"txBody" => {
                let in_cell = self
                    .stack
                    .last()
                    .and_then(|t| t.table.as_ref())
                    .is_some_and(|t| t.cur_cell.is_some());
                self.txt = if in_cell {
                    TxtTarget::Cell
                } else {
                    TxtTarget::Shape
                };
            }
            b"p" => self.new_paragraph(),
            b"br" => self.append_text("\n"),
            _ => {}
        }
        Ok(())
    }

    fn end(&mut self, name: &str) -> Result<()> {
        match name {
            "sp" | "pic" | "graphicFrame" | "grpSp" | "cxnSp" => self.pop_shape(),
            "txBody" => {
                self.txt = TxtTarget::None;
                Ok(())
            }
            "tr" => {
                if let Some(top) = self.stack.last_mut()
                    && let Some(t) = top.table.as_mut()
                    && let Some(row) = t.cur_row.take()
                {
                    t.rows.push(row);
                }
                Ok(())
            }
            "tc" => {
                if let Some(top) = self.stack.last_mut()
                    && let Some(t) = top.table.as_mut()
                    && let Some(cell) = t.cur_cell.take()
                    && let Some(row) = t.cur_row.as_mut()
                {
                    row.push(PptxTableCell {
                        text: cell.text,
                        grid_span: cell.grid_span,
                        row_span: cell.row_span,
                        h_merge: cell.h_merge,
                        v_merge: cell.v_merge,
                    });
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn pop_shape(&mut self) -> Result<()> {
        let b = self
            .stack
            .pop()
            .ok_or_else(|| Error::internal_invariant("PPTX shape stack underflow"))?;
        let index = self
            .stack
            .last()
            .map_or(self.root.len(), |p| p.children.len()) as u32;
        let shape = b.into_shape(index);
        if let Some(parent) = self.stack.last_mut() {
            parent.children.push(shape);
        } else {
            self.root.push(shape);
        }
        Ok(())
    }

    fn new_paragraph(&mut self) {
        match self.txt {
            TxtTarget::Shape => {
                if let Some(top) = self.stack.last_mut()
                    && !top.text.is_empty()
                {
                    top.text.push('\n');
                }
            }
            TxtTarget::Cell => {
                if let Some(cell) = self
                    .stack
                    .last_mut()
                    .and_then(|t| t.table.as_mut())
                    .and_then(|t| t.cur_cell.as_mut())
                    && !cell.text.is_empty()
                {
                    cell.text.push('\n');
                }
            }
            TxtTarget::None => {}
        }
    }

    fn append_text(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        match self.txt {
            TxtTarget::Shape => {
                if let Some(top) = self.stack.last_mut() {
                    top.text.push_str(s);
                }
            }
            TxtTarget::Cell => {
                if let Some(cell) = self
                    .stack
                    .last_mut()
                    .and_then(|t| t.table.as_mut())
                    .and_then(|t| t.cur_cell.as_mut())
                {
                    cell.text.push_str(s);
                }
            }
            TxtTarget::None => {}
        }
    }

    fn finish(self) -> SlideModel {
        let mut tables: Vec<PptxTable> = Vec::new();
        for s in &self.root {
            s.collect_tables(&mut tables);
        }
        SlideModel {
            part_name: String::new(),
            hidden: self.root_hidden,
            shapes: self.root,
            tables,
        }
    }
}

/// Parse and harden a `<p:sld>` part into its bounded [`SlideModel`].
pub fn parse_slide(
    bytes: &[u8],
    part_name: &str,
    _profile: &PptxExtractProfile,
    limits: Limits,
) -> Result<SlideModel> {
    let mut model = parse_shape_document(bytes, "sld", limits)?;
    model.part_name = part_name.to_string();
    Ok(model)
}

/// Parse and harden a `<p:notes>` part into its bounded [`NotesModel`].
pub fn parse_notes(
    bytes: &[u8],
    part_name: &str,
    _profile: &PptxExtractProfile,
    limits: Limits,
) -> Result<NotesModel> {
    let model = parse_shape_document(bytes, "notes", limits)?;
    Ok(NotesModel {
        part_name: part_name.to_string(),
        text: model.text(),
        shapes: model.shape_count() as u32,
    })
}

fn parse_shape_document(bytes: &[u8], expected_root: &str, limits: Limits) -> Result<SlideModel> {
    harden_xml(bytes, limits)?;
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut st = XmlState::new();
    let mut parser = SlideParser::new(limits);
    let mut saw_root = false;
    loop {
        let ev = reader.read_event().map_err(xml_err)?;
        st.event(limits)?;
        match ev {
            Event::Eof => break,
            Event::DocType(d) => accept_doctype(&d.into_inner())?,
            Event::Start(e) => {
                st.open(limits)?;
                if !saw_root {
                    check_root(&e, expected_root)?;
                    parser.set_root_hidden(&e, limits)?;
                    saw_root = true;
                }
                parser.element(&e)?;
            }
            Event::Empty(e) => {
                st.leaf(limits)?;
                if !saw_root {
                    check_root(&e, expected_root)?;
                    parser.set_root_hidden(&e, limits)?;
                    saw_root = true;
                }
                parser.element(&e)?;
                parser.end(e.name().local_name().as_ref())?;
            }
            Event::End(e) => {
                parser.end(e.name().local_name().as_ref())?;
                st.close();
            }
            Event::Text(t) => {
                let s = t.into_inner();
                st.text(s.len(), limits)?;
                parser.append_text(s.as_ref());
            }
            Event::GeneralRef(r) => {
                let mut s = String::new();
                push_ref(&mut s, r);
                parser.append_text(&s);
            }
            _ => {}
        }
    }
    if !saw_root {
        return Err(Error::invalid_xml_structure(
            "presentation shape part is empty",
        ));
    }
    Ok(parser.finish())
}

// ---------------------------------------------------------------------------
// XML helpers
// ---------------------------------------------------------------------------

fn check_root(e: &BytesStart<'_>, expected: &str) -> Result<()> {
    let local = e.name().local_name();
    if local.as_ref() != expected {
        return Err(Error::invalid_package_structure(format!(
            "root element <{}> is not <{expected}>",
            local.as_ref()
        )));
    }
    Ok(())
}

/// `true` for an XML boolean attribute written as `1` or `true`.
fn xml_flag(v: Option<&str>) -> bool {
    matches!(v, Some("1") | Some("true"))
}

fn parse_u32_attr(attrs: &[(String, String)], name: &str, default: u32) -> Result<u32> {
    match attr_of(attrs, name) {
        None => Ok(default),
        Some(v) => v
            .parse::<u32>()
            .map_err(|_| Error::invalid_package_structure(format!("{name} {v:?} is not a u32"))),
    }
}

fn push_ref(out: &mut String, r: quick_xml::events::BytesRef<'_>) {
    if r.is_char_ref() {
        if let Ok(Some(c)) = r.resolve_char_ref() {
            out.push(c);
        }
        return;
    }
    out.push_str(match r.into_inner().as_ref() {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        // No DTD/entity resolver: an undeclared entity is not expanded.
        _ => "",
    });
}

// ---------------------------------------------------------------------------
// Small canonical byte codec helpers
// ---------------------------------------------------------------------------

fn corrupt(msg: &str) -> Error {
    Error::invalid_package_structure(format!("corrupt PPTX model: {msg}"))
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_u64(out: &mut Vec<u8>, v: u64) {
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

fn put_opt_u32(out: &mut Vec<u8>, v: Option<u32>) {
    match v {
        Some(v) => {
            out.push(1);
            put_u32(out, v);
        }
        None => out.push(0),
    }
}

fn put_part(out: &mut Vec<u8>, p: &PptxPartRef) {
    put_str(out, &p.name);
    put_u32(out, p.ordinal);
    put_opt_str(out, p.content_type.as_deref());
}

fn put_opt_part(out: &mut Vec<u8>, p: Option<&PptxPartRef>) {
    match p {
        Some(p) => {
            out.push(1);
            put_part(out, p);
        }
        None => out.push(0),
    }
}

fn put_parts(out: &mut Vec<u8>, ps: &[PptxPartRef]) {
    put_u32(out, ps.len() as u32);
    for p in ps {
        put_part(out, p);
    }
}

fn read_part(r: &mut ByteReader<'_>) -> Result<PptxPartRef> {
    Ok(PptxPartRef {
        name: r.string()?,
        ordinal: r.u32()?,
        content_type: r.opt_string()?,
    })
}

fn read_opt_part(r: &mut ByteReader<'_>) -> Result<Option<PptxPartRef>> {
    match r.u8()? {
        0 => Ok(None),
        1 => Ok(Some(read_part(r)?)),
        _ => Err(corrupt("bad optional-part tag")),
    }
}

fn read_parts(r: &mut ByteReader<'_>) -> Result<Vec<PptxPartRef>> {
    let n = r.u32()?;
    let mut out = Vec::new();
    for _ in 0..n {
        out.push(read_part(r)?);
    }
    Ok(out)
}

fn put_table(out: &mut Vec<u8>, t: &PptxTable) {
    put_u32(out, t.rows.len() as u32);
    for row in &t.rows {
        put_u32(out, row.cells.len() as u32);
        for c in &row.cells {
            put_str(out, &c.text);
            put_u32(out, c.grid_span);
            put_u32(out, c.row_span);
            out.push(c.h_merge as u8);
            out.push(c.v_merge as u8);
        }
    }
}

fn read_table(r: &mut ByteReader<'_>) -> Result<PptxTable> {
    let nr = r.u32()?;
    let mut rows = Vec::new();
    for _ in 0..nr {
        let nc = r.u32()?;
        let mut cells = Vec::new();
        for _ in 0..nc {
            cells.push(PptxTableCell {
                text: r.string()?,
                grid_span: r.u32()?,
                row_span: r.u32()?,
                h_merge: r.u8()? != 0,
                v_merge: r.u8()? != 0,
            });
        }
        rows.push(PptxTableRow { cells });
    }
    Ok(PptxTable { rows })
}

fn put_shape(out: &mut Vec<u8>, s: &PptxShape) {
    put_u32(out, s.index);
    out.push(s.kind.tag());
    put_opt_str(out, s.name.as_deref());
    put_opt_u32(out, s.shape_id);
    put_opt_str(out, s.placeholder.as_deref());
    put_str(out, &s.text);
    put_opt_str(out, s.media_rel_id.as_deref());
    put_opt_str(out, s.chart_rel_id.as_deref());
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

fn read_shape(r: &mut ByteReader<'_>, depth: u32) -> Result<PptxShape> {
    if depth > MAX_SHAPE_DECODE_DEPTH {
        return Err(corrupt("shape nesting exceeds the decode bound"));
    }
    let index = r.u32()?;
    let kind = ShapeKind::from_tag(r.u8()?)?;
    let name = r.opt_string()?;
    let shape_id = r.opt_u32()?;
    let placeholder = r.opt_string()?;
    let text = r.string()?;
    let media_rel_id = r.opt_string()?;
    let chart_rel_id = r.opt_string()?;
    let table = match r.u8()? {
        0 => None,
        1 => Some(read_table(r)?),
        _ => return Err(corrupt("bad shape table tag")),
    };
    let n = r.u32()?;
    let mut children = Vec::new();
    for _ in 0..n {
        children.push(read_shape(r, depth + 1)?);
    }
    Ok(PptxShape {
        index,
        kind,
        name,
        shape_id,
        placeholder,
        text,
        media_rel_id,
        chart_rel_id,
        table,
        children,
    })
}

/// A bounds-checked little-endian reader shared by the model codecs.
struct ByteReader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> ByteReader<'a> {
    fn new(b: &'a [u8]) -> Self {
        ByteReader { b, at: 0 }
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| corrupt("read overflow"))?;
        if end > self.b.len() {
            return Err(corrupt("truncated"));
        }
        let s = &self.b[self.at..end];
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

    fn opt_u32(&mut self) -> Result<Option<u32>> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.u32()?)),
            _ => Err(corrupt("bad optional-u32 tag")),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_roundtrips_and_fingerprints() {
        let p = PptxExtractProfile::DEFAULT;
        assert_eq!(PptxExtractProfile::decode(&p.encode()).unwrap(), p);
        assert!(p.fingerprint().contains("n0"));
        let mut q = p;
        q.include_notes = true;
        q.include_hidden = true;
        assert_ne!(q.encode(), p.encode());
        assert_eq!(PptxExtractProfile::decode(&q.encode()).unwrap(), q);
    }

    #[test]
    fn parses_a_minimal_slide() {
        let xml = br#"<?xml version="1.0"?>
<p:sld xmlns:p="x" xmlns:a="y" xmlns:r="z">
<p:cSld><p:spTree>
<p:nvGrpSpPr/><p:grpSpPr/>
<p:sp>
  <p:nvSpPr><p:cNvPr id="2" name="Title 1"/><p:cNvSpPr/><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
  <p:txBody><a:bodyPr/><a:p><a:r><a:t>Hello</a:t></a:r></a:p><a:p><a:r><a:t>World</a:t></a:r></a:p></p:txBody>
</p:sp>
<p:pic><p:blipFill><a:blip r:embed="rId2"/></p:blipFill></p:pic>
<p:graphicFrame><a:graphic><a:graphicData><a:tbl>
  <a:tr><a:tc><a:txBody><a:p><a:r><a:t>a</a:t></a:r></a:p></a:txBody></a:tc>
         <a:tc gridSpan="2"><a:txBody><a:p><a:r><a:t>b</a:t></a:r></a:p></a:txBody></a:tc></a:tr>
  <a:tr><a:tc><a:txBody><a:p><a:r><a:t>c</a:t></a:r></a:p></a:txBody></a:tc></a:tr>
</a:tbl></a:graphicData></a:graphic></p:graphicFrame>
</p:spTree></p:cSld></p:sld>"#;
        let m = parse_slide(
            xml,
            "/ppt/slides/slide1.xml",
            &PptxExtractProfile::DEFAULT,
            Limits::DEFAULT,
        )
        .unwrap();
        assert_eq!(m.shapes.len(), 3);
        assert_eq!(m.shapes[0].kind, ShapeKind::Text);
        assert_eq!(m.shapes[0].name.as_deref(), Some("Title 1"));
        assert_eq!(m.shapes[0].placeholder.as_deref(), Some("title"));
        assert_eq!(m.shapes[0].text, "Hello\nWorld");
        assert_eq!(m.shapes[1].kind, ShapeKind::Picture);
        assert_eq!(m.shapes[1].media_rel_id.as_deref(), Some("rId2"));
        assert_eq!(m.shapes[2].kind, ShapeKind::Table);
        assert_eq!(m.tables.len(), 1);
        assert_eq!(m.tables[0].rows.len(), 2);
        assert_eq!(m.tables[0].rows[0].cells[0].text, "a");
        assert_eq!(m.tables[0].rows[0].cells[1].grid_span, 2);
        assert_eq!(m.title().as_deref(), Some("Hello\nWorld"));
        assert_eq!(SlideModel::decode(&m.encode()).unwrap(), m);
    }

    #[test]
    fn group_shapes_are_nested_and_flattened() {
        let xml = br#"<p:sld xmlns:p="x" xmlns:a="y"><p:cSld><p:spTree>
<p:grpSp><p:nvGrpSpPr><p:cNvPr id="1" name="G"/></p:nvGrpSpPr><p:grpSpPr/>
  <p:sp><p:txBody><a:p><a:r><a:t>inner</a:t></a:r></a:p></p:txBody></p:sp>
</p:grpSp>
</p:spTree></p:cSld></p:sld>"#;
        let m = parse_slide(xml, "/x", &PptxExtractProfile::DEFAULT, Limits::DEFAULT).unwrap();
        assert_eq!(m.shapes.len(), 1);
        assert_eq!(m.shapes[0].kind, ShapeKind::Group);
        assert_eq!(m.shapes[0].children.len(), 1);
        assert_eq!(m.shapes[0].text_deep(), "inner");
        assert_eq!(m.shape_count(), 2);
        assert_eq!(m.shape_by_flat_index(1).unwrap().text, "inner");
        assert_eq!(SlideModel::decode(&m.encode()).unwrap(), m);
    }

    #[test]
    fn parses_a_presentation_slide_order() {
        let xml = br#"<p:presentation xmlns:p="x" xmlns:r="z">
<p:sldSz cx="12192000" cy="6858000"/>
<p:sldIdLst><p:sldId id="256" r:id="rId3"/><p:sldId id="257" r:id="rId2"/></p:sldIdLst>
</p:presentation>"#;
        let m = parse_presentation(xml, Limits::DEFAULT).unwrap();
        assert_eq!(m.slide_size, Some((12192000, 6858000)));
        assert_eq!(m.slides.len(), 2);
        assert_eq!(m.slides[0].rel_id.as_deref(), Some("rId3"));
        assert_eq!(m.slides[0].id, Some(256));
        assert_eq!(m.slides[1].rel_id.as_deref(), Some("rId2"));
        assert_eq!(PresentationModel::decode(&m.encode()).unwrap(), m);
    }

    #[test]
    fn wrong_root_is_declined() {
        assert!(
            parse_slide(
                b"<html></html>",
                "/x",
                &PptxExtractProfile::DEFAULT,
                Limits::DEFAULT
            )
            .is_err()
        );
        assert!(parse_presentation(b"<document/>", Limits::DEFAULT).is_err());
    }
}
