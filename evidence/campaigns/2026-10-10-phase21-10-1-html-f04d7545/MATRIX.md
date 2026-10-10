# Phase 21.10.1 HTML court — matrix

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | opaque rc | html-native rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: | ---: |
| `basic.html` | html | 338 | 338 | true | true | true | 6 | -1 | -1 |
| `elements.html` | html | 353 | 353 | true | true | true | 6 | -1 | -1 |
| `rawtext.html` | html | 314 | 314 | true | true | true | 6 | -1 | -1 |
| `entities.html` | html | 282 | 282 | true | true | true | 6 | -1 | -1 |
| `malformed.html` | html | 282 | 282 | true | true | true | 6 | -1 | -1 |
| `large.html` | html | 2097497 | 2097497 | true | true | true | 6 | -1 | -1 |
| `xhtml.html` | xml | 141 | 141 | true | true | true | -1 | -1 | 6 |
| `prose.txt` | opaque | 63 | 63 | true | true | true | -1 | 6 | -1 |
| `junk.html` | opaque | 33 | 33 | true | true | true | -1 | 6 | -1 |
| `xxe.html` | opaque | 78 | 78 | true | true | true | -1 | 6 | -1 |
