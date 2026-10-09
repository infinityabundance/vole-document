# Phase 21.1.3 — XLSX economic court

**Question.** Against a source-retaining SQLite baseline *and* a DuckDB/Parquet analytical baseline, on contract-equivalent terms (the Phase-16+ C0–C5 capability contract extended for spreadsheet coordinates), can VOLE answer the same ten questions (Q1–Q10) it can answer, at comparable build/storage/cold/warm cost, while closing the original workbook byte-exactly?

**Method.** A deterministic self-authored XLSX corpus (`tools/fixtures/make-xlsx.py --corpus`) is regenerated at court time; each fixture is ingested by three lanes (VOLE field CLI; a source-retaining SQLite baseline; a DuckDB/Parquet baseline), Q1–Q10 are asked of each, and build/storage/cold/warm are measured. Persistent bytes are the **sum of regular-file sizes** (`find -type f -printf '%s'`), never `du -sb` (ADR-0049).

Corpus: **8 fixtures**; lanes **vole, sqlite, duckdb**; questions **Q1–Q10**; bootstrap **10000 resamples, seed 21310**, cluster-resampled by fixture; tie band **+/-10%**.

VOLE lane: **release** profile (`target/release/vole-document`); substrate: **--profile runtime --packed (packed seed segments in store/fieldpack/)**. The comparators (SQLite C, DuckDB, Python) are unaffected by the Rust profile while the entropyfs build is not, so the release default keeps the comparison fair to VOLE. All wall times are recorded and reported in **microseconds (`us`)**.

## Verdict

- **VOLE exactness (Q10): 8/8 byte-exact** (length + SHA-256 + `cmp`, after source + descriptor deletion in a fresh process).
- **COURT VERDICT: PASS** (fails unless exactness is 100 % on the VOLE lane for every fixture).

## Build + storage (per lane, per fixture)

Time columns are **microseconds (`us`)** (the raw TSVs carry `us`; nothing is relabelled or rescaled).

| fixture | src B | vole build us | sqlite build us | duckdb build us | vole B | sqlite B | duckdb B |
|---|---:|---:|---:|---:|---:|---:|---:|
| c01-basic.xlsx | 5069 | 26936 | 39062 | 87213 | 19399 | 102548 | 11064 |
| c02-shared-table.xlsx | 17929 | 25420 | 44653 | 95160 | 47242 | 213143 | 31944 |
| c03-multisheet.xlsx | 7581 | 25617 | 39900 | 87235 | 25983 | 110742 | 13753 |
| c04-styles.xlsx | 4056 | 24976 | 39392 | 86331 | 15806 | 106645 | 11150 |
| c05-wide.xlsx | 18282 | 25612 | 47172 | 92602 | 46218 | 254103 | 34319 |
| c06-large.xlsx | 575628 | 32874 | 323495 | 302843 | 1159737 | 5419163 | 988805 |
| c07-merges.xlsx | 4511 | 25163 | 39481 | 86460 | 17887 | 102548 | 10268 |
| c08-external.xlsx | 6330 | 25768 | 38929 | 86426 | 23481 | 102548 | 11669 |

`build us` is the best-of-N (min) of the retained repetitions (microseconds).

