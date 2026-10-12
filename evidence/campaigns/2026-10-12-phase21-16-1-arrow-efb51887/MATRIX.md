# Phase 21.16.1 Arrow IPC court — matrix

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `primitives.arrow` | arrow | 2778 | 2778 | true | true | true | -1 | -1 |
| `nullable.arrow` | arrow | 994 | 994 | true | true | true | -1 | -1 |
| `strings.arrow` | arrow | 1546 | 1546 | true | true | true | -1 | -1 |
| `temporal.arrow` | arrow | 1170 | 1170 | true | true | true | -1 | -1 |
| `multi_batch.arrow` | arrow | 1378 | 1378 | true | true | true | -1 | -1 |
| `fsb.arrow` | arrow | 490 | 490 | true | true | true | -1 | -1 |
| `stream.arrow` | arrow | 712 | 712 | true | true | true | -1 | -1 |
| `large.arrow` | arrow | 1502626 | 1502626 | true | true | true | -1 | -1 |
| `unsupported_decimal.arrow` | arrow | 578 | 578 | true | true | true | 6 | -1 |
| `unsupported_nested.arrow` | arrow | 690 | 690 | true | true | true | 6 | -1 |
| `unsupported_dictionary.arrow` | arrow | 594 | 594 | true | true | true | 6 | -1 |
| `unsupported_compressed.arrow` | arrow | 530 | 530 | true | true | true | 6 | -1 |
| `bomb.arrow` | arrow | 490 | 490 | true | true | true | 8 | -1 |
| `malformed_flatbuf.arrow` | arrow | 82 | 82 | true | true | true | 30 | -1 |
| `prose.txt` | opaque | 63 | 63 | true | true | true | -1 | 6 |
| `magic_only.bin` | opaque | 27 | 27 | true | true | true | -1 | 6 |
| `truncated.arrow` | opaque | 64 | 64 | true | true | true | -1 | 6 |
| `badlen.arrow` | opaque | 2778 | 2778 | true | true | true | -1 | 6 |
