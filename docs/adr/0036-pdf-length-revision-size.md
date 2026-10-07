# ADR-0036: PDF `/Length` / revision proceduralization is exact but loses on complete cost

- **Status:** Accepted — recorded negative result (Phase 13.1)
- **Date:** 2026-10-07

## Context

Phase 3's byte-authoritative physical scanner already *resolves* a PDF's
incremental revision chain and each stream's `/Length`; Phase 11 persists them as
queryable **observation** nodes (`PdfRevision`). Neither prices them. The ledger
carried the last unmeasured PDF structural idea as `PROPOSED`: *"`/Length`/
revision proceduralization as a **size** mechanism"* — a hypothesis distinct from
the xref-offset prediction ADR-0011/0012/0013 already rejected, because `/Length`
values and the `/Prev`/`startxref`/revision chain are a large, repetitive class of
structurally determined bytes.

The Phase-5 positional algebra the project already owns — `MARK_OFFSET` /
`EMIT_OFFSET` and the packed `PACK_SEGMENTS` item table (DRA v4/v5) — regenerates
a field by **emitting the decimal of a previously marked output position**. That
algebra can only reproduce an *absolute offset*: a `startxref`, an xref entry
offset, a trailer `/Prev`. A `/Length` value is a **relative** quantity (payload
byte count), which the deliberately position-only algebra has no instruction to
compute. Phase 13.1 implemented a bounded candidate anyway and measured it
honestly, because "the determination must pay its own encoding cost" (ADR-0011)
can only be answered by the complete-cost court.

## Decision

Implement **`PDF_LENGTH_REVISION`** — a legal candidate in the complete-cost court
(`encode --force pdf-length-revision`) — reusing only the existing positional ops
(no new opcode, no DRA-version bump, no universe change, no feature bit). It
builds one `PACK_SEGMENTS` program over one literal data object and regenerates
three field classes:

1. **xref entry offsets** — the 10-digit byte offset of an in-use entry whose
   value equals the position at which its target object's introducer was marked.
2. **`startxref` and trailer `/Prev`** — absolute offsets of a revision's `xref`
   section anchor (one mark slot per revision), i.e. the revision-chain redundancy.
3. **`/Length` values** — each directly-sized stream's payload count, regenerated
   by marking the *output offset equal to the length* and emitting that mark.
   This is the only way a relative quantity is expressible in the positional
   algebra: `EMIT_OFFSET` renders a marked absolute position as a fixed-width
   decimal, so a `/Length` `L` is reproduced exactly when the reconstruction
   passes through offset `L` before the field and a zero-width mark records it.

Every precondition failure — a non-classic xref (an xref **stream**), a
non-contiguous cover, more than 254 indirect objects, a field whose regenerated
digits would not match the source, a slot budget exhausted by the distinct-length
pool — falls back to a literal or declines the candidate. A field is emitted only
when the emitted decimal reproduces the source digits exactly, and the finished
program is round-tripped (serialize → parse → materialize → byte-compare) before
it is returned, so the candidate is admitted to the court only if byte-exact.

## Outcome

**Implement it, keep it exact, and record it as a negative result: 0 wins.**

Measured campaign `2026-10-07-phase13-pdf-length-revision-12fc84e` over **28
complete files** (11 Phase-7 producer/synthetic PDFs, 4 generator-family PDFs,
the 12-file `pdf-make-samples` set, and one `pdf-make-large` document), scored by
`tools/phase13-court.sh` — the same complete-cost court, plus generic compressors
via `tools/baselines.sh`:

- **Byte-exact where proposed: 21/21.** 7 files decline (4 xref-stream PDFs, a
  malformed PDF, a non-PDF, and the >254-object large PDF). H1 held.
- **vs the current VOLE ladder: win 0 / tie 0 / loss 21.** The candidate is never
  the smallest VOLE lane and never lowers the ladder: `ladder_total_excl ==
  best_vole_total_incl == 17,807,789` bytes, `lowers_ladder_count == 0`.
