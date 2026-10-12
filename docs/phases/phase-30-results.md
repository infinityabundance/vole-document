# Phase 30 results — close the measured gaps (large-input kernels; reading, index cost, framing pending)

Branch `phase30`. Phase 30 attacks, in order, the four gaps Phase 29's own report
named. The frozen contract is [phase-30-plan.md](phase-30-plan.md). Same rule as
every subphase: exactness is the invariant, negatives are recorded, no claim
without evidence.

```text
30.1  Large-input fused kernels   — SHIPPED (this document)
30.2  Coarse index for reading    — PENDING
30.3  Incremental index cost      — PENDING
30.4  Standing negatives          — PENDING
```

**Outcome in one line.** 30.1 turned the recorded Phase-29.1 large-input loss into
a win: the fused kernel now beats **pyarrow on every large Arrow and Parquet cell**
(Arrow **≈2.87 → 0.65**, Parquet **≈1.28 → 0.61**, ratios `< 1` favour fused) and
cut the large CSV loss **≈46.2 → 35.5** (improved ≈23 %, but still **10.5–42×
slower**). On the full econ matrix the pooled pyarrow median improved **0.0772 →
0.0461** (W/T/L **406/41/163 → 489/8/81**), exactness held, and the Arrow/Parquet
gain is a **code change**, not a comparator change. **The CSV loss remains**, and
30.2–30.4 are **not started**.

## 30.1 — Large-input fused kernels

Phase 29.1 showed the generalized fused kernel (`src/adapter/fused.rs`,
`FUSED_PLAN_VERSION = 1`; `fused-scan`) losing the large-input lane — `e_large`
fused/pyarrow ≈**2.7–4.2×** on Arrow and **15–52×** on CSV, winning Parquet only
the stats-skip `count`. 30.1 replaces the per-value `Vec` + `Value` decode in the
physical scanners with a **monomorphized, single-pass fold over the addressed
column's contiguous bytes**, so the reader never materializes an intermediate
per-value collection:

- **`src/adapter/fused.rs`** — one monomorphized, single-pass
  `push_i64_le::<W, SIGNED>` / `push_f64_le::<W>` fold (`chunks_exact(W)`
  alignment, no panic on a non-multiple remainder; the `SIGNED`/width branches
  resolve at monomorphization), plus an **exact `i128` sum** for the integer
  `avg` so the mean is divided once and matches the reference.
- **`src/adapter/arrow.rs`** — a **word-wise validity bitmap** (bounds-checked) so
  the null mask is consumed a machine word at a time instead of bit-by-bit.
- **`src/adapter/parquet.rs`** — a `PLAIN` + `max_def == 0` fast path with
  `checked_mul`/`get`, keeping the stats-skip `count` short-circuit.
- **`src/adapter/csv.rs`** — an **allocation-free SWAR field scanner** with
  bit-identical native int/float parsers (the field is scanned without building a
  `Value` or a row vector).

Exactness, the versioned plan, the validity/SQL-null semantics, and the typed
declines are unchanged: the kernel still reads **only the addressed column**,
everything outside its subset declines **typed**, and the kernel is off the
exactness path. Regression tests live in `tests/fused_kernel.rs`; the correctness
court is [`tools/phase29-1-1-fused-court.sh`](../../tools/phase29-1-1-fused-court.sh).

**The economic-court `IFS` defect (fixed).** The 29.1 econ court,
[`tools/phase29-1-fused-econ-court.sh`](../../tools/phase29-1-fused-econ-court.sh),
set its field separator with bash's `IFS=$'\t'`. The court's documented entry
point is `sh tools/…`, and dash's `sh` **does not expand `$'…'`**, so the
separator became the literal four characters `$\t` and the `plan.tsv` row parse
corrupted the paths/columns — a silent measurement defect. It is now
`TAB=$(printf '\t')` with `IFS="$TAB"` (a POSIX tab), and the same court is reused
by 30.1 with `ECON_FIXTURES=e_large`, so the 30.1 methodology is byte-identical to
29.1. (The court also gained backwards-compatible `CAMPAIGN_NAME`/`PHASE_TITLE`/
`PHASE_DESC`/`ECON_FIXTURES` knobs so a follow-on phase seals its own receipt.)

### Result (in-process steady-state; ratio `fused / comparator`, `< 1` favours fused)

Estimator: ADR-0054 paired per-(fixture, format, question) ratio, median +
geometric mean, fixed-seed cluster bootstrap 95 % CI, MDE; every raw sample
retained. The before numbers are the sealed Phase-29.1 control,
[`2026-10-12-phase29-1-fused-19d2c372`](../../evidence/campaigns/2026-10-12-phase29-1-fused-19d2c372/);
the after numbers are the 30.1 large-input court,
[`2026-10-12-phase30-1-kernels-2ec78328`](../../evidence/campaigns/2026-10-12-phase30-1-kernels-2ec78328/)
(`mode full`, `analytical`).

| large-input lane (`e_large`) | before | after | note |
|---|---:|---:|---|
| Arrow, fused / pyarrow median | **≈2.87** | **≈0.65** | **every cell `< 1.0`** |
| Parquet, fused / pyarrow median | **≈1.28** | **≈0.61** | **every cell `< 1.0`** |
| CSV, fused / pyarrow median | **≈46.2** | **≈35.5** | improved **≈23 %**, still **10.5–42× slower** |

