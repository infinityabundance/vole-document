# Phase 21.22.1 — GeoJSON (RFC 7946) court

**Question.** Does the GeoJSON adapter close exactly and expose a
representation-preserving model (the exact type token; coordinates nesting
with each number's exact span/literal spelling; properties order and
duplicate keys; id/bbox/geometry/features order; foreign members) on top of
the whole-source exact leaf — while keeping the bounded semantic
sub-detection boundary (before the generic JSON detector) and declining
malformed/ambiguous inputs typed?

**Method.** Each self-authored fixture is generated in-court with GNU
`/usr/bin/printf`, ingested via `field-build`, observed, then — after the
**source file and the standalone descriptor are deleted** — rematerialized
exactly in a fresh process and compared with `length + SHA-256 + cmp`. An
out-of-range feature/geometry, an unknown property, and a GeometryCollection's
coordinates are required to decline typed; the plain-JSON / unrelated-type /
shapeless-type controls and prose pin the detection boundaries. The court runs
in the pinned `dev` service using only POSIX `sh`, coreutils, git, and the
shipped binary (no python3, no jq).

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `fc.geojson` | geojson | 341 | 341 | true | true | true | 6 | -1 |
| `point.geojson` | geojson | 42 | 42 | true | true | true | 6 | -1 |
| `line.geojson` | geojson | 73 | 73 | true | true | true | 6 | -1 |
| `gc.geojson` | geojson | 131 | 131 | true | true | true | 6 | -1 |
| `plain.json` | json | 17 | 17 | true | true | true | -1 | 6 |
| `unrelated.json` | json | 33 | 33 | true | true | true | -1 | 6 |
| `shapeless.json` | json | 16 | 16 | true | true | true | -1 | 6 |
| `prose.txt` | opaque | 47 | 47 | true | true | true | -1 | 6 |

## Counts

```
fixtures 8
geojson_fixtures 4
exact_ok 8
exact_fail 0
surface_fail 0
typed_decline_features_ok 4
typed_decline_geometries_ok 4
typed_decline_properties_ok 4
typed_decline_gc_coordinates_ok 1
json_controls_ok 3
usage_ok 1
opaque_controls_ok 1
opaque_controls 1
binary c04b5a6827bbf8983ebc94808a0d3abf37dc363dae49bdb9805fde9db434a89b
```

## Per-fixture observation files (raw/)

- `fc.geojson.coords0.json`
- `fc.geojson.feature0.json`
- `fc.geojson.find.json`
- `fc.geojson.geom1.json`
- `fc.geojson.metadata.json`
- `fc.geojson.prop.json`
- `fc.geojson.search.json`
- `fc.geojson.text.json`
- `fc.geojson.type.bin`
- `fc.geojson.type.json`
- `gc.geojson.coords-decline.json`
- `gc.geojson.find.json`
- `gc.geojson.geom0.json`
- `gc.geojson.metadata.json`
- `gc.geojson.search.json`
- `gc.geojson.text.json`
- `gc.geojson.type.bin`
- `gc.geojson.type.json`
- `line.geojson.coords0.json`
- `line.geojson.find.json`
- `line.geojson.metadata.json`
- `line.geojson.search.json`
- `line.geojson.text.json`
- `line.geojson.type.bin`
- `line.geojson.type.json`
- `plain.json.geojson-decline.json`
- `point.geojson.find.json`
- `point.geojson.metadata.json`
- `point.geojson.search.json`
- `point.geojson.text.json`
- `point.geojson.type.bin`
- `point.geojson.type.json`
- `prose.txt.metadata.json`
- `results.tsv`
- `shapeless.json.geojson-decline.json`
- `unrelated.json.geojson-decline.json`

## Scope (honest)

- **Shipped here:** byte-based bounded semantic GeoJSON detection; one bounded
  GeoJSON model **reusing the shared JSON parser** (never a second JSON
  parser); native `geojson-type`/`geojson-feature`/`geojson-geometry`/
  `geojson-coordinates`/`geojson-property`/`geojson-find`; common
  `metadata`/`text`/`search-match`.
- **Precedence:** after the strong magic-byte binaries (PDF/ZIP/Parquet/
  Arrow); immediately before the generic JSON detector. A GeoJSON document is
  a more specific claim than a bare JSON value, so it is tried first.
- **Detection boundary:** GeoJSON's physical bytes are JSON. A GeoJSON document
  is claimed only when the root object's string `"type"` is one of the nine
  RFC 7946 names **and** the shape is consistent (a geometry has an array
  `coordinates`/`geometries`; a `Feature` has `geometry`+`properties`; a
  `FeatureCollection` has an array `features`). A plain JSON document, a
  JSON document whose `"type"` is unrelated, and a GeoJSON type name with no
  consistent shape all stay `Json`; prose stays `Opaque`.
- **Recorded negative (not distinguished):** a structure that is shaped like
  GeoJSON but is numerically invalid under RFC 7946 (a position that is not a
  `[lon, lat, (alt)]` array of numbers, an unclosed ring, a wrong coordinate
  arity) is still claimed and preserved **verbatim** — the adapter does not
  validate the positional grammar, so it neither normalizes nor rejects such a
  document.
- **Declines:** an out-of-range feature/geometry, an unknown property, a
  GeometryCollection's coordinates, a malformed `--geojson-property` argument,
  a cap breach, and a non-GeoJSON source are typed
  (`InvalidGeojsonStructure` rc 36, unsupported-feature rc 6, resource-limit
  rc 8, or usage rc 2); such input stays `Json`/`Opaque` when detection
  declines.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the GeoJSON
  model is derived (`Q_gen`) and never on the exactness path (ADR-0060: the
  model node depends on the `sha256(source)` root).
- **Not claimed here:** coordinate validation, CRS handling (RFC 7946 removed
  CRS), the economic court (separate script).
- **Never run on the host:** every command above ran in the pinned `dev`
  container.
