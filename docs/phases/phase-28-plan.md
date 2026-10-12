# Phase 28 — Execution, physical selectivity, and revision reuse

> **Status:** planned (frozen contract below). Branch `phase28`.
> Phase 27 closed the Phase-26 limitations and left **21 recorded detection
> findings**, the legacy-subset boundaries, and the Arrow legacy-framing decline.
> Phase 28 is the **performance programme**: it first finishes detection, then
> repairs the competitor envelope, then runs the **three decision experiments**
> that decide whether the larger architectural direction is justified — before any
> expensive rewrite. Same rule as every subphase: exactness is the invariant,
> negatives are recorded, no claim without evidence.

## The problem the evidence names

- Arrow IPC vs genuine `pyarrow`: **7.295× slower warm** — no competitive native
  columnar execution path.
- Parquet vs DuckDB: **no analytical win**; storage substantially worse.
- Phase 22.5 remote: **3.15× more bytes transferred** than page-level SQLite —
  physical dependency closure is too broad.
- Phase 22.6: **97.2 %** of persistent bytes is the exact-authority descriptor.
- Phase 22.7: **16/27 vs 27/27** correct grounded tasks — coverage, not tokens.

These are different populations and are **not** aggregatable. The next
investment-changing milestone is a resolved performance breakthrough, measured
against genuinely optimized competitors.

## The order (frozen)

```text
28.1  Detection completion      — widen logstream/asciidoc/cbor; CFBF fragmented root-mini-stream
28.2  Competitor envelope       — in-process harness; real Arrow for pyarrow/DuckDB; cold vs steady-state
28.3  Hot-path profiling        — attribute CPU/allocs/IO/RSS per stage (incl. the serve_source_range clone)
28.4  Remove proven waste       — selection before payload materialization (only if 28.3 proves it end-to-end)
28.5  EXPERIMENT 1 Fused kernel — native Arrow predicate/aggregate vs pyarrow/DuckDB on real Arrow
28.6  EXPERIMENT 2 Fragment locality — exact-authority fragment directory; minimum verified bytes for narrow requests
28.7  EXPERIMENT 3 Revision reuse — fragment-level dependency graph; reusable verified work on revision families
28.8+ Winning mechanisms        — build only what the three experiments justify (fused execution, physical
                                  fragmentation, incremental observation), each with its own falsifiable court
```

The three experiments (28.5–28.7) are **decision gates**: their outcomes set the
implementation order, not the attractiveness of the mechanisms on paper. A
negative gate is a publishable result and stops that branch of the programme.

## Acceptance (per subphase, unchanged)

Exactness is the invariant (`materialize == source`, length + SHA-256 + `cmp`,
after source **and** descriptor deletion, in a fresh process); Docker only,
digest-pinned and memory-capped; one branch (`phase28`) with commit-and-push per
subphase; a sealed receipt per court; **every raw sample retained**; no cap raised
to make a run pass; no claim without its estimator and interval (ADR-0054) and its
storage accounting (ADR-0049: sum of regular-file sizes, never `du -sb`; times in
microseconds). Every new mechanism must preserve deterministic identities, bounded
costs, and exact reconstruction — the failed Phase-15.6 adaptive-promotion
experiment is the standing warning against assuming more persistence saves work.

## Measurement discipline (28.2 onward)

- **In-process steady-state** is the headline; **cold-process** and **cold-cache**
  are reported separately. A native binary vs a freshly launched Python interpreter
  does not settle engine performance.
- The comparator gets the **same raw-source authority and provenance obligations**
  as the task requires, and may build whatever index it needs — with **all** its
  construction and storage costs charged.
- Each lane declares its **capability set**; missing / declined / incorrect /
  ungrounded answers are counted, not hidden.
- Report **p50/p95/p99**, peak RSS, allocations, and where possible CPU counters
  (`perf`) in the pinned container.
- Losses (workloads where SQLite or DuckDB is expected to win) stay in the
  published results.

## Scope honesty

Phase 28 does **not** assert VOLE is already faster than SQLite or DuckDB. It
tests, under equal contract, whether a provenance-preserving, physically
selective, fused observation executor can produce a resolved advantage — and
records the answer either way.
