# Phase 21.16 / 26.6 Arrow IPC economic court — matrix (four lanes)

Self-authored corpus; only **Q6 (exact closure)** is a byte-authority claim.
Ratios are VOLE / comparator; **< 1 favours VOLE**. Wall time is microseconds.

## Exactness (VOLE, after source + descriptor deletion)

| fixture | vole_ok | len_ok | sha_ok | cmp_ok |
|---|---|---|---|---|
| `e_small.arrow` | true | true | true | true |
| `e_multi.arrow` | true | true | true | true |
| `e_large.arrow` | true | true | true | true |

## Cross-lane answer agreement (per fixture, VOLE vs comparator)

| Q | question | vs sqlite (equal / gap / both-decline / mismatch) | vs duckdb (equal / gap / both-decline / mismatch) | vs pyarrow (equal / gap / both-decline / mismatch) |
|---|---|---|---|---|
| Q1 | a column's decoded values | 3 / 0 / 0 / 0 | 3 / 0 / 0 / 0 | 3 / 0 / 0 / 0 |
| Q2 | a predicate/filtered count (numeric column > K) | 3 / 0 / 0 / 0 | 3 / 0 / 0 / 0 | 3 / 0 / 0 / 0 |
| Q3 | a column's exact buffer span + min/max statistics | 0 / 3 / 0 / 0 | 3 / 0 / 0 / 0 | 0 / 3 / 0 / 0 |
| Q4 | the record-batch count | 0 / 3 / 0 / 0 | 3 / 0 / 0 / 0 | 3 / 0 / 0 / 0 |
| Q5 | a lexical find (rows in a string column containing S) | 3 / 0 / 0 / 0 | 3 / 0 / 0 / 0 | 3 / 0 / 0 / 0 |
| Q6 | `materialize --exact` (byte-authority) | 3 / 0 / 0 / 0 | 0 / 3 / 0 / 0 | 0 / 3 / 0 / 0 |

## Paired ratios VOLE/other (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 3 | 0.509 | 0.520 | 0.462..0.599 | 0.462..0.599 | 3 | 0 | 0 | 0.535 |
| build | duckdb | 3 | 0.626 | 0.776 | 0.600..1.244 | 0.600..1.244 | 2 | 0 | 1 | 0.818 |
| build | pyarrow | 3 | 0.532 | 0.675 | 0.526..1.099 | 0.526..1.099 | 2 | 1 | 0 | 0.711 |
| storage | sqlite | 3 | 0.816 | 0.788 | 0.627..0.955 | 0.627..0.955 | 2 | 1 | 0 | 0.951 |
| storage | duckdb | 3 | 2.003 | 2.158 | 1.726..2.908 | 1.726..2.908 | 0 | 0 | 3 | 2.000 |
| storage | pyarrow | 3 | 2.156 | 2.263 | 2.002..2.684 | 2.002..2.684 | 0 | 0 | 3 | 2.007 |
| cold | sqlite | 3 | 0.028 | 0.053 | 0.023..0.233 | 0.023..0.233 | 3 | 0 | 0 | 0.099 |
| cold | duckdb | 3 | 0.029 | 0.056 | 0.025..0.243 | 0.025..0.243 | 3 | 0 | 0 | 0.104 |
| cold | pyarrow | 3 | 0.026 | 0.050 | 0.021..0.226 | 0.021..0.226 | 3 | 0 | 0 | 0.093 |
| warm | sqlite | 3 | 7.838 | 7.542 | 4.509..12.140 | 4.509..12.140 | 0 | 0 | 3 | 4.894 |
| warm | duckdb | 3 | 1.387 | 2.184 | 1.122..6.697 | 1.122..6.697 | 0 | 0 | 3 | 4.028 |
| warm | pyarrow | 3 | 8.626 | 10.475 | 7.738..17.219 | 7.738..17.219 | 0 | 0 | 3 | 14.744 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` (a size-weighted pooled view) is reported separately. With only 3 fixture clusters the bootstrap is coarse and is stated as such.

**Read `cold` with care.** VOLE's `cold` is a single in-process observation (the driver excludes its own Python startup), while the SQLite, DuckDB and pyarrow `cold` samples include a fresh-process Python + `duckdb`/`pyarrow` import (tens of milliseconds). `warm` and `build`/`storage` are the fairer comparisons; the `cold` ratio is inflated by process startup and is retained only for completeness.

**Recorded plainly:** pyarrow (the reference Arrow reader, opening the `.arrow` file directly) and DuckDB both win the analytical computation axes (columnar projection, predicate execution, metadata statistics) — but the DuckDB lane reads a **Parquet projection** of the same logical table, because the pinned DuckDB wheel exposes only the Arrow **C data interface** scanners (`arrow_scan*`), not an Arrow IPC file reader. Only the pyarrow lane measures Arrow on its own terms. VOLE is an archival field with typed observations, not a query engine. Only Q6 (exact closure) is a byte-authority claim, and only VOLE and the source-retaining SQLite lane can make it; DuckDB and pyarrow decline Q6 (`not-native`), and both decline Q3 (no exact source buffer span or per-column statistics in the Arrow IPC metadata).
