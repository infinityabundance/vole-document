# Phase 21.19.1 MessagePack court — matrix

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `basic.msgpack` | msgpack | 10 | 10 | true | true | true | 6 | -1 |
| `widths.msgpack` | msgpack | 8 | 8 | true | true | true | 6 | -1 |
| `floats.msgpack` | msgpack | 15 | 15 | true | true | true | 6 | -1 |
| `bytestext.msgpack` | msgpack | 10 | 10 | true | true | true | 6 | -1 |
| `dupkeys.msgpack` | msgpack | 10 | 10 | true | true | true | 6 | -1 |
| `ext.msgpack` | msgpack | 12 | 12 | true | true | true | 6 | -1 |
| `map16.msgpack` | msgpack | 6 | 6 | true | true | true | 6 | -1 |
| `strict.json` | json | 21 | 21 | true | true | true | -1 | 6 |
| `control.cbor` | cbor | 12 | 12 | true | true | true | -1 | 6 |
| `single.msgpack` | opaque | 1 | 1 | true | true | true | -1 | 6 |
| `scalar.msgpack` | opaque | 2 | 2 | true | true | true | -1 | 6 |
| `badc1.msgpack` | opaque | 1 | 1 | true | true | true | -1 | 6 |
| `badmap.msgpack` | opaque | 3 | 3 | true | true | true | -1 | 6 |
| `trailing.msgpack` | opaque | 2 | 2 | true | true | true | -1 | 6 |
| `fixarray3.bin` | opaque | 4 | 4 | true | true | true | -1 | 6 |
| `fixmap2.bin` | opaque | 5 | 5 | true | true | true | -1 | 6 |
| `prose.txt` | opaque | 75 | 75 | true | true | true | -1 | 6 |
