# Phase 21.20.1 config court — matrix

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `ini.ini` | config | 44 | 44 | true | true | true | 6 | -1 |
| `env.env` | config | 38 | 38 | true | true | true | 6 | -1 |
| `props.properties` | config | 51 | 51 | true | true | true | 6 | -1 |
| `dup.ini` | config | 16 | 16 | true | true | true | 6 | -1 |
| `strict.json` | json | 21 | 21 | true | true | true | -1 | 6 |
| `table.csv` | csv | 8 | 8 | true | true | true | -1 | 6 |
| `doc.toml` | toml | 12 | 12 | true | true | true | -1 | 6 |
| `overlap.env` | opaque | 16 | 16 | true | true | true | -1 | 6 |
| `prose.txt` | opaque | 70 | 70 | true | true | true | -1 | 6 |
| `script.sh` | opaque | 25 | 25 | true | true | true | -1 | 6 |
| `badblock.ini` | opaque | 25 | 25 | true | true | true | -1 | 6 |
