# Phase 21.10 — HTML economic court

**Question.** Against a source-retaining SQLite baseline *and* a conventional HTML→derived-view (`html.parser`) baseline, can VOLE answer the same questions (Q1–Q8) it can answer, at comparable build/storage/cold/warm cost, while closing the original HTML byte-exactly — and does it add value by **preserving representation** (an element's and an attribute's exact source span, raw script/style bytes)?

**Method.** A deterministic self-authored HTML corpus (`tools/fixtures/make-html.py --corpus`) is regenerated at court time; each fixture is ingested by three lanes (VOLE field CLI; a source-retaining SQLite baseline; a conventional HTML→view `html.parser` baseline), Q1–Q8 are asked of each, and build/storage/cold/warm are measured. Persistent bytes are the **sum of regular-file sizes** (`find -type f -printf '%s'`), never `du -sb` (ADR-0049).

Corpus: **6 fixtures**; lanes **vole, sqlite, conv**; questions **Q1–Q8**; bootstrap **10000 resamples, seed 21101**, cluster-resampled by fixture; tie band **+/-10%**.

VOLE lane: **release** profile (`target/release/vole-document`); substrate: **--profile runtime --packed**. All wall times are **microseconds (`us`)**.

## Verdict

- **VOLE exactness (Q8): 6/6 byte-exact** (length + SHA-256 + `cmp`, after source + descriptor deletion in a fresh process).
- **COURT VERDICT: PASS** (fails unless exactness is 100% on the VOLE lane for every fixture).

## Build + storage (per lane, per fixture)

Time columns are **microseconds (`us`)**.

| fixture | src B | vole build us | sqlite build us | conv build us | vole B | sqlite B | conv B |
|---|---:|---:|---:|---:|---:|---:|---:|
| basic.html | 338 | 30513 | 26638 | 18386 | 5367 | 28707 | 312 |
| elements.html | 353 | 27895 | 24914 | 17677 | 5397 | 28707 | 306 |
| rawtext.html | 314 | 28050 | 26727 | 18505 | 5319 | 28707 | 344 |
| entities.html | 282 | 27740 | 25838 | 18353 | 5255 | 28707 | 304 |
| malformed.html | 282 | 28519 | 27031 | 19632 | 5255 | 28707 | 305 |
| large.html | 2097497 | 64321 | 370453 | 299584 | 4199697 | 3854375 | 1271879 |

`build us` is the best-of-N (min) of the retained repetitions.

## Paired ratios VOLE/other (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 6 | 1.064 | 0.801 | 0.612..1.133 | 0.433..1.110 | 1 | 3 | 2 | 0.413 |
| build | conv | 6 | 1.514 | 1.110 | 0.834..1.619 | 0.574..1.585 | 1 | 0 | 5 | 0.528 |
| storage | sqlite | 6 | 0.186 | 0.249 | 0.183..0.639 | 0.184..0.450 | 5 | 1 | 0 | 1.057 |
| storage | conv | 6 | 17.216 | 12.902 | 9.382..17.462 | 7.435..17.374 | 0 | 0 | 6 | 3.319 |
| cold | sqlite | 6 | 0.043 | 0.127 | 0.042..12.409 | 0.042..1.059 | 5 | 0 | 1 | 4.660 |
| cold | conv | 6 | 0.044 | 0.126 | 0.042..11.232 | 0.043..1.015 | 5 | 0 | 1 | 4.622 |
| warm | sqlite | 6 | 0.260 | 0.952 | 0.247..333.110 | 0.252..13.123 | 5 | 0 | 1 | 478.345 |
| warm | conv | 6 | 1.488 | 3.300 | 1.435..95.229 | 1.450..16.706 | 0 | 0 | 6 | 185.377 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` (a size-weighted pooled view) is reported separately and named as such. With only 6 fixture clusters the bootstrap is coarse and is stated as such, not as a precise interval.

The SQLite and conv cold paths run a fresh **Python** process per request, so their cold numbers include the interpreter start-up (measured bare start-up 4507 us); VOLE's cold path is a native binary. The cold ratio is dominated by that constant and is reported for completeness, not headlined.

## Per-lane totals (sum over fixtures; times in microseconds `us`)

| lane | build us | storage B | cold us | warm us |
|---|---:|---:|---:|---:|
| vole | 207038 | 4226290 | 4050295 | 3912383 |
| sqlite | 501601 | 3997910 | 869165 | 8179 |
| conv | 392137 | 1273450 | 876245 | 21105 |

## What each lane derives and what it declines

| Q | VOLE | SQLite (source-retaining) | conv (HTML→view) |
|---|---|---|---|
| Q1 | a heading's text | the parsed heading text | the parsed heading text |
| Q2 | an element's exact source span | no source span -> typed decline | no source span -> typed decline |
| Q3 | an attribute's exact source span | no source span -> typed decline | no source span -> typed decline |
| Q4 | a link target (href) | the parsed href | the parsed href |
| Q5 | raw `<script>`/`<style>` bytes | the retained raw text | the retained raw text |
| Q6 | a lexical find (text runs containing a pattern) | the run count (instr) | the run count |
| Q7 | the number of elements | the element count | the element count |
| Q8 | `materialize --exact` (byte-authority) | retained raw BLOB (byte-authority) | **DECLINE** `not-native` (the source is not retained) |

## Scope (honest)

- **Self-authored deterministic corpus, NOT a real-world population.** Fixtures are generated by `tools/fixtures/make-html.py` (Python stdlib only). Every claim is scoped to these files; the aggregate carries a fixture-clustered CI.
- **Only Q8 is a byte-authority claim.** `materialize --exact == source` (length + SHA-256 + `cmp`) reproduces the original bytes. Every other observation is a DERIVED projection; semantic agreement is not archival equality.
- **This is where VOLE claims value.** Conventionally loading HTML (`html.parser`) drops comments, attribute quoting (quoted/unquoted/boolean spelling), and every source offset, and it expands entity references. VOLE's Q2/Q3 expose exactly those source spans (entity references are surfaced literally), and Q5 exposes raw script/style bytes without ever executing them; its exactness is byte-authoritative for arbitrary HTML.
- **Entity references are not expanded** by VOLE; the comparator expands them, so any fixture plan whose find/text carries an entity is deliberately kept out of the agreement questions.
- **VOLE capability gaps are recorded, never papered over.** Any question VOLE declines is a typed decline (`rc` 6).
- **Nothing here is run on the host.** Every command ran in the pinned `doc-baseline` container.

