# ADR-0054: Paired, interleaved, repeated measurement with an interval — and why a performance claim must name its estimator

- **Status:** Accepted — measurement discipline (Phase 19.1/19.2)
- **Date:** 2026-10-08

## Context

Phase 18.5 recorded two equal-contract headlines as **single-run point estimates
reduced to a best-of-3 minimum**: the direct packed build at **0.82×** SQLite and
the warm one-session lane at **1.09×**. Both ratios were computed as a **ratio of
sums** (Σ VOLE / Σ SQLite over the 12-document subset). The build sum is
dominated by one document (`nist-pdf-0017` is 81% of VOLE's sum), so a ratio of
sums is a statement about the two lanes' *grand totals*, not about a typical
document. Phase 19.1 re-ran the same court with paired per-rep repetitions and
found that the two natural estimators disagree sharply on the build (paired
median **0.182** vs ratio of sums **~0.96**), and that the min reduction moves
both lanes' sums in the same run. A headline that does not state which estimator
produced it is therefore ambiguous.

The bind-mounted host store is also noisy enough (per-cell between-rep CV 5.4%,
p90 9.4%) that a point estimate without an interval cannot distinguish a small
effect from parity.

## Decision

1. **Paired, interleaved repetitions.** Every measured quantity is collected for
   **both** lanes **within each repetition** at every depth, in an interleaved
   order (odd reps lane A→B, even reps B→A; recorded per rep in `raw/order.tsv`),
   with a per-rep untimed warm-up so a freshly rewritten store's first read is not
   timed. Cross-lane comparisons use the **per-rep paired ratio**, never two
   independently sampled means.
2. **Retain every sample.** Nothing is reduced to min/median at collection time;
   the raw per-sample tables are part of the receipt. Reductions are re-derivable
   estimator choices.
3. **Report an interval, not a point.** Quote a fixed-seed **bootstrap 95% CI**
   (20,000 resamples), **cluster-resampled by document** — a document's repeated
   measures are correlated and are not treated as independent documents. Report
   **both** the median and the geometric-mean paired ratio; report the per-
   document **±10% tie band** (win/tie/loss) alongside.
4. **State the estimator.** A ratio is reported together with the estimator that
   produced it: **paired per-rep median / geometric mean** (a typical-rep
   statement) versus **ratio of sums** (a grand-total statement, which one heavy
   document can dominate). When the two disagree, **report both**; never silently
   substitute one for the other. Retain the best-of-3-min reduction only as an
   explicit control against the full-sample reduction.
5. **A claim is not a claim without its estimator and interval.** Any performance
   sentence must name the estimator and the interval in which it is asserted.

## Consequences

- **Phase 18's 0.82× build is superseded as a point estimate.** On the same
  samples, the paired per-rep build median is **0.182** (95% CI 0.102–0.228;
  11 win / 0 tie / 1 loss) — entirely below 1.0 — while the ratio of sums reads
  **~0.96**. The 0.82 → 0.96 move is **SQLite-lane host variance** (1893 → 1582
  ms); VOLE's own build sum is reproducible (1545.5 vs 1552 ms). The build win
  stands; its magnitude is estimator-dependent and is now stated as such.
- **Phase 18's 1.09× warm is superseded.** At N=100 the pooled median paired warm
  ratio is **1.292** (95% CI **0.994–1.658**, includes 1.0) and the geometric mean
  is **1.283** (95% CI **1.069–1.535**, excludes 1.0); VOLE is ~0.7 ms slower per
  session. The warm lane is a **modest real loss that is marginal under the median
  estimator and resolved under the geometric mean** — not parity, and not the
  1.09× point estimate.
- **Resolution limits are quoted.** At N=100 the median-CI half-width is ±0.332
  and the normal-approx MDE(80%) is ≈0.474; a smaller true median shift is not
  separable from 1.0 at this variance floor. Wide intervals are reported as wide.
- **Cost.** The paired design roughly doubles the measured work per repetition and
  requires retaining the raw samples; the tables are larger. This cost is the
  price of an interval.
- **Falsifier.** If a claim ever quoted a ratio without its estimator and interval,
  or silently replaced a ratio-of-sums with a paired median, the discipline would
  be violated. If a paired design and an independent sampling design disagreed
  beyond their stated intervals, the pairing assumption would be wrong. The 19.2
  independent identical re-run reproduced "includes 1.0", supporting the design.
- **Scope.** This is a measurement discipline, not a mechanism: it changes no wire
  byte, no decode path, and no `encode` output. It governs how future courts are
  read.

## References

- `docs/phases/phase-19-results.md` (19.1, 19.2)
- `evidence/campaigns/2026-10-08-phase19-repeat-d6c8c4c/` (paired N=10)
- `evidence/campaigns/2026-10-08-phase19-warm-6b66eab/` (warm N=100)
- `evidence/campaigns/2026-10-08-phase18-contract-packed-14a7e6f/` (the superseded
  single-run point estimates)
- ADR-0027 (cost accounting); ADR-0049 (storage accounting must state its method)
