# ADR-0037: A bounded COS-token phrase grammar is byte-exact, wins on the VOLE ladder, and still loses to generic compressors

- **Status:** Accepted — mixed result (scoped VOLE-ladder positive; recorded
  loss versus generic compressors) (Phase 13.2)
- **Date:** 2026-10-07

## Context

The ledger carried "PDF grammar/templates" as `PROPOSED` since Phase 8: *"must
pay definition cost"*. ADR-0010 already argued the direction for it — contextual
and **ordering** mechanisms rather than more per-kind marginal models — and the
four positional/procedural candidates (ADR-0011/0012/0013/0036) all lost to
per-site framing. What none of them tested is the *recurring COS syntax itself*:
the dictionary stems (`<< /Type /Page /Parent …`), the object framing
(`\nendobj\n`), and the `stream`/`endstream` boilerplate a producer repeats once
per object.

A grammar that stored each recurring COS phrase once and instantiated it by
reference would, in principle, remove that redundancy structurally. The
question the ledger demands be measured is whether such a grammar **pays its own
definition cost**: one template object, one reference per occurrence, and the
literal parameters in between.

## Decision

Implement **`PDF_COS_TEMPLATE`** — a legal candidate in the complete-cost court
(`encode --force pdf-cos-template`) — reusing **only** the existing DRA ops
`EMIT_OBJECT` (`0x01`, instantiate a template) and `INLINE` (`0x02`, its literal
parameters). No new opcode, no DRA-version bump, no universe change, no feature
bit (`src/adapter/pdf/cos_template.rs`).

The mechanism is a small, finite grammar:

1. **Lex** the input into the byte-authoritative COS token cover
   (`src/adapter/pdf/lexer.rs`) and treat each token as a terminal.
2. **Discover** repeated token n-grams (`n ≤ 16`) with a bounded hash table
   (`≤ 500 000` candidates, `≤ 8 000 000` windows, `≤ 200 000` tokens). A phrase
   is a template only if it **contains a structural COS token** — a name,
   dictionary/array delimiter, or an alphabetic keyword — and its estimated net
   saving, `(count−1)·len − count·(EmitObject + InlineSplit) − (len +
   ObjectRecord)`, is positive. Selection is a deterministic total order over
   `(score, byte_len, first_token, token_count)`, bounded to `512` templates;
   nothing depends on hash-map iteration order.
3. **Cover** the document greedily, leftmost-longest, over token boundaries:
   a match emits `Op::EmitObject`; the gaps between matches are carried as
   `Op::Inline` literals; only templates actually instantiated become object
   records.
4. **Verify** end to end (serialize → parse → materialize → byte-compare); an
   inexact program declines rather than emitting.

Every precondition failure (lex failure, no repeating structural phrase, no
instantiation, graph-op bound) declines honestly, and prediction never invents
bytes: a template fires only where its exact bytes recur, and the literal gaps
are copied verbatim.

## Outcome

**Implement it, keep it exact, and record a mixed result: a genuine but scoped
win on the VOLE ladder, and a loss to generic compressors.**

Measured campaign `2026-10-07-phase13-pdf-grammar-dfba2a4` over the **same 28
complete files** as 13.1 (11 Phase-7 producer/synthetic PDFs, 4 generator-family
PDFs, the 12-file `pdf-make-samples` set, and one `pdf-make-large` document),
scored by `tools/phase13-2-court.sh` plus `tools/baselines.sh`:

- **Byte-exact where proposed: 7/7.** 21 files decline (no COS phrase recurs
  enough to amortize the framing). H1 held.
- **vs the current VOLE ladder: win 0 / tie 4 / loss 3.** The four "ties" are
  files where this candidate *is* the new auto winner, so the ladder that
  contains it equals it. H2, read literally as pre-registered, held
  (`lowers_ladder_count == 0`).
- **vs the pre-existing VOLE lanes (the auto winner this subphase replaced):
  win 4 / tie 0 / loss 3.** `PDF_COS_TEMPLATE` becomes the best VOLE lane on
  `cairo-vector.pdf` (34,633 → **19,411** B), `libreoffice-export.pdf`
  (72,850 → **66,895**), `pdftex-doc.pdf` (24,076 → **18,197**) and
  `reportlab-multipage.pdf` (11,203 → **3,754**) — a **34,505 B** improvement in
  the auto-winner total over those files, and the first VOLE lane to beat
  `BYTE_RANS` **and** `PDF_DEFLATE_REPLAY_RANS` on real authoring output.
