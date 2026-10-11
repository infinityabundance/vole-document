# Phase 26.3.1 legacy PowerPoint (.ppt) court — matrix

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `ppt_bad_document.ppt` | ppt | - | - | true | - | - | - | 54 |
| `ppt_overrun.ppt` | ppt | - | - | true | - | - | - | 54 |
| `ppt_text_mismatch.ppt` | ppt | - | - | true | - | - | - | 54 |
| `ppt_truncated.ppt` | ppt | - | - | true | - | - | - | 54 |
| `ppt_bad_current_user.ppt` | ppt | - | - | true | - | - | - | 54 |
| `generic_cfbf.bin` | cfbf | - | - | true | - | - | - | 6 |
| `ppt_bad_first_record.ppt` | cfbf | - | - | true | - | - | - | 6 |
| `prose.txt` | opaque | - | - | true | - | - | - | 6 |
| `ppt_valid.ppt` | ppt | 2560 | 2560 | true | true | true | - | - |
