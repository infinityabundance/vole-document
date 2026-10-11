# Phase 21.26.2 — AsciiDoc (Asciidoctor) court

**Question.** Does the third Wave-2 prose format (the Asciidoctor input
language) close exactly and expose a representation-preserving AsciiDoc model
(exact block and inline spans, a level-0 document title and ==+ sections with
their exact marker and recorded level, document attributes with literal
attribute references, block attribute lines attached to the following block,
every delimited block kind with its exact delimiter and verbatim content, list
nesting, tables, admonitions, and inline markup plus the link:/image:/include::/
xref:/bare-URL macros) on top of the whole-source exact leaf, while keeping an
AsciiDoc-**specific** detection boundary (plain prose stays Opaque; a Markdown
document stays Markdown; a reStructuredText document stays Rst)?

**Method.** Each self-authored fixture is generated in-court with GNU
`/usr/bin/printf`, ingested via `field-build`, observed, then — after the
**source file and the standalone descriptor are deleted** — rematerialized
exactly in a fresh process and compared with `length + SHA-256 + cmp`. An
out-of-range native heading and an unsupported common pair are required to
decline typed; a malformed native argument is a usage error; the prose,
Markdown, and reST controls pin the detection boundaries. The court runs in the
pinned `dev` service using only POSIX `sh`, coreutils, git, and the shipped
binary (no python3, no jq).

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `basic.adoc` | asciidoc | 188 | 188 | true | true | true | 6 | 6 |
| `attributes.adoc` | asciidoc | 83 | 83 | true | true | true | 6 | 6 |
| `delimited.adoc` | asciidoc | 251 | 251 | true | true | true | 6 | 6 |
| `lists.adoc` | asciidoc | 113 | 113 | true | true | true | 6 | 6 |
| `tables.adoc` | asciidoc | 69 | 69 | true | true | true | 6 | 6 |
| `inline.adoc` | asciidoc | 251 | 251 | true | true | true | 6 | 6 |
| `admonitions.adoc` | asciidoc | 85 | 85 | true | true | true | 6 | 6 |
| `prose.txt` | opaque | 136 | 136 | true | true | true | -1 | 6 |
| `markdown.md` | markdown | 77 | 77 | true | true | true | -1 | -1 |
| `rest.rst` | rst | 40 | 40 | true | true | true | -1 | -1 |
| `attributes_spaced.adoc` | rst | 71 | 71 | true | true | true | -1 | -1 |

## Counts

```
fixtures 11
asciidoc_fixtures 7
control_fixtures 4
exact_ok 11
exact_fail 0
surface_fail 0
typed_declines_ok 7
boundary_ok 4
opaque_controls_ok 1
usage_ok 1
binary 17cfc5919092f8a88c49339de3fb64b38b38280ab91ca66e289288b57e947f0b
```

## Per-fixture observation files (raw/)

