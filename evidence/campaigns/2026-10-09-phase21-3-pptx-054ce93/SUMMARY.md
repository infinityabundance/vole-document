# Phase 21.2.3 — PPTX economic court

**Question.** Against a source-retaining SQLite baseline, on contract-equivalent terms, can VOLE answer the same eight questions (Q1–Q8) it can answer, at comparable build/storage/cold/warm cost, while closing the original presentation byte-exactly?

**Method.** A deterministic self-authored PPTX corpus (`tools/fixtures/make-pptx.py --corpus`) is regenerated at court time; each fixture is ingested by two lanes (VOLE field CLI; a source-retaining SQLite baseline), Q1–Q8 are asked of each, and build/storage/cold/warm are measured. PPTX is not a tabular/analytical format, so there is **no DuckDB comparator** (unlike the XLSX court). Persistent bytes are the **sum of regular-file sizes** (`find -type f -printf '%s'`), never `du -sb` (ADR-0049).

Corpus: **8 fixtures**; lanes **vole, sqlite**; questions **Q1–Q8**; bootstrap **10000 resamples, seed 21323**, cluster-resampled by fixture; tie band **+/-10%**.

VOLE lane: **release** profile (`target/release/vole-document`); substrate: **--profile runtime --packed (packed seed segments in store/fieldpack/)**. The comparator (SQLite C + Python) is unaffected by the Rust profile while the entropyfs build is not, so the release default keeps the comparison fair to VOLE. All wall times are recorded and reported in **microseconds (`us`)**.

## Verdict

- **VOLE exactness (Q8): 8/8 byte-exact** (length + SHA-256 + `cmp`, after source + descriptor deletion in a fresh process).
- **COURT VERDICT: PASS** (fails unless exactness is 100 % on the VOLE lane for every fixture).

## Build + storage (per lane, per fixture)

Time columns are **microseconds (`us`)** (the raw TSVs carry `us`; nothing is relabelled or rescaled).

| fixture | src B | vole build us | sqlite build us | vole B | sqlite B |
|---|---:|---:|---:|---:|---:|
| c01-basic.pptx | 512649 | 32619 | 35768 | 1038514 | 1089670 |
| c02-tables.pptx | 514236 | 31478 | 36120 | 1042532 | 1089670 |
| c03-media.pptx | 1233659 | 40439 | 41220 | 2480959 | 2531462 |
| c04-notes.pptx | 764056 | 35613 | 40618 | 1545126 | 1585287 |
| c05-charts.pptx | 514602 | 31590 | 37011 | 1043686 | 1089670 |
| c06-order.pptx | 539054 | 31950 | 38852 | 1093012 | 1138822 |
| c07-many.pptx | 1488311 | 44064 | 48102 | 2994061 | 3055752 |
| c08-mixed.pptx | 2445880 | 52648 | 49835 | 4909199 | 4952199 |

`build us` is the best-of-N (min) of the retained repetitions (microseconds).

