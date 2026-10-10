# Phase 21.4.1 ODP court — matrix

| fixture | src len | out len | src sha256 | sha256 match | cmp | exact | decline rc | bomb rc | slide0 text |
| --- | ---: | ---: | --- | --- | --- | --- | ---: | ---: | --- |
| `basic.odp` | 1744 | 1744 | `1d1e258582dc` | true | true | true | 6 | -1 | `Hello
World
Second paragraph` |
| `table.odp` | 1746 | 1746 | `3dc60c251243` | true | true | true | 6 | -1 | `Table
a	b
c	d` |
| `picture.odp` | 1950 | 1950 | `20f5987bd861` | true | true | true | 6 | -1 | `Picture` |
| `notes.odp` | 1678 | 1678 | `55647705cbee` | true | true | true | 6 | -1 | `Body text` |
| `order.odp` | 1695 | 1695 | `6ab262306a45` | true | true | true | 6 | -1 | `SECOND FILE` |
| `bomb.odp` | 1740 | 1740 | `62502dd3f224` | true | true | true | -1 | 8 | `SECOND FILE` |
