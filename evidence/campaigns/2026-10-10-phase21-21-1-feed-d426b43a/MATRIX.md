# Phase 21.21.1 feed court — matrix

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `rss.xml` | feed | 466 | 466 | true | true | true | 6 | -1 |
| `atom.atom` | feed | 518 | 518 | true | true | true | 6 | -1 |
| `plain.xml` | xml | 43 | 43 | true | true | true | -1 | 6 |
| `nochannel.xml` | xml | 25 | 25 | true | true | true | -1 | 6 |
| `nons.atom` | xml | 38 | 38 | true | true | true | -1 | 6 |
| `noentry.atom` | xml | 59 | 59 | true | true | true | -1 | 6 |
| `prose.txt` | opaque | 70 | 70 | true | true | true | -1 | 6 |
