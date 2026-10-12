# Phase 29.1 — generalized fused kernel: matrix

Steady-state medians (ns). Ratio is **fused / lane**; **< 1 favours fused**.

| fixture | format | question | fused | pyarrow | duckdb | current | fused/pyarrow | fused/duckdb |
|---|---|---|---:|---:|---:|---:|---:|---:|
| e_small | arrow | id:count | 391 | 13104 | 469135 | 17573 | 0.030 | 0.001 |
| e_small | arrow | id:sum | 380 | 12032 | 506936 | - | 0.032 | 0.001 |
| e_small | arrow | id:min | 370 | 12954 | 487039 | - | 0.029 | 0.001 |
| e_small | arrow | id:max | 370 | 12975 | 477230 | - | 0.029 | 0.001 |
| e_small | arrow | id:avg | 351 | 11923 | 482090 | - | 0.029 | 0.001 |
| e_small | arrow | id:grouped | 2034 | 31258 | 659561 | - | 0.065 | 0.003 |
| e_small | arrow | score:count | 391 | 13164 | 481378 | 22332 | 0.030 | 0.001 |
| e_small | arrow | score:sum | 361 | 11692 | 503229 | - | 0.031 | 0.001 |
| e_small | arrow | score:min | 391 | 13014 | 483582 | - | 0.030 | 0.001 |
| e_small | arrow | score:max | 391 | 13836 | 477460 | - | 0.028 | 0.001 |
| e_small | arrow | score:avg | 360 | 11792 | 481568 | - | 0.031 | 0.001 |
| e_small | parquet | id:count | 561 | 14356 | 293047 | 17753 | 0.039 | 0.002 |
| e_small | parquet | id:sum | 321 | 11892 | 295712 | - | 0.027 | 0.001 |
| e_small | parquet | id:min | 321 | 14397 | 307765 | - | 0.022 | 0.001 |
| e_small | parquet | id:max | 330 | 13556 | 311572 | - | 0.024 | 0.001 |
| e_small | parquet | id:avg | 321 | 12994 | 297926 | - | 0.025 | 0.001 |
| e_small | parquet | id:grouped | 1763 | 31279 | 451382 | - | 0.056 | 0.004 |
| e_small | parquet | score:count | 591 | 14407 | 290272 | 22643 | 0.041 | 0.002 |
| e_small | parquet | score:sum | 321 | 12774 | 292756 | - | 0.025 | 0.001 |
| e_small | parquet | score:min | 331 | 12965 | 315348 | - | 0.026 | 0.001 |
| e_small | parquet | score:max | 331 | 12994 | 312523 | - | 0.025 | 0.001 |
| e_small | parquet | score:avg | 321 | 11842 | 307534 | - | 0.027 | 0.001 |
| e_small | csv | id:count | 10390 | 13345 | 1099151 | - | 0.779 | 0.009 |
| e_small | csv | id:sum | 10369 | 11782 | 1109790 | - | 0.880 | 0.009 |
| e_small | csv | id:min | 10239 | 13595 | 1093841 | - | 0.753 | 0.009 |
| e_small | csv | id:max | 10319 | 13025 | 1101224 | - | 0.792 | 0.009 |
| e_small | csv | id:avg | 10339 | 12182 | 1088310 | - | 0.849 | 0.010 |
| e_small | csv | id:grouped | 12142 | 31669 | 1284987 | - | 0.383 | 0.009 |
| e_small | csv | score:count | 10510 | 13034 | 1104801 | - | 0.806 | 0.010 |
| e_small | csv | score:sum | 10429 | 11692 | 1097087 | - | 0.892 | 0.010 |
| e_small | csv | score:min | 10449 | 12914 | 1092919 | - | 0.809 | 0.010 |
| e_small | csv | score:max | 10350 | 13205 | 1102527 | - | 0.784 | 0.009 |
| e_small | csv | score:avg | 10439 | 11641 | 1087840 | - | 0.897 | 0.010 |
| e_multi | arrow | k:count | 1693 | 14687 | 534387 | 71975 | 0.115 | 0.003 |
| e_multi | arrow | k:sum | 1783 | 14267 | 540509 | - | 0.125 | 0.003 |
| e_multi | arrow | k:min | 1743 | 15539 | 538525 | - | 0.112 | 0.003 |
| e_multi | arrow | k:max | 1713 | 15529 | 547231 | - | 0.110 | 0.003 |
| e_multi | arrow | k:avg | 1783 | 14207 | 542973 | - | 0.126 | 0.003 |
| e_multi | arrow | k:grouped | 13295 | 100548 | 781428 | - | 0.132 | 0.017 |
| e_multi | parquet | k:count | 711 | 14968 | 297525 | 69169 | 0.048 | 0.002 |
| e_multi | parquet | k:sum | 611 | 14367 | 299890 | - | 0.043 | 0.002 |
| e_multi | parquet | k:min | 611 | 15600 | 322752 | - | 0.039 | 0.002 |
| e_multi | parquet | k:max | 611 | 15789 | 311631 | - | 0.039 | 0.002 |
| e_multi | parquet | k:avg | 631 | 14657 | 295431 | - | 0.043 | 0.002 |
| e_multi | parquet | k:grouped | 11963 | 101579 | 516113 | - | 0.118 | 0.023 |
| e_multi | csv | k:count | 143488 | 14447 | 4412142 | - | 9.932 | 0.033 |
| e_multi | csv | k:sum | 143547 | 12844 | 4459922 | - | 11.176 | 0.032 |
| e_multi | csv | k:min | 143758 | 13685 | 4439443 | - | 10.505 | 0.032 |
| e_multi | csv | k:max | 143918 | 13676 | 4441837 | - | 10.523 | 0.032 |
| e_multi | csv | k:avg | 143628 | 12764 | 4433281 | - | 11.253 | 0.032 |
| e_multi | csv | k:grouped | 154919 | 101760 | 4701071 | - | 1.522 | 0.033 |
| e_large | arrow | id:count | 27130 | 41257 | 647177 | 3247038 | 0.658 | 0.042 |
| e_large | arrow | id:sum | 34274 | 38812 | 660723 | - | 0.883 | 0.052 |
| e_large | arrow | id:min | 34324 | 44403 | 663699 | - | 0.773 | 0.052 |
| e_large | arrow | id:max | 34344 | 44764 | 655032 | - | 0.767 | 0.052 |
| e_large | arrow | id:avg | 34294 | 41788 | 666984 | - | 0.821 | 0.051 |
| e_large | arrow | score:count | 26680 | 40837 | 627431 | 4284193 | 0.653 | 0.043 |
| e_large | arrow | score:sum | 25738 | 39694 | 658649 | - | 0.648 | 0.039 |
| e_large | arrow | score:min | 41959 | 134471 | 658749 | - | 0.312 | 0.064 |
| e_large | arrow | score:max | 41287 | 133119 | 648400 | - | 0.310 | 0.064 |
| e_large | arrow | score:avg | 25818 | 40045 | 627420 | - | 0.645 | 0.041 |
| e_large | parquet | id:count | 4669 | 43050 | 354031 | 3029071 | 0.108 | 0.013 |
| e_large | parquet | id:sum | 36728 | 40436 | 356265 | - | 0.908 | 0.103 |
| e_large | parquet | id:min | 36358 | 46146 | 354011 | - | 0.788 | 0.103 |
| e_large | parquet | id:max | 36679 | 45696 | 374278 | - | 0.803 | 0.098 |
| e_large | parquet | id:avg | 36528 | 43451 | 360443 | - | 0.841 | 0.101 |
| e_large | parquet | score:count | 3867 | 40816 | 384427 | 4134554 | 0.095 | 0.010 |
| e_large | parquet | score:sum | 24275 | 41768 | 396780 | - | 0.581 | 0.061 |
| e_large | parquet | score:min | 44684 | 135423 | 430684 | - | 0.330 | 0.104 |
| e_large | parquet | score:max | 50184 | 133389 | 435944 | - | 0.376 | 0.115 |
| e_large | parquet | score:avg | 28063 | 40916 | 389276 | - | 0.686 | 0.072 |
| e_large | csv | id:count | 1565672 | 42840 | 10649680 | - | 36.547 | 0.147 |
| e_large | csv | id:sum | 1556985 | 36738 | 10624564 | - | 42.381 | 0.147 |
| e_large | csv | id:min | 1561453 | 37831 | 10631807 | - | 41.274 | 0.147 |
| e_large | csv | id:max | 1556784 | 38772 | 10581804 | - | 40.152 | 0.147 |
| e_large | csv | id:avg | 1559249 | 39754 | 10624103 | - | 39.222 | 0.147 |
| e_large | csv | score:count | 1318079 | 43211 | 10523685 | - | 30.503 | 0.125 |
| e_large | csv | score:sum | 1333257 | 38231 | 10751591 | - | 34.874 | 0.124 |
| e_large | csv | score:min | 1334469 | 128399 | 10879970 | - | 10.393 | 0.123 |
| e_large | csv | score:max | 1337405 | 127167 | 11037233 | - | 10.517 | 0.121 |
| e_large | csv | score:avg | 1332637 | 38121 | 10636115 | - | 34.958 | 0.125 |
| e_null | arrow | id:count | 210 | 13024 | 489944 | 7574 | 0.016 | 0.000 |
| e_null | arrow | id:sum | 190 | 11942 | 468825 | - | 0.016 | 0.000 |
| e_null | arrow | id:min | 181 | 12995 | 490204 | - | 0.014 | 0.000 |
| e_null | arrow | id:max | 190 | 12964 | 479906 | - | 0.015 | 0.000 |
| e_null | arrow | id:avg | 200 | 11782 | 471069 | - | 0.017 | 0.000 |
| e_null | arrow | id:grouped | 230 | 14778 | 653439 | - | 0.016 | 0.000 |
| e_null | arrow | score:count | 201 | 13024 | 465859 | 7825 | 0.015 | 0.000 |
| e_null | arrow | score:sum | 181 | 11631 | 478162 | - | 0.016 | 0.000 |
| e_null | arrow | score:min | 180 | 12753 | 471569 | - | 0.014 | 0.000 |
| e_null | arrow | score:max | 181 | 12814 | 457794 | - | 0.014 | 0.000 |
| e_null | arrow | score:avg | 181 | 11521 | 473283 | - | 0.016 | 0.000 |
| e_null | parquet | id:count | 300 | 13045 | 300551 | 8937 | 0.023 | 0.001 |
| e_null | parquet | id:sum | 250 | 11912 | 291634 | - | 0.021 | 0.001 |
| e_null | parquet | id:min | 240 | 13054 | 298167 | - | 0.018 | 0.001 |
| e_null | parquet | id:max | 241 | 13185 | 292446 | - | 0.018 | 0.001 |
| e_null | parquet | id:avg | 250 | 11893 | 298998 | - | 0.021 | 0.001 |
| e_null | parquet | id:grouped | 281 | 16190 | 431195 | - | 0.017 | 0.001 |
| e_null | parquet | score:count | 300 | 13024 | 318514 | 8927 | 0.023 | 0.001 |
| e_null | parquet | score:sum | 250 | 11992 | 297325 | - | 0.021 | 0.001 |
| e_null | parquet | score:min | 240 | 12774 | 300420 | - | 0.019 | 0.001 |
| e_null | parquet | score:max | 241 | 12654 | 296223 | - | 0.019 | 0.001 |
| e_null | parquet | score:avg | 241 | 11842 | 284721 | - | 0.020 | 0.001 |
| e_null | csv | id:count | 451 | 13345 | 587055 | - | 0.034 | 0.001 |
| e_null | csv | id:sum | 451 | 11883 | 581124 | - | 0.038 | 0.001 |
| e_null | csv | id:min | 461 | 12794 | 596123 | - | 0.036 | 0.001 |
| e_null | csv | id:max | 451 | 13435 | 581134 | - | 0.034 | 0.001 |
| e_null | csv | id:avg | 470 | 11872 | 580863 | - | 0.040 | 0.001 |
| e_null | csv | id:grouped | 511 | 14547 | 733920 | - | 0.035 | 0.001 |
| e_null | csv | score:count | 470 | 12934 | 578840 | - | 0.036 | 0.001 |
| e_null | csv | score:sum | 451 | 11542 | 591764 | - | 0.039 | 0.001 |
| e_null | csv | score:min | 451 | 12744 | 588057 | - | 0.035 | 0.001 |
| e_null | csv | score:max | 461 | 12704 | 586614 | - | 0.036 | 0.001 |
| e_null | csv | score:avg | 461 | 11822 | 584511 | - | 0.039 | 0.001 |

