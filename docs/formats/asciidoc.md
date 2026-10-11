# AsciiDoc

AsciiDoc (the **Asciidoctor** input language) is a **prose** format of Phase 21
Wave 2 (subphase 21.26.2). Like Markdown and reStructuredText it is **not a
package** — the whole source is the document, and the exact leaf is the source.
The adapter is gated behind the **non-default, dependency-free** `asciidoc = []`
feature.

## Authority boundary

AsciiDoc has **no magic bytes**, so detection is a documented, conservative
heuristic that requires an **AsciiDoc-specific structural mark**: a level-0
document title (`= ` on the first non-blank line followed by at least one further
block), a `==`+ section, a `|===` table, a complete delimited block, or a block
attribute line (`[…]`) followed by a block. **Markdown and
reStructuredText are tried first**, so a Markdown document stays `Markdown` and a
reST document stays `Rst`; plain prose stays `Opaque` and round-trips exactly
through the RAW lane. It runs immediately after reStructuredText and before
fixed-width.

**What it cannot distinguish.** An example (`====`), passthrough (`++++`), or
literal (`....`) block whose marker is `=`, `+`, or `.` and whose body is a
**single non-blank line** is indistinguishable from a **reST overline section
title** (`====\nTitle\n====`); reST is tried first, so such a source is admitted
as `Rst` (or stays `Opaque`) and is never reclassified as AsciiDoc. A document
attribute written in the canonical **spaced** form (`:name: value`) or the
**unset** form (`:name!:`) is a **reStructuredText field list**, so reST claims
it; only the **no-space** `:name:value` spelling stays AsciiDoc (the
`attributes_spaced.adoc` control demonstrates this). A source that is **only**
`:name: value`-shaped lines is also claimed earlier by **YAML/config**. The
AsciiDoc adapter still parses both attribute spellings inside a document it is
handed directly.

## Representation preservation (the point of the format)

A bounded, line-based AsciiDoc-subset parser keeps, for every construct, its
**exact source byte span** and bytes; the source is never re-flowed or rendered.
Preserved: a level-0 document title and `==`+ sections with their exact `=` marker
and a recorded level, paragraphs, document attributes (`:name: value` / `:name!:`)
with attribute references (`{name}`) surfaced **literally** (never expanded),
block attribute lines attached to the following block, delimited blocks
(listing/literal/example/sidebar/quote/open/passthrough) with their exact
delimiter and **verbatim** content, unordered/ordered/description lists with
nesting, tables (`|===` with `|` cell markers), admonitions (`NOTE:` and
`[NOTE]`), and inline markup (strong/emphasis/mono/passthrough/superscript/
subscript/mark) plus the `link:`, `image:`, `include::`, `xref:` and bare-URL
macros preserved verbatim.

## Supported observations

Common selectors: `metadata`, `text`, `heading`, `block`, `search-match`. Native:
`adoc-heading`, `adoc-block`, `adoc-attribute`, `adoc-inline`, `adoc-find`.

## Unsupported / honest cost

