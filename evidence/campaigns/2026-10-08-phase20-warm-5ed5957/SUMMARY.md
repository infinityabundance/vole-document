# Phase 20.3 — warm heterogeneous-query court (per-session overhead)

Builds each lane's store **once**, then repeats the WARM one-session heterogeneous query lane **N=100** times per (document, depth, lane) with an interleaved order (odd reps VOLE first, even reps SQLite first) and retains every sample. Same 12-document subset, same contract depths C0..C5, same source-retaining SQLite lane (`tools/fixtures/phase18-contract-packed.py`, byte-identical), same accounting as Phase 19.1. Wall times are integer microseconds.

- documents: **12** (nist-docx-0005, nist-docx-0008, nist-docx-0009, nist-docx-0014, nist-epub-0003, nist-epub-0006, nist-epub-0008, nist-epub-0009, nist-pdf-0002, nist-pdf-0004, nist-pdf-0016, nist-pdf-0017); depths: **C0..C5**; warm repetitions: **N=100**; bootstrap: **20000 resamples, seed 200103**, cluster-resampled by document; tie band **+/-10%**.
- interleaving: odd reps VOLE-then-SQLite, even reps SQLite-then-VOLE (per-rep order in `raw/order.tsv`); one untimed warm-up session per (document, depth, lane) precedes the timed reps, so no timed rep pays a cold-inode first touch.
- every individual warm sample is retained in `raw/warm_samples.tsv`; nothing is reduced to min/median at collection time.

## One-time builds (paid once per document, outside the warm loop)

| lane | depth | n | median ms | max ms |
|---|---|---:|---:|---:|
| VOLE | all | 12 | 13.24 | 1351.59 |
| SQLite | C0 | 12 | 71.36 | 373.77 |
| SQLite | C1 | 12 | 70.46 | 369.06 |
| SQLite | C2 | 12 | 70.10 | 372.91 |
| SQLite | C3 | 12 | 70.45 | 371.27 |
| SQLite | C4 | 12 | 69.97 | 372.21 |
| SQLite | C5 | 12 | 69.86 | 371.83 |

The build cost is paid ONCE here (it was the per-rep cost in Phase 19.1) and is not part of the warm ratio.

## Per-lane warm statistics (pooled over documents x reps)

### Warm one-session wall, every sample (pooled over documents x reps)

| lane | depth | n | median ms | mean ms | p25 ms | p75 ms | min ms | max ms | CV |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| VOLE | C0 | 1200 | 2.307 | 2.636 | 1.496 | 3.606 | 1.016 | 8.428 | 52.5% |
| VOLE | C1 | 1200 | 2.329 | 2.587 | 1.463 | 3.564 | 1.009 | 8.460 | 52.8% |
| VOLE | C2 | 1200 | 2.309 | 2.577 | 1.445 | 3.562 | 1.007 | 6.393 | 53.1% |
| VOLE | C3 | 1200 | 2.321 | 2.574 | 1.440 | 3.543 | 0.985 | 7.628 | 53.3% |
| VOLE | C4 | 1200 | 2.309 | 2.580 | 1.437 | 3.564 | 0.997 | 8.686 | 54.2% |
| VOLE | C5 | 1200 | 2.287 | 2.580 | 1.445 | 3.587 | 1.001 | 6.967 | 53.9% |
| SQLite | C0 | 1200 | 1.603 | 1.788 | 1.410 | 2.091 | 1.255 | 4.719 | 26.8% |
| SQLite | C1 | 1200 | 1.573 | 1.762 | 1.427 | 2.067 | 1.273 | 4.584 | 25.1% |
| SQLite | C2 | 1200 | 1.633 | 1.805 | 1.482 | 2.064 | 1.301 | 5.009 | 23.2% |
| SQLite | C3 | 1200 | 1.621 | 1.805 | 1.484 | 2.080 | 1.290 | 4.919 | 23.6% |
| SQLite | C4 | 1200 | 1.690 | 1.863 | 1.552 | 2.126 | 1.339 | 3.807 | 21.9% |
| SQLite | C5 | 1200 | 1.702 | 1.871 | 1.550 | 2.127 | 1.352 | 4.807 | 23.0% |

Full per (document, lane, depth) statistics: `raw/per_doc_lane_depth.csv`.

## Per-depth VOLE/SQLite warm ratio (paired per rep)

VOLE's packed warm session is depth-independent (the same store and request set serve every depth); the per-depth variation is therefore the SQLite envelope's depth cost. C0 is the first-touch depth worth watching.

