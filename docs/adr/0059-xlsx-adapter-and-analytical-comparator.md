# ADR-0059 — XLSX adapter scope: OPC SpreadsheetML, five distinct cell fields, and a mandatory analytical comparator

Status: accepted (Phase 21.1).
Extends ADR-0029/0030/0031. Relates: ADR-0032 (DOCX), ADR-0038 (ODT adapter
scope), ADR-0051 (direct field ingestion), ADR-0054 (paired measurement).
Cites phase-21 plan §21.1.

## Context

An `.xlsx` is an **OPC package** on the shared byte-authoritative ZIP layer
(ADR-0030): a ZIP carrying `[Content_Types].xml`, `_rels/.rels`, and a
SpreadsheetML workbook part. Unlike ODF (ADR-0038) it *is* OPC, so it reuses the
existing OPC content-type/relationship machinery rather than adding a second
package model.

Spreadsheets raise two questions the four text formats did not. First, a cell is
**not one value**: it carries a *stored formula*, a *cached result*, a
number-format-dependent *displayed value*, a *style*, and an *XML span* — five
different facts with different provenance, which a naive "cell value" pipeline
silently collapses (e.g. `=A1+B1` → `42`). Second, XLSX is simultaneously an
office document and a **tabular, analytical** workload, so comparing it only
against SQLite could simply pick the wrong competitor.

The two easy mistakes are (a) conflating those five fields or *evaluating* a
formula, and (b) treating a derived SpreadsheetML projection as if it were the
archival authority.

## Decision

* **OPC/SpreadsheetML discovery.** Detection is byte-based, never by extension:
  the package's content types must declare the SpreadsheetML workbook main content
  type (`application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml`),
  or an `officeDocument` relationship must target `xl/workbook.xml`. Mutual
  exclusivity with `docx` uses a **positive** WordprocessingML signal
  (`wordprocessingml.document.main+xml`, or a `word/document.xml` officeDocument
  target), *not* the absence of a SpreadsheetML one — so a Word document that
  embeds an Excel workbook is still `docx`, never `Opaque`.
* **Non-default feature.** `xlsx = ["opc"]` is opt-in; a build without it cannot
  detect XLSX and reports it `Opaque`, so the reported capability set always
  matches what the build serves.
* **Five distinct cell fields, never conflated.** `stored formula` (`<f>`),
  `cached result` (`<v>`), `displayed value` (a bounded, deterministic projection
  over a small grammar of number-format codes, labelled
  `deterministically-derived`), `resolved style` (`cellXfs`: number format, font,
  fill, alignment), and the cell's `XML span` are separate fields with separate
  provenance. **Formulas are never evaluated.**
* **Bounded SpreadsheetML model** in document order: sheet order/name/visibility;
  rows and cells (shared vs inline strings, booleans, errors); merged ranges (their
  `ref` strings); comments (`xl/comments*.xml`, keyed by cell, plus VML note
  anchors); internal (`location`) and external (`r:id`) hyperlinks resolved through
  the sheet `_rels`; defined/named ranges (hidden and local included); tables
  (`name`/`ref`/`columns` via `tableParts`); drawings/charts/media as a
  relationship graph (drawing bytes resolve exactly; chart XML is never evaluated);
  and package external relationships as typed metadata that is **never
  dereferenced**.
* **Typed declines, never silent answers.** An unsupported
  selector/representation pair, a BYTES request for a missing part, and an element
  that *references* a missing part all decline typed; a metadata observation for an
  *optional* part that is simply absent answers an explicit absence (`present:false`
  / `null`) at exit 0 — an answer, not a decline.
* **Exactness is inherited, never derived.** The exact leaf is the 12.2 ZIP member
  raw span; every SpreadsheetML model is `Q_gen` and off the exactness path.
  `materialize == source` (length + SHA-256 + `cmp`) holds after source **and**
  descriptor deletion in a fresh process.
* **Hostile-input guards.** Cell references are parsed with checked arithmetic (an
  over-long A1 reference declines typed rather than wrapping `u64`), coordinates are
  bounded to the Excel-conformant grid (`max_xlsx_col` 16384, `max_xlsx_row` 1<<20),
  and the sheet-text projection is bounded *before* it is built, so one hostile
  coordinate cannot drive an unbounded allocation.
* **A mandatory analytical comparator.** For the tabular/analytical formats
  (CSV/TSV, XLSX, ODS) the economic court **must** include a **DuckDB/Parquet**
  baseline alongside SQLite, because those engines already embody columnar
  projection, predicate pushdown, compressed pages, and metadata indexes; winning
  only against SQLite there could just mean the wrong competitor was chosen (the
  Phase-22 "maximize the competitor first" rule). DuckDB is compared as a
  columnar/tabular comparator only — it provides no exact-source closure or
  provenance, and it is not compared as though it did.
* **Release-profile fairness.** The economic court measures a **release** VOLE
  binary (`PROFILE=release`) and the established packed runtime substrate
  (`field-build --profile runtime --packed`), because the C/Python comparators are
  profile-invariant while the debug entropyfs build is not.

## Consequences

* An invalid or hostile workbook is still an exact archival object; only the
  *derived* observation is declined, and the `materialize == source` court passes
  (21.1.3 exactness **8/8**).
* Every derived answer is labelled (`exact == false`, basis
  `deterministically-derived`) and keeps a path back to the original bytes; the
  runtime's economic mechanism stands on its own merits (ADR-0056), not on any
  claim that RAW is a compact reconstructive program.
* The XLSX economic court (21.1.3) measures the frontier against **both** a
  source-retaining SQLite baseline and a DuckDB/Parquet baseline, scoped to a
  self-authored deterministic corpus; formulas/dependents and chart→table linkage
  are recorded **capability gaps**, never equivalences.

**Rejected:** evaluating formulas; collapsing formula/cached/displayed/style/span
into one "value"; treating a derived projection as the archival authority;
detecting XLSX by the mere presence of a `spreadsheetml` content type (which
misclassifies a DOCX that embeds a workbook); comparing a tabular format only
against SQLite; measuring the debug binary against C/Python competitors.
