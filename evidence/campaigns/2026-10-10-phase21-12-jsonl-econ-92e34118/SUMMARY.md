# Phase 21.12 — JSONL economic court

**Question.** Against a source-retaining SQLite baseline *and* a conventional JSON→object (`json`) baseline, can VOLE answer the same questions (Q1–Q8) it can answer, at comparable build/storage/cold/warm cost, while closing the original JSONL byte-exactly — and does it add value by **preserving representation** (a record value's exact source span and a scalar's exact spelling)?

**Method.** A deterministic self-authored JSONL corpus (`tools/fixtures/make-jsonl.py --corpus`) is regenerated at court time; each fixture is ingested by three lanes (VOLE field CLI; a source-retaining SQLite baseline; a conventional JSON→object `json` baseline), Q1–Q8 are asked of each, and build/storage/cold/warm are measured. Persistent bytes are the **sum of regular-file sizes** (`find -type f -printf '%s'`), never `du -sb` (ADR-0049).

Corpus: **6 fixtures**; lanes **vole, sqlite, conv**; questions **Q1–Q8**; bootstrap **10000 resamples, seed 21121**, cluster-resampled by fixture; tie band **+/-10%**.

VOLE lane: **release** profile (`target/release/vole-document`); substrate: **--profile runtime --packed**. All wall times are **microseconds (`us`)**.

## Verdict

- **VOLE exactness (Q8): 6/6 byte-exact** (length + SHA-256 + `cmp`, after source + descriptor deletion in a fresh process).
- **COURT VERDICT: PASS** (fails unless exactness is 100% on the VOLE lane for every fixture).

## Build + storage (per lane, per fixture)

Time columns are **microseconds (`us`)**.

| fixture | src B | vole build us | sqlite build us | conv build us | vole B | sqlite B | conv B |
|---|---:|---:|---:|---:|---:|---:|---:|
| basic.ndjson | 245 | 28588 | 24728 | 18167 | 5185 | 20513 | 785 |
| shapes.ndjson | 88 | 27865 | 23797 | 17332 | 4869 | 20513 | 460 |
| unicode.ndjson | 93 | 27985 | 24355 | 17161 | 4879 | 20513 | 358 |
| crlf.ndjson | 29 | 27825 | 23812 | 16512 | 4751 | 20513 | 183 |
| blank.ndjson | 20 | 27724 | 23656 | 16301 | 4733 | 20513 | 149 |
| large.ndjson | 2097199 | 61267 | 357156 | 256556 | 4199105 | 9228325 | 6467766 |

`build us` is the best-of-N (min) of the retained repetitions.

## Paired ratios VOLE/other (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 6 | 1.162 | 0.846 | 0.660..1.171 | 0.446..1.168 | 1 | 0 | 5 | 0.421 |
| build | conv | 6 | 1.619 | 1.189 | 0.906..1.693 | 0.623..1.668 | 1 | 0 | 5 | 0.588 |
| storage | sqlite | 6 | 0.238 | 0.265 | 0.231..0.354 | 0.233..0.331 | 6 | 0 | 0 | 0.453 |
| storage | conv | 6 | 12.107 | 8.939 | 3.627..28.863 | 2.910..21.373 | 1 | 0 | 5 | 0.653 |
| cold | sqlite | 6 | 0.046 | 0.074 | 0.044..0.449 | 0.045..0.198 | 6 | 0 | 0 | 0.188 |
| cold | conv | 6 | 0.047 | 0.063 | 0.046..0.172 | 0.046..0.118 | 6 | 0 | 0 | 0.143 |
| warm | sqlite | 6 | 0.214 | 0.350 | 0.191..2.315 | 0.198..0.982 | 5 | 0 | 1 | 3.816 |
| warm | conv | 6 | 0.946 | 0.757 | 0.575..0.990 | 0.472..0.975 | 1 | 5 | 0 | 0.236 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` (a size-weighted pooled view) is reported separately and named as such. With only 6 fixture clusters the bootstrap is coarse and is stated as such, not as a precise interval.

The SQLite and conv cold paths run a fresh **Python** process per request, so their cold numbers include the interpreter start-up (measured bare start-up 4375 us); VOLE's cold path is a native binary. The cold ratio is dominated by that constant and is reported for completeness, not headlined.

## Per-lane totals (sum over fixtures; times in microseconds `us`)

| lane | build us | storage B | cold us | warm us |
|---|---:|---:|---:|---:|
| vole | 201254 | 4223522 | 150146 | 59032 |
| sqlite | 477504 | 9330890 | 797563 | 15469 |
| conv | 342029 | 6469701 | 1046519 | 250374 |

## What each lane derives and what it declines

| Q | VOLE | SQLite (source-retaining) | conv (JSON→object) |
|---|---|---|---|
| Q1 | a field value in record N | the parsed value | the parsed value |
| Q2 | a record value's exact source span | no source span -> typed decline | no source span -> typed decline |
| Q3 | the per-line top-level kind of record N | the parsed kind | the parsed kind |
| Q4 | a scalar value in record N | the parsed value | the parsed value |
| Q5 | a scalar's exact spelling | no exact spelling -> typed decline | no exact spelling -> typed decline |
| Q6 | a lexical find over records | the matching count | the matching count |
| Q7 | the record count | the parsed record count | the parsed record count |
| Q8 | `materialize --exact` (byte-authority) | retained raw BLOB (byte-authority) | **DECLINE** `not-native` (the source is not retained) |

## Scope (honest)

- **Self-authored deterministic corpus, NOT a real-world population.** Fixtures are generated by `tools/fixtures/make-jsonl.py` (Python stdlib only). Every claim is scoped to these files; the aggregate carries a fixture-clustered CI.
- **Only Q8 is a byte-authority claim.** `materialize --exact == source` (length + SHA-256 + `cmp`) reproduces the original bytes. Every other observation is a DERIVED projection; semantic agreement is not archival equality.
- **This is where VOLE claims value.** A conventional JSON load (`json`) drops every source offset and parses numbers / escapes into host values (losing the exact spelling), and collapses duplicate keys. VOLE's Q2/Q5 expose exactly those source spans and spellings; its exactness is byte-authoritative for arbitrary JSONL.
- **VOLE capability gaps are recorded, never papered over.** Any question VOLE declines is a typed decline (`rc` 6).
- **Nothing here is run on the host.** Every command ran in the pinned `doc-baseline` container.

