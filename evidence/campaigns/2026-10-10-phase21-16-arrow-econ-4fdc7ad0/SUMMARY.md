# Phase 21.16 — Arrow IPC (analytical Wave 2) economic court

**Question.** On a self-authored Arrow IPC corpus, how do VOLE (an archival
field with typed observations), a source-retaining SQLite baseline, and the
DuckDB analytical comparator answer six contract questions, and does VOLE
still close **exactly**?

**Headline (recorded plainly).** DuckDB wins the analytical axes: columnar
projection, predicate execution, and metadata statistics are native to it,
while VOLE materializes a bounded observation and counts in the driver. The
pinned DuckDB wheel has no Arrow IPC file reader, so the comparator reads a
**Parquet projection** of the same logical table. VOLE's unique claims are
exactness (Q6, byte-authority) and the exact buffer span (Q3) from the
archival metadata; DuckDB is not a source-retaining store (Q6 declines
`not-native`).

## Matrix


Self-authored corpus; only **Q6 (exact closure)** is a byte-authority claim.
Ratios are VOLE / comparator; **< 1 favours VOLE**. Wall time is microseconds.

## Exactness (VOLE, after source + descriptor deletion)

| fixture | vole_ok | len_ok | sha_ok | cmp_ok |
|---|---|---|---|---|
| `e_small.arrow` | true | true | true | true |
| `e_multi.arrow` | true | true | true | true |
| `e_large.arrow` | true | true | true | true |

## Cross-lane answer agreement (per fixture, VOLE vs comparator)

| Q | question | vs sqlite (equal / gap / both-decline / mismatch) | vs duckdb (equal / gap / both-decline / mismatch) |
|---|---|---|---|
| Q1 | a column's decoded values | 3 / 0 / 0 / 0 | 3 / 0 / 0 / 0 |
| Q2 | a predicate/filtered count (numeric column > K) | 3 / 0 / 0 / 0 | 3 / 0 / 0 / 0 |
| Q3 | a column's exact buffer span + min/max statistics | 0 / 3 / 0 / 0 | 3 / 0 / 0 / 0 |
| Q4 | the record-batch count | 0 / 3 / 0 / 0 | 3 / 0 / 0 / 0 |
| Q5 | a lexical find (rows in a string column containing S) | 3 / 0 / 0 / 0 | 3 / 0 / 0 / 0 |
| Q6 | `materialize --exact` (byte-authority) | 3 / 0 / 0 / 0 | 0 / 3 / 0 / 0 |

## Paired ratios VOLE/other (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 3 | 0.455 | 0.495 | 0.448..0.595 | 0.448..0.595 | 3 | 0 | 0 | 0.514 |
| build | duckdb | 3 | 0.601 | 0.757 | 0.567..1.271 | 0.567..1.271 | 2 | 0 | 1 | 0.801 |
| storage | sqlite | 3 | 0.817 | 0.788 | 0.627..0.955 | 0.627..0.955 | 2 | 1 | 0 | 0.951 |
| storage | duckdb | 3 | 2.003 | 2.159 | 1.729..2.906 | 1.729..2.906 | 0 | 0 | 3 | 2.001 |
| cold | sqlite | 3 | 0.027 | 0.052 | 0.022..0.240 | 0.022..0.240 | 3 | 0 | 0 | 0.099 |
| cold | duckdb | 3 | 0.027 | 0.054 | 0.023..0.257 | 0.023..0.257 | 3 | 0 | 0 | 0.104 |
| warm | sqlite | 3 | 7.534 | 7.458 | 4.615..11.932 | 4.615..11.932 | 0 | 0 | 3 | 4.984 |
| warm | duckdb | 3 | 1.389 | 2.162 | 1.135..6.409 | 1.135..6.409 | 0 | 0 | 3 | 3.959 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` (a size-weighted pooled view) is reported separately. With only 3 fixture clusters the bootstrap is coarse and is stated as such.

**Read `cold` with care.** VOLE's `cold` is a single in-process observation (the driver excludes its own Python startup), while the SQLite and DuckDB `cold` samples include a fresh-process Python + `duckdb` import (tens of milliseconds). `warm` and `build`/`storage` are the fairer comparisons; the `cold` ratio is inflated by process startup and is retained only for completeness.

**Recorded plainly:** DuckDB wins the analytical axes (columnar projection, predicate execution, metadata statistics) — but note that the DuckDB lane reads a **Parquet projection** of the same logical table, because the pinned DuckDB wheel exposes only the Arrow **C data interface** scanners (`arrow_scan*`), not an Arrow IPC file reader. VOLE is an archival field with typed observations, not a query engine; its predicate/find counts are a decoded column plus a count in the driver. Only Q6 (exact closure) is a byte-authority claim, and only VOLE and the source-retaining SQLite lane can make it.

## Verdict

- VOLE exactness: **3/3**.
- Campaign: `evidence/campaigns/2026-10-10-phase21-16-arrow-econ-4fdc7ad0`.
- Profile: `release`; VOLE substrate `--profile runtime --packed`.
- Never run on the host: every command ran in the pinned, capped `analytical` container.
