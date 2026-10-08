# Phase 18.5 — batched packed-store durability syncs (and `observe-batch --packed`)

## Question

Phase 18.4 isolated the contract-court build wall as a **durability-sync** term:
`PackedSeedStore::insert` (`src/store/pack.rs`) called `f.sync_data()` once per
seed node, so the packed store issued ~one `fdatasync` per node — the same sync
count as the fs store's one `fsync` per node file (6792 vs 6842 for
`nist-pdf-0017`, 81% of the build sum). On the bind-mounted `/work` store that doc
cost ~9.4 s; in `/tmp` ~1.4 s. The lever: **sync once per segment, not per node.**

## Design and crash-consistency semantics

`SyncPolicy { Batch (default), Each }` (`src/store/pack.rs`). `Batch` appends
records to the open segment and syncs once per segment — at **seal**
(`sync_all` + immutable `.idx`) and at an explicit **flush** — instead of once per
record. `Each` restores the pre-18.5 per-record `fdatasync`.

The packed store is append-only and self-describing (each record is a `u32` length
prefix + body), so recovery is a **prefix** recovery. On reopen the open segment
is scanned from its header; scanning stops at the first prefix that is absent,
zero, oversized, or whose body does not fit, so:

* every record before that point is recovered, whole;
* no **partial** node is ever observable, and every fetched node is re-hashed
  against its id (the unchanged `get_node` gate);
* the torn tail is discarded (truncated on a read-write reopen; ignored on a
  read-only one); a sealed segment is never rewritten.

**Default changed to `Batch`.** Reasoning: exactness is untouched (the packed
namespace is never on the `materialize --exact` path; node identity, bytes, and
the `(id, len)` set are identical, and identical `.pack`/`.idx` bytes are produced
under either policy). The only guarantee moved is *when* a record becomes
power-durable. That is made safe by ordering: `FieldStore::put_field` now
**flushes** the packed segment before publishing any manifest, so a durable
manifest never references a non-durable node — a crash can lose an unflushed
suffix, never leave a dangling published field. `--sync=each` (CLI) and
`open_packed_with_policy` (library) are available for the strongest per-`put_node`
barrier.

## `observe-batch --packed`

The 18.4 typed rc-6 rejection is removed: `SessionOptions` gained `packed`, and
`DocumentFieldSession::open` opens the same `fieldpack/` store. The packed store
can now serve the warm one-session lane. Covered by the court (warm lane rc 0) and
by `tests/field_observe.rs::cli_observe_batch_supports_packed_store`.

## Measurements (worst doc + same-run 12-doc court)

Worst doc `nist-pdf-0017`, bind-mounted store, mechanism probe (`strace` counts):

| lane | before | after |
|---|---:|---:|
| packed wall | 9392 ms | **1415 ms** |
| packed `fdatasync` | 6792 | **0** |
| packed `fsync` | 52 | 54 |
| fs-direct wall | 10181 ms | 9963 ms (unchanged) |

`/tmp` packed is ~1374 ms before and ~1378 ms after — the residual is real work,
not sync round-trips. Policy probe (best-of-3 min): `Batch` 2644 ms vs `Each`
9230 ms for the same document.

Contract court (12 docs, `field-build ... --packed`, best-of-3 min):

| lane | build ms (sum) | vs SQLite (1893) |
|---|---:|---:|
| VOLE packed `field-build` | **1552** | **0.82×** |
| VOLE fs-direct | 11874 | 6.27× |
| SQLite C5 | 1893 | 1.00× |
| (18.4) packed | 11312 | 7.08× |
| (18.1) fs-direct | 11715 | 7.21× |
| (16.5/17.2) two-step | ~26000 | 10.74× |

VOLE is now the **build winner** on this subset (0.82× = ~1.22× faster than
SQLite); `nist-pdf-0017` moves 9187 ms → 1382 ms. Storage is unchanged
(7,778,087 B packed = 0.53× SQLite; 120 files vs 8621 fs), confirming batch
changed no stored byte. Warm session with `--packed` now exists: VOLE 226 ms vs
SQLite 208 ms over C0–C5 (**1.09×**); steady state C1–C5 192 vs 173 (**1.11×**),
versus the fs-direct reference 209 ms — the substrate does not change the query
path.

## Exactness

Court `materialize --exact --packed`: **12/12** reproduce original length + SHA-256.
Worst doc: len 1466246 = 1466246, SHA-256
`0df0fdd6…63cd4` = `…63cd4`, `byte_compare = equal`. The unit tests
`batch_policy_recovers_a_complete_prefix_after_a_torn_tail` and
`sync_policy_does_not_change_stored_bytes` pin prefix recovery and byte-identity
across policies.

## Gate

`cargo fmt --all --check` PASS · `cargo clippy --all-targets --all-features -- -D
warnings` PASS · `cargo test --locked --all-features` PASS · `cargo test --locked
--no-default-features` PASS · `sh tools/phase1-court.sh` PASS.

## Residual risks

* `Batch` can lose an unflushed suffix on power loss; the flush-before-manifest
  ordering keeps every *published* field consistent.
* The court fixture (`tools/fixtures/phase18-contract-packed.py`) hard-codes the
  stale 18.4 prose ("observe-batch has no --packed", "term unchanged"); its raw
  tables (copied under `raw/court/`) are authoritative. The fixture was not
  modified.
* Bind-mount wall variance is up to ~2×; build figures are best-of-3 mins, the
  mechanism probes single-shot.
* Packed warm RSS is not measured (the court times only the fs-direct warm lane).
