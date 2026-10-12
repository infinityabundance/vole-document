# Phase 28.5 — fused Arrow predicate/aggregate kernel: matrix

Ratios are **fused / comparator**; **< 1 favours the fused kernel**. Wall time is
microseconds. The headline is the **in-process steady-state** median; cold is separate.
Every lane answers the same logical question Q2 (`count(*)` where numeric column `> K`)
over the **same logical Arrow data**; Q2E additionally returns the matching rows'
coordinates (VOLE: exact source spans; pyarrow: `indices_nonzero`).

## Exactness (fused kernel == shipped path, per fixture)

| fixture | fused count | current count | equal |
|---|---:|---:|---|
| `e_small.arrow` | 99 | 99 | yes |
| `e_multi.arrow` | 499 | 499 | yes |
| `e_large.arrow` | 42553 | 42553 | yes |

## Cross-lane Q2 answer agreement (fused vs each lane)

| lane | equal | mismatch | detail |
|---|---:|---:|---|
| current_inproc | 3 | 0 | e_small.arrow: 99 vs 99; e_multi.arrow: 499 vs 499; e_large.arrow: 42553 vs 42553 |
| current_cli | 3 | 0 | e_small.arrow: 99 vs 99; e_multi.arrow: 499 vs 499; e_large.arrow: 42553 vs 42553 |
| pyarrow | 3 | 0 | e_small.arrow: 99 vs 99; e_multi.arrow: 499 vs 499; e_large.arrow: 42553 vs 42553 |
| duckdb | 3 | 0 | e_small.arrow: 99 vs 99; e_multi.arrow: 499 vs 499; e_large.arrow: 42553 vs 42553 |

## Paired ratios fused/comparator (median + geomean, 95% CI by fixture, MDE)

| metric | comparator | fixtures | samples | median | geomean | median 95% CI | geomean 95% CI | MDE |
|---|---|---:|---:|---:|---:|---|---|---:|
| Q2 | current_inproc | 3 | 27 | 0.0253 | 0.0328 | 0.0000..0.0423 | 0.0000..0.0424 | 0.0302 |
| Q2 | current_cli | 3 | 27 | 0.0046 | 0.0057 | 0.0000..0.0107 | 0.0000..0.0096 | 0.0077 |
| Q2 | pyarrow | 3 | 27 | 0.6000 | 1.9948 | 0.0000..6.4615 | 0.0000..6.4694 | 4.6181 |
| Q2 | duckdb | 3 | 27 | 0.0077 | 0.0358 | 0.0000..0.1670 | 0.0000..0.1657 | 0.1194 |
| Q2E | pyarrow | 3 | 27 | 0.6000 | 1.0312 | 0.0000..1.6607 | 0.0000..1.6627 | 1.1869 |

Timer resolution is 1 us: a fused sample below that floors at 0, so on the two small
fixtures the fused side is sub-microsecond and its ratio floors at 0. The discriminating
fixture is `e_large.arrow`, where both sides are well above the floor.

## Per-fixture steady medians (us) and paired ratios

| fixture | fused | current-inproc | current-cli | pyarrow | duckdb | fused/current-inproc | fused/current-cli | fused/pyarrow | fused/duckdb |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `e_small.arrow` | 0.0 | 16.0 | 754.0 | 4.0 | 377.0 | 0.0000 | 0.0000 | 0.0000 | 0.0000 |
| `e_multi.arrow` | 3.0 | 71.0 | 644.0 | 5.0 | 391.0 | 0.0423 | 0.0047 | 0.6000 | 0.0077 |
| `e_large.arrow` | 84.0 | 3316.0 | 7802.0 | 13.0 | 503.0 | 0.0253 | 0.0108 | 6.4615 | 0.1670 |

## Steady-state percentiles per lane (us)

