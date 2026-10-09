# Phase 21.7.1 — CSV/TSV (tabular, Wave 2) court

**Question.** Does the first Wave-2 tabular format close exactly and expose a
representation-preserving table (exact record/field spans and bytes, quotes,
embedded delimiters/newlines, CRLF vs LF, a BOM, ragged rows, a header, the
recorded dialect) on top of the whole-source exact leaf?

**Method.** Each self-authored CSV/TSV fixture (generated deterministically by
`tools/fixtures/make-csv.py`, Python stdlib only) is ingested via
`field-build`, observed, then — after the **source file and the standalone
descriptor are deleted** — rematerialized exactly in a fresh process and
compared with `length + SHA-256 + cmp`. Unsupported common pairs are asked for
and required to decline typed; the plain-text, one-column, and malformed
controls are required to be Opaque (a typed decline, never a panic).

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | src sha256 | sha256 match | cmp | exact | decline rc | opaque rc |
| --- | --- | ---: | ---: | --- | --- | --- | --- | ---: | ---: |
| `basic.csv` | csv | 43 | 43 | `84a071172374` | true | true | true | 6 | -1 |
| `quoted.csv` | csv | 57 | 57 | `4bd04183959e` | true | true | true | 6 | -1 |
| `crlf.csv` | csv | 54 | 54 | `bce76e2e2db1` | true | true | true | 6 | -1 |
| `bom.csv` | csv | 15 | 15 | `c343c48b2c2a` | true | true | true | 6 | -1 |
| `ragged.csv` | csv | 45 | 45 | `fd9dc65ce5f8` | true | true | true | 6 | -1 |
| `tsv.tsv` | csv | 49 | 49 | `b4e81925df9e` | true | true | true | 6 | -1 |
| `large.csv` | csv | 52428807 | 52428807 | `b86501e451d0` | true | true | true | 6 | -1 |
| `plain.txt` | opaque | 112 | 112 | `eaa754b7545f` | true | true | true | -1 | 6 |
| `malformed.csv` | opaque | 9 | 9 | `d6eeeaf202ac` | true | true | true | -1 | 6 |
| `onecol.csv` | opaque | 17 | 17 | `4fdbc441ea7b` | true | true | true | -1 | 6 |

## Counts

```
fixtures 10
exact_ok 10
exact_fail 0
typed_declines_ok 7
opaque_controls_ok 3
binary b8761d0d0fcde1df3c04556d66a3eae7c0c7bb2d66d05b2c651cd97d0a5b4777
```

## Per-fixture observation files (raw/)

- `basic.csv.cell.exact.json`
- `basic.csv.cell.text.json`
- `basic.csv.find.json`
- `basic.csv.header.json`
- `basic.csv.metadata.json`
- `basic.csv.range.json`
- `basic.csv.row.exact.json`
- `basic.csv.row.meta.json`
- `basic.csv.text.json`
- `bom.csv.cell.exact.json`
- `bom.csv.cell.text.json`
- `bom.csv.find.json`
- `bom.csv.header.json`
- `bom.csv.metadata.json`
- `bom.csv.range.json`
- `bom.csv.row.exact.json`
- `bom.csv.row.meta.json`
- `bom.csv.text.json`
- `crlf.csv.cell.exact.json`
- `crlf.csv.cell.text.json`
- `crlf.csv.find.json`
- `crlf.csv.header.json`
- `crlf.csv.metadata.json`
- `crlf.csv.range.json`
- `crlf.csv.row.exact.json`
- `crlf.csv.row.meta.json`
- `crlf.csv.text.json`
- `large.csv.cell.exact.json`
- `large.csv.cell.text.json`
- `large.csv.find.json`
- `large.csv.header.json`
- `large.csv.metadata.json`
- `large.csv.range.json`
- `large.csv.row.exact.json`
- `large.csv.row.meta.json`
- `large.csv.text.json`
- `malformed.csv.metadata.json`
- `onecol.csv.metadata.json`
- `plain.txt.metadata.json`
- `quoted.csv.cell.exact.json`
- `quoted.csv.cell.text.json`
- `quoted.csv.find.json`
- `quoted.csv.header.json`
- `quoted.csv.metadata.json`
- `quoted.csv.range.json`
- `quoted.csv.row.exact.json`
- `quoted.csv.row.meta.json`
- `quoted.csv.text.json`
- `ragged.csv.cell.exact.json`
- `ragged.csv.cell.text.json`
- `ragged.csv.find.json`
- `ragged.csv.header.json`
- `ragged.csv.metadata.json`
- `ragged.csv.range.json`
- `ragged.csv.row.exact.json`
- `ragged.csv.row.meta.json`
- `ragged.csv.text.json`
- `results.json`
- `tsv.tsv.cell.exact.json`
- `tsv.tsv.cell.text.json`
- `tsv.tsv.find.json`
- `tsv.tsv.header.json`
- `tsv.tsv.metadata.json`
- `tsv.tsv.range.json`
- `tsv.tsv.row.exact.json`
- `tsv.tsv.row.meta.json`
- `tsv.tsv.text.json`

## Scope (honest)

- **Shipped here:** byte-based conservative CSV/TSV detection; the bounded,
  streaming RFC 4180 CSV + TSV parser (exact record/field spans and bytes, the
  recorded dialect, original quoting with `""` escapes, embedded
  delimiters/newlines, CRLF/LF/CR, a BOM, ragged rows, blank lines, the header
  row); the canonical derived model; native `csv-row`/`csv-cell`/
  `csv-header`/`csv-range`/`csv-find`; common `metadata`/`text`/`table`/
  `cell`/`search-match`.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the CSV model
  is derived (`Q_gen`) and never on the exactness path (ADR-0060: the model node
  depends on the `sha256(source)` root, so no source-reading node aliases another
  field's source).
- **Ragged rows are handled, not declined:** a record whose field count differs
  from the header is preserved verbatim and reported; the header/majority column
  count is what detection requires.
- **The supported subset is bounded.** CSV has no magic bytes, so detection is a
  conservative heuristic (a stable delimiter, a consistent field count across a
  sampled majority of at least two records, at least two columns). A plain-text,
  one-column, or malformed document stays Opaque rather than being guessed at.
- **Not claimed here:** a full RFC 4180 conformance oracle, exotic quoting
  conventions (non-`"` quotes, escape characters), and the economic court.
- **Never run on the host:** every command above ran in the pinned
  `doc-baseline` container.
