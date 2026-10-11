# Phase 26.6 — Arrow IPC (analytical Wave 2) economic court (+ pyarrow lane)

**Question.** On a self-authored Arrow IPC corpus, how do VOLE (an archival
field with typed observations), a source-retaining SQLite baseline, a DuckDB
analytical baseline over a Parquet projection, and a pyarrow lane over the
Arrow IPC file itself answer six contract questions, and does VOLE still close
**exactly**?

**Headline (recorded plainly).** pyarrow — the reference Arrow reader, opening
the `.arrow` file directly — and DuckDB both win the analytical computation axes
(columnar projection, predicate execution, metadata statistics), while VOLE
materializes a bounded observation and counts in the driver. DuckDB still reads a
**Parquet projection** because its pinned wheel has no Arrow IPC reader; only the
pyarrow lane measures Arrow on its own terms. VOLE's unique claims are exactness
(Q6, byte-authority) and the exact buffer span (Q3) from the archival metadata;
neither DuckDB nor pyarrow is a source-retaining store (Q6 declines
`not-native`) and neither exposes a source buffer span or min/max statistics
(Q3 declines).

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
| build | sqlite | 3 | 0.442 | 0.484 | 0.441..0.581 | 0.441..0.581 | 3 | 0 | 0 | 0.501 |
| build | duckdb | 3 | 0.589 | 0.727 | 0.576..1.131 | 0.576..1.131 | 2 | 0 | 1 | 0.765 |
| build | pyarrow | 3 | 0.512 | 0.628 | 0.481..1.007 | 0.481..1.007 | 2 | 1 | 0 | 0.661 |
| storage | sqlite | 3 | 0.816 | 0.788 | 0.627..0.955 | 0.627..0.955 | 2 | 1 | 0 | 0.951 |
| storage | duckdb | 3 | 2.003 | 2.158 | 1.726..2.908 | 1.726..2.908 | 0 | 0 | 3 | 2.000 |
| storage | pyarrow | 3 | 2.156 | 2.263 | 2.002..2.684 | 2.002..2.684 | 0 | 0 | 3 | 2.007 |
| cold | sqlite | 3 | 0.025 | 0.050 | 0.021..0.231 | 0.021..0.231 | 3 | 0 | 0 | 0.094 |
| cold | duckdb | 3 | 0.026 | 0.053 | 0.023..0.241 | 0.023..0.241 | 3 | 0 | 0 | 0.100 |
| cold | pyarrow | 3 | 0.024 | 0.048 | 0.020..0.224 | 0.020..0.224 | 3 | 0 | 0 | 0.090 |
| warm | sqlite | 3 | 6.301 | 6.935 | 4.688..11.292 | 4.688..11.292 | 0 | 0 | 3 | 4.981 |
| warm | duckdb | 3 | 1.224 | 2.010 | 0.999..6.643 | 0.999..6.643 | 0 | 1 | 2 | 3.864 |
| warm | pyarrow | 3 | 7.295 | 9.452 | 6.812..16.994 | 6.812..16.994 | 0 | 0 | 3 | 14.131 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` (a size-weighted pooled view) is reported separately. With only 3 fixture clusters the bootstrap is coarse and is stated as such.

**Read `cold` with care.** VOLE's `cold` is a single in-process observation (the driver excludes its own Python startup), while the SQLite, DuckDB and pyarrow `cold` samples include a fresh-process Python + `duckdb`/`pyarrow` import (tens of milliseconds). `warm` and `build`/`storage` are the fairer comparisons; the `cold` ratio is inflated by process startup and is retained only for completeness.

**Recorded plainly:** pyarrow (the reference Arrow reader, opening the `.arrow` file directly) and DuckDB both win the analytical computation axes (columnar projection, predicate execution, metadata statistics) — but the DuckDB lane reads a **Parquet projection** of the same logical table, because the pinned DuckDB wheel exposes only the Arrow **C data interface** scanners (`arrow_scan*`), not an Arrow IPC file reader. Only the pyarrow lane measures Arrow on its own terms. VOLE is an archival field with typed observations, not a query engine. Only Q6 (exact closure) is a byte-authority claim, and only VOLE and the source-retaining SQLite lane can make it; DuckDB and pyarrow decline Q6 (`not-native`), and both decline Q3 (no exact source buffer span or per-column statistics in the Arrow IPC metadata).

## Verdict

- VOLE exactness: **3/3**.
- Campaign: `evidence/campaigns/2026-10-11-phase21-16-arrow-econ-74965426`.
- Profile: `release`; VOLE substrate `--profile runtime --packed`.
- Never run on the host: every command ran in the pinned, capped `analytical` container.
