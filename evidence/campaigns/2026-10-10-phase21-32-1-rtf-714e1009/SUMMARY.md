# Phase 21.32.1 — RTF adapter court

**Question.** Does an RTF document close exactly and expose a
representation-preserving model (the header, each group's exact brace span and
depth, each control word/symbol, each `\'hh` hex escape and `\uN` Unicode
escape with its `\ucN` fallback count, each destination verbatim, and each plain
text run byte-exactly) on top of the whole-source exact leaf, while keeping a
conservative content-only detection boundary (RTF only on the strong
`{tf<digit>` magic plus balanced braces; a leading-`{` junk blob and prose
stay `opaque`)?

**Method.** Each self-authored fixture is generated in-court with a POSIX
here-doc and GNU `/usr/bin/printf`, ingested via `field-build`, observed, then —
after the **source file and the standalone descriptor are deleted** —
rematerialized exactly in a fresh process and compared with `length + SHA-256 +
cmp`. An out-of-range index, a bad destination reference, and an unsupported
common pair are required to decline typed; the leading-`{` junk blob, prose, a
`{tfX` header with no version digit, and an unbalanced brace pin the detection
boundaries. The court runs in the pinned `dev` service using only POSIX `sh`,
coreutils, git, and the shipped binary (no python3, no jq).

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: |
| `doc.rtf` | rtf | 251 | 251 | true | true | true | -1 |
| `junk.txt` | opaque | 11 | 11 | true | true | true | 6 |
| `prose.txt` | opaque | 50 | 50 | true | true | true | -1 |
| `rtfx.rtf` | opaque | 12 | 12 | true | true | true | -1 |
| `broken.rtf` | opaque | 15 | 15 | true | true | true | -1 |

## Counts

```
fixtures 5
rtf_fixtures 1
control_fixtures 4
exact_ok 5
exact_fail 0
surface_fail 0
boundary_ok 4
opaque_control_ok 1
typed_declines_ok 1
binary dc80c8db48a18b0c001f5d3342305055e902295dc54f34bbe2d642721d3d47fb
```

## Per-fixture observation files (raw/)

- `build.log`
- `doc.rtf.common.search.json`
- `doc.rtf.control0.bin`
- `doc.rtf.control0.json`
- `doc.rtf.dest.bin`
- `doc.rtf.find.json`
- `doc.rtf.group1.json`
- `doc.rtf.header.json`
- `doc.rtf.header.meta.json`
- `doc.rtf.hex0.bin`
- `doc.rtf.metadata.json`
- `doc.rtf.text.json`
- `doc.rtf.unicode0.json`
- `junk.txt.rtf-decline.json`
- `results.tsv`

## Scope (honest)

- **Shipped here:** content-only, conservative RTF detection (the strong
  `{tf<digit>` magic plus a bounded brace-balance scan); the bounded,
  span-preserving header/group/control/hex/unicode/text/destination model; native
  `rtf-header`/`rtf-group`/`rtf-control`/`rtf-hex`/`rtf-unicode`/`rtf-text`/
  `rtf-dest`/`rtf-find`; common `metadata`/`text`/`search-match`.
- **Precedence:** RTF detection runs after the binary magics and the bounded JSON
  sub-detectors and before the generic JSON detector, so a `{`-leading document is
  never stolen by JSON.
- **Boundary (honest):** detection is a magic + brace-balance test, not a
  full-grammar test; a blob that begins with `{tf<digit>` and has balanced
  braces is admitted as RTF even if its control words are meaningless. The adapter
  does not render, does not resolve font/stylesheet/info semantics, and does not
  map the `\mac`/`\pc`/`\pca` code pages beyond reporting the raw decoded byte.
