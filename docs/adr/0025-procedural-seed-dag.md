# ADR-0025 — Procedural entropy seed DAG and its persistence

Status: accepted (Phase 11.0).
Extends ADR-0020 (content-addressed store), ADR-0021 (sharing granularity).

## Context

Phase 9 externalized *coarse* `OBJECT` entries into a content-addressed store and
lost to per-file LZ and to CDC: the granularity was wrong, and most
representation state (channels, graph/inline state, models, replay state,
framing) stayed embedded. EntropyFS's `Engine` is a blob store with a **closed**
representation algebra whose only procedural variant is the `UniformXofV1`
negative control, and no caller-injectable encoder — so it cannot natively hold
VOLE procedural semantics (research C).

## Decision

Introduce a **VOLE-owned, versioned, canonical procedural seed node** and store
each node as an individual content-addressed blob.

```text
SeedNode {
    canonical_version,
    node_kind            // computation kind, not storage kind
    materializer_id, materializer_version,
    dependencies[],      // canonical NodeIds actually read (dynamic read set)
    output_kind, logical_output_len,
    entropy_state? coordinate? transform?,
    residual_refs[], literal_refs[],
    source_provenance, resource_limits,
    content_id           // == NodeId
}
```

* `NodeId = BLAKE3-256("VOLE:PSEED:v1" || canonical_state || canonical dep ids)`,
  **domain-separated** from the object table's un-prefixed `Id`.
* The DAG is immutable and cycle-free. A changed input hashes to a *different*
  node: green = present with complete id-matching closure, red = absent. No
  mutation, no validity flags, no invalidation pass — early cutoff is exact.
* Nodes are independently fetchable in small units. `SeedStore` exposes
  `put`/`get`/`get_range`/`contains`/`list`, mirroring `ObjectStore`.
* `FsSeedStore` (files under the store root, atomic tmp→sync→rename, `pread`
  range reads) is the reference substrate. `EntropyFsSeedStore` stores one node
  per blob through the existing `entropyfs-store` feature; nodes are opaque to
  EntropyFS, and we **never claim** EntropyFS natively stores VOLE procedural
  state.
* No arbitrary execution: a node names a bounded, versioned materializer from a
  fixed registry. Unknown mandatory materializers fail closed. No plugins, no
  embedded scripts, no decode-time inference.
* `content_id` is a relational/advisory identity, never in `INTEGRITY`.

## Consequences

* Reuse is proved by **execution counters across a process boundary**
  (`nodes_executed` drops, `nodes_reused` equals the closure size), never by
  wall-clock and never by bytes alone — this kills the "warm HashMap" illusion.
* Cache/index bytes are a fourth accounting universe, never folded into the
  descriptor/store/source universes.
* GC generalizes the one-level object mark-sweep to a transitive closure with
  per-level `dangling` reporting.
* Cross-document sharing is not an acceptance gate: Phase 9 showed byte-level
  sharing loses; the Phase-11 benefit may be reuse of *state identity*, not
  fewer bytes.
