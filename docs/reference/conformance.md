# Conformance and courts

## Current courts (`cargo test --all-features`, inside Docker)

| Court | File | Properties |
|---|---|---|
| Unit | `src/**` | framing, header, CRC, SHA, DRA ops/program/coverage, court, materialize |
| Exact | `tests/exact.rs` | `materialize(encode(X)) == X`, digest equal, deterministic encode, bounded overhead, empty/tiny/large/random/text/sequential corpora |
| Malformed | `tests/malformed.rs` | every single-byte flip detected; truncation rejected; unknown mandatory record fails closed; unknown optional record skipped; duplicate records rejected; future version and feature bits fail closed; expansion bounded; the court rejects inexact candidates |
| Conformance | `tests/conformance.rs` | canonical `serialize(parse(x)) == x`; model round-trip; universe-id binding; golden structural length; coverage authority split; verify report accuracy |
| Entropy | `tests/entropy.rs` | Phase-2 acceptance gates: byte-exact round-trip; complete cost (model bytes charged, no "free model"); negative controls (RAW on incompressible/tiny, RLE on runs, deterministic tie-break); determinism; honest record when rANS loses |
| Goldens | `tests/goldens.rs` | **reference-oracle parity** (a from-scratch, integer-only decoder agrees byte-for-byte with `entropy::rans::decode_channel`, both ending at the encoder lower bound with the payload fully consumed); frozen model/capsule/descriptor golden bytes; truncation and single-byte flips ⇒ typed error; model decode never panics |
| Property | `tests/property.rs` | deterministic mutation/round-trip fuzzing over descriptor/model/channel/DRA parsers: `decode(encode(x)) == x`; `parse(serialize(d)) == d`; random and mutated bytes never panic and never report an internal invariant; oversized claims are bounded; limits never change reconstructed bytes |
| Soak (script) | `tools/soak-fuzz.sh` | longer deterministic run (`VOLE_FUZZ_ITERS`, default `200000`) of the property, entropy, goldens, and malformed courts |
| PDF physical | `tests/pdf.rs` | Phase-3 gates: total contiguous coverage of every corpus PDF; forced physical materialization byte-exact; validated (not extension-based) detection; incremental revisions; literal/hex/stream traps never split objects; hostile random bytes never panic; the court honestly still prefers RAW in Phase 3 |
| PDF channels | `tests/pdf_channels.rs` | Phase-4 gates: channel transposition (`join(split(x)) == x`); a forced typed-channel descriptor is byte-exact and its cost fully charged (models + payload non-zero, attribution sums to the serialized length); the honest `large_text_channels_compete` experiment forces RAW/BYTE_RANS/PDF_CHANNELS side by side and records the sizes, asserting only exactness and determinism — never that channels win |
| PDF oracle (script) | `tools/pdf-oracle.sh` | qpdf 11.3 differential court over a deterministic corpus: `qpdf --check` valid, object-number set agreement, `pdfinfo` page count; qpdf is an oracle, never the byte authority |
| Phase-4 ablation (script) | `tools/phase4-court.sh` | forced-candidate ablation via `encode --force KIND` (`raw`, `rle`, `byte-rans`, `pdf-physical`, `pdf-channels`) over the deterministic corpus: per-kind forced sizes, the cumulative ladder A0..A4, and the leave-one-out channel delta; every auto winner is `cmp`ed and `verify`ed byte-exact |
| PDF layout | `tests/pdf_layout.rs` | Phase-5 gates: the positional DRA ops (`MARK_OFFSET`/`EMIT_OFFSET`, DRA v4) round-trip and are bounded to 256 slots; a forced `PDF_LAYOUT` descriptor is byte-exact with fully charged cost; `classic.pdf` really marks positions and emits predicted offsets; a deliberately wrong source offset falls back to a literal and is never predicted; cross-reference streams, non-PDFs, and >255-object tables decline; identical input is deterministic; hostile corruption yields a typed error, never a panic; and the honest rejection gate asserts `PDF_LAYOUT` **loses** the complete-cost court on `classic.pdf` while the real winner round-trips exactly |
| Packed framing (Phase 5.7) | `src/dra/op.rs`, `src/adapter/pdf/layout.rs`, `tests/pdf_layout.rs` | `PACK_SEGMENTS` (opcode `0x08`, DRA v5) round-trips and rejects unknown tags, truncation, unmarked slots, bad widths, literal overruns, and unconsumed data; layout-v2 is one packed item table over one data object with adjacent literals coalesced; `many.pdf` (hundreds of xref entries) predicts ≥100 offsets and is byte-exact; forced layout-v2 beats RAW at scale but still loses to `BYTE_RANS`, asserted honestly |
| Phase-5.7 packed-framing court (script) | `tools/phase5-court.sh` | forced-candidate ablation (`encode --force KIND`) including `pdf-layout` (layout-v2) over the enlarged 11-file corpus: per-kind forced sizes, cumulative ladder A0..A5, leave-one-out layout delta, and the classic-xref subset; every auto winner is `cmp`ed and `verify`ed byte-exact, the qpdf oracle is re-checked, and the campaign `2026-10-05-phase5-4521778` is sealed |
| Packed channels (Phase 5.8) | `src/dra/op.rs`, `src/dra/program.rs`, `src/adapter/pdf/layout.rs` | `PACKED_CHANNELS` (opcode `0x09`, DRA v6) round-trips and rejects unknown item tags, truncation, unmarked slots, bad widths, literal overruns, missing channels, a declared-length mismatch, and unconsumed data; layout+rANS is one data channel plus one plan channel (serialized item table) with two models; `classic.pdf`/`bigtext.pdf`/`many.pdf` are byte-exact and deterministic, non-PDF and cross-reference-stream inputs decline, and the honest gate records that layout+rANS loses to `BYTE_RANS` |
| Phase-5.8 layout+rANS court (script) | `tools/phase5-8-court.sh` | forced-candidate ablation (`encode --force KIND`) including `pdf-layout-rans` over the 11-file corpus: per-kind forced sizes, cumulative ladder A0..A6, leave-one-out layout+rANS delta, and head-to-head win/lose/decline vs `BYTE_RANS`; every auto winner is `cmp`ed and `verify`ed byte-exact, the qpdf oracle is re-checked, and the campaign `2026-10-05-phase5-8-cf8048d` is sealed |
| PDF DEFLATE replay (Phase 6) | `tests/pdf_deflate.rs` | `DEFLATE_REPLAY` (DRA v8, `replay_codec`-tagged) and the two replay candidates: both replay variants are proposed for `flate.pdf` and every serialized descriptor reproduces the source byte-for-byte (length + digest + `cmp` + deep `verify`); shared plaintext is stored once (6 streams → strictly fewer unique plaintext channels); the rANS-plaintext lane beats the raw-plaintext lane; the unforced court on `flate.pdf` is byte-exact with an unknown winner tolerated; forcing a replay lane on a Flate-less PDF or a non-PDF is a typed `Usage` decline; replay proposal is deterministic; hostile correction blobs fail closed with a typed `CodecReplay`/`InvalidGraph` and never panic or silently reconstruct; a graph record naming an unknown `replay_codec` is rejected as `UnsupportedFeature`; a declared replay output above the static VOLE replay-profile admission limit (a policy bound; RFC 1951 permits unbounded empty non-final blocks) is rejected before the engine runs (ADR-0016); and the lexer stream-opacity change regresses no sample |
| Phase-6 replay court (script) | `tools/phase6-court.sh` | forced-candidate ablation (`encode --force KIND`) including `pdf-deflate-replay` and `pdf-deflate-replay-rans` over the 12-file corpus: per-kind forced sizes, cumulative ladder A0..A8, leave-one-out deltas for both replay mechanisms, and a head-to-head win/lose/decline vs `BYTE_RANS`; every auto winner is `cmp`ed and `verify`ed byte-exact, negative controls and typed declines are recorded verbatim, the qpdf oracle is re-checked, and the campaign `2026-10-05-phase6-0d0bb79` is sealed (the earlier DRA-v7 receipt `2026-10-05-phase6-ec92c1a` is retained as a historical amendment reference) |
| Flate correction-ratio harness (Phase 7.0) | `tests/deflate_stats.rs` | `deflate_stats` is diagnostics-only and never changes what the complete-cost court selects. On the real-zlib `flate.pdf`: exactly 6 `FlateDecode` streams, all replayed, aggregate `compressed_bytes` equals the sum of stream `data_len`, every replayed stream carries a non-empty plaintext, correction and rANS cost, and each correction is strictly smaller than its stream; the **deduplicated** shared-channel aggregate is strictly below the naive per-stream sum for a file whose plaintexts repeat; a non-PDF yields `is_pdf:false` with no streams and a zeroed summary (never an error); a PDF with no `FlateDecode` streams reports zero streams |
| Producer corpus + ratio campaign (Phase 7.0, script) | `tools/pdf-corpus.sh` | builds a locally-generated producer-stratified Flate corpus (Ghostscript `pdfwrite` at five `/PDFSETTINGS`, qpdf in four modes, a hand-written stored-block-zlib base, plus the Phase-3 synthetic set), validates every produced PDF with `qpdf --check`, records a provenance ledger (producer, version, exact command, SHA-256, `license:"locally-generated"`), and captures `deflate-stats` over every corpus PDF; every qpdf invocation passes `--deterministic-id`, so the qpdf corpus outputs are byte-reproducible across runs (Ghostscript output is not — it embeds a per-run `/ID` and timestamp); the campaign `2026-10-05-phase7-corpus-f1f8d26` is sealed and amended (not rewritten) by `2026-10-05-phase7-corpus-b-c4eb77e`, which re-measures after the lexer stream-boundary fix and records 24/24 replayed, 0 declined (acceptance 1.000); the Phase-6 win geometry appears only in our hand-authored fixtures, and the qpdf `--object-streams=preserve` output reproduces it merely by copying the fixture's two byte-identical raw streams (99.93% inherited), so it is not produced by a tested transformer (no new candidate adopted; every stream recorded verbatim) |
| Producer complete-cost court (Phase 7.0, script) | `tools/pdf-court.sh` | runs the real complete-cost court (`encode FILE OUT`) and four forced lanes (`encode --force raw|byte-rans|pdf-deflate-replay|pdf-deflate-replay-rans FILE OUT`) over every file in the locally-generated producer corpus, recording each lane's complete serialized `.voldoc` size; a forced kind the input does not propose is a typed `Usage` decline recorded as `null`; the auto winner is `verify`ed and `decode`d + `cmp`ed byte-exact; seals campaign `2026-10-05-phase7-court-99dc72e`: `PDF_DEFLATE_REPLAY_RANS` vs `BYTE_RANS` is win 3 / lose 8 / decline 12 over 23 files, **all 3 wins self-authored** (the `qpdf-preserve-objectstreams.pdf` win at −55,167 B is a preserved copy of the `hand-base2.pdf` fixture's byte-identical raw streams, 99.93% inherited); every genuinely transformed producer output loses or declines; no new candidate adopted |
| Generator-family corpus (Phase 7.0b, script) | `tools/pdf-corpus-producers.sh` | builds a locally-generated corpus from four real **authoring generators** with distinct DEFLATE behaviour — ReportLab 3.6.12, Cairo 1.20.1/libcairo 1.16.0, LibreOffice Writer 7.4.7.2, pdfTeX 3.141592653-2.6-1.40.24 — in the separate, opt-in `producers` image (`debian:bookworm-slim@sha256:3783cc01…`, the same digest as `tools`; ~724 MB); generates one PDF per available family from the same deterministic content document as `tools/pdf-corpus.sh`, fingerprints every `/FlateDecode` payload (`qpdf --json` + `qpdf --show-object --raw-stream-data`), applies `qpdf --deterministic-id --stream-data=preserve --object-streams=preserve` **only** when it leaves every payload byte-identical, builds each family twice to record byte-reproducibility, `qpdf --check`s every output, and records a provenance ledger; ReportLab/Cairo/pdfTeX are byte-reproducible, LibreOffice Writer is not (run-varying metadata); no third-party bytes (`.pdf` gitignored) |
| Generator-family complete-cost court (Phase 7.0b, script) | `tools/pdf-court.sh` + `tools/pdf-producers-analyze.py` | run over the four-family generator corpus; seals campaign `2026-10-05-phase7-producers-e071250`: exact-replay acceptance **87/87 = 1.000**, and `PDF_DEFLATE_REPLAY_RANS` vs `BYTE_RANS` is **win 1 / lose 3 / decline 0**. The Cairo delta (−24,137 B, 58,711 → 34,574) is **corrected in Phase 7.0c to a repeated-identical-bytes harness artifact**, not an authoring-generator witness: `tools/pdf-corpus-producers.sh` repeats one identical page six times, so Cairo's six streams are byte-identical in compressed bytes as well as plaintext, and generic LZ does ~2× better (`gzip -9` 17,382 B; `zlib -9` 17,376 B; `xz -9e` 16,852 B). The withdrawn framing ("a genuine authoring application produces the win region", "the producer creates the geometry", "first authoring-generator witness") is replaced by: *our generator repeated one identical page six times and Cairo emitted six byte-identical streams — a repeated-bytes region already captured better by generic LZ, not the shared-plaintext-vs-distinct-compression mechanism.* ReportLab (−3,312 B) and pdfTeX (−10,663 B) are the same identical-repeat story and lose; LibreOffice (−302,474 B) shares nothing. Every auto winner `verify`ed and `cmp`ed byte-exact; the delta is conditional on repeated identical page content and is not a population claim; `results.json` is not rewritten, the amendment is prose in the report; review `docs/reviews/phase7b-skeptic-review.md`; no candidate adopted or changed |
| Generic-compressor baseline ladder (Phase 7.0c, script) | `tools/baselines.sh` (+ `tools/baselines.jq`) | for every file in a given corpus dir records the source size and the smallest **lossless** complete-file size for `gzip -9`, `zstd -19 --long=27`, `xz -9e`, `brotli -q 11` — each decompressed and `cmp`'d against the source *before* it is scored — plus the complete serialized `.voldoc` size of every VOLE lane (auto, `raw`, `rle`, `byte-rans`, and every forced structural kind; a kind the input does not propose is a typed `Usage` decline recorded `null`); `best_vole` is the minimum across all VOLE lanes. Runs in the pinned opt-in `baseline` image (`dev` base `rust:1.99.0-slim-bookworm@sha256:452176c0…` + gzip/zstd/xz/brotli/jq). Over 27 corpus files (phase7 23 + producers 4) it seals campaign `2026-10-05-phase7-baselines-7b9f662`: **the best VOLE lane beats gzip/zstd/xz/brotli on 0 files**; the best generic is smaller on every file, +460,320 B total. All 27 auto winners `verify` + `cmp` byte-exact; no candidate adopted or changed |
| Fuzz regressions (Phase 7.1) | `tests/fuzz_regressions.rs` | committed crash/OOM fixtures from the coverage-guided campaign reproduce the fixed behavior: hostile `preflate` corrections are declined as a typed `CodecReplay` (never unwound into the caller), and the public analysis entry point fails closed on a non-zlib stream; the unbounded-allocation fixture is deliberately not executed in-process and is instead driven through the isolated worker in `tests/replay_isolation.rs` (upstream `preflate-rs` limitation, contained on the decode path by Phase 7.1b; ADR-0016) |
| Process-isolated replay (Phase 7.1b) | `tests/replay_isolation.rs`, `src/codec/deflate.rs` | the decoder's `DEFLATE_REPLAY` arm runs `preflate` in a child process under an `RLIMIT_AS` address-space cap and a wall-clock timeout (`replay_bounded`), so a malformed `.voldoc` cannot amplify memory; the positive court decodes a forced-replay `flate.pdf` descriptor byte-exactly through the worker; the committed F2 fixture, embedded in a descriptor, fails closed as a typed `CodecReplay` under a 32 MiB cap without exhausting the host; a configured-but-broken worker is a typed `CodecReplay` (exit 13) and never silently falls back to the in-process path; with no worker configured the library reconstructs in-process (the documented fallback, F2 residual); the worker stdin/stdout framing round-trips and refuses over-bound field lengths before allocating |
| Partial materialization (Phase 7.3) | `tests/observation.rs`, `tests/observation_cli.rs`, `tests/observation_disk.rs`, `tools/partial-court.sh` | the advisory `OBSERVATION_INDEX` record is **checked, never authority**: `Descriptor::parse` re-derives every op length via `analyze_ops` and rejects a contradiction; `materialize_observation`/`view` serve byte-ranges, indirect objects, encoded streams, and revisions byte-exactly (`obs == full[a..b]`), decline a descriptor with no index (never silently fully materialize), skip non-intersecting linear ops, and decode only the referenced entropy channels; absent/ambiguous/out-of-range selectors and empty/oversized requests are typed errors that never panic; the CLI accepts exactly one selector (`--byte-range`/`--pdf-object`/`--pdf-stream`/`--pdf-revision`) and `--stats` reports the resolved `range_start`/`range_len`; `tools/partial-court.sh` times the indexed lane against sequential `gzip`/`zstd`/`xz` at the same output offset, `cmp`s every served slice against the source, and seals campaign `2026-10-05-phase7-partial-a5764c9`: 18/18 queries byte-exact, a **scoped decode-CPU win with no I/O win in v1** (the CLI reads the whole descriptor; ADR-0018). `tests/observation_disk.rs` closes the durability gap: it generates a small corpus with the shared `pdf-make-large` generator, encodes it with the `--force`-equivalent indexed candidate, writes the descriptor to a temporary file, **re-reads** it, and asserts the served start/middle/end byte ranges and one indirect-object and one encoded-stream selector still equal the full materialization (and `physical::scan`). Independent review `docs/reviews/phase7c-skeptic-review.md` confirmed the headline and corrected the presentation: the honest decode-side bound is `descriptor_bytes_traversed` alone (~0.41–0.43 MB; `entropy_bytes_decoded` is a subset already included and must not be added), the win is decode **CPU** (RSS stays a loss), and the early-region boundary is ~8–16 MiB |
| Seek-based partial I/O (Phase 8) | `src/container/directory.rs`, `src/materialize/seek.rs`, `tests/observation_disk.rs`, `tools/seek-court.sh` | the optional `DIRECTORY` record (`seek_directory_v1`, tag `0x71`, first record at fixed offset 64, `FLAG_OPTIONAL`, new ignorable `FEATURE_SEEK_DIRECTORY` bit) is validated structurally (offset contiguity, class index vs a linear scan, channel-length table, size/entry bounds); the seek reader reads **only** the header, directory, GRAPH, OBSERVATION_INDEX, INTEGRITY, and the referenced OBJECT/ENTROPY_CHANNEL/MODEL records, declines a descriptor with no/oversized/unknown directory as `UnsupportedFeature` instead of reading the whole file, and rejects a lying directory as `InvalidContainer`; every seeked slice equals the full materialization slice byte-for-byte (start/middle/end + `--pdf-object`/`--pdf-stream`), tampered locators/class-index/channel-lengths/TRAILER counts are rejected, and the report marks `integrity_verified == false` (a partial read is an *observation*, not an archival verification; `materialize`/`decode`/`verify` remain authority); `view` peeks only the 64-byte header before choosing the seek path, so a seekable descriptor is never `fs::read` whole. `tools/seek-court.sh` records the instrumented `bytes_read`, a descriptor-file-attributed `strace -P` cross-check, syscall count, wall/CPU/RSS, the Phase-7 stats, and sequential gzip/zstd/xz compressed-prefix baselines; it seals campaign `2026-10-05-phase8-seek-08de2a9`: 18/18 byte-exact, a constant 439,679–461,367 B read (H1 18/18 ≤ 2.6 % of the descriptor; H2 8/8 late at 4.7 %–~21× fewer bytes than gzip's prefix), with the recorded losses at `a = 0` vs gzip and early (≤ ~1.7 MiB) vs xz. **Phase 8.4 amendment** (`tools/seekable-baselines.sh`, `tools/bgzf-seek-probe.pl`, `tools/xz-seek-probe.pl`, `tools/xz-block-reframe.pl`; tabix(bgzip)+pixz added to the `baseline` image): against **seekable/blocked** formats the same late query reads **more** — bgzip (BGZF) 23,808 B, `xz --block-size=64KiB` 15,344 B and 1MiB 179,892 B (VOLE 460,713 B is 2.6–30× more), while xz-4MiB (708,612 B) and pixz (2,810,832 B, 16 MiB blocks) read more than VOLE; xz/pixz covering blocks are decoded **in isolation** (`tools/xz-block-reframe.pl` rebuilds a minimal single-block `.xz`, demonstrating block independence) and every extracted slice is `cmp`-exact. **So the Phase-8 result is a scoped bytes-read win versus non-seekable sequential codecs, not a general random-access-I/O win**; whole-file is 3.01× xz and 0.46× BGZF. Review `docs/reviews/phase8-skeptic-review.md` (ADR-0019) |
| Cross-document content-addressed store (Phase 9.1–9.3) | `src/store/{mod,embedded,account}.rs`, `tests/store.rs`, `tools/store-cohort.sh`, `tools/store-court.sh`, `tools/chunk-dedup.sh` | the `EXTERNAL_REF` (`0x80`) wire record makes each object-table entry an inline `OBJECT` or an external `Id = BLAKE3-256` reference (`Id` is a *relational* name, never a substitute for the SHA-256 archival identity, and never appears in `INTEGRITY`); a store-backed descriptor sets the mandatory `FEATURE_EXTERNAL_OBJECTS` bit and a `--no-default-features` build fails closed with `UnsupportedFeature`; `externalize`/`hydrate` convert both ways and materialize identical bytes; `gc` mark-and-sweeps the roots' external-id union and a non-empty dangling set is a live `MissingExternalObject`; `EmbeddedStore` is a raw content-addressed directory with atomic `put`, a strict `get_range`, and per-object `remove`. The court measures the three frozen universes (standalone `S`, unique-reachable `U`, amortized `A`, with `Σ A_i == U` exactly) and compares `U` against per-file min LZ and the strongest pinned content-defined-chunk dedup. It seals campaign `2026-10-05-phase9-store-fdb2845` (ADR-0021): 37 locally generated files, 5,579,469 B source, 10 strata; 37/37 standalone + store-backed roots `cmp`-byte-exact and closure `dangling = 0`; `S = 3,762,694`, `U = A = 3,369,900`, unique object bytes `786,501`; per-file min LZ `1,304,307` (gzip 1,495,074 / zstd 1,334,444 / xz 1,307,612 / brotli 1,306,498); strongest CDC (borg 1.2.4, chunker `10,15,11,127`, `--compression none`, deterministic by a twice-run `unique_csize`) `771,383`; the same with `--compression zstd,19` is **non-deterministic** (observed 210,835–210,840, never a fixed citation). **The store loses the axis to all three.** The negative is robust (forced `--force pdf-deflate-replay` gives `U = 2,360,054`; a per-stratum candidate oracle ~`2,537,730`) but partly an externalization-granularity/candidate-selection artifact: the auto winner emits 0–1 objects per file. Under the auto candidate the only win is `repeat-bin` (byte-identical opaque files, `U = 133,048`); forcing `PDF_DEFLATE_REPLAY` — a candidate **in the current set**, one object per deflate stream — flips `shared-payload` to `U = 264,139`, a win over raw CDC (`285,257`) that still loses to LZ (`34,591`) and compressed CDC (`28,195`); no current candidate emits more than one object per file. Cross-document sharing is reported as store *amortization*, never "compression"; a store root is never compared to a whole document; the losing strata (incl. `shared-payload` and the pre-registered `shifted` control) and the fact that CDC captures the same sharing are recorded. Report `docs/evidence/phase9-store-report.md`; review `docs/reviews/phase9-skeptic-review.md` |
| Encoder-only search governance (Phase 10.1) | `src/encode/governor.rs`, `examples/governor_court.rs`, `tests/governor.rs`, `tools/governor-court.sh` | the non-default, **dependency-free** feature `dsfb-search = []` (not `dep:dsfb`) adds typed integer-only residual diagnostics, a parametric space over existing mechanisms (`scale_bits ∈ {8,10,12}`, `partition ∈ {ByKind,ByRole}`, `replay ∈ {Off,Dedup,DedupRans}`, `packed`, `depth ∈ 0..=9`), and a pure `govern`; **every candidate reaches the unmodified complete-cost court**, and the feature changes no wire bytes, no feature bit, and no decoder behavior. `tests/governor.rs`: `propose_configured(DEFAULT)` is byte-identical to `propose_all`; every grid config's candidates serialize/parse/materialize byte-exactly; `ByRole` differs from `ByKind` and is exact; `packed=false`, `replay=Off`, and `depth` prune the expected families; the guided driver is deterministic; a governed descriptor materializes byte-exactly; negative controls `Stop(Raw)` and match the RAW bytes; and a **grep gate** asserts `src/{materialize,container,dra,store}` reference neither `governor` nor `crate::encode` and that `dsfb-search` is empty and non-default. `tools/governor-court.sh` runs `Exhaustive`/`FixedHeuristic`/`DsfbGuided` over the tune/holdout/control cohort and seals campaign `2026-10-05-phase10-governor-d2b09c9` (ADR-0022): **H1 HELD** (guided never worse), **H2 HELD** (guided == exhaustive on 8/8 holdout with ≤ ½ candidates), **H3 HELD** (**no byte benefit**: `fixed == exhaustive` on every workload), **H4 HELD** (negatives byte-exact RAW). It also decodes a governor-produced descriptor in the **default** build (no `dsfb-search`) and byte-compares it. Recorded negative: the fixed complete-cost court is retained |

Test counts (inside the pinned `dev` image, Phase 10.1): **411** passing with
`--all-features` (0 failed, 1 ignored), of which **276** are library unit tests
(plus 1 ignored); **356** passing with the default feature set
(`default = ["rans", "store"]`; the replay and `dsfb-search` courts are skipped),
of which **262** are library unit tests; **292** passing with
`--no-default-features`, of which **240** are library unit tests; and **367**
passing with `--features dsfb-search` (default + the governor, without
`deflate-replay`). The MSRV image (`rust:1.89`) also passes with
`cargo test --locked --all-features`. Phase 10.1 added 3 library unit tests and
`tests/governor.rs` (8 integration tests, compiled only under the non-default
`dsfb-search` feature); `examples/governor_court.rs` is the court driver.

**Later counts (Phase 12.16 close-out, `0d23a02`).** The default feature set is now
`default = ["rans", "store", "field"]` (`field` added in Phase 11), and the full
gate reports **679** passing with `--all-features` and **306** with
`--no-default-features`, 0 failures; the non-PDF adapters add the `package`, `opc`,
`docx` and `epub` features (`src/adapter/package/`, `src/adapter/docx/`,
`src/adapter/epub/`). The Phase-10.1 counts above are retained as the historical
snapshot they are (`docs/phases/phase-12-results.md`, "Close-out gate ledger").

`encode --force KIND` is the ablation surface: it runs the *same* complete-cost
court over a one-element candidate set (`KIND` in `raw`, `rle`, `byte-rans`,
`pdf-physical`, `pdf-channels`, `pdf-layout`, `pdf-layout-rans`,
`pdf-deflate-replay`, `pdf-deflate-replay-rans`,
`pdf-deflate-replay-rans-indexed`, `pdf-length-revision`,
`pdf-cos-template`). Forcing selects
a lane; it never bypasses serialization, decoding, or the byte-compare, and a kind
the input does not propose fails with a typed usage error (recorded as `null`), not
a fabricated result.

Run everything:

```sh
docker compose run --rm --no-TTY dev cargo test  --all-features
docker compose run --rm --no-TTY dev cargo clippy --all-targets --all-features -- -D warnings
docker compose run --rm --no-TTY dev cargo fmt --all --check
```

## Phase 11–13 courts (added after the Phase-10 snapshot above)

The Phase-11 persistent procedural field and Phase-12 multi-format adapters add
their own courts (run in the same `cargo test --all-features` gate):

- **Field / observation** — `tests/field_authority.rs`, `tests/field_observe.rs`,
  `tests/field_partial.rs`, `tests/field_reuse.rs`, `tests/field_edit.rs`,
  `tests/share_account.rs`, `tests/observation_seek.rs`.
- **ZIP / OPC / OCF / adapters** — `tests/zip_physical.rs`, `tests/opc_core.rs`,
  `tests/package_field.rs`, `tests/docx_adapter.rs`, `tests/epub_adapter.rs`,
  `tests/epub_content.rs`, `tests/universal_api.rs`,
  `tests/cross_document_reuse.rs`.
- **Phase-12 courts** — `tests/phase12_courts.rs` (triplet / equivalence),
  `tests/phase12_lifetime.rs` (lifetime + ablation ladder),
  `tests/phase12_security.rs` (hostile input), with drivers
  `tools/phase12-*.sh` and `examples/phase12_share_court.rs`.
- **Fuzzing** — 8 Phase-12 targets (`zip_scan`, `zip_decode`, `opc_rels`,
  `docx_wml`, `epub_package`, `epub_content`, `xml_part`, `common_observe`) added
  to the `fuzz` service; every target returned `exit=0` in the
  `2026-10-06-phase12-security-33f6d04` campaign.

The Phase-13 subphases (byte-level checkpoints, ODT adapter, size-mechanism
courts, the `N5` gate) add their courts as they land; see
[`docs/phases/phase-13-plan.md`](../phases/phase-13-plan.md).

## Standing invariants

1. `materialize(encode(X)) == X` byte-for-byte for every admitted input.
2. Any single-byte mutation of a valid descriptor is detected (every byte is
   covered by the header CRC or a record CRC).
3. Unknown mandatory semantics fail closed; only explicitly-optional records are
   skipped.
4. Resource bounds are enforced before allocation.
5. Encoding is deterministic for a pinned universe and feature set.
6. `serialize(parse(x)) == x` for canonical descriptors.
7. **Reference-oracle parity**: an independent, from-scratch decoder reproduces
   `entropy::rans::decode_channel` byte-for-byte for every tested model, and both
   consume the payload exactly and return the decoder state to `RANS_BYTE_L`.
8. **Structural exact-consumption**: a channel stream is accepted only if it
   consumes its payload exactly and the decoder state returns to `RANS_BYTE_L`.
   This is a structural invariant, *not* a checksum: it rejects truncation and
   many corruptions, while content corruption is caught by the enclosing
   per-record CRC-32C and the whole-source SHA-256 — not by the channel check.
9. **Feature-set behaviour**: with the `rans` feature, channel-bearing
   descriptors materialize byte-for-byte; without it, channel-free descriptors
   still materialize exactly and channel-bearing ones fail closed with
   `UnsupportedFeature` (never a silent reinterpretation).
10. **Opt-in replay and its bounds**: the default build is permissive-only
    (`default = ["rans", "store", "field"]`; the `field` feature was added in
    Phase 11); the DEFLATE replay stack is opt-in. At decode time a
    `DEFLATE_REPLAY` op names its semantics with a `replay_codec` tag (unknown id
    ⇒ `UnsupportedFeature`), rejects a declared output above the VOLE
    replay-profile admission limit `min(max_output_bytes, max_replay_bytes,
    2*P+1024)` for a `P`-byte plaintext *before* running the engine (ADR-0016) — a
    policy bound, since RFC 1951 permits unbounded empty non-final blocks and so
    gives no finite `f(decompressed_size)` bound — bounds
    plaintext/corrections by `max_record_len`, and runs the engine **in an
    isolated child process** under an `RLIMIT_AS` cap and timeout, so a hostile
    correction blob cannot amplify the decoder's memory (fuzz finding F2,
    Phase 7.1b). Without a configured worker the library falls back to the
    in-process path (the residual).

