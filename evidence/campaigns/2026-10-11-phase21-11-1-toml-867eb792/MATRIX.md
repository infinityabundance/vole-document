# Phase 21.11.1 TOML court — matrix

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | opaque rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `basic.toml` | toml | 198 | 198 | true | true | true | 6 | -1 |
| `tables.toml` | toml | 196 | 196 | true | true | true | 6 | -1 |
| `arrays.toml` | toml | 358 | 358 | true | true | true | 6 | -1 |
| `inline.toml` | toml | 194 | 194 | true | true | true | 6 | -1 |
| `scalars.toml` | toml | 427 | 427 | true | true | true | 6 | -1 |
| `comments.toml` | toml | 179 | 179 | true | true | true | 6 | -1 |
| `large.toml` | toml | 2097282 | 2097282 | true | true | true | 6 | -1 |
| `prose.txt` | opaque | 67 | 67 | true | true | true | -1 | 6 |
| `dup.toml` | opaque | 25 | 25 | true | true | true | -1 | 6 |
| `junk.toml` | opaque | 28 | 28 | true | true | true | -1 | 6 |
