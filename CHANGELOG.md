# Changelog

All notable changes are recorded here. The format is pre-1.0 and provisional.

## [0.1.0-alpha.11] — unreleased

Phase 9 — **cross-document content-addressed store** (9.1 store core, 9.2
optional EntropyFS backend + store CLI). Exactness is unchanged: a store-backed
descriptor must materialize the exact same bytes as its standalone form. **No
size or win is claimed here** — whether object-granularity sharing beats per-file
LZ and generic content-defined-chunk dedup is the Phase 9.3 cohort measurement.

### Added

- **Phase 9.1 — `ObjectStore` + `EmbeddedStore` + store-backed descriptor form**
  (ADR-0020). `Id = BLAKE3-256(object bytes)` is the relational store namespace,
  kept strictly distinct from the SHA-256 whole-source *archival* identity (an
  `Id` never appears in `INTEGRITY`). New optional-but-load-bearing `EXTERNAL_REF`
  record (`0x80`; `id:[u8;32]`, `len:u64LE`, exactly 40 B) puts each object-table
  entry in one of two forms; the entry's position remains its `object_id`, so the
  DRA is untouched. A store-backed descriptor declares the mandatory
  `FEATURE_EXTERNAL_OBJECTS` (`1 << 1`) bit; a build without the new default
  `store` feature fails closed with `UnsupportedFeature` (exit 6). New universe
  suffix `+external-objects-v1` (prefix `phase9`; `dra-8` and `FORMAT_MINOR`
  unchanged). `externalize` (inline → external) and `hydrate` (external → inline)
  convert both ways and materialize identical bytes. `gc` mark-and-sweeps the
  union of the roots' external ids; a non-empty dangling set is a live
  `MissingExternalObject`. `EmbeddedStore` is a raw local content-addressed
  directory (`objects/<aa>/<bb>/<64-hex-id>`) with atomic write-then-rename `put`,
  strict `get_range`, per-object `remove`, and a `STORE` backend marker.
  `CostBreakdown::external_refs` charges the 40 B payload (framing to
  `record_framing`). Tests: `tests/store.rs`.
- **Phase 9.2 — optional `EntropyFsStore` + store CLI + docs.** A thin
  `ObjectStore` adapter over the embeddable `entropyfs = "=0.7.17"`
  `engine::Engine` (`default-features = false`), behind the non-default,
  heavy `entropyfs-store` feature (implies `store`; pulls a non-optional `dsfb`
  and a ~40-crate tree). `put → put_blob`, `get → get_blob` (full whole-blob
  BLAKE3 gate), `get_range → read_blob_range` plus an explicit strict
  `offset + len <= stored_len` check (the engine clips at EOF), `contains →
  contains`; `BlobId` is BLAKE3-256, asserted identical to our `Id`, so an
  `EmbeddedStore`-externalized descriptor resolves unchanged. `list`/`remove`
  decline with `UnsupportedFeature` (no per-blob delete), so mark-and-sweep GC
  cannot reclaim through it; reclamation is EntropyFS's own reachability-GC
  policy. New CLI: `decode --store STORE_DIR`, `store put INPUT.voldoc STORE_DIR`
  (writes `STORE_DIR/<stem>.voldoc`), `store account STORE_DIR ROOT...`, and
  `store gc STORE_DIR ROOT...`. `src/store/account.rs` computes the three
  universes. Tests: `tests/entropyfs.rs`; formatting both ways proven
  byte-exact.
- **Three accounting universes (kept permanently distinct).** Standalone
  `S = Σ|serialize(d_i)|` (whole-file; the only universe comparable to a per-file
  compressor); unique-reachable `U = Σ|serialize(e_i)| + Σ len(o)` (shared bytes
  counted once); amortized `A = Σ(|serialize(e_i)| + Σ len(o)/refcount(o))` with
  the split **fractional by reference count**, integerized by largest remainder so
  `Σ A_i == U` exactly. `store account` prints all three and the per-root split.
- Docs: `SPEC.md` (`EXTERNAL_REF` record, `ObjectSource` model, mandatory feature
  bits, universe string, feature policy), `docs/adr/0020-content-addressed-store.md`.

### Measured — Phase 9.3 store court (recorded negative)

- **Cross-document sharing does not beat per-file LZ or generic
  content-defined-chunk dedup** on the phase9 cohort (campaign
  `2026-10-05-phase9-store-fdb2845`; ADR-0021). 37 locally generated files,
  5,579,469 B source, 10 strata. Standalone `S = 3,762,694 B`; unique reachable
  `U = 3,369,900 B` (`==` amortized `A`); unique object bytes `786,501 B`.
  Per-file min LZ `1,304,307 B` (gzip 1,495,074 / zstd 1,334,444 / xz 1,307,612 /
  brotli 1,306,498). Strongest CDC (pinned borg 1.2.4, chunker `10,15,11,127`,
  `--compression none`) deterministic `771,383 B`; the same with
  `--compression zstd,19` is **non-deterministic** (observed 210,835–210,840 B,
  never a fixed citation). **`U` loses to all three.** All 37 standalone and
  store-backed roots decoded and `cmp` byte-exact; closure `dangling = 0`.
- **The negative is robust but its size is partly an artifact of candidate
  selection / externalization granularity, not of the mechanism's limits.**
  `externalize` replaces only `Descriptor.objects`, and the auto complete-cost
  winner codes most PDF bulk in entropy channels / `GRAPH` inlines, so it emits
  0–1 objects per file. Forcing `PDF_DEFLATE_REPLAY` — a candidate **in the
  current set** — emits one object per deflate stream: the global `U` falls to
  `2,360,054 B` (a per-stratum oracle gives ~`2,537,730 B`), still a loss vs LZ
  and raw CDC. The earlier claim that no current candidate could share finer than
  a whole file is **withdrawn**: the finest unit the current set can emit is the
  whole deflate stream; no current candidate emits more than one object per file,
  and finer sharing (channel payloads / sub-object chunks) remains a
  representation change.
