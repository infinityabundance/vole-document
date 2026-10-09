# Phase 21.6.1 YAML court — matrix

| fixture | format | src len | out len | src sha256 | sha256 match | cmp | exact | decline rc | opaque rc |
| --- | --- | ---: | ---: | --- | --- | --- | --- | ---: | ---: |
| `anchors.yaml` | yaml | 208 | 208 | `f57c57ab5742` | true | true | true | 6 | -1 |
| `tags.yaml` | yaml | 130 | 130 | `e6fc268d7ff9` | true | true | true | 6 | -1 |
| `multidoc.yaml` | yaml | 76 | 76 | `f3b439061f3f` | true | true | true | 6 | -1 |
| `styles.yaml` | yaml | 175 | 175 | `0ae79a85419b` | true | true | true | 6 | -1 |
| `comments.yaml` | yaml | 116 | 116 | `8b3e895ad038` | true | true | true | 6 | -1 |
| `dup.yaml` | yaml | 37 | 37 | `09ccc133fb8d` | true | true | true | 6 | -1 |
| `deep.yaml` | yaml | 6762 | 6762 | `60914ea5b582` | true | true | true | 6 | -1 |
| `large.yaml` | yaml | 2097250 | 2097250 | `a62012ba9e9c` | true | true | true | 6 | -1 |
| `plain.yaml` | opaque | 122 | 122 | `f5b72fecab85` | true | true | true | -1 | 6 |
| `malformed.yaml` | opaque | 9 | 9 | `8f4b64c9d54f` | true | true | true | -1 | 6 |
