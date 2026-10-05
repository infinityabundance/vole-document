# ADR-0013: Layout + rANS does not beat a whole-file order-0 rANS lane

- **Status:** Accepted — recorded negative result
- **Date:** 2026-10-05

## Context

ADR-0012 closed the packed-framing phase with a named next lever. Packed framing
(`PACK_SEGMENTS`, DRA v5) plus literal coalescing made structural layout prediction
**beat RAW at scale**, but the layout plan's residual data object was still stored
**literally**, so a monolithic order-0 `BYTE_RANS` lane — which entropy-codes the
whole file with one model — dominated it. ADR-0012's own consequence was explicit:
*entropy-code the residual*. Structural prediction should regenerate the fields it
can determine procedurally, and the bytes it cannot should be rANS-coded rather
than copied verbatim; that composition is the layered model of the prior art.

Phase 5.8 implemented exactly that lever:

- the `PACKED_CHANNELS` DRA op (opcode `0x09`), bumping the DRA graph to **version
  6** and moving the universe to
  `phase5-8;exact-bytes;dra-6;opaque+entropy+pdf+channels+offsets+packed+packed-channels`.
  It reconstructs output from a **data entropy channel** interpreted by a
  serialized item table carried in a **plan entropy channel**, with a declared
  output length validated at evaluation. Both channels reuse the
  `Literal`/`Mark`/`Emit` item codec of `PACK_SEGMENTS`, and the data object must
  be consumed exactly;
- the `PDF_LAYOUT_RANS` candidate, which codes the layout plan's literal data
  object (channel 0) and `encode_items` of its item table (channel 1) each as their
  own order-0 byte-rANS channel, with two model descriptors and a single
  `PACKED_CHANNELS` program op.

## Outcome

**Implement it, keep it exact, and record it as a negative result: do not adopt it
as a winning lane.** The mechanism works and is byte-exact, but on the tested
corpus it loses head-to-head to `BYTE_RANS` on every file for which it is proposed.

Measured campaign `2026-10-05-phase5-8-cf8048d` over a deterministic 11-file corpus
(verdict PASS; every auto winner round-trips byte-exactly through `cmp` + `verify`):

- **Head-to-head vs `BYTE_RANS`: win 0, lose 8, decline 3.** Where it is proposed
  it loses, and the margins are small but consistent: `classic.pdf` **+155** B
  (883 vs 728), `bigtext.pdf` **+167** B (38,341 vs 38,174), `many.pdf` **+713** B
  (5,914 vs 5,201). It is declined by the three files that have nothing to plan
  (the two opaque controls and the cross-reference-stream PDF).
- **The ladder does not move.** A0 RAW = 81,591; A2 +`BYTE_RANS` = 48,818;
  A6 +`PDF_LAYOUT_RANS` = 48,818. The **leave-one-out layout+rANS delta is 0** and
  `PDF_LAYOUT_RANS` is never the auto winner.
- **The overhead is the plan channel.** On `many.pdf` the layout+rANS size breaks
  down as data 7,877 / plan 1,815 / models 645 / payload 4,775, against
  `BYTE_RANS` 5,201. The plan channel alone is on the order of 1.8 KiB of metadata
  `BYTE_RANS` never pays.

## Reasoning

- **Channel 0 codes nearly the whole file.** After layout prediction, the literal
  data object is almost the entire document; channel 0 entropy-codes it against a
  single global histogram — precisely the job `BYTE_RANS` performs with one
  channel. Prediction only removes a small, bounded set of bytes (the xref entry
  offsets and `startxref`), so channel 0 is not meaningfully smaller than the
  `BYTE_RANS` payload it is compared against.
- **The plan channel and the second model are pure added metadata.** `BYTE_RANS`
  pays one model; layout+rANS pays two models plus a serialized item table that
  must travel somewhere. Those bytes are charged in the complete-cost court, as
  they must be, and they are not offset by the prediction.
- **The saved bytes are smaller than the plan costs at this scale.** Prediction
  saves on the order of **~7 bytes per xref entry after framing** (a 10-digit
  offset minus the tagged op / item that reproduces it). On documents with tens or
  hundreds of entries that is tens to hundreds of bytes — genuinely less than the
  cost of carrying the plan and its model, even after packed framing and
  coalescing amortize the per-item cost.
- **Prediction and entropy coding are not additive here.** They act on the same
  bytes: once channel 0 has entropy-coded the residual, the residual is small, and
  the structural saving is dwarfed by the plan's fixed overhead. The two levers do
  not compose into a win on this corpus.

## Consequences

- **The baseline to beat on this corpus is a whole-file order-0 rANS lane.** Any
  future PDF structural candidate must be priced against `BYTE_RANS`, not against
  RAW. Beating RAW at scale (ADR-0012) is necessary but not sufficient.
- **A per-site plan must pay for itself.** `PDF_LAYOUT_RANS` shows that a plan
  which names every marked position and every predicted field costs more metadata
  than the fields it predicts at this scale. A future win needs one of:
  1. **documents with far more predictable structure**, where the saved bytes are
     large relative to a roughly fixed plan cost;
  2. **a plan that costs less than it saves** — e.g. a denser encoding of the plan
     rather than one item per site; or
  3. **a candidate that removes structure *without* adding a per-site plan** — a
     canonical or parametric layout that reconstitutes correct offsets from a
     small set of parameters, rather than an explicit item table.
- **Preserve the evidence and the converging negatives.** Campaign
  `2026-10-05-phase5-8-cf8048d` (`ladder.json`, `report.md`, `results.jsonl`)
  remains under `evidence/campaigns/`. Four independent mechanisms now point at the
  same scoped conclusion: coarse typed lexical channels lose to a whole-file
  order-0 model (Phase 4, ADR-0010); framed structural layout prediction loses to
  RAW and `BYTE_RANS` (Phase 5, ADR-0011); packed framing lets prediction beat RAW
  at scale but not order-0 entropy coding (Phase 5.7, ADR-0012); and entropy-coding
  the residual as separate channels still loses to `BYTE_RANS` because the plan
  itself is added metadata (Phase 5.8, this ADR). At the tested scale, PDF
  structural proceduralization does not beat a whole-file order-0 rANS lane.
- **`PACKED_CHANNELS` (DRA v6) is a reusable primitive.** It is exact, bounded, and
  non-Turing-complete, and it is the natural container for *any* future plan that
  must travel as entropy-coded channels, independent of whether
  `PDF_LAYOUT_RANS` is adopted.
- **`PDF_LAYOUT_RANS` stays implemented and available** as a legal candidate in the
  complete-cost court, but is `RECORDED (rejected vs BYTE_RANS)`, not `ADOPTED`;
  it is never auto-selected on this corpus.

## References

- ADR-0010: typed lexical channels are rejected by complete cost (Phase 4)
- ADR-0011: layout prediction is exact but loses to DRA op framing (Phase 5)
- ADR-0012: packed framing threshold; entropy-code the residual (Phase 5.7)
- Campaign `2026-10-05-phase5-8-cf8048d`
