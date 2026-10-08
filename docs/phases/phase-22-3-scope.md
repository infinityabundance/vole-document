# Phase 22.3 — Fused heterogeneous execution: scope and frozen contract

> **22.3.0 COMPLETE — NEGATIVE; 22.3.1/22.3.2 NOT STARTED.** The measurement-only
> gate ran and returned **`SESSION-ALREADY-CAPTURES`**: the cross-request
> redundancy is already removed by the shipping resident session, so a new fused
> executor is **not** justified. See
> [phase-22-3-results.md](phase-22-3-results.md). Everything below is kept as the
> frozen contract; the `A`-stage design (22.3.1/22.3.2) is **not** being built.
>
> Originally: **DESIGN ONLY.** This document is a **frozen contract**, not a
> result. No code for 22.3 existed when it was written; (a) the decision to build
> was to be made on a **decisive measurement first** (22.3.0), and (b) a
> *different* subagent can audit an implementation against a contract it did not
> write. Every number quoted is a **prior** or a **target**, never a claim.
>
> Parent: [phase-22-plan.md](phase-22-plan.md) §22.3 (P2). Predecessor method:
> [phase-22-results.md](phase-22-results.md) §22.2 (the profiling gate that
> shipped nothing). Branch: `phase24`. Base: `main` @ `v0.1.0-alpha.27` (Phase 23).

## Question

A heterogeneous observation batch is currently evaluated as **N independent
dependencies**. Can it be evaluated as **one dependency closure and one
schedule** — decoding a shared node **once** and distributing the result — while
preserving each answer's identity, provenance, integrity checks, budgets, and
decline semantics?

Formally, for a known batch of requests `B = {r_1 … r_n}` decided at submission
time (not predicted), compare **physical work**:

```text
W_fused            ≤  ½ · Σ_i W_individual(r_i)      (the gate)
```

where `W` is counted in **decoded/materialized physical work** (node decodes,
decoded bytes, materializations), not wall time.

## Why this is not the refuted Phase-15 experiment (the distinctness test)

ADR-0046 (adaptive procedural promotion) is a **recorded negative** and is
**not** to be reopened. 22.3 is a different question with different falsifiers:

| | Phase 15.6 adaptive promotion (refuted) | Phase 22.3 fusion (this) |
|---|---|---|
| When decided | **Predicting future** requests from a past trace | **A known batch** at submission time |
| Persistent state | Wrote durable promoted nodes | **None** — in-memory, per call |
| Failure mode | Promoted bytes cut durable bytes 0.0% (bar 20%) | No wall/CPU/work win on an overlap-rich batch |
| Authority | On the exactness path | Never on the exactness path |

If any of the three 15.6 falsifiers could fire here, the design is wrong. The
frozen design states: **no persistent state, no future-request hypothesis, no
exactness-path involvement.** A fused execution that writes nothing and predicts
nothing cannot inherit the 15.6 failure — but it can still fail its own gate, and
that outcome is publishable.

## What already shares work today (the honest prior)

This is the single most important input to the decision, and it argues **against**
presuming a win. `DocumentFieldSession` ([`src/field/session.rs`](../../src/field/session.rs))
already shares, **across** the observations of one session:

1. **The open field** — the one-time `Descriptor::parse` (~**44.7 %** of the warm
   session, Phase 22.2) is paid once, not per observation.
2. **The open index store** (`FsIndexStore`) — opened once; the probe reuses it.
3. **The typed-model memo** (`ModelMemo`, content-keyed on decoded typed models) —
   the *same* memo is passed into every `observe_session` call, so a second
   observation that needs the same `DocxModel`/`EpubModel`/… does not re-parse it.
4. **The on-disk derived cache** (`DerivedCache` at `<root>/cache`) — consulted
   and filled by default when `use_cache` is true; a node output computed by one
   observation is served to later observations (and later processes) by content id.

And the batch itself is a **plain loop** — this is the gap:

```rust
pub fn observe_batch(&mut self, reqs: &[ObserveRequest], limits: Limits) -> … {
    reqs.iter().map(|r| self.observe(r, limits)).collect()
}
```

So the honest prior is: **the memo + derived cache may already remove most of the
cross-observation duplication**, leaving fusion a small residual — exactly the
"existing memoization captures most reuse" risk the plan names. The measurement
in 22.3.0 exists to find out *before* anything is built.

## The resolution problem, and why 22.3 cannot reuse the 22.2 court

The 22.2 warm court is an ADR-0054 paired **wall** court: cluster bootstrap over
**12 documents**, tie band ±10 %, and a minimum detectable effect at N=100 of
≈**0.399** (median-CI half-width ±0.279). Resolution there is capped by the
**between-document** variance and the **12 clusters** — raising N within a
document does not help. Any candidate smaller than ~0.40 of the ratio is
**unresolvable**, which is why the 22.2 directory (~0.13) was rejected.

22.3 therefore **does not** gate on a wall ratio. It splits the claim:

