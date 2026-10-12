# Phase 29.1 — generalized fused kernel: matrix

Steady-state medians (ns). Ratio is **fused / lane**; **< 1 favours fused**.

| fixture | format | question | fused | pyarrow | duckdb | current | fused/pyarrow | fused/duckdb |
|---|---|---|---:|---:|---:|---:|---:|---:|
| e_small | arrow | id:count | 1002 | 13234 | 498269 | 17623 | 0.076 | 0.002 |
| e_small | arrow | id:sum | 832 | 11981 | 509750 | - | 0.069 | 0.002 |
| e_small | arrow | id:min | 832 | 12774 | 536299 | - | 0.065 | 0.002 |
| e_small | arrow | id:max | 822 | 13004 | 501716 | - | 0.063 | 0.002 |
| e_small | arrow | id:avg | 942 | 11893 | 556750 | - | 0.079 | 0.002 |
| e_small | arrow | id:grouped | 2465 | 31498 | 706969 | - | 0.078 | 0.003 |
| e_small | arrow | score:count | 922 | 13074 | 523566 | 22441 | 0.071 | 0.002 |
| e_small | arrow | score:sum | 831 | 11781 | 521092 | - | 0.071 | 0.002 |
| e_small | arrow | score:min | 821 | 13136 | 543995 | - | 0.062 | 0.002 |
| e_small | arrow | score:max | 812 | 13124 | 516574 | - | 0.062 | 0.002 |
| e_small | arrow | score:avg | 812 | 11711 | 551248 | - | 0.069 | 0.001 |
| e_small | parquet | id:count | 752 | 13276 | 318013 | 18063 | 0.057 | 0.002 |
| e_small | parquet | id:sum | 431 | 11792 | 307884 | - | 0.037 | 0.001 |
| e_small | parquet | id:min | 441 | 13025 | 320658 | - | 0.034 | 0.001 |
| e_small | parquet | id:max | 421 | 13085 | 336227 | - | 0.032 | 0.001 |
| e_small | parquet | id:avg | 561 | 11872 | 311841 | - | 0.047 | 0.002 |
| e_small | parquet | id:grouped | 1954 | 30317 | 497109 | - | 0.064 | 0.004 |
| e_small | parquet | score:count | 781 | 12694 | 321110 | 22873 | 0.062 | 0.002 |
| e_small | parquet | score:sum | 501 | 11552 | 337580 | - | 0.043 | 0.001 |
| e_small | parquet | score:min | 501 | 13506 | 322812 | - | 0.037 | 0.002 |
| e_small | parquet | score:max | 511 | 12854 | 322991 | - | 0.040 | 0.002 |
| e_small | parquet | score:avg | 500 | 11752 | 324755 | - | 0.043 | 0.002 |
| e_small | csv | id:count | 11471 | 13104 | 1123575 | - | 0.875 | 0.010 |
| e_small | csv | id:sum | 11471 | 11742 | 1120440 | - | 0.977 | 0.010 |
| e_small | csv | id:min | 11411 | 12875 | 1120529 | - | 0.886 | 0.010 |
| e_small | csv | id:max | 11401 | 13085 | 1122724 | - | 0.871 | 0.010 |
| e_small | csv | id:avg | 11461 | 11812 | 1140357 | - | 0.970 | 0.010 |
| e_small | csv | id:grouped | 13315 | 30527 | 1344418 | - | 0.436 | 0.010 |
| e_small | csv | score:count | 12763 | 12964 | 1141059 | - | 0.984 | 0.011 |
| e_small | csv | score:sum | 12493 | 11672 | 1140668 | - | 1.070 | 0.011 |
| e_small | csv | score:min | 12814 | 13033 | 1157849 | - | 0.983 | 0.011 |
| e_small | csv | score:max | 12533 | 13104 | 1130628 | - | 0.956 | 0.011 |
| e_small | csv | score:avg | 12634 | 11631 | 1140657 | - | 1.086 | 0.011 |
| e_multi | arrow | k:count | 4839 | 14698 | 567288 | 72546 | 0.329 | 0.009 |
| e_multi | arrow | k:sum | 4809 | 13966 | 564592 | - | 0.344 | 0.009 |
| e_multi | arrow | k:min | 4829 | 15569 | 580033 | - | 0.310 | 0.008 |
| e_multi | arrow | k:max | 4809 | 15300 | 559985 | - | 0.314 | 0.009 |
| e_multi | arrow | k:avg | 5581 | 14246 | 610969 | - | 0.392 | 0.009 |
| e_multi | arrow | k:grouped | 15910 | 100908 | 831760 | - | 0.158 | 0.019 |
| e_multi | parquet | k:count | 1032 | 14948 | 319897 | 69480 | 0.069 | 0.003 |
| e_multi | parquet | k:sum | 882 | 14357 | 310589 | - | 0.061 | 0.003 |
| e_multi | parquet | k:min | 882 | 15509 | 336048 | - | 0.057 | 0.003 |
| e_multi | parquet | k:max | 882 | 15649 | 318815 | - | 0.056 | 0.003 |
| e_multi | parquet | k:avg | 1633 | 14497 | 311631 | - | 0.113 | 0.005 |
| e_multi | parquet | k:grouped | 11642 | 101289 | 556567 | - | 0.115 | 0.021 |
| e_multi | csv | k:count | 158506 | 14849 | 4696290 | - | 10.675 | 0.034 |
| e_multi | csv | k:sum | 158235 | 12512 | 4519109 | - | 12.647 | 0.035 |
| e_multi | csv | k:min | 158366 | 13635 | 4515282 | - | 11.615 | 0.035 |
| e_multi | csv | k:max | 158345 | 13896 | 4583609 | - | 11.395 | 0.035 |
| e_multi | csv | k:avg | 159287 | 12434 | 4540739 | - | 12.811 | 0.035 |
| e_multi | csv | k:grouped | 171039 | 99988 | 4758064 | - | 1.711 | 0.036 |
| e_large | arrow | id:count | 120815 | 43732 | 726855 | 3360919 | 2.763 | 0.166 |
| e_large | arrow | id:sum | 120255 | 41758 | 753024 | - | 2.880 | 0.160 |
| e_large | arrow | id:min | 121387 | 44073 | 688434 | - | 2.754 | 0.176 |
| e_large | arrow | id:max | 120195 | 44655 | 725343 | - | 2.692 | 0.166 |
| e_large | arrow | id:avg | 184935 | 44432 | 679918 | - | 4.162 | 0.272 |
| e_large | arrow | score:count | 123891 | 40976 | 690047 | 4417359 | 3.024 | 0.180 |
| e_large | arrow | score:sum | 120686 | 40056 | 694696 | - | 3.013 | 0.174 |
| e_large | arrow | score:min | 124682 | 133469 | 670991 | - | 0.934 | 0.186 |
| e_large | arrow | score:max | 125394 | 137196 | 674028 | - | 0.914 | 0.186 |
| e_large | arrow | score:avg | 120735 | 40776 | 692732 | - | 2.961 | 0.174 |
| e_large | parquet | id:count | 7504 | 40636 | 362918 | 3041733 | 0.185 | 0.021 |
| e_large | parquet | id:sum | 58399 | 40736 | 362667 | - | 1.434 | 0.161 |
| e_large | parquet | id:min | 58489 | 46567 | 360522 | - | 1.256 | 0.162 |
| e_large | parquet | id:max | 58358 | 46676 | 373316 | - | 1.250 | 0.156 |
| e_large | parquet | id:avg | 124352 | 42720 | 380310 | - | 2.911 | 0.327 |
| e_large | parquet | score:count | 10530 | 41266 | 388184 | 4248334 | 0.255 | 0.027 |
| e_large | parquet | score:sum | 75581 | 41949 | 397000 | - | 1.802 | 0.190 |
| e_large | parquet | score:min | 79468 | 134421 | 439940 | - | 0.591 | 0.181 |
| e_large | parquet | score:max | 79207 | 134892 | 440091 | - | 0.587 | 0.180 |
| e_large | parquet | score:avg | 75561 | 42770 | 425222 | - | 1.767 | 0.178 |
| e_large | csv | id:count | 1763709 | 42820 | 10602354 | - | 41.189 | 0.166 |
| e_large | csv | id:sum | 1764401 | 37611 | 10804601 | - | 46.912 | 0.163 |
| e_large | csv | id:min | 1773538 | 39715 | 10899087 | - | 44.657 | 0.163 |
| e_large | csv | id:max | 1778537 | 39345 | 11283033 | - | 45.204 | 0.158 |
| e_large | csv | id:avg | 3245983 | 64881 | 21147360 | - | 50.030 | 0.153 |
| e_large | csv | score:count | 1932014 | 43421 | 11198294 | - | 44.495 | 0.173 |
| e_large | csv | score:sum | 1908860 | 38562 | 11256512 | - | 49.501 | 0.170 |
| e_large | csv | score:min | 1941641 | 128030 | 10847271 | - | 15.166 | 0.179 |
| e_large | csv | score:max | 1924609 | 126336 | 11251705 | - | 15.234 | 0.171 |
| e_large | csv | score:avg | 1910573 | 36648 | 10900078 | - | 52.133 | 0.175 |
| e_null | arrow | id:count | 261 | 13415 | 531832 | 7524 | 0.019 | 0.000 |
| e_null | arrow | id:sum | 240 | 11762 | 513035 | - | 0.020 | 0.000 |
| e_null | arrow | id:min | 240 | 12675 | 522506 | - | 0.019 | 0.000 |
| e_null | arrow | id:max | 230 | 12713 | 507717 | - | 0.018 | 0.000 |
| e_null | arrow | id:avg | 241 | 11742 | 513929 | - | 0.021 | 0.000 |
| e_null | arrow | id:grouped | 271 | 14467 | 693252 | - | 0.019 | 0.000 |
| e_null | arrow | score:count | 260 | 13074 | 514790 | 7785 | 0.020 | 0.001 |
| e_null | arrow | score:sum | 230 | 11862 | 522786 | - | 0.019 | 0.000 |
| e_null | arrow | score:min | 240 | 12604 | 520322 | - | 0.019 | 0.000 |
| e_null | arrow | score:max | 241 | 12683 | 465429 | - | 0.019 | 0.001 |
| e_null | arrow | score:avg | 240 | 11351 | 519909 | - | 0.021 | 0.000 |
| e_null | parquet | id:count | 300 | 13024 | 338270 | 8887 | 0.023 | 0.001 |
| e_null | parquet | id:sum | 240 | 11611 | 326148 | - | 0.021 | 0.001 |
| e_null | parquet | id:min | 240 | 12744 | 337000 | - | 0.019 | 0.001 |
| e_null | parquet | id:max | 240 | 12894 | 309558 | - | 0.019 | 0.001 |
| e_null | parquet | id:avg | 240 | 11802 | 313595 | - | 0.020 | 0.001 |
| e_null | parquet | id:grouped | 271 | 14757 | 449258 | - | 0.018 | 0.001 |
| e_null | parquet | score:count | 300 | 12954 | 287507 | 9077 | 0.023 | 0.001 |
| e_null | parquet | score:sum | 230 | 11481 | 318955 | - | 0.020 | 0.001 |
| e_null | parquet | score:min | 230 | 12812 | 339894 | - | 0.018 | 0.001 |
| e_null | parquet | score:max | 231 | 12634 | 332410 | - | 0.018 | 0.001 |
| e_null | parquet | score:avg | 240 | 11533 | 302533 | - | 0.021 | 0.001 |
| e_null | csv | id:count | 491 | 13114 | 618194 | - | 0.037 | 0.001 |
| e_null | csv | id:sum | 481 | 11632 | 702509 | - | 0.041 | 0.001 |
| e_null | csv | id:min | 772 | 19346 | 842712 | - | 0.040 | 0.001 |
| e_null | csv | id:max | 480 | 12984 | 915568 | - | 0.037 | 0.001 |
| e_null | csv | id:avg | 891 | 12312 | 863040 | - | 0.072 | 0.001 |
| e_null | csv | id:grouped | 541 | 14758 | 856697 | - | 0.037 | 0.001 |
| e_null | csv | score:count | 581 | 13205 | 637218 | - | 0.044 | 0.001 |
| e_null | csv | score:sum | 541 | 11652 | 621388 | - | 0.046 | 0.001 |
| e_null | csv | score:min | 551 | 12904 | 607064 | - | 0.043 | 0.001 |
| e_null | csv | score:max | 551 | 12893 | 612402 | - | 0.043 | 0.001 |
| e_null | csv | score:avg | 561 | 11483 | 616249 | - | 0.049 | 0.001 |