| depth | docs | pooled pairs | median ratio | geometric-mean ratio | median-ratio 95% CI |
|---|---:|---:|---:|---:|---|
| C0 | 12 | 1200 | 1.321 | 1.340 | 1.067..1.667 |
| C1 | 12 | 1200 | 1.323 | 1.327 | 1.043..1.683 |
| C2 | 12 | 1200 | 1.273 | 1.284 | 1.001..1.616 |
| C3 | 12 | 1200 | 1.316 | 1.283 | 1.009..1.634 |
| C4 | 12 | 1200 | 1.234 | 1.239 | 0.970..1.569 |
| C5 | 12 | 1200 | 1.170 | 1.236 | 0.972..1.575 |

## Headline — warm heterogeneous query summed over C0..C5 per rep

The Phase-19.1/18.5 headline: per rep, the whole C0..C5 depth schedule is served in ONE session per lane, and the two lanes' session totals are paired by rep index.

### VOLE/SQLite warm session, C0..C5 folded per rep

- documents (clusters): **12**; paired samples: **1200**
- median paired ratio: **1.322**  (95% CI 1.008..1.630)
- geometric-mean paired ratio: **1.282**  (95% CI 1.072..1.531)
- pooled ratio CV (stdev/mean of the paired ratios): **31.6%**; median-CI half-width: **+/-0.311**. Resolution at this N: a true median shift smaller than ~0.311 is not separable from 1.0.
- per-document tie band +/-10%: **2 win / 3 tie / 7 loss** (win = ratio < 0.90, VOLE favourable; loss = ratio > 1.10)

| document | median ratio | samples | outcome |
|---|---:|---:|---|
| nist-docx-0005 | 0.873 | 100 | win |
| nist-docx-0008 | 0.763 | 100 | win |
| nist-docx-0009 | 1.126 | 100 | loss |
| nist-docx-0014 | 1.653 | 100 | loss |
| nist-epub-0003 | 1.647 | 100 | loss |
| nist-epub-0006 | 1.540 | 100 | loss |
| nist-epub-0008 | 1.082 | 100 | tie |
| nist-epub-0009 | 2.223 | 100 | loss |
| nist-pdf-0002 | 0.981 | 100 | tie |
| nist-pdf-0004 | 1.544 | 100 | loss |
| nist-pdf-0016 | 1.013 | 100 | tie |
| nist-pdf-0017 | 1.735 | 100 | loss |

## Before/after — the SAME frozen court, sealed predecessor vs this run

Predecessor campaign: `evidence/campaigns/2026-10-08-phase19-warm-6b66eab`.
Both runs use the identical frozen court and the identical Phase-19.1 estimator (depth-summed per rep, fixed-seed cluster bootstrap over the 12 documents); only the binary and the host epoch differ.

| run | docs | pairs | median ratio (95% CI) | geometric mean (95% CI) | W/T/L |
|---|---:|---:|---|---|---|
| BEFORE | 12 | 1200 | 1.292 (0.989..1.657) | 1.283 (1.068..1.538) | 2 / 3 / 7 |
| AFTER | 12 | 1200 | 1.322 (1.008..1.630) | 1.282 (1.072..1.531) | 2 / 3 / 7 |

| document | BEFORE VOLE/SQLite median | AFTER VOLE/SQLite median | direction |
|---|---:|---:|---|
| nist-docx-0005 | 0.866 | 0.873 | after higher |
| nist-docx-0008 | 0.754 | 0.763 | after higher |
| nist-docx-0009 | 1.130 | 1.126 | after lower |
| nist-docx-0014 | 1.703 | 1.653 | after lower |
| nist-epub-0003 | 1.640 | 1.647 | after higher |
| nist-epub-0006 | 1.535 | 1.540 | after higher |
| nist-epub-0008 | 1.063 | 1.082 | after higher |
| nist-epub-0009 | 2.243 | 2.223 | after lower |
| nist-pdf-0002 | 0.966 | 0.981 | after higher |
| nist-pdf-0004 | 1.605 | 1.544 | after lower |
| nist-pdf-0016 | 1.011 | 1.013 | after higher |
| nist-pdf-0017 | 1.731 | 1.735 | after higher |

A before/after difference in the headline median smaller than the after-run's median-CI half-width is **not** resolved by this court; the half-width is the honest resolution floor at this N.

## Variance floor and minimum detectable effect at N=100

