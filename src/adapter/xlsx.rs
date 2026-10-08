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

/// The parsed workbook sheet inventory, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkbookModel {
    /// The declared sheets, in document order.
    pub sheets: Vec<WorkbookSheet>,
}

impl WorkbookModel {
    /// Encode canonically.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"XLWB");
        out.push(1);
        put_u32(&mut out, self.sheets.len() as u32);
        for s in &self.sheets {
            put_str(&mut out, &s.name);
            put_opt_u32(&mut out, s.sheet_id);
            put_opt_str(&mut out, s.rel_id.as_deref());
            out.push(s.state.tag());
        }
        out
    }

    /// Decode a model produced by [`Self::encode`].
    pub fn decode(bytes: &[u8]) -> Result<WorkbookModel> {
        let mut r = ByteReader::new(bytes);
        if r.bytes(4)? != b"XLWB" {
            return Err(corrupt("bad XLSX workbook magic"));
        }
        if r.u8()? != 1 {
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
        if !r.at_end() {
            return Err(corrupt("XLSX workbook has trailing bytes"));
        }
        Ok(WorkbookModel { sheets })
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
    pub fn text(&self, mode: ValueMode) -> String {
        let mut out = String::new();
        for (i, row) in self.rows.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            let max_col = row.cells.iter().map(|c| c.col).max();
            if let Some(max_col) = max_col {
                for col in 0..=max_col {
                    if col > 0 {
                        out.push('\t');
                    }
                    if let Some(c) = row.cells.iter().find(|c| c.col == col)
                        && let Some(v) = self.facet(c, mode)
                    {
                        out.push_str(&v);
                    }
                }
            }
        }
        out
    }

    /// Encode canonically.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"XLSH");
        out.push(1);
        put_str(&mut out, &self.part_name);
        put_str(&mut out, &self.sheet_name);
        put_opt_str(&mut out, self.dimension.as_deref());
        put_u32(&mut out, self.merges.len() as u32);
        for m in &self.merges {
            put_str(&mut out, m);
        }
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
        if r.u8()? != 1 {
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
            rows,
        })
    }
}

// ---------------------------------------------------------------------------
// Styles (minimal cell-style table)
// ---------------------------------------------------------------------------

/// One `cellXfs` entry: a resolved reference set into the styles substreams.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellXf {
    /// `numFmtId` (builtin or custom).
    pub num_fmt_id: u32,
    /// `fontId`.
    pub font_id: u32,
    /// `fillId`.
    pub fill_id: u32,
}

/// A minimal cell-style table: custom number formats and the `cellXfs` refs.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StylesTable {
    /// `numFmtId` → `formatCode` for custom (`numFmtId >= 164`) formats.
    pub num_fmts: BTreeMap<u32, String>,
    /// The `cellXfs` entries, in order.
    pub cell_xfs: Vec<CellXf>,
}

impl StylesTable {
    /// The style ref for a cell-style index (`s`), if present.
    pub fn cell_style(&self, index: u32) -> Option<CellXf> {
        self.cell_xfs.get(index as usize).copied()
    }

    /// The number-format code for a `numFmtId`, if a custom format declares one.
    pub fn num_fmt_code(&self, num_fmt_id: u32) -> Option<&str> {
        self.num_fmts.get(&num_fmt_id).map(String::as_str)
    }
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

/// Parse and harden `xl/workbook.xml` into its sheet inventory.
pub fn parse_workbook(bytes: &[u8], limits: Limits) -> Result<WorkbookModel> {
    harden_xml(bytes, limits)?;
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut st = XmlState::new();
    let mut sheets: Vec<WorkbookSheet> = Vec::new();
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
                workbook_element(&mut sheets, &e, limits)?;
            }
            Event::Empty(e) => {
                st.leaf(limits)?;
                if !saw_root {
                    check_root(&e, "workbook")?;
                    saw_root = true;
                }
                workbook_element(&mut sheets, &e, limits)?;
            }
            Event::End(_) => st.close(),
            Event::Text(t) => st.text(t.len(), limits)?,
            _ => {}
        }
    }
    if !saw_root {
        return Err(Error::invalid_xml_structure("workbook XML part is empty"));
    }
    Ok(WorkbookModel { sheets })
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

