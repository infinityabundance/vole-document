# Phase 28 results — execution, physical selectivity, and revision reuse

Branch `phase28`. Phase 28 is the **performance programme**: it finishes detection,
repairs the competitor envelope, and runs the **three decision experiments** that
decide whether the larger architectural direction is justified — before any
expensive rewrite. Same rule as every subphase: exactness is the invariant,
negatives are recorded, no claim without evidence. The frozen contract is
[phase-28-plan.md](phase-28-plan.md).

```text
28.1  Detection completion      — CFBF fragmented root-mini-stream + logstream/asciidoc/cbor widening
28.2  Competitor envelope       — real Arrow for DuckDB; in-process steady-state; cold split
28.3  Hot-path profiling        — paired ablation of the serve_source_range clone
28.4  Remove proven waste       — (ships nothing: 28.3 resolved NEGATIVE)
28.5  EXPERIMENT 1 Fused kernel — native Arrow predicate/aggregate vs pyarrow/DuckDB
28.6  EXPERIMENT 2 Fragment locality — exact-authority fragment directory vs the closure
28.7  EXPERIMENT 3 Revision reuse — fragment-level dependency graph on revision families
```

**Outcome in one line.** 28.1 and 28.2 are **shipped/repair** work; 28.3 is a
**recorded NEGATIVE** (so **28.4 ships nothing**); 28.5 is **RESOLVED POSITIVE**,
28.6 is a **decision** (coarse structural index justified, per-record not), and
28.7 is **positive but marginal/scoped**. The three gates set the order of the
next programme, **28.8+ (the winning mechanisms)**.

## 28.1 — Detection completion (findings 21 → 15)

The stratified real-world population **v2 court** (`tools/realformats-stratified-v2-court.sh`)
was re-run and re-sealed: findings **21 → 15** (6 fixed), detection agreements
**268 → 274**, and every sample still **308/308 byte-exact** (0 skipped/blocked).
The trajectory is now **32 (26.5) → 25 (27.3) → 21 (27.5) → 15 (28.1)**.

The six conversions, each with a focused regression test:

| Sample(s) | Was | Now | Fix |
|---|---|---|---|
| `poi-document-testRangeDelete.doc` | `cfbf` | `doc` | `cfbf::stream_leading` now resolves a mini sector's absolute position through the **root mini stream's FAT chain** (`CfbfModel::mini_chain`, model v2), so a `.doc` whose `WordDocument` mini-stream **and** root mini stream are both fragmented still detects — `legacy_cfbf_detection::fragmented_mini_stream_in_fragmented_root_stream_is_detected` |
| `loghub-Mac-Mac_2k.log` | `opaque` | `logstream` | new **BSD-without-`<PRI>`** dialect (`Mmm dd hh:mm:ss host tag…: msg`) |
| Spark log sample | `opaque` | `logstream` | generic grammar accepts a **`YY/MM/DD HH:MM:SS`** slash timestamp |
| Proxifier log sample | `opaque` | `logstream` | generic grammar accepts a bracketed **`[MM.DD HH:MM:SS]`** timestamp — `logstream::tests::detects_the_widened_dialects`, `…::the_widened_grammar_still_declines_prose` |
| `asciidoctor-cli-nav.adoc` (an `xref:`-macro-only list) | `opaque` | `asciidoc` | AsciiDoc claims an **`xref:`-macro-only navigation list** — `asciidoc::tests::detects_a_cross_reference_list_and_declines_its_negations` |
| `cbor-data-tagged_date.cbor` (a top-level tag) | `opaque` | `cbor` | CBOR claims a **top-level tag at two nodes** — `cbor::tests::detects_a_tagged_scalar_and_rejects_a_bare_scalar` |

The remaining **15 findings** (5 `decline-opaque`, 10 `misclassified`) are
**recorded precedence/decline boundaries** with reasons, not failures: the reST
field-list precedence over spaced AsciiDoc attributes (5), the lone-scalar and
256-level-depth CBOR declines, the Word 6.0 FIB (`wIdent 0xA5DC`), the aborted
mbox/`multipart/report` EML, the no-timestamp/no-level HPC log record, the
config/CSV precedent claims over two log streams, the truncated `poi-Divino.pptx`,
the Markdown-over-reST claim on one `.rst`, and the YAML-declined workflow claimed
by config. Detection stays conservative (≥2 non-blank lines, **every** non-blank
line must match); no load-bearing precedence was reordered.

