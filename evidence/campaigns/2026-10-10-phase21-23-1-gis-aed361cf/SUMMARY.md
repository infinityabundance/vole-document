# Phase 21.23.1 — KML 2.2 / GPX 1.1 court

**Question.** Does the KML/GPX adapter close exactly and expose a
representation-preserving model (the recorded dialect; exact element/attribute
spans; element order; attribute spelling — KML geometry `<coordinates>`, GPX
`<trkpt lat=… lon=…>`; the namespace declaration) on top of the whole-source
exact leaf — while keeping the bounded semantic sub-detection boundary (before
the generic XML detector) and declining malformed/ambiguous inputs typed?

**Method.** Each self-authored fixture is generated in-court with GNU
`/usr/bin/printf`, ingested via `field-build`, observed, then — after the
**source file and the standalone descriptor are deleted** — rematerialized
exactly in a fresh process and compared with `length + SHA-256 + cmp`. An
out-of-range record/point and an unknown root field are required to decline
typed; the plain-XML and shaped-but-invalid controls and prose pin the detection
boundaries. The court runs in the pinned `dev` service using only POSIX `sh`,
coreutils, git, and the shipped binary (no python3, no jq).

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `kml.kml` | gis | 378 | 378 | true | true | true | 6 | -1 |
| `gpx.gpx` | gis | 416 | 416 | true | true | true | 6 | -1 |
| `plain.xml` | xml | 43 | 43 | true | true | true | -1 | 6 |
| `no-ns.kml` | xml | 44 | 44 | true | true | true | -1 | 6 |
| `no-child.kml` | xml | 45 | 45 | true | true | true | -1 | 6 |
| `no-ns.gpx` | xml | 33 | 33 | true | true | true | -1 | 6 |
| `no-child.gpx` | xml | 48 | 48 | true | true | true | -1 | 6 |
| `prose.txt` | opaque | 43 | 43 | true | true | true | -1 | 6 |

## Counts

```
fixtures 8
gis_fixtures 2
exact_ok 8
exact_fail 0
surface_fail 0
typed_decline_records_ok 2
typed_decline_points_ok 2
typed_decline_fields_ok 2
xml_controls_ok 5
xml_controls 5
usage_ok 1
opaque_controls_ok 1
opaque_controls 1
binary 8d3004495cad5569a760bf77aa8594a33be9f6c769a03f4bc723da5c1bd2a0c7
```

## Per-fixture observation files (raw/)

- `gpx.gpx.find.json`
- `gpx.gpx.metadata.json`
- `gpx.gpx.point0.json`
- `gpx.gpx.point2.json`
- `gpx.gpx.record0.json`
- `gpx.gpx.root.json`
- `gpx.gpx.search.json`
- `gpx.gpx.text.json`
- `kml.kml.field.json`
- `kml.kml.find.json`
- `kml.kml.metadata.json`
- `kml.kml.point0.json`
- `kml.kml.recfield.bin`
- `kml.kml.recfield.json`
- `kml.kml.record0.json`
- `kml.kml.root.json`
- `kml.kml.search.json`
- `kml.kml.text.json`
- `no-child.gpx.gis-decline.json`
- `no-child.kml.gis-decline.json`
- `no-ns.gpx.gis-decline.json`
- `no-ns.kml.gis-decline.json`
- `plain.xml.gis-decline.json`
- `prose.txt.metadata.json`
- `results.tsv`

## Scope (honest)

- **Shipped here:** byte-based bounded semantic KML/GPX detection; one bounded
  GIS model **reusing the shared XML parser** (never a second XML parser); native
  `gis-root`/`gis-field`/`gis-record`/`gis-record-field`/`gis-point`/
  `gis-find`; common `metadata`/`text`/`search-match`.
- **Precedence:** after the strong magic-byte binaries (PDF/ZIP/Parquet/Arrow),
  the JSON family, the binary structured-tree family, config, and feed;
  immediately before the generic standalone-XML detector. A KML/GPX document is
  a more specific claim than a bare XML tree, so it is tried first.
- **Detection boundary:** KML and GPX are XML. A document is claimed only when a
  `<kml>` root's bound namespace prefix is the OGC KML 2.2 URI **and** it has a
  `Document`/`Folder`/`Placemark` child, or a `<gpx>` root's bound namespace
  prefix is the GPX 1.1 URI **and** it has a `metadata`/`wpt`/`rte`/`trk`
  child. A plain XML document and a shaped-but-invalid `<kml>`/`<gpx>` stay
  `Xml`; prose stays `Opaque`.
- **Recorded negatives (not distinguished):** older KML namespace versions
  (2.0/2.1, and the pre-OGC Google Earth namespaces) and GPX 1.0 use different
  namespace URIs, so they are **not** claimed and stay `Xml`; only the KML 2.2
  and GPX 1.1 URIs are recognized. A non-default bound prefix (e.g.
  `<k:kml xmlns:k="…/kml/2.2">`) **is** recognized. The KML `<coordinates>`
  and GPX `lat`/`lon` positional grammars are **not** validated; a
  numerically-invalid shape is claimed and preserved verbatim.
- **Declines:** an out-of-range record/point, an unknown root field, a malformed
  `--gis-record-field` argument, a cap breach, and a non-geospatial source are
  typed (`InvalidGisStructure` rc 37, unsupported-feature rc 6, resource-limit
  rc 8, or usage rc 2); such input stays `Xml`/`Opaque` when detection
  declines.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the GIS model
  is derived (`Q_gen`) and never on the exactness path (ADR-0060: the model node
  depends on the `sha256(source)` root).
- **Not claimed here:** coordinate validation, CRS/projection handling, the
  economic court (separate script).
- **Never run on the host:** every command above ran in the pinned `dev`
  container.
