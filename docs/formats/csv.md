# CSV / TSV

CSV/TSV is the **tabular** family (Phase 21 Wave 2). Like JSON/YAML it is not a
package — the whole source is the document, and the exact leaf is the source. The
adapter is gated behind the **non-default, dependency-free** `csv = []` feature.

## Authority boundary

CSV has **no magic bytes**, so detection is a documented, conservative heuristic:
PDF/ZIP/JSON/YAML are tried first; then, after an optional BOM, a delimiter
(comma, then tab) is accepted only if it yields a **modal field count ≥ 2 over a
strict majority** of a sampled record prefix and the header shares it. Prose, a
one-column file, a single record, and malformed input all stay `Opaque` and
round-trip exactly through the RAW lane.

## Representation preservation

RFC 4180 CSV + TSV: quoted fields (`""` escapes), embedded delimiters/newlines/
quotes, CRLF/LF/CR, optional BOM, and a header row. Every record and field keeps
its **exact source byte span**; the **dialect** (delimiter, quote char, line
terminator) is recorded and reported; quoting/whitespace is not normalized away.
Ragged rows are preserved verbatim (the model reports `ragged_records` and
`modal_columns`), not silently reinterpreted.

## Supported observations

Common selectors: `metadata` (rows, cols, dialect, header), `text`, `table`,
`cell`, `find`. Native: `--csv-row N` (a record's exact bytes), `--csv-cell R:C`
(0-based) or `R:COLNAME` (header-name column), `--csv-header`, `--csv-range`,
`--csv-find`.

## Unsupported / honest cost

`Page(n)` is a typed decline (a table has no pagination). **VOLE has no CSV
index**: `--csv-row`/`--csv-cell` are O(offset) bounded-memory forward scans, not
constant-time lookups. Exotic quoting (non-`"` quotes, escape characters) is not
claimed. Over any `max_csv_*` cap, the model declines typed
(`InvalidCsvStructure`, exit 23).

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted CSV,
and after the source **and** descriptor are deleted in a fresh process (the 21.7.1
court and the 21.7 economic court, exactness **10/10** and **6/6**). The exact leaf
is the whole source; the derived model is never on the exactness path (ADR-0060:
the `CsvModel` node depends on the `DocumentExact` root keyed on `sha256(source)`).

## Security limits

`max_csv_*` caps (rows, cols, record bytes, field bytes, document bytes, sampled
records for detection); bounded memory — no full-file structure is built. No
external reference is fetched.

## Known limitations

A bounded, dialect-detecting CSV/TSV reader, not a full dialect library. The
economic court is measured on a **self-authored deterministic corpus**; only exact
closure is a byte-authority claim. The economic court includes the **mandatory
DuckDB/Parquet comparator** for tabular formats (ADR-0059) — and DuckDB **wins**
storage (Parquet/ZSTD ~6.4× smaller), large-file ingest, and indexed point reads
here; that loss is recorded, not hidden.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0059](../adr/0059-xlsx-adapter-and-analytical-comparator.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.7.1 CSV court: `tests/csv_adapter.rs`; campaign
  `evidence/campaigns/2026-10-09-phase21-7-1-csv-49f523ba/`.
- Phase 21.7 economic court: `tools/phase21-7-csv-court.sh` (SQLite + DuckDB);
  campaign `evidence/campaigns/2026-10-09-phase21-7-csv-econ-49f523ba/`.
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
