# Phase 22 results

Branch: `phase22`. Base: `main` @ `v0.1.0-alpha.25` (Phase 20). Plan
(PLANNED, not started at the time of writing): [phase-22-plan.md](phase-22-plan.md).

**Programme in progress.** The Phase-22 economic programme is ordered
22.1 → 22.7 (P0–P6); **22.1 is complete and sealed**, and **22.2–22.7 are still
planned**, not measured. Nothing below should be read as a Phase-22 frontier
claim: 22.1 is the *competitor envelope* that must exist **before** any such
claim is made. Every figure in 22.1 links to the sealed receipt
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
