# Phase 21.5.1 — JSON economic court

> **[SUPERSEDED: re-sealed by `evidence/campaigns/2026-10-09-phase21-5-json-econ-3400385`]**
> An external review established two defects in this receipt's comparator (FIX 1,
> Phase 21.5.3):
> 1. The SQLite lane's "uses `json1`/`jsonb`" claim was **false** — the image's
>    SQLite is **3.40.1** (2022-12-28) and JSONB exists only from **3.45.0**, so
>    JSONB was never used. The corrected lane runs the hash-pinned `pysqlite3`
>    module (bundled **SQLite 3.51.1**).
> 2. The duplicate-key count was **hardcoded to 1**, so "SQLite cannot count
>    duplicate keys" was never established — `json_tree`/`json_each` DO enumerate
>    duplicate members.
>
> Consequently the build/warm ratios below (**build median 1.350**, **warm median
> 0.527**) and the Q4 "equal 6 / mismatch 1" result are **superseded**. Re-measured
> against the modern-SQLite and (newly added) span-preserving Python comparators,
> Q4 is **7/7 equal** and VOLE's Q2/Q4/Q8 representation advantages do not survive;
> see the new campaign's SUMMARY for the corrected numbers and intervals.

**Question.** Against a source-retaining SQLite baseline using its built-in JSON functions, on contract-equivalent terms, can VOLE answer the same eight questions (Q1–Q8) it can answer, while closing the original JSON byte-exactly — and does it add value by **preserving representation** (spelling, order, duplicate keys, source spans)?

**Method.** A deterministic self-authored JSON corpus (`tools/fixtures/make-json.py --corpus`) is regenerated at court time; each fixture is ingested by two lanes (VOLE field CLI; a source-retaining SQLite baseline that keeps the raw bytes and uses `json1`/`jsonb`), Q1–Q8 are asked of each, and build/storage/cold/warm are measured. Persistent bytes are the **sum of regular-file sizes** (`find -type f -printf '%s'`), never `du -sb` (ADR-0049).

Corpus: **7 fixtures**; lanes **vole, sqlite**; questions **Q1–Q8**; bootstrap **10000 resamples, seed 21521**, cluster-resampled by fixture; tie band **+/-10%**.

VOLE lane: **release** profile (`target/release/vole-document`); substrate: **--profile runtime --packed**. The comparator (SQLite C + Python) is unaffected by the Rust profile while the entropyfs build is not, so the release default keeps the comparison fair to VOLE. All wall times are **microseconds (`us`)**.

## Verdict

- **VOLE exactness (Q6): 7/7 byte-exact** (length + SHA-256 + `cmp`, after source + descriptor deletion in a fresh process).
- **COURT VERDICT: PASS** (fails unless exactness is 100% on the VOLE lane for every fixture).

## Build + storage (per lane, per fixture)

Time columns are **microseconds (`us`)**.

| fixture | src B | vole build us | sqlite build us | vole B | sqlite B |
|---|---:|---:|---:|---:|---:|
| basic.json | 171 | 32884 | 24343 | 5033 | 8253 |
| unicode.json | 104 | 32495 | 24781 | 4899 | 8253 |
| numbers.json | 125 | 32520 | 24419 | 4941 | 8253 |
| dup.json | 43 | 32354 | 23959 | 4775 | 8253 |
| deep.json | 321 | 32027 | 24375 | 5333 | 8253 |
| scalar.json | 10 | 32750 | 23461 | 4709 | 8252 |
| large.json | 2126936 | 104875 | 69129 | 4258575 | 4264003 |

`build us` is the best-of-N (min) of the retained repetitions.

