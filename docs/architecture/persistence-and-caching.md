# Persistence and caching

VOLE keeps multiple persistence layers strictly separated, because conflating
them is the recurring measurement error this project guards against (ADR-0027).
Sharing is store *amortization*, never “compression”.

## Two digests, two roles

| Digest | Role | Authority |
|---|---|---|
| SHA-256 | whole reconstructed source; archival identity | parse / materialize / verify |
| BLAKE3-256 (`Id`) | one object's raw bytes; store namespace | relational / advisory |
| BLAKE3-256 (`NodeId`) | one seed node's canonical state; seed namespace | relational for observations |

An `Id`/`NodeId` is never a substitute for the source digest and never appears in
`INTEGRITY` (ADR-0020, ADR-0025).

## Object store

The object table is one ordered sequence; each entry is either an inline
`OBJECT` (`0x10`) or an `EXTERNAL_REF` (`0x80`, 40 bytes) naming a store object.
Parsing never needs the store; the entry position is the `object_id`, so the DRA
is untouched. A descriptor with any `EXTERNAL_REF` sets the mandatory
`FEATURE_EXTERNAL_OBJECTS` bit. `externalize`/`hydrate` must materialize identical
bytes, and `gc` is a mark-and-sweep over referenced roots. `EmbeddedStore` (a raw
content-addressed directory) is the reference backend (ADR-0020).

## Three accounting universes

Kept permanently distinct, so a store root is never compared to a whole file:

```text
S = Σ |serialize(d_i)|                                  # standalone
U = Σ |serialize(e_i)| + Σ len(o)                       # unique reachable
A = Σ (|serialize(e_i)| + Σ len(o)/refcount(o))         # amortized
```

The amortized split is fractional by reference count and integerized so
`Σ A_i == U` exactly. `S` is the only whole-file-comparable universe. A fourth
universe — the derived cache — is added in Phase 11 and is never folded in.

## EntropyFS backend (optional)

`EntropyFsStore` / `EntropyFsSeedStore` adapt the embeddable `entropyfs` engine
(`default-features = false`); `BlobId` is BLAKE3-256, identical to our `Id`.
`list`/`remove` decline (`unsupported-feature`) because the engine exposes no
per-blob delete, so mark-and-sweep GC cannot reclaim through it. It is viable but
heavy and never required for the standalone form (ADR-0008/0020).

## Packed seed substrate (optional)

`--packed` replaces the one-file-per-node `seed/` namespace with an immutable,
segmented, offset-addressed `fieldpack/` store: `NodeId -> (segment, offset,
len)`. Node identity is unchanged (`NodeId = BLAKE3-256(...)`), so field ids are
identical across backends and a packed store and a filesystem store of the same
descriptor are interchangeable; descriptor / manifest / index / cache remain
files. The measured win is **file/directory count** (111× fewer files, 62× fewer
directories over the correction re-measurement), **not bytes** — regular-file
bytes are at parity (ADR-0043, corrected by ADR-0049). Reads are safe `pread`
(`read_exact_at`); the crate forbids `unsafe`, so there is deliberately no
`mmap`.

## Packed-store durability model

The packed store is append-only and self-describing, so recovery is **prefix
recovery**: after any crash the recovered records are exactly a prefix of the
appended sequence, the torn tail is discarded, no partial node is ever
observable, every fetched node is re-hashed against its id, and a sealed segment
is never rewritten. The writer's durability policy is `SyncPolicy::Batch`
(default) — one sync per segment, at seal and an explicit flush — or
`SyncPolicy::Each` (one `fdatasync` per seed node, the stronger per-node
barrier). `put_field` **flushes before publishing a manifest**, so a durable
manifest never references a non-durable node. Batching these syncs is what
inverted the equal-contract build position (7.08× → 0.82×); the win is deleted
unnecessary durability syncs, **not** parallelism or a codec (ADR-0053).

## Derived cache

Observation caches are disposable, off-wire, and closure-keyed. `cache --clear`
reclaims them. A cache-served observation must count the cached payload it
physically re-reads before it is compared to another system's byte read (ADR-0027
correction).

