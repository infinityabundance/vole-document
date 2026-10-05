# Changelog

All notable changes are recorded here. The format is pre-1.0 and provisional.

## [0.1.0-alpha.6] — unreleased

### Added

- Phase 5.7 — packed segment framing (Phase-6 preparation):
  - `PACK_SEGMENTS` DRA op (opcode `0x08`), bumping the DRA graph to version `5`.
    One op carries a compact item table over a single data object —
    `Literal { len }` (a `u32` LEB128 varint), `Mark { slot }`, and
    `Emit { slot, width }` — so per-segment op framing is paid once instead of
    once per span. The data object must be consumed exactly; unknown tags,
    truncation, unmarked slots, bad widths, overruns, and unconsumed data are
    rejected before allocation.
  - Layout candidate rebuilt on the packed op (**layout-v2**) with **literal
    coalescing**, which merges adjacent literal runs (on `many.pdf`, 200 objects,
    9,881 B, the item table fell from **1,413 to 805** items).
  - A large classic-xref scale sample (`many.pdf`, hundreds of xref entries) to
    expose the scaling win.
- Phase-5.7 universe string
  `vole-document;universe;phase6-prep;exact-bytes;dra-5;opaque+entropy+pdf+channels+offsets`.
- Courts: packed-op/layout-v2 gates in `tests/pdf_layout.rs` and the extended
  forced-candidate ablation court `tools/phase5-court.sh` over the enlarged
  corpus.

### Measured

- Campaign `2026-10-05-phase5-4521778` (verdict PASS): an 11-file corpus; every
  auto winner round-trips byte-exactly (`cmp` + `verify`).
- Cumulative ladder (serialized `.voldoc` bytes): A0 RAW = 81,371;
  A2 +BYTE_RANS = 48,598; A5 +PDF_LAYOUT = 48,598. Leave-one-out layout delta = 0;
  layout wins 0 of 8 classic-xref samples and is never the auto winner.
- Forced sizes: `many.pdf` layout-v2 **10,069** vs RAW **10,215** (layout now
  **beats RAW by 146 B** at scale) vs `BYTE_RANS` 5,181; `classic.pdf` layout 711
  vs RAW 663; `bigtext.pdf` layout 65,929 vs RAW 65,883 / `BYTE_RANS` 38,154.
- **Measured partial positive:** packed framing plus coalescing makes structural
  layout prediction beat RAW at document scale, but the residual data object is
  still stored literally, so an order-0 `BYTE_RANS` lane dominates it. The
  remaining lever is to entropy-code the residual (structural prediction
  composed with rANS on the residual), not to pack literals further. Receipt
  under `evidence/campaigns/2026-10-05-phase5-4521778/`.

### Notes

- The wire format remains **PROVISIONAL** and is not frozen v1. The packed op and
  layout-v2 are exact and bounded; layout-v2 is `RECORDED (rejected vs
  BYTE_RANS)` with the residual-entropy-coded form `PROPOSED` (Phase 6+). See
  ADR-0012.

## [0.1.0-alpha.5] — unreleased

### Added

- Phase 5 — PDF classic-xref layout proceduralization:
  - Positional DRA ops `MARK_OFFSET` (opcode `0x06`) and `EMIT_OFFSET`
    (opcode `0x07`), bumping the DRA graph to version `4`. `MARK_OFFSET` records
    the current output position into one of 256 bounded slots (slot `255`
    reserved for the most recent xref section start); `EMIT_OFFSET` emits a
    marked position as a fixed-width, zero-padded decimal field.
  - `PDF_LAYOUT` candidate: regenerates classic cross-reference entry offsets and
    the `startxref` value from marked object/section positions instead of storing
    the digits, with literal per-site fallback. Byte-exact and verified
    end-to-end before it is returned.
  - `encode --force` accepts `pdf-layout`.