- **Architectural metric (the gate): deterministic physical-work counters.**
  `ObserveStats` already exposes `index_nodes_read`, `index_bytes_read`,
  `seed_nodes_fetched`, `seed_nodes_materialized`, `seed_nodes_executed`,
  `seed_nodes_reused`, `member_decodes`, `xml_parses` (ADR-0027). These are
  **exact integers per run**; determinism is *verified* by repeated runs (they
  must be identical), so there is **no noise floor** and small effects are
  resolvable exactly. 22.3 adds one measurement-only counter, `decoded_bytes`
  (bytes of node output the DAG materializer produced), on the same evidence
  surface — a counter addition changes no answer.
- **Economic metric (reported separately, ADR-0054): wall and CPU**, with the
  paired, interleaved, fixed-seed bootstrap estimator and its stated interval and
  MDE. Wall is **not** the gate (Phase 22.2's lesson), but a fusion that cuts
  decoded work yet does not move wall/CPU is reported as such, honestly.

This is what "a higher-resolution court first" means in practice: the gate lives
on a deterministic axis.

## The court — three stages, and the first one can end the phase

### 22.3.0 — Attribution and duplicate-work upper bound (measurement only; the gate)

**No mechanism is built in 22.3.0.** Before any fusion, measure the **upper bound**
of what fusion could possibly remove:

1. **Split the `dispatch` bucket.** Phase 22.2 measured `dispatch` at **32.0 %**
   as *one* bucket ("typed-model access vs answer construction are not separately
   timed"). Extend the env-gated profiler ([`src/field/prof.rs`](../../src/field/prof.rs),
   off by default) to separate: index descent, seed fetch, DAG materialize,
   typed-model decode (memo hit vs miss), adapter parse, and answer serialize.
2. **Count the duplicates directly.** For each pre-registered workload, record per
   request the set of `NodeId`s materialized/decoded and the typed models parsed.
   Compute, across the batch:
   ```text
   D  = Σ_i (work_i)  −  work(union of closures)      (the removable duplication)
   ```
   using the deterministic counters, with the memo+cache **disabled** (cold) and
   **enabled** (resident) separately.
3. **Decide.** If `D_max` (the cold-cache upper bound) is below the effect floor
   the plan demands (≥2× on the overlap-rich set), **the phase ends negative** and
   no mechanism ships — recorded exactly as 22.2 was. If it clears, 22.3.1 proceeds.

**Deliverable.** A sealed receipt `evidence/campaigns/<date>-phase22-3-dup-<sha>/`
with the attribution table, the per-workload duplicate-work bound, and a verdict.
**This documents the honest prior and is publishable either way.**

### 22.3.1 — The fused scheduler (only if 22.3.0 clears)

One dependency closure and one executed schedule per batch:

1. Resolve every request's selector entries from the index (once per distinct
   selector).
2. Take the **union** of the dependency closures; **deduplicate** by `NodeId`.
3. Topologically schedule the union; **decode each shared node once**; fill the
   in-session memo/cache from that single decode.
