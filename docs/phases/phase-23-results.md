# Phase 23 results — directory fsync (GAP 1) + a model-based power-loss proxy (GAP 2)

Branch `phase23`. Measured at commit `8816507` (dirty tree: the Phase-23 change
set, recorded in the receipt). Base `main` @ `v0.1.0-alpha.26` (Phase 22).

> **Corrected by [Phase 25](phase-25-results.md) (ADR-0058).** An external review
> found two gaps this court could not see: directory *ancestry* was not made
> durable, and the packed segment *header* was synced after its directory. Both
> are fixed there, the model now tracks directory ancestry, the verdicts are
> stricter (no manifest ⇒ reopen), and every cut now recovers the prefix. The
> scope of the claim below (a model, not a real power cut) is unchanged.

Phase 20.1 closed the crash story it could close — ordering, prefix-exact
recovery, fail-closed corruption — and **named the two gaps it left open**:

1. `write_atomic` never `fsync`ed the containing **directory**, so a rename
   could be lost across a power cut even though the file's contents were synced.
2. Process death does **not** evict the page cache, so **no `fsync` boundary was
   exercised** and `SyncPolicy::Batch` and `Each` were indistinguishable — which
   is *not* evidence that batching is power-safe.

Phase 23 closes both: the directory `fsync` is implemented (with a measured
cost and an explicit policy), and an honest **model-based power-loss proxy**
reconstructs the post-power-loss state from a barrier log and re-runs the
Phase-20.1 invariants against it.

## GAP 1 — make the rename durable

Every atomic writer now `fsync`s the containing directory after the rename, and
`fsync`s the directory when a new packed segment or index file is created:

- `src/field/mod.rs::write_atomic` (descriptor, manifest, external context,
  derived cache, promoted store);
- `src/store/seed.rs::FsSeedStore::put_node`;
- `src/field/index.rs::FsIndexStore::put`;
- `src/store/pack.rs` (`PackWriter::ensure_open` — new segment — and `seal` —
  the `.idx` publish, which goes through `write_atomic`).

All of these route through a small `src/store/durable.rs` seam so the barrier is
in one place. The behavior is governed by `DirSyncPolicy`:

- `Safe` (**default**): directory `fsync` after the rename and on new
  segment/index creation. This is what makes *“after a crash, a published
  manifest and the nodes it references survive”* hold (subject to the
  batch-flush boundary of [ADR-0053](../adr/0053-batched-packed-sync.md)).
- `Off` (`--dir-sync=off`): skip directory `fsync`s. Faster ingest; a lost
  rename is possible. The escape hatch exists to measure the cost and to prove
  the barrier is load-bearing — not as a recommended mode.

### Measured cost

Plain default-feature binary (no barrier-log overhead), median of 7 fresh
`field-build --profile runtime` ingests of `nist-pdf-0017` (1.4 MiB), one file
per seed/index node for the fs store, one segment + one `.idx` for the packed
store:

| backend | dir_sync | host bind `/work` (ext4) | container tmpfs | reps |
|---|---|---:|---:|---:|
| fs | `off` | **717 ms** | 19 ms | 7 |
| fs | `safe` | **1,400 ms** | 18 ms | 7 |
| packed | `off` | **23 ms** | 10 ms | 7 |
| packed | `safe` | **35 ms** | 9 ms | 7 |

On the real host filesystem the fs store's one-file-per-node layout pays
**~1.95×** (one directory `fsync` per node, ~6,800 of them for this document);
the packed store pays **~1.5×**, which is small in absolute terms (~12 ms)
because it dir-fsyncs once per new segment and once per sealed `.idx`. On the
container's tmpfs (no write-back to a real device) the cost is below noise. This
is why the SAFE option is the default and the cost is exposed as an explicit
policy rather than silently paid or silently skipped.

**Is the directory fsync necessary?** Yes, for the stated invariant, and the
proxy measures it directly: with `--dir-sync=off` the reconstruction loses
**every** published file (4/4 complete ingests, both backends, both policies →
0 manifests), so a published manifest does not survive; with `Safe` it survives
4/4 and materializes exactly. It is load-bearing, not decorative.

## GAP 2 — the model-based power-loss proxy

The non-default `power-log` feature makes `src/store/durable.rs` log every
durability barrier, in program order, to `$VOLE_POWER_LOG`:

```text
create  <path>            # truncating create (length 0)
write   <path> <len>      # <len> = file length after the write
fsync   <path> <len>      # the completed barrier covers [0, len)
fsync_data <path> <len>
dirsync <path>            # a directory fsync (makes entries in it durable)
rename  <from> <to>
setlen  <path> <len>
```

`tests/power_loss_proxy.rs` **folds the log into a per-path durability model**
and reconstructs the post-power-loss store:

- a file **survives** only if its `create`/`rename` was followed by a `dirsync`
  of its parent directory;
- its **content** is truncated to the length at its last completed file barrier
  (0 if it never had one) — everything written after the last barrier is lost.

