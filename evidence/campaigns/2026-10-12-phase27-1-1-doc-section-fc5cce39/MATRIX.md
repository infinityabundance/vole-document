# Phase 27.1.1 legacy Word (.doc) section court — matrix

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `doc_bad_clx_range.doc` | doc | - | - | true | - | - | - | 52 |
| `doc_fcmin_range.doc` | doc | - | - | true | - | - | - | 52 |
| `doc_nonmonotonic.doc` | doc | - | - | true | - | - | - | 52 |
| `doc_piece_outside.doc` | doc | - | - | true | - | - | - | 52 |
| `doc_bad_ident.doc` | cfbf | - | - | true | - | - | - | 6 |
| `doc_bad_nfib.doc` | cfbf | - | - | true | - | - | - | 6 |
| `generic_cfbf.bin` | cfbf | - | - | true | - | - | - | 6 |
| `prose.txt` | opaque | - | - | true | - | - | - | 6 |
| `doc_0table.doc` | doc | 3584 | 3584 | true | true | true | - | - |
| `doc_1table.doc` | doc | 3584 | 3584 | true | true | true | - | - |
| `doc_sections.doc` | doc | 3584 | 3584 | true | true | true | - | - |
