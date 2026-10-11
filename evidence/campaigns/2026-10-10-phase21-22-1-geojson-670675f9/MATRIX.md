# Phase 21.22.1 GeoJSON court — matrix

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
