# Phase 21.17.1 JSON5/JSONC court — matrix

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `basic.json5` | json5 | 129 | 129 | true | true | true | 6 | -1 |
| `jsonc.jsonc` | json5 | 137 | 137 | true | true | true | 6 | -1 |
| `numbers.json5` | json5 | 106 | 106 | true | true | true | 6 | -1 |
| `strings.json5` | json5 | 79 | 79 | true | true | true | 6 | -1 |
| `unicode.json5` | json5 | 51 | 51 | true | true | true | 6 | -1 |
| `comments.json5` | json5 | 98 | 98 | true | true | true | 6 | -1 |
| `large.json5` | json5 | 375294 | 375294 | true | true | true | 6 | -1 |
| `strict.json` | json | 28 | 28 | true | true | true | -1 | 6 |
| `malformed.json5` | opaque | 5 | 5 | true | true | true | -1 | 6 |
| `prose.txt` | opaque | 93 | 93 | true | true | true | -1 | 6 |
