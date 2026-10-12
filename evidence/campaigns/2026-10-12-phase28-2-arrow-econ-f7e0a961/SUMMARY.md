# Phase 28.2 — Arrow IPC (analytical Wave 2) economic court — competitor envelope

**Question.** On a self-authored Arrow IPC corpus, how do VOLE (an archival
field with typed observations), a source-retaining SQLite baseline, a DuckDB
analytical baseline over the **real Arrow data**, and a pyarrow lane over the
Arrow IPC file itself answer six contract questions, and does VOLE still close
**exactly**?

**Headline (recorded plainly).** pyarrow — the reference Arrow reader, opening
the `.arrow` file directly — and DuckDB over the **real Arrow data** (Phase 28.2:

the Arrow C data interface, not a Parquet projection) both win the analytical
computation axes (columnar projection, predicate execution), while VOLE
materializes a bounded observation and counts in the driver. VOLE's unique claims
are exactness (Q6, byte-authority) and the exact buffer span (Q3) from the archival
metadata; neither DuckDB nor pyarrow is a source-retaining store (Q6 declines
`not-native`) and Arrow IPC stores no per-column statistics or source span (Q3
declines `not-retained`).

**Measurement (28.2).** The headline is **in-process steady-state** (warm
connection/state; per-question median), reported separately from **cold-process**
(fresh OS process per question) and **cold-cache** (fresh connection/reader, warm
interpreter). Percentiles are in the matrix. The pre-28.2 numbers are kept as an
explicit historical control.

## Matrix


Self-authored corpus; only **Q6 (exact closure)** is a byte-authority claim.
Ratios are VOLE / comparator; **< 1 favours VOLE**. Wall time is microseconds.
The **headline is steady-state**; cold-process and cold-cache are separate.

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
| Q3 | a column's exact buffer span + min/max statistics | 0 / 3 / 0 / 0 | 0 / 3 / 0 / 0 | 0 / 3 / 0 / 0 |
| Q4 | the record-batch count | 0 / 3 / 0 / 0 | 3 / 0 / 0 / 0 | 3 / 0 / 0 / 0 |
| Q5 | a lexical find (rows in a string column containing S) | 3 / 0 / 0 / 0 | 3 / 0 / 0 / 0 | 3 / 0 / 0 / 0 |
| Q6 | `materialize --exact` (byte-authority) | 3 / 0 / 0 / 0 | 0 / 3 / 0 / 0 | 0 / 3 / 0 / 0 |

## Capability accounting (per lane, over 6 questions x 3 fixtures)

A lane that **declines** what another answers has not won an equal-contract
comparison; every typed decline is counted, never hidden.

| lane | answered | declined | decline codes | vs VOLE equal / gap / both-decline / mismatch |
|---|---:|---:|---|---|
| vole | 18 | 0 | - | (reference) |
| sqlite | 12 | 6 | not-retained=6 | 12 / 6 / 0 / 0 |
| duckdb | 12 | 6 | not-native=3, not-retained=3 | 12 / 6 / 0 / 0 |
| pyarrow | 12 | 6 | not-native=3, not-retained=3 | 12 / 6 / 0 / 0 |

