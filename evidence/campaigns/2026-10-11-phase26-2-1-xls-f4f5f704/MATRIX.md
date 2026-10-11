# Phase 26.2.1 legacy Excel (.xls) court — matrix

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `xls_truncated.xls` | xls | - | - | true | - | - | - | 53 |
| `xls_bad_sst_count.xls` | xls | - | - | true | - | - | - | 53 |
| `xls_bad_boundsheet.xls` | xls | - | - | true | - | - | - | 53 |
| `xls_malformed_continue.xls` | xls | - | - | true | - | - | - | 53 |
| `xls_bad_bof.xls` | cfbf | - | - | true | - | - | - | 6 |
| `xls_bad_type.xls` | cfbf | - | - | true | - | - | - | 6 |
| `generic_cfbf.bin` | cfbf | - | - | true | - | - | - | 6 |
| `prose.txt` | opaque | - | - | true | - | - | - | 6 |
| `xls_valid.xls` | xls | 2560 | 2560 | true | true | true | - | - |
