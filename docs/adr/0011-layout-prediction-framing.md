# ADR-0011: Layout prediction is exact but loses to DRA op framing

- **Status:** Accepted — recorded negative result (Phase 5)
- **Date:** 2026-10-05

## Context

The Phase-5 hypothesis was that a PDF's **structurally determined** bytes can be
*regenerated* instead of stored. The clearest instance is the classic
cross-reference layout: every xref entry's 10-digit byte offset is, by
construction, the position at which its target indirect object begins, and the
`startxref` value is the position of the `xref` section. These bytes are not
independent data — they are a function of the materialized layout. If the
decoder can *mark* where each object lands and then *emit* that position, the
digits never have to be persisted, and only the few offsets that genuinely
disagree with the layout need a literal residual.

The mechanism was implemented end to end and kept byte-exact:

- two positional DRA ops, `MARK_OFFSET` (`0x06`) and `EMIT_OFFSET` (`0x07`),
  bumping the DRA graph to **version 4**; 256 bounded slots with slot `255`
  reserved for the `xref` section start;
- the `PDF_LAYOUT` candidate, which marks each indirect object's introducer
  offset and the xref section start, emits each matching 10-digit entry field and
  the `startxref` value from those marks, and falls back to a literal `INLINE`
  for every site whose prediction precondition fails.

## Outcome

**Implement and keep the mechanism; record it as a negative result.** It is exact,
bounded, and it *does* predict — but it is not adopted because it loses on
complete serialized cost.

Measured campaign `2026-10-05-phase5-7193001` over a deterministic 10-file corpus
(verdict PASS; every file round-trips byte-exactly through its auto winner):

- On `classic.pdf` (329 B) the forced size was `PDF_LAYOUT = 798` against
  `RAW = 659`; the lane regenerates **3 of 4** xref entry offsets plus the
  `startxref` and still loses by 139 B.
- On the text-heavy scale sample `bigtext.pdf` (65,549 B) the forced size was
  `PDF_LAYOUT = 66,066` against `RAW = 65,879` and `BYTE_RANS = 38,150`;
  `incremental.pdf` regenerates **5 of 7** entries plus two `startxref` values
  and still loses (`968` vs `776`).
- Cumulative ladder (serialized bytes): A0 RAW = 71,116; A1 +RLE = 71,116;
  A2 +BYTE_RANS = 43,377; A3 +PDF_PHYSICAL = 43,377; A4 +PDF_CHANNELS = 43,377;
  A5 +PDF_LAYOUT = 43,377. The **leave-one-out layout delta is 0**, and layout
  wins **0 of 7** classic-xref files. Auto winners: RAW = 8, `BYTE_RANS` = 2,
  `PDF_PHYSICAL` / `PDF_CHANNELS` / `PDF_LAYOUT` = 0.

## Reasoning

- **The predicted structure is right; the container is too expensive.** Each
  predicted offset is correct and replaces up to ten stored digits with a
  regenerated number, but the reconstruction algebra carries it in **one op per
  literal segment**, and every op has fixed per-segment framing — a tag byte plus
  its operands, and (for the surrounding literals) a length prefix. The program
  must also carry a `MarkOffset` per object and per xref section plus an
  `EmitOffset` per predicted entry. On these documents that framing costs more
  than the ~7 digits saved per predicted offset.
- **The saving is per-site, the framing is per-op.** A predicted 10-digit field
  saves at most 10 bytes of literal payload, while the `EmitOffset` that replaces
  it plus the `MarkOffset` that enables it cost several bytes each; at document
  scale the fixed cost dominates. The absolute byte win shrinks, not grows, as
  offsets get narrower relative to the surrounding framing.
- **No information is created.** Prediction here is a *structural* win only if
  the regenerated representation is genuinely cheaper than the literal it
  replaces. Declaring that a byte is "determined" does not make it free — the
  determination must pay its own encoding cost.

## Consequences

- `PDF_LAYOUT` remains **implemented and available** (with the `rans` feature) and
  stays a legal candidate in the complete-cost court, but it is expected to lose
  and is never auto-selected on this corpus. It is `RECORDED (rejected on cost)`,
  not `ADOPTED`.
- The positional DRA ops (`MARK_OFFSET`/`EMIT_OFFSET`, DRA v4), the layout
  builder, and the forced-candidate ablation (`encode --force pdf-layout`) are
  retained as tested, bounded mechanisms and infrastructure.
- Structural prediction of this kind needs a **cheaper container** before it can
  pay: either a **packed segment table** (amortized framing — one table entry per
  segment instead of a tagged op per segment) or application only to **very large
  or many-offset documents**, where the per-op framing is amortized over far more
  predicted fields. Both are `PROPOSED` for later phases; this negative result is
  the format-design input that motivates them.
- The negative result is **preserved**: campaign `2026-10-05-phase5-7193001`,
  `ladder.json`, and `report.md` under `evidence/campaigns/`. Failed hypotheses
  are evidence, not deletions.
