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

- [ADRs](adr/README.md) — every frozen decision.
- [Phase plans and results](phases/) — the per-phase record.
- [Independent reviews](reviews/) — adversarial skeptic findings.
- [Evidence index](evidence/README.md) — campaigns and measurement reports.

## Project

- [Status and mechanism ledger](project/status.md) — the single authoritative status table.
- [Findings](project/findings.md) — consolidated results, positive and negative.
- [Roadmap](project/roadmap.md) — open `PROPOSED` items.
- [Changelog](project/changelog.md) — release history.
- [Documentation migration ledger](project/doc-migration-ledger.md) — where every old section went.
