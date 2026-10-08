# ADR-0058: durable directory *ancestry* and prefix recovery for unpublished segments

- **Status:** Accepted — adopted (Phase 25, correcting Phase 23)
- **Date:** 2026-10-08

## Context

Phase 23 ([ADR-0057](0057-directory-fsync-and-power-loss-proxy.md)) added a
directory `fsync` after every atomic publish and modelled power loss from a
barrier log. An external review found **two correctness gaps** that the Phase-23
court could not see, plus **one court weakness**. All three were verified in the
code before fixing.

1. **Directory ancestry was not durable.** `durable::create_dir_all` called
   `fs::create_dir_all` and logged `mkdir`, but never synced the **parent** of a
   newly created directory. Syncing a store's *leaf* directory (`index/aa/bb/`)
   therefore did **not** make its **ancestors** durable: a power cut could lose
   `index/aa/`, orphaning every node written beneath it, even though the leaf was
   synced. The same applied to `seed/aa/bb/`, `descriptor/`, `field/`, the store
   root, and `fieldpack/`.
2. **The packed-header window.** `PackWriter::ensure_open` did
   `create_file -> write header -> sync_dir(dir)` — it synced the containing
   **directory before** the new segment's **header bytes**. A durable directory
   entry could therefore point at a segment with an incomplete or missing header.
3. **The model ignored `mkdir`, and the court was too lenient.** `build_model`
   skipped `mkdir`, and `reconstruct` copied the live directory tree, so the proxy
   could not detect the loss of an unsynchronized ancestor directory. Separately,
   `strict_verdict` accepted a **no-manifest** outcome that merely failed closed,
   and `lenient_verdict` (used for the cut arms) ignored `manifests` and exactness
   entirely — so a cut leaving an unusable published field could pass.

The Phase-23 receipt recorded the symptom directly: `packed-batch-cut-record.after_body`
and `packed-batch-cut-flush.before_sync` recovered to `fail_closed` (a truncated
header) instead of reopening. Detecting corruption is not the same as the Phase-20
requirement, which is **prefix recovery**: with no published manifest the store
must **reopen** and expose exactly the valid prefix.

## Decision

1. **`durable::create_dir_all` makes every newly created directory entry durable.**
   It finds the missing components, creates them shallowest-first, and `fsync`s
   each new entry's parent (honoring `DirSyncPolicy`). Components that already
   exist are untouched. The store-open paths (`FieldStore::open`,
   `open_packed_with_policy`, `open_entropyfs`, `FsSeedStore::open_with_io`,
   `FsIndexStore::open_with_io`, `DerivedCache::open`, `PromotedStore::open`) now
   route through it instead of raw `fs::create_dir_all`.
2. **The packed segment header is synced before the directory.** `ensure_open` now
   does `create -> write header -> sync_all(file) -> sync_dir(dir)`, so a durable
   directory entry never precedes a durable header.
3. **An unpublished open segment with an incomplete/invalid header is treated as
   absent.** `scan_open_segment` returns `(no records, 0)` for a short or
   unparseable header (an unsealed segment is always unpublished — a manifest is
   only published after a flush + seal), so `open_read` exposes no such segment
   and `open_write` re-creates it from scratch at the same id. The store
   **reopens** and recovers the prefix; no wrong bytes are ever served.
4. **The proxy models directory creation and ancestry.** `build_model` folds
   `mkdir` into the per-path model; `reconstruct` prunes non-durable directories
   **deepest-first** (with everything beneath them) before pruning files.
5. **The verdicts are unified on the strict rule.** A no-manifest state must
   **reopen** (`reopens_ok`); a published manifest must be exact and serviceable.
   `lenient_verdict` is removed; every arm is judged by the same rule.

## Consequences

- **Exactness unchanged.** No wire byte, on-disk layout, or decode behavior
  changes; the fixes add directory `fsync`s and change only how an
  incomplete *unpublished* segment is recovered. `power-log` remains a
  pass-through when off.
- **Measured (receipt `evidence/campaigns/2026-10-08-phase23-durability-fc63436-phase25/`).**
  Plain default-feature binary, median of 3 fresh ingests of `nist-pdf-0017`, host
  bind mount: fs store `off` **723 ms → safe 1,381 ms** (~1.91×; the ancestry
  syncs add to the earlier per-node leaf syncs); packed `off` **22 ms → safe
  34 ms** (~1.55×); below noise on tmpfs. (Phase 23 measured 717 → 1,400 / 23 →
  35 with `reps=7`; the ancestry fix is within run-to-run variance at this scale.)
- **Proxy result — every cut now reopens.** 32 cases: **20 PASS / 0 FAIL / 0
  shipped-arm CRITICAL** under the **stricter** rules; the 12 counterfactual
  `model-drop` arms are all CRITICAL (sensitivity), and the ancestry model now
  removes whole subtrees (`fs` `--dir-sync=off`: 827 directories; `drop-seed`:
  812; `drop-index`: 10). `packed-batch-cut-record.after_body` and
  `packed-batch-cut-flush.before_sync`, which previously failed closed, now
  **open with the prefix recovered** (`truncated=1`, prefix only).
- **Falsifier.** If any shipped arm served a wrong-byte node, published a manifest
  referencing a lost node, materialized non-exactly, or failed to reopen with no
  manifest, the design would be wrong. None occurred.
- **Scope / residual (unchanged from ADR-0057).** The proxy is a **model derived
  from a barrier log, not a real power cut**: it does not exercise the device write
  cache, the filesystem journal, torn sectors, or a real directory-entry loss, and
  it assumes directory metadata is durable except for the entry barriers it now
  tracks (including ancestry). A physical proxy is still not reproducible in the
  pinned, unprivileged, hard-capped lane.

## References

- `src/store/durable.rs` (`create_dir_all`); `src/store/pack.rs` (`ensure_open`,
  `scan_open_segment`, `open_write_internal`); `src/field/mod.rs`,
  `src/store/seed.rs`, `src/field/index.rs`, `src/field/cache.rs`,
  `src/field/promote.rs` (the store-open paths)
- `tests/power_loss_proxy.rs` (`build_model`, `walk_dirs`, `reconstruct`,
  `strict_verdict`)
- `evidence/campaigns/2026-10-08-phase23-durability-fc63436-phase25/`
- [phase-25-results.md](../phases/phase-25-results.md)
- [ADR-0057](0057-directory-fsync-and-power-loss-proxy.md) (the directory-`fsync`
  decision this corrects); [ADR-0053](0053-batched-packed-sync.md)
  (flush-before-publish)
