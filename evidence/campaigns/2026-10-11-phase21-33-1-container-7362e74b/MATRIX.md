# Phase 21.33.1 archive-container court — matrix

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `office.docx` | docx | - | - | true | - | - | - | 6 |
| `office.odt` | odt | - | - | true | - | - | - | 6 |
| `pk_junk.bin` | opaque | - | - | true | - | - | - | 6 |
| `prose.txt` | opaque | - | - | true | - | - | - | 6 |
| `bad_eocd.zip` | opaque | - | - | true | - | - | - | 6 |
| `bad_checksum.tar` | opaque | - | - | true | - | - | - | 6 |
| `truncated.zip` | opaque | - | - | true | - | - | - | 6 |
| `truncated.tar` | container | - | - | true | - | - | - | 49 |
| `zip_stored.zip` | container | 132 | 132 | true | true | true | - | - |
| `zip_multi.zip` | container | 505 | 505 | true | true | true | - | - |
| `zip_nonutf8.zip` | container | 125 | 125 | true | true | true | - | - |
| `zip_empty.zip` | container | 22 | 22 | true | true | true | - | - |
| `tar_ustar.tar` | container | 3584 | 3584 | true | true | true | - | - |
| `tar_extended.tar` | container | 5632 | 5632 | true | true | true | - | - |
