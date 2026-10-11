# Phase 21.17 — JSON5 / JSONC economic court

**Question.** Against two conventional comparators — a source-retaining SQLite store that must first normalize JSON5 to *strict* JSON, and a conventional JSON5 -> object load — on contract-equivalent terms, can VOLE answer the same twelve questions (Q1–Q12) it can answer, while closing the original JSON5 byte-exactly — and does it add value by **preserving representation** (comments, source spans, key spans, duplicate keys, and exact numeric/escape spelling)?

**Method.** A deterministic self-authored JSON5/JSONC corpus (`tools/fixtures/make-json5.py --corpus`) is regenerated at court time; each fixture is ingested by **three lanes** (VOLE field CLI with `--profile runtime --packed`; a source-retaining SQLite baseline that keeps the raw bytes and queries `json_extract`/`json_type`/`json_tree` on a JSON5->strict-JSON normalization; and a conventional JSON5->object load via a vendored pure-Python loader), Q1–Q12 are asked of each, and build/storage/cold/warm are measured. Persistent bytes are the **sum of regular-file sizes** (`find -type f -printf '%s'`), never `du -sb` (ADR-0049). Wall times are **microseconds (`us`)**.

Corpus: **9 fixtures**; lanes **vole, sqlite, conv**; questions **Q1–Q12**; bootstrap **10000 resamples, seed 21721**, cluster-resampled by fixture; tie band **+/-10%**.

VOLE lane: **release** profile (`target/release/vole-document`); substrate: **--profile runtime --packed**. The comparators (SQLite C + Python, and the pure-Python JSON5 load) are unaffected by the Rust profile while the entropyfs build is not, so the release default keeps the comparison fair to VOLE.

## Verdict

- **VOLE exactness (Q6): 9/9 byte-exact** (length + SHA-256 + `cmp`, after source + descriptor deletion in a fresh process).
- **COURT VERDICT: PASS** (fails unless exactness is 100% on the VOLE lane for every fixture).

## Build + storage (per lane, per fixture)

Time columns are **microseconds (`us`)**.

| fixture | src B | vole build us | sqlite build us | conv build us | vole B | sqlite B | conv B |
|---|---:|---:|---:|---:|---:|---:|---:|
| basic.json5 | 129 | 28711 | 27607 | 19445 | 4953 | 8287 | 157 |
| jsonc.jsonc | 137 | 27873 | 26335 | 19177 | 4969 | 8287 | 133 |
| numbers.json5 | 106 | 27745 | 27251 | 19705 | 4907 | 8322 | 199 |
| strings.json5 | 79 | 27917 | 26500 | 19558 | 4851 | 8286 | 160 |
| unicode.json5 | 51 | 28158 | 25674 | 19186 | 4795 | 8286 | 150 |
| comments.json5 | 98 | 27821 | 28105 | 20158 | 4889 | 8286 | 116 |
| large.json5 | 1048665 | 50774 | 246249 | 207322 | 2102037 | 2056291 | 1138707 |
| dup.json5 | 44 | 28198 | 26885 | 18563 | 4781 | 8286 | 99 |
| deep.json5 | 508 | 28241 | 25631 | 19543 | 5711 | 8287 | 589 |

`build us` is the best-of-N (min) of the retained repetitions.

