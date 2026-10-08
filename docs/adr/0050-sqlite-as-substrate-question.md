# ADR-0050: Should the document field use an embedded DB as part of its physical substrate? (open question; no switch decided)

- **Status:** Open — recorded architectural question; **no switch decided** (Phase 16.5/16.6)
- **Date:** 2026-10-08

## Context

Phases 12–16 compared VOLE's persistent procedural field against a
source-retaining SQLite(+FTS) baseline **on axes VOLE chose**. Phase 16.5 forced
the baseline to satisfy the *same escalating capability contract* (C0 value → C1
native coordinate → C2 provenance/span → C3 exact closure → C4 revision lineage
→ C5 mixed batch) and asked the fairer question: **does SQLite still lose?**
Phase 16.6 then corrected the storage accounting that had hidden the answer.

The measured result (receipts below) is that **SQLite does not lose under the
equal contract**, and the earlier VOLE storage "advantage" was largely a
measurement artifact. That raises a first-order architectural question the user
explicitly asked to record rather than resolve: should the conceptual invention
keep *competing* with an embedded DB, or should it **use an embedded DB as part
of its physical substrate** for materialized observation state?

This ADR records the question, the measured basis, the options, and the next
measurements that would settle it. It changes no code and no wire byte.

## Measured basis

- **16.5 — contract-equivalent court**
  ([`2026-10-08-phase16-contract-45d2c0e`](../../evidence/campaigns/2026-10-08-phase16-contract-45d2c0e/);
  [phase-16-results.md](../phases/phase-16-results.md), 16.5). 12-document
  subset. Equality holds at C0–C3; **VOLE declines C4/C5** because its CLI has
  no revision query surface (`--revision N` = `unsupported observation`). SQLite
  builds **~10×** faster, serves the warm session **~1.47×** faster, ties on
  cold, and is the **only** lane that answers the full contract. Escalating
  C0 → C4 costs SQLite only **~+3 %** persistent bytes (the retained source blob
  dominates) with **flat** query cost.
- **16.6 — storage-accounting correction**
  ([`2026-10-08-phase16-storage-correction-2978e1d`](../../evidence/campaigns/2026-10-08-phase16-storage-correction-2978e1d/);
  [ADR-0049](0049-storage-accounting-correction.md)). `du -sb` counted 4096 B per
  directory inode, inflating the `fs` store **52 %**. Corrected to file-bytes-only
  over the 95-document common-success population: VOLE `fs`/SQLite **1.377× →
  0.906×**, packed/SQLite **0.921× → 0.914×**, packed/`fs` **0.669× → 1.009×**.
  Both VOLE backends are **at or below** SQLite on file bytes.
- **Net.** After 16.6, VOLE's *sole* measured edge over the baseline under the
  equal contract is storage — and it shrank to **~0.9×** (a ~10 % file-byte
  margin), against a **~10×** build-speed deficit [SUPERSEDED: Phase 18.5 measures VOLE at **0.82×** SQLite build on the same equal-contract court — a build advantage, not a deficit; see `docs/phases/phase-18-results.md` and ADR-0053], a **~1.5×** warm-latency
  deficit, and a hard **C4/C5 coverage gap**.

## The open question

Should the persistent document field (the inverse representation, the
dependency/provenance model, and the exact source closure) **keep competing with
SQLite as a from-scratch store**, or should it **use an embedded relational
database as part of its physical substrate** for *materialized observation
state* — indexes, caches, revision lineage, provenance tables — while the
inverse representation and provenance algebra remain the authored authority?

This is a substrate question, **not** an exactness question. Exactness is
`materialize(descriptor) == original_bytes` (length + SHA-256 + `cmp`) and is
unchanged by where derived/observation state lives. Any embedded DB would be a
**derived, advisory, rebuildable** substrate — never decoder authority — exactly
as `--packed`, the hierarchical observation index, and the derived cache already
are ([ADR-0008](0008-entropyfs-optional.md), [ADR-0024](0024-document-field-authority.md),
[ADR-0026](0026-observation-query-provenance.md)).

## Options (recorded; none chosen)

- **A — keep competing.** Close the capability gap first: add a **revision
  lineage query surface** to VOLE's CLI (the 16.5 C4/C5 decline) and the **lazy
  session open** isolated in 16.4, then re-run the equal-contract court. The
  inverse representation and provenance model remain the only differentiator.
- **B — embedded DB as substrate.** Keep the conceptual invention as the
  authority model, but store **materialized observation state** in an embedded
  relational DB (e.g. SQLite) instead of bespoke files / packed segments. The DB
  is a physical substrate for *derived* state; the exact standalone form stays
  sacred and SQLite never touches the decode path.
- **C — hybrid (optional backend).** Keep the current bespoke store as the
  standalone default and offer an **optional, non-default** embedded-DB-backed
  observation store, mirroring how `--packed` and `EntropyFS` are optional seed
  backends. This preserves the standalone form and lets a lifetime court choose.

## What would settle it

1. **A revision/lineage query surface** in VOLE's CLI, then a re-run of the
   16.5 contract court to see whether C4/C5 close and at what cost in bytes,
   build, and latency.
2. **Lazy session open**, then a resident re-measure (16.4) to see whether
   residency ever beats cold on large descriptors once the full `Field::open`
   parse is deferred.
3. **A substrate-swap court at equal capability:** the same C0–C5 contract over
   (a) the bespoke VOLE store, (b) a SQLite-backed observation store, and
   (c) the bespoke store plus a SQLite *index* — comparing persistent bytes,
   build time, cold/warm latency, file/directory count — **and** re-asserting
   `materialize --exact` on every lane.
4. **Maintenance/operational cost** of a bespoke store at equal capability
   (bytes, syscalls, file count, failure modes) versus an embedded DB.
5. **Storage at equal capability under the corrected unit** (16.6): the honest
   head-to-head is ~0.9×, not the old `du -sb` numbers.

## Decision

**No switch is decided.** No embedded DB is added, no CLI surface is changed, and
no store is replaced by this ADR. The question is recorded, with its measured
basis and the falsifiable next measurements above, so that it survives chat
history and can be settled by a court rather than by preference
(claim discipline; ADR-0023).

## Consequences

- The ledger keeps **both** facts: SQLite wins build, warm latency, and full
  contract coverage under equal capability (16.5); VOLE wins storage at ~0.9×
  after correction (16.6) and holds the exact standalone form.
- The field's remaining defensible differentiators are the **inverse
  representation**, the **dependency/provenance model**, and the **exact source
  closure** — none of which is a property of the storage engine. Where derived
  state physically lives is therefore an **implementation** choice, not a claim.
- If option B or C is ever taken, it must not contaminate decoder authority:
  exactness stays `materialize(descriptor) == original_bytes`, the DB stays
  derived/rebuildable, and the standalone form stays sacred (ADR-0008).
- **Do not** cite this ADR as a decision to adopt SQLite, nor as a vindication of
  the bespoke store. It records that neither is yet justified by evidence.

## References

- `evidence/campaigns/2026-10-08-phase16-contract-45d2c0e/` (16.5)
- `evidence/campaigns/2026-10-08-phase16-storage-correction-2978e1d/` (16.6)
- `docs/phases/phase-16-results.md` (16.4, 16.5, 16.6)
- ADR-0049 (storage-accounting correction); ADR-0042 (residency negative);
  ADR-0043 (packed seed store); ADR-0008 (EntropyFS optional); ADR-0023
  (consolidated findings); ADR-0024/0026 (field authority, provenance)
