//! OpenDocument Spreadsheet (ODS) adapter (Phase 21.3.1).
//!
//! An `.ods` is an **OpenDocument (ODF) package**: a ZIP archive whose first member
//! is the mandatory `stored` `mimetype`
//! (`application/vnd.oasis.opendocument.spreadsheet`), whose
//! `META-INF/manifest.xml` enumerates the package's files with their media types,
//! and whose main document part is the OpenDocument content stream (`content.xml`,
//! `office:document-content` → `office:body` → `office:spreadsheet`).
//!
//! ODF is *not* OPC: it has no `[Content_Types].xml` and no `_rels/.rels`
//! `officeDocument` relationship. Like ODT/EPUB, ODS therefore reuses the
//! byte-authoritative ZIP layer (ADR-0030) and the shared bounded-XML policy, but
//! discovers its main part **semantically** from the ODF manifest's declared media
//! type — never from a hardcoded `content.xml`. It shares the OPC *helpers* but does
//! not route through the OPC graph.
//!
//! Everything here is **derived (`Q_gen`) state**: exact authority remains the
//! Phase-12.2 ZIP member raw spans, so a hostile or malformed ODS is still an exact
//! archival object — only the *derived* observation declines, typed, and
//! `materialize(descriptor) == original_bytes` is untouched.
//!
//! Distinct cell facets are never conflated, mirroring the XLSX discipline: the
//! **stored formula** (`table:formula`, never evaluated), the **typed value**
//! (`office:value-type` + `office:value`/`office:boolean-value`/`office:date-value`/
//! `office:string-value`), the **displayed text** (`text:p`), the **cell style name**
//! (`table:style-name`), and the exact decoded-part byte span of the cell element
//! are separate fields.
//!
//! `table:number-columns-repeated`/`table:number-rows-repeated` can declare enormous
//! counts; the *expanded* cell grid is bounded by [`crate::limits::Limits::max_ods_cells`]
//! and a single repeat by [`crate::limits::Limits::max_ods_repeated_span`], declining
//! typed (resource limit) rather than allocating — the ODS analogue of the XLSX
//! coordinate bound (ADR-0059).

use std::collections::BTreeMap;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::adapter::package::xml::{
    XmlState, accept_doctype, attr_of, harden_xml, read_attrs, xml_err,
};
use crate::adapter::package::zip::{ZipMember, ZipPhysical, scan};
use crate::error::{Error, Result};
use crate::limits::Limits;

/// The mandatory ODF spreadsheet `mimetype` payload.
pub const ODS_MIMETYPE: &str = "application/vnd.oasis.opendocument.spreadsheet";
/// The additional accepted OpenDocument *spreadsheet* media type.
pub const ODS_MIMETYPE_TEMPLATE: &str = "application/vnd.oasis.opendocument.spreadsheet-template";
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

/// Version of the ODS extraction profile semantics.
pub const ODS_EXTRACT_PROFILE_VERSION: u32 = 1;

/// Whether a media type is an OpenDocument *spreadsheet* document type.
pub fn is_ods_spreadsheet_media_type(ct: &str) -> bool {
    matches!(ct, ODS_MIMETYPE | ODS_MIMETYPE_TEMPLATE)
}

/// Parse an A1-style (`B7`) or `row:col` (`6:1`, zero-based) cell position into a
/// 0-based `(col, row)`. Rejects a row/column beyond the bounded grid.
pub fn parse_cell_position(s: &str) -> Option<(u32, u32)> {
    if let Some((r, c)) = s.split_once(':')
        && let (Ok(row), Ok(col)) = (r.parse::<u32>(), c.parse::<u32>())
    {
        return Some((col, row));
    }
    a1_to_col_row(s)
}

/// Parse an A1-style cell reference (`B7`) into a 0-based grid column and a 0-based
/// row index. Column letters are case-insensitive; row numbers are 1-based.
pub fn a1_to_col_row(s: &str) -> Option<(u32, u32)> {
    let letters: String = s.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let digits: String = s.chars().skip(letters.len()).collect();
    if letters.is_empty() || digits.is_empty() || digits.len() != s.len() - letters.len() {
        return None;
    }
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut col: u32 = 0;
    for c in letters.chars() {
        let v = c.to_ascii_uppercase() as u32 - 'A' as u32 + 1;
        col = col.checked_mul(26)?.checked_add(v)?;
    }
    let col = col.checked_sub(1)?;
    let row: u32 = digits.parse().ok()?;
    if row == 0 {
        return None;
    }
    Some((col, row - 1))
}

// ---------------------------------------------------------------------------
// Extraction profile
// ---------------------------------------------------------------------------

/// A versioned, explicit OpenDocument spreadsheet-extraction profile. The profile
/// identity is recorded in every answer and hashed into the canonical selector, so
/// a projection is never hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OdsExtractProfile {
    /// Profile semantics version; must equal [`ODS_EXTRACT_PROFILE_VERSION`].
    pub version: u32,
    /// Include cell comments (`office:annotation`) in the derived model.
    pub comments: bool,
    /// Include hidden (`table:display="false"`) sheets in a whole-sheet projection.
    pub hidden: bool,
}

impl OdsExtractProfile {
    /// The declared default: comments included, hidden sheets excluded from
    /// whole-spreadsheet projections (but still addressable by sheet index).
    pub const DEFAULT: OdsExtractProfile = OdsExtractProfile {
        version: ODS_EXTRACT_PROFILE_VERSION,
        comments: true,
        hidden: false,
    };

    /// A stable, human-readable profile fingerprint used in canonical selectors.
    pub fn fingerprint(&self) -> String {
        format!(
            "v{}-c{}-h{}",
            self.version, self.comments as u8, self.hidden as u8
        )
    }

    /// The canonical 3-byte profile block.
    pub fn encode(&self) -> [u8; 3] {
        [self.version as u8, self.comments as u8, self.hidden as u8]
    }

    /// Decode a profile block produced by [`Self::encode`].
    pub fn decode(b: &[u8]) -> Result<OdsExtractProfile> {
        if b.len() != 3 {
            return Err(corrupt("ODS profile must be 3 bytes"));
        }
        if b[0] as u32 != ODS_EXTRACT_PROFILE_VERSION {
            return Err(Error::unsupported_version(format!(
                "ODS extraction profile version {} is not supported",
                b[0]
            )));
        }
        Ok(OdsExtractProfile {
            version: b[0] as u32,
            comments: b[1] != 0,
            hidden: b[2] != 0,
        })
    }
}

