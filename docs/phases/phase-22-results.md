# Phase 22 results

Branch: `phase22` (22.1; released `v0.1.0-alpha.26`). **22.2 was measured on the
`phase23` branch** and released in `v0.1.0-alpha.27`. Base: `main` @
`v0.1.0-alpha.25` (Phase 20). Plan: [phase-22-plan.md](phase-22-plan.md).

**Programme in progress.** The Phase-22 economic programme is ordered
22.1 → 22.7 (P0–P6); **22.1 and 22.2 are complete and sealed**, and **22.3–22.7
are still planned**, not measured. Nothing below should be read as a Phase-22
frontier claim: 22.1 is the *competitor envelope* that must exist **before** any
such claim is made, and 22.2 is a **profiling gate** that decided **not** to
build its candidate. Every figure in 22.1 links to the sealed receipt
[`evidence/campaigns/2026-10-08-phase22-competitors-86d9312`](../../evidence/campaigns/2026-10-08-phase22-competitors-86d9312/)
and to the frozen decision records [ADR-0053](../adr/0053-batched-packed-sync.md)
and [ADR-0054](../adr/0054-repeatability-and-paired-measurement.md).

## 22.1 Competitor envelope — strengthen the competitor first

**Question.** Is there a **strong competitor envelope**, including an adaptive
and a hybrid SQLite, before any VOLE frontier claim is made at all? The plan's
binding first consequence is that *the competitor is maximized first*: a court
that beats a weak or deliberately crippled baseline proves nothing
([phase-22-plan.md](phase-22-plan.md), governing thesis / honest prior).

**Receipt.**
[`2026-10-08-phase22-competitors-86d9312`](../../evidence/campaigns/2026-10-08-phase22-competitors-86d9312/)
(`SUMMARY.md`, `FRONTIER.md`, `raw/`; committed at `86d9312`, release profile,
base image `rust:1.99.0-slim-bookworm@sha256:452176c0…`, SQLite `3.40.1`,
`reps=10`, bootstrap 20,000 resamples seed `220019`, ±10% tie band, same
12-document C0–C5 court and same frozen `sql_for` answer SQL as the historical
control). Accounting is regular-file bytes for **both** lanes; `du -sb` is never
used ([ADR-0049](../adr/0049-storage-accounting-correction.md)).

**Result — the competitor was strengthened before anything was claimed.** Six
purpose-tuned SQLite configurations were added (`tools/fixtures/phase22-competitors.py`);
the Phase-18 baseline is kept **unmodified** as the historical-control lane
`hist`:

- **`minimal`** — source + value/metadata, no secondary indexes, no FTS.
- **`fts`** — minimal + **one** FTS5 index (external-content, `detail=none`,
  `columnsize=0`, unicode61 — no trigram).
- **`structural`** — + a `native_coord` column/index.
- **`full`** — all contract projections; **no FTS** because the contract has no
  search observation.
- **`adaptive`** — builds minimal, lazily materializes on first demand, all cost
  charged.
- **`hybrid`** — `full` + 128 MB cache, `journal_size_limit`, Poppler native PDF
  extraction.

All use one transaction plus `executemany` per build and explicit PRAGMAs.
`minimal`/`fts` cover only C0, `structural` covers C0–C1, and
`full`/`adaptive`/`hybrid` cover the full C0–C5 contract.

**Frontier (12 documents, N=10, pooled medians; `bytes` = regular-file bytes).**
VOLE's frontier point is depth-independent in build/bytes: build **13.2 ms**,
**7,778,087 B**, cold **8.44 ms**, warm **2.49 ms** (the C5 column).

| C5 lane | build ms | bytes | cold ms | warm ms |
|---|---:|---:|---:|---:|
| **VOLE** | **13.2** | **7,778,087** | **8.44** | 2.49 |
| `full` | 59.8 | 10,201,834 | 10.92 | **2.00** |
| `adaptive` | 58.2 | 9,661,209 | 68.63 | 2.01 |
| `hybrid` | ≈ `full` | ≈ `full` | ≈ `full` | ≈ `full` |
| `hist` (control) | 68.5 | 14,723,834 | 11.10 | 1.94 |

`adaptive`'s cold column is large **by construction**: its lazy materialization
is charged to the first cold call. `hist` carries the control's **two** FTS5
indexes.

**Paired VOLE/lane ratios at C5** (per-rep paired ratio; < 1 favours VOLE):

| axis | vs `full` | vs `adaptive` | vs `hist` (control) |
|---|---:|---:|---:|
| build | **0.219** | 0.210 | 0.194 |
| bytes | **0.762** | 0.805 | 0.528 |
| cold | **0.812** | 0.153 | 0.796 |
| warm | 1.211 (95% CI 1.006–1.483) | 1.238 | 1.253 |

