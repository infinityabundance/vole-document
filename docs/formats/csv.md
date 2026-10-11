# CSV / TSV / PSV

CSV/TSV/PSV is the **tabular** family (Phase 21 Wave 2). Like JSON/YAML it is
not a package — the whole source is the document, and the exact leaf is the
source. One bounded adapter with **one parser and three recorded delimiters**
(comma, tab, **pipe**) is gated behind the **non-default, dependency-free**
`csv = []` feature. PSV is therefore a *dialect* of this adapter, not a separate
format (see [PSV (pipe) dialect](#psv-pipe-dialect) below).

## Authority boundary

CSV has **no magic bytes**, so detection is a documented, conservative heuristic:
PDF/ZIP/JSON/YAML are tried first; then, after an optional BOM, a delimiter
(comma, then tab, then **pipe**) is accepted only if it yields a **modal field
count ≥ 2 over a strict majority** of a sampled record prefix and the header
shares it. The pipe dialect is tried **last**, and is **declined on a GFM/Markdown
delimiter row**, so a Markdown pipe table is never stolen. Prose, a one-column
file, a single record, and malformed input all stay `Opaque` and round-trip
exactly through the RAW lane.

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

## PSV (pipe) dialect

The pipe delimiter is the CSV/TSV adapter's **third recorded dialect** (Phase
21.25.1) — one parser, three delimiters, never a second parser. A pipe-delimited
table is claimed only with a consistent field count ≥ 2 across a sampled majority
of ≥ 2 records, mirroring the comma/tab rule, and is **declined when the source
carries a GFM/Markdown delimiter row** (so a Markdown pipe table is never
stolen). The dialect is recorded and reported (`pipe`); exact record/field spans,
original quoting/`""`/embedded delimiters, ragged rows, BOM, and the header are
preserved exactly as comma/tab. PSV needs no new feature gate (it is within
`csv = []`) and shares every native selector (`csv-row`/`csv-cell`/`csv-header`/
`csv-range`/`csv-find`) and common observation.

**Economic court.** PSV and fixed-width are measured together by the shared
engine against **three** comparators — a source-retaining SQLite store, a
conventional decode-to-host load, and the mandatory **DuckDB/Parquet** analytical
baseline (ADR-0059; DuckDB 1.5.6). On this 7-fixture corpus VOLE is byte-exact
**11/11** and keeps the build/cold/warm axes, while **DuckDB wins storage**
(paired median ~**6.6×**, geometric mean ~**7.5×**, ratio-of-sums ~**11.2×**
smaller; `large.psv` 49,804 B vs VOLE 529,027 B) — the mandatory tabular loss,
recorded not hidden. DuckDB matches VOLE on the columnar/tabular questions
(Q1/Q3/Q4/Q5/Q7/Q10) and is a **typed capability-gap** on the source-span/bytes,
recorded-dialect and column-layout/quoting questions (Q2/Q6/Q8/Q9/Q11/Q12) — 6/12
equal, 6/12 capability-gap, **0 mismatches**. See
[Formats/Fixed-width](fixedwidth.md) for the full paired-ratio table.

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
- Phase 21.25.1 PSV (pipe dialect) court (exactness 10/10, incl. the
  Markdown-boundary control):
  [2026-10-10-phase21-25-1-tabular-e3c77a86](../../evidence/campaigns/2026-10-10-phase21-25-1-tabular-e3c77a86/).
- Phase 21.25 tabular economic court (PSV + fixed-width; exactness 11/11) — the
  **3-lane** court (SQLite + conv + the mandatory DuckDB/Parquet lane, ADR-0059):
  [2026-10-10-phase21-25-tabular-econ-584ee52e](../../evidence/campaigns/2026-10-10-phase21-25-tabular-econ-584ee52e/).
  The earlier 2-lane court `…-phase21-25-tabular-econ-f0a3a5cf` (SQLite + conv
  only) is **superseded but retained**.
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
