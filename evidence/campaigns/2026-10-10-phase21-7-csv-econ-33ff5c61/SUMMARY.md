# Phase 21.7 — CSV/TSV economic court

**Question.** Against a source-retaining SQLite baseline *and* a DuckDB/Parquet analytical baseline, can VOLE answer the same eight questions (Q1–Q8) it can answer, at comparable build/storage/cold/warm cost, while closing the original CSV/TSV byte-exactly — and does it add value by **preserving representation** (exact raw field tokens, exact record bytes, source spans)?

**Method.** A deterministic self-authored CSV/TSV corpus (`tools/fixtures/make-csv.py --corpus`) is regenerated at court time; each fixture is ingested by three lanes (VOLE field CLI; a source-retaining SQLite baseline; a DuckDB/Parquet baseline), Q1–Q8 are asked of each, and build/storage/cold/warm are measured. Persistent bytes are the **sum of regular-file sizes** (`find -type f -printf '%s'`), never `du -sb` (ADR-0049).

Corpus: **6 fixtures**; lanes **vole, sqlite, duckdb**; questions **Q1–Q8**; bootstrap **10000 resamples, seed 21721**, cluster-resampled by fixture; tie band **+/-10%**.

VOLE lane: **release** profile (`target/release/vole-document`); substrate: **--profile runtime --packed**. All wall times are **microseconds (`us`)**.

## Verdict

- **VOLE exactness (Q8): 6/6 byte-exact** (length + SHA-256 + `cmp`, after source + descriptor deletion in a fresh process).
- **COURT VERDICT: PASS** (fails unless exactness is 100% on the VOLE lane for every fixture).

## Build + storage (per lane, per fixture)

Time columns are **microseconds (`us`)**.

| fixture | src B | vole build us | sqlite build us | duckdb build us | vole B | sqlite B | duckdb B |
|---|---:|---:|---:|---:|---:|---:|---:|
| basic.csv | 43 | 27887 | 31717 | 58247 | 4807 | 20580 | 756 |
| quoted.csv | 57 | 27652 | 30221 | 55482 | 4835 | 20580 | 739 |
| crlf.csv | 54 | 27790 | 30402 | 60533 | 4831 | 20580 | 751 |
| bom.csv | 15 | 28551 | 30166 | 59432 | 4751 | 20580 | 595 |
| ragged.csv | 45 | 27901 | 30054 | 58447 | 4811 | 20580 | 1244 |
| tsv.tsv | 49 | 27967 | 29905 | 58358 | 4817 | 20581 | 764 |

`build us` is the best-of-N (min) of the retained repetitions.

