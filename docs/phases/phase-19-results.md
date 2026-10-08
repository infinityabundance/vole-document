# Phase 19 results

Branch: `phase19`. Measured at commits `d6c8c4c` (19.1), `6b66eab` (19.2), and
`954dbc2` (19.3); base release `v0.1.0-alpha.23` (Phase 18).

Phase 19 is a **measurement-discipline phase**. Phase 18 closed with two
*single-shot* point estimates — the equal-contract build at **0.82×** SQLite and
the warm one-session lane at **1.09×** — each reduced to a best-of-3 minimum.
Phase 19 replaces those estimates with **paired, interleaved, repeated**
measurements and a fixed-seed bootstrap interval (19.1), extends the warm lane to
**N=100** (19.2), and runs the new direct build over the **full frozen
`real100-v1` population** (19.3) rather than the 12-document contract subset.
Every individual sample is retained; no number is reduced at collection time.
Every figure below links to a sealed receipt under
[`evidence/campaigns/`](../../evidence/campaigns/); estimator and interval are
stated wherever a ratio is claimed. Decision record:
[ADR-0054](../adr/0054-repeatability-and-paired-measurement.md).

## What Phase 19 established

- **The build win survives paired interleaved measurement — but its *magnitude*
  is estimator-dependent.** The paired per-rep VOLE/SQLite build median is
  **0.182** (95% CI **0.102–0.228**, entirely below 1.0; 11 win / 0 tie / 1 loss).
  The Phase-18 **ratio-of-sums** estimator, re-run on the same samples with a
  median-of-10 reduction, gives **~0.96** — because one heavy document
  (`nist-pdf-0017`) dominates VOLE's *total* build while SQLite's total is
  spread across documents. Both estimators agree the direction; they disagree on
  the size, so the claim must name its estimator (ADR-0054).
- **The Phase-18 build headline corrects from 0.82× to ~0.96×** under the
  ratio-of-sums estimator. The **VOLE** build sum is reproducible (1545.5 vs
  1552 ms across the two runs); the move is **SQLite-lane host variance**
  (1893 → 1582 ms), not a VOLE regression.
- **The warm position is a modest real loss (~1.29×), statistically marginal.**
  At N=100 the pooled median paired warm ratio is **1.292** (95% CI
  **0.994–1.658**) — the CI *includes* 1.0, so the median estimator does **not**
  resolve a loss — but the geometric-mean CI (**1.069–1.535**) **does** resolve
  > 1.0. VOLE is ~0.7 ms slower per warm session. The N=10 warm estimate
  (1.077) was under-sampled; the corrected reading is a **modest real loss, not
  parity**.
- **The new direct build is robust across the full frozen population.** The
  direct `field-build --packed --sync=batch` path builds **100/100** documents
  (pdf 60/60, docx 15/15, epub 25/25, every size class) and is **100/100 exact**
  (length + SHA-256), recovering the three documents the old two-step path
  failed. It pays a small byte cost (**+5.3%** on the 96 common documents)
  because it **fixes** the runtime/RAW program instead of **searching** for the
  smallest; **memory, not wall, is the next binding constraint** for sources
  ≳1 GiB.
- **Exactness is untouched throughout:** 12/12 VOLE `materialize --exact
  --packed` and 12/12 SQLite retained blob (19.1, 19.2); 480 warm-session
  envelopes compared with **0 value mismatches** (19.2); **100/100** on
  `real100-v1` (19.3).

## 19.1 Repeatability court (paired, interleaved, N=10)

**Question.** Phase 18.5's build (**0.82×**) and warm (**1.09×**) headlines each
came from one run with a best-of-3 minimum. Re-measure the SAME 12-document C0–C5
equal-contract court with **N=10 paired repetitions of both lanes at every
depth**, interleaved, retaining every sample and quoting a bootstrap interval:
is the build ratio < 1.0, and is the warm ratio > 1.0 or parity?

**Receipt.**
[`2026-10-08-phase19-repeat-d6c8c4c`](../../evidence/campaigns/2026-10-08-phase19-repeat-d6c8c4c/).

