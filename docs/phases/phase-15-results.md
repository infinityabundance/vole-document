# Phase 15 results

Branch: `phase15`. Base: `main` @ `c6977cd` (`v0.1.0-alpha.19`, Phase 14).
Plan: [`phase-15-plan.md`](phase-15-plan.md).

Phase 15 is a **performance programme**: it repairs the performance measurement
first, then measures whether the economic frontier can be earned. Each subphase
is measured to a sealed campaign; negatives and partials are recorded, not
buried. No claim here is a population claim — every corpus is frozen and every
number links to a receipt under
[`evidence/campaigns/`](../../evidence/campaigns/).

## What Phase 15 established

- **The court is repaired.** The frozen `real100-v1` court now runs on a
  **release** binary and reports the storage universes **separately** (persistent
  field store / optional standalone descriptor / transient ingest), never folded
  (ADR-0027 extended). Coverage and exactness are *identical* to the debug court
  — they are deterministic.
- **Two structural wins, both scoped:**
  - the **packed seed store** (15.3): persistent bytes **0.719×**, file count
    **0.009×** (111× fewer), latency at parity, exactness and field id unchanged;
  - **`zlib-rs` decompression** (15.5): **1.58×** the `miniz_oxide` scalar
    inflate rate at **1.00×** RSS, byte-identical — it **meets** the pre-registered
    bar and is the recommended backend swap (recommended, not yet adopted).
- **Two substantive negatives:**
  - **residency** (15.2): a resident session wins only below ~1 MiB; the cold
    lane is faster at 1–100 MiB (aggregate 7 ms cold vs 9 ms resident);
  - **adaptive promotion** (15.6): all three pre-registered falsifiers fire; the
    mechanism ships opt-in and default-off.
- **One deferral:** CUDA (15.8) is **not measured**. The gate is unopened and the
  pinned Docker lanes on this host cannot see the GPU, so no Docker-reproducible
  evidence can exist.

## Plan and stale-documentation sweep (15.0)

`docs/phases/phase-15-plan.md` was frozen as the umbrella document. The
**15.0** sweep corrected the stale `README.md` `## Current status` (it previously
read `alpha.16` / "Phase 13 in progress") and the stale `docs/project/status.md`
release line. No measurement; no receipt.

## Court repair (15.1)

**Question.** The frozen `real100-v1` frontier court was taken with an
**unoptimized debug binary**, a **per-query-process** model, and a storage
accounting that folded the transient `.voldoc` into the persistent footprint.
Re-run the *unchanged* court on the **release** binary with the storage universes
split.

**Receipt.** [`2026-10-07-real100-release-baseline-866f489`](../../evidence/campaigns/2026-10-07-real100-release-baseline-866f489/)
(`doc-baseline`, 6 GiB cap, 8 cpus, `cargo build --release --locked --all-features`).

**Result.**

| axis | VOLE (v) | SQLite/FTS (a1) | direct tooling (a0) |
| --- | --- | --- | --- |
| answered / declined | 455 / 245 | 506 / 194 | 508 / 192 |
| `materialize --exact` | 97/100 | 98/100 | 100/100 |

Storage universes, reported **separately** (never folded): VOLE **persistent**
field store `2,667,668,262 B`; VOLE optional standalone **descriptor**
`1,704,524,849 B`; **A1** SQLite db `2,891,784,192 B`.

**Held regions (wins).** Exactly two structural cells:
`pdf`/`text_repeat` (VOLE win) and `docx`/`table` (VOLE win). **Everything else
loses to SQLite/FTS**, and `exact` reconstruction loses to the source file.

**Negatives and limits.**

- The aggregate `all`/`text_repeat` cell sits at the **process floor** (v median
  **7 ms** vs a1 **6 ms**) and **flips between runs** purely from ~1 ms of median
  movement; it is **not a meaningful stratum**. The by-format / by-size strata
  are the meaningful units.
- **Large PDFs still fail at `encode`.** Of the 5 documents `>100 MiB`, **3 fail**
  (`nasa-pdf-0001` OOM-killed rc 137; `nasa-pdf-0002`/`nasa-pdf-0003` timed out
  rc 124) and **2 succeed** (`nasa-pdf-0020`, `nasa-pdf-0024`). This is the known
  Phase-14 bound (ADR-0041); 15.1 only re-measures it on the release binary.
