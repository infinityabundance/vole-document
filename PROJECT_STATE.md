# Project state and mechanism ledger

This is the single authoritative status table. "Implemented" and "proven" are
**not** interchangeable: a mechanism is `ADOPTED` only after its predeclared gate
passes and a sealed campaign exists. Failed hypotheses stay in the history — they
are evidence.

## Status vocabulary

`PROPOSED` → `PROTOTYPED` → `IMPLEMENTED` → `MEASURED` → `ADOPTED`
(or `RECORDED` / `REJECTED` / `STOPPED`).

## Ledger

| Mechanism | Phase | Status | Notes |
|---|---|---|---|
| Byte-exact invariant (`materialize(D) == X`) | 1 | ADOPTED | the only normative profile |
| Length-delimited record container | 1 | ADOPTED | framing CRC-32C; unknown-mandatory fails closed |
| Typed errors + stable exit codes | 1 | ADOPTED | `src/error.rs` |
| Centralized resource limits | 1 | ADOPTED | `Limits::{DEFAULT,STRICT}` |
| SHA-256 whole-source identity | 1 | ADOPTED | durable receipt |
| Literal DRA (`EMIT_OBJECT`, `INLINE`, `REPEAT_LAST`) | 1 | ADOPTED | bounded, non-Turing-complete |
| Coverage certificate (checked invariant) | 1 | ADOPTED | rejects gaps/overlaps before allocation |
| RAW exact opaque adapter | 1 | ADOPTED | correctness floor for every file type |
| Complete-cost court + decode-before-commit | 1 | ADOPTED | winner priced from serialized bytes only |
| Native rANS floor (order-0 / typed byte channels) | 2 | ADOPTED | campaign `2026-10-05-phase2-f6af30b`; substrate beneath structure, not the model |
| Entropy-seed **capsule** (full decoder-entry state) | 2 | ADOPTED | ADR-0006; never a scalar "magic seed" |
| RLE candidate (`REPEAT_LAST` run-length) | 2 | ADOPTED | campaign `2026-10-05-phase2-f6af30b`; wins long runs |
| BYTE_RANS candidate (order-0 byte channel) | 2 | ADOPTED | campaign `2026-10-05-phase2-f6af30b`; wins skewed + English-like text, model bytes charged |
| PDF lexical span cover (Phase 3.1) | 3 | ADOPTED | campaign `2026-10-05-phase3-486aa17`; hostile-safe contiguous cover of `[0,len)` |
| PDF byte-authoritative physical scanner (Phase 3.2–3.3) | 3 | ADOPTED | campaign `2026-10-05-phase3-486aa17`; structural spans, `/Length` resolution, CRLF/LF handling |
| PDF incremental revision map (Phase 3.4) | 3 | ADOPTED | campaign `2026-10-05-phase3-486aa17`; append-only revisions, `/Prev` chain, `/Size` never decreases |
| PDF object roles (xref-stream / object-stream detection) | 3 | ADOPTED | campaign `2026-10-05-phase3-486aa17`; conservative `/Type` classification |
| qpdf differential oracle court | 3 | ADOPTED | campaign `2026-10-05-phase3-486aa17`; object-number agreement 100%; oracle, never byte authority |
| PDF lexical channel transposition (`split`/`join`) | 4 | IMPLEMENTED | exact, reversible transposition of the Phase-3.1 cover; `KIND_COUNT = 12`; `tests/pdf_channels.rs` |
| `INTERLEAVE_CHANNELS` DRA op (DRA v3) | 4 | IMPLEMENTED | opcode `0x05`; bounded kind/length/payload replay with checked alignment |
| Compact entropy model wire v2 (sparse\|dense, smaller chosen) | 4 | ADOPTED | per-channel model overhead 7,224 → 1,981 B; legacy v1 dense still decodable |
| Forced-candidate ablation (`encode --force KIND`) | 4 | ADOPTED | one-element complete-cost court; `tools/phase4-court.sh`; forcing never bypasses exactness |
| PDF typed channels (`PDF_CHANNELS`) | 4 | RECORDED (rejected) | campaign `2026-10-05-phase4-3840bc4`; exact but loses to `BYTE_RANS` on complete cost (bigtext 46,432 vs 38,142; ladder delta 0) |
| Positional DRA ops (`MARK_OFFSET` / `EMIT_OFFSET`, DRA v4) | 5 | IMPLEMENTED | opcodes `0x06` / `0x07`; `MAX_OFFSET_SLOTS = 256`, slot 255 reserved for the xref section start; universe → phase-5 |
| PDF classic-xref layout (`PDF_LAYOUT`) | 5 | RECORDED (rejected on cost) | campaign `2026-10-05-phase5-7193001`; byte-exact and predicts xref offsets/`startxref`, but loses to RAW/`BYTE_RANS` on DRA op framing cost (ADR-0011) |
| Packed segment framing (`PACK_SEGMENTS`, DRA v5) | 5.7 | IMPLEMENTED | opcode `0x08`; one op + a compact varint item table over a single data object amortizes per-segment framing; universe → `phase6-prep` |
| PDF layout on packed framing (layout-v2, coalesced) | 5.7 | RECORDED (beats RAW at scale, rejected vs `BYTE_RANS`) | campaign `2026-10-05-phase5-4521778`; packed+coalesced layout beats RAW on `many.pdf` (10,069 vs 10,215), but the residual data object is stored literally so it loses to `BYTE_RANS` (5,181) and is never the auto winner (ADR-0012) |
| `PACKED_CHANNELS` DRA op (DRA v6) | 5.8 | IMPLEMENTED | opcode `0x09`; reconstructs from a data channel + a plan channel (serialized item table) with a declared output length validated at eval; universe → `phase5-8` |
| PDF layout + rANS (`PDF_LAYOUT_RANS`) | 5.8 | RECORDED (rejected vs `BYTE_RANS`) | campaign `2026-10-05-phase5-8-cf8048d`; byte-exact, but head-to-head wins 0 / loses 8 / declines 3, and the A6 rung adds a plan channel + a second model that `BYTE_RANS` never pays (ADR-0013) |
| PDF `/Length`/revision proceduralization | 6+ | PROPOSED | structural compression beyond xref offsets is not yet measured (stream replay is now measured — next rows) |
| Lexer stream opacity (`stream`+EOL is an opaque span) | 6 | ADOPTED | campaign `2026-10-05-phase6-0d0bb79`; stream-data bytes are a byte-authoritative span, so `/FlateDecode` stream spans are exact and `preflate` never discovers streams; 12/12 corpus files still round-trip |
| `DEFLATE_REPLAY` DRA op (DRA v8) | 6 | IMPLEMENTED | opcode `0x0A`; explicit `replay_codec` tag (`REPLAY_DEFLATE_PREFLATE_0_7_6`, an experimental version-coupled `preflate` layout) that fails closed on an unknown id; emits the exact raw DEFLATE bitstream from `(plaintext, corrections)` with a declared output length statically rejected above the VOLE replay-profile admission limit `min(max_output_bytes, max_replay_bytes, 2*P+1024)` before the engine runs (ADR-0016; a policy bound, not an RFC 1951 maximum) and validated at eval; plaintext/corrections bounded by `max_record_len`; `catch_unwind`-isolated; mandatory feature bit (opt-in `deflate-replay` cargo feature); universe → `phase6;…;dra-8;…+deflate-replay-preflate-0.7.6-experimental` |
| Exact DEFLATE replay, raw plaintext (`PDF_DEFLATE_REPLAY`) | 6 | RECORDED (rejected vs `BYTE_RANS`) | campaign `2026-10-05-phase6-0d0bb79`; byte-exact, but on `flate.pdf` 56,736 vs `BYTE_RANS` 49,291 (the plaintext is nearly as large as the bitstream it replaces) |
| Exact DEFLATE replay, shared rANS plaintext (`PDF_DEFLATE_REPLAY_RANS`) | 6 | ADOPTED | campaign `2026-10-05-phase6-0d0bb79`; `flate.pdf` 36,102 vs `BYTE_RANS` 49,291 (**−13,189 B**); 3 plaintext channels (one shared) for 6 streams; leave-one-out delta −13,189; **first measured positive for a PDF structural candidate** (ADR-0015) |
| Producer-stratified Flate correction-ratio harness (`deflate-stats`) | 7.0 | MEASURED | amendment campaign `2026-10-05-phase7-corpus-b-c4eb77e` (supersedes `2026-10-05-phase7-corpus-f1f8d26`, not rewritten); after the lexer stream-boundary fix, **24 Flate streams, 24 replayed / 0 declined (acceptance 1.000)** from Ghostscript 10.00.0, qpdf 11.3.0, a hand-written stored-block-zlib base, and the Phase-3 synthetic set; the 6 former `not_zlib` declines are gone and the stream census rose 17 → 24 (the old over-read had swallowed whole stream objects, e.g. `qpdf-preserve-objectstreams.pdf` reported zero); the Phase-6 win region now reproduces on qpdf transformer output (`qpdf-preserve-objectstreams.pdf` deduped rANS 55,531 vs naive 111,062) as well as the hand-written shared-plaintext fixtures (`hand-base2.pdf` 55,531 vs 111,062; `flate.pdf` 34,051 vs 89,437); **no new candidate** |
| Nested PDF content proceduralization | 7 | PROPOSED | the clearest embodiment of the thesis |
| PDF grammar/templates | 8 | PROPOSED | must pay definition cost |
| EntropyFS store-backed form | 9 | PROPOSED | optional substrate; `engine::Engine` |
| DSFB encoder-only search governance | 10 | PROPOSED | **zero** decode authority |
| Partial materialization | 11 | PROPOSED | checkpoints cost bytes |
| Fuzzing / property courts | 2 | IMPLEMENTED | `tests/property.rs`, `tests/goldens.rs`, `tools/soak-fuzz.sh`; targets listed in `CONFORMANCE.md` |
| Cross-document proceduralization | 12+ | PROPOSED | — |
| Non-PDF adapters (DOCX/ODT/EPUB/…) | later | PROPOSED | adapters over the same core |