impl Default for OdsExtractProfile {
    fn default() -> Self {
        OdsExtractProfile::DEFAULT
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
    /// Its decoded payload is an OpenDocument spreadsheet media type with no extra
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

/// The canonical ODS discovery model (the derived state of the `OdsModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OdsModel {
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

impl OdsModel {
    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"ODSM");
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
    pub fn decode(bytes: &[u8]) -> Result<OdsModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"ODSM" {
            return Err(corrupt("bad ODS model magic"));
        }
        if r.u8()? != 1 {
            return Err(corrupt("unsupported ODS model version"));
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
            return Err(corrupt("ODS model has trailing bytes"));
        }
        Ok(OdsModel {
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
        _ => return Err(corrupt("bad ODS part tag")),
    })
}

// ---------------------------------------------------------------------------
// Content model
// ---------------------------------------------------------------------------

/// One `office:annotation` (cell comment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellAnnotation {
    /// `dc:creator`, when present.
    pub author: Option<String>,
    /// `dc:date`, when present.
    pub date: Option<String>,
    /// The comment text (paragraphs joined by `\n`).
    pub text: String,
}

/// One cell (`table:table-cell` / `table:covered-table-cell`), with every facet
/// kept distinct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// 0-based physical cell position within its (expanded) row.
    pub grid_col: u32,
    /// `table:number-columns-spanned` (>= 1).
    pub col_span: u32,
    /// `table:number-rows-spanned` (>= 1).
    pub row_span: u32,
    /// The cell element is `table:covered-table-cell` (a span continuation).
    pub covered: bool,
    /// `office:value-type`, when present.
    pub value_type: Option<String>,
    /// `office:value` (float/percentage/currency), when present.
    pub value: Option<String>,
    /// `office:boolean-value`, when present.
    pub boolean_value: Option<String>,
    /// `office:date-value`, when present.
    pub date_value: Option<String>,
    /// `office:string-value`, when present.
    pub string_value: Option<String>,
    /// `table:formula` (stored formula, never evaluated), when present.
    pub formula: Option<String>,
    /// `table:style-name` (the cell style), when present.
    pub style_name: Option<String>,
    /// Displayed text (the cell's `text:p` paragraphs joined by `\n`).
    pub text: String,
    /// Decoded-part byte offset of the cell element.
    pub span_start: u32,
    /// Decoded-part byte length of the cell element.
    pub span_len: u32,
    /// The cell's `office:annotation` comment, when present.
    pub annotation: Option<CellAnnotation>,
}

impl Cell {
    /// The typed value the cell declares: the value type plus the corresponding
    /// raw value attribute, or `None` when the cell is untyped/text-only.
    pub fn typed_value(&self) -> Option<(&str, &str)> {
        let ty = self.value_type.as_deref()?;
        let v = match ty {
            "boolean" => self.boolean_value.as_deref(),
            "date" | "time" => self.date_value.as_deref(),
            "string" => self.string_value.as_deref(),
            _ => self.value.as_deref().or(self.string_value.as_deref()),
        }?;
        Some((ty, v))
    }
}

/// One row (`table:table-row`), after repeated-row expansion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// 0-based row index within the sheet.
    pub index: u32,
    /// Cells, in physical order.
    pub cells: Vec<Cell>,
}

/// One sheet (`table:table`), after repeated-row/-cell expansion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sheet {
    /// 0-based sheet index among top-level tables.
    pub index: u32,
    /// `table:name` (never locale-normalized).
    pub name: String,
    /// `table:display` (`false` is a hidden sheet).
    pub display: bool,
    /// `table:style-name`, when present.
    pub style_name: Option<String>,
    /// Rows, in order.
    pub rows: Vec<Row>,
}

impl Sheet {
    /// The sheet's text: rows joined by `\n`, cells by `\t`.
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

    /// The cell at a 0-based `(row, col)` physical position, if present.
    pub fn cell_at(&self, row: u32, col: u32) -> Option<&Cell> {
        self.rows
            .iter()
            .find(|r| r.index == row)?
            .cells
            .iter()
            .find(|c| c.grid_col == col)
    }

    /// Total expanded cells across the sheet.
    pub fn cell_count(&self) -> u64 {
        self.rows.iter().map(|r| r.cells.len() as u64).sum()
    }
}

/// One named expression (`table:named-range` / `table:named-expression`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedExpression {
    /// `table:name`.
    pub name: String,
    /// `"range"` or `"expression"`.
    pub kind: String,
    /// `table:base-cell-address`, when present.
    pub base_cell_address: Option<String>,
    /// `table:cell-range-address` (a named range), when present.
    pub cell_range_address: Option<String>,
    /// `table:expression` (a named expression, never evaluated), when present.
    pub expression: Option<String>,
}

/// One `style:style` with `style:family="table-cell"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellStyle {
    /// `style:name`.
    pub name: String,
    /// `style:family` (always `table-cell` in this model).
    pub family: String,
    /// `style:parent-style-name`, when present.
    pub parent: Option<String>,
    /// `style:data-style-name` (a number-format reference), when present.
    pub data_style: Option<String>,
    /// `style:table-cell-properties` attributes, in written order.
    pub table_cell_properties: Vec<(String, String)>,
    /// `style:text-properties` attributes, in written order.
    pub text_properties: Vec<(String, String)>,
}

/// One declared number-format style (`number:number-style` et al.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumberFormat {
    /// `style:name`.
    pub name: String,
    /// The style kind (`number`, `date`, `time`, `currency`, `percentage`,
    /// `boolean`, `text`).
    pub kind: String,
}

/// One `office:annotation` (cell comment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    /// The 0-based sheet index the annotated cell belongs to.
    pub sheet: u32,
    /// The 0-based row index of the annotated cell.
    pub row: u32,
    /// The 0-based physical column of the annotated cell.
    pub col: u32,
    /// `dc:creator`, when present.
    pub author: Option<String>,
    /// `dc:date`, when present.
    pub date: Option<String>,
    /// The comment text (paragraphs joined by `\n`).
    pub text: String,
}

/// The canonical, derived content model of the OpenDocument spreadsheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentModel {
    /// The absolute part name the model was parsed from.
    pub part_name: String,
    /// The verified root element local name.
    pub root_local: String,
    /// Sheets, in document order.
    pub sheets: Vec<Sheet>,
    /// Cell styles (`office:automatic-styles`), in document order.
    pub styles: Vec<CellStyle>,
    /// Number-format styles, in document order.
    pub number_formats: Vec<NumberFormat>,
    /// Named expressions, in document order.
    pub named_expressions: Vec<NamedExpression>,
    /// Cell comments, in document order.
    pub comments: Vec<Comment>,
    /// Non-fatal parse observations.
    pub issues: Vec<String>,
    /// Element nodes scanned while building this model.
    pub nodes: u64,
}

impl ContentModel {
    /// The sheet at a 0-based index, if present.
    pub fn sheet(&self, index: u32) -> Option<&Sheet> {
        self.sheets.get(index as usize)
    }

    /// Total expanded cells across all sheets.
    pub fn cell_count(&self) -> u64 {
        self.sheets.iter().map(|s| s.cell_count()).sum()
    }