- Under the **auto** candidate the only win is `repeat-bin` (four byte-identical
  opaque binaries): `U = 133,048 B` vs per-file LZ `524,308 B` (−74.6 %), CDC
  `140,640 B` raw (−5.4 %) / `133,865 B` compressed (−0.6 %, ~0.8 KB — real but
  marginal). `shared-payload` — the fixture expected to win — loses in the auto
  run (`U = S = 800,210 B` vs CDC `285,257 B`) because the auto winner codes the
  shared stream in entropy channels, not the object table; **forcing
  `PDF_DEFLATE_REPLAY` flips it to `264,139 B`** (2 unique objects), a win over
  *raw* CDC (`285,257 B`) that still loses to per-file LZ (`34,591 B`) and
  compressed CDC (`28,195 B`). `shared-bin`, `one-changed`, `incremental`,
  `reexport`, `repeat-pdf` and the pre-registered `shifted` control all lose to
  CDC. **Cause:** `externalize` replaces only `Descriptor.objects`; for
  channels/program winners that table is empty or a single whole-file object, so
  sharing is whole-object-granular and coarse and CDC captures the same (and
  near-duplicate) sharing the store misses. Independent adversarial review:
  `docs/evidence/phase9-skeptic-review.md`.
- **Tooling.** `tools/store-cohort.sh` (deterministic sharing cohort + committed
  `cohort.json` ledger), `tools/store-court.sh` (three universes vs per-file LZ
  and strongest CDC, per stratum), `tools/chunk-dedup.sh` (pinned borg CDC with a
  parameter sweep), `borgbackup=1.2.4-1` added to the `baseline` image. Full
  report `docs/evidence/phase9-store-report.md`; ADR-0021.

### Notes

- **Claim discipline.** No size or compression claim is made for the store; a
  store root reference is never reported as a whole-document size. The honest
  comparison (per-file LZ and generic CDC dedup over a reproducible cohort) and
  the pre-registered negative are now **measured and recorded**: cross-document
  sharing is store *amortization*, never "compression", and it loses the store
  axis to generic CDC on this cohort (ADR-0021). `dsfb` remains a hard dependency
  of the *optional* `entropyfs-store` feature only and retains **zero** decode
  authority (ADR-0008).
- **Corrections (independent adversarial review, `docs/evidence/phase9-skeptic-review.md`).**
  The Phase-9.3 claims were corrected after an independent review: (i) the
  negative is restated as robust but **partly an externalization-granularity /
  candidate-selection artifact** (`PDF_DEFLATE_REPLAY` is in the current set and,
  forced, flips `shared-payload` to a win over raw CDC); (ii) the "only win is
  byte-identical opaque repeats" wording is replaced by the accurate statement;
  (iii) the compressed-CDC citation is a **non-deterministic range**
  (210,835–210,840 B), with the deterministic raw figure (`771,383 B`) as the
  primary baseline. No measured campaign number in `results.json` was altered.

## [0.1.0-alpha.10] — unreleased

Phase 8 — **seek-based partial I/O** — plus the Phase-8.4 seekable-baseline
amendment that restates its result. Exactness is unchanged: the prime directive
is still `materialize(descriptor) == original_bytes`, and whole-file compression
remains a recorded negative against generic lossless tools (ADR-0017). Phase 8
adds one optional wire record (the seek `DIRECTORY`) and a `Read + Seek` reader,
and measures a new axis (random-access bytes read). The honest result is scoped:
a bytes-read win **only versus non-seekable sequential** codecs.

### Added

- Phase 8.1/8.2 — seek `DIRECTORY` record and `Read + Seek` reader (ADR-0019):
  - An optional `seek_directory_v1` record (`RecordTag::Directory = 0x71`) written
    as the first record at fixed offset 64 with `FLAG_OPTIONAL`, carrying
    LOCATORS, a CLASS_INDEX, and CHANNEL_LENGTHS. Its position is a constant, so
    no header field is consumed and `FORMAT_MINOR` does not bump; a new ignorable
    `FEATURE_SEEK_DIRECTORY` optional bit is set, and a decoder that ignores the
    record still materializes exactly. Bounded by `Limits::max_directory_bytes` /
    `max_directory_entries`.
  - `materialize_observation_seeked` (`src/materialize/seek.rs`) serves the same
    narrow observation as Phase 7.3 from a `Read + Seek` source. The directory is
    **advisory, never authority**: locators are cross-checked against record
    framing, the class index against a linear scan, and the observation index is
    re-derived over directory-derived lengths; a lying directory is rejected and
    a missing/oversized one declines (`UnsupportedFeature`). A partial read is an
    *observation* (`integrity_verified == false`); `materialize`/`decode`/`verify`
    remain the archival authority.
  - `view` peeks only the 64-byte header before choosing the seek path, so a
    seekable descriptor is never `fs::read` whole. New Phase-8 universe suffix
    `+seek-directory-v1`; non-seek serialization re-bases only by the universe
    length (+18 B on the stress corpus).
- Phase 8.3 — seek bytes-read court: `strace` added to the opt-in `baseline`
  image; `tools/seek-court.sh` / `tools/seek-table.jq` record the instrumented
  `bytes_read`, a descriptor-file-attributed `strace -P` cross-check, syscall
  count, wall/CPU/peak-RSS, and sequential gzip/zstd/xz compressed-prefix
  baselines.