- **No perf-class counters are claimed.** `perf` is absent from the lane
  (`strace` and `/usr/bin/time` are present but were not wired into the frozen
  court), so the plan's "where the lane provides them" yields nothing.
- Two earlier release-tagged runs were **invalidated and deleted**: one ran
  concurrently with host `cargo` builds (large timing outliers and unstable
  aggregate verdicts), one was an aborted empty launch.

**Interpretation.** The repair is a measurement correction, not an architecture
change. The release profile does not overturn the debug verdict's *shape*: VOLE
holds two structural regions and loses the rest. It does establish that the
durable work was already deterministic across profiles.

## Resident runtime (15.2)

**Question.** Does a resident `DocumentFieldSession` (open field + store, mapped
indexes, parsed manifest, typed-model / decoded-member caches, reused buffers)
plus an `observe-batch` API beat a cold per-observation process on the
`text_repeat` and mixed workloads?

**Receipt.** [`2026-10-07-real100-release-resident-78f7ea8`](../../evidence/campaigns/2026-10-07-real100-release-resident-78f7ea8/).

**Result — NEGATIVE / partial.**

| stratum (`text_repeat`) | faster lane |
| --- | --- |
| `all` (aggregate) | **cold** — 7 ms vs resident 9 ms |
| `<100KiB` | resident |
| `100KiB-1MiB` | resident |
| `1-10MiB` | cold |
| `10-50MiB` | cold |
| `50-100MiB` | cold |

The resident lane (`v_r`, many observations in **one** `observe-batch` process)
wins only below ~1 MiB. The resident lane also answers a heterogeneous
`session_mixed` batch that the other lanes decline — an informational witness, not
a verdict.

**Mechanism.** The cold `observe` path runs `narrow_probe` — a per-call manifest
+ derived-cache **short-circuit** that returns the derived node without building
the full evaluation context — whereas `observe-batch` always evaluates the full
path. Above ~1 MiB the per-observation cost of that full path exceeds the
once-per-batch process-spawn saving.

**Identified fix (recorded, not shipped).** Hoist `narrow_probe`'s manifest/index
opens into the session so residency keeps the short-circuit.

**Interpretation / limitation.** Repeated text observation costs ~1–2 ms per
observation regardless of lane on this corpus; the dominant lifetime costs are
**ingest** (encode + `field-ingest`) and **storage**, not repeated observation.
Residency is therefore **not** the lever Phase 15 is looking for. One frozen
corpus; no population claim.

## Packed seed store (15.3)

**Question.** Does an optional, immutable, segmented, mmap-able `fieldpack`
backend (`NodeId -> (segment, offset, len)`, identity unchanged) beat the
one-file-per-node reference on persistent bytes, file count, and cold-observation
latency?

**Receipt.** [`2026-10-07-phase15-packed-8c195e8`](../../evidence/campaigns/2026-10-07-phase15-packed-8c195e8/).

**Court.** A focused **12-document** subset of `real100-v1` — one document per
`(format, size_class)` stratum, **not** the frozen population. The **same**
descriptor is ingested into two store roots: filesystem vs `--packed`.

**Result — ADOPTED (seed namespace only).**

| metric | fs | packed | ratio |
| --- | ---: | ---: | ---: |
| persistent bytes (`du -sb`) | 352,671,955 | 253,737,799 | **0.719×** |
| file count | 25,574 | 237 | **0.009×** (111× fewer) |
| cold-observation wall | 1,555 ms | 1,541 ms | **0.991×** (parity) |

**Correctness.** Field id identical across the two backends **12/12**;
byte-exact `materialize --exact` **12/12 both**.

**Interpretation / limitation.** Only the **SEED** namespace is packed;
descriptor / manifest / index / cache stay files, so the win is bounded to the
seed store. The syscall summary (`strace -c -f`, one representative document) is
recorded, but the lane has `strace` and **not** `perf`, so no page-fault /
physical-read counters are claimed. One 12-document subset; no population claim.

## Bounded parallel ingest (15.4)

**Question.** Does a bounded worker pool over the pure, independent parts of
ingest (ZIP members, PDF streams, hashing, resource analysis, ready DAG nodes)
speed up `field-ingest`, and is the result deterministic?