Receipt: [`2026-10-12-realformats-stratified-v2-941e3d0f`](../../evidence/campaigns/2026-10-12-realformats-stratified-v2-941e3d0f/).
Per-format boundaries: [logstream](../formats/logstream.md), [AsciiDoc](../formats/asciidoc.md),
[CBOR](../formats/cbor.md), [DOC](../formats/doc.md).

## 28.2 — Competitor envelope (repaired and maximized)

The analytical economic courts were rebuilt around an **equal-contract envelope**
and re-measured with an **in-process steady-state** headline, reported separately
from **cold-process** and **cold-cache**.

- **DuckDB now reads the real Arrow data** — through the Arrow C data interface
  (`con.register` of the `pyarrow.ipc` table), **not** a Parquet projection. This
  removes the historical caveat that the pinned wheel had no Arrow IPC file
  reader; the Arrow comparator is now the genuine article.
- **pyarrow** reads the Arrow IPC file directly with column projection and a
  persistent reader; **SQLite** gets typed columns, a covering predicate index, and
  a tuned FTS5 external-content **trigram** index for the substring question.
- The **headline is steady-state** (in-process warm connection/state; per-question
  median); **cold-process** (fresh OS process per question, Python interpreter
  start-up charged to the Python lanes) and **cold-cache** (fresh connection within
  a warm interpreter) are reported separately. The container cannot drop the OS
  page cache, so `cold-cache` differs from `cold-process` only by the resident
  interpreter — recorded, not hidden.

**The honest result: VOLE loses every steady-state analytical axis.**

