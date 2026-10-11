# reStructuredText

reStructuredText (the **Docutils** input language) is a **prose** format of Phase
21 Wave 2 (subphase 21.26.1). Like Markdown it is **not a package** — the whole
source is the document, and the exact leaf is the source. The adapter is gated
behind the **non-default, dependency-free** `rst = []` feature.

## Authority boundary

reST has **no magic bytes**, so detection is a documented, conservative
heuristic that requires a **reST-specific structural signal**: an explicit-markup
start (`.. ` comment/directive/target/footnote/substitution), a grid or simple
table, a field list, a literal (`::`) or doctest (`>>>`) block, or a section
adornment whose character is **not** a Markdown construct. Markdown is tried
**first**, so a Markdown document is never stolen; plain prose stays `Opaque` and
round-trips exactly through the RAW lane. It runs immediately after Markdown and
before AsciiDoc and fixed-width.

**What it cannot distinguish.** An adornment of `-`, `*`, or `_` (a run of
length ≥ 3) is a **Markdown thematic break**; `~` (≥ 3) is a **Markdown fence
opener**; and `#` is a **Markdown ATX heading**. Those documents are admitted as
Markdown (or stay `Opaque`) and are **never reclassified as reST**. A tiny
standalone field list (`:name: value`) or directive (`.. name:: body`) is claimed
by **YAML first** (YAML precedes reST in the dispatcher), so it is not detected as
reST — the reST adapter's own structural-signal predicate still recognizes both.
The full directive option/argument grammar, multi-line section titles, and
structure nested inside directives/lists are left as literal text rather than
guessed.

## Representation preservation (the point of the format)

A bounded, line-based Docutils-subset parser keeps, for every construct, its
**exact source byte span** and bytes. The source is never re-flowed or rendered:
a block's exact bytes are literally `source[span]`, and the canonical text
projection is the source itself. Preserved: section titles with their exact
underline/overline adornment (character and length) and a recorded hierarchy,
paragraphs, explicit markup (comments, directives **preserved verbatim**,
substitution definitions, footnotes/citations, hyperlink targets),
field/option/definition lists, literal and doctest blocks, bullet/enumerated
lists with nesting, inline markup, and grid/simple tables.

## Supported observations

Common selectors: `metadata`, `text`, `heading`, `block`, `search-match`. Native:
`rst-heading`, `rst-block`, `rst-directive`, `rst-inline`, `rst-find`.

## Unsupported / honest cost

An out-of-range native title and an unsupported common pair (the common `table`)
decline typed (exit 6); a malformed `--rst-heading` argument is a usage error
(exit 2); plain prose stays `Opaque` (exit 6), never a panic. A full Docutils
conformance oracle is not claimed. Structure inside directives/lists and the full
directive grammar are left literal.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted reST
document, and after the source **and** descriptor are deleted in a fresh process
(adapter-court H1). Exactness is the whole source (a RAW-like `DocumentExact`):
the `RstModel` node is derived (`Q_gen`) and never on the exactness path
(ADR-0060: the model node depends on the `sha256(source)` root, so no
source-reading node aliases another field's source).

**Adapter-court H1:** **8/8 byte-exact** (6 reST fixtures + 1 opaque control + 1
Markdown control), 6 typed declines, 2 detection-boundary rows, 1 usage row.

## Economic court

Measured by `tools/phase21-26-1-rst-econ-court.sh` (a thin wrapper over the shared
`textfmt-court.sh` engine) against two comparators — a **source-retaining SQLite
store** with conventional extraction, and a conventional **decode-to-host-values
load** (`conv`). Corpus 7 fixtures, questions Q1–Q12, exactness **9/9**. Estimator
= paired per-fixture ratio VOLE/comparator, median + geometric mean with a
fixed-seed (2126), fixture-clustered 10000-resample 95 % CI, tie band ±10 %;
ratio-of-sums reported separately. A ratio < 1 favours VOLE.

| metric | comparator | median | geomean | median 95% CI | geomean 95% CI | W/T/L | ratio-of-sums |
|---|---|---:|---:|---|---|---:|---:|
| build | sqlite | 0.902 | 0.928 | 0.889..0.944 | 0.893..0.984 | 3/4/0 | 0.932 |
| build | conv | 1.223 | 1.232 | 1.194..1.263 | 1.199..1.272 | 0/0/7 | 1.235 |
| storage | sqlite | 0.602 | 0.599 | 0.589..0.604 | 0.594..0.604 | 7/0/0 | 0.603 |
| storage | conv | 9.160 | 7.032 | 7.665..12.022 | 3.334..11.267 | 1/0/6 | 0.918 |
| cold | sqlite | 0.029 | 0.031 | 0.028..0.029 | 0.027..0.040 | 7/0/0 | 0.033 |
| cold | conv | 0.030 | 0.033 | 0.029..0.031 | 0.029..0.042 | 7/0/0 | 0.035 |
| warm | sqlite | 0.086 | 0.097 | 0.082..0.092 | 0.083..0.127 | 7/0/0 | 0.157 |
| warm | conv | 1.058 | 0.889 | 1.027..1.136 | 0.598..1.105 | 1/4/2 | 0.334 |

Build is ~parity/slightly faster than SQLite, loses to the conventional load;
storage is ~0.60× SQLite; warm is a large win vs SQLite and **parity-to-slightly-
slower vs `conv`** (median 1.06, ratio-of-sums 0.334).

## Honest negatives

* reST **claims spaced `:name:` field lists ahead of AsciiDoc** — the canonical
  spaced form `:name: value` (and the unset form `:name!:`) is a reST field list,
  so reST (tried first) admits it and it is never reclassified as AsciiDoc.
* A **Markdown document stays Markdown** (`-`/`*`/`_` thematic break, `~` fence,
  `#` ATX heading are never re-read as reST adornments), and a tiny
  `:name: value` / `.. name:: body` source is claimed by **YAML first**.
* The supported subset is **bounded**: the full directive grammar, multi-line
  titles, and nested structure inside directives/lists are left as literal text.
* The **cold ratios** (≈0.03) are dominated by the comparators' fresh-process
  **Python start-up**, not by VOLE query speed; not a VOLE strength.
* The economic court is measured on a **self-authored deterministic corpus** (by
  `tools/fixtures/make-rst.py`); only exact closure (Q6) is a byte-authority
  claim. The **conventional load is deliberately the weaker comparator.**

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.26.1 reST adapter court (exactness 8/8):
  [2026-10-10-phase21-26-1-rst-91b02068](../../evidence/campaigns/2026-10-10-phase21-26-1-rst-91b02068/)
  (earlier runs `…-rst-0d2b839c`, `…-rst-43a1c327`).
- Phase 21.26.1 reST economic court (exactness 9/9):
  [2026-10-10-phase21-26-1-rst-econ-f0a3a5cf](../../evidence/campaigns/2026-10-10-phase21-26-1-rst-econ-f0a3a5cf/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
