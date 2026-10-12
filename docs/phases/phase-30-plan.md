# Phase 30 — Close the measured gaps (large-input kernels, reading requests, index cost, framing)

> **Status:** 30.1 **SHIPPED**; 30.2–30.4 **PENDING** — the contract below stays
> frozen. Branch `phase30`. Results: [phase-30-results.md](phase-30-results.md).
> Phase 29 produced the first generalized, court-verified execution wins but left
> four recorded gaps. Phase 30 attacks them in order, each with its own falsifiable
> court. Same rule as every subphase: exactness is the invariant, negatives are
> recorded, no claim without evidence.
>
> **Progress (2026-10-13).** **30.1 shipped** — the large-input fused kernels took
> the Arrow large lane **≈2.87 → 0.65** and Parquet **≈1.28 → 0.61** (every cell
> `< 1.0`) and cut CSV **≈46.2 → 35.5** (improved ≈23 %, still 10.5–42× slower),
> with the full-matrix pyarrow median **0.0772 → 0.0461** (W/T/L **406/41/163 →
> 489/8/81**) and exactness **3/3 · 12/12**. **30.2 (coarse index for reading),
> 30.3 (incremental index cost), and 30.4 (standing negatives) remain PENDING** —
> no implementation, measurement, or claim.

## The gaps (from the Phase-29 results)

1. **Fused kernel vs pyarrow on large inputs.** Large Arrow ~2.7–3.0× **slower**;
   CSV 15–51× **slower**; Parquet wins only the stats-skip `count`.
2. **Coarse index helps only span requests** (1/4 formats); the natural **reading**
   request narrows for PDF only, because the DOCX/ODT/Parquet model materializes
   the whole source.
3. **Incremental observation index cost 27.3%** of source (above the ≤10% bar),
   and reuse is **0%** on whole-document redrafts.
4. **Standing negatives:** 15 detection findings; legacy bounded subsets; Arrow
   legacy (pre-0.15) no-continuation framing.

## The order (frozen)

```text
30.1  Large-input fused kernels   — SHIPPED: Arrow + Parquet past pyarrow (every cell < 1.0); CSV improved, still lossy
30.2  Coarse index for reading    — PENDING: serve the natural reading request from coarse member spans
30.3  Incremental index cost      — PENDING: dependency-signature-only index (< 10%); keep correctness
30.4  Standing negatives          — PENDING: detection findings, legacy subset edges, Arrow legacy framing
```

## Acceptance (per subphase, unchanged)

Exactness is the invariant (`materialize == source`, length + SHA-256 + `cmp`, after
source **and** descriptor deletion, in a fresh process); Docker only, digest-pinned
and memory-capped — **one container at a time, clear the 26 GB host-RAM-backed
`dev` target after heavy builds**; one branch (`phase30`) with commit-and-push per
subphase; a sealed receipt per court; **every raw sample retained**; no cap raised
to make a run pass; no claim without its estimator and interval (ADR-0054) and
storage accounting (ADR-0049). Kernels/indexes stay bounded, no-panic, and
typed-declining outside their subset. In-process **steady-state** is the headline;
cold is separate; a CI straddling 1.0 is inconclusive, not a win.