- within-cell between-rep CV (median over all (doc, depth, lane) cells): **6.9%** (p90 13.8%). This is the host bind-mount repeat noise floor at fixed document and store.
- headline pooled paired-ratio CV: **31.6%**; median-ratio 95% CI half-width: **+/-0.311**.
- **minimum detectable effect at this N (80% power, normal approx): ~0.444** (i.e. a true median ratio shift smaller than this is not resolvable with N=100 on this variance floor).

## Exact original closure (length + SHA-256 + byte compare)

| id | fmt | VOLE ok | VOLE rc | VOLE ms | SQLite ok | SQLite rc | SQLite ms |
|---|---|---|---:|---:|---|---:|---:|
| nist-pdf-0002 | pdf | 1 | 0 | 4.6 | 1 | 0 | 27.3 |
| nist-pdf-0004 | pdf | 1 | 0 | 7.2 | 1 | 0 | 26.4 |
| nist-pdf-0016 | pdf | 1 | 0 | 4.5 | 1 | 0 | 26.6 |
| nist-pdf-0017 | pdf | 1 | 0 | 8.7 | 1 | 0 | 26.7 |
| nist-docx-0005 | docx | 1 | 0 | 3.8 | 1 | 0 | 28.3 |
| nist-docx-0008 | docx | 1 | 0 | 3.7 | 1 | 0 | 26.4 |
| nist-docx-0009 | docx | 1 | 0 | 5.1 | 1 | 0 | 27.6 |
| nist-docx-0014 | docx | 1 | 0 | 7.8 | 1 | 0 | 27.1 |
| nist-epub-0003 | epub | 1 | 0 | 3.8 | 1 | 0 | 26.5 |
| nist-epub-0006 | epub | 1 | 0 | 3.8 | 1 | 0 | 27.7 |
| nist-epub-0008 | epub | 1 | 0 | 3.6 | 1 | 0 | 27.2 |
| nist-epub-0009 | epub | 1 | 0 | 7.7 | 1 | 0 | 27.6 |

VOLE `materialize --exact --packed`: **12/12 byte-exact**. SQLite retained blob: **12/12 byte-exact**.

## Equivalence between lanes — warm session (untimed pass)

For every document and depth, the VOLE `observe-batch` JSONL and the SQLite session JSONL of the SAME contract query are compared with the frozen Phase-18 envelope logic (`phase18-contract-packed.py::_equiv`). `raw` = byte/SHA-identity; `projected` = documented text projection; `shape` = both answer but not byte-comparable by design (metadata schema; resource reference vs member bytes; revision below C4); `observable` = the revision observation of different things; `divergent` = PDF page text heuristic; `capability` = one lane declines; `decline` = both decline.

