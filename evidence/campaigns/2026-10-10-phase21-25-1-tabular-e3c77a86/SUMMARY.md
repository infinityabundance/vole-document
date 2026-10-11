# Phase 21.25.1 — tabular-extra (PSV + fixed-width) court

**Question.** Does the pipe delimiter close exactly as a third CSV dialect,
and does the new fixed-width adapter close exactly while exposing a
representation-preserving column-position table (per-column start/end
positions and widths, the uniform record width, exact per-record and
per-field padded spans, the terminator, a BOM, and the header row) — all on
top of the whole-source exact leaf, while keeping a maximally conservative
detection boundary (delimited and Markdown tables are never stolen; prose and
ambiguous aligned text stay Opaque)?

**Method.** Each self-authored fixture is generated in-court with GNU
`/usr/bin/printf`, ingested via `field-build`, observed, then — after the
**source file and the standalone descriptor are deleted** — rematerialized
exactly in a fresh process and compared with `length + SHA-256 + cmp`. An
out-of-range fixed-width record and an unsupported common pair are required to
decline typed; the delimited/Markdown/prose controls pin the detection
boundaries. The court runs in the pinned `dev` service using only POSIX
`sh`, coreutils, git, and the shipped binary (no python3, no jq).

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `basic.fw` | fixedwidth | 33 | 33 | true | true | true | 6 | 6 |
| `crlf.fw` | fixedwidth | 36 | 36 | true | true | true | 6 | 6 |
| `three.fw` | fixedwidth | 42 | 42 | true | true | true | 6 | 6 |
| `bom.fw` | fixedwidth | 36 | 36 | true | true | true | 6 | 6 |
| `comma.csv` | csv | 25 | 25 | true | true | true | -1 | -1 |
| `tab.tsv` | csv | 25 | 25 | true | true | true | -1 | -1 |
| `pipe.psv` | csv | 39 | 39 | true | true | true | -1 | -1 |
| `markdown.md` | markdown | 34 | 34 | true | true | true | -1 | -1 |
| `prose.txt` | opaque | 59 | 59 | true | true | true | -1 | 6 |
| `spaced.txt` | opaque | 30 | 30 | true | true | true | -1 | 6 |

## Counts

```
fixtures 10
fixedwidth_fixtures 4
delimited_fixtures 3
control_fixtures 3
exact_ok 10
exact_fail 0
surface_fail 0
typed_decline_rows_ok 4
boundary_ok 3
opaque_controls_ok 2
usage_ok 1
binary c349214461b06aa5ea8d0a0a2e6ba13cb88f3726f3900d946b8160fac82f37e1
```

## Per-fixture observation files (raw/)

- `basic.fw.cell11.exact.json`
- `basic.fw.cell11.text.json`
- `basic.fw.columns.json`
- `basic.fw.columns.meta.json`
- `basic.fw.find.json`
- `basic.fw.header.json`
- `basic.fw.metadata.json`
- `basic.fw.range.json`
- `basic.fw.row1.exact.json`
- `basic.fw.row1.meta.json`
- `basic.fw.text.json`
- `bom.fw.cell11.exact.json`
- `bom.fw.cell11.text.json`
- `bom.fw.columns.json`
- `bom.fw.columns.meta.json`
- `bom.fw.find.json`
- `bom.fw.header.json`
- `bom.fw.metadata.json`
- `bom.fw.range.json`
- `bom.fw.row1.exact.json`
- `bom.fw.row1.meta.json`
- `bom.fw.text.json`
- `build.log`
- `comma.csv.metadata.json`
- `crlf.fw.cell11.exact.json`
- `crlf.fw.cell11.text.json`
- `crlf.fw.columns.json`
- `crlf.fw.columns.meta.json`
- `crlf.fw.find.json`
- `crlf.fw.header.json`
- `crlf.fw.metadata.json`
- `crlf.fw.range.json`
- `crlf.fw.row1.exact.json`
- `crlf.fw.row1.meta.json`
- `crlf.fw.text.json`
- `pipe.psv.cell11.exact.json`
- `pipe.psv.metadata.json`
- `prose.txt.metadata.json`
- `results.tsv`
- `spaced.txt.metadata.json`
- `tab.tsv.metadata.json`
- `three.fw.cell11.exact.json`
- `three.fw.cell11.text.json`
- `three.fw.cellnm.text.json`
- `three.fw.columns.json`
- `three.fw.columns.meta.json`
- `three.fw.find.json`
- `three.fw.header.json`
- `three.fw.metadata.json`
- `three.fw.range.json`
- `three.fw.row1.exact.json`
- `three.fw.row1.meta.json`
- `three.fw.text.json`

## Scope (honest)

- **Shipped here:** the pipe delimiter as the CSV/TSV adapter's third recorded
  dialect (one parser, three delimiters — never a second parser); a new
  fixed-width (column-position) adapter and format; native `fixedwidth-row`/
  `fixedwidth-cell`/`fixedwidth-header`/`fixedwidth-columns`/
  `fixedwidth-range`/`fixedwidth-find`; common `metadata`/`text`/`table`/
  `cell`/`search-match`.
- **Detection boundary:** the pipe dialect is tried after comma and tab and is
  declined on a GFM/Markdown delimiter row, so a Markdown pipe table is never
  stolen. Fixed-width is claimed only with >= 3 sampled records of identical
  byte width, >= 2 non-empty columns, and interior whitespace gaps >= 2 columns
  wide — and only after declining any delimited or Markdown table.
- **Recorded negatives:** a trailing-space-trimmed (variable-width) fixed-width
  file, a single-space-separated two-column layout, and any file that also
  parses as a delimited table are NOT claimed; and a genuinely ambiguous
  aligned-text blob (equal-length lines, >= 2-wide gaps, >= 3 lines) is NOT
  distinguishable from a fixed-width table and IS claimed. Detection samples a
  bounded prefix; `parse` re-validates every line and declines the whole
  document typed if any differs. Character positions are byte positions.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): both the
  `CsvModel` (PSV) and the `FixedWidthModel` are derived (`Q_gen`) and never on
  the exactness path (ADR-0060: each model node depends on the `sha256(source)`
  root).
- **Not claimed here:** the economic court (separate script), a declared
  column map, and a full fixed-width-conformance oracle.
- **Never run on the host:** every command above ran in the pinned `dev`
  container.
