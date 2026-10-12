//! Bounded, representation-preserving legacy Excel 97–2003 (`.xls`, BIFF8)
//! **semantic** adapter (Phase 26.2), built on the CFBF/OLE2 physical layer (21.35).
//!
//! A `.xls` is a CFBF compound file whose `Workbook` stream (the legacy `Book` name
//! is accepted too) is a **BIFF record stream**: a byte-packed sequence of
//! `[u16 record id][u16 length][payload]` records inside `BOF`/`EOF` substreams. This
//! adapter **reuses the CFBF resolver** ([`crate::adapter::cfbf`]) to obtain that
//! stream as an exact byte span — it never writes a second OLE parser — and adds only
//! a bounded semantic model over the records.
//!
//! ## What is authoritative
//!
//! Like every Wave-2 format, a `.xls` has **no *separate* package layer**: the exact
//! leaf is the whole source (a `DocumentExact`) and every derived observation is a
//! bounded, span-preserving (`Q_gen`) projection of it. Nothing is re-encoded,
//! re-serialized, or re-OLE'd for exactness.
//!
//! ## What is parsed (the documented subset)
//!
//! * the **record stream** of the `Workbook`/`Book` stream: every
//!   `[u16 id][u16 length][payload]` record with its **exact byte span** (and its
//!   offset within the stream), in order. A truncated header or a payload that
//!   overruns the stream declines typed;
//! * the **`BOF` (`0x0809`)** framing and the **BIFF version** (`0x0600` = BIFF8):
//!   the stream must open with a `BOF` whose version is `0x0600` and whose substream
//!   type is a recognized one; the leading substream must be the **workbook globals**
//!   (`0x0005`). Each `BOF` opens a substream terminated by **`EOF` (`0x000A`)**;
//! * **`BOUNDSHEET` (`0x0085`)**: the sheet's `lbPlyPos` (its `BOF` stream position),
//!   `hsState` (visibility), `dt` (sheet type), and its name as a
//!   `ShortXLUnicodeString` (a 1-byte `cch`, an option-flags byte, then the
//!   compressed-8-bit or UTF-16LE characters), in order;
//! * the **`SST` (`0x00FC`)** shared-string table: `cstTotal`/`cstUnique` and each
//!   `XLUnicodeRichExtendedString` — its option flags (`fHighByte`, `fRichSt`,
//!   `fExtSt`), its `cch`, the rich-run count and extended-phonetic size, and its
//!   decoded characters. **`CONTINUE` (`0x003C`)** reassembly is supported: when a
//!   string's character array crosses a record boundary, the continuation record's
//!   leading option byte selects the width of the remaining characters;
//! * the **cell records** of each sheet substream, in sheet order, keeping the
//!   sheet/row grouping: **`LABELSST` (`0x00FD`)**, **`NUMBER` (`0x0203`)**,
//!   **`RK` (`0x027E`)**, **`BOOLERR` (`0x0205`)**, **`FORMULA` (`0x0006`)** with its
//!   cached-result spelling and its following **`STRING` (`0x0207`)** cached result,
//!   and **`BLANK` (`0x0201`)**/**`MULBLANK` (`0x00BE`)** (a `MULBLANK` run expands to
//!   one cell per column). Each cell records its row/column, its **exact record span**,
//!   and its **exact stored value spelling** (the `RK` compact numeric encoding is
//!   decoded by [`rk_value`]);
//! * **`DIMENSIONS` (`0x0200`)**, **`ROW` (`0x0208`)**, **`FORMAT` (`0x041E`)**, and
//!   **`XF` (`0x00E0`)** are recorded (their exact spans; `DIMENSIONS`/`ROW` per sheet).
//!
//! Record order and each record's **exact byte span** are preserved; nothing is
//! normalized or re-serialized.
//!
//! ## The boundary it cannot cross (honest)
//!
//! Detection is built **on top of** the CFBF detector: a source is claimed only when
//! the CFBF layer accepts it **and** its `Workbook`/`Book` stream begins with a
//! **BIFF8 `BOF`** (id `0x0809`, version `0x0600`) whose substream type is a
//! recognized worksheet/globals type. A generic compound file without such a `BOF`
//! stays [`Cfbf`](crate::field::document_format::DocumentFormat::Cfbf); a non-CFBF
//! source is unaffected; a `.xlsx` (an OPC ZIP) never reaches here.
//!
//! The **BIFF5/BIFF7** variants (an older `Book` stream with version `0x0500`/
//! `0x0700` and 1-byte string lengths) and OLE2 streams whose chain is **not
//! contiguous in the source** are **not** this format: the former fails the BIFF8
//! version check (and would be recorded honestly as `Cfbf` if the compound file is
//! otherwise valid), the latter declines typed at the reused CFBF resolver.
//!
//! A `.xls` that carries a plausible BIFF8 `BOF` but a malformed or out-of-subset body
//! (a bad `BOF` version, an unrecognized substream type, a leading substream that is
//! not workbook globals, a truncated record, an out-of-range `BOUNDSHEET` position,
//! an `SST` count mismatch or a malformed `CONTINUE`, a `FORMULA` string result with
//! no `STRING` record) is admitted by detection (its whole-source exact leaf is
//! unaffected) and its derived model then **declines typed**
//! ([`crate::ErrorClass::InvalidXlsStructure`]). Never a panic, never a guess.

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Wire version of the canonical derived model.
pub const MODEL_VERSION: u8 = 1;
/// Hard cap on decoded model records (defends the decoder against a hostile blob).
pub const MAX_MODEL_RECORDS: u32 = 1 << 24;
/// Hard cap on decoded model sheets.
pub const MAX_MODEL_SHEETS: u32 = 1 << 20;
/// Hard cap on decoded shared strings.
pub const MAX_MODEL_STRINGS: u32 = 1 << 24;
/// Hard cap on decoded cells.
pub const MAX_MODEL_CELLS: u32 = 1 << 26;
/// Hard cap on decoded text code units (SST + sheet names + formula strings).
pub const MAX_MODEL_TEXT_UNITS: u32 = 1 << 30;

/// `EOF` record (`0x000A`).
pub const REC_EOF: u16 = 0x000A;
/// `FORMULA` record (`0x0006`).
pub const REC_FORMULA: u16 = 0x0006;
/// `CONTINUE` record (`0x003C`).
pub const REC_CONTINUE: u16 = 0x003C;
/// `BOUNDSHEET` record (`0x0085`).
pub const REC_BOUNDSHEET: u16 = 0x0085;
/// `MULBLANK` record (`0x00BE`).
pub const REC_MULBLANK: u16 = 0x00BE;
/// `XF` record (`0x00E0`).
pub const REC_XF: u16 = 0x00E0;
/// `SST` record (`0x00FC`).
pub const REC_SST: u16 = 0x00FC;
/// `LABELSST` record (`0x00FD`).
pub const REC_LABELSST: u16 = 0x00FD;
/// `DIMENSIONS` record (`0x0200`).
pub const REC_DIMENSIONS: u16 = 0x0200;
/// `BLANK` record (`0x0201`).
pub const REC_BLANK: u16 = 0x0201;
/// `NUMBER` record (`0x0203`).
pub const REC_NUMBER: u16 = 0x0203;
/// `BOOLERR` record (`0x0205`).
pub const REC_BOOLERR: u16 = 0x0205;
/// `STRING` record (`0x0207`).
pub const REC_STRING: u16 = 0x0207;
/// `ROW` record (`0x0208`).
pub const REC_ROW: u16 = 0x0208;
/// `RK` record (`0x027E`).
pub const REC_RK: u16 = 0x027E;
/// `FORMAT` record (`0x041E`).
pub const REC_FORMAT: u16 = 0x041E;
/// `BOF` record (`0x0809`).
pub const REC_BOF: u16 = 0x0809;

