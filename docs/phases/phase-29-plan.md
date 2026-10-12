# Phase 29 — The winning mechanisms (generalize fused execution, coarse index, incremental observation)

> **Status:** planned (frozen contract below). Branch `phase29`.
> Phase 28's three decision gates set this order. Phase 29 builds only what the
> gates justified, each with its own falsifiable court. Same rule as every
> subphase: exactness is the invariant, negatives are recorded, no claim without
> evidence.

## What the gates decided (the mandate)

- **Experiment 1 (28.5): POSITIVE.** A bounded native fused kernel is a resolved
  end-to-end win over the shipped path, but it currently covers one format (Arrow)
  and one operator, and still **loses to pyarrow on large inputs (6.46×)** and the
  aggregate. → **generalize and push past the large-input gap.**
- **Experiment 2 (28.6): coarse-only.** The closure is the whole encoded document;
  a fragment directory is 42×–435× smaller but beats page-SQLite in only 3/5
  formats; per-record/tiny fragments lose to directory overhead. → **build a coarse
  structural index only** (PDF objects/pages, ZIP members, Parquet row groups).
- **Experiment 3 (28.7): marginal/scoped.** 83.4% of next-revision observations
  reuse on localized edits, but the win flips negative if per-observation identity
  nodes are persisted, and it is zero on whole-document redrafts. → **incremental
  observation only for localized edits, with the semantic edges that correctness
  requires.**
- **28.3 (NEGATIVE).** The `serve_source_range` clone is below the MDE; the
  dominant cost is derived-model decode → that is where kernel work pays.

## The order (frozen)

```text
29.1  Generalize fused execution — kernels for Parquet + CSV/tabular + more operators;
                                    vectorized/validity-aware; target the large-input pyarrow gap
29.2  Coarse structural index    — PDF objects/pages, ZIP members, Parquet row groups;
                                    measured narrower real closure, still exact
29.3  Incremental observation    — semantic dependency edges; localized-edit reuse;
                                    ADR-0060 preserved; index cost bounded
```

## Acceptance (per subphase, unchanged)

Exactness is the invariant (`materialize == source`, length + SHA-256 + `cmp`, after
source **and** descriptor deletion, in a fresh process); Docker only, digest-pinned
and memory-capped; one branch (`phase29`) with commit-and-push per subphase; a
sealed receipt per court; **every raw sample retained**; no cap raised to make a run
pass; no claim without its estimator and interval (ADR-0054) and storage accounting
(ADR-0049). Every kernel/index must be **bounded, no-panic, and typed-declining** on
anything outside its supported subset — never a wrong answer. The public observation
semantics and ADR-0060 identity rules are non-negotiable.

## Measurement discipline (carried from Phase 28)

In-process **steady-state** is the headline; **cold-process** and **cold-cache** are
reported separately. The comparator (pyarrow, DuckDB, optimized SQLite) gets the
**same raw-source authority and provenance obligations** and may build any index it
needs, with all construction + storage costs charged. Capability accounting
(missing/declined/incorrect) is reported. Losses stay in the published results. A
reported change with a CI straddling 1.0 is **inconclusive**, not a win.