| depth | fmt | obs | raw | projected | shape | observable | divergent | capability | decline | mismatch |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| C0 | pdf | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C0 | pdf | text | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0 |
| C0 | pdf | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C0 | pdf | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C0 | docx | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C0 | docx | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C0 | docx | doc-text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C0 | docx | heading | 1 | 0 | 0 | 0 | 0 | 0 | 3 | 0 |
| C0 | docx | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C0 | docx | resource | 0 | 0 | 1 | 0 | 0 | 0 | 3 | 0 |
| C0 | docx | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C0 | docx | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C0 | epub | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C0 | epub | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C0 | epub | doc-text | 1 | 3 | 0 | 0 | 0 | 0 | 0 | 0 |
| C0 | epub | heading | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C0 | epub | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C0 | epub | resource | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C0 | epub | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C0 | epub | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C1 | pdf | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C1 | pdf | text | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0 |
| C1 | pdf | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C1 | pdf | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C1 | docx | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C1 | docx | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C1 | docx | doc-text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C1 | docx | heading | 1 | 0 | 0 | 0 | 0 | 0 | 3 | 0 |
| C1 | docx | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C1 | docx | resource | 0 | 0 | 1 | 0 | 0 | 0 | 3 | 0 |
| C1 | docx | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C1 | docx | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C1 | epub | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C1 | epub | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C1 | epub | doc-text | 1 | 3 | 0 | 0 | 0 | 0 | 0 | 0 |
| C1 | epub | heading | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C1 | epub | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C1 | epub | resource | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C1 | epub | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C1 | epub | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C2 | pdf | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C2 | pdf | text | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0 |
| C2 | pdf | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C2 | pdf | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C2 | docx | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C2 | docx | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C2 | docx | doc-text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C2 | docx | heading | 1 | 0 | 0 | 0 | 0 | 0 | 3 | 0 |
| C2 | docx | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C2 | docx | resource | 0 | 0 | 1 | 0 | 0 | 0 | 3 | 0 |
| C2 | docx | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C2 | docx | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C2 | epub | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C2 | epub | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C2 | epub | doc-text | 1 | 3 | 0 | 0 | 0 | 0 | 0 | 0 |
| C2 | epub | heading | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C2 | epub | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C2 | epub | resource | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C2 | epub | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C2 | epub | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C3 | pdf | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C3 | pdf | text | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0 |
| C3 | pdf | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C3 | pdf | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C3 | docx | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C3 | docx | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C3 | docx | doc-text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C3 | docx | heading | 1 | 0 | 0 | 0 | 0 | 0 | 3 | 0 |
| C3 | docx | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C3 | docx | resource | 0 | 0 | 1 | 0 | 0 | 0 | 3 | 0 |
| C3 | docx | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C3 | docx | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C3 | epub | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C3 | epub | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C3 | epub | doc-text | 1 | 3 | 0 | 0 | 0 | 0 | 0 | 0 |
| C3 | epub | heading | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C3 | epub | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C3 | epub | resource | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C3 | epub | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C3 | epub | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C4 | pdf | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C4 | pdf | text | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0 |
| C4 | pdf | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C4 | pdf | revision | 0 | 0 | 0 | 4 | 0 | 0 | 0 | 0 |
| C4 | docx | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C4 | docx | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C4 | docx | doc-text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C4 | docx | heading | 1 | 0 | 0 | 0 | 0 | 0 | 3 | 0 |
| C4 | docx | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C4 | docx | resource | 0 | 0 | 1 | 0 | 0 | 0 | 3 | 0 |
| C4 | docx | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C4 | docx | revision | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 |
| C4 | epub | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C4 | epub | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C4 | epub | doc-text | 1 | 3 | 0 | 0 | 0 | 0 | 0 | 0 |
| C4 | epub | heading | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C4 | epub | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C4 | epub | resource | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C4 | epub | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C4 | epub | revision | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 |
| C5 | pdf | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C5 | pdf | text | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0 |
| C5 | pdf | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C5 | pdf | revision | 0 | 0 | 0 | 4 | 0 | 0 | 0 | 0 |
| C5 | docx | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C5 | docx | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C5 | docx | doc-text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C5 | docx | heading | 1 | 0 | 0 | 0 | 0 | 0 | 3 | 0 |
| C5 | docx | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C5 | docx | resource | 0 | 0 | 1 | 0 | 0 | 0 | 3 | 0 |
| C5 | docx | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C5 | docx | revision | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 |
| C5 | epub | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C5 | epub | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C5 | epub | doc-text | 1 | 3 | 0 | 0 | 0 | 0 | 0 | 0 |
| C5 | epub | heading | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C5 | epub | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C5 | epub | resource | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C5 | epub | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C5 | epub | revision | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 |

- observations compared: **480**; **value mismatches: 0**.
- Where the contract defines equality (`raw`/`projected`), the two lanes agree; the non-`raw` cells are the documented shape / observable / heuristic-projection distinctions, not value disagreements.

## Answers

**Is the warm VOLE/SQLite ratio resolved to > 1.0 (a real loss), resolved to <= 1.0 (parity or better), or still including 1.0?**

- Paired median ratio **1.322** (95% CI 1.008..1.630); geometric mean **1.282** (95% CI 1.072..1.531); per-document 2 win / 3 tie / 7 loss.
- The 95% CI lies **entirely above 1.0**: a real warm-session loss for VOLE at this N.

**Exactness / equivalence.** VOLE exact closure **12/12**, SQLite **12/12** byte-exact; warm-session answers compared **480**, value mismatches **0**.

## Caveats and honesty

- This is a **warm-only** court: the one-time store builds are paid once and are not part of the ratio. The build win question is Phase 19.1's and is unchanged.
- The bind-mounted host store is noisy; the between-rep CV and the quoted CI are the honest measure of that noise. If 1.0 remains inside the interval, that is reported, never massaged.
- Warm sessions run in the low-millisecond range, where process start-up is a material fraction of the wall; that start-up is part of BOTH lanes' measured cost and is not subtracted.
- Sampling unit for the CI is the document (cluster bootstrap), not the doc-rep pair; a document's 100 repeated measures are correlated and are not treated as 100 independent documents.
- MDE is a normal-approximation convenience derived from the bootstrap CI half-width; it is an order-of-magnitude resolution statement, not a measured quantity.
- recorded interleave orders: sqlite vole; vole sqlite.