    /// The whole-spreadsheet text: projected sheets joined by `\n`. Hidden sheets
    /// are skipped unless `include_hidden`.
    pub fn text(&self, include_hidden: bool) -> String {
        self.sheets
            .iter()
            .filter(|s| include_hidden || s.display)
            .map(|s| s.text())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Deterministically encode the model.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"ODSC");
        out.push(1);
        put_str(&mut out, &self.part_name);
        put_str(&mut out, &self.root_local);

        put_u32(&mut out, self.sheets.len() as u32);
        for s in &self.sheets {
            put_u32(&mut out, s.index);
            put_str(&mut out, &s.name);
            out.push(s.display as u8);
            put_opt_str(&mut out, s.style_name.as_deref());
            put_u32(&mut out, s.rows.len() as u32);
            for row in &s.rows {
                put_u32(&mut out, row.index);
                put_u32(&mut out, row.cells.len() as u32);
                for c in &row.cells {
                    put_u32(&mut out, c.grid_col);
                    put_u32(&mut out, c.col_span);
                    put_u32(&mut out, c.row_span);
                    out.push(c.covered as u8);
                    put_opt_str(&mut out, c.value_type.as_deref());
                    put_opt_str(&mut out, c.value.as_deref());
                    put_opt_str(&mut out, c.boolean_value.as_deref());
                    put_opt_str(&mut out, c.date_value.as_deref());
                    put_opt_str(&mut out, c.string_value.as_deref());
                    put_opt_str(&mut out, c.formula.as_deref());
                    put_opt_str(&mut out, c.style_name.as_deref());
                    put_str(&mut out, &c.text);
                    put_u32(&mut out, c.span_start);
                    put_u32(&mut out, c.span_len);
                    match &c.annotation {
                        None => out.push(0),
                        Some(a) => {
                            out.push(1);
                            put_opt_str(&mut out, a.author.as_deref());
                            put_opt_str(&mut out, a.date.as_deref());
                            put_str(&mut out, &a.text);
                        }
                    }
                }
            }
        }

        put_styles(&mut out, &self.styles);
        put_u32(&mut out, self.number_formats.len() as u32);
        for f in &self.number_formats {
            put_str(&mut out, &f.name);
            put_str(&mut out, &f.kind);
        }

        put_u32(&mut out, self.named_expressions.len() as u32);
        for n in &self.named_expressions {
            put_str(&mut out, &n.name);
            put_str(&mut out, &n.kind);
            put_opt_str(&mut out, n.base_cell_address.as_deref());
            put_opt_str(&mut out, n.cell_range_address.as_deref());
            put_opt_str(&mut out, n.expression.as_deref());
        }

        put_u32(&mut out, self.comments.len() as u32);
        for c in &self.comments {
            put_u32(&mut out, c.sheet);
            put_u32(&mut out, c.row);
            put_u32(&mut out, c.col);
            put_opt_str(&mut out, c.author.as_deref());
            put_opt_str(&mut out, c.date.as_deref());
            put_str(&mut out, &c.text);
        }

        put_strs(&mut out, &self.issues);
        out.extend_from_slice(&self.nodes.to_le_bytes());
        out
    }

    /// Decode a model produced by [`Self::encode`].
    pub fn decode(bytes: &[u8]) -> Result<ContentModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"ODSC" {
            return Err(corrupt("bad ODS content model magic"));
        }
        if r.u8()? != 1 {
            return Err(corrupt("unsupported ODS content model version"));
        }
        let part_name = r.string()?;
        let root_local = r.string()?;

        let ns = bounded(&mut r, "sheet")?;
        let mut sheets = Vec::with_capacity(ns as usize);
        for _ in 0..ns {
            let index = r.u32()?;
            let name = r.string()?;
            let display = r.u8()? != 0;
            let style_name = r.opt_string()?;
            let nrows = bounded(&mut r, "sheet row")?;
            let mut rows = Vec::with_capacity(nrows as usize);
            for _ in 0..nrows {
                let row_index = r.u32()?;
                let ncells = bounded(&mut r, "row cell")?;
                let mut cells = Vec::with_capacity(ncells as usize);
                for _ in 0..ncells {
                    cells.push(Cell {
                        grid_col: r.u32()?,
                        col_span: r.u32()?,
                        row_span: r.u32()?,
                        covered: r.u8()? != 0,
                        value_type: r.opt_string()?,
                        value: r.opt_string()?,
                        boolean_value: r.opt_string()?,
                        date_value: r.opt_string()?,
                        string_value: r.opt_string()?,
                        formula: r.opt_string()?,
                        style_name: r.opt_string()?,
                        text: r.string()?,
                        span_start: r.u32()?,
                        span_len: r.u32()?,
                        annotation: match r.u8()? {
                            0 => None,
                            1 => Some(CellAnnotation {
                                author: r.opt_string()?,
                                date: r.opt_string()?,
                                text: r.string()?,
                            }),
                            _ => return Err(corrupt("bad ODS annotation tag")),
                        },
                    });
                }
                rows.push(Row {
                    index: row_index,
                    cells,
                });
            }
            sheets.push(Sheet {
                index,
                name,
                display,
                style_name,
                rows,
            });
        }

        let styles = read_styles(&mut r)?;
        let nf = bounded(&mut r, "number format")?;
        let mut number_formats = Vec::with_capacity(nf as usize);
        for _ in 0..nf {
            number_formats.push(NumberFormat {
                name: r.string()?,
                kind: r.string()?,
            });
        }

        let nn = bounded(&mut r, "named expression")?;
        let mut named_expressions = Vec::with_capacity(nn as usize);
        for _ in 0..nn {
            named_expressions.push(NamedExpression {
                name: r.string()?,
                kind: r.string()?,
                base_cell_address: r.opt_string()?,
                cell_range_address: r.opt_string()?,
                expression: r.opt_string()?,
            });
        }

        let nc = bounded(&mut r, "comment")?;
        let mut comments = Vec::with_capacity(nc as usize);
        for _ in 0..nc {
            comments.push(Comment {
                sheet: r.u32()?,
                row: r.u32()?,
                col: r.u32()?,
                author: r.opt_string()?,
                date: r.opt_string()?,
                text: r.string()?,
            });
        }

        let issues = read_strs(&mut r)?;
        let nodes = u64::from_le_bytes(
            r.bytes(8)?
                .try_into()
                .map_err(|_| corrupt("content node counter"))?,
        );
        if !r.at_end() {
            return Err(corrupt("ODS content model has trailing bytes"));
        }
        Ok(ContentModel {
            part_name,
            root_local,
            sheets,
            styles,
            number_formats,
            named_expressions,
            comments,
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
    /// Named cell styles (`office:styles` + `office:automatic-styles`).
    pub styles: Vec<CellStyle>,
    /// Number-format styles.
    pub number_formats: Vec<NumberFormat>,
    /// Non-fatal parse observations.
    pub issues: Vec<String>,
    /// Element nodes scanned while building this model.
    pub nodes: u64,
}

impl StylesModel {
    /// Deterministically encode the model.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"ODST");
        out.push(1);
        put_str(&mut out, &self.part_name);
        put_str(&mut out, &self.root_local);
        put_styles(&mut out, &self.styles);
        put_u32(&mut out, self.number_formats.len() as u32);
        for f in &self.number_formats {
            put_str(&mut out, &f.name);
            put_str(&mut out, &f.kind);
        }
        put_strs(&mut out, &self.issues);
        out.extend_from_slice(&self.nodes.to_le_bytes());
        out
    }

    /// Decode a model produced by [`Self::encode`].
    pub fn decode(bytes: &[u8]) -> Result<StylesModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"ODST" {
            return Err(corrupt("bad ODS styles model magic"));
        }
        if r.u8()? != 1 {
            return Err(corrupt("unsupported ODS styles model version"));
        }
        let part_name = r.string()?;
        let root_local = r.string()?;
        let styles = read_styles(&mut r)?;
        let nf = bounded(&mut r, "number format")?;
        let mut number_formats = Vec::with_capacity(nf as usize);
        for _ in 0..nf {
            number_formats.push(NumberFormat {
                name: r.string()?,
                kind: r.string()?,
            });
        }
        let issues = read_strs(&mut r)?;
        let nodes = u64::from_le_bytes(
            r.bytes(8)?
                .try_into()
                .map_err(|_| corrupt("styles node counter"))?,
        );
        if !r.at_end() {
            return Err(corrupt("ODS styles model has trailing bytes"));
        }
        Ok(StylesModel {
            part_name,
            root_local,
            styles,
            number_formats,
            issues,
            nodes,
        })
    }
}

