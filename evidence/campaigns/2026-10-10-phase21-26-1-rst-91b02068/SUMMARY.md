# Phase 21.26.1 — reStructuredText (Docutils) court

**Question.** Does the next Wave-2 prose format (the Docutils input language)
close exactly and expose a representation-preserving reST model (exact block
and inline spans, section titles with their exact adornment and recorded
hierarchy, directives preserved verbatim, targets/footnotes/substitutions,
field/option/definition lists, literal and doctest blocks, list nesting, and
grid/simple tables) on top of the whole-source exact leaf, while keeping a
reST-**specific** detection boundary (plain prose and a Markdown document are
never stolen)?

**Method.** Each self-authored fixture is generated in-court with GNU
`/usr/bin/printf`, ingested via `field-build`, observed, then — after the
**source file and the standalone descriptor are deleted** — rematerialized
exactly in a fresh process and compared with `length + SHA-256 + cmp`. An
out-of-range native title and an unsupported common pair are required to decline
typed; a malformed native argument is a usage error; the prose and Markdown
controls pin the detection boundaries. The court runs in the pinned `dev`
service using only POSIX `sh`, coreutils, git, and the shipped binary (no
python3, no jq).

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `basic.rst` | rst | 134 | 134 | true | true | true | 6 | 6 |
| `explicit.rst` | rst | 159 | 159 | true | true | true | 6 | 6 |
| `lists.rst` | rst | 118 | 118 | true | true | true | 6 | 6 |
| `tables.rst` | rst | 126 | 126 | true | true | true | 6 | 6 |
| `inline.rst` | rst | 90 | 90 | true | true | true | 6 | 6 |
| `literal.rst` | rst | 86 | 86 | true | true | true | 6 | 6 |
| `prose.txt` | opaque | 128 | 128 | true | true | true | -1 | 6 |
| `markdown.md` | markdown | 77 | 77 | true | true | true | -1 | -1 |

## Counts

```
fixtures 8
rst_fixtures 6
control_fixtures 2
exact_ok 8
exact_fail 0
surface_fail 0
typed_declines_ok 6
boundary_ok 2
opaque_controls_ok 1
usage_ok 1
binary a7197a385beaa6ec9225569215042f2e16cc47bbc8c4ab130a578c70484a199d
```

## Per-fixture observation files (raw/)

- `basic.rst.block.exact.json`
- `basic.rst.block.meta.json`
- `basic.rst.common.block.json`
- `basic.rst.common.heading.json`
- `basic.rst.common.search.json`
- `basic.rst.find.json`
- `basic.rst.heading.json`
- `basic.rst.inline.json`
- `basic.rst.metadata.json`
- `basic.rst.text.json`
- `build.log`
- `explicit.rst.block.exact.json`
- `explicit.rst.block.meta.json`
- `explicit.rst.common.block.json`
- `explicit.rst.common.heading.json`
- `explicit.rst.common.search.json`
- `explicit.rst.directive.meta.json`
- `explicit.rst.directive.text.json`
- `explicit.rst.find.json`
- `explicit.rst.heading.json`
- `explicit.rst.metadata.json`
- `explicit.rst.text.json`
- `inline.rst.block.exact.json`
- `inline.rst.block.meta.json`
- `inline.rst.common.block.json`
- `inline.rst.common.heading.json`
- `inline.rst.common.search.json`
- `inline.rst.find.json`
- `inline.rst.heading.json`
- `inline.rst.inline.json`
- `inline.rst.metadata.json`
- `inline.rst.text.json`
- `lists.rst.block.exact.json`
- `lists.rst.block.meta.json`
- `lists.rst.common.block.json`
- `lists.rst.common.heading.json`
- `lists.rst.common.search.json`
- `lists.rst.find.json`
- `lists.rst.heading.json`
- `lists.rst.metadata.json`
- `lists.rst.text.json`
- `literal.rst.block.exact.json`
- `literal.rst.block.meta.json`
- `literal.rst.common.block.json`
- `literal.rst.common.heading.json`
- `literal.rst.common.search.json`
- `literal.rst.find.json`
- `literal.rst.heading.json`
- `literal.rst.metadata.json`
- `literal.rst.text.json`
- `prose.txt.metadata.json`
- `results.tsv`
- `tables.rst.block.exact.json`
- `tables.rst.block.meta.json`
- `tables.rst.common.block.json`
- `tables.rst.common.heading.json`
- `tables.rst.common.search.json`
- `tables.rst.find.json`
- `tables.rst.heading.json`
- `tables.rst.inline.json`
- `tables.rst.metadata.json`
- `tables.rst.text.json`

## Scope (honest)

- **Shipped here:** byte-based, conservative, reST-specific detection; the
  bounded, line-based Docutils-subset parser (exact block and inline spans and
  bytes, section titles and their adornment/hierarchy, explicit markup,
  directives preserved verbatim, targets/footnotes/substitutions,
  field/option/definition lists, literal and doctest blocks, bullet/enumerated
  lists with nesting, inline markup, and grid/simple tables); the canonical
  derived model; native `rst-heading`/`rst-block`/`rst-directive`/`rst-inline`/
  `rst-find`; common `metadata`/`text`/`heading`/`block`/`search-match`.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the reST
  model is derived (`Q_gen`) and never on the exactness path (ADR-0060: the
  model node depends on the `sha256(source)` root, so no source-reading node
  aliases another field's source).
- **The source is never re-flowed or rendered.** A block's exact bytes are
  literally `source[span]`; the canonical text projection is the source itself.
- **The supported subset is bounded.** reST has no magic bytes, so detection
  requires a reST-specific signal. The full directive option/argument grammar,
  multi-line section titles, and nested structure inside directives/lists are
  left as literal text rather than guessed.
- **The Markdown boundary is honest:** a `- \* _` adornment (>= 3) is a
  Markdown thematic break, `~` (>= 3) is a Markdown fence, and `#` is a
  Markdown ATX heading, so those documents are never reclassified as reST; and a
  tiny `:name: value` / `.. name:: body` source is claimed by YAML first.
- **Not claimed here:** a full Docutils conformance oracle.
- **Never run on the host:** every command above ran in the pinned `dev`
  container.
