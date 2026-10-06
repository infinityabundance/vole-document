# Project state and mechanism ledger

This is the single authoritative status table. "Implemented" and "proven" are
**not** interchangeable: a mechanism is `ADOPTED` only after its predeclared gate
passes and a sealed campaign exists. Failed hypotheses stay in the history — they
are evidence.

## Status vocabulary

**Current release:** `0.1.0-alpha.16` (Phase 12 — universal multi-format
document field: PDF + DOCX + EPUB). **Top-level verdict (ADR-0023, the
authoritative [`FINDINGS.md`](../FINDINGS.md)):** the current representation stack
does not beat purpose-built baselines on any measured axis; the durable results
are byte-exactness, an auditable representation, and the recorded negatives; the
only surviving wins are scoped (partial-decode CPU / bytes-read versus
non-seekable codecs, and whole-object dedup of identical opaque files, marginal
vs CDC). Exactness is unchanged; whole-file compression is a recorded loss
against generic lossless tools (ADR-0017). Phase 7 measured a scoped
random-access decode-CPU win (ADR-0018); Phase 8 measured a scoped random-access
bytes-read win **versus non-seekable sequential codecs** (ADR-0019); Phase 9 adds
the store and records a robust but partly externalization-granularity-artifact
loss on the store axis (ADR-0020/0021); Phase 10.1 adds an optional encoder-only
search governor that buys no bytes on its cohort (ADR-0022); Phase 11 adds a
persistent procedural field over the exact descriptor whose courts are
deliberately mixed (some wins, several losses and corrections); it changes no
exactness semantics and none of the ADR-0023 verdict above.

Phase 8 (branch `phase8`, ADR-0019) adds an optional seek `DIRECTORY` record and
a `Read + Seek` reader. On the same 33.8 MB corpus a seeked `view` reads a
constant **~0.44–0.46 MB** regardless of offset — a scoped **bytes-read win
versus non-seekable sequential** gzip/zstd/xz prefixes for mid/late queries.
It is **not** a general random-access-I/O win: a purpose-built seekable/blocked
format reads **2.6–30× less** for the same late query (bgzip ~24 KB; blocked xz
15–180 KB), and it loses at offset 0 and early queries. Whole-file size is
3.01× xz (and 0.46× BGZF); the offset-independent floor remains an honest
caveat (`docs/evidence/phase8-skeptic-review.md`).

Phase 9 (branch `phase9`, ADR-0020) adds a cross-document **content-addressed
store**. Two digests are kept distinct: SHA-256 stays the whole-source *archival*
identity, while `Id = BLAKE3-256(object bytes)` is the relational store namespace.
A store-backed descriptor form (`EXTERNAL_REF`, tag `0x80`, mandatory
`FEATURE_EXTERNAL_OBJECTS`) makes the object table inline-or-external; conversion
both ways (`externalize`/`hydrate`) must materialize identical bytes, and closure
/GC correctness is enforced. `EmbeddedStore` (a raw content-addressed directory)
is the reference backend; `EntropyFsStore` (the embeddable EntropyFS engine,
non-default feature `entropyfs-store`) is a verified optional adapter whose
`list`/`remove` decline because the engine exposes no per-blob delete. Three
accounting universes are kept permanently distinct (standalone / unique-reachable
/ amortized, with the amortized rule fractional by reference count and
`Σ amortized == unique reachable`). Phase 9.3 then **measured** the axis (campaign
`2026-10-05-phase9-store-fdb2845`, ADR-0021): over 37 locally generated files
(5,579,469 B, 10 deliberate-sharing strata) the unique-reachable universe
`U = 3,369,900 B` **loses** to per-file min LZ (`1,304,307 B`) and to the strongest
pinned content-defined-chunk dedup (borg 1.2.4, `771,383 B` raw deterministic /
210,835–210,840 B compressed non-deterministic). The negative is **robust** — a
forced `--force pdf-deflate-replay` run gives `U = 2,360,054 B` and a per-stratum
oracle ~`2,537,730 B`, both still losses — but its size is **partly an artifact of
externalization granularity / candidate selection**: the auto winner emits 0–1
objects per file. Under the auto candidate the only win is byte-identical opaque
repeats (`repeat-bin`, `U = 133,048 B`); forcing `PDF_DEFLATE_REPLAY` (one object
per deflate stream) flips `shared-payload` to `U = 264,139 B`, a win over *raw*
CDC (`285,257 B`) that still loses to LZ (`34,591 B`) and compressed CDC
(`28,195 B`). `shared-bin`, `one-changed`, `incremental`, `reexport`, `repeat-pdf`
and the pre-registered `shifted` control all lose to CDC, because the auto
winner's bulk lives in entropy channels / `GRAPH` bytes that `externalize` does
not share; no current candidate emits more than one object per file. Cross-document
sharing is reported as store *amortization*, never "compression"
(`docs/evidence/phase9-skeptic-review.md`).

