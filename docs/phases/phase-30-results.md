# Phase 30 results — close the measured gaps (large-input kernels, fused CSV, incremental index, coarse reading; standing negatives shipped)

Branch `phase30`. Phase 30 attacks, in order, the four gaps Phase 29's own report
named. The frozen contract is [phase-30-plan.md](phase-30-plan.md). Same rule as
every subphase: exactness is the invariant, negatives are recorded, no claim
without evidence.

```text
30.1  Large-input fused kernels   — SHIPPED (this document)
30.2  Fused CSV past pyarrow      — SHIPPED (this document)
30.3  Incremental index cost      — SHIPPED (this document)
30.4  Coarse index for reading    — SHIPPED (this document)
30.5  Standing negatives          — SHIPPED (this document)
```

**Outcome in one line.** 30.1 turned the recorded Phase-29.1 large-input loss into
a win: the fused kernel now beats **pyarrow on every large Arrow and Parquet cell**
(Arrow **≈2.87 → 0.65**, Parquet **≈1.28 → 0.61**, ratios `< 1` favour fused) and
cut the large CSV loss **≈46.2 → 35.5** (improved ≈23 %, but still **10.5–42×
slower**). On the full econ matrix the pooled pyarrow median improved **0.0772 →
0.0461** (W/T/L **406/41/163 → 489/8/81**), exactness held, and the Arrow/Parquet
gain is a **code change**, not a comparator change. The residual CSV loss was then
attacked directly: **30.2** pushes the fused CSV kernel **past pyarrow on equal
work** (paired median **0.362**, 95 % CI [0.312, 0.392], ≈2.8×, 49/0/1; the commit
re-run read 0.342), **30.3** shrinks the incremental
observation index to the gate's **≤10 %** bar (**27.29 % → 7.31 %**, scoped and
honest), and **30.4** turns the coarse index into a real **reading**-request win in
**3/4** formats (**1/4** in Phase 29.2). **30.5 (standing negatives) shipped** —
the five **`asciidoc → rst`** detection findings fixed by a narrow, reST-safe
predicate (findings **15 → 10**, agreements **274 → 279**, always **308/308
byte-exact**; the remaining **10** recorded with reasons), the PPT `recInstance ≥ 4`
"other" lists now named/selectable, and the **legacy (pre-0.15) Arrow
no-continuation stream framing fixed end-to-end**.

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
  fixture; this is a **recorded loss, not a capability gap**. (The pyarrow CSV lane
  here is **aggregation-only** over an already-parsed `Table`; **30.2** re-measures
  it **read-inclusively** — see below.)
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

## 30.2 — Fused CSV past pyarrow (read-inclusive)

Phase 30.1 closed the large-input gap for Arrow and Parquet but left the `e_large`
CSV lane a **recorded loss**. Reading the 30.1 harness showed *why the number was
not apples-to-apples*: the pyarrow **CSV** lane timed only the in-memory
aggregation (`pyarrow.compute` over an already-parsed `Table`) — `pyarrow.csv.read_csv`
sat outside the timed lambda. That is fair for Arrow and Parquet (a zero-copy
columnar scan) but for CSV it compares VOLE's **parse + aggregate** against
pyarrow's **aggregate only**. 30.2 does two things:

1. It **pushes the fused CSV kernel past pyarrow** on the same work. The scalar
   SWAR field scan becomes the SIMD `memchr3` search (SWAR fallback when
   `memmem-scan` is off), and a **bounded, deterministic, record-aligned
   multi-threaded** delimited scan ships behind `--workers N` (feature `parallel`):
   - **Record-aligned split.** The chunk boundaries are a **fixed function of the
     input length** — **not** the thread count — so the partition (and every
     partial accumulator) is deterministic; partial `Accumulator`s are merged
     **left-to-right**, and declines propagate from the **earliest** chunk, so
     **parallel and serial agree on errors** as well as answers.
   - **Conservative serial fallback.** Any quote character, a bare-CR dialect, a
     source `< 64 KiB`, or a `grouped` request stays on the serial scanner.
   - **Off the exactness path.** The versioned plan (`FUSED_PLAN_VERSION = 1`),
     validity/SQL-null semantics, and typed declines are unchanged; the kernel
     still reads **only the addressed column**. Four new tests.
