# Phase 20.1 — crash / power-cut fault-injection court

## Question

Does the packed seed store (`fieldpack/`, ADR-0053), under **both**
`SyncPolicy::Batch` (default) and `SyncPolicy::Each`, hold its recovery
invariants across an arbitrary crash at every meaningful durability
boundary — and is the recovered set exactly a prefix of the appended
record sequence, with a published manifest always consistent?

## Result

**PASS** — 1300 cases, PASS 1300 / FAIL 0 / CRITICAL 0.  
Sum of `bad_hash` (fetched bytes not matching the requested id): **0**.  
Nodes not served by the whole-node re-hash gate / truncated reads (`fetch_fail`): **212812** across **110** case(s).  
Prefix-resolution violations: **0**.  
Family-A stores left unusable-but-fail-closed (no manifest): **1**.  

## Failure / critical findings

None. No injection made the store serve bytes not matching the
requested id; every published manifest materialized exactly or declined
with a typed error.

Robustness observation (not a correctness failure): **1** family-A case(s)
left the store **unusable** but with **no** published manifest — a `SIGKILL`
inside the `PackWriter::ensure_open` window, after the `.pack` was created
but before its 24-byte header completed. The store then fails **closed** with a
typed `IntegrityMismatch` ("has a truncated header") on every later open and
never returns bytes. It is recoverable only by removing the sub-header stray
segment. This is the contract-permitted "detected, fails closed" outcome;
it is recorded, not hidden.

## Injection matrix

| injection | no-manifest reopens | no-manifest fail-closed | manifest exact-ok | manifest fail-closed | manifest other | CRITICAL | total |
|---|---|---|---|---|---|---|---|
| `sigkill/sigabrt-at-delay` | 67 | 1 | 860 | 0 | 0 | 0 | 928 |
| `sigkill/sigabrt-at-boundary` | 0 | 0 | 96 | 0 | 0 | 0 | 96 |
| `fault-inject-abort` | 28 | 0 | 12 | 0 | 0 | 0 | 40 |
| `truncate_pack` | 0 | 0 | 4 | 60 | 0 | 0 | 64 |
| `truncate_idx` | 0 | 0 | 0 | 44 | 0 | 0 | 44 |
| `flip_pack` | 0 | 0 | 32 | 0 | 0 | 0 | 32 |
| `flip_idx` | 0 | 0 | 10 | 34 | 0 | 0 | 44 |
| `zero_pack_tail` | 0 | 0 | 20 | 0 | 0 | 0 | 20 |
| `drop_idx` | 0 | 0 | 4 | 0 | 0 | 0 | 4 |
| `drop_idx_truncate_pack` | 0 | 0 | 4 | 24 | 0 | 0 | 28 |

## Batch vs Each

batch: 650 cases (650 PASS / 0 FAIL / 0 CRITICAL).  
each:  650 cases (650 PASS / 0 FAIL / 0 CRITICAL).  

The two policies produce the same verdict distribution. This is expected
*for this injection model* and is **not** evidence that batching is safe
under power loss: a `SIGKILL` does not evict the page cache, so no
`fsync`/`fdatasync` boundary is actually exercised (see scope).

## Assertions checked after every injection

* No published manifest → the store reopens (read-only and read-write);
  the recovered set is exactly the independent framing scan of the open
  segment (prefix property); every enumerated id re-hashes to itself.
* A published manifest → it opens, its root node is enumerable, and
  `materialize --exact` returns the exact source bytes.
* Observations (cold cache: page-1 text + metadata) are compared to a
  clean store of the same document; a tampered store may decline
  (typed, fail-closed) but must never answer with different content.

## Honest scope

* `SIGKILL`/`SIGABRT`/`abort()` stop the process but the OS page cache
  survives; every completed `write` is still readable after reopen. The
  court therefore proves the **ordering** and **prefix-recovery** design
  (flush-before-publish, no partial node, exactly-a-prefix recovery) and
  the **storage-corruption fail-closed** path — not that an un-`fsync`ed
  record is lost on true power loss.
* The `write_atomic` rename is never followed by a parent-directory
  `fsync`; the court cannot observe a torn/lost rename across a real power
  cut, so that ordering is argued, not measured.
* Family C needs `--features fault-inject`; without it, family C is
  reported as skipped.

See `MATRIX.md` for the full grid and `raw/cases.tsv` for every case.