Phase 10.1 (branch `phase10`, ADR-0022) adds an optional, **encoder-only** search
governor behind the non-default, **dependency-free** feature `dsfb-search = []`
(**not** `["dep:dsfb"]`). The published `dsfb 0.1.2` crate was inspected
empirically: it is *real*, builds under the pinned MSRV, and is present in the
lockfile **only transitively** as a non-optional dependency of the optional
EntropyFS engine — but its own manifest defines it as **"Drift-Slew Fusion
Bootstrap (DSFB) state estimation"**, a `f64`/`rand` observering over sensor
channels, with **no** candidate-family / residual / search-directive API; it is
therefore *unavailable-for-purpose* and is not a dependency of `dsfb-search`.
The governor defines typed, integer-only residual diagnostics, a tiny parametric
candidate space over *existing* mechanisms (`scale_bits`, `partition`, `replay`,
`packed`, `depth`), and a pure `govern` function; every candidate it can enable
reaches the *same* complete-cost court, and no governance state is persisted (no
wire change, `header.rs` untouched, no feature bit). Its court (campaign
`2026-10-05-phase10-governor-d2b09c9`) records a **negative**: `DsfbGuided` never
exceeds `FixedHeuristic` (H1) and matches the exhaustive grid minimum on 8/8
holdout with ≤ ½ the candidates (H2), negative controls `Stop(Raw)` byte-exactly
(H4) — but the fixed heuristic already attains the exhaustive minimum on **every**
workload, so the parametric search adds **zero** bytes (H3). The fixed
complete-cost court is retained and the governor remains an optional encoder
feature. Zero decode authority is proven, not asserted: a governor-produced
descriptor decodes byte-exactly in a default build **without** the feature.

Phase 11 (branch `phase11`, ADRs 0024–0028) adds a **persistent procedural
document field** over the exact `.voldoc` descriptor: a canonical, immutable
procedural seed DAG (`NodeId = BLAKE3-256("VOLE:PSEED:v1" + state + dep ids)`,
one `FsSeedStore` file per node, optional `EntropyFsSeedStore`), a bounded
advisory hierarchical observation index (`HIER_INDEX = 0x72`, fail-closed on a
lying/cyclic/out-of-closure index), and a typed observation query engine with
provenance (`basis` ∈ {authored, directly-observed, deterministically-derived,
inferred, heuristic, unresolved}) and `EXPLAIN` / `EXPLAIN ANALYZE`. Exactness
is unchanged: with the source removed and in a new process every court case
`materialize --exact`s with matching length + SHA-256 + `cmp` (campaign
`2026-10-06-phase11-63f43fb`). The courts are deliberately mixed and honest: the
field's **seed class** is 449 B cold / 0 B warm, but the honest total cold
observation is 232–364 KB (descriptor closure + index), so "bounded" is
page-closure-bounded, **not** O(1); cross-process reuse is real across an OS
process but **served by the disposable derived cache**, not seed-DAG
recomputation; the warm descriptor-free 8.4–8.9 KB is an **overhead-only** figure
(the warm process also reads the cached answer payload), so the warm byte win
over A1 is withdrawn on the large cases while the warm wall win and the vs-A0
win stand; the fair A1 preprocessed-SQLite baseline wins the narrow-query byte
court (24,393 B) and its wall crossover is absent on 3 of 5 documents; the
pinned-tokenizer (`bert-base-uncased`, offline, hash-verified at court time)
token court is 2 win / 2 tie / 2 loss vs page-local Poppler; the immutable edit
witness shares the descriptor and unaffected nodes **by content id** (0
descriptor bytes read) within a declared narrow subset; and finer-than-object
shareable units **lose to content-defined chunking** (208,001 B unique lower
bound vs 160,668 B CDC and 92,752 B `tar` + `xz -9e`; ADR-0028). Phase 11.13 is
an independent adversarial skeptic review that corrected the overreach in place
(F1–F8, `docs/reviews/phase-11-skeptic-review.md`).

Phase 12 (branch `phase12`, ADRs 0029–0035) makes the persistent procedural field
**format-universal**: PDF, DOCX and EPUB enter through separate native inverse
compilers — the Phase-11 PDF adapter, a WordprocessingML (DOCX) inverse and a
bounded-XHTML/OCF (EPUB) inverse — over a shared **byte-authoritative ZIP** layer
(physical member scanner + OPC/OCF package graph), converging on one
`DocumentField` that exposes **common** observations and retained **format-native**
structure with per-answer provenance (`format=<fmt>;common;<native>`). Exactness is
unchanged (`materialize == original`; length + SHA-256 + `cmp`, source and
descriptor deleted, fresh process). The courts are deliberately mixed: DOCX/EPUB
are byte-exact after source **and** descriptor deletion (38/38) and the triplet is
96/96; hostile input is 315/315 (16 reject / 22 opaque-preserve-and-decline / 7
accept, 45 fixtures) with the 12.14 demo sealed; the required ablation ladder
attributes the small-document win to the **content adapters** (A4→A5) and to
**persistent semantic reuse** (A5→A6, which trades bytes for CPU), with **EntropyFS
(A9) a loss**, `A7`/`A8` not separable, and the ZIP/OPC rungs invisible on the
frozen schedule. The lifetime result is honest and mixed: VOLE wins the
small-document frontier and the cold one-time comparison, but the source-retaining
SQLite+FTS5 baseline (A1) wins the large ~60 KB synthetic documents' byte frontier
(N=10–1000) and wall/CPU at N=1000; the LLM working set is 1 win / 8 tie / 3 loss
vs page-local B1 and 9 win / 0 tie / 3 loss vs the whole document (pinned
`bert-base-uncased`). **Cross-document durable work reuse is a recorded negative
(`N3`):** the warm `retained_inverse_work_fraction` 0.339907 falls to **0.0** after
`cache --clear` (in-process and fresh process), while representation identity
remains shared; the `N6` PDF no-regression gate is **closed** (A2 vs A11: 32/32
byte-exact, 0 regressions). An independent adversarial review
(`docs/reviews/phase-12-skeptic-review.md`) found, and the close-out corrected, the
security class tally, the unmeasured FTS5 claim, the un-run ablation ladder, the
missing PDF/reuse/demo receipts, and a default-feature clippy failure.

