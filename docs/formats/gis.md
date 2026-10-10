# KML / GPX

KML 2.2 / GPX 1.1 is the **geospatial** format of Phase 21 Wave 2 (subphase
21.23). KML and GPX are XML, so it is not a package — the whole source is the
document, and the exact leaf is the source. One bounded adapter covers both
dialects and **reuses the shared bounded XML parser**; it is gated behind the
**non-default** `gis = ["xml"]` feature.

## Authority boundary

A geospatial document's physical bytes are XML, so detection is a **bounded
semantic sub-detection** run **before** the generic standalone-XML detector. A
document is claimed only when a `<kml>` root's bound namespace prefix is the OGC
KML 2.2 URI (`http://www.opengis.net/kml/2.2`) **and** it has a
`Document`/`Folder`/`Placemark` child, or a `<gpx>` root's bound namespace prefix
is the GPX 1.1 URI (`http://www.topografix.com/GPX/1/1`) **and** it has a
`metadata`/`wpt`/`rte`/`trk` child. A plain XML document and a shaped-but-invalid
`<kml>`/`<gpx>` stay `Xml`; prose stays `Opaque` and round-trips exactly through
the RAW lane.

**What it cannot distinguish.** **Older namespaces stay `Xml`**: older KML
versions (2.0/2.1, and the pre-OGC Google Earth namespaces) and GPX 1.0 use
different namespace URIs, so they are not claimed; only the KML 2.2 and GPX 1.1
URIs are recognized. A non-default bound prefix (e.g. `<k:kml
xmlns:k="…/kml/2.2">`) **is** recognized. The KML `<coordinates>` and GPX
`lat`/`lon` positional grammars are **not** validated; a numerically-invalid
shape is claimed and preserved verbatim. CRS/projection handling is not claimed.

## Representation preservation (the point of the format)

A bounded geospatial projection over the shared XML element/attribute tree
preserves the recorded dialect (`kml`/`gpx`); exact element/attribute spans;
element order; attribute spelling (KML geometry `<coordinates>`, GPX `<trkpt
lat=… lon=…>`); and the namespace declaration.

## Supported observations

Common selectors: `metadata`, `text`, `search-match`. Native: `gis-root`,
`gis-field`, `gis-record`, `gis-record-field`, `gis-point`, `gis-find`.

## Unsupported / honest cost

Coordinate validation, CRS/projection, and geometry predicates are not claimed.
An out-of-range record/point, an unknown root field, a malformed
`--gis-record-field` argument, a cap breach, and a non-geospatial source are
typed declines (`InvalidGisStructure`, exit 37; `unsupported-feature`, exit 6;
`resource_limit`, exit 8; usage, exit 2); `Page(n)` is a typed decline.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted
KML/GPX, and after the source **and** descriptor are deleted in a fresh process.
The exact leaf is the whole source; the derived model is never on the exactness
path (ADR-0060: the model node depends on the `DocumentExact` root keyed on
`sha256(source)`). Adapter-court H1 exactness **8/8** (2 GIS fixtures + 5 plain-XML
and shaped-but-invalid controls + 1 opaque control).

## Economic court

Measured by `tools/phase21-23-gis-court.sh` (a thin wrapper over the shared
`textfmt-court.sh` engine) against two conventional comparators — a
**source-retaining store** plus conventional extraction, and a conventional
**decode-to-host-values load**. Corpus 6 fixtures, questions Q1–Q12, exactness
**10/10**. Estimator = paired per-fixture ratio VOLE/comparator, median +
geometric mean with a fixed-seed, fixture-clustered 95 % CI; ratio-of-sums
reported separately. A ratio < 1 favours VOLE.

| metric | comparator | median | geomean | median 95% CI | geomean 95% CI | W/T/L | ratio-of-sums |
|---|---|---:|---:|---|---|---:|---:|
| build | sqlite | 0.951 | 0.955 | 0.871..1.055 | 0.892..1.022 | 1/5/0 | 0.949 |
| build | conv | 0.953 | 0.922 | 0.787..1.064 | 0.787..1.031 | 1/5/0 | 0.895 |
| storage | sqlite | 0.650 | 0.661 | 0.634..0.702 | 0.639..0.694 | 6/0/0 | 0.737 |
| storage | conv | 0.610 | 0.587 | 0.528..0.632 | 0.529..0.624 | 6/0/0 | 0.463 |
| cold | sqlite | 0.029 | 0.036 | 0.028..0.066 | 0.028..0.054 | 6/0/0 | 0.042 |
| warm | sqlite | 0.105 | 0.122 | 0.088..0.242 | 0.087..0.200 | 6/0/0 | 0.318 |
| warm | conv | 0.110 | 0.128 | 0.090..0.255 | 0.092..0.208 | 6/0/0 | 0.329 |

Build is ~parity/slightly faster (median 0.95, mostly ties); storage ~0.65×
SQLite and ~0.61× the conventional load; warm ~0.11× both.

## Honest negatives

* **Older namespaces stay `Xml`**: KML 2.0/2.1 and GPX 1.0 are not recognized.
* A shaped-but-invalid KML/GPX (no namespace or no structural child) stays `Xml`.
* The KML/GPX positional grammars are not validated; the model preserves rather
  than interprets.
* The conventional load is deliberately the weaker comparator; a
  span-preserving loader is not built here.
* The economic court is measured on a **self-authored deterministic corpus**;
  only exact closure (Q6) is a byte-authority claim.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.23.1 KML/GPX adapter court: `tools/phase21-23-1-gis-court.sh`
  (exactness 8/8); campaign
  [2026-10-10-phase21-23-1-gis-aed361cf](../../evidence/campaigns/2026-10-10-phase21-23-1-gis-aed361cf/).
- Phase 21.23 economic court: `tools/phase21-23-gis-court.sh` (exactness 10/10);
  campaign
  [2026-10-10-phase21-23-gis-econ-735d9b69](../../evidence/campaigns/2026-10-10-phase21-23-gis-econ-735d9b69/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
