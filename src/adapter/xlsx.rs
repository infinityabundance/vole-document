//! SpreadsheetML (XLSX) adapter (Phase 21.1.1, the format programme's first
//! subphase).
//!
//! An `.xlsx` is an **OPC package** on the ZIP layer (Phase 12.2/12.3): the
//! workbook is identified **semantically** by the package `officeDocument`
//! relationship and its SpreadsheetML content type — never by a hardcoded
//! `/xl/workbook.xml`. This module adds the SpreadsheetML semantics on top of the
//! generic OPC core:
//!
//! * **workbook discovery** (relationship → content type → later root element),
//! * a **declared versioned extraction profile** (cached result vs stored formula;
//!   whether hidden sheets join a whole-workbook projection),
//! * **sheet discovery** by workbook relationship (r:id → sheet part), and
//! * a bounded SpreadsheetML subset parsed into canonical, derived
//!   ([`SheetModel`]/[`WorkbookModel`]) serializations.
//!
//! Everything here is **derived (`Q_gen`) state**: exact authority stays with the
//! Phase-12.2 ZIP member raw spans, and the exact original package still
//! materializes byte-identically regardless of any decline. Parsing never
//! evaluates a formula: a cell's *stored formula* (`<f>`), its *cached result*
//! (`<v>`), and its *underlying XML span* are three distinct observations and are
//! never conflated.

use std::collections::BTreeMap;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::adapter::package::opc::{
    OFFICE_DOCUMENT_REL, OFFICE_DOCUMENT_REL_STRICT, OpcModel, OpcPart,
};
use crate::adapter::package::xml::{
    XmlState, accept_doctype, attr_of, harden_xml, read_attrs, xml_err,
};
use crate::error::{Error, Result};
use crate::limits::Limits;

/// Version of the XLSX extraction profile semantics.
pub const XLSX_EXTRACT_PROFILE_VERSION: u32 = 1;

/// The SpreadsheetML workbook main content type (transitional and macro forms).
fn is_workbook_content_type(ct: &str) -> bool {
    matches!(
        ct,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"
            | "application/vnd.openxmlformats-officedocument.spreadsheetml.template.main+xml"
            | "application/vnd.ms-excel.sheet.macroEnabled.main+xml"
            | "application/vnd.ms-excel.template.macroEnabledTemplate.main+xml"
    )
}

/// The relationship type suffix (`…/<suffix>`) of a SpreadsheetML part.
fn rel_matches(rel_type: &str, suffix: &str) -> bool {
    rel_type == suffix || rel_type.ends_with(&format!("/{suffix}"))
}

fn workbook_relationship_types() -> [&'static str; 2] {
    [OFFICE_DOCUMENT_REL, OFFICE_DOCUMENT_REL_STRICT]
}

// ---------------------------------------------------------------------------
// Extraction profile
// ---------------------------------------------------------------------------

/// Which cell facet a text/common projection reads. The stored formula and the
/// cached result are distinct observations and are never conflated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueMode {
    /// The cached result (`<v>`), the last value the producer computed.
    Cached,
    /// The stored formula text (`<f>`), when present.
    Formula,
}

impl ValueMode {
    /// Stable lower-case name.
    pub const fn name(self) -> &'static str {
        match self {
            ValueMode::Cached => "cached",
            ValueMode::Formula => "formula",
        }
    }
}

/// A versioned, explicit spreadsheet-projection profile. The profile identity is
/// recorded in every answer (and hashed into the canonical selector), so a
/// projection is never hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XlsxExtractProfile {
    /// Profile semantics version; must equal [`XLSX_EXTRACT_PROFILE_VERSION`].
    pub version: u32,
    /// Which cell facet a text/common projection reads.
    pub values: ValueMode,
    /// Include hidden/very-hidden sheets in a whole-workbook projection.
    pub include_hidden: bool,
}

impl XlsxExtractProfile {
    /// The declared default profile: cached results, hidden sheets excluded from
    /// whole-workbook projections (but still addressable by sheet index).
    pub const DEFAULT: XlsxExtractProfile = XlsxExtractProfile {
        version: XLSX_EXTRACT_PROFILE_VERSION,
        values: ValueMode::Cached,
        include_hidden: false,
    };

    /// A stable, human-readable profile fingerprint used in canonical selectors.
    pub fn fingerprint(&self) -> String {
        format!(
            "v{}-{}-h{}",
            self.version,
            self.values.name(),
            self.include_hidden as u8
        )
    }

    /// The canonical 3-byte profile block.
    pub fn encode(&self) -> [u8; 3] {
        [
            self.version as u8,
            match self.values {
                ValueMode::Cached => 0,
                ValueMode::Formula => 1,
            },
            self.include_hidden as u8,
        ]
    }

    /// Decode a profile block produced by [`Self::encode`].
    pub fn decode(b: &[u8]) -> Result<XlsxExtractProfile> {
        if b.len() != 3 {
            return Err(corrupt("XLSX profile must be 3 bytes"));
        }
        if b[0] as u32 != XLSX_EXTRACT_PROFILE_VERSION {
            return Err(Error::unsupported_version(format!(
                "XLSX extraction profile version {} is not supported",
                b[0]
            )));
        }
        let values = match b[1] {
            0 => ValueMode::Cached,
            1 => ValueMode::Formula,
            _ => return Err(corrupt("bad value-mode selector")),
        };
        Ok(XlsxExtractProfile {
            version: b[0] as u32,
            values,
            include_hidden: b[2] != 0,
        })
    }
}

impl Default for XlsxExtractProfile {
    fn default() -> Self {
        XlsxExtractProfile::DEFAULT
    }
}

// ---------------------------------------------------------------------------
// Discovery model
// ---------------------------------------------------------------------------

/// A part that backs a workbook object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XlsxPartRef {
    /// Absolute OPC part name.
    pub name: String,
    /// Physical member ordinal.
    pub ordinal: u32,
    /// Content type, when known.
    pub content_type: Option<String>,
}

/// One worksheet part discovered through the workbook relationships.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XlsxSheetRef {
    /// The workbook relationship id (`r:id`) when discovered by relationship.
    pub rel_id: Option<String>,
    /// Deterministic discovery order (relationship-id order, or part-name order).
    pub order: u32,
    /// The backing worksheet part.
    pub part: XlsxPartRef,
}

/// The canonical XLSX discovery model: the workbook part, the styles and shared
/// strings parts, and the discovered worksheets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XlsxModel {
    /// The workbook part (resolved via the `officeDocument` relationship).
    pub workbook: XlsxPartRef,
    /// The styles part, when present.
    pub styles: Option<XlsxPartRef>,
    /// The shared-strings part, when present.
    pub shared_strings: Option<XlsxPartRef>,
    /// Discovered worksheet parts, sorted by `(order, part name)`.
    pub sheets: Vec<XlsxSheetRef>,
}

impl XlsxModel {
    /// Encode the discovery model canonically.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"XLSM");
        out.push(1);
        put_part(&mut out, &self.workbook);
        put_opt_part(&mut out, self.styles.as_ref());
        put_opt_part(&mut out, self.shared_strings.as_ref());
        put_u32(&mut out, self.sheets.len() as u32);
        for s in &self.sheets {
            put_opt_str(&mut out, s.rel_id.as_deref());
            put_u32(&mut out, s.order);
            put_part(&mut out, &s.part);
        }
        out
    }

    /// Decode a model produced by [`XlsxModel::encode`].
    pub fn decode(bytes: &[u8]) -> Result<XlsxModel> {
        let mut r = ByteReader::new(bytes);
        if r.bytes(4)? != b"XLSM" {
            return Err(corrupt("bad XLSX model magic"));
        }
        if r.u8()? != 1 {
            return Err(corrupt("unsupported XLSX model version"));
        }
        let workbook = read_part(&mut r)?;
        let styles = read_opt_part(&mut r)?;
        let shared_strings = read_opt_part(&mut r)?;
        let n = r.u32()?;
        let mut sheets = Vec::new();
        for _ in 0..n {
            let rel_id = r.opt_string()?;
            let order = r.u32()?;
            let part = read_part(&mut r)?;
            sheets.push(XlsxSheetRef {
                rel_id,
                order,
                part,
            });
        }
        if !r.at_end() {
            return Err(corrupt("XLSX model has trailing bytes"));
        }
        Ok(XlsxModel {
            workbook,
            styles,
            shared_strings,
            sheets,
        })
    }
}

// ---------------------------------------------------------------------------
// Workbook (sheet inventory) model
// ---------------------------------------------------------------------------

/// Declared worksheet visibility (`state` attribute).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetState {
    /// `state="visible"` (the default).
    Visible,
    /// `state="hidden"`.
    Hidden,
    /// `state="veryHidden"`.
    VeryHidden,
}

impl SheetState {
    /// Stable lower-case name.
    pub const fn name(self) -> &'static str {
        match self {
            SheetState::Visible => "visible",
            SheetState::Hidden => "hidden",
            SheetState::VeryHidden => "veryHidden",
        }
    }

    fn tag(self) -> u8 {
        match self {
            SheetState::Visible => 0,
            SheetState::Hidden => 1,
            SheetState::VeryHidden => 2,
        }
    }

    fn from_tag(b: u8) -> Result<SheetState> {
        Ok(match b {
            0 => SheetState::Visible,
            1 => SheetState::Hidden,
            2 => SheetState::VeryHidden,
            _ => return Err(corrupt("bad sheet-state tag")),
        })
    }
}

/// One `<sheet>` declared in `xl/workbook.xml`, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkbookSheet {
    /// `name` (the visible sheet name; never locale-normalized).
    pub name: String,
    /// `sheetId`, when present.
    pub sheet_id: Option<u32>,
    /// `r:id` → the workbook relationship naming the worksheet part.
    pub rel_id: Option<String>,
    /// Declared visibility.
    pub state: SheetState,
}

/// One `<definedName>` (a named/defined range) declared in `xl/workbook.xml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinedName {
    /// The defined name (`name`), e.g. `TaxRate`.
    pub name: String,
    /// `localSheetId` (the 0-based workbook sheet scope), when present.
    pub local_sheet_id: Option<u32>,
    /// `hidden="1"`.
    pub hidden: bool,
    /// `function="1"`.
    pub function: bool,
    /// The `refersTo` formula text (never evaluated).
    pub refers_to: String,
}

/// The parsed workbook sheet inventory, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkbookModel {
    /// The declared sheets, in document order.
    pub sheets: Vec<WorkbookSheet>,
    /// The declared defined/named ranges, in document order.
    pub defined_names: Vec<DefinedName>,
}

