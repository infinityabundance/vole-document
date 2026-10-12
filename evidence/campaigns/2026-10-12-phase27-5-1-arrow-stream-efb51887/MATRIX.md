# Phase 27.5.1 Arrow IPC stream court — matrix

| fixture | detected | src len | out len | sha256 match | cmp | exact | want |
| --- | --- | ---: | ---: | --- | --- | --- | --- |
| `stream.arrow` | arrow | 712 | 712 | true | true | true | arrow |
| `primitives.arrow` | arrow | 2778 | 2778 | true | true | true | arrow |
| `junk_cont.bin` | opaque | 128 | n/a | false | n/a | n/a | opaque |
| `truncated_stream.bin` | opaque | 64 | n/a | false | n/a | n/a | opaque |
| `nonschema_first.bin` | opaque | 520 | n/a | false | n/a | n/a | opaque |
| `legacy_stream.bin` | opaque | 196 | n/a | false | n/a | n/a | opaque |
| `prose.txt` | opaque | 63 | n/a | false | n/a | n/a | opaque |
