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
| Phase-5 forced-candidate court (`--force pdf-layout`) | **Measured** | `tools/phase5-court.sh`; campaign `2026-10-05-phase5-7193001` |
| PDF structural adapters (Phases 6–8) | Planned | — |
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

The entropy substrate is optional in the build: `default = ["rans"]`, and a
channel-bearing descriptor decoded without the feature returns an explicit
`UnsupportedFeature`, never a silent reinterpretation.

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
`pdf-layout`) for honest per-mechanism ablation; it never bypasses exactness,
and it fails with a typed usage error when the input does not propose that kind.

## Repository layout

```text
src/            one crate; modules for architectural separation
tests/          exact / malformed / conformance courts
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