The PDF **physical authority** (lexer span cover, structural scanner, revision
map, and object roles) is `ADOPTED` as of Phase 3 (campaign
`2026-10-05-phase3-486aa17`). The typed lexical-channel lane
(`split`/`join`, `INTERLEAVE_CHANNELS`, `PDF_CHANNELS`) is `IMPLEMENTED` and
measured as of Phase 4, but `PDF_CHANNELS` is **`RECORDED (rejected)`**: it is
exact and available, yet loses to `BYTE_RANS` on complete cost. Phase 5 adds the
positional DRA ops (`MARK_OFFSET` / `EMIT_OFFSET`, DRA v4, `IMPLEMENTED`) and the
classic-xref layout lane (`PDF_LAYOUT`), which is **`RECORDED (rejected on
cost)`**: it is byte-exact and genuinely predicts xref offsets and `startxref`,
but the per-segment DRA op framing costs more than the digits it saves, so it
loses to RAW/`BYTE_RANS` (campaign `2026-10-05-phase5-7193001`, ADR-0011). Phase
5.7 then amortizes that framing with the packed `PACK_SEGMENTS` op (DRA v5) and
literal coalescing: packed layout prediction now **beats RAW at scale**
(`many.pdf` 10,069 vs 10,215) but still loses to `BYTE_RANS`, because the residual
data object is stored literally (campaign `2026-10-05-phase5-4521778`, ADR-0012).
PDF structural compression beyond xref offsets — `/Length`/revision
proceduralization — and every cross-document mechanism remain `PROPOSED`
(Phases 7+). Phase 2 measured only the order-0 typed byte entropy floor over an
opaque mixed corpus; Phase 4 showed that coarse lexical transposition plus
per-channel order-0 models does not beat a monolithic order-0 channel; Phase 5
showed that correct structural prediction does not pay while each predicted field
still needs its own framed DRA op; Phase 5.7 showed that packing that framing
makes prediction beat RAW at scale; and Phase 5.8 composed prediction with
entropy coding of the residual (`PACKED_CHANNELS`, DRA v6) yet still does not beat
a monolithic order-0 channel, because the plan channel and a second model are
added metadata the monolithic lane never pays. Those four results converge on a
scoped negative: at the tested scale, proceduralizing *plain* PDF syntax does not
beat a whole-file order-0 rANS lane. Phase 6 attacks a **different layer** — bytes
the producer has already entropy-coded — and records the first positive: exact
DEFLATE replay of *shared* plaintext that also has a *large/weakly-coded*
appearance beats `BYTE_RANS` (campaign `2026-10-05-phase6-0d0bb79`, ADR-0015).

