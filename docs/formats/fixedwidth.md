# Fixed-width (column-position tables)

Fixed-width is the **column-position** tabular format of Phase 21 Wave 2
(subphase 21.25.1). Like CSV/TSV/PSV it is **not a package** — the whole source
is the document, and the exact leaf is the source. It is a **distinct adapter**
(not a fourth CSV delimiter) gated behind the **non-default** `fixedwidth =
["csv"]` feature (it reuses the CSV/PSV detector only to *decline* anything that
also parses as a delimited table).

## Authority boundary

Fixed-width has **no magic bytes**, so detection is a documented, maximally
conservative heuristic. A source is claimed only when it has **≥ 3 sampled
records of identical byte width**, an inferred layout with **≥ 2 non-empty
columns**, and **interior whitespace gaps ≥ 2 columns wide** — and **only after
declining any source that also parses as a delimited (CSV/TSV/PSV) table or as a
Markdown table**. Detection samples a bounded prefix
(`max_fixedwidth_sampled_lines_for_detection`); `parse` re-validates **every**
line and declines the whole document typed (`InvalidFixedWidthStructure`, exit 39)
if any differs. Character positions are **byte** positions (multibyte UTF-8
shifts columns). Prose, variable-width text, and a single-space two-column layout
stay `Opaque` and round-trip exactly through the RAW lane.

**What it cannot distinguish.** A fixed-width file whose trailing spaces are
**trimmed** (variable width) is not claimed; a **single-space-separated**
two-column layout is not claimed (the gap must be ≥ 2 wide); and a genuinely
**ambiguous aligned-text blob** (equal-length lines, ≥ 2-wide gaps, ≥ 3 lines)
is **not distinguishable** from a fixed-width table and **is** claimed — a
recorded negative, not a guess.

## Representation preservation (the point of the format)

The inferred **per-column start/end positions and widths**, the **uniform record
width**, every record's **exact content span** and every field's **exact padded
bytes**, the terminator, a BOM, and the header row are all preserved on top of the
whole-source exact leaf. Nothing is re-flowed, trimmed, or normalized.

## Supported observations

Common selectors: `metadata`, `text`, `table`, `cell`, `search-match`. Native:
`fixedwidth-row`, `fixedwidth-cell`, `fixedwidth-header`, `fixedwidth-columns`,
`fixedwidth-range`, `fixedwidth-find`.

## Unsupported / honest cost

A declared column map and a full fixed-width-conformance oracle are not claimed;
`Page(n)` is a typed decline (a table has no pagination). An out-of-range record
and an out-of-range cell decline typed (exit 6); a malformed `--fixedwidth-cell`
argument is a usage error (exit 2); a cap breach is a resource-limit decline
(exit 8). `--fixedwidth-row`/`--fixedwidth-cell` are O(offset) bounded-memory
forward scans, not constant-time lookups.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted
fixed-width document, and after the source **and** descriptor are deleted in a
fresh process (adapter-court H1). Exactness is the whole source (a RAW-like
`DocumentExact`): the `FixedWidthModel` node is derived (`Q_gen`) and never on the
exactness path (ADR-0060: the model node depends on the `sha256(source)` root, so
no source-reading node aliases another field's source).

**Adapter-court H1:** **10/10 byte-exact** (4 fixed-width fixtures + 3 delimited
controls + 1 Markdown control + 2 opaque controls), 4 typed-decline rows, 3
detection-boundary rows, 1 usage row.

## Economic court

Measured by `tools/phase21-25-tabular-econ-court.sh` (a thin wrapper over the
shared `textfmt-court.sh` engine) against **three** comparators — a
**source-retaining SQLite store** with conventional extraction, a conventional
**decode-to-host-values load** (`conv`), and the **mandatory DuckDB/Parquet
analytical baseline** (ADR-0059; DuckDB 1.5.6, the pinned `analytical` service).
Corpus 7 fixtures (`psv-basic.psv`, `psv-quotes.psv`, `fw-basic.fw`,
`fw-three.fw`, `fw-crlf.fw`, `large.psv`, `large.fw`), questions Q1–Q12, exactness
**11/11**. Estimator = paired per-fixture ratio VOLE/comparator, median +
geometric mean with a fixed-seed (2125), fixture-clustered 10000-resample 95 % CI,
tie band ±10 %; ratio-of-sums reported separately. A ratio < 1 favours VOLE.

| metric | comparator | median | geomean | median 95% CI | geomean 95% CI | W/T/L | ratio-of-sums |
|---|---|---:|---:|---|---|---:|---:|
| build | sqlite | 0.760 | 0.857 | 0.725..0.955 | 0.749..1.019 | 5/1/1 | 0.880 |
| build | conv | 0.928 | 0.994 | 0.869..1.248 | 0.836..1.187 | 3/2/2 | 0.998 |
| build | duckdb | 0.333 | 0.381 | 0.309..0.522 | 0.316..0.475 | 7/0/0 | 0.394 |
| storage | sqlite | 0.583 | 0.590 | 0.578..0.585 | 0.574..0.616 | 7/0/0 | 0.609 |
| storage | conv | 21.801 | 8.652 | 1.004..21.837 | 3.234..21.821 | 1/1/5 | 0.901 |
| storage | duckdb | 6.557 | 7.483 | 6.433..10.622 | 6.215..9.250 | 0/0/7 | 11.233 |
| cold | sqlite | 0.027 | 0.033 | 0.025..0.054 | 0.025..0.046 | 7/0/0 | 0.037 |
| cold | conv | 0.028 | 0.034 | 0.026..0.061 | 0.026..0.048 | 7/0/0 | 0.038 |
| cold | duckdb | 0.010 | 0.013 | 0.009..0.022 | 0.009..0.018 | 7/0/0 | 0.014 |
| warm | sqlite | 0.074 | 0.088 | 0.067..0.133 | 0.059..0.143 | 7/0/0 | 0.197 |
| warm | conv | 0.776 | 0.591 | 0.347..0.973 | 0.353..0.881 | 5/2/0 | 0.276 |
| warm | duckdb | 0.001 | 0.003 | 0.001..0.017 | 0.001..0.009 | 7/0/0 | 0.012 |

Build is faster than all three comparators (**median 0.33× DuckDB**, 0.76×
SQLite, 0.93× conv); storage is ~0.58–0.59× SQLite and **loses to DuckDB**
(median **6.6×**, geometric mean **7.5×**, ratio-of-sums **11.2×** larger); cold
and warm are large wins on this small-file corpus (cold **~0.01× DuckDB**, warm
**~0.001×**).

**DuckDB wins the storage axis — recorded, not hidden.** On `large.psv` DuckDB's
Parquet projection is **49,804 B** against VOLE's **529,027 B** (and on
`large.fw` 42,712 B vs 529,091 B), so the columnar baseline is ~10× smaller on
the large fixtures; VOLE keeps the build/cold/warm axes. This is exactly the
tabular-format loss ADR-0059 requires the court to state.

