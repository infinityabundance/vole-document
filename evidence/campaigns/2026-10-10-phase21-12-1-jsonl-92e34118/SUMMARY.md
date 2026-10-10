# Phase 21.12.1 — JSONL / NDJSON (line/event-stream Wave 2) court

**Question.** Does the JSONL format close exactly and expose a
representation-preserving **per-line** model (each record's exact line span
and terminator, exact value span and bytes, per-record kind, duplicate keys,
numeric/escape spelling, blank-line/CRLF/trailing-newline accounting) on top
of the whole-source exact leaf — while keeping the Json-vs-Jsonl boundary
(a single JSON value stays Json) and declining malformed/non-newline-bag
inputs typed?

**Method.** Each self-authored JSONL fixture (generated deterministically by
`tools/fixtures/make-jsonl.py`, Python stdlib only) is ingested via
`field-build`, observed, then — after the **source file and the standalone
descriptor are deleted** — rematerialized exactly in a fresh process and
compared with `length + SHA-256 + cmp`. An out-of-range record is asked for
and required to decline typed; the single-value, malformed, and
non-newline-bag controls pin the detection boundaries.

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `basic.ndjson` | jsonl | 245 | 245 | true | true | true | 6 | -1 |
| `shapes.ndjson` | jsonl | 88 | 88 | true | true | true | 6 | -1 |
| `unicode.ndjson` | jsonl | 93 | 93 | true | true | true | 6 | -1 |
| `crlf.ndjson` | jsonl | 29 | 29 | true | true | true | 6 | -1 |
| `blank.ndjson` | jsonl | 20 | 20 | true | true | true | 6 | -1 |
| `large.ndjson` | jsonl | 2097199 | 2097199 | true | true | true | 6 | -1 |
| `single.json` | json | 48 | 48 | true | true | true | -1 | 6 |
| `malformed.ndjson` | opaque | 13 | 13 | true | true | true | -1 | 6 |
| `bag.ndjson` | opaque | 15 | 15 | true | true | true | -1 | 6 |

## Counts

```
fixtures 9
normal 6
exact_ok 9
exact_fail 0
typed_declines_ok 6
json_control_ok 1
opaque_controls_ok 2
opaque_controls 2
binary d1382c4092bc8ecda27e48c6204c3c59fd929fc7f2aeed5869d8746646874dbb
```

## Per-fixture observation files (raw/)

- `bag.ndjson.metadata.json`
- `basic.ndjson.find.json`
- `basic.ndjson.line.bin`
- `basic.ndjson.line.json`
- `basic.ndjson.metadata.json`
- `basic.ndjson.ptr.bin`
- `basic.ndjson.ptr.json`
- `basic.ndjson.text.json`
- `blank.ndjson.find.json`
- `blank.ndjson.line.bin`
- `blank.ndjson.line.json`
- `blank.ndjson.metadata.json`
- `blank.ndjson.ptr.bin`
- `blank.ndjson.ptr.json`
- `blank.ndjson.text.json`
- `crlf.ndjson.find.json`
- `crlf.ndjson.line.bin`
- `crlf.ndjson.line.json`
- `crlf.ndjson.metadata.json`
- `crlf.ndjson.ptr.bin`
- `crlf.ndjson.ptr.json`
- `crlf.ndjson.text.json`
- `large.ndjson.find.json`
- `large.ndjson.line.bin`
- `large.ndjson.line.json`
- `large.ndjson.metadata.json`
- `large.ndjson.ptr.bin`
- `large.ndjson.ptr.json`
- `large.ndjson.text.json`
- `malformed.ndjson.metadata.json`
- `results.json`
- `shapes.ndjson.find.json`
- `shapes.ndjson.line.bin`
- `shapes.ndjson.line.json`
- `shapes.ndjson.metadata.json`
- `shapes.ndjson.ptr.bin`
- `shapes.ndjson.ptr.json`
- `shapes.ndjson.text.json`
- `single.json.jsonl-decline.json`
- `unicode.ndjson.find.json`
- `unicode.ndjson.line.bin`
- `unicode.ndjson.line.json`
- `unicode.ndjson.metadata.json`
- `unicode.ndjson.ptr.bin`
- `unicode.ndjson.ptr.json`
- `unicode.ndjson.text.json`

## Scope (honest)

- **Shipped here:** byte-based conservative JSONL detection (the Json-vs-Jsonl
  boundary); the bounded, per-line span-preserving model (records parsed by the
  shared JSON parser, never a second JSON parser); native
  `jsonl-line`/`jsonl-pointer`/`jsonl-find`; common `metadata`/`text`.
- **Precedence:** JSON is tried first (a single JSON value, even across
  several lines, stays Json); JSONL is tried immediately after JSON and before
  YAML/TOML/CSV/Markdown/XML/HTML, because each non-blank line must parse as
  exactly one JSON value — the most specific signal for a newline-separated
  stream.
- **Declines:** fewer than two records, a malformed line, an over-cap line/
  record/node budget, and a non-newline-separated bag are typed declines; such
  input stays Opaque.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the JSONL
  model is derived (`Q_gen`) and never on the exactness path (ADR-0060: the
  model node depends on the `sha256(source)` root).
- **Not claimed here:** the economic court (separate script).
- **Never run on the host:** every command above ran in the pinned
  `doc-baseline` container.
