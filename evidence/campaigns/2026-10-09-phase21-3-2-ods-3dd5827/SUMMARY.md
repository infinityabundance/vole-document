# Phase 21.3.2 — ODS economic court

**Question.** Against a source-retaining SQLite baseline *and* a DuckDB/Parquet analytical baseline, on contract-equivalent terms (the C0–C5 capability contract extended for spreadsheet coordinates), can VOLE answer the same ten questions (Q1–Q10) it can answer, at comparable build/storage/cold/warm cost, while closing the original workbook byte-exactly?

**Method.** A deterministic self-authored ODS corpus (`tools/fixtures/make-ods.py --corpus`) is regenerated at court time; each fixture is ingested by three lanes (VOLE field CLI; a source-retaining SQLite baseline; a DuckDB/Parquet baseline), Q1–Q10 are asked of each, and build/storage/cold/warm are measured. Persistent bytes are the **sum of regular-file sizes** (`find -type f -printf '%s'`), never `du -sb` (ADR-0049).

Corpus: **8 fixtures**; lanes **vole, sqlite, duckdb**; questions **Q1–Q10**; bootstrap **10000 resamples, seed 21320**, cluster-resampled by fixture; tie band **+/-10%**.

VOLE lane: **release** profile (`target/release/vole-document`); substrate: **--profile runtime --packed (packed seed segments in store/fieldpack/)**. The comparators (SQLite C, DuckDB, Python) are unaffected by the Rust profile while the entropyfs build is not, so the release default keeps the comparison fair to VOLE. All wall times are recorded and reported in **microseconds (`us`)**.

## Verdict

- **VOLE exactness (Q10): 8/8 byte-exact** (length + SHA-256 + `cmp`, after source + descriptor deletion in a fresh process).
- **COURT VERDICT: PASS** (fails unless exactness is 100 % on the VOLE lane for every fixture).

## Build + storage (per lane, per fixture)

Time columns are **microseconds (`us`)** (the raw TSVs carry `us`; nothing is relabelled or rescaled).

| fixture | src B | vole build us | sqlite build us | duckdb build us | vole B | sqlite B | duckdb B |
|---|---:|---:|---:|---:|---:|---:|---:|
| c01-basic.ods | 2025 | 448420 | 151361 | 74398 | 11983 | 65684 | 9077 |
| c02-typed.ods | 2530 | 436962 | 143793 | 76522 | 12993 | 73877 | 10046 |
| c03-repeated.ods | 1593 | 114262 | 59042 | 72112 | 11119 | 65682 | 6557 |
| c04-merged.ods | 1600 | 34341 | 34711 | 73841 | 11133 | 65682 | 6486 |
| c05-named.ods | 1739 | 23963 | 33491 | 75436 | 11411 | 65684 | 8928 |
| c06-comments.ods | 1953 | 24491 | 34796 | 80520 | 12429 | 65683 | 8126 |
| c07-styles.ods | 2515 | 24228 | 36179 | 83776 | 12963 | 73877 | 9316 |
| c08-large.ods | 1085182 | 38020 | 2620423 | 2429815 | 2178306 | 13754526 | 2311192 |

`build us` is the best-of-N (min) of the retained repetitions (microseconds).

