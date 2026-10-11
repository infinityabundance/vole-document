# Phase 21.8.1 — Markdown (prose, Wave 2) court

**Question.** Does the first Wave-2 prose format close exactly and expose a
representation-preserving prose model (exact block/inline spans and bytes,
headings and levels, list items, fenced code with its language, blockquotes,
tables, links/images with targets and titles, reference definitions, footnotes,
front matter) on top of the whole-source exact leaf?

**Method.** Each self-authored Markdown fixture (generated deterministically by
`tools/fixtures/make-markdown.py`, Python stdlib only) is ingested via
`field-build`, observed, then — after the **source file and the standalone
descriptor are deleted** — rematerialized exactly in a fresh process and
compared with `length + SHA-256 + cmp`. Unsupported common pairs are asked for
and required to decline typed; the plain-prose controls are required to be Opaque
(a typed decline, never a panic).

## Exactness (after source + descriptor deletion)

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

## Counts

```
fixtures 12
exact_ok 12
exact_fail 0
typed_declines_ok 10
opaque_controls_ok 2
binary a7197a385beaa6ec9225569215042f2e16cc47bbc8c4ab130a578c70484a199d
```

## Per-fixture observation files (raw/)

- `basic.md.block.exact.json`
- `basic.md.block.meta.json`
- `basic.md.find.json`
- `basic.md.heading.json`
- `basic.md.metadata.json`
- `basic.md.text.json`
- `blockquotes.md.block.exact.json`
- `blockquotes.md.block.meta.json`
- `blockquotes.md.find.json`
- `blockquotes.md.heading.json`
- `blockquotes.md.metadata.json`
- `blockquotes.md.text.json`
- `code.md.block.exact.json`
- `code.md.block.meta.json`
- `code.md.code.meta.json`
- `code.md.code.text.json`
- `code.md.find.json`
- `code.md.heading.json`
- `code.md.metadata.json`
- `code.md.text.json`
- `footnotes.md.block.exact.json`
- `footnotes.md.block.meta.json`
- `footnotes.md.find.json`
- `footnotes.md.heading.json`
- `footnotes.md.metadata.json`
- `footnotes.md.text.json`
- `frontmatter.md.block.exact.json`
- `frontmatter.md.block.meta.json`
- `frontmatter.md.find.json`
- `frontmatter.md.heading.json`
- `frontmatter.md.metadata.json`
- `frontmatter.md.text.json`
- `large.md.block.exact.json`
- `large.md.block.meta.json`
- `large.md.code.meta.json`
- `large.md.code.text.json`
- `large.md.find.json`
- `large.md.heading.json`
- `large.md.link.json`
- `large.md.metadata.json`
- `large.md.text.json`
- `links.md.block.exact.json`
- `links.md.block.meta.json`
- `links.md.find.json`
- `links.md.heading.json`
- `links.md.link.json`
- `links.md.metadata.json`
- `links.md.text.json`
- `lists.md.block.exact.json`
- `lists.md.block.meta.json`
- `lists.md.find.json`
- `lists.md.heading.json`
- `lists.md.metadata.json`
- `lists.md.text.json`
- `plain.txt.metadata.json`
- `prose.md.metadata.json`
- `results.json`
- `table.md.block.exact.json`
- `table.md.block.meta.json`
- `table.md.find.json`
- `table.md.heading.json`
- `table.md.metadata.json`
- `table.md.text.json`
- `toml_frontmatter.md.block.exact.json`
- `toml_frontmatter.md.block.meta.json`
- `toml_frontmatter.md.find.json`
- `toml_frontmatter.md.heading.json`
- `toml_frontmatter.md.metadata.json`
- `toml_frontmatter.md.text.json`

## Scope (honest)

- **Shipped here:** byte-based conservative Markdown detection; the bounded,
  line-based CommonMark-subset parser (exact block and inline spans and bytes,
  ATX headings and levels, paragraphs, ordered/unordered/nested lists, fenced
  code with language tags, indented code, blockquotes, GFM tables, inline and
  reference links and images with targets and titles, reference definitions,
  footnotes, and YAML/TOML front matter); the canonical derived model; native
  `md-heading`/`md-block`/`md-code`/`md-link`/`md-find`; common `metadata`/
  `text`/`heading`/`block`/`search-match`.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the Markdown
  model is derived (`Q_gen`) and never on the exactness path (ADR-0060: the model
  node depends on the `sha256(source)` root, so no source-reading node aliases
  another field's source).
- **The source is never re-flowed or rendered.** A block's exact bytes are
  literally `source[span]`; the canonical text projection is the source itself.
- **The supported subset is bounded.** Markdown has no magic bytes and plain
  prose is itself a valid Markdown paragraph, so detection requires a structural
  mark (an ATX heading, a fenced code block, front matter, a table, a reference
  definition, or a footnote definition). Plain prose stays Opaque rather than
  being guessed at. Setext headings, HTML blocks, and nested inline emphasis
  inside link text are left as literal text rather than guessed.
- **Not claimed here:** a full CommonMark conformance oracle and the economic
  court (a separate campaign).
- **Never run on the host:** every command above ran in the pinned
  `doc-baseline` container.
