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
| PDF `/Length`/revision proceduralization and stream replay | 5+ | PROPOSED | structural compression beyond xref offsets is not yet measured |
| Exact DEFLATE replay (`preflate-rs`) | 6 | PROPOSED | candidate, per-stream, exactness first |
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
loses to RAW/`BYTE_RANS` (campaign `2026-10-05-phase5-7193001`, ADR-0011). PDF
structural compression beyond xref offsets — `/Length`/revision
proceduralization, stream replay, and typed residuals — and every cross-document
mechanism remain `PROPOSED` (Phases 6+). Phase 2 measured only the order-0 typed
byte entropy floor over an opaque mixed corpus; Phase 4 showed that coarse
lexical transposition plus per-channel order-0 models does not beat a monolithic
order-0 channel, and Phase 5 showed that correct structural prediction does not
pay while each predicted field still needs its own framed DRA op.

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
