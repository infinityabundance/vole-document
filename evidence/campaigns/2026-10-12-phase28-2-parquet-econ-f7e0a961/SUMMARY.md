# Phase 28.2 — Parquet (analytical Wave 2) economic court — competitor envelope

**Question.** On a self-authored Parquet corpus, how do VOLE (an archival
field with typed observations), a source-retaining SQLite baseline, and the
mandatory DuckDB/Parquet analytical comparator answer six contract questions,
and does VOLE still close **exactly**?

**Headline (recorded plainly).** DuckDB wins the analytical axes: columnar
projection, predicate execution, compressed pages, and selective reads via
row-group statistics (zonemap pruning) are native to it, while VOLE materializes
a bounded observation and counts in the driver. VOLE's unique claims are
exactness (Q6, byte-authority) and the exact chunk span (Q3) from the archival
footer; DuckDB is not a source-retaining store (Q6 declines `not-native`).

**Measurement (28.2).** The headline is **in-process steady-state**, reported
separately from **cold-process** and **cold-cache**. The pre-28.2 numbers are
kept as an explicit historical control.

## Matrix


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

## Capability accounting (per lane, over 6 questions x 7 fixtures)

A lane that **declines** what another answers has not won an equal-contract
comparison; every typed decline is counted, never hidden.

| lane | answered | declined | decline codes | vs VOLE equal / gap / both-decline / mismatch |
|---|---:|---:|---|---|
| vole | 42 | 0 | - | (reference) |
| sqlite | 28 | 14 | not-retained=14 | 28 / 14 / 0 / 0 |
| duckdb | 35 | 7 | not-native=7 | 35 / 7 / 0 / 0 |

## Paired ratios VOLE/other (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 7 | 0.483 | 0.474 | 0.449..0.511 | 0.440..0.502 | 7 | 0 | 0 | 0.459 |
| build | duckdb | 7 | 0.662 | 0.702 | 0.624..0.668 | 0.625..0.850 | 6 | 0 | 1 | 0.726 |
| storage | sqlite | 7 | 0.178 | 0.192 | 0.144..0.181 | 0.153..0.275 | 7 | 0 | 0 | 0.514 |
| storage | duckdb | 7 | 5.968 | 6.423 | 5.760..11.284 | 4.077..9.146 | 0 | 0 | 7 | 2.015 |
| steady | sqlite | 7 | 230.941 | 126.568 | 159.500..256.188 | 39.065..246.375 | 0 | 0 | 7 | 6.077 |
| steady | duckdb | 7 | 1.462 | 1.833 | 1.423..1.583 | 1.425..2.919 | 0 | 0 | 7 | 4.174 |
| cold-process | sqlite | 7 | 0.022 | 0.030 | 0.021..0.022 | 0.021..0.060 | 7 | 0 | 0 | 0.055 |
| cold-process | duckdb | 7 | 0.009 | 0.014 | 0.009..0.009 | 0.009..0.034 | 7 | 0 | 0 | 0.037 |
| cold-cache | sqlite | 7 | 86.535 | 55.193 | 66.135..91.367 | 23.648..88.851 | 0 | 0 | 7 | 10.104 |
| cold-cache | duckdb | 7 | 4.155 | 4.299 | 4.014..4.393 | 3.734..5.118 | 0 | 0 | 7 | 4.963 |

A ratio < 1 favours VOLE. The **headline is `steady`** (in-process, warm
connection/state; per-question median). Question-level metrics (steady,
cold-process, cold-cache) are summed per fixture over the **common answer set**
(only questions BOTH lanes answer), so a lane is never charged for work its
comparator declines. `build`/`storage` are whole-artifact. The estimator is the
**paired per-fixture ratio**, summarised by the median and geometric mean with a
fixed-seed cluster bootstrap over fixtures; `ratio of sums` (a size-weighted pooled
view) is reported separately. With only 7 fixture clusters the bootstrap is coarse
and is stated as such.

## Steady-state percentiles per question (us)

