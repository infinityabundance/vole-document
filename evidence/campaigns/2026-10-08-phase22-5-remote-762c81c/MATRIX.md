# Phase 22.5 — per-observation matrix (MODEL: remote-selective-v1, 2026-10-08)

Bytes / requests are REAL local measurements; latency is MODELLED (T = N_requests * RTT + B_transferred / BW + C_decode  (sequential, P=1)).
`bytes_ratio` = VOLE selective bytes / SQLite page-level bytes (<1 favours VOLE).
`lat_ratio` = VOLE / SQLite modelled latency at the PRIMARY profile.

| id | fmt | size | obs | VOLE B | VOLE req | SQL B | SQL req | store B | db B | bytes_ratio | lat_ratio | VOLE sel. | SQL sel. | verdict |
|---|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| nist-pdf-0002 | pdf | 100KiB-1MiB | text | 273239 | 3 | 20480 | 3 | 381882 | 671744 | 13.34 | 1.08 | 0.716 | 0.030 | loss |
| nist-pdf-0002 | pdf | 100KiB-1MiB | bytes | 264466 | 1 | 274432 | 3 | 381882 | 671744 | 0.96 | 0.36 | 0.693 | 0.409 | unresolved |
| nist-pdf-0002 | pdf | 100KiB-1MiB | metadata | 264466 | 1 | 8192 | 1 | 381882 | 671744 | 32.28 | 1.06 | 0.693 | 0.012 | loss |
| nist-pdf-0004 | pdf | 1-10MiB | text | 1114599 | 3 | 20480 | 3 | 1216084 | 1712128 | 54.42 | 1.22 | 0.917 | 0.012 | loss |
| nist-pdf-0004 | pdf | 1-10MiB | bytes | 1105826 | 1 | 1118208 | 3 | 1216084 | 1712128 | 0.99 | 0.43 | 0.909 | 0.653 | unresolved |
| nist-pdf-0004 | pdf | 1-10MiB | metadata | 1105826 | 1 | 8192 | 1 | 1216084 | 1712128 | 134.99 | 1.44 | 0.909 | 0.005 | loss |
| nist-pdf-0016 | pdf | 100KiB-1MiB | text | 320078 | 3 | 20480 | 3 | 345470 | 417792 | 15.63 | 1.11 | 0.927 | 0.049 | loss |
| nist-pdf-0016 | pdf | 100KiB-1MiB | bytes | 311389 | 1 | 319488 | 3 | 345470 | 417792 | 0.97 | 0.36 | 0.901 | 0.765 | unresolved |
| nist-pdf-0016 | pdf | 100KiB-1MiB | metadata | 311389 | 1 | 8192 | 1 | 345470 | 417792 | 38.01 | 1.08 | 0.901 | 0.020 | loss |
| nist-pdf-0017 | pdf | 1-10MiB | text | 11038 | 4 | 20480 | 3 | 2841174 | 1966080 | 0.54 | 1.36 | 0.004 | 0.010 | unresolved |
| nist-pdf-0017 | pdf | 1-10MiB | bytes | 1467009 | 1 | 1478656 | 3 | 2841174 | 1966080 | 0.99 | 0.46 | 0.516 | 0.752 | unresolved |
| nist-pdf-0017 | pdf | 1-10MiB | metadata | 1467009 | 1 | 8192 | 1 | 2841174 | 1966080 | 179.08 | 1.60 | 0.516 | 0.004 | loss |
| nist-docx-0005 | docx | <100KiB | text | 60070 | 2 | 24576 | 4 | 65901 | 262144 | 2.44 | 0.52 | 0.912 | 0.094 | loss |
| nist-docx-0005 | docx | <100KiB | bytes | 57841 | 1 | 65536 | 3 | 65901 | 262144 | 0.88 | 0.33 | 0.878 | 0.250 | unresolved |
| nist-docx-0005 | docx | <100KiB | resource | - | - | - | - | 65901 | 262144 | - | - | - | - | declined(rc=6,sql_decl=1) |
| nist-docx-0008 | docx | <100KiB | text | 31808 | 2 | 20480 | 3 | 37639 | 126976 | 1.55 | 0.67 | 0.845 | 0.161 | loss |
| nist-docx-0008 | docx | <100KiB | bytes | 29579 | 1 | 40960 | 3 | 37639 | 126976 | 0.72 | 0.33 | 0.786 | 0.323 | unresolved |
| nist-docx-0008 | docx | <100KiB | resource | - | - | - | - | 37639 | 126976 | - | - | - | - | declined(rc=6,sql_decl=1) |
| nist-docx-0009 | docx | 100KiB-1MiB | text | 157081 | 2 | 20480 | 3 | 166832 | 483328 | 7.67 | 0.84 | 0.942 | 0.042 | loss |
| nist-docx-0009 | docx | 100KiB-1MiB | bytes | 153312 | 1 | 163840 | 3 | 166832 | 483328 | 0.94 | 0.34 | 0.919 | 0.339 | unresolved |
| nist-docx-0009 | docx | 100KiB-1MiB | resource | - | - | - | - | 166832 | 483328 | - | - | - | - | declined(rc=6,sql_decl=1) |
| nist-docx-0014 | docx | 100KiB-1MiB | text | 953868 | 2 | 24576 | 4 | 1194783 | 1564672 | 38.81 | 0.78 | 0.798 | 0.016 | loss |
| nist-docx-0014 | docx | 100KiB-1MiB | bytes | 949219 | 1 | 958464 | 3 | 1194783 | 1564672 | 0.99 | 0.42 | 0.794 | 0.613 | unresolved |
| nist-docx-0014 | docx | 100KiB-1MiB | resource | 953868 | 2 | 12288 | 2 | 1194783 | 1564672 | 77.63 | 1.54 | 0.798 | 0.008 | loss |
| nist-epub-0003 | epub | <100KiB | text | 93863 | 2 | 24576 | 4 | 102901 | 593920 | 3.82 | 0.51 | 0.912 | 0.041 | loss |
| nist-epub-0003 | epub | <100KiB | bytes | 90551 | 1 | 98304 | 3 | 102901 | 593920 | 0.92 | 0.33 | 0.880 | 0.166 | unresolved |
| nist-epub-0003 | epub | <100KiB | resource | 93553 | 2 | 12288 | 2 | 102901 | 593920 | 7.61 | 0.99 | 0.909 | 0.021 | loss |
| nist-epub-0006 | epub | 100KiB-1MiB | text | 110305 | 2 | 20480 | 3 | 120183 | 327680 | 5.39 | 0.67 | 0.918 | 0.062 | loss |
| nist-epub-0006 | epub | 100KiB-1MiB | bytes | 106663 | 1 | 114688 | 3 | 120183 | 327680 | 0.93 | 0.34 | 0.888 | 0.350 | unresolved |
| nist-epub-0006 | epub | 100KiB-1MiB | resource | 109995 | 2 | 12288 | 2 | 120183 | 327680 | 8.95 | 1.00 | 0.915 | 0.037 | loss |
| nist-epub-0008 | epub | <100KiB | text | 39069 | 2 | 20480 | 3 | 45867 | 139264 | 1.91 | 0.66 | 0.852 | 0.147 | loss |
| nist-epub-0008 | epub | <100KiB | bytes | 36637 | 1 | 45056 | 3 | 45867 | 139264 | 0.81 | 0.33 | 0.799 | 0.324 | unresolved |
| nist-epub-0008 | epub | <100KiB | resource | 38759 | 2 | 12288 | 2 | 45867 | 139264 | 3.15 | 0.98 | 0.845 | 0.088 | loss |
| nist-epub-0009 | epub | 1-10MiB | text | 1243893 | 2 | 24576 | 4 | 1259371 | 1933312 | 50.61 | 0.66 | 0.988 | 0.013 | loss |
| nist-epub-0009 | epub | 1-10MiB | bytes | 1238051 | 1 | 1249280 | 3 | 1259371 | 1933312 | 0.99 | 0.45 | 0.983 | 0.646 | unresolved |
| nist-epub-0009 | epub | 1-10MiB | resource | 1243583 | 2 | 12288 | 2 | 1259371 | 1933312 | 101.20 | 1.28 | 0.987 | 0.006 | loss |