## Paired ratios VOLE/sqlite (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 8 | 0.894 | 0.909 | 0.854..0.981 | 0.865..0.960 | 4 | 4 | 0 | 0.917 |
| storage | sqlite | 8 | 0.967 | 0.969 | 0.957..0.980 | 0.960..0.978 | 0 | 8 | 0 | 0.977 |
| cold | sqlite | 8 | 0.096 | 0.116 | 0.083..0.177 | 0.090..0.156 | 8 | 0 | 0 | 0.127 |
| warm | sqlite | 8 | 0.445 | 0.408 | 0.350..0.475 | 0.331..0.476 | 8 | 0 | 0 | 0.396 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` is reported separately and named as such. Each per-fixture lane cost (build, cold, warm) is the **minimum over the retained repetitions** (best-of-N); storage is measured once after the last build. Every raw sample is kept in `raw/build.tsv`, `raw/cold.tsv`, `raw/warm.tsv`. With only 8 fixture clusters the bootstrap is coarse and is stated as such, not as a precise interval.

The SQLite cold path runs a fresh **Python** process per request, so its cold numbers include interpreter start-up as part of that lane's honest per-request cost (measured bare start-up 4988 us); VOLE's cold path is a native binary. The cold ratio is therefore dominated by that constant and is reported for completeness, not headlined.

## Per-lane totals (sum over fixtures; times in microseconds `us`)

| lane | build us | storage B | cold us | warm us |
|---|---:|---:|---:|---:|
| vole | 300401 | 16147089 | 200821 | 4376 |
| sqlite | 327526 | 16532532 | 1575622 | 11058 |

## What each lane derives and what it declines

| Q | VOLE | SQLite (source-retaining) |
|---|---|---|
| Q1 | slide text (presentation-order index) | slide text (sldIdLst order) |
| Q2 | shape text (`--pptx-shape`, flattened pre-order) | shape text (flattened pre-order) |
| Q3 | notes text (`--pptx-notes`) | notes text (via `notesSlide` relationship) |
| Q4 | media decoded bytes (`--pptx-media --kind decoded`) | media decoded bytes (extracted BLOB) |
| Q5 | chart relationship id(s) on a slide (`--slide --kind structure`) | chart relationship id(s) on a slide |
| Q6 | slide part (relationship target) | slide part + resolved layout/media/notes targets |
| Q7 | slide member source span (`--slide --kind metadata` `source_span`) | slide member raw span (`find` of the member record) |
| Q8 | `materialize --exact` (byte-authority) | retained source blob (byte-authority) |

## Comparison normalization (contract-equivalence)

Where two lanes answer the SAME question the comparable value is normalized so the comparison is on the same contract and any projection is explicit (every judgement call is listed so it can be audited):

- **Q1/Q2/Q3 (text):** raw string equality of the projected text. The projection is the adapter's exact rule: each shape's text is its runs concatenated within a paragraph and paragraphs joined by `\n`; a table frame contributes its cells (`\t` between cells, `\n` between rows); `text_deep` joins a group's own text with its descendants. The baseline mirrors that rule.
- **Q2 (shape index):** the flattened pre-order shape index (groups counted as one node before their children) — the same order `shape_by_flat_index` uses.
- **Q3 (notes):** the corpus numbers its `notesSlideN.xml` parts so the `N`-th notes part (name-sorted, as `--pptx-notes` resolves them) is exactly the notes part linked from the `N`-th presentation slide; the baseline resolves the same part through the slide's `notesSlide` relationship. This alignment is a property of this self-authored corpus, stated as such.
- **Q4 (media):** the SHA-256 of the DECODED media member bytes (`--kind decoded` == `zipfile.read`); the media ordinal is the name-sorted part index (the adapter sorts media parts by name).
- **Q5 (chart reference):** the sorted set of chart relationship ids referenced by the slide's shapes (`--slide --kind structure` exposes the shape `chart` field; the baseline scans the slide's `<c:chart r:id>`); the resolved chart part name is a lane detail. **This is not chart data.**
- **Q6 (slide relationship):** the resolved slide part name in canonical `/`-rooted form (`/ppt/slides/slideN.xml`); the layout/media/notes targets are lane details.
- **Q7 (source span):** `[start, end)` of the slide's ZIP member record in the SOURCE (VOLE's raw-member `source_span` == the baseline's member-record span). This is a byte-address observation of the source, not a decoded-XML digest. **Judgement call:** the shipped VOLE surface exposes decoded member bytes only for media (`--pptx-media --kind decoded`); it does **not** return a slide/shape's decoded XML (the slide selector supports `text`/`structure`/`metadata` only). The court therefore compares the contract-equivalent observable both lanes expose — the slide member's source span — and records the *decoded slide XML digest* as a VOLE capability gap it does not claim.
- **Q8 (original bytes):** `{length, sha256}` of the whole presentation.

## Scope (honest)

- **Self-authored deterministic corpus, NOT a real-world population.** The eight decks are generated by `tools/fixtures/make-pptx.py --corpus` (Python stdlib only; no `python-pptx`; a fixed-state LCG grows each deck's PNG to a target size). Every claim is scoped to these files; the aggregate carries a fixture-clustered CI and is not extrapolated.
- **Only Q8 is a byte-authority claim.** `materialize --exact == source` (length + SHA-256 + `cmp`) and the retained blob reproduce the original bytes; that is the archival invariant. **Every other observation (Q1–Q7) is a DERIVED projection** of the PresentationML model. Semantic agreement is not archival equality.
- **PPTX is not tabular.** No DuckDB/Parquet comparator is used here; the required comparator is the source-retaining SQLite baseline. Chart *data* and image *decoding* are out of scope for both lanes (Q5 is a chart reference, Q4 is opaque resource bytes).
- **VOLE capability gaps are recorded, never papered over.** Any question VOLE declines is a typed decline (`rc` 6 / explicit code) and appears as a `capability-gap` in the matrix; it is never claimed as equivalence. Concretely, VOLE's shipped surface exposes no **decoded slide/shape XML digest** (only media supports `--kind decoded`), so no such digest is compared; where a question's feature is absent from both decks the row is a `both-decline`.
- **Nothing here is run on the host.** Every command ran in the pinned `doc-baseline` container (dev toolchain + python3 + sqlite3 + poppler).

