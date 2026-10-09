# Phase 21.1.2 XLSX semantic court — matrix

| fixture | src len | out len | src sha256 | sha256 match | cmp | exact | decline rc |
| --- | ---: | ---: | --- | --- | --- | --- | ---: |
| `single.xlsx` | 2997 | 2997 | `0c79770deb94` | true | true | true | 6 |
| `multi.xlsx` | 3260 | 3260 | `ba56e76a2318` | true | true | true | 6 |
| `semantic.xlsx` | 7027 | 7027 | `d641caf10f9d` | true | true | true | 6 |

| contract check | result |
| --- | --- |
| merged-range REF exposed (semantic A5:B5) | true |
| no-drawing explicit absence (single.xlsx, rc 0) | true |
| unsupported-pair typed decline rc 6 (all 3 fixtures) | true |
