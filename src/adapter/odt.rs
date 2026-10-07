//! OpenDocument Text (ODT) adapter (Phase 13.3, ADR-0038).
//!
//! An `.odt` is an **OpenDocument (ODF) package**: a ZIP archive whose first
//! member is the mandatory `stored` `mimetype` (`application/vnd.oasis.opendocument.text`),
//! whose `META-INF/manifest.xml` enumerates the package's files with their media
//! types, and whose main document part is the OpenDocument content stream
//! (`content.xml`, `office:document-content` → `office:body` → `office:text`).
//!
//! ODF is *not* OPC: it has no `[Content_Types].xml` and no `_rels/.rels`
//! `officeDocument` relationship. Like EPUB, ODT therefore reuses the byte-authoritative
//! ZIP layer (ADR-0030) and the shared bounded-XML policy, but discovers its main
//! part **semantically** from the ODF manifest's declared media type — never from a
//! hardcoded `content.xml`. It shares the OPC *helpers* (`part_name_from_member`,
//! base-dir/target resolution) but does not route through the OPC graph.
//!
//! Everything here is **derived (`Q_gen`) state**: exact authority remains the
//! Phase-12.2 ZIP member raw spans, so a hostile or malformed ODT is still an exact
//! archival object — only the *derived* observation declines, typed, and
//! `materialize(descriptor) == original_bytes` is untouched.

use std::collections::BTreeMap;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::adapter::package::xml::{
    XmlState, attr_of, doctype_declined, harden_xml, read_attrs, xml_err,
};
use crate::adapter::package::zip::{ZipMember, ZipPhysical, scan};
use crate::error::{Error, Result};
use crate::limits::Limits;

/// The mandatory ODF text `mimetype` payload.
pub const ODT_MIMETYPE: &str = "application/vnd.oasis.opendocument.text";
/// The additional accepted OpenDocument *text* media types.
pub const ODT_MIMETYPE_TEMPLATE: &str = "application/vnd.oasis.opendocument.text-template";
/// The OpenDocument master-document media type.
pub const ODT_MIMETYPE_MASTER: &str = "application/vnd.oasis.opendocument.text-master";
/// The OpenDocument web-document media type.
pub const ODT_MIMETYPE_WEB: &str = "application/vnd.oasis.opendocument.text-web";
/// The ODF package manifest member.
pub const MANIFEST_MEMBER: &str = "META-INF/manifest.xml";
/// The conventional main content part (advisory; the manifest is authority).
pub const CONTENT_MEMBER: &str = "content.xml";
/// The conventional styles part.
pub const STYLES_MEMBER: &str = "styles.xml";
/// The conventional metadata part.
pub const META_MEMBER: &str = "meta.xml";
/// The ODF package-manifest namespace.
pub const ODF_MANIFEST_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:manifest:1.0";

/// Sentinel ordinal meaning "not a member of this package".
const NO_MEMBER: u32 = u32::MAX;

/// Version of the ODT extraction profile semantics.
pub const ODT_EXTRACT_PROFILE_VERSION: u32 = 1;

/// Whether a media type is an OpenDocument *text* document type.
pub fn is_odt_text_media_type(ct: &str) -> bool {
    matches!(
        ct,
        ODT_MIMETYPE | ODT_MIMETYPE_TEMPLATE | ODT_MIMETYPE_MASTER | ODT_MIMETYPE_WEB
    )
}

// ---------------------------------------------------------------------------
// Extraction profile
// ---------------------------------------------------------------------------

/// How ODF tracked changes (`text:changed-region` + `text:change-*`) are resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OdtTrackedChanges {
    /// Accept every revision: insertions kept, deletions dropped.
    Final,
    /// Reject every revision: insertions dropped, deletions kept.
    Original,
    /// Keep both, in document order.
    All,
}

impl OdtTrackedChanges {
    /// Stable lower-case name.
    pub const fn name(self) -> &'static str {
        match self {
            OdtTrackedChanges::Final => "final",
            OdtTrackedChanges::Original => "original",
            OdtTrackedChanges::All => "all",
        }
    }
}

/// A versioned, explicit OpenDocument text-extraction profile. The profile identity
/// is recorded in every answer and hashed into the canonical selector, so a
/// projection is never hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OdtExtractProfile {
    /// Profile semantics version; must equal [`ODT_EXTRACT_PROFILE_VERSION`].
    pub version: u32,
    /// Tracked-changes resolution.
    pub tracked: OdtTrackedChanges,
    /// Include footnote text in the reading text.
    pub include_footnotes: bool,
    /// Include endnote text in the reading text.
    pub include_endnotes: bool,
    /// Include hidden (`text:display="none"`) content.
    pub hidden: bool,
    /// Render `text:tab` as `\t`.
    pub tabs: bool,
    /// Render `text:line-break` as `\n`.
    pub breaks: bool,
}

impl OdtExtractProfile {
    /// The declared default: revisions accepted, notes included, hidden excluded,
    /// tabs and breaks rendered.
    pub const DEFAULT: OdtExtractProfile = OdtExtractProfile {
        version: ODT_EXTRACT_PROFILE_VERSION,
        tracked: OdtTrackedChanges::Final,
        include_footnotes: true,
        include_endnotes: true,
        hidden: false,
        tabs: true,
        breaks: true,
    };

    /// A stable, human-readable profile fingerprint used in canonical selectors.
    pub fn fingerprint(&self) -> String {
        format!(
            "v{}-{}-f{}-e{}-h{}-t{}-b{}",
            self.version,
            self.tracked.name(),
            self.include_footnotes as u8,
            self.include_endnotes as u8,
            self.hidden as u8,
            self.tabs as u8,
            self.breaks as u8,
        )
    }

    /// The canonical 7-byte profile block.
    pub fn encode(&self) -> [u8; 7] {
        [
            self.version as u8,
            match self.tracked {
                OdtTrackedChanges::Final => 0,
                OdtTrackedChanges::Original => 1,
                OdtTrackedChanges::All => 2,
            },
            self.include_footnotes as u8,
            self.include_endnotes as u8,
            self.hidden as u8,
            self.tabs as u8,
            self.breaks as u8,
        ]
    }

    /// Decode a profile block produced by [`Self::encode`].
    pub fn decode(b: &[u8]) -> Result<OdtExtractProfile> {
        if b.len() != 7 {
            return Err(corrupt("ODT profile must be 7 bytes"));
        }
        if b[0] as u32 != ODT_EXTRACT_PROFILE_VERSION {
            return Err(Error::unsupported_version(format!(
                "ODT extraction profile version {} is not supported",
                b[0]
            )));
        }
        let tracked = match b[1] {
            0 => OdtTrackedChanges::Final,
            1 => OdtTrackedChanges::Original,
            2 => OdtTrackedChanges::All,
            _ => return Err(corrupt("bad ODT tracked-changes selector")),
        };
        Ok(OdtExtractProfile {
            version: b[0] as u32,
            tracked,
            include_footnotes: b[2] != 0,
            include_endnotes: b[3] != 0,
            hidden: b[4] != 0,
            tabs: b[5] != 0,
            breaks: b[6] != 0,
        })
    }
}