- `admonitions.adoc.attribute.json`
- `admonitions.adoc.block.exact.json`
- `admonitions.adoc.block.meta.json`
- `admonitions.adoc.common.block.json`
- `admonitions.adoc.common.heading.json`
- `admonitions.adoc.common.search.json`
- `admonitions.adoc.find.json`
- `admonitions.adoc.heading.json`
- `admonitions.adoc.metadata.json`
- `admonitions.adoc.text.json`
- `attributes.adoc.attribute.json`
- `attributes.adoc.block.exact.json`
- `attributes.adoc.block.meta.json`
- `attributes.adoc.common.block.json`
- `attributes.adoc.common.heading.json`
- `attributes.adoc.common.search.json`
- `attributes.adoc.find.json`
- `attributes.adoc.heading.json`
- `attributes.adoc.inline.json`
- `attributes.adoc.metadata.json`
- `attributes.adoc.text.json`
- `basic.adoc.attribute.json`
- `basic.adoc.block.exact.json`
- `basic.adoc.block.meta.json`
- `basic.adoc.common.block.json`
- `basic.adoc.common.heading.json`
- `basic.adoc.common.search.json`
- `basic.adoc.find.json`
- `basic.adoc.heading.json`
- `basic.adoc.heading1.json`
- `basic.adoc.inline.json`
- `basic.adoc.metadata.json`
- `basic.adoc.text.json`
- `build.log`
- `delimited.adoc.attribute.json`
- `delimited.adoc.block.exact.json`
- `delimited.adoc.block.meta.json`
- `delimited.adoc.common.block.json`
- `delimited.adoc.common.heading.json`
- `delimited.adoc.common.search.json`
- `delimited.adoc.find.json`
- `delimited.adoc.heading.json`
- `delimited.adoc.metadata.json`
- `delimited.adoc.text.json`
- `inline.adoc.attribute.json`
- `inline.adoc.block.exact.json`
- `inline.adoc.block.meta.json`
- `inline.adoc.common.block.json`
- `inline.adoc.common.heading.json`
- `inline.adoc.common.search.json`
- `inline.adoc.find.json`
- `inline.adoc.heading.json`
- `inline.adoc.inline.json`
- `inline.adoc.metadata.json`
- `inline.adoc.text.json`
- `lists.adoc.attribute.json`
- `lists.adoc.block.exact.json`
- `lists.adoc.block.meta.json`
- `lists.adoc.block1.json`
- `lists.adoc.common.block.json`
- `lists.adoc.common.heading.json`
- `lists.adoc.common.search.json`
- `lists.adoc.find.json`
- `lists.adoc.heading.json`
- `lists.adoc.metadata.json`
- `lists.adoc.text.json`
- `prose.txt.metadata.json`
- `results.tsv`
- `tables.adoc.attribute.json`
- `tables.adoc.block.exact.json`
- `tables.adoc.block.meta.json`
- `tables.adoc.common.block.json`
- `tables.adoc.common.heading.json`
- `tables.adoc.common.search.json`
- `tables.adoc.find.json`
- `tables.adoc.heading.json`
- `tables.adoc.inline.json`
- `tables.adoc.metadata.json`
- `tables.adoc.table.json`
- `tables.adoc.text.json`

## Scope (honest)

- **Shipped here:** byte-based, conservative, AsciiDoc-specific detection; the
  bounded, line-based AsciiDoc-subset parser (exact block and inline spans and
  bytes, document title/sections with their marker and level, document
  attributes with literal attribute references, block attribute lines attached
  to the following block, every delimited block kind with its delimiter and
  verbatim content, unordered/ordered/description lists with nesting, tables,
  admonitions, inline markup and macros); the canonical derived model; native
  `adoc-heading`/`adoc-block`/`adoc-attribute`/`adoc-inline`/`adoc-find`;
  common `metadata`/`text`/`heading`/`block`/`search-match`.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the AsciiDoc
  model is derived (`Q_gen`) and never on the exactness path (ADR-0060: the
  model node depends on the `sha256(source)` root, so no source-reading node
  aliases another field's source).
- **The source is never re-flowed or rendered.** A block's exact bytes are
  literally `source[span]`; the canonical text projection is the source itself.
- **The supported subset is bounded.** AsciiDoc has no magic bytes, so detection
  requires an AsciiDoc-specific signal. Attribute expansion, include/link
  resolution, the full cell/row-spanning table grammar, and structure nested
  inside delimited blocks are left as literal text rather than guessed.
- **The reST boundary is honest:** an `====`/`++++`/`....` block whose body is a
  single non-blank line is a reST overline title, so reST (tried first) admits it
  and it is never reclassified as AsciiDoc; the canonical spaced attribute form
  `:name: value` and the unset form `:name!:` are reST field lists, so reST
  claims them (the `attributes_spaced.adoc` control demonstrates this) and only
  the no-space `:name:value` spelling stays AsciiDoc; a source that is only
  `:name: value` lines is claimed by YAML/config first.
- **Not claimed here:** a full Asciidoctor conformance oracle.
- **Never run on the host:** every command above ran in the pinned `dev`
  container.
