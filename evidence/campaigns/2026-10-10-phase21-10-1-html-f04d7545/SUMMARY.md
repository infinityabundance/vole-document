# Phase 21.10.1 — HTML (error-recovering markup, Wave 2) court

**Question.** Does the HTML markup format close exactly and expose a
representation-preserving, error-recovering model (exact element/attribute/
text/comment/DOCTYPE spans, attribute quoting, void elements, raw script/style
bytes, literal entity references) on top of the whole-source exact leaf — while
refusing a DOCTYPE internal subset (so no entity expansion is possible)?

**Method.** Each self-authored HTML fixture (generated deterministically by
`tools/fixtures/make-html.py`, Python stdlib only) is ingested via
`field-build`, observed, then — after the **source file and the standalone
descriptor are deleted** — rematerialized exactly in a fresh process and
compared with `length + SHA-256 + cmp`. Unsupported common pairs are asked
for and required to decline typed; the prose/junk/DOCTYPE-subset controls are
required to be Opaque, and the well-formed XHTML control is required to stay XML.

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | opaque rc | html-native rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: | ---: |
| `basic.html` | html | 338 | 338 | true | true | true | 6 | -1 | -1 |
| `elements.html` | html | 353 | 353 | true | true | true | 6 | -1 | -1 |
| `rawtext.html` | html | 314 | 314 | true | true | true | 6 | -1 | -1 |
| `entities.html` | html | 282 | 282 | true | true | true | 6 | -1 | -1 |
| `malformed.html` | html | 282 | 282 | true | true | true | 6 | -1 | -1 |
| `large.html` | html | 2097497 | 2097497 | true | true | true | 6 | -1 | -1 |
| `xhtml.html` | xml | 141 | 141 | true | true | true | -1 | -1 | 6 |
| `prose.txt` | opaque | 63 | 63 | true | true | true | -1 | 6 | -1 |
| `junk.html` | opaque | 33 | 33 | true | true | true | -1 | 6 | -1 |
| `xxe.html` | opaque | 78 | 78 | true | true | true | -1 | 6 | -1 |

## Counts

```
fixtures 10
normal 6
exact_ok 10
exact_fail 0
typed_declines_ok 6
opaque_controls_ok 3
opaque_controls 3
binary eda16868af76728a4dc9b00ceb9c3c2dcddcfbd10993615e2ddd9cf2aefa6036
```

## Per-fixture observation files (raw/)

- `basic.html.attr.bin`
- `basic.html.attr.json`
- `basic.html.element.json`
- `basic.html.find.json`
- `basic.html.heading.json`
- `basic.html.link.json`
- `basic.html.metadata.json`
- `basic.html.path.json`
- `basic.html.scripts.json`
- `basic.html.text.json`
- `basic.html.token.bin`
- `elements.html.attr.bin`
- `elements.html.attr.json`
- `elements.html.element.json`
- `elements.html.find.json`
- `elements.html.heading.json`
- `elements.html.link.json`
- `elements.html.metadata.json`
- `elements.html.path.json`
- `elements.html.scripts.json`
- `elements.html.text.json`
- `elements.html.token.bin`
- `entities.html.attr.bin`
- `entities.html.attr.json`
- `entities.html.element.json`
- `entities.html.find.json`
- `entities.html.heading.json`
- `entities.html.link.json`
- `entities.html.metadata.json`
- `entities.html.path.json`
- `entities.html.scripts.json`
- `entities.html.text.json`
- `entities.html.token.bin`
- `junk.html.metadata.json`
- `large.html.attr.bin`
- `large.html.attr.json`
- `large.html.element.json`
- `large.html.find.json`
- `large.html.heading.json`
- `large.html.link.json`
- `large.html.metadata.json`
- `large.html.path.json`
- `large.html.scripts.json`
- `large.html.text.json`
- `large.html.token.bin`
- `malformed.html.attr.bin`
- `malformed.html.attr.json`
- `malformed.html.element.json`
- `malformed.html.find.json`
- `malformed.html.heading.json`
- `malformed.html.link.json`
- `malformed.html.metadata.json`
- `malformed.html.path.json`
- `malformed.html.scripts.json`
- `malformed.html.text.json`
- `malformed.html.token.bin`
- `prose.txt.metadata.json`
- `rawtext.html.attr.bin`
- `rawtext.html.attr.json`
- `rawtext.html.element.json`
- `rawtext.html.find.json`
- `rawtext.html.heading.json`
- `rawtext.html.link.json`
- `rawtext.html.metadata.json`
- `rawtext.html.path.json`
- `rawtext.html.scripts.json`
- `rawtext.html.text.json`
- `rawtext.html.token.bin`
- `results.json`
- `xhtml.html.html-native.json`
- `xhtml.html.metadata.json`
- `xxe.html.metadata.json`

## Scope (honest)

- **Shipped here:** byte-based conservative HTML detection; the bounded,
  span-preserving, **error-recovering** scanner (elements, attributes with
  double/single/unquoted/boolean quoting, text, comments, DOCTYPE, raw
  `script`/`style` content; entity references surfaced literally); the canonical
  derived model; native `html-path`/`html-element`/`html-attr`/`html-scripts`/
  `html-find`; common `metadata`/`text`/`heading`/`link`/`search-match`.
- **Encoding:** UTF-8 only (HTML has no `encoding_rs` here); a UTF-16 BOM, a NUL
  byte, or a non-UTF-8 byte string is a typed decline.
- **Security:** a DOCTYPE with an internal subset is refused, so no entity is
  ever resolved; `script`/`style` content is captured as raw bytes and is never
  executed or parsed as markup.
- **Precedence:** XML is tried before HTML, so a fully well-formed XHTML source
  stays XML; HTML only claims the `<`-bearing sources XML declines.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the HTML
  model is derived (`Q_gen`) and never on the exactness path (ADR-0060: the
  model node depends on the `sha256(source)` root).
- **Not claimed here:** the full HTML tree-construction recovery algorithm
  (adoption agency / foster parenting), CSS/JS interpretation, encodings beyond
  UTF-8, and the economic court.
- **Never run on the host:** every command above ran in the pinned
  `doc-baseline` container.
