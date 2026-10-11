# bibliography (BibTeX / RIS / CSL-JSON)

A bibliography — **BibTeX** (`.bib`), **RIS**, or **CSL-JSON** — is the
**bibliography** format of Phase 21 Wave 2 (subphase 21.31). One bounded adapter
covers the three dialects and records the dialect in the model. BibTeX and RIS
are line-anchored text (no magic bytes); CSL-JSON's physical bytes are JSON, so
the adapter **reuses the shared representation-preserving JSON parser** (never a
second JSON parser). It is **not a package** — the exact leaf is the whole
source — and is gated behind the **non-default, dependency-free** `bib =
["json"]` feature.

## Authority boundary

Detection is a bounded, **content-only** test (never a file name) run **before**
the generic JSON detector (a CSL-JSON array is otherwise claimed by JSON) and
**before** the weak text heuristics (YAML/TOML/config/CSV/Markdown/reST/AsciiDoc/
logstream/fixed-width). Claimed signals: a line-anchored `@type{` entry
sequence; an RIS `TY  - ` sequence whose **every** entry is `ER  - `-terminated;
or a JSON array (or single object) whose every element carries a string `id`
**and** a string `type`. The **recorded dialect** (`bibtex` / `ris` /
`csl_json`) lives in the model.

**What it cannot distinguish.** A JSON array of objects that each carry a string
`id` and a string `type` but are **not** bibliographic is byte-for-byte
indistinguishable from CSL-JSON and **is claimed as `csl_json`**; a single
GeoJSON `Feature`/`FeatureCollection` object is claimed by the GeoJSON detector
first. A plain JSON array, a plain JSON array of non-CSL objects, a generic JSON
object, and prose all stay `Json`/`Opaque`.

**RIS continuation ambiguity (recorded).** A RIS **continuation line** that
happens to begin with two uppercase alphanumerics and the `  - ` separator is
read as a **new tagged line** — the inherent RIS ambiguity — so the field
boundary there is the reading the grammar states.

## Representation preservation (the point of the format)

BibTeX keeps the entry key, the entry type and field names *as written*, the
field order, every value's exact spelling and delimiters (brace/quoted/number/
macro kept distinct, `#` concatenation recorded, duplicate fields preserved and
reported), and the whitespace/`@comment`/`%` bytes between entries. RIS keeps the
two-space `XX  - ` tag separator, the multi-line continuation span (flagged), and
the entry order. CSL-JSON keeps member order, duplicate keys, numeric/string-
escape spelling, key quoting, and every exact token span, plus each item's string
`id` and `type`. Nothing is normalized or re-serialized.

## Supported observations

Common selectors: `metadata`, `text`, `search-match`. Native: `bib-dialect`,
`bib-entry`, `bib-field`, `bib-key`, `bib-find`.

## Unsupported / honest cost

An out-of-range entry, an absent field/key, and an unsupported common pair
decline typed (exit 6); a plain JSON array of non-CSL objects and plain prose
decline typed (exit 6), never a panic. A BibTeX macro is **never resolved**
against `@string` (a value's kind is lexical).

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted
bibliography, and after the source **and** descriptor are deleted in a fresh
process. Exactness is the whole source (a RAW-like `DocumentExact`): the
`BibModel` node is derived (`Q_gen`) and never on the exactness path (ADR-0060:
the model node depends on the `sha256(source)` root).

**Adapter-court H1:** **5/5 byte-exact** (3 bib fixtures — `refs.bib`, `refs.ris`,
`refs.json` — + 2 detection-boundary controls: plain JSON array, prose; 1 typed
decline).

## Economic court

Measured by `tools/phase21-31-bib-econ-court.sh` (a thin wrapper over the shared
`textfmt-court.sh` engine) against two comparators — a **source-retaining SQLite
store** with conventional extraction, and a conventional **decode-to-host-values
load** (`conv`). Corpus 4 fixtures, questions Q1–Q12, exactness **6/6**.
Estimator = paired per-fixture ratio VOLE/comparator, median + geometric mean with
a fixed-seed (2133), fixture-clustered 10000-resample 95 % CI, tie band ±10 %;
ratio-of-sums reported separately. A ratio < 1 favours VOLE.

| metric | comparator | median | geomean | median 95% CI | geomean 95% CI | W/T/L | ratio-of-sums |
|---|---|---:|---:|---|---|---:|---:|
| build | sqlite | 0.776 | 0.817 | 0.674..1.096 | 0.699..1.004 | 3/1/0 | 0.829 |
| build | conv | 0.984 | 1.002 | 0.951..1.096 | 0.955..1.060 | 0/4/0 | 1.010 |
| storage | sqlite | 0.600 | 0.670 | 0.589..0.950 | 0.592..0.846 | 3/1/0 | 0.935 |
| storage | conv | 14.463 | 8.921 | 1.848..16.427 | 3.129..15.797 | 0/0/4 | 1.893 |
| cold | sqlite | 0.022 | 0.030 | 0.021..0.079 | 0.022..0.057 | 4/0/0 | 0.036 |
| cold | conv | 0.024 | 0.033 | 0.023..0.087 | 0.023..0.063 | 4/0/0 | 0.039 |
| warm | sqlite | 0.087 | 0.142 | 0.080..0.679 | 0.082..0.404 | 4/0/0 | 0.516 |
| warm | conv | 1.090 | 1.089 | 1.065..1.112 | 1.073..1.105 | 0/3/1 | 1.098 |

Build is ~parity; storage is ~0.60× SQLite but a **recorded loss vs the
conventional load**; cold is a win; warm is ~0.09× SQLite but a **recorded loss
vs the conventional in-process load**.

## Honest negatives

* **RIS continuation ambiguity (recorded):** a continuation line beginning with
  two uppercase alphanumerics and the `  - ` separator is read as a new tagged
  line, so that field boundary follows the grammar's reading.
* **CSL-JSON over-claim (recorded):** a non-bibliographic JSON array whose
  elements each carry a string `id` and `type` **is claimed as `csl_json`**.
* A BibTeX macro is never resolved against `@string`.
* **Storage-vs-`conv` and warm-vs-`conv` are recorded losses.**
* Only exact closure (Q6) is a byte-authority claim; the **conventional load is
  deliberately the weaker comparator**, and a span-preserving loader could in
  principle match VOLE on the representation axes.
* The **cold ratios** are dominated by the comparators' fresh-process **Python
  start-up**.
* The economic court is a **self-authored deterministic corpus** (by
  `tools/fixtures/make-bib.py`), not a real-world bibliography population.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.31.1 bibliography adapter court (exactness 5/5):
  [2026-10-10-phase21-31-1-bib-51bd19a3](../../evidence/campaigns/2026-10-10-phase21-31-1-bib-51bd19a3/).
- Phase 21.31 bibliography economic court (exactness 6/6):
  [2026-10-11-phase21-31-bib-econ-372a86e0](../../evidence/campaigns/2026-10-11-phase21-31-bib-econ-372a86e0/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