## Paired ratios fused/comparator (median + geomean, 95%% CI by cluster, MDE)

| comparator | clusters | samples | median | geomean | median 95% CI | geomean 95% CI | MDE | W/T/L |
|---|---:|---:|---:|---:|---|---|---:|---|
| pyarrow | 114 | 570 | 0.0772 | 0.2626 | 0.0635..0.3443 | 0.1694..0.4130 | 0.2007 | 406/41/163 |
| duckdb | 114 | 570 | 0.0034 | 0.0068 | 0.0017..0.0101 | 0.0046..0.0103 | 0.0060 | 570/0/0 |
| current | 14 | 70 | 0.0341 | 0.0243 | 0.0289..0.0390 | 0.0138..0.0382 | 0.0072 | 70/0/0 |

## Large-input gap (e_large) — fused / comparator

| format | question | fused/pyarrow | fused/duckdb |
|---|---|---:|---:|
| arrow | id:avg | 4.168 | 0.274 |
| arrow | id:count | 2.764 | 0.164 |
| arrow | id:max | 2.724 | 0.174 |
| arrow | id:min | 2.731 | 0.179 |
| arrow | id:sum | 2.967 | 0.163 |
| arrow | score:avg | 2.994 | 0.176 |
| arrow | score:count | 2.982 | 0.180 |
| arrow | score:max | 0.920 | 0.192 |
| arrow | score:min | 0.934 | 0.187 |
| arrow | score:sum | 3.026 | 0.173 |
| csv | id:avg | 47.147 | 0.132 |
| csv | id:count | 41.342 | 0.166 |
| csv | id:max | 47.395 | 0.158 |
| csv | id:min | 45.249 | 0.164 |
| csv | id:sum | 47.118 | 0.163 |
| csv | score:avg | 52.229 | 0.176 |
| csv | score:count | 45.163 | 0.176 |
| csv | score:max | 15.179 | 0.172 |
| csv | score:min | 15.204 | 0.178 |
| csv | score:sum | 49.701 | 0.169 |
| parquet | id:avg | 2.918 | 0.330 |
| parquet | id:count | 0.186 | 0.020 |
| parquet | id:max | 1.303 | 0.158 |
| parquet | id:min | 1.259 | 0.162 |
| parquet | id:sum | 1.429 | 0.160 |
| parquet | score:avg | 1.763 | 0.178 |
| parquet | score:count | 0.252 | 0.027 |
| parquet | score:max | 0.585 | 0.177 |
| parquet | score:min | 0.591 | 0.180 |
| parquet | score:sum | 1.808 | 0.190 |

