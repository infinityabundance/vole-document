# Phase 21.12.1 JSONL court — matrix

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `basic.ndjson` | jsonl | 245 | 245 | true | true | true | 6 | -1 |
| `shapes.ndjson` | jsonl | 88 | 88 | true | true | true | 6 | -1 |
| `unicode.ndjson` | jsonl | 93 | 93 | true | true | true | 6 | -1 |
| `crlf.ndjson` | jsonl | 29 | 29 | true | true | true | 6 | -1 |
| `blank.ndjson` | jsonl | 20 | 20 | true | true | true | 6 | -1 |
| `large.ndjson` | jsonl | 2097199 | 2097199 | true | true | true | 6 | -1 |
| `single.json` | json | 48 | 48 | true | true | true | -1 | 6 |
| `malformed.ndjson` | opaque | 13 | 13 | true | true | true | -1 | 6 |
| `bag.ndjson` | opaque | 15 | 15 | true | true | true | -1 | 6 |