## Cross-document sharing: recorded negatives

Three attempts, all measured, all negative:

| Attempt | Result | ADR / receipt |
|---|---|---|
| Object-granularity store (Phase 9) | unique-reachable `U = 3,369,900 B` loses to per-file min LZ `1,304,307 B` and to borg CDC `771,383 B`; robust (forced replay `U = 2,360,054 B`) but partly an externalization-granularity artifact | ADR-0021; `evidence/campaigns/2026-10-05-phase9-store-fdb2845/` |
| Finer-than-object units (Phase 11.14) | unique lower bound `208,001 B` loses to strongest CDC `160,668 B` and to `tar | xz -9e` `92,752 B`; per-stratum loss | ADR-0028; `evidence/campaigns/2026-10-06-phase11-share-d9f818a/` |
| State-level reuse (Phase 12.8) | warm `retained_inverse_work_fraction = 0.339907` falls to `0.0` after `cache --clear` (`N3` violated); only representation identity is shared | ADR-0034; `evidence/campaigns/2026-10-06-phase12-share-e7ef693/`, `…-share-controls-dce2705/` |

What survives is exact *representation identity*: a byte-identical resource
embedded in a DOCX and an EPUB resolves to one content-addressed blob
(`nodes_id_shared = 2`, `shared_resource_ids = 1`), with no second keying scheme.
That is a representation fact (0 bytes written for the shared blob), not durable
work reuse and not a size win — each document's exact descriptor is still stored
independently.

## Encoder-only search governance

Phase 10.1 adds an optional, encoder-only governor behind the non-default,
dependency-free feature `dsfb-search = []`. It produces typed integer residual
diagnostics, proposes candidates over existing mechanisms, and maps the
incumbent residual to a directive with a pure `govern` function. Zero decode
authority: no governance state is persisted, `header.rs` is untouched, and a
governor-produced descriptor decodes byte-exactly in a build without the feature.

Result (recorded negative). H1/H2/H4 hold, but the fixed heuristic already
attains the exhaustive minimum on every workload, so the parametric search adds
zero bytes (H3). The fixed complete-cost court is retained.

Receipt: `evidence/campaigns/2026-10-05-phase10-governor-d2b09c9/`. ADR-0022.

## Byte-level partial-materialization checkpoints

Phase 13.4 implemented the literal byte-level checkpoint records and measured
them against the Phase-8 seek floor. An optional `FLAG_OPTIONAL` `CHECKPOINT`
record (`0x60`, format `checkpoint_v1`) carries a bounded per-op output-boundary
table bound to the `GRAPH` record; the seek reader consumes a validated
checkpoint in place of the `OBSERVATION_INDEX` for a raw byte range, and a
lying/corrupt/non-optional checkpoint is rejected and the reader falls back to
the index lane. The mechanism is byte-exact and advisory, but a **recorded
negative**: it is materially redundant with the index's own op table, so the
checkpoint record is larger than the index it replaces and every query reads
*more* bytes (20/40/120 objects: +259/+439/+1,159 B per byte-range query) with
identical op work. Receipt:
`evidence/campaigns/2026-10-07-phase13-checkpoints-9d1306a/`. ADR-0039.

## Not yet built

No open byte-level partial-materialization item remains: the seek `DIRECTORY`
(Phase 8), the observation engine (Phase 11), and the byte-level checkpoints
(Phase 13.4) are all delivered and measured.

## Relevant ADRs

[0008](../adr/0008-entropyfs-optional.md),
[0020](../adr/0020-content-addressed-store.md),
[0021](../adr/0021-cross-document-sharing-result.md),
[0022](../adr/0022-encoder-only-search-governance.md),
[0025](../adr/0025-procedural-seed-dag.md),
[0027](../adr/0027-cost-accounting.md),
[0028](../adr/0028-finer-than-object-sharing.md),
[0034](../adr/0034-cross-document-identity-sharing.md),
[0043](../adr/0043-packed-seed-store.md),
[0049](../adr/0049-storage-accounting-correction.md),
[0053](../adr/0053-batched-packed-sync.md).
