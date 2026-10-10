# Phase 21.13.1 — EML / MIME (messaging Wave 2) court

**Question.** Does the EML/MIME format close exactly and expose a
representation-preserving **message** model (every header's exact
name/value/full span, header order and duplicate headers, folded headers, the
resolved `multipart/*` tree, and the exact
`Content-Transfer-Encoding`-decoded constituent bytes, incl. attachments) on
top of the whole-source exact leaf, while declining malformed inputs typed?

**Method.** Each self-authored EML fixture (generated deterministically by
`tools/fixtures/make-eml.py`, Python stdlib only) is ingested via
`field-build`, observed, then — after the **source file and the standalone
descriptor are deleted** — rematerialized exactly in a fresh process and
compared with `length + SHA-256 + cmp`. An out-of-range part is asked for and
required to decline typed; the `prose.txt` control pins the detection
boundary.

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `simple.eml` | eml | 201 | 201 | true | true | true | 6 | -1 |
| `mixed.eml` | eml | 626 | 626 | true | true | true | 6 | -1 |
| `alternative.eml` | eml | 380 | 380 | true | true | true | 6 | -1 |
| `nested.eml` | eml | 402 | 402 | true | true | true | 6 | -1 |
| `folded.eml` | eml | 395 | 395 | true | true | true | 6 | -1 |
| `large.eml` | eml | 2053092 | 2053092 | true | true | true | 6 | -1 |
| `prose.txt` | opaque | 129 | 129 | true | true | true | -1 | 6 |

## Counts

```
fixtures 7
normal 6
exact_ok 7
exact_fail 0
typed_declines_ok 6
opaque_controls_ok 1
opaque_controls 1
binary 5dd16b54db0561523d6c5f86dbebd60c7961d244de9ab326c612f0c5316a7557
```

## Per-fixture observation files (raw/)

- `alternative.eml.attachments.json`
- `alternative.eml.body.json`
- `alternative.eml.find.json`
- `alternative.eml.header.json`
- `alternative.eml.metadata.json`
- `alternative.eml.part.bin`
- `alternative.eml.part.json`
- `alternative.eml.text.json`
- `folded.eml.attachments.json`
- `folded.eml.body.json`
- `folded.eml.find.json`
- `folded.eml.header.json`
- `folded.eml.metadata.json`
- `folded.eml.part.bin`
- `folded.eml.part.json`
- `folded.eml.text.json`
- `large.eml.attachments.json`
- `large.eml.body.json`
- `large.eml.find.json`
- `large.eml.header.json`
- `large.eml.metadata.json`
- `large.eml.part.bin`
- `large.eml.part.json`
- `large.eml.resource.bin`
- `large.eml.text.json`
- `mixed.eml.attachments.json`
- `mixed.eml.body.json`
- `mixed.eml.find.json`
- `mixed.eml.header.json`
- `mixed.eml.metadata.json`
- `mixed.eml.part.bin`
- `mixed.eml.part.json`
- `mixed.eml.resource.bin`
- `mixed.eml.text.json`
- `nested.eml.attachments.json`
- `nested.eml.body.json`
- `nested.eml.find.json`
- `nested.eml.header.json`
- `nested.eml.metadata.json`
- `nested.eml.part.bin`
- `nested.eml.part.json`
- `nested.eml.text.json`
- `prose.txt.metadata.json`
- `results.json`
- `simple.eml.attachments.json`
- `simple.eml.body.json`
- `simple.eml.find.json`
- `simple.eml.header.json`
- `simple.eml.metadata.json`
- `simple.eml.part.bin`
- `simple.eml.part.json`
- `simple.eml.text.json`

## Scope (honest)

- **Shipped here:** byte-based conservative EML detection; the bounded,
  span-preserving message model; native
  `eml-header`/`eml-part`/`eml-attachments`/`eml-body`/`eml-find`; common
  `metadata`/`text`/`resource`/`search-match`.
- **Precedence:** EML is tried after JSON/JSONL and before
  YAML/TOML/CSV/Markdown/XML/HTML.
- **Declines:** a multipart without a boundary, an unknown transfer encoding,
  a non-UTF-8 charset for text, and encrypted/signed S/MIME are typed declines.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the EML
  model is derived (`Q_gen`) and never on the exactness path (ADR-0060: the
  model node depends on the `sha256(source)` root).
- **Not claimed here:** the economic court (separate script).
- **Never run on the host:** every command above ran in the pinned
  `doc-baseline` container.