## Paired ratios VOLE/sqlite (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 7 | 1.350 | 1.366 | 1.314..1.396 | 1.327..1.420 | 0 | 0 | 7 | 1.398 |
| storage | sqlite | 7 | 0.599 | 0.644 | 0.579..0.646 | 0.587..0.752 | 6 | 1 | 0 | 0.994 |
| cold | sqlite | 7 | 0.051 | 0.072 | 0.050..0.053 | 0.050..0.149 | 7 | 0 | 0 | 0.166 |
| warm | sqlite | 7 | 0.527 | 0.580 | 0.353..0.644 | 0.449..0.781 | 6 | 0 | 1 | 1.108 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` is reported separately and named as such. Each per-fixture lane cost (build, cold, warm) is the **minimum over the retained repetitions** (best-of-N); storage is measured once after the last build. Every raw sample is kept in `raw/build.tsv`, `raw/cold.tsv`, `raw/warm.tsv`. With only 7 fixture clusters the bootstrap is coarse and is stated as such, not as a precise interval.

The SQLite cold path runs a fresh **Python** process per request, so its cold numbers include interpreter start-up as part of that lane's honest per-request cost (measured bare start-up 4386 us); VOLE's cold path is a native binary. The cold ratio is therefore dominated by that constant and is reported for completeness, not headlined.

## Per-lane totals (sum over fixtures; times in microseconds `us`)

| lane | build us | storage B | cold us | warm us |
|---|---:|---:|---:|---:|
| vole | 299905 | 4288265 | 162526 | 46445 |
| sqlite | 214467 | 4313520 | 978518 | 41934 |

## What each lane derives and what it declines

| Q | VOLE | SQLite (`json1`/`jsonb`) |
|---|---|---|
| Q1 | a scalar at a pointer (value spelling preserved) | `json_extract` value (normalized) |
| Q2 | the exact source span of a node | no source span exists in SQLite -> typed decline |
| Q3 | a node's kind | `json_type` (normalized) |
| Q4 | key existence + duplicate-key count | existence only; duplicates are collapsed |
| Q5 | an array element (value spelling preserved) | `json_extract` at the index |
| Q6 | `materialize --exact` (byte-authority) | retained raw BLOB (byte-authority) |
| Q7 | lexical find over keys/strings (with spans) | `json_tree` scan over keys/strings (no spans) |
| Q8 | the exact raw token bytes at a pointer | `json()` re-serialization (spelling not preserved) |

## Comparison normalization (contract-equivalence)

- **Q1/Q5 (value):** string equality after normalization; VOLE returns the exact source token (e.g. `1e3`), SQLite returns a normalized number/string. A spelling difference is a recorded mismatch.
- **Q3 (kind):** SQLite `json_type` is normalized to the VOLE vocabulary (`text`->`string`, `integer`/`real`->`number`).
- **Q4 (duplicates):** VOLE reports the true member count for the key; SQLite collapses duplicates, so its count is 1 for an existing key. The duplicate-key fixture therefore MISMATCHES by design — that mismatch is the representation value VOLE adds.
- **Q7 (find):** the match set is compared as `(pointer, role, text)`; SQLite `fullkey` is normalized to an RFC 6901 pointer.
- **Q8 (token):** VOLE returns the exact source token bytes; SQLite returns `json()` re-serialization. They agree only where spelling coincides (e.g. a plain integer) and mismatch where it does not.

## Scope (honest)

- **Self-authored deterministic corpus, NOT a real-world population.** The fixtures are generated by `tools/fixtures/make-json.py` (Python stdlib only). Every claim is scoped to these files; the aggregate carries a fixture-clustered CI and is not extrapolated.
- **Only Q6 is a byte-authority claim.** `materialize --exact == source` (length + SHA-256 + `cmp`) and the retained blob reproduce the original bytes. Every other observation is a DERIVED projection; semantic agreement is not archival equality.
- **This is where VOLE claims value.** SQLite's `jsonb` is a serious comparator and is cheaper on some metrics, but it is value-oriented: it does not preserve numeric/escape spelling, member order, duplicate keys, or source spans. VOLE's Q2/Q4/Q8 expose exactly those distinctions, and its exactness is byte-authoritative for arbitrary JSON.
- **VOLE capability gaps are recorded, never papered over.** Any question VOLE declines is a typed decline (`rc` 6) and appears as a `capability-gap`, never claimed as equivalence.
- **Nothing here is run on the host.** Every command ran in the pinned `doc-baseline` container (dev toolchain + python3 + sqlite3).