impl WorkbookModel {
    /// Encode canonically.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"XLWB");
        out.push(2);
        put_u32(&mut out, self.sheets.len() as u32);
        for s in &self.sheets {
            put_str(&mut out, &s.name);
            put_opt_u32(&mut out, s.sheet_id);
            put_opt_str(&mut out, s.rel_id.as_deref());
            out.push(s.state.tag());
        }
        put_u32(&mut out, self.defined_names.len() as u32);
        for d in &self.defined_names {
            put_str(&mut out, &d.name);
            put_opt_u32(&mut out, d.local_sheet_id);
            out.push(d.hidden as u8);
            out.push(d.function as u8);
            put_str(&mut out, &d.refers_to);
        }
        out
    }

    /// Decode a model produced by [`Self::encode`].
    pub fn decode(bytes: &[u8]) -> Result<WorkbookModel> {
        let mut r = ByteReader::new(bytes);
        if r.bytes(4)? != b"XLWB" {
            return Err(corrupt("bad XLSX workbook magic"));
        }
        if r.u8()? != 2 {
            return Err(corrupt("unsupported XLSX workbook version"));
        }
        let n = r.u32()?;
        let mut sheets = Vec::new();
        for _ in 0..n {
            let name = r.string()?;
            let sheet_id = r.opt_u32()?;
            let rel_id = r.opt_string()?;
            let state = SheetState::from_tag(r.u8()?)?;
            sheets.push(WorkbookSheet {
                name,
                sheet_id,
                rel_id,
                state,
            });
        }
        let nd = r.u32()?;
        let mut defined_names = Vec::new();
        for _ in 0..nd {
            let name = r.string()?;
            let local_sheet_id = r.opt_u32()?;
            let hidden = r.u8()? != 0;
            let function = r.u8()? != 0;
            let refers_to = r.string()?;
            defined_names.push(DefinedName {
                name,
                local_sheet_id,
                hidden,
                function,
                refers_to,
            });
        }
        if !r.at_end() {
            return Err(corrupt("XLSX workbook has trailing bytes"));
        }
        Ok(WorkbookModel {
            sheets,
            defined_names,
        })
    }
}

// ---------------------------------------------------------------------------
// Worksheet (cell) model
// ---------------------------------------------------------------------------

/// One cell of a worksheet.
///
/// `formula` is the stored formula text and `value` is the cached result — they
/// are separate fields, never merged. `span` is the exact decoded-part byte span
/// of the `<c>` element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XlsxCell {
    /// The A1 reference as written (empty when the source omitted `r`).
    pub reference: String,
    /// 0-based column index.
    pub col: u32,
    /// 0-based row index.
    pub row: u32,
    /// The raw `t` type tag (`""`/`None` is the numeric default).
    pub type_tag: Option<String>,
    /// The cached result / displayed text (a shared string is resolved).
    pub value: Option<String>,
    /// The stored formula text, when present.
    pub formula: Option<String>,
    /// The cell-style index (`s`), when present.
    pub style: Option<u32>,
    /// Decoded-part byte offset of the `<c>` element.
    pub span_start: u32,
    /// Decoded-part byte length of the `<c>` element.
    pub span_len: u32,
}

impl XlsxCell {
    /// A short, stable type label derived from the raw tag (never the value).
    pub fn kind(&self) -> &'static str {
        match self.type_tag.as_deref() {
            None | Some("") | Some("n") => "number",
            Some("s") => "shared-string",
            Some("inlineStr") => "inline-string",
            Some("b") => "boolean",
            Some("e") => "error",
            Some("str") => "formula-string",
            Some("d") => "date",
            _ => "unknown",
        }
    }
}

/// One `<row>` of a worksheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XlsxRow {
    /// 0-based row index.
    pub index: u32,
    /// Cells present in the row, in document order.
    pub cells: Vec<XlsxCell>,
}

/// One `<hyperlink>` declared in a worksheet. Whether it is an internal
/// (`location`) or external (`r:id` → a relationship) link is a distinct
/// observation; the target is never dereferenced here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetHyperlink {
    /// The cell (or range) reference (`ref`).
    pub reference: String,
    /// The relationship id (`r:id`) naming the target, when present.
    pub rel_id: Option<String>,
    /// The internal `location` (e.g. `Sheet2!A1`), when present.
    pub location: Option<String>,
    /// The `display` text override, when present.
    pub display: Option<String>,
    /// The `tooltip` text, when present.
    pub tooltip: Option<String>,
}

/// The parsed bounded view of one worksheet part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetModel {
    /// Absolute OPC part name of the worksheet.
    pub part_name: String,
    /// The sheet name resolved from the workbook inventory (empty if unknown).
    pub sheet_name: String,
    /// The declared `dimension` ref, when present.
    pub dimension: Option<String>,
    /// Declared merged ranges (`ref` values), in document order.
    pub merges: Vec<String>,
    /// Declared hyperlinks, in document order.
    pub hyperlinks: Vec<SheetHyperlink>,
    /// `tableParts` relationship ids (`r:id`), in document order.
    pub table_parts: Vec<String>,
    /// The `<drawing r:id>` relationship id, when present.
    pub drawing_rel_id: Option<String>,
    /// The `<legacyDrawing r:id>` (VML) relationship id, when present.
    pub legacy_drawing_rel_id: Option<String>,
    /// Rows, in document order.
    pub rows: Vec<XlsxRow>,
}

impl SheetModel {
    /// Total number of cells.
    pub fn cell_count(&self) -> u64 {
        self.rows.iter().map(|r| r.cells.len() as u64).sum()
    }

    /// The cell at a 0-based `(row, col)`, if present.
    pub fn cell_at(&self, row: u32, col: u32) -> Option<&XlsxCell> {
        self.rows
            .iter()
            .find(|r| r.index == row)?
            .cells
            .iter()
            .find(|c| c.col == col)
    }

    /// A deterministic per-cell facet (cached result or stored formula).
    pub fn facet(&self, cell: &XlsxCell, mode: ValueMode) -> Option<String> {
        match mode {
            ValueMode::Cached => cell.value.clone(),
            ValueMode::Formula => cell.formula.clone(),
        }
    }

    /// The sheet rendered as TSV: rows joined by `\n`, cells (filled by column
    /// position) joined by `\t`. A deterministic projection of the cached values.
    ///
    /// Before building, the **projected grid size** (the sum over rows of
    /// `max_col + 1`, i.e. the number of tab-separated fields that will be
    /// emitted, including empty ones) is compared against `max_projected_cells`;
    /// a sheet whose projection would exceed the bound is declined typed rather
    /// than expanded. Each row is merged-walked over its `col`-sorted cells, so
    /// the cost is bounded by the projection rather than a per-column search.
    pub fn text(&self, mode: ValueMode, max_projected_cells: u64) -> Result<String> {
        let mut projected: u64 = 0;
        for row in &self.rows {
            if let Some(max_col) = row.cells.iter().map(|c| c.col).max() {
                projected = projected.saturating_add(u64::from(max_col) + 1);
                if projected > max_projected_cells {
                    return Err(Error::resource_limit("sheet text projection exceeds bound"));
                }
            }
        }
        let mut out = String::new();
        for (i, row) in self.rows.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            let Some(max_col) = row.cells.iter().map(|c| c.col).max() else {
                continue;
            };
            let mut cells: Vec<&XlsxCell> = row.cells.iter().collect();
            cells.sort_by_key(|c| c.col);
            let mut next = 0usize;
            for col in 0..=max_col {
                if col > 0 {
                    out.push('\t');
                }
                while next < cells.len() && cells[next].col < col {
                    next += 1;
                }
                if next < cells.len()
                    && cells[next].col == col
                    && let Some(v) = self.facet(cells[next], mode)
                {
                    out.push_str(&v);
                }
            }
        }
        Ok(out)
    }

    /// Encode canonically.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"XLSH");
        out.push(2);
        put_str(&mut out, &self.part_name);
        put_str(&mut out, &self.sheet_name);
        put_opt_str(&mut out, self.dimension.as_deref());
        put_u32(&mut out, self.merges.len() as u32);
        for m in &self.merges {
            put_str(&mut out, m);
        }
        put_u32(&mut out, self.hyperlinks.len() as u32);
        for h in &self.hyperlinks {
            put_str(&mut out, &h.reference);
            put_opt_str(&mut out, h.rel_id.as_deref());
            put_opt_str(&mut out, h.location.as_deref());
            put_opt_str(&mut out, h.display.as_deref());
            put_opt_str(&mut out, h.tooltip.as_deref());
        }
        put_u32(&mut out, self.table_parts.len() as u32);
        for t in &self.table_parts {
            put_str(&mut out, t);
        }
        put_opt_str(&mut out, self.drawing_rel_id.as_deref());
        put_opt_str(&mut out, self.legacy_drawing_rel_id.as_deref());
        put_u32(&mut out, self.rows.len() as u32);
        for row in &self.rows {
            put_u32(&mut out, row.index);
            put_u32(&mut out, row.cells.len() as u32);
            for c in &row.cells {
                put_str(&mut out, &c.reference);
                put_u32(&mut out, c.col);
                put_u32(&mut out, c.row);
                put_opt_str(&mut out, c.type_tag.as_deref());
                put_opt_str(&mut out, c.value.as_deref());
                put_opt_str(&mut out, c.formula.as_deref());
                put_opt_u32(&mut out, c.style);
                put_u32(&mut out, c.span_start);
                put_u32(&mut out, c.span_len);
            }
        }
        out
    }

    /// Decode a model produced by [`Self::encode`].
    pub fn decode(bytes: &[u8]) -> Result<SheetModel> {
        let mut r = ByteReader::new(bytes);
        if r.bytes(4)? != b"XLSH" {
            return Err(corrupt("bad XLSX sheet magic"));
        }
        if r.u8()? != 2 {
            return Err(corrupt("unsupported XLSX sheet version"));
        }
        let part_name = r.string()?;
        let sheet_name = r.string()?;
        let dimension = r.opt_string()?;
        let nm = r.u32()?;
        let mut merges = Vec::new();
        for _ in 0..nm {
            merges.push(r.string()?);
        }
        let nh = r.u32()?;
        let mut hyperlinks = Vec::new();
        for _ in 0..nh {
            hyperlinks.push(SheetHyperlink {
                reference: r.string()?,
                rel_id: r.opt_string()?,
                location: r.opt_string()?,
                display: r.opt_string()?,
                tooltip: r.opt_string()?,
            });
        }
        let nt = r.u32()?;
        let mut table_parts = Vec::new();
        for _ in 0..nt {
            table_parts.push(r.string()?);
        }
        let drawing_rel_id = r.opt_string()?;
        let legacy_drawing_rel_id = r.opt_string()?;
        let nr = r.u32()?;
        let mut rows = Vec::new();
        for _ in 0..nr {
            let index = r.u32()?;
            let nc = r.u32()?;
            let mut cells = Vec::new();
            for _ in 0..nc {
                let reference = r.string()?;
                let col = r.u32()?;
                let row = r.u32()?;
                let type_tag = r.opt_string()?;
                let value = r.opt_string()?;
                let formula = r.opt_string()?;
                let style = r.opt_u32()?;
                let span_start = r.u32()?;
                let span_len = r.u32()?;
                cells.push(XlsxCell {
                    reference,
                    col,
                    row,
                    type_tag,
                    value,
                    formula,
                    style,
                    span_start,
                    span_len,
                });
            }
            rows.push(XlsxRow { index, cells });
        }
        if !r.at_end() {
            return Err(corrupt("XLSX sheet has trailing bytes"));
        }
        Ok(SheetModel {
            part_name,
            sheet_name,
            dimension,
            merges,
            hyperlinks,
            table_parts,
            drawing_rel_id,
            legacy_drawing_rel_id,
            rows,
        })
    }
}

// ---------------------------------------------------------------------------
// Styles (cell-style table)
// ---------------------------------------------------------------------------

/// A resolved text alignment (`<alignment>`), as written.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Alignment {
    /// `horizontal` (`left`/`center`/`right`/…), when present.
    pub horizontal: Option<String>,
    /// `vertical` (`top`/`center`/`bottom`), when present.
    pub vertical: Option<String>,
    /// `wrapText` (default false).
    pub wrap_text: bool,
}

/// A resolved font (`<font>`), as written. Only the facets Phase 21.1.2 exposes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FontStyle {
    /// `<b/>` (a `val="0"` means *not* bold).
    pub bold: bool,
    /// `<i/>`.
    pub italic: bool,
    /// `<sz val="…"/>`, as written (never reformatted).
    pub size: Option<String>,
    /// `<name val="…"/>`.
    pub name: Option<String>,
}