/// The BIFF8 version word (`0x0600`).
pub const BIFF8_VERSION: u16 = 0x0600;
/// Substream type: workbook globals.
pub const ST_WORKBOOK_GLOBALS: u16 = 0x0005;
/// Substream type: worksheet or dialog sheet.
pub const ST_WORKSHEET: u16 = 0x0010;
/// Substream type: chart.
pub const ST_CHART: u16 = 0x0020;
/// Substream type: macro sheet.
pub const ST_MACRO: u16 = 0x0040;

/// The `Workbook` stream name (UTF-16LE, NUL-terminated in the directory).
pub const STREAM_WORKBOOK: &str = "Workbook";
/// The legacy `Book` stream name (BIFF5/7-era compound files).
pub const STREAM_BOOK: &str = "Book";

/// Recorded stream name marker: `Workbook`.
pub const NAME_WORKBOOK: u8 = 0;
/// Recorded stream name marker: legacy `Book`.
pub const NAME_BOOK: u8 = 1;

/// Cell kind: a `LABELSST` (an SST index).
pub const CELL_LABELSST: u8 = 0;
/// Cell kind: a `NUMBER` (an IEEE-754 double).
pub const CELL_NUMBER: u8 = 1;
/// Cell kind: an `RK` (a compact numeric encoding).
pub const CELL_RK: u8 = 2;
/// Cell kind: a `BOOLERR`.
pub const CELL_BOOLERR: u8 = 3;
/// Cell kind: a `FORMULA` (its cached-result spelling).
pub const CELL_FORMULA: u8 = 4;
/// Cell kind: a `BLANK`.
pub const CELL_BLANK: u8 = 5;
/// Cell kind: a column of a `MULBLANK` run.
pub const CELL_MULBLANK: u8 = 6;

/// `FORMULA` cached-result marker: the result is numeric (not special).
pub const FORMULA_NUMERIC: u8 = 0xFF;

/// Match field: a sheet name.
pub const FIELD_SHEET: u8 = 0;
/// Match field: a shared-string-table string.
pub const FIELD_STRING: u8 = 1;

/// Stable match-field name.
pub const fn field_name(field: u8) -> &'static str {
    match field {
        FIELD_STRING => "string",
        _ => "sheet",
    }
}

/// A stable stream-name for the recorded marker.
pub const fn stream_name(name: u8) -> &'static str {
    match name {
        NAME_BOOK => STREAM_BOOK,
        _ => STREAM_WORKBOOK,
    }
}

/// A stable name for a BIFF substream type.
pub const fn substream_name(t: u16) -> &'static str {
    match t {
        ST_WORKBOOK_GLOBALS => "globals",
        ST_WORKSHEET => "worksheet",
        ST_CHART => "chart",
        ST_MACRO => "macro",
        _ => "unknown",
    }
}

/// Whether a BIFF substream type is one this adapter recognizes.
pub const fn is_substream_type(t: u16) -> bool {
    matches!(t, ST_WORKBOOK_GLOBALS | ST_WORKSHEET | ST_CHART | ST_MACRO)
}

/// A stable name for a `BOUNDSHEET` sheet type (`dt`).
pub const fn sheet_type_name(dt: u8) -> &'static str {
    match dt {
        0x00 => "worksheet",
        0x01 => "macro",
        0x02 => "chart",
        0x06 => "vbmodule",
        _ => "unknown",
    }
}

/// A stable name for a cell kind.
pub const fn cell_kind_name(kind: u8) -> &'static str {
    match kind {
        CELL_LABELSST => "labelsst",
        CELL_NUMBER => "number",
        CELL_RK => "rk",
        CELL_BOOLERR => "boolerr",
        CELL_FORMULA => "formula",
        CELL_BLANK => "blank",
        CELL_MULBLANK => "mulblank",
        _ => "unknown",
    }
}

/// The BIFF version name (`biff8`), for a recorded version word.
pub const fn version_name(version: u16) -> &'static str {
    match version {
        BIFF8_VERSION => "biff8",
        0x0500 => "biff5",
        0x0700 => "biff7",
        _ => "biff",
    }
}

/// The 16-bit number of a `BOUNDSHEET` visibility state.
pub const fn state_name(state: u8) -> &'static str {
    match state & 0x03 {
        0 => "visible",
        1 => "hidden",
        2 => "veryhidden",
        _ => "unknown",
    }
}

/// Decode an `RK` (`0x027E`) compact numeric value into an `f64`.
pub fn rk_value(rk: u32) -> f64 {
    let f_x100 = rk & 0x0000_0001 != 0;
    let f_int = rk & 0x0000_0002 != 0;
    let mut v = if f_int {
        ((rk as i32) >> 2) as f64
    } else {
        f64::from_bits(((rk & 0xFFFF_FFFC) as u64) << 32)
    };
    if f_x100 {
        v /= 100.0;
    }
    v
}

/// One BIFF record with its exact source span and stream offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XlsRecord {
    /// The record id.
    pub id: u16,
    /// `(offset, length)` of the whole record (header + payload) in the source.
    pub span: (u64, u64),
    /// The record's offset within the `Workbook`/`Book` stream.
    pub stream_off: u64,
    /// The payload length in bytes.
    pub len: u32,
}

impl XlsRecord {
    /// `(offset, length)` of the record's payload in the source.
    pub fn payload_span(&self) -> (u64, u64) {
        (self.span.0 + 4, self.span.0 + 4 + self.len as u64)
    }
}

/// One shared string (`XLUnicodeRichExtendedString`) of the `SST`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XlsSstString {
    /// The option-flags byte (bit 0 `fHighByte`, bit 2 `fExtSt`, bit 3 `fRichSt`).
    pub flags: u8,
    /// Whether the characters are compressed 8-bit (`fHighByte == 0`).
    pub compressed: bool,
    /// Whether the string carries rich-text runs (`fRichSt`).
    pub rich: bool,
    /// Whether the string carries extended phonetic data (`fExtSt`).
    pub ext: bool,
    /// The decoded characters (exact).
    pub text: Vec<u16>,
    /// `(offset, length)` of the record the string begins in.
    pub rec_span: (u64, u64),
    /// The number of records the string's bytes span (1 unless `CONTINUE`-split).
    pub n_parts: u32,
}

/// The `SST` (`0x00FC`) shared-string table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XlsSst {
    /// `cstTotal` (the total count of string references).
    pub cst_total: u32,
    /// `cstUnique` (the unique string count).
    pub cst_unique: u32,
    /// The unique strings, in order.
    pub strings: Vec<XlsSstString>,
    /// `(offset, length)` of the `SST` record and each of its `CONTINUE` records.
    pub record_spans: Vec<(u64, u64)>,
}

impl XlsSst {
    /// The shared string at `index`, if present.
    pub fn string(&self, index: u32) -> Option<&XlsSstString> {
        self.strings.get(index as usize)
    }
}

/// One sheet (`BOUNDSHEET`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XlsSheet {
    /// The sheet name (UTF-16 code units; exact).
    pub name: Vec<u16>,
    /// The name's option-flags byte.
    pub name_flags: u8,
    /// Whether the name is compressed 8-bit.
    pub name_compressed: bool,
    /// `lbPlyPos`: the stream position of the sheet's `BOF`.
    pub pos: u32,
    /// `dt`: the sheet type.
    pub dt: u8,
    /// `hsState`: the visibility state.
    pub state: u8,
    /// `(offset, length)` of the `BOUNDSHEET` record.
    pub rec_span: (u64, u64),
    /// `(offset, length)` of the sheet's `BOF` record.
    pub bof_span: (u64, u64),
    /// `(offset, length)` of the sheet's `DIMENSIONS` record (or `(0, 0)` if absent).
    pub dim_span: (u64, u64),
    /// The exact spans of the sheet's `ROW` records, in order.
    pub row_spans: Vec<(u64, u64)>,
    /// The sheet's cell arena range `[cell_start, cell_end)`.
    pub cell_start: u32,
    pub cell_end: u32,
}

