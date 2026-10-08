# VOLE-Document documentation

Navigational index. The repository entrance is [`../README.md`](../README.md);
the exact-bytes invariant and the operational rules for agents live in
[`../AGENTS.md`](../AGENTS.md).

## Start here

- [Project README](../README.md) — what VOLE-Document is, in one page.
- [Security policy and threat model](SECURITY.md) — the normative hostile-input contract.

## Architecture (evergreen)

What the system is *today*, with rationale and receipts linked out to ADRs and
phase results.

- [Overview](architecture/overview.md) — the idea, the pipeline, the invariant.
- [Authority and exactness](architecture/authority-and-exactness.md) — the layered authority model.
- [Inverse proceduralization](architecture/inverse-proceduralization.md) — the DRA, residual channels, and the measured structural courts.
- [The document field](architecture/document-field.md) — persistent, queryable representation.
- [Observations and provenance](architecture/observations-and-provenance.md) — the typed observation algebra, `EXPLAIN`, and partial materialization.
- [Persistence and caching](architecture/persistence-and-caching.md) — the seed DAG, stores, indexes, and the derived cache.
- [Multi-format adapters](architecture/multi-format-adapters.md) — the shared ZIP layer and the PDF/DOCX/EPUB/ODT natives.

## Formats

Per-format authority boundaries, native inverse representation, and exactness.

- [PDF](formats/pdf.md)
- [DOCX](formats/docx.md)
- [EPUB](formats/epub.md)
- [ODT](formats/odt.md)

## Reference

- [Specification](reference/specification.md) — the `.voldoc` wire format (provisional).
- [Conformance and courts](reference/conformance.md) — what is tested and how.
- [CLI](reference/cli.md) — the command surface.
- [Format support](reference/format-support.md) — capability matrix.

## Research history

Frozen rationale and receipts, kept as a durable record (not rewritten).

- [ADRs](adr/README.md) — every frozen decision (through ADR-0058).
- [Phase plans and results](phases/) — the per-phase record. Recent results:
  [Phase 15](phases/phase-15-results.md), [Phase 16](phases/phase-16-results.md),
  [Phase 17](phases/phase-17-results.md), [Phase 18](phases/phase-18-results.md),
  [Phase 19](phases/phase-19-results.md), [Phase 20](phases/phase-20-results.md),
  [Phase 22](phases/phase-22-results.md) (economic programme — **complete**:
  22.1 competitor envelope, 22.2 profiling gate, 22.3.0 fused-execution gate,
  22.4 lifetime frontier, 22.5 remote materialization, 22.6
  compact-representation headroom, 22.7 agent cost; 22.2/22.3.0/22.5/22.6/22.7
  are recorded negatives and 22.4 a region split — see
  [phase-22-3-results.md](phases/phase-22-3-results.md),
  [phase-22-4-results.md](phases/phase-22-4-results.md),
  [phase-22-5-results.md](phases/phase-22-5-results.md),
  [phase-22-6-results.md](phases/phase-22-6-results.md),
  [phase-22-7-results.md](phases/phase-22-7-results.md)),
  [Phase 23](phases/phase-23-results.md) (durability: directory `fsync` + a
  model-based power-loss proxy),
  [Phase 25](phases/phase-25-results.md) (durability corrections: directory
  ancestry + unpublished-segment recovery); planned (not started):
  [Phase 21](phases/phase-21-plan.md); the Phase 22 programme is pre-registered in
  [phase-22-plan.md](phases/phase-22-plan.md).
- [Independent reviews](reviews/) — adversarial skeptic findings.
- [Evidence index](evidence/README.md) — campaigns and measurement reports.

## Project

- [Status and mechanism ledger](project/status.md) — the single authoritative status table.
- [Findings](project/findings.md) — consolidated results, positive and negative.
- [Roadmap](project/roadmap.md) — open `PROPOSED` items.
- [Changelog](project/changelog.md) — release history.
- [Documentation migration ledger](project/doc-migration-ledger.md) — where every old section went.