**Design.** 12 documents × depths C0–C5 × **N=10** reps of **both** lanes;
interleaved order (odd reps VOLE→SQLite, even reps SQLite→VOLE; per-rep order in
`raw/order.tsv`); one untimed warm-up precedes each lane's timed queries;
bootstrap **20,000 resamples, seed 190019**, **cluster-resampled by document**;
**±10%** tie band. Every sample is retained in `raw/*_samples.tsv`. Persistent
bytes are the sum of **regular-file** sizes for both lanes (`du -sb` is never
used). Same source-retaining SQLite fixture as Phase 18
(`phase18-contract-packed.py`, byte-identical).

**Result — BUILD established as < 1.0; the magnitude is estimator-dependent.**

| estimator (build, VOLE/SQLite) | VOLE | SQLite C5 | ratio |
|---|---:|---:|---:|
| Phase-18 best-of-3 (min of first 3), ratio of sums | 1545.5 ms | 1582.6 ms | **0.977** |
| this court median-of-10, ratio of sums | 1544.6 ms | 1604.4 ms | **0.963** |

- **Paired per-rep median ratio 0.182** (95% CI **0.102–0.228**); geometric mean
  **0.203** (95% CI 0.128–0.395). The CI lies **entirely below 1.0**.
- Per-document ±10% band: **11 win / 0 tie / 1 loss**. The single loss is
  `nist-pdf-0017` at **4.440×** (its large store write lands on the VOLE lane).
- **Correction to the Phase-18 headline:** the ratio-of-sums estimator reads
  **~0.96**, not **0.82**, because VOLE's *total* build is dominated by that one
  heavy document while SQLite's total is spread. The 0.82 → 0.96 move is
  **SQLite-lane host variance** (1893 → 1582 ms); VOLE's own build sum is
  reproducible (1545.5 vs 1552 ms).

**WARM — not resolved at N=10.**

- Paired median ratio **1.077** (95% CI **0.809–1.391**, includes 1.0);
  geometric mean **1.065** (95% CI 0.880–1.287). Per-document ±10% band:
  **5 win / 1 tie / 6 loss**.

**Best-of-3 (min) vs median-of-10 (the min-reduction control).**

| quantity | Phase-18 best-of-3 (min) | this court median-of-10 |
|---|---:|---:|
| build ratio of sums | 0.977 | 0.963 |
| warm ratio of sums | 1.218 | 1.194 |

The min reduction did **not** materially bias either estimate (**≤2.4%**); it
shrinks both lanes' sums, and because the heavy store-write tail lives on the
VOLE lane, min-of-3 is expected to flatter VOLE relative to median-of-10.

**Exactness.** VOLE `materialize --exact --packed` **12/12**; SQLite retained
blob **12/12** (length + SHA-256).

**Interpretation.** The equal-contract build win is **real and not within
parity** on this subset: the paired median and its interval sit well below 1.0.
Its *magnitude* is a property of the estimator — a paired per-rep median (0.18)
answers "how much faster is VOLE on a typical rep", while a ratio of sums
(~0.96) answers "how do the two lanes' grand totals compare" and is dominated by
one document. The warm lane is indistinguishable from parity at this sample size.

**Limitation.** A 12-document subset; the bind-mounted host store is noisy and
the wide intervals are reported as wide. Warm sessions run in the low-millisecond
range, where process start-up (part of both lanes) is a material fraction. The
sampling unit is the document (cluster bootstrap), not the doc-rep pair.

## 19.2 High-N warm repeat (N=100)

**Question.** The 19.1 warm ratio (1.077) had a CI that included 1.0. Is the warm
lane a resolved loss, a resolved parity/win, or still undecided at higher N?
Build each lane's store **once**, then repeat the warm one-session lane **N=100**
per (document, depth, lane).

**Receipt.**
[`2026-10-08-phase19-warm-6b66eab`](../../evidence/campaigns/2026-10-08-phase19-warm-6b66eab/).

**Design.** Stores built **once** per document per lane (the one-time build is
not part of the warm ratio); warm one-session lane repeated **N=100** per
(document, depth, lane); interleaved odd/even order; one untimed warm-up session
per (document, depth, lane); every sample retained in `raw/warm_samples.tsv`;
bootstrap **20,000 resamples, seed 190102**, cluster-resampled by document;
**±10%** tie band.

