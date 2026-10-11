# package metadata

The package-metadata manifests — npm `package.json` / `package-lock.json`, Cargo
`Cargo.toml` / `Cargo.lock`, and Python `pyproject.toml` — are the **manifest**
format of Phase 21 Wave 2 (subphase 21.29). A manifest's physical bytes belong to
another format (JSON or TOML), so it is **not a package** and the exact leaf is
the whole source. The adapter **reuses the shared JSON and TOML parsers** (never
a second parser) and is gated behind the **non-default** `pkgmeta = ["json",
"toml"]` feature (dependency-free; it implies both reuses).

## Authority boundary

Detection is a bounded, **content-only** semantic sub-test (never a file name)
requiring a strong manifest shape, run **before** the generic JSON and TOML
detectors. Claimed shapes: a JSON object with string `name`+`version` **and** a
third package-specific key; a JSON `lockfileVersion`+`packages`/`dependencies`;
a TOML `[package]` table with a string `name`; a TOML
`[build-system]`/`[project]`/`[tool.poetry]`; or a TOML `[[package]]` lockfile
shape. The **recorded dialect** (`npm_package` / `cargo_manifest` /
`pyproject_manifest` / `cargo_lock` / `npm_lock`) lives in the model.

**What it cannot distinguish.** A generic JSON `{"name":…,"version":…}` with
fewer than two package-specific keys is indistinguishable from a minimal
`package.json` and **stays `Json`**; a generic TOML `name`/`version` with no
`[package]` table **stays `Toml`**; a virtual Cargo workspace manifest carrying
only `[workspace]` **stays `Toml`**; and prose stays `Opaque`. Detection never
consults a file name.

## Representation preservation (the point of the format)

The reused JSON/TOML arena keeps member order, duplicate keys, numeric and
string-escape spelling, and key quoting, and every recorded **section** (a named
table / array-of-tables / array element in TOML, or an object / array member in
JSON) and every key/value **entry** keeps its exact name/key/value spans, grouped
by section in document order. Inline-table and array-valued dependency specs are
preserved verbatim. Nothing is normalized or re-serialized.

## Supported observations

Common selectors: `metadata`, `text`, `search-match`. Native:
`pkgmeta-section`, `pkgmeta-entry`, `pkgmeta-key`, `pkgmeta-find`.

## Unsupported / honest cost

An out-of-range section/entry, an absent key, and an unsupported common pair
decline typed (exit 6); a malformed native key reference is a usage error
(exit 2); generic JSON/TOML controls and prose stay `Json`/`Toml`/`Opaque`. A
package resolver and any rendering are **not claimed**.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted
manifest, and after the source **and** descriptor are deleted in a fresh process.
Exactness is the whole source (a RAW-like `DocumentExact`): the manifest model is
derived (`Q_gen`) and never on the exactness path (ADR-0060: the model node
depends on the `sha256(source)` root).

**Adapter-court H1:** **8/8 byte-exact** (5 pkgmeta fixtures — `package.json`,
`Cargo.toml`, `pyproject.toml`, `Cargo.lock`, `package-lock.json` — + 3
detection-boundary controls: generic JSON, generic TOML, prose; 1 typed decline;
1 usage row).

## Economic court

Measured by `tools/phase21-29-pkgmeta-econ-court.sh` (a thin wrapper over the
shared `textfmt-court.sh` engine) against two comparators — a **source-retaining
SQLite store** with conventional extraction, and a conventional
**decode-to-host-values load** (`conv`). Corpus 6 fixtures, questions Q1–Q12,
exactness **9/9**. Estimator = paired per-fixture ratio VOLE/comparator, median +
geometric mean with a fixed-seed (2131), fixture-clustered 10000-resample 95 %
CI, tie band ±10 %; ratio-of-sums reported separately. A ratio < 1 favours VOLE.

| metric | comparator | median | geomean | median 95% CI | geomean 95% CI | W/T/L | ratio-of-sums |
|---|---|---:|---:|---|---|---:|---:|
| build | sqlite | 0.723 | 0.745 | 0.710..0.806 | 0.715..0.789 | 6/0/0 | 0.748 |
| build | conv | 0.915 | 0.911 | 0.892..0.926 | 0.894..0.923 | 1/5/0 | 0.911 |
| storage | sqlite | 0.606 | 0.650 | 0.596..0.782 | 0.598..0.761 | 5/1/0 | 0.932 |
| storage | conv | 15.841 | 11.515 | 8.441..18.387 | 5.402..17.861 | 0/0/6 | 1.926 |
| cold | sqlite | 0.024 | 0.029 | 0.023..0.051 | 0.023..0.043 | 6/0/0 | 0.033 |
| cold | conv | 0.025 | 0.031 | 0.025..0.055 | 0.025..0.046 | 6/0/0 | 0.035 |
| warm | sqlite | 0.109 | 0.148 | 0.105..0.404 | 0.106..0.276 | 6/0/0 | 0.506 |
| warm | conv | 1.292 | 1.236 | 1.098..1.343 | 1.105..1.326 | 0/1/5 | 0.965 |

Build is a win on both lanes; storage is ~0.61× SQLite but a **recorded loss vs
the conventional load**; cold is a win; warm is ~0.11× SQLite but a **recorded
loss vs the conventional in-process load**.

## Honest negatives

* **Boundary (recorded):** a generic JSON `name`+`version` stays `Json`; a
  generic TOML `name`/`version` with no `[package]` table stays `Toml`; a
  workspace-only manifest stays `Toml`.
* **Storage-vs-`conv` and warm-vs-`conv` are recorded losses.**
* Only exact closure (Q6) is a byte-authority claim; the **conventional load is
  deliberately the weaker comparator**, and a span-preserving loader could in
  principle match VOLE on the representation axes.
* The **cold ratios** are dominated by the comparators' fresh-process **Python
  start-up**.
* The economic court is a **self-authored deterministic corpus** (by
  `tools/fixtures/make-pkgmeta.py`), not a real-world manifest population.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.29.1 package-metadata adapter court (exactness 8/8):
  [2026-10-10-phase21-29-1-pkgmeta-867eb792](../../evidence/campaigns/2026-10-10-phase21-29-1-pkgmeta-867eb792/).
- Phase 21.29 package-metadata economic court (exactness 9/9):
  [2026-10-11-phase21-29-pkgmeta-econ-372a86e0](../../evidence/campaigns/2026-10-11-phase21-29-pkgmeta-econ-372a86e0/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
