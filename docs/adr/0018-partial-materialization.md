# ADR-0018: Partial materialization is a scoped random-access query-cost result, measured against generic compressors

- **Status:** Accepted — scoped positive on decode CPU/allocation, with an
  explicit no-I/O-win caveat (Phase 7.3)
- **Date:** 2026-10-05

## Context

Whole-file compression loses to generic tools on every file tested (ADR-0017),
so VOLE's pivoted claim is *random-access / query cost*: serving one narrow output
range without materializing the whole document. A descriptor may carry an
optional `OBSERVATION_INDEX` record (`observation_index_v1`) and the `view` CLI
serves a byte range, a PDF indirect object, an encoded stream, or a revision,
reporting measured `ObservationStats` (`src/materialize/observation.rs`).

The claim is falsifiable and must be measured on a large, multi-object document
against the honest alternative: a sequential decoder must inflate the whole
prefix to reach a late output offset.

## Decision

- **Adopt the mechanism for query serving; claim only decode work.**
  `materialize_observation` skips ops that do not intersect the requested range
  when the program is a sequence of linear, independent block producers (the
  replay lane's `INLINE · DEFLATE_REPLAY · INLINE` shape qualifies), and decodes
  only the referenced entropy channels. The index is advisory and re-checked
  against the authoritative program at parse time; every served slice is
  byte-compared against the full materialization.
- **The measurement corpus is a deterministic, large, multi-object PDF**
  (`pdf-make-large DIR [OBJECTS]`, default 800 pages/streams, ≥ 32 MiB, distinct
  per-stream zlib content). The regenerable bytes are gitignored; the ledger is
  committed.
- **Two axes, two verdicts.** Whole-file size (ADR-0017) is reported separately
  and remains a loss. Only the query-cost court is a potential win.
- **Do not claim an I/O win while v1 reads the whole descriptor.** The CLI reads
  the entire `.voldoc` into RAM (`fs::read`) and parses it, so
  `descriptor_bytes_traversed` is a **CPU-side approximation**, not a bytes-read
  figure. Until an mmap/seek reader lands, the honest metric is decode
  CPU/allocation, and the receipt must say so.

## Consequences

- **Measured result (Phase 7.3, receipt
  `evidence/campaigns/2026-10-05-phase7-partial-a5764c9/`):** on a 33,789,340 B
  (32.22 MiB), 800-stream, 800-replayed PDF, all **18/18** pre-registered queries
  are byte-exact. For mid/late queries the indexed lane touches ~0.41–0.46 MB
  (`descriptor_bytes_traversed + entropy_bytes_decoded`) regardless of offset,
  versus gzip inflating `a+len`: in the late region (≥ 50 % in) it is
  **1.4–2.3 %** of gzip's bytes and **~2–5× faster** than gzip / **~4–13×**
  faster than xz on CPU. Contract hypothesis H1 is met vs gzip and xz (8/8 late
  queries) and **not** met vs zstd.
- **Where it loses (recorded, not hidden):** in the early region (≤ ~8 MiB) the
  constant ~0.02–0.03 s / ~38 MB descriptor parse loses to gzip/xz/zstd, which
  process only 256 B–8 MB; **zstd's raw decompressor is faster than VOLE at every
  point** (≤ 0.01 s), at ~36 MB RSS; VOLE's peak RSS is ~38 MB versus gzip's
  ~1.2 MB; and whole-file size is still **2.98×** xz.
- **The decisive v1 caveat:** on-disk I/O is **not** reduced. At 31 MiB gzip
  reads a ~9.8 MB compressed prefix while VOLE reads its entire **17.5 MB**
  `.voldoc`. There is **no bytes-read win**. `descriptor_bytes_traversed` tracks
  bytes the path *needed* on the CPU side, not bytes fetched from storage.
- **Prerequisite for the next claim:** an **mmap/seek descriptor reader** so the
  index's record directory can be walked without materializing the whole
  container, making `descriptor_bytes_traversed` a real I/O figure. Only then can
  a "bytes read to serve a query" win be claimed; until then any such statement
  would be a CPU-cost claim in I/O clothing.
- **Neutral/degenerate regime acknowledged:** the stress corpus uses level-0
  (stored) zlib streams, so corrections are tiny (28 B/stream) and the corpus is
  the weak-producer regime. Distinct per-stream plaintext is what makes the
  query court non-degenerate; it is not a survey of strong real-world DEFLATE
  (cf. ADR-0017's producer corpus).

## References

- `src/materialize/observation.rs` (`materialize_observation`,
  `ObservationStats`), `src/container/observation.rs` (`observation_index_v1`),
  `src/adapter/pdf/replay.rs` (`propose_pdf_deflate_replay_rans_indexed`)
- `src/main.rs` (`pdf-make-large`, `view`), `tools/partial-court.sh`,
  `tools/partial-table.jq`
- Receipt `evidence/campaigns/2026-10-05-phase7-partial-a5764c9/`
- `docs/evidence/phase7-partial-report.md`
- ADR-0017: generic lossless compressors are the whole-file comparator