- **vs the best generic compressor: win 0 / tie 0 / loss 7.** On every file it
  proposes, brotli/xz/zstd are 2–4× smaller (`cairo-vector` 19,411 vs brotli
  16,670; `reportlab-multipage` 3,754 vs brotli 2,012; `libreoffice-export`
  66,895 vs brotli 56,810). H3 held.
- **It also loses to the pre-existing ladder on 3 files it does propose:**
  `large.pdf` (17,190,273 → 33,591,494, streams dominate), `flate.pdf` (36,161 →
  57,927) and `many.pdf` (5,301 → 9,634 — order-0 rANS already models the
  repeated object bodies more cheaply than the phrase framing).

## Reasoning

- **The mechanism is real when boilerplate recurs, and only then.** A template
  is a genuine structural win once an occurrence count amortizes its framing:
  four real producer documents carry enough repeated COS syntax that the phrase
  grammar beats the whole-file order-0 lane and even DEFLATE replay. On those
  files it is not a tie or a technicality — the auto winner is measurably
  smaller.
- **It cannot approach a generic LZ, because it pays framing per occurrence and
  codes its parameters as raw literals.** Each instantiation costs an
  `EmitObject` op (5 B) plus a split `Inline` op (5 B), and the parameter gaps
  are stored with no entropy model at all. A generic compressor shares the same
  phrase with far cheaper back-references *and* entropy-codes the result, so once
  a document is anything other than cleanly repeated syntax (large streams,
  skewed text) the grammar loses badly.
- **The decline rate is the honest bound.** 21/28 outputs carry no COS phrase
  that recurs often enough for the per-occurrence cost to be paid (`<< /Type
  /Page` appears once per page but the object numbers between keys differ, so a
  *byte-identical* phrase is scarce). This is why the mechanism is not a general
  compressor: it needs literal repetition, exactly what generic LZ exploits
  better.
- **No information is created and no gate is weakened.** The candidate reuses
  the existing ops, changes no exactness semantics, adds no wire byte, and is
  priced from its actual serialized bytes in the same court as every other lane.

## Consequences

- **The top-level verdict is unchanged (ADR-0023).** A bounded structural
  grammar beats a whole-file order-0 lane on repetitive syntax, but not a
  purpose-built generic compressor: 0/7 wins versus the best of
  gzip/zstd/xz/brotli. The scoped win is on the *VOLE ladder*, not on the honest
  whole-file comparator.
- **`PDF_COS_TEMPLATE` stays implemented and is a legal candidate.** It is the
  auto winner on four corpus files and a forced ablation lane elsewhere; no lane
  is removed or changed by this ADR.
- **No wire change.** The candidate reuses `EMIT_OBJECT`/`INLINE`; the universe
  string, DRA version, header, and feature bits are untouched and every prior
  descriptor stays decodable.
- **The governor family order grows by one.** `DEPTH_MAX` moved `10 → 11` and
  the encoder-only `dsfb-search` config portfolio now includes the new family so
  `propose_configured(DEFAULT)` still equals `propose_all`. The governor adds no
  wire authority and buys no bytes (ADR-0022).
- **The database of PDF structural negatives is complete for this layer.** With
the typed-channel negative (ADR-0010), the three layout negatives
(ADR-0011/0012/0013) and the `/Length`/revision negative (ADR-0036), six
independent PDF structural candidates now converge: at the tested scale,
proceduralizing plain PDF syntax does not beat a whole-file order-0 rANS lane;
this one *ties or wins* on repetitive syntax but never beats a generic
compressor.
- **The evidence is preserved.** Campaign
  `2026-10-07-phase13-pdf-grammar-dfba2a4` (`receipt.json`, `SUMMARY.md`,
  `commands.txt`, `gates.txt`, `raw/`) remains under `evidence/campaigns/`; the
  pre-registered hypotheses (H1–H4) and their outcome are recorded there.

## References

- ADR-0010: typed lexical channels are rejected by complete cost (Phase 4)
- ADR-0011/0012/0013: layout prediction loses to per-site framing / order-0
- ADR-0017: generic lossless compressors are the whole-file comparator
- ADR-0036: PDF `/Length`/revision proceduralization is exact but loses
- Campaign `2026-10-07-phase13-pdf-grammar-dfba2a4`; `tools/phase13-2-court.sh`,
  `tools/baselines.sh`
- `docs/phases/phase-13-results.md`