## Honest negatives

* A **GFM delimiter row** is a Markdown table, so the **PSV pipe dialect** (and
  therefore the delimited path fixed-width deliberately declines against) is
  **declined** there — a Markdown pipe table is never stolen (recorded).
* Fixed-width **requires ≥ 3 uniform-width records** and **declines on delimited
  or Markdown input**; a trimmed variable-width file and a single-space two-column
  layout are **not** claimed.
* An **ambiguous aligned-text blob** (equal-length lines, ≥ 2-wide gaps, ≥ 3
  lines) **is** claimed — it is not distinguishable from a fixed-width table (a
  recorded negative), and positions are **byte** positions.
* The **storage vs `conv` median is 21.8×** (small files dominate, where the
  conventional store keeps a few hundred bytes against VOLE's ~4.8 KB
  descriptor+DAG+index); the **ratio-of-sums is 0.901**, so on the large fixtures
  VOLE is smaller. Both are reported, neither is hidden.
* The **cold ratios** (≈0.01–0.03) are dominated by the comparators' fresh-process
  **Python/CLI start-up**, not by VOLE query speed (see `python_startup_us` in the
  receipt environment); they are not a VOLE strength.
* The economic court is measured on a **self-authored deterministic corpus**;
  only exact closure (Q6) is a byte-authority claim. The **conventional load is
  deliberately the weaker comparator** (it drops spans, exact spelling, and order).
* **DuckDB wins the storage axis** (paired median ~6.6×, geometric mean ~7.5×,
  ratio-of-sums ~11.2× smaller; e.g. `large.psv` 49,804 B vs VOLE 529,027 B),
  while VOLE keeps build (~0.33×), cold (~0.01×) and warm (~0.001×). DuckDB
  **equivalence** over Q1–Q12: **6 equal** (Q1/Q3/Q4/Q5/Q7/Q10 — the columnar /
  tabular questions) and **6 capability-gap** (Q2/Q6/Q8/Q9/Q11/Q12 — the exact
  source-span/bytes, recorded-dialect and column-layout/quoting questions), with
  **0 mismatches**; its ADR-0059 lane is carried **alongside** SQLite.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0059](../adr/0059-xlsx-adapter-and-analytical-comparator.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.25.1 tabular-extra adapter court (exactness 10/10):
  [2026-10-10-phase21-25-1-tabular-e3c77a86](../../evidence/campaigns/2026-10-10-phase21-25-1-tabular-e3c77a86/).
- Phase 21.25 tabular economic court (PSV + fixed-width; exactness 11/11) — the
  **3-lane** court (SQLite + conv + the mandatory DuckDB/Parquet lane, ADR-0059):
  [2026-10-10-phase21-25-tabular-econ-584ee52e](../../evidence/campaigns/2026-10-10-phase21-25-tabular-econ-584ee52e/).
  The earlier **2-lane** court
  [`…-f0a3a5cf`](../../evidence/campaigns/2026-10-10-phase21-25-tabular-econ-f0a3a5cf/)
  (SQLite + conv only) is **superseded but retained**, not rewritten.
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