**Durability is compared explicitly, never assumed equal.** The matched
headline setting is SQLite `journal_mode=WAL` + `synchronous=NORMAL`, which
stays internally consistent after a crash but can lose the **latest committed
transactions** on certain power failures; VOLE's packed `Batch`
([ADR-0053](../adr/0053-batched-packed-sync.md); Phase 20.1) flushes before
manifest publish, recovers a strict prefix, and never exposes a partial node.
**Neither side has a true-power-loss receipt** (Phase 20.1 injects process death
only). A `synchronous=FULL` probe is measured separately: build **6.4 → 24.3 ms**
(**~3.6×**), bytes unchanged. `NORMAL` and `FULL` are reported separately and
never conflated.

**Closure.** Equivalence **1920/1920** configuration envelopes byte-identical to
the `full` reference, **0** mismatches; exactness **12/12** VOLE and **6/6** per
document per configuration (length + SHA-256).

> **Caveat on what that 1920/1920 means.** Those envelopes are **SQLite-to-SQLite**
> (each configuration against the `full` SQLite reference) — they are *not* 1,920
> VOLE-vs-SQLite semantic matches. The court separately records **96 divergent PDF
> text observations** (VOLE's heuristic page-text projection vs Poppler), which
> remain relevant to any claim of equivalent answers. The VOLE-vs-SQLite comparison
> is therefore equality of *contract shape* plus a recorded divergence on one
> observation class, not full semantic identity.

**Interpretation — the shrinkage is the finding.** Strengthening the competitor
is the point of 22.1, and the strengthened competitor shrinks the previously
reported VOLE edges. The direction of every surviving edge is unchanged, but the
"solo storage win" headline was measured against a baseline carrying a
**contract-dead** trigram/FTS index, so the honest equal-contract storage
advantage is **~0.76–0.80×, not 0.53×**. The **warm win does not exist**: VOLE
is **1.15–1.25× slower** and `full`/`hybrid`/`hist` win the warm axis, consistent
with the durable warm loss recorded in [ADR-0054](../adr/0054-repeatability-and-paired-measurement.md).

**Limitation.** The 12-document contract subset is not the frozen population.
**Not measured:** true power loss on either side; `FULL` on a multi-commit
workload; a genuinely native DOCX/EPUB hybrid extractor (`hybrid`'s native side
is Poppler for PDF plus Python stdlib for OOXML/EPUB, not a rewrite of VOLE's
adapters); cross-host results. The bind-mounted store is noisy and intervals are
reported as wide.

## What 22.1 established

- **No capability gap remains.** Every required C0–C5 contract depth is covered
  by at least one optimized configuration, and the historical control is
  retained, not replaced. A configuration is only compared at a depth it
  actually covers.
- **The earlier storage headline was measured against a contract-dead index.**
  The `hist` control carries the Phase-18 baseline's **two** FTS5 indexes,
  although the contract has **no search observation**. Removing that dead index
  (`hist` 14,723,834 B → `full` 10,201,834 B at C5) is **≈ +44% bytes / 4.5 MB
  over 12 docs** of pure baseline overhead. The honest storage advantage against
  a Pareto-tuned equal-contract SQLite is therefore **~0.76×** (`full`) to
  **~0.80×** (`adaptive`) — **not the previously reported 0.53×**.
- **The build advantage survives, but is smaller.** ~5.2× faster than the
  historical control (`hist`, paired build 0.194) becomes ~4.6–4.8× faster than
  the optimized lanes (paired build **0.219** vs `full`, 0.210 vs `adaptive`).
  VOLE still builds the subset faster than every tuned configuration.
- **The cold advantage survives** (~0.81× vs `full`), and is much larger vs
  `adaptive`'s charged-on-first-demand lazy materialization (0.153).
- **The warm advantage does not exist.** VOLE is **1.15–1.25× slower** warm
  (1.211 vs `full`, 95% CI 1.006–1.483, i.e. excluding 1.0; 1.253 vs `hist`);
  `full`, `hybrid`, and `hist` win the warm axis. This matches the durable warm
  loss in Phase 20.3 / ADR-0054.
- **Exactness and answer equivalence are untouched:** 12/12 VOLE exact, 6/6 per
  document per configuration, 1920/1920 envelopes byte-identical, 0 mismatches.

## 22.2 Compact query-native directory — profiling gate says DO NOT BUILD

**Question.** `src/field/index.rs` persists hash-addressed index nodes as
**individual files**. Can the warm request critical path be made materially
cheaper — without adding persistent bytes or weakening integrity checks?

**Method — attribute the warm session before designing a layout.** Per the
Phase-22 plan's own rule ("if index traversal is a small part of total work,
don't force an index redesign merely because the phase is named after it"), the
subphase ran a **decisive profiling gate first**, with five independent
instruments, and shipped **no structural change**.

**Receipt.**
[`2026-10-08-phase22-2-d81689c`](../../evidence/campaigns/2026-10-08-phase22-2-d81689c/)
(`SUMMARY.md`, `raw/profile/`, `raw/court-summary.md`, `raw/warm_samples.tsv`);
profiler `src/field/prof.rs` (env-gated `VOLE_PROFILE_OPEN`, **off by default**),
`tools/phase22-2-profile.sh`, `tools/phase22-2-court.sh`. Same 12-document
subset; the SQLite lane is the Phase-22.1 tuned `full` envelope; ADR-0054
estimator (N=100 paired, interleaved, every sample retained, bootstrap 20,000
seed 220200). Same accounting as 22.1 (ADR-0049).

**Stage attribution (12-doc packed subset, one warm session per document,
pooled 22,219 µs):**

| stage | share |
|---|---:|
| open (manifest + descriptor read + `Descriptor::parse`) | **55.5 %** |
| — of which `Descriptor::parse` | **44.7 %** |
| request loop | 44.5 % |
| — dispatch (evaluation core) | 32.0 % |
| — serialize (answer JSON) | 8.4 % |
| — materialize / typed-model decode | 6.3 % |
| — **probe (selector resolution)** | **0.4 %** |
| index node read + BLAKE3 verify | 10.5 % |
| index node decode | 0.1 % |

**The index term is redundancy, not lookup work.** `strace -e trace=openat`
shows **493 index-node opens for 20 distinct files** across the 12 sessions:
`FsIndexStore` stores hash-addressed nodes as individual files, the trees are
depth-0 (one leaf is the root), and every selector descent **re-opens, re-reads
and re-hashes the same immutable node** (`nist-epub-0009` opens one file **117×**).
`parse_node` is 0.1 %.

**Computed headroom — below resolution.** A *perfect* offset-addressed selector
directory removes at most the index share (**10.6 %** pooled / 10.0 % median;
PDF 0.9–3.6 %, DOCX 4.2–15.7 %, EPUB 15.0–24.1 %), touches none of the 44.7 %
parse, 32.0 % dispatch or 8.4 % serialize, and would **add** persistent bytes.
Implied headline shift ≈ **0.13**. The court's **minimum detectable effect at
N=100 is ≈ 0.399** (half-width ±0.279), so a perfect directory is **~3× below
the MDE and cannot be credited even if built**.

**Before/after (the AFTER binary is the BEFORE binary plus the off-by-default
profiler — no wire byte, no on-disk layout, no answer change):**

| run | docs | pairs | median ratio (95 % CI) | geo-mean (95 % CI) | W/T/L |
|---|---:|---:|---|---|---|
| BEFORE — Phase 22.1 (`2026-10-08-phase22-competitors-86d9312`) | 12 | 1200 | 1.211 (1.006–1.483) | — | — |
| AFTER — this run | 12 | 1200 | **1.293 (1.002–1.561)** | **1.246 (1.048–1.488)** | 2 / 4 / 6 |

The two runs agree within their intervals: a **NULL**. The warm position is a
**real loss vs the tuned envelope, unchanged** — the selector layout is **not**
the lever.

**Integrity / semantics / bytes.** VOLE `materialize --exact --packed` **12/12**,
the tuned SQLite `full` retained blob **12/12**; **480** warm answers, **0** value
mismatches; persistent bytes VOLE **7,778,087 B** vs `full` **10,199,040 B** =
**0.763×**, reproducing 22.1's 0.762× storage headline byte-for-byte. No layout
byte changed.

## What 22.2 established

- **The index/selector layer is not the warm bottleneck.** It is **10.6 %**
  pooled, dominated by redundant re-reads of one immutable content-addressed
  leaf (493 opens / 20 files); selector resolution is **0.4 %**; the dominant term
  is the one-time full **`Descriptor::parse` (44.7 %)** at field open.
- **Nothing structural was shipped**, and the before/after warm court is a
  **NULL** (1.211 → 1.293, overlapping CIs). The candidate cannot be resolved by
  this court (~0.13 implied shift vs MDE ≈0.40), so building it would be
  unjustifiable complexity — precisely the "failed gate is a publishable
  negative" outcome the plan pre-registered.
- **Cheapest real lever identified for later, not built:** an **in-session
  verified-node memo** (one immutable leaf re-read 24–117×) removes most of the
  10.6 % for **zero** persistent bytes — still sub-MDE, so a future phase with a
  **higher-resolution court** is required before crediting it. `dispatch` (32 %)
  is a single bucket and must be split before anything is credited against it.
