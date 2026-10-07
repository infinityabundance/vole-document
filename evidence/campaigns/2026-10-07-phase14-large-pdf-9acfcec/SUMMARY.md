# Phase 14 — large-PDF encode bound

Frozen architecture, no tuning, over the real100-v1 PDFs >= 100 MB in the capped doc-baseline lane (6 GiB, /usr/bin/time -v).

| id | source bytes | wall ms | peak RSS KB | rc | candidate | exact |
|---|---:|---:|---:|---:|---|---|
| nasa-pdf-0001 | 408854600 | 3300 | 6281148 | 137 | no |  |
| nasa-pdf-0002 | 308803168 | 276390 | 5355072 | 0 | candidate:BYTE_RANS | yes |
| nasa-pdf-0003 | 217666672 | 236250 | 3757704 | 0 | candidate:BYTE_RANS | yes |
| nasa-pdf-0020 | 178050443 | 15560 | 3118560 | 0 | candidate:PDF_COS_TEMPLATE | yes |
| nasa-pdf-0024 | 168513117 | 38680 | 2954884 | 0 | candidate:PDF_CHANNELS | yes |

rc 137 = SIGKILL (peak RSS 6.28 GiB exceeded the 6 GiB cap); rc 124 = 180 s timeout.
Before/after (all-features, same machine): nasa-pdf-0003 395 s -> 236 s; nasa-pdf-0002 timeout -> 276 s completed; nasa-pdf-0024 38 s; nasa-pdf-0020 16 s.
Peak RSS is unchanged (~17x input): the memory bound is recorded, not solved (ADR-0041).
