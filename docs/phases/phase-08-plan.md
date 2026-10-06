# Phase 8 — Seek-based partial I/O

Branch: `phase8`. Base: `main` @ `26982aa` (`v0.1.0-alpha.9`).
Status: **IN PROGRESS**.

## Why

Phase 7.3 measured a real but partial win: `view` touches **~0.41–0.43 MB of
decode work regardless of offset** on a 33.8 MB PDF, but **v1 reads the whole
framed descriptor from disk** (17.5 MB), so bytes-read is a *loss* to `gzip`
(9.8 MB) at a late query. The skeptic was explicit: a bytes-read random-access
claim requires an mmap/seek reader. This phase supplies it.

The objective, stated as a falsifiable claim:

```text
bytes_read(view, query)  <<  bytes_read(sequential descriptor parse)
```

and, for small observations of large documents, a bytes-read win over
`gzip`/`zstd`/`xz` decompressing a prefix to the same offset.

## Deliverables

- A **seek directory** in the container: a bounded, length-delimited directory
  record plus directory offset/length carried in the existing header reserved
  bytes, so a reader can locate the records it needs with a single seek rather
  than a linear scan. Backward compatible: a descriptor with no directory is
  still fully, sequentially materializable; unknown/absent directory means
  "sequential only".
- A **seek reader** over `Read + Seek` that reads the 64-byte header, the
  directory, and then only the records a query needs (graph, the referenced
  objects, the referenced entropy channels, the observation index). It reuses
  the Phase-7 partial-decode logic; it does not change decode semantics.
- A CLI/stat surface that reports **actual bytes read** (distinct from the
  Phase-7 CPU-side approximation `descriptor_bytes_traversed`), plus a
  correctness guarantee that a seeked `view` slice equals the full-materialize
  slice byte-for-byte.
- Honest measurement against `gzip`/`zstd`/`xz` on the 33.8 MB corpus:
  bytes read, syscall read() bytes, wall/CPU time, peak RSS — including where
  the seek form loses (small descriptors, early queries, per-record framing).

## Invariants (unchanged)

- `materialize(descriptor) == original_bytes` (length + SHA-256 + byte compare).
- The directory is **advisory**: it is validated against the program/records and
  is never authority; a wrong directory must be rejected, not trusted.
- Docker only; one `phaseN` branch; commit + push per subphase; an independent
  skeptic tries to falsify the headline.

## Subphases

- **8.0** plan + research/design freeze (this file + an independent design
  subagent). ADR for the seekable layout.
- **8.1** directory record + header directory fields + two-pass serialization;
  universe bump; tests; re-base pinned values.
- **8.2** seek reader + `view` using it when a directory exists; bytes-read
  stats; correctness tests (seeked slice == full slice).
- **8.3** measurement campaign vs gzip/zstd/xz (bytes read, syscalls, time, RSS)
  + ADR + report.
- **8.4** skeptic + docs + release.

## Non-goals

Changing the DRA, candidates, or entropy semantics; making whole-file size
competitive with LZ (ADR-0017 stands); population claims from synthetic corpora.