- **vs the best generic compressor: win 0 / tie 0 / loss 21.** On
  `cairo-vector.pdf` the candidate is 59,016 B against brotli's 16,670 B (3.54×);
  on `classic.pdf` 905 B against RAW's 783 B and brotli's 172 B; on
  `bigtext.pdf` 66,123 B against xz's 824 B (80×). H2 and H3 held.
- The candidate's own total on the 21 proposed files is **920,497 B** — it is
  *larger than the source* on essentially every file (e.g. 329 → 905 on
  `classic.pdf`), because the framing it adds exceeds the digits it removes.

## Reasoning

- **The saving is per-site; the framing is per-field.** A regenerated `/Length`
  field saves its decimal width — typically 1–7 bytes — but costs a
  `PackItem::Mark` (tag + slot) plus a `PackItem::Emit` (tag + slot + width) plus
  the literal split it forces, and, for the offset-equality trick, a slot per
  *distinct* length. The fixed per-field cost exceeds the saved digits at every
  tested scale. This is exactly the failure mode ADR-0011 named and ADR-0012/0013
  re-measured for xref offsets.
- **`/Length` is not a structural prediction in the intended sense.** Marking the
  position whose *value* happens to equal the length is a re-encoding of an
  integer as an offset, not a derivation from adjacent stream geometry; the DRA
  has no difference instruction, so a genuine `/Length = end − start` is not
  expressible without a wire change this ADR deliberately does not take. The
  measurement is retained precisely because it shows even this expressive
  workaround does not pay.
- **The revision chain is small.** `/Prev` and `startxref` occur once per
  revision; on the corpus (mostly single-revision PDFs) there is almost nothing
  to regenerate, and predicting it cannot offset the cost of the `/Length` and
  xref framing carried alongside it.
- **No information is created and no gate is weakened.** The candidate reuses the
  existing ops, changes no exactness semantics, adds no wire byte, and is priced
  from its actual serialized bytes in the same court as every other lane.

## Consequences

- **`PDF_LENGTH_REVISION` stays implemented and available** as a forced lane for
  honest per-mechanism ablation; it is `RECORDED (rejected on cost)`, never the
  auto winner, and no lane is removed or changed by this ADR.
- **The last unmeasured PDF structural idea is now measured.** The ledger row for
  "PDF `/Length`/revision proceduralization (as a size mechanism)" moves from
  `PROPOSED` to `MEASURED (recorded negative)`. With ADR-0011/0012/0013, five
  independent PDF structural candidates now converge on the same scoped result:
  at the tested scale, proceduralizing plain PDF syntax does not beat a
  whole-file order-0 rANS lane, and never approaches a generic compressor.
- **`PDF_LAYOUT`'s 254-object bound is inherited.** Like the classic-xref layout
  lane, the candidate declines on documents with more than 254 indirect objects
  (the >254-object `pdf-make-large` file declines); this is a shared slot-budget
  bound, not a new limitation.
- **The bounded `/Length` limit is recorded, not hidden.** A future `/Length =
  end − start` mechanism would need a difference op (a new opcode and DRA/universe
  bump); this ADR records that the expressive workaround already loses, so such a
  change is not motivated by the current corpus.
- **The evidence is preserved.** Campaign
  `2026-10-07-phase13-pdf-length-revision-12fc84e` (`receipt.json`, `SUMMARY.md`,
  `commands.txt`, `raw/`) remains under `evidence/campaigns/`; the pre-registered
  hypotheses (H1–H4) and their outcome are recorded there.

## References

- ADR-0011: layout prediction is exact but loses to DRA op framing (Phase 5)
- ADR-0012: packed framing makes layout beat RAW at scale, but not order-0
  entropy coding (Phase 5.7)
- ADR-0013: layout + rANS does not beat a whole-file order-0 rANS lane (Phase 5.8)
- ADR-0017: generic lossless compressors are the whole-file comparator
- Campaign `2026-10-07-phase13-pdf-length-revision-12fc84e`;
  `tools/phase13-court.sh`, `tools/baselines.sh`
- `docs/phases/phase-13-results.md`