`PROPOSED` → `PROTOTYPED` → `IMPLEMENTED` → `MEASURED` → `ADOPTED`
(or `RECORDED` / `REJECTED` / `STOPPED` / `SUPERSEDED` / `PARTLY DELIVERED`).

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
| PDF `/Length`/revision proceduralization (as a **size** mechanism) | 6+ | PROPOSED | structural compression beyond xref offsets is still unmeasured; Phase 11 persists revisions as *observation* nodes (`PdfRevision`) — recovered and queryable, but **not** a size candidate |
| Lexer stream opacity (`stream`+EOL is an opaque span) | 6 | ADOPTED | campaign `2026-10-05-phase6-0d0bb79`; stream-data bytes are a byte-authoritative span, so `/FlateDecode` stream spans are exact and `preflate` never discovers streams; 12/12 corpus files still round-trip |
| `DEFLATE_REPLAY` DRA op (DRA v8) | 6 | IMPLEMENTED | opcode `0x0A`; explicit `replay_codec` tag (`REPLAY_DEFLATE_PREFLATE_0_7_6`, an experimental version-coupled `preflate` layout) that fails closed on an unknown id; emits the exact raw DEFLATE bitstream from `(plaintext, corrections)` with a declared output length statically rejected above the VOLE replay-profile admission limit `min(max_output_bytes, max_replay_bytes, 2*P+1024)` before the engine runs (ADR-0016; a policy bound, not an RFC 1951 maximum) and validated at eval; plaintext/corrections bounded by `max_record_len`; `catch_unwind`-isolated; mandatory feature bit (opt-in `deflate-replay` cargo feature); universe → `phase6;…;dra-8;…+deflate-replay-preflate-0.7.6-experimental` |
| Exact DEFLATE replay, raw plaintext (`PDF_DEFLATE_REPLAY`) | 6 | RECORDED (rejected vs `BYTE_RANS`) | campaign `2026-10-05-phase6-0d0bb79`; byte-exact, but on `flate.pdf` 56,736 vs `BYTE_RANS` 49,291 (the plaintext is nearly as large as the bitstream it replaces) |
| Exact DEFLATE replay, shared rANS plaintext (`PDF_DEFLATE_REPLAY_RANS`) | 6 | ADOPTED | campaign `2026-10-05-phase6-0d0bb79`; `flate.pdf` 36,102 vs `BYTE_RANS` 49,291 (**−13,189 B**); 3 plaintext channels (one shared) for 6 streams; leave-one-out delta −13,189; **first measured positive for a PDF structural candidate** (ADR-0015); the candidate is implemented and can win **when shared plaintext is present**, but the Phase-7.0 corpus shows that enabling condition is **not produced by the tested transformers** — its only Phase-7.0 witnesses are our self-authored fixtures (and a `--object-streams=preserve` copy of one); Phase 7.0b's Cairo delta (58,711 → 34,574 B) is a repeated-identical-bytes **harness artifact** — our generator repeated one identical page six times and Cairo emitted six streams byte-identical in *compressed* bytes as well as plaintext — so it witnesses a region generic LZ compresses ~2× better (gzip -9 17,382 B; xz -9e 16,852 B), **not** the shared-plaintext-vs-distinct-compression mechanism; the enabling condition remains unproven on real authoring output (see `docs/evidence/phase7b-skeptic-review.md`) |
| Producer-stratified Flate correction-ratio harness (`deflate-stats`) | 7.0 | MEASURED | amendment campaign `2026-10-05-phase7-corpus-b-c4eb77e` (supersedes `2026-10-05-phase7-corpus-f1f8d26`, not rewritten); after the lexer stream-boundary fix, **24 Flate streams, 24 replayed / 0 declined (acceptance 1.000)** from Ghostscript 10.00.0, qpdf 11.3.0, a hand-written stored-block-zlib base, and the Phase-3 synthetic set; the 6 former `not_zlib` declines are gone and the stream census rose 17 → 24 (the old over-read had swallowed whole stream objects, e.g. `qpdf-preserve-objectstreams.pdf` reported zero); the Phase-6 win region appears **only in our hand-authored fixtures** (`hand-base2.pdf` deduped rANS 55,531 vs naive 111,062; `_synthetic/flate.pdf` 34,051 vs 89,437); `qpdf-preserve-objectstreams.pdf` shows the same geometry only because qpdf copied and renumbered the fixture's two byte-identical raw streams (`ec028dc1…`), so **99.93% of that win is inherited**, not produced by a transformer; corpus-wide `correction/compressed` p50 = **0.014716** (the `0.004518` is the `pdf-make-samples` subset median only); **no new candidate** |
| Complete-cost court over the producer corpus (`PDF_DEFLATE_REPLAY_RANS` vs `BYTE_RANS`) | 7.0 | RECORDED (measurement, no new candidate) | campaign `2026-10-05-phase7-court-99dc72e`; over 23 locally generated files (Ghostscript 10.00.0 ×5, qpdf 11.3.0 ×4, hand ×2, `pdf-make-samples` ×12) the real CLI court gives replay-rANS **win 3 / lose 8 / decline 12** vs `BYTE_RANS`. Wins are exactly the Phase-6 shared-plaintext geometry, **all self-authored**: `hand-base2.pdf` −55,126 B (112,011 → 56,885), `_synthetic/flate.pdf` −13,189 B, and `qpdf-preserve-objectstreams.pdf` **−55,167 B** (112,147 → 56,980) — the last is a `qpdf --object-streams=preserve` **copy** of `hand-base2.pdf`'s two byte-identical raw streams, **99.93% inherited**, not a transformed-producer win. Every genuinely transformed producer output loses or declines: it loses on all 5 Ghostscript variants and both qpdf compression variants (unique strongly-compressed plaintext; 8 losses total) and declines on the 12 files with no replayable Flate lane. The Phase-6 win is real and byte-exact, but its enabling condition (shared plaintext) is **not produced by the tested transformers**, motivating Phase 7.2. All 23 auto winners `verify` + `cmp` byte-exact. qpdf corpus outputs are byte-reproducible (`--deterministic-id`); Ghostscript outputs are not (per-run `/ID`). **No population claim**; qpdf/Ghostscript are transformers |
| Generator-family Flate corpus (`producers` image) | 7.0b | MEASURED | new opt-in `producers` Docker stage/service (`debian:bookworm-slim@sha256:3783cc01…`, same digest as `tools`; `vole-document/producers:bookworm`, ~724 MB, kept out of the fast `tools` gate); real *authoring* generators with distinct DEFLATE: **ReportLab 3.6.12**, **Cairo 1.20.1/libcairo 1.16.0**, **LibreOffice Writer 7.4.7.2**, **pdfTeX 3.141592653-2.6-1.40.24** (all four actually run; none skipped), plus qpdf 11.3.0; generator `tools/pdf-corpus-producers.sh` fingerprints every `/FlateDecode` payload and applies `qpdf --deterministic-id --stream-data=preserve --object-streams=preserve` only if it leaves every payload byte-identical; ReportLab/Cairo/pdfTeX byte-reproducible, LibreOffice not (run-varying metadata). **Exact-replay acceptance 87/87 = 1.000**; corpus-wide `correction/compressed` p50 = 0.047945. All `.pdf` bytes gitignored; ledger `evidence/corpus/phase7-producers/provenance.json`. **No candidate changed** |
| Complete-cost court over the generator-family corpus (`PDF_DEFLATE_REPLAY_RANS` vs `BYTE_RANS`) | 7.0b | RECORDED (measurement; **claim corrected in 7.0c**) | campaign `2026-10-05-phase7-producers-e071250`; over 4 locally generated generator files the real CLI court gives replay-rANS **win 1 / lose 3 / decline 0** (Cairo −24,137 B, 58,711 → 34,574; ReportLab −3,312 B; pdfTeX −10,663 B; LibreOffice −302,474 B). **Correction (7.0c):** the Cairo delta is a **repeated-identical-bytes harness artifact** — `tools/pdf-corpus-producers.sh` repeats one identical page six times, so Cairo's six streams are byte-identical in compressed bytes *and* plaintext; generic LZ captures ~2× more (`gzip -9` 17,382 B, `zlib -9` 17,376 B, `xz -9e` 16,852 B), and `BYTE_RANS` is a weak order-0 baseline with no LZ. The withdrawn framing is "a genuine authoring application produces the win region" / "first authoring-generator witness"; correct characterization is *our generator repeated one identical page six times and Cairo emitted six byte-identical streams — a repeated-bytes region, not the shared-plaintext-vs-distinct-compression mechanism*. All 4 auto winners `verify` + `cmp` byte-exact; no population claim; **no wire/candidate change**. Review: `docs/evidence/phase7b-skeptic-review.md` |
| Generic-compressor baseline ladder (gzip/zstd/xz/brotli vs VOLE) | 7.0c | RECORDED (measurement; the honest comparison) | campaign `2026-10-05-phase7-baselines-7b9f662`; new opt-in `baseline` image (`debian`→`dev` pinned base `rust:1.99.0-slim-bookworm@sha256:452176c0…`, adds gzip/zstd/xz/brotli/jq) and driver `tools/baselines.sh` compare complete files over 27 corpus files (phase7 23 + producers 4) against `gzip -9`, `zstd -19 --long=27`, `xz -9e`, `brotli -q 11` (each round-trip verified lossless) and the **best VOLE lane** (minimum over auto/raw/rle/byte-rans and every forced structural kind). Result: **VOLE beats gzip/zstd/xz/brotli on 0 files**; the best generic is smaller on every file, +460,320 B in total (phase7 +410,355; producers +49,965). On `cairo-vector.pdf` the best VOLE 34,574 B is 2.07× brotli's 16,670 B; on `flate.pdf` 36,102 B is 1.91× xz's 18,884 B. Prior "wins" were relative to the weak order-0 `BYTE_RANS` lane and do not survive a generic baseline. All 27 auto winners `verify` + `cmp` byte-exact; **no wire/candidate change**. Report `docs/evidence/phase7-corpus-report.md`; full table `baseline-table.md` |
| Nested PDF content proceduralization | 7 | PARTLY DELIVERED (observation) | "the clearest embodiment of the thesis"; Phase 11 recovered content operators + text runs + page content as **observations** (`ContentOperators`/`TextRuns`/`PageContent`, Stage-C deepening, ADR-0024/0026) — but **not** as a size mechanism, which remains unmeasured |
| PDF grammar/templates | 8 | PROPOSED | must pay definition cost |
| EntropyFS store-backed form (`EntropyFsStore` adapter) | 9.2 | IMPLEMENTED (optional; not measured) | non-default feature `entropyfs-store`; a thin `ObjectStore` adapter over `entropyfs 0.7.17` `engine::Engine` (`default-features = false`) — `put → put_blob`, `get → get_blob` (whole-blob BLAKE3 gate), `get_range → read_blob_range` + a strict `offset+len <= stored_len` check, `contains`. `BlobId` is BLAKE3-256, identical to our `Id`; an `EmbeddedStore`-externalized descriptor materializes byte-exactly through it (`tests/entropyfs.rs`). `list`/`remove` decline with `UnsupportedFeature` (no per-blob delete), so mark-and-sweep GC cannot reclaim through it. Viable but **heavy** (non-optional `dsfb` + a ~40-crate tree), never required for the standalone form (ADR-0020) |
| `ObjectStore` + `EmbeddedStore` + store-backed descriptor form | 9.1 | ADOPTED | `Id = BLAKE3-256` (archival identity stays SHA-256); `EXTERNAL_REF` (`0x80`, 40 B) + mandatory `FEATURE_EXTERNAL_OBJECTS`; `externalize`/`hydrate`; `gc` mark-and-sweep; `EmbeddedStore` (raw content-addressed directory, atomic write-then-rename `put`, strict `get_range`, per-object `remove`); `tests/store.rs`. Universe sufficed `+external-objects-v1` |
| Three accounting universes (standalone / unique-reachable / amortized) | 9.1/9.2 | ADOPTED | `src/store/account.rs`; `store account` CLI. `S = Σ|serialize(d_i)|`, `U = Σ|serialize(e_i)| + Σ len(o)`, `A = Σ(|serialize(e_i)| + Σ len(o)/refcount(o))` with the amortized split fractional by reference count and integerized so `Σ A_i == U` exactly. `S` is the only whole-file-comparable universe (ADR-0020) |
| Cross-document store court (three universes vs per-file LZ and generic CDC) | 9.3 | RECORDED (store axis: loss) | campaign `2026-10-05-phase9-store-fdb2845` (ADR-0021, `docs/evidence/phase9-store-report.md`, `docs/evidence/phase9-skeptic-review.md`); 37 locally generated files, 5,579,469 B, 10 strata; 37/37 standalone + store-backed roots `cmp`-byte-exact, closure `dangling = 0`. `S = 3,762,694`; `U = A = 3,369,900`; unique object bytes `786,501`. Per-file min LZ `1,304,307`; strongest CDC (borg 1.2.4, chunker `10,15,11,127`, `--compression none`) deterministic `771,383`, with `--compression zstd,19` **non-deterministic** 210,835–210,840. **`U` loses to all three.** The negative is robust (forced `--force pdf-deflate-replay` `U = 2,360,054`; per-stratum oracle ~`2,537,730`) but partly an externalization-granularity/candidate-selection artifact: the auto winner emits 0–1 objects per file. Auto-candidate only win: `repeat-bin` (4 byte-identical opaque binaries, `U = 133,048` vs LZ 524,308 / CDC 140,640 / CDC-zstd 133,863). Forcing `PDF_DEFLATE_REPLAY` (one object per deflate stream — a current-set candidate) flips `shared-payload` to `264,139` (win over raw CDC 285,257; loss to LZ 34,591 / zstd 28,195); no current candidate emits >1 object/file. `shifted` loses to CDC exactly as pre-registered. `tools/store-cohort.sh`, `tools/store-court.sh`, `tools/chunk-dedup.sh` |
| Encoder-only search governance (typed residual diagnostics + parametric space + pure governor + court) | 10.1 | IMPLEMENTED (feature `dsfb-search`, encoder-only) / search RECORDED (negative) | campaign `2026-10-05-phase10-governor-d2b09c9` (ADR-0022); dependency-free, non-default `dsfb-search = []` (**not** `dep:dsfb`); `src/encode/governor.rs`; `ResidualClass`/`ResidualDiagnostic`/`ResidualTrace` + `SearchConfig{scale_bits∈{8,10,12}, partition∈{ByKind,ByRole}, replay∈{Off,Dedup,DedupRans}, packed, depth}` + `govern(&ResidualTrace)->SearchDirective`; **zero decode authority** (no wire change, `header.rs` untouched, no feature bit; grep gate; a governed descriptor decodes byte-exactly in a default build without the feature). Court: H1 (guided never worse than fixed) HELD, H2 (guided==exhaustive on 8/8 holdout with ≤½ candidates) HELD, **H3 (no measurable byte benefit: `fixed == exhaustive` on every workload) HELD**, H4 (negative controls `Stop(Raw)` byte-exact) HELD. The fixed complete-cost court is retained; the parametric search buys nothing on this small locally generated cohort (no population claim) |
| **Consolidated findings (top-level verdict)** | **10.2** | **ADOPTED (superseding decision)** | **[`FINDINGS.md`](../FINDINGS.md)** + **ADR-0023**: the current representation stack does not beat a purpose-built baseline on any measured axis. Whole-file size (0/27, ADR-0017), random-access bytes read vs seekable/blocked (2.6–30× more, ADR-0019), and cross-document sharing vs LZ + CDC (`U = 3,369,900` vs 1,304,307 vs 771,383, ADR-0021) are recorded losses; the only scoped wins are partial-decode CPU / bytes-read vs **non-seekable** codecs (ADR-0018/0019) and whole-object dedup of identical opaque files (marginal vs CDC, ADR-0021); the governor buys no bytes (ADR-0022). Falsified claims (Phase-6 qpdf, Phase-7.0b Cairo) recorded with their reviews. Docs-only; no wire/semantics change |
| Procedural seed DAG (canonical immutable nodes + content-addressed `SeedStore`) | 11.2 | IMPLEMENTED | ADR-0025; `NodeId = BLAKE3-256("VOLE:PSEED:v1" + canonical state + canonical dependency ids)`, domain-separated from the object `Id`; `FsSeedStore` (atomic tmp→sync→rename, range reads) + optional `EntropyFsSeedStore` (one blob per node); green = present with a complete id-matching closure, red = absent; no mutation/invalidation pass; exactness unchanged (`materialize(root) == original`) |
| Hierarchical observation index (`HIER_INDEX = 0x72`) | 11.3 | IMPLEMENTED | ADR-0024/0026; bounded, advisory, re-derivable; a lying/cyclic/out-of-closure/oversized index is rejected fail-closed; a missing index falls back to the honest prefix path; every served slice is an observation (`integrity_verified == false`) |
| Observation query engine + provenance + `EXPLAIN` | 11.5–11.6 | IMPLEMENTED / MEASURED | ADRs 0024/0026/0027; typed `FieldAnswer{basis, scope, dependency ids, source spans}`, `basis ∈ {authored, directly-observed, deterministically-derived, inferred, heuristic, unresolved}`; `EXPLAIN` / `EXPLAIN ANALYZE`; deterministic planner + per-component late-materialization frontier; no agent/LLM/model on the decode path |
| Fair-baseline, lifetime, and pinned-tokenizer courts (A0 raw tooling, A1 preprocessed SQLite, LLM working set) | 11.9/11.10/11.12 | RECORDED (mixed) | campaigns `2026-10-06-phase11-63f43fb`, `…-lifetime-5a7edd3`, `…-llm-tokens-824faa9`; new pinned `db-baseline` and `llm-workingset` services. Exactness after source removal **confirmed** (length + SHA-256 + `cmp`, new process); A1 SQLite wins the narrow-query byte court (24,393 B); VOLE crosses A0 on wall on every doc but the A1 wall crossover is **absent on 3/5** documents; tokens (`bert-base-uncased`, pinned offline, hash-verified) 2 win / 2 tie / 2 loss vs page-local B1; no "tokens saved" without naming the tokenizer |
| Descriptor-free warm path (partial/lazy descriptor reads) | 11 priority #2 | RECORDED (overhead-only + wall win) | campaign `…-desc-free-a8ad6f4`; warm `descriptor_bytes_read == 0` and `seed_nodes_executed == 0`; the 8.4–8.9 KB is **overhead-only** (the warm process also reads the cached answer: 65.9 KB `large-400`, 527 KB `large`), so the byte win over A1 is withdrawn on the large cases; warm wall win and vs-A0 win stand |
| Immutable edit witness (declared narrow subset) | 11.12 | IMPLEMENTED (scoped) | ADR-0025; `src/field/edit.rs`; shares the descriptor/root/unaffected nodes **by content id** (0 bytes read, 0 blobs opened); 2 new seed nodes; `materialize(R1) == original` trivially; cost 43,688 B index read vs 8,776 B written (recorded loss) |
| Finer-than-object shareable units | 11.14 | RECORDED (loss to CDC) | ADR-0028; campaign `…-share-d9f818a`; unique **lower bound** 208,001 B loses to strongest CDC (160,668 B) and to `tar` + `xz -9e` (92,752 B); per-stratum loss on the near stratum |
| Partial materialization byte-level checkpoints (beyond v1) | 11+ | SUPERSEDED (checkpoint bytes remain) | Phase 11 delivered the partial-materialization architecture — observation engine, selective late materialization, hierarchical index, descriptor-free warm path (ADRs 0024/0026/0027); only literal byte-level checkpoint records remain future work |
| Coverage-guided fuzzing (`cargo-fuzz`/libFuzzer) | 7.1 | IMPLEMENTED | pinned dated nightly + `cargo-fuzz 0.13.2`; ten targets in `fuzz/fuzz_targets/`; bounded campaign `2026-10-05-phase7-fuzz-ca6a92b` (9/10 targets zero-crash; `deflate_replay` reported two upstream `preflate-rs` findings — F1 mitigated via the library's fail-closed `catch_unwind` boundary + regression test, F2 unbounded allocation now **contained on the decode path by process isolation**, ADR-0016); deterministic property/soak courts retained (`tests/property.rs`, `tools/soak-fuzz.sh`) |
| Process-isolated DEFLATE replay (F2 containment) | 7.1b | IMPLEMENTED | the decoder's `DEFLATE_REPLAY` arm calls `replay_bounded`: `preflate` runs in a child process (`sh -c 'ulimit -v …; ulimit -t …; exec "$0" __replay-worker'`) under an `RLIMIT_AS` address-space cap and a wall-clock timeout, returning a typed `CodecReplay` on abort/timeout/crash/malformed reply; hidden `__replay-worker` stdin/stdout protocol with length-bounded fields and a non-aborting panic hook; knobs `VOLE_REPLAY_WORKER` / `VOLE_REPLAY_MEM_MB` / `VOLE_REPLAY_TIMEOUT_MS` (default 30000); the CLI sets itself as the default worker (safe library setter, since `std::env::set_var` is `unsafe` under Rust 2024), while a library embedder with no worker falls back to the in-process `replay_raw` (the residual); no wire/candidate change; `tests/replay_isolation.rs` |
| Partial materialization (`OBSERVATION_INDEX` record + `view`) | 7.3 | ADOPTED (scoped) | optional `observation_index_v1` record (tag `0x70`, **advisory only**; re-checked against the authoritative `Program::analyze_ops` at parse, never authority) + `materialize_observation` and the `view` CLI (`--byte-range`/`--pdf-object`/`--pdf-stream`/`--pdf-revision`). When the program is a sequence of linear independent ops it evaluates only the ops intersecting `[a,b)` and decodes only the referenced entropy channels; a non-linear program honestly falls back to a prefix. Every served slice is byte-compared against the full materialization. Measured (campaign `2026-10-05-phase7-partial-a5764c9`): **18/18 pre-registered queries byte-exact**; mid/late queries touch ~0.41–0.43 MB (`descriptor_bytes_traversed` alone; its `entropy_bytes_decoded` breakdown is a subset already counted there and must not be added) versus gzip inflating `a+len` (late region 1.4–2.3 % of gzip's bytes, ~2–5× faster than gzip, ~4–13× than xz on CPU). **Scoped positive on decode CPU; NO I/O win in v1** — the CLI reads and parses the whole 17.5 MB descriptor, so `descriptor_bytes_traversed` is a CPU-side approximation; it loses in the early region (≤ ~8–16 MiB), never beats zstd's raw decoder, has ~38 MB peak RSS vs gzip's ~1.2 MB, and whole-file size still loses 2.98× to xz (ADR-0018) |
| Deterministic large multi-object corpus generator (`pdf-make-large`) | 7.3 | ADOPTED (tooling) | encode-time, binary-only subcommand (`pdf-make-large DIR [OBJECTS]`); assembles a valid classic-xref PDF of `OBJECTS` (default 800) pages each with its own **distinct** real zlib `FlateDecode` stream, correct `/Length` and offsets by construction, ≥32 MiB; no library or runtime dependency; regenerable `.pdf` bytes gitignored; ledger `evidence/corpus/phase7-large/provenance.json` |
| Query-cost court vs generic compressors (`tools/partial-court.sh`) | 7.3 | RECORDED (measurement) | times the indexed VOLE `view` against sequential gzip -9 / zstd -19 / xz -9e at the same output range; each baseline decoder is timed directly by `/usr/bin/time -v` via a FIFO-fed consumer that closes at the target offset, `pv -b -n` counts the compressed bytes read, and every VOLE slice is `cmp`'d against the source. Result: **scoped positive on decode work**: at 31 MiB VOLE touches ~1.4 % of gzip's inflated bytes and is ~1.4 % of gzip's bytes at every late query; **no I/O win** (VOLE reads its whole 17.5 MB `.voldoc` vs gzip's ~9.8 MB prefix) and zstd's raw decoder is faster on wall time everywhere (ADR-0018) |
| Seek `DIRECTORY` record + `Read + Seek` reader | 8.1–8.2 | ADOPTED | optional `seek_directory_v1` record (`RecordTag::Directory = 0x71`) written as the first record at fixed offset 64 with `FLAG_OPTIONAL` (no header field, no `FORMAT_MINOR` bump; new ignorable `FEATURE_SEEK_DIRECTORY` optional bit), carrying LOCATORS (offset + payload_len per record), CLASS_INDEX (O(1) k-th record of a class), and CHANNEL_LENGTHS. `materialize_observation_seeked` reads only the header, DIRECTORY, GRAPH, OBSERVATION_INDEX, INTEGRITY, and the referenced OBJECT/ENTROPY_CHANNEL/MODEL records. The directory is **advisory, never authority**: every locator is cross-checked against the record framing, the class index against a linear scan, and the observation index is re-derived over directory-derived lengths — a lying directory is rejected; a missing/oversized/unknown one declines with `UnsupportedFeature` and never silently reads the whole file. A partial read is an **observation**: `integrity_verified == false`; `materialize`/`decode`/`verify` remain the archival authority. `cmd_view` peeks only the 64-byte header before choosing the seek path. Non-seek serialization is unchanged except the `+seek-directory-v1` universe suffix (+18 B on this corpus). Tests: `src/container/directory.rs` (roundtrip/geometry/framing), `src/materialize/seek.rs` (seeked slice == full slice, tamper rejections, no-directory decline) |
| Seek bytes-read court (`tools/seek-court.sh`) | 8.3 | ADOPTED (scoped) | campaign `2026-10-05-phase8-seek-08de2a9`; seekable descriptor 17,566,832 B (DIRECTORY +27,390 B; index + directory = 174,101 B over the non-seek base). 18/18 pre-registered queries byte-exact; the seeked `view` reads a constant **439,679–461,367 B** (floor = header 64 + DIRECTORY 27,390 + GRAPH 265,462 + OBSERVATION_INDEX 146,711 + INTEGRITY 52), ≤ 2.6 % of the descriptor for every query (H1 18/18) and 4.7 %–~21× fewer bytes than gzip's compressed prefix in the late region (H2 8/8); a `strace -P` descriptor-file cross-check equals the instrumented count + exactly 64 B (the header peek). CPU ~0.00 s and peak RSS ~3.8 MB (vs Phase 7's ~38 MB). **Loses on bytes at a=0 vs gzip and early (≤ ~1.7 MiB) vs xz**; whole-file size still 3.01× xz. One locally generated corpus; no population claim (ADR-0019) |
| Seekable/blocked random-access baseline (`tools/seekable-baselines.sh`) | 8.4 | RECORDED (amendment; **falsifies the “general random-access win” framing**) | campaign `2026-10-05-phase8-seek-08de2a9` amendment (`seekable.jsonl`/`seekable-report.md`); adds tabix(bgzip 1.16) + pixz 1.0.7 to the `baseline` image. Compared to `bgzip -l 9` (BGZF), `xz --block-size=64KiB|1MiB|4MiB`, and `pixz` (16 MiB blocks) on the same 6 byte-ranges (all slices `cmp`-exact; xz/pixz covering blocks decoded **in isolation**). Late query (`a=32,505,856`): VOLE 460,713 B vs bgzip **23,808 B** (~19×), xz-64KiB **15,344 B** (~30×), xz-1MiB **179,892 B** (~2.6×), xz-4MiB 708,612 B (0.65×, VOLE wins), pixz 2,810,832 B (0.16×, VOLE wins). BGZF is also smaller whole-file (7,995,600 B < 17,566,832 B). **VOLE reads 2.6–30× more than BGZF/compact-block xz; not a general random-access-I/O win.** `docs/evidence/phase8-skeptic-review.md` |
| Universal multi-format `DocumentField` (PDF+DOCX+EPUB over a byte-authoritative ZIP layer) | 12.0–12.10 | ADOPTED | ADRs 0029–0035; `src/adapter/package/` (physical ZIP span scan + OPC/OCF package graph), `src/adapter/docx/`, `src/adapter/epub/`; one format-agnostic `field-ingest` + common CLI vocabulary with `format=<fmt>;common;<native>` provenance; byte-based format detection. Exactness unchanged (`materialize == original`; length + SHA-256 + `cmp`). Sealed receipts: removal **38/38**, triplet **96/96** (`2026-10-06-phase12-triplet-dc4d5a3`, `…-removal-dc4d5a3`), PDF no-regression `…-pdf-noregression-0d23a02` (A2 vs A11, 32/32 exact, 0 regressions; **`N6` closed**) |
| Lifetime + LLM working-set courts (A0 raw tooling, A1 source-retaining SQLite+FTS5, pre-registered ablation ladder) | 12.11/12.11b/12.12 | RECORDED (mixed) | campaigns `2026-10-06-phase12-lifetime-3eaf576`, `…-lifetime-ablations-06db12a`, `…-llm-3eaf576`, `…-fts5-amendment-22302f9`; VOLE wins the small-document frontier and the cold one-time comparison but A1 wins the large ~60 KB synthetic documents' byte frontier (N=10–1000) and wall/CPU at N=1000; the ladder attributes the win to the content adapters (A4→A5) and to persistent semantic reuse (A5→A6, bytes for CPU), with EntropyFS (A9) a **loss** and `A7`/`A8` **not separable**; tokens (pinned `bert-base-uncased`) 1 win / 8 tie / 3 loss vs page-local B1 and 9 win / 0 tie / 3 loss vs whole-document B0; FTS5-trigram ties `LIKE` but reads more (`unicode61` misses embedded markers) |
| Cross-document procedural reuse (state-level) | 12.8 | RECORDED (`N3` VIOLATED — negative) | ADRs 0034/0035; campaign `2026-10-06-phase12-share-e7ef693` + controls `…-share-controls-dce2705`; a byte-identical resource shared DOCX↔EPUB resolves to one content-addressed blob (representation identity shared; warm `retained_inverse_work_fraction` 0.339907), but reuse falls to **0.0** after `cache --clear` (in-process and fresh process), so cross-document **work** reuse is a recorded negative; strongest raw CDC baseline saved −1374 B |
| Security/fuzz for ZIP/OPC/OCF/XML surfaces | 12.13 | ADOPTED (scoped) | campaign `2026-10-06-phase12-security-33f6d04`; **315/315** court assertions over 45 hostile fixtures (16 reject / 22 opaque-preserve-and-decline / 7 accept; all `materialize --exact`); library court 15/15; 8 new fuzz targets `exit=0`; no new crash/hang/amplification; `N4` (decline-rate threshold) **not evaluated** |
| Cross-document proceduralization | 12+ | RECORDED (12.8: representation identity shared; durable **work** reuse negative, `N3`) | byte-level sharing is measured and negative (Phase 9 store: `U` loses to LZ and CDC, ADR-0021; Phase 11.14 finer-than-object units lose to CDC/`tar|xz`, ADR-0028); Phase 12 measured **state-level** sharing: one content-addressed blob shared DOCX↔EPUB, but the warm reuse fraction drops to 0.0 after `cache --clear` |
| Non-PDF adapters (DOCX/ODT/EPUB/…) | 12 (DOCX/EPUB) | ADOPTED (DOCX/EPUB); ODT PROPOSED | adapters over the same core; Phase 12 delivered the DOCX (WordprocessingML) and EPUB (OCF/XHTML) adapters (ADRs 0029–0035); ODT and other formats remain PROPOSED |

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
proceduralization — remains `PROPOSED` (as a *size* mechanism; the byte-level
cross-document axis is now measured and negative, ADR-0021/ADR-0028). Phase 2 measured only the order-0 typed byte entropy floor over an
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
