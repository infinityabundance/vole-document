# Phase 21.35.1 CFBF/OLE2 court — matrix

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `cfbf_badtype.cfb` | cfbf | - | - | true | - | - | - | 51 |
| `cfbf_cyclic.cfb` | cfbf | - | - | true | - | - | - | 51 |
| `cfbf_truncated_stream.cfb` | cfbf | - | - | true | - | - | - | 51 |
| `cfbf_bad_dir_sector.cfb` | opaque | - | - | true | - | - | - | 6 |
| `cfbf_badsig.cfb` | opaque | - | - | true | - | - | - | 6 |
| `cfbf_badshift.cfb` | opaque | - | - | true | - | - | - | 6 |
| `sig_only.bin` | opaque | - | - | true | - | - | - | 6 |
| `prose.txt` | opaque | - | - | true | - | - | - | 6 |
| `cfbf_v3.cfb` | cfbf | 8192 | 8192 | true | true | true | - | - |
| `cfbf_v4.cfb` | cfbf | 28672 | 28672 | true | true | true | - | - |
