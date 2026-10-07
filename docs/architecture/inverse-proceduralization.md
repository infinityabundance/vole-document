# Inverse proceduralization

VOLE recovers a bounded deterministic reconstruction program from a document's
bytes. This document describes that program (the DRA), the entropy substrate
beneath it, and the measured structural courts. The whole-file comparison that
bounds every candidate is in [Findings](../project/findings.md).

## The Document Reconstruction Algebra (DRA)

The DRA is a literal, bounded, non-Turing-complete reconstruction program with a
**checked coverage certificate**: the outputs of its ops must form a contiguous
cover of `[0, len)` with no gap and no overlap, validated before any allocation
(ADR-0005). There are no loops and no recursion, so it cannot run away.

Literal-subset ops (Phase 1): `EMIT_OBJECT`, `INLINE`, `REPEAT_LAST`. Later ops,
each bumping the DRA graph version and the universe string:

| Op | Ver | Opcode | What it reconstructs |
|---|---|---|---|
| `INTERLEAVE_CHANNELS` | v3 | `0x05` | typed parallel lexical channels |
| `MARK_OFFSET` / `EMIT_OFFSET` | v4 | `0x06`/`0x07` | bounded positional slots (xref offsets) |
| `PACK_SEGMENTS` | v5 | `0x08` | one op + a compact varint item table over one data object |
| `PACKED_CHANNELS` | v6 | `0x09` | a data channel interpreted by a plan channel |
| `DEFLATE_REPLAY` | v8 | `0x0A` | the exact raw DEFLATE bitstream from `(plaintext, corrections)` |

A semantic change bumps the universe string; optional capabilities use ignorable
optional feature bits. The wire format is pre-1.0 and not yet frozen
([Specification](../reference/specification.md), ADR-0004).

## Entropy substrate

The entropy stack is order-0 and optional (`default = ["rans"]`). A channel
carries its model, decoder-entry state, renormalization payload, and counts — a
full *capsule*, never a scalar "seed". "rANS state alone reconstructs arbitrary
data" is false and is not claimed. Wire model v2 serializes to whichever of
sparse or dense is strictly smaller; compact sparse models cut per-channel model
overhead from 7,224 B to 1,981 B (Phase 4).

### Phase 2 — the order-0 floor (measured)

On a deterministic 9-file mixed corpus the cumulative core→full ladder over
serialized `.voldoc` bytes is `sum_source = 590081`, `sum_core = 464474`
(RAW + RLE), `sum_full = 291304` (RAW + RLE + `BYTE_RANS`). The entire delta is
attributed to the two files where `BYTE_RANS` wins. Negative controls hold:
`BYTE_RANS` never wins on random 64 KiB, and model bytes are charged like any
other bytes, so on tiny or high-entropy inputs order-0 rANS loses to RAW/RLE.

- Receipt: `evidence/campaigns/2026-10-05-phase2-f6af30b/`.
- ADR-0006.

## The structural courts (Phases 3–6, 7.0)

Each court compares a candidate against the *complete* serialized cost of a
`BYTE_RANS` lane (Phase 2) unless stated otherwise. Wins over `BYTE_RANS` alone
are per-mechanism ablations, not compression claims (ADR-0017).

### Phase 3 — PDF physical authority

A byte-authoritative PDF scanner: owned lexer, conservative structural span
cover, `/Length` resolution, and an append-only revision map. On a 9-item corpus
(7 valid PDFs plus `malformed.pdf` and `notpdf.bin` controls) coverage is
`all_covered = true` (171 spans, 19 objects, 8 revisions) and `all_exact = true`.
Detection requires a `%PDF-` header **and** an indirect object **and** `%%EOF`;
the extension is never authority. qpdf object-number agreement is 100% (oracle
only).

Result. The literal `PDF_PHYSICAL` candidate loses to RAW on all 9 items — the
expected Phase-3 outcome, since it persists each span as one literal `INLINE` op
with no structural compression.

- Receipt: `evidence/campaigns/2026-10-05-phase3-486aa17/`. ADR-0009.

### Phase 4 — typed lexical channels (recorded negative)

Phase 4 transposes the lexical cover into typed parallel channels (kind id,
length, payload per lexical kind) reconstructed by `INTERLEAVE_CHANNELS`, each
channel order-0 rANS-coded with its own model.

