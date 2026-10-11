# MHTML (MIME HTML)

MHTML (MIME HTML, RFC 2557) is the **web-archive** format of Phase 21 Wave 2
(subphase 21.27). A browser-saved MHTML page is a MIME `multipart/related`
message whose root part is HTML and whose other parts are the page's
sub-resources, so it is **not a package** — the whole source is the document and
the exact leaf is the source. The adapter **reuses the EML MIME layer and the
HTML scanner** (never a second MIME or HTML parser) and is gated behind the
**non-default** `mhtml = ["eml", "html"]` feature (dependency-free; it implies
both reuses).

## Authority boundary

MHTML's physical bytes are a MIME message, so detection is a **bounded semantic
sub-detection run before the generic EML detector**: the root must be
`multipart/related` **and** carry an MHTML-specific signal — a top-level
`Snapshot-Content-Location`/`Content-Base`, or a `From`+`Subject` envelope with
a `text/html` part. A plain `multipart/related` email without those markers
stays `Eml`, a plain HTML document (no MIME envelope) stays `Html`, and a plain
non-`multipart/related` MIME message stays `Eml`.

**What it cannot distinguish (an honest over-approximation).** A plain
`multipart/related` message that carries a `From`+`Subject` envelope and a
`text/html` part **is admitted as MHTML** — the envelope signal is genuine, and
this is a documented over-approximation. `Content-Base` URL resolution is never
performed, and no sub-resource URL is resolved or fetched.

## Representation preservation (the point of the format)

The whole MIME envelope is preserved: the header block with its exact folded
spans, the ordered parts with their exact spans, and the ordered sub-resources
keyed by `Content-Location`/`Content-ID`. The root HTML part is parsed by the
reused HTML scanner (ordered sub-resources, the root part located as the
`start=` `Content-ID`, else the first `text/html` part, else the first part).
Quoted-printable/base64 parts are decoded **only for a derived observation**: the
exact raw (encoded) body span is retained, so the decoded bytes are never a
substitute for the encoded form, and vice versa.

## Supported observations

Common selectors: `metadata`, `text`, `resource`, `search-match`. Native:
`mhtml-root`, `mhtml-resource`, `mhtml-location`, `mhtml-find`.

## Unsupported / honest cost

An out-of-range native resource and an unsupported common pair decline typed
(exit 6); a malformed native argument is a usage error (exit 2); plain prose
stays `Opaque` (exit 6). A browser/rendering oracle, CSS/JS execution, and
sub-resource URL resolution are **not claimed**.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted
MHTML, and after the source **and** descriptor are deleted in a fresh process.
Exactness is the whole source (a RAW-like `DocumentExact`): the MHTML model is
derived (`Q_gen`) and never on the exactness path (ADR-0060: the model node
depends on the `sha256(source)` root, so no source-reading node aliases another
field's source).

**Adapter-court H1:** **9/9 byte-exact** (4 MHTML fixtures — `base`, `nostart`,
`envelope`, `large` — + 5 detection-boundary controls + 1 typed decline + 1
usage row).

## Economic court

Measured by `tools/phase21-27-mhtml-econ-court.sh` (a thin wrapper over the
shared `textfmt-court.sh` engine) against two comparators — a **source-retaining
SQLite store** with conventional extraction, and a conventional
**decode-to-host-values load** (`conv`). Corpus 4 fixtures, questions Q1–Q12,
exactness **7/7**. Estimator = paired per-fixture ratio VOLE/comparator, median +
geometric mean with a fixed-seed (2129), fixture-clustered 10000-resample 95 %
CI, tie band ±10 %; ratio-of-sums reported separately. A ratio < 1 favours VOLE.

| metric | comparator | median | geomean | median 95% CI | geomean 95% CI | W/T/L | ratio-of-sums |
|---|---|---:|---:|---|---|---:|---:|
| build | sqlite | 0.690 | 0.673 | 0.608..0.709 | 0.631..0.707 | 4/0/0 | 0.672 |
| build | conv | 0.821 | 0.806 | 0.727..0.860 | 0.755..0.853 | 4/0/0 | 0.805 |
| storage | sqlite | 0.709 | 0.736 | 0.607..0.971 | 0.626..0.877 | 3/1/0 | 0.959 |
| storage | conv | 12.298 | 8.214 | 1.907..15.908 | 3.104..14.590 | 0/0/4 | 1.958 |
| cold | sqlite | 0.026 | 0.036 | 0.022..0.118 | 0.022..0.078 | 4/0/0 | 0.046 |
| cold | conv | 0.028 | 0.038 | 0.022..0.122 | 0.023..0.081 | 4/0/0 | 0.049 |
| warm | sqlite | 0.128 | 0.245 | 0.090..2.432 | 0.100..1.147 | 3/0/1 | 1.553 |
| warm | conv | 1.608 | 1.969 | 1.183..4.944 | 1.301..3.659 | 0/0/4 | 4.635 |

Build is faster than both; storage is ~0.71× SQLite but **losses badly to the
conventional load** (VOLE stores the model, the conventional lane stores only the
decoded root); cold is a win; **warm is a recorded loss vs SQLite** (median
0.128 but 3/0/1 and ratio-of-sums 1.553) and a loss vs `conv`.

## Honest negatives

* **Over-approximation (recorded):** a plain `multipart/related` message with a
  `From`+`Subject` envelope and a `text/html` part **is admitted as MHTML** — the
  envelope is a genuine signal, so this is a documented over-approximation, not a
  defect to patch.
* **Storage-vs-`conv` is a recorded loss**, and **warm-vs-SQLite is a recorded
  loss** (ratio-of-sums 1.553).
* Only exact closure (Q6) is a byte-authority claim; the **conventional load is
  deliberately the weaker comparator** (it drops spans, exact spelling, duplicate
  keys, and member/attribute order), and a span-preserving loader could in
  principle match VOLE on those.
* The **cold ratios** are dominated by the comparators' fresh-process **Python
  start-up**, not by VOLE query speed.
* The economic court is measured on a **self-authored deterministic corpus** (by
  `tools/fixtures/make-mhtml.py`), not a real-world population.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.27.1 MHTML adapter court (exactness 9/9):
  [2026-10-10-phase21-27-1-mhtml-eccb438c](../../evidence/campaigns/2026-10-10-phase21-27-1-mhtml-eccb438c/).
- Phase 21.27 MHTML economic court (exactness 7/7):
  [2026-10-11-phase21-27-mhtml-econ-372a86e0](../../evidence/campaigns/2026-10-11-phase21-27-mhtml-econ-372a86e0/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
