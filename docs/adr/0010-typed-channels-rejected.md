# ADR-0010: Typed lexical channels are rejected by complete cost

- **Status:** Accepted — recorded negative result (Phase 4)
- **Date:** 2026-10-05

## Context

The Phase-4 hypothesis was that splitting an exact PDF byte stream into **typed
lexical channels** — one kind id per token, one length per token, and one payload
stream per lexical kind — and entropy-coding each channel with its own order-0
model would beat a single order-0 model over the whole file. The intuition: a
kind-`Regular` stream, a kind-`Whitespace` stream, and a kind-`LiteralString`
stream each have a much lower per-symbol entropy than the interleaved mixture, so
per-channel models should pay for themselves.

The mechanism was implemented end to end and kept byte-exact:

- a deterministic, exactly-reversible transposition (`split`/`join`,
  `KIND_COUNT = 12`) over the byte-authoritative Phase-3 lexical cover;
- the bounded `INTERLEAVE_CHANNELS` DRA op (DRA v3, opcode `0x05`) to reconstruct
  from the kind/length/payload channels;
- the `PDF_CHANNELS` candidate, one order-0 byte-rANS model per channel;
- compact **model wire v2** (sparse/dense, smaller chosen) to cut the per-channel
  model records that the split adds.

## Decision / Outcome

**Implement and keep the mechanism; reject it as an adopted winning lane.** It is
exact and bounded, but it is not adopted because it loses on complete serialized
cost.

Measured campaign `2026-10-05-phase4-3840bc4` over a deterministic 10-file corpus
(verdict PASS; every file round-trips byte-exactly through its auto winner):

- On the text-heavy scale sample `bigtext.pdf` (65,549 B) the forced sizes were
  `RAW = 65,871`, `BYTE_RANS = 38,142`, `PDF_CHANNELS = 46,432`. Typed channels
  beat RAW by ~21.6% but **lose to `BYTE_RANS` by ~8,290 B**.
- Cumulative ladder (serialized bytes): A0 RAW = 71,036; A1 +RLE = 71,036;
  A2 +BYTE_RANS = 43,297; A3 +PDF_PHYSICAL = 43,297; A4 +PDF_CHANNELS = 43,297.
  The **leave-one-out channel delta is 0** — adding the channel lane changes
  nothing at the corpus level because it never wins an item.
- Auto winners: RAW = 8, `BYTE_RANS = 2`, `PDF_PHYSICAL = 0`, `PDF_CHANNELS = 0`.

The compact v2 model did its job — per-channel model overhead fell from 7,224 B
to 1,981 B — and it still was not enough to close the gap.

## Reasoning

- **Marginal vs. contextual modelling.** Per-channel order-0 models are only
  *marginal* byte distributions. A monolithic order-0 model over the whole file
  already captures the union of the same marginals; re-partitioning the data
  cannot lower the sum of the parts' self-information, because splitting does not
  create information and the per-channel models are still memoryless.
- **Overheads the split adds.** The kind stream, the length stream (4 bytes per
  token), one model record per channel (up to 13), and the `INTERLEAVE_CHANNELS`
  program all cost bytes that the monolithic channel does not pay. On a
  ~65 KB sample these overheads exceed the (already impossible) entropy savings.
- **What would actually pay is conditioning, not partitioning.** To recover a
  win, a symbol's code must be conditioned on *context* — neighbouring tokens,
  the kind that precedes/follows it, or structural state — and the token
  *ordering* must itself be modelled. That is a strictly stronger mechanism than
  a transposition, and it is Phase 5+ work, not Phase 4.

## Consequences

- `PDF_CHANNELS` remains **implemented and available** (with the `rans` feature)
  and stays a legal candidate in the complete-cost court, but it is expected to
  lose and is never auto-selected on this corpus. It is `RECORDED (rejected)`,
  not `ADOPTED`.
- `split`/`join`, `INTERLEAVE_CHANNELS` (DRA v3), model wire v2, and the
  forced-candidate ablation (`encode --force KIND`) are retained as tested,
  bounded mechanisms and infrastructure.
- Phase 5 (xref/`startxref`/`/Length`/revision proceduralization) and Phase 8
  (grammar/templates) must pursue **contextual and ordering** mechanisms rather
  than more per-kind marginal models; this negative result is the evidence that
  motivates that direction.
- The negative result is **preserved**: campaign `2026-10-05-phase4-3840bc4`,
  `ladder.json`, and `report.md` under `evidence/campaigns/`. Failed hypotheses
  are evidence, not deletions.