- Phase-5 universe string
  `vole-document;universe;phase-5;exact-bytes;dra-4;opaque+entropy+pdf+channels+offsets`.
- Courts: `tests/pdf_layout.rs` and the forced-candidate ablation court
  `tools/phase5-court.sh`.

### Measured

- Campaign `2026-10-05-phase5-7193001` (verdict PASS): on a deterministic 10-file
  corpus every file round-trips byte-exactly through its auto-winning lane
  (`cmp` + `verify`), and the qpdf oracle re-check passes (classic 4/4, twopage
  6/6, incremental 5/5).
- Cumulative ladder (serialized `.voldoc` bytes): A0 RAW = 71,116;
  A1 +RLE = 71,116; A2 +BYTE_RANS = 43,377; A3 +PDF_PHYSICAL = 43,377;
  A4 +PDF_CHANNELS = 43,377; A5 +PDF_LAYOUT = 43,377. Leave-one-out layout
  delta = 0. Auto winners: RAW = 8, BYTE_RANS = 2, PDF_PHYSICAL = 0,
  PDF_CHANNELS = 0, PDF_LAYOUT = 0.
- Prediction works and is exact: `classic.pdf` regenerates 3 of 4 xref entry
  offsets plus the `startxref`; `incremental.pdf` regenerates 5 of 7 entries plus
  two `startxref` values. Forced sizes still lose: `classic.pdf` 798 vs RAW 659;
  `bigtext.pdf` 66,066 vs RAW 65,879 / `BYTE_RANS` 38,150. Layout wins 0 of 7
  classic-xref files.
- **Recorded negative result:** correct structural prediction does not pay while
  each predicted field needs its own framed DRA op; the per-segment
  `MarkOffset`/`EmitOffset` framing costs more than the ~7 digits saved per
  offset. `PDF_LAYOUT` stays implemented and available but is **rejected by the
  complete-cost court**. Receipt under
  `evidence/campaigns/2026-10-05-phase5-7193001/`.

### Notes

- The wire format remains **PROVISIONAL** and is not frozen v1. The positional
  ops and the layout lane are exact and bounded but lose on complete cost;
  amortizing the framing (e.g. a packed segment table) or applying prediction to
  very large / many-offset documents is `PROPOSED` (Phases 6+). See ADR-0011.

## [0.1.0-alpha.4] — unreleased

### Added

- Phase 4 — PDF lexical/structural typed channels:
  - Deterministic, exactly-reversible **channel transposition** (`split`/`join`):
    the Phase-3.1 lexical span cover is transposed into one kind id per token,
    one 4-byte length per token, and one concatenated payload stream per lexical
    kind (`KIND_COUNT = 12`). `join(split(x)) == x` by construction for every
    byte string.
  - `INTERLEAVE_CHANNELS` DRA op (opcode `0x05`), bumping the DRA graph to
    version `3`. It replays the kind/length sequence against the per-kind
    payload channels with bounded, non-overlapping output spans.
  - `PDF_CHANNELS` candidate: one order-0 byte-rANS model per channel, built on
    the fixed channel layout (ch0 kinds, ch1 lengths, ch2..13 per-kind
    payloads).
  - Compact **model wire v2**: sparse `[symbol u8][freq u16]` entries for present
    symbols or the dense 256-entry table, whichever serializes smaller; legacy
    v1 dense models remain decodable.
  - Forced-candidate ablation: `encode --force KIND` runs the same complete-cost
    court over a one-element candidate set (`raw`, `rle`, `byte-rans`,
    `pdf-physical`, `pdf-channels`).
- Phase-4 universe string
  `vole-document;universe;phase-4;exact-bytes;dra-3;opaque+entropy+pdf+channels`.
- Courts: `tests/pdf_channels.rs` and the forced-candidate ablation court
  `tools/phase4-court.sh`.

### Measured

- Campaign `2026-10-05-phase4-3840bc4` (verdict PASS): on a deterministic 10-file
  corpus, every file round-trips byte-exactly through its auto-winning lane
  (`cmp` + `verify`), and the qpdf oracle re-check passes.
