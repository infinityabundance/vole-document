# ADR-0060 — Source-scoped node identity: a node whose output reads the source must carry a source-identity input

Status: accepted (Phase 21.2.1).
Relates: ADR-0024 (field authority), ADR-0030 (ZIP physical layer), ADR-0049
(storage accounting), ADR-0059 (XLSX/analytical comparator).

## Context

The persistent field is a content-addressed seed DAG. Each node's `NodeId` is a
hash of its kind, its params, and its dependency ids, and the **on-disk derived
output cache** (`store/cache/<NodeId>`) is keyed on the `NodeId` **alone** and is
**shared across every field in a store**. `materialize` therefore relies on one
invariant:

> a seed node's output is a pure function of its `NodeId`.

Some nodes violate it. `DocumentExact`, `SourceSlice`, `PdfObject`,
`PdfRevision`, `PdfStreamEncoded`, `PackageRoot`, and `PackageMemberRaw` all
produce output **read from the source bytes** (`serve_document` or
`serve_range(span)`), but their ids were derived only from their params — which,
for a span node, are just `(offset, len)`, and for an exact root were empty. Two
different documents with a member at the same `(offset, len)` (very common for
structurally similar OOXML packages: `ppt/presentation.xml` was `comp=257 /
uncomp=541` in two different decks) hash to the **same** `NodeId`. The second
field's observation then hit the first field's cached output.

This was latent until Phase 21.2 built several PPTX fields sharing one store. It
manifested as a deck's common `text` collapsing to a single slide (the
presentation model returned was another deck's), and — found by the same audit —
`observe --byte-range ... --kind exact` returning **another document's bytes**.
Field ids were also store-content-dependent (the same source hashed differently
depending on what else was in the store).

## Decision

Every node whose output is read from the source bytes **must include a
source-identity input**, so distinct sources can never alias in the shared cache:

* **Exact roots** (`DocumentExact`, `PackageRoot`) key their params on
  `sha256(source)`. The **source digest**, not the descriptor id, is used: two
  different reconstruction programs that rebuild the *same* source must share the
  exact root (an existing invariant asserted by the phase-17 direct-field court).
* **Span-reading nodes** (`SourceSlice`/`ResourceRef`, `PdfObject`/`PdfRevision`/
  `PdfStreamEncoded`, `PackageMemberRaw`) take the field's exact-authority
  **root node id as a dependency**, so a member's identity includes its source's
  identity.

Every other node kind derives only from its params and deps and therefore
inherits source-scoping transitively. This changes only node **identity**, never
the emitted bytes, the answer shape, or the exactness path.

## Consequences

* The cache invariant holds again: the same node id always names the same bytes,
  within and across fields in one store. A deck's text, a slide's resolved part,
  and a byte-range's bytes are never served from a different document.
* **Field ids are now a pure function of the source**, independent of the store's
  other contents (verified by `tests/byte_range_identity.rs` and the PPTX
  regression `interleaved_pptx_fields_do_not_alias_in_a_shared_store`).
* Previously sealed field ids differ from the new ones for package/PDF fields;
  receipts record their own run, so this is a forward identity refinement, not a
  `.voldoc` wire-format change.
* A regression court now interleaves multiple fields in one store, and the PPTX
  economic court asserts the full common text (not a substring), so this class of
  cross-field contamination cannot pass unnoticed again.

**Rejected:** caching source-reading nodes on span/empty params; keying the derived
cache on (field, NodeId); using the descriptor id for exact roots (breaks
program-independence); leaving the `--byte-range` path unfixed as "pre-existing".
