# real100-v1 frontier map

Documents measured: **100**. Lanes: VOLE (frozen), SQLite/FTS (A1), direct tooling (A0). Tie band: ±10% of the fastest median. `decline` = the lane has no such observation for the format / returned a typed error.

## Answered vs declined (all ops)

| lane | answered | declined |
|---|---:|---:|
| VOLE | 455 | 245 |
| SQLite/FTS | 506 | 194 |
| direct tooling | 508 | 192 |

### Overall

| stratum | workload | VOLE | SQLite/FTS | direct tooling | fastest |
|---|---|---|---|---|---|
| all | text_once | loss | win | loss | SQLite/FTS |
| all | text_repeat | loss | win | loss | SQLite/FTS |
| all | heading | loss | win | loss | SQLite/FTS |
| all | table | loss | win | loss | SQLite/FTS |
| all | resource | loss | win | loss | SQLite/FTS |
| all | metadata | loss | win | loss | SQLite/FTS |
| all | exact | loss | loss | win | direct tooling |

### By format

| stratum | workload | VOLE | SQLite/FTS | direct tooling | fastest |
|---|---|---|---|---|---|
| pdf | text_once | loss | win | loss | SQLite/FTS |
| pdf | text_repeat | win | tie | loss | VOLE |
| pdf | heading | - | - | - | — |
| pdf | table | - | - | - | — |
| pdf | resource | - | - | - | — |
| pdf | metadata | loss | win | loss | SQLite/FTS |
| pdf | exact | loss | loss | win | direct tooling |
| docx | text_once | loss | win | loss | SQLite/FTS |
| docx | text_repeat | loss | win | loss | SQLite/FTS |
| docx | heading | loss | win | loss | SQLite/FTS |
| docx | table | loss | win | loss | SQLite/FTS |
| docx | resource | loss | win | loss | SQLite/FTS |
| docx | metadata | loss | win | loss | SQLite/FTS |
| docx | exact | loss | loss | win | direct tooling |
| epub | text_once | loss | win | loss | SQLite/FTS |
| epub | text_repeat | loss | win | loss | SQLite/FTS |
| epub | heading | loss | win | loss | SQLite/FTS |
| epub | table | loss | win | loss | SQLite/FTS |
| epub | resource | loss | win | loss | SQLite/FTS |
| epub | metadata | loss | win | loss | SQLite/FTS |
| epub | exact | loss | loss | win | direct tooling |

### By size class

| stratum | workload | VOLE | SQLite/FTS | direct tooling | fastest |
|---|---|---|---|---|---|
| 1-10MiB | text_once | loss | win | loss | SQLite/FTS |
| 1-10MiB | text_repeat | win | tie | loss | VOLE |
| 1-10MiB | heading | loss | win | loss | SQLite/FTS |
| 1-10MiB | table | loss | win | loss | SQLite/FTS |
| 1-10MiB | resource | loss | win | loss | SQLite/FTS |
| 1-10MiB | metadata | loss | win | loss | SQLite/FTS |
| 1-10MiB | exact | loss | loss | win | direct tooling |
| 10-50MiB | text_once | loss | win | loss | SQLite/FTS |
| 10-50MiB | text_repeat | loss | win | loss | SQLite/FTS |
| 10-50MiB | heading | loss | win | loss | SQLite/FTS |
| 10-50MiB | table | loss | win | loss | SQLite/FTS |
| 10-50MiB | resource | loss | win | loss | SQLite/FTS |
| 10-50MiB | metadata | loss | win | loss | SQLite/FTS |
| 10-50MiB | exact | loss | loss | win | direct tooling |
| 100KiB-1MiB | text_once | loss | win | loss | SQLite/FTS |
| 100KiB-1MiB | text_repeat | loss | win | loss | SQLite/FTS |
| 100KiB-1MiB | heading | loss | win | loss | SQLite/FTS |
| 100KiB-1MiB | table | loss | win | loss | SQLite/FTS |
| 100KiB-1MiB | resource | loss | win | loss | SQLite/FTS |
| 100KiB-1MiB | metadata | loss | win | loss | SQLite/FTS |
| 100KiB-1MiB | exact | loss | loss | win | direct tooling |
| 50-100MiB | text_once | loss | win | loss | SQLite/FTS |
| 50-100MiB | text_repeat | loss | win | loss | SQLite/FTS |
| 50-100MiB | heading | - | - | - | — |
| 50-100MiB | table | - | - | - | — |
| 50-100MiB | resource | - | - | - | — |
| 50-100MiB | metadata | loss | win | loss | SQLite/FTS |
| 50-100MiB | exact | loss | loss | win | direct tooling |
| <100KiB | text_once | loss | win | loss | SQLite/FTS |
| <100KiB | text_repeat | win | tie | loss | VOLE |
| <100KiB | heading | loss | win | loss | SQLite/FTS |
| <100KiB | table | loss | win | loss | SQLite/FTS |
| <100KiB | resource | win | loss | loss | VOLE |
| <100KiB | metadata | loss | win | loss | SQLite/FTS |
| <100KiB | exact | loss | loss | win | direct tooling |
| >100MiB | text_once | loss | win | loss | SQLite/FTS |
| >100MiB | text_repeat | tie | win | loss | SQLite/FTS |
| >100MiB | heading | - | - | - | — |
| >100MiB | table | - | - | - | — |
| >100MiB | resource | - | - | - | — |
| >100MiB | metadata | loss | win | loss | SQLite/FTS |
| >100MiB | exact | loss | loss | win | direct tooling |

