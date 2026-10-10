# Phase 21.14 Parquet economic court — matrix

Self-authored corpus; only **Q6 (exact closure)** is a byte-authority claim.
Ratios are VOLE / comparator; **< 1 favours VOLE**. Wall time is microseconds.

## Exactness (VOLE, after source + descriptor deletion)

| fixture | vole_ok | len_ok | sha_ok | cmp_ok |
|---|---|---|---|---|
| `small_plain.parquet` | true | true | true | true |
| `optional.parquet` | true | true | true | true |
| `dictionary.parquet` | true | true | true | true |
| `gzip.parquet` | true | true | true | true |
| `multi_rg.parquet` | true | true | true | true |
| `two_pages.parquet` | true | true | true | true |
| `large.parquet` | true | true | true | true |

## Cross-lane answer agreement (per fixture, VOLE vs comparator)

| Q | question | vs sqlite (equal / gap / both-decline / mismatch) | vs duckdb (equal / gap / both-decline / mismatch) |
|---|---|---|---|
| Q1 | a column's decoded values | 7 / 0 / 0 / 0 | 7 / 0 / 0 / 0 |
| Q2 | a predicate/filtered count (numeric column > K) | 7 / 0 / 0 / 0 | 7 / 0 / 0 / 0 |
| Q3 | a column chunk's exact span + min/max statistics | 0 / 7 / 0 / 0 | 7 / 0 / 0 / 0 |
| Q4 | the row-group count | 0 / 7 / 0 / 0 | 7 / 0 / 0 / 0 |
| Q5 | a lexical find (rows in a string column containing S) | 7 / 0 / 0 / 0 | 7 / 0 / 0 / 0 |
| Q6 | `materialize --exact` (byte-authority) | 7 / 0 / 0 / 0 | 0 / 7 / 0 / 0 |

## Paired ratios VOLE/other (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 7 | 0.474 | 0.490 | 0.458..0.488 | 0.464..0.533 | 7 | 0 | 0 | 0.502 |
| build | duckdb | 7 | 0.601 | 0.676 | 0.598..0.636 | 0.600..0.841 | 6 | 0 | 1 | 0.700 |
| storage | sqlite | 7 | 0.406 | 0.424 | 0.328..0.413 | 0.349..0.569 | 6 | 1 | 0 | 0.942 |
| storage | duckdb | 7 | 6.602 | 7.396 | 6.337..14.595 | 4.431..11.162 | 0 | 0 | 7 | 2.015 |
| cold | sqlite | 7 | 0.018 | 0.026 | 0.018..0.019 | 0.018..0.053 | 7 | 0 | 0 | 0.047 |
| cold | duckdb | 7 | 0.019 | 0.027 | 0.019..0.020 | 0.019..0.055 | 7 | 0 | 0 | 0.049 |
| warm | sqlite | 7 | 12.161 | 10.574 | 11.825..13.305 | 7.462..12.880 | 0 | 0 | 7 | 4.901 |
| warm | duckdb | 7 | 0.967 | 1.211 | 0.892..1.080 | 0.912..1.998 | 3 | 3 | 1 | 2.163 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` (a size-weighted pooled view) is reported separately. With only 7 fixture clusters the bootstrap is coarse and is stated as such.

**Read `cold` with care.** VOLE's `cold` is a single in-process observation (the driver excludes its own Python startup), while the SQLite and DuckDB `cold` samples include a fresh-process Python + `duckdb` import (tens of milliseconds). `warm` (a repeated in-process measure for every lane) and `build`/`storage` are the fairer comparisons; the `cold` ratio is inflated by process startup and is retained only for completeness.

**Recorded plainly:** DuckDB wins the analytical axes (projection, predicate execution, compressed pages, selective reads via metadata statistics). VOLE is an archival field with typed observations, not a query engine; its predicate counts are a decoded column plus a count in the driver. Only Q6 (exact closure) is a byte-authority claim, and only VOLE and the source-retaining SQLite lane can make it.
