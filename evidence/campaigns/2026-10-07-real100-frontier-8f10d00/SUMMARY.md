# real100-v1 frontier map

Documents measured: **100**. Lanes: VOLE (frozen), SQLite/FTS (A1), direct tooling (A0). Tie band: ±10% of the fastest median. `decline` = the lane has no such observation for the format / returned a typed error.

## Answered vs declined (all ops)

| lane | answered | declined |
|---|---:|---:|
| VOLE | 399 | 301 |
| SQLite/FTS | 506 | 194 |
| direct tooling | 508 | 192 |

### Overall

| stratum | workload | VOLE | SQLite/FTS | direct tooling | fastest |
|---|---|---|---|---|---|
| all | text_once | loss | win | loss | SQLite/FTS |
| all | text_repeat | win | tie | loss | VOLE |
| all | heading | loss | win | loss | SQLite/FTS |
| all | table | win | tie | loss | VOLE |
| all | resource | loss | win | loss | SQLite/FTS |
| all | metadata | loss | win | loss | SQLite/FTS |
| all | exact | loss | loss | win | direct tooling |

### By format

| stratum | workload | VOLE | SQLite/FTS | direct tooling | fastest |
|---|---|---|---|---|---|
| pdf | text_once | loss | win | loss | SQLite/FTS |
| pdf | text_repeat | win | loss | loss | VOLE |
| pdf | heading | - | - | - | — |
| pdf | table | - | - | - | — |
| pdf | resource | - | - | - | — |
| pdf | metadata | loss | win | loss | SQLite/FTS |
| pdf | exact | loss | loss | win | direct tooling |
| docx | text_once | loss | win | loss | SQLite/FTS |
| docx | text_repeat | loss | win | loss | SQLite/FTS |
| docx | heading | loss | win | loss | SQLite/FTS |
| docx | table | win | tie | loss | VOLE |
| docx | resource | loss | win | loss | SQLite/FTS |
| docx | metadata | win | tie | loss | VOLE |
| docx | exact | loss | win | tie | SQLite/FTS |
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
| 10-50MiB | text_repeat | win | tie | loss | VOLE |
| 10-50MiB | heading | decline | win | loss | SQLite/FTS |
| 10-50MiB | table | decline | win | loss | SQLite/FTS |
| 10-50MiB | resource | loss | win | loss | SQLite/FTS |
| 10-50MiB | metadata | loss | win | loss | SQLite/FTS |
| 10-50MiB | exact | loss | loss | win | direct tooling |
| 100KiB-1MiB | text_once | loss | win | loss | SQLite/FTS |
| 100KiB-1MiB | text_repeat | loss | win | loss | SQLite/FTS |
| 100KiB-1MiB | heading | loss | win | loss | SQLite/FTS |
| 100KiB-1MiB | table | loss | win | loss | SQLite/FTS |
| 100KiB-1MiB | resource | win | tie | loss | VOLE |
| 100KiB-1MiB | metadata | loss | win | loss | SQLite/FTS |
| 100KiB-1MiB | exact | loss | win | tie | SQLite/FTS |
| 50-100MiB | text_once | loss | win | loss | SQLite/FTS |
| 50-100MiB | text_repeat | loss | win | loss | SQLite/FTS |
| 50-100MiB | heading | - | - | - | — |
| 50-100MiB | table | - | - | - | — |
| 50-100MiB | resource | - | - | - | — |
| 50-100MiB | metadata | loss | win | loss | SQLite/FTS |
| 50-100MiB | exact | loss | loss | win | direct tooling |
| <100KiB | text_once | loss | win | loss | SQLite/FTS |
| <100KiB | text_repeat | loss | win | loss | SQLite/FTS |
| <100KiB | heading | loss | win | loss | SQLite/FTS |
| <100KiB | table | win | tie | loss | VOLE |
| <100KiB | resource | win | loss | loss | VOLE |
| <100KiB | metadata | win | tie | loss | VOLE |
| <100KiB | exact | loss | loss | win | direct tooling |
| >100MiB | text_once | loss | win | loss | SQLite/FTS |
| >100MiB | text_repeat | loss | win | loss | SQLite/FTS |
| >100MiB | heading | - | - | - | — |
| >100MiB | table | - | - | - | — |
| >100MiB | resource | - | - | - | — |
| >100MiB | metadata | loss | win | loss | SQLite/FTS |
| >100MiB | exact | loss | loss | win | direct tooling |