**Result — NOT resolved under the median estimator; resolved under the geometric
mean.**

| statistic (warm, C0–C5 folded per rep) | value | 95% CI |
|---|---:|---|
| pooled median paired ratio | **1.292** | **0.994–1.658** |
| pooled geometric-mean paired ratio | **1.283** | **1.069–1.535** |

- **Median CI includes 1.0**, so the warm ratio is **not resolved at N=100** on
  this store and host. Per-document ±10% band: **2 win / 3 tie / 7 loss**.
- The **geometric-mean CI does resolve > 1.0**.
- **Pooled per-lane warm medians:** VOLE **2.25–2.32 ms** (CV ≈53%) vs SQLite
  **1.54–1.65 ms** (CV ≈21–25%) — VOLE is **~0.7 ms slower per session**.

**Per-depth median VOLE/SQLite warm ratio (paired per rep):**

| depth | median ratio | 95% CI |
|---|---:|---|
| C0 | 1.326 | 1.047–1.719 |
| C1 | 1.282 | 1.027–1.701 |
| C2 | 1.259 | 0.997–1.650 |
| C3 | 1.259 | 0.990–1.641 |
| C4 | 1.179 | 0.959–1.592 |
| C5 | 1.305 | 0.951–1.592 |

C0 and C1 are **resolved losses** (lower bound > 1.0); C2–C5 **include 1.0** —
SQLite's envelope grows with depth, shrinking the gap. VOLE's packed warm
session is depth-independent (the same store and request set serve every depth).

**Variance floor and minimum detectable effect at N=100.**

- within-cell between-rep CV (median over all (doc, depth, lane) cells):
  **5.4%** (p90 9.4%) — the host bind-mount repeat noise floor;
- headline pooled paired-ratio CV **32.2%**; median-CI half-width **±0.332**;
- **MDE (80% power, normal approx) ≈ 0.474** — a true median shift smaller than
  this is not resolvable with N=100 at this variance floor (an order-of-magnitude
  resolution statement, not a measured quantity).
- An **independent identical re-run also gave "includes 1.0"**.

**The point estimate moved from 1.077 (N=10) to 1.292 (N=100): the N=10 warm
estimate was under-sampled.** The corrected reading is a **modest real loss, not
parity**.

**Exactness / equivalence.** VOLE `materialize --exact --packed` **12/12**;
SQLite retained blob **12/12**; **480** warm-session envelopes compared, **0
value mismatches** (the non-`raw` cells are the documented
shape/observable/projection distinctions, not value disagreements).

**Interpretation.** Warm latency is a genuine but small VOLE deficit that the
median estimator cannot separate from 1.0 at this N, while the geometric mean —
less sensitive to the right tail of the ratio distribution — resolves it. The
honest statement is therefore a **loss of ~1.29× with a marginal median interval
and a resolved geometric-mean interval**.

**Limitation.** Warm-only: the one-time builds are paid once and are excluded
from the ratio. The interval depends on the store and host; a wider host noise
floor would widen it. The MDE uses a normal approximation derived from the
bootstrap half-width.

## 19.3 Direct build over the full `real100-v1` population

**Question.** Every Phase-18 build court used a 12-document subset. Is the new
direct build (`field-build SRC --store DIR --profile runtime --packed
--sync=batch`) robust across the **entire frozen `real100-v1` population** — does
it build and reconstruct every document, what does it cost, and where does it
hit its next limit?

**Receipt.**
[`2026-10-08-phase19-real100-direct-954dbc2`](../../evidence/campaigns/2026-10-08-phase19-real100-direct-954dbc2/).

**Result — 100/100 built, 100/100 exact.**

| group | docs | build ok | rate |
|---|---:|---:|---:|
| all | 100 | **100** | **100.0%** |
| pdf | 60 | 60 | 100.0% |
| docx | 15 | 15 | 100.0% |
| epub | 25 | 25 | 100.0% |

- rc histogram is **all 0** — no `rc 124` timeouts, no `rc 137` OOMs. The OLD
  two-step `encode`+`field-ingest --packed` path built **97/100** (`rc124`×2,
  `rc137`×1 on `nasa-pdf-0001`/`0002`/`0003`); the direct path **recovered all
  three**.
