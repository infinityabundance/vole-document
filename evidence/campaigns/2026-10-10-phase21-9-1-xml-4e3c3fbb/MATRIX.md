# Phase 21.9.1 XML court — matrix

| fixture | format | src len | out len | src sha256 | sha256 match | cmp | exact | decline rc | opaque rc |
| --- | --- | ---: | ---: | --- | --- | --- | --- | ---: | ---: |
| `basic.xml` | xml | 197 | 197 | `2ac55c2e46ff` | true | true | true | 6 | -1 |
| `namespaces.xml` | xml | 116 | 116 | `3235e4a74b1f` | true | true | true | 6 | -1 |
| `mixed.xml` | xml | 147 | 147 | `4373e9e8a809` | true | true | true | 6 | -1 |
| `attrs.xml` | xml | 64 | 64 | `6d757617b8bd` | true | true | true | 6 | -1 |
| `dtd.xml` | xml | 113 | 113 | `12f8c49cd739` | true | true | true | 6 | -1 |
| `large.xml` | xml | 2097289 | 2097289 | `eba23fa2a4df` | true | true | true | 6 | -1 |
| `xxe.xml` | opaque | 76 | 76 | `a1ce57288c14` | true | true | true | -1 | 6 |
| `billion.xml` | opaque | 212 | 212 | `b5201c93cfea` | true | true | true | -1 | 6 |
| `junk.xml` | opaque | 32 | 32 | `54a64c0f2e9b` | true | true | true | -1 | 6 |
| `prose.txt` | opaque | 65 | 65 | `69672bd0e463` | true | true | true | -1 | 6 |