/// A resolved fill (`<fill>` / `<patternFill>`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FillStyle {
    /// `patternType` (`none`/`solid`/`gray125`/…).
    pub pattern_type: Option<String>,
    /// `fgColor` `rgb`/`indexed`/`theme` value, as written.
    pub fg_color: Option<String>,
    /// `bgColor` value, as written.
    pub bg_color: Option<String>,
}

/// One `cellXfs` entry: a resolved reference set into the styles substreams.
///
/// The `alignment` is a distinct field from the number format, font, and fill;
/// they are separate observations and are never conflated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellXf {
    /// `numFmtId` (builtin or custom).
    pub num_fmt_id: u32,
    /// `fontId`.
    pub font_id: u32,
    /// `fillId`.
    pub fill_id: u32,
    /// The nested `<alignment>`, when present.
    pub alignment: Option<Alignment>,
}

/// A fully resolved cell style: the number-format id and code, the font, the
/// fill, and the alignment. This is the *style* observation — distinct from the
/// cell's value, formula, and span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellStyle {
    /// `numFmtId` (builtin or custom).
    pub num_fmt_id: u32,
    /// The format code (a custom `formatCode`, else a known builtin code).
    pub format_code: Option<String>,
    /// The resolved font, when `fontId` names one.
    pub font: Option<FontStyle>,
    /// The resolved fill, when `fillId` names one.
    pub fill: Option<FillStyle>,
    /// The alignment, when declared.
    pub alignment: Option<Alignment>,
}

/// A cell-style table: custom number formats, fonts, fills, and the `cellXfs`
/// references that compose them per style index.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StylesTable {
    /// `numFmtId` → `formatCode` for custom (`numFmtId >= 164`) formats.
    pub num_fmts: BTreeMap<u32, String>,
    /// The `<fonts>` entries, in order.
    pub fonts: Vec<FontStyle>,
    /// The `<fills>` entries, in order.
    pub fills: Vec<FillStyle>,
    /// The `cellXfs` entries, in order.
    pub cell_xfs: Vec<CellXf>,
}

impl StylesTable {
    /// The style ref for a cell-style index (`s`), if present.
    pub fn cell_style(&self, index: u32) -> Option<CellXf> {
        self.cell_xfs.get(index as usize).cloned()
    }

    /// The number-format code for a `numFmtId`, if a custom format declares one.
    pub fn num_fmt_code(&self, num_fmt_id: u32) -> Option<&str> {
        self.num_fmts.get(&num_fmt_id).map(String::as_str)
    }

    /// The format code for a `numFmtId`: a custom code, else a known builtin.
    pub fn format_code(&self, num_fmt_id: u32) -> Option<&str> {
        self.num_fmt_code(num_fmt_id)
            .or_else(|| builtin_format_code(num_fmt_id))
    }

    /// The font for a `fontId`.
    pub fn font(&self, font_id: u32) -> Option<&FontStyle> {
        self.fonts.get(font_id as usize)
    }

    /// The fill for a `fillId`.
    pub fn fill(&self, fill_id: u32) -> Option<&FillStyle> {
        self.fills.get(fill_id as usize)
    }

    /// Resolve a cell-style index into its full [`CellStyle`].
    pub fn style_for(&self, index: u32) -> Option<CellStyle> {
        let xf = self.cell_xfs.get(index as usize)?;
        Some(CellStyle {
            num_fmt_id: xf.num_fmt_id,
            format_code: self.format_code(xf.num_fmt_id).map(str::to_string),
            font: self.font(xf.font_id).cloned(),
            fill: self.fill(xf.fill_id).cloned(),
            alignment: xf.alignment.clone(),
        })
    }
}

/// The format code for a known ECMA-376 builtin `numFmtId`, when one is defined.
/// Unlisted ids return `None` (no projection; never a guess).
fn builtin_format_code(id: u32) -> Option<&'static str> {
    Some(match id {
        0 => "General",
        1 => "0",
        2 => "0.00",
        3 => "#,##0",
        4 => "#,##0.00",
        9 => "0%",
        10 => "0.00%",
        11 => "0.00E+00",
        13 => "#,##0",
        14 => "m/d/yyyy",
        15 => "d-mmm-yy",
        16 => "d-mmm",
        22 => "m/d/yyyy h:mm",
        37 => "#,##0 ;(#,##0)",
        38 => "#,##0 ;[Red](#,##0)",
        39 => "#,##0.00;(#,##0.00)",
        49 => "@",
        _ => return None,
    })
}

/// A **deterministic** projection of a cell's cached numeric value under a bounded
/// subset of number-format codes. Returns `None` when the format is not in the
/// supported subset or the cached value is not a finite number — i.e. the
/// projection is *not available*, never a guess. This never evaluates a formula.
pub fn format_displayed(cached: &str, format_code: &str) -> Option<String> {
    let v: f64 = cached.trim().parse().ok()?;
    if !v.is_finite() {
        return None;
    }
    if format_code.eq_ignore_ascii_case("general") || format_code == "@" {
        return Some(cached.to_string());
    }
    let percent = format_code.ends_with('%');
    let core = if percent {
        &format_code[..format_code.len() - 1]
    } else {
        format_code
    };
    // Only fixed decimal/grouping patterns are projected.
    if core.is_empty()
        || !core
            .chars()
            .all(|c| c == '#' || c == '0' || c == ',' || c == '.')
    {
        return None;
    }
    let (int_part, frac_part) = match core.split_once('.') {
        Some((a, b)) => (a, b),
        None => (core, ""),
    };
    if frac_part.contains('.') {
        return None;
    }
    let decimals = frac_part.len();
    // A bounded projection: a format code asking for more than 30 fractional
    // digits is not projected (never a large allocation on a hostile code).
    if decimals > 30 {
        return None;
    }
    let grouping = int_part.contains(',');
    let scaled = if percent { v * 100.0 } else { v };
    let mut s = if decimals > 0 {
        format!("{scaled:.decimals$}")
    } else {
        format!("{scaled:.0}")
    };
    if grouping {
        s = group_thousands(&s, decimals);
    }
    if percent {
        s.push('%');
    }
    Some(s)
}

/// Insert thousands separators into the integer part of a decimal string.
fn group_thousands(s: &str, decimals: usize) -> String {
    let (sign, rest) = match s.strip_prefix('-') {
        Some(r) => ("-", r),
        None => ("", s),
    };
    let int_len = rest
        .len()
        .saturating_sub(decimals.saturating_add(usize::from(decimals > 0)));
    let (int_part, tail) = rest.split_at(int_len.min(rest.len()));
    let mut grouped = String::new();
    for (i, ch) in int_part.chars().enumerate() {
        if i > 0 && (int_part.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    format!("{sign}{grouped}{tail}")
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Discover the workbook part by relationship (package `_rels/.rels`), failing
/// closed on zero, ambiguous, external, or non-part targets.
fn find_workbook(model: &OpcModel) -> Result<&OpcPart> {
    let types = workbook_relationship_types();
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
            "package has no officeDocument relationship (not an XLSX workbook)",
        ));
    }
    let part = target.ok_or_else(|| {
        Error::invalid_package_structure("officeDocument relationship has no internal target")
    })?;
    let ct = part.content_type.as_deref().unwrap_or("");
    if !is_workbook_content_type(ct) && !part.name.to_ascii_lowercase().ends_with("/workbook.xml") {
        return Err(Error::invalid_package_structure(format!(
            "officeDocument target {:?} has content type {:?}, not a SpreadsheetML workbook",
            part.name, ct
        )));
    }
    Ok(part)
}

/// The workbook's relationships whose type matches one of `suffixes`, resolved to
/// a package part, returned as `(rel_id, part)` in relationship-id order.
fn workbook_related(
    model: &OpcModel,
    workbook: &OpcPart,
    suffixes: &[&str],
) -> Vec<(String, OpcPart)> {
    let mut out: Vec<(String, OpcPart)> = Vec::new();
    let Some((_, rels)) = model.part_rels.iter().find(|(o, _)| *o == workbook.ordinal) else {
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

fn parts_by_content_type(model: &OpcModel, suffix: &str) -> Vec<OpcPart> {
    let mut out: Vec<OpcPart> = model
        .parts
        .iter()
        .filter(|p| {
            p.content_type
                .as_deref()
                .is_some_and(|ct| ct.ends_with(suffix))
        })
        .cloned()
        .collect();
    out.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });
    out
}

fn to_ref(p: &OpcPart) -> XlsxPartRef {
    XlsxPartRef {
        name: p.name.clone(),
        ordinal: p.ordinal,
        content_type: p.content_type.clone(),
    }
}

fn related_or_ct(
    model: &OpcModel,
    workbook: &OpcPart,
    suffixes: &[&str],
    ct_suffix: &str,
) -> Option<XlsxPartRef> {
    workbook_related(model, workbook, suffixes)
        .into_iter()
        .next()
        .map(|(_, p)| to_ref(&p))
        .or_else(|| parts_by_content_type(model, ct_suffix).first().map(to_ref))
}

fn discover(model: &OpcModel, limits: Limits) -> Result<XlsxModel> {
    let workbook = find_workbook(model)?;

    let styles = related_or_ct(model, workbook, &["styles"], "spreadsheetml.styles+xml");
    let shared_strings = related_or_ct(
        model,
        workbook,
        &["sharedStrings"],
        "spreadsheetml.sharedStrings+xml",
    );

    let mut sheets: Vec<XlsxSheetRef> = Vec::new();
    let related = workbook_related(model, workbook, &["worksheet"]);
    if !related.is_empty() {
        for (order, (rel_id, p)) in related.iter().enumerate() {
            if sheets.len() as u64 >= u64::from(limits.max_xlsx_sheets) {
                return Err(Error::resource_limit(
                    "workbook declares more sheets than max_xlsx_sheets",
                ));
            }
            sheets.push(XlsxSheetRef {
                rel_id: Some(rel_id.clone()),
                order: order as u32,
                part: to_ref(p),
            });
        }
    } else {
        for (order, p) in parts_by_content_type(model, "spreadsheetml.worksheet+xml")
            .into_iter()
            .enumerate()
        {
            if sheets.len() as u64 >= u64::from(limits.max_xlsx_sheets) {
                return Err(Error::resource_limit(
                    "workbook declares more sheets than max_xlsx_sheets",
                ));
            }
            sheets.push(XlsxSheetRef {
                rel_id: None,
                order: order as u32,
                part: to_ref(&p),
            });
        }
    }

    Ok(XlsxModel {
        workbook: to_ref(workbook),
        styles,
        shared_strings,
        sheets,
    })
}

/// Build the canonical XLSX discovery model from a canonical OPC model (the
/// derived [`crate::field::node::NodeKind::XlsxModel`] computation).
pub fn build_xlsx_model(opc_bytes: &[u8], limits: Limits) -> Result<Vec<u8>> {
    let model = OpcModel::decode(opc_bytes)?;
    let xlsx = discover(&model, limits)?;
    Ok(xlsx.encode())
}

// ---------------------------------------------------------------------------
// Node parameters
// ---------------------------------------------------------------------------

/// Canonical parameters for a [`crate::field::node::NodeKind::XlsxWorkbook`] node:
/// `version(1) · ordinal(4) · len-prefixed part name`.
pub fn workbook_params(ordinal: u32, part_name: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(9 + part_name.len());
    out.push(1);
    put_u32(&mut out, ordinal);
    put_str(&mut out, part_name);
    out
}