## Fuzz targets (added as parsers land)

The Phase-2 property/mutation court already exercises the `.voldoc`
header/record parser, the DRA parser/evaluator, the rANS model parser, the rANS
channel decoder, and the coverage certificate (`tests/property.rs`,
`tests/goldens.rs`, `tools/soak-fuzz.sh`). Phase 3 additionally exercises the PDF
lexical cover and physical scanner over hostile random bytes (`tests/pdf.rs`).
Phase 4 additionally exercises the typed-channel transposition and the
`INTERLEAVE_CHANNELS` evaluator, including corrupt, misaligned, and overrunning
channels (`tests/pdf_channels.rs`). Phase 5 additionally exercises the positional
DRA ops and the classic-xref layout builder, including unmarked-slot and
width-bound rejection, wrong-offset fallback, decline preconditions, and hostile
corruption (`tests/pdf_layout.rs`). Phase 5.7 additionally exercises the
`PACK_SEGMENTS` item-table parser and evaluator (unknown tags, truncation,
unmarked slots, bad widths, literal overruns, and unconsumed data) and the
coalesced layout-v2 builder (`tests/pdf_layout.rs`). Phase 5.8 additionally
exercises the `PACKED_CHANNELS` item-table-over-channels evaluator (declared-length
mismatch, missing channels, truncated plan, unconsumed data) and the layout+rANS
candidate builder (two-channel exactness, determinism, and decline) in
`src/dra/program.rs` and `src/adapter/pdf/layout.rs`. Phase 6 additionally
exercises the `DEFLATE_REPLAY` evaluator (declared-length mismatch, missing
objects, hostile correction blobs that must fail closed) in `src/dra/program.rs`,
the bounded replay wrapper in `src/codec/deflate.rs`, and the replay candidate
builders and lexer stream-opacity change over the real-zlib `flate.pdf` sample in
`tests/pdf_deflate.rs`.

