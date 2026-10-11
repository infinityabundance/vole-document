# Phase 21.18.1 CBOR court — matrix

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `basic.cbor` | cbor | 12 | 12 | true | true | true | 6 | -1 |
| `widths.cbor` | cbor | 4 | 4 | true | true | true | 6 | -1 |
| `floats.cbor` | cbor | 18 | 18 | true | true | true | 6 | -1 |
| `tags.cbor` | cbor | 12 | 12 | true | true | true | 6 | -1 |
| `bytestext.cbor` | cbor | 9 | 9 | true | true | true | 6 | -1 |
| `dupkeys.cbor` | cbor | 7 | 7 | true | true | true | 6 | -1 |
| `indef.cbor` | cbor | 13 | 13 | true | true | true | 6 | -1 |
| `large.cbor` | cbor | 4003 | 4003 | true | true | true | 6 | -1 |
| `strict.json` | json | 21 | 21 | true | true | true | -1 | 6 |
| `single.cbor` | opaque | 1 | 1 | true | true | true | -1 | 6 |
| `scalar.cbor` | opaque | 2 | 2 | true | true | true | -1 | 6 |
| `badmap.cbor` | opaque | 4 | 4 | true | true | true | -1 | 6 |
| `unterm.cbor` | opaque | 4 | 4 | true | true | true | -1 | 6 |
| `msgpack_fixarray.bin` | opaque | 4 | 4 | true | true | true | -1 | 6 |
| `msgpack_fixmap.bin` | opaque | 5 | 5 | true | true | true | -1 | 6 |
| `prose.txt` | opaque | 68 | 68 | true | true | true | -1 | 6 |
