# Phase 21.20 — config economic court — config economic court

**Question.** Against two conventional comparators — a source-retaining store that keeps the raw bytes plus a conventional extraction, and a conventional decode-to-host-values load — on contract-equivalent terms, can VOLE answer the same question family (Q1–Q12) it can answer, while closing the original config byte-exactly, and does it add value by **preserving representation** (source spans, exact spelling, attribute/quote/continuation markers, duplicate keys, member order)?

Corpus: **9 fixtures**; lanes **vole, sqlite, conv**; questions **Q1–Q12**; bootstrap **10000 resamples, seed 2120**, cluster-resampled by fixture; tie band **+/-10%**.

VOLE lane: **release** profile (`target/release/vole-document`); substrate: **--profile runtime --packed**.

## Verdict

- **VOLE exactness (Q6): 14/14 byte-exact** (length + SHA-256 + `cmp`, after source + descriptor deletion in a fresh process).
- **COURT VERDICT: PASS**.

## Build + storage (per lane, per fixture)

Time columns are **microseconds (`us`)**.

| fixture | src B | vole build us | sqlite build us | conv build us | vole B | sqlite B | conv B |
|---|---:|---:|---:|---:|---:|---:|---:|
| ini-basic.ini | 71 | 28128 | 27785 | 26595 | 4851 | 8256 | 9100 |
| ini-comments.ini | 69 | 28290 | 29635 | 29677 | 4847 | 8256 | 9020 |
| ini-spaces.ini | 57 | 28120 | 29579 | 28809 | 4823 | 8256 | 8894 |
| env-basic.env | 38 | 28382 | 29683 | 29340 | 4785 | 8256 | 8864 |
| env-quotes.env | 55 | 28437 | 29854 | 28722 | 4819 | 8256 | 8887 |
| props-basic.properties | 51 | 27860 | 28020 | 26717 | 4818 | 8263 | 8928 |
| props-colon.properties | 35 | 28327 | 30273 | 29242 | 4786 | 8263 | 8998 |
| dup.ini | 34 | 28265 | 41226 | 38999 | 4777 | 8256 | 9045 |
| large.ini | 262195 | 34376 | 54645 | 77727 | 529110 | 1744964 | 3221188 |

`build us` is the best-of-N (min) of the retained repetitions.

## Paired ratios VOLE/comparator (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 9 | 0.953 | 0.886 | 0.686..0.994 | 0.787..0.971 | 2 | 7 | 0 | 0.865 |
| build | conv | 9 | 0.969 | 0.876 | 0.725..1.043 | 0.727..1.002 | 2 | 7 | 0 | 0.824 |
| storage | sqlite | 9 | 0.583 | 0.542 | 0.579..0.587 | 0.468..0.584 | 9 | 0 | 0 | 0.313 |
| storage | conv | 9 | 0.537 | 0.471 | 0.528..0.542 | 0.361..0.539 | 9 | 0 | 0 | 0.172 |
| cold | sqlite | 9 | 0.028 | 0.030 | 0.027..0.029 | 0.027..0.036 | 9 | 0 | 0 | 0.032 |
| cold | conv | 9 | 0.029 | 0.030 | 0.026..0.030 | 0.027..0.037 | 9 | 0 | 0 | 0.032 |
| warm | sqlite | 9 | 0.083 | 0.079 | 0.068..0.088 | 0.072..0.086 | 9 | 0 | 0 | 0.095 |
| warm | conv | 9 | 0.082 | 0.081 | 0.071..0.088 | 0.074..0.089 | 9 | 0 | 0 | 0.103 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` is reported separately and named as such. Each per-fixture lane cost is the **minimum over the retained repetitions** (best-of-N); storage is measured once after the last build. Every raw sample is kept in `raw/build.tsv`, `raw/storage.tsv`, `raw/cold.tsv`, `raw/warm.tsv`.

## Per-lane totals (sum over fixtures; times in microseconds `us`)

| lane | build us | storage B | cold us | warm us |
|---|---:|---:|---:|---:|
| vole | 260185 | 567616 | 80516 | 6021 |
| sqlite | 300700 | 1811026 | 2507894 | 63064 |
| conv | 315828 | 3292924 | 2491296 | 58601 |

## What each lane derives and what it declines

| Q | VOLE | source-retaining baseline | conventional load |
|---|---|---|---|
| Q1 | a decoded entry value | extracted `key=value` (value) | host value |
| Q2 | an entry's exact source span | no source span -> typed decline | no source span -> typed decline |
| Q3 | a line's kind (section/entry/comment/blank) | extracted line kind | configparser has no line kinds -> decline |
| Q4 | the duplicate-key count for an entry (same_key_entries) | enumerated entries (duplicates preserved) | duplicates collapsed -> decline |
| Q5 | a section header (name + header count) | section names (duplicates collapsed by name) | section names |
| Q6 | `materialize --exact` (byte authority) | retained raw BLOB (byte authority) | no source bytes -> typed decline |
| Q7 | `config-find` over decoded keys/values (with spans) | scan over extracted keys/values (no spans) | scan over host values (no spans) |
| Q8 | the exact raw line token bytes | no source token -> typed decline | no source token -> typed decline |
| Q9 | a comment line's kind + marker | extracted comment markers | comments dropped -> typed decline |
| Q10 | key/separator exact spans + marker | no source span -> typed decline | no source span -> typed decline |
| Q11 | the recorded dialect (ini/env/properties) | stored dialect | dialect not recorded -> typed decline |
| Q12 | quoting/export/continuation spelling flags | no source spelling -> typed decline | no source spelling -> typed decline |

## Scope (honest)

- **Self-authored deterministic corpus, NOT a real-world population.** The fixtures are generated by `tools/fixtures/make-config.py` (Python stdlib only). Every claim is scoped to these files.
- **Only Q6 is a byte-authority claim.**
- **The conventional load is deliberately the weaker comparator.** It drops spans, exact spelling, duplicate keys, and member/attribute order; a span-preserving loader could in principle match VOLE on those, and no claim is made against one.
- **VOLE capability gaps are recorded, never papered over** — any question VOLE declines is shown as a `capability-gap`.
- **Nothing here is run on the host**; every command ran in a pinned container.