## Paired ratios VOLE/other (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 6 | 0.922 | 0.919 | 0.897..0.941 | 0.901..0.935 | 1 | 5 | 0 | 0.919 |
| build | duckdb | 6 | 0.479 | 0.479 | 0.468..0.489 | 0.469..0.488 | 6 | 0 | 0 | 0.479 |
| storage | sqlite | 6 | 0.234 | 0.234 | 0.232..0.235 | 0.232..0.235 | 6 | 0 | 0 | 0.234 |
| storage | duckdb | 6 | 6.396 | 6.111 | 5.086..7.264 | 4.983..7.129 | 0 | 0 | 6 | 5.950 |
| cold | sqlite | 6 | 0.043 | 0.043 | 0.042..0.045 | 0.042..0.044 | 6 | 0 | 0 | 0.043 |
| cold | duckdb | 6 | 0.014 | 0.014 | 0.013..0.014 | 0.013..0.014 | 6 | 0 | 0 | 0.014 |
| warm | sqlite | 6 | 0.661 | 0.663 | 0.640..0.688 | 0.645..0.684 | 6 | 0 | 0 | 0.663 |
| warm | duckdb | 6 | 0.015 | 0.015 | 0.015..0.016 | 0.015..0.016 | 6 | 0 | 0 | 0.015 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` (a size-weighted pooled view, which the review flagged can disagree with the medians) is reported separately and named as such. With only 6 fixture clusters the bootstrap is coarse and is stated as such, not as a precise interval.

The SQLite and DuckDB cold paths run a fresh **Python** process per request, so their cold numbers include the interpreter start-up (measured bare start-up 4943 us); VOLE's cold path is a native binary. The cold ratio is dominated by that constant and is reported for completeness, not headlined.

## Large-file case (the cheap high-volume court)

| lane | source B | ingest us | ingest MB/s | store/index B | selective read (us) | selective value |
|---|---:|---:|---:|---:|---:|---|
| vole | 52428807 | 807462 | 61.922 | 104862352 | 158844 | `0` |
| sqlite | 52428807 | 7546227 | 6.626 | 270970997 | 129 | `0` |
| duckdb | 52428807 | 510254 | 97.990 | 4335268 | 1074 | `0` |

Ingest throughput is `source MB / (ingest wall seconds)`; VOLE's is `field-build --profile runtime --packed` (ingest does **not** parse the table — the model is derived on demand), so it is a bounded-memory store write, not a full load. The selective read is a point read (`csv-row`/cell on the VOLE lane; a primary-key lookup on SQLite; a Parquet point query on DuckDB); CSV has no index, so VOLE's read scans forward in bounded memory.

## Per-lane totals (sum over fixtures; times in microseconds `us`)

| lane | build us | storage B | cold us | warm us |
|---|---:|---:|---:|---:|
| vole | 167748 | 28852 | 34942 | 348 |
| sqlite | 182465 | 123481 | 808162 | 525 |
| duckdb | 350499 | 4849 | 2553110 | 22539 |

## What each lane derives and what it declines

| Q | VOLE | SQLite (source-retaining) | DuckDB (Parquet) |
|---|---|---|---|
| Q1 | a cell's decoded value | the loaded value (normalized) | the parsed value (SQL) |
| Q2 | a cell's exact raw token (quotes preserved) | no raw token retained -> typed decline | no raw token retained -> typed decline |
| Q3 | a record's exact bytes | no record bytes/span -> typed decline | no record bytes/span -> typed decline |
| Q4 | the header names | the header names | the header names |
| Q5 | the cell values of a column over a row range | the same values (SQL) | the same values (SQL) |
| Q6 | the number of data rows | the row count | the row count |
| Q7 | a lexical find (matches in a column) | the match count (SQL `instr`) | the match count (SQL `contains`) |
| Q8 | `materialize --exact` (byte-authority) | retained raw BLOB (byte-authority) | **DECLINE** `not-native` (Parquet has no original bytes) |

## Scope (honest)

- **Self-authored deterministic corpus, NOT a real-world population.** Fixtures are generated by `tools/fixtures/make-csv.py` (Python stdlib only). Every claim is scoped to these files; the aggregate carries a fixture-clustered CI.
- **Only Q8 is a byte-authority claim.** `materialize --exact == source` (length + SHA-256 + `cmp`) reproduces the original bytes. Every other observation is a DERIVED projection; semantic agreement is not archival equality.
- **This is where VOLE claims value.** A conventional CSV load is value-oriented: it unquotes fields and drops the raw token, the exact record bytes, and every source span. VOLE's Q2/Q3 expose exactly those distinctions, and its exactness is byte-authoritative for arbitrary CSV/TSV.
- **The baselines' CSV → rows step is inherently lossy** — that is the point of the comparison.
- **DuckDB is a serious columnar comparator.** It answers Q1/Q4/Q5/Q6/Q7 well and will likely win some tabular axes; it provides no exact-source closure or provenance (Q2/Q3/Q8 are typed declines, Q8 `not-native`), and it is not compared as though it carried exactness. Where it beats VOLE, that is recorded, not hidden.
- **Ragged rows diverge by design (the one recorded Q4 mismatch).** On `ragged.csv` DuckDB's loader pads rows to the widest record and auto-names the extra columns (`column3`, `column4`), so its header differs from VOLE/SQLite; VOLE preserves the header and every ragged row verbatim. This is a representation distinction, not a VOLE error, and it is reported as a mismatch rather than hidden.
- **VOLE capability gaps are recorded, never papered over.** Any question VOLE declines is a typed decline (`rc` 6).
- **Nothing here is run on the host.** Every command ran in the pinned `analytical` container.