fn put_attrs(out: &mut Vec<u8>, attrs: &[(String, String)]) {
    put_u32(out, attrs.len() as u32);
    for (k, v) in attrs {
        put_str(out, k);
        put_str(out, v);
    }
}

fn read_attr_pairs(r: &mut BinReader<'_>) -> Result<Vec<(String, String)>> {
    let n = bounded(r, "attribute")?;
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        out.push((r.string()?, r.string()?));
    }
    Ok(out)
}

fn put_styles(out: &mut Vec<u8>, styles: &[CellStyle]) {
    put_u32(out, styles.len() as u32);
    for s in styles {
        put_str(out, &s.name);
        put_str(out, &s.family);
        put_opt_str(out, s.parent.as_deref());
        put_opt_str(out, s.data_style.as_deref());
        put_attrs(out, &s.table_cell_properties);
        put_attrs(out, &s.text_properties);
    }
}

fn read_styles(r: &mut BinReader<'_>) -> Result<Vec<CellStyle>> {
    let n = bounded(r, "style")?;
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        out.push(CellStyle {
            name: r.string()?,
            family: r.string()?,
            parent: r.opt_string()?,
            data_style: r.opt_string()?,
            table_cell_properties: read_attr_pairs(r)?,
            text_properties: read_attr_pairs(r)?,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Node parameters
// ---------------------------------------------------------------------------

/// Canonical parameters for an [`crate::field::node::NodeKind::OdsContent`] node:
/// `version(1) · member ordinal(4) · profile(3) · len-prefixed part name`.
pub fn content_params(ordinal: u32, part_name: &str, profile: &OdsExtractProfile) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + part_name.len());
    out.push(1);
    out.extend_from_slice(&ordinal.to_le_bytes());
    out.extend_from_slice(&profile.encode());
    put_str(&mut out, part_name);
    out
}

/// Decode parameters produced by [`content_params`].
pub fn read_content_params(params: &[u8]) -> Result<(u32, String, OdsExtractProfile)> {
    let mut r = BinReader::new(params);
    if r.u8()? != 1 {
        return Err(corrupt("unsupported ODS content params version"));
    }
    let ordinal = r.u32()?;
    let profile = OdsExtractProfile::decode(r.bytes(3)?)?;
    let part_name = r.string()?;
    if !r.at_end() {
        return Err(corrupt("ODS content params have trailing bytes"));
    }
    Ok((ordinal, part_name, profile))
}

/// Canonical parameters for an [`crate::field::node::NodeKind::OdsStyles`] node:
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
        return Err(corrupt("unsupported ODS styles params version"));
    }
    let ordinal = r.u32()?;
    let part_name = r.string()?;
    if !r.at_end() {
        return Err(corrupt("ODS styles params have trailing bytes"));
    }
    Ok((ordinal, part_name))
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Build the canonical ODS discovery model from an exact ODF/ZIP source (the
/// derived [`crate::field::node::NodeKind::OdsModel`] computation).
///
/// Fails closed with a typed error when the package has no decodable ODF manifest
/// (an `.ods` always has one). The exact bytes remain recoverable regardless.
pub fn build_ods_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    let physical = scan(source, limits)?;
    physical.validate(source.len() as u64)?;
    let model = discover(source, &physical, limits)?;
    Ok(model.encode())
}

