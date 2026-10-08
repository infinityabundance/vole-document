# Phase 22.4 results — unknown-query lifetime frontier

Branch `staging`. Measured at commit `b4225fe` (dirty tree: the Phase-22.4 change
set, recorded in the receipt). Base `main` @ `v0.1.0-alpha.28` (Phase 25).

**Verdict: `MIXED` — a region split, not a blanket win.** On a pre-registered
**hidden** schedule, VOLE leads in a **resolved** PDF region (~**0.73–0.83×** the
tuned SQLite `full` cumulative query cost per equivalent successful observation),
**loses** in a **resolved** EPUB region (~**1.50–1.82×**), and is **unresolved**
(pooled, DOCX, and every size class; MDE ±0.29–0.36) elsewhere. Storage is a
resolved **win** (~**0.47×**); peak **RSS is a resolved loss** (~1.25×). This is
exactly the "no advantage outside a narrowly characterized region, if any" prior
the [scope](phase-22-3-scope.md)/plan set, and it is recorded as such.

## Question

[Phase 22.4](phase-22-plan.md) (P3): on a schedule the encoder never saw, which
system wins the **cumulative lifetime frontier per equivalent successful
observation**, with **all adaptation costs charged**? The binding prior is a
recorded negative: Phase 15.6 adaptive procedural promotion **lost** (ADR-0046).

## Method

For each of the 12 `real100-v1` documents (4 pdf / 4 docx / 4 epub), both systems
see the document and choose their representation under a predeclared budget; a
**pre-registered, seeded hidden schedule** is then revealed and served as **one
warm session per lane**, interleaved, with every adaptation charged:

- **VOLE** — packed store (`field-build --profile runtime --packed`), schedule
  served by `observe-batch` (its derived-cache writes are counted by its own
  stats).
- **SQLite** — the tuned envelope `full` (primary) and `adaptive` (lazy
  materialization, all cost charged), plus the `hist` historical control.
- **Estimator** — ADR-0054: paired per-rep ratios, interleaved, every sample
  retained, fixed-seed cluster bootstrap by document, ±10% tie band, median and
  geometric mean, and the **MDE**.

**Pre-registered schedule (frozen).** Generator `tools/fixtures/phase22-4-schedule.py`;
seed **220400**, length **24**, one schedule per document seeded
`sha256("phase22-4|220400|<id>|<fmt>")`. Proportions are an **undisclosed seeded
draw** (weights `Uniform(0.5,2.0)`, coverage-guaranteed, twice-shuffled) — not
tuned to VOLE. Pooled slot counts: `bytes 39, text 48, doc-text 22, heading 25,
table 21, resource 27, metadata 44, revision 62`. The seed and the per-document
schedule are in the receipt.

**Receipt.**
[`2026-10-08-phase22-4-lifetime-b4225fe`](../../evidence/campaigns/2026-10-08-phase22-4-lifetime-b4225fe/)
(+ a second-seed check
[`…-seedcheck`](../../evidence/campaigns/2026-10-08-phase22-4-lifetime-b4225fe-seedcheck/),
seed `220477`). Service `doc-baseline`. Court
`tools/phase22-4-lifetime-court.sh`.

## Frontier by region (VOLE / SQLite; < 1 favours VOLE)

**Cumulative query cost per equivalent successful observation** (primary = tuned
`full`; pooled 12, and per format):

| region | C0 | C3 | C5 | MDE | verdict |
|---|---|---|---|---|---|
| pooled (12) | 1.066 (0.820–1.484) | 1.076 (0.804–1.394) | 1.001 (0.742–1.324) | ±0.29–0.36 | **unresolved** |
| **pdf (4)** | 0.816 (0.775–0.907) | 0.815 (0.786–0.938) | 0.750 (0.697–0.893) | ±0.07–0.10 | **win (resolved)** |
| docx (4) | 0.940 (0.730–1.323) | 0.934 (0.651–1.294) | 0.882 (0.607–1.205) | ±0.29–0.33 | unresolved |
| **epub (4)** | 1.696 (1.546–1.969) | 1.633 (1.396–1.756) | 1.504 (1.301–1.640) | ±0.17–0.24 | **loss (resolved)** |

Other axes (pooled): **store bytes 0.468 (0.313–0.737)** → **win** (docx 0.321,
epub 0.348; pdf 0.769 unresolved); **peak RSS 1.24–1.32** (CIs exclude 1.0) →
**loss**; **build** 0.525 (0.222–2.060) → unresolved (docx 0.375 win). Size-class
regions (`<100KiB`, `100KiB–1MiB`, `1–10MiB`) are all **unresolved**; the
secondary `adaptive`/`hist` query regions are unresolved (0.98–1.16).

## Exactness / equivalence

- `materialize --exact` **12/12** byte-exact (length + SHA-256 + `cmp`) for VOLE,
  `full`, `adaptive`, and `hist`; `cmp` VOLE-vs-SQLite **equal for all 12**.
- Equivalence over every depth × schedule slot (n=1728): **0 value mismatches**;
  **102 divergent** observations, all PDF page-text (VOLE's heuristic projection
  vs Poppler, by design — the same divergence Phase 22.1 recorded).
- Declines excluded identically from both numerator and denominator (at C0–C3 the
  baseline declines `revision`, a C4 capability, while VOLE answers it; common
  206/288; at C4–C5 common 243/288).

## What this does and does not prove

- **Proves (measured, interval-bounded).** On this schedule, VOLE's cumulative
  query cost per equivalent observation is a **resolved win in the PDF region**
  and a **resolved loss in the EPUB region**, with the pooled population
  **unresolved** at this cluster count (4 docs/format). Storage is a resolved
  win; RSS is a resolved loss.
- **Does not prove** a cause. The court measures the split but does not decompose
  which schedule slot drives it (the formats differ in vocabulary — pdf has no
  `doc-text`); no mechanism is claimed. Four clusters per format is few — a
  one-document change could move a region.
- **Robustness.** A second schedule (seed `220477`, REPS=15) **reproduced the
  split** (pdf win ~0.75–0.83×; epub loss ~2.0–2.35×; docx/pooled unresolved).

## Decision

Recorded as a **region-scoped mixed result**, not a frontier win. It does not
justify a mechanism change; it **characterizes** where the existing runtime leads
and loses under an unknown-query schedule. The pre-registered negative prior was
largely borne out: no pooled advantage, a narrow resolved PDF win, and a resolved
EPUB loss. See [ADR-0054](../adr/0054-repeatability-and-paired-measurement.md)
for the estimator discipline and [phase-22-plan.md](phase-22-plan.md) §22.4 for
the gate.
