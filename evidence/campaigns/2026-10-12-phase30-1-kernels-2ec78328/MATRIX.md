# Phase 30.1 — generalized fused kernel: matrix

Steady-state medians (ns). Ratio is **fused / lane**; **< 1 favours fused**.

| fixture | format | question | fused | pyarrow | duckdb | current | fused/pyarrow | fused/duckdb |
|---|---|---|---:|---:|---:|---:|---:|---:|
| e_large | arrow | id:count | 27011 | 40676 | 633982 | 3272072 | 0.664 | 0.043 |
| e_large | arrow | id:sum | 34194 | 38852 | 635104 | - | 0.880 | 0.054 |
| e_large | arrow | id:min | 34213 | 47298 | 639382 | - | 0.723 | 0.054 |
| e_large | arrow | id:max | 34173 | 44533 | 643700 | - | 0.767 | 0.053 |
| e_large | arrow | id:avg | 34193 | 41537 | 645394 | - | 0.823 | 0.053 |
| e_large | arrow | score:count | 27141 | 41117 | 657736 | 4320978 | 0.660 | 0.041 |
| e_large | arrow | score:sum | 25688 | 39824 | 647517 | - | 0.645 | 0.040 |
| e_large | arrow | score:min | 42139 | 135703 | 703793 | - | 0.311 | 0.060 |
| e_large | arrow | score:max | 41247 | 133790 | 648920 | - | 0.308 | 0.064 |
| e_large | arrow | score:avg | 25769 | 42449 | 644432 | - | 0.607 | 0.040 |
| e_large | parquet | id:count | 4709 | 42560 | 356034 | 3007909 | 0.111 | 0.013 |
| e_large | parquet | id:sum | 36298 | 41347 | 362346 | - | 0.878 | 0.100 |
| e_large | parquet | id:min | 36248 | 45786 | 365412 | - | 0.792 | 0.099 |
| e_large | parquet | id:max | 36608 | 46246 | 361795 | - | 0.792 | 0.101 |
| e_large | parquet | id:avg | 36488 | 44002 | 356665 | - | 0.829 | 0.102 |
| e_large | parquet | score:count | 4579 | 40876 | 385899 | 4256017 | 0.112 | 0.012 |
| e_large | parquet | score:sum | 27672 | 39725 | 383815 | - | 0.697 | 0.072 |
| e_large | parquet | score:min | 51346 | 134651 | 430873 | - | 0.381 | 0.119 |
| e_large | parquet | score:max | 50244 | 134230 | 436093 | - | 0.374 | 0.115 |
| e_large | parquet | score:avg | 24576 | 41768 | 386651 | - | 0.588 | 0.064 |
| e_large | csv | id:count | 1561181 | 43852 | 10950321 | - | 35.601 | 0.143 |
| e_large | csv | id:sum | 1551604 | 36849 | 10649450 | - | 42.107 | 0.146 |
| e_large | csv | id:min | 1557855 | 39804 | 10803467 | - | 39.138 | 0.144 |
| e_large | csv | id:max | 1560891 | 40505 | 10868237 | - | 38.536 | 0.144 |
| e_large | csv | id:avg | 1582060 | 40756 | 10783670 | - | 38.818 | 0.147 |
| e_large | csv | score:count | 1349326 | 44212 | 10627879 | - | 30.519 | 0.127 |
| e_large | csv | score:sum | 1372590 | 39334 | 10696327 | - | 34.896 | 0.128 |
| e_large | csv | score:min | 1384351 | 131545 | 10737134 | - | 10.524 | 0.129 |
| e_large | csv | score:max | 1407645 | 143497 | 11747528 | - | 9.810 | 0.120 |
| e_large | csv | score:avg | 1941160 | 63518 | 20526028 | - | 30.561 | 0.095 |

## Paired ratios fused/comparator (median + geomean, 95%% CI by cluster, MDE)