/// One cell record (or one `MULBLANK` column).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XlsCell {
    /// The cell kind (`CELL_*`).
    pub kind: u8,
    /// The zero-based row.
    pub row: u32,
    /// The zero-based column.
    pub col: u32,
    /// `(offset, length)` of the whole cell record.
    pub rec_span: (u64, u64),
    /// `LABELSST`: the shared-string index.
    pub isst: u32,
    /// `RK`: the raw compact numeric encoding.
    pub rk_raw: u32,
    /// `NUMBER`/`FORMULA`: the exact 8-byte stored spelling.
    pub num_bits: u64,
    /// `BOOLERR`: the value byte.
    pub bool_err: u8,
    /// `BOOLERR`: whether the value is an error (else a boolean).
    pub is_error: bool,
    /// `FORMULA`: the cached-result special marker ([`FORMULA_NUMERIC`] = numeric).
    pub formula_special: u8,
    /// `FORMULA`: whether a following `STRING` record carries the cached string.
    pub has_string: bool,
    /// `FORMULA`: `(offset, length)` of the cached `STRING` record (or `(0, 0)`).
    pub string_span: (u64, u64),
    /// `FORMULA`: the cached string result (exact).
    pub text: Vec<u16>,
}

/// The canonical derived legacy-XLS model (the materialization of an `XlsModel` node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XlsModel {
    /// The exact source length the spans are relative to.
    pub doc_len: u64,
    /// `(offset, length)` of the `Workbook`/`Book` stream in the source.
    pub wb_span: (u64, u64),
    /// Which stream name was used ([`NAME_WORKBOOK`] or [`NAME_BOOK`]).
    pub stream_name: u8,
    /// The BIFF version word (`0x0600`).
    pub biff_version: u16,
    /// The leading substream type (`0x0005` = workbook globals).
    pub root_substream: u16,
    /// Every record of the stream, in order.
    pub records: Vec<XlsRecord>,
    /// The shared-string table, if present.
    pub sst: Option<XlsSst>,
    /// The sheets, in `BOUNDSHEET` order.
    pub sheets: Vec<XlsSheet>,
    /// The cell arena (sheet order, then record order).
    pub cells: Vec<XlsCell>,
    /// The exact spans of the `XF` records, in order.
    pub xf_spans: Vec<(u64, u64)>,
    /// The exact spans of the `FORMAT` records, in order.
    pub format_spans: Vec<(u64, u64)>,
}

impl XlsModel {
    /// The sheet at `index`, if present.
    pub fn sheet(&self, index: u32) -> Option<&XlsSheet> {
        self.sheets.get(index as usize)
    }

    /// The cell at arena `index`, if present.
    pub fn cell(&self, index: u32) -> Option<&XlsCell> {
        self.cells.get(index as usize)
    }

    /// The cell arena slice of a sheet.
    pub fn sheet_cells(&self, sheet: &XlsSheet) -> &[XlsCell] {
        let s = sheet.cell_start as usize;
        let e = sheet.cell_end as usize;
        self.cells.get(s..e).unwrap_or(&[])
    }

    /// The decoded text of a `LABELSST` cell, if it resolves.
    pub fn label_text(&self, cell: &XlsCell) -> Option<String> {
        if cell.kind != CELL_LABELSST {
            return None;
        }
        let s = self.sst.as_ref()?.string(cell.isst)?;
        Some(String::from_utf16_lossy(&s.text))
    }

    /// The declared character count of the shared-string-table record series.
    pub fn text_units(&self) -> u64 {
        let mut n = 0u64;
        if let Some(sst) = &self.sst {
            n = n.saturating_add(sst.strings.iter().map(|s| s.text.len() as u64).sum());
        }
        n = n.saturating_add(self.sheets.iter().map(|s| s.name.len() as u64).sum());
        n = n.saturating_add(self.cells.iter().map(|c| c.text.len() as u64).sum::<u64>());
        n
    }