- **Build wall:** median **64 ms**, sum **221,408 ms**; slowest `nasa-pdf-0003`
  **87.5 s** (under the 180 s budget).
- **Peak RSS:** median **30.9 MiB**, max **2342 MiB** (`nasa-pdf-0001`, a
  409 MiB source ≈ **5.7×** source). **Memory, not wall, is the binding
  constraint** for sources ≳1 GiB.
- **Persistent store:** **2,829,898,049 B** = **1.036×** source; **2,691** files
  / **4,610** directories (regular-file bytes only; `du -sb` never used).
- **Exactness 100/100** (length + SHA-256).
- **Cold coverage:** text **92/100** (90.0%, pdf 54/60, docx 13/15, epub 25/25),
  metadata **98/100** (98.0%). All declines are **typed**: `rc 6`
  (unsupported-feature) for `nasa-pdf-eb-01/02/06/07/08/10` `text` (no page 1);
  `rc 20` `InvalidPackageStructure` for `nist-docx-0011/0012` (`text`+`metadata`)
  — which the old baseline also declined and which still `materialize --exact`
  byte-exactly.

**Comparison against the OLD path** (file-size-corrected; the `du -sb` figures in
the release-baseline and phase16-packed-full campaigns are deliberately **not**
compared). Common documents (new build ok ∧ old packed ingest ok): **96**.

| quantity | NEW `field-build --packed --sync=batch` | OLD `encode`+`field-ingest --packed` |
|---|---:|---:|
| build wall (median) | **61 ms** | 2,095 ms |
| build wall (sum) | 45,979 ms | 1,066,335 ms |
| persistent bytes (sum) | 1,830,073,797 B | 1,738,386,483 B |

- The direct build is far faster — **paired median ratio NEW/OLD 0.083×** over 96
  documents — but stores **+5.3%** bytes (1.830 vs 1.738 GB) because it **FIXES**
  the runtime/RAW program instead of **SEARCHING** for the smallest. Packed stores
  are byte-identical on **15/96** documents (all epub), i.e. where both paths pick
  the same program the writer reproduces the old bytes exactly.

**Scope note — the subset is not representative.** On the **full** population
VOLE's bytes are **~0.96×** the SQLite `real100` database, **NOT** the **0.53×**
of the 12-document contract subset. The contract subset understates SQLite's
footprint; the full-population ratio is the honest storage statement.

**Interpretation.** The direct, fixed-program build is **robust**: it admits and
byte-exactly reconstructs every document of the frozen population, including the
three large PDFs the searched path could not finish, and it does so in tens of
milliseconds. The price of skipping the candidate search is a small persistent
storage cost; the next wall is **memory**, not time, for ≳1 GiB sources. The
storage headline must be stated per population.

**Limitation.** One process per document (no worker concurrency); wall and RSS are
single-run and the bind-mounted store is noisy; peak RSS is bounded by the 6 GiB
container cap. No population claim beyond the frozen `real100-v1` corpus.

## Cross-cutting notes

- Every exactness statement in Phase 19 is the same invariant:
  `materialize(descriptor) == original_bytes` (length + SHA-256; `cmp` where a
  local source remains). 19.1/19.2 use it on the 12-document subset, 19.3 across
  the frozen population.
- **No wire byte, decode path, or `encode` output changed in Phase 19.** Phase 19
  is measurement and documentation; 19.3 re-measures the Phase-18 build path
  unchanged.
- **Estimator discipline is the phase's durable output:** a performance claim
  states its estimator and its interval (paired per-rep median vs ratio of sums;
  bootstrap median vs geometric mean incl./excl. 1.0). See
  [ADR-0054](../adr/0054-repeatability-and-paired-measurement.md).
- The Phase-18 **0.82×** build and **1.09×** warm headlines are **superseded as
  point estimates**, not deleted: 0.82× re-reads as ~0.96× under the ratio-of-sums
  estimator (with the build win still established under the paired estimator), and
  1.09× re-reads as ~1.29× (a modest, marginal loss) at N=100.
