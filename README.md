# VOLE-Document

Byte-exact procedural document storage.

VOLE-Document persists a **bounded deterministic reconstruction description** of a
document — reconstruction structure, parameters/state, typed residual channels,
and (in the entropy phases) typed rANS channels — and materializes the exact
original bytes on demand.

The governing invariant of the exact profile is uncompromising:

```text
materialize(descriptor) == original_bytes
```

Parsing successfully, producing "the same" text, the same object graph, the same
pages, the same rendering, or a canonical re-save are **not** substitutes.

> This project is deliberately *not* "a PDF optimizer that happens to use rANS".
> rANS is the entropy substrate beneath the representation, never the procedural
> model. See [`SPEC.md`](SPEC.md) and [`docs/`](docs/) for the architecture.

## Status

| Area | State | Evidence |
|---|---|---|
| Exact `.voldoc` container (framing, header, records) | **Implemented** | `src/container/`, unit + conformance courts |
| Typed errors + stable exit codes | **Implemented** | `src/error.rs` |
| Centralized resource limits | **Implemented** | `src/limits.rs` |
| CRC32C framing + SHA-256 archival identity | **Implemented** | `src/integrity.rs` |
| Document Reconstruction Algebra (literal subset) | **Implemented** | `src/dra/` |
| Coverage certificate (checked invariant) | **Implemented** | `src/dra/program.rs` |
| RAW exact opaque adapter | **Implemented** | `src/adapter/opaque/` |
| Candidate complete-cost court + decode-before-commit | **Implemented** | `src/encode/` |
| CLI (`encode`/`decode`/`verify`/`inspect`/`capabilities`) | **Implemented** | `src/main.rs` |
| Exact court over a mixed corpus | **Measured** | `evidence/campaigns/` |
| Native rANS floor (order-0 / typed byte channels) | **Measured** | campaign `2026-10-05-phase2-f6af30b` |
| RLE candidate (`REPEAT_LAST` run-length) | **Measured** | campaign `2026-10-05-phase2-f6af30b` |
| BYTE_RANS candidate (order-0 byte channel) | **Measured** | campaign `2026-10-05-phase2-f6af30b` |
| Entropy capsule (full decoder-entry state, not a seed) | **Measured** | ADR-0006; `src/entropy/` |
| PDF lexical span cover (Phase 3.1) | **Measured** | campaign `2026-10-05-phase3-486aa17` |
| PDF byte-authoritative physical scanner (Phase 3.2–3.3) | **Measured** | campaign `2026-10-05-phase3-486aa17` |
| PDF incremental revision map (Phase 3.4) | **Measured** | campaign `2026-10-05-phase3-486aa17` |
| qpdf differential oracle court (oracle, never authority) | **Measured** | `tools/pdf-oracle.sh`; campaign `2026-10-05-phase3-486aa17` |
| PDF lexical channel transposition (`split`/`join`) | **Implemented** | `src/adapter/pdf/channels.rs`; `tests/pdf_channels.rs` |
| `INTERLEAVE_CHANNELS` DRA op (DRA v3) | **Measured** | `src/dra/op.rs`; campaign `2026-10-05-phase4-3840bc4` |
| Compact entropy model wire v2 (sparse/dense, smaller chosen) | **Measured** | `src/entropy/model.rs`; campaign `2026-10-05-phase4-3840bc4` |
| Forced-candidate ablation (`encode --force KIND`) | **Measured** | `tools/phase4-court.sh`; campaign `2026-10-05-phase4-3840bc4` |
| PDF typed channels (`PDF_CHANNELS`) | **Recorded (rejected on corpus)** | campaign `2026-10-05-phase4-3840bc4`; exact but loses to `BYTE_RANS` on complete cost (ADR-0010) |
| Positional DRA ops (`MARK_OFFSET` / `EMIT_OFFSET`, DRA v4) | **Implemented** | `src/dra/op.rs`; `tests/pdf_layout.rs` |
| PDF layout candidate (`PDF_LAYOUT`) | **Recorded (rejected on cost)** | campaign `2026-10-05-phase5-7193001`; byte-exact and predicts xref offsets/`startxref`, but loses to RAW/`BYTE_RANS` on DRA framing cost (ADR-0011) |
| Packed segment framing (`PACK_SEGMENTS`, DRA v5) | **Implemented** | `src/dra/op.rs`; opcode `0x08`, one op + a compact varint item table over a single data object, amortizing per-segment framing |
| PDF layout on packed framing (layout-v2) | **Recorded (beats RAW at scale, rejected vs `BYTE_RANS`)** | campaign `2026-10-05-phase5-4521778`; packed+coalesced layout beats RAW on `many.pdf` (10,069 vs 10,215) but still loses to `BYTE_RANS` (5,181); residual stored literally (ADR-0012) |
| `PACKED_CHANNELS` DRA op (DRA v6) | **Implemented** | opcode `0x09`; reconstructs from a data channel + a plan channel (serialized item table) with a declared output length validated at eval; universe → `phase5-8` |
| PDF layout + rANS (`PDF_LAYOUT_RANS`) | **Recorded — rejected vs `BYTE_RANS`** | campaign `2026-10-05-phase5-8-cf8048d`; byte-exact, but wins 0 / loses 8 / declines 3 head-to-head (ADR-0013) |
| Phase-5 forced-candidate court (`--force pdf-layout`) | **Measured** | `tools/phase5-court.sh`; campaigns `2026-10-05-phase5-7193001` and `2026-10-05-phase5-4521778` |
| Phase-5.8 forced-candidate court (`--force pdf-layout-rans`) | **Measured** | `tools/phase5-8-court.sh`; campaign `2026-10-05-phase5-8-cf8048d` |
| Lexer stream opacity (`stream`+EOL opaque span) | **Adopted** | campaign `2026-10-05-phase6-0d0bb79`; stream-data bytes are a byte-authoritative span, so `/FlateDecode` stream spans are exact |
| `DEFLATE_REPLAY` DRA op (DRA v8) | **Implemented** | `src/dra/op.rs`; opcode `0x0A`, explicit `replay_codec` tag (`preflate-0.7.6-experimental`), exact raw-DEFLATE replay from `(plaintext, corrections)` with a declared output length statically bounded before the engine runs, `catch_unwind`-isolated, mandatory feature bit (opt-in `deflate-replay` cargo feature) |
| Exact DEFLATE replay, raw plaintext (`PDF_DEFLATE_REPLAY`) | **Recorded — rejected vs `BYTE_RANS`** | campaign `2026-10-05-phase6-0d0bb79`; byte-exact, but on `flate.pdf` 56,736 vs `BYTE_RANS` 49,291 (plaintext ≈ bitstream) |
| Exact DEFLATE replay, shared rANS plaintext (`PDF_DEFLATE_REPLAY_RANS`) | **Adopted — first structural win** | campaign `2026-10-05-phase6-0d0bb79`; `flate.pdf` 36,102 vs `BYTE_RANS` 49,291 (**−13,189 B**) (ADR-0015) |
| Phase-6 replay court (`--force pdf-deflate-replay[-rans]`) | **Measured** | `tools/phase6-court.sh`; campaign `2026-10-05-phase6-0d0bb79` |
| Producer-stratified Flate ratio harness (`deflate-stats`) | **Measured** | `tools/pdf-corpus.sh`; amendment campaign `2026-10-05-phase7-corpus-b-c4eb77e` (supersedes `2026-10-05-phase7-corpus-f1f8d26`); 24/24 replayed, 0 declined; diagnostic only, no new candidate |
| Generator-family Flate corpus (`producers`: ReportLab/Cairo/LibreOffice/pdfTeX) | **Measured** | campaign `2026-10-05-phase7-producers-e071250`; 87/87 replayed; `PDF_DEFLATE_REPLAY_RANS` beats `BYTE_RANS` on **Cairo** (58,711 → 34,574, −24,137 B) — the first **authoring-generator** witness; ReportLab/pdfTeX lose, LibreOffice declines to share; no candidate changed |
| PDF structural adapters (Phases 7–8) | Planned | — |
| EntropyFS store-backed form (Phase 9) | Planned | — |
| DSFB search governance (Phase 10) | Planned | — |
| Partial materialization (Phase 11) | Planned | — |

