# real100-v1 frontier map

Documents measured: **100**. Lanes: VOLE cold (v), VOLE resident (v_r, one process per batch), SQLite/FTS (A1), direct tooling (A0). Tie band: ±10% of the fastest median. `decline` = the lane has no such observation for the format / returned a typed error.

## Answered vs declined (all ops)

| lane | answered | declined |
|---|---:|---:|
| VOLE | 455 | 245 |
| VOLE resident | 184 | 16 |
| SQLite/FTS | 506 | 194 |
| direct tooling | 508 | 192 |

### Overall

| stratum | workload | VOLE | VOLE resident | SQLite/FTS | direct tooling | fastest |
|---|---|---|---|---|---|---|
| all | text_once | loss | decline | win | loss | SQLite/FTS |
| all | text_repeat | win | loss | tie | loss | VOLE |
| all | heading | loss | decline | win | loss | SQLite/FTS |
| all | table | loss | decline | win | loss | SQLite/FTS |
| all | resource | loss | decline | win | loss | SQLite/FTS |
| all | metadata | loss | decline | win | loss | SQLite/FTS |
| all | session_mixed | decline | win | decline | decline | VOLE resident |
| all | exact | loss | decline | loss | win | direct tooling |

### By format

| stratum | workload | VOLE | VOLE resident | SQLite/FTS | direct tooling | fastest |
|---|---|---|---|---|---|---|
| pdf | text_once | loss | decline | win | loss | SQLite/FTS |
| pdf | text_repeat | win | loss | loss | loss | VOLE |
| pdf | heading | - | - | - | - | — |
| pdf | table | - | - | - | - | — |
| pdf | resource | - | - | - | - | — |
| pdf | metadata | loss | decline | win | loss | SQLite/FTS |
| pdf | session_mixed | decline | win | decline | decline | VOLE resident |
| pdf | exact | loss | decline | loss | win | direct tooling |
| docx | text_once | loss | decline | win | loss | SQLite/FTS |
| docx | text_repeat | loss | win | loss | loss | VOLE resident |
| docx | heading | loss | decline | win | loss | SQLite/FTS |
| docx | table | loss | decline | win | loss | SQLite/FTS |
| docx | resource | loss | decline | win | loss | SQLite/FTS |
| docx | metadata | win | decline | tie | loss | VOLE |
| docx | session_mixed | decline | win | decline | decline | VOLE resident |
| docx | exact | loss | decline | loss | win | direct tooling |
| epub | text_once | loss | decline | win | loss | SQLite/FTS |
| epub | text_repeat | loss | loss | win | loss | SQLite/FTS |
| epub | heading | loss | decline | win | loss | SQLite/FTS |
| epub | table | loss | decline | win | loss | SQLite/FTS |
| epub | resource | loss | decline | win | loss | SQLite/FTS |
| epub | metadata | loss | decline | win | loss | SQLite/FTS |
| epub | session_mixed | decline | win | decline | decline | VOLE resident |
| epub | exact | loss | decline | loss | win | direct tooling |

### By size class

