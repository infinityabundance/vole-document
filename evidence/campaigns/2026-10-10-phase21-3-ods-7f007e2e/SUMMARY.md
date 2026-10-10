# Phase 21.3.1 — ODS (OpenDocument Spreadsheet) court

**Question.** Does the first OpenDocument *spreadsheet* format close exactly and
expose a real spreadsheet model on the shared ODF package substrate?

**Method.** Each self-authored ODS fixture (generated deterministically by
`tools/fixtures/make-ods.py`, Python stdlib only) is ingested via
`field-build`, observed, then — after the **source file and the standalone
descriptor are deleted** — rematerialized exactly in a fresh process and
compared with `length + SHA-256 + cmp`. Unsupported common pairs are asked
for and required to decline typed; the repeated-row bomb is required to
decline as a resource limit.

## Exactness (after source + descriptor deletion)

| fixture | src len | out len | src sha256 | sha256 match | cmp | exact | decline rc | bomb rc |
| --- | ---: | ---: | --- | --- | --- | --- | ---: | ---: |
| `basic.ods` | 1534 | 1534 | `2d16edfeb07d` | true | true | true | 6 | -1 |
| `multi.ods` | 1533 | 1533 | `f53a8b26a796` | true | true | true | 6 | -1 |
| `values.ods` | 1766 | 1766 | `0416f37fae8a` | true | true | true | 6 | -1 |
| `merged.ods` | 1526 | 1526 | `d6d494a0d094` | true | true | true | 6 | -1 |
| `named.ods` | 1569 | 1569 | `0cc2f833b146` | true | true | true | 6 | -1 |
| `comments.ods` | 1556 | 1556 | `55dc0b8bed31` | true | true | true | 6 | -1 |
| `styles.ods` | 1692 | 1692 | `8de504519e8a` | true | true | true | 6 | -1 |
| `bomb.ods` | 1511 | 1511 | `65a66aba3ae4` | true | true | true | -1 | 8 |

## Counts

```
fixtures 8
exact_ok 8
exact_fail 0
typed_declines_ok 7
bomb_declines_ok 1
binary dcfad49a927c2f96c47515c5ec00fdde7b116fc563373483fee754af4af92cd1
```

## Per-fixture observation files (raw/)

- `basic.ods.cellA1.json`
- `basic.ods.comments.json`
- `basic.ods.metadata.json`
- `basic.ods.named.json`
- `basic.ods.sheet0.json`
- `basic.ods.styles.json`
- `basic.ods.text.json`
- `bomb.ods.text.json`
- `comments.ods.cellA1.json`
- `comments.ods.comments.json`
- `comments.ods.metadata.json`
- `comments.ods.named.json`
- `comments.ods.sheet0.json`
- `comments.ods.styles.json`
- `comments.ods.text.json`
- `merged.ods.cellA1.json`
- `merged.ods.comments.json`
- `merged.ods.metadata.json`
- `merged.ods.named.json`
- `merged.ods.sheet0.json`
- `merged.ods.styles.json`
- `merged.ods.text.json`
- `multi.ods.cellA1.json`
- `multi.ods.comments.json`
- `multi.ods.metadata.json`
- `multi.ods.named.json`
- `multi.ods.sheet0.json`
- `multi.ods.styles.json`
- `multi.ods.text.json`
- `named.ods.cellA1.json`
- `named.ods.comments.json`
- `named.ods.metadata.json`
- `named.ods.named.json`
- `named.ods.sheet0.json`
- `named.ods.styles.json`
- `named.ods.text.json`
- `results.json`
- `styles.ods.cellA1.json`
- `styles.ods.comments.json`
- `styles.ods.metadata.json`
- `styles.ods.named.json`
- `styles.ods.sheet0.json`
- `styles.ods.styles.json`
- `styles.ods.text.json`
- `values.ods.cellA1.json`
- `values.ods.comments.json`
- `values.ods.metadata.json`
- `values.ods.named.json`
- `values.ods.sheet0.json`
- `values.ods.styles.json`
- `values.ods.text.json`

## Scope (honest)

- **Shipped here:** byte-based ODS detection; the ODF-manifest-backed
  discovery model; the parsed sheet inventory (name/order/visibility); the
  distinct cell facets (stored formula, typed value, displayed text, style
  name, decoded-part span); bounded repeated-cell/-row expansion; merged
  spans; named expressions; cell styles; cell comments; native
  `ods-sheet`/`ods-cell`/`ods-find`/`ods-styles`/`ods-named-expressions`/
  `ods-comments`; common `metadata`/`text`/`table`/`cell`/`search-match`.
- **Exactness** is inherited from the Phase-12.2 ZIP member raw spans: the ODS
  model is derived (`Q_gen`) and never on the exactness path.
- **Not claimed here:** formula evaluation, number-format rendering,
  pivot tables, charts/drawings, external data references, and the economic
  court.
- **Never run on the host:** every command above ran in the pinned
  `doc-baseline` container.
