# ADR-0019: Seek-based partial I/O makes a late observation view a measured bytes-read win

- **Status:** Accepted — scoped positive on bytes read (Phase 8.3), on one
  large locally generated PDF; the early/small-descriptor losses are recorded
- **Date:** 2026-10-05

## Context

ADR-0018 adopted partial materialization but was explicit that it was a
**decode-CPU** claim: v1 `view` read the whole framed descriptor (`fs::read`)
and parsed it, so `descriptor_bytes_traversed` was a CPU-side approximation and
on-disk I/O was **not** reduced. The stated prerequisite for any bytes-read
claim was a seek/mmap descriptor reader. Phase 8 supplies it.

The container is a sequence of length-delimited, CRC-framed records after a
fixed 64-byte header. Serialization already writes each record class
contiguously, so a reader that knows every record's offset can fetch only the
records a query needs. The design was frozen in
`research/subagents/phase-08/seek-layout-contract.md` before implementation.

## Decision

- **An optional seek `DIRECTORY` record at fixed offset 64.** A new
  `RecordTag::Directory = 0x71`, written `FLAG_OPTIONAL`, carries every record's
  absolute offset and payload length (LOCATORS), a per-class index
  (CLASS_INDEX), and each entropy channel's `decoded_length` (CHANNEL_LENGTHS),
  in format `seek_directory_v1`. Its position is a **constant** (64), so no
  header field is consumed; an ignorable `FEATURE_SEEK_DIRECTORY` optional bit
  is set in the header. `FORMAT_MINOR` does not bump. A decoder that does not
  understand it skips it (`None`/unknown optional arm) and still fully
  materializes, because the reconstruction program alone is complete.
- **The directory is advisory, never authority.** Every locator is cross-checked
  against the record it points at (tag byte and payload length), every record's
  own CRC32C is verified when read, the directory geometry is validated, the
  `CLASS_INDEX` is re-derived against a linear scan, and the `OBSERVATION_INDEX`
  is re-derived over the directory-derived object/channel lengths. A lying
  directory is **rejected, never trusted**. A descriptor with no directory, or
  a missing/oversized/unknown one, is **declined** with `UnsupportedFeature`;
  the reader never silently falls back to reading the whole file.
- **`view` is an observation, not a verification.** A partial read cannot
  recompute the whole-source SHA-256, so a served slice reports
  `integrity_verified == false` and is an *observation* consistent with the
  descriptor's own validated program/index/directory. `materialize`/`decode`/
  `verify` remain the archival authority. The reader reports a real
  `bytes_read` from an internal `CountingReader`.
- **The CLI really seeks.** `cmd_view` peeks only the fixed 64-byte header to
  learn whether the seek feature is advertised; a descriptor that advertises it
  is served by the seek reader over the same handle. It never `fs::read`s the
  whole descriptor on that path. A descriptor without the feature keeps the
  Phase-7 in-memory path.

## Consequences

- **Measured result (Phase 8.3, receipt
  `evidence/campaigns/2026-10-05-phase8-seek-08de2a9/`):** on the same
  33,789,340 B / 800-stream PDF as Phase 7, the seekable descriptor is
  17,566,832 B (the directory adds 27,390 B; index + directory = 174,101 B over
  the non-seek base). **All 18/18 pre-registered queries are byte-exact.** The
  seeked view reads a **constant 439,679–461,367 B** regardless of offset:
  439,679 B at `a = 0` is exactly header 64 + DIRECTORY 27,390 + GRAPH 265,462 +
  OBSERVATION_INDEX 146,711 + INTEGRITY 52, and a channel-bearing query adds one
  referenced channel + model. That is ≤ 2.6 % of the descriptor for every query
  (H1 met 18/18) and, in the late region (≥ 50 % in, 8/8), **4.7 %–~21× fewer
  bytes than gzip's compressed prefix** (9,764,864 B vs 460,713 B at 31 MiB) and
  ~12–13× fewer than zstd/xz (H2 met 8/8). CPU drops to ~0.00 s and peak RSS
  from Phase 7's ~38 MB to **~3.8 MB**. The `strace` cross-check, attributed to
  the descriptor file alone, equals the instrumented count + exactly 64 B (the
  CLI header peek) for every query — no hidden whole-file read.
- **Where it loses (recorded, not hidden).** At `a = 0` the constant floor
  (439,679 B) exceeds what **gzip** reads first (327,680 B) and what **xz** reads
  (73,728 B); at early queries (≤ ~1.7 MiB) it loses to **xz**'s tiny compressed
  prefix (253,952–368,640 B). The floor is constant in the offset and does not
  shrink with output size, so it would dominate a descriptor smaller than
  ~9 MB: this is a large-document mechanism, not a small-file one.
  `max_directory_bytes` (1 MiB) bounds the directory's growth on record-heavy
  descriptors. The sequential decoders' compressed-prefix figures are measured
  at a pipe and include a bounded read-ahead (an upper bound). OS page cache
  changes wall time, not `read()` byte counts.
- **Whole-file size is a separate, unchanged axis.** The best VOLE lane is
  **3.01×** xz on this corpus; the directory buys query capability, not size
  (ADR-0017 stands).
- **Non-seekable bytes are preserved up to the universe string.** A descriptor
  with `seek_directory == false` emits the same record sequence and payloads as
  before; only the `+seek-directory-v1` UNIVERSE suffix re-bases the whole file
  (+18 B on this corpus), a deliberate versioned change (SPEC.md's universe rule).

## References

- `research/subagents/phase-08/seek-layout-contract.md` (frozen design)
- `src/container/directory.rs` (`seek_directory_v1`, validation),
  `src/container/record.rs` (`RecordTag::Directory`, `read_record_at`),
  `src/container/header.rs` (`FEATURE_SEEK_DIRECTORY`),
  `src/container/descriptor.rs` (two-pass `serialize`, directory parse arm),
  `src/materialize/seek.rs` (`materialize_observation_seeked`, `CountingReader`)
- `src/main.rs` (`view`), `tools/seek-court.sh`, `tools/seek-table.jq`
- Receipt `evidence/campaigns/2026-10-05-phase8-seek-08de2a9/`
- `docs/evidence/phase8-seek-report.md`
- ADR-0018: partial materialization (the decode-CPU precursor);
  ADR-0017: generic lossless compressors are the whole-file comparator