    /// Deterministically encode the model (little-endian, length-prefixed).
    pub fn encode(&self) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        out.extend_from_slice(b"XLSM");
        out.push(MODEL_VERSION);
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        w_span(&mut out, self.wb_span);
        out.push(self.stream_name);
        out.extend_from_slice(&self.biff_version.to_le_bytes());
        out.extend_from_slice(&self.root_substream.to_le_bytes());
        // records
        out.extend_from_slice(&(self.records.len() as u32).to_le_bytes());
        for r in &self.records {
            out.extend_from_slice(&r.id.to_le_bytes());
            w_span(&mut out, r.span);
            out.extend_from_slice(&r.stream_off.to_le_bytes());
            out.extend_from_slice(&r.len.to_le_bytes());
        }
        // sst
        match &self.sst {
            None => out.push(0),
            Some(sst) => {
                out.push(1);
                out.extend_from_slice(&sst.cst_total.to_le_bytes());
                out.extend_from_slice(&sst.cst_unique.to_le_bytes());
                out.extend_from_slice(&(sst.record_spans.len() as u32).to_le_bytes());
                for s in &sst.record_spans {
                    w_span(&mut out, *s);
                }
                out.extend_from_slice(&(sst.strings.len() as u32).to_le_bytes());
                for s in &sst.strings {
                    out.push(s.flags);
                    out.push(s.compressed as u8);
                    out.push(s.rich as u8);
                    out.push(s.ext as u8);
                    w_span(&mut out, s.rec_span);
                    out.extend_from_slice(&s.n_parts.to_le_bytes());
                    w_units(&mut out, &s.text);
                }
            }
        }
        // sheets
        out.extend_from_slice(&(self.sheets.len() as u32).to_le_bytes());
        for s in &self.sheets {
            out.push(s.name_flags);
            out.push(s.name_compressed as u8);
            w_units(&mut out, &s.name);
            out.extend_from_slice(&s.pos.to_le_bytes());
            out.push(s.dt);
            out.push(s.state);
            w_span(&mut out, s.rec_span);
            w_span(&mut out, s.bof_span);
            w_span(&mut out, s.dim_span);
            out.extend_from_slice(&(s.row_spans.len() as u32).to_le_bytes());
            for rs in &s.row_spans {
                w_span(&mut out, *rs);
            }
            out.extend_from_slice(&s.cell_start.to_le_bytes());
            out.extend_from_slice(&s.cell_end.to_le_bytes());
        }
        // cells
        out.extend_from_slice(&(self.cells.len() as u32).to_le_bytes());
        for c in &self.cells {
            out.push(c.kind);
            out.extend_from_slice(&c.row.to_le_bytes());
            out.extend_from_slice(&c.col.to_le_bytes());
            w_span(&mut out, c.rec_span);
            out.extend_from_slice(&c.isst.to_le_bytes());
            out.extend_from_slice(&c.rk_raw.to_le_bytes());
            out.extend_from_slice(&c.num_bits.to_le_bytes());
            out.push(c.bool_err);
            out.push(c.is_error as u8);
            out.push(c.formula_special);
            out.push(c.has_string as u8);
            w_span(&mut out, c.string_span);
            w_units(&mut out, &c.text);
        }
        // xf spans / format spans
        out.extend_from_slice(&(self.xf_spans.len() as u32).to_le_bytes());
        for s in &self.xf_spans {
            w_span(&mut out, *s);
        }
        out.extend_from_slice(&(self.format_spans.len() as u32).to_le_bytes());
        for s in &self.format_spans {
            w_span(&mut out, *s);
        }
        out
    }

    /// Decode a model produced by [`Self::encode`]. Fail-closed on any corruption.
    pub fn decode(bytes: &[u8]) -> Result<XlsModel> {
        let mut r = BinReader::new(bytes);
        if r.bytes(4)? != b"XLSM" {
            return Err(corrupt("bad model magic"));
        }
        if r.u8()? != MODEL_VERSION {
            return Err(corrupt("unsupported model version"));
        }
        let doc_len = r.u64()?;
        let wb_span = r.span()?;
        check_span(wb_span, doc_len, "Workbook stream span")?;
        let stream_name = r.u8()?;
        if stream_name != NAME_WORKBOOK && stream_name != NAME_BOOK {
            return Err(corrupt("bad stream-name marker"));
        }
        let biff_version = r.u16()?;
        if biff_version != BIFF8_VERSION {
            return Err(corrupt("model BIFF version is not BIFF8"));
        }
        let root_substream = r.u16()?;
        if !is_substream_type(root_substream) {
            return Err(corrupt("model root substream type is unknown"));
        }
        let n_records = r.u32()?;
        if n_records > MAX_MODEL_RECORDS {
            return Err(corrupt("model record count is implausible"));
        }
        let mut records = Vec::new();
        for _ in 0..n_records {
            let id = r.u16()?;
            let span = r.span()?;
            check_span(span, wb_span.1, "record span")?;
            let stream_off = r.u64()?;
            let len = r.u32()?;
            if span.1 - span.0 != 4 + len as u64 {
                return Err(corrupt("record span disagrees with its length"));
            }
            if stream_off + span.1 - span.0 > wb_span.1 {
                return Err(corrupt("record lies outside the workbook stream"));
            }
            records.push(XlsRecord {
                id,
                span,
                stream_off,
                len,
            });
        }
        let sst = if r.u8()? != 0 {
            let cst_total = r.u32()?;
            let cst_unique = r.u32()?;
            if cst_unique > MAX_MODEL_STRINGS {
                return Err(corrupt("model SST count is implausible"));
            }
            let n_spans = r.u32()?;
            if n_spans > MAX_MODEL_RECORDS {
                return Err(corrupt("model SST record-span count is implausible"));
            }
            let mut record_spans = Vec::new();
            for _ in 0..n_spans {
                let s = r.span()?;
                check_span(s, doc_len, "SST record span")?;
                record_spans.push(s);
            }
            let n_strings = r.u32()?;
            if n_strings > MAX_MODEL_STRINGS {
                return Err(corrupt("model SST string count is implausible"));
            }
            let mut strings = Vec::new();
            let mut units_total: u64 = 0;
            for _ in 0..n_strings {
                let flags = r.u8()?;
                let compressed = r.u8()? != 0;
                let rich = r.u8()? != 0;
                let ext = r.u8()? != 0;
                let rec_span = r.span()?;
                check_span(rec_span, doc_len, "SST string record span")?;
                let n_parts = r.u32()?;
                let text = r.units()?;
                units_total = units_total.saturating_add(text.len() as u64);
                if units_total > MAX_MODEL_TEXT_UNITS as u64 {
                    return Err(corrupt("model text is implausible"));
                }
                strings.push(XlsSstString {
                    flags,
                    compressed,
                    rich,
                    ext,
                    text,
                    rec_span,
                    n_parts,
                });
            }
            Some(XlsSst {
                cst_total,
                cst_unique,
                strings,
                record_spans,
            })
        } else {
            None
        };
        let n_sheets = r.u32()?;
        if n_sheets > MAX_MODEL_SHEETS {
            return Err(corrupt("model sheet count is implausible"));
        }
        let mut sheets = Vec::new();
        let mut units_total: u64 = 0;
        for _ in 0..n_sheets {
            let name_flags = r.u8()?;
            let name_compressed = r.u8()? != 0;
            let name = r.units()?;
            units_total = units_total.saturating_add(name.len() as u64);
            if units_total > MAX_MODEL_TEXT_UNITS as u64 {
                return Err(corrupt("model text is implausible"));
            }
            let pos = r.u32()?;
            let dt = r.u8()?;
            let state = r.u8()?;
            let rec_span = r.span()?;
            check_span(rec_span, doc_len, "sheet record span")?;
            let bof_span = r.span()?;
            check_span(bof_span, doc_len, "sheet BOF span")?;
            let dim_span = r.span()?;
            check_span(dim_span, doc_len, "sheet DIMENSIONS span")?;
            let n_rows = r.u32()?;
            if n_rows > MAX_MODEL_RECORDS {
                return Err(corrupt("model sheet ROW count is implausible"));
            }
            let mut row_spans = Vec::new();
            for _ in 0..n_rows {
                let s = r.span()?;
                check_span(s, doc_len, "sheet ROW span")?;
                row_spans.push(s);
            }
            let cell_start = r.u32()?;
            let cell_end = r.u32()?;
            sheets.push(XlsSheet {
                name,
                name_flags,
                name_compressed,
                pos,
                dt,
                state,
                rec_span,
                bof_span,
                dim_span,
                row_spans,
                cell_start,
                cell_end,
            });
        }
        let n_cells = r.u32()?;
        if n_cells > MAX_MODEL_CELLS {
            return Err(corrupt("model cell count is implausible"));
        }
        let mut cells = Vec::new();
        for _ in 0..n_cells {
            let kind = r.u8()?;
            let row = r.u32()?;
            let col = r.u32()?;
            let rec_span = r.span()?;
            check_span(rec_span, doc_len, "cell record span")?;
            let isst = r.u32()?;
            let rk_raw = r.u32()?;
            let num_bits = r.u64()?;
            let bool_err = r.u8()?;
            let is_error = r.u8()? != 0;
            let formula_special = r.u8()?;
            let has_string = r.u8()? != 0;
            let string_span = r.span()?;
            check_span(string_span, doc_len, "cell STRING span")?;
            let text = r.units()?;
            units_total = units_total.saturating_add(text.len() as u64);
            if units_total > MAX_MODEL_TEXT_UNITS as u64 {
                return Err(corrupt("model text is implausible"));
            }
            cells.push(XlsCell {
                kind,
                row,
                col,
                rec_span,
                isst,
                rk_raw,
                num_bits,
                bool_err,
                is_error,
                formula_special,
                has_string,
                string_span,
                text,
            });
        }
        for s in &sheets {
            if s.cell_start > s.cell_end || s.cell_end as usize > cells.len() {
                return Err(corrupt("a sheet cell range is out of bounds"));
            }
        }
        let n_xf = r.u32()?;
        if n_xf > MAX_MODEL_RECORDS {
            return Err(corrupt("model XF count is implausible"));
        }
        let mut xf_spans = Vec::new();
        for _ in 0..n_xf {
            let s = r.span()?;
            check_span(s, doc_len, "XF record span")?;
            xf_spans.push(s);
        }
        let n_fmt = r.u32()?;
        if n_fmt > MAX_MODEL_RECORDS {
            return Err(corrupt("model FORMAT count is implausible"));
        }
        let mut format_spans = Vec::new();
        for _ in 0..n_fmt {
            let s = r.span()?;
            check_span(s, doc_len, "FORMAT record span")?;
            format_spans.push(s);
        }
        if !r.at_end() {
            return Err(corrupt("model has trailing bytes"));
        }
        Ok(XlsModel {
            doc_len,
            wb_span,
            stream_name,
            biff_version,
            root_substream,
            records,
            sst,
            sheets,
            cells,
            xf_spans,
            format_spans,
        })
    }
}

fn check_span(span: (u64, u64), bound: u64, what: &'static str) -> Result<()> {
    if span.0 > span.1 || span.1 > bound {
        return Err(corrupt(format!("{what} is outside its container")));
    }
    Ok(())
}

/// A lexical-match record from [`find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XlsMatch {
    /// [`FIELD_SHEET`] or [`FIELD_STRING`].
    pub field: u8,
    /// The sheet index or shared-string index.
    pub index: u32,
    /// The match's first byte offset in the source.
    pub start: u64,
    /// One past the match's last byte in the source.
    pub end: u64,
    /// The matched text.
    pub text: String,
}