Result. `PDF_CHANNELS` is exact but loses to `BYTE_RANS`: on `bigtext.pdf` 46,432
vs 38,142 B (~8,290 B worse); the ladder leave-one-out delta is 0 and it wins 0
items. The 10-file ladder is `A0 = 71,036`, `A1 = 71,036`, `A2 = A3 = A4 = 43,297`.

Interpretation. Coarse lexical transposition plus per-channel order-0 models
cannot beat a whole-file order-0 model: the kind and length streams plus extra
per-channel model records cost more than the transposition saves. A win would
require *conditioning and ordering* across tokens, not more marginal models.

- Receipt: `evidence/campaigns/2026-10-05-phase4-3840bc4/`. ADR-0010.

### Phase 5 — classic-xref layout prediction (recorded negative)

`MARK_OFFSET`/`EMIT_OFFSET` mark each indirect object's introducer offset and the
xref section start, then regenerate the 10-digit xref entry offsets and
`startxref`.

Result. Prediction is correct and the descriptor exact (`classic.pdf` regenerates
3 of 4 entry offsets plus `startxref`; `incremental.pdf` 5 of 7 plus two
`startxref`), but `PDF_LAYOUT` loses on complete cost: `classic.pdf` 798 vs RAW
659; it wins 0 of 7 classic-xref files, and the leave-one-out delta is 0.

Interpretation. The predicted structure is right; the reconstruction *container*
is too expensive. The DRA pays a fixed per-segment framing (tag + operand bytes)
per mark and per emit, which exceeds the ~7 digits a predicted offset saves.

- Receipt: `evidence/campaigns/2026-10-05-phase5-7193001/`. ADR-0011.

### Phase 5.7 — packed segment framing (partial positive)

`PACK_SEGMENTS` amortizes per-segment framing: one op plus a compact varint item
table (`Literal`/`Mark`/`Emit`) over a single data object. The layout candidate
was rebuilt on it (layout-v2); literal coalescing cut the `many.pdf` (200 objects)
item table from 1,413 to 805 items.

Result. On `many.pdf` layout-v2 is 10,069 vs RAW 10,215 — packed framing makes
structural prediction beat RAW at scale, which the per-segment lane never did.
It still loses to `BYTE_RANS` (5,181) and wins 0 of 8 classic-xref samples, so it
is never the auto winner.

Interpretation. The container cost is fixed, but the residual data object is
stored literally, so any order-0 entropy lane dominates it. The remaining lever
is to entropy-code the residual.

- Receipt: `evidence/campaigns/2026-10-05-phase5-4521778/`. ADR-0012.

### Phase 5.8 — layout + rANS residual (recorded negative)

`PACKED_CHANNELS` reconstructs from a data channel plus a plan channel (the
serialized item table); `PDF_LAYOUT_RANS` codes both as their own order-0 rANS
channels.

Result. Head-to-head vs `BYTE_RANS`: win 0 / lose 8 / decline 3. On `many.pdf`
the layout+rANS breakdown is data 7,877, plan 1,815, models 645, payload 4,775.

Interpretation. Channel 0 codes nearly the whole file against a single global
histogram — the job `BYTE_RANS` does with one channel — while the plan channel
and a second model are pure added metadata the monolithic lane never pays.

- Receipt: `evidence/campaigns/2026-10-05-phase5-8-cf8048d/`. ADR-0013.

### Converging negatives (Phases 4/5/5.7/5.8)

Four independent plain-syntax proceduralizations converge on one scoped negative:
at the tested scale, proceduralizing plain PDF syntax does not beat a whole-file
order-0 rANS lane. A future win requires documents with far more predictable
structure, a plan that costs less than it saves, or a candidate that removes
structure *without* adding a per-site plan.

### Phase 6 — exact DEFLATE replay (first measured positive, scoped)

Phase 6 attacks a different layer: bytes the producer has already entropy-coded.
`DEFLATE_REPLAY` (DRA v8) reconstructs the *original* raw DEFLATE bitstream of a
`/FlateDecode` stream from `(plaintext, corrections)`. The `replay_codec` tag
names an experimental, version-coupled `preflate-0.7.6` layout and fails closed
on an unknown tag; the byte-authoritative scanner owns stream discovery, so
`preflate` never discovers streams. Decode-time output is statically bounded
before the engine runs (ADR-0016) and the operation is isolated under
`catch_unwind` and a per-stream process cap.

