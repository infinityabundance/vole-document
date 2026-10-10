# Phase 21.16.1 — Arrow IPC (analytical Wave 2) court

**Question.** Does the Arrow IPC format close exactly and expose a bounded
derived model — the Flatbuffers footer/leading-schema inventory (schema,
record batches with exact source spans) and the decoded values for the common
primitive/binary types — on top of the whole-source exact leaf, while
declining unsupported types/compression and bombs typed?

**Method.** Each self-authored Arrow IPC fixture (generated deterministically
by `tools/fixtures/make-arrow.py`, Python stdlib only — a hand-built
Flatbuffers metadata writer and columnar buffer encoders) is ingested via
`field-build`, observed, then — after the **source file and the standalone
descriptor are deleted** — rematerialized exactly in a fresh process and
compared with `length + SHA-256 + cmp`. Unsupported/bomb fixtures are
required to decline typed; the Opaque controls pin the detection boundary.

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `primitives.arrow` | arrow | 2786 | 2786 | true | true | true | -1 | -1 |
| `nullable.arrow` | arrow | 986 | 986 | true | true | true | -1 | -1 |
| `strings.arrow` | arrow | 1474 | 1474 | true | true | true | -1 | -1 |
| `temporal.arrow` | arrow | 1194 | 1194 | true | true | true | -1 | -1 |
| `multi_batch.arrow` | arrow | 1410 | 1410 | true | true | true | -1 | -1 |
| `fsb.arrow` | arrow | 514 | 514 | true | true | true | -1 | -1 |
| `stream.arrow` | arrow | 712 | 712 | true | true | true | -1 | -1 |
| `large.arrow` | arrow | 1502738 | 1502738 | true | true | true | -1 | -1 |
| `unsupported_decimal.arrow` | arrow | 602 | 602 | true | true | true | 6 | -1 |
| `unsupported_nested.arrow` | arrow | 714 | 714 | true | true | true | 6 | -1 |
| `unsupported_dictionary.arrow` | arrow | 586 | 586 | true | true | true | 6 | -1 |
| `unsupported_compressed.arrow` | arrow | 554 | 554 | true | true | true | 6 | -1 |
| `bomb.arrow` | arrow | 514 | 514 | true | true | true | 8 | -1 |
| `malformed_flatbuf.arrow` | arrow | 82 | 82 | true | true | true | 30 | -1 |
| `prose.txt` | opaque | 63 | 63 | true | true | true | -1 | 6 |
| `magic_only.bin` | opaque | 27 | 27 | true | true | true | -1 | 6 |
| `truncated.arrow` | opaque | 64 | 64 | true | true | true | -1 | 6 |
| `badlen.arrow` | opaque | 2786 | 2786 | true | true | true | -1 | 6 |

## Counts

```
fixtures 18
normal 8
exact_ok 18
exact_fail 0
typed_declines_ok 6
opaque_controls_ok 4
opaque_controls 4
binary bbe39bb9b69146ba3c2e1caa46f87dc525b067286978eb5ce5413d76a7aea770
```

## Per-fixture observation files (raw/)

- `badlen.arrow.metadata.json`
- `bomb.arrow.column.err`
- `bomb.arrow.column.txt`
- `fsb.arrow.batch.json`
- `fsb.arrow.cell.json`
- `fsb.arrow.column.bin`
- `fsb.arrow.column.json`
- `fsb.arrow.metadata.json`
- `fsb.arrow.schema.json`
- `fsb.arrow.table.json`
- `large.arrow.batch.json`
- `large.arrow.cell.json`
- `large.arrow.column.bin`
- `large.arrow.column.json`
- `large.arrow.column.json.full.json`
- `large.arrow.metadata.json`
- `large.arrow.schema.json`
- `large.arrow.table.json`
- `large.arrow.table.json.full.json`
- `magic_only.bin.metadata.json`
- `malformed_flatbuf.arrow.column.err`
- `malformed_flatbuf.arrow.column.txt`
- `multi_batch.arrow.batch.json`
- `multi_batch.arrow.cell.json`
- `multi_batch.arrow.column.bin`
- `multi_batch.arrow.column.json`
- `multi_batch.arrow.metadata.json`
- `multi_batch.arrow.schema.json`
- `multi_batch.arrow.table.json`
- `nullable.arrow.batch.json`
- `nullable.arrow.cell.json`
- `nullable.arrow.column.bin`
- `nullable.arrow.column.json`
- `nullable.arrow.metadata.json`
- `nullable.arrow.schema.json`
- `nullable.arrow.table.json`
- `primitives.arrow.batch.json`
- `primitives.arrow.cell.json`
- `primitives.arrow.column.bin`
- `primitives.arrow.column.json`
- `primitives.arrow.metadata.json`
- `primitives.arrow.schema.json`
- `primitives.arrow.table.json`
- `prose.txt.metadata.json`
- `results.json`
- `stream.arrow.batch.json`
- `stream.arrow.cell.json`
- `stream.arrow.column.bin`
- `stream.arrow.column.json`
- `stream.arrow.metadata.json`
- `stream.arrow.schema.json`
- `stream.arrow.table.json`
- `strings.arrow.batch.json`
- `strings.arrow.cell.json`
- `strings.arrow.column.bin`
- `strings.arrow.column.json`
- `strings.arrow.metadata.json`
- `strings.arrow.schema.json`
- `strings.arrow.table.json`
- `temporal.arrow.batch.json`
- `temporal.arrow.cell.json`
- `temporal.arrow.column.bin`
- `temporal.arrow.column.json`
- `temporal.arrow.metadata.json`
- `temporal.arrow.schema.json`
- `temporal.arrow.table.json`
- `truncated.arrow.metadata.json`
- `unsupported_compressed.arrow.column.err`
- `unsupported_compressed.arrow.column.txt`
- `unsupported_decimal.arrow.column.err`
- `unsupported_decimal.arrow.column.txt`
- `unsupported_dictionary.arrow.column.err`
- `unsupported_dictionary.arrow.column.txt`
- `unsupported_nested.arrow.column.err`
- `unsupported_nested.arrow.column.txt`

## Scope (honest)

- **Shipped here:** byte-based conservative Arrow IPC detection; the bounded,
  dependency-free Flatbuffers reader; the schema/inventory observation; decoded
  values for Int (all widths, signed/unsigned), FloatingPoint (half/single/
  double), Boolean, Date/Time/Timestamp/Duration (as raw ints), Utf8/LargeUtf8,
  Binary/LargeBinary, FixedSizeBinary, with validity bitmaps; file and stream
  formats; multi-batch; native `arrow-schema`/`arrow-column`/`arrow-batch`/
  `arrow-cell`; common `metadata`/`text`/`table`/`cell`/`search-match`.
- **Typed declines:** Null, Decimal, Interval, every nested type (List/
  LargeList/FixedSizeList/ListView/LargeListView/Struct/Map/Union/
  RunEndEncoded), BinaryView/Utf8View, dictionary-encoded fields, big-endian
  bodies, and any `BodyCompression` — each is declined typed, never guessed.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the Arrow
  model is derived (`Q_gen`) and never on the exactness path (ADR-0060: the
  model node depends on the `sha256(source)` root).
- **Not claimed here:** the economic court (separate script).
- **Never run on the host:** every command above ran in a pinned container.
