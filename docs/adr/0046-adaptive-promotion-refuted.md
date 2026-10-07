# ADR-0046: Adaptive procedural promotion is refuted on the corpus (mechanism shipped opt-in, default-off)

- **Status:** Accepted — recorded negative result; mechanism opt-in (Phase 15.6)
- **Date:** 2026-10-07

## Context

The Phase-15 plan proposed persisting compact **structural tapes** (not just final
answers) and a cost governor that promotes a reusable intermediate only when
expected future saved work exceeds materialization + storage rent. The claim is
that an *adaptive* promotion policy beats fixed SQLite-Minimal/Full/Adaptive
baselines on an unforeseen-query-diversity frontier. Three falsifiers were
pre-registered before measuring.

## Decision

Implement the promotion layer behind `--promote[=BYTES]` (a durable,
byte-budgeted layer over the reused intermediates), **off by default**, never on
the exactness path, and run a diversity court plus a revision court. Adopt only
if the pre-registered falsifiers do not fire.

## Consequences

- **All three pre-registered falsifiers fire** (receipts
  `evidence/campaigns/2026-10-07-phase15-diversity-4786f8e/` and
  `…-phase15-revision-4786f8e/`):

  | falsifier | threshold | measured | verdict |
  | --- | --- | --- | --- |
  | F1 diversity | `v_on` > `sq_adapt` by >10% at some depth | never crosses; `sq_adapt` stays within 10% of the best lane at **every** depth | **REFUTES** |
  | F2 mechanism | promoted bytes cut durable bytes ≥20% at equal-or-better latency | **0.0%** cut, at equal-or-worse latency | **REFUTES** |
  | F3 revision | best lane's retained cross-revision work ≥20% | **+0.3%** for every lane | **REFUTES** |

- **`sq_full` is fastest at every depth.** The governor's durable store adds no
  byte distinction over the existing representation, so there is nothing worth
  promoting.
- **The mechanism ships opt-in and default-off.** It is not on the exactness
  path; a descriptor produced with promotion is identical to one produced without
  it. It is retained so the negative is reproducible.
- **Limits.** Three documents in the diversity court and four revision families;
  no population claim. The negative is recorded, not retired.

## References

- `--promote` in `src/main.rs`; the promotion governor module under `src/field/`
- `docs/phases/phase-15-results.md` (15.6)
- `evidence/campaigns/2026-10-07-phase15-diversity-4786f8e/`,
  `evidence/campaigns/2026-10-07-phase15-revision-4786f8e/`
- ADR-0022: encoder-only search governance (the same "a governor buys no bytes on
  the tested cohort" shape)
