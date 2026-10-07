# ADR-0044: Bounded parallel ingest is exactly deterministic and neutral on speed (non-default `parallel` feature)

- **Status:** Accepted — implemented, non-default; speed-neutral, determinism positive (Phase 15.4)
- **Date:** 2026-10-07

## Context

Ingest has naturally independent work: ZIP members, PDF streams, BLAKE3 hashing,
resource analysis, and ready DAG nodes. The Phase-15 plan proposed a bounded
Rayon pool over those sites, with morsel-sized tasks and a deterministic merge,
swept over 1/2/4/8/16 workers. A parallelism change is only admissible if it
cannot perturb the exactness path.

## Decision

Add the **non-default** `parallel` feature (`["field", "dep:rayon"]`) and a
`field-ingest --workers N` flag. The pool is used **only** when `--workers > 1`
(absent or `1` is serial; `0` means `available_parallelism`). The feature changes
no wire bytes, no persisted artifact, and no decoder behavior.

## Consequences

- **Measured result (receipt
  `evidence/campaigns/2026-10-07-phase15-workers-122c026/`):** a **10-document**
  subset; median speedup vs serial is **1.00× at 2/4/8** and **0.99× at 16**
  (the lane is capped at `cpus: 8`, so 16 oversubscribes it; the largest PDF
  reaches ~1.11× at w4). Parallelism is **neutral on wall time** on this corpus.
- **Determinism is POSITIVE and is the real product:** field id identical across
  **every** worker count **10/10**, and `materialize --exact` == source for
  **every** count **10/10**. Worker count provably cannot change the
  representation or the bytes.
- **One unexplained outlier is recorded, not hidden:** `nist-pdf-0004`
  (6 s / 14 s / 29 s / 14 s / 1 s across w1/w2/w4/w8/w16).
- **Limits.** One 10-document subset; no population claim. Non-default: a build
  without `parallel` is identical, and no descriptor depends on it.

## References

- `Cargo.toml` (`parallel = ["field", "dep:rayon"]`); `src/field/ingest*`;
  `--workers` in `src/main.rs`; `docs/phases/phase-15-results.md` (15.4)
- `evidence/campaigns/2026-10-07-phase15-workers-122c026/`
