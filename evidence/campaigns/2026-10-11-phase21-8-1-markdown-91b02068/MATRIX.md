# Phase 21.8.1 Markdown court — matrix

| fixture | format | src len | out len | src sha256 | sha256 match | cmp | exact | decline rc | opaque rc |
| --- | --- | ---: | ---: | --- | --- | --- | --- | ---: | ---: |
| `basic.md` | markdown | 150 | 150 | `bb57781d1b11` | true | true | true | 6 | -1 |
| `lists.md` | markdown | 92 | 92 | `db84f5f8cd4b` | true | true | true | 6 | -1 |
| `code.md` | markdown | 124 | 124 | `078c2dff2459` | true | true | true | 6 | -1 |
| `table.md` | markdown | 69 | 69 | `279d43039e48` | true | true | true | 6 | -1 |
| `links.md` | markdown | 166 | 166 | `0967ecf816b2` | true | true | true | 6 | -1 |
| `blockquotes.md` | markdown | 55 | 55 | `e6fddf523d35` | true | true | true | 6 | -1 |
| `footnotes.md` | markdown | 94 | 94 | `74df0c7b8ec3` | true | true | true | 6 | -1 |
| `frontmatter.md` | markdown | 77 | 77 | `fd918862466a` | true | true | true | 6 | -1 |
| `toml_frontmatter.md` | markdown | 56 | 56 | `7ff2f7bc620f` | true | true | true | 6 | -1 |
| `large.md` | markdown | 3145792 | 3145792 | `603ee136e113` | true | true | true | 6 | -1 |
| `plain.txt` | opaque | 132 | 132 | `63160313db90` | true | true | true | -1 | 6 |
| `prose.md` | opaque | 114 | 114 | `4a9d3dc94f20` | true | true | true | -1 | 6 |