### Phase 6 scope and the first measured positive

- **Mechanism.** Phase 6 adds the `DEFLATE_REPLAY` DRA op (opcode `0x0A`), bumping
the DRA graph to **version 8** and moving the universe to
`phase6;exact-bytes;dra-8;opaque+entropy+pdf+channels+offsets+packed+packed-channels+deflate-replay-preflate-0.7.6-experimental`.
The op carries an explicit `replay_codec` tag (`REPLAY_DEFLATE_PREFLATE_0_7_6`)
that names the correction representation as an **experimental** version-coupled
`preflate` layout (not frozen v1); an unknown codec fails closed as
`UnsupportedFeature`. It emits exactly
`recreate_whole_deflate_stream(plaintext, corrections)` — the
raw DEFLATE bytes (RFC 1951, no zlib wrapper) — with a `declared_output_len`
statically rejected above the VOLE replay-profile admission limit (a policy
bound: RFC 1951 permits unbounded empty non-final blocks, so it gives no finite
`f(decompressed_size)` bound) **before** the
engine runs (ADR-0016) and validated at evaluation; the plaintext and corrections
inputs are bounded by `max_record_len`. Reconstruction is isolated with
`catch_unwind` so hostile corrections fail closed. A mandatory
`FEATURE_DEFLATE_REPLAY` bit is
declared whenever the op is present. A lexer fix makes `stream` + EOL payloads
**opaque spans**, so the physical scanner owns stream-data bytes and a lone
`/FlateDecode` is classified from the object dictionary; `preflate` never
discovers streams.
- **Candidates.** `PDF_DEFLATE_REPLAY` replaces each eligible stream span with
`INLINE(header) · DEFLATE_REPLAY · INLINE(Adler-32)`, storing plaintexts and
corrections as content-deduplicated objects. `PDF_DEFLATE_REPLAY_RANS` codes each
**unique** plaintext as its own order-0 byte-rANS channel, shared by every stream
that produces it, so shared plaintext is stored once and decoded once.
- **Complete-cost verdict — first positive.** On `flate.pdf` (57,513 B) the rANS
lane is the auto winner at **36,102 B**, a **13,189 B win** over `BYTE_RANS`
(49,291 B) and a 21,806 B win over RAW (57,908 B). The raw-plaintext variant
loses (56,736 B, 7,445 B above `BYTE_RANS`). Ladder A0 RAW 139,950 → A2
+`BYTE_RANS` 98,560 → A6 +`PDF_LAYOUT_RANS` 98,560 → A7 +`PDF_DEFLATE_REPLAY`
98,560 → A8 +`PDF_DEFLATE_REPLAY_RANS` **85,371**; leave-one-out replay-rANS
delta **−13,189**. Head-to-head vs `BYTE_RANS`: **win 1, lose 0, decline 11**
(the other files have no lone FlateDecode stream). Auto winners: RAW = 8,
`BYTE_RANS` = 3, `PDF_DEFLATE_REPLAY_RANS` = 1; every winner byte-exact.
- **Why it wins.** The sample exposes 6 streams but only **3 unique plaintexts**;
only `p1` is shared (across four streams at levels 0/1/6/9), while `p2` and `p3`
are unique (`streams=6 replayed=6 channels=3 objects=6`). Of `p1`'s four
appearances exactly one is weakly coded — level 0 is stored (~verbatim,
31,998 B); level 1 is only ~19% of the plaintext (6,197 B), and levels 6 and 9
are strong — so the rANS lane's win rests on the shared plaintext *also* having a
large/weakly-coded appearance, not on four weak bitstreams. The rANS lane codes
`p1` once and every stream that reproduces it references the same channel, so the
large stored appearance is re-expressed by order-0 rANS over the shared plaintext
rather than carried as a bitstream; the winner's cost is dominated by the single
32,723 B entropy payload against `BYTE_RANS`'s 49,291 B. Section scoped honestly:
this is **one composed sample**. The winning region is a plaintext that is
**shared across streams** *and* has a **large/weakly-coded** appearance; neither
alone wins, and the losing region is **unique, strongly-compressed** plaintext,
where the plaintext is no smaller than the bitstream it replaces (the
raw-plaintext result is exactly that regime). Recorded as a measured positive
(ADR-0015).
- **What this does not license.** The four plain-syntax negatives stand; Phase 6
does not show that PDF structural proceduralization generally beats `BYTE_RANS`,
only that exact replay of already-entropy-coded streams whose plaintext is shared
*and* has a large/weakly-coded appearance does on this one case. The natural
successors are nested plaintext proceduralization (Phase 7) and cross-document
plaintext sharing (Phase 9).

