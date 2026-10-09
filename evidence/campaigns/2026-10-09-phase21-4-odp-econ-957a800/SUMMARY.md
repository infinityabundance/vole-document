# Phase 21.4.2 — ODP economic court

**Question.** Against a source-retaining SQLite baseline, on contract-equivalent terms, can VOLE answer the same eight questions (Q1–Q8) it can answer, at comparable build/storage/cold/warm cost, while closing the original presentation byte-exactly?

**Method.** A deterministic self-authored ODP corpus (`tools/fixtures/make-odp.py --corpus`) is regenerated at court time; each fixture is ingested by two lanes (VOLE field CLI; a source-retaining SQLite baseline), Q1–Q8 are asked of each, and build/storage/cold/warm are measured. ODP is not a tabular/analytical format, so there is **no DuckDB comparator** (unlike the XLSX court). Persistent bytes are the **sum of regular-file sizes** (`find -type f -printf '%s'`), never `du -sb` (ADR-0049).

Corpus: **8 fixtures**; lanes **vole, sqlite**; questions **Q1–Q8**; bootstrap **10000 resamples, seed 21423**, cluster-resampled by fixture; tie band **+/-10%**.

VOLE lane: **release** profile (`target/release/vole-document`); substrate: **--profile runtime --packed (packed seed segments in store/fieldpack/)**. The comparator (SQLite C + Python) is unaffected by the Rust profile while the entropyfs build is not, so the release default keeps the comparison fair to VOLE. All wall times are recorded and reported in **microseconds (`us`)**.

## Verdict

- **VOLE exactness (Q8): 8/8 byte-exact** (length + SHA-256 + `cmp`, after source + descriptor deletion in a fresh process).
- **COURT VERDICT: PASS** (fails unless exactness is 100 % on the VOLE lane for every fixture).

## Build + storage (per lane, per fixture)

Time columns are **microseconds (`us`)** (the raw TSVs carry `us`; nothing is relabelled or rescaled).

| fixture | src B | vole build us | sqlite build us | vole B | sqlite B |
|---|---:|---:|---:|---:|---:|
| c01-basic.odp | 526748 | 31836 | 34652 | 1586446 | 1097824 |
| c02-tables.odp | 576419 | 32421 | 35266 | 1734940 | 1196128 |
| c03-media.odp | 923963 | 37008 | 36045 | 2778188 | 1892448 |
| c04-notes.odp | 637526 | 33432 | 35066 | 1918594 | 1319008 |
| c05-order.odp | 739168 | 34855 | 35507 | 2224278 | 1523808 |
| c06-many.odp | 1233324 | 38395 | 39115 | 2475211 | 2531425 |
| c07-mixed.odp | 1744187 | 45615 | 41813 | 3496937 | 3534944 |
| c08-large.odp | 2677100 | 54762 | 49470 | 5362763 | 5480546 |

`build us` is the best-of-N (min) of the retained repetitions (microseconds).