| fixture | lane | Q | min | p50 | p95 | p99 | n |
|---|---|---|---:|---:|---:|---:|---:|
| `small_plain.parquet` | vole | Q1 | 419 | 449.0 | 524.5 | 538.5 | 15 |
| `small_plain.parquet` | vole | Q2 | 404 | 433.0 | 492.6 | 510.5 | 15 |
| `small_plain.parquet` | vole | Q3 | 379 | 426.0 | 480.3 | 492.1 | 15 |
| `small_plain.parquet` | vole | Q4 | 363 | 399.0 | 468.1 | 505.6 | 15 |
| `small_plain.parquet` | vole | Q5 | 369 | 419.0 | 485.4 | 529.1 | 15 |
| `small_plain.parquet` | vole | Q6 | 2709 | 2798.0 | 4553.4 | 5504.3 | 15 |
| `small_plain.parquet` | sqlite | Q1 | 4 | 4.0 | 6.3 | 6.9 | 15 |
| `small_plain.parquet` | sqlite | Q2 | 2 | 2.0 | 3.0 | 3.0 | 15 |
| `small_plain.parquet` | sqlite | Q3 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `small_plain.parquet` | sqlite | Q4 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `small_plain.parquet` | sqlite | Q5 | 6 | 7.0 | 7.0 | 7.0 | 15 |
| `small_plain.parquet` | sqlite | Q6 | 3 | 3.0 | 3.3 | 3.9 | 15 |
| `small_plain.parquet` | duckdb | Q1 | 186 | 206.0 | 290.7 | 385.3 | 15 |
| `small_plain.parquet` | duckdb | Q2 | 236 | 246.0 | 305.8 | 331.6 | 15 |
| `small_plain.parquet` | duckdb | Q3 | 263 | 307.0 | 522.6 | 658.1 | 15 |
| `small_plain.parquet` | duckdb | Q4 | 285 | 325.0 | 383.9 | 385.6 | 15 |
| `small_plain.parquet` | duckdb | Q5 | 250 | 259.0 | 293.6 | 294.7 | 15 |
| `small_plain.parquet` | duckdb | Q6 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `optional.parquet` | vole | Q1 | 355 | 428.0 | 472.6 | 479.3 | 15 |
| `optional.parquet` | vole | Q2 | 368 | 389.0 | 418.8 | 422.2 | 15 |
| `optional.parquet` | vole | Q3 | 366 | 409.0 | 455.6 | 467.9 | 15 |
| `optional.parquet` | vole | Q4 | 349 | 383.0 | 407.6 | 425.5 | 15 |
| `optional.parquet` | vole | Q5 | 352 | 375.0 | 407.6 | 419.9 | 15 |
| `optional.parquet` | vole | Q6 | 2625 | 2702.0 | 3488.0 | 3980.8 | 15 |
| `optional.parquet` | sqlite | Q1 | 3 | 3.0 | 4.6 | 5.7 | 15 |
| `optional.parquet` | sqlite | Q2 | 2 | 2.0 | 3.0 | 3.0 | 15 |
| `optional.parquet` | sqlite | Q3 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `optional.parquet` | sqlite | Q4 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `optional.parquet` | sqlite | Q5 | 6 | 6.0 | 6.3 | 6.9 | 15 |
| `optional.parquet` | sqlite | Q6 | 2 | 3.0 | 3.0 | 3.0 | 15 |
| `optional.parquet` | duckdb | Q1 | 176 | 200.0 | 230.5 | 238.9 | 15 |
| `optional.parquet` | duckdb | Q2 | 239 | 254.0 | 289.6 | 301.9 | 15 |
| `optional.parquet` | duckdb | Q3 | 236 | 307.0 | 393.1 | 453.0 | 15 |
| `optional.parquet` | duckdb | Q4 | 263 | 350.0 | 413.1 | 456.2 | 15 |
| `optional.parquet` | duckdb | Q5 | 247 | 283.0 | 302.3 | 302.9 | 15 |
| `optional.parquet` | duckdb | Q6 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `dictionary.parquet` | vole | Q1 | 368 | 406.0 | 492.1 | 496.0 | 15 |
| `dictionary.parquet` | vole | Q2 | 353 | 421.0 | 467.1 | 499.0 | 15 |
| `dictionary.parquet` | vole | Q3 | 350 | 410.0 | 466.9 | 491.0 | 15 |
| `dictionary.parquet` | vole | Q4 | 341 | 356.0 | 423.4 | 439.1 | 15 |
| `dictionary.parquet` | vole | Q5 | 361 | 394.0 | 444.9 | 457.8 | 15 |
| `dictionary.parquet` | vole | Q6 | 2584 | 2705.0 | 2793.6 | 2833.9 | 15 |
| `dictionary.parquet` | sqlite | Q1 | 5 | 5.0 | 6.6 | 7.7 | 15 |
| `dictionary.parquet` | sqlite | Q2 | 2 | 2.0 | 3.0 | 3.0 | 15 |
| `dictionary.parquet` | sqlite | Q3 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `dictionary.parquet` | sqlite | Q4 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `dictionary.parquet` | sqlite | Q5 | 7 | 7.0 | 8.0 | 8.0 | 15 |
| `dictionary.parquet` | sqlite | Q6 | 2 | 3.0 | 3.0 | 3.0 | 15 |
| `dictionary.parquet` | duckdb | Q1 | 193 | 237.0 | 466.8 | 638.2 | 15 |
| `dictionary.parquet` | duckdb | Q2 | 250 | 293.0 | 329.2 | 337.0 | 15 |
| `dictionary.parquet` | duckdb | Q3 | 256 | 329.0 | 450.2 | 558.8 | 15 |
| `dictionary.parquet` | duckdb | Q4 | 298 | 326.0 | 437.8 | 441.2 | 15 |
| `dictionary.parquet` | duckdb | Q5 | 252 | 271.0 | 288.9 | 296.2 | 15 |
| `dictionary.parquet` | duckdb | Q6 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `gzip.parquet` | vole | Q1 | 388 | 446.0 | 487.6 | 488.7 | 15 |
| `gzip.parquet` | vole | Q2 | 377 | 411.0 | 472.2 | 502.4 | 15 |
| `gzip.parquet` | vole | Q3 | 369 | 410.0 | 521.8 | 659.6 | 15 |
| `gzip.parquet` | vole | Q4 | 348 | 374.0 | 412.8 | 427.4 | 15 |
| `gzip.parquet` | vole | Q5 | 354 | 386.0 | 456.0 | 467.2 | 15 |
| `gzip.parquet` | vole | Q6 | 2642 | 2688.0 | 2738.8 | 2759.0 | 15 |
| `gzip.parquet` | sqlite | Q1 | 4 | 5.0 | 5.6 | 6.7 | 15 |
| `gzip.parquet` | sqlite | Q2 | 2 | 2.0 | 3.0 | 3.0 | 15 |
| `gzip.parquet` | sqlite | Q3 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `gzip.parquet` | sqlite | Q4 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `gzip.parquet` | sqlite | Q5 | 6 | 7.0 | 7.0 | 7.0 | 15 |
| `gzip.parquet` | sqlite | Q6 | 3 | 3.0 | 3.3 | 3.9 | 15 |
| `gzip.parquet` | duckdb | Q1 | 178 | 200.0 | 238.9 | 246.2 | 15 |
| `gzip.parquet` | duckdb | Q2 | 250 | 263.0 | 298.5 | 306.9 | 15 |
| `gzip.parquet` | duckdb | Q3 | 266 | 293.0 | 401.6 | 458.7 | 15 |
| `gzip.parquet` | duckdb | Q4 | 286 | 360.0 | 439.8 | 471.2 | 15 |
| `gzip.parquet` | duckdb | Q5 | 242 | 253.0 | 385.2 | 516.2 | 15 |
| `gzip.parquet` | duckdb | Q6 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `multi_rg.parquet` | vole | Q1 | 386 | 461.0 | 552.2 | 576.8 | 15 |
| `multi_rg.parquet` | vole | Q2 | 370 | 430.0 | 615.3 | 800.7 | 15 |
| `multi_rg.parquet` | vole | Q3 | 358 | 410.0 | 483.5 | 486.3 | 15 |
| `multi_rg.parquet` | vole | Q4 | 355 | 394.0 | 454.0 | 465.2 | 15 |
| `multi_rg.parquet` | vole | Q5 | 358 | 393.0 | 508.5 | 528.1 | 15 |
| `multi_rg.parquet` | vole | Q6 | 2822 | 2863.0 | 4481.2 | 5597.8 | 15 |
| `multi_rg.parquet` | sqlite | Q1 | 6 | 7.0 | 8.3 | 8.9 | 15 |
| `multi_rg.parquet` | sqlite | Q2 | 2 | 3.0 | 3.0 | 3.0 | 15 |
| `multi_rg.parquet` | sqlite | Q3 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `multi_rg.parquet` | sqlite | Q4 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `multi_rg.parquet` | sqlite | Q5 | 13 | 13.0 | 14.0 | 14.0 | 15 |
| `multi_rg.parquet` | sqlite | Q6 | 3 | 3.0 | 3.3 | 3.9 | 15 |
| `multi_rg.parquet` | duckdb | Q1 | 187 | 217.0 | 325.1 | 401.8 | 15 |
| `multi_rg.parquet` | duckdb | Q2 | 242 | 262.0 | 292.7 | 308.9 | 15 |
| `multi_rg.parquet` | duckdb | Q3 | 245 | 296.0 | 395.9 | 403.2 | 15 |
| `multi_rg.parquet` | duckdb | Q4 | 284 | 359.0 | 421.8 | 442.0 | 15 |
| `multi_rg.parquet` | duckdb | Q5 | 266 | 294.0 | 325.9 | 344.4 | 15 |
| `multi_rg.parquet` | duckdb | Q6 | 0 | 0.0 | 1.3 | 1.9 | 15 |
| `two_pages.parquet` | vole | Q1 | 390 | 449.0 | 595.2 | 631.0 | 15 |
| `two_pages.parquet` | vole | Q2 | 346 | 383.0 | 409.5 | 412.3 | 15 |
| `two_pages.parquet` | vole | Q3 | 370 | 420.0 | 645.0 | 969.8 | 15 |
| `two_pages.parquet` | vole | Q4 | 350 | 401.0 | 448.5 | 456.9 | 15 |
| `two_pages.parquet` | vole | Q5 | 350 | 370.0 | 454.3 | 454.9 | 15 |
| `two_pages.parquet` | vole | Q6 | 2701 | 2741.0 | 2779.7 | 2801.5 | 15 |
| `two_pages.parquet` | sqlite | Q1 | 6 | 6.0 | 7.6 | 8.7 | 15 |
| `two_pages.parquet` | sqlite | Q2 | 2 | 2.0 | 3.0 | 3.0 | 15 |
| `two_pages.parquet` | sqlite | Q3 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `two_pages.parquet` | sqlite | Q4 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `two_pages.parquet` | sqlite | Q5 | 7 | 8.0 | 8.0 | 8.0 | 15 |
| `two_pages.parquet` | sqlite | Q6 | 2 | 3.0 | 3.3 | 3.9 | 15 |
| `two_pages.parquet` | duckdb | Q1 | 180 | 199.0 | 232.0 | 237.6 | 15 |
| `two_pages.parquet` | duckdb | Q2 | 237 | 272.0 | 305.7 | 321.9 | 15 |
| `two_pages.parquet` | duckdb | Q3 | 237 | 311.0 | 327.6 | 328.7 | 15 |
| `two_pages.parquet` | duckdb | Q4 | 258 | 324.0 | 402.3 | 414.1 | 15 |
| `two_pages.parquet` | duckdb | Q5 | 258 | 292.0 | 316.9 | 318.6 | 15 |
| `two_pages.parquet` | duckdb | Q6 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `large.parquet` | vole | Q1 | 14226 | 15084.0 | 15813.7 | 16104.3 | 15 |
| `large.parquet` | vole | Q2 | 10395 | 10754.0 | 11324.9 | 11595.4 | 15 |
| `large.parquet` | vole | Q3 | 10304 | 10861.0 | 11677.0 | 12265.0 | 15 |
| `large.parquet` | vole | Q4 | 4803 | 5011.0 | 5840.6 | 6653.7 | 15 |
| `large.parquet` | vole | Q5 | 11407 | 12414.0 | 12853.8 | 12991.6 | 15 |
| `large.parquet` | vole | Q6 | 10846 | 11149.0 | 11640.6 | 11843.3 | 15 |
| `large.parquet` | sqlite | Q1 | 9205 | 9244.0 | 9283.7 | 9299.9 | 15 |
| `large.parquet` | sqlite | Q2 | 243 | 244.0 | 253.2 | 266.6 | 15 |
| `large.parquet` | sqlite | Q3 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `large.parquet` | sqlite | Q4 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `large.parquet` | sqlite | Q5 | 1171 | 1177.0 | 1201.4 | 1217.1 | 15 |
| `large.parquet` | sqlite | Q6 | 1286 | 1294.0 | 1307.1 | 1311.0 | 15 |
| `large.parquet` | duckdb | Q1 | 5244 | 5482.0 | 6362.3 | 6446.9 | 15 |
| `large.parquet` | duckdb | Q2 | 272 | 296.0 | 338.3 | 338.9 | 15 |
| `large.parquet` | duckdb | Q3 | 248 | 314.0 | 347.8 | 356.8 | 15 |
| `large.parquet` | duckdb | Q4 | 311 | 360.0 | 441.2 | 482.6 | 15 |
| `large.parquet` | duckdb | Q5 | 1013 | 1059.0 | 1171.9 | 1207.2 | 15 |
| `large.parquet` | duckdb | Q6 | 0 | 0.0 | 1.3 | 1.9 | 15 |

