# ADR-0012: Packed framing makes layout prediction beat RAW at scale, but not order-0 entropy coding

- **Status:** Accepted — measured, partial positive (Phase 5.7)
- **Date:** 2026-10-05

## Context

ADR-0010 (typed channels rejected) and ADR-0011 (layout prediction rejected on
framing) isolated the same root cause: the reconstruction algebra charged
**per-segment framing** — one tagged op, a tag byte, and a length prefix per
literal run — and that framing exceeded the bytes that structural prediction or
typed channels could save at document scale. ADR-0011's own consequence named the
cure: a **packed segment table** that amortizes framing, so the per-segment cost
is paid once instead of once per span.

Phase 5.7 implemented that cure:

- the `PACK_SEGMENTS` DRA op (opcode `0x08`), bumping the DRA graph to **version
  5** and moving the universe to
  `phase6-prep;exact-bytes;dra-5;opaque+entropy+pdf+channels+offsets`. One op
  carries a compact item table over a single data object:
  `Literal { len }` (a `u32` LEB128 varint), `Mark { slot }`, and
  `Emit { slot, width }`;
- the classic-xref layout candidate rebuilt on that op (**layout-v2**), with
  every literal byte accumulated into one data object;
- **literal coalescing** (5.7.2b), which merges adjacent literal runs into a
  single item. On `many.pdf` (200 objects, 9,881 B) this cut the item table from
  **1,413 to 805** items;
- a larger scale sample (`many.pdf`, hundreds of xref entries) to expose the
  amortization win.

## Outcome

**Keep the mechanism and the partial win; do not adopt layout as the winning
lane.** Packed framing plus coalescing makes structural layout prediction beat
RAW at document scale for the first time, but it is still dominated by a
monolithic order-0 entropy lane.

Measured campaign `2026-10-05-phase5-4521778` over a deterministic 11-file corpus
(verdict PASS; every auto winner round-trips byte-exactly through `cmp` +
`verify`):

- On `many.pdf` (9,881 B) the forced sizes were `PDF_LAYOUT = 10,069` against
  `RAW = 10,215` — **layout now beats RAW by 146 B**, which the per-segment
  Phase-5 lane never achieved — and `BYTE_RANS = 5,181`.
- On the text-heavy scale sample `bigtext.pdf` (65,549 B): `PDF_LAYOUT = 65,929`,
  `RAW = 65,883`, `BYTE_RANS = 38,154`. On `classic.pdf` (329 B):
  `PDF_LAYOUT = 711`, `RAW = 663`.
- Cumulative ladder (serialized bytes): A0 RAW = 81,371; A2 +BYTE_RANS = 48,598;
  A5 +PDF_LAYOUT = 48,598. The **leave-one-out layout delta is 0**, layout wins
  **0 of 8** classic-xref samples, and it is never the auto winner.

## Reasoning

- **The framing is fixed.** Packing the item table removed the per-segment op
  cost that killed the Phase-5 lane; on a many-offset document the amortized plan
  is genuinely cheaper than storing the predicted digits, so prediction beats RAW
  exactly where ADR-0011 predicted it would.
- **But the residual is stored literally.** Once the structure is predicted, the
  remaining bytes — the copied literal runs that are *not* determined by the
  layout — are written into the data object verbatim. That object is ordinary
  document data, and a monolithic order-0 `BYTE_RANS` lane codes it measurably
  better than the packed literal object does. So the packed lane wins against RAW
  (no entropy coding at all) and loses against BYTE_RANS (order-0 entropy coding
  over the whole file).
- **Structural prediction must be composed with entropy coding of the residual.**
  Prediction and entropy coding are not competing explanations of the same bytes;
  they act on different parts. The predicted fields should be regenerated
  procedurally, and the *residual* — what prediction cannot determine — should be
  entropy-coded rather than stored literally. That layering is the paper's model,
  and it is the next lever, not more literal packing.
- **No free lunch.** Packing pays its own byte cost and is charged in the court;
  it wins only where the amortization exceeds that cost. This remains a scoped,
  empirical result, not a general compression claim.

## Consequences

- **Next lever: layout + rANS on the residual.** The candidate to build is a
  packed structural program whose literal/data object is itself an
  entropy-coded channel (structural prediction composed with an order-0 rANS
  residual). This is `PROPOSED` (Phase 6+), motivated directly by this result.
- **`PACK_SEGMENTS` (DRA v5) is a reusable primitive.** It is exact, bounded, and
  non-Turing-complete, and it is the natural container for *any* future packed
  structural plan (not only xref layout), independent of whether layout-v2 is
  adopted.
- **layout-v2 stays implemented and available** as a legal candidate in the
  complete-cost court, but is `RECORDED (beats RAW at scale, rejected vs
  BYTE_RANS)`, not `ADOPTED`; it is never auto-selected on this corpus.
- **The result is preserved as evidence.** Campaign
  `2026-10-05-phase5-4521778` (`ladder.json`, `report.md`, `results.jsonl`)
  remains under `evidence/campaigns/`. The partial positive and its threshold are
  format-design inputs, not deletions; the earlier negative results (ADR-0010,
  ADR-0011) remain valid and are explained by the same framing analysis this ADR
  closes.