"Implemented" means the mechanism exists and is tested. "Measured" means there is
a sealed campaign under `evidence/`. The Phase-1 core establishes exactness,
framing, integrity, bounds, and receipts before any entropy or format-aware
mechanism is allowed to compete; Phase 2 then measures entropy channels on that
same exactness floor.

### Phase 2 measured results

Phase 2 is **order-0 typed byte channels only** — no context model, no typed
residuals, and no format awareness. On a 9-file mixed corpus the cumulative
core→full ladder over serialized `.voldoc` bytes is:

```text
sum_source = 590081
sum_core   = 464474   (RAW + RLE)
sum_full   = 291304   (RAW + RLE + BYTE_RANS)
delta      = 173170   (sum_core - sum_full)
```

The entire delta is attributed to the two files where `BYTE_RANS` wins
(`text-256k.bin` 262144 → 143746; `skewed.bin` 65536 → 11382). `RLE` wins the
long runs (`zeros-64k.bin` 65536 → 283; `runs.bin` 65536 → 3088). Negative
controls hold: `BYTE_RANS` never wins on random 64 KiB (stored RAW at 65536 →
65845, a 309-byte fixed framing overhead) or on empty/one-byte inputs (RLE).
The canonical model's bytes are charged like any other bytes, so on tiny or
high-entropy inputs order-0 rANS loses to RAW/RLE as required — this is a scoped
measurement on one deterministic corpus, not a general compression claim.

