# GeoJSON

GeoJSON (RFC 7946) is the **spatial** format of Phase 21 Wave 2 (subphase 21.22).
A GeoJSON document's physical bytes are JSON, so it is not a package — the whole
source is the document, and the exact leaf is the source. The adapter **reuses
the shared representation-preserving JSON parser** (never a second parser) and is
gated behind the **non-default** `geojson = ["json"]` feature.

## Authority boundary

A GeoJSON document's physical bytes are JSON, so detection is a **bounded
semantic sub-detection** run **before** the generic JSON detector. A document is
claimed only when the root object's string `"type"` is one of the nine RFC 7946
type names **and** the shape is consistent (a geometry has an array
`coordinates`/`geometries`; a `Feature` has `geometry`+`properties`; a
`FeatureCollection` has an array `features`). A plain JSON document, a JSON
document whose `"type"` is unrelated, and a GeoJSON type name with no consistent
shape all stay `Json`; prose stays `Opaque` and round-trips exactly through the
RAW lane.

**What it cannot distinguish.** A structure that is *shaped* like GeoJSON but is
numerically invalid under RFC 7946 (a position that is not a `[lon, lat, (alt)]`
array of numbers, an unclosed ring, a wrong coordinate arity) is still claimed
and preserved **verbatim** — the adapter does not validate the positional
grammar, so it neither normalizes nor rejects such a document. CRS handling
(removed by RFC 7946) is not claimed.

## Representation preservation (the point of the format)

The GeoJSON model embeds the JSON node arena (kind, exact `[start, end)` token
span, ordered children), so it preserves the exact `"type"` token; `coordinates`
nesting with each number's exact span and literal spelling (never reparsed);
`properties` order and **duplicate keys**; `id`/`bbox`/`geometry`/`features`
order; and every foreign (non-core) member.

## Supported observations

Common selectors: `metadata`, `text`, `search-match`. Native: `geojson-type`,
`geojson-feature`, `geojson-geometry`, `geojson-coordinates`,
`geojson-property`, `geojson-find`.

## Unsupported / honest cost

Coordinate validation, CRS/projection handling, and geometry predicates
(intersects/contains/area) are not claimed. An out-of-range feature/geometry, an
unknown property, a GeometryCollection's coordinates, a malformed
`--geojson-property` argument, a cap breach, and a non-GeoJSON source are typed
declines (`InvalidGeojsonStructure`, exit 36; `unsupported-feature`, exit 6;
`resource_limit`, exit 8; usage, exit 2); `Page(n)` is a typed decline.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted
GeoJSON, and after the source **and** descriptor are deleted in a fresh process.
The exact leaf is the whole source; the derived model is never on the exactness
path (ADR-0060: the model node depends on the `DocumentExact` root keyed on
`sha256(source)`). Adapter-court H1 exactness **8/8** (4 GeoJSON fixtures + 3
JSON controls + 1 opaque control).

## Economic court

Measured by `tools/phase21-22-geojson-court.sh` (a thin wrapper over the shared
`textfmt-court.sh` engine) against two conventional comparators — a
**source-retaining store** plus conventional extraction, and a conventional
**decode-to-host-values load**. Corpus 7 fixtures, questions Q1–Q12, exactness
**11/11**. Estimator = paired per-fixture ratio VOLE/comparator, median +
geometric mean with a fixed-seed, fixture-clustered 95 % CI; ratio-of-sums
reported separately. A ratio < 1 favours VOLE.

| metric | comparator | median | geomean | median 95% CI | geomean 95% CI | W/T/L | ratio-of-sums |
|---|---|---:|---:|---|---|---:|---:|
| build | sqlite | 1.013 | 0.991 | 0.939..1.014 | 0.960..1.030 | 0/7/0 | 0.993 |
| build | conv | 0.943 | 0.960 | 0.940..1.033 | 0.903..1.009 | 1/6/0 | 0.950 |
| storage | sqlite | 0.613 | 0.655 | 0.603..0.668 | 0.608..0.739 | 6/1/0 | 0.899 |
| storage | conv | 0.601 | 0.610 | 0.594..0.633 | 0.597..0.624 | 7/0/0 | 0.632 |
| cold | sqlite | 0.030 | 0.034 | 0.029..0.031 | 0.029..0.047 | 7/0/0 | 0.039 |
| warm | sqlite | 0.090 | 0.100 | 0.079..0.096 | 0.080..0.142 | 7/0/0 | 0.227 |
| warm | conv | 1.097 | 1.012 | 0.968..1.149 | 0.847..1.138 | 1/3/3 | 0.640 |

Build is ~parity with SQLite (median 1.013); storage ~0.61× both; warm ~0.09×
SQLite but ~parity vs the conventional in-process load (median 1.097, a recorded
mixed result).

## Found-and-fixed quality finding (DEFECT FIX)

The GeoJSON economic court found a **real defect**: **7 GeoJSON observation
projections emitted invalid JSON** (a stray `"`). It was fixed in
`src/field/observe.rs` (commit `ba4df0d4`, "test(21.20-21.24): consolidated
profile-format economic courts + fix GeoJSON JSON validity"). The defect was
found by **parsing the observation outputs** in the economic court; the adapter
court's substring assertions had missed it. This is recorded as a
found-and-fixed quality finding, not hidden.

## Honest negatives

* Numerically-invalid-but-shaped GeoJSON is claimed and preserved verbatim (no
  positional validation).
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

- Phase 21.22.1 GeoJSON adapter court: `tools/phase21-22-1-geojson-court.sh`
  (exactness 8/8); campaign
  [2026-10-10-phase21-22-1-geojson-670675f9](../../evidence/campaigns/2026-10-10-phase21-22-1-geojson-670675f9/).
- Phase 21.22 economic court: `tools/phase21-22-geojson-court.sh` (exactness
  11/11; found the JSON-validity defect); campaign
  [2026-10-10-phase21-22-geojson-econ-735d9b69](../../evidence/campaigns/2026-10-10-phase21-22-geojson-econ-735d9b69/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
