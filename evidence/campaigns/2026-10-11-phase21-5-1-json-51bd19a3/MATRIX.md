# Phase 21.5.1 JSON court — matrix

| fixture | format | src len | out len | src sha256 | sha256 match | cmp | exact | decline rc | malformed rc |
| --- | --- | ---: | ---: | --- | --- | --- | --- | ---: | ---: |
| `basic.json` | json | 171 | 171 | `986ac004ec19` | true | true | true | 6 | -1 |
| `unicode.json` | json | 104 | 104 | `d3a37097aa92` | true | true | true | 6 | -1 |
| `numbers.json` | json | 125 | 125 | `8b0e5592ee9b` | true | true | true | 6 | -1 |
| `dup.json` | json | 43 | 43 | `58b1d57e1436` | true | true | true | 6 | -1 |
| `deep.json` | json | 321 | 321 | `b54b24043f3e` | true | true | true | 6 | -1 |
| `scalar.json` | json | 10 | 10 | `c775e7b757ed` | true | true | true | 6 | -1 |
| `large.json` | json | 2126936 | 2126936 | `4567f08cca81` | true | true | true | 6 | -1 |
| `malformed.json` | opaque | 19 | 19 | `dc5e9ca6d31d` | true | true | true | -1 | 6 |
