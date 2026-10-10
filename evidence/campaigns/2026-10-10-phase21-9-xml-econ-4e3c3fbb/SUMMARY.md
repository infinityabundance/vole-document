# Phase 21.9 — XML economic court

**Question.** Against a source-retaining SQLite baseline *and* a conventional XML→dict/text (`ElementTree`) baseline, can VOLE answer the same questions (Q1–Q8) it can answer, at comparable build/storage/cold/warm cost, while closing the original XML byte-exactly — and does it add value by **preserving representation** (an element's and an attribute's exact source span, a namespace binding)?

**Method.** A deterministic self-authored XML corpus (`tools/fixtures/make-xml.py --corpus`) is regenerated at court time; each fixture is ingested by three lanes (VOLE field CLI; a source-retaining SQLite baseline; a conventional XML→dict/text ElementTree baseline), Q1–Q8 are asked of each, and build/storage/cold/warm are measured. Persistent bytes are the **sum of regular-file sizes** (`find -type f -printf '%s'`), never `du -sb` (ADR-0049).

Corpus: **6 fixtures**; lanes **vole, sqlite, conv**; questions **Q1–Q8**; bootstrap **10000 resamples, seed 21991**, cluster-resampled by fixture; tie band **+/-10%**.

VOLE lane: **release** profile (`target/release/vole-document`); substrate: **--profile runtime --packed**. All wall times are **microseconds (`us`)**.

## Verdict

- **VOLE exactness (Q8): 6/6 byte-exact** (length + SHA-256 + `cmp`, after source + descriptor deletion in a fresh process).
- **COURT VERDICT: PASS** (fails unless exactness is 100% on the VOLE lane for every fixture).

## Build + storage (per lane, per fixture)

Time columns are **microseconds (`us`)**.

| fixture | src B | vole build us | sqlite build us | conv build us | vole B | sqlite B | conv B |
|---|---:|---:|---:|---:|---:|---:|---:|
| basic.xml | 197 | 28072 | 26632 | 19504 | 5081 | 32802 | 560 |
| namespaces.xml | 116 | 29168 | 26275 | 19590 | 4919 | 32802 | 269 |
| mixed.xml | 147 | 28208 | 26727 | 20102 | 4981 | 32802 | 242 |
| attrs.xml | 64 | 27435 | 25590 | 18973 | 4813 | 32802 | 252 |
| dtd.xml | 113 | 28368 | 26193 | 19408 | 4913 | 32802 | 137 |
| large.xml | 2097289 | 67135 | 423830 | 284987 | 4199277 | 13295655 | 7257267 |

`build us` is the best-of-N (min) of the retained repetitions.

## Paired ratios VOLE/other (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 6 | 1.064 | 0.781 | 0.606..1.097 | 0.411..1.087 | 1 | 4 | 1 | 0.375 |
| build | conv | 6 | 1.443 | 1.070 | 0.819..1.475 | 0.583..1.464 | 1 | 0 | 5 | 0.545 |
| storage | sqlite | 6 | 0.151 | 0.170 | 0.148..0.235 | 0.149..0.219 | 6 | 0 | 0 | 0.314 |
| storage | conv | 6 | 18.693 | 10.517 | 4.826..28.222 | 2.973..23.858 | 1 | 0 | 5 | 0.582 |
| cold | sqlite | 6 | 0.042 | 0.109 | 0.036..8.700 | 0.038..0.838 | 5 | 0 | 1 | 3.061 |
| cold | conv | 6 | 0.043 | 0.097 | 0.038..3.623 | 0.039..0.547 | 5 | 0 | 1 | 2.509 |
| warm | sqlite | 6 | 0.205 | 0.589 | 0.105..183.796 | 0.132..8.048 | 5 | 0 | 1 | 282.423 |
| warm | conv | 6 | 1.013 | 1.318 | 0.566..7.373 | 0.693..3.483 | 2 | 2 | 2 | 13.542 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` (a size-weighted pooled view) is reported separately and named as such. With only 6 fixture clusters the bootstrap is coarse and is stated as such, not as a precise interval.

The SQLite and conv cold paths run a fresh **Python** process per request, so their cold numbers include the interpreter start-up (measured bare start-up 4357 us); VOLE's cold path is a native binary. The cold ratio is dominated by that constant and is reported for completeness, not headlined.

## Per-lane totals (sum over fixtures; times in microseconds `us`)

| lane | build us | storage B | cold us | warm us |
|---|---:|---:|---:|---:|
| vole | 208386 | 4223984 | 2715662 | 2614389 |
| sqlite | 555247 | 13459665 | 887241 | 9257 |
| conv | 382564 | 7258727 | 1082182 | 193057 |

## What each lane derives and what it declines

| Q | VOLE | SQLite (source-retaining) | conv (XML→dict/text) |
|---|---|---|---|
| Q1 | an element's text at a path | the parsed element text | the parsed element text |
| Q2 | an element's exact source span | no source span -> typed decline | no source span -> typed decline |
| Q3 | an attribute's exact source span | no source span -> typed decline | no source span -> typed decline |
| Q4 | an attribute's value | the parsed attribute value | the parsed attribute value |
| Q5 | a namespace URI for a prefix | the recorded prefix->URI binding | **DECLINE** `not-native` (the prefix binding is lost) |
| Q6 | a lexical find (character-data runs containing a pattern) | the run count (instr) | the run count |
| Q7 | the number of elements | the element count | the element count |
| Q8 | `materialize --exact` (byte-authority) | retained raw BLOB (byte-authority) | **DECLINE** `not-native` (the source is not retained) |

## Scope (honest)

- **Self-authored deterministic corpus, NOT a real-world population.** Fixtures are generated by `tools/fixtures/make-xml.py` (Python stdlib only). Every claim is scoped to these files; the aggregate carries a fixture-clustered CI.
- **Only Q8 is a byte-authority claim.** `materialize --exact == source` (length + SHA-256 + `cmp`) reproduces the original bytes. Every other observation is a DERIVED projection; semantic agreement is not archival equality.
- **This is where VOLE claims value.** Conventionally loading XML (`ElementTree`) drops comments, processing instructions, CDATA boundaries, attribute quoting, namespace-declaration spelling, and every source offset, and it expands entity references. VOLE's Q2/Q3 expose exactly those source spans (entity references are surfaced literally), and Q5 keeps the prefix→URI binding; its exactness is byte-authoritative for arbitrary XML.
- **Entity references are not expanded** by VOLE; the comparators (`ElementTree`) expand them, so any fixture plan whose text carries an entity is deliberately kept out of the agreement questions.
- **VOLE capability gaps are recorded, never papered over.** Any question VOLE declines is a typed decline (`rc` 6).
- **Nothing here is run on the host.** Every command ran in the pinned `doc-baseline` container.

