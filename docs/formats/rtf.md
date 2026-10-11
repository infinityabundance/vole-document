# RTF (Rich Text Format)

RTF (Rich Text Format) is the **word-processor markup** format of Phase 21 Wave 2
(subphase 21.32). RTF is a text, group-brace grammar with control words — closer
to a bounded tree than to a flat line format — but it is **not a package** (no
OPC/ZIP layer): the exact leaf is the whole source. The adapter is gated behind
the **non-default, dependency-free** `rtf = []` feature.

## Authority boundary

Detection is a **strong magic test, not a full-grammar test**: the source must
begin (after an optional UTF-8 BOM) with `{\rtf` followed by a digit, and its
group braces must be balanced under a bounded scan, so a plain text or
leading-`{` source stays `Opaque`. It is tried with the strong-magic detectors,
after the binary magics and before the JSON family; an RTF source begins with
`{`, which is not a valid JSON document.

**What it cannot distinguish.** A blob that begins with `{\rtf<digit>` and has
balanced braces **is admitted as RTF even if its control words are meaningless** —
detection is magic + brace balance, not grammar conformance. A leading-`{` junk
blob, prose, a `{\rtfX` header with **no version digit**, and an **unbalanced
brace** all stay `Opaque`.

## Representation preservation (the point of the format)

The header (`{\rtf1\ansi\deffN …}` with the exact `\rtf` version and
charset/`\deff` control words), every group `{ … }` with its exact brace span and
nesting depth, every control word (`\word` with its optional numeric parameter,
`\fs24`) and control symbol (`\\`, `\{`, `\}`, `\~`, `\-`, `\_`, `\*`), every hex
escape (`\'hh`, spelling preserved with the decoded byte under the declared
charset) and Unicode escape (`\uN` with its `\ucN` fallback count — the literal
form **and** the fallback bytes are preserved, never dropped), every destination
(`\*` ignore-destination, `\fonttbl`, `\colortbl`, `\stylesheet`, `\info`, and
`\pict`/`\object` binary groups) verbatim with exact spans, and every plain text
run byte-exactly (never whitespace-normalized — RTF uses a space to terminate a
control word).

## Supported observations

Common selectors: `metadata`, `text`, `search-match`. Native: `rtf-header`,
`rtf-group`, `rtf-control`, `rtf-hex`, `rtf-unicode`, `rtf-text`, `rtf-dest`,
`rtf-find`.

## Unsupported / honest cost

An out-of-range index, a bad destination reference, and an unsupported common
pair decline typed (exit 6). The adapter **does not render**, does not resolve
font/stylesheet/info semantics, and does not map the `\mac`/`\pc`/`\pca` code
pages beyond reporting the raw decoded byte.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted RTF
document, and after the source **and** descriptor are deleted in a fresh process.
Exactness is the whole source (a RAW-like `DocumentExact`): the RTF model is
derived (`Q_gen`) and never on the exactness path (ADR-0060: the model node
depends on the `sha256(source)` root).

**Adapter-court H1:** **5/5 byte-exact** (1 RTF fixture `doc.rtf` + 4
detection-boundary controls: leading-`{` junk, prose, `{\rtfX` with no version
digit, unbalanced brace; 1 typed decline).

## Economic court

Measured by `tools/phase21-32-rtf-econ-court.sh` (a thin wrapper over the shared
`textfmt-court.sh` engine) against two comparators — a **source-retaining SQLite
store** with conventional extraction, and a conventional **decode-to-host-values
load** (`conv`). Corpus 2 fixtures, questions Q1–Q12, exactness **6/6**.
Estimator = paired per-fixture ratio VOLE/comparator, median + geometric mean with
a fixed-seed (2134), fixture-clustered 10000-resample 95 % CI, tie band ±10 %;
ratio-of-sums reported separately. A ratio < 1 favours VOLE.

| metric | comparator | median | geomean | median 95% CI | geomean 95% CI | W/T/L | ratio-of-sums |
|---|---|---:|---:|---|---|---:|---:|
| build | sqlite | 0.664 | 0.663 | 0.627..0.701 | 0.627..0.701 | 2/0/0 | 0.664 |
| build | conv | 0.950 | 0.947 | 0.884..1.015 | 0.884..1.015 | 1/1/0 | 0.941 |
| storage | sqlite | 1.292 | 1.109 | 0.629..1.956 | 0.629..1.956 | 1/0/1 | 1.917 |
| storage | conv | 1077.372 | 215.200 | 21.711..2133.032 | 21.711..2133.032 | 0/0/2 | 1096.881 |
| cold | sqlite | 0.047 | 0.040 | 0.023..0.071 | 0.023..0.071 | 2/0/0 | 0.046 |
| cold | conv | 0.051 | 0.044 | 0.025..0.077 | 0.025..0.077 | 2/0/0 | 0.051 |
| warm | sqlite | 1.522 | 0.588 | 0.118..2.927 | 0.118..2.927 | 1/0/1 | 2.020 |
| warm | conv | 38.762 | 10.792 | 1.533..75.991 | 1.533..75.991 | 0/0/2 | 39.612 |

Build is a win on both lanes; **storage-vs-SQLite is a recorded loss** (median
1.292) and storage-vs-`conv` is a very large recorded loss; cold is a win; warm is
a **mixed recorded loss** (1/0/1, ratio-of-sums 2.020).

## Honest negatives

* **Detection is a magic + brace-balance test, not a full-grammar test** — a
  `{\rtf<digit>`-prefixed blob with balanced braces **is admitted as RTF even if
  its control words are meaningless**.
* The adapter does **not render**, does not resolve font/stylesheet/info
  semantics, and does not map `\mac`/`\pc`/`\pca` code pages beyond the raw
  decoded byte.
* **Storage-vs-SQLite is a recorded loss**, and warm-vs-SQLite is a mixed loss.
* Only exact closure (Q6) is a byte-authority claim; the **conventional load is
  deliberately the weaker comparator**.
* The **cold ratios** are dominated by the comparators' fresh-process **Python
  start-up**.
* The economic court is a **self-authored deterministic corpus** (by
  `tools/fixtures/make-rtf.py`), not a real-world RTF population.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.32.1 RTF adapter court (exactness 5/5):
  [2026-10-10-phase21-32-1-rtf-714e1009](../../evidence/campaigns/2026-10-10-phase21-32-1-rtf-714e1009/).
- Phase 21.32 RTF economic court (exactness 6/6):
  [2026-10-11-phase21-32-rtf-econ-372a86e0](../../evidence/campaigns/2026-10-11-phase21-32-rtf-econ-372a86e0/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
