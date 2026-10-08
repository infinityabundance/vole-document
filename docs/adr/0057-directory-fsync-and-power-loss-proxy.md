# ADR-0057: `fsync` the containing directory on atomic publishes, and measure power loss with a barrier-log model

- **Status:** Accepted — adopted (Phase 23)
- **Date:** 2026-10-08

## Context

ADR-0043/0053 store the field's descriptor, manifest, index nodes, and seed
nodes with atomic publishes (`tmp -> fsync -> rename`). On strict POSIX a rename
is **not durable across a power cut until the containing directory is synced**;
the file's own `fsync` guarantees nothing about the directory entry. Phase 20.1
recorded this explicitly as a gap it could not measure: process death
(`SIGKILL`/`SIGABRT`) does not evict the page cache, so no `fsync` boundary is
exercised, and `SyncPolicy::Batch` and `Each` were indistinguishable — which is
not evidence that batching is power-safe.

So two things were missing: (1) a directory `fsync` making a published manifest
and the nodes it references survive a power cut, and (2) an honest way to
observe the loss of un-barriered bytes without a real power cut.

## Decision

1. **Directory durability.** Every atomic writer `fsync`s the containing
   directory after the rename, and `fsync`s the directory when a new packed
   segment or index file is created. Implemented once in `src/store/durable.rs`
   and used by `field::write_atomic`, `FsSeedStore::put_node`,
   `FsIndexStore::put`, and the packed writer (`ensure_open`, `seal`).
2. **An explicit policy.** `DirSyncPolicy { Safe, Off }` — `Safe` (**default**)
   performs the directory `fsync`; `Off` (`--dir-sync=off`) skips it. The
   default is SAFE because the extra barrier is required for the invariant; the
   `Off` arm exists to *measure* the cost and to *show* the barrier is
   load-bearing, not as a recommended operating mode.
3. **A model-based power-loss proxy.** Under the non-default `power-log`
   feature, every durability barrier is logged with the byte range it covered
   (`fsync <path> <len>`, plus `create`/`rename`/`dirsync`/`setlen`). The proxy
   (`tests/power_loss_proxy.rs`) folds the log into a per-path model — a file
   survives only if its create/rename was followed by a parent `dirsync`, and
   its content is truncated to the length at its last completed file barrier —
   and re-runs the Phase-20.1 invariants against the reconstructed store, under
   both sync policies and both the fs and packed stores.

## Consequences

- **Exactness unchanged.** The directory `fsync` and the barrier log change no
  wire byte, no on-disk layout, and no decode behavior; without `power-log` the
  instrumented wrappers are pass-throughs.
- **Measured (receipt `evidence/campaigns/2026-10-08-phase23-durability-8816507/`).**
  Plain default-feature binary, median of 7 fresh ingests of `nist-pdf-0017`,
  host ext4 bind mount: fs store `off` **717 ms → safe 1,400 ms** (~1.95×; one
  directory `fsync` per node, ~6,800 for this document); packed store `off`
  **23 ms → safe 35 ms** (~1.5×, small in absolute terms — one directory
  `fsync` per new segment and per sealed `.idx`). On the container tmpfs the
  cost is below noise.
- **Proxy result.** 32 cases: 20 PASS / 0 FAIL / 0 CRITICAL on shipped arms;
  12 CRITICAL on the *counterfactual* `model-drop` arms that remove a directory
  barrier in the model — the sensitivity control that shows the model would have
  caught the pre-GAP-1 code. Complete `Safe` ingests publish a surviving
  manifest **4/4**; complete `Off` ingests lose **4/4**; every reconstructed
  published manifest materialized exactly; no wrong bytes.
- **`Batch` vs `Each`, precisely.** Under the model they are **equivalent for
  the published-manifest invariant** (flush-before-publish, ADR-0053). They
  differ only for the unreferenced open-segment tail: at the `flush.before_sync`
  cut, `Batch` reconstructs to a truncated header and `Each` to the synced
  records. This is a narrower, stronger statement than “both pass the crash
  court”.
- **Falsifier.** If a shipped arm were ever CRITICAL — a published manifest
  referencing a node a modelled power loss removes, a materialize mismatch, or
  wrong bytes — the design would be wrong. None occurred; the model-drop arms
  confirm the check is sensitive.
- **Scope / residual.** The proxy is a **model derived from a barrier log, not a
  real power cut**. It does not exercise the device write cache, the filesystem
  journal, torn sectors, or a real directory-entry loss, and it assumes
  directory metadata is durable except for the file-entry barriers it tracks. A
  physical proxy was attempted and is not reproducible in the pinned,
  unprivileged, hard-capped `dev` lane (`drop_caches` cannot discard dirty
  pages; device-mapper fault targets need privileges). The residual is a true
  hardware power cut losing a rename the kernel had not yet written, or
  reordering coalesced writes.

## References

- `src/store/durable.rs` (`DirSyncPolicy`, `sync_dir`, the barrier journal);
  `src/field/mod.rs`, `src/store/seed.rs`, `src/field/index.rs`,
  `src/store/pack.rs` (the instrumented writers); `src/main.rs` (`--dir-sync`)
- `tests/power_loss_proxy.rs`, `tools/phase23-powerloss-court.sh`,
  `tools/fixtures/phase23-powerloss-aggregate.sh`
- `evidence/campaigns/2026-10-08-phase23-durability-8816507/`
- [phase-23-results.md](../phases/phase-23-results.md)
- ADR-0053 (batched packed syncs; the flush-before-publish ordering the proxy
  relies on); ADR-0043 (the packed seed store)
