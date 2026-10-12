# Phase 29.3 — incremental observation

campaign: evidence/campaigns/2026-10-12-phase29-3-incremental-879e6334

| family | fmt | change | revs | reused/nobs (rev>1) | reuse% | semantic-only invalid | incr==full | cas diverges |
|---|---|---|---|---|---|---|---|---|
| nist-sp800-34 | docx | real-draft-edit | 4 | 0/1155 | 0.0% | 0 | yes | no |
| docx-style | docx | style-table | 3 | 0/1042 | 0.0% | 1042 | yes | yes |
| xlsx-ss | xlsx | shared-string | 3 | 8/32 | 25.0% | 24 | yes | yes |
| csv-text | csv | text-edit | 3 | 23254/26643 | 87.3% | 0 | yes | no |
| json-text | json | text-edit | 3 | 6895/11643 | 59.2% | 0 | yes | no |

## Totals

| lane | obs | reused | recomputed | bytes recomputed | bytes reused | latency us | index B | persistent B |
|---|---|---|---|---|---|---|---|---|
| full | 60771 | 0 | 60771 | 2540635 | 0 | 49980107 | - | - |
| cas | 60771 | 31223 | 29548 | 1337213 | 1203254 | 50072289 | 745108 | 1127517 |
| incremental | 60771 | 30157 | 30614 | 1408687 | 1131948 | 110698576 | 745108 | 1127517 |

- dependency edges (incremental, last revision): 67486
- exactness (materialize --exact == source, fresh process): ALL OK
- correctness (incremental answer_digest == full rebuild): ALL OK
- semantic load-bearing (a physical-only CAS diverges on a style/shared-string case): YES
