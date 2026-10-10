# Phase 21.9.1 — XML (structured-tree, Wave 2) court

**Question.** Does the XML structured-tree format close exactly and expose a
representation-preserving model (exact element/attribute/text/CDATA/comment/PI
spans, literal entity references, namespace declarations) on top of the
whole-source exact leaf — while refusing a DTD internal subset so no XXE or
billion-laughs expansion is possible?

**Method.** Each self-authored XML fixture (generated deterministically by
`tools/fixtures/make-xml.py`, Python stdlib only) is ingested via
`field-build`, observed, then — after the **source file and the standalone
descriptor are deleted** — rematerialized exactly in a fresh process and
compared with `length + SHA-256 + cmp`. Unsupported common pairs are asked
for and required to decline typed; the XXE/billion-laughs/`<-junk`/prose
controls are required to be Opaque.

## Exactness (after source + descriptor deletion)

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

## Counts

```
fixtures 10
exact_ok 10
exact_fail 0
typed_declines_ok 6
opaque_controls_ok 4
opaque_controls 4
binary dbde32d724aee763febd14eb61ff769c991d5ad9be8db790ae1da0ca44961c0f
```

## Per-fixture observation files (raw/)

- `attrs.xml.attr.bin`
- `attrs.xml.attr.json`
- `attrs.xml.element.json`
- `attrs.xml.find.json`
- `attrs.xml.metadata.json`
- `attrs.xml.path.json`
- `attrs.xml.text.json`
- `attrs.xml.token.bin`
- `basic.xml.attr.bin`
- `basic.xml.attr.json`
- `basic.xml.element.json`
- `basic.xml.find.json`
- `basic.xml.metadata.json`
- `basic.xml.path.json`
- `basic.xml.text.json`
- `basic.xml.token.bin`
- `billion.xml.metadata.json`
- `dtd.xml.element.json`
- `dtd.xml.find.json`
- `dtd.xml.metadata.json`
- `dtd.xml.path.json`
- `dtd.xml.text.json`
- `dtd.xml.token.bin`
- `junk.xml.metadata.json`
- `large.xml.element.json`
- `large.xml.find.json`
- `large.xml.metadata.json`
- `large.xml.path.json`
- `large.xml.text.json`
- `large.xml.token.bin`
- `mixed.xml.element.json`
- `mixed.xml.find.json`
- `mixed.xml.metadata.json`
- `mixed.xml.path.json`
- `mixed.xml.text.json`
- `mixed.xml.token.bin`
- `namespaces.xml.element.json`
- `namespaces.xml.find.json`
- `namespaces.xml.metadata.json`
- `namespaces.xml.namespaces.json`
- `namespaces.xml.path.json`
- `namespaces.xml.text.json`
- `namespaces.xml.token.bin`
- `prose.txt.metadata.json`
- `results.json`
- `xxe.xml.metadata.json`

## Scope (honest)

- **Shipped here:** byte-based conservative XML detection; the bounded,
  span-preserving scanner (elements, attributes, text, CDATA, comments, PIs,
  DOCTYPE, namespaces; entity references surfaced literally); the canonical
  derived model; native `xml-path`/`xml-element`/`xml-attr`/
  `xml-namespaces`/`xml-find`; common `metadata`/`text`/`search-match`.
- **Security:** a benign `<!DOCTYPE…>` (bare or PUBLIC/SYSTEM) is accepted
  and ignored (never fetched); a declaration with an internal subset is
  refused, so no entity is ever resolved — no XXE, no billion-laughs.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the XML
  model is derived (`Q_gen`) and never on the exactness path (ADR-0060: the
  model node depends on the `sha256(source)` root, so no source-reading node
  aliases another field's source).
- **Not claimed here:** a general XPath engine, XML Schema/DTD validation,
  C14N, encoding switching beyond UTF-8, XInclude, and the economic court.
- **Never run on the host:** every command above ran in the pinned
  `doc-baseline` container.