The entropy substrate is optional in the build: `default = ["rans"]`. The exact
DEFLATE replay stack is **opt-in** (`--features deflate-replay`); a
channel-bearing or replay-bearing descriptor decoded without the required
feature returns an explicit `UnsupportedFeature`, never a silent
reinterpretation.

Receipt:
[`evidence/campaigns/2026-10-05-phase2-f6af30b/`](evidence/campaigns/2026-10-05-phase2-f6af30b/).

### Phase 3 measured results

Phase 3 adds a **byte-authoritative PDF physical scanner**: an owned lexer, a
conservative structural span cover, `/Length` resolution, and an append-only
revision map. The sealed campaign `2026-10-05-phase3-486aa17` runs over a
deterministic 9-item corpus (7 valid PDFs plus `malformed.pdf` and `notpdf.bin`
as negative controls):

- **Coverage** — `all_covered = true`: 171 spans, 19 objects, and 8 revisions
  across the corpus, partitioned into a contiguous cover of `[0, len)` with no
  gap and no overlap.
- **Byte-exactness** — `all_exact = true`: `materialize(descriptor) ==
  original_bytes` for every item, including both negative controls through the
  opaque RAW lane.
- **Validated detection** — a file is a PDF only when the bytes show a `%PDF-`
  header **and** an indirect object **and** a `%%EOF`; the extension is never
  authority, and both controls report `is_pdf = false`.
- **Revision map** — the incremental input yields two append-only revisions with
  a `/Prev` chain, and `/Size` is treated as never decreasing.
- **qpdf oracle** — 100% object-number agreement with qpdf 11.3 (classic 4/4,
  two-page 6/6, incremental 5/5); `qpdf --check` reports valid; `pdfinfo` pages
  1/2/1. Divergence is expected where objects are compressed inside object
  streams: those have no physical `N G obj` marker, so a physical scanner
  enumerates fewer objects than qpdf's semantic view. qpdf is an oracle, never
  the byte authority.

The literal PDF candidate currently **loses to RAW**: RAW won all 9 items and
`PDF_PHYSICAL` won 0. This is the **expected Phase-3 result** — the physical lane
persists each span as one literal `INLINE` op with no structural compression, so
its per-span overhead loses once complete cost is charged. Structural
compression (xref/`/Length` proceduralization, stream replay) is Phase 5+ and is
not claimed here.

Receipt:
[`evidence/campaigns/2026-10-05-phase3-486aa17/`](evidence/campaigns/2026-10-05-phase3-486aa17/).

### Phase 4 measured results

Phase 4 transposes the byte-authoritative lexical cover into **typed channels**
(one kind id per token, one 4-byte length per token, one payload stream per
lexical kind) and reconstructs them with the bounded `INTERLEAVE_CHANNELS` DRA op
(DRA v3). Each channel gets its own order-0 byte-rANS model, and model wire **v2**
serializes to whichever of sparse or dense is smaller. The sealed campaign
`2026-10-05-phase4-3840bc4` runs a forced-candidate ablation (`encode --force
KIND`) over a deterministic 10-file corpus:

```text
A0 RAW            = 71036
A1 + RLE          = 71036
A2 + BYTE_RANS    = 43297
A3 + PDF_PHYSICAL = 43297
A4 + PDF_CHANNELS = 43297     leave-one-out channel delta = 0
```

All 10 files round-trip byte-exactly (`cmp` + `verify`) and the qpdf oracle
re-check passes. Auto winners: RAW = 8, `BYTE_RANS` = 2, `PDF_PHYSICAL` = 0,
`PDF_CHANNELS` = 0. On the text-heavy scale sample `bigtext.pdf` (65,549 B) the
forced sizes were RAW = 65,871, `BYTE_RANS` = 38,142, `PDF_CHANNELS` = 46,432:
typed channels beat RAW by ~21.6% but **lose to `BYTE_RANS` by ~8,290 B**. Compact
sparse models cut the per-channel model overhead from 7,224 B to 1,981 B, which
was not enough to close the gap. The honest conclusion is that coarse lexical
transposition plus per-channel order-0 models does **not** beat a monolithic
order-0 `BYTE_RANS` on this corpus, so the typed lexical channels were **rejected
by the complete-cost court** and `PDF_CHANNELS` is recorded (not adopted). A win
would require *conditioning and ordering* rather than more marginal per-kind
models — that is Phase 5+ work (ADR-0010).

Receipt:
[`evidence/campaigns/2026-10-05-phase4-3840bc4/`](evidence/campaigns/2026-10-05-phase4-3840bc4/).

