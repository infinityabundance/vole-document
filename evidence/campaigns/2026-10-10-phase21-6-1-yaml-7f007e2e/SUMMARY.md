# Phase 21.6.1 — YAML (structured-tree, Wave 2) court

**Question.** Does the second Wave-2 structured-tree format close exactly and
expose a representation-preserving model (exact spans, kind/style, token bytes,
anchors/aliases, tags, documents, mapping order, duplicate keys, comments) on top
of the whole-source exact leaf?

**Method.** Each self-authored YAML fixture (generated deterministically by
`tools/fixtures/make-yaml.py`, Python stdlib only) is ingested via
`field-build`, observed, then — after the **source file and the standalone
descriptor are deleted** — rematerialized exactly in a fresh process and
compared with `length + SHA-256 + cmp`. Unsupported common pairs are asked for
and required to decline typed; the plain-text and malformed controls are required
to be Opaque (a typed decline, never a panic).

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | src sha256 | sha256 match | cmp | exact | decline rc | opaque rc |
| --- | --- | ---: | ---: | --- | --- | --- | --- | ---: | ---: |
| `anchors.yaml` | yaml | 208 | 208 | `f57c57ab5742` | true | true | true | 6 | -1 |
| `tags.yaml` | yaml | 130 | 130 | `e6fc268d7ff9` | true | true | true | 6 | -1 |
| `multidoc.yaml` | yaml | 76 | 76 | `f3b439061f3f` | true | true | true | 6 | -1 |
| `styles.yaml` | yaml | 175 | 175 | `0ae79a85419b` | true | true | true | 6 | -1 |
| `comments.yaml` | yaml | 116 | 116 | `8b3e895ad038` | true | true | true | 6 | -1 |
| `dup.yaml` | yaml | 37 | 37 | `09ccc133fb8d` | true | true | true | 6 | -1 |
| `deep.yaml` | yaml | 6762 | 6762 | `60914ea5b582` | true | true | true | 6 | -1 |
| `large.yaml` | yaml | 2097250 | 2097250 | `a62012ba9e9c` | true | true | true | 6 | -1 |
| `plain.yaml` | opaque | 122 | 122 | `f5b72fecab85` | true | true | true | -1 | 6 |
| `malformed.yaml` | opaque | 9 | 9 | `8f4b64c9d54f` | true | true | true | -1 | 6 |

## Counts

```
fixtures 10
exact_ok 10
exact_fail 0
typed_declines_ok 8
opaque_controls_ok 2
binary dcfad49a927c2f96c47515c5ec00fdde7b116fc563373483fee754af4af92cd1
```

## Per-fixture observation files (raw/)

- `anchors.yaml.anchor.json`
- `anchors.yaml.documents.json`
- `anchors.yaml.find.json`
- `anchors.yaml.metadata.json`
- `anchors.yaml.node.json`
- `anchors.yaml.path.json`
- `anchors.yaml.text.json`
- `anchors.yaml.token.json`
- `comments.yaml.documents.json`
- `comments.yaml.find.json`
- `comments.yaml.metadata.json`
- `comments.yaml.node.json`
- `comments.yaml.path.json`
- `comments.yaml.text.json`
- `comments.yaml.token.json`
- `deep.yaml.documents.json`
- `deep.yaml.find.json`
- `deep.yaml.metadata.json`
- `deep.yaml.node.json`
- `deep.yaml.text.json`
- `dup.yaml.documents.json`
- `dup.yaml.find.json`
- `dup.yaml.metadata.json`
- `dup.yaml.node.json`
- `dup.yaml.path.json`
- `dup.yaml.text.json`
- `dup.yaml.token.json`
- `large.yaml.documents.json`
- `large.yaml.find.json`
- `large.yaml.metadata.json`
- `large.yaml.node.json`
- `large.yaml.path.json`
- `large.yaml.text.json`
- `large.yaml.token.json`
- `malformed.yaml.metadata.json`
- `multidoc.yaml.documents.json`
- `multidoc.yaml.find.json`
- `multidoc.yaml.metadata.json`
- `multidoc.yaml.node.json`
- `multidoc.yaml.path.json`
- `multidoc.yaml.text.json`
- `multidoc.yaml.token.json`
- `plain.yaml.metadata.json`
- `results.json`
- `styles.yaml.documents.json`
- `styles.yaml.find.json`
- `styles.yaml.metadata.json`
- `styles.yaml.node.json`
- `styles.yaml.path.json`
- `styles.yaml.text.json`
- `styles.yaml.token.json`
- `tags.yaml.documents.json`
- `tags.yaml.find.json`
- `tags.yaml.metadata.json`
- `tags.yaml.node.json`
- `tags.yaml.path.json`
- `tags.yaml.text.json`
- `tags.yaml.token.json`

## Scope (honest)

- **Shipped here:** byte-based conservative YAML detection; the bounded,
  representation-preserving parser (exact node spans, anchors/aliases as a graph,
  literal tags, scalar styles plain/single/double/literal/folded, multiple
  documents, mapping order, duplicate keys, merge keys surfaced, comment spans);
  the canonical derived model; native `yaml-path`/`yaml-node`/`yaml-documents`/
  `yaml-anchor`/`yaml-find`; common `metadata`/`text`/`search-match`.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the YAML model
  is derived (`Q_gen`) and never on the exactness path (ADR-0060: the model node
  depends on the `sha256(source)` root, so no source-reading node aliases another
  field's source).
- **The supported subset is bounded.** Directives, explicit keys, flow-collection
  keys, multi-line plain/quoted scalars, and tab indentation are DECLINED with a
  typed error (so the input stays Opaque) rather than guessed at.
- **Not claimed here:** the YAML 1.2 full spec, tag resolution/schema typing, and
  the economic court.
- **Never run on the host:** every command above ran in the pinned
  `doc-baseline` container.
