# Phase 22.3.0 results — duplicate work across a heterogeneous batch (measurement only)

Branch `phase24`. Measured at commit `ca9f29e` (dirty tree: the Phase-22.3.0 change
set, recorded in the receipt). Base `main` @ `v0.1.0-alpha.27` (Phase 23).

**Verdict: `SESSION-ALREADY-CAPTURES` — 22.3 closes negative for a new mechanism;
nothing is shipped.** The cross-request redundancy a fused executor was meant to
remove is **already removed by the shipping resident session** (typed-model memo +
derived cache); the only structurally unexploited axis is the index/selector
descent, which Phase 22.2 already measured as **sub-MDE**. This is the
publishable-negative outcome the [scope](phase-22-3-scope.md) pre-registered and
the plan's "existing memoization captures most reuse" risk made likely.

## Question

[Phase 22.3](phase-22-3-scope.md) asks whether a heterogeneous batch should be
executed as **one dependency closure and one schedule** (`W_fused ≤ ½ Σ
W_individual`). `DocumentFieldSession::observe_batch` is currently a plain loop
(`reqs.iter().map(|r| self.observe(r, ...))`). **22.3.0 is the measurement-only
gate**: before building anything, measure the removable duplication and — crucially
— how much of it the *shipping* session already removes.

## Method

For each of 12 `real100-v1` documents (4 pdf / 4 docx / 4 epub), build the packed
store **once** (`field-build --profile runtime --packed`), then run, each from a
**cold** derived cache:

| lane | what it is |
|---|---|
| **resident** | today's default session (typed-model memo **+** derived cache) |
| **indep** | the same batch with `--no-cache` on **every request line** (memo **and** cache off) → Σ W_individual |
| **indep2** | a repeat of `indep` (determinism check) |
| **null** | a two-request, largely-disjoint control (indep mode) |

The batch is the **frozen Phase-22.2/22.1 contract schedule** (`bytes text
metadata revision` for pdf; `bytes text doc-text heading table resource metadata
revision` for docx/epub). All metrics are **deterministic node/byte counters**
parsed from the `observe-batch` JSONL — no statistics, no estimators.

**Receipt.**
[`2026-10-08-phase22-3-dup-ca9f29e`](../../evidence/campaigns/2026-10-08-phase22-3-dup-ca9f29e/)
(`SUMMARY.md`, `MATRIX.md`, `raw/*.jsonl` — every lane retained). Court
`tools/phase22-3-dup-court.sh`, aggregator `tools/fixtures/phase22-3-dup.py`.
Service `doc-baseline` (`mem_limit == memswap_limit == 6g`, `pids_limit 4096`,
`cpus 8`). Determinism **PASS** (indep ≡ indep2).

## The metric that was rejected

A first aggregation ranked documents by `indep_exec / |U|`, where `|U|` is the
distinct union of every answer's `dependency_ids`. **That metric is unsound and
the `CLEARS` it produced is withdrawn:**

- `dependency_ids` is a *provenance* list, **not the executed closure** — it omits
  nodes pulled in as dependencies (e.g. an epub `block:0` answer names 2 ids but
  executes 7 nodes).
- For DOCX/EPUB many observations report the **same shared model nodes**, so the
  null-overlap control collapses to `struct_dup_proxy = 2.0` (`--block 0` and
  `--block 1` share the model) instead of ≈1.0. The proxy cannot separate
  `--block 0` from `--block 1`.

The corrected aggregator uses the deterministic work counters and keeps
`dependency_ids` only as a clearly-labelled **secondary** proxy.

## Result — deterministic counters

| fmt | docs | indep `exec` | resident `exec` | **exec_dedup** | idx_dedup | seed_bytes_dedup | cache_write_dedup |
|---|---:|---:|---:|---:|---:|---:|---:|
| docx | 4 | 148 | 36 | **4.11×** | 1.00× | 4.50× | 5.12× |
| epub | 4 | 647 | 177 | **3.66×** | 1.00× | 1.81× | 21.05× |
| pdf | 4 | 26 | 26 | **1.00×** | 1.00× | 0.99× | 1.00× |
| **pooled** | 12 | **821** | **239** | **3.44×** | **1.00×** | — | — |

Per-document: DOCX `exec` falls 33→9 (×3.67) to 49→9 (×5.44); EPUB 103–198→24–63.

## Reading

- **The duplication is large and is already captured by the shipping session.** An
  independent DOCX/EPUB batch executes **3.7–4.1×** more seed nodes than the
  default resident session. `seed_nodes_executed` collapses to the per-document
  minimum after the first observation (the typed-model memo for DOCX;
  memo **+** cache reuse for EPUB — resident `seed_nodes_reused` is 0 for DOCX and
  62/48/34/59 for EPUB). The `≥2×` target a fused executor was meant to hit is
  therefore **already met by shipped code** exactly where duplication exists.
- **PDF has no duplicate work to fuse.** Its frozen schedule
  (page-text / byte-range / metadata / revision-lineage) is mutually disjoint:
  `indep_exec == resident_exec` and the closure proxy is ≈1.0.
- **The one axis the session never dedupes is the index/selector descent.**
  `index_nodes_read` is **identical** with and without the memo/cache (`idx_dedup`
  = 1.00 for all 12 documents; 461 pooled node reads). A fused executor resolving
  all selectors in one pass could remove that — and that is precisely the **22.2**
  candidate, where index read+verify was **10.5 %** of the warm session and a
  *perfect* selector directory was measured **sub-MDE** (implied shift ≈0.13 vs
  MDE ≈0.40).

## What this does and does not prove

- **Proves (deterministic).** On the frozen contract schedule, the shipping
  resident session already removes ≥2× of the independent batch's seed-node
  execution on DOCX/EPUB and also cuts seed bytes (4.5×) and derived-cache writes
  (5–21×); the index descent is not deduped by any lane.
- **Does not prove** that a fused executor is impossible in principle, nor that
  node counts equal time/bytes. The **byte-weighted** view is dominated by the
  (undeduped) index bytes, so the composite byte ratio is ≈1.0 while the
  node-execution ratio is ≈4×; a clean `≥2×` gate on decoded/materialized work is
  **not established** by this court. Neither side has a true timing claim here —
  this is a counter court (the DOCX/EPUB *resident* lane is even slower in wall
  because `--no-cache` disables cache consultation but not cache writes; wall is
  recorded, not interpreted).

## Decision

**Do not build a fused executor as scoped (22.3.1).** The redundancy it targets is
already removed by the session's memo + cache; the only structurally unexploited
axis (index/selector descent) is the previously-rejected 22.2 candidate. **No
production code changed** in 22.3.0; one measurement-only counter court and its
aggregator were added.

**What would change the decision.** A workload whose requests share *executed
nodes the memo/cache cannot share* — i.e. where `resident_exec` stays far above
the true union — or a **decoded-bytes** counter showing the removable work is
byte-heavy rather than node-heavy (the counter this court lacked). Either would
justify reopening 22.3 with a new falsifiable hypothesis, per the standing rule.

See [ADR-0054](../adr/0054-repeatability-and-paired-measurement.md) for the
estimator discipline this court does *not* need (its metrics are deterministic)
and [phase-22-results.md](phase-22-results.md) §22.2 for the index finding.