/// Decode parameters produced by [`workbook_params`].
pub fn read_workbook_params(params: &[u8]) -> Result<(u32, String)> {
    let mut r = ByteReader::new(params);
    if r.u8()? != 1 {
        return Err(corrupt("unsupported XLSX workbook params version"));
    }
    let ordinal = r.u32()?;
    let name = r.string()?;
    if !r.at_end() {
        return Err(corrupt("XLSX workbook params have trailing bytes"));
    }
    Ok((ordinal, name))
}

/// Canonical parameters for a [`crate::field::node::NodeKind::XlsxSheet`] node:
/// `version(1) · ordinal(4) · shared_flag(1) [· shared_ordinal(4)] · profile(3) ·
/// len-prefixed part name · len-prefixed sheet name`.
pub fn sheet_params(
    ordinal: u32,
    part_name: &str,
    sheet_name: &str,
    profile: &XlsxExtractProfile,
    shared_ordinal: Option<u32>,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(24 + part_name.len() + sheet_name.len());
    out.push(1);
    put_u32(&mut out, ordinal);
    match shared_ordinal {
        Some(o) => {
            out.push(1);
            put_u32(&mut out, o);
        }
        None => out.push(0),
    }
    out.extend_from_slice(&profile.encode());
    put_str(&mut out, part_name);
    put_str(&mut out, sheet_name);
    out
}

/// Decode parameters produced by [`sheet_params`].
pub fn read_sheet_params(
    params: &[u8],
) -> Result<(u32, Option<u32>, XlsxExtractProfile, String, String)> {
    let mut r = ByteReader::new(params);
    if r.u8()? != 1 {
        return Err(corrupt("unsupported XLSX sheet params version"));
    }
    let ordinal = r.u32()?;
    let shared_ordinal = match r.u8()? {
        0 => None,
        1 => Some(r.u32()?),
        _ => return Err(corrupt("bad XLSX sheet shared flag")),
    };
    let profile = XlsxExtractProfile::decode(r.bytes(3)?)?;
    let part_name = r.string()?;
    let sheet_name = r.string()?;
    if !r.at_end() {
        return Err(corrupt("XLSX sheet params have trailing bytes"));
    }
    Ok((ordinal, shared_ordinal, profile, part_name, sheet_name))
}

// ---------------------------------------------------------------------------
// Workbook parsing
// ---------------------------------------------------------------------------

/// Parse and harden `xl/workbook.xml` into its sheet inventory and defined names.
pub fn parse_workbook(bytes: &[u8], limits: Limits) -> Result<WorkbookModel> {
    harden_xml(bytes, limits)?;
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut st = XmlState::new();
    let mut sheets: Vec<WorkbookSheet> = Vec::new();
    let mut defined_names: Vec<DefinedName> = Vec::new();
    let mut cur_defined: Option<DefinedName> = None;
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
                    check_root(&e, "workbook")?;
                    saw_root = true;
                }
                if e.name().local_name().as_ref() == "definedName" {
                    if defined_names.len() as u64 >= u64::from(limits.max_xlsx_defined_names) {
                        return Err(Error::resource_limit(
                            "workbook declares more defined names than max_xlsx_defined_names",
                        ));
                    }
                    let attrs = read_attrs(&e, limits)?;
                    let name = attr_of(&attrs, "name").unwrap_or("").to_string();
                    let local_sheet_id = match attr_of(&attrs, "localSheetId") {
                        Some(v) => Some(v.parse::<u32>().map_err(|_| {
                            Error::invalid_package_structure(format!(
                                "definedName localSheetId {v:?} is not a u32"
                            ))
                        })?),
                        None => None,
                    };
                    cur_defined = Some(DefinedName {
                        name,
                        local_sheet_id,
                        hidden: xml_flag(attr_of(&attrs, "hidden")),
                        function: xml_flag(attr_of(&attrs, "function")),
                        refers_to: String::new(),
                    });
                } else {
                    workbook_element(&mut sheets, &e, limits)?;
                }
            }
            Event::Empty(e) => {
                st.leaf(limits)?;
                if !saw_root {
                    check_root(&e, "workbook")?;
                    saw_root = true;
                }
                workbook_element(&mut sheets, &e, limits)?;
            }
            Event::End(e) => {
                if e.name().local_name().as_ref() == "definedName"
                    && let Some(d) = cur_defined.take()
                {
                    defined_names.push(d);
                }
                st.close();
            }
            Event::Text(t) => {
                let s = t.into_inner();
                st.text(s.len(), limits)?;
                if let Some(d) = cur_defined.as_mut() {
                    d.refers_to.push_str(s.as_ref());
                }
            }
            Event::GeneralRef(r) => {
                if let Some(d) = cur_defined.as_mut() {
                    push_ref(&mut d.refers_to, r);
                }
            }
            _ => {}
        }
    }
    if !saw_root {
        return Err(Error::invalid_xml_structure("workbook XML part is empty"));
    }
    Ok(WorkbookModel {
        sheets,
        defined_names,
    })
}

