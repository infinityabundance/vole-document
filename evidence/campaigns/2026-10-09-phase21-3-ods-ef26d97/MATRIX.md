# Phase 21.3.1 ODS court — matrix

| fixture | src len | out len | src sha256 | sha256 match | cmp | exact | decline rc | bomb rc |
| --- | ---: | ---: | --- | --- | --- | --- | ---: | ---: |
| `basic.ods` | 1534 | 1534 | `2d16edfeb07d` | true | true | true | 6 | -1 |
| `multi.ods` | 1533 | 1533 | `f53a8b26a796` | true | true | true | 6 | -1 |
| `values.ods` | 1766 | 1766 | `0416f37fae8a` | true | true | true | 6 | -1 |
| `merged.ods` | 1526 | 1526 | `d6d494a0d094` | true | true | true | 6 | -1 |
| `named.ods` | 1569 | 1569 | `0cc2f833b146` | true | true | true | 6 | -1 |
| `comments.ods` | 1556 | 1556 | `55dc0b8bed31` | true | true | true | 6 | -1 |
| `styles.ods` | 1692 | 1692 | `8de504519e8a` | true | true | true | 6 | -1 |
| `bomb.ods` | 1511 | 1511 | `65a66aba3ae4` | true | true | true | -1 | 8 |
