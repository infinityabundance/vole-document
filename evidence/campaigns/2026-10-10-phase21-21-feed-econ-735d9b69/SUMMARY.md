# Phase 21.21 — RSS/Atom economic court — feed economic court

**Question.** Against two conventional comparators — a source-retaining store that keeps the raw bytes plus a conventional extraction, and a conventional decode-to-host-values load — on contract-equivalent terms, can VOLE answer the same question family (Q1–Q12) it can answer, while closing the original feed byte-exactly, and does it add value by **preserving representation** (source spans, exact spelling, attribute/quote/continuation markers, duplicate keys, member order)?

Corpus: **7 fixtures**; lanes **vole, sqlite, conv**; questions **Q1–Q12**; bootstrap **10000 resamples, seed 2121**, cluster-resampled by fixture; tie band **+/-10%**.

VOLE lane: **release** profile (`target/release/vole-document`); substrate: **--profile runtime --packed**.

## Verdict

- **VOLE exactness (Q6): 11/11 byte-exact** (length + SHA-256 + `cmp`, after source + descriptor deletion in a fresh process).
- **COURT VERDICT: PASS**.

## Build + storage (per lane, per fixture)

Time columns are **microseconds (`us`)**.

| fixture | src B | vole build us | sqlite build us | conv build us | vole B | sqlite B | conv B |
|---|---:|---:|---:|---:|---:|---:|---:|
| rss-basic.xml | 408 | 28434 | 29438 | 29687 | 5519 | 8257 | 8648 |
| rss-attrs.xml | 353 | 28250 | 29621 | 29428 | 5409 | 8257 | 8579 |
| rss-cdata.xml | 372 | 28216 | 29084 | 28527 | 5447 | 8257 | 8550 |
| atom-basic.xml | 316 | 28392 | 29911 | 28531 | 5336 | 8258 | 8537 |
| atom-multilink.xml | 334 | 29461 | 31926 | 32015 | 5372 | 8258 | 8583 |
| dup-fields.xml | 301 | 28123 | 27443 | 28049 | 5305 | 8257 | 8560 |
| large.xml | 262339 | 34850 | 35022 | 43378 | 529390 | 614468 | 958322 |

`build us` is the best-of-N (min) of the retained repetitions.

## Paired ratios VOLE/comparator (median + geometric mean, 95% CI by fixture)

| metric | comparator | n | median | geomean | median 95% CI | geomean 95% CI | wins | ties | losses | ratio of sums |
|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|
| build | sqlite | 7 | 0.966 | 0.968 | 0.949..0.995 | 0.947..0.992 | 0 | 7 | 0 | 0.968 |
| build | conv | 7 | 0.960 | 0.945 | 0.920..0.995 | 0.891..0.986 | 1 | 6 | 0 | 0.937 |
| storage | sqlite | 7 | 0.655 | 0.680 | 0.646..0.668 | 0.649..0.737 | 7 | 0 | 0 | 0.846 |
| storage | conv | 7 | 0.626 | 0.618 | 0.620..0.637 | 0.594..0.633 | 7 | 0 | 0 | 0.556 |
| cold | sqlite | 7 | 0.030 | 0.036 | 0.030..0.034 | 0.030..0.051 | 7 | 0 | 0 | 0.041 |
| cold | conv | 7 | 0.031 | 0.036 | 0.029..0.033 | 0.030..0.053 | 7 | 0 | 0 | 0.042 |
| warm | sqlite | 7 | 0.120 | 0.144 | 0.108..0.124 | 0.111..0.228 | 7 | 0 | 0 | 0.415 |
| warm | conv | 7 | 0.120 | 0.151 | 0.114..0.122 | 0.116..0.251 | 7 | 0 | 0 | 0.478 |

A ratio < 1 favours VOLE. The estimator is the **paired per-fixture ratio**, summarised by the median and geometric mean with a fixed-seed cluster bootstrap over fixtures; `ratio of sums` is reported separately and named as such. Each per-fixture lane cost is the **minimum over the retained repetitions** (best-of-N); storage is measured once after the last build. Every raw sample is kept in `raw/build.tsv`, `raw/storage.tsv`, `raw/cold.tsv`, `raw/warm.tsv`.

## Per-lane totals (sum over fixtures; times in microseconds `us`)

| lane | build us | storage B | cold us | warm us |
|---|---:|---:|---:|---:|
| vole | 205726 | 561778 | 76550 | 11121 |
| sqlite | 212445 | 664012 | 1844747 | 26820 |
| conv | 219615 | 1009779 | 1822555 | 23250 |

## What each lane derives and what it declines

| Q | VOLE | source-retaining baseline | conventional load |
|---|---|---|---|
| Q1 | a decoded entry-field value | extracted field text | host value |
| Q2 | an entry-field element's exact source span | no source span -> typed decline | no source span -> typed decline |
| Q3 | a target field's local name | extracted field name | host tag name |
| Q4 | the count of same-named fields in a record | count of extracted same-named fields | count of same-tag children |
| Q5 | a record descriptor (index + ordered field names) | extracted record (ordered fields) | host record |
| Q6 | `materialize --exact` (byte authority) | retained raw BLOB (byte authority) | no source bytes -> typed decline |
| Q7 | `feed-find` over decoded field values (with spans) | scan over extracted field text (no spans) | host-structure walk (no spans) |
| Q8 | a channel field's exact element bytes | no source token -> decline | no source token -> decline |
| Q9 | a field element's attributes (order + spelling + spans) | no attribute spelling -> typed decline | no attribute spelling -> decline |
| Q10 | channel field names in document order | extracted channel field names | host child names |
| Q11 | the recorded dialect (rss/atom) | stored dialect | dialect not recorded -> typed decline |
| Q12 | an entry's ordered field spans | no source span -> typed decline | no source span -> typed decline |

## Scope (honest)

- **Self-authored deterministic corpus, NOT a real-world population.** The fixtures are generated by `tools/fixtures/make-feed.py` (Python stdlib only). Every claim is scoped to these files.
- **Only Q6 is a byte-authority claim.**
- **The conventional load is deliberately the weaker comparator.** It drops spans, exact spelling, duplicate keys, and member/attribute order; a span-preserving loader could in principle match VOLE on those, and no claim is made against one.
- **VOLE capability gaps are recorded, never papered over** — any question VOLE declines is shown as a `capability-gap`.
- **Nothing here is run on the host**; every command ran in a pinned container.

