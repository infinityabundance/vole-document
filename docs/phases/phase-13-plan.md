# Phase 13 — Closing the remaining proposals and the last open gate

Branch: `phase13` (merged to `main` as `v0.1.0-alpha.17`). Base: `main` @ `13efaf3`
(`v0.1.0-alpha.16`, Phase 12).
Status: **COMPLETE** (except the independent-review gate — see
[phase-13-skeptic-review.md](../reviews/phase-13-skeptic-review.md): the mandated
independent skeptic subagent was canceled, so that gate is only partially met and
is recorded, not claimed).

Phase 13 exists because Phase 12's own ledger still carries unmeasured `PROPOSED`
items and one open acceptance gate. It closes them honestly — measuring each, and
recording a negative where the mechanism loses (the project's precedent).

## Scope (from the Phase-12 review of `PROPOSED`/`Planned` entries)

1. **PDF `/Length`/revision proceduralization as a *size* mechanism** — never
   measured as compression (Phase 11 persists revisions as *observation* nodes
   only). Measure it head-to-head vs the RAW/`BYTE_RANS`/generic-compressor
   ladders; record the outcome.
2. **PDF grammar/templates** — never attempted. Measure a bounded
   grammar/template candidate that must pay its definition cost.
3. **Non-PDF/DOCX/EPUB adapters** — ODT first (ODF packaged in ZIP, so it
   reuses the 12.x ZIP layer — not the OPC graph — with an OpenDocument content
   model), then record other formats as still out of scope.
4. **Partial-materialization byte-level checkpoint records (beyond v1)** — the
   literal checkpoints left after Phase 8/11.
5. **Gate `N5`** (package-index-only explanation) — the only skeptic gate not
   sealed; close it with a receipt (or record why it is definitionally answered).
6. **Stale documentation** — the checkpoint rows and any other doc that no longer
   matches the ledger.

## Rules (unchanged)

Exactness is the invariant, never a competitive claim. Docker only; every service
digest-pinned and memory-capped. One production crate. Subagents one at a time
(research read-only under gitignored `research/subagents/phase-13/`). Commit +
push per subphase. An independent skeptic tries to falsify each headline.
Negative results are recorded, not buried. Every claim names corpus, workload,
and the accounting universes (ADR-0027).

## Subphases

- **13.0** plan + **stale-documentation fix** (reconcile README/PROJECT_STATE
  checkpoint rows and any other stale entry) + freeze.
- **13.1** PDF `/Length`/revision size-mechanism court (measure; expected
  negative) + receipt.
- **13.2** PDF grammar/templates court (measure; expected negative) + receipt.
- **13.3** ODT adapter over the ZIP layer (ODF, not OPC; OpenDocument content
  model; exact ODT bytes; common + native observations) + exactness/removal courts.
- **13.4** byte-level partial-materialization checkpoints + court.
- **13.5** `N5` gate receipt + sweep for any other open gate.
- **13.6** independent skeptic review + release (`v0.1.0-alpha.17`), then delete
  the branch.

## Acceptance

PDF no-regression preserved; each new adapter exact (`materialize == original`,
length + SHA-256 + `cmp`) and queryable after source+descriptor deletion in a
fresh process; every `PROPOSED` item either measured (with a recorded outcome) or
explicitly re-scoped; `N5` sealed; docs consistent with the ledger.