impl Default for OdtExtractProfile {
    fn default() -> Self {
        OdtExtractProfile::DEFAULT
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
    /// Its decoded payload is an OpenDocument text media type with no extra bytes.
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

/// The canonical ODT discovery model (the derived state of the `OdtModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OdtModel {
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
    /// Non-fatal structural observations.
    pub issues: Vec<String>,
}

impl OdtModel {
    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"ODTM");
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
        put_strs(&mut out, &self.issues);
        out
    }

    /// Decode a model produced by [`Self::encode`].
    pub fn decode(bytes: &[u8]) -> Result<OdtModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"ODTM" {
            return Err(corrupt("bad ODT model magic"));
        }
        if r.u8()? != 1 {
            return Err(corrupt("unsupported ODT model version"));
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
        let issues = read_strs(&mut r)?;
        if !r.at_end() {
            return Err(corrupt("ODT model has trailing bytes"));
        }
        Ok(OdtModel {
            mimetype,
            manifest,
            content,
            styles,
            meta,
            issues,
        })
    }
}

fn put_opt_part(out: &mut Vec<u8>, p: Option<&PartRef>) {
    match p {
        None => out.push(0),
        Some(p) => {
            out.push(1);
            put_str(out, &p.name);
            put_u32(out, p.ordinal);
            put_opt_str(out, p.media_type.as_deref());
        }
    }
}

fn read_opt_part(r: &mut BinReader<'_>) -> Result<Option<PartRef>> {
    Ok(match r.u8()? {
        0 => None,
        1 => Some(PartRef {
            name: r.string()?,
            ordinal: r.u32()?,
            media_type: r.opt_string()?,
        }),
        _ => return Err(corrupt("bad ODT part tag")),
    })
}

// ---------------------------------------------------------------------------
// Content model
// ---------------------------------------------------------------------------

/// One text run (`text:span` with its style).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    /// 0-based index within its paragraph.
    pub index: u32,
    /// Profile-resolved run text.
    pub text: String,
    /// `text:style-name`, when present.
    pub style_id: Option<String>,
}

/// One paragraph or heading (`text:p` / `text:h`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paragraph {
    /// 0-based index within its own class (paragraphs and headings counted
    /// separately).
    pub index: u32,
    /// Profile-resolved text.
    pub text: String,
    /// `text:style-name`, when present.
    pub style_id: Option<String>,
    /// `text:outline-level` (1-based) for a `text:h`; `None` for a `text:p`.
    pub heading_level: Option<u8>,
    /// The `text:span` runs, in order.
    pub runs: Vec<Run>,
}

impl Paragraph {
    /// Whether this paragraph is a heading.
    pub fn is_heading(&self) -> bool {
        self.heading_level.is_some()
    }
}

/// One table cell (`table:table-cell` / `table:covered-table-cell`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// 0-based physical cell position within its row.
    pub grid_col: u32,
    /// `table:number-columns-spanned` (>= 1).
    pub col_span: u32,
    /// `table:number-rows-spanned` (>= 1).
    pub row_span: u32,
    /// The cell element is `table:covered-table-cell` (a span continuation).
    pub covered: bool,
    /// Profile-resolved cell text (paragraphs joined by `\n`).
    pub text: String,
}

/// One table row (`table:table-row`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Cells, in order.
    pub cells: Vec<Cell>,
}

/// One table (`table:table`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    /// 0-based index among top-level tables.
    pub index: u32,
    /// Rows, in order.
    pub rows: Vec<Row>,
}

impl Table {
    /// The table's text: rows joined by `\n`, cells by `\t`.
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

/// One list item (`text:list-item`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    /// 0-based index within its list.
    pub index: u32,
    /// Item text (paragraphs joined by `\n`).
    pub text: String,
}

/// One list (`text:list`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct List {
    /// 0-based index among top-level lists.
    pub index: u32,
    /// Items, in order.
    pub items: Vec<ListItem>,
}

impl List {
    /// The list's text: items joined by `\n`.
    pub fn text(&self) -> String {
        self.items
            .iter()
            .map(|i| i.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// A block in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// A paragraph or heading.
    Paragraph(Paragraph),
    /// A table.
    Table(Table),
    /// A list.
    List(List),
}

impl Block {
    /// The block's reading text.
    pub fn text(&self) -> String {
        match self {
            Block::Paragraph(p) => p.text.clone(),
            Block::Table(t) => t.text(),
            Block::List(l) => l.text(),
        }
    }
}

/// A hyperlink (`text:a`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hyperlink {
    /// The anchor's text.
    pub text: String,
    /// `xlink:href` exactly as written.
    pub href: String,
    /// The href is an absolute URI: inert, never fetched.
    pub external: bool,
}

/// One note (`text:note`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// `text:id`, when present.
    pub id: String,
    /// `text:note-class` (`footnote` | `endnote`).
    pub kind: String,
    /// The note-body text.
    pub text: String,
}

/// One embedded resource reference (`draw:image`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    /// `xlink:href` exactly as written.
    pub href: String,
    /// The resolved package member, when a safe relative path.
    pub member: Option<String>,
    /// The target is an absolute URI: inert, never fetched.
    pub external: bool,
}

/// One section (`text:section`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// `text:name`, when present.
    pub name: Option<String>,
    /// 1-based nesting depth.
    pub depth: u32,
}

/// The canonical, derived content model of the OpenDocument main part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentModel {
    /// The absolute part name the model was parsed from.
    pub part_name: String,
    /// The verified root element local name.
    pub root_local: String,
    /// Blocks in document order.
    pub blocks: Vec<Block>,
    /// Hyperlinks in document order.
    pub hyperlinks: Vec<Hyperlink>,
    /// Bookmark names in document order.
    pub bookmarks: Vec<String>,
    /// Notes in document order.
    pub notes: Vec<Note>,
    /// Resource references in document order.
    pub resources: Vec<Resource>,
    /// Sections in document order.
    pub sections: Vec<Section>,
    /// Non-fatal parse observations.
    pub issues: Vec<String>,
    /// Element nodes scanned while building this model (the bounded work counter).
    pub nodes: u64,
}