2. It measures the honest, apples-to-apples comparator: pyarrow
   `read_csv(bytes) + aggregate` and DuckDB `read_csv_auto` (**read-inclusive**),
   while still reporting the Phase-30.1 aggregation-only number for continuity.

**Result (in-process steady-state; ratio `fused / comparator`, `< 1` favours
fused).** Estimator ADR-0054 paired per-question ratio, fixed-seed cluster
bootstrap (B = 20000) 95 % CI, MDE; every raw sample retained. Court
[`tools/phase30-2-csv-court.sh`](../../tools/phase30-2-csv-court.sh); receipt
[`2026-10-13-phase30-2-csv-8ed3e80b`](../../evidence/campaigns/2026-10-13-phase30-2-csv-8ed3e80b/);
control Phase 30.1
[`2026-10-12-phase30-1-kernels-2ec78328`](../../evidence/campaigns/2026-10-12-phase30-1-kernels-2ec78328/).

| comparison (large CSV, `e_large`) | clusters | samples | median | median 95 % CI | geomean | MDE | W/T/L |
|---|---:|---:|---:|---|---:|---:|---:|
| pyarrow **read-inclusive** (parse + aggregate) | 10 | 50 | **0.362** | [0.312, 0.392] | 0.360 | 0.058 | **49/0/1** |
| DuckDB read-inclusive | 10 | 50 | **0.033** | [0.032, 0.034] | 0.035 | 0.001 | 50/0/0 |
| pyarrow **aggregation-only** (30.1 continuity) | 10 | 50 | **9.710** | [5.300, 12.104] | 9.028 | 4.863 | 0/0/50 |
| DuckDB aggregation-only | 10 | 50 | 0.835 | [0.805, 0.913] | 0.856 | 0.078 | 38/15/12 |

**The headline: the fused CSV kernel beats pyarrow on the SAME work** — fused /
pyarrow(**read-inclusive**) median **0.362**, 95 % CI **[0.312, 0.392]** (**≈2.8×
faster**, **49/0/1**) → **PASS**. Against DuckDB's read-inclusive lane it is
**0.033** (≈30×, **50/0/0**).

**Receipt note (which number is canonical).** The *sealed* paired-bootstrap receipt
[`…-8ed3e80b`](../../evidence/campaigns/2026-10-13-phase30-2-csv-8ed3e80b/) is
**canonical**: median **0.362** (95 % CI **[0.312, 0.392]**, W/T/L **49/0/1**),
serial **0.805** [0.742, 0.914]. The shipped commit message quotes an earlier
re-run at **0.342 / 50-50 / serial 1.012**; that figure is recorded here as the
**commit** number, not the receipt. Either way the read-inclusive fused/pyarrow
ratio is **well below 1.0**, and the **parallel split** is the margin.

**The honest caution (do not read the headline against the 30.1 number).** The
Phase-30.1 CSV pyarrow lane timed **only aggregation** over an already-parsed
`Table`, and its median was **35.5**. Against that aggregation-only lane the kernel
is **still ~9× slower** (the 30.1 → 30.2 continuity reading is **35.5 → 9.0** on the
geometric mean; the sealed aggregation-only median is **9.710**). **Serial
continuity:** with `--workers 1` the fused scanner against pyarrow read-inclusive is
**0.805** [0.742, 0.914] in the sealed receipt (the commit re-run read **1.012**) —
the **parallel split is what crosses the line**; a single thread is at
parity/slightly behind.

**Cold (fresh-process wall, ns).** fused median **3,088,644**; pyarrow
**76,271,908**; DuckDB **206,848,406**.

**Correctness and exactness.** `materialize --exact == source` **3/3** (fixture),
after the source and descriptor are deleted in a fresh process. Oracle agreement
**52/52** (pyarrow and DuckDB on the same logical data, including empty-field SQL
NULLs and a `grouped` question). No cell changed, only its speed.

## 30.3 — Incremental observation index (VOLEINC2)

Phase 29.3 verified incremental observation **correct** but recorded its index at
**27.3 %** of source — **above** the Phase-28.7 gate's ≤10 % bar. 30.3 ships
**VOLEINC2**, a versioned index that reaches the bar (on the sealed court metric)
while keeping the correctness 29.3 proved:

- **Format.** `VOLEINC2` = a 44-byte header + **interned** signature table + varint
  references + a **32-byte output content id** per entry.
