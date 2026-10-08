# Phase 23 — power-loss proxy: durability matrix (model)

Source: `proxy.tsv`. Cases: **32** (PASS 20 / FAIL 0 / CRITICAL 12).  
Shipped-arm CRITICAL: **0** (must be 0)   Counterfactual (`model-drop`) CRITICAL: **12** (expected, proves sensitivity).  
Wrong-byte nodes served (sum `bad_hash`): **0**.  

## Grid (backend x policy x dir_sync x arm)

| backend | policy | dir_sync | arm | PASS | CRITICAL | total |
|---|---|---|---|---|---|---|
| fs | batch | safe | complete | 1 | 0 | 1 |
| fs | batch | safe | model-drop | 0 | 3 | 3 |
| fs | batch | safe | cut | 1 | 0 | 1 |
| fs | batch | off | complete | 1 | 0 | 1 |
| fs | each | safe | complete | 1 | 0 | 1 |
| fs | each | safe | model-drop | 0 | 3 | 3 |
| fs | each | safe | cut | 1 | 0 | 1 |
| fs | each | off | complete | 1 | 0 | 1 |
| packed | batch | safe | complete | 1 | 0 | 1 |
| packed | batch | safe | model-drop | 0 | 3 | 3 |
| packed | batch | safe | cut | 5 | 0 | 5 |
| packed | batch | off | complete | 1 | 0 | 1 |
| packed | each | safe | complete | 1 | 0 | 1 |
| packed | each | safe | model-drop | 0 | 3 | 3 |
| packed | each | safe | cut | 5 | 0 | 5 |
| packed | each | off | complete | 1 | 0 | 1 |

`complete` and `cut` are the shipped arms. `model-drop` arms are
counterfactual model faults (a directory barrier is discarded) and must be
CRITICAL, or the model is not sensitive to the GAP-1 class of bug.

## GAP 1 — directory-fsync cost (median ms; bind = /work, tmp = /tmp)

| backend | dir_sync | bind median | tmp median | reps |
|---|---|---|---|---|
| fs | safe | 1381 | 18 | 3 |
| fs | off | 723 | 17 | 3 |
| packed | safe | 34 | 9 | 3 |
| packed | off | 22 | 9 | 3 |

## Batch vs Each at `flush.before_sync` (packed, open state per policy)

| policy | store state at the cut |
|---|---|
| batch |  reopens_okx1 |
| each |  reopens_okx1 |