| fixture | lane | Q | min | p50 | p95 | p99 | n |
|---|---|---|---:|---:|---:|---:|---:|
| `e_small.arrow` | fused | Q2 | 0 | 0.0 | 0.0 | 0.0 | 135 |
| `e_small.arrow` | fused | Q2E | 0 | 0.0 | 0.0 | 0.0 | 135 |
| `e_small.arrow` | current_inproc | Q2 | 16 | 16.0 | 16.0 | 16.0 | 135 |
| `e_small.arrow` | current_inproc | Q2E | 16 | 16.0 | 16.0 | 16.0 | 135 |
| `e_small.arrow` | current_cli | Q2 | 707 | 707.4 | 707.8 | 707.9 | 9 |
| `e_small.arrow` | pyarrow | Q2 | 3 | 3.0 | 3.0 | 3.0 | 135 |
| `e_small.arrow` | pyarrow | Q2E | 3 | 3.0 | 3.0 | 3.0 | 135 |
| `e_small.arrow` | duckdb | Q2 | 276 | 298.8 | 310.3 | 310.3 | 135 |
| `e_multi.arrow` | fused | Q2 | 3 | 3.0 | 3.0 | 3.0 | 135 |
| `e_multi.arrow` | fused | Q2E | 3 | 3.0 | 3.0 | 3.0 | 135 |
| `e_multi.arrow` | current_inproc | Q2 | 70 | 70.0 | 70.0 | 70.0 | 135 |
| `e_multi.arrow` | current_inproc | Q2E | 70 | 70.0 | 70.0 | 70.0 | 135 |
| `e_multi.arrow` | current_cli | Q2 | 594 | 595.6 | 597.1 | 597.2 | 9 |
| `e_multi.arrow` | pyarrow | Q2 | 4 | 4.0 | 4.0 | 4.0 | 135 |
| `e_multi.arrow` | pyarrow | Q2E | 4 | 4.0 | 4.3 | 4.3 | 135 |
| `e_multi.arrow` | duckdb | Q2 | 288 | 304.8 | 315.2 | 315.6 | 135 |
| `e_large.arrow` | fused | Q2 | 82 | 82.0 | 82.0 | 82.0 | 135 |
| `e_large.arrow` | fused | Q2E | 91 | 91.0 | 91.0 | 91.0 | 135 |
| `e_large.arrow` | current_inproc | Q2 | 3241 | 3243.0 | 3246.2 | 3246.6 | 135 |
| `e_large.arrow` | current_inproc | Q2E | 3238 | 3248.7 | 3254.0 | 3254.0 | 135 |
| `e_large.arrow` | current_cli | Q2 | 7649 | 7650.3 | 7651.4 | 7651.5 | 9 |
| `e_large.arrow` | pyarrow | Q2 | 12 | 12.0 | 12.0 | 12.0 | 135 |
| `e_large.arrow` | pyarrow | Q2E | 54 | 54.0 | 54.0 | 54.0 | 135 |
| `e_large.arrow` | duckdb | Q2 | 413 | 414.3 | 415.0 | 415.0 | 135 |

## Cold-process (fresh OS process / fresh connection), Q2, median us

| fixture | lane | cold median us | n |
|---|---|---:|---:|
| `e_small.arrow` | fused | 695.0 | 9 |
| `e_small.arrow` | current_cli | 754.0 | 9 |
| `e_small.arrow` | pyarrow | 56589.0 | 9 |
| `e_small.arrow` | duckdb | 103456.0 | 9 |
| `e_multi.arrow` | fused | 813.0 | 9 |
| `e_multi.arrow` | current_cli | 644.0 | 9 |
| `e_multi.arrow` | pyarrow | 50798.0 | 9 |
| `e_multi.arrow` | duckdb | 93856.0 | 9 |
| `e_large.arrow` | fused | 7355.0 | 9 |
| `e_large.arrow` | current_cli | 7802.0 | 9 |
| `e_large.arrow` | pyarrow | 58030.0 | 9 |
| `e_large.arrow` | duckdb | 110793.0 | 9 |

## Evidence variant Q2E (matching-row coordinates) agreement

| fixture | fused count | fused rows | pyarrow count | pyarrow rows | prefix agrees | truncated |
|---|---:|---:|---:|---:|---|---|
| `e_small.arrow` | 99 | 99 | 99 | 99 | yes | fused=False pyarrow=False |
| `e_multi.arrow` | 499 | 499 | 499 | 499 | yes | fused=False pyarrow=False |
| `e_large.arrow` | 42553 | 1024 | 42553 | 1024 | yes | fused=True pyarrow=True |

