# Phase 21.18 — CBOR economic court

**Question.** Against two conventional comparators — a source-retaining SQLite store that must first normalize the binary source to *strict* JSON, and a conventional CBOR -> host-value load — on contract-equivalent terms, can VOLE answer the same twelve questions (Q1–Q12) it can answer, while closing the original CBOR byte-exactly — and does it add value by **preserving representation** (source spans, the exact encoding width/format byte, byte-vs-text, duplicate keys, tag/extension type, and float width)?

**Method.** A deterministic self-authored CBOR corpus (`tools/fixtures/make-cbor.py --corpus`) is regenerated at court time; each fixture is ingested by **three lanes** (VOLE field CLI with `--profile runtime --packed`; a source-retaining SQLite baseline that keeps the raw bytes and queries `json_extract`/`json_type`/`json_tree` on a binary->strict-JSON normalization; and a conventional binary->host-value load via a vendored pure-Python decoder), Q1–Q12 are asked of each, and build/storage/cold/warm are measured. Persistent bytes are the **sum of regular-file sizes** (`find -type f -printf '%s'`), never `du -sb` (ADR-0049). Wall times are **microseconds (`us`)**.

Corpus: **11 fixtures**; lanes **vole, sqlite, conv**; questions **Q1–Q12**; bootstrap **10000 resamples, seed 211821**, cluster-resampled by fixture; tie band **+/-10%**.

## Pre-registered hypotheses

- **H1 (byte-exactness, or FAIL).** For every fixture — including the malformed/Opaque controls — VOLE `materialize --exact == source` (length + SHA-256 + `cmp`) after the source file AND the standalone descriptor are deleted, in a fresh process. The court FAILS unless this is 100 %.
- **H2 (contract questions answered where possible).** All three lanes answer the same Q1–Q12 where their model permits: the coarse value questions (Q1/Q3/Q5), the duplicate-key count (Q4), byte authority (Q6), and lexical find (Q7).
- **H3 (typed declines / pinned boundaries).** Every question a lane cannot answer is a TYPED decline (rc 6, never a silent empty answer); a malformed pointer is a usage error (rc 2); the detection boundaries (strict JSON, the sibling binary format, lone scalars, malformed and ambiguous inputs, prose) stay pinned.
- **H4 (economics, ADR-0054).** build/storage/cold/warm are measured per lane with a named estimator: the paired per-fixture ratio, summarised by median and geometric mean with a fixed-seed, fixture-clustered bootstrap **95 % CI**; the ratio-of-sums is reported **separately** and named as such; every raw sample is retained.

VOLE lane: **release** profile (`target/release/vole-document`); substrate: **--profile runtime --packed**. The comparators (SQLite C + Python, and the pure-Python load) are unaffected by the Rust profile while the entropyfs build is not, so the release default keeps the comparison fair to VOLE.

## Verdict

- **VOLE exactness (Q6): 20/20 byte-exact** (length + SHA-256 + `cmp`, after source + descriptor deletion in a fresh process).
- **COURT VERDICT: PASS** (fails unless exactness is 100% on the VOLE lane for every fixture).

## Build + storage (per lane, per fixture)

Time columns are **microseconds (`us`)**.

| fixture | src B | vole build us | sqlite build us | conv build us | vole B | sqlite B | conv B |
|---|---:|---:|---:|---:|---:|---:|---:|
| basic.cbor | 12 | 29780 | 28264 | 20313 | 4713 | 8266 | 139 |
| widths.cbor | 7 | 27455 | 25643 | 18489 | 4701 | 8265 | 123 |
| floats.cbor | 18 | 27586 | 26660 | 20104 | 4725 | 8266 | 122 |
| bytestext.cbor | 9 | 27963 | 26102 | 18848 | 4705 | 8265 | 135 |
| dupkeys.cbor | 7 | 27807 | 28764 | 19904 | 4701 | 8265 | 114 |
| tags.cbor | 15 | 27703 | 27088 | 19328 | 4719 | 8266 | 132 |
| indef.cbor | 13 | 27484 | 26594 | 19150 | 4715 | 8266 | 123 |
| nested.cbor | 13 | 27619 | 26291 | 19853 | 4715 | 8266 | 141 |
| mapkeys.cbor | 13 | 28590 | 25754 | 20096 | 4715 | 8307 | 142 |
| nonfinite.cbor | 10 | 27419 | 26310 | 18543 | 4709 | 8301 | 133 |
| large.cbor | 4003 | 28030 | 29531 | 20940 | 12700 | 12364 | 6109 |

`build us` is the best-of-N (min) of the retained repetitions.

