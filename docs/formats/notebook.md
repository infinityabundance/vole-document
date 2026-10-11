# Jupyter notebook (.ipynb)

A Jupyter notebook (`.ipynb`, nbformat) is the **document-shaped** format of
Phase 21 Wave 2 (subphase 21.24). Its physical bytes are JSON, so it is not a
package — the whole source is the document, and the exact leaf is the source. The
adapter **reuses the shared representation-preserving JSON parser** (never a
second parser) and is gated behind the **non-default** `notebook = ["json"]`
feature.

## Authority boundary

A notebook's physical bytes are JSON, so detection is a **bounded semantic
sub-detection** run **before** the generic JSON detector (and after GeoJSON). A
notebook is claimed only when the root object has a plain non-negative
integer-literal `nbformat` (≥ 1) and an array `cells`, every cell an object with
a string `cell_type`, and every present recognized field nbformat-shaped (a
string-or-array-of-strings `source`, a number-or-null `execution_count`, object
`metadata`/`attachments`, an array `outputs` of objects with a string
`output_type`). A plain JSON document, a JSON document that merely has a `cells`
key, a JSON document whose `cells` are not objects, and a non-integer `nbformat`
all stay `Json`; prose stays `Opaque` and round-trips exactly through the RAW
lane.

**What it cannot distinguish.** `cell_type`/`output_type` strings are preserved
but **not restricted** to the known set; `nbformat` is **not restricted** to a
version. A `source` **string is never split** and a `source` **array is never
joined** (the two forms cannot be conflated). A number literal is never reparsed,
so `4.0`/`4e0`/`-0` is not an integer `nbformat` and stays `Json`.

## Representation preservation (the point of the format)

The notebook model embeds the JSON node arena, so it preserves the exact
`nbformat`/`nbformat_minor`; the exact `cell_type` string; the exact `source`
representation (a string vs an array of lines, never re-joined); the exact
`execution_count`; cell/output order; `metadata`; and `attachments`. Every output
type (`stream`/`execute_result`/`display_data`/`error`) is reported with its
fields and its exact token bytes.

## Supported observations

Common selectors: `metadata`, `text`, `search-match`. Native: `notebook-nbformat`,
`notebook-cell`, `notebook-cell-type`, `notebook-cell-source`,
`notebook-cell-output`, `notebook-find`.

## Unsupported / honest cost

Executing a notebook and validating every nbformat schema rule are not claimed.
An out-of-range cell/output, a cell with no `source`, a malformed
`--notebook-cell-output` argument, a cap breach, and a non-notebook source are
typed declines (`InvalidNotebookStructure`, exit 38; `unsupported-feature`,
exit 6; `resource_limit`, exit 8; usage, exit 2); `Page(n)` is a typed decline.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted
notebook, and after the source **and** descriptor are deleted in a fresh process.
The exact leaf is the whole source; the derived model is never on the exactness
path (ADR-0060: the model node depends on the `DocumentExact` root keyed on
`sha256(source)`). Adapter-court H1 exactness **7/7** (3 notebook fixtures + 3
JSON controls + 1 opaque control).

## Economic court

Measured by `tools/phase21-24-notebook-court.sh` (a thin wrapper over the shared
`textfmt-court.sh` engine) against two conventional comparators — a
**source-retaining store** plus conventional extraction, and a conventional
**decode-to-host-values load**. Corpus 7 fixtures, questions Q1–Q12, exactness
**11/11**. Estimator = paired per-fixture ratio VOLE/comparator, median +
geometric mean with a fixed-seed, fixture-clustered 95 % CI; ratio-of-sums
reported separately. A ratio < 1 favours VOLE.

| metric | comparator | median | geomean | median 95% CI | geomean 95% CI | W/T/L | ratio-of-sums |
|---|---|---:|---:|---|---|---:|---:|
| build | sqlite | 0.971 | 0.992 | 0.939..1.017 | 0.944..1.062 | 0/6/1 | 0.996 |
| build | conv | 0.947 | 0.962 | 0.935..1.008 | 0.936..0.992 | 0/7/0 | 0.961 |
| storage | sqlite | 0.641 | 0.676 | 0.623..0.684 | 0.628..0.758 | 6/1/0 | 0.909 |
| storage | conv | 0.620 | 0.622 | 0.607..0.636 | 0.611..0.634 | 7/0/0 | 0.635 |
| cold | sqlite | 0.030 | 0.034 | 0.028..0.033 | 0.029..0.047 | 7/0/0 | 0.038 |
| warm | sqlite | 0.091 | 0.098 | 0.078..0.099 | 0.080..0.133 | 7/0/0 | 0.192 |
| warm | conv | 1.008 | 0.990 | 0.992..1.171 | 0.805..1.132 | 1/3/3 | 0.602 |

Build is ~parity (median 0.95–0.97); storage ~0.64× SQLite and ~0.62× the
conventional load; warm ~0.09× SQLite but ~parity vs the conventional in-process
load (median 1.008, a recorded mixed result).

## Honest negatives

* `cell_type`/`output_type` are preserved but not restricted; `nbformat` is not
  version-restricted; a number literal is never reparsed.
* The conventional load collapses duplicate keys (a recorded capability gap); a
  span-preserving loader is not built here.
* The economic court is measured on a **self-authored deterministic corpus**;
  only exact closure (Q6) is a byte-authority claim.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.24.1 notebook adapter court: `tools/phase21-24-1-notebook-court.sh`
  (exactness 7/7); campaign
  [2026-10-10-phase21-24-1-notebook-b6860e87](../../evidence/campaigns/2026-10-10-phase21-24-1-notebook-b6860e87/).
- Phase 21.24 economic court: `tools/phase21-24-notebook-court.sh` (exactness
  11/11); campaign
  [2026-10-10-phase21-24-notebook-econ-735d9b69](../../evidence/campaigns/2026-10-10-phase21-24-notebook-econ-735d9b69/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