- Phase 8.4 — seekable/blocked random-access baseline (the honest comparison):
  `bgzip` (htslib 1.16 via the `tabix` package) and `pixz` 1.0.7 added to the
  pinned `baseline` stage; `tools/seekable-baselines.sh`,
  `tools/bgzf-seek-probe.pl`, `tools/xz-seek-probe.pl`,
  `tools/xz-block-reframe.pl`, `tools/seekable-table.jq` build and measure
  `bgzip -l 9` (BGZF), `xz --block-size=64KiB|1MiB|4MiB`, and `pixz`.

### Measured

- Campaign `2026-10-05-phase8-seek-08de2a9` (seek bytes-read court; ADR-0019) —
  on a 33,789,340 B (32.22 MiB), 800-stream deterministic PDF (seekable
  descriptor 17,566,832 B), the seeked `view` reads a **constant
  439,679–461,367 B** for all 18 pre-registered queries (18/18 byte-exact; floor
  = header 64 + DIRECTORY 27,390 + GRAPH 265,462 + OBSERVATION_INDEX 146,711 +
  INTEGRITY 52). That is **4.7 %–~21× fewer bytes than gzip's compressed prefix**
  and ~12–13× fewer than zstd/xz in the late region; CPU ~0.00 s and peak RSS
  ~3.8 MB (from ~38 MB). The `strace` cross-check equals the instrumented count +
  exactly 64 B (the header peek). **Restated (Phase 8.4):** this is a win only
  against **non-seekable sequential** codecs. Against **seekable/blocked**
  formats, the same late query (`--byte-range=32505856:256`; VOLE 460,713 B)
  reads **more**: bgzip 23,808 B (~19×), `xz --block-size=64KiB` 15,344 B (~30×),
  `xz 1MiB` 179,892 B (~2.6×); only `xz 4MiB` (708,612 B) and pixz 2,810,832 B
  (16 MiB blocks) read more than VOLE. **Not a general random-access-I/O win.**
  It also loses at `a = 0` vs gzip (327,680 B) and xz (73,728 B) and at early
  queries (≤ ~1.7 MiB) vs xz's prefix. `zstd --seekable` does not exist in zstd
  1.5.4. One locally generated corpus; no population claim. Receipts under
  `evidence/campaigns/2026-10-05-phase8-seek-08de2a9/` (`report.md`,
  `query-table.md`, `seekable.jsonl`, `seekable-table.md`, `seekable-report.md`);
  reports `docs/evidence/phase8-seek-report.md`,
  `docs/evidence/phase8-skeptic-review.md`.
- **Whole-file remains a recorded negative (ADR-0017).** The best VOLE lane on
  this corpus is **3.01×** xz, and even the seekable BGZF archive (7,995,600 B)
  is smaller than the 17,566,832 B seekable descriptor (0.46×). The directory
  buys query capability, not size.

### Notes

- **Claim discipline.** The seeked `view` reads a *floor* dominated by
  GRAPH + OBSERVATION_INDEX + DIRECTORY: a 256-byte request still incurs
  ~440 KB (~1,700×), and the write-up records where it loses at offset 0, at
  early queries, and against compact-block seekable formats. Independent review:
  `docs/evidence/phase8-skeptic-review.md` (independent adversarial reviewer,
  Phase 8).
- **Validator caveat.** A *referenced* channel's `decoded_length` is
  cross-checked against its record (`InvalidContainer` on disagreement); an
  *unreferenced* `CHANNEL_LENGTHS` entry is not validated. Benign: `analyze_ops`
  never uses an unused length, so no wrong bytes are served.
- No DRA, candidate, or entropy semantics changed in Phase 8.

## [0.1.0-alpha.9] — unreleased

Phase 7 is **hardening plus a pivoted result**. It closes two Phase-6 unknowns
(behaviour on a producer corpus, and coverage-guided fuzzing), then retires the
whole-file *compression* claim honestly and measures a different axis —
random-access decode cost. Phase 7 adds no wire record beyond the optional,
advisory `OBSERVATION_INDEX` (Phase 7.3). Phase 8 (branch `phase8`, ADR-0019)
adds a second optional wire record, the seek `DIRECTORY`, and a seek reader; its
measured **bytes-read** result is recorded under Measured below. Exactness is
unchanged in both phases.

### Added

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
  - `tools/pdf-corpus.sh` builds a locally-generated, producer-stratified corpus
    from distinct lineages (Ghostscript 10.00.0 at
    `/default`/`/prepress`/`/printer`/`/ebook`/`/screen`, qpdf 11.3.0
    compress/linearize/object-streams=preserve/nocompress, a hand-written
    stored-block-zlib base, plus the Phase-3 synthetic set), validating every
    produced PDF with `qpdf --check` and writing a provenance ledger
    (`provenance.json`). The regenerable `.pdf` bytes are gitignored; no
    third-party bytes. `--deterministic-id` makes qpdf outputs byte-reproducible;
    Ghostscript `pdfwrite` output is not (per-run `/ID`, recorded in the ledger).
  - Harness tests in `tests/deflate_stats.rs`; the real complete-cost court is
    `tools/pdf-court.sh` (`encode FILE OUT` and
    `encode --force raw|byte-rans|pdf-deflate-replay|pdf-deflate-replay-rans FILE
    OUT`), which records each lane's complete `.voldoc` size and `verify` +
    `decode`/`cmp`s the auto winner byte-exact. A forced kind the input does not
    propose is a typed `Usage` decline recorded as `null`.