---

# Phase 20.3 — warm-session open profiling (why no safe lever closed the gap)

This section is the profiling evidence behind the Phase-20.3 decision to **ship no
production change**. It was produced by `tools/phase20-warm-profile.sh` (new,
read-only to the frozen courts) and the env-gated open profiler
(`VOLE_PROFILE_OPEN=1`, off by default; a single `getenv` per open on the default
path). Raw output: `raw/profile/`.

## Breakdown of one warm VOLE session (`observe-batch`, packed store, warm cache)

All times are the **median of 5** in-process runs, microseconds. `session_open` is
attributed inside `DocumentFieldSession::open`; the process floor is measured
separately (below).

| document | descriptor B | store open | manifest read | descriptor read+hash | **descriptor parse** | index open | session open | request loop | per-req dispatch |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nist-docx-0008 | 29,264 | 29 | 7 | 13 | **58** | 1 | 121 | 244 | 186 |
| nist-docx-0014 | 948,903 | 31 | 7 | 254 | **1621** | 3 | 1915 | 1010 | 689 |
| nist-pdf-0004 | 1,105,542 | 37 | 5 | 318 | **1861** | 2 | 2248 | 81 | 63 |
| nist-pdf-0017 | 1,466,727 | 135 | 9 | 517 | **2589** | 5 | 3264 | 197 | 169 |
| nist-epub-0009 | 1,237,736 | 44 | 9 | 438 | **2201** | 5 | 2720 | 3631 | 2717 |

Process-start floor (200 runs each, median): `/bin/true` **0.275 ms**, the VOLE
binary with no work **0.390 ms**, `sqlite3 :memory: 'select 1'` **0.620 ms**.
Syscall time is a small fraction: `strace -c` totals ~0.1-1.8 ms per session, so
the cost is CPU (the parse), not I/O (`raw/profile/*.strace.txt`).

## Dominant term

**`Descriptor::parse`** — a linear re-framing + full cross-validation of the
`.voldoc` blob (~1.7 ns/byte, ≈590 MB/s) and the **entire `Field::open`**. It is
1.6-2.6 ms for the 0.9-1.5 MB descriptors and is ~45-50% of a warm session for
the documents that lose; for the winning documents (29 KB descriptor) it is 58 µs.
This is the same one-time full-parse mechanism Phase 16.4 found in the fs-direct
world.

## Candidate levers, each checked rather than assumed

1. **Lazy / partial session open** (open only the manifest; parse on first miss).
   It **cannot** help this contract. `probe_eligible` covers only
   `(Page, Text|Preview|Structure)` and `(Stream, Decoded|Operators)`; the frozen
   schedule also issues `metadata`, `revision`/`revisions`, and (docx/epub)
   `--block 0 --kind text`, `doc-text`, `heading`, `table`, `resource`, none of
   which is probe-eligible. The first such request forces the full parse, so the
   total parse cost is unchanged (docx/epub force it on request #2; pdf on #3).
2. **Avoiding a redundant manifest/descriptor re-read or re-hash when the session
   already holds the field.** There is none: the session opens the field once;
   `observe_session` reuses the already-open `Field`, its parsed manifest, and one
   `FsIndexStore` across every request (the Phase-16.4 hoist).
3. **Hoisting per-request setup.** Already done (manifest, index, probe carry).
4. **Reducing per-request allocation / dispatch.** Dispatch is 63 µs (pdf, 4 obs)
   to 2.7 ms (epub, 8 text-heavy obs): real but bounded work the SQLite lane
   precomputes; it is not the gap driver for the PDFs, and no safe reduction was
   found.
5. **Changing the parse itself** (lazy-decode the advisory `ObservationIndex` /
   `SeekDirectory` / `Checkpoint` records, or skip a validation) lives in
   `src/container/`, is outside this subagent's permitted surface, and would
   weaken decoder authority or change the observation contract. Not attempted.

## Resolution floor

The court's median-ratio 95% CI half-width is **±0.311** and the MDE at N=100 is
**~0.44**. Eliminating the parse entirely would move the median ratio by ≈0.3
(≈0.7 ms on a ≈2.3 ms session) — **below the MDE**, i.e. not resolvable by this
court at N=100 even if fully realised. The before/after above is therefore a
**null**: the warm position is unchanged, and the loss is recorded as a **durable
property** of the resident session under this contract, not tuned away.
