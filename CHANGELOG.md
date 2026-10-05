# Changelog

All notable changes are recorded here. The format is pre-1.0 and provisional.

## [0.1.0-alpha.8] — unreleased

### Added

- Phase 6 — exact DEFLATE replay (`preflate-rs`):
  - `DEFLATE_REPLAY` DRA op (opcode `0x0A`), bumping the DRA graph to version
    `8`. It begins with a `replay_codec` tag (`REPLAY_DEFLATE_PREFLATE_0_7_6`),
    which names the correction representation as an **experimental,
    version-coupled** preflate-0.7.6 layout (not frozen v1); an unknown codec fails
    closed as `UnsupportedFeature` (never `InvalidGraph`). It emits exactly
    `recreate_whole_deflate_stream(plaintext, corrections)` — the **raw** DEFLATE
    bytes (RFC 1951, no zlib wrapper) — with a `declared_output_len` statically
    rejected above the VOLE replay-profile admission limit (a policy bound: RFC 1951
    permits unbounded empty non-final blocks, so no finite `f(decompressed_size)`
    bound exists) *before* the engine runs (ADR-0016) and validated at evaluation;
    plaintext and corrections are bounded
    by `max_record_len`. `source_kind` is `0` (plaintext
    from the object table) or `1` (plaintext from an entropy channel).
    Reconstruction is isolated with `catch_unwind`, so hostile corrections yield a
    typed `CodecReplay`/`InvalidGraph`, never a panic or a silent success. A
    mandatory `FEATURE_DEFLATE_REPLAY` bit is declared whenever the op is present;
    a build without the feature fails closed with `UnsupportedFeature`.
  - A lexer change: `stream` followed by EOL now makes the payload an **opaque
    span**, so a PDF's stream-data bytes are a byte-authoritative span and a lone
    `/FlateDecode` is classified from the object dictionary. `preflate` never
    decides stream boundaries.
  - `PDF_DEFLATE_REPLAY` candidate: physical span order; each eligible stream span
    becomes `INLINE(zlib header) · DEFLATE_REPLAY · INLINE(Adler-32)`, with the
    plaintexts and correction blobs as content-deduplicated objects.
  - `PDF_DEFLATE_REPLAY_RANS` candidate: each **unique** plaintext is one order-0
    byte-rANS `ENTROPY_CHANNEL`, shared by every stream that produces it, so
    shared plaintext is stored once and decoded once; corrections stay raw
    deduplicated objects.
  - `encode --force` accepts `pdf-deflate-replay` and `pdf-deflate-replay-rans`.
  - Dependency `preflate-rs = "=0.7.6"` under the **opt-in** `deflate-replay`
    feature (`default = ["rans"]`; enable with `--features deflate-replay`),
    which transitively pulls LGPL-3.0-or-later `cabac` (ADR-0014). The default
    build is permissive-only. `flate2` (dev-only) uses the `zlib-rs` backend.
- Phase-6 universe string
  `vole-document;universe;phase6;exact-bytes;dra-8;opaque+entropy+pdf+channels+offsets+packed+packed-channels+deflate-replay-preflate-0.7.6-experimental`.
- Courts: the Phase-6 replay gates in `tests/pdf_deflate.rs` and the
  forced-candidate ablation court `tools/phase6-court.sh` over the 12-file
  corpus.
- Phase 7.0 — producer-stratified Flate correction-ratio harness:
  - `vole-document deflate-stats INPUT...` emits, per `FlateDecode` stream,
    `compressed_bytes`/`plaintext_bytes`/`correction_bytes`/`rans_plaintext_bytes`
    and the ratios `correction/compressed`, `(plaintext+corr)/compressed`,
    `(rans(plaintext)+corr)/compressed`, plus replayed/declined counts. It is
    **diagnostics only** and changes no wire format or candidate behavior.
  - The summary reports the rANS complete cost two ways so shared plaintext is
    not overcounted: `replayed_rans_full_bytes` (naive per-stream sum) and
    `replayed_rans_dedup_bytes` (one charge per **unique** plaintext plus one per
    **unique** correction blob, mirroring the shared-channel candidate).
  - `tools/pdf-corpus.sh` builds a locally-generated corpus from distinct
    producer lineages (Ghostscript 10.00.0 at `/default`/`/prepress`/`/printer`/
    `/ebook`/`/screen`, qpdf 11.3.0 compress/linearize/object-streams=preserve/
    nocompress, a hand-written stored-block-zlib base, plus the Phase-3 synthetic
    set), validating every produced PDF with `qpdf --check` and writing a
    provenance ledger (`provenance.json`). The regenerable `.pdf` bytes are
    gitignored; no third-party bytes.
  - Harness tests in `tests/deflate_stats.rs`.
  - `tools/pdf-court.sh` runs the real complete-cost court (`encode FILE OUT` and
    `encode --force raw|byte-rans|pdf-deflate-replay|pdf-deflate-replay-rans FILE
    OUT`) over every corpus file and records each lane's complete serialized
    `.voldoc` size; a kind the input does not propose is a typed `Usage` decline
    recorded as `null`, and the auto winner is `verify`ed and `decode`d+`cmp`ed
    byte-exact.
  - `tools/pdf-corpus.sh` now passes `--deterministic-id` to every qpdf
    invocation, so the qpdf corpus outputs are byte-reproducible across runs.
    Ghostscript `pdfwrite` output is not (it embeds a per-run `/ID` and
    timestamp); the caveat is recorded in `provenance.json`.