/// Parse and harden `xl/styles.xml` into its minimal cell-style table.
pub fn parse_styles_table(bytes: &[u8], limits: Limits) -> Result<StylesTable> {
    harden_xml(bytes, limits)?;
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut st = XmlState::new();
    let mut table = StylesTable::default();
    let mut in_cell_xfs = false;
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
                    "cellXfs" => in_cell_xfs = true,
                    "numFmt" => styles_num_fmt(&mut table, &e, limits)?,
                    "xf" if in_cell_xfs => styles_xf(&mut table, &e, limits)?,
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
                    "numFmt" => styles_num_fmt(&mut table, &e, limits)?,
                    "xf" if in_cell_xfs => styles_xf(&mut table, &e, limits)?,
                    _ => {}
                }
            }
            Event::End(e) => {
                if e.name().local_name().as_ref() == "cellXfs" {
                    in_cell_xfs = false;
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

fn styles_xf(table: &mut StylesTable, e: &BytesStart<'_>, limits: Limits) -> Result<()> {
    let attrs = read_attrs(e, limits)?;
    let num_fmt_id = parse_u32_attr(&attrs, "numFmtId")?;
    let font_id = parse_u32_attr(&attrs, "fontId")?;
    let fill_id = parse_u32_attr(&attrs, "fillId")?;
    table.cell_xfs.push(CellXf {
        num_fmt_id,
        font_id,
        fill_id,
    });
    Ok(())
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
                    b"row" => {
                        let attrs = read_attrs(&e, limits)?;
                        let index = row_index(&attrs)?;
                        cur_row = Some((index, Vec::new()));
                    }
                    b"c" => {
                        let attrs = read_attrs(&e, limits)?;
                        cell_pos_before = pos_before;
                        cur_cell = Some(start_cell(&attrs, &cur_row)?);
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
                    b"c" => {
                        let attrs = read_attrs(&e, limits)?;
                        let start = u32::try_from(pos_before).unwrap_or(0);
                        let end = u32::try_from(reader.buffer_position()).unwrap_or(start);
                        let mut cell = start_cell(&attrs, &cur_row)?;
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
        if let Some((col, row)) = a1_to_col_row(r) {
            c.col = col;
            c.row = row;
        }
    } else if let Some((_, cells)) = cur_row {
        c.col = cells.iter().map(|c| c.col + 1).max().unwrap_or(0);
    }
    Ok(c)
}

/// Parse an A1-style reference (`B7`) into a 0-based `(column, row)`.
pub fn a1_to_col_row(reference: &str) -> Option<(u32, u32)> {
    let bytes = reference.as_bytes();
    let mut i = 0;
    let mut col: u64 = 0;
    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
        let upper = bytes[i].to_ascii_uppercase();
        col = col * 26 + u64::from(upper - b'A' + 1);
        i += 1;
    }
    if i == 0 || i >= bytes.len() {
        return None;
    }
    let mut row: u64 = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            return None;
        }
        row = row * 10 + u64::from(bytes[i] - b'0');
        i += 1;
    }
    if col == 0 || row == 0 {
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
        };
        assert_eq!(WorkbookModel::decode(&wb.encode()).unwrap(), wb);

        let sheet = SheetModel {
            part_name: "/xl/worksheets/sheet1.xml".to_string(),
            sheet_name: "Alpha".to_string(),
            dimension: Some("A1:B2".to_string()),
            merges: vec!["A1:B1".to_string()],
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
        assert_eq!(m.text(ValueMode::Cached), "hello\t42\n42");
        assert_eq!(m.text(ValueMode::Formula), "\t\nSUM(B1:B1)");
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

        let styles = br#"<styleSheet><numFmts count="1"><numFmt numFmtId="164" formatCode="0.00"/></numFmts><cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0"/><xf numFmtId="164" fontId="1" fillId="0"/></cellXfs></styleSheet>"#;
        let st = parse_styles_table(styles, Limits::DEFAULT).unwrap();
        assert_eq!(st.cell_style(1).unwrap().num_fmt_id, 164);
        assert_eq!(st.num_fmt_code(164), Some("0.00"));
    }

    #[test]
    fn wrong_root_is_declined() {
        assert!(parse_worksheet(b"<html></html>", "/x", "S", None, Limits::DEFAULT).is_err());
        assert!(parse_workbook(b"<document/>", Limits::DEFAULT).is_err());
    }
}
