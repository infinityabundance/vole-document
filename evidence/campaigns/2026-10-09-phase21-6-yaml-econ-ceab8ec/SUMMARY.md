# Phase 21.6 — YAML economic court

**Question.** Against a source-retaining SQLite baseline that runs a conventional YAML → native-object normalization (the offline PyYAML stand-in: expand aliases, merge `<<`, drop tags/comments, normalize styles) and queries SQLite's `json1` functions (SQLite 3.40.1; **JSONB is not used** — it requires SQLite ≥ 3.45), can VOLE answer the same eight questions (Q1–Q8) it can answer, while closing the original YAML byte-exactly — and does it add value by **preserving representation** (spans, the anchor graph, tags, styles, merge keys)?

> **Correction (recorded, not hidden).** This court's baseline uses `json1` only, not `jsonb` (the image's SQLite is 3.40.1). Unlike the *corrected* JSON economic court (which now uses SQLite ≥ 3.45 with JSONB and additionally compares against a span-preserving conventional baseline), this YAML court compares **only** against a normalizing SQLite baseline. Its duplicate-key handling and span/representation "gaps" are therefore stated relative to a normalizing baseline; a span-preserving YAML comparator is a **recorded follow-up**, and the representation claims here should not be read as beating a span-preserving competitor. See the JSON court's correction for what a serious comparator changes.

**Method.** A deterministic self-authored YAML corpus (`tools/fixtures/make-yaml.py --corpus`) is regenerated at court time; each fixture is ingested by two lanes (VOLE field CLI; the source-retaining SQLite baseline), Q1–Q8 are asked of each, and build/storage/cold/warm are measured. Persistent bytes are the **sum of regular-file sizes** (`find -type f -printf '%s'`), never `du -sb` (ADR-0049).

Corpus: **8 fixtures**; lanes **vole, sqlite**; questions **Q1–Q8**; bootstrap **10000 resamples, seed 21621**, cluster-resampled by fixture; tie band **+/-10%**.

VOLE lane: **release** profile (`target/release/vole-document`); substrate: **--profile runtime --packed**. All wall times are **microseconds (`us`)**.

## Verdict

- **VOLE exactness (Q8): 8/8 byte-exact** (length + SHA-256 + `cmp`, after source + descriptor deletion in a fresh process).
- **COURT VERDICT: PASS** (fails unless exactness is 100% on the VOLE lane for every fixture).

## Build + storage (per lane, per fixture)

Time columns are **microseconds (`us`)**.

| fixture | src B | vole build us | sqlite build us | vole B | sqlite B |
|---|---:|---:|---:|---:|---:|
| anchors.yaml | 208 | 29321 | 29194 | 5107 | 8266 |
| tags.yaml | 130 | 28804 | 28200 | 4951 | 8266 |
| multidoc.yaml | 76 | 29145 | 28789 | 4841 | 8264 |
| styles.yaml | 175 | 28923 | 28858 | 5041 | 8265 |
| comments.yaml | 116 | 29695 | 28123 | 4923 | 8265 |
| dup.yaml | 37 | 29125 | 30902 | 4763 | 8264 |
| deep.yaml | 6762 | 29871 | 29898 | 18218 | 12363 |
| large.yaml | 2097250 | 86926 | 565550 | 4199203 | 4206673 |

`build us` is the best-of-N (min) of the retained repetitions.

## Paired ratios VOLE/sqlite (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 8 | 1.003 | 0.795 | 0.942..1.021 | 0.494..1.021 | 1 | 7 | 0 | 0.379 |
| storage | sqlite | 8 | 0.604 | 0.713 | 0.586..0.998 | 0.594..0.903 | 6 | 1 | 1 | 0.995 |
| cold | sqlite | 8 | 0.039 | 0.055 | 0.036..0.046 | 0.037..0.115 | 8 | 0 | 0 | 0.133 |
| warm | sqlite | 8 | 0.725 | 1.125 | 0.612..1.213 | 0.697..2.432 | 6 | 0 | 2 | 9.095 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` is reported separately and named as such. With only 8 fixture clusters the bootstrap is coarse and is stated as such.

The SQLite cold path runs a fresh **Python** process per request, so its cold numbers include interpreter start-up (measured bare start-up 5284 us); VOLE's cold path is a native binary. The cold ratio is dominated by that constant and is reported for completeness, not headlined.

## Per-lane totals (sum over fixtures; times in microseconds `us`)

| lane | build us | storage B | cold us | warm us |
|---|---:|---:|---:|---:|
| vole | 291810 | 4247047 | 185884 | 59237 |
| sqlite | 769514 | 4268626 | 1398882 | 6513 |

## What each lane derives and what it declines

| Q | VOLE | SQLite (conventional YAML → JSON → `json1`) |
|---|---|---|
| Q1 | a scalar at a dotted path (exact token spelling) | `json_extract` value (normalized) |
| Q2 | a node's exact source span | no source span exists -> typed decline |
| Q3 | an anchor and the aliases that target it | aliases are expanded -> typed decline |
| Q4 | a node's literal tag text | tags are resolved and dropped -> typed decline |
| Q5 | the number of documents in the stream | the document list length |
| Q6 | a scalar's style (plain/single/double/literal/folded) | styles are normalized -> typed decline |
| Q7 | a `<<` merge member (surfaced, never merged) | `<<` is merged into the mapping -> typed decline |
| Q8 | `materialize --exact` (byte-authority) | retained raw BLOB (byte-authority) |

## Scope (honest)

- **Self-authored deterministic corpus, NOT a real-world population.** Fixtures are generated by `tools/fixtures/make-yaml.py` (Python stdlib only). Every claim is scoped to these files.
- **Only Q8 is a byte-authority claim.** `materialize --exact == source` (length + SHA-256 + `cmp`) reproduces the original bytes. Every other observation is a DERIVED projection; semantic agreement is not archival equality.
- **This is where VOLE claims value.** A conventional YAML stack is value-oriented: it expands aliases, merges `<<`, and drops tags, styles, comments, and spans. VOLE's Q2/Q3/Q4/Q6/Q7 expose exactly those distinctions, and its exactness is byte-authoritative for arbitrary YAML.
- **The baseline's YAML → native step is inherently lossy** — that is the point of the comparison; the pinned image ships no `yaml` module, so the baseline implements a bounded, conventional normalization inline (never a network fetch).
- **VOLE capability gaps are recorded, never papered over.** Any question VOLE declines is a typed decline (`rc` 6).
- **Nothing here is run on the host.** Every command ran in the pinned `doc-baseline` container.