### Phase 5.7 scope and the packed-framing threshold

- **Mechanism.** Phase 5.7 adds the `PACK_SEGMENTS` DRA op (opcode `0x08`),
  bumping the DRA graph to **version 5** and moving the universe to
  `phase6-prep;exact-bytes;dra-5;opaque+entropy+pdf+channels+offsets`. One op
  carries a compact item table — `Literal` (varint length), `Mark`, and `Emit` —
  over a single data object, so per-segment framing is paid once instead of once
  per span. The op is bounded and non-Turing-complete like the rest of the DRA,
  and the data object must be consumed exactly.
- **Candidate.** The classic-xref layout candidate was rebuilt on the packed op
  (**layout-v2**); literal coalescing merges adjacent literal runs, cutting the
  `many.pdf` (200 objects, 9,881 B) item table from **1,413 to 805** items.
- **Complete-cost verdict — partial positive.** Packed framing makes structural
  prediction **beat RAW at scale**: on `many.pdf` layout-v2 is 10,069 vs RAW
  10,215 (a 146 B win), which the per-segment Phase-5 lanes never achieved. The
  candidate is nevertheless **not adopted**: it still loses to `BYTE_RANS`
  (5,181), wins 0 of the 8 classic-xref samples, and the leave-one-out layout
  delta is 0, so layout is never the auto winner (11-file ladder A0 RAW 81,371 →
  A2 +`BYTE_RANS` 48,598 → A5 +`PDF_LAYOUT` 48,598). `classic.pdf` layout 711 vs
  RAW 663; `bigtext.pdf` layout 65,929 vs RAW 65,883 / `BYTE_RANS` 38,154.