## Paired ratios VOLE/comparator (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 11 | 1.042 | 1.036 | 1.023..1.071 | 1.009..1.062 | 0 | 10 | 1 | 1.035 |
| build | conv | 11 | 1.433 | 1.427 | 1.391..1.479 | 1.399..1.454 | 0 | 0 | 11 | 1.426 |
| storage | sqlite | 11 | 0.570 | 0.601 | 0.569..0.571 | 0.569..0.669 | 10 | 1 | 0 | 0.629 |
| storage | conv | 11 | 35.406 | 27.933 | 33.440..38.333 | 16.369..37.415 | 0 | 0 | 11 | 8.069 |
| cold | sqlite | 11 | 0.031 | 0.031 | 0.030..0.032 | 0.031..0.032 | 11 | 0 | 0 | 0.031 |
| cold | conv | 11 | 0.033 | 0.034 | 0.032..0.035 | 0.033..0.035 | 11 | 0 | 0 | 0.034 |
| warm | sqlite | 11 | 0.467 | 0.573 | 0.433..0.824 | 0.462..0.746 | 9 | 1 | 1 | 0.686 |
| warm | conv | 11 | 3.750 | 3.996 | 3.625..4.346 | 3.742..4.308 | 0 | 0 | 11 | 3.845 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` is reported separately and named as such. Each per-fixture lane cost (build, cold, warm) is the **minimum over the retained repetitions** (best-of-N); storage is measured once after the last build. Every raw sample is kept in `raw/build.tsv`, `raw/storage.tsv`, `raw/cold.tsv`, `raw/warm.tsv`. With only 11 fixture clusters the bootstrap is coarse and is stated as such, not as a precise interval.

Both conventional comparators run a fresh **Python** process per request, so their cold numbers include interpreter start-up as part of that lane's honest per-request cost (measured bare start-up 4826 us); VOLE's cold path is a native binary. The cold ratio is therefore dominated by that constant and is reported for completeness, not headlined.

## Per-lane totals (sum over fixtures; times in microseconds `us`)

| lane | build us | storage B | cold us | warm us |
|---|---:|---:|---:|---:|
| vole | 307436 | 59818 | 83200 | 1857 |
| sqlite | 297001 | 95097 | 2652023 | 2708 |
| conv | 215568 | 7413 | 2476428 | 483 |

## What each lane derives and what it declines

| Q | VOLE | source-retaining SQLite (strict-JSON normalized) | conventional CBOR->host |
|---|---|---|---|
| Q1 | a scalar at a pointer (class + canonical value) | `json_extract` value (normalized) | host value |
| Q2 | the exact source span of a node | no source span exists -> typed decline | no source span -> typed decline |
| Q3 | a node's value class (number/string/bool/null/array/object) | `json_type` (normalized) | host-value class |
| Q4 | key existence + duplicate-key count | `json_tree` fullkey count (duplicates **enumerated**) | duplicate keys collapsed -> typed decline |
| Q5 | an array element value at an index | `json_extract` at the index (normalized) | host value |
| Q6 | `materialize --exact` (byte-authority) | retained raw BLOB (byte-authority) | no source bytes -> typed decline |
| Q7 | lexical find over text keys/values (with spans) | `json_tree` scan over keys/strings (no spans) | host-structure walk (no spans) |
| Q8 | the exact raw token bytes at a pointer | `json()` re-serialization (spelling lost) -> typed decline | host value (token lost) -> typed decline |
| Q9 | the exact encoding width/format byte actually used | normalized to JSON numbers -> typed decline | host integers unmarshal -> typed decline |
| Q10 | the byte-vs-text kind (bin/bytes vs str/text) | byte/text conflated by the JSON normalization -> typed decline | byte/text conflated by the load -> typed decline |
| Q11 | the tag of the node (preserved, never resolved) | not preserved by the JSON normalization -> typed decline | discarded by the load -> typed decline |
| Q12 | the float width (half/single/double) | width lost by the JSON normalization -> typed decline | host float; width lost -> typed decline |

## Comparison normalization (contract-equivalence)

- **Q1/Q5 (value):** compared as `(class, canonical value)`, where the class is the coarse host class (`number`/`string`/`bool`/`null`/`array`/`object`). The conventional model **conflates** byte and text strings (a byte string is decoded to a host string), so a byte string's value is compared as a string; the *byte-vs-text* distinction itself is Q10, where both comparators decline.
- **Q3 (class):** SQLite `json_type` is normalized to the same coarse vocabulary (`text`->`string`, `integer`/`real`->`number`).
- **Q4 (duplicates):** the SQLite lane's normalization preserves duplicate members, so `json_tree` counts them exactly; the conventional load collapses them and declines.
- **Q7 (find):** the match set is compared as `(pointer, role, text)`; SQLite `fullkey` is normalized to an RFC 6901 pointer and spans are not compared (neither comparator keeps them).
- **Q9/Q10/Q11/Q12 (representation):** the exact encoding width/format byte, the byte-vs-text kind, the tag/extension type, and the float width are VOLE's representation surface; both comparators decline.
- **Recorded mismatches (duplicate keys).** For a document with duplicate keys (`dupkeys.cbor`), VOLE resolves a pointer to the **first** matching member while the conventional load collapses to the **last** (Q1/Q5), and VOLE's lexical find reports **two** matches while the collapsed load reports one (Q7). Those differences are counted as `mismatch`, never hidden.

## Scope (honest)

- **Self-authored deterministic corpus, NOT a real-world population.** The fixtures are generated by `tools/fixtures/make-cbor.py` (Python stdlib only). Every claim is scoped to these files; the aggregate carries a fixture-clustered CI and is not extrapolated.
- **Only Q6 is a byte-authority claim.** `materialize --exact == source` (length + SHA-256 + `cmp`) and the retained blob reproduce the original bytes. Every other observation is a DERIVED projection (`Q_gen`, `exact:false`); semantic agreement is not archival equality.
- **The strict-JSON boundary is a real comparator limitation.** A source-retaining store fronted by *strict* JSON cannot represent a map with a non-text key, nor `NaN`: those sources cannot be normalized at all, so the SQLite lane declines every JSON question for them (recorded, e.g. the `mapkeys.cbor` and `nonfinite` fixtures).
- **The conventional load is deliberately the weaker comparator.** It is a binary->host-value load: it drops spans, the encoding width/signedness, the byte-vs-text kind, duplicate keys, the tag/extension type, and float width. A *representation-preserving* decoder could in principle match VOLE on several of those; such a lane is **not** built here and no claim is made against it — the honest differentiator this court measures is exact closure (Q6) plus the representation surface against these two comparators.
- **VOLE capability gaps are recorded, never papered over.** Any question VOLE declines is a typed decline (rc 6) and appears as a `capability-gap`, never claimed as equivalence.
- **Nothing here is run on the host.** Every command ran in a pinned container (dev toolchain + python3 + sqlite3).

