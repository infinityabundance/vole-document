# Phase 15 — Performance programme: repair the court, then earn the frontier

Branch: `phase15` (staging). Base: `main` @ `c6977cd` (`v0.1.0-alpha.19`).
Status: **IN PROGRESS**.

## Why

Phase 11–14 established a byte-exact, multi-format persistent `DocumentField`
and measured it honestly — but the `real100-v1` frontier court was taken with an
**unoptimized debug binary** and a **per-query-process** model, and its storage
accounting folded the transient `.voldoc` into the persistent footprint. Those
are asymmetries, not architecture. Phase 15 repairs the measurement first, then
earns the economic frontier, in order:

> make one persistent reconstructive representation cheaper over a document's
> **lifetime** than maintaining, updating, reopening and storing a growing set of
> purpose-specific materialized views.

The defensible question is **not** "SQLite cannot represent this" but "how much
state must a conventional database materialize *in advance* to offer the same
future capability surface?" — the precompute–recompute frontier.

## Rules (unchanged, enforced)

Exactness is the invariant (`materialize == source`, length + SHA-256 + `cmp`).
**Docker only** — never `cargo`/tools/corpus work on the host; every command runs
in a digest-pinned, memory-capped compose service. One production crate. One
branch per phase; commit + push after every subphase. Subagents: **one at a
time**, **read-only research**, in the **minimal capped `tools`/`realcorpus`
lane** — never on the host. Negative and partial results are recorded, never
buried. No claim without a sealed receipt; no tuning of the population against
the measurement.

## Subphases

- **15.0** plan + stale-documentation sweep (README Current status).
- **15.1 Court repair.** Re-run the frozen `real100-v1` court unchanged in
  **release** mode (`cargo build --release --locked --all-features`); report
  **persistent required** vs **transient ingest** vs **optional standalone
  descriptor** storage universes separately (ADR-0027 four universes, extended);
  add `perf stat`-class counters where the lane provides them. Receipt:
  `real100-release-baseline`.
- **15.2 Resident runtime.** `DocumentFieldSession` (open field + store, mapped
  indexes, parsed manifest, typed-model / decoded-member / validated-node caches,
  reused buffers) and an `observe-batch` API; a resident court (`text_repeat` and
  a mixed session as **one** process, versus a resident SQLite connection with
  prepared statements). Receipt: resident vs cold.
- **15.3 Packed field store.** An optional immutable segmented, mmap-able
  `fieldpack` backend (identity unchanged): `NodeId -> (segment, offset, len)`;
  measure syscalls, page faults, physical reads, persistent bytes, latency vs the
  one-file-per-node reference.
- **15.4 Parallel CPU.** A bounded Rayon pool; parallelize independent ZIP
  members, PDF streams, BLAKE3 hashing, resource analysis, and ready DAG nodes;
  morsel-sized tasks, deterministic merge. Sweep 1/2/4/8/16 workers on the
  author's 8-core/16-thread machine.
- **15.5 SIMD + Deflate court.** Safe-API SIMD (`memchr`, `simdutf8`) in the
  structural scanners; a DEFLATE ablation (`miniz_oxide` current/SIMD, `zlib-rs`,
  `zune-inflate`) over real `real100` members. Report per-hot-loop and end-to-end.
- **15.6 Adaptive procedural promotion.** Persist compact **structural tapes**
  (not just final answers); a cost governor promotes a reusable intermediate only
  when expected future saved work exceeds materialization + storage rent.
  Unforeseen-query-diversity court vs SQLite-Minimal/Full/Adaptive.
- **15.7 Durable cross-root derivations.** Canonical derivation identity
  `(algorithm-version, dependency NodeIds, parameters)` so identical inputs share
  *computed* state, not merely representation identity; re-run `N3` on the real
  publication/revision families.
- **15.8 CUDA batch lane.** Only if CPU profiling shows decode/scan bandwidth
  dominates; batch independent members/chunks, overlap copies, and require a CPU
  crossover below which CUDA declines automatically.

Cross-cutting: **contract-equivalent benchmarking** (C0 value → C5 batch) so
speed is only compared where both systems satisfy the same contract; the
**query-diversity frontier** and the **revision court**. Final metric: a lifetime
cost `C = C_ingest + Σ C_queries + Σ C_updates + C_storage + C_io` and its
break-even in `queries/document`, `sessions/document`, `revisions/document`.

## Acceptance

Each subphase: exactness preserved where it touches the exactness path; behaviour
changes are behaviour-preserving where claimed (pinned by the existing courts);
a sealed receipt with base image digest, toolchain, `Cargo.lock` SHA-256, commit
and dirty state; negatives/partials recorded; docs reconciled. Phase complete only
when every listed subphase is measured and the release is honestly scoped.
