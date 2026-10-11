# Phase 21.27.1 MHTML court — matrix

| fixture | format | src len | out len | sha256 match | cmp | exact | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: |
| `base.mhtml` | mhtml | 876 | 876 | true | true | true | -1 |
| `nostart.mhtml` | mhtml | 322 | 322 | true | true | true | -1 |
| `envelope.mhtml` | mhtml | 158 | 158 | true | true | true | -1 |
| `large.mhtml` | mhtml | 67225 | 67225 | true | true | true | -1 |
| `prose.txt` | opaque | 92 | 92 | true | true | true | 6 |
| `plain.html` | html | 54 | 54 | true | true | true | -1 |
| `plain.eml` | eml | 87 | 87 | true | true | true | -1 |
| `related.eml` | eml | 155 | 155 | true | true | true | -1 |
| `related_no_marker.eml` | eml | 118 | 118 | true | true | true | -1 |
