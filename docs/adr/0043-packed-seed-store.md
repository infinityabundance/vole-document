# ADR-0043: A segmented, offset-addressed packed fieldpack backend replaces the one-file-per-node seed store (seed namespace only)

- **Status:** Accepted — adopted (scoped: the seed namespace only) (Phase 15.3)
- **Date:** 2026-10-07

## Context

The persistent procedural field (ADR-0025) stores one immutable blob per seed
node in `FsSeedStore`. On the `real100-v1` corpus a single large document can
produce tens of thousands of nodes, which is a file-count and physical-byte
problem for both storage and open cost. The Phase-15 plan proposed an optional
immutable **segmented, offset-addressed** `fieldpack` backend whose identity is
unchanged: `NodeId -> (segment, offset, len)`. Reads are safe `pread`
(`read_exact_at`); the crate forbids `unsafe`, so there is deliberately **no
mmap** — the win is fewer files and few large contiguous segments, not mapping.

## Decision

Add an **optional** packed seed store behind `field-ingest`/`observe`/… as
`--packed`. It replaces the `seed/` namespace with a `fieldpack/` directory of
segments. Node identity (`NodeId = BLAKE3-256(...)`) is unchanged, so **field ids
are unchanged** and a packed store and a filesystem store of the same descriptor
are interchangeable. `--packed` is mutually exclusive with `--entropyfs`, and
`observe-batch` does not support it. Only the **seed namespace** is packed;
descriptor / manifest / index / cache remain files.

## Consequences

- **Measured result (receipt
  `evidence/campaigns/2026-10-07-phase15-packed-8c195e8/`):** a focused
  **12-document** subset (one per `(format, size_class)` stratum) with the **same**
  descriptor ingested into both roots:

  | metric | fs | packed | ratio |
  | --- | ---: | ---: | ---: |
  | persistent bytes (`du -sb`) | 352,671,955 | 253,737,799 | **0.719×** |
  | file count (`find -type f`) | 25,574 | 237 | **0.009×** (111× fewer) |
  | cold-observation wall | 1,555 ms | 1,541 ms | **0.991×** (parity) |

  Correctness: field id identical across backends **12/12**; byte-exact
  `materialize --exact` **12/12 both**.
- **The win is structural, not latency.** Packed is far smaller and emits ~111×
  fewer files at parity latency; it does **not** claim to be faster.
- **Scope.** Only the seed namespace is packed. The syscall summary is recorded
  (`strace -c -f`, one representative document), but the lane has `strace` and
  **not** `perf`, so no page-fault / physical-read counters are claimed.
- **Limits.** One 12-document subset, **not** the frozen population; no
  population claim.

## References

- `src/field/` (`FieldStore::open_packed`, `fieldpack`); `--packed` in
  `src/main.rs`; `docs/phases/phase-15-results.md` (15.3)
- `evidence/campaigns/2026-10-07-phase15-packed-8c195e8/`
- ADR-0025: the procedural seed DAG stored one blob per node
