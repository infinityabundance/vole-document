# Phase 21.23.1 KML/GPX court — matrix

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
