# Phase 23 — durability: directory fsync (GAP 1) + a model-based power-loss proxy (GAP 2)

## Question

Phase 20.1 left two gaps open: (1) `write_atomic` never `fsync`ed the
containing **directory**, so a rename could be lost across a power cut even
though the file's contents were synced; (2) process death does not evict the
page cache, so **no `fsync` boundary was exercised** and `SyncPolicy::Batch`
and `Each` were indistinguishable — not evidence that batching is power-safe.

## GAP 1 — the change and its measured cost

Every atomic writer now dir-fsyncs after the rename and on new segment/index
creation: `field::write_atomic`, `FsSeedStore::put_node`,
`FsIndexStore::put`, and the packed writer's `ensure_open`/`seal`. The
default is `DirSyncPolicy::Safe`; `--dir-sync=off` is the explicit escape
hatch used only to measure and to show the barrier is load-bearing.

| backend | dir_sync | filesystem | reps | median ms | min | max |
|---|---|---|---|---|---|---|
| fs | off | tmp | 3 | 17 | 17 | 18 |
| fs | off | bind | 3 | 723 | 711 | 743 |
| fs | safe | tmp | 3 | 18 | 18 | 19 |
| fs | safe | bind | 3 | 1381 | 1381 | 1409 |
| packed | off | tmp | 3 | 9 | 9 | 9 |
| packed | off | bind | 3 | 22 | 22 | 23 |
| packed | safe | tmp | 3 | 9 | 9 | 9 |
| packed | safe | bind | 3 | 34 | 33 | 37 |

Cost is measured with the plain default-feature binary (no barrier-log
overhead), median of `3` fresh ingests of `real100-v1/documents/nist/pdf/nist-pdf-0017.pdf` (the fs store
writes one dir-fsynced file per seed/index node; the packed store dir-fsyncs
once per new segment and per sealed index).

## GAP 2 — the model-based power-loss proxy

Under the non-default `power-log` feature every durability barrier is logged
in program order with the byte range it covered (`fsync <path> <len>`), plus
`create`/`rename`/`dirsync`. `tests/power_loss_proxy.rs` folds that log
into a per-path durability model — a file survives only if its create/rename
was followed by a parent `dirsync`, and its content is truncated to the length
at its last completed file barrier (0 if none) — then checks the Phase-20.1
invariants against the reconstructed store: with no manifest it reopens, no
partial node is fetchable, every present id re-hashes; with a manifest the root
node and all index nodes exist and `materialize --exact` equals the source
(length + byte compare, hence SHA-256), and cold observations match a clean
baseline. Batch and Each x fs and packed, complete and cut (abort at
`record.after_body`, `flush.*`, `seal.before_idx`, `manifest.after_publish`).

## Result

PASS — cases 32, PASS 20, FAIL 0, CRITICAL 12.
Shipped-arm CRITICAL: **0**. Counterfactual (`model-drop`)
CRITICAL: **12** (expected; proves the model is sensitive).
Complete `safe` ingests with a surviving manifest: 4/4.
Complete `off` ingests whose published store was entirely lost: 4/4.

## Critical findings

None. Under the model, no shipped arm (`safe` or `off`, complete or cut)
lost or corrupted a published field: every present id re-hashed, every cut
store that could not serve failed closed with a typed error, and every
reconstructed published manifest materialized exactly. The `model-drop` arms
discard a directory barrier and are all CRITICAL, so the model would have
flagged the pre-GAP-1 code (which did exactly that).

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

## What the model does and does not prove

**Proves (under the model).** With `DirSyncPolicy::Safe`, a completed ingest's
published manifest and the root/index nodes it references survive a simulated
power loss; with `--dir-sync=off` the same run's entire published store is
reconstructed as lost — so the directory fsync is required for the invariant,
not decorative. Batch and Each are equivalent **for the published-manifest
invariant** because `put_field` flushes the open segment before publishing;
they differ only for the unreferenced open-segment tail (visible at the
`flush.before_sync` cut: Batch loses the appended records, Each keeps them).

**Does not prove.** This is a MODEL derived from a barrier log, not a real
power cut: it does not exercise the storage device's volatile write cache, the
filesystem journal, torn sectors, or a real directory-entry loss, and it assumes
directory metadata is durable except for the file-entry barriers it tracks. A
physical proxy inside the pinned, unprivileged, hard-capped `dev` lane was not
available: `drop_caches` cannot discard dirty (un-fsynced) pages and
`dm-flakey`/loopback fault devices need privileges the lane does not have.
Residual: a true hardware power cut could still lose a rename that the kernel
had not yet written to the directory, or reorder writes the device coalesced,
in ways a software model cannot see.

## Gates

fmt=0 clippy=0 test-all-features=0 test-no-default=0 phase1=0 (0 = pass, -1 = skipped).

Receipt: `evidence/campaigns/2026-10-08-phase23-durability-fc63436-phase25/receipt.json`. Raw: `evidence/campaigns/2026-10-08-phase23-durability-fc63436-phase25/raw/proxy.tsv`, `evidence/campaigns/2026-10-08-phase23-durability-fc63436-phase25/raw/dirsync-timing.tsv`, `evidence/campaigns/2026-10-08-phase23-durability-fc63436-phase25/raw/proxy.log`.
