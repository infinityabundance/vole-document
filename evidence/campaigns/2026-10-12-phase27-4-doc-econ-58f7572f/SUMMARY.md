# Phase 27.4 — legacy Word 97–2003 (.doc, MS-DOC) economic court — doc economic court

**Question.** Against two conventional comparators — a source-retaining store that keeps the raw bytes plus a conventional extraction, and a conventional decode-to-host-values load — on contract-equivalent terms, can VOLE answer the same question family (Q1–Q12) it can answer, while closing the original doc byte-exactly, and does it add value by **preserving representation** (source spans, exact spelling, attribute/quote/continuation markers, duplicate keys, member order)?

**Lane C is `comparator_absent` for doc.** There is no faithful pure-Python/stdlib decoder for a legacy .doc file in the pinned image, so rather than fabricate one, every Lane-C question is recorded as a typed `comparator_absent` decline (exactly as the HDF5/CFBF courts do). The head-to-head is therefore **limited to a single substantive comparator** — the source-retaining SQLite store (byte authority) — and the source-retaining store itself declines every *structural* question for the same reason. Read the paired ratios below as VOLE-vs-a-bytes-store, not as a full decode-vs-decode comparison.

Corpus: **3 fixtures**; lanes **vole, sqlite, conv**; questions **Q1–Q12**; bootstrap **10000 resamples, seed 2138**, cluster-resampled by fixture; tie band **+/-10%**.

VOLE lane: **release** profile (`target/release/vole-document`); substrate: **--profile runtime --packed**.

## Verdict

- **VOLE exactness (Q6): 7/7 byte-exact** (length + SHA-256 + `cmp`, after source + descriptor deletion in a fresh process).
- **COURT VERDICT: PASS**.

## Build + storage (per lane, per fixture)

Time columns are **microseconds (`us`)**.

| fixture | src B | vole build us | sqlite build us | conv build us | vole B | sqlite B | conv B |
|---|---:|---:|---:|---:|---:|---:|---:|
| doc_0table.doc | 3584 | 28919 | 37173 | 30201 | 11858 | 8257 | 125 |
| doc_1table.doc | 3584 | 28442 | 35901 | 26902 | 11858 | 8257 | 125 |
| doc_sections.doc | 3584 | 27732 | 34243 | 27252 | 11858 | 8257 | 127 |

`build us` is the best-of-N (min) of the retained repetitions.

## Paired ratios VOLE/comparator (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 3 | 0.792 | 0.793 | 0.778..0.810 | 0.778..0.810 | 3 | 0 | 0 | 0.793 |
| build | conv | 3 | 1.018 | 1.010 | 0.958..1.057 | 0.958..1.057 | 0 | 3 | 0 | 1.009 |
| storage | sqlite | 3 | 1.436 | 1.436 | 1.436..1.436 | 1.436..1.436 | 0 | 0 | 3 | 1.436 |
| storage | conv | 3 | 94.864 | 94.363 | 93.370..94.864 | 93.370..94.864 | 0 | 0 | 3 | 94.361 |
| cold | sqlite | 3 | 0.026 | 0.027 | 0.022..0.036 | 0.022..0.036 | 3 | 0 | 0 | 0.028 |
| cold | conv | 3 | 0.026 | 0.028 | 0.024..0.037 | 0.024..0.037 | 3 | 0 | 0 | 0.029 |
| warm | sqlite | 3 | 0.100 | 0.101 | 0.095..0.108 | 0.095..0.108 | 3 | 0 | 0 | 0.101 |
| warm | conv | 3 | 1.541 | 1.509 | 1.411..1.579 | 1.411..1.579 | 0 | 0 | 3 | 1.507 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` is reported separately and named as such. Each per-fixture lane cost is the **minimum over the retained repetitions** (best-of-N); storage is measured once after the last build. Every raw sample is kept in `raw/build.tsv`, `raw/storage.tsv`, `raw/cold.tsv`, `raw/warm.tsv`.

## Per-lane totals (sum over fixtures; times in microseconds `us`)

| lane | build us | storage B | cold us | warm us |
|---|---:|---:|---:|---:|
| vole | 85093 | 35574 | 28942 | 452 |
| sqlite | 107317 | 24771 | 1033615 | 4488 |
| conv | 84355 | 377 | 995803 | 300 |

## What each lane derives and what it declines

| Q | VOLE | source-retaining baseline | conventional load |
|---|---|---|---|
| Q1 | a piece's decoded text | no faithful extraction -> typed decline | comparator_absent |
| Q2 | a piece's exact source span | no source span -> typed decline | comparator_absent |
| Q3 | the recorded dialect (Word version) | no faithful extraction -> typed decline | comparator_absent |
| Q4 | the piece count | no faithful extraction -> typed decline | comparator_absent |
| Q5 | the FIB descriptor (version/table/ccps/spans) | no faithful extraction -> typed decline | comparator_absent |
| Q6 | `materialize --exact` (byte authority) | retained raw BLOB (byte authority) | no source bytes -> typed decline |
| Q7 | `doc-find` over piece texts (with spans) | no source span -> typed decline | comparator_absent |
| Q8 | a piece's exact `WordDocument` bytes | no source bytes -> typed decline | comparator_absent |
| Q9 | a CP section's text | no faithful extraction -> typed decline | comparator_absent |
| Q10 | a piece's compressed flag | no faithful extraction -> typed decline | comparator_absent |
| Q11 | the recorded format (doc) | stored format | comparator_absent |
| Q12 | a piece's exact stream byte span | no source span -> typed decline | comparator_absent |

## Scope (honest)

- **Self-authored deterministic corpus, NOT a real-world population.** The fixtures are generated by `tools/fixtures/make-doc.py` (Python stdlib only). Every claim is scoped to these files.
- **Only Q6 is a byte-authority claim.**
- **The conventional load is deliberately the weaker comparator.** It drops spans, exact spelling, duplicate keys, and member/attribute order; a span-preserving loader could in principle match VOLE on those, and no claim is made against one.
- **VOLE capability gaps are recorded, never papered over** — any question VOLE declines is shown as a `capability-gap`.
- **Lane C has NO faithful stdlib decoder for DOC**: legacy Word 97–2003 (.doc, MS-DOC) has no faithful Python-stdlib decoder, so Lane C declines every question as comparator_absent. Every Lane-C answer for that scope is recorded as a typed `comparator_absent` decline, never a silent or fabricated answer; VOLE capability is shown as `capability-gap` against it.
- **Cold is Python-startup-dominated.** Each comparator cold probe is a *fresh* `python3` process, so the cold ratio mostly measures interpreter startup, not query cost; the **warm** column (one resident session) is the honest per-query comparison. Both are reported.
- **The median and the ratio of sums can diverge** (the ratio of sums weights by absolute cost; the median treats each fixture equally). Both are reported **separately** and neither is presented as the estimator: the paired per-fixture ratio summarised by the median + geometric mean (with a fixed-seed cluster bootstrap 95% CI) is the stated estimator, and `ratio of sums` is named as such.
- **The single substantive comparator limits the comparison.** With Lane C `comparator_absent`, only the source-retaining SQLite store is a real counterpart, and it too stores the bytes rather than decoding them, so the axes on which VOLE is *decisively* cheaper or costlier on a faithful decode are not exercised here.
- **Nothing here is run on the host**; every command ran in a pinned container.