Candidates. `PDF_DEFLATE_REPLAY` (raw, deduplicated plaintext) and
`PDF_DEFLATE_REPLAY_RANS` (each unique plaintext is one shared order-0 channel).

Result. On `flate.pdf` (57,513 B) the rANS variant is the auto winner at
36,102 B, a 13,189 B win over `BYTE_RANS` (49,291 B). Head-to-head vs
`BYTE_RANS`: win 1 / lose 0 / decline 11. The raw variant loses (56,736 B).

Interpretation. On this sample 6 streams expose only 3 unique plaintexts, and
only one is shared (across four streams at levels 0/1/6/9). Exactly one
appearance is weakly coded (level 0, stored), so the win needs a plaintext that
is *both shared across streams* and *large/weakly coded*. Neither conjunct alone
wins (four negative controls); the losing region is unique, strongly-compressed
plaintext, where the plaintext is no smaller than the bitstream it replaces. This
is one composed sample on one synthetic fixture.

- Receipt: `evidence/campaigns/2026-10-05-phase6-0d0bb79/`. ADR-0015/0016.

### Phase 7.0 — producer-stratified Flate ratio and complete-cost court

`tools/pdf-corpus.sh` builds a locally generated corpus (Ghostscript 10.00.0 at
five settings, qpdf 11.3.0 in four modes, a hand-written stored-block-zlib base,
plus the synthetic set), and `deflate-stats` measures the exact-replay ratio per
`FlateDecode` stream.

Result. On 24 streams (after a lexer stream-boundary fix) acceptance is 24/24.
The complete-cost court over 23 files gives `PDF_DEFLATE_REPLAY_RANS` vs
`BYTE_RANS` **win 3 / lose 8 / decline 12 — all 3 wins self-authored**:

```text
qpdf-preserve-objectstreams.pdf  BYTE_RANS 112147 -> 56980  (-55167)
hand-base2.pdf                   BYTE_RANS 112011 -> 56885  (-55126)
_synthetic/flate.pdf             BYTE_RANS  49291 -> 36102  (-13189)
```

Interpretation. The "qpdf win" is a `qpdf --object-streams=preserve` copy of two
byte-identical raw streams already authored in `hand-base2.pdf`; 99.93% of it is
inherited. Every genuinely transformed producer output loses or declines. The
enabling condition (a plaintext both shared and large/weakly coded) is not
produced by the tested transformers, and qpdf/Ghostscript are transformers, not
authoring applications. Browser/PDFium, LibreOffice, pdfTeX, and Adobe outputs
remain a recorded gap.

- Receipts: `evidence/campaigns/2026-10-05-phase7-corpus-b-c4eb77e/`,
  `evidence/campaigns/2026-10-05-phase7-court-99dc72e/`. Report
  [phase7-corpus-report.md](../evidence/phase7-corpus-report.md).

### Phase 7.0b — authoring-generator corpus, and a corrected claim

An opt-in `producers` image ran four real authoring generators (ReportLab,
Cairo, LibreOffice, pdfTeX). Complete-cost `PDF_DEFLATE_REPLAY_RANS` vs
`BYTE_RANS`: win 1 / lose 3 / decline 0 — the one "win" on `cairo-vector.pdf`
(−24,137 B). An independent review found the generator repeated one identical
page six times, so Cairo emitted six streams byte-identical in *compressed* bytes
as well as plaintext; generic LZ does ~2× better (gzip 17,382 B, xz-9e
16,852 B). The "first authoring-generator witness" framing is withdrawn: it
witnesses a repeated-bytes region generic LZ captures better, not the
shared-plaintext mechanism.

- Receipt: `evidence/campaigns/2026-10-05-phase7-producers-e071250/`. Review
  [phase7b-skeptic-review.md](../reviews/phase7b-skeptic-review.md).

## Scope

Every corpus here is locally generated and deterministic; none is a population
sample. Whole-file compression is a recorded loss to generic lossless tools
(0/27) — see [Findings](../project/findings.md). Exactness is unchanged
regardless of which candidate wins.
