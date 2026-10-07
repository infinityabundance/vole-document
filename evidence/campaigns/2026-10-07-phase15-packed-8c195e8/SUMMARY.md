# phase15-packed — packed seed store vs one-file-per-node reference

Documents: **12** (SUBSET of `real100-v1`, joined on the `(format, size_class)` strata — **not** the frozen 100-document population). Ratios are `packed / fs`; a ratio < 1 means packed is smaller/faster, > 1 means the reverse. Within ±10% of 1.0 is labelled parity. This court makes **no** claim that packed is better.

## Correctness

| check | pass | of |
|---|---:|---:|
| fs `materialize --exact` (sha256+len == manifest) | 12 | 12 |
| packed `materialize --exact` (sha256+len == manifest) | 12 | 12 |
| field id identical across the two stores | 12 | 12 |

## Ratios (packed / fs)

| metric | sum(fs) | sum(packed) | sum ratio | median per-doc ratio | reading |
|---|---:|---:|---:|---:|---|
| persistent bytes (`du -sb`) | 352671955 | 253737799 | 0.719x | 0.447x | packed smaller |
| file count (`find -type f`) | 25574 | 237 | 0.009x | 0.037x | packed smaller |
| cold observation wall (ms) | 1555 | 1541 | 0.991x | 1.000x | ≈parity |

Medians are over the 12 documents with a successful encode (bytes/files over all of them; latency only over documents where both cold reads returned rc=0).

### Per document

| id | fmt | size class | src B | fs B | packed B | B ratio | fs files | packed files | files ratio | fs cold ms | packed cold ms | lat ratio | fs exact | packed exact | field = |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|---|
| nist-docx-0013 | docx | <100KiB | 25400 | 387644 | 68304 | 0.176x | 44 | 5 | 0.114x | 21 | 21 | 1.000x | yes | yes | yes |
| nist-epub-0008 | epub | <100KiB | 35841 | 365415 | 78635 | 0.215x | 40 | 5 | 0.125x | 13 | 13 | 1.000x | yes | yes | yes |
| nist-docx-0009 | docx | 100KiB-1MiB | 152516 | 648529 | 199053 | 0.307x | 62 | 5 | 0.081x | 36 | 38 | 1.056x | yes | yes | yes |
| nist-epub-0006 | epub | 100KiB-1MiB | 105867 | 614715 | 152951 | 0.249x | 62 | 5 | 0.081x | 14 | 16 | 1.143x | yes | yes | yes |
| nist-pdf-0002 | pdf | 100KiB-1MiB | 263703 | 3722067 | 446715 | 0.120x | 586 | 10 | 0.017x | 18 | 20 | 1.111x | yes | yes | yes |
| nist-docx-0015 | docx | 1-10MiB | 1568485 | 3702902 | 3135422 | 0.847x | 77 | 5 | 0.065x | 47 | 45 | 0.957x | yes | yes | yes |
| nist-epub-0009 | epub | 1-10MiB | 1237255 | 2018063 | 1292139 | 0.640x | 102 | 5 | 0.049x | 18 | 17 | 0.944x | yes | yes | yes |
| nist-pdf-0004 | pdf | 1-10MiB | 1105061 | 4298868 | 1278964 | 0.298x | 536 | 10 | 0.019x | 19 | 21 | 1.105x | yes | yes | yes |
| nasa-epub-0006 | epub | 10-50MiB | 10867010 | 12983154 | 10988046 | 0.846x | 313 | 8 | 0.026x | 177 | 177 | 1.000x | yes | yes | yes |
| nasa-pdf-eb-09 | pdf | 10-50MiB | 11308153 | 20036634 | 11755174 | 0.587x | 1840 | 19 | 0.010x | 77 | 74 | 0.961x | yes | yes | yes |
| nasa-pdf-0029 | pdf | 50-100MiB | 54750188 | 65693704 | 51276572 | 0.781x | 3428 | 29 | 0.008x | 110 | 110 | 1.000x | yes | yes | yes |
| nasa-pdf-0024 | pdf | >100MiB | 168513117 | 238200260 | 173065824 | 0.727x | 18484 | 131 | 0.007x | 1005 | 989 | 0.984x | yes | yes | yes |

### Syscall summary (`strace -c -f`, one representative document)

The `doc-baseline` lane has `strace` but not `perf`; these are whole-process syscall totals for one cold observation per backend.

| id | backend | total seconds | total syscalls | raw |
|---|---|---:|---:|---|
| nist-docx-0013 | fs | 0.000000 | 142 | `nist-docx-0013.fs.strace.txt` |
| nist-docx-0013 | packed | 0.000000 | 152 | `nist-docx-0013.packed.strace.txt` |