### One-time costs (build/ingest, per document)

| id | format | size | VOLE encode ms | VOLE ingest ms | VOLE bytes | A1 build ms | A1 db bytes |
|---|---|---|---:|---:|---:|---:|---:|
| nasa-epub-0001 | epub | 1-10MiB | 119 | 307 | 10243395 | 124 | 9383936 |
| nasa-epub-0002 | epub | 10-50MiB | 536 | 773 | 50247420 | 128 | 22065152 |
| nasa-epub-0003 | epub | 1-10MiB | 101 | 200 | 7971544 | 51 | 4026368 |
| nasa-epub-0004 | epub | 1-10MiB | 324 | 301 | 10584204 | 57 | 5337088 |
| nasa-epub-0005 | epub | 1-10MiB | 120 | 263 | 10794100 | 53 | 5517312 |
| nasa-epub-0006 | epub | 10-50MiB | 266 | 567 | 23849433 | 112 | 13516800 |
| nasa-epub-0007 | epub | 10-50MiB | 1207 | 1692 | 97632529 | 182 | 47693824 |
| nasa-epub-0008 | epub | 10-50MiB | 651 | 1134 | 54100005 | 127 | 26001408 |
| nasa-epub-0009 | epub | 10-50MiB | 2031 | 1416 | 82309579 | 154 | 40054784 |
| nasa-epub-0010 | epub | 10-50MiB | 1381 | 1441 | 92590347 | 27 | 0 |
| nasa-epub-0011 | epub | 1-10MiB | 603 | 832 | 22875361 | 140 | 14790656 |
| nasa-epub-0012 | epub | 10-50MiB | 1007 | 1034 | 91539976 | 135 | 35094528 |
| nasa-epub-0013 | epub | 1-10MiB | 150 | 255 | 11604524 | 52 | 5734400 |
| nasa-epub-0014 | epub | 1-10MiB | 133 | 220 | 8457554 | 116 | 7942144 |
| nasa-epub-0015 | epub | 1-10MiB | 146 | 258 | 11166004 | 63 | 5615616 |
| nasa-pdf-eb-01 | pdf | 1-10MiB | 1236 | 4788 | 25328940 | 6364 | 10489856 |
| nasa-pdf-eb-02 | pdf | 10-50MiB | 1526 | 2316 | 49076643 | 2089 | 23789568 |
| nasa-pdf-eb-03 | pdf | 1-10MiB | 1793 | 783 | 6748742 | 269 | 2138112 |
| nasa-pdf-eb-04 | pdf | 1-10MiB | 3614 | 814 | 10358963 | 548 | 3948544 |
| nasa-pdf-eb-05 | pdf | 1-10MiB | 2393 | 853 | 6782112 | 241 | 2125824 |
| nasa-pdf-eb-06 | pdf | 1-10MiB | 390 | 2456 | 20132649 | 2627 | 8409088 |
| nasa-pdf-eb-07 | pdf | 10-50MiB | 6482 | 10494 | 109814322 | 1820 | 41668608 |
| nasa-pdf-eb-08 | pdf | 10-50MiB | 1724 | 2066 | 77245477 | 2037 | 35909632 |
| nasa-pdf-eb-09 | pdf | 10-50MiB | 3345 | 2390 | 31244043 | 1800 | 11776000 |
| nasa-pdf-eb-10 | pdf | 10-50MiB | 1545 | 3519 | 41795882 | 1720 | 20025344 |
| nasa-pdf-0001 | pdf | >100MiB | 1773 | -1 | 0 | 9297 | 420708352 |
| nasa-pdf-0002 | pdf | >100MiB | 180026 | -1 | 0 | 19918 | 330067968 |
| nasa-pdf-0003 | pdf | >100MiB | 180041 | -1 | 0 | 14061 | 238088192 |
| nasa-pdf-0004 | pdf | 50-100MiB | 94111 | 10406 | 179120665 | 4653 | 82190336 |
| nasa-pdf-0005 | pdf | 50-100MiB | 4493 | 2866 | 128384767 | 416 | 62504960 |
| nasa-pdf-0006 | pdf | 10-50MiB | 1076 | 1354 | 43905615 | 331 | 20557824 |
| nasa-pdf-0007 | pdf | 1-10MiB | 1487 | 1004 | 23028128 | 308 | 10502144 |
| nasa-pdf-0008 | pdf | 1-10MiB | 504 | 1265 | 14821040 | 277 | 6262784 |
| nasa-pdf-0009 | pdf | 1-10MiB | 237 | 925 | 9474639 | 248 | 3538944 |
| nasa-pdf-0010 | pdf | 1-10MiB | 164 | 608 | 6619580 | 153 | 2457600 |
| nasa-pdf-0011 | pdf | 1-10MiB | 204 | 710 | 7711845 | 186 | 2908160 |
| nasa-pdf-0012 | pdf | 50-100MiB | 8357 | 12489 | 180666283 | 4934 | 77893632 |
| nasa-pdf-0013 | pdf | 50-100MiB | 19444 | 7861 | 186313329 | 1377 | 90951680 |
| nasa-pdf-0014 | pdf | 50-100MiB | 20242 | 24668 | 183841417 | 25298 | 74096640 |
| nasa-pdf-0015 | pdf | 10-50MiB | 5626 | 2887 | 89833052 | 587 | 45846528 |
| nasa-pdf-0016 | pdf | 10-50MiB | 8029 | 44009 | 86669826 | 8756 | 26701824 |
| nasa-pdf-0017 | pdf | 10-50MiB | 2278 | 21308 | 46961719 | 1784 | 18219008 |
| nasa-pdf-0018 | pdf | 1-10MiB | 532 | 2069 | 17335793 | 527 | 6111232 |
| nasa-pdf-0019 | pdf | 50-100MiB | 6272 | 5745 | 177330969 | 1842 | 83656704 |
| nasa-pdf-0020 | pdf | >100MiB | 16487 | 5533 | 366883329 | 1312 | 180592640 |
| nasa-pdf-0021 | pdf | 10-50MiB | 3370 | 1605 | 76019069 | 239 | 36720640 |
| nasa-pdf-0022 | pdf | 10-50MiB | 2045 | 4777 | 51613337 | 1470 | 22261760 |
| nasa-pdf-0023 | pdf | 1-10MiB | 570 | 1586 | 19490360 | 367 | 8859648 |
| nasa-pdf-0024 | pdf | >100MiB | 42916 | 28896 | 406683374 | 23416 | 176824320 |
| nasa-pdf-0025 | pdf | 50-100MiB | 5125 | 7777 | 149287114 | 2782 | 74457088 |
| nasa-pdf-0026 | pdf | 10-50MiB | 4996 | 3518 | 54448386 | 330 | 23425024 |
| nasa-pdf-0027 | pdf | 1-10MiB | 9958 | 5099 | 33111645 | 2699 | 15372288 |
| nasa-pdf-0028 | pdf | 100KiB-1MiB | 241 | 828 | 4812255 | 147 | 962560 |
| nasa-pdf-0029 | pdf | 50-100MiB | 151443 | 5195 | 116033625 | 1347 | 56590336 |
| nasa-pdf-0030 | pdf | 1-10MiB | 2138 | 675 | 20832518 | 170 | 9383936 |
| nist-pdf-0001 | pdf | 100KiB-1MiB | 1483 | 1168 | 6155856 | 399 | 1695744 |
| nist-pdf-0002 | pdf | 100KiB-1MiB | 719 | 709 | 3977836 | 270 | 999424 |
| nist-pdf-0003 | pdf | 100KiB-1MiB | 2025 | 1068 | 6314378 | 562 | 2232320 |
| nist-pdf-0004 | pdf | 1-10MiB | 1700 | 662 | 5398522 | 362 | 2273280 |
| nist-pdf-0005 | pdf | 1-10MiB | 2106 | 1108 | 7452724 | 315 | 2125824 |
| nist-pdf-0006 | pdf | 10-50MiB | 2366 | 1011 | 29661249 | 203 | 13623296 |
| nist-pdf-0007 | pdf | 100KiB-1MiB | 1324 | 818 | 3351266 | 208 | 1036288 |
| nist-pdf-0008 | pdf | 1-10MiB | 2558 | 1299 | 10313200 | 466 | 3534848 |
| nist-pdf-0009 | pdf | 1-10MiB | 3033 | 1295 | 11466981 | 338 | 3948544 |
| nist-pdf-0010 | pdf | 1-10MiB | 9338 | 5811 | 30786675 | 531 | 6365184 |
| nist-pdf-0011 | pdf | 50-100MiB | 142258 | 90678 | 281341256 | 3255 | 88100864 |
| nist-pdf-0012 | pdf | 1-10MiB | 2893 | 1098 | 10510150 | 446 | 3661824 |
| nist-pdf-0013 | pdf | 50-100MiB | 109043 | 76808 | 126046370 | 10693 | 84881408 |
| nist-pdf-0014 | pdf | 1-10MiB | 19950 | 11396 | 49103148 | 9585 | 13459456 |
| nist-pdf-0015 | pdf | 1-10MiB | 14458 | 8340 | 46758410 | 3115 | 13611008 |
| nist-pdf-0016 | pdf | 100KiB-1MiB | 524 | 214 | 1855343 | 75 | 503808 |
| nist-pdf-0017 | pdf | 1-10MiB | 4425 | 9305 | 31251852 | 325 | 2334720 |
| nist-pdf-0018 | pdf | 1-10MiB | 1045 | 526 | 5676505 | 153 | 1945600 |
| nist-pdf-0019 | pdf | 1-10MiB | 8648 | 731 | 7014531 | 200 | 2564096 |
| nist-pdf-0020 | pdf | 1-10MiB | 2056 | 910 | 6402496 | 332 | 2039808 |
| nist-docx-0001 | docx | 1-10MiB | 66 | 164 | 5347276 | 41 | 2469888 |
| nist-docx-0002 | docx | 1-10MiB | 88 | 143 | 5179534 | 43 | 2428928 |
| nist-docx-0003 | docx | 1-10MiB | 89 | 166 | 5297166 | 39 | 2498560 |
| nist-docx-0004 | docx | 1-10MiB | 87 | 148 | 5245124 | 42 | 2449408 |
| nist-docx-0005 | docx | <100KiB | 12 | 41 | 401924 | 40 | 364544 |
| nist-docx-0006 | docx | <100KiB | 13 | 41 | 401684 | 37 | 356352 |
| nist-docx-0007 | docx | <100KiB | 12 | 45 | 363784 | 39 | 319488 |
| nist-docx-0008 | docx | <100KiB | 8 | 41 | 332588 | 33 | 196608 |
| nist-docx-0009 | docx | 100KiB-1MiB | 24 | 76 | 800952 | 99 | 618496 |
| nist-docx-0010 | docx | <100KiB | 15 | 76 | 599310 | 41 | 172032 |
| nist-docx-0011 | docx | 100KiB-1MiB | 26 | 99 | 944792 | 54 | 679936 |
| nist-docx-0012 | docx | 100KiB-1MiB | 25 | 113 | 1022634 | 48 | 610304 |
| nist-docx-0013 | docx | <100KiB | 8 | 59 | 412828 | 33 | 143360 |
| nist-docx-0014 | docx | 100KiB-1MiB | 29 | 146 | 2828957 | 125 | 1822720 |
| nist-docx-0015 | docx | 1-10MiB | 49 | 126 | 5269635 | 42 | 1769472 |
| nist-epub-0001 | epub | 100KiB-1MiB | 28 | 89 | 933896 | 55 | 1503232 |
| nist-epub-0002 | epub | 100KiB-1MiB | 21 | 83 | 780040 | 48 | 1089536 |
| nist-epub-0003 | epub | <100KiB | 17 | 67 | 634706 | 45 | 909312 |
| nist-epub-0004 | epub | 100KiB-1MiB | 49 | 185 | 1763778 | 63 | 1970176 |
| nist-epub-0005 | epub | 100KiB-1MiB | 43 | 90 | 1142540 | 42 | 946176 |
| nist-epub-0006 | epub | 100KiB-1MiB | 19 | 76 | 721036 | 41 | 487424 |
| nist-epub-0007 | epub | 100KiB-1MiB | 27 | 76 | 775486 | 52 | 1486848 |
| nist-epub-0008 | epub | <100KiB | 9 | 52 | 401710 | 23 | 0 |
| nist-epub-0009 | epub | 1-10MiB | 35 | 135 | 3255772 | 54 | 2461696 |
| nist-epub-0010 | epub | 100KiB-1MiB | 21 | 83 | 769784 | 52 | 1323008 |

### Exact reconstruction (length + SHA-256 of materialized bytes)

| lane | byte-exact | of |
|---|---:|---:|
| VOLE `materialize --exact` | 97 | 100 |
| SQLite/FTS (retained source blob) | 98 | 100 |
| direct tooling (the source file) | 100 | 100 |