impl ContentModel {
    /// Body-level paragraphs, in document order.
    pub fn paragraphs(&self) -> impl Iterator<Item = &Paragraph> {
        self.blocks.iter().filter_map(|b| match b {
            Block::Paragraph(p) => Some(p),
            _ => None,
        })
    }

    /// Headings (`text:h`), in document order.
    pub fn headings(&self) -> impl Iterator<Item = &Paragraph> {
        self.paragraphs().filter(|p| p.is_heading())
    }

    /// Top-level tables, in document order.
    pub fn tables(&self) -> impl Iterator<Item = &Table> {
        self.blocks.iter().filter_map(|b| match b {
            Block::Table(t) => Some(t),
            _ => None,
        })
    }

    /// Top-level lists, in document order.
    pub fn lists(&self) -> impl Iterator<Item = &List> {
        self.blocks.iter().filter_map(|b| match b {
            Block::List(l) => Some(l),
            _ => None,
        })
    }

    /// The full reading text: block texts joined by `\n`.
    pub fn text(&self) -> String {
        self.blocks
            .iter()
            .map(|b| b.text())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Deterministically encode the model.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"ODCT");
        out.push(1);
        put_str(&mut out, &self.part_name);
        put_str(&mut out, &self.root_local);

        put_u32(&mut out, self.blocks.len() as u32);
        for b in &self.blocks {
            match b {
                Block::Paragraph(p) => {
                    out.push(0);
                    put_u32(&mut out, p.index);
                    put_opt_str(&mut out, p.style_id.as_deref());
                    out.push(p.heading_level.unwrap_or(0xFF));
                    put_str(&mut out, &p.text);
                    put_u32(&mut out, p.runs.len() as u32);
                    for r in &p.runs {
                        put_u32(&mut out, r.index);
                        put_str(&mut out, &r.text);
                        put_opt_str(&mut out, r.style_id.as_deref());
                    }
                }
                Block::Table(t) => {
                    out.push(1);
                    put_u32(&mut out, t.index);
                    put_u32(&mut out, t.rows.len() as u32);
                    for row in &t.rows {
                        put_u32(&mut out, row.cells.len() as u32);
                        for c in &row.cells {
                            put_u32(&mut out, c.grid_col);
                            put_u32(&mut out, c.col_span);
                            put_u32(&mut out, c.row_span);
                            out.push(c.covered as u8);
                            put_str(&mut out, &c.text);
                        }
                    }
                }
                Block::List(l) => {
                    out.push(2);
                    put_u32(&mut out, l.index);
                    put_u32(&mut out, l.items.len() as u32);
                    for it in &l.items {
                        put_u32(&mut out, it.index);
                        put_str(&mut out, &it.text);
                    }
                }
            }
        }

        put_u32(&mut out, self.hyperlinks.len() as u32);
        for h in &self.hyperlinks {
            put_str(&mut out, &h.text);
            put_str(&mut out, &h.href);
            out.push(h.external as u8);
        }
        put_strs(&mut out, &self.bookmarks);
        put_u32(&mut out, self.notes.len() as u32);
        for n in &self.notes {
            put_str(&mut out, &n.id);
            put_str(&mut out, &n.kind);
            put_str(&mut out, &n.text);
        }
        put_u32(&mut out, self.resources.len() as u32);
        for r in &self.resources {
            put_str(&mut out, &r.href);
            put_opt_str(&mut out, r.member.as_deref());
            out.push(r.external as u8);
        }
        put_u32(&mut out, self.sections.len() as u32);
        for s in &self.sections {
            put_opt_str(&mut out, s.name.as_deref());
            put_u32(&mut out, s.depth);
        }
        put_strs(&mut out, &self.issues);
        out.extend_from_slice(&self.nodes.to_le_bytes());
        out
    }

    /// Decode a model produced by [`Self::encode`].
    pub fn decode(bytes: &[u8]) -> Result<ContentModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"ODCT" {
            return Err(corrupt("bad ODT content model magic"));
        }
        if r.u8()? != 1 {
            return Err(corrupt("unsupported ODT content model version"));
        }
        let part_name = r.string()?;
        let root_local = r.string()?;

        let n = bounded(&mut r, "content block")?;
        let mut blocks = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let tag = r.u8()?;
            blocks.push(match tag {
                0 => {
                    let index = r.u32()?;
                    let style_id = r.opt_string()?;
                    let h = r.u8()?;
                    let heading_level = if h == 0xFF { None } else { Some(h) };
                    let text = r.string()?;
                    let nr = bounded(&mut r, "run")?;
                    let mut runs = Vec::with_capacity(nr as usize);
                    for _ in 0..nr {
                        runs.push(Run {
                            index: r.u32()?,
                            text: r.string()?,
                            style_id: r.opt_string()?,
                        });
                    }
                    Block::Paragraph(Paragraph {
                        index,
                        text,
                        style_id,
                        heading_level,
                        runs,
                    })
                }
                1 => {
                    let index = r.u32()?;
                    let nrows = bounded(&mut r, "table row")?;
                    let mut rows = Vec::with_capacity(nrows as usize);
                    for _ in 0..nrows {
                        let ncells = bounded(&mut r, "table cell")?;
                        let mut cells = Vec::with_capacity(ncells as usize);
                        for _ in 0..ncells {
                            cells.push(Cell {
                                grid_col: r.u32()?,
                                col_span: r.u32()?,
                                row_span: r.u32()?,
                                covered: r.u8()? != 0,
                                text: r.string()?,
                            });
                        }
                        rows.push(Row { cells });
                    }
                    Block::Table(Table { index, rows })
                }
                2 => {
                    let index = r.u32()?;
                    let nitems = bounded(&mut r, "list item")?;
                    let mut items = Vec::with_capacity(nitems as usize);
                    for _ in 0..nitems {
                        items.push(ListItem {
                            index: r.u32()?,
                            text: r.string()?,
                        });
                    }
                    Block::List(List { index, items })
                }
                _ => return Err(corrupt("unknown ODT content block tag")),
            });
        }

        let nh = bounded(&mut r, "hyperlink")?;
        let mut hyperlinks = Vec::with_capacity(nh as usize);
        for _ in 0..nh {
            hyperlinks.push(Hyperlink {
                text: r.string()?,
                href: r.string()?,
                external: r.u8()? != 0,
            });
        }
        let bookmarks = read_strs(&mut r)?;
        let nn = bounded(&mut r, "note")?;
        let mut notes = Vec::with_capacity(nn as usize);
        for _ in 0..nn {
            notes.push(Note {
                id: r.string()?,
                kind: r.string()?,
                text: r.string()?,
            });
        }
        let nres = bounded(&mut r, "resource")?;
        let mut resources = Vec::with_capacity(nres as usize);
        for _ in 0..nres {
            resources.push(Resource {
                href: r.string()?,
                member: r.opt_string()?,
                external: r.u8()? != 0,
            });
        }
        let ns = bounded(&mut r, "section")?;
        let mut sections = Vec::with_capacity(ns as usize);
        for _ in 0..ns {
            sections.push(Section {
                name: r.opt_string()?,
                depth: r.u32()?,
            });
        }
        let issues = read_strs(&mut r)?;
        let nodes = u64::from_le_bytes(
            r.bytes(8)?
                .try_into()
                .map_err(|_| corrupt("content node counter"))?,
        );
        if !r.at_end() {
            return Err(corrupt("ODT content model has trailing bytes"));
        }
        Ok(ContentModel {
            part_name,
            root_local,
            blocks,
            hyperlinks,
            bookmarks,
            notes,
            resources,
            sections,
            issues,
            nodes,
        })
    }
}