An out-of-range native heading and an unsupported common pair (the common `table`)
decline typed (exit 6); a malformed `--adoc-heading` argument is a usage error
(exit 2); plain prose stays `Opaque` (exit 6), never a panic. **Attribute
expansion, include/link resolution, the full cell/row-spanning table grammar, and
structure nested inside delimited blocks are not claimed** (left as literal
text). A full Asciidoctor conformance oracle is not claimed.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted
AsciiDoc document, and after the source **and** descriptor are deleted in a fresh
process (adapter-court H1). Exactness is the whole source (a RAW-like
`DocumentExact`): the `AsciidocModel` node is derived (`Q_gen`) and never on the
exactness path (ADR-0060: the model node depends on the `sha256(source)` root, so
no source-reading node aliases another field's source).

**Adapter-court H1:** **11/11 byte-exact** (7 AsciiDoc fixtures + 1 opaque
control + 1 Markdown control + 1 reST control + 1 spaced-attribute control that
lands as `Rst`), 7 typed declines, 4 detection-boundary rows, 1 usage row.

## Economic court

Measured by `tools/phase21-26-2-asciidoc-econ-court.sh` (a thin wrapper over the
shared `textfmt-court.sh` engine) against two comparators — a **source-retaining
SQLite store** with conventional extraction, and a conventional **decode-to-host-
values load** (`conv`). Corpus 7 fixtures, questions Q1–Q12, exactness **11/11**.
Estimator = paired per-fixture ratio VOLE/comparator, median + geometric mean with
a fixed-seed (2127), fixture-clustered 10000-resample 95 % CI, tie band ±10 %;
ratio-of-sums reported separately. A ratio < 1 favours VOLE.

| metric | comparator | median | geomean | median 95% CI | geomean 95% CI | W/T/L | ratio-of-sums |
|---|---|---:|---:|---|---|---:|---:|
| build | sqlite | 0.915 | 0.944 | 0.886..0.985 | 0.901..1.001 | 2/4/1 | 0.951 |
| build | conv | 1.254 | 1.243 | 1.169..1.281 | 1.195..1.299 | 0/0/7 | 1.249 |
| storage | sqlite | 0.602 | 0.608 | 0.592..0.631 | 0.596..0.622 | 7/0/0 | 0.596 |
| storage | conv | 7.475 | 6.129 | 5.211..12.067 | 3.050..10.157 | 1/0/6 | 0.897 |
| cold | sqlite | 0.028 | 0.030 | 0.025..0.029 | 0.026..0.038 | 7/0/0 | 0.032 |
| cold | conv | 0.030 | 0.032 | 0.027..0.030 | 0.028..0.041 | 7/0/0 | 0.034 |
| warm | sqlite | 0.086 | 0.097 | 0.079..0.091 | 0.080..0.130 | 7/0/0 | 0.168 |
| warm | conv | 1.069 | 0.907 | 1.035..1.097 | 0.629..1.113 | 1/5/1 | 0.364 |

Build is ~parity/slightly faster than SQLite, loses to the conventional load;
storage is ~0.60× SQLite; warm is a large win vs SQLite and **parity-to-slightly-
slower vs `conv`** (median 1.07, ratio-of-sums 0.364).

## Honest negatives

* **An AsciiDoc-specific mark is required.** Without a document title, `==`
  section, `|===` table, complete delimited block, or block-attribute line
  followed by a block, a source is not claimed.
* A single-non-blank-line `====`/`++++`/`....` block is a **reST overline title**,
  so reST (tried first) admits it and it is never reclassified as AsciiDoc.
* The canonical spaced attribute `:name: value` and unset `:name!:` are **reST
  field lists** (reST is tried first); only the **no-space** `:name:value`
  spelling stays AsciiDoc, and a source that is only `:name: value` lines is
  claimed earlier by **YAML/config**.
* The **cold ratios** (≈0.03) are dominated by the comparators' fresh-process
  **Python start-up**, not by VOLE query speed; not a VOLE strength.
* The economic court is measured on a **self-authored deterministic corpus** (by
  `tools/fixtures/make-asciidoc.py`); only exact closure (Q6) is a byte-authority
  claim. The **conventional load is deliberately the weaker comparator.**

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.26.2 AsciiDoc adapter court (exactness 11/11):
  [2026-10-10-phase21-26-2-asciidoc-91b02068](../../evidence/campaigns/2026-10-10-phase21-26-2-asciidoc-91b02068/)
  (earlier run `…-asciidoc-43a1c327`).
- Phase 21.26.2 AsciiDoc economic court (exactness 11/11):
  [2026-10-10-phase21-26-2-asciidoc-econ-f0a3a5cf](../../evidence/campaigns/2026-10-10-phase21-26-2-asciidoc-econ-f0a3a5cf/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
