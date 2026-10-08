# Phase 19.3 — NEW direct build path over the FULL frozen real100-v1

Robustness of `field-build SRC --store DIR --profile runtime --packed --sync=batch` (one process, fixed `runtime` program, no candidate search) across every document of the frozen `real100-v1` corpus. Build wall and peak RSS are measured with `/usr/bin/time -v` under `timeout` (rc 124 = timeout, 137 = OOM/SIGKILL). Persistent bytes are the sum of **regular-file** sizes only; `du -sb` is never used. The OLD path's bytes in `2026-10-07-real100-release-baseline-866f489` and `2026-10-08-phase16-packed-full-0b21928` were `du -sb` (directory-inode inflated) and are **not** compared byte-for-byte; the comparison below uses the file-size-corrected rows of `2026-10-08-phase16-storage-correction-2978e1d`.

Documents measured: **100**. Build successes: **100/100**.

## 1. Build success (rc 0) by format and size class

| group | docs | build ok | rate |
|---|---:|---:|---:|
| all | 100 | 100 | 100.0% |
| pdf | 60 | 60 | 100.0% |
| docx | 15 | 15 | 100.0% |
| epub | 25 | 25 | 100.0% |
| size:<100KiB | 8 | 8 | 100.0% |
| size:100KiB-1MiB | 17 | 17 | 100.0% |
| size:1-10MiB | 40 | 40 | 100.0% |
| size:10-50MiB | 20 | 20 | 100.0% |
| size:50-100MiB | 10 | 10 | 100.0% |
| size:>100MiB | 5 | 5 | 100.0% |

## 2. Build wall + peak RSS by format and size class (successes only)

| group | docs | median ms | sum ms | median RSS MiB | max RSS MiB |
|---|---:|---:|---:|---:|---:|
| all | 100 | 64 | 221,408 | 30.9 | 2342.0 |
| pdf | 60 | 307 | 218,619 | 63.7 | 2342.0 |
| docx | 15 | 10 | 242 | 4.4 | 16.4 |
| epub | 25 | 51 | 2,547 | 28.2 | 270.7 |
| size:<100KiB | 8 | 8 | 70 | 3.8 | 3.9 |
| size:100KiB-1MiB | 17 | 10 | 320 | 5.0 | 13.0 |
| size:1-10MiB | 40 | 55 | 5,899 | 22.8 | 394.7 |
| size:10-50MiB | 20 | 370 | 9,503 | 130.6 | 270.7 |
| size:50-100MiB | 10 | 1,406 | 24,562 | 441.0 | 511.0 |
| size:>100MiB | 5 | 5,055 | 181,054 | 1248.2 | 2342.0 |

## 3. Persistent store by format and size class (successes only)

| group | docs | sum B | median B | sum files | sum dirs | vs source |
|---|---:|---:|---:|---:|---:|---:|
| all | 100 | 2,829,898,049 | 4,852,533 | 2,691 | 4,610 | 1.036× |
| pdf | 60 | 2,517,071,837 | 9,923,782 | 2,459 | 4,226 | 1.025× |
| docx | 15 | 14,379,000 | 174,848 | 75 | 120 | 1.167× |
| epub | 25 | 298,447,212 | 4,388,433 | 157 | 264 | 1.129× |
| size:<100KiB | 8 | 510,990 | 58,419 | 40 | 64 | 1.266× |
| size:100KiB-1MiB | 17 | 6,866,414 | 291,457 | 118 | 190 | 1.202× |
| size:1-10MiB | 40 | 175,018,249 | 3,407,386 | 583 | 1,010 | 1.076× |
| size:10-50MiB | 20 | 579,382,458 | 25,027,755 | 395 | 704 | 1.077× |
| size:50-100MiB | 10 | 762,592,952 | 77,817,245 | 730 | 1,285 | 1.027× |
| size:>100MiB | 5 | 1,305,526,986 | 226,532,666 | 825 | 1,357 | 1.018× |

## 4. Exact reconstruction (length + SHA-256) by format

| format | build ok | materialize ok | of build ok | exact rc≠0 |
|---|---:|---:|---:|---:|
| pdf | 60 | 60 | 100.0% | 0 |
| docx | 15 | 15 | 100.0% | 0 |
| epub | 25 | 25 | 100.0% | 0 |
| (all) | 100 | 100 | 100.0% | 0 |

## 5. Cold-observation coverage (answered / declined)

Coverage is measured only on documents whose build succeeded (no field ⇒ no observation). `text` = `--page 1 --kind text` (pdf) / `--block 0 --kind text` (docx/epub); `metadata` = `--metadata --kind metadata`.