- **Why.** Packed framing fixes the container cost, but the residual data object
  is still stored **literally**. Once the structure is predicted, what remains is
  ordinary byte data that a monolithic order-0 `BYTE_RANS` lane codes better than
  the packed literal object. The remaining lever is therefore to **entropy-code
  the residual** — structural prediction composed with an rANS residual, the
  paper's layered model — not to pack the literals further. Recorded as a measured,
  partial positive (campaign `2026-10-05-phase5-4521778`, ADR-0012).

### Phase 5.8 scope and the converging negatives

- **Mechanism.** Phase 5.8 adds the `PACKED_CHANNELS` DRA op (opcode `0x09`),
  bumping the DRA graph to **version 6** and moving the universe to
  `phase5-8;exact-bytes;dra-6;opaque+entropy+pdf+channels+offsets+packed+packed-channels`.
  It reconstructs output from a **data entropy channel** interpreted by a
  serialized item table carried in a **plan entropy channel**, with a declared
  output length validated at evaluation. Both channels use the same
  `Literal`/`Mark`/`Emit` item codec as `PACK_SEGMENTS`; the data object must be
  consumed exactly.
- **Candidate.** `PDF_LAYOUT_RANS` keeps the layout plan but codes the plan's
  literal data object (channel 0) and `encode_items` of its item table
  (channel 1) each as their own order-0 byte-rANS channel with its own model.