The reconstruction is then checked with the Phase-20.1 invariants: with no
published manifest the store reopens, no partial node is fetchable, and every
present id re-hashes; with a published manifest the root node and all index
nodes exist, `materialize --exact` equals the source (length + byte compare,
hence SHA-256), and cold observations match a clean baseline. It runs under
**both** `SyncPolicy::Batch` and `Each` and **both** the fs and packed stores,
complete and cut (deterministic abort at `record.after_body`, `flush.before_sync`,
`flush.after_sync`, `seal.before_idx`, `manifest.after_publish`).

**Receipt.**
[`2026-10-08-phase23-durability-8816507`](../../evidence/campaigns/2026-10-08-phase23-durability-8816507/);
court `tools/phase23-powerloss-court.sh`, proxy `tests/power_loss_proxy.rs`
(`--features power-log,fault-inject`), aggregator
`tools/fixtures/phase23-powerloss-aggregate.sh`. Base image
`rust:1.99.0-slim-bookworm@sha256:452176c0…`, service `dev`
(`mem_limit == memswap_limit == 8g`, `pids_limit 4096`), `rustc 1.99.0`,
`cargo 1.99.0`. Document `nist-pdf-0002`.

**Result — PASS: 32 cases, 20 PASS / 0 FAIL / 0 CRITICAL on shipped arms; 12
CRITICAL on the counterfactual arms.**

| backend | policy | dir_sync | arm | PASS | CRITICAL | total |
|---|---|---|---|---:|---:|---:|
| fs | batch/each | safe | complete | 1 | 0 | 1 |
| fs | batch/each | off | complete | 1 | 0 | 1 |
| fs | batch/each | safe | cut | 1 | 0 | 1 |
| fs | batch/each | safe | model-drop | 0 | 3 | 3 |
| packed | batch/each | safe | complete | 1 | 0 | 1 |
| packed | batch/each | off | complete | 1 | 0 | 1 |
| packed | batch/each | safe | cut | 5 | 0 | 5 |
| packed | batch/each | safe | model-drop | 0 | 3 | 3 |

(The full grid is in the campaign's `MATRIX.md`.)

- **No shipped arm was CRITICAL** (`bad_hash = 0`, no wrong bytes). Complete
  `Safe` ingests publish a surviving manifest **4/4**; complete `Off` ingests
  lose **4/4**; every cut store that could not serve failed **closed** with a
  typed error; every reconstructed published manifest materialized exactly.
- **The counterfactual arms are the sensitivity control.** A `model-drop` arm
  discards one directory barrier subtree in the *model* (not the code) and is
  CRITICAL **3/3 per backend per policy** — e.g. dropping `fieldpack` leaves a
  published manifest whose segment is gone (`MissingRoot`), dropping `index`
  leaves it unserviceable (`MissingIndex`), dropping `descriptor` makes it fail
  closed. This is why the model is trusted to have caught GAP 1 had it shipped.

**Batch vs Each under the model.** They are **equivalent for the
published-manifest invariant**, because `FieldStore::put_field` flushes the open
segment before publishing — a durable manifest never references a batch-pending
record. They differ only for the **unreferenced** open-segment tail, which is
visible at the `flush.before_sync` cut: `Batch` reconstructs to a truncated
header (`fail_closed`), `Each` reconstructs to the already-synced records
(`reopens_ok`). So the honest statement is narrower and stronger than Phase
20.1's: batching is power-safe *for the published field*, and the difference it
does make is confined to records no manifest references.

### A physical proxy (second arm) — not reproducible in the pinned lane

A truly physical proxy was attempted and is **not** reproducible inside the
pinned, hard-capped, unprivileged `dev` lane: `/proc/sys/vm/drop_caches` cannot
discard **dirty** (un-fsynced) pages — the kernel writes them back rather than
dropping them, so it cannot model the loss of an un-barriered write — and
device-mapper fault targets (`dm-flakey`, `dm-log-writes`) or loopback tricks
need privileges the lane does not have. This is stated rather than approximated.

## Claim, and what remains unproven

**Claim.** With the default `DirSyncPolicy::Safe`, after a model power loss a
completed ingest's published manifest and the nodes it references survive, and
`materialize --exact` equals the source; with `--dir-sync=off` the published
store is lost, so the directory `fsync` is required for the invariant. Under the
model, `Batch` and `Each` are power-equivalent for the published field.

**Unproven (the residual).** This is a **model derived from a barrier log, not a
real power cut.** It does not exercise the storage device's volatile write
cache, the filesystem journal, torn sectors, or a real directory-entry loss, and
it assumes directory metadata is durable except for the file-entry barriers it
tracks. A true hardware power cut could still lose a rename the kernel had not
yet written to the directory, or reorder writes the device coalesced, in ways a
software model cannot see.

## Gate outcomes (all in the `dev` service)

| gate | rc |
|---|---:|
| `cargo fmt --all --check` | 0 |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 |
| `cargo test --locked --all-features` | 0 |
| `cargo test --locked --no-default-features` | 0 |
| `sh tools/phase1-court.sh` | 0 |

The default build is unchanged in shape: without `power-log` the instrumented
wrappers are pass-throughs (no wire byte, no on-disk layout, no decode
behavior); `--dir-sync` defaults to `Safe`.

See [ADR-0057](../adr/0057-directory-fsync-and-power-loss-proxy.md) for the
decision and its scope, and the [ADR-0053
amendment](../adr/0053-batched-packed-sync.md) that closes its “argued, not
measured” note.
