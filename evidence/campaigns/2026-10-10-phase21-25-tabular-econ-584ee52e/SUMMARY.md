# Phase 21.25 — PSV + fixed-width economic court — tabular economic court

**Question.** Against three conventional comparators — a source-retaining store that keeps the raw bytes plus a conventional extraction, a conventional decode-to-host-values load, and the mandatory **DuckDB/Parquet** analytical baseline (ADR-0059) — on contract-equivalent terms, can VOLE answer the same question family (Q1–Q12) it can answer, while closing the original tabular byte-exactly, and does it add value by **preserving representation** (source spans, exact spelling, quoting markers, the recorded dialect and the recovered column layout)?

Corpus: **7 fixtures**; lanes **vole, sqlite, conv, duckdb**; questions **Q1–Q12**; bootstrap **10000 resamples, seed 2125**, cluster-resampled by fixture; tie band **+/-10%**.

VOLE lane: **release** profile (`target/release/vole-document`); substrate: **--profile runtime --packed**.

## Verdict

- **VOLE exactness (Q6): 11/11 byte-exact** (length + SHA-256 + `cmp`, after source + descriptor deletion in a fresh process).
- **COURT VERDICT: PASS**.

## Build + storage (per lane, per fixture)

Time columns are **microseconds (`us`)**.

| fixture | src B | vole build us | sqlite build us | conv build us | duckdb build us | vole B | sqlite B | conv B | duckdb B |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| psv-basic.psv | 25 | 28772 | 39693 | 30995 | 86433 | 4770 | 8256 | 207 | 724 |
| psv-quotes.psv | 30 | 49472 | 51800 | 47111 | 94727 | 4780 | 8256 | 219 | 729 |
| fw-basic.fw | 33 | 28781 | 32162 | 32081 | 92644 | 4818 | 8262 | 221 | 749 |
| fw-three.fw | 42 | 29766 | 39149 | 23855 | 96294 | 4836 | 8262 | 252 | 867 |
| fw-crlf.fw | 36 | 28116 | 39752 | 32351 | 93844 | 4826 | 8262 | 221 | 749 |
| large.psv | 262148 | 36429 | 48083 | 53538 | 101863 | 529027 | 942148 | 672182 | 49804 |
| large.fw | 262164 | 55482 | 41246 | 37442 | 85658 | 529091 | 794698 | 527233 | 42712 |

`build us` is the best-of-N (min) of the retained repetitions.

