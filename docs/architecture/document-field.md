# The document field

A persisted document is a **field**: a persistent, content-addressed procedural
substrate that can serve many typed observations, of which the exact original
bytes are exactly one. The field is the Phase-11 idea; Phase 12 made it
multi-format (see [Multi-format adapters](multi-format-adapters.md)).

The exactness contract is unchanged: with all field machinery deleted,
`materialize(descriptor) == original_bytes` still holds. Authority is layered
(see [Authority and exactness](authority-and-exactness.md), ADR-0024/ADR-0029).

## Procedural seed DAG

Each recovered computation is a canonical, versioned node stored one blob per
node in a content-addressed seed store (ADR-0025):

```text
NodeId = BLAKE3-256("VOLE:PSEED:v1" || canonical_state || canonical dep ids)
```

- The id is domain-separated from the object table's un-prefixed `Id`.
- The DAG is immutable and cycle-free. A changed input hashes to a *different*
  node: green = present with a complete id-matching closure, red = absent. There
  is no mutation, validity flag, or invalidation pass.
- A node names a bounded, versioned materializer from a fixed registry. Unknown
  mandatory materializers fail closed; there are no plugins, scripts, or
  decode-time inference.

## Hierarchical observation index

A bounded, advisory, re-derivable tree keyed by selector (fanout ≤ 256, depth ≤
3, node ≤ 8 KiB) accelerates navigation and never defines truth. A
lying/cyclic/out-of-closure/oversized node is rejected fail-closed; a missing
index falls back to the honest prefix path (ADR-0024/0026).

## Progressive inverse compiler

Recovery is staged: **Stage A** durable capture; **Stage B** cheap eager
inversion (physical spans, revisions, objects, streams, decoded streams, page
tree, `/ObjStm`-hosted page objects); **Stage C** demand-driven deepening.
Progressive inversion is the only implemented mode.

## Observation engine

Typed selectors × representations, each answer a typed `FieldAnswer` with a
`basis` and `integrity_scope`, deterministic planning, and
`EXPLAIN`/`EXPLAIN ANALYZE`. See
[Observations and provenance](observations-and-provenance.md).

## Selective late materialization and reuse

A narrow observation resolves its minimum dependency closure and materializes as
late as possible. The derived cache is off-wire, closure-keyed, and disposable.
With the source deleted, a fresh process can still query the field and
rematerialize the exact original.

## Measured results (Phase 11)

The courts are deliberately mixed. Four accounting universes are kept separate
(source / descriptor+store / procedural field / derived cache; ADR-0027).
Representative page-1-text numbers
(`evidence/campaigns/2026-10-06-phase11-desc-free-a8ad6f4/`):

| Lane | Bytes read | Wall | Note |
|---|---:|---:|---|
| Field, warm (cache-served) | 8.4–8.9 KB overhead | ~99 µs | 0 descriptor bytes |
| Field, cold (partial descriptor) | 0.08–0.36 MB | — | re-derives text from the page channel |
| A1 preprocessed SQLite | 24,393 B | ~1 ms | one-time extraction charged |
| A0 Poppler `pdftotext -f1 -l1` | 136,128 B | ~8 ms | — |

Wins (scoped):

- Warm observations read **0** descriptor bytes and execute **0** seed nodes.
- Warm wall is a real win (~99 µs vs A1 ~1 ms and A0 ~8 ms).
- A cold narrow observation reads its record closure (~0.22–0.36 MB), not the
  whole ~17 MB descriptor (−98%).
- Exactness after source removal: length + SHA-256 + `cmp`, fresh process.

Corrections recorded by the independent skeptic
([phase-11-skeptic-review.md](../reviews/phase-11-skeptic-review.md)):

- The 8.4–8.9 KB figure is **overhead-only**; the warm process also re-reads the
  cached answer payload (65.9 KB on `large-400`, 527 KB on `large`), so the warm
  *byte* win over A1 is withdrawn on the large cases. The warm *wall* win and
  the vs-A0 win stand.
- The seed class is 449 B cold / 0 B warm, but the honest total cold observation
  is 232–364 KB (descriptor closure + index); “bounded” means
  page-closure-bounded, not O(1).
- Cross-process reuse is served by the disposable derived cache, not by seed-DAG
  recomputation: after `cache --clear` a fresh process re-executes the nodes.

The fair A1 preprocessed-SQLite baseline wins the narrow-query byte court
(24,393 B) and its wall crossover is absent on 3 of 5 documents. The
pinned-tokenizer (`bert-base-uncased`, offline, hash-verified) token court is
2 win / 2 tie / 2 loss vs page-local extraction — a working-set measure, never a
text-quality claim.

## Immutable edit witness (declared narrow subset)

`src/field/edit.rs` provides an immutable edit witness that shares the
descriptor, the root id, and every unaffected seed node and index entry **by
content id** (0 descriptor bytes read, 0 blobs opened), adding 2 new seed nodes.
It overrides one page's content per call (≤ 48 KiB); `materialize(R1)` still
yields the original bytes. Its cost (43,688 B of index read vs 8,776 B written)
is recorded as a loss.

## Recorded negative: cross-document work reuse (`N3`)

ADR-0034 defines the reuse fraction as state/work, not bytes, and mandates a
post-`cache --clear` control, an OS witness, and a CDC baseline. With those
controls (`evidence/campaigns/2026-10-06-phase12-share-controls-dce2705/`), the
warm reuse fraction `0.339907` falls to **0.0** after the cache clear, in-process
and in a fresh process (`nodes_reused = 0`). Per ADR-0035, cross-document
**work** reuse is recorded as a negative; only exact *representation identity*
(one content-addressed blob shared DOCX↔EPUB) remains genuinely shared.

## Relevant ADRs

[0024](../adr/0024-document-field-authority.md),
[0025](../adr/0025-procedural-seed-dag.md),
[0026](../adr/0026-observation-query-provenance.md),
[0027](../adr/0027-cost-accounting.md),
[0034](../adr/0034-cross-document-identity-sharing.md). Results:
[phase-11-results.md](../phases/phase-11-results.md),
[phase-12-results.md](../phases/phase-12-results.md).