| comparator | clusters | samples | median | geomean | median 95% CI | geomean 95% CI | MDE | W/T/L |
|---|---:|---:|---:|---:|---|---|---:|---|
| pyarrow | 30 | 150 | 0.7915 | 1.9919 | 0.6637..3.6609 | 1.0190..4.0320 | 2.1421 | 99/4/51 |
| duckdb | 30 | 150 | 0.0976 | 0.0752 | 0.0566..0.1181 | 0.0593..0.0928 | 0.0439 | 150/0/0 |
| current | 4 | 20 | 0.0046 | 0.0033 | 0.0013..0.0083 | 0.0015..0.0072 | 0.0050 | 20/0/0 |

## Large-input gap (e_large) — fused / comparator

| format | question | fused/pyarrow | fused/duckdb |
|---|---|---:|---:|
| arrow | id:avg | 0.823 | 0.053 |
| arrow | id:count | 0.663 | 0.043 |
| arrow | id:max | 0.770 | 0.053 |
| arrow | id:min | 0.725 | 0.053 |
| arrow | id:sum | 0.880 | 0.054 |
| arrow | score:avg | 0.607 | 0.040 |
| arrow | score:count | 0.661 | 0.043 |
| arrow | score:max | 0.310 | 0.064 |
| arrow | score:min | 0.309 | 0.067 |
| arrow | score:sum | 0.603 | 0.040 |
| csv | id:avg | 39.253 | 0.147 |
| csv | id:count | 35.635 | 0.142 |
| csv | id:max | 38.474 | 0.147 |
| csv | id:min | 39.138 | 0.145 |
| csv | id:sum | 42.218 | 0.145 |
| csv | score:avg | 29.855 | 0.093 |
| csv | score:count | 30.787 | 0.127 |
| csv | score:max | 10.258 | 0.122 |
| csv | score:min | 10.258 | 0.129 |
| csv | score:sum | 34.939 | 0.128 |
| parquet | id:avg | 0.830 | 0.103 |
| parquet | id:count | 0.111 | 0.014 |
| parquet | id:max | 0.792 | 0.101 |
| parquet | id:min | 0.791 | 0.101 |
| parquet | id:sum | 0.894 | 0.102 |
| parquet | score:avg | 0.588 | 0.064 |
| parquet | score:count | 0.114 | 0.012 |
| parquet | score:max | 0.377 | 0.118 |
| parquet | score:min | 0.384 | 0.119 |
| parquet | score:sum | 0.691 | 0.072 |

## Fused steady-state distribution (median of the per-cell percentiles, ns)

| format | cells | p50 | p95 | p99 |
|---|---:|---:|---:|---:|
| arrow | 10 | 34178 | 34352 | 44426 |
| csv | 10 | 1557640 | 1601603 | 1635659 |
| parquet | 10 | 36323 | 39777 | 44590 |

## Cold (fresh process per question, same warmup+iters; open + read + op), median ns

Cold total process wall, so it folds in field open, source materialization, and
the reader startup that the steady median deliberately excludes.

| lane | median ns |
|---|---:|
| duckdb | 122691264 |
| fused | 7881808 |
| pyarrow | 77630621 |

| comparator | cells | fused/comparator cold median | cold geomean |
|---|---:|---:|---:|
| duckdb | 30 | 0.1051 | 0.1148 |
| pyarrow | 30 | 0.1119 | 0.1938 |

## Correctness: fused vs the oracle (pyarrow/DuckDB), per format × operator

| format | operator | comparisons | disagreements |
|---|---|---:|---:|
| arrow | avg | 4 | 0 |
| arrow | count | 4 | 0 |
| arrow | max | 4 | 0 |
| arrow | min | 4 | 0 |
| arrow | sum | 4 | 0 |
| csv | avg | 4 | 0 |
| csv | count | 4 | 0 |
| csv | max | 4 | 0 |
| csv | min | 4 | 0 |
| csv | sum | 4 | 0 |
| parquet | avg | 4 | 0 |
| parquet | count | 4 | 0 |
| parquet | max | 4 | 0 |
| parquet | min | 4 | 0 |
| parquet | sum | 4 | 0 |

## Capability accounting: declines (never a wrong answer)

| lane | format | operator | declines |
|---|---|---|---:|
| — | — | — | 0 (no addressed column fell outside any lane's subset) |