| format | obs | asked | answered | declined | answered rate |
|---|---|---:|---:|---:|---:|
| pdf | text | 60 | 54 | 6 | 90.0% |
| pdf | metadata | 60 | 60 | 0 | 100.0% |
| docx | text | 15 | 13 | 2 | 86.7% |
| docx | metadata | 15 | 13 | 2 | 86.7% |
| epub | text | 25 | 25 | 0 | 100.0% |
| epub | metadata | 25 | 25 | 0 | 100.0% |
| (all) | text | 100 | 92 | 8 | 92.0% |
| (all) | metadata | 100 | 98 | 2 | 98.0% |

## 6. Failures (never hidden)

Build failures: **0/100**.

No successful build failed exact reconstruction.

## 7. Comparison against the OLD path (file-size-corrected)

The old path is the two-step `encode` + `field-ingest --packed`. Its persistent bytes here come from the phase16 storage-correction campaign, measured in the SAME unit as this court (sum of regular-file sizes). The `du -sb` figures in the release-baseline and phase16-packed-full campaigns are **not** used.

Common documents (new build ok ∧ old packed ingest ok): **96**.

| quantity | NEW `field-build --packed --sync=batch` | OLD `encode`+`field-ingest --packed` |
|---|---:|---:|
| persistent bytes (sum) | 1,830,073,797 | 1,738,386,483 |
| regular files (sum) | 2,011 | 2,010 |
| build wall (sum ms) | 45,979 | 1,066,335 |
| build wall (median ms) | 61 | 2,095 |

- **persistent bytes: NEW 1.053× OLD** on the common set (1,830,073,797 vs 1,738,386,483 B); the new direct path is **never smaller** than the old searched path per document.
- packed stores byte-identical (new vs old): **15/96** documents. The packed *format* is unchanged; where both paths select the same program the writer reproduces the old bytes exactly.
- build-wall paired ratio NEW/OLD: median **0.083×** over 96 documents
- source bytes (common docs): 1,750,917,648; new store 1,830,073,797 B, old store 1,738,386,483 B.

### Why the stores differ where they do (control)

The old path's `encode` **searches candidate programs**; the new `field-build --profile runtime` **fixes** the runtime/RAW program. The control (`raw/control-oldpath.json`) re-runs the OLD two-step commands with the CURRENT binary on two mismatching documents:

| id | current-search candidate | current search B | old B | new field-build B |
|---|---|---:|---:|---:|
| nist-docx-0005 | BYTE_RANS | 65,713 | 65,713 | 65,901 |
| nasa-pdf-0015 | PDF_CHANNELS | 41,613,002 | 41,604,319 | 44,744,347 |

The new direct path trades a small persistent-storage cost for skipping the candidate search: it stores the fixed runtime/RAW program, so its packed store is >= the old searched store on every common document (aggregate +5.3% on the 96 common docs). Where both paths pick the same program the stores are byte-identical (15/96, all epub).

### Old-path build success vs the new direct path

- OLD two-step `encode`: **97/100** builds succeeded (rc histogram {'0': 97, '124': 2, '137': 1}); `packed_ingest_ok` = 97.
- NEW direct `field-build`: **100/100** succeeded, including every document the OLD path failed.
- OLD `encode` failures, all recovered by the new direct path: `nasa-pdf-0001` (pdf, 408854600 B, rc 137), `nasa-pdf-0002` (pdf, 308803168 B, rc 124), `nasa-pdf-0003` (pdf, 217666672 B, rc 124).

## 8. Honest robustness picture

- **Build success: 100/100** (100.0%). Median build wall 64 ms, sum 221,408 ms; peak RSS median 30.9 MiB, max 2342.0 MiB (6 GiB cap).
- **Exactness: 100/100** successful builds close byte-exactly (length + SHA-256). It holds on every success.
- **Cold coverage: 190/200** observations answered (95.0%). Declines are typed and recorded (see §5).
- **Persistent store: 2,829,898,049 B** over 100 docs = **1.036×** the source bytes (2,731,242,321 B); 2,691 regular files, 4,610 directories.
- **No build failures.**
- **Coverage declines (typed; nobody failed to build or materialize):**
    - rc 6 (unsupported-feature): nasa-pdf-eb-01:text, nasa-pdf-eb-02:text, nasa-pdf-eb-06:text, nasa-pdf-eb-07:text, nasa-pdf-eb-08:text, nasa-pdf-eb-10:text.
    - rc 20 (InvalidPackageStructure): nist-docx-0011:text, nist-docx-0011:metadata, nist-docx-0012:text, nist-docx-0012:metadata.