| Court | VOLE / comparator (steady median) | Notes |
|---|---|---|
| Arrow vs **DuckDB** | **2.361×** (≈2.25× on the receipt's envelope summary) | DuckDB over the real Arrow data |
| Arrow vs **pyarrow** | **27.372×** (≈29.1× on the envelope summary) | pyarrow on the `.arrow` file directly |
| Parquet vs **DuckDB** | **1.462×** (≈1.47×) | DuckDB with Parquet zonemap/statistics pruning |

VOLE **wins build** (paired medians **0.425×** SQLite / **0.472×** DuckDB /
**0.540×** pyarrow on Arrow; **0.483×** SQLite / **0.662×** DuckDB on Parquet)
and **wins storage vs SQLite** (**0.358×** Arrow, **0.178×** Parquet) but
**loses storage** to the analytical engines (Arrow **2.148×** DuckDB / **2.156×**
pyarrow; Parquet **5.968×** DuckDB). Only **Q6 (exact closure)** is a
byte-authority claim, and only VOLE and the source-retaining SQLite lane can make
it; DuckDB and pyarrow decline it typed (`not-native`), and both decline the exact
buffer/span question (Q3, `not-retained`) that VOLE answers from archival
metadata. VOLE exactness **3/3** (Arrow) and **7/7** (Parquet).

This is exactly the envelope Phase 28.5's Experiment 1 tests against: the shipped
VOLE steady-state was **2.25× slower than DuckDB and 29.1× slower than pyarrow**
on the six-question Arrow envelope.

Receipts: [`2026-10-12-phase28-2-arrow-econ-f7e0a961`](../../evidence/campaigns/2026-10-12-phase28-2-arrow-econ-f7e0a961/),
[`2026-10-12-phase28-2-parquet-econ-f7e0a961`](../../evidence/campaigns/2026-10-12-phase28-2-parquet-econ-f7e0a961/).
Historical controls (pre-28.2): [`phase21-16-arrow-econ-efb51887`](../../evidence/campaigns/2026-10-12-phase21-16-arrow-econ-efb51887/),
[`phase21-14-parquet-econ-bfe32a33`](../../evidence/campaigns/2026-10-10-phase21-14-parquet-econ-bfe32a33/).

## 28.3 — Hot-path profiling gate (recorded NEGATIVE)

A **measurement-only** paired ablation of the `serve_source_range` clone: the
selection-before-materialization variant is compiled only under the non-default
`phase28-3-ablation` feature and enabled by `VOLE_SERVE_SELECTIVE`; the derived
cache is off; the current and selective variants run in the **same binary** on 6
real documents (PDF/DOCX/EPUB) over 168 paired samples.

- **Selection-before-materialization is a no-op on this corpus.** The object table
  is a single always-referenced whole-source inline object, so the selective variant
  copies the **same bytes** (`clone_bytes == referenced_bytes` for every
  `(doc, obs)`); there is no observation where the window references fewer bytes
  than the whole object.
- End-to-end paired effect (selective/current): pooled **median 0.9999** (95% CI
  **0.9982 .. 1.0030**), geometric mean **1.0040** (95% CI **0.9925 .. 1.0145**);
  **MDE** median-CI **0.0035**, geomean-CI **0.0157**.
- The **observation ceiling for ANY clone-removal** — the share of wall spent in
  the whole `serve_source_range` call (clone + window eval + channel decode) — is a
  median **0.09%** across `(doc, obs)`; the clone alone is a fraction of that, far
  below the MDE. The dominant cost is the **derived-model decode** (observe ms ≈
  materialize ms), not the clone.

**Verdict: NEGATIVE — no resolved end-to-end improvement.** **28.4 ships
nothing**: neither the selection-before-materialization rewrite (it copies the same
bytes) nor a clone-win claim. Any remaining clone cost is a zero-copy-eval
question a selective rewrite does not address. Exactness 21/21 pairs, 0
mismatches; the referenced-bytes control is identical across variants.

Receipt: [`2026-10-12-phase28-3-profile-af713225`](../../evidence/campaigns/2026-10-12-phase28-3-profile-af713225/).

## 28.5 — Experiment 1: fused Arrow predicate/aggregate kernel (RESOLVED POSITIVE)

A bounded native kernel reads the **substrate Arrow bytes** (from the field
authority, never the source file), decodes **only the addressed numeric column**
natively, applies the predicate in native form, and aggregates directly —
optionally attaching bounded exact source evidence. Exposed as the `arrow-fused`
subcommand with an in-process `--also-current` ablation lane. **Supported**:
fixed-width integer and floating-point columns (+ optional validity). **Everything
else** — utf8, binary, bool, temporal, compressed, nested — **declines typed**,
never a wrong answer.

Question Q2 is `count(*) where numeric column > K` over the **same logical Arrow
data** for every lane; Q2E additionally returns the matching rows' coordinates
(VOLE: exact source spans; pyarrow: `indices_nonzero`). Exactness **3/3**; the
counts agree across `current_inproc`, `current_cli`, `pyarrow`, and `duckdb`
(99 / 499 / 42553).

| Gate (paired, in-process steady-state) | median | geomean | 95% CI | MDE | Verdict |
|---|---:|---:|---|---:|---|
| fused vs **current-VOLE in-process** (primary) | **0.0253** | 0.0328 | 0.0000..0.0423 | 0.0302 | **resolved faster** |
| fused vs **current-VOLE CLI** (deployed shape) | **0.0046** | 0.0057 | 0.0000..0.0107 | 0.0077 | **resolved faster** |
| fused vs **DuckDB** | **0.0077** | 0.0358 | 0.0000..0.1657 | 0.1194 | wins on aggregate |
| fused vs **pyarrow** | 0.6000 | **1.9948** | 0.0000..6.4694 | 4.6181 | **loses on aggregate** |

- The fused kernel is **~30× faster than DuckDB** on this question (was **2.25×
  slower** for the shipped path in 28.2) and **~40× faster than the shipped
  in-process VOLE path**; on the two small fixtures it is **near pyarrow**.
- It **still loses to pyarrow on the large fixture** (`fused/pyarrow` = **6.4615×**;
  pyarrow's vectorized kernel beats a fused scalar scan) and on the aggregate
  (geomean **1.99 > 1**). On Q2E the aggregate is a near-tie (geomean **1.0312**),
  with VOLE returning **bounded exact source spans** and pyarrow **matching
  indices**; both truncate to the same bound (evidence cap 1024) on the large
  fixture.
- Timer resolution is 1 µs, so the fused side floors at 0 on the small fixtures
  and their ratios floor at 0; **`e_large.arrow` is the discriminating fixture.**

**Recorded semantic finding.** The shipped `observe --kind text` driver **parses
integers only** and so silently yields **0** for a **non-integral float
predicate**; the fused kernel compares natively and matches pyarrow. The economic
corpus's Q2 column is an integer, where the two agree. This is a real correctness
distinction in the shipped driver, recorded, not hidden.

**Verdict: RESOLVED POSITIVE.** The fused kernel is a resolved end-to-end
improvement over the shipped VOLE path beyond the MDE, with exact agreement on the
count — but it does **not** claim to beat pyarrow everywhere. Contract with the
experiment's court: [phase-28-plan.md](phase-28-plan.md). Receipt:
[`2026-10-12-phase28-5-fused-arrow-e41a78fe`](../../evidence/campaigns/2026-10-12-phase28-5-fused-arrow-e41a78fe/);
historical control [`phase28-2-arrow-econ-f7e0a961`](../../evidence/campaigns/2026-10-12-phase28-2-arrow-econ-f7e0a961/).

## 28.6 — Experiment 2: fragment locality (coarse structural index justified)

**Measurement only** — no wire format, no planner change, no decoder change. Five
large documents (PDF 36.4 MB, DOCX 2.29 MB, JSON 16.8 MB, CSV 16.8 MB, Parquet
16.8 MB), each with a narrow request. The question: for a narrow request, how does
the **current** VOLE closure compare with the **minimum verified bytes** a
fragment directory would need, and with a real page-level SQLite control?

The current VOLE closure is **the whole encoded document** — it reads the whole
descriptor (`descriptor_read_mode=full`, the Phase-22.5 mechanism); an independent
`strace` store-read count agrees within ~0.1%.

| fmt | source B | VOLE `bytes_read` | ideal floor B | best fragment floor+dir | page SQLite B | VOLE/(floor+dir) | VOLE/SQL(pages) |
|---|---:|---:|---:|---:|---:|---:|---:|
| pdf | 36,404,840 | 36,414,410 | 61,856 | 83,697 (native) | 135,284 (29 pages) | 435.1× | 269.2× |
| docx | 2,286,200 | 2,309,293 | 15,192 | 17,400 (native) | 28,788 (7 pages) | 132.7× | 80.2× |
| json | 16,777,304 | 16,778,341 | 72 | 65,584 (fixed_16384) | 12,404 (3 pages) | 255.8× | 1352.7× |
| csv | 16,777,245 | 16,778,079 | 54 | 65,584 (fixed_16384) | 12,404 (3 pages) | 255.8× | 1352.6× |
| parquet | 16,779,953 | 16,780,999 | 400,023 | 401,271 (native_chunk) | 450,676 (110 pages) | 41.8× | 37.2× |

- A fragment directory's **floor+dir is ≥10× smaller than the whole-document
  closure in 5/5 formats** (42×–435× smaller) — the mechanism is that the closure
  is the whole encoded document while a directory needs only the format-native unit
  plus a bounded **coarse** directory.
- **But it beats the page-level SQLite control in only 3/5 formats**: **PDF/DOCX
  (and marginally Parquet) win; CSV/JSON lose** (a 4 KiB page-mapped SQLite is
  already 12,404 B vs 65,584 B floor+dir).
- **Directory overhead is the binding constraint for row/record formats.** The
  largest per-unit native directory is CSV/native: **23,539,920 B for 490,415
  units** (48 B per source record) — **larger than the source**. A per-record
  structural index is **not viable**; only a **coarse** index (objects / members /
  row groups / column chunks) is.

**Decision.** 28.8+ is justified as a **coarse structural index** — PDF
objects/pages, ZIP members, Parquet row groups/column chunks — **not** a
per-record index and **not** a fan-out of tiny fixed fragments. For CSV/JSON the
page-level control is not beaten on bytes+directory, so a fragmentation branch
there must justify itself on something other than bytes (e.g. no source
retention), or be dropped. Exactness **5/5**. Historical control: **Phase 22.5**
(VOLE selective transferred a median **3.15×** more bytes than page-level SQLite —
0 wins / 17 losses / 7 unresolved). Receipt:
[`2026-10-12-phase28-6-fragment-locality-dad05faa`](../../evidence/campaigns/2026-10-12-phase28-6-fragment-locality-dad05faa/).

## 28.7 — Experiment 3: cross-revision dependency reuse (positive, marginal, scoped)

**Measurement only** — a simulation; no wire format, no planner/decoder change, no
production-storage edit. Three lanes on authentic revision families (docx
real-draft-edits, docx style-table, xlsx shared-string, csv/json text-edits):

- **L1 full** — rebuild each revision from scratch.
- **L2 cas** — a fragment-level **content-addressed** store (content-defined
  chunking, dedup by hash, **no semantic edges**), so it must recompute every
  observation.
- **L3 graph** — L2 **plus semantic dependency edges**, so an observation whose
  full closure is unchanged is **reused**.

Identities stay separate (ADR-0060): exact revision SHA-256, physical fragment
content id, semantic context signature, and derived-observation content id are
distinct namespaces. (Context: the existing content-addressed descriptor store
retained ~0 cross-revision work, because 97.2% of persistent bytes is the whole
exact descriptor (Phase 22.6) and any change invalidates it wholesale —
fragmenting the authority is what localizes a revision.)

- **Next-revision observations reused: 314,725 of 377,549 = 83.4%.**
- Graph work saved: **314,294 of 566,242** verified node executions (**55.5%** of
  the full-rebuild work).
- The **minimal dependency-signature index is 3,160,480 B = 9.13% of L1 raw** (the
  CAS store already captures the byte reuse: **12,634,084 B saved vs a full
  rebuild, 36.5% of L1 raw**, and the graph adds only **3,115,552 B** of index).
- **The semantic edges are load-bearing: 986 invalidations have UNCHANGED physical
  bytes** and would be reused **wrong** by a physical-only store.
- Exactness **20/20**.

**Fragility (stated, not hidden).** The verdict is **marginal on the 10% index
criterion** (9.13%). If the design **also persists a 48 B per-observation identity
node**, the index becomes **12,289,312 B (35.51%)** and the **verdict flips to
NEGATIVE**. **Scope:** the work win lives in **localized edits**; on a
whole-document redraft it is **0 by construction** (and the byte baseline there is
also ~0). Criterion met: retained ≥20% ✓, index ≤10% of L1 ✓ (marginal), graph
delta < CAS byte saving ✓.

**Verdict: RESOLVED — positive but marginal/scoped.** Receipt:
[`2026-10-12-phase28-7-revision-reuse-b06a2f76`](../../evidence/campaigns/2026-10-12-phase28-7-revision-reuse-b06a2f76/).

## 28.8+ — the winning mechanisms (next programme)

Phase 28 **does not** assert VOLE is already faster than SQLite or DuckDB. It
tested, under equal contract, whether a provenance-preserving, physically
selective, fused observation executor can produce a resolved advantage — and
recorded the answer either way. The three decision gates now set the **order** of
the next programme (not the attractiveness of the mechanisms on paper):

1. **Fused execution (28.5 — RESOLVED POSITIVE).** A bounded native
   predicate/aggregate kernel that decodes only the addressed column is a resolved
   end-to-end win over the shipped path and beats DuckDB on the tested question; it
   still loses to pyarrow's vectorized kernel on the large fixture. Build the fused
   **observation executor** as a real mechanism, with its own falsifiable court and
   the pyarrow large-fixture loss carried forward.
2. **Physical fragmentation (28.6 — coarse structural index).** Build a **coarse
   structural** fragment directory (PDF objects/pages, ZIP members, Parquet
   row groups/column chunks) — never a per-record index or a fan-out of tiny fixed
   fragments. CSV/JSON must be justified on an axis other than bytes (page-level
   SQLite is not beaten on bytes+directory).
3. **Incremental observation (28.7 — marginal/scoped).** The fragment-level
   dependency graph is positive only under the **minimal dependency-signature
   index**; persisting per-observation identity nodes flips it negative, and it is
   scoped to **localized edits** (0% on whole-document redrafts). Adopt only with
   the index model fixed and the scope explicit.

Each mechanism ships only with its own falsifiable court, bounded costs, exact
reconstruction, and the negatives above retained.
