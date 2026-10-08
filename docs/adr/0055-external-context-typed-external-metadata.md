# ADR-0055: External facts are a separate typed layer — `ExternalContext`, basis `ExternalMetadata`

- **Status:** Accepted — the C4b action closes under an equal external input (Phase 20.4)
- **Date:** 2026-10-08

## Context

The contract court (C0–C5) asks a source-retaining SQLite baseline and VOLE to
answer the *same* escalating contract. Phase 16.5 recorded that VOLE declined
C4/C5; Phase 17.2 split the decline in two
([ADR-0052](0052-revision-lineage-surface.md)):

- **C4a — document-native lineage** (a PDF's internal incremental chain): VOLE
  answers it natively; it is byte-derivable, so it is a *where-the-work-happens*
  difference, not hidden information.
- **C4b — corpus/external lineage** (a dataset family id / member id / head
  flag): **external metadata a single document cannot derive.** A PDF does not
  know which corpus family it belongs to or whether it is its family's head.

Phase 17.2 and 18.1 recorded C4b as a **contract-definition** question: either
the C4 tuple is the right definition (and a single-document exact field is the
wrong tool by construction), or VOLE should ingest the metadata as an explicit
external input and answer it as a derived observation. Phase 20.4 answers that
question **without contaminating the document-derived field**.

## Decision

1. **External facts are a separate, explicitly-typed layer.**
   `ExternalContext { dataset_id, lineage { family, member, head,
   revision_family }, origin (closed enum `harness|operator|catalog`), source }`
   is canonically encoded (`VOLECTX1`, versioned, fail-closed on bad
   magic/version/trailing bytes) and stored **beside** the document-derived
   field at `<store>/external/<FieldId>`. It is **not** in the seed DAG, the
   observation index, the field manifest, the descriptor blob, or the exactness
   authority. Removing it is one `unlink` and touches nothing else.

2. **The answer carries an explicit, honest basis.** A new
   `Selector::ExternalLineage` (`external-lineage`) is answered for
   `--kind lineage`, through `observe --external-lineage --kind lineage` and
   `field-external --store --field (--lineage FAMILY:MEMBER:HEAD | --clear)`.
   Every answer uses `Basis::ExternalMetadata`, whose `is_exact()` is **false**;
   the answer record reports `basis=external-metadata`, `exact=false`, empty
   `dependency_ids`, `integrity_scope=none`, `bytes_read=0`, and
   `provenance=external-context;origin=…;source=…`.

3. **Absence is a typed decline, never a guess.** A field with no attached
   context returns `UnsupportedFeature` (rc 6). The facts are never a default,
   never inferred, and never on the decode path.

4. **The layer is provenance-isolated by construction.** Because the layer is
   disjoint from the exactness authority, every document-derived observation and
   `materialize(descriptor) == original_bytes` are **byte-identical** whether or
   not a context is attached; an external input cannot reach decoder authority.

## Measured result (Phase 20.4)

Receipt:
[`2026-10-08-phase20-c4b-5ab2e76`](../../evidence/campaigns/2026-10-08-phase20-c4b-5ab2e76/);
[phase-20-results.md](../phases/phase-20-results.md), 20.4. The court supplies
the SAME family/member/head to **both** lanes (SQLite at build; VOLE as a typed
`ExternalContext`):

| assertion | result |
|---|---|
| plain `--metadata` byte-identical before-attach / after-attach / after-clear | **12/12** |
| `materialize --exact` matches attached and after removal | **12/12** |
| external query declines typed (rc 6) after clear | **12/12** |
| C4b answered by both lanes under an equal external input | **12/12** |
| VOLE's C4b tuple equals SQLite's | **12/12** |

Cost over the 12-document subset: VOLE attach **26 ms** writing **1001 B** of
sidecar; external query **8 ms** (reads only the sidecar) versus SQLite's
**19 ms** C4 query (folded into its C4 build). Storage **0.49×**, build
**7.34×**, warm **1.38×** SQLite — the frontier is otherwise unchanged from
Phase 18.1/19.

## How this closes C4b — and what it does not claim

**C4b is no longer a VOLE capability gap.** Supplied the same external fact,
both lanes answer the tuple and the tuples match. The remaining difference is
**cost**, not coverage.

What the ADR does **not** claim:

- It does **not** say C4b is document-derivable. It is dataset metadata, and the
  answer basis says so (`external-metadata`, `exact=false`); the tuple must never
  be cited as a document-derived observation.
- It does **not** fold the external answer into the heterogeneous batch. **C5b
  is not measured** (the court measures the C4b query as a separate step beside
  the schedule); that is a recorded residual.
- It does **not** weaken provenance, mutate the core field, or let an external
  input touch decoder authority — the ADR-0050/0052 open question would stand
  unchanged if any of those were required. None was.

## Consequences

- A real capability is added: an explicit, typed, removable external layer that
  answers corpus/external lineage without touching the exact field.
- The exactness path is untouched; the sidecar is store-adjacent and separately
  reversible, so a mistaken attach cannot corrupt the field.
- The open C4b *coverage* question from
  [ADR-0052](0052-revision-lineage-surface.md) closes under an equal external
  input. The Phase-16 substrate question
  ([ADR-0050](0050-sqlite-as-substrate-question.md)) is unchanged and no option
  of it is chosen here.
- A benchmark must not be satisfied by letting external metadata leak into the
  document-derived field; this ADR makes that separation structural.

## References

- `src/field/external.rs` (`ExternalContext`, `ExternalLineage`,
  `ExternalOrigin`, `VOLECTX1`); `src/field/mod.rs`
  (`put_external_context`/`get_external_context`); `src/field/observe.rs`
  (`Selector::ExternalLineage`); `src/field/provenance.rs`
  (`Basis::ExternalMetadata`)
- `evidence/campaigns/2026-10-08-phase20-c4b-5ab2e76/`
- [phase-20-results.md](../phases/phase-20-results.md) (20.4);
  [phase-17-results.md](../phases/phase-17-results.md) (17.2, the split)
- ADR-0052 (revision-lineage surface; the C4 split); ADR-0050
  (SQLite-as-substrate open question); ADR-0026 (observation/provenance model)