## Paired ratios VOLE/comparator (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 7 | 0.760 | 0.857 | 0.725..0.955 | 0.749..1.019 | 5 | 1 | 1 | 0.880 |
| build | conv | 7 | 0.928 | 0.994 | 0.869..1.248 | 0.836..1.187 | 3 | 2 | 2 | 0.998 |
| build | duckdb | 7 | 0.333 | 0.381 | 0.309..0.522 | 0.316..0.475 | 7 | 0 | 0 | 0.394 |
| storage | sqlite | 7 | 0.583 | 0.590 | 0.578..0.585 | 0.574..0.616 | 7 | 0 | 0 | 0.609 |
| storage | conv | 7 | 21.801 | 8.652 | 1.004..21.837 | 3.234..21.821 | 1 | 1 | 5 | 0.901 |
| storage | duckdb | 7 | 6.557 | 7.483 | 6.433..10.622 | 6.215..9.250 | 0 | 0 | 7 | 11.233 |
| cold | sqlite | 7 | 0.027 | 0.033 | 0.025..0.054 | 0.025..0.046 | 7 | 0 | 0 | 0.037 |
| cold | conv | 7 | 0.028 | 0.034 | 0.026..0.061 | 0.026..0.048 | 7 | 0 | 0 | 0.038 |
| cold | duckdb | 7 | 0.010 | 0.013 | 0.009..0.022 | 0.009..0.018 | 7 | 0 | 0 | 0.014 |
| warm | sqlite | 7 | 0.074 | 0.088 | 0.067..0.133 | 0.059..0.143 | 7 | 0 | 0 | 0.197 |
| warm | conv | 7 | 0.776 | 0.591 | 0.347..0.973 | 0.353..0.881 | 5 | 2 | 0 | 0.276 |
| warm | duckdb | 7 | 0.001 | 0.003 | 0.001..0.017 | 0.001..0.009 | 7 | 0 | 0 | 0.012 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` is reported separately and named as such. Each per-fixture lane cost is the **minimum over the retained repetitions** (best-of-N); storage is measured once after the last build. Every raw sample is kept in `raw/build.tsv`, `raw/storage.tsv`, `raw/cold.tsv`, `raw/warm.tsv`.

## Per-lane totals (sum over fixtures; times in microseconds `us`)

| lane | build us | storage B | cold us | warm us |
|---|---:|---:|---:|---:|
| vole | 256818 | 1082148 | 88849 | 10230 |
| sqlite | 291885 | 1778144 | 2418632 | 51798 |
| conv | 257373 | 1200535 | 2315734 | 37028 |
| duckdb | 651463 | 96334 | 6332307 | 832400 |

## What each lane derives and what it declines

| Q | VOLE | source-retaining baseline | conventional load | DuckDB/Parquet |
|---|---|---|---|---|
| Q1 | a decoded cell's text | extracted cell text | host cell text | projected cell text (SQL) |
| Q2 | a cell's exact source span | no source span -> typed decline | no source span -> typed decline | no source span -> typed decline |
| Q3 | a record's column count | extracted column count | host column count | projected column count |
| Q4 | the table's record count | extracted row count | host row count | projected row count |
| Q5 | a record descriptor (ordered cell texts) | extracted record | host record | projected record (SQL projection) |
| Q6 | `materialize --exact` (byte authority) | retained raw BLOB (byte authority) | no source bytes -> typed decline | not-native (no original bytes) -> typed decline |
| Q7 | `tabular-find` over cell text (with spans) | scan over extracted cells (no spans) | scan over host cells (no spans) | scan over projected cells (no spans) |
| Q8 | a cell's exact padded/raw bytes | re-decoded -> typed decline | host value -> typed decline | no source bytes -> typed decline |
| Q9 | a record's exact content bytes | no source bytes -> typed decline | host value -> typed decline | no source bytes -> typed decline |
| Q10 | the header row's cell names (ordered) | extracted header names | host header names | projected header names |
| Q11 | the recorded dialect (pipe vs fixedwidth) | stored dialect | dialect not recorded -> typed decline | dialect not recorded -> typed decline |
| Q12 | the recovered column layout / per-cell quoting spelling | no layout/spelling -> typed decline | no layout/spelling -> typed decline | no layout/spelling -> typed decline |

## Scope (honest)

- **Self-authored deterministic corpus, NOT a real-world population.** The fixtures are generated by `tools/fixtures/make-tabular.py` (Python stdlib only). Every claim is scoped to these files.
- **Only Q6 is a byte-authority claim.**
- **The conventional load is deliberately the weaker comparator.** It drops spans, exact spelling, duplicate keys, and member/attribute order; a span-preserving loader could in principle match VOLE on those, and no claim is made against one.
- **VOLE capability gaps are recorded, never papered over** — any question VOLE declines is shown as a `capability-gap`.
- **The DuckDB/Parquet lane is the mandatory ADR-0059 analytical comparator**, carried alongside SQLite. It answers the columnar/tabular questions (Q1/Q3/Q4/Q5/Q7/Q10) from a Parquet projection; exact source bytes/spans (Q2/Q8/Q9), the recorded dialect (Q11) and the recovered column layout/quoting spelling (Q12) are typed declines, never silent answers. Where it wins or loses an axis, the paired-ratio table above records it.
- **Nothing here is run on the host**; every command ran in a pinned container.

