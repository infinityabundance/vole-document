# Phase 26.4.1 legacy Outlook message (.msg) court — matrix

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `msg_short_props.msg` | msg | - | - | true | - | - | - | 55 |
| `msg_entry_overrun.msg` | msg | - | - | true | - | - | - | 55 |
| `msg_bad_substream_name.msg` | msg | - | - | true | - | - | - | 55 |
| `msg_unreferenced_substream.msg` | msg | - | - | true | - | - | - | 55 |
| `msg_unsupported_substream.msg` | msg | - | - | true | - | - | - | 55 |
| `msg_count_mismatch.msg` | msg | - | - | true | - | - | - | 55 |
| `msg_recip_no_props.msg` | msg | - | - | true | - | - | - | 55 |
| `msg_bad_recip_suffix.msg` | msg | - | - | true | - | - | - | 55 |
| `generic_cfbf.bin` | cfbf | - | - | true | - | - | - | 6 |
| `msg_missing_props.msg` | cfbf | - | - | true | - | - | - | 6 |
| `prose.txt` | opaque | - | - | true | - | - | - | 6 |
| `msg_valid.msg` | msg | 5120 | 5120 | true | true | true | - | - |
