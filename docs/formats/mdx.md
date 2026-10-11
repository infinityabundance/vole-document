# MDX

MDX (Markdown + JSX + ESM) is a **prose** format of Phase 21 Wave 2 (subphase
21.26.3). It is a **superset** of Markdown, so it is **not a package** — the whole
source is the document, and the exact leaf is the source. The adapter **reuses the
Markdown parser and model** (never a second prose parser) and layers the
MDX-specific constructs on top; it is gated behind the **non-default** `mdx =
["markdown"]` feature.

## Authority boundary

MDX has **no magic bytes**. Detection is tried **before** generic Markdown and
before HTML, and requires an **MDX-specific signal on top of a successful Markdown
parse**: a top-level ESM `import`/`export` statement, a JSX component (a
capitalized or namespaced element name) or fragment, a JSX-specific attribute (a
brace value or spread), or a whole-line MDX block expression `{…}`. The scanner
**skips fenced/indented code, front matter, and inline code spans**, so a JSX/ESM
construct inside code never makes a Markdown document MDX. A plain Markdown
document stays `Markdown`, a plain HTML document stays `Html`, and plain prose
stays `Opaque` (and round-trips exactly through the RAW lane). It also declines
anything already claimed by JSON/YAML/CSV/HTML/XML/reST/AsciiDoc (defence in
depth).

**What it cannot distinguish.** A **plain HTML element** with only lowercase tags
and quoted attributes (`<div class="x">…</div>`) is indistinguishable from the
same JSX and carries **no MDX signal**, so MDX declines it and it stays `Html`;
only a capitalized/namespaced name or a JSX-specific attribute distinguishes JSX
from HTML. An **inline** `{…}` expression alone does not admit a document as MDX
(plain prose is full of balanced braces), so a Markdown document with an inline
`{x}` stays `Markdown`; only a **whole-line block expression** (or a
component/ESM/JSX-attribute signal) admits it. A component/capitalized tag inside a
Markdown **table cell** likewise admits MDX (a documented over-approximation).

## Representation preservation (the point of the format)

The full Markdown surface (headings, paragraphs, lists, fenced code, tables,
links, reference definitions, footnotes, front matter) is preserved by the reused
model, and the MDX constructs are recorded with **exact source spans**: ESM
`import`/`export` statements (default/named/multi-line), JSX elements and
fragments with their attributes and nested children (recorded as **extents**,
**never executed and never parsed as JavaScript**), and MDX `{ … }` expressions
inline and block (brace-balanced with string literals, escapes, and comments
respected). The source is never re-flowed or rendered: a block's, element's, or
expression's exact bytes are literally `source[span]`.

## Supported observations

Common selectors: `metadata`, `text`, `heading`, `block`, `search-match`. Native:
`mdx-heading`, `mdx-block`, `mdx-esm`, `mdx-jsx`, `mdx-expression`, `mdx-find`.

## Unsupported / honest cost

An out-of-range native ESM/JSX/expression and an unsupported common pair (the
common `table`) decline typed (exit 6); a malformed `--mdx-heading` argument is a
usage error (exit 2); plain prose stays `Opaque` (exit 6), never a panic. A full
MDX/JSX conformance oracle and JavaScript evaluation are **not claimed**.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted MDX
document, and after the source **and** descriptor are deleted in a fresh process
(adapter-court H1). Exactness is the whole source (a RAW-like `DocumentExact`):
the `MdxModel` node is derived (`Q_gen`) and never on the exactness path
(ADR-0060: the model node depends on the `sha256(source)` root, so no
source-reading node aliases another field's source).

**Adapter-court H1:** **11/11 byte-exact** (5 MDX fixtures + 6 controls: opaque
prose, Markdown, Markdown-with-JSX-in-inline-code, Markdown-with-inline-`{x}`,
HTML, and a brace blob), 6 detection-boundary rows, 1 typed decline, 1 usage row.

## Economic court

Measured by `tools/phase21-26-3-mdx-econ-court.sh` (a thin wrapper over the shared
`textfmt-court.sh` engine) against two comparators — a **source-retaining SQLite
store** with conventional extraction, and a conventional **decode-to-host-values
load** (`conv`). Corpus 6 fixtures, questions Q1–Q12, exactness **12/12**.
Estimator = paired per-fixture ratio VOLE/comparator, median + geometric mean with
a fixed-seed (2128), fixture-clustered 10000-resample 95 % CI, tie band ±10 %;
ratio-of-sums reported separately. A ratio < 1 favours VOLE.

| metric | comparator | median | geomean | median 95% CI | geomean 95% CI | W/T/L | ratio-of-sums |
|---|---|---:|---:|---|---|---:|---:|
| build | sqlite | 0.904 | 0.911 | 0.882..0.948 | 0.889..0.939 | 3/3/0 | 0.913 |
| build | conv | 1.202 | 1.195 | 1.171..1.213 | 1.176..1.210 | 0/0/6 | 1.196 |
| storage | sqlite | 0.594 | 0.599 | 0.592..0.611 | 0.593..0.608 | 6/0/0 | 0.595 |
| storage | conv | 9.932 | 6.197 | 3.495..10.469 | 2.711..10.271 | 1/0/5 | 0.890 |
| cold | sqlite | 0.029 | 0.032 | 0.027..0.044 | 0.027..0.041 | 6/0/0 | 0.034 |
| cold | conv | 0.029 | 0.034 | 0.028..0.050 | 0.029..0.045 | 6/0/0 | 0.036 |
| warm | sqlite | 0.081 | 0.093 | 0.076..0.146 | 0.077..0.129 | 6/0/0 | 0.161 |
| warm | conv | 1.027 | 0.836 | 0.629..1.060 | 0.554..1.049 | 1/5/0 | 0.346 |

Build is ~parity/slightly faster than SQLite, loses to the conventional load;
storage is ~0.59–0.60× SQLite; warm is a large win vs SQLite and **parity vs
`conv`** (median 1.03, ratio-of-sums 0.346).

## Honest negatives

* **MDX needs an ESM/JSX/expression signal.** A plain HTML element with lowercase
  tags and quoted attributes carries no MDX signal, so it stays **`Html`**.
* An **inline `{x}` alone does not promote Markdown** — only a whole-line block
  expression (or a component/ESM/JSX-attribute signal) admits MDX.
* A component/capitalized tag inside a Markdown **table cell** admits MDX — a
  **documented over-approximation**.
* JSX-looking text inside fenced/indented code or inline code spans is **never**
  an MDX signal.
* The **cold ratios** (≈0.03) are dominated by the comparators' fresh-process
  **Python start-up**, not by VOLE query speed; not a VOLE strength.
* The economic court is measured on a **self-authored deterministic corpus** (by
  `tools/fixtures/make-mdx.py`); only exact closure (Q6) is a byte-authority
  claim. The **conventional load is deliberately the weaker comparator.**

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.26.3 MDX adapter court (exactness 11/11):
  [2026-10-10-phase21-26-3-mdx-91b02068](../../evidence/campaigns/2026-10-10-phase21-26-3-mdx-91b02068/).
- Phase 21.26.3 MDX economic court (exactness 12/12):
  [2026-10-10-phase21-26-3-mdx-econ-f0a3a5cf](../../evidence/campaigns/2026-10-10-phase21-26-3-mdx-econ-f0a3a5cf/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