- **Complete-cost verdict — rejected vs `BYTE_RANS`.** The candidate is exact and
  fully charged, but head-to-head it wins **0**, loses **8**, and is declined by
  **3** of 11 files; the ladder A0 RAW 81,591 → A2 +`BYTE_RANS` 48,818 →
  A6 +`PDF_LAYOUT_RANS` 48,818 does not move below A5, and the leave-one-out
  layout+rANS delta is **0**. Forced sizes: `classic.pdf` RAW 683 / `BYTE_RANS`
  728 / `PDF_LAYOUT` 731 / `PDF_LAYOUT_RANS` 883; `bigtext.pdf` RAW 65,903 /
  `BYTE_RANS` 38,174 / `PDF_LAYOUT_RANS` 38,341; `many.pdf` RAW 10,235 /
  `BYTE_RANS` 5,201 / `PDF_LAYOUT_RANS` 5,914 (breakdown: data 7,877, plan 1,815,
  models 645, payload 4,775).
- **Why.** Channel 0 entropy-codes nearly the whole file against a single global
  histogram — the same job `BYTE_RANS` performs with one channel — while the plan
  channel (1,815 B on `many.pdf`) plus a second model are pure added metadata the
  monolithic lane never pays. The structural prediction removes fewer bytes than
  the plan channel adds, so `PDF_LAYOUT_RANS` stays above `BYTE_RANS` wherever it
  is proposed. Recorded honestly (campaign `2026-10-05-phase5-8-cf8048d`,
  ADR-0013).
- **Converging negatives (Phases 4 / 5 / 5.7 / 5.8).** Four independent mechanisms
  now point at the same conclusion on this deterministic corpus: coarse typed
  lexical channels do not beat a whole-file order-0 model (Phase 4, ADR-0010);
  correct structural layout prediction does not pay while each field needs a
  framed op (Phase 5, ADR-0011); amortizing that framing lets prediction beat RAW
  at scale but not order-0 entropy coding (Phase 5.7, ADR-0012); and entropy-coding
  the residual as separate channels still loses to `BYTE_RANS` because the plan
  itself is added metadata (Phase 5.8, ADR-0013). At the tested scale, PDF
  structural proceduralization does not beat a whole-file order-0 rANS lane. A
  future win requires documents with far more predictable structure, a plan that
  costs less than it saves, or a candidate that removes structure **without**
  adding a per-site plan (e.g. a canonical/parametric layout).

### Phase 5 scope and the recorded layout rejection

- **Mechanism.** Phase 5 introduces two positional DRA ops, `MARK_OFFSET`
  (`0x06`) and `EMIT_OFFSET` (`0x07`), bumping the DRA graph to **version 4**.
  `MARK_OFFSET` records the current output position into one of 256 bounded slots
  (slot 255 reserved for the xref section start); `EMIT_OFFSET` emits a marked
  position as a fixed-width, zero-padded decimal field. Both are bounded and
  non-Turing-complete like the rest of the DRA.
