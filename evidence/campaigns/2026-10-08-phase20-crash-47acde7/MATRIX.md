# Phase 20.1 crash court — injection x outcome matrix

Source: `evidence/campaigns/2026-10-08-phase20-crash-47acde7/raw/cases.tsv`  
Total cases: **1300** (PASS 1300 / FAIL 0 / CRITICAL 0)  
Wrong-bytes nodes served (sum `bad_hash`): **0**   Prefix violations: **0**   Nodes not served (re-hash mismatch or truncated away, `fetch_fail`): **212812**  

## Injection x outcome

| injection | no-manifest/reopens | no-manifest/fail-closed | manifest/exact-ok | manifest/fail-closed | manifest/other | CRITICAL | total |
|---|---|---|---|---|---|---|---|
| `sigkill/sigabrt-at-delay` | 67 | 1 | 860 | 0 | 0 | 0 | **928** |
| `sigkill/sigabrt-at-boundary` | 0 | 0 | 96 | 0 | 0 | 0 | **96** |
| `fault-inject-abort` | 28 | 0 | 12 | 0 | 0 | 0 | **40** |
| `truncate_pack` | 0 | 0 | 4 | 60 | 0 | 0 | **64** |
| `truncate_idx` | 0 | 0 | 0 | 44 | 0 | 0 | **44** |
| `flip_pack` | 0 | 0 | 32 | 0 | 0 | 0 | **32** |
| `flip_idx` | 0 | 0 | 10 | 34 | 0 | 0 | **44** |
| `zero_pack_tail` | 0 | 0 | 20 | 0 | 0 | 0 | **20** |
| `drop_idx` | 0 | 0 | 4 | 0 | 0 | 0 | **4** |
| `drop_idx_truncate_pack` | 0 | 0 | 4 | 24 | 0 | 0 | **28** |

## Injection x verdict, split by sync policy

| injection | batch PASS/FAIL/CRIT | each PASS/FAIL/CRIT |
|---|---|---|
| `sigkill/sigabrt-at-delay` | 464/0/0 | 464/0/0 |
| `sigkill/sigabrt-at-boundary` | 48/0/0 | 48/0/0 |
| `fault-inject-abort` | 20/0/0 | 20/0/0 |
| `truncate_pack` | 32/0/0 | 32/0/0 |
| `truncate_idx` | 22/0/0 | 22/0/0 |
| `flip_pack` | 16/0/0 | 16/0/0 |
| `flip_idx` | 22/0/0 | 22/0/0 |
| `zero_pack_tail` | 10/0/0 | 10/0/0 |
| `drop_idx` | 2/0/0 | 2/0/0 |
| `drop_idx_truncate_pack` | 14/0/0 | 14/0/0 |

## Family x verdict

| family | PASS | FAIL | CRITICAL |
|---|---|---|---|
| A process death | 1024 | 0 | 0 |
| B storage corruption | 236 | 0 | 0 |
| C deterministic abort | 40 | 0 | 0 |

Every non-`CRITICAL` cell is a pass: either the store reopens and its
recovered set re-hashes exactly, or it fails closed with a typed error.
`manifest/fail-closed` counts a published field whose seed nodes were
tampered: `materialize --exact` or the cold observation declined with a
typed error rather than returning altered bytes. A `CRITICAL` would mean a
corrupted store served bytes that do not match the requested id, or a
published manifest materialized/observed content differing from the clean
source.

A `manifest/exact-ok` cell means the field stayed serviceable through its
descriptor (`materialize --exact`) and the selected cold observation. A body
flip in a seed node outside the observation closure is still caught when
the node is enumerated, by the unchanged whole-node re-hash gate (counted in
`fetch_fail`); `exact-ok` never means wrong bytes were served.