### Phase 5 measured results

Phase 5 adds **positional DRA ops** (`MARK_OFFSET` / `EMIT_OFFSET`, DRA v4)
and the first candidate that replaces literal structural bytes with
*procedurally determined* ones: the classic cross-reference **layout** lane
(`PDF_LAYOUT`), which marks each indirect object's introducer offset and the
`xref` section start, then regenerates the 10-digit xref entry offsets and the
`startxref` value from those marks. The sealed campaign
`2026-10-05-phase5-7193001` runs the forced-candidate ablation
(`encode --force KIND`) over a deterministic 10-file corpus:

```text
A0 RAW            = 71116
A1 + RLE          = 71116
A2 + BYTE_RANS    = 43377
A3 + PDF_PHYSICAL = 43377
A4 + PDF_CHANNELS = 43377
A5 + PDF_LAYOUT   = 43377     leave-one-out layout delta = 0
```

All 10 files round-trip byte-exactly through their auto winner (`cmp` +
`verify`); auto winners are RAW = 8, `BYTE_RANS` = 2, and `PDF_PHYSICAL` /
`PDF_CHANNELS` / `PDF_LAYOUT` = 0. **The prediction works and the descriptor is
exact** — `classic.pdf` regenerates 3 of 4 xref entry offsets plus the
`startxref`, and `incremental.pdf` regenerates 5 of 7 entries plus two
`startxref` values — but the lane still **loses to RAW and `BYTE_RANS` on
complete cost**: `classic.pdf` 798 vs RAW 659, and `bigtext.pdf` 66,066 vs RAW
65,879 / `BYTE_RANS` 38,150. Layout wins 0 of the 7 classic-xref files. The
reason is framing, not prediction: the DRA pays a `MarkOffset` per object and
per xref section plus an `EmitOffset` per predicted entry, and that per-segment
op framing (tag + operand length) costs more than the ~7 digits saved per
predicted offset at document scale. The predicted structure is right; the
reconstruction *container* is too expensive. This is recorded honestly as a
negative result (ADR-0011) and a format-design input for later phases.

Receipt:
[`evidence/campaigns/2026-10-05-phase5-7193001/`](evidence/campaigns/2026-10-05-phase5-7193001/).

### Packed framing (Phase 5.7) measured results

Phase 5.7 attacks the framing root cause directly. The `PACK_SEGMENTS` DRA op
(opcode `0x08`, bumping the DRA graph to **version 5**, universe
`phase6-prep;…;dra-5;…+packed`) amortizes per-segment framing: instead of one
tagged op per literal run, it carries **one op plus a compact varint item table**
(`Literal` varint-length / `Mark` / `Emit`) over a single data object. The layout
candidate was rebuilt on this op (**layout-v2**), and literal coalescing (5.7.2b)
merges adjacent literal runs: on `many.pdf` (200 objects, 9,881 B) the item table
fell from **1,413 to 805** items.

The sealed campaign `2026-10-05-phase5-4521778` runs the forced-candidate
ablation over an 11-file corpus:

```text
A0 RAW            = 81371
A2 + BYTE_RANS    = 48598
A5 + PDF_LAYOUT   = 48598     leave-one-out layout delta = 0
```

Forced sizes:

```text
many.pdf     layout-v2 10069   RAW 10215   BYTE_RANS 5181   (layout beats RAW by 146 B)
classic.pdf  layout      711   RAW   663
bigtext.pdf  layout    65929   RAW 65883   BYTE_RANS 38154
```

Packed framing plus coalescing makes **structural layout prediction beat RAW at
scale** (`many.pdf` 10,069 vs 10,215), which the per-segment Phase-5 framing never
managed. It is still **not adopted**: it loses to `BYTE_RANS` (5,181), wins 0 of
the 8 classic-xref samples, and the leave-one-out layout delta is 0, so layout is
never the auto winner. The honest conclusion is that the framing is fixed, but the
residual data object is stored **literally**, so any order-0 entropy lane
dominates it. The remaining lever is to entropy-code the residual data object —
structural prediction **composed with** rANS on the residual, which is the paper's
layered model — not more literal packing. This is recorded as a partial positive
(ADR-0012).

Receipt:
[`evidence/campaigns/2026-10-05-phase5-4521778/`](evidence/campaigns/2026-10-05-phase5-4521778/).

### Layout + rANS residual (Phase 5.8) measured results

Phase 5.8 builds the lever ADR-0012 named: the `PACKED_CHANNELS` DRA op (opcode
`0x09`, DRA **v6**, universe
`phase5-8;…;dra-6;…+packed+packed-channels`) reconstructs from a **data channel**
plus a **plan channel** (the serialized item table) with a declared output length
validated at eval, and the `PDF_LAYOUT_RANS` candidate codes the layout plan's
data object and its item table each as their own order-0 rANS channel. The sealed
campaign `2026-10-05-phase5-8-cf8048d` runs the forced-candidate ablation over the
11-file corpus:

```text
A0 RAW               = 81591
A2 + BYTE_RANS       = 48818
A6 + PDF_LAYOUT_RANS = 48818     leave-one-out layout+rANS delta = 0
```

Forced sizes:

```text
classic.pdf  RAW 683  BYTE_RANS 728  PDF_LAYOUT 731  PDF_LAYOUT_RANS 883
bigtext.pdf  RAW 65903  BYTE_RANS 38174  PDF_LAYOUT_RANS 38341
many.pdf     RAW 10235  BYTE_RANS 5201  PDF_LAYOUT 10089  PDF_LAYOUT_RANS 5914
```

On `many.pdf` the layout+rANS size breaks down as data 7,877, plan 1,815, models
645, payload 4,775. Head-to-head against `BYTE_RANS`: **win 0, lose 8, declined
3**. The honest conclusion: layout+rANS does **not** beat `BYTE_RANS`. Channel 0
codes nearly the whole file — the same job `BYTE_RANS` does with one channel — so
the plan channel (1,815 B on `many.pdf`) plus a second model are added metadata
`BYTE_RANS` never pays. Three phases (4, 5, 5.7) plus this one converge: at the
tested scale, PDF structural proceduralization does not beat a whole-file order-0
rANS lane.

Receipt:
[`evidence/campaigns/2026-10-05-phase5-8-cf8048d/`](evidence/campaigns/2026-10-05-phase5-8-cf8048d/).

### Exact DEFLATE replay (Phase 6) measured results

Phases 4–5.8 all proceduralize **plain** syntax that `BYTE_RANS` already models
well. Phase 6 attacks a different layer: bytes the producer has **already
entropy-coded**. The `DEFLATE_REPLAY` DRA op (opcode `0x0A`, DRA **v8**, universe
`phase6;…;dra-8;…+deflate-replay-preflate-0.7.6-experimental`) reconstructs the
*original* raw DEFLATE bitstream of a `/FlateDecode` stream from `(plaintext,
corrections)`. Its `replay_codec` tag names the correction representation as an
experimental, version-coupled preflate-0.7.6 layout (not frozen v1), and an
unknown tag fails closed. The
byte-authoritative scanner owns stream discovery and `/Filter` classification;
`preflate` never discovers streams. A lexer fix makes `stream`+EOL payloads opaque
spans. Two candidates use the op: `PDF_DEFLATE_REPLAY` (raw, deduplicated
plaintext objects) and `PDF_DEFLATE_REPLAY_RANS` (each unique plaintext is one
shared order-0 byte-rANS channel). The sealed campaign
`2026-10-05-phase6-0d0bb79` runs the forced-candidate ablation over the 12-file
corpus:

```text
A0 RAW                      = 139950
A2 + BYTE_RANS              =  98560
A6 + PDF_LAYOUT_RANS        =  98560
A7 + PDF_DEFLATE_REPLAY     =  98560
A8 + PDF_DEFLATE_REPLAY_RANS=  85371     leave-one-out replay-rANS delta = -13189
```

Forced sizes on `flate.pdf` (57,513 B):

```text
RAW 57908   BYTE_RANS 49291   PDF_DEFLATE_REPLAY 56736   PDF_DEFLATE_REPLAY_RANS 36102
```

`PDF_DEFLATE_REPLAY_RANS` is the auto winner on `flate.pdf` and **beats
`BYTE_RANS` by 13,189 B**. The six `FlateDecode` streams are the content
plaintext `p1` at levels 9/6/1/0 (one shared plaintext, four appearances), a
graphics stream `p2` at level 6, and an incompressible stream `p3` at level 6
that DEFLATE stores; the descriptor replays all six and codes their **3 unique
plaintexts** as order-0 channels (`streams=6 replayed=6 channels=3 objects=6`).
Of `p1`'s four appearances only the level-0 stream is weakly coded (stored);
level 1 is ~19% of the plaintext and levels 6/9 are strong, so the win needs the
shared plaintext to *also* have a large/weakly-coded appearance. `BYTE_RANS`
order-0-codes the six streams to 49,291 B; it does not carry them verbatim. The
raw-plaintext variant loses (56,736 B) because a strongly-compressed stream's
plaintext is nearly as large as the stream it replaces. Head-to-head vs
`BYTE_RANS`: **win 1, lose 0, decline 11** (the other files have no lone
`FlateDecode` stream); every auto winner is exact (`cmp` + `verify`). This is
**one composed sample**, at commit `0d0bb79`, measured on a single synthetic
fixture: the win requires a shared plaintext that also has a large/weakly-coded
appearance (unique strongly-compressed streams lose, by up to 4.46×), and the
losing region is unique, strongly-compressed plaintext. It is the first measured
positive for a PDF structural candidate
(ADR-0015); the plain-syntax converging negatives (ADR-0010–ADR-0013) stand.