fn workbook_element(
    sheets: &mut Vec<WorkbookSheet>,
    e: &BytesStart<'_>,
    limits: Limits,
) -> Result<()> {
    if e.name().local_name().as_ref() != "sheet" {
        return Ok(());
    }
    if sheets.len() as u64 >= u64::from(limits.max_xlsx_sheets) {
        return Err(Error::resource_limit(
            "workbook declares more sheets than max_xlsx_sheets",
        ));
    }
    let attrs = read_attrs(e, limits)?;
    // A `<sheet>` without a name is still recorded (name empty) rather than
    // silently dropped: the sheet index is the physical coordinate.
    let name = attr_of(&attrs, "name").unwrap_or("").to_string();
    let sheet_id = match attr_of(&attrs, "sheetId") {
        Some(v) => Some(v.parse::<u32>().map_err(|_| {
            Error::invalid_package_structure(format!("sheet sheetId {v:?} is not a u32"))
        })?),
        None => None,
    };
    let rel_id = attr_of(&attrs, "id").map(str::to_string);
    let state = match attr_of(&attrs, "state") {
        None | Some("visible") => SheetState::Visible,
        Some("hidden") => SheetState::Hidden,
        Some("veryHidden") => SheetState::VeryHidden,
        Some(other) => {
            return Err(Error::invalid_package_structure(format!(
                "unknown sheet state {other:?}"
            )));
        }
    };
    sheets.push(WorkbookSheet {
        name,
        sheet_id,
        rel_id,
        state,
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared-strings parsing
// ---------------------------------------------------------------------------

/// Parse and harden `xl/sharedStrings.xml` into its string table (in order).
pub fn parse_shared_strings(bytes: &[u8], limits: Limits) -> Result<Vec<String>> {
    harden_xml(bytes, limits)?;
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut st = XmlState::new();
    let mut out: Vec<String> = Vec::new();
    let mut cur: Option<String> = None;
    let mut in_t = 0u32;
    let mut saw_root = false;
    loop {
        let ev = reader.read_event().map_err(xml_err)?;
        st.event(limits)?;
        match ev {
            Event::Eof => break,
            Event::DocType(d) => accept_doctype(&d.into_inner())?,
            Event::Start(e) => {
                st.open(limits)?;
                let local = e.name().local_name().as_ref().to_string();
                if !saw_root {
                    check_root(&e, "sst")?;
                    saw_root = true;
                }
                match local.as_str() {
                    "si" => {
                        if out.len() as u64 >= u64::from(limits.max_xlsx_shared_strings) {
                            return Err(Error::resource_limit(
                                "shared strings exceed max_xlsx_shared_strings",
                            ));
                        }
                        cur = Some(String::new());
                    }
                    "t" => in_t += 1,
                    _ => {}
                }
            }
            Event::Empty(e) => {
                st.leaf(limits)?;
                let local = e.name().local_name().as_ref().to_string();
                if !saw_root {
                    check_root(&e, "sst")?;
                    saw_root = true;
                }
                if local == "si" {
                    if out.len() as u64 >= u64::from(limits.max_xlsx_shared_strings) {
                        return Err(Error::resource_limit(
                            "shared strings exceed max_xlsx_shared_strings",
                        ));
                    }
                    out.push(String::new());
                }
            }
            Event::End(e) => {
                let local = e.name().local_name().as_ref().to_string();
                match local.as_str() {
                    "si" => {
                        if let Some(s) = cur.take() {
                            out.push(s);
                        }
                    }
                    "t" => in_t = in_t.saturating_sub(1),
                    _ => {}
                }
                st.close();
            }
            Event::Text(t) => {
                let s = t.into_inner();
                st.text(s.len(), limits)?;
                if in_t > 0
                    && let Some(cur) = cur.as_mut()
                {
                    cur.push_str(s.as_ref());
                }
            }
            Event::GeneralRef(r) => {
                if in_t > 0
                    && let Some(cur) = cur.as_mut()
                {
                    push_ref(cur, r);
                }
            }
            _ => {}
        }
    }
    if !saw_root {
        return Err(Error::invalid_xml_structure(
            "shared-strings XML part is empty",
        ));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Styles parsing
// ---------------------------------------------------------------------------

/// Parse and harden `xl/styles.xml` into its cell-style table (custom number
/// formats, fonts, fills, and `cellXfs` references). Bounded by
/// `max_xlsx_style_records`.
#[allow(clippy::too_many_lines)]
pub fn parse_styles_table(bytes: &[u8], limits: Limits) -> Result<StylesTable> {
    harden_xml(bytes, limits)?;
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut st = XmlState::new();
    let mut table = StylesTable::default();
    let mut in_cell_xfs = false;
    let mut in_fonts = false;
    let mut in_fills = false;
    let mut in_font = false;
    let mut in_fill = false;
    let mut in_pattern = false;
    let mut in_xf = false;
    // The font/fill currently being assembled (a placeholder is pushed on open so
    // its index matches document order even for a self-closing element).
    let mut cur_font: Option<FontStyle> = None;
    let mut cur_fill: Option<FillStyle> = None;
    let mut cur_xf: Option<CellXf> = None;
    let mut saw_root = false;
    loop {
        let ev = reader.read_event().map_err(xml_err)?;
        st.event(limits)?;
        match ev {
            Event::Eof => break,
            Event::DocType(d) => accept_doctype(&d.into_inner())?,
            Event::Start(e) => {
                st.open(limits)?;
                let local = e.name().local_name().as_ref().to_string();
                if !saw_root {
                    check_root(&e, "styleSheet")?;
                    saw_root = true;
                }
                match local.as_str() {
                    "fonts" => in_fonts = true,
                    "fills" => in_fills = true,
                    "cellXfs" => in_cell_xfs = true,
                    "font" if in_fonts => {
                        charge_style(&table, limits)?;
                        cur_font = Some(FontStyle::default());
                        in_font = true;
                    }
                    "fill" if in_fills => {
                        charge_style(&table, limits)?;
                        cur_fill = Some(FillStyle::default());
                        in_fill = true;
                    }
                    "patternFill" if in_fill => {
                        in_pattern = true;
                        if let Some(f) = cur_fill.as_mut() {
                            let attrs = read_attrs(&e, limits)?;
                            f.pattern_type = attr_of(&attrs, "patternType").map(str::to_string);
                        }
                    }
                    "b" if in_font => font_flag(&mut cur_font, FontFlag::Bold, &e, limits)?,
                    "i" if in_font => font_flag(&mut cur_font, FontFlag::Italic, &e, limits)?,
                    "sz" if in_font => {
                        let attrs = read_attrs(&e, limits)?;
                        if let Some(f) = cur_font.as_mut() {
                            f.size = attr_of(&attrs, "val").map(str::to_string);
                        }
                    }
                    "name" if in_font => {
                        let attrs = read_attrs(&e, limits)?;
                        if let Some(f) = cur_font.as_mut() {
                            f.name = attr_of(&attrs, "val").map(str::to_string);
                        }
                    }
                    "fgColor" if in_pattern => {
                        let attrs = read_attrs(&e, limits)?;
                        if let Some(f) = cur_fill.as_mut() {
                            f.fg_color = color_value(&attrs);
                        }
                    }
                    "bgColor" if in_pattern => {
                        let attrs = read_attrs(&e, limits)?;
                        if let Some(f) = cur_fill.as_mut() {
                            f.bg_color = color_value(&attrs);
                        }
                    }
                    "xf" if in_cell_xfs => {
                        charge_style(&table, limits)?;
                        cur_xf = Some(styles_xf_new(&e, limits)?);
                        in_xf = true;
                    }
                    "alignment" if in_xf => {
                        let attrs = read_attrs(&e, limits)?;
                        if let Some(xf) = cur_xf.as_mut() {
                            xf.alignment = Some(Alignment {
                                horizontal: attr_of(&attrs, "horizontal").map(str::to_string),
                                vertical: attr_of(&attrs, "vertical").map(str::to_string),
                                wrap_text: attr_of(&attrs, "wrapText")
                                    .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true")),
                            });
                        }
                    }
                    "numFmt" => styles_num_fmt(&mut table, &e, limits)?,
                    _ => {}
                }
            }
            Event::Empty(e) => {
                st.leaf(limits)?;
                let local = e.name().local_name().as_ref().to_string();
                if !saw_root {
                    check_root(&e, "styleSheet")?;
                    saw_root = true;
                }
                match local.as_str() {
                    "font" if in_fonts => {
                        charge_style(&table, limits)?;
                        table.fonts.push(FontStyle::default());
                    }
                    "fill" if in_fills => {
                        charge_style(&table, limits)?;
                        table.fills.push(FillStyle::default());
                    }
                    "b" if in_font => font_flag(&mut cur_font, FontFlag::Bold, &e, limits)?,
                    "i" if in_font => font_flag(&mut cur_font, FontFlag::Italic, &e, limits)?,
                    "sz" if in_font => {
                        let attrs = read_attrs(&e, limits)?;
                        if let Some(f) = cur_font.as_mut() {
                            f.size = attr_of(&attrs, "val").map(str::to_string);
                        }
                    }
                    "name" if in_font => {
                        let attrs = read_attrs(&e, limits)?;
                        if let Some(f) = cur_font.as_mut() {
                            f.name = attr_of(&attrs, "val").map(str::to_string);
                        }
                    }
                    "patternFill" if in_fill => {
                        if let Some(f) = cur_fill.as_mut() {
                            let attrs = read_attrs(&e, limits)?;
                            f.pattern_type = attr_of(&attrs, "patternType").map(str::to_string);
                        }
                    }
                    "fgColor" if in_pattern => {
                        let attrs = read_attrs(&e, limits)?;
                        if let Some(f) = cur_fill.as_mut() {
                            f.fg_color = color_value(&attrs);
                        }
                    }
                    "bgColor" if in_pattern => {
                        let attrs = read_attrs(&e, limits)?;
                        if let Some(f) = cur_fill.as_mut() {
                            f.bg_color = color_value(&attrs);
                        }
                    }
                    "xf" if in_cell_xfs => {
                        charge_style(&table, limits)?;
                        table.cell_xfs.push(styles_xf_new(&e, limits)?);
                    }
                    "alignment" if in_xf => {
                        let attrs = read_attrs(&e, limits)?;
                        if let Some(xf) = cur_xf.as_mut() {
                            xf.alignment = Some(Alignment {
                                horizontal: attr_of(&attrs, "horizontal").map(str::to_string),
                                vertical: attr_of(&attrs, "vertical").map(str::to_string),
                                wrap_text: attr_of(&attrs, "wrapText")
                                    .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true")),
                            });
                        }
                    }
                    "numFmt" => styles_num_fmt(&mut table, &e, limits)?,
                    _ => {}
                }
            }
            Event::End(e) => {
                match e.name().local_name().as_ref() {
                    "fonts" => in_fonts = false,
                    "fills" => in_fills = false,
                    "cellXfs" => in_cell_xfs = false,
                    "font" if in_font => {
                        if let Some(f) = cur_font.take() {
                            table.fonts.push(f);
                        }
                        in_font = false;
                    }
                    "fill" if in_fill => {
                        if let Some(f) = cur_fill.take() {
                            table.fills.push(f);
                        }
                        in_fill = false;
                    }
                    "patternFill" => in_pattern = false,
                    "xf" if in_xf => {
                        if let Some(x) = cur_xf.take() {
                            table.cell_xfs.push(x);
                        }
                        in_xf = false;
                    }
                    _ => {}
                }
                st.close();
            }
            Event::Text(t) => st.text(t.len(), limits)?,
            _ => {}
        }
    }
    if !saw_root {
        return Err(Error::invalid_xml_structure("styles XML part is empty"));
    }
    Ok(table)
}

enum FontFlag {
    Bold,
    Italic,
}

fn font_flag(
    font: &mut Option<FontStyle>,
    flag: FontFlag,
    e: &BytesStart<'_>,
    limits: Limits,
) -> Result<()> {
    let attrs = read_attrs(e, limits)?;
    // `<b/>` is bold; `<b val="0"/>` is explicitly *not* bold.
    let on = !attr_of(&attrs, "val").is_some_and(|v| v == "0" || v.eq_ignore_ascii_case("false"));
    if let Some(f) = font.as_mut() {
        match flag {
            FontFlag::Bold => f.bold = on,
            FontFlag::Italic => f.italic = on,
        }
    }
    Ok(())
}

fn color_value(attrs: &[(String, String)]) -> Option<String> {
    attr_of(attrs, "rgb")
        .or_else(|| attr_of(attrs, "indexed"))
        .or_else(|| attr_of(attrs, "theme"))
        .map(str::to_string)
}

fn charge_style(table: &StylesTable, limits: Limits) -> Result<()> {
    let total = table.fonts.len() as u64 + table.fills.len() as u64 + table.cell_xfs.len() as u64;
    if total >= u64::from(limits.max_xlsx_style_records) {
        return Err(Error::resource_limit(
            "styles exceed max_xlsx_style_records",
        ));
    }
    Ok(())
}

fn styles_num_fmt(table: &mut StylesTable, e: &BytesStart<'_>, limits: Limits) -> Result<()> {
    let attrs = read_attrs(e, limits)?;
    let id = attr_of(&attrs, "numFmtId")
        .ok_or_else(|| Error::invalid_package_structure("numFmt lacks numFmtId"))?
        .parse::<u32>()
        .map_err(|_| Error::invalid_package_structure("numFmt numFmtId is not a u32"))?;
    let code = attr_of(&attrs, "formatCode").unwrap_or("").to_string();
    table.num_fmts.insert(id, code);
    Ok(())
}

fn styles_xf_new(e: &BytesStart<'_>, limits: Limits) -> Result<CellXf> {
    let attrs = read_attrs(e, limits)?;
    Ok(CellXf {
        num_fmt_id: parse_u32_attr(&attrs, "numFmtId")?,
        font_id: parse_u32_attr(&attrs, "fontId")?,
        fill_id: parse_u32_attr(&attrs, "fillId")?,
        alignment: None,
    })
}

fn parse_u32_attr(attrs: &[(String, String)], name: &str) -> Result<u32> {
    match attr_of(attrs, name) {
        None => Ok(0),
        Some(v) => v
            .parse::<u32>()
            .map_err(|_| Error::invalid_package_structure(format!("{name} {v:?} is not a u32"))),
    }
}

// ---------------------------------------------------------------------------
// Worksheet parsing
// ---------------------------------------------------------------------------

struct CellBuilder {
    reference: String,
    col: u32,
    row: u32,
    type_tag: Option<String>,
    value: Option<String>,
    formula: Option<String>,
    style: Option<u32>,
    span_start: u32,
    inline: String,
    in_is: bool,
    in_t: bool,
    in_v: bool,
    in_f: bool,
}

impl CellBuilder {
    fn new() -> Self {
        CellBuilder {
            reference: String::new(),
            col: 0,
            row: 0,
            type_tag: None,
            value: None,
            formula: None,
            style: None,
            span_start: 0,
            inline: String::new(),
            in_is: false,
            in_t: false,
            in_v: false,
            in_f: false,
        }
    }

    /// Append decoded character data to the field the current context selects.
    fn push_text(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        if self.in_t && self.in_is {
            self.inline.push_str(s);
        } else if self.in_v {
            self.value.get_or_insert_with(String::new).push_str(s);
        } else if self.in_f {
            self.formula.get_or_insert_with(String::new).push_str(s);
        }
    }

    fn finalize(self, shared: Option<&[String]>, span_len: u32) -> XlsxCell {
        // The cached value: a shared string index is resolved against the table;
        // an inline string is the concatenated `<is>` text; otherwise `<v>` raw.
        let value = match self.type_tag.as_deref() {
            Some("s") => self
                .value
                .as_deref()
                .and_then(|v| v.trim().parse::<usize>().ok())
                .and_then(|i| shared.and_then(|t| t.get(i)).cloned())
                .or(self.value),
            Some("inlineStr") => Some(self.inline),
            _ => {
                if self.inline.is_empty() {
                    self.value
                } else {
                    Some(self.inline)
                }
            }
        };
        XlsxCell {
            reference: self.reference,
            col: self.col,
            row: self.row,
            type_tag: self.type_tag,
            value,
            formula: self.formula,
            style: self.style,
            span_start: self.span_start,
            span_len,
        }
    }
}

/// Parse and harden one worksheet (`xl/worksheets/sheetN.xml`) into a
/// [`SheetModel`]. Shared-string cells are resolved against `shared` when given.
pub fn parse_worksheet(
    bytes: &[u8],
    part_name: &str,
    sheet_name: &str,
    shared: Option<&[String]>,
    limits: Limits,
) -> Result<SheetModel> {
    harden_xml(bytes, limits)?;
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut st = XmlState::new();
    let mut dimension: Option<String> = None;
    let mut merges: Vec<String> = Vec::new();
    let mut hyperlinks: Vec<SheetHyperlink> = Vec::new();
    let mut table_parts: Vec<String> = Vec::new();
    let mut drawing_rel_id: Option<String> = None;
    let mut legacy_drawing_rel_id: Option<String> = None;
    let mut rows: Vec<XlsxRow> = Vec::new();
    let mut cur_row: Option<(u32, Vec<XlsxCell>)> = None;
    let mut cur_cell: Option<CellBuilder> = None;
    let mut total_cells: u64 = 0;
    let mut saw_root = false;
    let mut cell_pos_before: u64 = 0;

    loop {
        let pos_before = reader.buffer_position();
        let ev = reader.read_event().map_err(xml_err)?;
        st.event(limits)?;
        match ev {
            Event::Eof => break,
            Event::DocType(d) => accept_doctype(&d.into_inner())?,
            Event::Start(e) => {
                st.open(limits)?;
                let local = e.name().local_name().as_ref().as_bytes().to_vec();
                if !saw_root {
                    check_root(&e, "worksheet")?;
                    saw_root = true;
                }
                match local.as_slice() {
                    b"dimension" => {
                        let attrs = read_attrs(&e, limits)?;
                        dimension = attr_of(&attrs, "ref").map(str::to_string);
                    }
                    b"mergeCells" => {}
                    b"mergeCell" => {
                        let attrs = read_attrs(&e, limits)?;
                        push_merge(&mut merges, &attrs, limits)?;
                    }
                    b"hyperlink" => {
                        let attrs = read_attrs(&e, limits)?;
                        push_hyperlink(&mut hyperlinks, &attrs, limits)?;
                    }
                    b"tablePart" => {
                        let attrs = read_attrs(&e, limits)?;
                        push_rel_id(&mut table_parts, &attrs, "tableParts", limits)?;
                    }
                    b"drawing" => {
                        let attrs = read_attrs(&e, limits)?;
                        drawing_rel_id = attr_of(&attrs, "id").map(str::to_string);
                    }
                    b"legacyDrawing" => {
                        let attrs = read_attrs(&e, limits)?;
                        legacy_drawing_rel_id = attr_of(&attrs, "id").map(str::to_string);
                    }
                    b"row" => {
                        let attrs = read_attrs(&e, limits)?;
                        let index = row_index(&attrs)?;
                        cur_row = Some((index, Vec::new()));
                    }
                    b"c" => {
                        let attrs = read_attrs(&e, limits)?;
                        cell_pos_before = pos_before;
                        cur_cell = Some(start_cell(&attrs, &cur_row, limits)?);
                    }
                    b"is" => {
                        if let Some(c) = cur_cell.as_mut() {
                            c.in_is = true;
                        }
                    }
                    b"t" => {
                        if let Some(c) = cur_cell.as_mut()
                            && c.in_is
                        {
                            c.in_t = true;
                        }
                    }
                    b"v" => {
                        if let Some(c) = cur_cell.as_mut() {
                            c.in_v = true;
                        }
                    }
                    b"f" => {
                        if let Some(c) = cur_cell.as_mut() {
                            c.in_f = true;
                        }
                    }
                    _ => {}
                }
            }
            Event::Empty(e) => {
                st.leaf(limits)?;
                let local = e.name().local_name().as_ref().as_bytes().to_vec();
                if !saw_root {
                    check_root(&e, "worksheet")?;
                    saw_root = true;
                }
                match local.as_slice() {
                    b"dimension" => {
                        let attrs = read_attrs(&e, limits)?;
                        dimension = attr_of(&attrs, "ref").map(str::to_string);
                    }
                    b"mergeCell" => {
                        let attrs = read_attrs(&e, limits)?;
                        push_merge(&mut merges, &attrs, limits)?;
                    }
                    b"hyperlink" => {
                        let attrs = read_attrs(&e, limits)?;
                        push_hyperlink(&mut hyperlinks, &attrs, limits)?;
                    }
                    b"tablePart" => {
                        let attrs = read_attrs(&e, limits)?;
                        push_rel_id(&mut table_parts, &attrs, "tableParts", limits)?;
                    }
                    b"drawing" => {
                        let attrs = read_attrs(&e, limits)?;
                        drawing_rel_id = attr_of(&attrs, "id").map(str::to_string);
                    }
                    b"legacyDrawing" => {
                        let attrs = read_attrs(&e, limits)?;
                        legacy_drawing_rel_id = attr_of(&attrs, "id").map(str::to_string);
                    }
                    b"c" => {
                        let attrs = read_attrs(&e, limits)?;
                        let start = u32::try_from(pos_before).unwrap_or(0);
                        let end = u32::try_from(reader.buffer_position()).unwrap_or(start);
                        let mut cell = start_cell(&attrs, &cur_row, limits)?;
                        cell.span_start = start;
                        let cell = cell.finalize(shared, end.saturating_sub(start));
                        charge_cell(&mut total_cells, limits)?;
                        push_cell(&mut cur_row, cell)?;
                    }
                    b"row" => {
                        if let Some((index, cells)) = cur_row.take() {
                            rows.push(XlsxRow { index, cells });
                        }
                    }
                    _ => {}
                }
            }
            Event::End(e) => {
                let local = e.name().local_name().as_ref().as_bytes().to_vec();
                match local.as_slice() {
                    b"c" => {
                        if let Some(mut cell) = cur_cell.take() {
                            let end = u32::try_from(reader.buffer_position()).unwrap_or(0);
                            cell.span_start = u32::try_from(cell_pos_before).unwrap_or(0);
                            let len = end.saturating_sub(cell.span_start);
                            let cell = cell.finalize(shared, len);
                            charge_cell(&mut total_cells, limits)?;
                            push_cell(&mut cur_row, cell)?;
                        }
                    }
                    b"row" => {
                        if let Some((index, cells)) = cur_row.take() {
                            rows.push(XlsxRow { index, cells });
                        }
                    }
                    b"is" => {
                        if let Some(c) = cur_cell.as_mut() {
                            c.in_is = false;
                        }
                    }
                    b"t" => {
                        if let Some(c) = cur_cell.as_mut() {
                            c.in_t = false;
                        }
                    }
                    b"v" => {
                        if let Some(c) = cur_cell.as_mut() {
                            c.in_v = false;
                        }
                    }
                    b"f" => {
                        if let Some(c) = cur_cell.as_mut() {
                            c.in_f = false;
                        }
                    }
                    b"mergeCells" => {}
                    _ => {}
                }
                st.close();
            }
            Event::Text(t) => {
                let s = t.into_inner();
                st.text(s.len(), limits)?;
                if let Some(c) = cur_cell.as_mut() {
                    c.push_text(s.as_ref());
                }
            }
            Event::GeneralRef(r) => {
                if let Some(c) = cur_cell.as_mut() {
                    let mut s = String::new();
                    push_ref(&mut s, r);
                    c.push_text(&s);
                }
            }
            _ => {}
        }
    }
    if !saw_root {
        return Err(Error::invalid_xml_structure("worksheet XML part is empty"));
    }
    if let Some((index, cells)) = cur_row.take() {
        rows.push(XlsxRow { index, cells });
    }
    Ok(SheetModel {
        part_name: part_name.to_string(),
        sheet_name: sheet_name.to_string(),
        dimension,
        merges,
        hyperlinks,
        table_parts,
        drawing_rel_id,
        legacy_drawing_rel_id,
        rows,
    })
}

fn charge_cell(total: &mut u64, limits: Limits) -> Result<()> {
    *total = total.saturating_add(1);
    if *total > limits.max_xlsx_cells {
        return Err(Error::resource_limit("worksheet exceeds max_xlsx_cells"));
    }
    Ok(())
}

fn push_cell(cur_row: &mut Option<(u32, Vec<XlsxCell>)>, cell: XlsxCell) -> Result<()> {
    match cur_row {
        Some((_, cells)) => cells.push(cell),
        None => {
            return Err(Error::invalid_package_structure(
                "worksheet has a cell outside any row",
            ));
        }
    }
    Ok(())
}

fn push_merge(merges: &mut Vec<String>, attrs: &[(String, String)], limits: Limits) -> Result<()> {
    if merges.len() as u64 >= u64::from(limits.max_xlsx_merges) {
        return Err(Error::resource_limit("merges exceed max_xlsx_merges"));
    }
    if let Some(r) = attr_of(attrs, "ref") {
        merges.push(r.to_string());
    }
    Ok(())
}

fn push_hyperlink(
    hyperlinks: &mut Vec<SheetHyperlink>,
    attrs: &[(String, String)],
    limits: Limits,
) -> Result<()> {
    if hyperlinks.len() as u64 >= u64::from(limits.max_xlsx_hyperlinks) {
        return Err(Error::resource_limit(
            "worksheet declares more hyperlinks than max_xlsx_hyperlinks",
        ));
    }
    hyperlinks.push(SheetHyperlink {
        reference: attr_of(attrs, "ref").unwrap_or("").to_string(),
        rel_id: attr_of(attrs, "id").map(str::to_string),
        location: attr_of(attrs, "location").map(str::to_string),
        display: attr_of(attrs, "display").map(str::to_string),
        tooltip: attr_of(attrs, "tooltip").map(str::to_string),
    });
    Ok(())
}

/// Push a `r:id`-style relationship reference from a leaf element.
fn push_rel_id(
    out: &mut Vec<String>,
    attrs: &[(String, String)],
    what: &str,
    limits: Limits,
) -> Result<()> {
    if out.len() as u64 >= u64::from(limits.max_xlsx_tables) {
        return Err(Error::resource_limit(format!(
            "{what} exceeds max_xlsx_tables"
        )));
    }
    if let Some(id) = attr_of(attrs, "id") {
        out.push(id.to_string());
    }
    Ok(())
}

fn row_index(attrs: &[(String, String)]) -> Result<u32> {
    match attr_of(attrs, "r") {
        None => Ok(0),
        Some(v) => {
            let one_based: u32 = v
                .parse()
                .map_err(|_| Error::invalid_package_structure("row r is not a u32"))?;
            Ok(one_based.saturating_sub(1))
        }
    }
}

fn start_cell(
    attrs: &[(String, String)],
    cur_row: &Option<(u32, Vec<XlsxCell>)>,
    limits: Limits,
) -> Result<CellBuilder> {
    let mut c = CellBuilder::new();
    if let Some(row) = cur_row {
        c.row = row.0;
    }
    c.type_tag = attr_of(attrs, "t").map(str::to_string);
    c.style = match attr_of(attrs, "s") {
        Some(v) => Some(
            v.parse::<u32>()
                .map_err(|_| Error::invalid_package_structure("cell s is not a u32"))?,
        ),
        None => None,
    };
    if let Some(r) = attr_of(attrs, "r") {
        c.reference = r.to_string();
        // A present-but-unparseable reference is declined typed, never silently
        // rewritten to an implicit position (which would be a wrong answer).
        let (col, row) = a1_to_col_row(r)
            .ok_or_else(|| Error::invalid_package_structure("cell r is not an A1 reference"))?;
        c.col = col;
        c.row = row;
    } else if let Some((_, cells)) = cur_row {
        c.col = cells.iter().map(|c| c.col + 1).max().unwrap_or(0);
    }
    // Reject coordinates outside the Excel-conformant grid before they reach the
    // derived model (a bounded coordinate can never drive a huge projection).
    if c.col >= limits.max_xlsx_col || c.row >= limits.max_xlsx_row {
        return Err(Error::resource_limit(
            "cell coordinate exceeds max_xlsx_col/max_xlsx_row",
        ));
    }
    Ok(c)
}

/// Parse an A1-style reference (`B7`) into a 0-based `(column, row)`. Absolute
/// markers (`$A$1`, `A$1`, `$A1`) are tolerated. Returns `None` for anything that
/// is not an A1 reference, including an over-long reference whose column or row
/// would overflow `u64` (checked arithmetic, never a wrap).
pub fn a1_to_col_row(reference: &str) -> Option<(u32, u32)> {
    let bytes = reference.as_bytes();
    let mut i = 0;
    // Tolerate leading absolute-reference markers (`$A$1`, `$A1`).
    while i < bytes.len() && bytes[i] == b'$' {
        i += 1;
    }
    let letters_start = i;
    let mut col: u64 = 0;
    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
        let upper = bytes[i].to_ascii_uppercase();
        col = col
            .checked_mul(26)?
            .checked_add(u64::from(upper - b'A' + 1))?;
        i += 1;
    }
    if i == letters_start || i >= bytes.len() {
        return None;
    }
    // Tolerate an absolute marker before the row digits (`A$1`).
    if bytes[i] == b'$' {
        i += 1;
    }
    let row_start = i;
    let mut row: u64 = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            return None;
        }
        row = row
            .checked_mul(10)?
            .checked_add(u64::from(bytes[i] - b'0'))?;
        i += 1;
    }
    if i == row_start || col == 0 || row == 0 {
        return None;
    }
    let col = u32::try_from(col - 1).ok()?;
    let row = u32::try_from(row - 1).ok()?;
    Some((col, row))
}

/// Convert a 0-based `(column, row)` into an A1-style reference (`B7`).
pub fn col_row_to_a1(col: u32, row: u32) -> String {
    let mut c = u64::from(col) + 1;
    let mut letters: Vec<char> = Vec::new();
    while c > 0 {
        let rem = ((c - 1) % 26) as u8;
        letters.push((b'A' + rem) as char);
        c = (c - 1) / 26;
    }
    letters.reverse();
    format!("{}{}", letters.into_iter().collect::<String>(), row + 1)
}

// ---------------------------------------------------------------------------
// Comments, VML, tables, drawings (Phase 21.1.2)
// ---------------------------------------------------------------------------

/// One cell comment (`xl/comments*.xml`), keyed by cell reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellComment {
    /// The A1 cell reference (`ref`).
    pub cell: String,
    /// The resolved author (via `authorId` into `<authors>`), when present.
    pub author: Option<String>,
    /// The comment text (the concatenated `<text>` runs; never interpreted).
    pub text: String,
}

/// One VML note anchor (`xl/drawings/vmlDrawing*.vml`): the cell a comment shape
/// sits on. The VML is a legacy drawing surface; only the note anchor and shape
/// id are exposed here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmlNote {
    /// The A1 cell reference resolved from the VML `Row`/`Column` client data.
    pub cell: String,
    /// The VML shape id, when present.
    pub shape_id: Option<String>,
}