Phase 7.1 adds **coverage-guided** fuzzing on top of the deterministic courts: a
standalone `cargo-fuzz` package in `fuzz/` (excluded from `cargo package`) with
ten libFuzzer targets — `voldoc_parse`, `voldoc_roundtrip`, `dra_program`,
`rans_model`, `rans_channel`, `pdf_lexer`, `pdf_scan`, `pdf_xref`,
`deflate_replay`, `materializer` — built and run in a pinned dated-nightly `fuzz`
Docker service. `tools/fuzz.sh` runs a bounded per-target campaign and records
duration, coverage, execs, and crashes; the sealed campaign
`2026-10-05-phase7-fuzz-ca6a92b` observed zero crashes on nine of ten targets and
reported two upstream `preflate-rs` findings (a shift-overflow panic, mitigated by
the library's fail-closed `catch_unwind` boundary, and an unbounded reconstruction
allocation now contained on the decode path by Phase-7.1b process isolation;
ADR-0016). `tests/fuzz_regressions.rs` holds the minimized fixtures and asserts
the fixed behavior; `tests/replay_isolation.rs` drives the F2 fixture through the
isolated worker.
Still planned for later phases: a stream-boundary parser.

Useful properties: never panic · bounded failure · round trip · descriptor
parse/serialize stability · materialized length bound · RAW fallback preserves
bytes · mutated `voldoc` either verifies or returns a typed error. Maintain a
minimized regression fixture for every discovered crash/hang/invariant violation.

## Compatibility policy (provisional)

The format is **pre-1.0** and not a stability commitment. A descriptor's meaning
is pinned by its universe string and header version. Once v1 freezes, old streams
must decode exactly; new semantics require a new optional feature, a new
mandatory feature, a new universe, or a new major format — never a silent
reinterpretation. Golden `.voldoc` fixtures will be added at freeze time.

## Evidence

Every sealed run lives under `evidence/campaigns/<date>-<phase>-<gitsha>/` with a
`manifest.json`, `environment.json`, and results. Receipts are immutable; new runs
get new directories. Corrections are amendments, not rewrites.
