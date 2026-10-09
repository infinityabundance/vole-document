# XLSX

XLSX (Office Open XML SpreadsheetML) enters through the shared byte-authoritative
**OPC** layer (ADR-0030/0031) with a **SpreadsheetML** native model. The adapter is
gated behind the **non-default** `xlsx = ["opc"]` feature (ADR-0059): a build
without it cannot detect XLSX and reports it `Opaque`, so the reported capability
set always matches what the build can serve.

## Authority boundary

The exact physical source is the ZIP member cover. The workbook main part
(`xl/workbook.xml`) and the sheet parts are located **semantically** from
`[Content_Types].xml` and the OPC relationship graph, never from a hardcoded path
alone. Decode-time SpreadsheetML *conformance* is a differential judgement
reported as typed issues and never gates exactness: an invalid or hostile workbook
is still an exact archival object. Detection is byte-based and mutually exclusive
with DOCX — `docx` is signalled by a **positive** WordprocessingML main-part
content type (or an `officeDocument` relationship targeting `word/document.xml`),
so a Word document that *embeds* an Excel workbook is still `docx` and is not
misclassified (`Opaque`).

## Physical representation

`[Content_Types].xml`, `_rels/.rels`, `xl/workbook.xml`, `xl/worksheets/sheetN.xml`,
`xl/sharedStrings.xml`, `xl/styles.xml`, `xl/comments*.xml`, `xl/tables/*.xml`,
`xl/drawings/*.xml`, `xl/charts/*.xml`, `xl/media/*`, and the `_rels/*` parts.
Members are identified by `(archive ordinal, local-header offset)`; the exact leaf
is the raw compressed span (no unzip/rezip). A malformed workbook/sheet or a
missing part referenced by an element is a typed decline
(`InvalidXmlStructure` / `InvalidPackageStructure`); a missing *optional* part is
an explicit absence, not a decline.

## Native inverse representation

The workbook/sheet model: sheet order, names, and visibility (`state`
visible/hidden/very-hidden); rows and cells with the **five distinct fields** —
the **stored formula** (`<f>`), the **cached result** (`<v>`), the deterministic
number-format **displayed value** (a bounded grammar over a small set of format
codes, labelled `deterministically-derived`), the resolved **cell style**
(number format, font, fill, alignment via `cellXfs`), and the cell's exact XOR
**XML span**; shared vs inline strings; booleans/errors; merged ranges (their
`ref` strings); cell comments (`xl/comments*.xml` keyed by cell, plus VML note
anchors); internal (`location`) and external (`r:id`) hyperlinks resolved through
the sheet `_rels`; defined/named ranges (including hidden and local names);
tables (`name`/`ref`/`columns` via `tableParts`); drawings/charts/media as a
relationship graph (the drawing part's exact/decoded bytes resolve; chart XML is
exposed but never evaluated); and package external relationships as **typed
metadata that is never dereferenced**.

## Supported observations

Common selectors: `metadata`, `text`, `table`, `cell` (and `find`). Native:
`xlsx-sheet` (`--sheet N`), `xlsx-cell` (`--xlsx-cell A1 [--sheet N]`),
`xlsx-styles`, `xlsx-comments`, `xlsx-hyperlinks`, `xlsx-tables`, `xlsx-drawing`,
`xlsx-defined-names`, `xlsx-external-rels`, plus raw or decoded members.

## Unsupported observations

`Page(n)` is a typed decline — a spreadsheet has no intrinsic pagination and pages
are never synthesized. **Formulas are never evaluated**: formula *dependents* and
a chart's *data references* are not derived (typed declines), and a chart's
rendered output is never produced. Charts are exposed as parts, not as data.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any `.xlsx`,
including workbooks the adapter declines to interpret natively, and after the
source **and** descriptor are deleted in a fresh process (the 21.1 courts and the
21.1.3 economic court, exactness **8/8**). Exactness is inherited from the
Phase-12.2 ZIP member raw spans; every SpreadsheetML model is derived (`Q_gen`) and
is **never on the exactness path**.

## Security limits

XML is derived-only (bounded depth/events/nodes/attributes/text). Cell references
are parsed with **checked arithmetic** (an over-long A1 reference declines typed,
never wraps) and coordinates are bounded to the Excel-conformant grid
(`max_xlsx_col` = 16384, `max_xlsx_row` = 1<<20); the sheet-text projection is
bounded before it is built, so one hostile coordinate cannot drive an unbounded
allocation. No `PathBuf` is ever built from a member name; external targets are
inert strings and never fetched; no formula is ever executed. See
[Security](../SECURITY.md).

## Known limitations

This is a **bounded** SpreadsheetML subset, not a spreadsheet engine. It does not
evaluate formulas, compute dependents, resolve chart data references, or render.
The displayed value is a deterministic projection over a bounded set of number
formats, never the recalculated value. Rich-text/comment formatting beyond plain
text and shared-formula arrays are not modelled. The economic court is measured on
a **self-authored deterministic corpus**, not a real-world population; only the
materialized workbook is a byte-authority claim, every other observation is a
derived projection. All measurement corpora are locally generated.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0030](../adr/0030-zip-physical-layer.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0059](../adr/0059-xlsx-adapter-and-analytical-comparator.md).

## Evidence

- Phase 21.1.1 XLSX court: `tests/xlsx_adapter.rs`; campaign
  `evidence/campaigns/2026-10-08-phase21-1-xlsx-4d26514/`.
- Phase 21.1.2 semantic court: `tools/phase21-2-xlsx-semantic-court.sh`; campaign
  `evidence/campaigns/2026-10-09-phase21-2-xlsx-5802be9/`.
- Phase 21.1.3 economic court: `tools/phase21-3-xlsx-court.sh` (SQLite + DuckDB);
  campaign `evidence/campaigns/2026-10-09-phase21-3-xlsx-b2400f1/`.
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