- Phase 7.0b — generator-family Flate corpus (real *authoring* generators):
  - A separate, opt-in `producers` `Dockerfile` stage / compose service
    (`vole-document/producers:bookworm`, base
    `debian:bookworm-slim@sha256:3783cc01…` — the same digest as `tools`)
    carrying generators with distinct DEFLATE behaviour: **ReportLab 3.6.12**,
    **Cairo 1.20.1/libcairo 1.16.0**, **LibreOffice Writer 7.4.7.2**, **pdfTeX
    3.141592653-2.6-1.40.24** (plus qpdf 11.3.0 for `/ID` normalization). It is
    deliberately **not** part of the fast `tools` gate; the image grows to
    ~724 MB. All four families built and ran; none was skipped.
  - `tools/pdf-corpus-producers.sh` generates one PDF per available family from
    the same deterministic content document as `tools/pdf-corpus.sh`, fingerprints
    every `/FlateDecode` payload (`qpdf --json` + `qpdf --show-object
    --raw-stream-data`), applies `qpdf --deterministic-id --stream-data=preserve
    --object-streams=preserve` **only** when it leaves every payload
    byte-identical, builds each family twice to record byte-reproducibility,
    `qpdf --check`s every output, and writes a provenance ledger. LibreOffice
    output is not byte-reproducible (run-varying metadata); the caveat is
    recorded. The regenerable `.pdf` bytes are gitignored; no third-party bytes.
  - `tools/pdf-producers-analyze.py` reduces the `deflate-stats` and complete-cost
    court outputs to a per-producer analysis. No wire format, candidate, or
    decode path changed.
- Phase 7.0c — generic-compressor baseline ladder (ADR-0017):
  - A pinned, opt-in `baseline` `Dockerfile` stage / compose service
    (`vole-document/baseline:1.99.0`, derived from the `dev` stage on base
    `rust:1.99.0-slim-bookworm@sha256:452176c0…`, so rustc/cargo match the
    measurement binary) adding the generic compressors `gzip`, `zstd`, `xz` and
    `brotli` (plus `jq`). It is deliberately **not** part of any fast gate.
  - `tools/baselines.sh` records, for every corpus file, the source size and the
    smallest **lossless** complete-file size for `gzip -9`, `zstd -19 --long=27`,
    `xz -9e` and `brotli -q 11` (each decompressed and `cmp`'d against the source
    before it is scored), plus the complete serialized `.voldoc` size of every
    VOLE lane (auto, `raw`, `rle`, `byte-rans`, and every forced structural kind).
    `tools/baselines.jq`, `tools/baselines-merge.jq` and `tools/baselines-table.jq`
    reduce and render the JSON table.
- Phase 7.1 — coverage-guided fuzzing:
  - A standalone `cargo-fuzz` package in `fuzz/` (kept out of `cargo package` by
    `exclude = ["fuzz", "research", "evidence"]`) with ten libFuzzer targets:
    `voldoc_parse`, `voldoc_roundtrip`, `dra_program`, `rans_model`,
    `rans_channel`, `pdf_lexer`, `pdf_scan`, `pdf_xref`, `deflate_replay`,
    `materializer`. Each asserts the hostile-input invariants (never panic or
    report an internal invariant, bounded work, deterministic, reconstructed
    length == declared length, unknown versions/features/codecs fail closed).
  - A pinned **dated nightly** fuzz toolchain: a `fuzz` `Dockerfile` stage on
    `rustlang/rust:nightly-bookworm-slim-2026-10-04@sha256:58f725c9…`
    (`rustc 1.101.0-nightly (db8f076d2 2026-10-03)`, `cargo
    1.101.0-nightly`) plus `cargo-fuzz 0.13.2` and `libfuzzer-sys 0.4.13`, wired
    as the `fuzz` compose service (host network, a dedicated `fuzz/target`
    volume). `RUSTUP_TOOLCHAIN` overrides the root `rust-toolchain.toml`.
  - `tools/fuzz.sh` runs a bounded campaign (`FUZZ_SECONDS`/`FUZZ_RSS_MB`, default
    60 s / 2048 MiB per target), copies the committed `fuzz/seeds/` into each
    target's corpus, and records duration, coverage, execs, and crash/OOM counts.
  - Five small committed seed inputs under `fuzz/seeds/`; regeneration is
    documented in `fuzz/README.md`.
