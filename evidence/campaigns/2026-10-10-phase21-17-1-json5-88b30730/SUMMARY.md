# Phase 21.17.1 — JSON5 / JSONC (structured-extra Wave 2) court

**Question.** Does the JSON5/JSONC format close exactly and expose a
representation-preserving **superset** model (comments with exact spans,
unquoted keys, single quotes, trailing commas, hex/leading-dot/Infinity/NaN
numbers, string continuations, the extended whitespace set, member order,
duplicate keys, numeric/escape spelling, and a recorded jsonc-vs-json5
dialect) on top of the whole-source exact leaf — while keeping the
strict-Json-vs-Json5 boundary and declining malformed/non-JSON inputs typed?

**Method.** Each self-authored fixture is ingested via `field-build`,
observed, then — after the **source file and the standalone descriptor are
deleted** — rematerialized exactly in a fresh process and compared with
`length + SHA-256 + cmp`. An out-of-range pointer is asked for and required
to decline typed; the strict-JSON, malformed, and prose controls pin the
detection boundaries. The court runs in the pinned `dev` service using only
POSIX `sh`, coreutils, git, and the shipped binary (no python3, no jq).

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `basic.json5` | json5 | 129 | 129 | true | true | true | 6 | -1 |
| `jsonc.jsonc` | json5 | 137 | 137 | true | true | true | 6 | -1 |
| `numbers.json5` | json5 | 106 | 106 | true | true | true | 6 | -1 |
| `strings.json5` | json5 | 79 | 79 | true | true | true | 6 | -1 |
| `unicode.json5` | json5 | 51 | 51 | true | true | true | 6 | -1 |
| `comments.json5` | json5 | 98 | 98 | true | true | true | 6 | -1 |
| `large.json5` | json5 | 375294 | 375294 | true | true | true | 6 | -1 |
| `strict.json` | json | 28 | 28 | true | true | true | -1 | 6 |
| `malformed.json5` | opaque | 5 | 5 | true | true | true | -1 | 6 |
| `prose.txt` | opaque | 93 | 93 | true | true | true | -1 | 6 |

## Counts

```
fixtures 10
normal 7
exact_ok 10
exact_fail 0
surface_fail 0
typed_declines_ok 7
json_control_ok 1
opaque_controls_ok 2
opaque_controls 2
binary 3b385ff7f742bd01aacdddfead4d0b967cf2ff9f83ba856b4b0ccb162e545ae2
```

## Per-fixture observation files (raw/)

- `basic.json5.comments.json`
- `basic.json5.find.json`
- `basic.json5.metadata.json`
- `basic.json5.node.json`
- `basic.json5.ptr.bin`
- `basic.json5.ptr.json`
- `basic.json5.text.json`
- `comments.json5.comments.json`
- `comments.json5.find.json`
- `comments.json5.metadata.json`
- `comments.json5.node.json`
- `comments.json5.ptr.bin`
- `comments.json5.ptr.json`
- `comments.json5.text.json`
- `jsonc.jsonc.comments.json`
- `jsonc.jsonc.find.json`
- `jsonc.jsonc.metadata.json`
- `jsonc.jsonc.node.json`
- `jsonc.jsonc.ptr.bin`
- `jsonc.jsonc.ptr.json`
- `jsonc.jsonc.text.json`
- `large.json5.comments.json`
- `large.json5.find.json`
- `large.json5.metadata.json`
- `large.json5.node.json`
- `large.json5.ptr.bin`
- `large.json5.ptr.json`
- `large.json5.text.json`
- `malformed.json5.metadata.json`
- `numbers.json5.comments.json`
- `numbers.json5.find.json`
- `numbers.json5.metadata.json`
- `numbers.json5.node.json`
- `numbers.json5.ptr.bin`
- `numbers.json5.ptr.json`
- `numbers.json5.text.json`
- `prose.txt.metadata.json`
- `results.tsv`
- `strict.json.json5-decline.json`
- `strings.json5.comments.json`
- `strings.json5.find.json`
- `strings.json5.metadata.json`
- `strings.json5.node.json`
- `strings.json5.ptr.bin`
- `strings.json5.ptr.json`
- `strings.json5.text.json`
- `unicode.json5.comments.json`
- `unicode.json5.find.json`
- `unicode.json5.metadata.json`
- `unicode.json5.node.json`
- `unicode.json5.ptr.bin`
- `unicode.json5.ptr.json`
- `unicode.json5.text.json`

## Scope (honest)

- **Shipped here:** byte-based conservative JSON5/JSONC detection (a strict
  JSON source stays `Json`); the bounded JSON5 parser reusing the JSON
  adapter's node arena and span policy; native
  `json5-pointer`/`json5-node`/`json5-find`/`json5-comments`; common
  `metadata`/`text`/`search-match`; a recorded dialect.
- **Precedence:** JSON first (strict JSON is never reclassified); JSON5/JSONC
  immediately after JSON and before JSONL/YAML/TOML/CSV/Markdown/XML/HTML.
- **Declines:** a malformed source, an over-cap node/comment/string/depth/
  document budget, and an out-of-range pointer are typed declines; such input
  stays `Opaque`.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the JSON5
  model is derived (`Q_gen`) and never on the exactness path (ADR-0060: the
  model node depends on the `sha256(source)` root).
- **IdentifierName:** the exact ECMAScript/Unicode ID_Start/ID_Continue tables
  are approximated with `char::is_alphabetic`/`is_alphanumeric` (a bounded,
  deterministic, no-dependency approximation), not a byte-exact port.
- **Not claimed here:** the economic court (separate script).
- **Never run on the host:** every command above ran in the pinned `dev`
  container.
