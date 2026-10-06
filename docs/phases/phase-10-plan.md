# Phase 10 — DSFB encoder-only search governance, then the negative-results consolidation

Branch: `phase10`. Base: `main` @ `615997f` (`v0.1.0-alpha.11`).
Status: **IN PROGRESS**.

## Why

Two remaining pieces of the programme:

1. The brief's Phase 10: **DSFB encoder-only search governance** with **zero
   decode authority** — it may observe residuals and suggest candidate families
   or search budgets, but a suggestion only ever creates another candidate that
   must independently pass coverage, exactness, and complete-cost courts.
2. The honest **negative-results consolidation** the last nine phases earned:
   one authoritative document stating what was built, what was measured, against
   which baselines, what won, what lost, and why.

## Subphases

- **10.0** plan + independent research freeze (this file + a design subagent;
  inspect the real `dsfb` crate API and freeze the governance contract).
- **10.1** typed residual diagnostics + an optional `dsfb`-guided search governor
  (feature `dsfb-search`, never in the decode dependency path), plus DSFB's own
  court.
- **10.2** the top-level negative-results consolidation (`FINDINGS.md` +
  a superseding ADR), with receipt links.

## DSFB rules (from the brief, non-negotiable)

- **Zero decode authority.** DSFB is not in the decoder dependency path; the
  decoder never runs DSFB, never guesses, never searches. No hidden DSFB state
  is stored that a decoder has to rediscover.
- **Encoder-side observation only.** DSFB examines a typed *residual diagnostic
  trace* (source region, format structure, candidate family, residual class,
  residual magnitude, run/transition structure, periodicity, recurrence, local
  context, candidate cost) and may recommend: try a deeper template search, a
  different context partition, a grammar rule, xref prediction, a producer
  profile, a larger/smaller budget, or "stop and accept RAW".
- **A recommendation is only a candidate.** It must pass the same coverage,
  exact-reconstruction, and complete-cost courts as everything else.
- **Honest court.** On tractable workloads with an *exhaustive* candidate oracle,
  compare **Exhaustive / FixedHeuristic / DsfbGuided** on final complete bytes,
  candidates evaluated, CPU, wall time, and peak memory. The success condition is
  `DsfbGuided final cost <= FixedHeuristic final cost` while approaching
  Exhaustive with materially fewer candidates. If the marginal benefit is
  negligible, record that honestly (EntropyFS's own precedent: DSFB's benefit can
  shrink as the search floor improves). Do not manufacture a win.

## Consolidation rules (10.2)

- One authoritative `FINDINGS.md` + a superseding ADR; every claim links to a
  sealed receipt and names the baseline it was measured against.
- State plainly: what won (a scoped partial-decode CPU win; whole-object dedup of
  identical opaque files), what lost (whole-file size, random-access I/O, and
  cross-document sharing, all against purpose-built baselines), and why (the
  DRA/entropy/entropy-channel representation is coarser than LZ77 plus
  seekable-block formats).
- Keep claim discipline: exactness is shared with lossless compressors; no
  "compression" language for sharing; no population claims; the DRA/entropy
  stack is a substrate, not a magic seed.

## Invariants (unchanged)

Byte-exactness everywhere; Docker only; one `phaseN` branch; commit + push per
subphase; an independent skeptic tries to falsify each subphase's headline.
