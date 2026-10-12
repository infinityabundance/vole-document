# Phase 29 results — generalized fused execution, coarse structural index, incremental observation

Branch `phase29`. Phase 29 builds the three mechanisms the Phase-28 decision gates
justified, each with its own falsifiable court: the fused execution kernel
(**29.1**, generalizing 28.5), the coarse structural index (**29.2**, shipping
28.6's decision), and revision-aware incremental observation (**29.3**, shipping
28.7's decision). Same rule as every subphase: exactness is the invariant,
negatives are recorded, no claim without evidence. The frozen contract is
[phase-29-plan.md](phase-29-plan.md).

```text
29.1  Generalize fused execution — Arrow/Parquet/CSV/fixed-width, count/sum/min/max/avg/grouped
29.2  Coarse structural index    — PDF objects/pages, ZIP members, Parquet row groups
29.3  Incremental observation    — semantic dependency edges; localized-edit reuse
```

**Outcome in one line.** 29.1 is a **resolved steady-state win** over the shipped
path and every DuckDB cell, with the large-input pyarrow gap narrowed but **not
closed**; 29.2 **ships the coarse index** and is a **win on span requests**
(closure beats the whole-descriptor control 4/4) with a **recorded honest loss on
natural reading requests** (1/4); 29.3 is **correct and reusable** but
**MARGINAL/scoped** on index cost (27.3% of source, above the gate's 10% bar).
Phase 28's controls (28.6 fragment locality, 28.7 revision reuse) are retained.

## 29.1 — Generalized fused execution kernel

Experiment 1 (28.5) proved a bounded native fused kernel is a resolved end-to-end
win over the shipped path but covered **one format (Arrow) and one operator** and
**still lost to pyarrow on large inputs (6.46×)**. 29.1 generalizes it: **one
versioned logical plan** — projection + equality/`gt`/inclusive-range predicate +
`count`/`sum`/`min`/`max`/`avg`/`grouped` accumulator — over a **shared interface**
(`src/adapter/fused.rs`, `FUSED_PLAN_VERSION = 1`) with a **physical scanner per
format** (Arrow, Parquet, CSV/TSV, fixed-width). The accumulator is
**validity-aware** and consumes **native** `i64`/`f64` chunks plus an optional
validity mask; it never builds a `Value` enum, allocates per value, or renders
text. Only the **addressed column** is decoded. Exposed as the **`fused-scan`**
subcommand.

**Null semantics (the exact rule, SQL-like).** A `NULL` is never equal to, less
than, or greater than anything, so a predicate is **false** on a null row; every
aggregate **excludes nulls**; an aggregate over zero satisfying rows is SQL
`NULL`; a grouped count counts non-null rows per distinct integer value. This is
exactly the rule pyarrow and DuckDB apply, so the answers are directly comparable.

**Supported subset, per format** (everything else is a **typed decline**, never a
wrong answer):

| Format | Supported (addressed column) | Typed declines observed by the court |
|---|---|---|
| Arrow | fixed-width integer (`i64`/`i32`) and float (`f64`) columns, nullable (validity mask) | `utf8`, `decimal`, dictionary-encoded, `grouped` on a float column |
| Parquet | plain and dictionary-encoded integer columns; row-group **min/max/`null_count` statistics skip**; all-match `count` short-circuit | a `utf8`/string column, an unsupported codec, `grouped` on a float column |
| CSV / TSV | numeric columns with `--kind int` | a non-numeric field (`--kind int` on a name column) |
| Fixed-width | column-position integer fields with `--kind int` | a non-numeric field |

**Result (in-process steady-state; ratio is fused/comparator, < 1 favours fused;
ADR-0054 paired estimator, fixed-seed cluster bootstrap, every raw sample
retained).** VOLE exactness **12/12** (fixture × format) and oracle agreement
**228/228** (114 clusters; count/sum/min/max/avg/grouped across Arrow/Parquet/CSV).

| Comparator | clusters | median | geomean | 95% CI (geomean) | MDE | W/T/L |
|---|---:|---:|---:|---|---:|---|
| DuckDB | 114 | **0.0034** | 0.0068 | 0.0046..0.0103 | 0.0060 | **570/0/0** |
| current in-process `observe` path (count only) | 14 | **0.0341** | 0.0243 | 0.0138..0.0382 | 0.0072 | **70/0/0** |
| pyarrow | 114 | **0.0772** | 0.2626 | 0.1694..0.4130 | 0.2007 | 406/41/163 |

- The fused kernel **beats DuckDB in every cell** (median **0.0033–0.0034**,
  570 wins / 0 ties / 0 losses) and the **shipped in-process `observe` path** on
  `count` (median **0.034**, 70/0/0).
- Against **pyarrow** the pooled median is **0.078** (resolved faster), but the
  distribution is split: **406 wins / 41 ties / 163 losses** — pyarrow's
  vectorized C++ kernel wins the large inputs.
- **The large-input (`e_large`) Arrow gap narrowed from 6.46× (28.5) to ~2.7–3.0×**
  on the non-`avg` cells (e.g. `id:count` **2.76×**, `id:sum` **2.97×**,
  `score:count` **2.98×**); `id:avg` stays worst at **4.17×**, and `score:min`/
  `score:max` are **< 1** (the kernel wins). The gap is **improved, not closed**.
- **Parquet statistics-skip `count` is 5.4× faster than pyarrow** (`e_large`
  `parquet/id:count` **0.186**), because the kernel answers from row-group
  min/max/`null_count` without decoding the column.
- **CSV is 15–51× slower than pyarrow** on the large fixture (`e_large`
  `csv/*` ratios **15.2–52.2**): pyarrow's C++ CSV reader dominates; this is a
  recorded loss, not a capability gap.
- Cold (fresh process per question, folding field open + source materialization +
  reader start-up) is reported **separately**: fused/duckdb median **0.014**,
  fused/pyarrow median **0.016**.

Receipt: [`2026-10-12-phase29-1-fused-19d2c372`](../../evidence/campaigns/2026-10-12-phase29-1-fused-19d2c372/).
Correctness court: [`tools/phase29-1-1-fused-court.sh`](../../tools/phase29-1-1-fused-court.sh);
economic court: [`tools/phase29-1-fused-econ-court.sh`](../../tools/phase29-1-fused-econ-court.sh).
Control: Phase 28.5 [`…-fused-arrow-e41a78fe`](../../evidence/campaigns/2026-10-12-phase28-5-fused-arrow-e41a78fe/).

## 29.2 — Coarse structural index

Experiment 2 (28.6) measured the current closure for a narrow request as the
**whole encoded descriptor** and decided a **coarse** index (PDF objects/pages,
ZIP members, Parquet row groups/column chunks) — not per-record, not tiny fixed
fragments. 29.2 **ships that index**: `src/field/coarse.rs` and the
**`coarse-index`**/**`coarse-read`** subcommands. It records a partition of
`[0, source_len)` into **format-native addressable units** with byte ranges in the
authority object, and serves a narrow request from **real `pread` range reads** of
whole units. The authority is the descriptor's single verbatim `OBJECT` payload,
whose SHA-256 is **checked** against the declared source SHA-256 (never assumed);
a descriptor whose authority is not a verbatim single object, a missing/malformed
index, an unknown format, or an uncovered unit is a **typed decline** that the
caller turns into a full read — **never a wrong answer**. No seed node is added
(ADR-0060 unaffected), and `materialize --exact` does not touch the index.

**Three real defects were found and fixed** while reviewing the interrupted
implementation (each with the review recorded, not rewritten):

1. **The `--use-map` lane never worked** — the fetch refused `expand=false`.
   Split into `fetch` / `read_unit` so the prefetch lane reads its planned units.
2. **Unbounded Thrift-Compact recursion** → a stack overflow on a hostile Parquet
   footer. Added `MAX_THRIFT_DEPTH = 64` with a typed decline.
3. **An O(n²) PDF object scan** on a hostile all-object blob, now linear.

Two request shapes are measured (the index's native **span** granularity vs the
**natural reading** request), against the **whole-descriptor control**
(`observe-batch` cold; the Phase-22.5/28.6 "current VOLE" closure) and the real
page-level **SQLite** control. `closure` is the honest physical total (unit bytes
+ persisted index + manifest/index/seed reads).

| fmt | shape | source B | control B | units B | index B | closure B | page-SQLite B | closure/control | vs control | closure/SQLite | vs SQLite |
|---|---|---:|---:|---:|---:|---:|---:|---:|:--:|---:|:--:|
| pdf | span | 3,236,057 | 3,236,824 | 7,091 | 30,461 | 37,552 | 28,788 | **86.2×** | WIN | 0.77× | LOSS |
| pdf | reading | 3,236,057 | 3,245,913 | 7,087 | 30,461 | 46,545 | n/m | **69.7×** | WIN | n/a | n/m |
| docx | span | 3,284,935 | 3,285,766 | 655,411 | 218 | 655,629 | 675,956 | **5.01×** | WIN | 1.03× | WIN |
| docx | reading | 3,284,935 | 3,290,209 | 3,291,799 | 218 | 3,296,460 | n/m | 1.00× | LOSS | n/a | n/m |
| odt | span | 3,284,480 | 3,285,310 | 655,407 | 218 | 655,625 | 680,052 | **5.01×** | WIN | 1.04× | WIN |
| odt | reading | 3,284,480 | 3,289,667 | 3,291,064 | 218 | 3,295,639 | n/m | 1.00× | LOSS | n/a | n/m |
| parquet | span | 4,195,070 | 4,195,784 | 1,550,025 | 201 | 1,550,226 | 1,757,300 | **2.71×** | WIN | 1.13× | WIN |
| parquet | reading | 4,195,070 | 4,196,116 | 8,390,140 | 201 | 8,390,673 | n/m | 0.50× | LOSS | n/a | n/m |

- **Span requests: the closure is narrower than the whole-descriptor control in
  4/4 formats**, and the **source payload is cut to one native unit** — **PDF
  456×, DOCX/ODT 5×, Parquet 2.7×** smaller. The closure **beats page-SQLite in
  3/4** (on source bytes alone, ignoring the persistent directory, **4/4**); PDF
  loses the closure comparison only because its directory is heavy (**30,461 B**,
  ~34 B per object unit).
- **Reading requests win in only 1/4 formats (PDF).** Where the format model
  instead materializes the whole exact source (DOCX/ODT/Parquet), the coarse units
  are **built but inert** for those requests; Parquet's reading closure is **2.0×**
  its control (the model materializes the whole `DocumentExact` root). This is the
  **honest loss** the court reports rather than assuming the index narrows every
  request.

Exactness held **4/4 formats** (`materialize --exact == source`). Coarse-lane wall
is 11,522–13,885 µs and peak RSS 12,188–14,736 kB (one cold process). Receipt:
[`2026-10-12-phase29-2-coarse-index-1cda34b4`](../../evidence/campaigns/2026-10-12-phase29-2-coarse-index-1cda34b4/);
court: [`tools/phase29-2-coarse-index-court.sh`](../../tools/phase29-2-coarse-index-court.sh).
Historical control: Phase 28.6
[`…-fragment-locality-dad05faa`](../../evidence/campaigns/2026-10-12-phase28-6-fragment-locality-dad05faa/).

## 29.3 — Revision-aware incremental observation

Experiment 3 (28.7) **simulated** cross-revision dependency reuse and named the
load-bearing fact: on authentic revision families many invalidations have
**byte-unchanged physical fragments**, so a physical-only content-addressed store
would reuse them **wrong**; only **semantic edges** (a DOCX style/numbering table,
an XLSX shared-string table) make reuse correct. 29.3 **ships the bounded
mechanism** — `src/field/incremental.rs` and the **`field-incremental`**
subcommand: a **versioned durable index** (`VOLEINC1`, 20 B header + 128 B/entry)
mapping a stable observation identity to its verified dependency signature
(**physical** fragment content ids **plus** **semantic** context content ids) and
the content id of its verified derived output. Three lanes: **full** (rebuild
everything), **cas** (a physical-only versioned CAS control), and **incremental**
(reuse only where physical **and** semantic signatures match). The four ADR-0060
namespaces — exact revision SHA-256, physical fragment id, semantic context id,
derived-output id — stay **distinct**; anything outside the subset declines typed.

| family | fmt | change | revs | reused/obs (rev>1) | reuse% | semantic-only invalid | incr == full | CAS diverges |
|---|---|---|---|---:|---:|---:|:--:|:--:|
| nist-sp800-34 | docx | real-draft-edit | 4 | 0/1155 | 0.0% | 0 | yes | no |
| docx-style | docx | style-table | 3 | 0/1042 | 0.0% | 1042 | yes | **yes** |
| xlsx-ss | xlsx | shared-string | 3 | 8/32 | 25.0% | 24 | yes | **yes** |
| csv-text | csv | text-edit | 3 | 23254/26643 | **87.3%** | 0 | yes | no |
| json-text | json | text-edit | 3 | 6895/11643 | **59.2%** | 0 | yes | no |

- **Correctness holds**: the incremental lane's answers **equal a full rebuild on
  every revision** (`answer_digest` match), *including* the DOCX style and XLSX
  shared-string cases — where the **physical-only CAS reuses 1042 DOCX paragraphs /
  24 XLSX cells and diverges wrong** (`cas diverges = yes`). This is the 28.7 fact
  reproduced by a **real mechanism**: the semantic edges are **load-bearing**
  (**1,066** semantic-only invalidations with unchanged physical bytes).
- **Reuse: 87.3% (CSV) / 59.2% (JSON)** on localized text edits; **0%** on the
  real whole-document draft chain (`nist-sp800-34`) — reuse is scoped to
  **localized edits**, exactly as 28.7 predicted.
- **Index cost: 745,108 B = 27.3% of the 2,730,316 B of source.** This is **above
  the Phase-28.7 gate's ≤10% bar** (28.7 hit 9.13% only under its *simulated*
  minimal index; the shipped index — even with the **per-observation identity node
  dropped** to hold it down — is heavier). Verdict: **MARGINAL/scoped**.

Exactness **16/16 revisions** (`materialize --exact == source`, fresh process,
after source **and** descriptor deletion). Receipt:
[`2026-10-12-phase29-3-incremental-879e6334`](../../evidence/campaigns/2026-10-12-phase29-3-incremental-879e6334/);
court: [`tools/phase29-3-incremental-court.sh`](../../tools/phase29-3-incremental-court.sh).
Historical control: Phase 28.7
[`…-revision-reuse-b06a2f76`](../../evidence/campaigns/2026-10-12-phase28-7-revision-reuse-b06a2f76/).

## Verdict

- **29.1 — a resolved steady-state win, a recorded large-input loss.** One shared
  versioned interface, four format scanners, six operators, validity-aware. Beats
  **DuckDB every cell** (median 0.0033, 570/0/0) and the shipped path (70/0, median
  0.034); vs **pyarrow** median 0.078 but split (406/41/163). The large-input Arrow
  gap narrowed **6.46× → ~2.7–3.0×**; Parquet stats-skip `count` is **5.4× faster
  than pyarrow**; **CSV is 15–51× slower**. Exactness 12/12, oracle agreement
  228/228. Every out-of-subset column declines **typed**.
- **29.2 — ships the coarse index; a win on span, an honest loss on reading.**
  Three real defects fixed. Span requests narrow the source payload to one unit
  (**PDF 456×, DOCX/ODT 5×, Parquet 2.7×**), the closure beats the control **4/4**
  and page-SQLite **3/4**; the natural **reading** request narrows in only **1/4**
  formats and the PDF directory is heavy (**30 KB**). Exactness 4/4.
- **29.3 — correct and reusable, but marginal/scoped.** Incremental answers equal
  a full rebuild on **every** revision, including the style/shared-string cases
  where the physical-only CAS **diverges wrong**. Reuse **87.3% CSV / 59.2% JSON**,
  **0%** on the real whole-document draft chain; index **27.3%** of source (**above
  the gate's 10% bar**) → **MARGINAL/scoped**. Exactness 16/16.

Each mechanism keeps its Phase-28 control, its negatives, and its typed declines.
