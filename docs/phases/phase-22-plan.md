# Phase 22 — Economic programme: a strong competitor, shared execution, lifetime cost

> **IN PROGRESS.** Subphases **22.1 (competitor envelope) and 22.2 (compact
> query-native directory — profiling gate) are complete** — 22.1 was released in
> `v0.1.0-alpha.26`, 22.2 in `v0.1.0-alpha.27` (both sealed; see
> [phase-22-results.md](phase-22-results.md)). **22.2 shipped *nothing*
> structurally**: the index/selector layer is 10.6 % of the warm session and
> sub-resolution, so the phase is a **publishable negative**. Subphases
> **22.3–22.7 remain planned and not started.** Everything below that is not 22.1
> or 22.2 is a **target** or a **gate**, never a result. (The format programme,
> [Phase 21](phase-21-plan.md), remains deferred and unstarted; this programme
> runs on the already-shipped PDF/DOCX/EPUB/ODT surface, and its last format
> subphase (P7) is **Phase 21's** work evaluated through this thesis.)
>
> This plan synthesises an **external technical review** into the repo's plan
> style. Where the review's claim could not be rendered exactly, the deviation is
> noted in the final message to the orchestrator, not silently smoothed.

Branch: `phase22` (22.1; merged as `v0.1.0-alpha.26`; **22.2 measured on
`phase23`**, released `v0.1.0-alpha.27`). Base: `main` @ `v0.1.0-alpha.25` (Phase 20).
Predecessors (read this plan with them): [phase-20-results.md](phase-20-results.md),
[Phase 21 plan](phase-21-plan.md), [ADR-0050](../adr/0050-sqlite-as-substrate-question.md),
[ADR-0051](../adr/0051-direct-field-ingestion.md),
[ADR-0053](../adr/0053-batched-packed-sync.md),
[ADR-0054](../adr/0054-repeatability-and-paired-measurement.md),
[ADR-0056](../adr/0056-runtime-vs-research-reconstruction-programs.md).

## Governing thesis

> VOLE-Document should aim to provide **more exact, independently useful
> observations per byte stored, per byte read, and per unit of computation** than
> the best conventional document infrastructure an expert could build.
>
> The target is **not** "beat one SQLite configuration"; it is a better
> **capability/cost frontier** against the **strongest honest competitor**, in a
> **commercially relevant workload region**, with **bounded statistics** and **no
> loss of accuracy or capability**.

Two consequences follow and are binding on every subphase:

1. **The competitor is maximized first.** A court that beats a weak or
   deliberately crippled baseline proves nothing. 22.1 exists so that every later
   subphase is measured against the strongest configuration the competitor camp
   can honestly build.
2. **A narrow win in a commercially important region is worth more than a tiny
   noisy win everywhere.** The economic headline is a **frontier**, reported in
   the region where the workload actually lives — not a single averaged ratio.

## The honest prior

The record this programme competes against is **mixed and mostly negative**, and
the plan refuses to pretend otherwise (ADR-0023; `FINDINGS.md`):

- VOLE does **not** beat purpose-built baselines on the whole-file, random-access
  I/O, or cross-document sharing axes (ADR-0017/0019/0021/0028) — all recorded
  losses.
- VOLE **builds faster** under the equal contract (paired median **0.182**,
  95% CI 0.102–0.228; ADR-0054) and **stores ~0.53×** on the 12-document subset
  but **~0.96×** on the full population (Phase 19.3), **ties** on cold, and is a
  **modest ~1.29× slower** on the warm lane (ADR-0054).
- **Residency above ~1 MiB is a recorded negative** (ADR-0042), **adaptive
  procedural promotion is refuted** (ADR-0046), and **durable cross-root
  derivation reuse is refuted** (ADR-0047).

So a VOLE win under any Phase-22 gate should **not be presumed likely**. The
programme's value includes the case where it produces **publishable negative
results** (see below) with bounded intervals.

## The order (frozen)

The review's priority labels P0–P7 map onto subphases as follows. Ordering is
by dependency and risk, not by expected payoff.

| Subphase | Review | Question |
|---|---|---|
| **22.1** | **P0** | Is there a **strong competitor envelope**, including an adaptive and a hybrid SQLite, before any frontier claim is made? |
| **22.2** | **P1** | Can a **compact query-native directory / hot-cold layout** cut warm CPU/I/O/allocation without adding storage or weakening integrity? **Complete (negative)** — index is 10.6 % and sub-MDE; nothing shipped |
| **22.3** | **P2** | Can a heterogeneous batch be executed as **one dependency closure and one schedule**, sharing work already known to the batch? | **22.3.0 complete — negative (`SESSION-ALREADY-CAPTURES`, `2026-10-08-phase22-3-dup-ca9f29e`).** The duplication is real (3.66–4.11× node execution on DOCX/EPUB) but the shipping session already removes it; the only undeduped axis (index descent) is the 22.2 sub-MDE candidate. 22.3.1/22.3.2 not started |
| **22.4** | **P3** | On an **unknown** query schedule, which system wins the **cumulative lifetime frontier**? | **Complete — region split (`MIXED`, `2026-10-08-phase22-4-lifetime-b4225fe`).** Pre-registered hidden schedule (seed 220400), all adaptation charged: resolved VOLE **win** in the PDF region (0.73–0.83×), resolved **loss** in EPUB (1.50–1.82×), **unresolved** pooled/DOCX/size-classes; storage 0.468× win, RSS a loss |
| **22.5** | **P4** | Can bounded **remote** selective materialization reduce bytes transferred at competitive p95 latency? |
| **22.6** | **P5** | Is there a **more compact structural representation** at equal observation capability (no reopened dedup)? |
| **22.7** | **P6** | Does the whole **document-agent** workload cost ≥2× less per **correct, grounded** task? |
| — | **P7** | **XLSX/PPTX** economic validation — **Phase 21's** work, evaluated through this thesis (see below). |

---

## 22.1 Competitor envelope (P0) — complete

**Recorded (22.1, `2026-10-08-phase22-competitors-86d9312`, released
`v0.1.0-alpha.26`).** Six purpose-tuned SQLite configurations were added and the
Phase-18 baseline kept unmodified as the `hist` control; **no capability gap
remains**. Against the tuned envelope at C5: build **0.219×** `full` (0.194×
`hist`), bytes **0.762×** `full` (0.805× `adaptive`; the old 0.53× was against a
contract-dead FTS index), cold **0.812×**, warm a **loss** (**1.211×**, 95 % CI
1.006–1.483). See [phase-22-results.md](phase-22-results.md).

**Question.** What is the **strongest honest competitor** an expert could build
for each required capability, before any VOLE frontier claim is made?

**Method.** Build a **SQLite competitive envelope** — a family of configurations,
each correctly tuned for its purpose, with **appropriate transactions, prepared
statements, batching, cache budgets and durability settings**:

| Lane | Representation |
|---|---|
| **SQLite-Minimal** | source + minimal metadata / indexes |
| **SQLite-FTS** | correctly tuned FTS5 — external-content / contentless indexing, `detail=full\|column\|none`, optional token counts, prefix indexes, tokenizer choice |
| **SQLite-Structural** | native coordinates + selectively materialized structure |
| **SQLite-Full** | all required contract projections |
| **SQLite-Adaptive** | builds additional indexes as workloads reveal themselves |
| **SQLite-Hybrid** | SQLite + retained source + an optimized native parser/cache |

The existing **Python/Poppler baseline is retained as a HISTORICAL CONTROL**;
optimized competitors are **added**, never substituted for, and the old court is
**not erased** (ADR-0054: superseded estimates are kept, not deleted; ADR-0049:
the byte-accounting method is stated).

**Durability contracts are compared explicitly.** The current harness runs the
baseline at `journal_mode=WAL`, `synchronous=NORMAL`, which SQLite documents as
consistency-preserving but able to **lose the latest committed transactions** on
certain power failures. That is compared, honestly and side by side, against
VOLE's **batched packed-store boundaries** (ADR-0053; Phase 20.1), whose
prefix-recovery / fail-closed properties are adversarially tested but whose
**true power-loss and torn-rename behaviour is argued, not measured**. Neither
side gets a free durability pass.

**Gate.** At least **one optimized configuration per required contract**, with
**no omitted competitor capability**. A configuration that silently drops a
capability the contract demands is a missing competitor, not a win.

---

## 22.2 Compact query-native directory / hot-cold layout (P1) — **complete (negative)**

**Question.** `src/field/index.rs` persists hash-addressed index nodes as
**individual files**. Can the request critical path be made materially cheaper
without adding persistent bytes or weakening integrity checks?

**Recorded (22.2, `2026-10-08-phase22-2-d81689c`).** A decisive profiling gate ran
first and **no structural layout was shipped**. The warm session is **open 55.5 %
(`Descriptor::parse` 44.7 %) + loop 44.5 %**; inside the loop selector resolution
(**probe**) is **0.4 %** and index-node read+verify is **10.5 %** — and that
10.5 % is **redundancy** (493 opens for 20 files: one immutable root leaf
depth-0 re-read 24–117×), not lookup work. A *perfect* selector directory removes
at most **10.6 %**, an implied shift of **~0.13** against a court **MDE ≈0.399**:
**below resolution**. The before/after warm court vs the tuned `full` envelope is
a **NULL** (1.211 → 1.293, overlapping CIs) with no layout byte changed. Left as
a candidate for a future **higher-resolution** court: an **in-session
verified-node memo** (zero persistent bytes, still sub-MDE).

**Method (as originally planned).** Profile the full critical path —
selector resolution → index-node access → verification → decoding → typed-model
access → answer construction → serialization — for **allocations, syscalls,
bytes read, CPU, page faults, cache misses, wall**. Then test, separately:

- **(A) Compact offset-addressed selector directory.** Delta-coded offsets /
  bit-packed integers over the **dense integer selector spaces** (pages, objects,
  streams, blocks), atop the **existing safe `read_exact_at` substrate** — **no
  mmap needed**.
- **(B) Hot/cold separation.** Split hot structure (selector directory, typed
  node headers, dependency ids, coordinate tables, provenance refs) from cold
  payloads (exact byte authority, large images, compressed resources,
  unrequested members).
- **(C) Direct-access typed structural columns.** Borrow **Apache Arrow layout
  principles** for direct typed access **without embedding Arrow**.

**Gate.** A **meaningful reduction in warm CPU / I/O / allocations** without a
substantial increase in persistent storage and **without weakening integrity
checks**. The same **observation results and provenance** must be returned — not
merely a cached string.

**Falsifier.** If a faster path changes an answer, its provenance, or a
verification outcome, or if it needs a persistent-byte increase that is not
repaid, the mechanism is rejected and the loss is recorded.

---

## 22.3 Fused heterogeneous execution (P2)

> **22.3.0 complete — negative; 22.3.1/22.3.2 not started.** The measurement-only
> gate returned **`SESSION-ALREADY-CAPTURES`**: an independent DOCX/EPUB batch
> executes **3.66–4.11×** more seed nodes than the shipping resident session
> (memo + derived cache already remove the redundancy), and the only axis the
> session never dedupes is the index/selector descent — the **22.2 sub-MDE**
> candidate. So a new fused executor is **not** justified and nothing ships. See
> [phase-22-3-results.md](phase-22-3-results.md) and the frozen contract
> [phase-22-3-scope.md](phase-22-3-scope.md).

**Question.** A heterogeneous batch is currently evaluated as N independent
dependencies. Can it be evaluated as **ONE dependency closure and ONE schedule**?

**Method.** Collect the selector-index entries for the batch; union their
dependencies; deduplicate immutable nodes; schedule decodes **once**; distribute
the materialized values back into individual answers while preserving **each
answer's identity, provenance, integrity checks, budgets and decline
semantics**. Measure `W_fused` vs `Σ W_individual` in **physical work** (node
reads, decoded bytes, allocations, CPU, wall).

**Controls.** Today's **independent** VOLE observations; today's **resident
batch**; and an **optimized SQLite session** on one connection with prepared
statements and shared application-level parsing.

**Gate.** An ambitious **≥2× reduction in actual decoded/materialized work** on
an overlap-rich workload, with **wall reported separately** (wall is not the
claim; decoded work is).

**Distinction from the Phase-15 failure.** This is explicitly **not** adaptive
procedural promotion (ADR-0046, refuted). Fusing shares work **within a KNOWN
batch** and adds **no persistent promoted state** — the three falsifiers that
fired in 15.6 have no purchase here because there is nothing promoted and no
hypothesis about future saved work.

---

## 22.4 Unknown-query lifetime frontier (P3)

**Question.** On a schedule the encoder never saw, which system wins the
**cumulative lifetime frontier** per equivalent successful observation?

**Method — four stages, both systems seeing the same documents:**

- **Stage A — ingest.** Both systems see the documents and choose their
  representation under a **predeclared budget**.
- **Stage B — reveal.** A **hidden schedule** is revealed (text, exact spans,
  structure, resources, provenance, revisions, mixed) in **undisclosed
  proportions**.
- **Stage C — adaptive execution.** SQLite **may build indexes and caches**;
  VOLE **may evaluate further state**. **ALL costs, storage growth and
  invalidation are charged** — no free adaptation.
- **Stage D — compare.** The **cumulative lifetime frontier** per equivalent
  successful observation.

**Method note.** SQLite is allowed a **serious adaptive strategy** — not a
deliberately crippled static schema. Anything less is not a competitor (22.1).

**Recorded prior.** The Phase-15 promotion experiment **lost** (ADR-0046). A VOLE
win here should **NOT be presumed likely**; the pre-registered expectation is
**no advantage outside a narrowly characterized region**, if any.

**Gate.** A measured, interval-bounded frontier advantage in a **stated
workload region**, with all adaptation costs charged. Otherwise the loss is the
result.

---

## 22.5 Large and remote selective materialization (P4)

**Question.** Bounded local reads exist (Phase 20.2). Can they extend to
**remote** storage?

**Method.** An **offset-addressed field** fetches a **compact directory** and
only the **necessary immutable segments** via **byte-range reads** (S3 supports
range reads), with a **range planner** that: resolves the dependency closure;
maps nodes to segment offsets; **coalesces neighbouring ranges**; batches within
a **byte/latency budget**; fetches **only required segments**; verifies returned
identities; and evaluates. Minimize

```text
N_requests · C_request + B_transferred · C_byte + C_decode + C_latency
```

**Control.** Give the SQLite competitor a **comparably optimized remote cache and
source-range interface** (22.1).

**Gate.** A **major reduction in bytes transferred / read** at **competitive p95
latency**.

---

## 22.6 Compact structural representation (P5)

**Question.** Can an **existing typed index/structural node** be represented
**more compactly while preserving the same observation capability**?

**Method.** Find the **minimum sufficient representation** per format across
each required capability — exact reconstruction, structural navigation, text,
resource lookup, native coordinates, provenance, revision interpretation — and
**share what can be interned once**: structural identifiers, string dictionaries,
delta-coded coordinates, compact relationship edges, shared offset maps; consider
**compressed bitmaps with rank/select** for sparse membership.

**CRITICAL — do not reopen falsified results.** The falsified **cross-document
dedup** (ADR-0021/0028) and **adaptive-promotion** (ADR-0046) results are **not
to be reopened** without a genuinely **NEW mechanism** and a **NEW falsifiable
hypothesis**. The question here is narrower and different: *can an existing typed
index / structural node be represented more compactly at equal capability?* — not
*does sharing across documents or adaptive promotion pay?* (already answered
negative).

**Gate.** A **significant full-population persistent-byte improvement with no
observation loss**.

---

## 22.7 Agent end-to-end economic court (P6)

**Question.** Across the **whole document-agent workload**, what is the **cost
per correct, grounded task**?

**Method.** Measure cost per **CORRECT, GROUNDED** task over the full workload —
correctness, grounding, model-input tokens, tool work, wall, document-preparation
compute, storage, peak memory, total cost — on **frozen multi-format inspection
workflows**, with the **baseline allowed the same task-level optimizations**.

**Method rule.** **Keep answer correctness fixed before any token-efficiency
claim.** A cheaper wrong answer is not an economic win.

**Gate.** **≥2× lower cost per correct grounded task.**

---

## P7 — XLSX/PPTX economic validation (belongs to Phase 21)

P7 is the XLSX/PPTX economic validation, and it is **Phase 21's** work
([Phase 21 plan](phase-21-plan.md), 21.1). It is evaluated **through this
economic thesis** — the competitor envelope (22.1) and the lifetime cost model
(below) — rather than by **format count**. Completing more formats is not the
claim; a better capability/cost frontier on the same contract is.

---

## The world-class lifetime cost model (applies to every subphase)

Every Phase-22 court reports **both** of the following, normalised **per
successful contract-equivalent answer** so that lanes answering different numbers
of questions are not compared dishonestly:

**1. The physical cost vector** (all eight components, never folded into one
number without also reporting the vector):

```text
ingest time · query time · update time · persistent bytes · bytes read ·
bytes transferred · peak RSS · I/O request count
```

**2. A monetary cost** (compute + storage + I/O + network + model + operations).

**Reporting rules.** A large advantage in a **narrow but commercially important
region** is worth more than a **tiny noisy advantage everywhere** — so a frontier
is reported **region by region**, with its estimator and interval (ADR-0054), not
as one averaged headline. A monetary cost must state its unit prices and the
date they were taken; a physical vector must state the byte-accounting method
(ADR-0049).

## Failed gates are publishable

**A failed gate is a result, not a failure.** Every Phase-22 subphase that does
not clear its gate is recorded as a **publishable negative**: the receipt is
sealed, the interval is reported, the falsifier is named, and the mechanism is
recorded **do not re-attempt without new evidence** (the repo's standing practice,
ADR-0023). A programme that honestly reports "the strong competitor still wins in
this region" has succeeded in its actual purpose.

## What is NOT to be pursued

Explicitly out of scope for Phase 22, each for a measured reason already in the
record:

- **Another generic Rayon pass.** Bounded parallel ingest is speed-neutral
  (ADR-0044); a generic parallelism sweep re-measures a known null.
- **Speculative CUDA.** The bandwidth gate is unopened and the pinned Docker
  lanes cannot see the GPU (ADR-0048, deferred not measured).
- **More compression candidates.** The whole-file axis is a recorded loss against
  generic compressors (ADR-0017); adding candidates does not change it.
- **Reviving the refuted cross-root-derivation / adaptive-promotion results.**
  Both are recorded negatives (ADR-0046/0047); 22.6 states the only admissible
  reopening condition — a new mechanism and a new falsifiable hypothesis.

## IP / presentation distinction

The runtime DocumentField programme and the research/codec programme are
**two different programmes with different claims**, and Phase 22 is where the
distinction must be kept exact. See
[ADR-0056](../adr/0056-runtime-vs-research-reconstruction-programs.md): the
runtime's economic mechanism (persistent dependency structure, typed exactness
and provenance contracts, selective materialization, format-native access, and
the cost of providing many observation types) **must stand on its own merits**;
exact reconstruction of an intact RAW source is the **entry condition**, not the
result, and the two must not be **conflated in the IP story or in diligence**.

## Acceptance (per subphase, unchanged)

Exactness is the invariant: `materialize(descriptor) == original_bytes` (length +
SHA-256 + `cmp`). **Docker only** — never `cargo`/tools/corpus work on the host;
every command runs in a digest-pinned, memory-capped compose service. One
production crate. One branch per phase, commit and push after every subphase;
research subagents are read-only. **No cap is raised to make a run pass**, and no
validate-or-decline rule is weakened. Negatives and partials are recorded, never
buried. Every performance claim states its **estimator and interval**
(ADR-0054). No claim without a sealed receipt.
