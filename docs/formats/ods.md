# ODS

ODS (OpenDocument Spreadsheet) enters through the bounded OpenDocument inverse
compiler over the shared byte-authoritative ZIP layer (ADR-0030/0038), exactly
like ODT. ODF is **not** OPC, so the adapter reuses the ZIP physical layer and the
bounded-XML policy but does not route through the OPC graph. The adapter is gated
behind the **non-default** `ods = ["opc"]` feature.

## Authority boundary

The exact physical source is the ZIP member cover. The main content part is
located **semantically** from `META-INF/manifest.xml` (the `content.xml`
file-entry, ODF 1.2 §2.2.1), never from a hardcoded path alone. Detection is
byte-based: the mandatory stored `mimetype` equals
`application/vnd.oasis.opendocument.spreadsheet`, or the manifest declares that
media type — mutually exclusive with ODT (which declares the *text* media type).
Decode-time OpenDocument *conformance* is a differential judgement reported as
typed issues and never gates exactness.

## Physical representation

`mimetype` (first entry, stored), `META-INF/manifest.xml`, `content.xml`,
`styles.xml`, `meta.xml`. Members are identified by `(archive ordinal,
local-header offset)`; the exact leaf is the raw compressed span (no
unzip/rezip). A missing or malformed manifest is a typed decline
(`InvalidPackageStructure` / `InvalidXmlStructure`).

## Native inverse representation

The `office:body`/`office:spreadsheet` content model: sheets (`table:table` with
`table:name`, order, `table:display`); rows (`table:table-row`) and cells
(`table:table-cell`) with `office:value-type` (float/string/boolean/date/
currency/percentage) and the matching `office:value` / `office:boolean-value` /
`office:date-value` / `office:string-value`; the displayed `<text:p>` content; the
**stored formula** (`table:formula`); cell style names; merged cells
(`table:number-columns-spanned`/`-rows-spanned`, `covered` cells); repeated
cells/rows (`table:number-columns-repeated`/`-rows-repeated`); named expressions
(`table:named-expressions`); cell styles (`style:style` family `table-cell` +
`style:table-cell-properties`/`style:text-properties` + `number:` formats); and
cell comments (`office:annotation`).

The **crucial distinctions** are never conflated: a cell's *stored formula*, its
*typed value* (`office:value` + type), its *displayed text* (`text:p`), its
*style*, and its exact *XML span* are separate fields. **Formulas are never
evaluated.**

## Supported observations

Common selectors: `metadata`, `text`, `table`, `cell`, `find`. Native:
`--ods-sheet N` (metadata/text/structure), `--ods-cell` (sheet + coordinate),
`--ods-styles`, `--ods-named-expressions`, `--ods-comments`, `--ods-find`, plus
raw or decoded members.

## Unsupported observations

`Page(n)` is a typed decline — a spreadsheet has no intrinsic pagination. Formula
*dependents* are not derived (no evaluation). An ODS-native resource/media
selector is not offered (embedded resources are reached through the package
layer); this is a recorded capability gap, not a silent empty answer.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any `.ods`,
including packages the adapter declines to interpret natively, and after the
source **and** descriptor are deleted in a fresh process (the 21.3.1 court and the
21.3.2 economic court, exactness **8/8**). Exactness is inherited from the
Phase-12.2 ZIP member raw spans; the spreadsheet model is derived (`Q_gen`) and is
**never on the exactness path**.

## Security limits

XML is derived-only (bounded depth/events/nodes/attributes/text; `quick-xml`, no
DTD/entities). `table:number-columns-repeated` / `-rows-repeated` can declare
enormous counts: a declaration above `max_ods_repeated_span` declines typed
(resource limit) **before** allocation, and the expanded grid is charged against
`max_ods_cells`, so an empty-row bomb also declines typed. No `PathBuf` is ever
built from a member name; external targets are inert strings and never fetched.
See [Security](../SECURITY.md).

## Known limitations

This is a **bounded** OpenDocument spreadsheet subset, not a spreadsheet engine.
It does not evaluate formulas, render number formats, or resolve pivot tables,
charts, or external data references. The economic court is measured on a
**self-authored deterministic corpus**, not a real-world population; only the
materialized workbook is a byte-authority claim, every other observation is a
derived projection.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0030](../adr/0030-zip-physical-layer.md),
[0031](../adr/0031-common-observation-model.md),
[0038](../adr/0038-odt-adapter-scope.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.3.1 ODS court: `tests/ods_adapter.rs`; campaign
  `evidence/campaigns/2026-10-09-phase21-3-ods-ef26d97/`.
- Phase 21.3.2 economic court: `tools/phase21-3-ods-court.sh` (SQLite + DuckDB);
  campaign `evidence/campaigns/2026-10-09-phase21-3-2-ods-3dd5827/`.
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
