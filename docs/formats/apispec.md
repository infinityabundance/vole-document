# API / specification documents

An API specification — a JSON Schema, an OpenAPI 3.x, a Swagger 2.0, or an
AsyncAPI document — is the **API/spec** format of Phase 21 Wave 2 (subphase
21.30). An API spec's physical bytes are JSON, so it is **not a package** and the
exact leaf is the whole source. The adapter **reuses the shared
representation-preserving JSON parser** (never a second JSON parser) and is gated
behind the **non-default, dependency-free** `apispec = ["json"]` feature.

## Authority boundary

Detection is a bounded, **content-only** semantic sub-test (never a file name)
run **before** the generic JSON detector: the root must be a JSON object
carrying a **strong, root-level string marker** — a JSON-Schema `$schema` URI, an
`openapi` `3.`-prefixed string, a `swagger` equal to `"2.0"`, or a non-empty
`asyncapi` string. The **recorded dialect** (`json_schema` / `openapi` /
`swagger` / `asyncapi`) lives in the model.

**What it cannot distinguish.** A JSON-Schema-**shaped** object with **no**
`$schema` is indistinguishable from a plain JSON tree and **stays `Json`**; a
plain JSON object that merely has a `properties`/`paths`/`components` key without
the marker also **stays `Json`**; an unrelated `openapi`/`swagger` value stays
`Json`; and prose stays `Opaque`. Detection never consults a file name.

## Representation preservation (the point of the format)

The reused arena keeps member order, duplicate keys, numeric/string-escape
spelling, key quoting, and every exact token span. It records the exact
spec-version token, every **object** (a role plus exact value/naming spans and a
member range), every key/value **member** (exact key/value spans, in document
order, duplicates preserved), and every **`$ref`** member. `$ref` targets are
preserved **verbatim and are never resolved, dereferenced, or fetched**
(dangling/external/cyclic references are recorded as written). Nothing is
normalized, resolved, or re-serialized.

## Supported observations

Common selectors: `metadata`, `text`, `search-match`. Native:
`apispec-dialect`, `apispec-version`, `apispec-object`, `apispec-ref`,
`apispec-find`.

## Unsupported / honest cost

An out-of-range object/`$ref` and an unsupported common pair decline typed
(exit 6); the generic-JSON and JSON-Schema-shaped controls and prose stay
`Json`/`Opaque`. A schema validator, a `$ref` resolver, and any rendering are
**not claimed**.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted API
spec, and after the source **and** descriptor are deleted in a fresh process.
Exactness is the whole source (a RAW-like `DocumentExact`): the API-spec model is
derived (`Q_gen`) and never on the exactness path (ADR-0060: the model node
depends on the `sha256(source)` root).

**Adapter-court H1:** **7/7 byte-exact** (4 apispec fixtures — `schema.json`,
`openapi.json`, `swagger.json`, `asyncapi.json` — + 3 detection-boundary
controls: generic JSON, JSON-Schema-shaped-without-`$schema`, prose; 1 typed
decline).

## Economic court

Measured by `tools/phase21-30-apispec-econ-court.sh` (a thin wrapper over the
shared `textfmt-court.sh` engine) against two comparators — a **source-retaining
SQLite store** with conventional extraction, and a conventional
**decode-to-host-values load** (`conv`). Corpus 5 fixtures, questions Q1–Q12,
exactness **8/8**. Estimator = paired per-fixture ratio VOLE/comparator, median +
geometric mean with a fixed-seed (2132), fixture-clustered 10000-resample 95 %
CI, tie band ±10 %; ratio-of-sums reported separately. A ratio < 1 favours VOLE.

| metric | comparator | median | geomean | median 95% CI | geomean 95% CI | W/T/L | ratio-of-sums |
|---|---|---:|---:|---|---|---:|---:|
| build | sqlite | 0.798 | 0.791 | 0.696..0.930 | 0.735..0.864 | 4/1/0 | 0.797 |
| build | conv | 1.031 | 1.040 | 0.984..1.131 | 1.002..1.089 | 0/4/1 | 1.046 |
| storage | sqlite | 0.620 | 0.784 | 0.616..1.957 | 0.618..1.241 | 4/0/1 | 1.812 |
| storage | conv | 27.245 | 64.667 | 21.954..2829.861 | 23.812..431.206 | 0/0/5 | 543.803 |
| cold | sqlite | 0.023 | 0.030 | 0.021..0.092 | 0.022..0.053 | 5/0/0 | 0.037 |
| cold | conv | 0.024 | 0.031 | 0.022..0.100 | 0.023..0.056 | 5/0/0 | 0.039 |
| warm | sqlite | 0.113 | 0.250 | 0.102..6.878 | 0.104..1.316 | 4/0/1 | 2.467 |
| warm | conv | 1.491 | 3.833 | 1.377..190.793 | 1.402..27.096 | 0/0/5 | 39.940 |

Build is ~0.80× SQLite and ~parity with the conventional load (one loss on the
large fixture); **storage has a recorded loss vs SQLite** on the large fixture
(ratio-of-sums 1.812) and a large **recorded loss vs `conv`**; cold is a win;
**warm has a recorded loss vs SQLite** on the large fixture (ratio-of-sums
2.467).

## Honest negatives

* **Boundary (recorded):** a JSON-Schema-shaped object with no `$schema` **stays
  `Json`**; a plain JSON object with a `properties`/`paths`/`components` key but
  no marker also stays `Json`.
* **Storage-vs-SQLite and warm-vs-SQLite are recorded losses** on the large
  fixture (the model is larger than the normalized store), and storage-vs-`conv`
  is a very large recorded loss.
* `$ref` is never resolved or fetched; validation is not performed.
* Only exact closure (Q6) is a byte-authority claim; the **conventional load is
  deliberately the weaker comparator**, and a span-preserving loader could in
  principle match VOLE on the representation axes.
* The **cold ratios** are dominated by the comparators' fresh-process **Python
  start-up**.
* The economic court is a **self-authored deterministic corpus** (by
  `tools/fixtures/make-apispec.py`), not a real-world spec population.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.30.1 API-spec adapter court (exactness 7/7):
  [2026-10-10-phase21-30-1-apispec-d2f77c3b](../../evidence/campaigns/2026-10-10-phase21-30-1-apispec-d2f77c3b/).
- Phase 21.30 API-spec economic court (exactness 8/8):
  [2026-10-11-phase21-30-apispec-econ-372a86e0](../../evidence/campaigns/2026-10-11-phase21-30-apispec-econ-372a86e0/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