- Cumulative ladder (serialized `.voldoc` bytes): A0 RAW = 71,036;
  A1 +RLE = 71,036; A2 +BYTE_RANS = 43,297; A3 +PDF_PHYSICAL = 43,297;
  A4 +PDF_CHANNELS = 43,297. Leave-one-out channel delta = 0. Auto winners:
  RAW = 8, BYTE_RANS = 2, PDF_PHYSICAL = 0, PDF_CHANNELS = 0.
- On the text-heavy scale sample `bigtext.pdf` (65,549 B) forced sizes were
  RAW = 65,871, BYTE_RANS = 38,142, PDF_CHANNELS = 46,432: typed channels beat
  RAW but lose to BYTE_RANS by ~8,290 B. Compact sparse models cut per-channel
  model overhead from 7,224 B to 1,981 B, which is not enough to close the gap.
- **Recorded negative result:** coarse lexical transposition plus per-channel
  order-0 models does **not** beat a monolithic order-0 `BYTE_RANS` on this
  corpus. `PDF_CHANNELS` is preserved as an exact, available lane but is
  **rejected by the complete-cost court**, not adopted. Receipt under
  `evidence/campaigns/2026-10-05-phase4-3840bc4/`.

### Notes

- The wire format remains **PROVISIONAL** and is not frozen v1. The typed-channel
  lane is exact and bounded but loses on complete cost; contextual/ordering
  mechanisms that could condition a channel on its neighbours are `PROPOSED`
  (Phases 5+). See ADR-0010.

## [0.1.0-alpha.3] — unreleased

### Added

- Phase 3 — byte-authoritative PDF physical authority:
  - Owned PDF lexical scanner (whitespace, comments, literal/hex strings with
    escapes and nesting, names, delimiters, regular tokens) producing a
    contiguous, non-overlapping span cover of `[0, len)`.
  - Conservative physical structure scanner: `%PDF-` header, `obj`/`endobj`,
    `stream`/`endstream`, `xref`, `trailer`, `startxref`, `%%EOF`, and comments;
    stream payloads are treated as opaque bytes when `/Length` is unusable.
  - Direct/indirect `/Length` resolution with CRLF/LF handling, and an
    append-only revision map (`/Prev` chain; `/Size` never decreases).
  - xref-stream and object-stream structural role detection.
  - Validated PDF detection (a `%PDF-` header **and** an indirect object **and**
    `%%EOF`); file extensions are never authority.
  - `PdfPhysical` view plus a `PDF_PHYSICAL` candidate that persists the physical
    partition as one literal `INLINE` op per span, with conservative opaque
    fallback. Span kinds are deterministic analysis metadata recomputable by
    `scan`.
- `source_format = 1` (PDF) and the Phase-3 universe string
  `vole-document;universe;phase-3;exact-bytes;dra-2;opaque+entropy+pdf`.
- Courts: `tests/pdf.rs` (total coverage, forced physical exactness, validated
  detection, revision mapping, hostile random bytes) and the qpdf differential
  oracle `tools/pdf-oracle.sh`.

### Measured

- Campaign `2026-10-05-phase3-486aa17` (verdict PASS): deterministic 9-item
  corpus (7 valid PDFs plus `malformed.pdf` and `notpdf.bin` negative controls).
  Coverage `all_covered = true` — 171 spans, 19 objects, 8 revisions; exactness
  `all_exact = true` (`materialize(descriptor) == original_bytes` for every
  item). qpdf 11.3 oracle: 100% object-number agreement (classic 4/4, two-page
  6/6, incremental 5/5), `qpdf --check` valid, `pdfinfo` pages 1/2/1. Court
  outcome: RAW won all 9 items and `PDF_PHYSICAL` won 0 — the expected Phase-3
  result, because the literal physical lane carries no structural compression
  yet (that is Phase 5). Receipt under
  `evidence/campaigns/2026-10-05-phase3-486aa17/`.

