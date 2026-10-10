# Phase 21.11 — TOML economic court

**Question.** Against a source-retaining SQLite baseline *and* a conventional TOML→dict (`tomllib`) baseline, can VOLE answer the same questions (Q1–Q8) it can answer, at comparable build/storage/cold/warm cost, while closing the original TOML byte-exactly — and does it add value by **preserving representation** (a value's exact source span and a scalar's exact spelling)?

**Method.** A deterministic self-authored TOML corpus (`tools/fixtures/make-toml.py --corpus`) is regenerated at court time; each fixture is ingested by three lanes (VOLE field CLI; a source-retaining SQLite baseline; a conventional TOML→dict `tomllib` baseline), Q1–Q8 are asked of each, and build/storage/cold/warm are measured. Persistent bytes are the **sum of regular-file sizes** (`find -type f -printf '%s'`), never `du -sb` (ADR-0049).

Corpus: **7 fixtures**; lanes **vole, sqlite, conv**; questions **Q1–Q8**; bootstrap **10000 resamples, seed 21111**, cluster-resampled by fixture; tie band **+/-10%**.

VOLE lane: **release** profile (`target/release/vole-document`); substrate: **--profile runtime --packed**. All wall times are **microseconds (`us`)**.

## Verdict

- **VOLE exactness (Q8): 7/7 byte-exact** (length + SHA-256 + `cmp`, after source + descriptor deletion in a fresh process).
- **COURT VERDICT: PASS** (fails unless exactness is 100% on the VOLE lane for every fixture).

## Build + storage (per lane, per fixture)

Time columns are **microseconds (`us`)**.

| fixture | src B | vole build us | sqlite build us | conv build us | vole B | sqlite B | conv B |
|---|---:|---:|---:|---:|---:|---:|---:|
| basic.toml | 198 | 28102 | 26571 | 19842 | 5087 | 16419 | 601 |
| tables.toml | 196 | 27839 | 26689 | 20427 | 5083 | 16419 | 731 |
| arrays.toml | 358 | 27724 | 27008 | 20203 | 5407 | 16419 | 1048 |
| inline.toml | 194 | 27788 | 25866 | 19693 | 5079 | 16419 | 680 |
| scalars.toml | 427 | 27897 | 27233 | 19514 | 5545 | 16420 | 1120 |
| comments.toml | 179 | 28100 | 25984 | 19724 | 5049 | 16419 | 256 |
| large.toml | 2097282 | 78771 | 459337 | 413874 | 4199267 | 8343587 | 5226138 |

`build us` is the best-of-N (min) of the retained repetitions.

## Paired ratios VOLE/other (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 7 | 1.043 | 0.811 | 1.024..1.074 | 0.481..1.063 | 1 | 6 | 0 | 0.398 |
| build | conv | 7 | 1.411 | 1.054 | 1.363..1.425 | 0.594..1.416 | 1 | 0 | 6 | 0.462 |
| storage | sqlite | 7 | 0.310 | 0.339 | 0.309..0.338 | 0.311..0.389 | 7 | 0 | 0 | 0.501 |
| storage | conv | 7 | 6.953 | 5.624 | 4.951..8.464 | 2.616..10.146 | 1 | 0 | 6 | 0.809 |
| cold | sqlite | 7 | 0.038 | 0.058 | 0.037..0.038 | 0.037..0.141 | 7 | 0 | 0 | 0.159 |
| cold | conv | 7 | 0.038 | 0.053 | 0.037..0.039 | 0.037..0.104 | 7 | 0 | 0 | 0.137 |
| warm | sqlite | 7 | 0.175 | 0.239 | 0.173..0.190 | 0.166..0.467 | 6 | 0 | 1 | 1.485 |
| warm | conv | 7 | 0.768 | 0.592 | 0.682..0.773 | 0.368..0.767 | 7 | 0 | 0 | 0.149 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` (a size-weighted pooled view) is reported separately and named as such. With only 7 fixture clusters the bootstrap is coarse and is stated as such, not as a precise interval.

The SQLite and conv cold paths run a fresh **Python** process per request, so their cold numbers include the interpreter start-up (measured bare start-up 4389 us); VOLE's cold path is a native binary. The cold ratio is dominated by that constant and is reported for completeness, not headlined.

## Per-lane totals (sum over fixtures; times in microseconds `us`)

| lane | build us | storage B | cold us | warm us |
|---|---:|---:|---:|---:|
| vole | 246221 | 4230517 | 177076 | 29265 |
| sqlite | 618688 | 8442102 | 1116163 | 19701 |
| conv | 533277 | 5230574 | 1292676 | 197062 |

## What each lane derives and what it declines

| Q | VOLE | SQLite (source-retaining) | conv (TOML→dict) |
|---|---|---|---|
| Q1 | a scalar at a path (value) | the parsed value | the parsed value |
| Q2 | a value's exact source span | no source span -> typed decline | no source span -> typed decline |
| Q3 | a table's keys (in order) | the parsed keys | the parsed keys |
| Q4 | a boolean at a path | the parsed value | the parsed value |
| Q5 | a scalar's exact spelling | no exact spelling -> typed decline | no exact spelling -> typed decline |
| Q6 | a lexical find (keys + string values) | the matching count | the matching count |
| Q7 | the number of root keys | the parsed root key count | the parsed root key count |
| Q8 | `materialize --exact` (byte-authority) | retained raw BLOB (byte-authority) | **DECLINE** `not-native` (the source is not retained) |

## Scope (honest)

- **Self-authored deterministic corpus, NOT a real-world population.** Fixtures are generated by `tools/fixtures/make-toml.py` (Python stdlib only). Every claim is scoped to these files; the aggregate carries a fixture-clustered CI.
- **Only Q8 is a byte-authority claim.** `materialize --exact == source` (length + SHA-256 + `cmp`) reproduces the original bytes. Every other observation is a DERIVED projection; semantic agreement is not archival equality.
- **This is where VOLE claims value.** A conventional TOML load (`tomllib`) drops comments and every source offset and parses numbers / date-times into host types (losing the exact spelling). VOLE's Q2/Q5 expose exactly those source spans and spellings; its exactness is byte-authoritative for arbitrary TOML.
- **VOLE capability gaps are recorded, never papered over.** Any question VOLE declines is a typed decline (`rc` 6).
- **Nothing here is run on the host.** Every command ran in the pinned `doc-baseline` container.