// ---------------------------------------------------------------------------
// Node parameters
// ---------------------------------------------------------------------------

/// Canonical parameters for an [`crate::field::node::NodeKind::OdtContent`] node:
/// `version(1) · member ordinal(4) · profile(7) · len-prefixed part name`.
pub fn content_params(ordinal: u32, part_name: &str, profile: &OdtExtractProfile) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + part_name.len());
    out.push(1);
    out.extend_from_slice(&ordinal.to_le_bytes());
    out.extend_from_slice(&profile.encode());
    put_str(&mut out, part_name);
    out
}

/// Decode parameters produced by [`content_params`].
pub fn read_content_params(params: &[u8]) -> Result<(u32, String, OdtExtractProfile)> {
    let mut r = BinReader::new(params);
    if r.u8()? != 1 {
        return Err(corrupt("unsupported ODT content params version"));
    }
    let ordinal = r.u32()?;
    let profile = OdtExtractProfile::decode(r.bytes(7)?)?;
    let part_name = r.string()?;
    if !r.at_end() {
        return Err(corrupt("ODT content params have trailing bytes"));
    }
    Ok((ordinal, part_name, profile))
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Build the canonical ODT discovery model from an exact ODF/ZIP source (the
/// derived [`crate::field::node::NodeKind::OdtModel`] computation).
///
/// Fails closed with a typed error when the package has no decodable ODF manifest
/// (an `.odt` always has one). The exact bytes remain recoverable regardless.
pub fn build_odt_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    let physical = scan(source, limits)?;
    physical.validate(source.len() as u64)?;
    let model = discover(source, &physical, limits)?;
    Ok(model.encode())
}

fn discover(source: &[u8], physical: &ZipPhysical, limits: Limits) -> Result<OdtModel> {
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

    // The ODF manifest is mandatory for an ODT package.
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
    // main content stream, ODF 1.2 §2.2.1), else a non-root entry declaring an
    // OpenDocument text media type. The extension is never the authority — the ODF
    // manifest is.
    let content = find_part(&manifest, &lookup, |e| e.full_path == CONTENT_MEMBER).or_else(|| {
        find_part(&manifest, &lookup, |e| {
            e.full_path != "/" && is_odt_text_media_type(&e.media_type)
        })
    });
    if content.is_none() {
        issues.push(
            "ODF manifest declares no OpenDocument text content part and no content.xml"
                .to_string(),
        );
    }
    let styles = find_part(&manifest, &lookup, |e| e.full_path == STYLES_MEMBER);
    let meta = find_part(&manifest, &lookup, |e| e.full_path == META_MEMBER);

    if mimetype.present && !mimetype.exact_bytes && !mimetype.conformant {
        issues.push(
            "mimetype member is present but not a conformant OpenDocument text declaration"
                .to_string(),
        );
    }

    Ok(OdtModel {
        mimetype,
        manifest,
        content,
        styles,
        meta,
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
    let exact_bytes = media_type.as_deref().is_some_and(is_odt_text_media_type);
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
            Event::DocType(_) => return Err(doctype_declined()),
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
    // The package-root entry ("/") declares the package media type; it is never a
    // member and is not an unresolved-member issue.
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
// content.xml parsing
// ---------------------------------------------------------------------------

/// The resolved kind of an ODF tracked-changes region.
const CHANGE_INSERTION: u8 = 0;
const CHANGE_DELETION: u8 = 1;
const CHANGE_FORMAT: u8 = 2;
const CHANGE_OTHER: u8 = 3;

/// Parse the OpenDocument main part into a canonical [`ContentModel`], honoring
/// `profile`. Only `content.xml` is ever parsed here; exact member bytes stay
/// authoritative.
pub fn parse_content(
    bytes: &[u8],
    part_name: &str,
    profile: &OdtExtractProfile,
    limits: Limits,
) -> Result<ContentModel> {
    harden_xml(bytes, limits)?;
    let changes = collect_changed_regions(bytes, limits)?;
    let mut p = Parser::new(profile, &changes);
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut saw_root = false;
    loop {
        let ev = reader.read_event().map_err(xml_err)?;
        p.nodes = p.nodes.saturating_add(1);
        if p.nodes > limits.max_xml_nodes {
            return Err(Error::resource_limit("ODT content exceeds max_xml_nodes"));
        }
        match ev {
            Event::Eof => break,
            Event::DocType(_) => return Err(doctype_declined()),
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
                p.on_close(local.as_bytes());
            }
            Event::Text(t) => {
                let s = t.into_inner();
                p.on_text(s.as_ref(), limits)?;
            }
            Event::GeneralRef(r) => {
                p.on_text(&entity_ref_text(r), limits)?;
            }
            _ => {}
        }
    }
    if !saw_root {
        return Err(Error::invalid_xml_structure("content.xml is empty"));
    }
    if !p.stack_closed() {
        return Err(Error::invalid_xml_structure(
            "content.xml is not well-formed (unclosed elements)",
        ));
    }
    Ok(p.finish(part_name))
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

/// A first pass collecting ODF tracked-changes regions: `change-id → (kind, text)`.
fn collect_changed_regions(xml: &[u8], limits: Limits) -> Result<BTreeMap<String, (u8, String)>> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut st = XmlState::new();
    let mut out: BTreeMap<String, (u8, String)> = BTreeMap::new();
    let mut depth: u32 = 0;
    let mut cur_id: Option<String> = None;
    let mut cur_kind: u8 = CHANGE_OTHER;
    let mut cur_text = String::new();
    let mut in_citation = 0u32;
    loop {
        let ev = reader.read_event().map_err(xml_err)?;
        st.event(limits)?;
        match ev {
            Event::Eof => break,
            Event::DocType(_) => return Err(doctype_declined()),
            Event::Start(e) => {
                st.open(limits)?;
                let local = e.name().local_name().as_ref().to_string();
                if depth == 0 && local == "changed-region" {
                    let attrs = read_attrs(&e, limits)?;
                    cur_id = attr_of(&attrs, "id").map(str::to_string);
                    cur_kind = CHANGE_OTHER;
                    cur_text.clear();
                    depth = 1;
                } else if depth > 0 {
                    if local == "insertion" {
                        cur_kind = CHANGE_INSERTION;
                    } else if local == "deletion" {
                        cur_kind = CHANGE_DELETION;
                    } else if local == "format-change" {
                        cur_kind = CHANGE_FORMAT;
                    } else if local == "note-citation" {
                        in_citation += 1;
                    }
                    depth += 1;
                }
            }
            Event::Empty(e) => {
                st.leaf(limits)?;
                let local = e.name().local_name().as_ref().to_string();
                if depth == 0 && local == "changed-region" {
                    let attrs = read_attrs(&e, limits)?;
                    if let Some(id) = attr_of(&attrs, "id") {
                        out.insert(id.to_string(), (CHANGE_OTHER, String::new()));
                    }
                } else if depth > 0 {
                    match local.as_str() {
                        "insertion" => cur_kind = CHANGE_INSERTION,
                        "deletion" => cur_kind = CHANGE_DELETION,
                        "format-change" => cur_kind = CHANGE_FORMAT,
                        _ => {}
                    }
                }
            }
            Event::End(e) => {
                st.close();
                let local = e.name().local_name().as_ref().to_string();
                if depth > 0 {
                    if local == "note-citation" {
                        in_citation = in_citation.saturating_sub(1);
                    }
                    depth -= 1;
                    if depth == 0
                        && let Some(id) = cur_id.take()
                    {
                        out.insert(id, (cur_kind, core::mem::take(&mut cur_text)));
                    }
                }
            }
            Event::Text(t) => {
                st.text(t.len(), limits)?;
                if depth > 0 && in_citation == 0 {
                    cur_text.push_str(t.into_inner().as_ref());
                }
            }
            Event::GeneralRef(r) if depth > 0 && in_citation == 0 => {
                cur_text.push_str(&entity_ref_text(r));
            }
            _ => {}
        }
    }
    Ok(out)
}

enum Cont {
    Body,
    Text,
    Para {
        para: ParaBuilder,
        heading: Option<u8>,
    },
    Span {
        run: RunBuilder,
        hidden: bool,
    },
    Link {
        link: LinkBuilder,
    },
    List {
        list: ListBuilder,
    },
    ListItem {
        item: ItemBuilder,
    },
    Table {
        table: TableBuilder,
    },
    Row {
        row: RowBuilder,
    },
    Cell {
        cell: CellBuilder,
        covered: bool,
    },
    Note,
    Section,
    Changes,
    Other,
}

struct ParaBuilder {
    text: String,
    style_id: Option<String>,
    runs: Vec<Run>,
}

struct RunBuilder {
    text: String,
    style_id: Option<String>,
}

struct LinkBuilder {
    href: String,
    external: bool,
    text: String,
}

struct ItemBuilder {
    paragraphs: Vec<String>,
    raw: String,
}

struct CellBuilder {
    col_span: u32,
    row_span: u32,
    paragraphs: Vec<String>,
    raw: String,
}

struct RowBuilder {
    cells: Vec<Cell>,
}

struct TableBuilder {
    rows: Vec<Row>,
}

struct ListBuilder {
    items: Vec<ListItem>,
}

struct Parser<'a> {
    profile: &'a OdtExtractProfile,
    changes: &'a BTreeMap<String, (u8, String)>,
    stack: Vec<Cont>,
    blocks: Vec<Block>,
    hyperlinks: Vec<Hyperlink>,
    bookmarks: Vec<String>,
    notes: Vec<Note>,
    resources: Vec<Resource>,
    sections: Vec<Section>,
    issues: Vec<String>,
    para_counter: u32,
    heading_counter: u32,
    table_counter: u32,
    list_counter: u32,
    item_counter: u32,
    active: Vec<u8>,
    hidden_depth: u32,
    in_note: bool,
    note: Option<Note>,
    note_text: String,
    in_note_citation: u32,
    section_depth: u32,
    root_local: String,
    nodes: u64,
}

impl<'a> Parser<'a> {
    fn new(profile: &'a OdtExtractProfile, changes: &'a BTreeMap<String, (u8, String)>) -> Self {
        Parser {
            profile,
            changes,
            stack: vec![Cont::Body],
            blocks: Vec::new(),
            hyperlinks: Vec::new(),
            bookmarks: Vec::new(),
            notes: Vec::new(),
            resources: Vec::new(),
            sections: Vec::new(),
            issues: Vec::new(),
            para_counter: 0,
            heading_counter: 0,
            table_counter: 0,
            list_counter: 0,
            item_counter: 0,
            active: Vec::new(),
            hidden_depth: 0,
            in_note: false,
            note: None,
            note_text: String::new(),
            in_note_citation: 0,
            section_depth: 0,
            root_local: String::new(),
            nodes: 0,
        }
    }

    fn stack_closed(&self) -> bool {
        self.stack.len() == 1
    }

    fn check_root(&mut self, e: &BytesStart<'_>) -> Result<()> {
        let local = e.name().local_name().as_ref().to_string();
        if local != "document-content" && local != "document" {
            return Err(Error::invalid_package_structure(format!(
                "ODT content root element <{local}> is not <office:document-content>"
            )));
        }
        self.root_local = local;
        Ok(())
    }

    fn in_text_body(&self) -> bool {
        self.stack.iter().any(|c| matches!(c, Cont::Text))
    }

    fn in_changes(&self) -> bool {
        self.stack.iter().any(|c| matches!(c, Cont::Changes))
    }

    fn in_table_cell(&self) -> bool {
        matches!(self.stack.last(), Some(Cont::Cell { .. }))
    }

    /// Whether text at the current position is included by the tracked-changes
    /// profile and the hidden-text policy.
    fn text_included(&self) -> bool {
        if self.hidden_depth > 0 && !self.profile.hidden {
            return false;
        }
        let mut deletion = false;
        let mut insertion = false;
        for k in &self.active {
            if *k == CHANGE_DELETION {
                deletion = true;
            } else if *k == CHANGE_INSERTION {
                insertion = true;
            }
        }
        if deletion {
            matches!(
                self.profile.tracked,
                OdtTrackedChanges::Original | OdtTrackedChanges::All
            )
        } else if insertion {
            matches!(
                self.profile.tracked,
                OdtTrackedChanges::Final | OdtTrackedChanges::All
            )
        } else {
            true
        }
    }

    fn on_text(&mut self, s: &str, _limits: Limits) -> Result<()> {
        if s.is_empty() || s.chars().all(char::is_whitespace) {
            return Ok(());
        }
        if !self.text_included() {
            return Ok(());
        }
        if self.in_note && self.in_note_citation == 0 {
            self.note_text.push_str(s);
            return Ok(());
        }
        if self.in_note {
            return Ok(());
        }
        if !self.in_text_body() || self.in_changes() {
            return Ok(());
        }
        self.push_text(s);
        Ok(())
    }

    fn push_text(&mut self, s: &str) {
        let mut span_done = false;
        let mut link_done = false;
        for c in self.stack.iter_mut().rev() {
            match c {
                Cont::Span { run, .. } if !span_done => {
                    run.text.push_str(s);
                    span_done = true;
                }
                Cont::Link { link } if !link_done => {
                    link.text.push_str(s);
                    link_done = true;
                }
                _ => {}
            }
        }
        for c in self.stack.iter_mut().rev() {
            match c {
                Cont::Para { para, .. } => {
                    para.text.push_str(s);
                    return;
                }
                Cont::ListItem { item } => {
                    item.raw.push_str(s);
                    return;
                }
                Cont::Cell { cell, .. } => {
                    cell.raw.push_str(s);
                    return;
                }
                _ => {}
            }
        }
    }

    fn on_open(&mut self, e: &BytesStart<'_>, limits: Limits, empty: bool) -> Result<()> {
        let local = e.name().local_name().as_ref().to_string();
        match local.as_str() {
            "text" => self.stack.push(Cont::Text),
            "tracked-changes" => self.stack.push(Cont::Changes),
            "p" | "h" => {
                if self.in_text_body() && !self.in_changes() && !self.in_note {
                    let attrs = read_attrs(e, limits)?;
                    let style = attr_of(&attrs, "style-name").map(str::to_string);
                    let heading = if local == "h" {
                        attr_of(&attrs, "outline-level")
                            .and_then(|v| v.parse::<u8>().ok())
                            .or(Some(1))
                    } else {
                        None
                    };
                    self.stack.push(Cont::Para {
                        para: ParaBuilder {
                            text: String::new(),
                            style_id: style,
                            runs: Vec::new(),
                        },
                        heading,
                    });
                } else {
                    self.stack.push(Cont::Other);
                }
            }
            "span" => {
                let attrs = read_attrs(e, limits)?;
                let style = attr_of(&attrs, "style-name").map(str::to_string);
                let hidden = attr_of(&attrs, "display") == Some("none");
                if hidden {
                    self.hidden_depth += 1;
                }
                self.stack.push(Cont::Span {
                    run: RunBuilder {
                        text: String::new(),
                        style_id: style,
                    },
                    hidden,
                });
            }
            "a" => {
                let attrs = read_attrs(e, limits)?;
                let href = attr_of(&attrs, "href").unwrap_or("").to_string();
                let external = is_absolute_uri(&href);
                self.stack.push(Cont::Link {
                    link: LinkBuilder {
                        href,
                        external,
                        text: String::new(),
                    },
                });
            }
            "list" => {
                if self.in_text_body() && !self.in_changes() {
                    self.stack.push(Cont::List {
                        list: ListBuilder { items: Vec::new() },
                    });
                } else {
                    self.stack.push(Cont::Other);
                }
            }
            "list-item" => {
                self.item_counter = 0;
                self.stack.push(Cont::ListItem {
                    item: ItemBuilder {
                        paragraphs: Vec::new(),
                        raw: String::new(),
                    },
                });
            }
            "table" => {
                if self.in_text_body() && !self.in_changes() {
                    self.stack.push(Cont::Table {
                        table: TableBuilder { rows: Vec::new() },
                    });
                } else {
                    self.stack.push(Cont::Other);
                }
            }
            "table-row" => self.stack.push(Cont::Row {
                row: RowBuilder { cells: Vec::new() },
            }),
            "table-cell" | "covered-table-cell" => {
                let attrs = read_attrs(e, limits)?;
                let col_span = attr_of(&attrs, "number-columns-spanned")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(1)
                    .max(1);
                let row_span = attr_of(&attrs, "number-rows-spanned")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(1)
                    .max(1);
                self.stack.push(Cont::Cell {
                    cell: CellBuilder {
                        col_span,
                        row_span,
                        paragraphs: Vec::new(),
                        raw: String::new(),
                    },
                    covered: local == "covered-table-cell",
                });
            }
            "note" => {
                let attrs = read_attrs(e, limits)?;
                let id = attr_of(&attrs, "id").unwrap_or("").to_string();
                let kind = attr_of(&attrs, "note-class")
                    .unwrap_or("footnote")
                    .to_string();
                self.in_note = true;
                self.note_text.clear();
                self.note = Some(Note {
                    id,
                    kind,
                    text: String::new(),
                });
                self.stack.push(Cont::Note);
            }
            "note-citation" => self.in_note_citation += 1,
            "section" => {
                let attrs = read_attrs(e, limits)?;
                let name = attr_of(&attrs, "name").map(str::to_string);
                self.section_depth = self.section_depth.saturating_add(1);
                self.sections.push(Section {
                    name,
                    depth: self.section_depth,
                });
                self.stack.push(Cont::Section);
            }
            "tab" => {
                if self.profile.tabs && self.in_text_body() && self.text_included() {
                    self.push_text("\t");
                }
                self.stack.push(Cont::Other);
            }
            "line-break" => {
                if self.profile.breaks && self.in_text_body() && self.text_included() {
                    self.push_text("\n");
                }
                self.stack.push(Cont::Other);
            }
            "s" => {
                let attrs = read_attrs(e, limits)?;
                let c = attr_of(&attrs, "c")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(1)
                    .min(1 << 16);
                if self.in_text_body() && self.text_included() {
                    let spaces: String = core::iter::repeat_n(' ', c as usize).collect();
                    self.push_text(&spaces);
                }
                self.stack.push(Cont::Other);
            }
            "bookmark" | "bookmark-start" => {
                let attrs = read_attrs(e, limits)?;
                if let Some(name) = attr_of(&attrs, "name") {
                    self.bookmarks.push(name.to_string());
                }
                self.stack.push(Cont::Other);
            }
            "image" => {
                let attrs = read_attrs(e, limits)?;
                if let Some(href) = attr_of(&attrs, "href") {
                    let external = is_absolute_uri(href);
                    let member = if external {
                        None
                    } else {
                        resolve_member_path(href, limits)
                    };
                    self.resources.push(Resource {
                        href: href.to_string(),
                        member,
                        external,
                    });
                }
                self.stack.push(Cont::Other);
            }
            "change-start" => {
                let attrs = read_attrs(e, limits)?;
                if let Some(id) = attr_of(&attrs, "change-id") {
                    let kind = self
                        .changes
                        .get(id)
                        .map(|(k, _)| *k)
                        .unwrap_or(CHANGE_OTHER);
                    self.active.push(kind);
                }
                self.stack.push(Cont::Other);
            }
            "change-end" => {
                self.active.pop();
                self.stack.push(Cont::Other);
            }
            "change" => {
                let attrs = read_attrs(e, limits)?;
                if let Some(id) = attr_of(&attrs, "change-id")
                    && let Some((kind, text)) = self.changes.get(id)
                {
                    let include = match *kind {
                        CHANGE_DELETION => matches!(
                            self.profile.tracked,
                            OdtTrackedChanges::Original | OdtTrackedChanges::All
                        ),
                        CHANGE_INSERTION => matches!(
                            self.profile.tracked,
                            OdtTrackedChanges::Final | OdtTrackedChanges::All
                        ),
                        _ => false,
                    };
                    if include && self.in_text_body() && self.text_included() {
                        let t = text.clone();
                        self.push_text(&t);
                    }
                }
                self.stack.push(Cont::Other);
            }
            _ => self.stack.push(Cont::Other),
        }
        if empty {
            // Emulate the matching End for self-closing elements.
            self.on_close(local.as_bytes());
        }
        Ok(())
    }

    fn on_close(&mut self, local: &[u8]) {
        // The root element's `End` must not pop the sentinel `Body` frame.
        if self.stack.len() <= 1 {
            return;
        }
        match local {
            b"span" => {
                let popped = self.stack.pop();
                if let Some(Cont::Span { run, hidden }) = popped {
                    if hidden {
                        self.hidden_depth = self.hidden_depth.saturating_sub(1);
                    }
                    if let Some(Cont::Para { para, .. }) = self
                        .stack
                        .iter_mut()
                        .rev()
                        .find(|c| matches!(c, Cont::Para { .. }))
                        && (!run.text.is_empty() || run.style_id.is_some())
                    {
                        let index = para.runs.len() as u32;
                        para.runs.push(Run {
                            index,
                            text: run.text,
                            style_id: run.style_id,
                        });
                    }
                }
            }
            b"p" | b"h" => self.close_para(),
            b"a" => {
                let popped = self.stack.pop();
                if let Some(Cont::Link { link }) = popped
                    && (!link.href.is_empty() || !link.text.is_empty())
                {
                    self.hyperlinks.push(Hyperlink {
                        text: link.text,
                        href: link.href,
                        external: link.external,
                    });
                }
            }
            b"list" => self.close_list(),
            b"list-item" => self.close_list_item(),
            b"table" => self.close_table(),
            b"table-row" => {
                if let Some(Cont::Row { row }) = self.stack.pop()
                    && let Some(Cont::Table { table }) = self.stack.last_mut()
                {
                    table.rows.push(Row { cells: row.cells });
                }
            }
            b"table-cell" | b"covered-table-cell" => self.close_cell(),
            b"note" => {
                if let Some(Cont::Note) = self.stack.pop() {
                    let text = core::mem::take(&mut self.note_text);
                    if let Some(mut note) = self.note.take() {
                        note.text = text;
                        let include = if note.kind == "endnote" {
                            self.profile.include_endnotes
                        } else {
                            self.profile.include_footnotes
                        };
                        if include {
                            self.notes.push(note);
                        }
                    }
                    self.in_note = false;
                }
            }
            b"note-citation" => self.in_note_citation = self.in_note_citation.saturating_sub(1),
            b"section" => {
                self.section_depth = self.section_depth.saturating_sub(1);
                if matches!(self.stack.last(), Some(Cont::Section)) {
                    self.stack.pop();
                }
            }
            b"tracked-changes" | b"text" => {
                self.stack.pop();
            }
            _ => {
                self.stack.pop();
            }
        }
    }

    fn close_para(&mut self) {
        let popped = self.stack.pop();
        let Some(Cont::Para { para, heading }) = popped else {
            return;
        };
        if self.in_note {
            self.note_text.push_str(&para.text);
            return;
        }
        if self.in_table_cell() {
            if let Some(Cont::Cell { cell, .. }) = self.stack.last_mut() {
                cell.paragraphs.push(para.text);
            }
            return;
        }
        if matches!(self.stack.last(), Some(Cont::ListItem { .. })) {
            if let Some(Cont::ListItem { item }) = self.stack.last_mut() {
                item.paragraphs.push(para.text);
            }
            return;
        }
        if !self.in_text_body() || self.in_changes() {
            return;
        }
        let index = match heading {
            Some(_) => {
                let i = self.heading_counter;
                self.heading_counter += 1;
                i
            }
            None => {
                let i = self.para_counter;
                self.para_counter += 1;
                i
            }
        };
        self.blocks.push(Block::Paragraph(Paragraph {
            index,
            text: para.text,
            style_id: para.style_id,
            heading_level: heading,
            runs: para.runs,
        }));
    }

    fn close_list(&mut self) {
        let Some(Cont::List { list }) = self.stack.pop() else {
            return;
        };
        if !self.in_text_body() || self.in_changes() {
            return;
        }
        let index = self.list_counter;
        self.list_counter += 1;
        self.blocks.push(Block::List(List {
            index,
            items: list.items,
        }));
    }

    fn close_list_item(&mut self) {
        let Some(Cont::ListItem { item }) = self.stack.pop() else {
            return;
        };
        let text = if item.paragraphs.is_empty() {
            item.raw
        } else {
            item.paragraphs.join("\n")
        };
        if let Some(Cont::List { list }) = self.stack.last_mut() {
            let index = list.items.len() as u32;
            list.items.push(ListItem { index, text });
        }
    }

    fn close_cell(&mut self) {
        let Some(Cont::Cell { cell, covered }) = self.stack.pop() else {
            return;
        };
        let text = if cell.paragraphs.is_empty() {
            cell.raw
        } else {
            cell.paragraphs.join("\n")
        };
        if let Some(Cont::Row { row }) = self.stack.last_mut() {
            let grid_col = row.cells.len() as u32;
            row.cells.push(Cell {
                grid_col,
                col_span: cell.col_span,
                row_span: cell.row_span,
                covered,
                text,
            });
        }
    }

    fn close_table(&mut self) {
        let Some(Cont::Table { table }) = self.stack.pop() else {
            return;
        };
        if !self.in_text_body() || self.in_changes() {
            return;
        }
        let index = self.table_counter;
        self.table_counter += 1;
        self.blocks.push(Block::Table(Table {
            index,
            rows: table.rows,
        }));
    }

    fn finish(mut self, part_name: &str) -> ContentModel {
        let mut issues = core::mem::take(&mut self.issues);
        if !self.active.is_empty() {
            issues.push("content.xml has an unterminated text:change-start region".to_string());
        }
        ContentModel {
            part_name: part_name.to_string(),
            root_local: if self.root_local.is_empty() {
                "document-content".to_string()
            } else {
                self.root_local
            },
            blocks: self.blocks,
            hyperlinks: self.hyperlinks,
            bookmarks: self.bookmarks,
            notes: self.notes,
            resources: self.resources,
            sections: self.sections,
            issues,
            nodes: self.nodes,
        }
    }
}

/// Whether a reference is an absolute URI (has an RFC 3986 scheme) or
/// protocol-relative. ODF hrefs in the body are inert strings, never fetched.
fn is_absolute_uri(href: &str) -> bool {
    if href.starts_with("//") {
        return true;
    }
    let b = href.as_bytes();
    if b.is_empty() || !b[0].is_ascii_alphabetic() {
        return false;
    }
    let mut i = 1;
    while i < b.len() {
        let c = b[i];
        if c == b':' {
            return true;
        }
        if !(c.is_ascii_alphanumeric() || c == b'+' || c == b'-' || c == b'.') {
            return false;
        }
        i += 1;
    }
    false
}

/// Resolve an ODF package-relative reference to a member name (clamped inside the
/// package root, `..` rejected). Never a host path.
fn resolve_member_path(target: &str, limits: Limits) -> Option<String> {
    if target.is_empty() || target.bytes().any(|b| b == 0 || b == b'\\' || b < 0x20) {
        return None;
    }
    let path = target.split(['#', '?']).next().unwrap_or("");
    if path.is_empty() {
        return None;
    }
    let mut segs: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                segs.pop()?;
            }
            other => segs.push(other),
        }
    }
    if segs.is_empty() {
        return None;
    }
    let resolved = segs.join("/");
    if resolved.len() as u64 > u64::from(limits.max_opc_part_name_bytes) {
        return None;
    }
    Some(resolved)
}