Receipt:
[`evidence/campaigns/2026-10-05-phase6-0d0bb79/`](evidence/campaigns/2026-10-05-phase6-0d0bb79/).

### Producer-stratified Flate ratio (Phase 7.0) measured results

`tools/pdf-corpus.sh` builds a locally-generated corpus from distinct producer
lineages (Ghostscript 10.00.0 at five `/PDFSETTINGS`, qpdf 11.3.0 in four modes,
a hand-written stored-block-zlib base, plus the Phase-3 synthetic set), and
`vole-document deflate-stats` measures the exact-replay ratio per `FlateDecode`
stream — `correction/compressed`, `(plaintext+corr)/compressed`,
`(rANS(plaintext)+corr)/compressed` — with p10/p50/p90 by producer, the
exact-replay acceptance rate, and every decline. It also reports two aggregate
complete costs so shared plaintext is not overcounted:
`replayed_rans_full_bytes` (naive per-stream sum) and
`replayed_rans_dedup_bytes` (one charge per unique plaintext + one per unique
correction, matching the shared-channel candidate).

On 24 `FlateDecode` streams (re-measured after the Stage-A lexer stream-boundary
fix): **24 replayed, 0 declined (acceptance 1.000)**. The pre-fix run saw only 17
streams (11 replayed, 6 declined); the census rose because the old over-read had
swallowed whole stream objects (every qpdf-generated file was undercounted;
`qpdf-preserve-objectstreams.pdf` had reported zero). The Phase-6 win region
appears **only in our own hand-authored fixtures** (`hand-base2.pdf`: deduped
rANS 55,531 vs naive 111,062; `flate.pdf`: 34,051 vs 89,437). The same geometry
in `qpdf-preserve-objectstreams.pdf` (55,531 vs 111,062) is present **only
because qpdf `--object-streams=preserve` copied and renumbered the two
byte-identical raw streams already authored in `hand-base2.pdf`** (confirmed via
`qpdf --raw-stream-data`: all four stream hashes are `ec028dc1…`); the fixture
already wins 112,011 → 56,885 B and qpdf adds +41 B, so **99.93% of the reported
qpdf win is inherited**. No genuinely transformed producer output exhibits the
region. Corpus-wide `correction/compressed` p10/p50/p90 = 0.000320 / **0.014716**
/ 0.097360; the `0.004518` median is the `pdf-make-samples` subset only.
The six former declines were all `not_zlib` **because of a scanner locality
limitation** — the lexer's `find_endstream` required an EOL before `endstream`,
which Ghostscript omits (spec "should", qpdf-tolerated) — not because those
streams are not zlib (they begin `78 9c`, confirmed via `qpdf --raw-stream-data`).
That limitation is **fixed** (commit `c4eb77e`): `find_endstream` now locates the
`endstream` keyword by right-termination. This is a **scoped** result about
locally generated files; qpdf/Ghostscript are transformers, not authoring apps,
and browser/office/TeX families remain a recorded gap. No candidate is adopted.

Receipt:
[`evidence/campaigns/2026-10-05-phase7-corpus-b-c4eb77e/`](evidence/campaigns/2026-10-05-phase7-corpus-b-c4eb77e/)
(amendment; supersedes the original
[`2026-10-05-phase7-corpus-f1f8d26/`](evidence/campaigns/2026-10-05-phase7-corpus-f1f8d26/));
report: [`docs/evidence/phase7-corpus-report.md`](docs/evidence/phase7-corpus-report.md).

**Complete-cost court (the decisive Phase-7.0 measurement).** The ratio harness
is a diagnostic; the court decides. A second sealed campaign
(`2026-10-05-phase7-court-99dc72e`, commit `99dc72e`, driver
`tools/pdf-court.sh`) runs the real CLI over all 23 locally generated corpus
files — unforced and with `--force raw|byte-rans|pdf-deflate-replay|pdf-deflate-replay-rans`
— and compares complete serialized `.voldoc` sizes. `PDF_DEFLATE_REPLAY_RANS` vs
`BYTE_RANS`: **win 3, lose 8, decline 12 — all 3 wins self-authored**. The wins
are exactly the Phase-6 shared-plaintext geometry, and in all three the unforced
court picks `PDF_DEFLATE_REPLAY_RANS`:

```text
qpdf-preserve-objectstreams.pdf  BYTE_RANS 112147 -> PDF_DEFLATE_REPLAY_RANS 56980  (-55167)
hand-base2.pdf                   BYTE_RANS 112011 -> PDF_DEFLATE_REPLAY_RANS 56885  (-55126)
_synthetic/flate.pdf             BYTE_RANS  49291 -> PDF_DEFLATE_REPLAY_RANS 36102  (-13189)
```

The reported "qpdf win" is not a transformed-producer result:
`qpdf --object-streams=preserve` copied and renumbered the two byte-identical raw
streams (`ec028dc1…`) already authored in our `hand-base2.pdf` fixture (the
fixture wins 112,011 → 56,885 B; qpdf adds only **+41 B**, so **99.93% of the
55,167 B win is inherited**). Every **genuinely transformed** producer output
loses or declines on this corpus: all 5 Ghostscript variants and both qpdf
compression variants (unique, strongly-compressed plaintext) **lose**, and the 12
files with no replayable Flate lane **decline** (a forced lane the input does not
propose is a typed `Usage` error, recorded `null`). So on this locally generated
corpus exact replay **wins 3 / loses 8 / declines 12, and all 3 wins are
self-authored** (two fixtures plus a preserved copy of one). `--deterministic-id`
is a `/ID`-only normalization (55,165 B no-flag vs 55,167 B flagged): it makes
the court reproducible but neither creates nor destroys the win. The Phase-6 win
is real and byte-exact, but its enabling condition (a plaintext shared across
streams with a large/weakly-coded appearance) is **not produced by the tested
transformers**, which motivates Phase 7.2 (nested content proceduralization). All
23 auto winners are `verify` + `cmp` byte-exact. The corpus is **locally
generated** and is **not a population sample**; qpdf and Ghostscript are
**transformers, not authoring applications**, and browser/PDFium, LibreOffice,
pdfTeX and Adobe outputs remain a recorded gap. No candidate is adopted and no
wire format changed.

Receipt:
[`evidence/campaigns/2026-10-05-phase7-court-99dc72e/`](evidence/campaigns/2026-10-05-phase7-court-99dc72e/).

### Generator-family Flate corpus (Phase 7.0b) measured results

The Phase-7.0 gap was that qpdf and Ghostscript are **transformers** and never emit
the shared plaintext the win region needs. Phase 7.0b adds a separate, opt-in
`producers` image (`Dockerfile` stage + compose service, base
`debian:bookworm-slim@sha256:3783cc01…`, the same digest as `tools`; ~724 MB) with
four real **authoring generators**: ReportLab 3.6.12, Cairo 1.20.1/libcairo 1.16.0,
LibreOffice Writer 7.4.7.2, and pdfTeX 3.141592653-2.6-1.40.24. All four ran.
`tools/pdf-corpus-producers.sh` generates one PDF per family from the same
deterministic content document as `tools/pdf-corpus.sh`, fingerprints every
`/FlateDecode` payload, and applies
`qpdf --deterministic-id --stream-data=preserve --object-streams=preserve` only when
it leaves every payload byte-identical (ReportLab, Cairo, LibreOffice; pdfTeX kept
raw). ReportLab/Cairo/pdfTeX are byte-reproducible; LibreOffice is not.

`deflate-stats`: **87 Flate streams, 87 replayed, 0 declined → acceptance 1.000**;
corpus-wide `correction/compressed` p10/p50/p90 = 0.034759 / 0.047945 / 0.068028.

Complete-cost court: `PDF_DEFLATE_REPLAY_RANS` vs `BYTE_RANS` — **win 1 / lose 3 /
decline 0**:

```text
cairo-vector.pdf        BYTE_RANS  58711 -> PDF_DEFLATE_REPLAY_RANS  34574  (-24137)  WIN
reportlab-multipage.pdf BYTE_RANS  11144 -> PDF_DEFLATE_REPLAY_RANS  14456  (+3312)   lose
pdftex-doc.pdf          BYTE_RANS  24435 -> PDF_DEFLATE_REPLAY_RANS  35098  (+10663)  lose
libreoffice-export.pdf  BYTE_RANS  72791 -> PDF_DEFLATE_REPLAY_RANS 375265 (+302474)  lose
```

**Cairo — a genuine authoring generator — wins.** For a repeated page it emits six
byte-identical page content streams; the shared-channel lane stores that plaintext
once and replay-rANS beats `BYTE_RANS` under complete cost by **24,137 B**, and the
unforced court selects it. This is the **first authoring-generator witness** of the
Phase-6 win region (Phase 7.0 had only self-authored fixtures). ReportLab and pdfTeX
emit the same shared geometry yet **lose** at complete cost (framing overhead on a
small file; already-tight streams), and LibreOffice shares nothing — shared plaintext
is necessary but **not sufficient**. The win is **conditional** on repeated identical
page content and is **not** a population claim; complete cost is authoritative and
the `deflate-stats` dedup aggregate is a diagnostic that over-states wins. No
candidate or wire format changed.