### One-time costs (build/ingest, per document)

| id | format | size | VOLE encode ms | VOLE ingest ms | VOLE bytes | A1 build ms | A1 db bytes |
|---|---|---|---:|---:|---:|---:|---:|
| nasa-epub-0001 | epub | 1-10MiB | 116 | 310 | 10243395 | 121 | 9383936 |
| nasa-epub-0002 | epub | 10-50MiB | 544 | 773 | 50247420 | 124 | 22065152 |
| nasa-epub-0003 | epub | 1-10MiB | 171 | 488 | 7971544 | 380 | 4026368 |
| nasa-epub-0004 | epub | 1-10MiB | 138 | 203 | 10584204 | 50 | 5337088 |
| nasa-epub-0005 | epub | 1-10MiB | 117 | 259 | 10794100 | 53 | 5517312 |
| nasa-epub-0006 | epub | 10-50MiB | 268 | 563 | 23849433 | 102 | 13516800 |
| nasa-epub-0007 | epub | 10-50MiB | 1151 | 1646 | 97632529 | 143 | 47693824 |
| nasa-epub-0008 | epub | 10-50MiB | 594 | 1101 | 54100005 | 98 | 26001408 |
| nasa-epub-0009 | epub | 10-50MiB | 979 | 1354 | 82309579 | 124 | 40054784 |
| nasa-epub-0010 | epub | 10-50MiB | 1032 | 10541 | 92590347 | 25 | 0 |
| nasa-epub-0011 | epub | 1-10MiB | 562 | 563 | 22875361 | 129 | 14790656 |
| nasa-epub-0012 | epub | 10-50MiB | 846 | 927 | 91539976 | 129 | 35094528 |
| nasa-epub-0013 | epub | 1-10MiB | 147 | 255 | 11604524 | 50 | 5734400 |
| nasa-epub-0014 | epub | 1-10MiB | 118 | 211 | 8457554 | 100 | 7942144 |
| nasa-epub-0015 | epub | 1-10MiB | 131 | 255 | 11166004 | 51 | 5615616 |
| nasa-pdf-eb-01 | pdf | 1-10MiB | 1107 | 4749 | 25328940 | 5876 | 10489856 |
| nasa-pdf-eb-02 | pdf | 10-50MiB | 1457 | 2287 | 49076643 | 2092 | 23789568 |
| nasa-pdf-eb-03 | pdf | 1-10MiB | 1997 | 860 | 6748742 | 256 | 2138112 |
| nasa-pdf-eb-04 | pdf | 1-10MiB | 3637 | 801 | 10358963 | 561 | 3948544 |
| nasa-pdf-eb-05 | pdf | 1-10MiB | 2408 | 835 | 6782112 | 239 | 2125824 |
| nasa-pdf-eb-06 | pdf | 1-10MiB | 386 | 2446 | 20132649 | 2656 | 8409088 |
| nasa-pdf-eb-07 | pdf | 10-50MiB | 6677 | 10513 | 109814322 | 1873 | 41668608 |
| nasa-pdf-eb-08 | pdf | 10-50MiB | 1712 | 2055 | 77245477 | 2092 | 35909632 |
| nasa-pdf-eb-09 | pdf | 10-50MiB | 3370 | 2359 | 31244043 | 1863 | 11776000 |
| nasa-pdf-eb-10 | pdf | 10-50MiB | 1635 | 3458 | 41795882 | 1764 | 20025344 |
| nasa-pdf-0001 | pdf | >100MiB | 1753 | -1 | 0 | 8238 | 420708352 |
| nasa-pdf-0002 | pdf | >100MiB | 180024 | -1 | 0 | 14758 | 330067968 |
| nasa-pdf-0003 | pdf | >100MiB | 180013 | -1 | 0 | 12490 | 238088192 |
| nasa-pdf-0004 | pdf | 50-100MiB | 91398 | 10185 | 179120665 | 4312 | 82190336 |
| nasa-pdf-0005 | pdf | 50-100MiB | 4170 | 2627 | 128384767 | 380 | 62504960 |
| nasa-pdf-0006 | pdf | 10-50MiB | 1005 | 1328 | 43905615 | 304 | 20557824 |
| nasa-pdf-0007 | pdf | 1-10MiB | 1407 | 941 | 23028128 | 290 | 10502144 |
| nasa-pdf-0008 | pdf | 1-10MiB | 488 | 1253 | 14821040 | 258 | 6262784 |
| nasa-pdf-0009 | pdf | 1-10MiB | 228 | 910 | 9474639 | 240 | 3538944 |
| nasa-pdf-0010 | pdf | 1-10MiB | 150 | 601 | 6619580 | 137 | 2457600 |
| nasa-pdf-0011 | pdf | 1-10MiB | 161 | 682 | 7711845 | 174 | 2908160 |
| nasa-pdf-0012 | pdf | 50-100MiB | 8021 | 12382 | 180666283 | 4694 | 77893632 |
| nasa-pdf-0013 | pdf | 50-100MiB | 19290 | 7834 | 186313329 | 2439 | 90951680 |
| nasa-pdf-0014 | pdf | 50-100MiB | 20191 | 23506 | 183841417 | 24955 | 74096640 |
| nasa-pdf-0015 | pdf | 10-50MiB | 5641 | 2883 | 89833052 | 588 | 45846528 |
| nasa-pdf-0016 | pdf | 10-50MiB | 8081 | 14960 | 86669826 | 7274 | 26701824 |
| nasa-pdf-0017 | pdf | 10-50MiB | 2055 | 5407 | 46961719 | 1699 | 18219008 |
| nasa-pdf-0018 | pdf | 1-10MiB | 739 | 2428 | 17335793 | 489 | 6111232 |
| nasa-pdf-0019 | pdf | 50-100MiB | 6228 | 5934 | 177330969 | 1698 | 83656704 |
| nasa-pdf-0020 | pdf | >100MiB | 15840 | 5320 | 366883329 | 1126 | 180592640 |
| nasa-pdf-0021 | pdf | 10-50MiB | 3804 | 14589 | 76019069 | 1132 | 36720640 |
| nasa-pdf-0022 | pdf | 10-50MiB | 2459 | 32141 | 51613337 | 1410 | 22261760 |
| nasa-pdf-0023 | pdf | 1-10MiB | 759 | 1757 | 19490360 | 351 | 8859648 |
| nasa-pdf-0024 | pdf | >100MiB | 42583 | 28275 | 406683374 | 21990 | 176824320 |
| nasa-pdf-0025 | pdf | 50-100MiB | 5004 | 7687 | 149287114 | 2734 | 74457088 |
| nasa-pdf-0026 | pdf | 10-50MiB | 4978 | 3497 | 54448386 | 325 | 23425024 |
| nasa-pdf-0027 | pdf | 1-10MiB | 9996 | 5026 | 33111645 | 2664 | 15372288 |
| nasa-pdf-0028 | pdf | 100KiB-1MiB | 230 | 819 | 4812255 | 139 | 962560 |
| nasa-pdf-0029 | pdf | 50-100MiB | 150459 | 5150 | 116033625 | 1309 | 56590336 |
| nasa-pdf-0030 | pdf | 1-10MiB | 2109 | 665 | 20832518 | 167 | 9383936 |
| nist-pdf-0001 | pdf | 100KiB-1MiB | 1464 | 1161 | 6155856 | 386 | 1695744 |
| nist-pdf-0002 | pdf | 100KiB-1MiB | 707 | 705 | 3977836 | 269 | 999424 |
| nist-pdf-0003 | pdf | 100KiB-1MiB | 2020 | 1060 | 6314378 | 551 | 2232320 |
| nist-pdf-0004 | pdf | 1-10MiB | 1698 | 662 | 5398522 | 365 | 2273280 |
| nist-pdf-0005 | pdf | 1-10MiB | 2090 | 1104 | 7452724 | 307 | 2125824 |
| nist-pdf-0006 | pdf | 10-50MiB | 2286 | 982 | 29661249 | 209 | 13623296 |
| nist-pdf-0007 | pdf | 100KiB-1MiB | 1317 | 744 | 3351266 | 197 | 1036288 |
| nist-pdf-0008 | pdf | 1-10MiB | 2547 | 1263 | 10313200 | 447 | 3534848 |
| nist-pdf-0009 | pdf | 1-10MiB | 3006 | 1267 | 11466981 | 302 | 3948544 |
| nist-pdf-0010 | pdf | 1-10MiB | 9047 | 5638 | 30786675 | 514 | 6365184 |
| nist-pdf-0011 | pdf | 50-100MiB | 141792 | 105847 | 281341256 | 3275 | 88100864 |
| nist-pdf-0012 | pdf | 1-10MiB | 2890 | 1132 | 10510150 | 450 | 3661824 |
| nist-pdf-0013 | pdf | 50-100MiB | 109930 | 77261 | 126046370 | 11012 | 84881408 |
| nist-pdf-0014 | pdf | 1-10MiB | 19973 | 11358 | 49103148 | 9492 | 13459456 |
| nist-pdf-0015 | pdf | 1-10MiB | 14452 | 8305 | 46758410 | 3086 | 13611008 |
| nist-pdf-0016 | pdf | 100KiB-1MiB | 743 | 793 | 1855343 | 72 | 503808 |
| nist-pdf-0017 | pdf | 1-10MiB | 4646 | 9384 | 31251852 | 320 | 2334720 |
| nist-pdf-0018 | pdf | 1-10MiB | 1076 | 530 | 5676505 | 163 | 1945600 |
| nist-pdf-0019 | pdf | 1-10MiB | 8669 | 741 | 7014531 | 206 | 2564096 |
| nist-pdf-0020 | pdf | 1-10MiB | 2060 | 905 | 6402496 | 334 | 2039808 |
| nist-docx-0001 | docx | 1-10MiB | 67 | 165 | 5347276 | 50 | 2469888 |
| nist-docx-0002 | docx | 1-10MiB | 92 | 131 | 5179534 | 41 | 2428928 |
| nist-docx-0003 | docx | 1-10MiB | 90 | 160 | 5297166 | 43 | 2498560 |
| nist-docx-0004 | docx | 1-10MiB | 88 | 155 | 5245124 | 41 | 2449408 |
| nist-docx-0005 | docx | <100KiB | 11 | 47 | 401924 | 41 | 364544 |
| nist-docx-0006 | docx | <100KiB | 12 | 42 | 401684 | 40 | 356352 |
| nist-docx-0007 | docx | <100KiB | 10 | 42 | 363784 | 44 | 319488 |
| nist-docx-0008 | docx | <100KiB | 9 | 46 | 332588 | 35 | 196608 |
| nist-docx-0009 | docx | 100KiB-1MiB | 25 | 98 | 800952 | 96 | 618496 |
| nist-docx-0010 | docx | <100KiB | 14 | 63 | 599310 | 33 | 172032 |
| nist-docx-0011 | docx | 100KiB-1MiB | 26 | 95 | 944792 | 49 | 679936 |
| nist-docx-0012 | docx | 100KiB-1MiB | 24 | 114 | 1022634 | 47 | 610304 |
| nist-docx-0013 | docx | <100KiB | 8 | 53 | 412828 | 33 | 143360 |
| nist-docx-0014 | docx | 100KiB-1MiB | 29 | 127 | 2828957 | 127 | 1822720 |
| nist-docx-0015 | docx | 1-10MiB | 51 | 127 | 5269635 | 42 | 1769472 |
| nist-epub-0001 | epub | 100KiB-1MiB | 27 | 94 | 933896 | 56 | 1503232 |
| nist-epub-0002 | epub | 100KiB-1MiB | 20 | 78 | 780040 | 51 | 1089536 |
| nist-epub-0003 | epub | <100KiB | 16 | 72 | 634706 | 46 | 909312 |
| nist-epub-0004 | epub | 100KiB-1MiB | 50 | 180 | 1763778 | 64 | 1970176 |
| nist-epub-0005 | epub | 100KiB-1MiB | 42 | 90 | 1142540 | 44 | 946176 |
| nist-epub-0006 | epub | 100KiB-1MiB | 19 | 79 | 721036 | 41 | 487424 |
| nist-epub-0007 | epub | 100KiB-1MiB | 26 | 83 | 775486 | 56 | 1486848 |
| nist-epub-0008 | epub | <100KiB | 9 | 48 | 401710 | 23 | 0 |
| nist-epub-0009 | epub | 1-10MiB | 36 | 136 | 3255772 | 56 | 2461696 |
| nist-epub-0010 | epub | 100KiB-1MiB | 20 | 84 | 769784 | 58 | 1323008 |

### Exact reconstruction (length + SHA-256 of materialized bytes)

| lane | byte-exact | of |
|---|---:|---:|
| VOLE `materialize --exact` | 97 | 100 |
| SQLite/FTS (retained source blob) | 98 | 100 |
| direct tooling (the source file) | 100 | 100 |

