# Phase 9.3 store court — cohort `phase9`

Cohort: 37 files, source 5579469 B. All standalone and all store-backed roots decoded and `cmp`-ed byte-exact (37/37); closure dangling 0.

## Totals (bytes)

| metric | value |
|---|---:|
| source_bytes | 5579469 |
| standalone_bytes (S) | 3762694 |
| unique_reachable_bytes (U) | 3369900 |
| amortized_bytes_total (A = U) | 3369900 |
| unique_object_bytes | 786501 |
| store_physical_bytes | 786501 |
| gzip9_sum | 1495074 |
| zstd19_sum | 1334444 |
| xz9e_sum | 1307612 |
| brotli11_sum | 1306498 |
| perfile_min_lz_sum | 1304307 |
| cdc_unique_raw | 771383 |
| cdc_unique_zstd (chunk compression on, **non-deterministic**) | 210835–210840 |
| cdc_params | 10,15,11,127 |

`cdc_unique_raw` (`--compression none`) is deterministic (run1 == run2) and is the primary comparison. `cdc_unique_zstd` is **not** deterministic across runs (observed 210,835–210,840 B); the single value printed here in `results.json` is one such run and must not be quoted as a fixed figure.

VOLE unique-reachable vs per-file min LZ: **LOSS**; vs strongest CDC (borg 1.2.4, params 10,15,11,127, compression none): **LOSS**; vs the same CDC with chunk compression (zstd,19): **LOSS**; vs min(both): **LOSS**.

The negative is robust but its size is partly an artifact of candidate selection / externalization granularity: the auto winner emits 0–1 objects per file, whereas forcing `PDF_DEFLATE_REPLAY` (a candidate in the current set) emits one object per deflate stream and lowers the global `U` to 2,360,054 B (per-stratum oracle ~2,537,730 B), flipping `shared-payload` to 264,139 B — a win over *raw* CDC (285,257 B) that still loses to LZ (34,591 B) and CDC+zstd (28,195 B). No current candidate emits more than one object per file. See `docs/evidence/phase9-skeptic-review.md`.

## Per stratum (bytes)

| stratum | files | source | standalone | unique reachable | unique object | per-file min LZ | CDC raw | CDC zstd | vs LZ | vs CDC | vs CDC+zstd |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---|---|
| dup-resource | 1 | 181787 | 56944 | 56984 | 29 | 3454 | 101586 | 11531 | LOSS | WIN | LOSS |
| incremental | 5 | 394305 | 246454 | 246454 | 0 | 6793 | 51192 | 7082 | LOSS | LOSS | LOSS |
| one-changed | 2 | 516111 | 320109 | 320109 | 0 | 14040 | 270984 | 27014 | LOSS | LOSS | LOSS |
| p7 | 6 | 751150 | 362526 | 362577 | 29 | 25042 | 226918 | 31864 | LOSS | LOSS | LOSS |
| reexport | 3 | 266646 | 173863 | 173863 | 0 | 15184 | 96854 | 17854 | LOSS | LOSS | LOSS |
| repeat-bin | 4 | 524288 | 526104 | 133048 | 131072 | 524308 | 140640 | 133863 | WIN | WIN | WIN |
| repeat-pdf | 4 | 483380 | 298720 | 298720 | 0 | 11536 | 82106 | 9196 | LOSS | LOSS | LOSS |
| shared-bin | 5 | 655400 | 657670 | 657870 | 655400 | 655425 | 147253 | 138451 | TIE | LOSS | LOSS |
| shared-payload | 5 | 1290290 | 800210 | 800210 | 0 | 34591 | 285257 | 28199 | LOSS | LOSS | LOSS |
| shifted | 2 | 516112 | 320094 | 320094 | 0 | 13934 | 288854 | 28865 | LOSS | LOSS | LOSS |

WIN = VOLE unique-reachable < baseline (or within 2% for TIE). A store root alone is never compared to a whole file: `standalone_bytes` is the whole-file universe.