### Notes

- The wire format remains **PROVISIONAL** and is not frozen v1. The PDF physical
  authority is implemented and measured; PDF structural compression and
  xref/`/Length` proceduralization remain `PROPOSED` (Phases 5+).

## [0.1.0-alpha.2] — unreleased

### Added

- Phase 2 — native rANS floor (order-0 / typed byte channels):
  - Canonical integer-only entropy model (`MODEL`, 516-byte dense table over the
    byte alphabet) with deterministic normalization and validation.
  - Scalar order-0 byte rANS codec built on the `ryg-rans-rs` **safe manual**
    API; the panicking convenience decoder is not used.
  - Complete entropy **capsule** (model + decoder state + renormalization payload
    + counts) as a typed `ENTROPY_CHANNEL` (33-byte header + payload); never a
    scalar "seed".
  - `DECODE_CHANNEL` DRA op (DRA version 2) and materialization of channels.
  - `BYTE_RANS` and `RLE` candidates competing on complete serialized cost, with
    model bytes charged like any other bytes.
  - Feature `rans` (`default = ["rans"]`); `--no-default-features` keeps the exact
    RAW/RLE floor and returns an explicit `UnsupportedFeature` for
    channel-bearing descriptors instead of a silent reinterpretation.
- Phase-2 universe string
  `vole-document;universe;phase-2;exact-bytes;dra-2;opaque+entropy`.
- Courts: entropy acceptance gates (`tests/entropy.rs`), reference-oracle parity
  and frozen goldens (`tests/goldens.rs`), deterministic property/mutation
  fuzzing (`tests/property.rs`), and the soak script `tools/soak-fuzz.sh`.

### Measured

- Campaign `2026-10-05-phase2-f6af30b` (verdict PASS): on a 9-file mixed corpus,
  `sum_source = 590081`, `sum_core = 464474` (RAW + RLE), `sum_full = 291304`
  (RAW + RLE + BYTE_RANS), `delta = 173170`, fully attributed to the two
  `BYTE_RANS` wins. Negative controls hold (RAW on random, RLE on tiny); model
  cost is charged. This is a scoped order-0 result, not a general compression
  claim. Receipt under `evidence/campaigns/2026-10-05-phase2-f6af30b/`.

### Notes

- The wire format remains **PROVISIONAL** and is not frozen v1. PDF and other
  format-aware mechanisms remain `PROPOSED` (Phases 3+).

## [0.1.0-alpha.1] — unreleased

### Added

- Exact `.voldoc` container core (Phase 1):
  - 64-byte fixed header with CRC-32C self-check and fail-closed version/feature
    negotiation.
  - Length-delimited records with per-record CRC-32C.
  - Typed errors with stable, documented CLI exit codes.
  - Centralized resource [`Limits`].
  - SHA-256 whole-source archival identity.
- Document Reconstruction Algebra literal subset (`EMIT_OBJECT`, `INLINE`,
  `REPEAT_LAST`) with a derived **coverage certificate** validated before
  allocation.
- RAW exact opaque adapter (the correctness floor for every file type).
- Complete-cost candidate court with decode-before-commit: the winner is priced
  from its serialized bytes and byte-compared against the source.
- CLI: `encode`, `decode`, `materialize`, `verify`, `inspect`, `capabilities`.
- Docker-only, digest-pinned toolchain gates: stable (Rust 1.99.0), MSRV
  (Rust 1.89.0), and a PDF oracle tools image (qpdf/Poppler/MuPDF/Ghostscript).
- Courts: exact, malformed (hostile input), conformance.
- Phase 0 research synthesis and ADRs.

### Notes

- No compression headline was claimed in Phase 1. rANS and format-aware
  mechanisms were `PROPOSED` (Phases 2+), not `ADOPTED`.