## Paired ratios fused/comparator (median + geomean, 95%% CI by cluster, MDE)

| comparator | clusters | samples | median | geomean | median 95% CI | geomean 95% CI | MDE | W/T/L |
|---|---:|---:|---:|---:|---|---|---:|---|
| pyarrow | 114 | 570 | 0.0461 | 0.1683 | 0.0367..0.1263 | 0.1095..0.2616 | 0.0640 | 489/8/81 |
| duckdb | 114 | 570 | 0.0023 | 0.0047 | 0.0011..0.0093 | 0.0033..0.0069 | 0.0059 | 570/0/0 |
| current | 14 | 70 | 0.0229 | 0.0133 | 0.0084..0.0270 | 0.0071..0.0221 | 0.0133 | 70/0/0 |

## Large-input gap (e_large) — fused / comparator

| format | question | fused/pyarrow | fused/duckdb |
|---|---|---:|---:|
| arrow | id:avg | 0.819 | 0.052 |
| arrow | id:count | 0.658 | 0.043 |
| arrow | id:max | 0.767 | 0.053 |
| arrow | id:min | 0.773 | 0.052 |
| arrow | id:sum | 0.885 | 0.052 |
| arrow | score:avg | 0.645 | 0.041 |
| arrow | score:count | 0.653 | 0.043 |
| arrow | score:max | 0.312 | 0.065 |
| arrow | score:min | 0.313 | 0.064 |
| arrow | score:sum | 0.639 | 0.039 |
| csv | id:avg | 39.172 | 0.147 |
| csv | id:count | 36.835 | 0.146 |
| csv | id:max | 40.152 | 0.147 |
| csv | id:min | 41.366 | 0.147 |
| csv | id:sum | 42.362 | 0.146 |
| csv | score:avg | 34.498 | 0.124 |
| csv | score:count | 30.452 | 0.125 |
| csv | score:max | 10.514 | 0.121 |
| csv | score:min | 10.405 | 0.123 |
| csv | score:sum | 34.874 | 0.124 |
| parquet | id:avg | 0.845 | 0.101 |
| parquet | id:count | 0.109 | 0.013 |
| parquet | id:max | 0.803 | 0.097 |
| parquet | id:min | 0.818 | 0.103 |
| parquet | id:sum | 0.902 | 0.103 |
| parquet | score:avg | 0.688 | 0.072 |
| parquet | score:count | 0.093 | 0.010 |
| parquet | score:max | 0.378 | 0.114 |
| parquet | score:min | 0.329 | 0.102 |
| parquet | score:sum | 0.587 | 0.062 |

## Fused steady-state distribution (median of the per-cell percentiles, ns)

| format | cells | p50 | p95 | p99 |
|---|---:|---:|---:|---:|
| arrow | 38 | 391 | 488 | 528 |
| csv | 38 | 10449 | 11223 | 12758 |
| parquet | 38 | 450 | 521 | 532 |

## Cold (fresh process per question, same warmup+iters; open + read + op), median ns

Cold total process wall, so it folds in field open, source materialization, and
the reader startup that the steady median deliberately excludes.

| lane | median ns |
|---|---:|
| duckdb | 82742132 |
| fused | 913048 |
| pyarrow | 66353470 |

| comparator | cells | fused/comparator cold median | cold geomean |
|---|---:|---:|---:|
| duckdb | 114 | 0.0116 | 0.0203 |
| pyarrow | 114 | 0.0138 | 0.0284 |

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

