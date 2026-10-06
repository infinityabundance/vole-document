# Phase 10.1 — encoder-only search governance: court receipt

Commit `d2b09c94432c61de3ff256429fab8d46d38adcdc` (branch `phase10`), run 2026-10-06T04:15:37Z.
Feature under test: **`dsfb-search`** (dependency-free, non-default) with
`deflate-replay` so the replay axis is exercised. Zero decode authority.

## Headline (pre-registered hypotheses)

```json
{"H1_never_worse":true,"H1_failures":[],"H2_approaches_exhaustive":true,"H2_holdout_hits":8,"H2_holdout_total":8,"H3_honest_failure":true,"H3_all_equal":true,"H3_median_benefit_permille":0,"H3_median_overhead_permille":0,"H4_negative_control_raw":true,"H4_failures":[]}
```

Interpretation:

- **H1** — `DsfbGuided.final <= FixedHeuristic.final` on every workload.
- **H2** — on the disjoint **holdout** set, `DsfbGuided.final == Exhaustive.final`
  with `<= 1/2` of Exhaustive's candidates.
- **H3** — the honest negative: the fixed heuristic already attains the exhaustive
  minimum on every workload here, so the parametric space adds **no byte benefit**.
  Both the mechanism and this negative are recorded; nothing is manufactured.
- **H4** — negative controls `Stop(Raw)` and match the RAW descriptor byte-for-byte.

## Zero decode authority

A governor-produced descriptor (`flate.pdf`, winner
`PDF_DEFLATE_REPLAY_RANS`) was decoded by the **default** build (no
`dsfb-search`) and byte-compared:

```json
{
  "default_build": {
    "decode_build": "cargo build --locked (default features: rans,store; NO dsfb-search)",
    "workload": "many.pdf",
    "governed_descriptor": "evidence/scratch/phase10-governor/governed-many.voldoc",
    "descriptor_sha256": "53d17e0a42a7102e423f419d8498352fcbadc95d91f18dfbb74b2ecb66b217bc",
    "source_sha256": "ad307a337c35f18dcd3df833d57c83c7ccf49c294336f81d28f427d6fddbd3bf",
    "decoded_sha256": "ad307a337c35f18dcd3df833d57c83c7ccf49c294336f81d28f427d6fddbd3bf",
    "byte_compare": "identical"
  },
  "capability_build": {
    "decode_build": "cargo build --locked --features deflate-replay (underlying capability; STILL no dsfb-search)",
    "workload": "flate.pdf",
    "governed_descriptor": "evidence/scratch/phase10-governor/governed-flate.voldoc",
    "descriptor_sha256": "6611500296829ff8e767edad17568ca5dbe20db6464cc534bccf7fb91f519228",
    "source_sha256": "3648e1ad820f6f7e4e590030f3ae14ca0d7adc276b73288021e609d83f2ff1ff",
    "decoded_sha256": "3648e1ad820f6f7e4e590030f3ae14ca0d7adc276b73288021e609d83f2ff1ff",
    "byte_compare": "identical"
  },
  "header_references_governor": 0,
  "decode_path_references_governor_or_encode": 0
}
```

## Court table

```
workload             src set      strategy   winner                          final     cands    wall_ms
classic.pdf          329 tune     fixed      RAW                               783         7          0
classic.pdf          329 tune     exhaustive RAW                               783       414         15
classic.pdf          329 tune     guided     RAW                               783         7          0
objstm.pdf           341 tune     fixed      RAW                               795         7          0
objstm.pdf           341 tune     exhaustive RAW                               795       414         15
objstm.pdf           341 tune     guided     RAW                               795         7          0
flate.pdf          57513 tune     fixed      PDF_DEFLATE_REPLAY_RANS         36161        10         37
flate.pdf          57513 tune     exhaustive PDF_DEFLATE_REPLAY_RANS         36161       438       2009
flate.pdf          57513 tune     guided     PDF_DEFLATE_REPLAY_RANS         36161       150        531
xrefstream.pdf       251 holdout  fixed      RAW                               705         5          0
xrefstream.pdf       251 holdout  exhaustive RAW                               705       342         11
xrefstream.pdf       251 holdout  guided     RAW                               705         5          0
incremental.pdf      456 holdout  fixed      BYTE_RANS                         900         7          0
incremental.pdf      456 holdout  exhaustive BYTE_RANS                         900       414         20
incremental.pdf      456 holdout  guided     BYTE_RANS                         900       126          5
mixedeol.pdf         248 holdout  fixed      RAW                               702         7          0
mixedeol.pdf         248 holdout  exhaustive RAW                               702       414         12
mixedeol.pdf         248 holdout  guided     RAW                               702         7          0
trapstream.pdf       250 holdout  fixed      RAW                               704         7          0
trapstream.pdf       250 holdout  exhaustive RAW                               704       414         12
trapstream.pdf       250 holdout  guided     RAW                               704         7          0
many.pdf            9881 holdout  fixed      BYTE_RANS                        5301         7          6
many.pdf            9881 holdout  exhaustive BYTE_RANS                        5301       414        397
many.pdf            9881 holdout  guided     BYTE_RANS                        5301       126        105
bigtext.pdf        65549 holdout  fixed      BYTE_RANS                       38274         7         27
bigtext.pdf        65549 holdout  exhaustive BYTE_RANS                       38274       414       2095
bigtext.pdf        65549 holdout  guided     BYTE_RANS                       38274       126        481
notpdf.bin            41 holdout  fixed      RAW                               495         3          0
notpdf.bin            41 holdout  exhaustive RAW                               495       234          1
notpdf.bin            41 holdout  guided     RAW                               495         3          0
malformed.pdf         85 control  fixed      RAW                               539         3          0
malformed.pdf         85 control  exhaustive RAW                               539       234          2
malformed.pdf         85 control  guided     RAW                               539         3          0
synthetic-rle       8192 tune     fixed      RLE                               428         3          1
synthetic-rle       8192 tune     exhaustive RLE                               428       234        164
synthetic-rle       8192 tune     guided     RLE                               428        30         18
synthetic-rans      9000 tune     fixed      BYTE_RANS                        5568         3          3
synthetic-rans      9000 tune     exhaustive BYTE_RANS                        5568       234        262
synthetic-rans      9000 tune     guided     BYTE_RANS                        5568        54         59
synthetic-raw       8192 holdout  fixed      RAW                              8646         3          2
synthetic-raw       8192 holdout  exhaustive RAW                              8646       234        240
synthetic-raw       8192 holdout  guided     RAW                              8646         3          3
hypotheses: {"H1_never_worse":true,"H1_failures":[],"H2_approaches_exhaustive":true,"H2_holdout_hits":8,"H2_holdout_total":8,"H3_honest_failure":true,"H3_all_equal":true,"H3_median_benefit_permille":0,"H3_median_overhead_permille":0,"H4_negative_control_raw":true,"H4_failures":[]}
```

## Method

- `tools/governor-court.sh` (this file) inside the pinned `dev` container.
- Workloads: deterministic in-memory samples (`src/adapter/pdf/samples.rs`) plus
  a synthetic opaque trio, split into disjoint tune/holdout/control sets before
  measuring. H2 is judged only on the holdout set; no population claim.
- Every candidate reaches the unmodified `court::run`: serialize → parse →
  materialize → byte-compare → complete-cost.