| stratum | workload | VOLE | VOLE resident | SQLite/FTS | direct tooling | fastest |
|---|---|---|---|---|---|---|
| 1-10MiB | text_once | loss | decline | win | loss | SQLite/FTS |
| 1-10MiB | text_repeat | win | loss | tie | loss | VOLE |
| 1-10MiB | heading | loss | decline | win | loss | SQLite/FTS |
| 1-10MiB | table | loss | decline | win | loss | SQLite/FTS |
| 1-10MiB | resource | loss | decline | win | loss | SQLite/FTS |
| 1-10MiB | metadata | loss | decline | win | loss | SQLite/FTS |
| 1-10MiB | session_mixed | decline | win | decline | decline | VOLE resident |
| 1-10MiB | exact | loss | decline | loss | win | direct tooling |
| 10-50MiB | text_once | loss | decline | win | loss | SQLite/FTS |
| 10-50MiB | text_repeat | win | loss | tie | loss | VOLE |
| 10-50MiB | heading | loss | decline | win | loss | SQLite/FTS |
| 10-50MiB | table | loss | decline | win | loss | SQLite/FTS |
| 10-50MiB | resource | loss | decline | win | loss | SQLite/FTS |
| 10-50MiB | metadata | loss | decline | win | loss | SQLite/FTS |
| 10-50MiB | session_mixed | decline | win | decline | decline | VOLE resident |
| 10-50MiB | exact | loss | decline | loss | win | direct tooling |
| 100KiB-1MiB | text_once | loss | decline | win | loss | SQLite/FTS |
| 100KiB-1MiB | text_repeat | loss | win | loss | loss | VOLE resident |
| 100KiB-1MiB | heading | loss | decline | win | loss | SQLite/FTS |
| 100KiB-1MiB | table | loss | decline | win | loss | SQLite/FTS |
| 100KiB-1MiB | resource | loss | decline | win | loss | SQLite/FTS |
| 100KiB-1MiB | metadata | loss | decline | win | loss | SQLite/FTS |
| 100KiB-1MiB | session_mixed | decline | win | decline | decline | VOLE resident |
| 100KiB-1MiB | exact | loss | decline | loss | win | direct tooling |
| 50-100MiB | text_once | loss | decline | win | loss | SQLite/FTS |
| 50-100MiB | text_repeat | win | loss | tie | loss | VOLE |
| 50-100MiB | heading | - | - | - | - | — |
| 50-100MiB | table | - | - | - | - | — |
| 50-100MiB | resource | - | - | - | - | — |
| 50-100MiB | metadata | loss | decline | win | loss | SQLite/FTS |
| 50-100MiB | session_mixed | decline | win | decline | decline | VOLE resident |
| 50-100MiB | exact | loss | decline | loss | win | direct tooling |
| <100KiB | text_once | loss | decline | win | loss | SQLite/FTS |
| <100KiB | text_repeat | loss | win | loss | loss | VOLE resident |
| <100KiB | heading | loss | decline | win | loss | SQLite/FTS |
| <100KiB | table | win | decline | tie | loss | VOLE |
| <100KiB | resource | win | decline | tie | loss | VOLE |
| <100KiB | metadata | win | decline | tie | loss | VOLE |
| <100KiB | session_mixed | decline | win | decline | decline | VOLE resident |
| <100KiB | exact | loss | decline | loss | win | direct tooling |
| >100MiB | text_once | loss | decline | win | loss | SQLite/FTS |
| >100MiB | text_repeat | loss | loss | win | loss | SQLite/FTS |
| >100MiB | heading | - | - | - | - | — |
| >100MiB | table | - | - | - | - | — |
| >100MiB | resource | - | - | - | - | — |
| >100MiB | metadata | loss | decline | win | loss | SQLite/FTS |
| >100MiB | session_mixed | decline | win | decline | decline | VOLE resident |
| >100MiB | exact | loss | decline | loss | win | direct tooling |

### One-time costs + storage universes (per document)

VOLE persistent = the field store alone after `field-ingest` (the standalone `.voldoc` may be deleted); descriptor = the optional standalone `.voldoc`; transient = store + descriptor during ingest.

