# Phase 30.3 — incremental observation

campaign: evidence/campaigns/2026-10-13-phase30-3-incremental-f3765c2b

| family | fmt | change | revs | reused/nobs (rev>1) | reuse% | semantic-only invalid | incr==full | cas diverges | index B | sigs | index/src % |
|---|---|---|---|---|---|---|---|---|---|---|---|
| nist-sp800-34 | docx | real-draft-edit | 4 | 0/1155 | 0.0% | 0 | yes | no | 5684 | 2 | 3.07% |
| docx-style | docx | style-table | 3 | 0/1042 | 0.0% | 1042 | yes | yes | 17822 | 2 | 10.98% |
| xlsx-ss | xlsx | shared-string | 3 | 8/32 | 25.0% | 24 | yes | yes | 684 | 3 | 2.92% |
| csv-text | csv | text-edit | 3 | 23254/26643 | 87.3% | 0 | yes | no | 453904 | 54 | 38.48% |
| json-text | json | text-edit | 3 | 6895/11643 | 59.2% | 0 | yes | no | 199622 | 52 | 16.92% |

## Totals

| lane | obs | reused | recomputed | bytes recomputed | bytes reused | latency us | index B | persistent B |
|---|---|---|---|---|---|---|---|---|
| full | 60771 | 0 | 60771 | 2540635 | 0 | 48630613 | - | - |
| cas | 60771 | 31223 | 29548 | 1337213 | 1203254 | 48521168 | 199622 | 582031 |
| incremental | 60771 | 30157 | 30614 | 1408687 | 1131948 | 48564923 | 199622 | 582031 |

- dependency edges (incremental, last revision): 67486
- index signatures interned (incremental, last revision): 52
- index bytes (controlling family, last revision): 199622
- index bytes (sum over families, last revision): 677716
- exactness (materialize --exact == source, fresh process): ALL OK
- correctness (incremental answer_digest == full rebuild): ALL OK
- semantic load-bearing (a physical-only CAS diverges on a style/shared-string case): YES