## Paired ratios VOLE/other (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 3 | 0.425 | 0.422 | 0.391..0.454 | 0.391..0.454 | 3 | 0 | 0 | 0.413 |
| build | duckdb | 3 | 0.472 | 0.564 | 0.443..0.858 | 0.443..0.858 | 3 | 0 | 0 | 0.596 |
| build | pyarrow | 3 | 0.540 | 0.649 | 0.480..1.053 | 0.480..1.053 | 2 | 1 | 0 | 0.685 |
| storage | sqlite | 3 | 0.358 | 0.378 | 0.287..0.524 | 0.287..0.524 | 3 | 0 | 0 | 0.519 |
| storage | duckdb | 3 | 2.148 | 2.248 | 2.002..2.641 | 2.002..2.641 | 0 | 0 | 3 | 2.006 |
| storage | pyarrow | 3 | 2.156 | 2.263 | 2.002..2.685 | 2.002..2.685 | 0 | 0 | 3 | 2.007 |
| steady | sqlite | 3 | 11.603 | 11.595 | 3.769..35.643 | 3.769..35.643 | 0 | 0 | 3 | 4.364 |
| steady | duckdb | 3 | 2.361 | 3.167 | 2.129..6.318 | 2.129..6.318 | 0 | 0 | 3 | 5.341 |
| steady | pyarrow | 3 | 27.372 | 33.088 | 14.476..91.421 | 14.476..91.421 | 0 | 0 | 3 | 15.464 |
| cold-process | sqlite | 3 | 0.027 | 0.050 | 0.022..0.205 | 0.022..0.205 | 3 | 0 | 0 | 0.088 |
| cold-process | duckdb | 3 | 0.007 | 0.015 | 0.005..0.097 | 0.005..0.097 | 3 | 0 | 0 | 0.036 |
| cold-process | pyarrow | 3 | 0.013 | 0.028 | 0.009..0.174 | 0.009..0.174 | 3 | 0 | 0 | 0.065 |
| cold-cache | sqlite | 3 | 29.104 | 17.886 | 4.685..41.969 | 4.685..41.969 | 0 | 0 | 3 | 6.793 |
| cold-cache | duckdb | 3 | 4.615 | 4.358 | 3.241..5.531 | 3.241..5.531 | 0 | 0 | 3 | 4.849 |
| cold-cache | pyarrow | 3 | 52.637 | 38.951 | 17.997..62.383 | 17.997..62.383 | 0 | 0 | 3 | 23.525 |

## Steady-state percentiles per question (us)

