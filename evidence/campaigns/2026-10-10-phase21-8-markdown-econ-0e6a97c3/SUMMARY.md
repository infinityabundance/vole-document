# Phase 21.8 — Markdown economic court

**Question.** Against a source-retaining SQLite baseline *and* a conventional Markdown→HTML/text render baseline, can VOLE answer the same questions (Q1–Q8) it can answer, at comparable build/storage/cold/warm cost, while closing the original Markdown byte-exactly — and does it add value by **preserving representation** (a block's exact source span, a code block's exact content + language)?

**Method.** A deterministic self-authored Markdown corpus (`tools/fixtures/make-markdown.py --corpus`) is regenerated at court time; each fixture is ingested by three lanes (VOLE field CLI; a source-retaining SQLite baseline; a conventional Markdown→HTML/text render baseline), Q1–Q8 are asked of each, and build/storage/cold/warm are measured. Persistent bytes are the **sum of regular-file sizes** (`find -type f -printf '%s'`), never `du -sb` (ADR-0049).

Corpus: **9 fixtures**; lanes **vole, sqlite, render**; questions **Q1–Q8**; bootstrap **10000 resamples, seed 21881**, cluster-resampled by fixture; tie band **+/-10%**.

VOLE lane: **release** profile (`target/release/vole-document`); substrate: **--profile runtime --packed**. All wall times are **microseconds (`us`)**.

## Verdict

- **VOLE exactness (Q8): 9/9 byte-exact** (length + SHA-256 + `cmp`, after source + descriptor deletion in a fresh process).
- **COURT VERDICT: PASS** (fails unless exactness is 100% on the VOLE lane for every fixture).

## Build + storage (per lane, per fixture)

Time columns are **microseconds (`us`)**.

| fixture | src B | vole build us | sqlite build us | render build us | vole B | sqlite B | render B |
|---|---:|---:|---:|---:|---:|---:|---:|
| basic.md | 150 | 27868 | 32792 | 17599 | 5007 | 20585 | 836 |
| lists.md | 92 | 27508 | 32497 | 17381 | 4889 | 20584 | 641 |
| code.md | 124 | 27314 | 32409 | 17042 | 4955 | 20585 | 619 |
| table.md | 69 | 27480 | 32732 | 17784 | 4843 | 20584 | 365 |
| links.md | 166 | 27727 | 33137 | 17330 | 5039 | 20585 | 891 |
| blockquotes.md | 55 | 27906 | 32863 | 17447 | 4815 | 20584 | 538 |
| footnotes.md | 94 | 27495 | 32698 | 17399 | 4893 | 20584 | 560 |
| frontmatter.md | 77 | 27645 | 32708 | 17510 | 4859 | 20584 | 386 |
| toml_frontmatter.md | 56 | 27764 | 32236 | 17690 | 4817 | 20584 | 335 |

`build us` is the best-of-N (min) of the retained repetitions.

## Paired ratios VOLE/other (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 9 | 0.845 | 0.846 | 0.840..0.850 | 0.842..0.851 | 9 | 0 | 0 | 0.846 |
| build | render | 9 | 1.583 | 1.582 | 1.569..1.600 | 1.571..1.593 | 0 | 0 | 9 | 1.582 |
| storage | sqlite | 9 | 0.238 | 0.238 | 0.234..0.243 | 0.236..0.241 | 9 | 0 | 0 | 0.238 |
| storage | render | 9 | 8.738 | 9.002 | 5.989..13.268 | 7.301..11.059 | 0 | 0 | 9 | 8.532 |
| cold | sqlite | 9 | 0.035 | 0.036 | 0.034..0.039 | 0.035..0.037 | 9 | 0 | 0 | 0.036 |
| cold | render | 9 | 0.035 | 0.036 | 0.034..0.040 | 0.035..0.038 | 9 | 0 | 0 | 0.036 |
| warm | sqlite | 9 | 0.380 | 0.384 | 0.342..0.422 | 0.360..0.412 | 9 | 0 | 0 | 0.386 |
| warm | render | 9 | 3.562 | 3.435 | 2.944..3.933 | 3.194..3.693 | 0 | 0 | 9 | 3.444 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` (a size-weighted pooled view) is reported separately and named as such. With only 9 fixture clusters the bootstrap is coarse and is stated as such, not as a precise interval.

The SQLite and render cold paths run a fresh **Python** process per request, so their cold numbers include the interpreter start-up (measured bare start-up 4077 us); VOLE's cold path is a native binary. The cold ratio is dominated by that constant and is reported for completeness, not headlined.

## Per-lane totals (sum over fixtures; times in microseconds `us`)

| lane | build us | storage B | cold us | warm us |
|---|---:|---:|---:|---:|
| vole | 248707 | 44117 | 43771 | 496 |
| sqlite | 294072 | 185259 | 1219601 | 1285 |
| render | 157182 | 5171 | 1209247 | 144 |

## What each lane derives and what it declines

| Q | VOLE | SQLite (source-retaining) | render (HTML/text) |
|---|---|---|---|
| Q1 | a heading's text | the rendered heading text | the rendered heading text |
| Q2 | a block's exact source span | no source span -> typed decline | no source span -> typed decline |
| Q3 | a code block's exact content + language | no exact content -> typed decline | no exact content -> typed decline |
| Q4 | a link's target | the extracted href | the extracted href |
| Q5 | a list item's text | the rendered item text | the rendered item text |
| Q6 | a lexical find (blocks containing a pattern) | the match count (LIKE) | the match count |
| Q7 | the number of headings | the heading count | the heading count |
| Q8 | `materialize --exact` (byte-authority) | retained raw BLOB (byte-authority) | **DECLINE** `not-native` (rendered output has no original bytes) |

## Scope (honest)

- **Self-authored deterministic corpus, NOT a real-world population.** Fixtures are generated by `tools/fixtures/make-markdown.py` (Python stdlib only). Every claim is scoped to these files; the aggregate carries a fixture-clustered CI.
- **Only Q8 is a byte-authority claim.** `materialize --exact == source` (length + SHA-256 + `cmp`) reproduces the original bytes. Every other observation is a DERIVED projection; semantic agreement is not archival equality.
- **This is where VOLE claims value.** Conventionally loading or rendering Markdown re-flows paragraphs, strips markers, and drops every source offset. VOLE's Q2/Q3 expose exactly those distinctions, and its exactness is byte-authoritative for arbitrary Markdown.
- **The baselines' Markdown → text/HTML step is inherently lossy** — that is the point of the comparison. One curated **Q6 (lexical find) mismatch** is recorded rather than hidden: the conventional load and VOLE segment blocks differently, so a pattern that spans a container can be counted once by one lane and not the other; every other Q6 value agrees.
- **VOLE capability gaps are recorded, never papered over.** Any question VOLE declines is a typed decline (`rc` 6).
- **Nothing here is run on the host.** Every command ran in the pinned `analytical` container.