/// A bounded, case-sensitive lexical search over each sheet name and each shared
/// string. The span is the record span the match lies in (the per-character bytes are
/// not contiguous when a string is `CONTINUE`-split — recorded honestly).
pub fn find(model: &XlsModel, pattern: &str, max_bytes: u64) -> Result<Vec<XlsMatch>> {
    if pattern.is_empty() {
        return Ok(Vec::new());
    }
    let mut out: Vec<XlsMatch> = Vec::new();
    let mut estimated: u64 = 0;
    let mut push = |field: u8, index: u32, span: (u64, u64), text: String| -> Result<()> {
        estimated = estimated.saturating_add(64 + text.len() as u64);
        if estimated > max_bytes {
            return Err(Error::resource_limit(format!(
                "xls find exceeded the {max_bytes}-byte budget"
            )));
        }
        out.push(XlsMatch {
            field,
            index,
            start: span.0,
            end: span.1,
            text,
        });
        Ok(())
    };
    for (i, s) in model.sheets.iter().enumerate() {
        let name = String::from_utf16_lossy(&s.name);
        if name.contains(pattern) {
            push(FIELD_SHEET, i as u32, s.rec_span, name)?;
        }
    }
    if let Some(sst) = &model.sst {
        for (i, s) in sst.strings.iter().enumerate() {
            let text = String::from_utf16_lossy(&s.text);
            if text.contains(pattern) {
                push(FIELD_STRING, i as u32, s.rec_span, text)?;
            }
        }
    }
    Ok(out)
}