| fixture | lane | Q | min | p50 | p95 | p99 | n |
|---|---|---|---:|---:|---:|---:|---:|
| `e_small.arrow` | vole | Q1 | 407 | 448.0 | 530.4 | 534.9 | 15 |
| `e_small.arrow` | vole | Q2 | 413 | 462.0 | 541.2 | 554.6 | 15 |
| `e_small.arrow` | vole | Q3 | 1165 | 1376.0 | 1467.0 | 1467.0 | 15 |
| `e_small.arrow` | vole | Q4 | 363 | 406.0 | 492.0 | 525.6 | 15 |
| `e_small.arrow` | vole | Q5 | 403 | 421.0 | 470.4 | 474.9 | 15 |
| `e_small.arrow` | vole | Q6 | 2719 | 2768.0 | 2992.2 | 3285.6 | 15 |
| `e_small.arrow` | sqlite | Q1 | 40 | 40.0 | 43.0 | 43.0 | 15 |
| `e_small.arrow` | sqlite | Q2 | 3 | 3.0 | 3.0 | 3.0 | 15 |
| `e_small.arrow` | sqlite | Q3 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `e_small.arrow` | sqlite | Q4 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `e_small.arrow` | sqlite | Q5 | 66 | 67.0 | 68.6 | 69.7 | 15 |
| `e_small.arrow` | sqlite | Q6 | 5 | 5.0 | 6.3 | 6.9 | 15 |
| `e_small.arrow` | duckdb | Q1 | 191 | 211.0 | 244.4 | 260.1 | 15 |
| `e_small.arrow` | duckdb | Q2 | 269 | 353.0 | 424.9 | 426.6 | 15 |
| `e_small.arrow` | duckdb | Q3 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `e_small.arrow` | duckdb | Q4 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `e_small.arrow` | duckdb | Q5 | 194 | 252.0 | 305.3 | 305.9 | 15 |
| `e_small.arrow` | duckdb | Q6 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `e_small.arrow` | pyarrow | Q1 | 7 | 8.0 | 9.0 | 9.0 | 15 |
| `e_small.arrow` | pyarrow | Q2 | 5 | 5.0 | 6.6 | 7.7 | 15 |
| `e_small.arrow` | pyarrow | Q3 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `e_small.arrow` | pyarrow | Q4 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `e_small.arrow` | pyarrow | Q5 | 6 | 6.0 | 9.0 | 9.0 | 15 |
| `e_small.arrow` | pyarrow | Q6 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `e_multi.arrow` | vole | Q1 | 591 | 625.0 | 756.9 | 809.0 | 15 |
| `e_multi.arrow` | vole | Q2 | 627 | 673.0 | 734.1 | 777.2 | 15 |
| `e_multi.arrow` | vole | Q3 | 1591 | 1728.0 | 1878.7 | 1906.1 | 15 |
| `e_multi.arrow` | vole | Q4 | 415 | 448.0 | 507.6 | 508.7 | 15 |
| `e_multi.arrow` | vole | Q5 | 579 | 608.0 | 672.8 | 687.4 | 15 |
| `e_multi.arrow` | vole | Q6 | 2826 | 2863.0 | 2949.6 | 2961.9 | 15 |
| `e_multi.arrow` | sqlite | Q1 | 371 | 373.0 | 376.0 | 376.0 | 15 |
| `e_multi.arrow` | sqlite | Q2 | 5 | 5.0 | 5.0 | 5.0 | 15 |
| `e_multi.arrow` | sqlite | Q3 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `e_multi.arrow` | sqlite | Q4 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `e_multi.arrow` | sqlite | Q5 | 17 | 18.0 | 18.6 | 19.7 | 15 |
| `e_multi.arrow` | sqlite | Q6 | 14 | 15.0 | 15.0 | 15.0 | 15 |
| `e_multi.arrow` | duckdb | Q1 | 345 | 371.0 | 453.6 | 493.9 | 15 |
| `e_multi.arrow` | duckdb | Q2 | 320 | 377.0 | 449.9 | 451.6 | 15 |
| `e_multi.arrow` | duckdb | Q3 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `e_multi.arrow` | duckdb | Q4 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `e_multi.arrow` | duckdb | Q5 | 199 | 249.0 | 300.2 | 308.0 | 15 |
| `e_multi.arrow` | duckdb | Q6 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `e_multi.arrow` | pyarrow | Q1 | 64 | 66.0 | 102.9 | 104.6 | 15 |
| `e_multi.arrow` | pyarrow | Q2 | 5 | 6.0 | 7.6 | 8.7 | 15 |
| `e_multi.arrow` | pyarrow | Q3 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `e_multi.arrow` | pyarrow | Q4 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `e_multi.arrow` | pyarrow | Q5 | 14 | 14.0 | 15.0 | 15.0 | 15 |
| `e_multi.arrow` | pyarrow | Q6 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `e_large.arrow` | vole | Q1 | 9638 | 10101.0 | 10695.6 | 10719.1 | 15 |
| `e_large.arrow` | vole | Q2 | 9895 | 10340.0 | 10764.5 | 10907.3 | 15 |
| `e_large.arrow` | vole | Q3 | 21844 | 22638.0 | 23738.6 | 23823.7 | 15 |
| `e_large.arrow` | vole | Q4 | 4595 | 4813.0 | 5144.1 | 5456.0 | 15 |
| `e_large.arrow` | vole | Q5 | 10361 | 10922.0 | 11394.3 | 11758.9 | 15 |
| `e_large.arrow` | vole | Q6 | 9981 | 10278.0 | 10856.8 | 11448.2 | 15 |
| `e_large.arrow` | sqlite | Q1 | 8656 | 8674.0 | 8696.8 | 8700.2 | 15 |
| `e_large.arrow` | sqlite | Q2 | 231 | 232.0 | 234.0 | 234.0 | 15 |
| `e_large.arrow` | sqlite | Q3 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `e_large.arrow` | sqlite | Q4 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `e_large.arrow` | sqlite | Q5 | 905 | 908.0 | 911.8 | 915.2 | 15 |
| `e_large.arrow` | sqlite | Q6 | 1229 | 1233.0 | 1290.0 | 1362.8 | 15 |
| `e_large.arrow` | duckdb | Q1 | 4748 | 4859.0 | 5322.6 | 5413.3 | 15 |
| `e_large.arrow` | duckdb | Q2 | 417 | 466.0 | 653.3 | 687.5 | 15 |
| `e_large.arrow` | duckdb | Q3 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `e_large.arrow` | duckdb | Q4 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `e_large.arrow` | duckdb | Q5 | 352 | 401.0 | 426.5 | 429.3 | 15 |
| `e_large.arrow` | duckdb | Q6 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `e_large.arrow` | pyarrow | Q1 | 1652 | 1662.0 | 1668.5 | 1671.3 | 15 |
| `e_large.arrow` | pyarrow | Q2 | 14 | 15.0 | 16.6 | 17.7 | 15 |
| `e_large.arrow` | pyarrow | Q3 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `e_large.arrow` | pyarrow | Q4 | 0 | 0.0 | 1.0 | 1.0 | 15 |
| `e_large.arrow` | pyarrow | Q5 | 816 | 822.0 | 834.5 | 842.9 | 15 |
| `e_large.arrow` | pyarrow | Q6 | 0 | 0.0 | 1.0 | 1.0 | 15 |

