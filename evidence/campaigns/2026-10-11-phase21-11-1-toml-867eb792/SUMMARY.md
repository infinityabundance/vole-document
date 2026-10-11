# Phase 21.11.1 — TOML (structured-tree Wave 2) court

**Question.** Does the TOML format close exactly and expose a
representation-preserving model (exact table/array/inline-table/key/value/
comment spans, dotted keys, arrays of tables, and every scalar's exact
spelling) on top of the whole-source exact leaf — while **enforcing** TOML's
duplicate-key/redefinition rules (so violated input stays Opaque)?

**Method.** Each self-authored TOML fixture (generated deterministically by
`tools/fixtures/make-toml.py`, Python stdlib only) is ingested via
`field-build`, observed, then — after the **source file and the standalone
descriptor are deleted** — rematerialized exactly in a fresh process and
compared with `length + SHA-256 + cmp`. Unsupported common pairs are asked
for and required to decline typed; the prose/duplicate-key/junk controls are
required to be Opaque.

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | opaque rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `basic.toml` | toml | 198 | 198 | true | true | true | 6 | -1 |
| `tables.toml` | toml | 196 | 196 | true | true | true | 6 | -1 |
| `arrays.toml` | toml | 358 | 358 | true | true | true | 6 | -1 |
| `inline.toml` | toml | 194 | 194 | true | true | true | 6 | -1 |
| `scalars.toml` | toml | 427 | 427 | true | true | true | 6 | -1 |
| `comments.toml` | toml | 179 | 179 | true | true | true | 6 | -1 |
| `large.toml` | toml | 2097282 | 2097282 | true | true | true | 6 | -1 |
| `prose.txt` | opaque | 67 | 67 | true | true | true | -1 | 6 |
| `dup.toml` | opaque | 25 | 25 | true | true | true | -1 | 6 |
| `junk.toml` | opaque | 28 | 28 | true | true | true | -1 | 6 |

## Counts

```
fixtures 10
normal 7
exact_ok 10
exact_fail 0
typed_declines_ok 7
opaque_controls_ok 3
opaque_controls 3
binary cb8e73ba77081c3fab6d7e8acd6877e7d383919a5ce7d28f254bb7125f5acb83
```

## Per-fixture observation files (raw/)

- `arrays.toml.find.json`
- `arrays.toml.metadata.json`
- `arrays.toml.path.json`
- `arrays.toml.table.json`
- `arrays.toml.text.json`
- `arrays.toml.token.bin`
- `basic.toml.find.json`
- `basic.toml.metadata.json`
- `basic.toml.path.json`
- `basic.toml.table.json`
- `basic.toml.text.json`
- `basic.toml.token.bin`
- `comments.toml.find.json`
- `comments.toml.metadata.json`
- `comments.toml.path.json`
- `comments.toml.table.json`
- `comments.toml.text.json`
- `comments.toml.token.bin`
- `dup.toml.metadata.json`
- `inline.toml.find.json`
- `inline.toml.metadata.json`
- `inline.toml.path.json`
- `inline.toml.table.json`
- `inline.toml.text.json`
- `inline.toml.token.bin`
- `junk.toml.metadata.json`
- `large.toml.find.json`
- `large.toml.metadata.json`
- `large.toml.path.json`
- `large.toml.table.json`
- `large.toml.text.json`
- `large.toml.token.bin`
- `prose.txt.metadata.json`
- `results.json`
- `scalars.toml.find.json`
- `scalars.toml.metadata.json`
- `scalars.toml.path.json`
- `scalars.toml.table.json`
- `scalars.toml.text.json`
- `scalars.toml.token.bin`
- `tables.toml.find.json`
- `tables.toml.metadata.json`
- `tables.toml.path.json`
- `tables.toml.table.json`
- `tables.toml.text.json`
- `tables.toml.token.bin`

## Scope (honest)

- **Shipped here:** byte-based conservative TOML detection; the bounded,
  span-preserving parser (tables, arrays of tables, dotted keys, inline
  tables, arrays, keys, values, comments; every scalar's exact spelling); the
  canonical derived model; native `toml-path`/`toml-table`/`toml-find`;
  common `metadata`/`text`.
- **Precedence:** TOML is tried after JSON/YAML and before CSV/Markdown/XML/
  HTML, because its complete-parse signal is strong and a TOML comment (`#`
  at column 0) would otherwise be misread as a Markdown ATX heading.
- **Rules enforced:** duplicate keys and table redefinitions are typed
  declines (rc 26), not silently preserved; such input stays Opaque.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the TOML
  model is derived (`Q_gen`) and never on the exactness path (ADR-0060: the
  model node depends on the `sha256(source)` root).
- **Not claimed here:** the economic court (separate script), calendar
  validation of date-times, and encodings beyond UTF-8.
- **Never run on the host:** every command above ran in the pinned
  `doc-baseline` container.