| id | format | size | VOLE encode ms | VOLE ingest ms | VOLE persistent B | VOLE descriptor B | VOLE transient B | A1 build ms | A1 db B |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|
| nasa-epub-0001 | epub | 1-10MiB | 99 | 308 | 5901005 | 4342390 | 10243395 | 119 | 9383936 |
| nasa-epub-0002 | epub | 10-50MiB | 437 | 755 | 31325247 | 18922173 | 50247420 | 107 | 22065152 |
| nasa-epub-0003 | epub | 1-10MiB | 86 | 199 | 4478732 | 3492812 | 7971544 | 48 | 4026368 |
| nasa-epub-0004 | epub | 1-10MiB | 138 | 514 | 5795016 | 4789188 | 10584204 | 49 | 5337088 |
| nasa-epub-0005 | epub | 1-10MiB | 101 | 256 | 5935638 | 4858462 | 10794100 | 51 | 5517312 |
| nasa-epub-0006 | epub | 10-50MiB | 252 | 558 | 12983154 | 10866279 | 23849433 | 99 | 13516800 |
| nasa-epub-0007 | epub | 10-50MiB | 1024 | 1632 | 50799089 | 46833440 | 97632529 | 145 | 47693824 |
| nasa-epub-0008 | epub | 10-50MiB | 518 | 1091 | 28703018 | 25396987 | 54100005 | 103 | 26001408 |
| nasa-epub-0009 | epub | 10-50MiB | 861 | 1313 | 42790706 | 39518873 | 82309579 | 130 | 40054784 |
| nasa-epub-0010 | epub | 10-50MiB | 912 | 1224 | 47595423 | 44994924 | 92590347 | 28 | 0 |
| nasa-epub-0011 | epub | 1-10MiB | 492 | 559 | 12492308 | 10383053 | 22875361 | 126 | 14790656 |
| nasa-epub-0012 | epub | 10-50MiB | 727 | 911 | 58189047 | 33350929 | 91539976 | 127 | 35094528 |
| nasa-epub-0013 | epub | 1-10MiB | 124 | 255 | 6449398 | 5155126 | 11604524 | 49 | 5734400 |
| nasa-epub-0014 | epub | 1-10MiB | 99 | 210 | 4696160 | 3761394 | 8457554 | 100 | 7942144 |
| nasa-epub-0015 | epub | 1-10MiB | 135 | 270 | 6195322 | 4970682 | 11166004 | 52 | 5615616 |
| nasa-pdf-eb-01 | pdf | 1-10MiB | 722 | 4837 | 20922701 | 4406239 | 25328940 | 5957 | 10489856 |
| nasa-pdf-eb-02 | pdf | 10-50MiB | 1264 | 31058 | 28483847 | 20592796 | 49076643 | 3797 | 23789568 |
| nasa-pdf-eb-03 | pdf | 1-10MiB | 2455 | 17069 | 5149877 | 1598865 | 6748742 | 547 | 2138112 |
| nasa-pdf-eb-04 | pdf | 1-10MiB | 3473 | 2607 | 6977790 | 3381173 | 10358963 | 577 | 3948544 |
| nasa-pdf-eb-05 | pdf | 1-10MiB | 2021 | 859 | 5304673 | 1477439 | 6782112 | 248 | 2125824 |
| nasa-pdf-eb-06 | pdf | 1-10MiB | 351 | 2483 | 14543697 | 5588952 | 20132649 | 2834 | 8409088 |
| nasa-pdf-eb-07 | pdf | 10-50MiB | 4667 | 10968 | 69438879 | 40375443 | 109814322 | 2001 | 41668608 |
| nasa-pdf-eb-08 | pdf | 10-50MiB | 1631 | 2118 | 41936181 | 35309296 | 77245477 | 2664 | 35909632 |
| nasa-pdf-eb-09 | pdf | 10-50MiB | 3291 | 2451 | 20036634 | 11207409 | 31244043 | 1902 | 11776000 |
| nasa-pdf-eb-10 | pdf | 10-50MiB | 1158 | 3652 | 26880268 | 14915614 | 41795882 | 1829 | 20025344 |
| nasa-pdf-0001 | pdf | >100MiB | 3347 | -1 |  |  | 0 | 10408 | 420708352 |
| nasa-pdf-0002 | pdf | >100MiB | 180026 | -1 |  |  | 0 | 12885 | 330067968 |
| nasa-pdf-0003 | pdf | >100MiB | 180019 | -1 |  |  | 0 | 11731 | 238088192 |
| nasa-pdf-0004 | pdf | 50-100MiB | 77086 | 10255 | 102330488 | 76790177 | 179120665 | 4457 | 82190336 |
| nasa-pdf-0005 | pdf | 50-100MiB | 4096 | 2622 | 66607938 | 61776829 | 128384767 | 412 | 62504960 |
| nasa-pdf-0006 | pdf | 10-50MiB | 971 | 1335 | 23901529 | 20004086 | 43905615 | 333 | 20557824 |
| nasa-pdf-0007 | pdf | 1-10MiB | 1251 | 969 | 13203818 | 9824310 | 23028128 | 301 | 10502144 |
| nasa-pdf-0008 | pdf | 1-10MiB | 440 | 21223 | 9774468 | 5046572 | 14821040 | 473 | 6262784 |
| nasa-pdf-0009 | pdf | 1-10MiB | 279 | 19232 | 6702833 | 2771806 | 9474639 | 580 | 3538944 |
| nasa-pdf-0010 | pdf | 1-10MiB | 205 | 17098 | 4690845 | 1928735 | 6619580 | 156 | 2457600 |
| nasa-pdf-0011 | pdf | 1-10MiB | 150 | 2809 | 5415311 | 2296534 | 7711845 | 179 | 2908160 |
| nasa-pdf-0012 | pdf | 50-100MiB | 6005 | 12550 | 106771782 | 73894501 | 180666283 | 4883 | 77893632 |
| nasa-pdf-0013 | pdf | 50-100MiB | 17295 | 7981 | 102893908 | 83419421 | 186313329 | 1312 | 90951680 |
| nasa-pdf-0014 | pdf | 50-100MiB | 13099 | 24111 | 120415507 | 63425910 | 183841417 | 25193 | 74096640 |
| nasa-pdf-0015 | pdf | 10-50MiB | 5136 | 2816 | 48528919 | 41304133 | 89833052 | 570 | 45846528 |
| nasa-pdf-0016 | pdf | 10-50MiB | 4965 | 15276 | 64794987 | 21874839 | 86669826 | 7652 | 26701824 |
| nasa-pdf-0017 | pdf | 10-50MiB | 1743 | 5462 | 32285946 | 14675773 | 46961719 | 1766 | 18219008 |
| nasa-pdf-0018 | pdf | 1-10MiB | 687 | 2770 | 12463926 | 4871867 | 17335793 | 548 | 6111232 |
| nasa-pdf-0019 | pdf | 50-100MiB | 5808 | 5863 | 96875713 | 80455256 | 177330969 | 1684 | 83656704 |
| nasa-pdf-0020 | pdf | >100MiB | 15020 | 10313 | 188918710 | 177964619 | 366883329 | 5859 | 180592640 |
| nasa-pdf-0021 | pdf | 10-50MiB | 4026 | 17929 | 39683105 | 36335964 | 76019069 | 1259 | 36720640 |
| nasa-pdf-0022 | pdf | 10-50MiB | 2177 | 17374 | 33327144 | 18286193 | 51613337 | 1789 | 22261760 |
| nasa-pdf-0023 | pdf | 1-10MiB | 455 | 1501 | 12382851 | 7107509 | 19490360 | 562 | 8859648 |
| nasa-pdf-0024 | pdf | >100MiB | 39157 | 29105 | 238200260 | 168483114 | 406683374 | 28923 | 176824320 |
| nasa-pdf-0025 | pdf | 50-100MiB | 4588 | 8036 | 84506307 | 64780807 | 149287114 | 3368 | 74457088 |
| nasa-pdf-0026 | pdf | 10-50MiB | 4763 | 3557 | 32696509 | 21751877 | 54448386 | 358 | 23425024 |
| nasa-pdf-0027 | pdf | 1-10MiB | 8799 | 5395 | 24713572 | 8398073 | 33111645 | 2766 | 15372288 |
| nasa-pdf-0028 | pdf | 100KiB-1MiB | 193 | 831 | 4312573 | 499682 | 4812255 | 169 | 962560 |
| nasa-pdf-0029 | pdf | 50-100MiB | 136214 | 5110 | 65693704 | 50339921 | 116033625 | 1325 | 56590336 |
| nasa-pdf-0030 | pdf | 1-10MiB | 1953 | 678 | 11835231 | 8997287 | 20832518 | 176 | 9383936 |
| nist-pdf-0001 | pdf | 100KiB-1MiB | 1411 | 1172 | 5641854 | 514002 | 6155856 | 403 | 1695744 |
| nist-pdf-0002 | pdf | 100KiB-1MiB | 681 | 717 | 3722067 | 255769 | 3977836 | 280 | 999424 |
| nist-pdf-0003 | pdf | 100KiB-1MiB | 1932 | 1081 | 5509691 | 804687 | 6314378 | 569 | 2232320 |
| nist-pdf-0004 | pdf | 1-10MiB | 1577 | 673 | 4298868 | 1099654 | 5398522 | 379 | 2273280 |
| nist-pdf-0005 | pdf | 1-10MiB | 2013 | 1116 | 6160266 | 1292458 | 7452724 | 324 | 2125824 |
| nist-pdf-0006 | pdf | 10-50MiB | 2143 | 983 | 16459048 | 13202201 | 29661249 | 203 | 13623296 |
| nist-pdf-0007 | pdf | 100KiB-1MiB | 1006 | 441 | 2804442 | 546824 | 3351266 | 211 | 1036288 |
| nist-pdf-0008 | pdf | 1-10MiB | 2381 | 1284 | 7848343 | 2464857 | 10313200 | 464 | 3534848 |
| nist-pdf-0009 | pdf | 1-10MiB | 2755 | 1267 | 8401674 | 3065307 | 11466981 | 308 | 3948544 |
| nist-pdf-0010 | pdf | 1-10MiB | 8418 | 5749 | 25269226 | 5517449 | 30786675 | 518 | 6365184 |
| nist-pdf-0011 | pdf | 50-100MiB | 130467 | 102402 | 195127959 | 86213297 | 281341256 | 3383 | 88100864 |
| nist-pdf-0012 | pdf | 1-10MiB | 3099 | 1141 | 7613847 | 2896303 | 10510150 | 458 | 3661824 |
| nist-pdf-0013 | pdf | 50-100MiB | 91135 | 76038 | 103722407 | 22323963 | 126046370 | 10047 | 84881408 |
| nist-pdf-0014 | pdf | 1-10MiB | 18567 | 11393 | 43138497 | 5964651 | 49103148 | 9321 | 13459456 |
| nist-pdf-0015 | pdf | 1-10MiB | 13279 | 8460 | 37330858 | 9427552 | 46758410 | 3029 | 13611008 |
| nist-pdf-0016 | pdf | 100KiB-1MiB | 479 | 212 | 1546342 | 309001 | 1855343 | 83 | 503808 |
| nist-pdf-0017 | pdf | 1-10MiB | 2633 | 9666 | 29966647 | 1285205 | 31251852 | 320 | 2334720 |
| nist-pdf-0018 | pdf | 1-10MiB | 989 | 530 | 4166228 | 1510277 | 5676505 | 155 | 1945600 |
| nist-pdf-0019 | pdf | 1-10MiB | 6401 | 768 | 5086197 | 1928334 | 7014531 | 202 | 2564096 |
| nist-pdf-0020 | pdf | 1-10MiB | 1931 | 914 | 5239578 | 1162918 | 6402496 | 440 | 2039808 |
| nist-docx-0001 | docx | 1-10MiB | 90 | 5396 | 3061230 | 2286046 | 5347276 | 297 | 2469888 |
| nist-docx-0002 | docx | 1-10MiB | 152 | 2083 | 2938406 | 2241128 | 5179534 | 176 | 2428928 |
| nist-docx-0003 | docx | 1-10MiB | 146 | 2248 | 3029745 | 2267421 | 5297166 | 175 | 2498560 |
| nist-docx-0004 | docx | 1-10MiB | 152 | 2006 | 2987054 | 2258070 | 5245124 | 164 | 2449408 |
| nist-docx-0005 | docx | <100KiB | 57 | 847 | 344613 | 57311 | 401924 | 263 | 364544 |
| nist-docx-0006 | docx | <100KiB | 61 | 687 | 344493 | 57191 | 401684 | 152 | 356352 |
| nist-docx-0007 | docx | <100KiB | 44 | 857 | 321447 | 42337 | 363784 | 895 | 319488 |
| nist-docx-0008 | docx | <100KiB | 64 | 1304 | 303801 | 28787 | 332588 | 195 | 196608 |
| nist-docx-0009 | docx | 100KiB-1MiB | 73 | 1547 | 648529 | 152423 | 800952 | 221 | 618496 |
| nist-docx-0010 | docx | <100KiB | 59 | 1315 | 533178 | 66132 | 599310 | 191 | 172032 |
| nist-docx-0011 | docx | 100KiB-1MiB | 71 | 1837 | 783033 | 161759 | 944792 | 189 | 679936 |
| nist-docx-0012 | docx | 100KiB-1MiB | 64 | 2003 | 870202 | 152432 | 1022634 | 188 | 610304 |
| nist-docx-0013 | docx | <100KiB | 62 | 1018 | 387644 | 25184 | 412828 | 222 | 143360 |
| nist-docx-0014 | docx | 100KiB-1MiB | 77 | 2053 | 1882300 | 946657 | 2828957 | 281 | 1822720 |
| nist-docx-0015 | docx | 1-10MiB | 116 | 1674 | 3702902 | 1566733 | 5269635 | 204 | 1769472 |
| nist-epub-0001 | epub | 100KiB-1MiB | 65 | 1656 | 765011 | 168885 | 933896 | 175 | 1503232 |
| nist-epub-0002 | epub | 100KiB-1MiB | 65 | 1473 | 662935 | 117105 | 780040 | 188 | 1089536 |
| nist-epub-0003 | epub | <100KiB | 56 | 1180 | 544497 | 90209 | 634706 | 108 | 909312 |
| nist-epub-0004 | epub | 100KiB-1MiB | 58 | 932 | 1426907 | 336871 | 1763778 | 93 | 1970176 |
| nist-epub-0005 | epub | 100KiB-1MiB | 41 | 133 | 867285 | 275255 | 1142540 | 45 | 946176 |
| nist-epub-0006 | epub | 100KiB-1MiB | 15 | 74 | 614715 | 106321 | 721036 | 36 | 487424 |
| nist-epub-0007 | epub | 100KiB-1MiB | 22 | 72 | 629509 | 145977 | 775486 | 53 | 1486848 |
| nist-epub-0008 | epub | <100KiB | 9 | 47 | 365415 | 36295 | 401710 | 22 | 0 |
| nist-epub-0009 | epub | 1-10MiB | 34 | 135 | 2018063 | 1237709 | 3255772 | 52 | 2461696 |
| nist-epub-0010 | epub | 100KiB-1MiB | 18 | 79 | 649615 | 120169 | 769784 | 53 | 1323008 |

Totals over 100 docs: VOLE persistent 2667668262 B, VOLE descriptor 1704524849 B, A1 db 2891784192 B.

### Exact reconstruction (length + SHA-256 of materialized bytes)

| lane | byte-exact | of |
|---|---:|---:|
| VOLE `materialize --exact` | 97 | 100 |
| SQLite/FTS (retained source blob) | 98 | 100 |
| direct tooling (the source file) | 100 | 100 |