/// One `<tableColumn>` of a table part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableColumn {
    /// The `id` attribute, when present.
    pub id: Option<u32>,
    /// The column `name`.
    pub name: String,
}

/// A parsed `xl/tables/table*.xml` (`<table>`): name, ref, and columns.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SheetTable {
    /// The `name` attribute.
    pub name: Option<String>,
    /// The `displayName` attribute.
    pub display_name: Option<String>,
    /// The `ref` range (e.g. `A1:C4`).
    pub reference: Option<String>,
    /// The columns, in document order.
    pub columns: Vec<TableColumn>,
}

/// A parsed drawing part (`xl/drawings/drawing*.xml`): the anchor count and the
/// relationship ids of any charts and images it references. Charts are **never
/// evaluated**; only the relationship graph is exposed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DrawingModel {
    /// The number of drawing anchors (`oneCellAnchor`/`twoCellAnchor`/`absoluteAnchor`).
    pub anchors: u32,
    /// The `r:id` of each `<c:chart>` reference, in document order.
    pub chart_rel_ids: Vec<String>,
    /// The `r:embed` of each `<a:blip>` (image) reference, in document order.
    pub image_rel_ids: Vec<String>,
}

/// `true` for an XML boolean attribute written as `1` or `true`.
fn xml_flag(v: Option<&str>) -> bool {
    matches!(v, Some("1") | Some("true"))
}

