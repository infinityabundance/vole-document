# ADR-0053: Batch the packed seed store's durability syncs (one `fsync` per segment) and let `observe-batch` serve the packed store

- **Status:** Accepted — adopted (Phase 18.5)
- **Date:** 2026-10-08

## Context

Phase 18.4 measured the contract court's build wall as a **durability-sync** term,
not a file-count term: `PackedSeedStore::insert` called `f.sync_data()` once per
seed node (`src/store/pack.rs`), so the packed store issued ~one `fdatasync` per
node — the same count as the fs store's one `fsync` per node file (6792 vs 6842
for `nist-pdf-0017`, 81% of the build sum). On the bind-mounted `/work` store that
document cost ~9.4 s; the identical build in `/tmp` cost ~1.4 s. The packed
backend's file-count collapse was therefore masked by an unchanged per-node sync,
making it a storage-shape win only. 18.4 also recorded that `observe-batch`
rejected `--packed` with a typed `UnsupportedFeature` (rc 6), so the packed store
could not serve the warm session.

The packed store is append-only and self-describing (each record is a `u32` length
prefix plus body), with a torn-tail truncation on read-write reopen — so batching
syncs to the segment level is architecturally available.

## Decision

1. Add `SyncPolicy { Batch, Each }` to the packed store. `Batch` (**the default**)
   appends records to the open segment and forces them to stable storage once per
   segment — at **seal** (which also publishes the immutable `.idx`) and at an
   explicit **flush** — instead of once per record. `Each` restores the pre-18.5
   per-record `fdatasync`. Exposed as `--sync=batch|each` and at the library level
   (`PackedSeedStore::open_write_with_policy`, `FieldStore::open_packed_with_policy`).
2. Preserve ordering: `FieldStore::put_field` **flushes** the packed segment
   before publishing a manifest, so a durable manifest never references
   non-durable nodes. This closes the only window in which batching could leave a
   published field inconsistent.
3. Let `observe-batch` open the packed store (`SessionOptions { packed }`), so the
   one-session warm lane is no longer fs-only.

### Crash-consistency semantics (precise)

Recovery of the open segment is a **prefix** recovery: the framing scan stops at
the first length prefix that is absent, zero, oversized, or whose body does not
fit in the file, and every record before that point is recovered whole; the torn
tail is discarded (truncated on a read-write reopen; ignored on a read-only one).
No **partial** node is ever observable, and every fetched node is re-hashed
against its id. A sealed segment is never rewritten. Therefore, after a crash:
records not yet forced to stable storage may be lost (with `Batch`, at most those
appended since the last seal/flush), but the recovered set is always a prefix; a
published field manifest is always consistent because it is flushed first; and a
segment that lost its tail may legitimately be left incomplete (recovery drops the
tail and the caller appends from the recovered end).

## Consequences

- **Exactness unchanged.** The packed namespace is never on the `materialize
  --exact` path; node identity, canonical bytes, the whole-node re-hash gate, the
  strict range semantics, and the `(id, len)` set are unchanged, and identical
  `.pack`/`.idx` bytes are produced under either policy (unit-tested).
- **Measured (receipt `evidence/campaigns/2026-10-08-phase18-batched-sync-14a7e6f/`).**
  Worst document `nist-pdf-0017`, bind-mounted store: packed wall **9392 → 1415
  ms**, packed `fdatasync` **6792 → 0** (`fsync` 52 → 54); fs-direct unchanged
  (10181 → 9963 ms); `/tmp` packed unchanged (~1374 → ~1378 ms, the residual is
  real work). Same-run 12-document contract court: VOLE `field-build --packed`
  build sum **1552 ms vs SQLite 1893 ms = 0.82×** (was **7.08×** in 18.4; fs-direct
  6.27× in the same run; two-step 10.74×), `nist-pdf-0017` **9187 → 1382 ms**.
  Storage unchanged (7,778,087 B = 0.53× SQLite; 120 vs 8621 files). Warm
  `observe-batch --packed` now exists: **1.09×** SQLite over C0–C5 (1.11× at steady
  state C1–C5), versus the fs-direct warm reference — the substrate does not change
  the query path. **SUPERSEDED as competitor statements by Phase 22.1: the 0.53×
  storage and 1.09× warm figures are against the Phase-18 *historical-control*
  configuration; against a Pareto-tuned equal-contract SQLite the storage
  advantage is 0.762× (`full`) / 0.805× (`adaptive`) and warm is a loss (1.211×
  `full`, 95% CI 1.006–1.483).** See
  [phase-22-results.md](../phases/phase-22-results.md).
- **`Batch` is the default** because it preserves every correctness invariant and
  only moves *when* a record becomes power-durable; the stronger per-`put_node`
  barrier remains available as `--sync=each`.
- **Falsifier.** If a crash test ever observed a fetched node whose bytes differ
  from its id, or a published manifest whose root node was missing after recovery,
  the design would be wrong. Unit tests pin prefix recovery and policy-independent
  bytes; the manifest-ordering guarantee is enforced by construction
  (flush-before-publish) rather than by a power-loss test.
- **Scope.** The packed seed namespace only; descriptor / manifest / index / cache
  remain files. The court's 12-document subset is not the frozen population.

## References

