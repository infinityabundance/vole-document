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
shared `textfmt-court.sh` engine) against two comparators — a **source-retaining
SQLite store** with conventional extraction, and a conventional **decode-to-host-
values load** (`conv`). Corpus 7 fixtures (`psv-basic.psv`, `psv-quotes.psv`,
`fw-basic.fw`, `fw-three.fw`, `fw-crlf.fw`, `large.psv`, `large.fw`), questions
Q1–Q12, exactness **11/11**. Estimator = paired per-fixture ratio
VOLE/comparator, median + geometric mean with a fixed-seed (2125), fixture-
clustered 10000-resample 95 % CI, tie band ±10 %; ratio-of-sums reported
separately. A ratio < 1 favours VOLE.

| metric | comparator | median | geomean | median 95% CI | geomean 95% CI | W/T/L | ratio-of-sums |
|---|---|---:|---:|---|---|---:|---:|
| build | sqlite | 0.909 | 0.939 | 0.863..0.971 | 0.883..1.017 | 3/3/1 | 0.948 |
| build | conv | 1.157 | 1.140 | 1.034..1.243 | 1.050..1.222 | 0/2/5 | 1.135 |
| storage | sqlite | 0.583 | 0.590 | 0.578..0.585 | 0.574..0.616 | 7/0/0 | 0.609 |
| storage | conv | 21.801 | 8.652 | 1.004..21.837 | 3.234..21.821 | 1/1/5 | 0.901 |
| cold | sqlite | 0.027 | 0.034 | 0.026..0.061 | 0.026..0.048 | 7/0/0 | 0.039 |
| cold | conv | 0.028 | 0.036 | 0.028..0.066 | 0.028..0.051 | 7/0/0 | 0.041 |
| warm | sqlite | 0.057 | 0.081 | 0.054..0.132 | 0.056..0.133 | 7/0/0 | 0.202 |
| warm | conv | 0.737 | 0.555 | 0.386..0.762 | 0.364..0.756 | 7/0/0 | 0.299 |

Build is ~parity/slightly faster than SQLite (median 0.91) but loses to the
conventional in-memory load (median 1.16); storage is ~0.58–0.59× SQLite; cold
and warm are large wins on this small-file corpus.

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
* The **cold ratios** (≈0.03) are dominated by the comparators' fresh-process
  **Python start-up**, not by VOLE query speed (see `python_startup_us` in the
  receipt environment); they are not a VOLE strength.
* The economic court is measured on a **self-authored deterministic corpus**;
  only exact closure (Q6) is a byte-authority claim. The **conventional load is
  deliberately the weaker comparator** (it drops spans, exact spelling, and order).
* The shared engine's comparators here are **SQLite + a conventional load**; the
  mandatory **DuckDB/Parquet** comparator for tabular formats (ADR-0059) is
  carried by the 21.7 CSV/TSV court, not by this tabular-extra run — recorded as a
  scope caveat rather than papered over.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0059](../adr/0059-xlsx-adapter-and-analytical-comparator.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.25.1 tabular-extra adapter court (exactness 10/10):
  [2026-10-10-phase21-25-1-tabular-e3c77a86](../../evidence/campaigns/2026-10-10-phase21-25-1-tabular-e3c77a86/).
- Phase 21.25 tabular economic court (exactness 11/11):
  [2026-10-10-phase21-25-tabular-econ-f0a3a5cf](../../evidence/campaigns/2026-10-10-phase21-25-tabular-econ-f0a3a5cf/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