### Fixed

- PDF lexer stream boundary (`src/adapter/pdf/lexer.rs::find_endstream`): the
  `endstream` keyword is now located by **right-termination** (the byte
  immediately after `endstream` must be PDF whitespace, a PDF delimiter, or EOF)
  instead of requiring a preceding CR/LF. Real producers — confirmed for
  **Ghostscript 10.00.0** — write the stream payload directly before `endstream`
  with **no intervening EOL**, which previously made the opaque payload span
  over-read to a *later* `endstream`, swallowing whole stream objects and
  mis-slicing the `/Length` region (the captured bytes began `0x0a`, so
  `try_replay` declined them as `not_zlib`). The physical scanner's
  `/Length`-based resolution is unchanged; no wire format or candidate changed.
  New lexer tests cover the no-EOL case, the normal `\nendstream` case, an
  `endstream` at EOF, and an `endstream`-like run lacking a right terminator; a
  new integration test builds a PDF whose payload abuts `endstream` and checks
  the exact scan and byte-exact `try_replay`.

### Measured

- Campaign `2026-10-05-phase7-corpus-f1f8d26` (verdict RECORDED; diagnostic, no
  new candidate): 17 `FlateDecode` streams, **11 replayed / 6 declined**
  (acceptance 0.647). The 6 transformer declines were `not_zlib` from a recorded
  **scanner locality limitation** (the lexer required an EOL before `endstream`,
  which Ghostscript omits), not a zlib verdict. Superseded by the amendment
  below.
- Campaign `2026-10-05-phase7-corpus-b-c4eb77e` (amendment; verdict RECORDED;
  diagnostic, no new candidate) — re-measured after the lexer stream-boundary
  fix: the same corpus now yields **24 `FlateDecode` streams, 24 replayed / 0
  declined (acceptance 1.000)** across Ghostscript 10.00.0, qpdf 11.3.0, a
  hand-written stored-block-zlib base, and the Phase-3 synthetic set. The census
  rose 17 → 24 because the old over-read had swallowed whole stream objects
  (every qpdf-generated file was undercounted; `qpdf-preserve-objectstreams.pdf`
  had reported zero). The Phase-6 win region appears **only in our own
  hand-authored fixtures** (`hand-base2.pdf` deduped rANS 55,531 vs naive
  111,062; `flate.pdf` 34,051 vs 89,437). `qpdf-preserve-objectstreams.pdf`
  shows the same 111,062 → 55,531 geometry only because qpdf copied and
  renumbered the fixture's two byte-identical raw streams — the fixture already
  wins 112,011 → 56,885 and qpdf adds +41 B, so **99.93% of that win is
  inherited**, not produced by a transformer. Corpus-wide `correction/compressed`
  p50 is **0.014716** (p10 0.000320 / p90 0.097360); `0.004518` is the
  `pdf-make-samples` subset median only. Scoped to locally generated files;
  qpdf/Ghostscript are transformers, not authoring apps; browser/office/TeX
  families remain a recorded gap. Report: `docs/evidence/phase7-corpus-report.md`.