- Phase 7.1b — process-isolated DEFLATE replay (contains fuzz finding F2):
  - The **decode path** no longer calls `preflate` in-process. A hidden worker
    subcommand `__replay-worker` (not advertised in `USAGE`) reads a framed
    `(plaintext, corrections, declared_len)` request from stdin, calls
    `recreate_whole_deflate_stream`, and writes a framed reply to stdout. Framing
    is little-endian: request `[u32 plaintext_len][plaintext][u32
    corrections_len][corrections][u32 declared_len]`; reply `[u8 status]
    [u32 payload_len][payload]` (`0` = raw DEFLATE, `1` = a UTF-8 error message).
    Field lengths above a small internal bound are refused before allocation, and
    a preflate panic is caught by a non-aborting hook and returned as an error
    reply instead of an abort with lost output.
  - `replay_bounded(plaintext, corrections, declared_len, limits)` spawns
    `sh -c 'ulimit -v <KB> 2>/dev/null; ulimit -t <SEC> 2>/dev/null; exec "$0"
    __replay-worker' <worker>` with `stdin`/`stdout`/`stderr` piped, an
    `RLIMIT_AS` address-space cap, and a polled wall-clock timeout that `kill()`s
    on expiry. A non-zero exit, an abort (SIGABRT, the likely memory-cap case), a
    malformed reply, or a timeout each yields a typed `CodecReplay` error; a child
    `status=1` message is surfaced verbatim.
  - The decoder (`Program::eval`'s `DEFLATE_REPLAY` arm) calls `replay_bounded`;
    the in-process `replay_raw` is retained for the encoder's own `try_replay`
    verification and the fuzz targets. No wire format, candidate, or DRA op
    changed.
  - Knobs and defaults: `VOLE_REPLAY_WORKER` (worker executable; the CLI sets
    itself via a safe library setter when unset, since `std::env::set_var` is
    `unsafe` under Rust 2024), `VOLE_REPLAY_MEM_MB` (address-space cap; default
    `clamp((plaintext+corrections+declared) * 8 + 64 MiB, 256 MiB,
    limits.max_replay_bytes.min(2 GiB))`), `VOLE_REPLAY_TIMEOUT_MS` (default
    30000). With no worker configured the library falls back to the in-process
    path; a configured-but-broken worker is a typed error and **never** silently
    falls back.
  - Courts: `tests/replay_isolation.rs` (positive byte-exact decode through the
    worker, the F2 fixture failing closed under a 32 MiB cap, no silent fallback
    on a broken worker, and the documented in-process fallback) plus the
    worker-protocol framing round-trip unit test in `src/codec/deflate.rs`.
- Phase 7.3 — partial materialization and observation views (ADR-0018):
  - `Program::analyze_ops(object_lens, channel_lens, limits) -> Result<Vec<u64>>`
    returns the exact output length each instruction produces, sharing one walk
    with `analyze` so every existing rejection is unchanged.
  - A new optional, advisory `OBSERVATION_INDEX` record (tag `0x70`, written with
    `FLAG_OPTIONAL`, placed after `GRAPH` and before `INTEGRITY`) carries an op
    table, PDF selectors, and output-block digests gated by a `section_flags:
    u8`. It is **checked, never authority**: `Descriptor::parse` re-derives each
    `out_len` via `analyze_ops`, checks dependency ids and selector/digest ranges
    against the analyzed total, and rejects any contradiction with
    `CoverageViolation`. A decoder that ignores it still materializes exactly.
    `Descriptor` gains `observation_index: Option<ObservationIndex>` and an
    optional header feature bit `FEATURE_OBSERVATION_INDEX`; `cost.index` charges
    the record payload + framing, and `cost.total()` remains exactly the
    serialized length.
  - `materialize_observation` / `view_to_bytes`
    (`src/materialize/observation.rs`) serve one output range and report measured
    `ObservationStats` (ops/channels/objects touched, entropy bytes decoded,
    descriptor bytes traversed, work amplification). A descriptor without an
    index is **declined**, never silently fully materialized; when the program is
    a sequence of linear independent ops, only the ops intersecting the range are
    evaluated and only the referenced entropy channels are decoded.
  - `vole-document view INPUT.voldoc [OUTPUT] --byte-range A:L | --pdf-object N:G
    | --pdf-stream N:G | --pdf-revision I [--stats]`; the `--stats` JSON reports
    the resolved `range_start`/`range_len` so a harness can price sequential
    baselines at the same output offset.
  - `PDF_DEFLATE_REPLAY_RANS_INDEXED`
    (`encode --force pdf-deflate-replay-rans-indexed`) attaches the advisory
    observation index built from the same physical scan.
  - `vole-document pdf-make-large DIR [OBJECTS]`: an encode-time, binary-only
    deterministic generator of a valid classic-xref PDF with `OBJECTS` (default
    800) distinct real-zlib `FlateDecode` streams and ≥32 MiB of source; correct
    `/Length` and offsets by construction; no library or runtime dependency; the
    regenerable `.pdf` bytes are gitignored.
- Phase-7 universe string
  `vole-document;universe;phase7;exact-bytes;dra-8;opaque+entropy+pdf+channels+offsets+packed+packed-channels+deflate-replay-preflate-0.7.6-experimental+observation-index-v1`
  (DRA stays v8; no existing candidate bytes change beyond the universe bump and
  the optional index record).
- Phase 8 — seek-based partial I/O (branch `phase8`, ADR-0019):
  - An optional seek `DIRECTORY` record (`RecordTag::Directory = 0x71`,
    `seek_directory_v1`) written as the first record at fixed offset 64 with
    `FLAG_OPTIONAL`, carrying per-record LOCATORS (tag, offset, payload_len), a
    CLASS_INDEX for O(1) class lookup, and CHANNEL_LENGTHS. Its position is a
    constant, so no header field is consumed and `FORMAT_MINOR` does not bump; a
    new ignorable `FEATURE_SEEK_DIRECTORY` optional bit is set and a decoder that
    ignores the record still materializes exactly. Descriptor serialization
    becomes a two-pass build when the directory is enabled, and `cost.directory`
    charges its payload + framing so `cost.total()` stays exactly the serialized
    length. Bounded by `Limits::max_directory_bytes` / `max_directory_entries`; an
    oversized directory is declined before allocation.
  - `materialize_observation_seeked` (`src/materialize/seek.rs`) serves the same
    narrow observation as Phase 7.3 but from a `Read + Seek` source, reading only
    the header, DIRECTORY, GRAPH, OBSERVATION_INDEX, INTEGRITY, and the referenced
    OBJECT/ENTROPY_CHANNEL/MODEL records. The directory is **advisory, never
    authority**: locators are cross-checked against record framing, the class
    index against a linear scan, and the observation index is re-derived over
    directory-derived lengths; a missing/oversized/lying directory is declined
    (`UnsupportedFeature`) or rejected (`InvalidContainer`), never silently fully
    read. The report carries a real `bytes_read` (an internal `CountingReader`)
    and `integrity_verified == false` — a partial read is an *observation*, not
    an archival verification.
  - `view` peeks only the 64-byte header before choosing the seek path, so the
    CLI no longer `fs::read`s the whole descriptor when it is seekable.
  - New Phase-8 universe suffix `+seek-directory-v1`; the serialized record
    sequence for `seek_directory == false` is otherwise byte-identical to
    Phase 7 (non-seek lanes re-base only by the universe length, +18 B here).
  - Measurement tooling: `strace` added to the opt-in `baseline` image, and
    `tools/seek-court.sh` / `tools/seek-table.jq` record the instrumented
    `bytes_read`, a descriptor-file-attributed `strace` cross-check, syscall
    count, wall/CPU/peak-RSS, the Phase-7 stats, and sequential gzip/zstd/xz
    compressed-prefix baselines.

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
  This moves the Phase-7.0 exact-replay acceptance from **11/17** to **24/24**
  (`1.000`) and raises the stream census (the old over-read had hidden whole
  stream objects). New lexer tests cover the no-EOL case, the normal
  `\nendstream` case, an `endstream` at EOF, and an `endstream`-like run lacking
  a right terminator; a new integration test builds a PDF whose payload abuts
  `endstream` and checks the exact scan and byte-exact `try_replay`.
- **ADR-0016 correction (Phase 7.0).** The `DEFLATE_REPLAY` output bound is a
  VOLE **replay-profile admission limit** —
  `min(max_output_bytes, max_replay_bytes, 2*P + 1024)` for a `P`-byte plaintext
  — a deliberate **policy** bound, **not an RFC 1951 maximum**. RFC 1951 permits
  arbitrarily many **empty non-final stored blocks**, so a legal bitstream that
  inflates to zero bytes can be arbitrarily large and no finite
  `f(decompressed_size)` bound exists. `2*P + 1024` is an algorithmic expansion
  figure, not a theorem. The alpha.8 prose ("statically rejected above
  `2*P + 1024`") is superseded; the limit may decline a legitimate exact replay
  whose real output exceeds the profile, and that decline is an honest admission
  cost. No behavior changed — the re-framing makes the existing bound's nature
  explicit.
- Fuzz finding **F1** (upstream `preflate-rs` 0.7.6 `1 << params.window_bits`
  shift-overflow on hostile corrections): mitigated by installing a
  non-aborting panic hook in the fuzz targets so `replay_raw`'s documented
  `catch_unwind` boundary is exercised, with a minimized 35-byte regression
  fixture (`tests/fixtures/deflate_replay_shift_overflow.bin`) and a test
  asserting a typed `CodecReplay` error.

### Measured

- Campaign `2026-10-05-phase7-corpus-b-c4eb77e` (amendment; verdict RECORDED;
  diagnostic, no new candidate) — re-measured after the lexer stream-boundary
  fix: **24 `FlateDecode` streams, 24 replayed / 0 declined (acceptance 1.000)**
  across Ghostscript 10.00.0, qpdf 11.3.0, a hand-written stored-block-zlib base,
  and the Phase-3 synthetic set; the census rose 17 → 24 because the old
  over-read had swallowed whole stream objects. Corpus-wide
  `correction/compressed` **p10 0.000320 / p50 0.014716 / p90 0.097360** (the
  `0.004518` figure is the `pdf-make-samples` subset median only). The Phase-6
  win region appears **only in our own hand-authored fixtures**
  (`hand-base2.pdf` deduped rANS 55,531 vs naive 111,062; `_synthetic/flate.pdf`
  34,051 vs 89,437); `qpdf-preserve-objectstreams.pdf` shows the same geometry
  only because qpdf copied and renumbered the fixture's two byte-identical raw
  streams (`ec028dc1…`), so **99.93% of that win is inherited**, not produced by
  a transformer. Supersedes the original `2026-10-05-phase7-corpus-f1f8d26`
  (11/17), which is retained, not rewritten. Report
  `docs/evidence/phase7-corpus-report.md`; review
  `docs/evidence/phase7-skeptic-review.md`.
- Campaign `2026-10-05-phase7-court-99dc72e` (verdict RECORDED; measurement, no
  new candidate) — the decisive **complete-cost court** over the 23-file locally
  generated producer corpus. `PDF_DEFLATE_REPLAY_RANS` vs `BYTE_RANS`:
  **win 3 / lose 8 / decline 12 — all 3 wins self-authored.** The wins are
  exactly the Phase-6 shared-plaintext geometry: `hand-base2.pdf`
  (112,011 → 56,885, −55,126 B), `_synthetic/flate.pdf` (49,291 → 36,102,
  −13,189 B), and `qpdf-preserve-objectstreams.pdf` (112,147 → 56,980,
  −55,167 B) — the last only because qpdf `--object-streams=preserve` copied the
  fixture's two byte-identical raw streams, **99.93% inherited**. Every
  **genuinely transformed** producer output loses or declines (all 5 Ghostscript
  variants and both qpdf compression variants lose; the 12 files with no
  replayable Flate lane decline). All 23 auto winners `verify` + `cmp`
  byte-exact. The corpus is locally generated and is **not** a population
  sample; qpdf/Ghostscript are transformers. Receipt under
  `evidence/campaigns/2026-10-05-phase7-court-99dc72e/`.
- Campaign `2026-10-05-phase7-producers-e071250` (verdict RECORDED; measurement,
  no new candidate) — the Phase-7.0b **generator-family** corpus and
  complete-cost court over four real *authoring generators* (ReportLab 3.6.12,
  Cairo 1.20.1/libcairo 1.16.0, LibreOffice Writer 7.4.7.2, pdfTeX
  3.141592653-2.6-1.40.24). Exact-replay acceptance is **87/87 = 1.000**
  (corpus-wide `correction/compressed` p10/p50/p90 =
  0.034759/0.047945/0.068028). Under complete cost vs `BYTE_RANS`: win 1 / lose 3
  / decline 0 (Cairo −24,137 B, 58,711 → 34,574; ReportLab −3,312 B; pdfTeX
  −10,663 B; LibreOffice −302,474 B). **Corrected in Phase 7.0c:** the Cairo
  delta is a **repeated-identical-bytes harness artifact** — our generator drew
  one identical page six times, so Cairo emitted six streams byte-identical in
  *compressed* bytes as well as plaintext; generic LZ captures ~2× more
  (`gzip -9` 17,382 B, `zlib -9` 17,376 B, `xz -9e` 16,852 B) and `BYTE_RANS` is
  a weak order-0 baseline with no LZ. The withdrawn framing ("a genuine authoring
  application produces the win region", "first authoring-generator witness") is
  corrected: *a repeated-identical-bytes region already captured better by
  generic LZ, not the shared-plaintext-vs-distinct-compression mechanism.* All 4
  auto winners `verify` + `cmp` byte-exact; no population claim. Prose amendment
  in the receipt; review `docs/evidence/phase7b-skeptic-review.md`.
- Campaign `2026-10-05-phase7-baselines-7b9f662` (verdict RECORDED; measurement,
  no new candidate; ADR-0017) — the **generic-compressor baseline ladder** over
  27 corpus files (phase7 23 + producers 4), comparing complete files against
  `gzip -9`, `zstd -19 --long=27`, `xz -9e`, `brotli -q 11` (each round-trip
  verified lossless) and the best VOLE lane (minimum over auto/raw/rle/byte-rans
  and every forced structural kind). **The headline honest negative: the best
  VOLE lane beats gzip/zstd/xz/brotli on 0 files**; the best generic is smaller
  on every file, **+460,320 B** in total (phase7 +410,355; producers +49,965). On
  `cairo-vector.pdf` the best VOLE 34,574 B is **2.07×** brotli's 16,670 B; on
  `_synthetic/flate.pdf` 36,102 B is **1.91×** xz's 18,884 B. Prior "wins" were
  relative to the weak order-0 `BYTE_RANS` lane and do not survive the generic
  ladder. All 27 auto winners `verify` + `cmp` byte-exact. Receipt under
  `evidence/campaigns/2026-10-05-phase7-baselines-7b9f662/` (full table
  `baseline-table.md`); report section in `docs/evidence/phase7-corpus-report.md`.
- Campaign `2026-10-05-phase7-fuzz-ca6a92b` (verdict PASS_WITH_FINDINGS) — a
  bounded coverage-guided libFuzzer campaign (60 s/target, `-rss_limit_mb=2048`)
  over the ten targets in the pinned nightly `fuzz` service. **Nine of ten
  targets observed zero crashes** (e.g. `voldoc_parse` 23.3M execs, cov 89;
  `rans_model` 58.9M execs; `pdf_xref` cov 736 / ft 4229). `deflate_replay`
  reported two **upstream `preflate-rs` 0.7.6** findings: (F1) the
  shift-overflow panic, mitigated as described under Fixed; and (F2) an
  **unbounded reconstruction allocation** (a 33-byte hostile
  `(plaintext, corrections)` pair, peak RSS 2532 MiB at the 2048 MiB policy),
  reported as an upstream limitation — `REPLAY_OUTPUT_RATIO` bounds the declared
  output, not the third-party decoder's internal allocation, and preflate 0.7.6
  has no bounded streaming reconstruction sink (ADR-0016). **F2 is now contained
  on the decode path** by Phase 7.1b process isolation; the same artifact run
  under the pinned `fuzz` service reports `libFuzzer: out-of-memory` with
  `1744830464` bytes in one allocation and `peak_rss_mb: 2524`, while a CLI
  `decode` under a 32 MiB worker cap fails closed as a typed error without
  exhausting the host. Fuzzing is evidence, not a proof of absence. No wire
  format or candidate changed. Receipt under
  `evidence/campaigns/2026-10-05-phase7-fuzz-ca6a92b/`.
- Campaign `2026-10-05-phase7-partial-a5764c9` (verdict SCOPED POSITIVE;
  measurement, no wire/candidate change beyond the index record) — the
  **partial-materialization query court** on a 33,789,340 B (32.22 MiB),
  800-stream deterministic PDF (`pdf-make-large`; 800 replayed / 0 declined;
  `qpdf --check` rc 0). All 18 pre-registered queries (6 byte-ranges at
  0/1/8/16/24/31 MiB + 8 `--pdf-stream` + 4 `--pdf-object`) are byte-exact. For
  mid/late queries the indexed lane touches **~0.41–0.43 MB**
  (`descriptor_bytes_traversed` alone; its `entropy_bytes_decoded` breakdown is a
  subset already counted there and must not be added) versus gzip inflating
  `a + len`: in the late region that is **1.4–2.3 %** of gzip's bytes, and VOLE
  is **~2–5× faster than gzip** and **~4–13× faster than xz** on decode CPU. It
  **loses** in the early region (≤ ~8–16 MiB), **never beats zstd's raw
  decompressor** on wall time, uses ~38 MB peak RSS versus gzip's ~1.2 MB, and
  whole-file size is still **2.98×** xz (best VOLE 17,392,713 B / indexed
  17,539,424 B vs xz 5,841,896 B). The decisive v1 caveat: `view` reads and
  parses the **whole** descriptor, so on-disk I/O is **not** reduced and
  `descriptor_bytes_traversed` is a CPU-side approximation, not a bytes-read
  figure. Receipt under
  `evidence/campaigns/2026-10-05-phase7-partial-a5764c9/`; report
  `docs/evidence/phase7-partial-report.md`; review
  `docs/evidence/phase7c-skeptic-review.md`; ADR-0018.
- Campaign `2026-10-05-phase8-seek-08de2a9` (verdict SCOPED POSITIVE on bytes
  read **versus non-seekable sequential codecs**; ADR-0019) — the **seek-based
  partial-I/O court**, closing ADR-0018's explicit no-I/O-win caveat. On the same
  33,789,340 B (32.22 MiB), 800-stream deterministic PDF as Phase 7.3, the
  seekable descriptor is **17,566,832 B** (the new DIRECTORY adds 27,390 B; index
  + directory = 174,101 B over the non-seek base). All 18 pre-registered queries
  are byte-exact. The seeked `view` reads a **constant 439,679–461,367 B**
  regardless of offset — exactly header 64 + DIRECTORY 27,390 + GRAPH 265,462 +
  OBSERVATION_INDEX 146,711 + INTEGRITY 52 at `a = 0`, plus one referenced channel
  + model where a channel is needed — which is ≤ 2.6 % of the descriptor for
  every query (H1 18/18). In the late region (≥ 50 % in, H2 8/8) that is
  **4.7 %–~21× fewer bytes than gzip's compressed prefix** (9,764,864 B vs
  460,713 B at 31 MiB) and ~12–13× fewer than zstd/xz. A `strace -P`
  descriptor-file cross-check equals the instrumented count + exactly 64 B (the
  header peek), so there is no hidden whole-file read. CPU drops to ~0.00 s and
  peak RSS from Phase 7's ~38 MB to **~3.8 MB**. **Amendment (Phase 8.4): this is
  not a general random-access-I/O win** — against seekable/blocked formats the
  same late query reads **more**: bgzip (BGZF) 23,808 B (~19×), xz
  --block-size=64KiB 15,344 B (~30×), xz 1MiB 179,892 B (~2.6×) all read less
  than VOLE's 460,713 B (only xz 4MiB 708,612 B and pixz 2,810,832 B read more),
  and BGZF's whole file (7,995,600 B) is smaller than the seekable descriptor.
  It also **loses on bytes at `a = 0`** versus gzip (327,680 B) and xz (73,728 B)
  and at early queries (≤ ~1.7 MiB) versus xz's tiny compressed prefix; the
  floor is constant in the offset and would dominate a descriptor below ~9 MB.
  Whole-file size is still **3.01×** xz. One locally generated corpus; **no
  population claim**. Receipt under `evidence/campaigns/2026-10-05-phase8-seek-08de2a9/`
  (`report.md`, `query-table.md`, `seekable.jsonl`, `seekable-table.md`,
  `seekable-report.md`); reports `docs/evidence/phase8-seek-report.md`,
  `docs/evidence/phase8-skeptic-review.md`; drivers `tools/seek-court.sh`,
  `tools/seekable-baselines.sh`.

### Notes

- **Honest framing.** VOLE's prime directive is exactness
  (`materialize(descriptor) == original_bytes`), and on whole-file size VOLE's
  best lane loses to every generic lossless compressor tested (ADR-0017). The
  pivot is deliberate: Phase 7 measures **random-access decode cost** and Phase 8
  the **random-access I/O cost** (bytes read), each against sequential
  decompression to the same offset. Both axes are separate receipts and separate
  verdicts and are never conflated. The Phase-8 bytes-read win is scoped to one
  large locally generated PDF and **only to non-seekable sequential codecs**: a
  purpose-built seekable/blocked format (BGZF ~24 KB; blocked xz 15–180 KB) reads
  **2.6–30× less** for the same late query, so it is not a general
  random-access-I/O win; it also loses at the start of the file and versus xz's
  early prefix.
- The wire format remains **PROVISIONAL** and is not frozen v1. `OBSERVATION_INDEX`
  (`observation_index_v1`) is optional and **advisory**: it is re-derived and
  re-checked against the authoritative program at parse and can only cause a
  `CoverageViolation`, never change what `materialize` produces. It costs bytes
  (146,711 B on the stress corpus) and does not make the descriptor smaller.
- The Phase-6 win over `BYTE_RANS` is real and byte-exact, but its enabling
  condition (a plaintext shared across streams **and** a large/weakly-coded
  appearance) is **not produced by the tested transformers**: on the Phase-7.0
  corpus every genuine transformer output loses or declines, and the Phase-7.0b
  Cairo "win" is a repeated-identical-bytes harness artifact already beaten by
  generic LZ. The `ADOPTED` status of `PDF_DEFLATE_REPLAY_RANS` therefore means
  "implemented and can win when shared plaintext is present", not "shown to win
  on real producer output". Closing that gap is Phase 7.2 (nested content
  proceduralization).
- **F2 residual.** Process isolation bounds the *decoder's* exposure: a
  malformed `.voldoc` cannot amplify memory in the decoder process because the
  third-party allocation runs in a child with a hard address-space cap and a
  timeout. It is **not** a proof that `preflate` is bounded. The residual is a
  library consumer that decodes untrusted `.voldoc` without a worker
  (`VOLE_REPLAY_WORKER` unset and no installed default): that path is in-process
  and still exposed. The CLI installs itself as the default worker.
- Fuzzing is **evidence, not a proof of absence**: nine zero-crash targets mean
  no crash was observed in that 60 s budget on that build, not that none exists.
  Both `deflate_replay` findings are upstream `preflate-rs` defects surfaced
  through our wrapper.
- No candidate is adopted, removed, or changed by the baseline ladder or the
  query-cost court; the only new encode-time code is the `pdf-make-large`
  generator and the optional index record.

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
    rejected above `2*P + 1024` for a `P`-byte plaintext *before* the engine runs
    (ADR-0016) and validated at evaluation; plaintext and corrections are bounded
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

### Measured

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
  and bounded; `PDF_DEFLATE_REPLAY_RANS` is `ADOPTED` as a
  winning candidate and `PDF_DEFLATE_REPLAY` is `RECORDED (rejected vs
  BYTE_RANS)`. This is the first PDF structural candidate to beat a whole-file
  order-0 rANS lane; the earlier converging negatives (ADR-0010–ADR-0013) apply
  to proceduralizing *plain* syntax, while exact replay attacks bytes that are
  *already* entropy-coded (ADR-0015).
- The `deflate-replay` feature is **opt-in**: the default build is
  `default = ["rans"]` (permissive-only), and `--features deflate-replay` (or
  `--all-features`) enables the lane; a build without it rejects the op with
  `UnsupportedFeature` (see ADR-0014 for the LGPL consequence).

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