/// Parse and harden an `xl/comments*.xml` part into its cell-keyed comments.
pub fn parse_comments(bytes: &[u8], limits: Limits) -> Result<Vec<CellComment>> {
    harden_xml(bytes, limits)?;
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut st = XmlState::new();
    let mut authors: Vec<String> = Vec::new();
    let mut cur_author: Option<String> = None;
    let mut comments: Vec<CellComment> = Vec::new();
    let mut cur: Option<CellComment> = None;
    let mut in_text = false;
    let mut in_t = false;
    let mut saw_root = false;
    loop {
        let ev = reader.read_event().map_err(xml_err)?;
        st.event(limits)?;
        match ev {
            Event::Eof => break,
            Event::DocType(d) => accept_doctype(&d.into_inner())?,
            Event::Start(e) => {
                st.open(limits)?;
                let local = e.name().local_name().as_ref().to_string();
                if !saw_root {
                    check_root(&e, "comments")?;
                    saw_root = true;
                }
                match local.as_str() {
                    "author" => cur_author = Some(String::new()),
                    "comment" => {
                        if comments.len() as u64 >= u64::from(limits.max_xlsx_comments) {
                            return Err(Error::resource_limit("comments exceed max_xlsx_comments"));
                        }
                        let attrs = read_attrs(&e, limits)?;
                        let author = attr_of(&attrs, "authorId")
                            .and_then(|v| v.parse::<usize>().ok())
                            .and_then(|i| authors.get(i).cloned());
                        cur = Some(CellComment {
                            cell: attr_of(&attrs, "ref").unwrap_or("").to_string(),
                            author,
                            text: String::new(),
                        });
                    }
                    "text" => in_text = true,
                    "t" if in_text => in_t = true,
                    _ => {}
                }
            }
            Event::Empty(e) => {
                st.leaf(limits)?;
                if !saw_root {
                    check_root(&e, "comments")?;
                    saw_root = true;
                }
            }
            Event::End(e) => {
                match e.name().local_name().as_ref() {
                    "author" => {
                        if let Some(a) = cur_author.take() {
                            authors.push(a);
                        }
                    }
                    "comment" => {
                        if let Some(c) = cur.take() {
                            comments.push(c);
                        }
                    }
                    "text" => in_text = false,
                    "t" => in_t = false,
                    _ => {}
                }
                st.close();
            }
            Event::Text(t) => {
                let s = t.into_inner();
                st.text(s.len(), limits)?;
                if let Some(a) = cur_author.as_mut() {
                    a.push_str(s.as_ref());
                } else if in_t && let Some(c) = cur.as_mut() {
                    c.text.push_str(s.as_ref());
                }
            }
            Event::GeneralRef(r) => {
                if let Some(a) = cur_author.as_mut() {
                    push_ref(a, r);
                } else if in_t && let Some(c) = cur.as_mut() {
                    push_ref(&mut c.text, r);
                }
            }
            _ => {}
        }
    }
    if !saw_root {
        return Err(Error::invalid_xml_structure("comments XML part is empty"));
    }
    Ok(comments)
}

/// Parse a legacy VML drawing part into its note anchors (`ObjectType="Note"`).
/// The VML root element is not constrained (VML has no single fixed root).
pub fn parse_vml_notes(bytes: &[u8], limits: Limits) -> Result<Vec<VmlNote>> {
    harden_xml(bytes, limits)?;
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut st = XmlState::new();
    let mut notes: Vec<VmlNote> = Vec::new();
    let mut shape_id: Option<String> = None;
    let mut in_note = false;
    let mut in_row = false;
    let mut in_col = false;
    let mut row: Option<u32> = None;
    let mut col: Option<u32> = None;
    loop {
        let ev = reader.read_event().map_err(xml_err)?;
        st.event(limits)?;
        match ev {
            Event::Eof => break,
            Event::DocType(d) => accept_doctype(&d.into_inner())?,
            Event::Start(e) => {
                st.open(limits)?;
                vml_element(
                    &e,
                    limits,
                    &mut shape_id,
                    &mut in_note,
                    &mut in_row,
                    &mut in_col,
                    &mut row,
                    &mut col,
                )?;
            }
            Event::Empty(e) => {
                st.leaf(limits)?;
                vml_element(
                    &e,
                    limits,
                    &mut shape_id,
                    &mut in_note,
                    &mut in_row,
                    &mut in_col,
                    &mut row,
                    &mut col,
                )?;
            }
            Event::End(e) => {
                match e.name().local_name().as_ref() {
                    "shape" => shape_id = None,
                    "ClientData" => {
                        if in_note {
                            if notes.len() as u64 >= u64::from(limits.max_xlsx_comments) {
                                return Err(Error::resource_limit(
                                    "VML notes exceed max_xlsx_comments",
                                ));
                            }
                            let cell = match (col, row) {
                                (Some(c), Some(r)) => col_row_to_a1(c, r),
                                _ => String::new(),
                            };
                            notes.push(VmlNote {
                                cell,
                                shape_id: shape_id.clone(),
                            });
                        }
                        in_note = false;
                        in_row = false;
                        in_col = false;
                        row = None;
                        col = None;
                    }
                    "Row" => in_row = false,
                    "Column" => in_col = false,
                    _ => {}
                }
                st.close();
            }
            Event::Text(t) => {
                let s = t.into_inner();
                st.text(s.len(), limits)?;
                if in_row && let Ok(v) = s.as_ref().trim().parse::<u32>() {
                    row = Some(v);
                } else if in_col && let Ok(v) = s.as_ref().trim().parse::<u32>() {
                    col = Some(v);
                }
            }
            _ => {}
        }
    }
    Ok(notes)
}

#[allow(clippy::too_many_arguments)]
fn vml_element(
    e: &BytesStart<'_>,
    limits: Limits,
    shape_id: &mut Option<String>,
    in_note: &mut bool,
    in_row: &mut bool,
    in_col: &mut bool,
    row: &mut Option<u32>,
    col: &mut Option<u32>,
) -> Result<()> {
    match e.name().local_name().as_ref() {
        "shape" => {
            let attrs = read_attrs(e, limits)?;
            *shape_id = attr_of(&attrs, "id").map(str::to_string);
        }
        "ClientData" => {
            let attrs = read_attrs(e, limits)?;
            if attr_of(&attrs, "ObjectType") == Some("Note") {
                *in_note = true;
                *row = None;
                *col = None;
            }
        }
        "Row" if *in_note => *in_row = true,
        "Column" if *in_note => *in_col = true,
        _ => {}
    }
    Ok(())
}

/// Parse and harden an `xl/tables/table*.xml` part into its name/ref/columns.
pub fn parse_table(bytes: &[u8], limits: Limits) -> Result<SheetTable> {
    harden_xml(bytes, limits)?;
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut st = XmlState::new();
    let mut table = SheetTable::default();
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
                    check_root(&e, "table")?;
                    saw_root = true;
                }
                table_element(&mut table, &e, limits)?;
            }
            Event::Empty(e) => {
                st.leaf(limits)?;
                if !saw_root {
                    check_root(&e, "table")?;
                    saw_root = true;
                }
                table_element(&mut table, &e, limits)?;
            }
            Event::End(_) => st.close(),
            Event::Text(t) => st.text(t.len(), limits)?,
            _ => {}
        }
    }
    if !saw_root {
        return Err(Error::invalid_xml_structure("table XML part is empty"));
    }
    Ok(table)
}

fn table_element(table: &mut SheetTable, e: &BytesStart<'_>, limits: Limits) -> Result<()> {
    let attrs = read_attrs(e, limits)?;
    match e.name().local_name().as_ref() {
        "table" => {
            table.name = attr_of(&attrs, "name").map(str::to_string);
            table.display_name = attr_of(&attrs, "displayName").map(str::to_string);
            table.reference = attr_of(&attrs, "ref").map(str::to_string);
        }
        "tableColumn" => {
            if table.columns.len() as u64 >= u64::from(limits.max_xlsx_table_columns) {
                return Err(Error::resource_limit(
                    "table has more columns than max_xlsx_table_columns",
                ));
            }
            let id = match attr_of(&attrs, "id") {
                Some(v) => Some(v.parse::<u32>().map_err(|_| {
                    Error::invalid_package_structure("tableColumn id is not a u32")
                })?),
                None => None,
            };
            table.columns.push(TableColumn {
                id,
                name: attr_of(&attrs, "name").unwrap_or("").to_string(),
            });
        }
        _ => {}
    }
    Ok(())
}

