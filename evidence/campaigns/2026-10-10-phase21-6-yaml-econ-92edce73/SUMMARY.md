# Phase 21.6.2 — YAML economic court

**Question.** Against a source-retaining SQLite baseline that runs a conventional YAML → native-object normalization (the offline PyYAML stand-in: expand aliases, merge `<<`, drop tags/comments, normalize styles) and queries SQLite's JSON functions on a hash-pinned modern SQLite (`jsonb`), **and** against a span-preserving pure-Python YAML scanner, can VOLE answer the same eight questions (Q1–Q8) while closing the original YAML byte-exactly — and does it add value by **preserving representation** (spans, the anchor graph, tags, styles, merge keys)?

**Method.** A deterministic self-authored YAML corpus (`tools/fixtures/make-yaml.py --corpus`) is regenerated at court time; each fixture is ingested by **three lanes** (VOLE field CLI; the source-retaining SQLite baseline; a span-preserving pure-Python YAML scanner that retains the source and records every node's exact byte span), Q1–Q8 are asked of each, and build/storage/cold/warm are measured. Persistent bytes are the **sum of regular-file sizes** (`find -type f -printf '%s'`), never `du -sb` (ADR-0049).

Corpus: **8 fixtures**; lanes **vole, sqlite, spanpy**; questions **Q1–Q8**; bootstrap **10000 resamples, seed 21621**, cluster-resampled by fixture; tie band **+/-10%**.

VOLE lane: **release** profile (`target/release/vole-document`); substrate: **--profile runtime --packed**. The comparators (SQLite C + Python, and the pure-Python span-preserving scanner) are unaffected by the Rust profile while the entropyfs build is not, so the release default keeps the comparison fair to VOLE. All wall times are **microseconds (`us`)**.

## Verdict

- **VOLE exactness (Q8): 8/8 byte-exact** (length + SHA-256 + `cmp`, after source + descriptor deletion in a fresh process).
- **COURT VERDICT: PASS** (fails unless exactness is 100% on the VOLE lane for every fixture).

## Build + storage (per lane, per fixture)

Time columns are **microseconds (`us`)**.

| fixture | src B | vole build us | sqlite build us | spanpy build us | vole B | sqlite B | spanpy B |
|---|---:|---:|---:|---:|---:|---:|---:|
| anchors.yaml | 208 | 28198 | 26182 | 20189 | 5107 | 8266 | 1157 |
| tags.yaml | 130 | 27873 | 25541 | 17749 | 4951 | 8265 | 736 |
| multidoc.yaml | 76 | 28013 | 26481 | 18638 | 4841 | 8264 | 725 |
| styles.yaml | 175 | 29131 | 27778 | 18474 | 5041 | 8265 | 815 |
| comments.yaml | 116 | 27988 | 26207 | 17243 | 4923 | 8265 | 549 |
| dup.yaml | 37 | 28389 | 25577 | 17365 | 4763 | 8264 | 652 |
| deep.yaml | 6762 | 27814 | 26698 | 19275 | 18218 | 12363 | 11241 |
| large.yaml | 2097250 | 72466 | 490990 | 1063734 | 4199203 | 5664849 | 16307733 |

`build us` is the best-of-N (min) of the retained repetitions.

## Paired ratios VOLE/comparator (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 8 | 1.063 | 0.836 | 1.042..1.091 | 0.507..1.082 | 1 | 6 | 1 | 0.400 |
| build | spanpy | 8 | 1.537 | 1.039 | 1.397..1.623 | 0.472..1.576 | 1 | 0 | 7 | 0.226 |
| storage | sqlite | 8 | 0.604 | 0.687 | 0.586..0.741 | 0.594..0.864 | 7 | 0 | 1 | 0.742 |
| storage | spanpy | 8 | 6.431 | 3.678 | 1.621..7.305 | 1.571..6.958 | 1 | 0 | 7 | 0.260 |
| cold | sqlite | 8 | 0.041 | 0.058 | 0.039..0.042 | 0.040..0.120 | 8 | 0 | 0 | 0.136 |
| cold | spanpy | 8 | 0.045 | 0.050 | 0.044..0.048 | 0.045..0.059 | 8 | 0 | 0 | 0.073 |
| warm | sqlite | 8 | 0.829 | 1.463 | 0.714..1.188 | 0.775..4.473 | 6 | 0 | 2 | 26.037 |
| warm | spanpy | 8 | 1.471 | 1.879 | 1.250..2.196 | 1.398..2.901 | 1 | 0 | 7 | 5.935 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` is reported separately and named as such. With only 8 fixture clusters the bootstrap is coarse and is stated as such.

## Supersedes — Phase 21.6 (ceab8ec)

`[SUPERSEDED: evidence/campaigns/2026-10-09-phase21-6-yaml-econ-ceab8ec]` — that campaign's SQLite lane ran SQLite **3.40.1** (no `jsonb`) and compared only against a normalizing baseline; it could not answer Q2/Q3/Q4/Q6/Q7 and recorded a span-preserving YAML comparator as a follow-up. This campaign runs a hash-pinned modern SQLite (**3.51.1**, via `pysqlite3`, JSONB=True) **and** adds the span-preserving pure-Python scanner.

| metric | comparator | then (median) | now (median) |
|---|---|---:|---:|
| build | sqlite | [SUPERSEDED: 1.003] | 1.063 |
| storage | sqlite | [SUPERSEDED: 0.604] | 0.604 |
| cold | sqlite | [SUPERSEDED: 0.039] | 0.041 |
| warm | sqlite | [SUPERSEDED: 0.734] | 0.829 |

The headline change: the representation questions this court used to credit VOLE (Q2 span, Q3 anchors, Q4 tags, Q6 styles, Q7 merge keys) are now answered by a *span-preserving conventional YAML scanner* too, so VOLE's representation advantage over a serious competitor **shrinks or vanishes**. Verdict still FAILs unless VOLE exactness is 100%.

Both conventional comparator cold paths (SQLite C + Python, and the pure-Python span-preserving scanner) run a fresh **Python** process per request, so their cold numbers include interpreter start-up as part of that lane's honest per-request cost (measured bare start-up 5133 us); VOLE's cold path is a native binary. The cold ratio is therefore dominated by that constant and is reported for completeness, not headlined.

## Per-lane totals (sum over fixtures; times in microseconds `us`)

| lane | build us | storage B | cold us | warm us |
|---|---:|---:|---:|---:|
| vole | 269872 | 4247047 | 169846 | 47127 |
| sqlite | 675454 | 5726801 | 1245777 | 1810 |
| spanpy | 1192667 | 16323608 | 2338220 | 7941 |

## What each lane derives and what it declines

| Q | VOLE | SQLite (conventional YAML → JSON, modern) | span-preserving Python |
|---|---|---|---|
| Q1 | a scalar at a dotted path (exact token spelling) | `json_extract` value (normalized) | decoded scalar at the dotted path |
| Q2 | a node's exact source span | no source span exists -> typed decline | exact byte span `[start,end)` |
| Q3 | an anchor and the aliases that target it | aliases are expanded -> typed decline | anchor graph (name + alias count), never expanded |
| Q4 | a node's literal tag text | tags are resolved and dropped -> typed decline | literal tag text (or null) |
| Q5 | the number of documents in the stream | the document list length | the document count |
| Q6 | a scalar's style (plain/single/double/literal/folded) | styles are normalized -> typed decline | scalar/container style |
| Q7 | a `<<` merge member (surfaced, never merged) | `<<` is merged into the mapping -> typed decline | `<<` surfaced as a member (kind + match count) |
| Q8 | `materialize --exact` (byte-authority) | retained raw BLOB (byte-authority) | retained raw bytes (byte-authority) |

## Scope (honest)

- **Self-authored deterministic corpus, NOT a real-world population.** Fixtures are generated by `tools/fixtures/make-yaml.py` (Python stdlib only). Every claim is scoped to these files.
- **Only Q8 is a byte-authority claim.** `materialize --exact == source` (length + SHA-256 + `cmp`) reproduces the original bytes. Every other observation is a DERIVED projection; semantic agreement is not archival equality.
- **What survives against the strongest competitor.** Against a *span-preserving* pure-Python YAML scanner, VOLE's representation advantages in Q2 (spans), Q3 (anchor graph), Q4 (tags), Q6 (styles), and Q7 (merge keys) **no longer distinguish it** — the conventional scanner answers them too. VOLE's remaining, real advantages are (i) byte-authoritative exact closure of the whole source (Q8) and (ii) the economics. This is the honest picture the corrected comparator shows.
- **The normalizing SQLite lane's YAML → native step is inherently lossy** — that is the point of the comparison; the pinned image ships no `yaml` module, so both baselines implement a bounded, conventional scanner inline (never a network fetch).
- **VOLE capability gaps are recorded, never papered over.** Any question VOLE declines is a typed decline (`rc` 6 or `rc` 2) and appears as a `capability-gap`, never claimed as equivalence.
- **Nothing here is run on the host.** Every command ran in the pinned `analytical` container (dev toolchain + python3 + hash-pinned modern SQLite).