Receipt:
[`evidence/campaigns/2026-10-05-phase7-producers-e071250/`](evidence/campaigns/2026-10-05-phase7-producers-e071250/);
script `tools/pdf-corpus-producers.sh`; ledger
[`evidence/corpus/phase7-producers/provenance.json`](evidence/corpus/phase7-producers/provenance.json).

## Quick start (Docker only)

All project commands run inside pinned containers. The host only invokes Docker.

```sh
# Build the toolchain image (pinned by digest in Dockerfile)
docker compose build dev

# Build, test, lint, format
docker compose run --rm --no-TTY dev cargo test  --all-features
docker compose run --rm --no-TTY dev cargo clippy --all-targets --all-features -- -D warnings
docker compose run --rm --no-TTY dev cargo fmt --all --check

# MSRV gate (Rust 1.89)
docker compose build msrv
docker compose run --rm --no-TTY msrv cargo build --locked

# Phase 1 exact court (writes an evidence receipt)
docker compose run --rm --no-TTY dev sh tools/phase1-court.sh
```

CLI surface (activated by the pipeline, not by extension — extensions are hints,
never authority):

```text
vole-document encode      [--force KIND] INPUT   OUTPUT.voldoc
vole-document decode      INPUT.voldoc   OUTPUT
vole-document materialize INPUT.voldoc   OUTPUT
vole-document verify      INPUT.voldoc
vole-document inspect     INPUT.voldoc
vole-document capabilities
```

`encode --force KIND` forces the complete-cost court to consider only one
candidate family (`raw`, `rle`, `byte-rans`, `pdf-physical`, `pdf-channels`,
`pdf-layout`, `pdf-layout-rans`, `pdf-deflate-replay`, `pdf-deflate-replay-rans`)
for honest per-mechanism ablation; it never
bypasses exactness, and it fails with a typed usage error when the input does not
propose that kind.

## Fuzzing

Two layers, both Docker-only:

- **Deterministic property/mutation courts** (`tests/property.rs`,
  `tests/goldens.rs`, `tests/malformed.rs`) plus the longer soak run
  `tools/soak-fuzz.sh` (`VOLE_FUZZ_ITERS`, default 200000).
- **Coverage-guided libFuzzer targets** (Phase 7.1) in the standalone `fuzz/`
  `cargo-fuzz` package (excluded from `cargo package`), built and run in the
  pinned dated-nightly `fuzz` Docker service:

  ```sh
  docker compose build fuzz
  docker compose run --rm --no-TTY fuzz cargo fuzz build
  FUZZ_SECONDS=60 docker compose run --rm --no-TTY fuzz sh tools/fuzz.sh
  ```

Ten targets cover the `.voldoc` container parser/materializer, the
`encode`→`decode` round trip, the DRA decoder/analyzer/evaluator, the rANS model
and channel decoders, the PDF lexer and physical scanner, xref/`/Prev`, the
`DEFLATE_REPLAY` wrapper, and `verify`. See `fuzz/README.md` for the pinned
toolchain, seeds, and regeneration. The sealed campaign
`evidence/campaigns/2026-10-05-phase7-fuzz-ca6a92b/` observed zero crashes on
nine of ten targets and reported two upstream `preflate-rs` findings (one
mitigated fail-closed, one recorded upstream resource limitation).

## Repository layout

```text
src/            one crate; modules for architectural separation
tests/          exact / malformed / conformance courts
fuzz/           cargo-fuzz coverage-guided targets (excluded from the crate)
tools/          court and gate scripts (run inside Docker)
docs/           architecture, ADRs, security, phase notes
evidence/       immutable campaign receipts (machine-readable)
research/       LOCAL ONLY — gitignored (paper, snapshots, subagent findings)
```

`research/` is intentionally excluded from version control. Durable findings that
matter to a phase are frozen into ADRs and phase notes and referenced from
receipts by hash.

## Licensing

Dual-licensed under either MIT or Apache-2.0, at your option. See
[`LICENSE-MIT`](LICENSE-MIT) and [`LICENSE-APACHE`](LICENSE-APACHE).

**Third-party license note.** The `deflate-replay` feature (Phase 6) is
**opt-in** and depends on `preflate-rs`, which depends on `cabac`, licensed
**LGPL-3.0-or-later**. The default build (`default = ["rans"]`) is
**permissive-only** and contains no LGPL code. Rust links statically by default,
so a binary built **with** `--features deflate-replay` (or `--all-features`)
contains LGPL code and carries the corresponding obligations. See
[ADR-0014](docs/adr/0014-lgpl-cabac-dependency.md).