**Receipt.** [`2026-10-07-phase15-workers-122c026`](../../evidence/campaigns/2026-10-07-phase15-workers-122c026/).

**Court.** A **10-document** subset of `real100-v1`, `field-ingest --workers {1,
2, 4, 8, 16}` after one `encode` per document. The `doc-baseline` lane is capped
at `cpus: 8`, so `--workers 16` **oversubscribes** it.

**Result — neutral on speed, exactly deterministic (non-default feature).**

| workers | median speedup vs serial | note |
| ---: | ---: | --- |
| 2 | 1.00× | |
| 4 | 1.00× | largest PDF ~1.11× at w4 |
| 8 | 1.00× | saturates the lane |
| 16 | 0.99× | oversubscribed (> 8 cpus) |

Determinism is **POSITIVE**: field id identical across **every** worker count
**10/10**, and `materialize --exact` == source for **every** count **10/10**.

**Limits.** One unexplained outlier is recorded, not hidden: `nist-pdf-0004`
(6 s / 14 s / 29 s / 14 s / 1 s across w1/w2/w4/w8/w16). One 10-document subset;
no population claim. `parallel` is a **non-default** feature; the pool is used
only when `--workers > 1`.

**Interpretation.** Parallelism changes no wire bytes, no persisted artifact, and
no decoder behavior. Its value here is the determinism witness (the exactness path
is provably unaffected by worker count), not a wall-time win on this corpus.

## DEFLATE backend ablation (15.5)

**Question.** Is the shipped `miniz_oxide` inflate the fastest *correct* safe-Rust
backend, and is a safe-API SIMD scan of the PDF stream-end hot loop
output-preserving?

**Receipt.** [`2026-10-07-phase15-deflate-e676166`](../../evidence/campaigns/2026-10-07-phase15-deflate-e676166/).

**Court.** Real `real100-v1` compressed members: **52,498 members, 88 documents,
481 MiB compressed / 2,768 MiB decoded**. Every candidate must equal the
`miniz_oxide` reference byte-for-byte. Adoption bar, pre-registered: **≥ 1.25×
GB/s AND ≤ 1.10× peak RSS**.

**Result — one adoption, one recommendation, one disqualification.**

| candidate | GB/s | vs miniz | mismatches | RSS vs miniz | bar |
| --- | ---: | ---: | ---: | ---: | --- |
| miniz (scalar) | 1.231 | reference | 0 | reference | — |
| miniz-simd (adler SIMD) | 1.366 | 1.11× | 0 | 1.00× | below bar |
| **zlib-rs** | **1.938** | **1.58×** | **0** | **1.00×** | **MEETS bar** |
| zune-inflate | 1.922 | 1.56× | **255** | 0.93× | **DISQUALIFIED (incorrect)** |

- **`zune-inflate` is disqualified for incorrectness** — it decoded 255 members
  differently from the reference; speed is irrelevant when the bytes differ.
- **`miniz-simd` is enabled** (the optional `simd` adler path): a free,
  output-preserving 1.11×, below the bar but with no cost.
- **`zlib-rs` meets the pre-registered bar** and is the **recommended backend
  swap** — byte-identical, RSS-neutral, 1.58×. It is **not yet adopted**; the
  court measures only.

Also in this subphase: the scalar PDF `find_endstream` substring scan was replaced
with a reused `memchr::memmem::Finder` (differential tests assert equality with the
retained scalar oracle). No `perf` in the lane; the harness reports its own peak
RSS and `/usr/bin/time -v` agrees.

**Interpretation / limitation.** 88 of 100 documents contributed members; 12,001
members were declined (unsupported filters) and are counted, not hidden. One
frozen corpus; no population claim. The court does not change the shipped inflater
— adoption is a separate, deliberate act.

## Adaptive procedural promotion (15.6)

**Question.** If the field persists compact **structural tapes** (not just final
answers) and a cost governor promotes a reusable intermediate only when expected
future saved work exceeds materialization + storage rent, does it beat a
SQLite-Minimal/Full/Adaptive baseline on an unforeseen-query-diversity frontier?

**Receipts.** [`2026-10-07-phase15-diversity-4786f8e`](../../evidence/campaigns/2026-10-07-phase15-diversity-4786f8e/)
and [`2026-10-07-phase15-revision-4786f8e`](../../evidence/campaigns/2026-10-07-phase15-revision-4786f8e/).