Full econ matrix ([`2026-10-12-phase29-1-fused-2ec78328`](../../evidence/campaigns/2026-10-12-phase29-1-fused-2ec78328/),
114 clusters / 570 samples):

| comparator | metric | before (19d2c372) | after (2ec78328) |
|---|---|---:|---:|
| pyarrow | median | **0.0772** | **0.0461** |
| pyarrow | W/T/L | **406/41/163** | **489/8/81** |
| pyarrow | geomean | 0.2626 | 0.1683 |
| DuckDB | W/T/L | 570/0/0 | 570/0/0 |
| shipped in-process `observe` (`count`) | W/T/L | 70/0/0 | 70/0/0 |

- **Arrow large inputs now beat pyarrow in every cell** (`id:avg` 0.823,
  `id:count` 0.663, `score:sum` 0.603, `score:min`/`score:max` 0.310/0.309). The
  Phase-29.1 `id:avg` 4.17× outlier is gone.
- **Parquet large inputs beat pyarrow in every cell** too, including the
  stats-skip `count` (`id:count` 0.111, `score:count` 0.114).
- **CSV remains a recorded loss.** The kernel improved ≈23 % (bilinear decode
  removed) but pyarrow's C++ CSV reader still wins **10.5–42×** on the large
  fixture; this is a **recorded loss, not a capability gap**.
- Against **DuckDB** the kernel is a win in **every cell**; on the large-input
  court the W/T/L is **150/0/0** (the pooled full-matrix DuckDB lane is
  **570/0/0**).
- **Exactness: 3/3** (fixture × format) on the large-input court, and **12/12** on
  the full econ court; oracle agreement **228/228** — no corrected cell, only a
  faster one.

### The `dev` kernel lane is not the headline

The court,
[`tools/phase30-1-kernels-court.sh`](../../tools/phase30-1-kernels-court.sh), has
two lanes. The **`analytical` lane (`mode full`)** regenerates the aligned
`e_large` corpus and measures the live fused-vs-pyarrow/DuckDB comparison — **it
is authoritative**, and it is the receipt sealed above. The **`dev`/kernel lane
(`mode kernel`, no python3/pyarrow/duckdb)** measures the fused kernel over the
committed `tools/fixtures/{arrow,parquet,csv}/large.*` fixtures and checks
exactness/structural correctness only; its **absolute nanosecond figures are not
comparable to `e_large`** (different rows/columns, and it cannot claim a
comparator ratio it cannot measure), so no comparison should be read from it. The
sealed receipt on disk is the analytical one; any kernel-lane output is written
beside it under `kernel-lane/` and never clobbers it.

Receipt: [`2026-10-12-phase30-1-kernels-2ec78328`](../../evidence/campaigns/2026-10-12-phase30-1-kernels-2ec78328/)
(`+ kernel-lane/` for a `dev` run).
Court: [`tools/phase30-1-kernels-court.sh`](../../tools/phase30-1-kernels-court.sh)
(delegating to [`tools/phase29-1-fused-econ-court.sh`](../../tools/phase29-1-fused-econ-court.sh)
with `ECON_FIXTURES=e_large`).
Control: Phase 29.1 [`…-fused-19d2c372`](../../evidence/campaigns/2026-10-12-phase29-1-fused-19d2c372/).

## 30.2–30.4 — pending

These subphases are **planned and frozen** in [phase-30-plan.md](phase-30-plan.md)
and **not started**; nothing is implemented, measured, or claimed for them:

- **30.2 — coarse index for reading requests.**
  [Phase 29.2](../phases/phase-29-results.md) shipped the coarse structural index
  but recorded that the **natural reading request narrows in only 1/4 formats**
  (DOCX/ODT/Parquet materialize the whole exact source). 30.2 is to **serve the
  natural reading request from coarse member spans** — pending.
- **30.3 — incremental index cost.**
  Phase 29.3 verified incremental observation correct but at an index of
  **27.3 %** of source, **above the Phase-28.7 gate's ≤10 % bar**. 30.3 is to reach
  a **dependency-signature-only index (< 10 %)** while keeping correctness —
  pending.
- **30.4 — standing negatives.**
  The **15 detection findings**, the **legacy bounded subsets**, and the **legacy
  Arrow (pre-0.15) no-continuation framing** remain recorded. 30.4 is to attack
  them — pending.

## Verdict

- **30.1 — SHIPPED: a resolved large-input win on Arrow and Parquet, a recorded
  CSV loss.** The monomorphized single-pass folds, word-wise validity, and the
  allocation-free CSV SWAR scanner took the large Arrow lane **≈2.87 → 0.65** and
  Parquet **≈1.28 → 0.61** (every cell `< 1.0`), and cut CSV **≈46.2 → 35.5**
  (improved ≈23 %, still 10.5–42× slower). Full-matrix pyarrow median **0.0772 →
  0.0461**, W/T/L **406/41/163 → 489/8/81**; vs DuckDB **150/0/0** on the
  large-input court. Exactness **3/3** (large-input) / **12/12** (full econ),
  oracle agreement **228/228**. The econ court's `IFS` defect is fixed. The `dev`
  kernel lane is exactness/structural only — its absolute ns are **not**
  comparable, and the `analytical` run is authoritative.
- **30.2–30.4 — PENDING.** No implementation, measurement, or claim.
