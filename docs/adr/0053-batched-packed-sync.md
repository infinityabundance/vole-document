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
  the query path.
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