- Campaign `2026-10-05-phase7-court-99dc72e` (verdict RECORDED; measurement, no
  new candidate) — the decisive **complete-cost court** over the 23-file locally
  generated corpus, running `encode` and the forced lanes
  (`raw`/`byte-rans`/`pdf-deflate-replay`/`pdf-deflate-replay-rans`) and comparing
  complete serialized `.voldoc` sizes. `PDF_DEFLATE_REPLAY_RANS` vs `BYTE_RANS`:
  **win 3 / lose 8 / decline 12 — all 3 wins self-authored.** The wins are exactly
  the Phase-6 shared-plaintext geometry: `hand-base2.pdf` (112,011 → 56,885,
  −55,126 B), `_synthetic/flate.pdf` (49,291 → 36,102, −13,189 B), and
  `qpdf-preserve-objectstreams.pdf` (112,147 → 56,980, −55,167 B) — the last only
  because qpdf `--object-streams=preserve` copied the fixture's two byte-identical
  raw streams; the fixture already wins 112,011 → 56,885 B and qpdf adds +41 B, so
  **99.93% of that win is inherited** and it is **not** a real-producer result.
  Every **genuinely transformed** producer output loses or declines: all 5
  Ghostscript variants and both qpdf compression variants lose, and the 12 files
  with no replayable Flate lane decline. The Phase-6 win is real and byte-exact,
  but its enabling condition (shared plaintext) is **not produced by the tested
  transformers**, motivating Phase 7.2 (nested content proceduralization). Every
  auto winner is `verify`'d and `cmp`'d byte-exact (23/23). `--deterministic-id`
  is a `/ID`-only normalization (55,165 B no-flag vs 55,167 B flagged). The corpus
  is locally generated and is **not** a population sample; qpdf/Ghostscript are
  transformers. Receipt under `evidence/campaigns/2026-10-05-phase7-court-99dc72e/`.
- Campaign `2026-10-05-phase6-0d0bb79` (DRA v8, verdict PASS; the earlier
  `2026-10-05-phase6-ec92c1a` receipt is retained): a 12-file corpus; every
  auto winner round-trips byte-exactly (`cmp` + `verify`, `all_exact=true`); the
  qpdf differential oracle re-checks clean.
- **On `flate.pdf` (57,513 B) the shared-plaintext rANS replay lane is the auto
  winner at 36,102 B, a 13,189 B win over `BYTE_RANS` (49,291 B)** — the first
  measured positive for a PDF structural candidate. The raw-plaintext variant
  `PDF_DEFLATE_REPLAY` = 56,736 B loses to `BYTE_RANS` (7,445 B larger), and RAW =
  57,908 B.
- The sample exposes 6 FlateDecode streams but only **3 unique plaintexts**; the
  rANS lane stores 3 plaintext channels (only `p1`, across four streams, is
  shared) plus 6 correction objects.
- Cumulative ladder: A0 RAW = 139,950; A2 +`BYTE_RANS` = 98,560;
  A6 +`PDF_LAYOUT_RANS` = 98,560; A7 +`PDF_DEFLATE_REPLAY` = 98,560;
  A8 +`PDF_DEFLATE_REPLAY_RANS` = **85,371**. Leave-one-out delta for the rANS
  replay mechanism = **−13,189**; auto winners RAW = 8, `BYTE_RANS` = 3,
  `PDF_DEFLATE_REPLAY_RANS` = 1.
- Head-to-head `PDF_DEFLATE_REPLAY_RANS` vs `BYTE_RANS`: **win 1, lose 0,
  decline 11** (only `flate.pdf` has a lone FlateDecode stream; every decline is
  recorded verbatim, never scored).
- **Scoped result.** One composed sample at commit `0d0bb79`: the winning region
  is a plaintext that is *shared across streams* **and** *also* has a
  large/weakly-coded appearance (neither sharing alone nor weak coding alone
  wins); the losing region is unique, strongly-compressed plaintext, where the
  plaintext is no smaller than the bitstream it replaces. Receipt under
  `evidence/campaigns/2026-10-05-phase6-0d0bb79/`.

### Notes

- The wire format remains **PROVISIONAL** and is not frozen v1. `DEFLATE_REPLAY`
  (DRA v8; `replay_codec`-tagged, statically resource-bounded — ADR-0016) is exact
  and bounded; `PDF_DEFLATE_REPLAY_RANS` is `ADOPTED` as a winning candidate
  **when shared plaintext is present** (the candidate is implemented and proposed
  for every lone-`FlateDecode` input; the tested evidence for the enabling
  condition is our self-authored fixtures, not real producer output) and
  `PDF_DEFLATE_REPLAY` is `RECORDED (rejected vs
  BYTE_RANS)`. This is the first PDF structural candidate to beat a whole-file
  order-0 rANS lane; the earlier converging negatives (ADR-0010–ADR-0013) apply
  to proceduralizing *plain* syntax, while exact replay attacks bytes that are
  *already* entropy-coded (ADR-0015).