fn discover(source: &[u8], physical: &ZipPhysical, limits: Limits) -> Result<OdsModel> {
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

    // The ODF manifest is mandatory for an ODS package.
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
    // *spreadsheet* media type. The extension is never the authority.
    let content = find_part(&manifest, &lookup, |e| e.full_path == CONTENT_MEMBER).or_else(|| {
        find_part(&manifest, &lookup, |e| {
            e.full_path != "/" && is_ods_spreadsheet_media_type(&e.media_type)
        })
    });
    if content.is_none() {
        issues.push(
            "ODF manifest declares no OpenDocument spreadsheet content part and no content.xml"
                .to_string(),
        );
    }
    let styles = find_part(&manifest, &lookup, |e| e.full_path == STYLES_MEMBER);
    let meta = find_part(&manifest, &lookup, |e| e.full_path == META_MEMBER);

    if mimetype.present && !mimetype.exact_bytes && !mimetype.conformant {
        issues.push(
            "mimetype member is present but not a conformant OpenDocument spreadsheet declaration"
                .to_string(),
        );
    }

    Ok(OdsModel {
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
    let exact_bytes = media_type
        .as_deref()
        .is_some_and(is_ods_spreadsheet_media_type);
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

/// Parse the OpenDocument spreadsheet main part into a canonical [`ContentModel`],
/// honoring `profile`. Only `content.xml` is ever parsed here; exact member bytes
/// stay authoritative.
pub fn parse_content(
    bytes: &[u8],
    part_name: &str,
    profile: &OdsExtractProfile,
    limits: Limits,
) -> Result<ContentModel> {
    let p = parse_document(bytes, part_name, profile, limits, Mode::Content)?;
    Ok(p.content)
}

/// Parse an OpenDocument styles part (`styles.xml`) into a canonical
/// [`StylesModel`].
pub fn parse_styles(bytes: &[u8], part_name: &str, limits: Limits) -> Result<StylesModel> {
    let profile = OdsExtractProfile::DEFAULT;
    let p = parse_document(bytes, part_name, &profile, limits, Mode::Styles)?;
    Ok(StylesModel {
        part_name: p.content.part_name,
        root_local: p.content.root_local,
        styles: p.content.styles,
        number_formats: p.content.number_formats,
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

fn parse_document(
    bytes: &[u8],
    part_name: &str,
    profile: &OdsExtractProfile,
    limits: Limits,
    mode: Mode,
) -> Result<Parsed> {
    harden_xml(bytes, limits)?;
    let mut p = Parser::new(profile, mode, limits);
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut saw_root = false;
    loop {
        let pos_before = reader.buffer_position();
        let ev = reader.read_event().map_err(xml_err)?;
        p.nodes = p.nodes.saturating_add(1);
        if p.nodes > limits.max_xml_nodes {
            return Err(Error::resource_limit("ODS content exceeds max_xml_nodes"));
        }
        match ev {
            Event::Eof => break,
            Event::DocType(d) => accept_doctype(&d.into_inner())?,
            Event::Start(e) => {
                if !saw_root {
                    p.check_root(&e)?;
                    saw_root = true;
                }
                p.on_open(&e, limits, pos_before, false, 0)?;
            }
            Event::Empty(e) => {
                if !saw_root {
                    p.check_root(&e)?;
                    saw_root = true;
                }
                let pos_after = reader.buffer_position();
                p.on_open(&e, limits, pos_before, true, pos_after)?;
            }
            Event::End(e) => {
                let local = e.name().local_name().as_ref().to_string();
                p.on_close(local.as_bytes(), reader.buffer_position(), limits)?;
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
        return Err(Error::invalid_xml_structure("ODS content.xml is empty"));
    }
    if !p.stack_closed() {
        return Err(Error::invalid_xml_structure(
            "ODS content.xml is not well-formed (unclosed elements)",
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

const META_NONE: u8 = 0;
const META_CREATOR: u8 = 1;
const META_DATE: u8 = 2;

enum Cont {
    Body,
    AutoStyles,
    Spreadsheet,
    NamedExprs,
    Style(StyleBuilder),
    Table(TableBuilder),
    Row(RowBuilder),
    Cell(CellBuilder, bool),
    Para(ParaBuilder),
    Annotation(AnnotationBuilder),
    Other,
}

struct CellBuilder {
    repeat: u32,
    col_span: u32,
    row_span: u32,
    value_type: Option<String>,
    value: Option<String>,
    boolean_value: Option<String>,
    date_value: Option<String>,
    string_value: Option<String>,
    formula: Option<String>,
    style_name: Option<String>,
    paragraphs: Vec<String>,
    raw: String,
    span_start: u32,
    annotation: Option<AnnotationBuilder>,
}

struct RowBuilder {
    repeat: u32,
    cells: Vec<Cell>,
}

struct TableBuilder {
    name: String,
    display: bool,
    style_name: Option<String>,
    rows: Vec<Row>,
}

struct ParaBuilder {
    text: String,
}

struct AnnotationBuilder {
    author: Option<String>,
    date: Option<String>,
    paragraphs: Vec<String>,
    raw: String,
}

struct StyleBuilder {
    name: String,
    family: String,
    parent: Option<String>,
    data_style: Option<String>,
    table_cell_properties: Vec<(String, String)>,
    text_properties: Vec<(String, String)>,
}

struct Parser<'a> {
    profile: &'a OdsExtractProfile,
    mode: Mode,
    stack: Vec<Cont>,
    sheets: Vec<Sheet>,
    styles: Vec<CellStyle>,
    number_formats: Vec<NumberFormat>,
    named_expressions: Vec<NamedExpression>,
    comments: Vec<Comment>,
    issues: Vec<String>,
    root_local: String,
    expanded_grid: u64,
    merges: u32,
    meta_kind: u8,
    meta_depth: u32,
    meta_buf: String,
    limits: Limits,
    nodes: u64,
}

impl<'a> Parser<'a> {
    fn new(profile: &'a OdsExtractProfile, mode: Mode, limits: Limits) -> Self {
        Parser {
            profile,
            mode,
            stack: vec![Cont::Body],
            sheets: Vec::new(),
            styles: Vec::new(),
            number_formats: Vec::new(),
            named_expressions: Vec::new(),
            comments: Vec::new(),
            issues: Vec::new(),
            root_local: String::new(),
            expanded_grid: 0,
            merges: 0,
            meta_kind: META_NONE,
            meta_depth: 0,
            meta_buf: String::new(),
            limits,
            nodes: 0,
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
                "ODS content root element <{local}> is not <{expect}>"
            )));
        }
        self.root_local = local;
        Ok(())
    }

    fn in_spreadsheet(&self) -> bool {
        matches!(self.stack.last(), Some(Cont::Spreadsheet))
    }

    fn repeated_count(attrs: &[(String, String)], name: &str, limits: Limits) -> Result<u32> {
        let raw = attr_of(attrs, name);
        let n = raw.and_then(|v| v.parse::<u32>().ok()).unwrap_or(1);
        let n = n.max(1);
        if n > limits.max_ods_repeated_span {
            return Err(Error::resource_limit(format!(
                "{name} exceeds max_ods_repeated_span"
            )));
        }
        Ok(n)
    }

    fn on_text(&mut self, s: &str, _limits: Limits) -> Result<()> {
        if s.is_empty() || s.chars().all(char::is_whitespace) {
            return Ok(());
        }
        if self.meta_depth > 0 {
            self.meta_buf.push_str(s);
            return Ok(());
        }
        self.push_text(s);
        Ok(())
    }

    fn push_text(&mut self, s: &str) {
        for c in self.stack.iter_mut().rev() {
            match c {
                Cont::Para(p) => {
                    p.text.push_str(s);
                    return;
                }
                Cont::Annotation(a) => {
                    a.raw.push_str(s);
                    return;
                }
                Cont::Cell(cell, _) => {
                    cell.raw.push_str(s);
                    return;
                }
                _ => {}
            }
        }
    }

    fn nearest_style_mut(&mut self) -> Option<&mut StyleBuilder> {
        self.stack.iter_mut().rev().find_map(|c| match c {
            Cont::Style(s) => Some(s),
            _ => None,
        })
    }

    fn stash_props(&mut self, cell_props: bool, attrs: Vec<(String, String)>) {
        if let Some(s) = self.nearest_style_mut() {
            if cell_props {
                s.table_cell_properties = attrs;
            } else {
                s.text_properties = attrs;
            }
        }
    }

    fn on_open(
        &mut self,
        e: &BytesStart<'_>,
        limits: Limits,
        pos_before: u64,
        empty: bool,
        pos_after: u64,
    ) -> Result<()> {
        let local = e.name().local_name().as_ref().to_string();
        match local.as_str() {
            "automatic-styles" | "styles" => self.stack.push(Cont::AutoStyles),
            "style" => {
                let attrs = read_attrs(e, limits)?;
                let family = attr_of(&attrs, "family").unwrap_or("").to_string();
                if family == "table-cell" {
                    if self.styles.len() as u64 >= u64::from(limits.max_ods_styles) {
                        return Err(Error::resource_limit("styles exceed max_ods_styles"));
                    }
                    self.stack.push(Cont::Style(StyleBuilder {
                        name: attr_of(&attrs, "name").unwrap_or("").to_string(),
                        family,
                        parent: attr_of(&attrs, "parent-style-name").map(str::to_string),
                        data_style: attr_of(&attrs, "data-style-name").map(str::to_string),
                        table_cell_properties: Vec::new(),
                        text_properties: Vec::new(),
                    }));
                } else {
                    self.stack.push(Cont::Other);
                }
            }
            "table-cell-properties" => {
                let attrs = read_attrs(e, limits)?;
                self.stash_props(true, attrs);
                self.stack.push(Cont::Other);
            }
            "text-properties" => {
                let attrs = read_attrs(e, limits)?;
                self.stash_props(false, attrs);
                self.stack.push(Cont::Other);
            }
            "number-style" | "date-style" | "time-style" | "currency-style"
            | "percentage-style" | "boolean-style" | "text-style" => {
                let attrs = read_attrs(e, limits)?;
                if self.number_formats.len() as u64 >= u64::from(limits.max_ods_styles) {
                    return Err(Error::resource_limit(
                        "number formats exceed max_ods_styles",
                    ));
                }
                self.number_formats.push(NumberFormat {
                    name: attr_of(&attrs, "name").unwrap_or("").to_string(),
                    kind: local.trim_end_matches("-style").to_string(),
                });
                self.stack.push(Cont::Other);
            }
            "spreadsheet" if self.mode == Mode::Content => self.stack.push(Cont::Spreadsheet),
            "table" if self.in_spreadsheet() => {
                if self.sheets.len() as u64 >= u64::from(limits.max_ods_sheets) {
                    return Err(Error::resource_limit("spreadsheet exceeds max_ods_sheets"));
                }
                let attrs = read_attrs(e, limits)?;
                self.stack.push(Cont::Table(TableBuilder {
                    name: attr_of(&attrs, "name").unwrap_or("").to_string(),
                    display: attr_of(&attrs, "display") != Some("false"),
                    style_name: attr_of(&attrs, "style-name").map(str::to_string),
                    rows: Vec::new(),
                }));
            }
            "table-row" => {
                if let Some(Cont::Table(_)) = self.stack.last() {
                    let attrs = read_attrs(e, limits)?;
                    let repeat = Self::repeated_count(&attrs, "number-rows-repeated", limits)?;
                    self.stack.push(Cont::Row(RowBuilder {
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
                    if col_span > 1 || row_span > 1 {
                        self.merges = self.merges.saturating_add(1);
                        if self.merges as u64 > u64::from(limits.max_ods_merges) {
                            return Err(Error::resource_limit("merges exceed max_ods_merges"));
                        }
                    }
                    self.stack.push(Cont::Cell(
                        CellBuilder {
                            repeat,
                            col_span,
                            row_span,
                            value_type: attr_of(&attrs, "value-type").map(str::to_string),
                            value: attr_of(&attrs, "value").map(str::to_string),
                            boolean_value: attr_of(&attrs, "boolean-value").map(str::to_string),
                            date_value: attr_of(&attrs, "date-value").map(str::to_string),
                            string_value: attr_of(&attrs, "string-value").map(str::to_string),
                            formula: attr_of(&attrs, "formula").map(str::to_string),
                            style_name: attr_of(&attrs, "style-name").map(str::to_string),
                            paragraphs: Vec::new(),
                            raw: String::new(),
                            span_start: u32::try_from(pos_before).unwrap_or(0),
                            annotation: None,
                        },
                        local == "covered-table-cell",
                    ));
                } else {
                    self.stack.push(Cont::Other);
                }
            }
            "annotation" => {
                if self.in_cell() {
                    self.stack.push(Cont::Annotation(AnnotationBuilder {
                        author: None,
                        date: None,
                        paragraphs: Vec::new(),
                        raw: String::new(),
                    }));
                } else {
                    self.stack.push(Cont::Other);
                }
            }
            "creator" => {
                self.meta_kind = META_CREATOR;
                self.meta_depth = 1;
                self.meta_buf.clear();
                self.stack.push(Cont::Other);
            }
            "date" => {
                self.meta_kind = META_DATE;
                self.meta_depth = 1;
                self.meta_buf.clear();
                self.stack.push(Cont::Other);
            }
            "named-expressions" => self.stack.push(Cont::NamedExprs),
            "named-range" | "named-expression" => {
                if self.mode == Mode::Content {
                    if self.named_expressions.len() as u64
                        >= u64::from(limits.max_ods_named_expressions)
                    {
                        return Err(Error::resource_limit(
                            "named expressions exceed max_ods_named_expressions",
                        ));
                    }
                    let attrs = read_attrs(e, limits)?;
                    self.named_expressions.push(NamedExpression {
                        name: attr_of(&attrs, "name").unwrap_or("").to_string(),
                        kind: if local == "named-range" {
                            "range".to_string()
                        } else {
                            "expression".to_string()
                        },
                        base_cell_address: attr_of(&attrs, "base-cell-address").map(str::to_string),
                        cell_range_address: attr_of(&attrs, "cell-range-address")
                            .map(str::to_string),
                        expression: attr_of(&attrs, "expression").map(str::to_string),
                    });
                }
                self.stack.push(Cont::Other);
            }
            "p" | "h" => {
                if self.meta_depth == 0 {
                    self.stack.push(Cont::Para(ParaBuilder {
                        text: String::new(),
                    }));
                } else {
                    self.stack.push(Cont::Other);
                }
            }
            "s" => {
                let attrs = read_attrs(e, limits)?;
                let c = attr_of(&attrs, "c")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(1)
                    .min(1 << 16);
                if self.meta_depth == 0 {
                    let spaces: String = core::iter::repeat_n(' ', c as usize).collect();
                    self.push_text(&spaces);
                }
                self.stack.push(Cont::Other);
            }
            "tab" => {
                if self.meta_depth == 0 {
                    self.push_text("\t");
                }
                self.stack.push(Cont::Other);
            }
            "line-break" => {
                if self.meta_depth == 0 {
                    self.push_text("\n");
                }
                self.stack.push(Cont::Other);
            }
            _ => self.stack.push(Cont::Other),
        }
        if empty {
            self.close_element(local.as_bytes(), pos_after, limits)?;
        }
        Ok(())
    }

    fn in_cell(&self) -> bool {
        self.stack.iter().any(|c| matches!(c, Cont::Cell(..)))
    }

    fn on_close(&mut self, local: &[u8], pos_after: u64, limits: Limits) -> Result<()> {
        self.close_element(local, pos_after, limits)
    }

    fn close_element(&mut self, local: &[u8], pos_after: u64, limits: Limits) -> Result<()> {
        match local {
            b"table" => {
                if let Some(Cont::Table(tb)) = self.stack.pop() {
                    let index = self.sheets.len() as u32;
                    self.sheets.push(Sheet {
                        index,
                        name: tb.name,
                        display: tb.display,
                        style_name: tb.style_name,
                        rows: tb.rows,
                    });
                }
            }
            b"table-row" => {
                if let Some(Cont::Row(rb)) = self.stack.pop() {
                    // The cell copies were already charged at cell close; charge the
                    // *additional* copies from `table:number-rows-repeated` here. An
                    // over-large repeat declines typed *before* any row is cloned.
                    let extra =
                        (rb.repeat as u64 - 1).saturating_mul((rb.cells.len() as u64).max(1));
                    let new_grid = self.expanded_grid.saturating_add(extra);
                    if new_grid > limits.max_ods_cells {
                        return Err(Error::resource_limit(
                            "spreadsheet exceeds max_ods_cells after repeated-row expansion",
                        ));
                    }
                    self.expanded_grid = new_grid;
                    if let Some(Cont::Table(tb)) = self.stack.last_mut() {
                        for _ in 0..rb.repeat {
                            let index = tb.rows.len() as u32;
                            tb.rows.push(Row {
                                index,
                                cells: rb.cells.clone(),
                            });
                        }
                    }
                }
            }
            b"table-cell" | b"covered-table-cell" => {
                if let Some(Cont::Cell(cb, covered)) = self.stack.pop() {
                    let new_grid = self.expanded_grid.saturating_add(cb.repeat as u64);
                    if new_grid > limits.max_ods_cells {
                        return Err(Error::resource_limit(
                            "spreadsheet exceeds max_ods_cells after repeated-cell expansion",
                        ));
                    }
                    self.expanded_grid = new_grid;
                    let span_len =
                        u32::try_from(pos_after.saturating_sub(cb.span_start as u64)).unwrap_or(0);
                    if let Some(Cont::Row(rb)) = self.stack.last_mut() {
                        for _ in 0..cb.repeat {
                            let grid_col = rb.cells.len() as u32;
                            rb.cells.push(finish_cell(&cb, covered, grid_col, span_len));
                        }
                    }
                }
            }
            b"p" | b"h" => {
                if let Some(Cont::Para(pb)) = self.stack.pop() {
                    if let Some(Cont::Annotation(a)) = self.stack.last_mut() {
                        a.paragraphs.push(pb.text);
                    } else if let Some(Cont::Cell(cell, _)) = self.stack.last_mut() {
                        cell.paragraphs.push(pb.text);
                    }
                }
            }
            b"annotation" => {
                if let Some(Cont::Annotation(a)) = self.stack.pop()
                    && let Some(Cont::Cell(cell, _)) = self.stack.last_mut()
                {
                    cell.annotation = Some(a);
                }
            }
            b"creator" | b"date" => {
                // The `creator`/`date` frame was pushed at open; pop it first so the
                // enclosing annotation is the stack top when we assign.
                self.stack.pop();
                if self.meta_depth > 0 {
                    self.meta_depth = 0;
                    let buf = core::mem::take(&mut self.meta_buf);
                    if let Some(Cont::Annotation(a)) = self.stack.last_mut() {
                        match self.meta_kind {
                            META_CREATOR => a.author = Some(buf),
                            META_DATE => a.date = Some(buf),
                            _ => {}
                        }
                    }
                    self.meta_kind = META_NONE;
                }
            }
            b"style" => {
                if let Some(Cont::Style(sb)) = self.stack.pop() {
                    self.styles.push(CellStyle {
                        name: sb.name,
                        family: sb.family,
                        parent: sb.parent,
                        data_style: sb.data_style,
                        table_cell_properties: sb.table_cell_properties,
                        text_properties: sb.text_properties,
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
        // Materialize comments from the cells' annotations (bounded, in order).
        if self.profile.comments {
            let bound = u64::from(self.limits.max_ods_comments);
            let mut comments: Vec<Comment> = Vec::new();
            'outer: for sheet in &self.sheets {
                for row in &sheet.rows {
                    for cell in &row.cells {
                        if comments.len() as u64 >= bound {
                            self.issues
                                .push("comments exceed max_ods_comments".to_string());
                            break 'outer;
                        }
                        if let Some(a) = &cell.annotation {
                            comments.push(Comment {
                                sheet: sheet.index,
                                row: row.index,
                                col: cell.grid_col,
                                author: a.author.clone(),
                                date: a.date.clone(),
                                text: a.text.clone(),
                            });
                        }
                    }
                }
            }
            self.comments = comments;
        }
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
            sheets: self.sheets,
            styles: self.styles,
            number_formats: self.number_formats,
            named_expressions: self.named_expressions,
            comments: self.comments,
            issues,
            nodes: self.nodes,
        }
    }
}

fn finish_cell(cb: &CellBuilder, covered: bool, grid_col: u32, span_len: u32) -> Cell {
    let text = if cb.paragraphs.is_empty() {
        cb.raw.clone()
    } else {
        cb.paragraphs.join("\n")
    };
    Cell {
        grid_col,
        col_span: cb.col_span,
        row_span: cb.row_span,
        covered,
        value_type: cb.value_type.clone(),
        value: cb.value.clone(),
        boolean_value: cb.boolean_value.clone(),
        date_value: cb.date_value.clone(),
        string_value: cb.string_value.clone(),
        formula: cb.formula.clone(),
        style_name: cb.style_name.clone(),
        text,
        span_start: cb.span_start,
        span_len,
        annotation: cb.annotation.as_ref().map(|a| CellAnnotation {
            author: a.author.clone(),
            date: a.date.clone(),
            text: if a.paragraphs.is_empty() {
                a.raw.clone()
            } else {
                a.paragraphs.join("\n")
            },
        }),
    }
}

// ---------------------------------------------------------------------------
// Codec helpers
// ---------------------------------------------------------------------------

fn corrupt(msg: &str) -> Error {
    Error::invalid_package_structure(format!("corrupt ODS model: {msg}"))
}

fn bounded(r: &mut BinReader<'_>, what: &str) -> Result<u32> {
    let n = r.u32()?;
    if n as u64 > 1 << 24 {
        return Err(corrupt(&format!("ODS {what} count is implausible")));
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
        let p = OdsExtractProfile::DEFAULT;
        assert_eq!(OdsExtractProfile::decode(&p.encode()).unwrap(), p);
        assert!(p.fingerprint().contains("c1"));
        let mut q = p;
        q.comments = false;
        q.hidden = true;
        assert_ne!(q.encode(), p.encode());
        assert_eq!(OdsExtractProfile::decode(&q.encode()).unwrap(), q);
    }

    #[test]
    fn params_roundtrip() {
        let p = OdsExtractProfile::DEFAULT;
        let params = content_params(7, "content.xml", &p);
        let (o, n, prof) = read_content_params(&params).unwrap();
        assert_eq!((o, n.as_str()), (7, "content.xml"));
        assert_eq!(prof, p);
        let sp = styles_params(3, "styles.xml");
        assert_eq!(
            read_styles_params(&sp).unwrap(),
            (3, "styles.xml".to_string())
        );
    }

    #[test]
    fn model_roundtrip() {
        let m = OdsModel {
            mimetype: MimetypeFacts {
                present: true,
                first: true,
                stored: true,
                no_extra: true,
                exact_bytes: true,
                media_type: Some(ODS_MIMETYPE.to_string()),
                ordinal: 0,
                conformant: true,
            },
            manifest: vec![ManifestEntry {
                full_path: "content.xml".into(),
                media_type: "text/xml".into(),
                version: None,
                ordinal: 2,
            }],
            content: Some(PartRef {
                name: "content.xml".into(),
                ordinal: 2,
                media_type: Some("text/xml".into()),
            }),
            styles: None,
            meta: None,
            issues: vec![],
        };
        assert_eq!(OdsModel::decode(&m.encode()).unwrap(), m);
    }

    #[test]
    fn cell_reference_parsing() {
        assert_eq!(a1_to_col_row("B7"), Some((1, 6)));
        assert_eq!(a1_to_col_row("A1"), Some((0, 0)));
        assert_eq!(parse_cell_position("6:1"), Some((1, 6)));
        assert_eq!(a1_to_col_row("7B"), None);
    }

    #[test]
    fn content_parses_sheets_cells_and_formulas() {
        let doc = concat!(
            r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" "#,
            r#"xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" "#,
            r#"xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" "#,
            r#"xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"><office:automatic-styles>"#,
            r#"<style:style style:name="ce1" style:family="table-cell"><style:table-cell-properties style:vertical-align="middle"/></style:style>"#,
            r#"</office:automatic-styles><office:body><office:spreadsheet>"#,
            r#"<table:table table:name="S1"><table:table-row>"#,
            r#"<table:table-cell office:value-type="float" office:value="1.5" table:style-name="ce1"><text:p>1.5</text:p></table:table-cell>"#,
            r#"<table:table-cell table:number-columns-repeated="2"><text:p>x</text:p></table:table-cell>"#,
            r#"<table:table-cell table:formula="of:=SUM(A1:A1)" office:value-type="float" office:value="3"><text:p>3</text:p></table:table-cell>"#,
            r#"</table:table-row>"#,
            r#"<table:table-row table:number-rows-repeated="2"><table:table-cell table:number-columns-spanned="2"><text:p>m</text:p></table:table-cell></table:table-row>"#,
            r#"</table:table>"#,
            r#"<table:table table:name="Hidden" table:display="false"><table:table-row><table:table-cell><text:p>h</text:p></table:table-cell></table:table-row></table:table>"#,
            r#"<table:named-expressions>"#,
            r#"<table:named-range table:name="R1" table:cell-range-address="S1.A1:S1.A1"/>"#,
            r#"<table:named-expression table:name="E1" table:expression="of:=1+1"/>"#,
            r#"</table:named-expressions>"#,
            r#"</office:spreadsheet></office:body></office:document-content>"#,
        );
        let m = parse_content(
            doc.as_bytes(),
            "content.xml",
            &OdsExtractProfile::DEFAULT,
            Limits::DEFAULT,
        )
        .unwrap();
        assert_eq!(m.sheets.len(), 2);
        assert_eq!(m.sheets[0].name, "S1");
        let s0 = &m.sheets[0];
        // Row 0: A1 + 2 repeated + formula = 4 cells.
        assert_eq!(s0.rows.len(), 3); // 1 + 2 repeated
        assert_eq!(s0.rows[0].cells.len(), 4);
        assert_eq!(s0.rows[0].cells[0].text, "1.5");
        assert_eq!(s0.rows[0].cells[0].typed_value(), Some(("float", "1.5")));
        assert_eq!(s0.rows[0].cells[0].style_name.as_deref(), Some("ce1"));
        assert_eq!(
            s0.rows[0].cells[3].formula.as_deref(),
            Some("of:=SUM(A1:A1)")
        );
        assert_eq!(s0.rows[0].cells[3].value.as_deref(), Some("3"));
        assert!(s0.rows[1].cells[0].col_span == 2);
        assert!(!m.sheets[1].display);
        assert_eq!(m.styles.len(), 1);
        assert_eq!(m.styles[0].name, "ce1");
        assert_eq!(m.named_expressions.len(), 2);
        assert_eq!(m.named_expressions[0].kind, "range");
        assert_eq!(m.named_expressions[1].kind, "expression");
        // Hidden sheet excluded by default; included under a hidden-inclusive profile.
        assert!(!m.text(false).contains('h'));
        assert!(m.text(true).contains('h'));
    }

    #[test]
    fn repeated_row_bomb_declines() {
        // A row repeated far beyond any plausible bound must fail closed, not
        // allocate. The bound is enforced at the repeated-span declaration.
        let doc = concat!(
            r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" "#,
            r#"xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" "#,
            r#"xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0"><office:body><office:spreadsheet>"#,
            r#"<table:table table:name="B"><table:table-row table:number-rows-repeated="4000000">"#,
            r#"<table:table-cell table:number-columns-repeated="1048576"><text:p>b</text:p></table:table-cell>"#,
            r#"</table:table-row></table:table>"#,
            r#"</office:spreadsheet></office:body></office:document-content>"#,
        );
        let e = parse_content(
            doc.as_bytes(),
            "content.xml",
            &OdsExtractProfile::DEFAULT,
            Limits::STRICT,
        )
        .unwrap_err();
        assert_eq!(e.class(), crate::error::ErrorClass::ResourceLimit);
    }

    #[test]
    fn wrong_root_is_declined() {
        let doc = br#"<wrong:thing xmlns:wrong="urn:x"/><wrong:thing/>"#;
        let e = parse_content(
            doc,
            "content.xml",
            &OdsExtractProfile::DEFAULT,
            Limits::DEFAULT,
        )
        .unwrap_err();
        assert_eq!(e.class(), crate::error::ErrorClass::InvalidPackageStructure);
    }
}
