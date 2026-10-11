# Phase 21.22 — GeoJSON economic court — geojson economic court

**Question.** Against two conventional comparators — a source-retaining store that keeps the raw bytes plus a conventional extraction, and a conventional decode-to-host-values load — on contract-equivalent terms, can VOLE answer the same question family (Q1–Q12) it can answer, while closing the original geojson byte-exactly, and does it add value by **preserving representation** (source spans, exact spelling, attribute/quote/continuation markers, duplicate keys, member order)?

Corpus: **7 fixtures**; lanes **vole, sqlite, conv**; questions **Q1–Q12**; bootstrap **10000 resamples, seed 2122**, cluster-resampled by fixture; tie band **+/-10%**.

VOLE lane: **release** profile (`target/release/vole-document`); substrate: **--profile runtime --packed**.

## Verdict

- **VOLE exactness (Q6): 11/11 byte-exact** (length + SHA-256 + `cmp`, after source + descriptor deletion in a fresh process).
- **COURT VERDICT: PASS**.

## Build + storage (per lane, per fixture)

Time columns are **microseconds (`us`)**.

| fixture | src B | vole build us | sqlite build us | conv build us | vole B | sqlite B | conv B |
|---|---:|---:|---:|---:|---:|---:|---:|
| points.geojson | 383 | 28380 | 30218 | 30121 | 5500 | 8239 | 8581 |
| feature.geojson | 118 | 28627 | 30489 | 30456 | 4960 | 8239 | 8357 |
| coords.geojson | 124 | 27463 | 27105 | 26593 | 4972 | 8239 | 8368 |
| foreign.geojson | 164 | 27601 | 27223 | 26930 | 5052 | 8239 | 8409 |
| geomcoll.geojson | 220 | 27785 | 27395 | 26716 | 5164 | 8239 | 8448 |
| dupkeys.geojson | 127 | 28222 | 29703 | 29943 | 4978 | 8239 | 8357 |
| large.geojson | 264262 | 35789 | 33191 | 43820 | 533267 | 577586 | 841855 |

`build us` is the best-of-N (min) of the retained repetitions.

## Paired ratios VOLE/comparator (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 7 | 1.013 | 0.991 | 0.939..1.014 | 0.960..1.030 | 0 | 7 | 0 | 0.993 |
| build | conv | 7 | 0.943 | 0.960 | 0.940..1.033 | 0.903..1.009 | 1 | 6 | 0 | 0.950 |
| storage | sqlite | 7 | 0.613 | 0.655 | 0.603..0.668 | 0.608..0.739 | 6 | 1 | 0 | 0.899 |
| storage | conv | 7 | 0.601 | 0.610 | 0.594..0.633 | 0.597..0.624 | 7 | 0 | 0 | 0.632 |
| cold | sqlite | 7 | 0.030 | 0.034 | 0.029..0.031 | 0.029..0.047 | 7 | 0 | 0 | 0.039 |
| cold | conv | 7 | 0.031 | 0.037 | 0.030..0.034 | 0.030..0.052 | 7 | 0 | 0 | 0.041 |
| warm | sqlite | 7 | 0.090 | 0.100 | 0.079..0.096 | 0.080..0.142 | 7 | 0 | 0 | 0.227 |
| warm | conv | 7 | 1.097 | 1.012 | 0.968..1.149 | 0.847..1.138 | 1 | 3 | 3 | 0.640 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` is reported separately and named as such. Each per-fixture lane cost is the **minimum over the retained repetitions** (best-of-N); storage is measured once after the last build. Every raw sample is kept in `raw/build.tsv`, `raw/storage.tsv`, `raw/cold.tsv`, `raw/warm.tsv`.

## Per-lane totals (sum over fixtures; times in microseconds `us`)

| lane | build us | storage B | cold us | warm us |
|---|---:|---:|---:|---:|
| vole | 203867 | 563893 | 74196 | 8973 |
| sqlite | 205324 | 627020 | 1915826 | 39514 |
| conv | 214579 | 892375 | 1799837 | 14025 |

## What each lane derives and what it declines

| Q | VOLE | source-retaining baseline | conventional load |
|---|---|---|---|
| Q1 | a `properties` scalar (kind + canonical value) | `json_extract` value (normalized) | host value |
| Q2 | a `properties` value's exact source span | no source span -> typed decline | no source span -> typed decline |
| Q3 | a geometry's `type` | `json_extract` type | host type |
| Q4 | the duplicate-key count in `properties` | `json_tree` duplicate enumeration | duplicate keys collapsed -> decline |
| Q5 | a feature descriptor (type + foreign member keys) | normalized feature view | host feature view |
| Q6 | `materialize --exact` (byte authority) | retained raw BLOB (byte authority) | no source bytes -> typed decline |
| Q7 | `geojson-find` over keys/strings (with spans) | scan over keys/strings (no spans) | host-structure walk (no spans) |
| Q8 | a property value's exact token bytes | re-serialized -> typed decline | host value -> typed decline |
| Q9 | each coordinate number's exact spelling | normalized numbers -> typed decline | host numbers -> typed decline |
| Q10 | a feature's foreign (non-core) member keys | `json_each` member keys | host member keys |
| Q11 | the root `type` (Feature/FeatureCollection) | `json_extract` type | host type |
| Q12 | a geometry's exact coordinate token bytes | no source token -> typed decline | no source token -> typed decline |

## Scope (honest)

- **Self-authored deterministic corpus, NOT a real-world population.** The fixtures are generated by `tools/fixtures/make-geojson.py` (Python stdlib only). Every claim is scoped to these files.
- **Only Q6 is a byte-authority claim.**
- **The conventional load is deliberately the weaker comparator.** It drops spans, exact spelling, duplicate keys, and member/attribute order; a span-preserving loader could in principle match VOLE on those, and no claim is made against one.
- **VOLE capability gaps are recorded, never papered over** — any question VOLE declines is shown as a `capability-gap`.
- **Nothing here is run on the host**; every command ran in a pinned container.