## Paired ratios VOLE/sqlite (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 8 | 0.982 | 0.995 | 0.936..1.091 | 0.952..1.044 | 0 | 7 | 1 | 1.005 |
| storage | sqlite | 8 | 1.448 | 1.256 | 0.979..1.460 | 1.084..1.394 | 0 | 3 | 5 | 1.162 |
| cold | sqlite | 8 | 0.120 | 0.136 | 0.095..0.208 | 0.106..0.183 | 8 | 0 | 0 | 0.149 |
| warm | sqlite | 8 | 0.396 | 0.384 | 0.343..0.423 | 0.354..0.412 | 8 | 0 | 0 | 0.389 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` is reported separately and named as such. Each per-fixture lane cost (build, cold, warm) is the **minimum over the retained repetitions** (best-of-N); storage is measured once after the last build. Every raw sample is kept in `raw/build.tsv`, `raw/cold.tsv`, `raw/warm.tsv`. With only 8 fixture clusters the bootstrap is coarse and is stated as such, not as a precise interval.

The SQLite cold path runs a fresh **Python** process per request, so its cold numbers include interpreter start-up as part of that lane's honest per-request cost (measured bare start-up 4385 us); VOLE's cold path is a native binary. The cold ratio is therefore dominated by that constant and is reported for completeness, not headlined.

## Per-lane totals (sum over fixtures; times in microseconds `us`)

| lane | build us | storage B | cold us | warm us |
|---|---:|---:|---:|---:|
| vole | 308324 | 21577357 | 212092 | 4182 |
| sqlite | 306934 | 18576131 | 1426921 | 10749 |

## What each lane derives and what it declines

| Q | VOLE | SQLite (source-retaining) |
|---|---|---|
| Q1 | slide text (`--odp-slide`, document order) | slide text (`draw:page` document order) |
| Q2 | shape text (`--odp-shape`, flattened pre-order) | shape text (flattened pre-order) |
| Q3 | notes text (`--odp-notes`, slide index) | notes text (`presentation:notes`) |
| Q4 | media decoded bytes (`--odp-media --kind decoded`) | media decoded bytes (extracted BLOB) |
| Q5 | slide structure — shape kinds (`--odp-slide --kind structure`) | slide shape kinds (flattened) |
| Q6 | slide master-page reference (`--odp-slide --kind metadata` `master`) | slide master-page (`draw:master-page-name`) |
| Q7 | slide source span (`--odp-slide --kind metadata` `source_span`) | content.xml member raw span |
| Q8 | `materialize --exact` (byte-authority) | retained source blob (byte-authority) |

## Comparison normalization (contract-equivalence)

Where two lanes answer the SAME question the comparable value is normalized so the comparison is on the same contract and any projection is explicit:

- **Q1/Q2/Q3 (text):** raw string equality of the projected text. The projection is the adapter's exact rule: each shape's text is its `text:p` paragraphs' run text joined by `\n`, a table frame contributes its cells (`\t` between cells, `\n` between rows), and `text_deep` joins a group's own text with its descendants. The baseline mirrors that rule.
- **Q2 (shape index):** the flattened pre-order shape index (groups counted as one node before their children).
- **Q3 (notes):** the notes page attached to the `N`-th slide (a property of this self-authored corpus, stated as such).
- **Q4 (media):** the SHA-256 of the DECODED `Pictures/*` member bytes; the ordinal is the name-sorted media-part index.
- **Q5 (structure):** the sorted set of shape kinds on a slide (recursively including group children) — a structural observation, not rendered geometry.
- **Q6 (master-page reference):** the `draw:master-page-name` of the slide (resolved to a `style:master-page` in the styles part).
- **Q7 (source span):** `[start, end)` of the `content.xml` member record in the SOURCE. ODP slides all live in the *single* content part, so the slide's source span is that part's member span (VOLE's raw-member `source_span` == the baseline's member-record span).
- **Q8 (original bytes):** `{length, sha256}` of the whole presentation.

## Scope (honest)

- **Self-authored deterministic corpus, NOT a real-world population.** The eight decks are generated by `tools/fixtures/make-odp.py --corpus` (Python stdlib only; no `odfpy`; a fixed-state LCG grows each deck's stored PNG to a target size). Every claim is scoped to these files; the aggregate carries a fixture-clustered CI and is not extrapolated.
- **Only Q8 is a byte-authority claim.** `materialize --exact == source` (length + SHA-256 + `cmp`) and the retained blob reproduce the original bytes; that is the archival invariant. **Every other observation (Q1–Q7) is a DERIVED projection** of the OpenDocument presentation model. Semantic agreement is not archival equality.
- **ODP is not tabular.** No DuckDB/Parquet comparator is used here; the required comparator is the source-retaining SQLite baseline. Slide rendering, animation, transitions, and image decoding are out of scope for both lanes.
- **VOLE capability gaps are recorded, never papered over.** Any question VOLE declines is a typed decline (`rc` 6 / explicit code) and appears as a `capability-gap` in the matrix; it is never claimed as equivalence.
- **Nothing here is run on the host.** Every command ran in the pinned `doc-baseline` container (dev toolchain + python3 + sqlite3).