## Paired ratios VOLE/other (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 8 | 0.636 | 0.497 | 0.543..0.662 | 0.311..0.649 | 8 | 0 | 0 | 0.347 |
| build | duckdb | 8 | 0.290 | 0.256 | 0.267..0.298 | 0.198..0.296 | 8 | 0 | 0 | 0.230 |
| storage | sqlite | 8 | 0.202 | 0.197 | 0.174..0.229 | 0.177..0.217 | 8 | 0 | 0 | 0.211 |
| storage | duckdb | 8 | 1.610 | 1.578 | 1.347..1.889 | 1.398..1.778 | 0 | 0 | 8 | 1.218 |
| cold | sqlite | 8 | 0.029 | 0.040 | 0.026..0.035 | 0.027..0.079 | 8 | 0 | 0 | 0.077 |
| cold | duckdb | 8 | 0.011 | 0.016 | 0.011..0.014 | 0.011..0.030 | 8 | 0 | 0 | 0.030 |
| warm | sqlite | 8 | 0.697 | 1.113 | 0.688..1.760 | 0.734..1.979 | 5 | 0 | 3 | 4.171 |
| warm | duckdb | 8 | 0.067 | 0.151 | 0.061..0.229 | 0.070..0.479 | 7 | 0 | 1 | 0.927 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` is reported separately and named as such. Each per-fixture lane cost (build, cold, warm) is the **minimum over the retained repetitions** (best-of-N, as for build); storage is measured once after the last build. Every raw sample is kept in `raw/build.tsv`, `raw/cold.tsv`, `raw/warm.tsv`. With only 8 fixture clusters the bootstrap is coarse and is stated as such, not as a precise interval.

The SQLite and DuckDB cold paths run a fresh **Python** process per request, so their cold numbers include the interpreter start-up as part of that lane's honest per-request cost (measured bare start-up 5967 us); VOLE's cold path is a native binary. The cold ratio is therefore dominated by that constant and is reported for completeness, not headlined.

## Per-lane totals (sum over fixtures; times in microseconds `us`)

| lane | build us | storage B | cold us | warm us |
|---|---:|---:|---:|---:|
| vole | 212366 | 1355753 | 178543 | 70853 |
| sqlite | 612084 | 6411440 | 2315566 | 16987 |
| duckdb | 924270 | 1112972 | 5863630 | 76404 |

## What each lane derives and what it declines

| Q | VOLE | SQLite (source-retaining) | DuckDB (columnar) |
|---|---|---|---|
| Q1 | cached cell value (metadata) | cached value (indexed) | cached value (SQL) |
| Q2 | stored formula text | stored formula (indexed) | stored formula (SQL) |
| Q3 | **DECLINE** (no formula evaluation / dependency graph) | lexical dependents (stored-formula text scan) | lexical dependents (SQL scan) |
| Q4 | containing table name(s) | containing table name(s) | containing table name(s) (join) |
| Q5 | resolved cell style (numFmt/font/fill/align) | style via cell_xfs join | style via SQL join |
| Q6 | worksheet part (relationship target) | worksheet part | worksheet part |
| Q7 | **DECLINE** (chart->table linkage not exposed) | chart `<c:f>` range vs table ref (only where a chart exists) | **DECLINE** (no chart data in the projection) |
| Q8 | exact decoded `<c>` bytes + member span | exact decoded `<c>` sha256 + raw span | **DECLINE** (no exact span) |
| Q9 | drawing part decoded bytes | drawing + media member bytes | **DECLINE** (no embedded members) |
| Q10 | `materialize --exact` (byte-authority) | retained source blob (byte-authority) | **DECLINE** `not-native` (labelled blob passthrough only) |

## Comparison normalization (contract-equivalence)

Where two lanes answer the SAME question the comparable value is normalized so the comparison is on the same contract and any projection is explicit (every judgement call is listed so it can be audited):

- **Q1/Q2 (value / formula):** raw string equality of the *cached* value and the *stored* formula text; a missing formula is `null` on both lanes.
- **Q4 (containing table):** the sorted set of table names whose `ref` rectangle contains the cell (empty set when there is no table).
- **Q5 (style):** a normalized tuple `{numFmtId, formatCode, font{bold,italic,size,name}, fill{patternType,fgColor,bgColor}, alignment{horizontal,vertical,wrapText}}`. VOLE resolves a *builtin* `numFmtId` to its ECMA-376 code; the baseline mirrors the same builtin table (`builtin_format_code` in `src/adapter/xlsx.rs`), so `numFmtId` 0 reads `General` on both lanes.
- **Q6 (worksheet relationship):** the resolved part name in canonical leading-slash form (`/xl/worksheets/sheetN.xml`).
- **Q8 (exact XML span):** the SHA-256 of the DECODED `<c>` element bytes (both lanes read the same uncompressed worksheet member); the raw member span is reported as a lane detail and cross-checks equal, but the strong check is the decoded-bytes hash.
- **Q9 (embedded resource):** the SHA-256 of the decoded drawing part (both lanes read the same member); VOLE's inability to expose the media PNG is a recorded decline, not a normalization.
- **Q10 (original bytes):** `{length, sha256}` of the whole workbook.

## Scope (honest)

- **Self-authored deterministic corpus, NOT a real-world population.** The eight workbooks are generated by `tools/fixtures/make-xlsx.py --corpus` (Python stdlib only, fixed-seed LCG). Every claim is scoped to these files; the aggregate carries a fixture-clustered CI and is not extrapolated.
- **Only Q10 is a byte-authority claim.** `materialize --exact == source` (length + SHA-256 + `cmp`) and the retained blob reproduce the original bytes; that is the archival invariant. **Every other observation (Q1–Q9) is a DERIVED projection** of the SpreadsheetML model — the cached value, the stored formula (never evaluated), the style/table/drawing views. Semantic agreement is not archival equality.
- **The XLSX derived-model caveat is live here.** Unlike the PDF-text caveat (irrelevant: no text heuristic is exercised), every non-Q10 answer is a deterministic derived projection and the court compares it as such.
- **Q3 and Q7 are recorded VOLE capability gaps, never equivalences.** VOLE does not evaluate formulas and does not expose a chart's data references; the baselines answer both (lexically / by parsing `<c:f>`).
- **DuckDB is a comparator for columnar/tabular questions.** It answers Q1/Q2/Q4/Q5/Q6 (and Q3 lexically) but provides no exact-source closure or provenance; Q8/Q9/Q10 are typed declines (Q10 `not-native`), and it is not compared as though it carried exactness.
- **Nothing here is run on the host.** Every command ran in the pinned `analytical` container (dev toolchain + python3 + sqlite3 + hash-pinned DuckDB 1.5.6).

