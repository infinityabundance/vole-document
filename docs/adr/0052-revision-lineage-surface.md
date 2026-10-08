# ADR-0052: Expose PDF revision lineage — and why the contract's C4 tuple is external corpus metadata

- **Status:** Accepted — surface added; contract C4/C5 remain open (Phase 17.2)
- **Date:** 2026-10-08

## Context

Phase 16.5 forced a source-retaining SQLite baseline and VOLE to satisfy the
*same* escalating contract (C0 value → C1 native coordinate → C2
provenance/span → C3 exact closure → **C4 revision lineage** → C5 mixed batch).
VOLE **declined C4/C5** because its CLI exposed no revision query surface
([ADR-0050](0050-sqlite-as-substrate-question.md)). That was recorded as a
**surface** gap.

Phase 17.2 adds the surface and re-runs the court, which shows the Phase-16.5
decline actually **splits in two**: a *surface* half that VOLE can and now does
answer, and a *contract-definition* half that no single-document field can derive
from PDF bytes.

## Decision

1. **Expose revision lineage as a first-class observation.** Add
   `Selector::Revisions` (canonical `revisions`) and `Representation::Lineage`
   (`lineage`), with CLI `observe --revisions --kind lineage` and
   `observe --revision N --kind lineage`, advertised in capabilities.

2. **Compute it once at ingest from the byte-authoritative scan.** A new
   `NodeKind::PdfRevisionLineage` is computed **once** at PDF ingest from the
   existing Phase-3 byte-authoritative scan — never by re-parsing the source at
   query time — and indexed under `SEL_REVISIONS` (whole lineage) and
   `SEL_REVISION_LINEAGE` (per revision). An observe is therefore **O(depth)**
   into the store. The answer carries the `%PDF-` header, the revision count, the
   ordered revision indices and byte spans, the resolved `startxref`/`/Prev`
   chains, and each revision's object/stream membership.

3. **Non-PDF formats are a typed decline, never an empty answer.** Incremental
   revisions are a PDF concept with no DOCX/EPUB/ODT analog, so an observe on a
   format with no revision structure returns `UnsupportedFeature` (rc 6).

4. **The new nodes/index kinds are derived state only.** They change store bytes
   only. The `.voldoc` descriptor and `materialize(descriptor) == original_bytes`
   are untouched, preserving the decoder-authority boundary.

## Measured result (Phase 17.2)

Receipt:
[`2026-10-08-phase17-revision-1179386`](../../evidence/campaigns/2026-10-08-phase17-revision-1179386/);
[phase-17-results.md](../phases/phase-17-results.md), 17.2.

Contract court re-run (SQLite lane byte-identical to Phase 16.5, verified by
diff) on the same **12-document** subset:

| depth | VOLE cold ms | SQLite cold ms | VOLE warm ms | SQLite warm ms | VOLE B | SQLite B | VOLE satisfies? |
|---|---:|---:|---:|---:|---:|---:|---|
| C0 | 138 | 129 | 37 | 25 | 6,988,757 | 14,270,464 | yes |
| C1 | 135 | 128 | 38 | 25 | 6,988,757 | 14,376,960 | yes |
| C2 | 136 | 124 | 36 | 28 | 6,988,757 | 14,622,720 | yes |
| C3 | 129 | 125 | 37 | 27 | 6,988,757 | 14,622,720 | yes |
| C4 | 133 | 129 | 37 | 29 | 6,988,757 | 14,721,024 | **no (different observable)** |
| C5 | 142 | 134 | 37 | 29 | 6,988,757 | 14,721,024 | **no (different observable)** |

Cost vs SQLite on this subset: storage **0.47×** (lineage adds **74,980 B** over
12 documents), build **10.74×**, cold 138 vs 129 ms, warm 37 vs 25 ms.

## The C4 finding

**The surface half is fixed.** VOLE now answers a revision-lineage observation
for every PDF, cold and in a mixed batch, where Phase 16.5 declined.

**C4 still does not close.** The contract's C4 tuple is the **corpus
family/member/head** — external metadata frozen with the corpus, which the PDF
bytes cannot derive. So the two lanes answer *different* observables: VOLE
reports the PDF's internal incremental revision chain; the baseline reports the
corpus tuple. Neither value can equal the other by construction, so the court
records it as **`different observable`** — never equality, never an error. For
DOCX/EPUB VOLE declines typed while the baseline answers the corpus tuple.

The honest conclusion is therefore a **contract question**, not a code gap:
either the contract's **C4 definition** is the right one (in which case a
single-document exact field is the wrong tool for C4 by construction), or VOLE
should **ingest corpus/revision-family metadata as an explicit external input**
and answer the tuple as a derived observation. Phase 17 does not decide this; it
records both, with the measured basis.

## Residual risk (recorded, not hidden)

Lineage fidelity is bounded by the Phase-3 `%%EOF` scanner. On `nist-pdf-0016`
the scanner splits at an embedded early `%%EOF` at offset 505 and reports an
inverted `/Prev`. This is **not** an independent PDF-conformance oracle; a
purpose-built oracle would be required to separate scanner error from a genuinely
unusual document.

## Consequences

- A real capability is added: revision lineage is now queryable cold and in a
  mixed batch, with a typed decline for formats that have none.
- **C4/C5 remain open** and the query is re-framed: the missing piece is corpus
  metadata, not a selector. ADR-0050's substrate question is unchanged, and none
  of its options is chosen here.
- Exactness, the descriptor, and `materialize == bytes` are untouched; the new
  node/index kinds are derived and rebuildable.
- The lineage answer must not be cited as a PDF-conformance claim while its
  fidelity rests on the `%%EOF` scanner.

## References

- `evidence/campaigns/2026-10-08-phase17-revision-1179386/`
- `evidence/campaigns/2026-10-08-phase16-contract-45d2c0e/` (the C4 decline)
- `docs/phases/phase-17-results.md` (17.2); `docs/phases/phase-16-results.md` (16.5)
- `src/field/observe.rs` (`Selector::Revisions`, `Representation::Lineage`),
  `src/field/node.rs` (`NodeKind::PdfRevisionLineage`),
  `src/field/index.rs` (`SEL_REVISIONS`, `SEL_REVISION_LINEAGE`)
- ADR-0050 (SQLite-as-substrate open question); ADR-0036 (PDF `/Length`/revision
  as a size mechanism); ADR-0009 (PDF bytes are the authority);
  ADR-0026 (observation/provenance model)