- The `deflate-replay` feature is **opt-in**: the default build is
  `default = ["rans"]` (permissive-only), and `--features deflate-replay` (or
  `--all-features`) enables the lane; a build without it rejects the op with
  `UnsupportedFeature` (see ADR-0014 for the LGPL consequence).
- The Phase-7.0 complete-cost court over the producer corpus does not change any
  adoption: it confirms the Phase-6 win region under complete cost on 3/23
  locally generated files, **all three self-authored** (two shared-plaintext
  fixtures plus a `--object-streams=preserve` copy of one). Every genuinely
  transformed producer output loses or declines, so the enabling condition
  (shared plaintext) is not produced by the tested transformers. It is scoped to
  locally generated files and makes no population claim; qpdf and Ghostscript are
  transformers, not authoring applications.

## [0.1.0-alpha.7] — unreleased

### Added

- Phase 5.8 — layout + rANS residual:
  - `PACKED_CHANNELS` DRA op (opcode `0x09`), bumping the DRA graph to version
    `6`. It reconstructs output from a **data** entropy channel interpreted by a
    serialized item table carried in a **plan** entropy channel, with a declared
    output length validated at evaluation. Unknown tags, truncation, unmarked
    slots, bad widths, literal overruns, missing channels, a declared-length
    mismatch, and unconsumed data are rejected with typed errors before an inexact
    result is ever returned.
  - `PDF_LAYOUT_RANS` candidate: codes the layout plan's literal data object
    (channel 0) and `encode_items` of its item table (channel 1) each as their own
    order-0 byte-rANS channel with its own model, reconstructed by one
    `PACKED_CHANNELS` op. Byte-exact and verified serialize → parse → materialize →
    byte-compare before it is returned.
  - `encode --force` accepts `pdf-layout-rans`.
- Phase-5.8 universe string
  `vole-document;universe;phase5-8;exact-bytes;dra-6;opaque+entropy+pdf+channels+offsets+packed+packed-channels`.
- Courts: layout+rANS gates in `src/adapter/pdf/layout.rs`, the `PACKED_CHANNELS`
  evaluator gates in `src/dra/program.rs`, and the Phase-5.8 forced-candidate
  ablation court `tools/phase5-8-court.sh` over the 11-file corpus.

### Measured

- Campaign `2026-10-05-phase5-8-cf8048d` (verdict PASS): an 11-file corpus; every
  auto winner round-trips byte-exactly (`cmp` + `verify`).
- Cumulative ladder (serialized `.voldoc` bytes): A0 RAW = 81,591;
  A2 +BYTE_RANS = 48,818; A6 +PDF_LAYOUT_RANS = 48,818. Leave-one-out layout+rANS
  delta = 0; layout+rANS wins 0 of 8 head-to-head comparisons and is never the
  auto winner.
- Forced sizes: `classic.pdf` RAW **683** / `BYTE_RANS` **728** / `PDF_LAYOUT`
  **731** / `PDF_LAYOUT_RANS` **883**; `bigtext.pdf` RAW **65,903** /
  `BYTE_RANS` **38,174** / `PDF_LAYOUT_RANS` **38,341**; `many.pdf` RAW **10,235**
  / `BYTE_RANS` **5,201** / `PDF_LAYOUT` **10,089** / `PDF_LAYOUT_RANS` **5,914**
  (breakdown: data 7,877, plan 1,815, models 645, payload 4,775).
- **Recorded negative result:** head-to-head against `BYTE_RANS` the layout+rANS
  candidate wins 0, loses 8, and is declined by 3 files. Channel 0 codes nearly
  the whole file — the same job `BYTE_RANS` does with one channel — while the plan
  channel plus a second model are added metadata `BYTE_RANS` never pays.
  `PDF_LAYOUT_RANS` stays implemented and available but is **rejected by the
  complete-cost court**. Receipt under
  `evidence/campaigns/2026-10-05-phase5-8-cf8048d/`.

### Notes

- The wire format remains **PROVISIONAL** and is not frozen v1. `PACKED_CHANNELS`
  (DRA v6) is exact and bounded; `PDF_LAYOUT_RANS` is `RECORDED (rejected vs
  BYTE_RANS)`. This is the fourth converging negative (ADR-0010, ADR-0011,
  ADR-0012, ADR-0013): at the tested scale, PDF structural proceduralization does
  not beat a whole-file order-0 rANS lane.

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