## Paired ratios VOLE/comparator (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 9 | 1.049 | 0.877 | 0.990..1.097 | 0.606..1.067 | 1 | 7 | 1 | 0.598 |
| build | conv | 9 | 1.445 | 1.188 | 1.380..1.477 | 0.795..1.466 | 1 | 0 | 8 | 0.760 |
| storage | sqlite | 9 | 0.590 | 0.637 | 0.579..0.689 | 0.586..0.723 | 8 | 1 | 0 | 1.009 |
| storage | conv | 9 | 31.548 | 21.606 | 9.696..42.147 | 10.570..36.126 | 0 | 0 | 9 | 1.878 |
| cold | sqlite | 9 | 0.032 | 0.041 | 0.032..0.034 | 0.032..0.068 | 9 | 0 | 0 | 0.067 |
| cold | conv | 9 | 0.035 | 0.044 | 0.034..0.037 | 0.035..0.070 | 9 | 0 | 0 | 0.071 |
| warm | sqlite | 9 | 0.074 | 0.100 | 0.069..0.114 | 0.072..0.171 | 9 | 0 | 0 | 0.614 |
| warm | conv | 9 | 1.042 | 0.966 | 0.906..1.115 | 0.799..1.103 | 1 | 6 | 2 | 0.507 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` is reported separately and named as such. Each per-fixture lane cost (build, cold, warm) is the **minimum over the retained repetitions** (best-of-N); storage is measured once after the last build. Every raw sample is kept in `raw/build.tsv`, `raw/storage.tsv`, `raw/cold.tsv`, `raw/warm.tsv`. With only 9 fixture clusters the bootstrap is coarse and is stated as such, not as a precise interval.

Both conventional comparators run a fresh **Python** process per request, so their cold numbers include interpreter start-up as part of that lane's honest per-request cost (measured bare start-up 4457 us); VOLE's cold path is a native binary. The cold ratio is therefore dominated by that constant and is reported for completeness, not headlined.

## Per-lane totals (sum over fixtures; times in microseconds `us`)

| lane | build us | storage B | cold us | warm us |
|---|---:|---:|---:|---:|
| vole | 275438 | 2141893 | 147495 | 35823 |
| sqlite | 460237 | 2122618 | 2216542 | 58346 |
| conv | 362657 | 1140310 | 2090277 | 70594 |

## What each lane derives and what it declines

| Q | VOLE | source-retaining SQLite (strict-JSON normalized) | conventional JSON5->object |
|---|---|---|---|
| Q1 | a scalar at a pointer (exact token spelling) | `json_extract` value (normalized) | host value |
| Q2 | the exact source span of a node | no source span exists -> typed decline | no source span -> typed decline |
| Q3 | a node's kind | `json_type` (normalized) | host-value kind |
| Q4 | key existence + duplicate-key count | `json_tree` fullkey count (duplicates **enumerated**) | duplicate keys collapsed -> typed decline |
| Q5 | an array element value at an index | `json_extract` at the index (normalized) | host value |
| Q6 | `materialize --exact` (byte-authority) | retained raw BLOB (byte-authority) | no source bytes -> typed decline |
| Q7 | lexical find over keys/strings (with spans) | `json_tree` scan over keys/strings (no spans) | host-structure walk (no spans) |
| Q8 | the exact raw string token bytes at a pointer | `json()` re-serialization (spelling lost) -> typed decline | host value (token lost) -> typed decline |
| Q9 | comment count + exact comment spans (JSON5-only) | comments stripped by normalization -> typed decline | comments dropped -> typed decline |
| Q10 | an unquoted/plain key's exact span + key kind (JSON5-only) | no key span exists -> typed decline | no key span -> typed decline |
| Q11 | the recorded dialect (jsonc vs json5, JSON5-only) | stored dialect column | dialect not recorded -> typed decline |
| Q12 | the exact raw numeric token at a pointer (JSON5-only) | numbers normalized -> typed decline | numbers parsed -> typed decline |

## Comparison normalization (contract-equivalence)

- **Q1/Q5 (value):** compared as `(kind, canonical value)`. VOLE's canonical value decodes strings and canonicalizes numbers (`0xFF` -> `255`, `.5` -> `0.5`, `Infinity` -> `inf`) via the same vendored loader, so a *value* answer is compared on value, not on spelling; the spelling difference is captured separately by Q8/Q12. A spelling difference is therefore NOT counted as a Q1/Q5 mismatch.
- **Q3 (kind):** SQLite `json_type` is normalized to the VOLE vocabulary (`text`->`string`, `integer`/`real`->`number`).
- **Q4 (duplicates):** the SQLite lane's normalization preserves duplicate members, so `json_tree` counts them exactly; the conventional load collapses them and declines. A duplicate-key **count** difference (e.g. in the `dup` fixture) is a recorded mismatch, not hidden.
- **Q7 (find):** the match set is compared as `(pointer, role, text)`; SQLite `fullkey` is normalized to an RFC 6901 pointer.
- **Q8/Q12 (token spelling):** VOLE returns the exact source token bytes; both comparators decline (SQLite re-serializes; the conventional load yields host values).
- **Q9/Q10/Q11 (JSON5-only):** comment spans, key spans, and the recorded dialect are VOLE's representation surface; the comparators decline, except the SQLite lane's **stored dialect column** (Q11).

## Scope (honest)

- **Self-authored deterministic corpus, NOT a real-world population.** The fixtures are generated by `tools/fixtures/make-json5.py` (Python stdlib only). Every claim is scoped to these files; the aggregate carries a fixture-clustered CI and is not extrapolated.
- **Only Q6 is a byte-authority claim.** `materialize --exact == source` (length + SHA-256 + `cmp`) and the retained blob reproduce the original bytes. Every other observation is a DERIVED projection (`Q_gen`, `exact:false`); semantic agreement is not archival equality.
- **The strict-JSON boundary is a real comparator limitation.** A source-retaining store fronted by *strict* JSON cannot represent JSON5's `Infinity`/`NaN` at all: `Infinity` is mapped to `1e999`, and a source containing `NaN` cannot be normalized, so that lane declines every JSON question for it (recorded, e.g. the `numbers` fixture).
- **The conventional load is deliberately the weaker comparator.** It is a JSON5->object load (not a span-preserving scanner): it drops spans, comments, duplicate keys, key spans, the dialect, and spelling. A *span-preserving* JSON5 loader could in principle match VOLE on Q2/Q8/Q10/Q12; such a lane is **not** built here and no claim is made against it — the honest differentiator this court measures is exact closure (Q6) plus the representation surface against these two comparators.
- **VOLE capability gaps are recorded, never papered over.** Any question VOLE declines is a typed decline (rc 6) and appears as a `capability-gap`, never claimed as equivalence.
- **Nothing here is run on the host.** Every command ran in a pinned container (dev toolchain + python3 + sqlite3).

