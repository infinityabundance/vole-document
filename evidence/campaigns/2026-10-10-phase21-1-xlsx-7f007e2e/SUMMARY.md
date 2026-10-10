# Phase 21.1.1 — XLSX (SpreadsheetML) court

**Question.** Does the first format of the format programme close exactly and
expose a real SpreadsheetML model on the shared OPC surface?

**Method.** Each self-authored XLSX fixture (generated deterministically by
`tools/fixtures/make-xlsx.py`, Python stdlib only) is ingested via
`field-build`, observed, then — after the **source file and the standalone
descriptor are deleted** — rematerialized exactly in a fresh process and
compared with `length + SHA-256 + cmp`. Unsupported common pairs are asked
for and required to decline typed.

## Exactness (after source + descriptor deletion)

| fixture | src len | out len | src sha256 | sha256 match | cmp | exact | decline rc |
| --- | ---: | ---: | --- | --- | --- | --- | ---: |
| `single.xlsx` | 2997 | 2997 | `0c79770deb94` | true | true | true | 6 |
| `multi.xlsx` | 3260 | 3260 | `ba56e76a2318` | true | true | true | 6 |

## Counts

```
fixtures 2
exact_ok 2
exact_fail 0
typed_declines_ok 2
binary dcfad49a927c2f96c47515c5ec00fdde7b116fc563373483fee754af4af92cd1
```

## Per-fixture observation files (raw/)

- `multi.xlsx.cellA1.json`
- `multi.xlsx.cellA1.structure.json`
- `multi.xlsx.metadata.json`
- `multi.xlsx.sheet0.json`
- `multi.xlsx.text.json`
- `results.json`
- `single.xlsx.cellA1.json`
- `single.xlsx.cellA1.structure.json`
- `single.xlsx.metadata.json`
- `single.xlsx.sheet0.json`
- `single.xlsx.text.json`

## Scope (honest)

- **Shipped here:** byte-based XLSX detection; the OPC-backed workbook
  discovery model; the parsed workbook inventory (name/order/visibility);
  shared/inline strings, numbers/booleans/errors, a stored formula with a
  cached value, a minimal style table, and merged ranges; native
  `xlsx-sheet`/`xlsx-cell`/`xlsx-find`; common `metadata`/`text`/`table`/
  `cell`/`search-match`.
- **Exactness** is inherited from the Phase-12.2 ZIP member raw spans: the XLSX
  model is derived (`Q_gen`) and never on the exactness path.
- **Not claimed here:** displayed values (number-format rendering), formula
  evaluation, charts/drawings, external data references, pivot tables, and the
  economic court. Those are later subphases (21.1.2/21.1.3).
- **Never run on the host:** every command above ran in the pinned
  `doc-baseline` container.
