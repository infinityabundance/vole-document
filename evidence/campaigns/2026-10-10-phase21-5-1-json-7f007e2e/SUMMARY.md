# Phase 21.5.1 — JSON (structured-tree, Wave 2) court

**Question.** Does the first Wave-2 structured-tree format close exactly and
expose a representation-preserving model (exact spans, kind, token bytes,
member order, duplicate keys) on top of the whole-source exact leaf?

**Method.** Each self-authored JSON fixture (generated deterministically by
`tools/fixtures/make-json.py`, Python stdlib only) is ingested via
`field-build`, observed, then — after the **source file and the standalone
descriptor are deleted** — rematerialized exactly in a fresh process and
compared with `length + SHA-256 + cmp`. Unsupported common pairs are asked
for and required to decline typed; the malformed control is required to be
Opaque (a typed decline, never a panic).

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | src sha256 | sha256 match | cmp | exact | decline rc | malformed rc |
| --- | --- | ---: | ---: | --- | --- | --- | --- | ---: | ---: |
| `basic.json` | json | 171 | 171 | `986ac004ec19` | true | true | true | 6 | -1 |
| `unicode.json` | json | 104 | 104 | `d3a37097aa92` | true | true | true | 6 | -1 |
| `numbers.json` | json | 125 | 125 | `8b0e5592ee9b` | true | true | true | 6 | -1 |
| `dup.json` | json | 43 | 43 | `58b1d57e1436` | true | true | true | 6 | -1 |
| `deep.json` | json | 321 | 321 | `b54b24043f3e` | true | true | true | 6 | -1 |
| `scalar.json` | json | 10 | 10 | `c775e7b757ed` | true | true | true | 6 | -1 |
| `large.json` | json | 2126936 | 2126936 | `4567f08cca81` | true | true | true | 6 | -1 |
| `malformed.json` | opaque | 19 | 19 | `dc5e9ca6d31d` | true | true | true | -1 | 6 |

## Counts

```
fixtures 8
exact_ok 8
exact_fail 0
typed_declines_ok 7
opaque_malformed_ok 1
binary dcfad49a927c2f96c47515c5ec00fdde7b116fc563373483fee754af4af92cd1
```

## Per-fixture observation files (raw/)

- `basic.json.find.json`
- `basic.json.metadata.json`
- `basic.json.node.json`
- `basic.json.pointer.json`
- `basic.json.text.json`
- `basic.json.token.bin`
- `deep.json.find.json`
- `deep.json.metadata.json`
- `deep.json.node.json`
- `deep.json.pointer.json`
- `deep.json.text.json`
- `deep.json.token.bin`
- `dup.json.find.json`
- `dup.json.metadata.json`
- `dup.json.node.json`
- `dup.json.pointer.json`
- `dup.json.text.json`
- `dup.json.token.bin`
- `large.json.find.json`
- `large.json.metadata.json`
- `large.json.node.json`
- `large.json.pointer.json`
- `large.json.text.json`
- `large.json.token.bin`
- `malformed.json.metadata.json`
- `numbers.json.find.json`
- `numbers.json.metadata.json`
- `numbers.json.node.json`
- `numbers.json.pointer.json`
- `numbers.json.text.json`
- `numbers.json.token.bin`
- `results.json`
- `scalar.json.find.json`
- `scalar.json.metadata.json`
- `scalar.json.node.json`
- `scalar.json.text.json`
- `unicode.json.find.json`
- `unicode.json.metadata.json`
- `unicode.json.node.json`
- `unicode.json.pointer.json`
- `unicode.json.text.json`
- `unicode.json.token.bin`

## Scope (honest)

- **Shipped here:** byte-based conservative JSON detection; the bounded,
  representation-preserving parser (exact token spans, member order,
  duplicate keys, numeric/escape spelling); the canonical derived model;
  native `json-pointer`/`json-node`/`json-find`; common `metadata`/`text`/
  `search-match`.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the JSON
  model is derived (`Q_gen`) and never on the exactness path (ADR-0060: the
  model node depends on the `sha256(source)` root, so no source-reading node
  aliases another field's source).
- **Not claimed here:** schema/JSON-Schema validation, JSONPath/JMESPath
  dialects, number canonicalization, and the economic court.
- **Never run on the host:** every command above ran in the pinned
  `doc-baseline` container.
