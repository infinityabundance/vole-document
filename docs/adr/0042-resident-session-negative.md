# ADR-0042: Residency (a `DocumentFieldSession` with `observe-batch`) does not beat the cold per-observation lane above ~1 MiB (recorded negative)

- **Status:** Accepted — recorded negative result (Phase 15.2)
- **Date:** 2026-10-07

## Context

The Phase-15 plan's premise is that the `real100-v1` frontier court was taken
under a **per-query-process** model, so repeated observation pays a process spawn
and a store re-open per query. The obvious repair is a resident runtime: open the
field and store once, map the indexes, parse the manifest once, keep typed-model
/ decoded-member / validated-node caches and reused buffers, and expose an
`observe-batch` API that serves many observations in one process. The question is
whether that residency, on its own, is a measurable win.

## Decision

Implement `DocumentFieldSession` and an `observe-batch` command (one process
serving many observations, one JSON answer per line), then measure it against the
cold lane on the frozen `real100-v1` court. Record the result; do **not** retire
the mechanism on a single negative.

## Consequences

- **Measured result (receipt
  `evidence/campaigns/2026-10-07-real100-release-resident-78f7ea8/`):** on the
  `all`/`text_repeat` aggregate the **cold** lane is faster (**7 ms** median vs
  the resident lane's **9 ms**). By size class, the resident lane (`v_r`) wins
  only below ~1 MiB (`<100KiB`, `100KiB-1MiB`); the cold lane wins at
  `1-10MiB`, `10-50MiB`, and `50-100MiB`.
- **Mechanism (the interesting part).** The cold `observe` path runs
  `narrow_probe` — a per-call manifest + derived-cache **short-circuit** that
  returns the derived node *without* building the full evaluation context. The
  `observe-batch` path always evaluates the full path. Above ~1 MiB the
  per-observation cost of the full path exceeds the once-per-batch process-spawn
  saving, so residency alone loses.
- **Identified fix (recorded, not shipped).** Hoist `narrow_probe`'s
  manifest/index opens into the session, so the resident lane keeps the
  short-circuit for the derived-node case. Until then the resident lane is not
  the lever Phase 15 is looking for.
- **The resident lane is still the only lane that answers a heterogeneous
  `session_mixed` batch** (the others declare it a typed decline). This is an
  informational witness, not a verdict.
- **Limits.** One frozen corpus; the aggregate sits near the process/cache noise
  floor, so by-size strata are the meaningful units. No population claim.
- **Interpretation.** Repeated text observation costs ~1–2 ms per observation
  regardless of lane on this corpus; the dominant lifetime costs are **ingest**
  (encode + `field-ingest`) and **storage**, not repeated observation.

## References

- `src/field/session.rs` (`DocumentFieldSession`, `observe_batch`)
- `src/main.rs` (`observe-batch`); `docs/phases/phase-15-results.md` (15.2)
- `evidence/campaigns/2026-10-07-real100-release-resident-78f7ea8/`
