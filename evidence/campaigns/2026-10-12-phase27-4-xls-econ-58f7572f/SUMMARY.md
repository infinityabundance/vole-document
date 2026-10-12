# Phase 27.4 — legacy Excel 97–2003 (.xls, BIFF8) economic court — xls economic court

**Question.** Against two conventional comparators — a source-retaining store that keeps the raw bytes plus a conventional extraction, and a conventional decode-to-host-values load — on contract-equivalent terms, can VOLE answer the same question family (Q1–Q12) it can answer, while closing the original xls byte-exactly, and does it add value by **preserving representation** (source spans, exact spelling, attribute/quote/continuation markers, duplicate keys, member order)?

**Lane C is `comparator_absent` for xls.** There is no faithful pure-Python/stdlib decoder for a legacy .xls file in the pinned image, so rather than fabricate one, every Lane-C question is recorded as a typed `comparator_absent` decline (exactly as the HDF5/CFBF courts do). The head-to-head is therefore **limited to a single substantive comparator** — the source-retaining SQLite store (byte authority) — and the source-retaining store itself declines every *structural* question for the same reason. Read the paired ratios below as VOLE-vs-a-bytes-store, not as a full decode-vs-decode comparison.

Corpus: **1 fixtures**; lanes **vole, sqlite, conv**; questions **Q1–Q12**; bootstrap **10000 resamples, seed 2139**, cluster-resampled by fixture; tie band **+/-10%**.

VOLE lane: **release** profile (`target/release/vole-document`); substrate: **--profile runtime --packed**.

## Verdict

- **VOLE exactness (Q6): 5/5 byte-exact** (length + SHA-256 + `cmp`, after source + descriptor deletion in a fresh process).
- **COURT VERDICT: PASS**.

## Build + storage (per lane, per fixture)

Time columns are **microseconds (`us`)**.

| fixture | src B | vole build us | sqlite build us | conv build us | vole B | sqlite B | conv B |
|---|---:|---:|---:|---:|---:|---:|---:|
| xls_valid.xls | 2560 | 31756 | 57951 | 36823 | 9810 | 8257 | 124 |

`build us` is the best-of-N (min) of the retained repetitions.

## Paired ratios VOLE/comparator (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 1 | 0.548 | 0.548 | 0.548..0.548 | 0.548..0.548 | 1 | 0 | 0 | 0.548 |
| build | conv | 1 | 0.862 | 0.862 | 0.862..0.862 | 0.862..0.862 | 1 | 0 | 0 | 0.862 |
| storage | sqlite | 1 | 1.188 | 1.188 | 1.188..1.188 | 1.188..1.188 | 0 | 0 | 1 | 1.188 |
| storage | conv | 1 | 79.113 | 79.113 | 79.113..79.113 | 79.113..79.113 | 0 | 0 | 1 | 79.113 |
| cold | sqlite | 1 | 0.025 | 0.025 | 0.025..0.025 | 0.025..0.025 | 1 | 0 | 0 | 0.025 |
| cold | conv | 1 | 0.027 | 0.027 | 0.027..0.027 | 0.027..0.027 | 1 | 0 | 0 | 0.027 |
| warm | sqlite | 1 | 0.111 | 0.111 | 0.111..0.111 | 0.111..0.111 | 1 | 0 | 0 | 0.111 |
| warm | conv | 1 | 1.642 | 1.642 | 1.642..1.642 | 1.642..1.642 | 0 | 0 | 1 | 1.642 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` is reported separately and named as such. Each per-fixture lane cost is the **minimum over the retained repetitions** (best-of-N); storage is measured once after the last build. Every raw sample is kept in `raw/build.tsv`, `raw/storage.tsv`, `raw/cold.tsv`, `raw/warm.tsv`.

## Per-lane totals (sum over fixtures; times in microseconds `us`)

| lane | build us | storage B | cold us | warm us |
|---|---:|---:|---:|---:|
| vole | 31756 | 9810 | 8840 | 179 |
| sqlite | 57951 | 8257 | 359047 | 1610 |
| conv | 36823 | 124 | 332871 | 109 |

## What each lane derives and what it declines

| Q | VOLE | source-retaining baseline | conventional load |
|---|---|---|---|
| Q1 | a sheet's name | no faithful extraction -> typed decline | comparator_absent |
| Q2 | a sheet's exact source span | no source span -> typed decline | comparator_absent |
| Q3 | the recorded dialect (BIFF version) | no faithful extraction -> typed decline | comparator_absent |
| Q4 | the sheet count | no faithful extraction -> typed decline | comparator_absent |
| Q5 | a sheet descriptor (name/pos/type/span) | no faithful extraction -> typed decline | comparator_absent |
| Q6 | `materialize --exact` (byte authority) | retained raw BLOB (byte authority) | no source bytes -> typed decline |
| Q7 | `xls-find` over sheet names/shared strings (with spans) | no source span -> typed decline | comparator_absent |
| Q8 | a cell's exact record bytes | no source bytes -> typed decline | comparator_absent |
| Q9 | a cell's stored value | no faithful extraction -> typed decline | comparator_absent |
| Q10 | a cell descriptor (row/col/kind/span) | no faithful extraction -> typed decline | comparator_absent |
| Q11 | the recorded format (xls) | stored format | comparator_absent |
| Q12 | a BIFF record descriptor (id/len/span) | no source span -> typed decline | comparator_absent |

## Scope (honest)

- **Self-authored deterministic corpus, NOT a real-world population.** The fixtures are generated by `tools/fixtures/make-xls.py` (Python stdlib only). Every claim is scoped to these files.
- **Only Q6 is a byte-authority claim.**
- **The conventional load is deliberately the weaker comparator.** It drops spans, exact spelling, duplicate keys, and member/attribute order; a span-preserving loader could in principle match VOLE on those, and no claim is made against one.
- **VOLE capability gaps are recorded, never papered over** — any question VOLE declines is shown as a `capability-gap`.
- **Lane C has NO faithful stdlib decoder for XLS**: legacy Excel 97–2003 (.xls, BIFF8) has no faithful Python-stdlib decoder, so Lane C declines every question as comparator_absent. Every Lane-C answer for that scope is recorded as a typed `comparator_absent` decline, never a silent or fabricated answer; VOLE capability is shown as `capability-gap` against it.
- **Cold is Python-startup-dominated.** Each comparator cold probe is a *fresh* `python3` process, so the cold ratio mostly measures interpreter startup, not query cost; the **warm** column (one resident session) is the honest per-query comparison. Both are reported.
- **The median and the ratio of sums can diverge** (the ratio of sums weights by absolute cost; the median treats each fixture equally). Both are reported **separately** and neither is presented as the estimator: the paired per-fixture ratio summarised by the median + geometric mean (with a fixed-seed cluster bootstrap 95% CI) is the stated estimator, and `ratio of sums` is named as such.
- **The single substantive comparator limits the comparison.** With Lane C `comparator_absent`, only the source-retaining SQLite store is a real counterpart, and it too stores the bytes rather than decoding them, so the axes on which VOLE is *decisively* cheaper or costlier on a faithful decode are not exercised here.
- **Single-fixture corpus (n=1):** the fixed-seed cluster bootstrap is **degenerate** (its 95% CI collapses to the point estimate), so the paired ratio and every derived cost are point estimates over one file, not a population estimate.
- **Nothing here is run on the host**; every command ran in a pinned container.

