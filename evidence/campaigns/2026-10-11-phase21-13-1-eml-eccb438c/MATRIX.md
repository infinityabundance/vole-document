# Phase 21.13.1 EML court — matrix

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `simple.eml` | eml | 201 | 201 | true | true | true | 6 | -1 |
| `mixed.eml` | eml | 626 | 626 | true | true | true | 6 | -1 |
| `alternative.eml` | eml | 380 | 380 | true | true | true | 6 | -1 |
| `nested.eml` | eml | 402 | 402 | true | true | true | 6 | -1 |
| `folded.eml` | eml | 395 | 395 | true | true | true | 6 | -1 |
| `large.eml` | eml | 2053092 | 2053092 | true | true | true | 6 | -1 |
| `prose.txt` | opaque | 129 | 129 | true | true | true | -1 | 6 |
