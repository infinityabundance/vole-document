# Phase 22.3.0 — duplicate work across a known batch (deterministic counters)

Lane: `indep` disables the typed-model memo AND the derived cache (`--no-cache` on every request line); `resident` is the shipping default. Both start from a cold cache. Stores built once per document.

## Per document

| id | fmt | req/ans/dec | indep exec | resident exec | exec_dedup | idx_dedup | seed_bytes_dedup | cache_write_dedup | resident reused |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|
| nist-docx-0005 | docx | 8/5/3 | 33 | 9 | 3.6667 | 1.0000 | 4.0000 | 3.9996 | 0 |
| nist-docx-0008 | docx | 8/5/3 | 33 | 9 | 3.6667 | 1.0000 | 4.0000 | 3.9990 | 0 |
| nist-docx-0009 | docx | 8/5/3 | 33 | 9 | 3.6667 | 1.0000 | 4.0000 | 3.9999 | 0 |
| nist-docx-0014 | docx | 8/7/1 | 49 | 9 | 5.4444 | 1.0000 | 6.0000 | 5.9999 | 0 |
| nist-epub-0003 | epub | 8/7/1 | 193 | 48 | 4.0208 | 1.0000 | 1.8990 | 7.3801 | 62 |
| nist-epub-0006 | epub | 8/7/1 | 153 | 42 | 3.6429 | 1.0000 | 1.8030 | 12.5953 | 48 |
| nist-epub-0008 | epub | 8/7/1 | 103 | 24 | 4.2917 | 1.0000 | 1.9091 | 10.8996 | 34 |
| nist-epub-0009 | epub | 8/7/1 | 198 | 63 | 3.1429 | 1.0000 | 1.6892 | 27.7516 | 59 |
| nist-pdf-0002 | pdf | 4/4/0 | 7 | 7 | 1.0000 | 1.0000 | 0.9637 | 1.0000 | 0 |
| nist-pdf-0004 | pdf | 4/4/0 | 7 | 7 | 1.0000 | 1.0000 | 0.9572 | 1.0000 | 0 |
| nist-pdf-0016 | pdf | 4/4/0 | 7 | 7 | 1.0000 | 1.0000 | 0.9175 | 1.0000 | 0 |
| nist-pdf-0017 | pdf | 4/4/0 | 5 | 5 | 1.0000 | 1.0000 | 0.9982 | 1.0000 | 0 |

## Per format (ratio of sums)

| fmt | docs | indep exec | resident exec | exec_dedup | idx_dedup | seed_bytes_dedup | cache_write_dedup |
|---|---:|---:|---:|---:|---:|---:|---:|
| docx | 4 | 148 | 36 | 4.1111 | 1.0000 | 4.5000 | 5.1248 |
| epub | 4 | 647 | 177 | 3.6554 | 1.0000 | 1.8090 | 21.0487 |
| pdf | 4 | 26 | 26 | 1.0000 | 1.0000 | 0.9913 | 1.0000 |

## Closure proxy (SECONDARY — `dependency_ids`; unreliable, see SUMMARY)

| id | indep sum_deps | indep |U| | struct_dup_proxy | null sum_deps | null |U| | null_dup_proxy |
|---|---:|---:|---:|---:|---:|---:|
| nist-docx-0005 | 12 | 3 | 4.0000 | 6 | 3 | 2.0000 |
| nist-docx-0008 | 12 | 3 | 4.0000 | 6 | 3 | 2.0000 |
| nist-docx-0009 | 12 | 3 | 4.0000 | 6 | 3 | 2.0000 |
| nist-docx-0014 | 18 | 3 | 6.0000 | 6 | 3 | 2.0000 |
| nist-epub-0003 | 6 | 6 | 1.0000 | 4 | 2 | 2.0000 |
| nist-epub-0006 | 6 | 6 | 1.0000 | 4 | 2 | 2.0000 |
| nist-epub-0008 | 6 | 6 | 1.0000 | 4 | 2 | 2.0000 |
| nist-epub-0009 | 6 | 6 | 1.0000 | 4 | 2 | 2.0000 |
| nist-pdf-0002 | 4 | 4 | 1.0000 | 6 | 6 | 1.0000 |
| nist-pdf-0004 | 4 | 4 | 1.0000 | 6 | 6 | 1.0000 |
| nist-pdf-0016 | 4 | 4 | 1.0000 | 6 | 6 | 1.0000 |
| nist-pdf-0017 | 4 | 4 | 1.0000 | 6 | 6 | 1.0000 |

**Verdict: SESSION-ALREADY-CAPTURES**