## Fused steady-state distribution (median of the per-cell percentiles, ns)

| format | cells | p50 | p95 | p99 |
|---|---:|---:|---:|---:|
| arrow | 38 | 936 | 1031 | 1043 |
| csv | 38 | 12724 | 13665 | 15209 |
| parquet | 38 | 656 | 750 | 933 |

## Cold (fresh process per question, same warmup+iters; open + read + op), median ns

Cold total process wall, so it folds in field open, source materialization, and
the reader startup that the steady median deliberately excludes.

| lane | median ns |
|---|---:|
| duckdb | 102363758 |
| fused | 1201180 |
| pyarrow | 75098066 |

| comparator | cells | fused/comparator cold median | cold geomean |
|---|---:|---:|---:|
| duckdb | 114 | 0.0140 | 0.0233 |
| pyarrow | 114 | 0.0163 | 0.0329 |

## Correctness: fused vs the oracle (pyarrow/DuckDB), per format × operator

| format | operator | comparisons | disagreements |
|---|---|---:|---:|
| arrow | avg | 14 | 0 |
| arrow | count | 14 | 0 |
| arrow | grouped | 6 | 0 |
| arrow | max | 14 | 0 |
| arrow | min | 14 | 0 |
| arrow | sum | 14 | 0 |
| csv | avg | 14 | 0 |
| csv | count | 14 | 0 |
| csv | grouped | 6 | 0 |
| csv | max | 14 | 0 |
| csv | min | 14 | 0 |
| csv | sum | 14 | 0 |
| parquet | avg | 14 | 0 |
| parquet | count | 14 | 0 |
| parquet | grouped | 6 | 0 |
| parquet | max | 14 | 0 |
| parquet | min | 14 | 0 |
| parquet | sum | 14 | 0 |

## Capability accounting: declines (never a wrong answer)

| lane | format | operator | declines |
|---|---|---|---:|
| — | — | — | 0 (no addressed column fell outside any lane's subset) |

