# Phase 21.7.1 CSV court — matrix

| fixture | format | src len | out len | src sha256 | sha256 match | cmp | exact | decline rc | opaque rc |
| --- | --- | ---: | ---: | --- | --- | --- | --- | ---: | ---: |
| `basic.csv` | csv | 43 | 43 | `84a071172374` | true | true | true | 6 | -1 |
| `quoted.csv` | csv | 57 | 57 | `4bd04183959e` | true | true | true | 6 | -1 |
| `crlf.csv` | csv | 54 | 54 | `bce76e2e2db1` | true | true | true | 6 | -1 |
| `bom.csv` | csv | 15 | 15 | `c343c48b2c2a` | true | true | true | 6 | -1 |
| `ragged.csv` | csv | 45 | 45 | `fd9dc65ce5f8` | true | true | true | 6 | -1 |
| `tsv.tsv` | csv | 49 | 49 | `b4e81925df9e` | true | true | true | 6 | -1 |
| `large.csv` | csv | 52428807 | 52428807 | `b86501e451d0` | true | true | true | 6 | -1 |
| `plain.txt` | opaque | 112 | 112 | `eaa754b7545f` | true | true | true | -1 | 6 |
| `malformed.csv` | opaque | 9 | 9 | `d6eeeaf202ac` | true | true | true | -1 | 6 |
| `onecol.csv` | opaque | 17 | 17 | `4fdbc441ea7b` | true | true | true | -1 | 6 |