- **Candidate.** `PDF_LAYOUT` marks each indirect object's introducer offset and
  the classic `xref` section start, then regenerates each 10-digit xref entry
  offset and the `startxref` value from those marks. It applies only to
  classic-cross-reference PDFs with no cross-reference stream and no more than
  255 markable objects; every other file declines. Whenever a precondition fails
  (a mismatched offset, a malformed table, too many objects) the site falls back
  to a literal `INLINE` — a wrong source offset is never predicted or invented.
- **Prediction is correct and the descriptor is exact.** On the sealed corpus,
  `classic.pdf` regenerates 3 of 4 xref entry offsets plus the `startxref`, and
  `incremental.pdf` regenerates 5 of 7 entries plus two `startxref` values. Every
  forced layout descriptor serializes, parses back, and materializes
  byte-for-byte.
- **Complete-cost verdict — rejected on cost.** The cumulative ladder is A0 =
  71,116, A1 = 71,116, A2 = A3 = A4 = A5 = 43,377, the leave-one-out layout delta
  is 0, and `PDF_LAYOUT` wins 0 items (0 of 7 classic-xref files). Forced sizes:
  `classic.pdf` 798 vs RAW 659; `bigtext.pdf` 66,066 vs RAW 65,879 / `BYTE_RANS`
  38,150.
- **Why.** The DRA pays a `MarkOffset` per object and per xref section plus an
  `EmitOffset` per predicted entry. Each such op carries fixed per-segment
  framing (a tag byte plus operand bytes) in the reconstruction program, and that
  framing exceeds the ~7 digits a predicted offset saves at document scale. The
  predicted structure is right; the reconstruction *container* is too expensive.
  `PDF_LAYOUT` stays implemented and available but loses; the negative result and
  its framing analysis are preserved (campaign `2026-10-05-phase5-7193001`,
  ADR-0011).

### Phase 4 scope and the recorded typed-channel rejection

- **Mechanism.** Phase 4 transposes the byte-authoritative Phase-3 lexical cover
  into typed parallel channels (one kind id per token, one 4-byte length per
  token, one payload stream per lexical kind) and reconstructs with the bounded
  `INTERLEAVE_CHANNELS` DRA op (DRA v3, opcode `0x05`). `join(split(x)) == x` by
  construction; every corpus file round-trips byte-exactly (`cmp` + `verify`).
- **Models.** Each channel gets its own order-0 byte-rANS model. Wire **model
  v2** serializes to whichever of sparse or dense is strictly smaller (ties pick
  dense); legacy v1 dense models remain decodable. Compact sparse models cut the
  per-channel model overhead from 7,224 B to 1,981 B on the scale sample.
- **Complete-cost verdict — rejected.** Even after the model-cost cut,
  `PDF_CHANNELS` loses to `BYTE_RANS` on `bigtext.pdf` (46,432 vs 38,142 B,
  ~8,290 B worse). Across the 10-file corpus the cumulative ladder is
  A0 = 71,036, A1 = 71,036, A2 = A3 = A4 = 43,297, the leave-one-out channel
  delta is 0, and `PDF_CHANNELS` wins 0 items. Typed channels beat RAW
  (~21.6% on `bigtext.pdf`) but the complete-cost court rejects them.
- **Why.** Coarse lexical transposition plus per-channel order-0 models cannot
  beat a whole-file order-0 model: the kind and length streams and the extra
  per-channel model records cost more than the transposition saves. Recovering a
  win requires *conditioning* and *ordering* (context across tokens), not more
  per-kind marginal models. `PDF_CHANNELS` stays implemented and available but
  loses; the negative result is preserved (campaign `2026-10-05-phase4-3840bc4`,
  ADR-0010).

### Phase 3 scope and the recorded RAW win