## Paired ratios VOLE/other (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 8 | 0.852 | 0.734 | 0.670..2.963 | 0.205..1.752 | 4 | 1 | 3 | 0.368 |
| build | duckdb | 8 | 0.391 | 0.570 | 0.289..5.710 | 0.152..1.814 | 5 | 0 | 3 | 0.386 |
| storage | sqlite | 8 | 0.175 | 0.174 | 0.169..0.182 | 0.168..0.180 | 8 | 0 | 0 | 0.159 |
| storage | duckdb | 8 | 1.356 | 1.375 | 1.278..1.696 | 1.201..1.544 | 0 | 1 | 7 | 0.955 |
| cold | sqlite | 8 | 0.034 | 0.075 | 0.032..0.170 | 0.038..0.191 | 7 | 0 | 1 | 0.217 |
| cold | duckdb | 8 | 0.012 | 0.027 | 0.012..0.063 | 0.014..0.065 | 8 | 0 | 0 | 0.077 |
| warm | sqlite | 8 | 0.477 | 0.719 | 0.370..0.707 | 0.424..1.674 | 7 | 0 | 1 | 9.492 |
| warm | duckdb | 8 | 0.022 | 0.050 | 0.017..0.034 | 0.019..0.259 | 7 | 0 | 1 | 1.894 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` is reported separately and named as such. Each per-fixture lane cost (build, cold, warm) is the **minimum over the retained repetitions** (best-of-N, as for build); storage is measured once after the last build. Every raw sample is kept in `raw/build.tsv`, `raw/cold.tsv`, `raw/warm.tsv`. With only 8 fixture clusters the bootstrap is coarse and is stated as such, not as a precise interval.

The SQLite and DuckDB cold paths run a fresh **Python** process per request, so their cold numbers include the interpreter start-up as part of that lane's honest per-request cost (measured bare start-up 4914 us); VOLE's cold path is a native binary. The cold ratio is therefore dominated by that constant and is reported for completeness, not headlined.

## Per-lane totals (sum over fixtures; times in microseconds `us`)

| lane | build us | storage B | cold us | warm us |
|---|---:|---:|---:|---:|
| vole | 1144687 | 2262337 | 425222 | 211469 |
| sqlite | 3113796 | 14230695 | 1962316 | 22278 |
| duckdb | 2966420 | 2369728 | 5497701 | 111658 |

## What each lane derives and what it declines

| Q | VOLE | SQLite (source-retaining) | DuckDB (columnar) |
|---|---|---|---|
| Q1 | typed cell value (metadata) | typed value (indexed) | typed value (SQL) |
| Q2 | stored formula text | stored formula (indexed) | stored formula (SQL) |
| Q3 | **DECLINE** (no formula evaluation / dependency graph) | lexical dependents (stored-formula text scan) | lexical dependents (SQL scan) |
| Q4 | containing named range(s) | containing named range(s) | containing named range(s) (join) |
| Q5 | resolved cell style (automatic/named) | style via styles join | style via SQL join |
| Q6 | content part holding the sheet | content part | content part |
| Q7 | named expressions referencing the sheet | named expressions referencing the sheet | named expressions referencing the sheet (SQL) |
| Q8 | exact decoded cell XML bytes + member span | exact decoded cell sha256 + raw span | **DECLINE** (no exact span) |
| Q9 | **DECLINE** (no ODS-native resource/media selector) | embedded image member bytes (from the retained package) | **DECLINE** (no embedded members) |
| Q10 | `materialize --exact` (byte-authority) | retained source blob (byte-authority) | **DECLINE** `not-native` (labelled blob passthrough only) |

## VOLE ODS/common surface exercised (per fixture, cold)

Every shipped ODS/common selector is driven once per fixture; the `ok` column is `rc == 0`. The common `--resource` selector is expected to decline (ODS exposes no resource/media selector), recorded as a capability gap, not a court failure; `--ods-find` needs `--kind text`.

| surface | ok | decline |
|---|---:|---:|
| cell | 8 | 0 |
| metadata | 8 | 0 |
| ods-cell | 8 | 0 |
| ods-cell-exact | 8 | 0 |
| ods-comments | 8 | 0 |
| ods-find | 8 | 0 |
| ods-named-expressions | 8 | 0 |
| ods-sheet | 8 | 0 |
| ods-styles | 8 | 0 |
| resource | 0 | 8 |
| table | 8 | 0 |
| text | 8 | 0 |

## Comparison normalization (contract-equivalence)

Where two lanes answer the SAME question the comparable value is normalized so the comparison is on the same contract and any projection is explicit (every judgement call is listed so it can be audited):

- **Q1 (typed value):** the object `{type, value}` where `type` is `office:value-type` and `value` is the type's matching attribute (`office:value` for float/percentage/currency, `office:boolean-value` for boolean, `office:date-value` for date/time, `office:string-value` (or the cell text) for string). A cell with no `office:value-type` reads `{null, null}`.
- **Q2 (formula):** the raw `table:formula` text; a missing formula is `null` on every lane.
- **Q4 (containing named range):** the sorted set of named-range names whose `table:cell-range-address` is on the same sheet and contains the cell.
- **Q5 (style):** a normalized object `{name, family, parent, data_style, table_cell_properties, text_properties}`, resolved by the cell's `table:style-name` against the combined automatic + named table-cell styles (automatic preferred on a name clash); no style name reads `null`.
- **Q6 (content part):** the resolved main content part name (in ODS every sheet lives in the single main content part, so this is `content.xml` — the honest ODS structure, not a per-sheet part).
- **Q7 (named-expression reference):** the sorted set of named-expression names whose `table:base-cell-address` or `table:cell-range-address` names the sheet.
- **Q8 (exact cell span):** the SHA-256 of the DECODED `<table:table-cell>` element bytes; both lanes scan the same decompressed content member with the same `<` … matching-`>` rule; the raw member span is a lane detail.
- **Q9 (embedded resource):** the SHA-256 of the decoded embedded image member (the first manifest entry with an `image/*` media type).
- **Q10 (original bytes):** `{length, sha256}` of the whole workbook.

## Scope (honest)

- **Self-authored deterministic corpus, NOT a real-world population.** The eight workbooks are generated by `tools/fixtures/make-ods.py --corpus` (Python stdlib only, fixed-seed LCG). Every claim is scoped to these files; the aggregate carries a fixture-clustered CI and is not extrapolated.
- **Only Q10 is a byte-authority claim.** `materialize --exact == source` (length + SHA-256 + `cmp`) and the retained blob reproduce the original bytes; that is the archival invariant. **Every other observation (Q1–Q9) is a DERIVED projection** of the OpenDocument spreadsheet model — the typed value, the stored formula (never evaluated), the style/named-range/comment/resource views. Semantic agreement is not archival equality.
- **Q3 and Q9 are recorded VOLE capability gaps, never equivalences.** VOLE does not evaluate formulas and exposes no ODS-native resource/media selector; the SQLite baseline answers both (lexically / from the retained package).
- **Baseline durability contract.** The source-retaining SQLite baseline runs `journal_mode=WAL` with `synchronous=NORMAL`: a committed transaction survives a process crash, but WAL is not a media-failure guarantee. The DuckDB lane writes Parquet files directly (no WAL); both are destroyed and re-created at court time.
- **DuckDB is a comparator for columnar/tabular questions.** It answers Q1/Q2/Q4/Q5/Q6/Q7 (and Q3 lexically) but provides no exact-source closure or provenance; Q8/Q9/Q10 are typed declines (Q10 `not-native`), and it is not compared as though it carried exactness.
- **Nothing here is run on the host.** Every command ran in the pinned `analytical` container (dev toolchain + python3 + sqlite3 + hash-pinned DuckDB 1.5.6).

