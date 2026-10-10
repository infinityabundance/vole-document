# Phase 21.16.1 Arrow IPC court — matrix

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `primitives.arrow` | arrow | 2786 | 2786 | true | true | true | -1 | -1 |
| `nullable.arrow` | arrow | 986 | 986 | true | true | true | -1 | -1 |
| `strings.arrow` | arrow | 1474 | 1474 | true | true | true | -1 | -1 |
| `temporal.arrow` | arrow | 1194 | 1194 | true | true | true | -1 | -1 |
| `multi_batch.arrow` | arrow | 1410 | 1410 | true | true | true | -1 | -1 |
| `fsb.arrow` | arrow | 514 | 514 | true | true | true | -1 | -1 |
| `stream.arrow` | arrow | 712 | 712 | true | true | true | -1 | -1 |
| `large.arrow` | arrow | 1502738 | 1502738 | true | true | true | -1 | -1 |
| `unsupported_decimal.arrow` | arrow | 602 | 602 | true | true | true | 6 | -1 |
| `unsupported_nested.arrow` | arrow | 714 | 714 | true | true | true | 6 | -1 |
| `unsupported_dictionary.arrow` | arrow | 586 | 586 | true | true | true | 6 | -1 |
| `unsupported_compressed.arrow` | arrow | 554 | 554 | true | true | true | 6 | -1 |
| `bomb.arrow` | arrow | 514 | 514 | true | true | true | 8 | -1 |
| `malformed_flatbuf.arrow` | arrow | 82 | 82 | true | true | true | 30 | -1 |
| `prose.txt` | opaque | 63 | 63 | true | true | true | -1 | 6 |
| `magic_only.bin` | opaque | 27 | 27 | true | true | true | -1 | 6 |
| `truncated.arrow` | opaque | 64 | 64 | true | true | true | -1 | 6 |
| `badlen.arrow` | opaque | 2786 | 2786 | true | true | true | -1 | 6 |