- **Physical authority only.** Phase 3 establishes the owned, byte-authoritative
  PDF physical view: a lexical span cover, a conservative structural scanner
  (`%PDF-`, `obj`/`endobj`, `stream`/`endstream`, `xref`, `trailer`,
  `startxref`, `%%EOF`), `/Length` resolution, and an append-only revision map.
  Coverage and exactness gates pass (`all_covered = true`, `all_exact = true`)
  on the deterministic 9-item corpus.
- **Detection is validated, not extension-based.** A file is a PDF only when
  the bytes contain a `%PDF-` header, at least one complete indirect object, and
  at least one `%%EOF`; otherwise it falls back to the opaque exact lane.
- **The PDF candidate loses to RAW on purpose.** The Phase-3 candidate persists
  the physical partition as one literal `INLINE` op per span and performs no
  structural compression, so RAW won all 9 items and `PDF_PHYSICAL` won 0. This
  is the expected result, recorded rather than hidden; structural wins are
  Phase 5+.
- **qpdf is an oracle.** Object-number agreement is 100% on the differential
  court (classic 4/4, two-page 6/6, incremental 5/5). Objects inside object
  streams have no physical `N G obj` marker and are expected to diverge from
  qpdf's semantic view; qpdf is never the byte authority.

### Phase 2 scope, feature gating, and honest limits

- **Scope.** Phase 2 is order-0 byte channels only. It wins where the byte
  histogram repays the 516-byte canonical model; it loses on tiny and
  high-entropy inputs after model cost is charged, and the negative controls pin
  that outcome rather than hiding it.
- **Feature gating.** `default = ["rans"]`. Built with `--no-default-features`,
  the exact RAW/RLE floor still compiles and materializes channel-free
  descriptors exactly; a descriptor that declares `MODEL`/`ENTROPY_CHANNEL`
  records returns `UnsupportedFeature` (exit code 6) rather than being silently
  reinterpreted.
- **Capsule, never a seed.** A channel carries its model, decoder state,
  renormalization payload, and counts. `rANS state alone reconstructs arbitrary
  data` is false and is not claimed.

## Phase 0 research (frozen into this repo)

The prior-art paper is the architectural authority. Independent Phase-0 subagent
findings were produced under the (gitignored) `research/subagents/phase-00/` and
are frozen into ADRs and this ledger. Key verified facts adopted:

- `ryg-rans-rs` 0.5.1 is `ryg_rans_rs::byte::*`; safe manual decode via
  `rans_byte_dec_init` / `rans_byte_dec_advance_symbol` / `rans_byte_dec_renorm`
  returning `Result`; the `alloc_utils::decode` convenience path **panics** and is
  banned from the normative hostile-input path. See ADR-0006.
- `preflate-rs` 0.7.6 exposes `preflate_whole_deflate_stream` /
  `recreate_whole_deflate_stream` (plus a streaming pair); raw DEFLATE only;
  correction state is an opaque, version-coupled bitcode+CABAC blob; reconstruction
  can panic on hostile state and must be isolated and bounded. See ADR-0007.
- `entropyfs` 0.7.17 exposes an embeddable `engine::Engine` (BLAKE3 `BlobId`,
  `put_blob`/`get_blob`/`read_blob_range`), usable with `default-features=false`;
  `dsfb` is a hard (non-optional) dependency; a store directory + exclusive lock
  is required. See ADR-0008.
- PDF xref entries are literal file byte offsets; incremental updates are an
  append-only revision chain; `/Size` never decreases; object-stream membership is
  physical. qpdf JSON omits offsets and transparently decrypts, so it is an oracle,
  never the authority. See ADR-0009.

## Freeze policy

The wire format is **not** frozen. It becomes v1 only when the exact core, bounds,
integrity, and the PDF golden path are stable and a hostile-input format court is
green (see `docs/adr/0004`). Until then, all format changes bump the universe
string and/or the header minor version.