/// The canonical text rendering of the model: each sheet name followed by its
/// `LABELSST` and string-result `FORMULA` cell values. Bounded by `max_bytes`.
pub fn canonical_text(model: &XlsModel, _source: &[u8], max_bytes: u64) -> Result<String> {
    let mut out = String::new();
    for (i, sheet) in model.sheets.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&String::from_utf16_lossy(&sheet.name));
        for c in model.sheet_cells(sheet) {
            let text = match c.kind {
                CELL_LABELSST => model.label_text(c),
                CELL_FORMULA if c.has_string => Some(String::from_utf16_lossy(&c.text)),
                _ => None,
            };
            if let Some(t) = text
                && !t.is_empty()
            {
                out.push('\t');
                out.push_str(&t);
            }
        }
        if out.len() as u64 > max_bytes {
            return Err(Error::resource_limit(format!(
                "xls canonical text exceeded the {max_bytes}-byte budget"
            )));
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Fast-fail legacy-XLS (BIFF8) detector, built **on top of** the CFBF detector: the
/// source must be a compound file the CFBF layer accepts and its `Workbook`/`Book`
/// stream must begin with a BIFF8 `BOF` (id `0x0809`, version `0x0600`) whose
/// substream type is a recognized worksheet/globals type. Never consults a file name.
pub fn detect(source: &[u8], limits: Limits) -> bool {
    if source.len() as u64 > limits.max_xls_document_bytes {
        return false;
    }
    if !crate::adapter::cfbf::detect(source, limits) {
        return false;
    }
    let Ok(model) = crate::adapter::cfbf::parse(source, limits) else {
        return false;
    };
    let Some(entry) = find_workbook(&model) else {
        return false;
    };
    // Read only the `BOF` prefix: a genuine `.xls` whose `Workbook`/`Book` chain
    // is **fragmented** is still a `.xls`, and its leading `BOF` lives in the
    // stream's first chunk. Detection therefore reads the prefix rather than
    // requiring the whole stream to be one contiguous span.
    let Ok(wb) = crate::adapter::cfbf::stream_leading(source, &model, entry, 8) else {
        return false;
    };
    if wb.len() < 8 {
        return false;
    }
    let id = u16::from_le_bytes([wb[0], wb[1]]);
    if id != REC_BOF {
        return false;
    }
    let len = u16::from_le_bytes([wb[2], wb[3]]);
    if len < 4 {
        return false;
    }
    let version = u16::from_le_bytes([wb[4], wb[5]]);
    if version != BIFF8_VERSION {
        return false;
    }
    let subtype = u16::from_le_bytes([wb[6], wb[7]]);
    is_substream_type(subtype)
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse `source` into an [`XlsModel`] under `limits`. Declines typed on any malformed
/// or over-large input; never panics.
pub fn parse(source: &[u8], limits: Limits) -> Result<XlsModel> {
    let len = source.len() as u64;
    if len > limits.max_xls_document_bytes {
        return Err(Error::resource_limit(format!(
            "xls source is {len} bytes, above the {}-byte document cap",
            limits.max_xls_document_bytes
        )));
    }
    let cfbf = crate::adapter::cfbf::parse(source, limits)?;
    let (entry, stream_name) = find_workbook_named(&cfbf)
        .ok_or_else(|| invalid("the compound file has no Workbook or Book stream"))?;
    let wb = crate::adapter::cfbf::stream_bytes(source, entry)?;
    let base = entry.data_span.0;
    let wb_span = (base, base + wb.len() as u64);

    let records = scan_records(wb, base, limits)?;
    let first = records
        .first()
        .ok_or_else(|| invalid("the workbook stream is empty"))?;
    if first.id != REC_BOF {
        return Err(invalid("the workbook stream does not begin with a BOF"));
    }
    let bof_pl = payload(wb, first);
    let biff_version = rd_u16(bof_pl, 0).ok_or_else(|| invalid("the BOF is truncated"))?;
    if biff_version != BIFF8_VERSION {
        return Err(invalid(format!(
            "unsupported BIFF version 0x{biff_version:04X}"
        )));
    }
    let root_substream = rd_u16(bof_pl, 2).ok_or_else(|| invalid("the BOF is truncated"))?;
    if !is_substream_type(root_substream) {
        return Err(invalid(format!(
            "unknown BOF substream type 0x{root_substream:04X}"
        )));
    }
    if root_substream != ST_WORKBOOK_GLOBALS {
        return Err(invalid(
            "the workbook stream does not begin with a workbook-globals substream",
        ));
    }

    // The globals substream runs from the leading BOF up to the first EOF.
    let g_eof = (1..records.len())
        .find(|&i| records[i].id == REC_EOF)
        .ok_or_else(|| invalid("the globals substream has no EOF"))?;

    let mut sheets: Vec<XlsSheet> = Vec::new();
    let mut sst: Option<XlsSst> = None;
    let mut xf_spans: Vec<(u64, u64)> = Vec::new();
    let mut format_spans: Vec<(u64, u64)> = Vec::new();

    // Collect BOUNDSHEET/DIMENSIONS/ROW/FORMAT/XF/SST from the globals substream.
    let mut i = 1usize;
    while i < g_eof {
        let r = &records[i];
        match r.id {
            REC_BOUNDSHEET => {
                if sheets.len() as u64 >= limits.max_xls_sheets {
                    return Err(Error::resource_limit(format!(
                        "the workbook has more than {} sheets",
                        limits.max_xls_sheets
                    )));
                }
                let s = parse_boundsheet(wb, r)?;
                sheets.push(s);
            }
            REC_SST => {
                if sst.is_none() {
                    let mut parts: Vec<&[u8]> = Vec::new();
                    let mut spans: Vec<(u64, u64)> = Vec::new();
                    parts.push(payload(wb, r));
                    spans.push(r.span);
                    let mut j = i + 1;
                    while j < records.len() && records[j].id == REC_CONTINUE {
                        parts.push(payload(wb, &records[j]));
                        spans.push(records[j].span);
                        j += 1;
                    }
                    sst = Some(parse_sst(&parts, &spans, limits)?);
                    i = j;
                    continue;
                }
            }
            REC_XF => xf_spans.push(r.span),
            REC_FORMAT => format_spans.push(r.span),
            _ => {}
        }
        i += 1;
    }

    // Resolve each BOUNDSHEET to its BOF substream and collect the sheet's cells.
    let mut cells: Vec<XlsCell> = Vec::new();
    for sheet in sheets.iter_mut() {
        let pos = sheet.pos as u64;
        let bof_idx = records
            .iter()
            .position(|rr| rr.id == REC_BOF && rr.stream_off == pos)
            .ok_or_else(|| invalid(format!("BOUNDSHEET position {pos} does not address a BOF")))?;
        if bof_idx == 0 {
            return Err(invalid("a BOUNDSHEET addresses the workbook-globals BOF"));
        }
        let bof = &records[bof_idx];
        let st = rd_u16(payload(wb, bof), 2).ok_or_else(|| invalid("a sheet BOF is truncated"))?;
        if !matches!(st, ST_WORKSHEET | ST_CHART | ST_MACRO) {
            return Err(invalid("a BOUNDSHEET does not address a sheet substream"));
        }
        sheet.bof_span = bof.span;
        let eof_idx = (bof_idx + 1..records.len())
            .find(|&j| records[j].id == REC_EOF)
            .ok_or_else(|| invalid("a sheet substream has no EOF"))?;
        let cell_start = cells.len() as u32;
        for k in (bof_idx + 1)..eof_idx {
            let r = &records[k];
            match r.id {
                REC_DIMENSIONS => sheet.dim_span = r.span,
                REC_ROW => sheet.row_spans.push(r.span),
                REC_LABELSST | REC_NUMBER | REC_RK | REC_BOOLERR | REC_FORMULA | REC_BLANK
                | REC_MULBLANK => {
                    scan_cell(wb, &records, k, &mut cells, limits)?;
                }
                _ => {}
            }
        }
        sheet.cell_start = cell_start;
        sheet.cell_end = cells.len() as u32;
    }

    // Bound the aggregate derived text (SST strings + sheet names + formula strings).
    let text_units = aggregate_text_units(&sst, &sheets, &cells);
    if text_units > limits.max_xls_text_bytes {
        return Err(Error::resource_limit(format!(
            "xls derived text is above the {}-byte text cap",
            limits.max_xls_text_bytes
        )));
    }

    Ok(XlsModel {
        doc_len: len,
        wb_span,
        stream_name,
        biff_version,
        root_substream,
        records,
        sst,
        sheets,
        cells,
        xf_spans,
        format_spans,
    })
}

/// Parse `source` and return its canonical derived model, encoded (the materialization
/// of an `XlsModel` node).
pub fn build_xls_model(source: &[u8], limits: Limits) -> Result<Vec<u8>> {
    Ok(parse(source, limits)?.encode())
}

/// Scan the `Workbook`/`Book` stream into its record list with exact spans.
fn scan_records(wb: &[u8], base: u64, limits: Limits) -> Result<Vec<XlsRecord>> {
    let mut out: Vec<XlsRecord> = Vec::new();
    let mut off = 0usize;
    while off < wb.len() {
        if wb.len() - off < 4 {
            return Err(invalid("a BIFF record header is truncated"));
        }
        let id = u16::from_le_bytes([wb[off], wb[off + 1]]);
        let payload_len = u16::from_le_bytes([wb[off + 2], wb[off + 3]]) as usize;
        let rec_end = off
            .checked_add(4)
            .and_then(|o| o.checked_add(payload_len))
            .ok_or_else(|| invalid("a BIFF record length overflows"))?;
        if rec_end > wb.len() {
            return Err(invalid("a BIFF record extends past the stream"));
        }
        if out.len() as u64 >= limits.max_xls_records {
            return Err(Error::resource_limit(format!(
                "the workbook has more than {} records",
                limits.max_xls_records
            )));
        }
        out.push(XlsRecord {
            id,
            span: (base + off as u64, base + rec_end as u64),
            stream_off: off as u64,
            len: payload_len as u32,
        });
        off = rec_end;
    }
    Ok(out)
}

/// Parse a `BOUNDSHEET` record's name and fields.
fn parse_boundsheet(wb: &[u8], r: &XlsRecord) -> Result<XlsSheet> {
    let pl = payload(wb, r);
    if pl.len() < 8 {
        return Err(invalid("a BOUNDSHEET is truncated"));
    }
    let pos = rd_u32(pl, 0).ok_or_else(|| invalid("a BOUNDSHEET is truncated"))?;
    let state = pl[4];
    let dt = pl[5];
    let cch = pl[6] as usize;
    let name_flags = pl[7];
    let name_compressed = name_flags & 0x01 == 0;
    let unit = if name_compressed { 1 } else { 2 };
    let need = 8 + cch * unit;
    if pl.len() < need {
        return Err(invalid("a BOUNDSHEET name is truncated"));
    }
    let mut name = Vec::with_capacity(cch);
    for k in 0..cch {
        if name_compressed {
            name.push(pl[8 + k] as u16);
        } else {
            let o = 8 + k * 2;
            name.push(u16::from_le_bytes([pl[o], pl[o + 1]]));
        }
    }
    Ok(XlsSheet {
        name,
        name_flags,
        name_compressed,
        pos,
        dt,
        state,
        rec_span: r.span,
        bof_span: (0, 0),
        dim_span: (0, 0),
        row_spans: Vec::new(),
        cell_start: 0,
        cell_end: 0,
    })
}

/// Parse one cell record (or a `MULBLANK` run) into the cell arena.
fn scan_cell(
    wb: &[u8],
    records: &[XlsRecord],
    index: usize,
    cells: &mut Vec<XlsCell>,
    limits: Limits,
) -> Result<()> {
    let r = &records[index];
    let pl = payload(wb, r);
    let mut push = |cell: XlsCell| -> Result<()> {
        if cells.len() as u64 >= limits.max_xls_cells {
            return Err(Error::resource_limit(format!(
                "the workbook has more than {} cells",
                limits.max_xls_cells
            )));
        }
        cells.push(cell);
        Ok(())
    };
    let blank = XlsCell {
        kind: CELL_BLANK,
        row: 0,
        col: 0,
        rec_span: r.span,
        isst: 0,
        rk_raw: 0,
        num_bits: 0,
        bool_err: 0,
        is_error: false,
        formula_special: FORMULA_NUMERIC,
        has_string: false,
        string_span: (0, 0),
        text: Vec::new(),
    };
    match r.id {
        REC_LABELSST => {
            if pl.len() < 10 {
                return Err(invalid("a LABELSST is truncated"));
            }
            push(XlsCell {
                kind: CELL_LABELSST,
                row: rd_u16(pl, 0).unwrap() as u32,
                col: rd_u16(pl, 2).unwrap() as u32,
                isst: rd_u32(pl, 6).unwrap(),
                ..blank
            })?;
        }
        REC_NUMBER => {
            if pl.len() < 14 {
                return Err(invalid("a NUMBER is truncated"));
            }
            push(XlsCell {
                kind: CELL_NUMBER,
                row: rd_u16(pl, 0).unwrap() as u32,
                col: rd_u16(pl, 2).unwrap() as u32,
                num_bits: rd_u64(pl, 6).unwrap(),
                ..blank
            })?;
        }
        REC_RK => {
            if pl.len() < 10 {
                return Err(invalid("an RK is truncated"));
            }
            push(XlsCell {
                kind: CELL_RK,
                row: rd_u16(pl, 0).unwrap() as u32,
                col: rd_u16(pl, 2).unwrap() as u32,
                rk_raw: rd_u32(pl, 6).unwrap(),
                ..blank
            })?;
        }
        REC_BOOLERR => {
            if pl.len() < 8 {
                return Err(invalid("a BOOLERR is truncated"));
            }
            push(XlsCell {
                kind: CELL_BOOLERR,
                row: rd_u16(pl, 0).unwrap() as u32,
                col: rd_u16(pl, 2).unwrap() as u32,
                bool_err: pl[6],
                is_error: pl[7] != 0,
                ..blank
            })?;
        }
        REC_BLANK => {
            if pl.len() < 6 {
                return Err(invalid("a BLANK is truncated"));
            }
            push(XlsCell {
                kind: CELL_BLANK,
                row: rd_u16(pl, 0).unwrap() as u32,
                col: rd_u16(pl, 2).unwrap() as u32,
                ..blank
            })?;
        }
        REC_MULBLANK => {
            if pl.len() < 8 {
                return Err(invalid("a MULBLANK is truncated"));
            }
            let row = rd_u16(pl, 0).unwrap() as u32;
            let mut off = 6usize;
            loop {
                let v =
                    rd_u16(pl, off).ok_or_else(|| invalid("a MULBLANK run has no terminator"))?;
                let col = (v & 0x00FF) as u32;
                push(XlsCell {
                    kind: CELL_MULBLANK,
                    row,
                    col,
                    rec_span: r.span,
                    isst: 0,
                    rk_raw: 0,
                    num_bits: 0,
                    bool_err: 0,
                    is_error: false,
                    formula_special: FORMULA_NUMERIC,
                    has_string: false,
                    string_span: (0, 0),
                    text: Vec::new(),
                })?;
                off += 2;
                if v & 0x0100 != 0 {
                    break;
                }
            }
        }
        REC_FORMULA => {
            if pl.len() < 20 {
                return Err(invalid("a FORMULA is truncated"));
            }
            let row = rd_u16(pl, 0).unwrap() as u32;
            let col = rd_u16(pl, 2).unwrap() as u32;
            let num = rd_u64(pl, 6).unwrap();
            let num_bytes = num.to_le_bytes();
            let special = if num_bytes[6] == 0xFF && num_bytes[7] == 0xFF {
                num_bytes[0]
            } else {
                FORMULA_NUMERIC
            };
            if special == 0 {
                // A string result: a STRING record must immediately follow.
                let next = records
                    .get(index + 1)
                    .ok_or_else(|| invalid("a string FORMULA has no STRING record"))?;
                if next.id != REC_STRING {
                    return Err(invalid("a string FORMULA has no STRING record"));
                }
                let text = decode_unicode_string(payload(wb, next))?;
                push(XlsCell {
                    kind: CELL_FORMULA,
                    row,
                    col,
                    num_bits: num,
                    formula_special: special,
                    has_string: true,
                    string_span: next.span,
                    text,
                    ..blank
                })?;
            } else {
                push(XlsCell {
                    kind: CELL_FORMULA,
                    row,
                    col,
                    num_bits: num,
                    formula_special: special,
                    ..blank
                })?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Decode an `XLUnicodeString` (a 2-byte `cch`, an option-flags byte, characters).
fn decode_unicode_string(pl: &[u8]) -> Result<Vec<u16>> {
    if pl.len() < 3 {
        return Err(invalid("a STRING is truncated"));
    }
    let cch = rd_u16(pl, 0).unwrap() as usize;
    let flags = pl[2];
    let compressed = flags & 0x01 == 0;
    let unit = if compressed { 1 } else { 2 };
    let need = 3 + cch * unit;
    if pl.len() < need {
        return Err(invalid("a STRING is truncated"));
    }
    let mut text = Vec::with_capacity(cch);
    for k in 0..cch {
        if compressed {
            text.push(pl[3 + k] as u16);
        } else {
            let o = 3 + k * 2;
            text.push(u16::from_le_bytes([pl[o], pl[o + 1]]));
        }
    }
    Ok(text)
}

/// Parse the `SST` record series (the `SST` payload plus each `CONTINUE` payload).
fn parse_sst(parts: &[&[u8]], spans: &[(u64, u64)], limits: Limits) -> Result<XlsSst> {
    let mut r = SstReader::new(parts);
    let cst_total = r
        .u32()
        .ok_or_else(|| invalid("the SST header is truncated"))?;
    let cst_unique = r
        .u32()
        .ok_or_else(|| invalid("the SST header is truncated"))?;
    if cst_unique as u64 > limits.max_xls_sst_strings {
        return Err(Error::resource_limit(format!(
            "the SST has {cst_unique} strings, above max_xls_sst_strings {}",
            limits.max_xls_sst_strings
        )));
    }
    let mut strings: Vec<XlsSstString> = Vec::new();
    for _ in 0..cst_unique {
        let cch = r
            .u16()
            .ok_or_else(|| invalid("an SST string header is truncated"))?;
        let flags = r
            .byte()
            .ok_or_else(|| invalid("an SST string header is truncated"))?;
        let compressed = flags & 0x01 == 0;
        let rich = flags & 0x08 != 0;
        let ext = flags & 0x04 != 0;
        let c_run = if rich {
            r.u16()
                .ok_or_else(|| invalid("an SST rich-run count is truncated"))?
        } else {
            0
        };
        let cb_ext = if ext {
            r.u32()
                .ok_or_else(|| invalid("an SST extended-phonetic size is truncated"))?
        } else {
            0
        };
        let start_part = r.part;
        let mut text: Vec<u16> = Vec::new();
        r.char_array(cch, compressed, &mut text)
            .ok_or_else(|| invalid("an SST string is truncated or its CONTINUE is malformed"))?;
        if rich {
            r.skip(c_run as usize * 4)
                .ok_or_else(|| invalid("an SST rich-run array is truncated"))?;
        }
        if ext {
            r.skip(cb_ext as usize)
                .ok_or_else(|| invalid("an SST extended-phonetic data is truncated"))?;
        }
        let rec_span = *spans
            .get(start_part)
            .ok_or_else(|| invalid("SST part has no record span"))?;
        let n_parts = (r.part - start_part + 1) as u32;
        strings.push(XlsSstString {
            flags,
            compressed,
            rich,
            ext,
            text,
            rec_span,
            n_parts,
        });
    }
    if !r.at_end() {
        return Err(invalid(
            "the SST has trailing bytes (its string count disagrees with its data)",
        ));
    }
    Ok(XlsSst {
        cst_total,
        cst_unique,
        strings,
        record_spans: spans.to_vec(),
    })
}

/// The aggregate derived text unit count (SST strings + sheet names + formula
/// strings). This is the byte-budget the caller checks against `max_xls_text_bytes`.
fn aggregate_text_units(sst: &Option<XlsSst>, sheets: &[XlsSheet], cells: &[XlsCell]) -> u64 {
    let mut n = 0u64;
    if let Some(sst) = sst {
        n = n.saturating_add(sst.strings.iter().map(|s| s.text.len() as u64).sum());
    }
    n = n.saturating_add(sheets.iter().map(|s| s.name.len() as u64).sum());
    n = n.saturating_add(cells.iter().map(|c| c.text.len() as u64).sum());
    n
}

/// A record-boundary-aware reader over the `SST` record series.
struct SstReader<'a> {
    parts: &'a [&'a [u8]],
    part: usize,
    off: usize,
}

impl<'a> SstReader<'a> {
    fn new(parts: &'a [&'a [u8]]) -> Self {
        SstReader {
            parts,
            part: 0,
            off: 0,
        }
    }

    fn at_end(&self) -> bool {
        if self.part >= self.parts.len() {
            return true;
        }
        if self.off < self.parts[self.part].len() {
            return false;
        }
        self.parts[self.part + 1..].iter().all(|p| p.is_empty())
    }

    fn byte(&mut self) -> Option<u8> {
        while self.part < self.parts.len() && self.off >= self.parts[self.part].len() {
            self.part += 1;
            self.off = 0;
        }
        if self.part >= self.parts.len() {
            return None;
        }
        let b = *self.parts[self.part].get(self.off)?;
        self.off += 1;
        Some(b)
    }

    fn u16(&mut self) -> Option<u16> {
        let a = self.byte()?;
        let b = self.byte()?;
        Some(u16::from_le_bytes([a, b]))
    }

    fn u32(&mut self) -> Option<u32> {
        let a = self.byte()?;
        let b = self.byte()?;
        let c = self.byte()?;
        let d = self.byte()?;
        Some(u32::from_le_bytes([a, b, c, d]))
    }

    fn skip(&mut self, mut n: usize) -> Option<()> {
        while n > 0 {
            while self.part < self.parts.len() && self.off >= self.parts[self.part].len() {
                self.part += 1;
                self.off = 0;
            }
            if self.part >= self.parts.len() {
                return None;
            }
            let avail = self.parts[self.part].len() - self.off;
            let take = avail.min(n);
            self.off += take;
            n -= take;
        }
        Some(())
    }

    /// Read `cch` characters, crossing record boundaries. When the character array
    /// continues into a new record, the continuation's leading option byte selects the
    /// width of the remaining characters.
    fn char_array(&mut self, cch: u16, mut compressed: bool, out: &mut Vec<u16>) -> Option<()> {
        let mut left = cch as usize;
        while left > 0 {
            if self.part >= self.parts.len() {
                return None;
            }
            if self.off >= self.parts[self.part].len() {
                // Cross into the continuation and consume its option byte.
                if self.part + 1 >= self.parts.len() {
                    return None;
                }
                self.part += 1;
                let flag = *self.parts[self.part].first()?;
                compressed = flag & 0x01 == 0;
                self.off = 1;
                if self.off >= self.parts[self.part].len() {
                    return None;
                }
            }
            let avail = self.parts[self.part].len() - self.off;
            let unit = if compressed { 1 } else { 2 };
            if avail < unit {
                // A character split across the boundary is not representable.
                return None;
            }
            let can = (avail / unit).min(left);
            let p = self.parts[self.part];
            for _ in 0..can {
                if compressed {
                    out.push(p[self.off] as u16);
                    self.off += 1;
                } else {
                    out.push(u16::from_le_bytes([p[self.off], p[self.off + 1]]));
                    self.off += 2;
                }
            }
            left -= can;
        }
        Some(())
    }
}

/// Find the `Workbook` (preferred) or legacy `Book` stream, with its name marker.
/// The well-known name is matched **ASCII-case-insensitively**: some real writers
/// uppercase it (a `WORKBOOK` stream that opens with a BIFF8 `BOF` is a workbook).
fn find_workbook_named(
    model: &crate::adapter::cfbf::CfbfModel,
) -> Option<(&crate::adapter::cfbf::CfbfEntry, u8)> {
    if let Some(e) =
        find_named(model, STREAM_WORKBOOK).or_else(|| find_named_ci(model, STREAM_WORKBOOK))
    {
        return Some((e, NAME_WORKBOOK));
    }
    find_named(model, STREAM_BOOK)
        .or_else(|| find_named_ci(model, STREAM_BOOK))
        .map(|e| (e, NAME_BOOK))
}

/// Find the `Workbook`/`Book` stream entry (used by detection).
fn find_workbook(
    model: &crate::adapter::cfbf::CfbfModel,
) -> Option<&crate::adapter::cfbf::CfbfEntry> {
    find_workbook_named(model).map(|(e, _)| e)
}

/// Find a directory stream entry by its verbatim name.
fn find_named<'a>(
    model: &'a crate::adapter::cfbf::CfbfModel,
    name: &str,
) -> Option<&'a crate::adapter::cfbf::CfbfEntry> {
    model.entries.iter().find(|e| {
        e.obj_type == crate::adapter::cfbf::OBJ_STREAM
            && crate::adapter::cfbf::entry_name_text(e) == name
    })
}

/// Find a directory stream entry by an ASCII-case-insensitive well-known name.
fn find_named_ci<'a>(
    model: &'a crate::adapter::cfbf::CfbfModel,
    name: &str,
) -> Option<&'a crate::adapter::cfbf::CfbfEntry> {
    model.entries.iter().find(|e| {
        e.obj_type == crate::adapter::cfbf::OBJ_STREAM
            && crate::adapter::cfbf::entry_name_text(e).eq_ignore_ascii_case(name)
    })
}