- `src/store/pack.rs` (`SyncPolicy`, `insert`, `flush`, `seal`); `src/field/mod.rs`
  (`FieldStore::put_field`, `open_packed_with_policy`); `src/field/session.rs`
  (`SessionOptions`); `src/main.rs` (`--sync`, `observe-batch`)
- `evidence/campaigns/2026-10-08-phase18-batched-sync-14a7e6f/`
- `evidence/campaigns/2026-10-08-phase18-contract-packed-14a7e6f/` (the re-run
  contract court; its fixture prose is stale, the raw tables are authoritative)
- ADR-0043 (the packed seed store); ADR-0042 (`observe-batch` residency)

## Amendment (Phase 20.1) — the fault-injection evidence, with its scope

The statement above that the manifest-ordering guarantee is "enforced by
construction rather than by a power-loss test" is now backed by a **hostile
fault-injection court**. The original decision is unchanged; this amendment adds
the measured evidence and states exactly what it does and does not prove.

**Receipt.** `evidence/campaigns/2026-10-08-phase20-crash-47acde7/` (court
`tests/crash_recovery.rs` via `tools/phase20-crash-court.sh`; base image
`rust:1.99.0-slim-bookworm@sha256:452176c0…`, service `dev`
`mem_limit == memswap_limit == 8g`, `pids_limit 4096`). Two representative PDFs,
`reps=2`.

**Result — 1,300 cases, PASS 1,300 / FAIL 0 / CRITICAL 0**, under **both**
`SyncPolicy::Batch` and `SyncPolicy::Each` (650/650 each). Three families:
process death (`SIGKILL`/`SIGABRT` delay sweep + manifest/idx-boundary kills,
1,024), storage corruption (truncate `.pack`/`.idx`, bit flips, zeroed tail,
dropped `.idx`, 236), and deterministic in-code aborts at ten named writer
points behind the non-default `fault-inject` feature (40).

- `bad_hash` (id-mismatched bytes served) **0**; prefix-resolution violations
  **0**;
- the whole-node re-hash gate rejected **212,812** enumerated nodes across
  **110** cases;
- corrupted artefacts consistently **fail closed** with a typed error;
- one robustness observation (not a correctness failure): a `SIGKILL` inside
  `PackWriter::ensure_open`, after the `.pack` was created but before its 24-byte
  header completed, leaves the store **unusable with no manifest**; every later
  open fails closed with a typed `IntegrityMismatch` ("truncated header") and
  never returns bytes.

**What this proves.** Flush-before-publish ordering; no partial node ever
fetchable; recovery is exactly a prefix; corruption fails closed — under both
policies, across every injected boundary.

**What this does not prove (scope, explicit).** `Batch` and `Each` gave an
identical verdict distribution, which is **expected under this injection model
and is not evidence that batching is power-safe**: `SIGKILL`/`SIGABRT`/`abort()`
stop the process but the OS page cache survives, so **no `fsync`/`fdatasync`
boundary is actually exercised** and the court does not measure loss of
un-`fsync`ed records under **true power loss**. The `write_atomic` rename is
never followed by a parent-directory `fsync`, so a **torn/lost rename** across a
real power cut remains **argued, not measured**. Family C requires the
non-default `fault-inject` feature.

See [phase-20-results.md](../phases/phase-20-results.md) (20.1) for the full
injection × outcome matrix.

## Amendment (Phase 23) — the directory-`fsync` gap closes, and power loss is modelled

The amendment above states that the `write_atomic` rename is "never followed by
a parent-directory `fsync`, so a torn/lost rename across a real power cut
remains argued, not measured". **Phase 23 closes that gap and measures it.**

- Every atomic writer now `fsync`s the containing directory after the rename (and
  on new segment/index creation), with `DirSyncPolicy::Safe` the default and
  `--dir-sync=off` an explicit escape hatch. On the host ext4 bind mount the fs
  store's one-file-per-node layout pays ~1.95× (`nist-pdf-0017`: 717 → 1,400 ms,
  median of 7); the packed store pays ~1.5× (23 → 35 ms), because it dir-fsyncs
  once per new segment and per sealed `.idx`. On the container tmpfs the cost is
  below noise.
- A **model-based power-loss proxy** (`tests/power_loss_proxy.rs`, non-default
  `power-log` feature) logs every barrier with its byte range and reconstructs
  the post-power-loss state (only barrier-covered bytes survive) before re-running
  the Phase-20.1 invariants. Result: with `Safe`, a completed ingest's published
  manifest and the nodes it references survive **4/4** and materialize exactly;
  with `--dir-sync=off` the published store is lost **4/4**; no shipped arm was
  CRITICAL. It is a **model**, not a real power cut (see
  [ADR-0057](0057-directory-fsync-and-power-loss-proxy.md) for the residual).
- The `Batch` vs `Each` distinction is now stated precisely: under the model they
  are **equivalent for the published-manifest invariant** because `put_field`
  flushes before publishing; they differ only for the unreferenced open-segment
  tail (visible at the `flush.before_sync` cut: `Batch` reconstructs to a
  truncated header, `Each` to the synced records).

Receipt: `evidence/campaigns/2026-10-08-phase23-durability-8816507/`; see
[phase-23-results.md](../phases/phase-23-results.md).