- **Entry model.** A **positional slot** (identity = the enumerated key's ordinal,
  which is **not** persisted) + an interned **physical** signature ref (varint) + an
  interned **semantic** signature ref (varint) + the output content id.
- **Identity scope.** `BLAKE3(format)[..16]` in the header; a scope/version mismatch
  is "no prior index", **never** reuse.
- **Identity separation (ADR-0060).** The exact revision SHA-256, the physical
  fragment id, the semantic context id, and the derived-observation output id stay
  **distinct**; only the *persisted* identity node is dropped.

**Result (sealed court metric).** The metric is the **controlling (last) family's
index over the sum of ALL families' source bytes** — the Phase-28.7 gate's ≤10 %
bar. Court [`tools/phase30-3-incremental-court.sh`](../../tools/phase30-3-incremental-court.sh);
receipt [`2026-10-13-phase30-3-incremental-f3765c2b`](../../evidence/campaigns/2026-10-13-phase30-3-incremental-f3765c2b/)
(control Phase 29.3 [`…-879e6334`](../../evidence/campaigns/2026-10-12-phase29-3-incremental-879e6334/),
`VOLEINC1`).

| metric | 29.3 (`VOLEINC1`) | 30.3 (`VOLEINC2`) | gate |
|---|---:|---:|---|
| sealed index cost | **27.29 %** | **7.31 %** | **≤10 % — OK** |
| index bytes / source | 745,108 / 2,730,316 | 199,622 / 2,730,316 | |
| reuse (rev > 1) | 49.6 % | **49.6 %** (Δ +0.0 pp) | unchanged |
| incremental == full rebuild | holds | **holds** | required |

- **Correctness holds.** Incremental answers **equal a full rebuild on every
  revision** (answer digest); the physical-only CAS **still diverges wrong** on a
  style / shared-string case, so the semantic edges are load-bearing (**1,066**
  semantic-only invalidations). Exactness **16/16** revisions.
- **Honest caveats (both recorded).**
  1. The **honest whole-set sum** — the index summed over *all* families
     (677,716 B) over the same source — is **24.82 %**, still **> 10 %**. A
     per-observation output content id is **32 B** and a CSV row averages ~**30 B**,
     so the CSV family's **own** index/source ratio is **38.48 %** by construction;
     the sealed metric (dominated by the last family) is what the gate named.
  2. **Whole-document-redraft reuse is 0 %** (the real `nist-sp800-34` DOCX draft
     chain) — the reuse win is on **localized** edits by construction.
- **Persistent bytes** (index + content-addressed outputs) **582,031**; bytes
  recomputed full **2,540,635** / cas **1,337,213** / incremental **1,408,687**.

**Verdict: RESOLVED (positive, scoped).**

## 30.4 — Coarse unit-addressed reading requests

Phase 29.2 shipped the coarse structural index but recorded that the natural
**reading** request (page-1 / block-0 / parquet-cell text) narrowed for **PDF only
(1/4)** — the DOCX/ODT/Parquet format-model nodes materialized the whole exact
`DocumentExact` source, so their coarse units were built but **inert**. 30.4 serves
the reading request from the **coarse member spans** (the ZIP member / column chunk)
**plus the ZIP central directory**, so its closure is **unit + directory**, not the
whole document. The closures are real `pread` bytes (units + directory + manifest /
index / seed); corpus and reconstruction are unchanged from 29.2, so 29.2's numbers
are the historical control. Court
[`tools/phase30-4-coarse-reading-court.sh`](../../tools/phase30-4-coarse-reading-court.sh);
receipt [`2026-10-13-phase30-4-coarse-reading-6764974f`](../../evidence/campaigns/2026-10-13-phase30-4-coarse-reading-6764974f/).

| fmt | shape | control B | closure B | page-SQLite B | closure/control | vs control | closure/SQLite | exact |
|---|---|---:|---:|---:|---:|:--:|---:|:--:|
| pdf | reading | 3,245,913 | 46,545 | 24,692 | 69.74× | WIN | 0.53× | yes |
| docx | reading | 3,290,209 | **13,003** | 20,596 | **253.03×** | WIN | 1.58× | yes |
| odt | reading | 3,289,667 | **12,456** | 20,596 | **264.10×** | WIN | 1.65× | yes |
| parquet | reading | 4,196,116 | 8,390,673 | 4,685,956 | **0.50×** | **LOSS** | 0.56× | yes |