4. Distribute the materialized values back into the **individual** answers,
   preserving each answer's: `FieldAnswer` identity, `basis`/`scope`/`provenance`,
   `dependency_ids`, integrity verification, byte/op budgets, and **typed
   declines** (an unsupported pair still declines — fusion must never invent an
   answer for one request from a sibling's result).
5. Emit the **same `ObserveStats` semantics per answer** (the per-answer counters
   stay honest — they report the shared work attributable to that answer).

**Invariant.** For every request, the fused answer must be **byte-identical** to
the independent answer (same `FieldAnswer` value, same provenance), and the
decline set must be identical. This is checkable directly.

### 22.3.2 — Controls and the gate

Every Stage-0/1 number is reported against **three controls**:

| Control | Why it is required |
|---|---|
| **Independent cold observations** | The naive floor: one process/closure per request. |
| **Today's resident `observe-batch` with its memo + cache** | **The real comparator** — fusion must beat *this*, not the naive floor, or it is re-crediting work the session already removed. |
| **Optimized SQLite session** | One connection, prepared statements, shared application-level parsing — the equal-contract conventional lane (per the plan's "maximize the competitor first"). |

## Pre-registered workloads (overlap-rich, frozen before measurement)

Fusion only can win where requests **share** a closure. The overlap-rich set is
pre-registered; a **null-overlap control** (requests with disjoint closures) is
included and must show **~1.0×** (fusion must not make disjoint work slower).

Illustrative candidates (to be frozen as concrete `ObserveRequest` lists):

- **PDF page-local overlap:** `Page(17) × {Text, Structure, Operators}` +
  `Stream(s) used by page 17` + `Page(17) Preview` — shares the page content
  stream decode and the page's typed model.
- **DOCX story-local overlap:** several `DocxParagraph`/`DocxStory` observations
  over one `DocxModel` — shares the model parse and the story closure.
- **Resource fan-out:** `Member(n) × DecodedBytes` for several resources the
  package manifest references — shares the package/OPC model and member spans.
- **Cross-depth schedule (the contract shape):** the C0–C5 schedule the courts
  already use, as one batch — the realistic agent case.
- **Null-overlap control:** a batch whose closures are disjoint, e.g. unrelated
  pages/objects — the falsifier that fusion adds no tax.

## Metrics (per answer and per batch)

**Deterministic (gate axis):** index node reads; index bytes; seed nodes fetched;
seed nodes materialized/executed/reused; `member_decodes`; `xml_parses`;
`decoded_bytes` (new); **allocations** (the existing `LD_PRELOAD` counter,
`tools/fixtures/phase22-2-malloc-count.c`); **CPU** (user+sys via `/usr/bin/time -v`).

**Economic (separate axis, ADR-0054):** wall per batch, paired and interleaved,
cluster bootstrap by document, fixed seed, ±10 % tie band, median **and**
geometric mean, every sample retained; report W/T/L and the MDE.

**Correctness (must not move):** per-request `FieldAnswer` byte-identity vs the
independent lane; provenance identity; identical decline set; `materialize --exact`
still 12/12 (fusion is observe-only, never on the archival path).

## Gate, and what a failure means

- **Pass:** `W_fused ≤ ½ · Σ W_individual` in deterministic decoded/materialized
  work on the overlap-rich set, with **zero** answer/provenance/decline
  divergence, and **no** regression (≈1.0×, within the null-overlap band) on the
  disjoint control. Wall/CPU reported separately and must not regress materially.
- **Fail (publishable):** if the removable duplication `D_max` (22.3.0) is below
  the floor, or the fused work-saving is < 2×, or any answer/provenance/decline
  changes — record the negative with its receipt and **do not ship**. As in 22.2,
  a phase that honestly reports "the resident session already captured this reuse"
  has succeeded in its actual purpose.

**Falsifiers (any one rejects the mechanism):**
1. A fused answer differs by one byte from the independent answer.
2. A request that declines independently is answered by fusion (or vice versa).
3. Provenance or `dependency_ids` differ.
4. `W_fused ≥ ½ Σ W_individual` — the architectural hypothesis is false.
5. The disjoint-overlap control regresses materially (fusion adds overhead where
   there is nothing to share).
6. Persistence of any kind is introduced (this becomes 15.6 by another name).

## Subphase summary

| Subphase | Kind | Question | Can end the phase? |
|---|---|---|---|
| **22.3.0** | measurement | What is the removable duplicate-work upper bound across a known batch, and where is the 32 % `dispatch` bucket actually spent? | **Complete — negative (`SESSION-ALREADY-CAPTURES`).** The duplication is real (3.66–4.11× node execution on DOCX/EPUB) but already removed by the shipping session; the index axis (the only undeduped one) is the 22.2 sub-MDE candidate. See [phase-22-3-results.md](phase-22-3-results.md) |
| **22.3.1** | implementation | Build the one-closure/one-schedule executor with per-answer identity preserved. | **Not started — not justified by 22.3.0** |
| **22.3.2** | court | Does `W_fused ≤ ½ Σ W_individual` hold vs the **resident-with-cache** control, with zero divergence? | Not started |

Branch: `phase24`; commit and push after each subphase; a sealed receipt per court
under `evidence/campaigns/`.

## Frozen contract (the checklist an independent subagent audits against)

1. **No persistence.** Fusion adds no durable node, no `promoted/`, no manifest
   change, no wire byte, no new feature bit.
2. **No decode-authority change.** Nothing in `src/container/`; the index stays
   advisory (ADR-0024); `materialize --exact` is untouched.
3. **Per-answer identity.** Fused answers are byte-identical to independent ones
   (value, provenance, `dependency_ids`), and declines are identical.
4. **Determinism.** Physical-work counters are identical across repeated runs of
   the same batch on the same store; field ids unchanged.
5. **Gate on work, wall separate.** The ≥2× gate is on deterministic
   decoded/materialized work; wall/CPU carry their own ADR-0054 interval and MDE.
6. **Controls included.** The resident-with-memo-and-cache lane is a control, not
   an afterthought; the disjoint control must read ~1.0×.
7. **Honest negative.** A sub-floor result ships nothing and is recorded.

## Acceptance (unchanged, repo-wide)

Exactness is the invariant (`materialize == source`; length + SHA-256 + `cmp`).
Docker only; digest-pinned and memory-capped; no cap raised to make a run pass.
One branch per phase, commit-and-push per subphase; a sealed receipt per court;
negatives and partials recorded; every performance claim states its estimator and
interval (ADR-0054); no claim without evidence.

## Open risks (stated up front)

- **The prior is against a win.** The session already shares the descriptor parse,
  the index store, the typed-model memo, and the derived cache; fusion may be
  sub-floor. That is precisely why 22.3.0 runs first.
- **The gate axis may not imply the economic axis.** A ≥2× decoded-work cut can
  coexist with a flat wall if the removed work was already cheap; both are
  reported, and the wall result is not spun.
- **Overlap-rich workloads are, by construction, favorable.** The disjoint control
  and the realistic C0–C5 batch keep the claim honest; a win is claimed only in
  the region where the workload actually lives.
