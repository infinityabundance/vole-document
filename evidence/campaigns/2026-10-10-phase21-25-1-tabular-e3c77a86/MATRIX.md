# Phase 21.25.1 tabular-extra court — matrix

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `basic.fw` | fixedwidth | 33 | 33 | true | true | true | 6 | 6 |
| `crlf.fw` | fixedwidth | 36 | 36 | true | true | true | 6 | 6 |
| `three.fw` | fixedwidth | 42 | 42 | true | true | true | 6 | 6 |
| `bom.fw` | fixedwidth | 36 | 36 | true | true | true | 6 | 6 |
| `comma.csv` | csv | 25 | 25 | true | true | true | -1 | -1 |
| `tab.tsv` | csv | 25 | 25 | true | true | true | -1 | -1 |
| `pipe.psv` | csv | 39 | 39 | true | true | true | -1 | -1 |
| `markdown.md` | markdown | 34 | 34 | true | true | true | -1 | -1 |
| `prose.txt` | opaque | 59 | 59 | true | true | true | -1 | 6 |
| `spaced.txt` | opaque | 30 | 30 | true | true | true | -1 | 6 |