- **Reading requests now narrow in 3/4 formats** (Phase 29.2: **1/4**). The DOCX and
  ODT reading closures fall from **~3.29 MB** to **13.0 / 12.5 KB** (**~253× /
  ~264×**), beating the page-SQLite control's **20.6 KB**.
- **Span requests** remain narrower than the control in **4/4** formats.
- **Exactness held: 4/4** formats `materialize --exact == source`. **ADR-0060
  untouched** (no seed node added).

**Remaining loss (recorded).** The **Parquet reading** closure is still **0.50×** its
control (i.e. ~2.0× the bytes). The `ParquetCell` projection slices the containing
column chunk out of a **whole-source buffer** (`parquet_source()` materializes the
`DocumentExact` root) and the `ParquetModel` node reads the whole source; making the
cell **unit-addressed** needs a **range source threaded through the Parquet value
decoders**, which this subphase does not do. A footer-only model would not flip it
on its own (the whole-source read in `parquet_source()` remains).

**Directory overhead.** The persisted `VCI1` index (the directory the read pays) is
**30,461 B** for PDF (~34 B/object unit), **218 B** for DOCX/ODT (8 members +
directory) and **201 B** for Parquet; it is charged once per observation in
`closure`.

## 30.5 — Standing negatives (shipped)

30.5 attacks the standing negatives Phase 30 recorded. **Exactness is unchanged**:
every sample still closes byte-exactly (`materialize == source`).

**Detection — findings 15 → 10, agreements 274 → 279, always 308/308 byte-exact.**
The stratified real-world population **v2** court was re-run and re-sealed
(`2026-10-13-realformats-stratified-v2-889b8952`). Five **`asciidoc → rst`**
misclassifications are **fixed** by a narrow, conservative predicate: a new
`asciidoc::has_asciidoc_only_mark` (`src/adapter/asciidoc.rs`) admits an
**AsciiDoc-only** mark reST cannot produce — a column-0 preprocessor/conditional
directive (`ifdef::`/`ifndef::`/`ifeval::`/`endif::`/`include::`) with a bracketed
tail, or a `[#anchor]` block anchor — and `rst::looks_like_other`
(`src/adapter/rst.rs`) now **declines** such a source, so it falls through to the
AsciiDoc detector. There is **no detector reorder** and no change to a genuine reST
field list (regression test
`asciidoc::tests::detects_asciidoc_only_marks_and_declines_their_negations`). The
**remaining 10 findings** stay recorded with reasons:

| finding | reason |
|---|---|
| `cbor-data-integer.cbor` | a lone 2-byte scalar (`18 2a`) below the deliberate structural bar |
| `cbor-rfc8949-good.cbor` | a well-formed map nesting **512** levels, above the deliberate `max_cbor_depth` DoS cap (256) |
| `poi-document-Bug60942b.doc` | a **Word 6.0** FIB (`nFib` 0x0065) outside the Word 97–2003 subset — stays at the CFBF layer |
| `cpython-msg-25.eml` | an **mbox** whose `multipart/report` boundary is folded and truncated |
| `loghub-HPC-HPC_2k.log` | a record with **no timestamp and no level** — not a log-stream line |
| `loghub-OpenStack-OpenStack_2k.log` | claimed by the load-bearing `config` detector (precedence; not reordered) |
| `loghub-Windows-Windows_2k.log` | claimed by the CSV detector (precedence; not reordered) |
| `poi-Divino.pptx` | **truncated** — no EOCD / central directory, so not a valid OPC package |
| `cpython-whatsnew-3.3.rst` | **reST → Markdown** precedence (the over+underlined title is a Markdown thematic break) |
| `cpython-workflows-project-updater.yml` | **YAML → config** precedence (a `${{ … }}` expression is outside the bounded YAML subset) |

**PPT `recInstance ≥ 4` "other" lists are now named and selectable.** The legacy PPT
list arena (`--ppt-list`) names every `SlideListWithText` container unambiguously and
selects the non-conformant `other:<recInstance>` lists (`recInstance >= 4`, which
MS-PPT does not define) it previously left unaddressable —
`ppt::slide_list_label` / `ppt::slide_list_from_name` round-trip (`src/adapter/ppt.rs`;
tests in `tests/ppt_adapter.rs`); the documented `slides`/`masters`/`notes`/`handout`
projection (recInstance 0–3) is unchanged.