A ratio < 1 favours VOLE. The **headline is `steady`** (in-process, warm
connection/state; per-question median). Every question-level metric (steady,
cold-process, cold-cache) is summed per fixture over the **common answer set**:
only the questions BOTH lanes answer, so a lane is never charged for work its
comparator declines (and vice versa). `build`/`storage` are whole-artifact and not
question-conditioned. `cold-process` is a fresh OS process per question (Python
interpreter start-up included for the Python lanes; a fresh CLI process for VOLE).
`cold-cache` is a fresh connection/reader within one warm-interpreter process. The
estimator is the **paired per-fixture ratio**, summarised by the median and geometric
mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` is reported
separately. With only 3 fixture clusters the bootstrap is coarse and is stated
as such.

**Cold vs steady.** In 28.2 the Python lanes' cold-process samples still include
interpreter + DuckDB/pyarrow import (tens of milliseconds), which is exactly what
the review said must not decide the headline; the steady-state column removes that.
**Cold-cache honesty:** the container is unprivileged, so the OS page cache cannot
be dropped; `cold-cache` differs from `cold-process` only by the resident
interpreter, and that limitation is recorded rather than hidden.

**Envelope (28.2).** DuckDB now reads the **real Arrow data** through the Arrow C
data interface (`con.register` of the `pyarrow.ipc` table), not a Parquet
projection. SQLite gets typed columns, a covering predicate index, and a tuned
FTS5 external-content **trigram** index for the substring question. pyarrow reads
the Arrow IPC file directly with column projection and a persistent reader. VOLE
is unchanged (an archival field with typed observations, not a query engine).
Where a strengthened competitor now wins more, it is recorded here plainly — on the
steady-state analytical axes all three competitors beat VOLE.

**Architectural asymmetry (recorded, not hidden).** VOLE's unit of execution is a
fresh CLI process per question, so its steady-state is repeated warm-cache CLI
invocations and it cannot hold an in-process warm engine state; the SQLite/DuckDB/
pyarrow steady-state is a persistent in-process connection/reader. The steady-state
comparison therefore measures VOLE's real deployed shape against the competitors'
warm engines.

**Historical control.** pre-28.2 (26.6): DuckDB read a Parquet projection; `warm` was a single-shot session with no percentiles and summed ALL six questions (not the equal-contract common set); `cold` was fresh-process-per-question; pyarrow re-opened and `read_all()`-ed the file on every query. Its VOLE/pyarrow warm median was 8.626x (geomean 10.475x) over the unequal all-six-question sum, so it is a historical control, not a like-for-like delta. Campaign `evidence/campaigns/2026-10-12-phase21-16-arrow-econ-efb51887`.

## Verdict

- VOLE exactness: **3/3**.
- Campaign: `evidence/campaigns/2026-10-12-phase28-2-arrow-econ-f7e0a961`.
- Profile: `release`; VOLE substrate `--profile runtime --packed`.
- Historical control (pre-28.2): `evidence/campaigns/2026-10-12-phase21-16-arrow-econ-efb51887`.
- Never run on the host: every command ran in the pinned, capped `analytical` container.