**Result — NEGATIVE: all three pre-registered falsifiers fire.**

| falsifier | threshold | measured | verdict |
| --- | --- | --- | --- |
| F1 diversity | `v_on` > `sq_adapt` by >10% at some depth | never crosses; `sq_adapt` stays within 10% of the best lane at **every** depth | **REFUTES** |
| F2 mechanism | promoted bytes cut durable bytes ≥20% at equal-or-better latency | **0.0%** cut, at equal-or-worse latency | **REFUTES** |
| F3 revision | best lane's retained cross-revision work ≥20% | **+0.3%** (< 20%) for every lane | **REFUTES** |

`sq_full` is fastest at **every** depth. The mechanism ships **opt-in**
(`--promote`), **default-off**, and is never on the exactness path.

**Interpretation / limitation.** The governor's durable store adds no byte
distinction over the existing representation, so there is nothing to promote.
Three documents in the diversity court and four revision families; no population
claim. The negative is recorded, not retired.

## Durable cross-root derivations (15.7)

**Question.** With a canonical derivation identity `(algorithm-version,
dependency NodeIds, parameters)`, do identical inputs share *computed* state (not
merely representation identity), so that the Phase-12 `N3` reuse condition
(ADR-0034/0035) is satisfied on the real publication/revision families?

**Receipt.** [`2026-10-07-phase15-crossroot-7429d61`](../../evidence/campaigns/2026-10-07-phase15-crossroot-7429d61/).

**Result — NEGATIVE (`N3` VIOLATED, as the design predicted).**

- **23** real families, **56/56** members ingested, one shared `FieldStore` per
  family.
- **Cross-member derived reuse = 0 nodes** (warm in-order reuse 30 = empty-cache
  floor 30).
- Post-`cache --clear` fresh-process reuse **above** the intra-observation floor
  = **0**. The counter is live (cache-intact fresh process reused 85).
- Representation identity **is** shared: **44** nodes id-shared, **4** shared
  resources (`79,720 B`).
- For contrast, chunk-level borg CDC saved **32,158,196** source bytes — versus
  **0** derived-output bytes saved.

**Interpretation / limitation.** There is no cross-member derived reuse even warm;
the warm `seed_nodes_reused` is entirely each member's own intra-observation
(diamond) reuse, which an empty cache already shows. Representation identity is
shared but no *computed* state is, and nothing survives `cache --clear`. **No Rust
change was made** — the court is measurement-first and shows a durable
`DerivationStore` is not warranted. No compression claim: a shared blob is scored
as state/work.

## CUDA batch lane (15.8)

**Status — DEFERRED, not measured.** This is an explicit, reasoned deferral, not
a silent omission. Reasons (design record:
`research/subagents/phase-15/design-15.8-cuda.md`):

1. **The gate is unopened.** The plan makes 15.8 conditional on CPU profiling
   showing that decode/scan **bandwidth** dominates. That gate (G1/G2, derived
   from the 15.4/15.5 results) is not opened by those results.
2. **Docker on this host cannot see the GPU.** No NVIDIA container runtime is
   registered (`/etc/docker/daemon.json` lists no `runtimes`; `docker info` lists
   only `io.containerd.runc.v2` and `runc`), so `gpus:` / `runtime: nvidia`
   cannot be satisfied and a container cannot receive `/dev/nvidia*`.
3. **`nvCOMP` is proprietary** and is not on the `deny.toml` allow-list, while
   the policy gate runs `[graph] all-features = true`.
4. **The repo requires Docker-reproducible evidence** (`AGENTS.md` "Docker only";
   ADR-0003). An unreproducible lane is not admissible.

A GPU inflate lane, if ever pursued, would be a **derived (`Q_gen`) accelerator
only**, byte-compared against the `miniz_oxide` reference; the exact leaf stays the
raw compressed span. No library change, no wire change, no decoder authority.

## Cross-cutting notes

- Every exactness statement in Phase 15 is the same invariant:
  `materialize(descriptor) == original_bytes` (length + SHA-256 + `cmp`).
- No cap was raised and no validate-or-decline rule was weakened to make a run
  pass.
- No corpus or schedule was tuned against any measurement; the `real100-v1`
  manifest is frozen by SHA-256.