**Arrow legacy (pre-0.15) no-continuation stream framing is fixed end-to-end.** The
bare legacy framing — a 4-byte little-endian metadata length with **no `0xFFFFFFFF`
continuation marker**, a first `Schema` message, and a 4-byte zero end-of-stream — is
now claimed by the **detector** *and* accepted by the bounded **parser**
(`src/adapter/arrow.rs`), conservatively gated on all-zero metadata-to-body padding
and an end-of-stream marker that is the **exact end of the source**; near-misses
still decline (regression tests
`arrow_adapter::legacy_stream_framing_is_detected_and_built`,
`arrow_adapter::legacy_stream_near_misses_decline`).

**Legacy subset edges** (typed unclaimed surfaces, not defects): the DOC
story→section mapping, textbox anchors, and piece-local `doc-find` remain out of the
bounded subset — recorded in the negatives ledger
([roadmap](../project/roadmap.md), [status](../project/status.md)).

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
- **30.2 — SHIPPED: the CSV loss is closed on equal work.** With a record-aligned,
  bounded parallel scan and a SIMD field search, the fused kernel beats pyarrow on
  the **same work** (parse + aggregate): paired median **0.362** [0.312, 0.392]
  (**≈2.8×**, **49/0/1**, receipt-canonical; the commit re-run read **0.342**,
  50/50); vs DuckDB **0.033**. **The honest caution stands:** against the Phase-30.1
  *aggregation-only* pyarrow lane the kernel is still **~9× slower** (**35.5 →
  9.0**), and the **serial** scanner (`--workers 1`) against pyarrow read-inclusive
  is **0.805** (the commit re-run read 1.012) — the parallel split is the margin.
  Exactness **3/3**; oracle **52/52**.
- **30.3 — SHIPPED: the incremental index reaches the ≤10 % bar, scoped and honest.**
  `VOLEINC2` takes the sealed court metric **27.29 % → 7.31 %** (≤10 %), reuse
  **49.6 %** unchanged, incremental == full rebuild **holds** (the physical-only CAS
  still diverges wrong), exactness **16/16**. **Honest caveats:** the whole-set sum is
  **24.82 %** (still >10 %; a 32 B output id cannot be a small fraction of a ~30 B CSV
  row) and whole-document-redraft reuse is **0 %**.
- **30.4 — SHIPPED: the coarse index now serves reading requests.** Reading requests
  narrow in **3/4** formats (from 1/4): DOCX/ODT closures **~3.29 MB → 13.0 / 12.5 KB**
  (**~253× / ~264×**), beating page-SQLite's **20.6 KB**; PDF unchanged; **Parquet
  reading is still 0.50×** (needs a range source through the Parquet decoders —
  recorded). Exactness **4/4**; ADR-0060 untouched.
- **30.5 — SHIPPED: the standing negatives are attacked and the phase is closed.**
  Detection findings **15 → 10** (agreements **274 → 279**, always **308/308
  byte-exact**): the five `asciidoc → rst` fixes via the narrow
  `asciidoc::has_asciidoc_only_mark` predicate + `rst::looks_like_other`, with no
  detector reorder; the **remaining 10 recorded with reasons** (CBOR lone scalar /
  512-level depth; Word 6.0 FIB; mbox `.eml`; HPC/OpenStack/Windows log precedence;
  truncated `.pptx`; reST→Markdown / YAML→config precedence). The PPT
  `recInstance ≥ 4` "other" lists are now named/selectable (`other:<recInstance>`),
  and the **legacy (pre-0.15) Arrow no-continuation stream framing is fixed
  end-to-end** (detector + bounded parser). Exactness unchanged (**308/308**);
  `--all-features` **1574 tests / 0 failures**, default-feature **562 pass / 0 fail**.

**Framing (recorded plainly).** VOLE is **still not a faster general-purpose
engine**. What Phase 29–30 add is a set of **court-verified, scoped wins**: against
pyarrow and DuckDB on **large Arrow/Parquet scans** (30.1 — every large Arrow and
Parquet cell `< 1.0`), on **the predicate-count question** (28.5/29.1 — resolved
over DuckDB, median ~0.0033), and on **equal-work CSV** (30.2 — read-inclusive
parse + aggregate). These do **not** generalize to a broad speed claim: the
recorded losses stand beside them (the large-fixture pyarrow loss in 29.1, the
aggregation-only CSV lane in 30.2, and the Parquet reading request in 30.4).