/// The payload bytes of a record.
fn payload<'a>(wb: &'a [u8], r: &XlsRecord) -> &'a [u8] {
    let s = r.stream_off as usize + 4;
    let e = s + r.len as usize;
    wb.get(s..e).unwrap_or(&[])
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn rd_u16(b: &[u8], off: usize) -> Option<u16> {
    let s = b.get(off..off + 2)?;
    Some(u16::from_le_bytes([s[0], s[1]]))
}

fn rd_u32(b: &[u8], off: usize) -> Option<u32> {
    let s = b.get(off..off + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn rd_u64(b: &[u8], off: usize) -> Option<u64> {
    let s = b.get(off..off + 8)?;
    Some(u64::from_le_bytes([
        s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
    ]))
}

fn invalid(message: impl Into<String>) -> Error {
    Error::invalid_xls_structure(format!("xls: {}", message.into()))
}

fn corrupt(message: impl Into<String>) -> Error {
    Error::invalid_xls_structure(format!("xls model: {}", message.into()))
}

fn w_span(out: &mut Vec<u8>, s: (u64, u64)) {
    out.extend_from_slice(&s.0.to_le_bytes());
    out.extend_from_slice(&s.1.to_le_bytes());
}

fn w_units(out: &mut Vec<u8>, units: &[u16]) {
    out.extend_from_slice(&(units.len() as u32).to_le_bytes());
    for u in units {
        out.extend_from_slice(&u.to_le_bytes());
    }
}

/// A minimal fail-closed little-endian reader for model decoding.
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
        let out = self
            .b
            .get(self.at..end)
            .ok_or_else(|| corrupt("unexpected end of model"))?;
        self.at = end;
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        let b = self.bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
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

    fn span(&mut self) -> Result<(u64, u64)> {
        Ok((self.u64()?, self.u64()?))
    }

    fn units(&mut self) -> Result<Vec<u16>> {
        let n = self.u32()?;
        if n as u64 > MAX_MODEL_TEXT_UNITS as u64 {
            return Err(corrupt("text length is implausible"));
        }
        let bytes = self.bytes(n as usize * 2)?;
        Ok(bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect())
    }

    fn at_end(&self) -> bool {
        self.at == self.b.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_and_name_tables_are_stable() {
        assert_eq!(version_name(BIFF8_VERSION), "biff8");
        assert_eq!(version_name(0x0500), "biff5");
        assert_eq!(stream_name(NAME_WORKBOOK), "Workbook");
        assert_eq!(stream_name(NAME_BOOK), "Book");
        assert_eq!(substream_name(ST_WORKBOOK_GLOBALS), "globals");
        assert!(is_substream_type(ST_WORKSHEET));
        assert!(!is_substream_type(0x0001));
        assert_eq!(cell_kind_name(CELL_LABELSST), "labelsst");
        assert_eq!(sheet_type_name(0x00), "worksheet");
        assert_eq!(state_name(1), "hidden");
    }

    #[test]
    fn rk_decodes_int_x100_and_float() {
        // A 30-bit signed integer: 7 -> rk = (7 << 2) | 0x02.
        assert_eq!(rk_value((7u32 << 2) | 0x02), 7.0);
        // Divided by 100 when the low bit is set.
        assert_eq!(rk_value((7u32 << 2) | 0x02 | 0x01), 0.07);
        // The top 30 bits of an IEEE-754 double (1.0 == 0x3FF0_0000_0000_0000).
        assert_eq!(rk_value(0x3FF0_0000), 1.0);
        // A negative integer.
        assert_eq!(rk_value(((-3i32 as u32) << 2) | 0x02), -3.0);
    }

    #[test]
    fn prose_is_not_xls() {
        assert!(!detect(
            b"The quick brown fox jumps over the lazy dog.\nPlain prose.\n",
            Limits::DEFAULT
        ));
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut seed: u64 = 0x2620_0000_5156_4c53;
        for _ in 0..1024 {
            let mut buf = [0u8; 1024];
            for b in buf.iter_mut() {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                *b = (seed >> 33) as u8;
            }
            let _ = detect(&buf, Limits::DEFAULT);
            let _ = parse(&buf, Limits::STRICT);
        }
        let mut buf = vec![0u8; 4096];
        buf[..8].copy_from_slice(&crate::adapter::cfbf::SIGNATURE);
        for _ in 0..256 {
            for b in buf[8..].iter_mut() {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                *b = (seed >> 33) as u8;
            }
            let _ = detect(&buf, Limits::STRICT);
            let _ = parse(&buf, Limits::STRICT);
        }
    }
}
