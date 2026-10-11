# Phase 21.28.1 log-stream court — matrix

| fixture | format | src len | out len | sha256 match | cmp | exact | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: |
| `generic.log` | logstream | 99 | 99 | true | true | true | -1 |
| `rfc5424.log` | logstream | 143 | 143 | true | true | true | -1 |
| `rfc3164.log` | logstream | 92 | 92 | true | true | true | -1 |
| `crlf.log` | logstream | 26 | 26 | true | true | true | -1 |
| `blank.log` | logstream | 25 | 25 | true | true | true | -1 |
| `prose.txt` | opaque | 94 | 94 | true | true | true | 6 |
| `single.json` | json | 8 | 8 | true | true | true | -1 |
| `stream.jsonl` | jsonl | 16 | 16 | true | true | true | 6 |
| `bsd_nocolon.log` | yaml | 74 | 74 | true | true | true | -1 |