/// Parse and harden an `xl/drawings/drawing*.xml` part into its anchor count and
/// chart/image relationship references. Charts are never evaluated.
pub fn parse_drawing(bytes: &[u8], limits: Limits) -> Result<DrawingModel> {
    harden_xml(bytes, limits)?;
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut st = XmlState::new();
    let mut model = DrawingModel::default();
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
                    check_root(&e, "wsDr")?;
                    saw_root = true;
                }
                drawing_element(&mut model, &e, limits)?;
            }
            Event::Empty(e) => {
                st.leaf(limits)?;
                if !saw_root {
                    check_root(&e, "wsDr")?;
                    saw_root = true;
                }
                drawing_element(&mut model, &e, limits)?;
            }
            Event::End(_) => st.close(),
            Event::Text(t) => st.text(t.len(), limits)?,
            _ => {}
        }
    }
    if !saw_root {
        return Err(Error::invalid_xml_structure("drawing XML part is empty"));
    }
    Ok(model)
}

fn drawing_element(model: &mut DrawingModel, e: &BytesStart<'_>, limits: Limits) -> Result<()> {
    match e.name().local_name().as_ref() {
        "oneCellAnchor" | "twoCellAnchor" | "absoluteAnchor" => {
            model.anchors = model.anchors.saturating_add(1);
        }
        "chart" => {
            let attrs = read_attrs(e, limits)?;
            if let Some(id) = attr_of(&attrs, "id") {
                if model.chart_rel_ids.len() as u64 >= u64::from(limits.max_xlsx_drawings) {
                    return Err(Error::resource_limit("drawing references too many charts"));
                }
                model.chart_rel_ids.push(id.to_string());
            }
        }
        "blip" => {
            let attrs = read_attrs(e, limits)?;
            if let Some(id) = attr_of(&attrs, "embed") {
                if model.image_rel_ids.len() as u64 >= u64::from(limits.max_xlsx_drawings) {
                    return Err(Error::resource_limit("drawing references too many images"));
                }
                model.image_rel_ids.push(id.to_string());
            }
        }
        _ => {}
    }
    Ok(())
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
    Error::invalid_package_structure(format!("corrupt XLSX model: {msg}"))
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

fn put_opt_u32(out: &mut Vec<u8>, v: Option<u32>) {
    match v {
        Some(v) => {
            out.push(1);
            put_u32(out, v);
        }
        None => out.push(0),
    }
}

fn put_part(out: &mut Vec<u8>, p: &XlsxPartRef) {
    put_str(out, &p.name);
    put_u32(out, p.ordinal);
    put_opt_str(out, p.content_type.as_deref());
}

fn put_opt_part(out: &mut Vec<u8>, p: Option<&XlsxPartRef>) {
    match p {
        Some(p) => {
            out.push(1);
            put_part(out, p);
        }
        None => out.push(0),
    }
}

fn read_part(r: &mut ByteReader<'_>) -> Result<XlsxPartRef> {
    Ok(XlsxPartRef {
        name: r.string()?,
        ordinal: r.u32()?,
        content_type: r.opt_string()?,
    })
}

fn read_opt_part(r: &mut ByteReader<'_>) -> Result<Option<XlsxPartRef>> {
    match r.u8()? {
        0 => Ok(None),
        1 => Ok(Some(read_part(r)?)),
        _ => Err(corrupt("bad optional-part tag")),
    }
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
        let p = XlsxExtractProfile::DEFAULT;
        assert_eq!(XlsxExtractProfile::decode(&p.encode()).unwrap(), p);
        assert!(p.fingerprint().contains("cached"));
        let mut q = p;
        q.values = ValueMode::Formula;
        q.include_hidden = true;
        assert_ne!(q.encode(), p.encode());
        assert_eq!(XlsxExtractProfile::decode(&q.encode()).unwrap(), q);
    }

    #[test]
    fn a1_roundtrips() {
        for (col, row, a1) in [
            (0u32, 0u32, "A1"),
            (1, 6, "B7"),
            (25, 0, "Z1"),
            (26, 0, "AA1"),
        ] {
            assert_eq!(col_row_to_a1(col, row), a1);
            assert_eq!(a1_to_col_row(a1), Some((col, row)));
        }
        assert_eq!(a1_to_col_row(""), None);
        assert_eq!(a1_to_col_row("7B"), None);
    }

    #[test]
    fn model_roundtrip() {
        let shape = |s: &str, o: u32| XlsxPartRef {
            name: s.to_string(),
            ordinal: o,
            content_type: Some("ct".to_string()),
        };
        let m = XlsxModel {
            workbook: shape("/xl/workbook.xml", 2),
            styles: Some(shape("/xl/styles.xml", 3)),
            shared_strings: Some(shape("/xl/sharedStrings.xml", 4)),
            sheets: vec![XlsxSheetRef {
                rel_id: Some("rId1".to_string()),
                order: 0,
                part: shape("/xl/worksheets/sheet1.xml", 5),
            }],
        };
        assert_eq!(XlsxModel::decode(&m.encode()).unwrap(), m);
    }

    #[test]
    fn workbook_and_sheet_roundtrip() {
        let wb = WorkbookModel {
            sheets: vec![
                WorkbookSheet {
                    name: "Alpha".to_string(),
                    sheet_id: Some(1),
                    rel_id: Some("rId1".to_string()),
                    state: SheetState::Visible,
                },
                WorkbookSheet {
                    name: "Hidden".to_string(),
                    sheet_id: Some(2),
                    rel_id: Some("rId2".to_string()),
                    state: SheetState::Hidden,
                },
            ],
            defined_names: vec![DefinedName {
                name: "TaxRate".to_string(),
                local_sheet_id: Some(0),
                hidden: false,
                function: false,
                refers_to: "Sheet1!$A$1".to_string(),
            }],
        };
        assert_eq!(WorkbookModel::decode(&wb.encode()).unwrap(), wb);

        let sheet = SheetModel {
            part_name: "/xl/worksheets/sheet1.xml".to_string(),
            sheet_name: "Alpha".to_string(),
            dimension: Some("A1:B2".to_string()),
            merges: vec!["A1:B1".to_string()],
            hyperlinks: vec![SheetHyperlink {
                reference: "A1".to_string(),
                rel_id: Some("rId3".to_string()),
                location: None,
                display: Some("site".to_string()),
                tooltip: None,
            }],
            table_parts: vec!["rId4".to_string()],
            drawing_rel_id: Some("rId5".to_string()),
            legacy_drawing_rel_id: None,
            rows: vec![XlsxRow {
                index: 0,
                cells: vec![XlsxCell {
                    reference: "A1".to_string(),
                    col: 0,
                    row: 0,
                    type_tag: Some("s".to_string()),
                    value: Some("hello".to_string()),
                    formula: None,
                    style: Some(0),
                    span_start: 10,
                    span_len: 20,
                }],
            }],
        };
        assert_eq!(SheetModel::decode(&sheet.encode()).unwrap(), sheet);
    }

    #[test]
    fn parses_a_minimal_worksheet() {
        let xml = br#"<?xml version="1.0"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<dimension ref="A1:B2"/>
<sheetData>
<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1"><v>42</v></c></row>
<row r="2"><c r="A2"><f>SUM(B1:B1)</f><v>42</v></c></row>
</sheetData>
<mergeCells count="1"><mergeCell ref="A1:B1"/></mergeCells>
</worksheet>"#;
        let shared = vec!["hello".to_string()];
        let m = parse_worksheet(
            xml,
            "/xl/worksheets/sheet1.xml",
            "S",
            Some(&shared),
            Limits::DEFAULT,
        )
        .unwrap();
        assert_eq!(m.dimension.as_deref(), Some("A1:B2"));
        assert_eq!(m.merges, vec!["A1:B1".to_string()]);
        assert_eq!(m.cell_at(0, 0).unwrap().value.as_deref(), Some("hello"));
        assert_eq!(m.cell_at(0, 1).unwrap().value.as_deref(), Some("42"));
        let f = m.cell_at(1, 0).unwrap();
        assert_eq!(f.formula.as_deref(), Some("SUM(B1:B1)"));
        assert_eq!(f.value.as_deref(), Some("42"));
        assert!(f.span_len > 0);
        assert_eq!(
            m.text(ValueMode::Cached, u64::MAX).unwrap(),
            "hello\t42\n42"
        );
        assert_eq!(
            m.text(ValueMode::Formula, u64::MAX).unwrap(),
            "\t\nSUM(B1:B1)"
        );
        // The projection is bounded: a bound below the projected grid size
        // declines typed instead of building the string.
        assert_eq!(
            m.text(ValueMode::Cached, 2).unwrap_err().class(),
            crate::ErrorClass::ResourceLimit
        );
    }

    #[test]
    fn parses_workbook_and_shared_strings_and_styles() {
        let wb = br#"<workbook xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Alpha" sheetId="1" r:id="rId1"/><sheet name="Hid" sheetId="2" r:id="rId2" state="hidden"/></sheets></workbook>"#;
        let m = parse_workbook(wb, Limits::DEFAULT).unwrap();
        assert_eq!(m.sheets.len(), 2);
        assert_eq!(m.sheets[0].name, "Alpha");
        assert_eq!(m.sheets[0].rel_id.as_deref(), Some("rId1"));
        assert_eq!(m.sheets[1].state, SheetState::Hidden);

        let sst = br#"<sst><si><t>a&amp;b</t></si><si><r><t>x</t></r><r><t>y</t></r></si></sst>"#;
        let table = parse_shared_strings(sst, Limits::DEFAULT).unwrap();
        assert_eq!(table, vec!["a&b".to_string(), "xy".to_string()]);

        let styles = br#"<styleSheet><numFmts count="1"><numFmt numFmtId="164" formatCode="0.00"/></numFmts><fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><b/><i/><sz val="14"/><name val="Arial"/></font></fonts><fills count="1"><fill><patternFill patternType="solid"><fgColor rgb="FFFF0000"/></patternFill></fill></fills><cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0"/><xf numFmtId="164" fontId="1" fillId="0"><alignment horizontal="center" wrapText="1"/></xf></cellXfs></styleSheet>"#;
        let st = parse_styles_table(styles, Limits::DEFAULT).unwrap();
        assert_eq!(st.cell_style(1).unwrap().num_fmt_id, 164);
        assert_eq!(st.num_fmt_code(164), Some("0.00"));
        let style = st.style_for(1).unwrap();
        assert_eq!(style.format_code.as_deref(), Some("0.00"));
        assert_eq!(style.font.as_ref().map(|f| f.bold), Some(true));
        assert_eq!(style.font.as_ref().map(|f| f.italic), Some(true));
        assert_eq!(
            style.font.as_ref().and_then(|f| f.name.as_deref()),
            Some("Arial")
        );
        assert_eq!(
            style.fill.as_ref().and_then(|f| f.pattern_type.as_deref()),
            Some("solid")
        );
        assert_eq!(
            style
                .alignment
                .as_ref()
                .and_then(|a| a.horizontal.as_deref()),
            Some("center")
        );
        assert!(style.alignment.as_ref().is_some_and(|a| a.wrap_text));
        // A bounded, deterministic display projection.
        assert_eq!(format_displayed("42", "0.00").as_deref(), Some("42.00"));
        assert_eq!(format_displayed("0.5", "0.00%").as_deref(), Some("50.00%"));
        assert_eq!(format_displayed("1234", "#,##0").as_deref(), Some("1,234"));
        assert_eq!(format_displayed("x", "0.00"), None);
        assert_eq!(format_displayed("42", "[$-409]d-mmm"), None);
    }

    #[test]
    fn wrong_root_is_declined() {
        assert!(parse_worksheet(b"<html></html>", "/x", "S", None, Limits::DEFAULT).is_err());
        assert!(parse_workbook(b"<document/>", Limits::DEFAULT).is_err());
    }
}
