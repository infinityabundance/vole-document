# Phase 21.26.3 — MDX (Markdown + JSX/ESM) court

**Question.** Does MDX — Markdown with JSX and ESM layered on top — close
exactly and expose a representation-preserving model (exact ESM/JSX/expression
spans, JSX attributes and nested children, fragments, brace-balanced inline and
block expressions, and the full reused Markdown surface) on top of the
whole-source exact leaf, while keeping an MDX-**specific** detection boundary
(plain prose stays Opaque; a Markdown document stays Markdown, even with
JSX-looking inline code or an inline `{x}`; a plain HTML document stays Html)?

**Method.** Each self-authored fixture is generated in-court with GNU
`/usr/bin/printf`, ingested via `field-build`, observed, then — after the
**source file and the standalone descriptor are deleted** — rematerialized
exactly in a fresh process and compared with `length + SHA-256 + cmp`. An
out-of-range native selector and an unsupported common pair are required to
decline typed; a malformed native argument is a usage error; the prose,
Markdown, inline-code, inline-expression, and HTML controls pin the detection
boundaries. The court runs in the pinned `dev` service using only POSIX `sh`,
coreutils, git, and the shipped binary (no python3, no jq).

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: |
| `esm.mdx` | mdx | 142 | 142 | true | true | true | -1 |
| `jsx.mdx` | mdx | 85 | 85 | true | true | true | -1 |
| `expr.mdx` | mdx | 107 | 107 | true | true | true | -1 |
| `mixed.mdx` | mdx | 96 | 96 | true | true | true | -1 |
| `surface.mdx` | mdx | 212 | 212 | true | true | true | -1 |
| `prose.txt` | opaque | 114 | 114 | true | true | true | 6 |
| `markdown.md` | markdown | 46 | 46 | true | true | true | -1 |
| `markdown_code.md` | markdown | 30 | 30 | true | true | true | -1 |
| `inline_expr.md` | markdown | 29 | 29 | true | true | true | -1 |
| `html.html` | html | 54 | 54 | true | true | true | -1 |
| `braces.txt` | opaque | 15 | 15 | true | true | true | -1 |

## Counts

```
fixtures 11
mdx_fixtures 5
control_fixtures 6
exact_ok 11
exact_fail 0
surface_fail 0
boundary_ok 6
opaque_controls_ok 1
typed_declines_ok 1
usage_ok 1
binary 17cfc5919092f8a88c49339de3fb64b38b38280ab91ca66e289288b57e947f0b
```

## Per-fixture observation files (raw/)

- `build.log`
- `esm.mdx.block.exact.json`
- `esm.mdx.block.meta.json`
- `esm.mdx.common.block.json`
- `esm.mdx.common.heading.json`
- `esm.mdx.common.search.json`
- `esm.mdx.esm0.json`
- `esm.mdx.esm1.json`
- `esm.mdx.find.json`
- `esm.mdx.heading.json`
- `esm.mdx.metadata.json`
- `esm.mdx.text.json`
- `expr.mdx.block.exact.json`
- `expr.mdx.block.meta.json`
- `expr.mdx.common.block.json`
- `expr.mdx.common.heading.json`
- `expr.mdx.common.search.json`
- `expr.mdx.esm0.json`
- `expr.mdx.expr0.json`
- `expr.mdx.expr1.json`
- `expr.mdx.find.json`
- `expr.mdx.heading.json`
- `expr.mdx.metadata.json`
- `expr.mdx.text.json`
- `jsx.mdx.block.exact.json`
- `jsx.mdx.block.meta.json`
- `jsx.mdx.common.block.json`
- `jsx.mdx.common.heading.json`
- `jsx.mdx.common.search.json`
- `jsx.mdx.find.json`
- `jsx.mdx.heading.json`
- `jsx.mdx.jsx0.json`
- `jsx.mdx.jsx1.json`
- `jsx.mdx.jsx2.json`
- `jsx.mdx.metadata.json`
- `jsx.mdx.text.json`
- `mixed.mdx.block.exact.json`
- `mixed.mdx.block.meta.json`
- `mixed.mdx.common.block.json`
- `mixed.mdx.common.heading.json`
- `mixed.mdx.common.search.json`
- `mixed.mdx.find.json`
- `mixed.mdx.heading.json`
- `mixed.mdx.jsx0.json`
- `mixed.mdx.metadata.json`
- `mixed.mdx.text.json`
- `prose.txt.metadata.json`
- `results.tsv`
- `surface.mdx.block.exact.json`
- `surface.mdx.block.meta.json`
- `surface.mdx.common.block.json`
- `surface.mdx.common.heading.json`
- `surface.mdx.common.search.json`
- `surface.mdx.find.json`
- `surface.mdx.heading.json`
- `surface.mdx.metadata.json`
- `surface.mdx.text.json`

## Scope (honest)

- **Shipped here:** byte-based, conservative, MDX-specific detection before
  Markdown and HTML; the reuse of the Markdown parser/model for the prose
  surface; the exact-span ESM/JSX/expression arenas; the canonical derived
  model; native `mdx-heading`/`mdx-block`/`mdx-esm`/`mdx-jsx`/
  `mdx-expression`/`mdx-find`; common `metadata`/`text`/`heading`/`block`/
  `search-match`.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the MDX model
  is derived (`Q_gen`) and never on the exactness path (ADR-0060: the model node
  depends on the `sha256(source)` root, so no source-reading node aliases another
  field's source).
- **The source is never re-flowed or rendered.** A block's, element's, or
  expression's exact bytes are literally `source[span]`; the canonical text
  projection is the source itself.
- **JSX is never executed and never parsed as JavaScript**; only extents, names,
  attribute counts, and child counts are recorded.
- **The boundary is honest:** a lowercase, quoted-attribute-only HTML element is
  indistinguishable from the same JSX and stays Html; an inline `{…}` alone does
  not admit MDX; JSX-looking text inside fenced/indented code or inline code spans
  is never an MDX signal.
- **Not claimed here:** a full MDX/JSX conformance oracle, nor JavaScript
  evaluation.
- **Never run on the host:** every command above ran in the pinned `dev`
  container.
