# Phase 21.14.1 — Parquet (analytical Wave 2) court

**Question.** Does the Parquet format close exactly and expose a bounded
derived model — the Thrift-Compact footer's schema, row-group and
column-chunk inventory (each chunk with its exact source span and
statistics), and the decoded values for PLAIN and RLE_DICTIONARY across the
common physical types — on top of the whole-source exact leaf, while
declining unsupported codecs/encodings and bombs typed?

**Method.** Each self-authored Parquet fixture (generated deterministically
by `tools/fixtures/make-parquet.py`, Python stdlib only — the`analytical`
lane is where DuckDB cross-checks the very same bytes) is ingested via
`field-build`, observed, then — after the **source file and the standalone
descriptor are deleted** — rematerialized exactly in a fresh process and
compared with `length + SHA-256 + cmp`. Unsupported/bomb fixtures are
required to decline typed; the Opaque controls pin the detection boundary.

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `small_plain.parquet` | parquet | 972 | 972 | true | true | true | -1 | -1 |
| `optional.parquet` | parquet | 334 | 334 | true | true | true | -1 | -1 |
| `dictionary.parquet` | parquet | 329 | 329 | true | true | true | -1 | -1 |
| `gzip.parquet` | parquet | 976 | 976 | true | true | true | -1 | -1 |
| `multi_rg.parquet` | parquet | 1034 | 1034 | true | true | true | -1 | -1 |
| `two_pages.parquet` | parquet | 529 | 529 | true | true | true | -1 | -1 |
| `large.parquet` | parquet | 2097603 | 2097603 | true | true | true | -1 | -1 |
| `unsupported_codec.parquet` | parquet | 329 | 329 | true | true | true | 6 | -1 |
| `unsupported_encoding.parquet` | parquet | 158 | 158 | true | true | true | 6 | -1 |
| `bomb.parquet` | parquet | 192 | 192 | true | true | true | 8 | -1 |
| `prose.txt` | opaque | 87 | 87 | true | true | true | -1 | 6 |
| `truncated.parquet` | opaque | 968 | 968 | true | true | true | -1 | 6 |
| `badlen.parquet` | opaque | 972 | 972 | true | true | true | -1 | 6 |

## Counts

```
fixtures 13
normal 7
exact_ok 13
exact_fail 0
typed_declines_ok 3
opaque_controls_ok 3
opaque_controls 3
binary 3181149fa43e495858bf88fd7173f2b660f86654105805d9a174f92a36e5b840
```

## Per-fixture observation files (raw/)

- `badlen.parquet.metadata.json`
- `bomb.parquet.column.err`
- `bomb.parquet.column.txt`
- `bomb.parquet.metadata.json`
- `dictionary.parquet.cell.json`
- `dictionary.parquet.column.bin`
- `dictionary.parquet.column.json`
- `dictionary.parquet.metadata.json`
- `dictionary.parquet.rowgroup.json`
- `dictionary.parquet.schema.json`
- `dictionary.parquet.table.json`
- `gzip.parquet.cell.json`
- `gzip.parquet.column.bin`
- `gzip.parquet.column.json`
- `gzip.parquet.metadata.json`
- `gzip.parquet.rowgroup.json`
- `gzip.parquet.schema.json`
- `gzip.parquet.table.json`
- `large.parquet.cell.json`
- `large.parquet.column.bin`
- `large.parquet.column.json`
- `large.parquet.column.json.full.json`
- `large.parquet.metadata.json`
- `large.parquet.rowgroup.json`
- `large.parquet.schema.json`
- `large.parquet.table.json`
- `large.parquet.table.json.full.json`
- `multi_rg.parquet.cell.json`
- `multi_rg.parquet.column.bin`
- `multi_rg.parquet.column.json`
- `multi_rg.parquet.metadata.json`
- `multi_rg.parquet.rowgroup.json`
- `multi_rg.parquet.schema.json`
- `multi_rg.parquet.table.json`
- `optional.parquet.cell.json`
- `optional.parquet.column.bin`
- `optional.parquet.column.json`
- `optional.parquet.metadata.json`
- `optional.parquet.rowgroup.json`
- `optional.parquet.schema.json`
- `optional.parquet.table.json`
- `prose.txt.metadata.json`
- `results.json`
- `small_plain.parquet.cell.json`
- `small_plain.parquet.column.bin`
- `small_plain.parquet.column.json`
- `small_plain.parquet.metadata.json`
- `small_plain.parquet.rowgroup.json`
- `small_plain.parquet.schema.json`
- `small_plain.parquet.table.json`
- `truncated.parquet.metadata.json`
- `two_pages.parquet.cell.json`
- `two_pages.parquet.column.bin`
- `two_pages.parquet.column.json`
- `two_pages.parquet.metadata.json`
- `two_pages.parquet.rowgroup.json`
- `two_pages.parquet.schema.json`
- `two_pages.parquet.table.json`
- `unsupported_codec.parquet.column.err`
- `unsupported_codec.parquet.column.txt`
- `unsupported_codec.parquet.metadata.json`
- `unsupported_encoding.parquet.column.err`
- `unsupported_encoding.parquet.column.txt`
- `unsupported_encoding.parquet.metadata.json`

## Scope (honest)

- **Shipped here:** byte-based conservative Parquet detection; the bounded,
  dependency-free Thrift-Compact footer reader; the schema/inventory
  observation; decoded values for PLAIN + RLE_DICTIONARY across
  BOOLEAN/INT32/INT64/FLOAT/DOUBLE/BYTE_ARRAY/FIXED_LEN_BYTE_ARRAY, with
  UNCOMPRESSED + GZIP; native
  `parquet-schema`/`parquet-column`/`parquet-row-group`/`parquet-cell`; common
  `metadata`/`text`/`table`/`cell`/`search-match`.
- **Typed declines:** INT96, DATA_PAGE_V2, deprecated BIT_PACKED levels, the
  DELTA_* / GROUP_VAR_INT / BYTE_STREAM_SPLIT encodings, the SNAPPY/ZSTD/
  BROTLI/LZO/LZ4 codecs, repeated/nested columns, and legacy `min`/`max`
  statistics — each is declined typed, never guessed.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the Parquet
  model is derived (`Q_gen`) and never on the exactness path (ADR-0060: the
  model node depends on the `sha256(source)` root).
- **Not claimed here:** the economic court (separate script).
- **Never run on the host:** every command above ran in a pinned container.