**Cold vs steady.** `cold-process` includes a fresh Python + `duckdb` import for the
Python lanes; the steady-state headline removes it. **Cold-cache honesty:** the
container is unprivileged, so the OS page cache cannot be dropped; `cold-cache`
differs from `cold-process` only by the resident interpreter.

**Recorded plainly:** DuckDB wins the analytical axes (projection, predicate
execution, compressed pages, selective reads via metadata statistics). VOLE is
an archival field with typed observations, not a query engine; its predicate
counts are a decoded column plus a count in the driver. Only Q6 (exact closure)
is a byte-authority claim, and only VOLE and the source-retaining SQLite lane
can make it.

**Envelope (28.2).** SQLite gets typed columns, a covering predicate index, and a
tuned FTS5 external-content trigram index for the substring question; DuckDB reuses
SQL-level prepared statements and its Parquet zonemap/statistics pruning. Where a
strengthened competitor now wins more, it is recorded here plainly.

**Historical control.** pre-28.2 (21.14): `warm` was a single-shot session with no percentiles; `cold` was fresh-process-per-question; the SQLite lane had no typed columns or indexes. A historical control, not a like-for-like delta. Campaign `evidence/campaigns/2026-10-10-phase21-14-parquet-econ-bfe32a33`.

## Verdict

- VOLE exactness: **7/7**.
- Campaign: `evidence/campaigns/2026-10-12-phase28-2-parquet-econ-f7e0a961`.
- Profile: `release`; VOLE substrate `--profile runtime --packed`.
- Historical control (pre-28.2): `evidence/campaigns/2026-10-10-phase21-14-parquet-econ-bfe32a33`.
- Never run on the host: every command ran in the pinned, capped `analytical` container.
