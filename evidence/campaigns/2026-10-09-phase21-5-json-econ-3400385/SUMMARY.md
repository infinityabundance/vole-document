# Phase 21.5.1 — JSON economic court

**Question.** Against a source-retaining SQLite baseline using its built-in JSON functions, on contract-equivalent terms, can VOLE answer the same eight questions (Q1–Q8) it can answer, while closing the original JSON byte-exactly — and does it add value by **preserving representation** (spelling, order, duplicate keys, source spans)?

**Method.** A deterministic self-authored JSON corpus (`tools/fixtures/make-json.py --corpus`) is regenerated at court time; each fixture is ingested by **three lanes** (VOLE field CLI; a source-retaining SQLite baseline that keeps the raw bytes and queries `json1`/`jsonb` on a hash-pinned modern SQLite; and a **span-preserving pure-Python baseline** that retains the source and records every token's exact byte span), Q1–Q8 are asked of each, and build/storage/cold/warm are measured. Persistent bytes are the **sum of regular-file sizes** (`find -type f -printf '%s'`), never `du -sb` (ADR-0049).

Corpus: **7 fixtures**; lanes **vole, sqlite, spanpy**; questions **Q1–Q8**; bootstrap **10000 resamples, seed 21521**, cluster-resampled by fixture; tie band **+/-10%**.

VOLE lane: **release** profile (`target/release/vole-document`); substrate: **--profile runtime --packed**. The comparators (SQLite C + Python, and the pure-Python span-preserving scanner) are unaffected by the Rust profile while the entropyfs build is not, so the release default keeps the comparison fair to VOLE. All wall times are **microseconds (`us`)**.

## Verdict

- **VOLE exactness (Q6): 7/7 byte-exact** (length + SHA-256 + `cmp`, after source + descriptor deletion in a fresh process).
- **COURT VERDICT: PASS** (fails unless exactness is 100% on the VOLE lane for every fixture).

## Build + storage (per lane, per fixture)

Time columns are **microseconds (`us`)**.

| fixture | src B | vole build us | sqlite build us | spanpy build us | vole B | sqlite B | spanpy B |
|---|---:|---:|---:|---:|---:|---:|---:|
| basic.json | 171 | 31519 | 37210 | 27798 | 5033 | 8254 | 882 |
| unicode.json | 104 | 29760 | 35561 | 23236 | 4899 | 8254 | 524 |
| numbers.json | 125 | 32126 | 44174 | 31134 | 4941 | 8254 | 603 |
| dup.json | 43 | 29990 | 35963 | 23544 | 4775 | 8253 | 445 |
| deep.json | 321 | 30175 | 34958 | 22188 | 5333 | 8254 | 3040 |
| scalar.json | 10 | 29935 | 35131 | 22446 | 4709 | 8253 | 139 |
| large.json | 2126936 | 71957 | 47815 | 1517822 | 4258575 | 4264004 | 14446349 |

`build us` is the best-of-N (min) of the retained repetitions.

## Paired ratios VOLE/comparator (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 7 | 0.847 | 0.899 | 0.834..0.863 | 0.795..1.083 | 6 | 0 | 1 | 0.943 |
| build | spanpy | 7 | 1.274 | 0.772 | 1.032..1.334 | 0.299..1.294 | 1 | 1 | 5 | 0.153 |
| storage | sqlite | 7 | 0.599 | 0.644 | 0.579..0.646 | 0.587..0.752 | 6 | 1 | 0 | 0.994 |
| storage | spanpy | 7 | 8.194 | 5.036 | 1.754..10.730 | 1.590..12.793 | 1 | 0 | 6 | 0.297 |
| cold | sqlite | 7 | 0.037 | 0.052 | 0.035..0.038 | 0.035..0.109 | 7 | 0 | 0 | 0.113 |
| cold | spanpy | 7 | 0.044 | 0.044 | 0.043..0.047 | 0.042..0.046 | 7 | 0 | 0 | 0.046 |
| warm | sqlite | 7 | 0.401 | 0.521 | 0.373..0.617 | 0.397..0.754 | 6 | 0 | 1 | 1.148 |
| warm | spanpy | 7 | 1.027 | 1.113 | 0.822..1.295 | 0.949..1.296 | 2 | 2 | 3 | 0.970 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` is reported separately and named as such. Each per-fixture lane cost (build, cold, warm) is the **minimum over the retained repetitions** (best-of-N); storage is measured once after the last build. Every raw sample is kept in `raw/build.tsv`, `raw/cold.tsv`, `raw/warm.tsv`. With only 7 fixture clusters the bootstrap is coarse and is stated as such, not as a precise interval.

## Supersedes — Phase 21.5.3 (FIX 1)

`[SUPERSEDED: evidence/campaigns/2026-10-09-phase21-5-json-econ-0d94667]` — that campaign's SQLite lane claimed `json1`/`jsonb` but actually ran SQLite **3.40.1** (JSONB ships only from 3.45.0) and **hardcoded** `duplicate_count = 1`. This campaign runs a hash-pinned modern SQLite (**3.51.1**, via `pysqlite3`) and counts duplicates with `json_tree`; every prior ratio is superseded by the tables above.

| metric | comparator | then | now (median) |
|---|---|---:|---:|
| build | sqlite | [SUPERSEDED: 1.350] | 0.847 |
| storage | sqlite | [SUPERSEDED: 0.599] | 0.599 |
| cold | sqlite | [SUPERSEDED: 0.051] | 0.037 |
| warm | sqlite | [SUPERSEDED: 0.527] | 0.401 |

Q4 then: 6 equal + 1 mismatch (hardcoded duplicate_count=1). Q4 now: `json_tree` enumerates duplicate members, so VOLE and SQLite **agree** — the prior mismatch was a measurement artefact (a hardcoded constant), not a SQLite limitation. Against the added span-preserving pure-Python baseline VOLE matches on Q1–Q8 wherever it answers, so VOLE's Q2/Q4/Q8 representation advantages do **not** survive against the strongest competitor; only Q6 (byte-exact closure) and the economics remain. See the campaign's SUMMARY `Scope` section.

Both conventional comparator cold paths (SQLite C + Python, and the pure-Python span-preserving scanner) run a fresh **Python** process per request, so their cold numbers include interpreter start-up as part of that lane's honest per-request cost (measured bare start-up 8540 us); VOLE's cold path is a native binary. The cold ratio is therefore dominated by that constant and is reported for completeness, not headlined.

## Per-lane totals (sum over fixtures; times in microseconds `us`)

| lane | build us | storage B | cold us | warm us |
|---|---:|---:|---:|---:|
| vole | 255462 | 4288265 | 182503 | 60033 |
| sqlite | 270812 | 4313526 | 1612796 | 52311 |
| spanpy | 1668168 | 14451982 | 3925935 | 61912 |

## What each lane derives and what it declines

| Q | VOLE | SQLite (`json1`/`jsonb`, modern) | span-preserving Python |
|---|---|---|---|
| Q1 | a scalar at a pointer (value spelling preserved) | `json_extract` value (normalized) | decoded string / raw number token |
| Q2 | the exact source span of a node | no source span exists in SQLite -> typed decline | exact byte span `[start,end)` |
| Q3 | a node's kind | `json_type` (normalized) | node kind |
| Q4 | key existence + duplicate-key count | `json_tree` fullkey count (duplicates **enumerated**) | member count (duplicates kept) |
| Q5 | an array element (value spelling preserved) | `json_extract` at the index | decoded string / raw number token |
| Q6 | `materialize --exact` (byte-authority) | retained raw BLOB (byte-authority) | retained raw bytes (byte-authority) |
| Q7 | lexical find over keys/strings (with spans) | `json_tree` scan over keys/strings (no spans) | scanner walk (no spans returned) |
| Q8 | the exact raw token bytes at a pointer | `json()` re-serialization (spelling not preserved) | exact raw source token bytes |

## Comparison normalization (contract-equivalence)

- **Q1/Q5 (value):** string equality after normalization; VOLE returns the exact source token (e.g. `1e3`), SQLite returns a normalized number/string. A spelling difference is a recorded mismatch.
- **Q3 (kind):** SQLite `json_type` is normalized to the VOLE vocabulary (`text`->`string`, `integer`/`real`->`number`).
- **Q4 (duplicates, CORRECTED in 21.5.3):** the true member count is counted with `json_tree` (which enumerates duplicate members), so SQLite now agrees with VOLE and with the span-preserving scanner. The earlier receipt's "SQLite collapses duplicates -> count 1" was a measurement artefact of a hardcoded constant, not a property of SQLite.
- **Q7 (find):** the match set is compared as `(pointer, role, text)`; SQLite `fullkey` is normalized to an RFC 6901 pointer.
- **Q8 (token):** VOLE returns the exact source token bytes; SQLite returns `json()` re-serialization, but the span-preserving Python scanner returns the exact raw token bytes, so Q8 matches VOLE there.

## Scope (honest)

- **Self-authored deterministic corpus, NOT a real-world population.** The fixtures are generated by `tools/fixtures/make-json.py` (Python stdlib only). Every claim is scoped to these files; the aggregate carries a fixture-clustered CI and is not extrapolated.
- **Only Q6 is a byte-authority claim.** `materialize --exact == source` (length + SHA-256 + `cmp`) and the retained blob reproduce the original bytes. Every other observation is a DERIVED projection; semantic agreement is not archival equality.
- **What survives against the strongest competitor (CORRECTED).** Against a *span-preserving* pure-Python scanner, VOLE's representation advantages in Q2 (spans), Q4 (duplicate counts), and Q8 (raw token bytes) **largely evaporate** — the conventional scanner answers them too. VOLE's remaining, real advantages are (i) byte-authoritative exact closure of the whole source (Q6) and (ii) economics (storage and query cost). This is the honest picture the corrected comparator shows.
- **VOLE capability gaps are recorded, never papered over.** Any question VOLE declines is a typed decline (`rc` 6) and appears as a `capability-gap`, never claimed as equivalence.
- **Nothing here is run on the host.** Every command ran in a pinned container (dev toolchain + python3 + the hash-pinned modern SQLite).