// ---------------------------------------------------------------------------
// Codec helpers
// ---------------------------------------------------------------------------

fn corrupt(msg: &str) -> Error {
    Error::invalid_package_structure(format!("corrupt ODT model: {msg}"))
}

fn bounded(r: &mut BinReader<'_>, what: &str) -> Result<u32> {
    let n = r.u32()?;
    if n as u64 > 1 << 24 {
        return Err(corrupt(&format!("ODT {what} count is implausible")));
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
        let p = OdtExtractProfile::DEFAULT;
        assert_eq!(OdtExtractProfile::decode(&p.encode()).unwrap(), p);
        assert!(p.fingerprint().contains("final"));
        let mut q = p;
        q.tracked = OdtTrackedChanges::Original;
        assert_ne!(q.encode(), p.encode());
        assert_eq!(OdtExtractProfile::decode(&q.encode()).unwrap(), q);
    }

    #[test]
    fn content_params_roundtrip() {
        let p = OdtExtractProfile::DEFAULT;
        let params = content_params(7, "content.xml", &p);
        let (o, n, prof) = read_content_params(&params).unwrap();
        assert_eq!(o, 7);
        assert_eq!(n, "content.xml");
        assert_eq!(prof, p);
    }

    #[test]
    fn model_roundtrip() {
        let m = OdtModel {
            mimetype: MimetypeFacts {
                present: true,
                first: true,
                stored: true,
                no_extra: true,
                exact_bytes: true,
                media_type: Some(ODT_MIMETYPE.to_string()),
                ordinal: 0,
                conformant: true,
            },
            manifest: vec![ManifestEntry {
                full_path: "content.xml".into(),
                media_type: ODT_MIMETYPE.into(),
                version: None,
                ordinal: 2,
            }],
            content: Some(PartRef {
                name: "content.xml".into(),
                ordinal: 2,
                media_type: Some(ODT_MIMETYPE.into()),
            }),
            styles: None,
            meta: None,
            issues: vec![],
        };
        assert_eq!(OdtModel::decode(&m.encode()).unwrap(), m);
    }

    #[test]
    fn absolute_uri_detection() {
        assert!(is_absolute_uri("https://example.com/x"));
        assert!(is_absolute_uri("//host/x"));
        assert!(!is_absolute_uri("Pictures/x.png"));
        assert!(!is_absolute_uri("#anchor"));
    }

    #[test]
    fn content_parses_tracked_changes_headings_and_tables() {
        let doc = concat!(
            r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" "#,
            r#"xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" "#,
            r#"xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0"><office:body><office:text>"#,
            r#"<text:h text:outline-level="2">Head</text:h>"#,
            r#"<text:p>a<text:change-start text:change-id="i"/>ins<text:change-end text:change-id="i"/>"#,
            r#"<text:change-start text:change-id="d"/>del<text:change-end text:change-id="d"/></text:p>"#,
            r#"<table:table><table:table-row><table:table-cell><text:p>c1</text:p></table:table-cell>"#,
            r#"</table:table-row></table:table>"#,
            r#"<text:tracked-changes>"#,
            r#"<text:changed-region text:id="i"><text:insertion/></text:changed-region>"#,
            r#"<text:changed-region text:id="d"><text:deletion/></text:changed-region>"#,
            r#"</text:tracked-changes>"#,
            r#"</office:text></office:body></office:document-content>"#,
        );
        let m = parse_content(
            doc.as_bytes(),
            "content.xml",
            &OdtExtractProfile::DEFAULT,
            Limits::DEFAULT,
        )
        .unwrap();
        assert_eq!(m.blocks.len(), 3);
        assert_eq!(m.headings().count(), 1);
        assert_eq!(m.headings().next().unwrap().heading_level, Some(2));
        // Final: insertions kept, deletions dropped.
        assert_eq!(m.text(), "Head\nains\nc1");
        assert_eq!(m.tables().count(), 1);
        assert_eq!(m.tables().next().unwrap().rows[0].cells[0].text, "c1");

        let mut original = OdtExtractProfile::DEFAULT;
        original.tracked = OdtTrackedChanges::Original;
        let mo = parse_content(doc.as_bytes(), "content.xml", &original, Limits::DEFAULT).unwrap();
        // Original: insertions dropped, deletions kept.
        assert_eq!(mo.text(), "Head\nadel\nc1");
    }

    #[test]
    fn wrong_root_is_declined() {
        let doc = br#"<wrong:thing xmlns:wrong="urn:x"/><wrong:thing/>"#;
        let e = parse_content(
            doc,
            "content.xml",
            &OdtExtractProfile::DEFAULT,
            Limits::DEFAULT,
        )
        .unwrap_err();
        assert_eq!(e.class(), crate::error::ErrorClass::InvalidPackageStructure);
    }
}
