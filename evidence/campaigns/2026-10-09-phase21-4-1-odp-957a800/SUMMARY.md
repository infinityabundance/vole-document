# Phase 21.4.1 — ODP (OpenDocument Presentation) court

**Question.** Does the last Wave-1 office format — OpenDocument Presentation —
close exactly and expose a real presentation model on the shared ODF package
substrate (reusing ODT/ODS)?

**Method.** Each self-authored ODP fixture (generated deterministically by
`tools/fixtures/make-odp.py`, Python stdlib only) is ingested via
`field-build`, observed, then — after the **source file and the standalone
descriptor are deleted** — rematerialized exactly in a fresh process and
compared with `length + SHA-256 + cmp`. Unsupported common pairs are asked
for and required to decline typed; a repeated-row bomb is required to
decline as a resource limit; slide order is required to be the `draw:page`
document order (never a page-name order).

## Exactness (after source + descriptor deletion)

| fixture | src len | out len | src sha256 | sha256 match | cmp | exact | decline rc | bomb rc | slide0 text |
| --- | ---: | ---: | --- | --- | --- | --- | ---: | ---: | --- |
| `basic.odp` | 1744 | 1744 | `1d1e258582dc` | true | true | true | 6 | -1 | `Hello
World
Second paragraph` |
| `table.odp` | 1746 | 1746 | `3dc60c251243` | true | true | true | 6 | -1 | `Table
a	b
c	d` |
| `picture.odp` | 1950 | 1950 | `20f5987bd861` | true | true | true | 6 | -1 | `Picture` |
| `notes.odp` | 1678 | 1678 | `55647705cbee` | true | true | true | 6 | -1 | `Body text` |
| `order.odp` | 1695 | 1695 | `6ab262306a45` | true | true | true | 6 | -1 | `SECOND FILE` |
| `bomb.odp` | 1740 | 1740 | `62502dd3f224` | true | true | true | -1 | 8 | `SECOND FILE` |

## Counts

```
fixtures 6
exact_ok 6
exact_fail 0
typed_declines_ok 5
bomb_declines_ok 1
binary 798a6c6d2585f389d72cb653c0a773675fc1ad36c0fc88c56b1867523a9892d5
```

## Per-fixture observation files (raw/)

- `basic.odp.find.json`
- `basic.odp.masters.json`
- `basic.odp.metadata.json`
- `basic.odp.shape0.json`
- `basic.odp.slide0.json`
- `basic.odp.slide0.structure.json`
- `basic.odp.slide0.text.txt`
- `basic.odp.text.json`
- `bomb.odp.text.json`
- `notes.odp.find.json`
- `notes.odp.masters.json`
- `notes.odp.metadata.json`
- `notes.odp.notes.json`
- `notes.odp.shape0.json`
- `notes.odp.slide0.json`
- `notes.odp.slide0.structure.json`
- `notes.odp.slide0.text.txt`
- `notes.odp.text.json`
- `order.odp.find.json`
- `order.odp.masters.json`
- `order.odp.metadata.json`
- `order.odp.shape0.json`
- `order.odp.slide0.json`
- `order.odp.slide0.structure.json`
- `order.odp.slide0.text.txt`
- `order.odp.text.json`
- `picture.odp.find.json`
- `picture.odp.masters.json`
- `picture.odp.media.json`
- `picture.odp.metadata.json`
- `picture.odp.shape0.json`
- `picture.odp.slide0.json`
- `picture.odp.slide0.structure.json`
- `picture.odp.slide0.text.txt`
- `picture.odp.text.json`
- `results.json`
- `table.odp.find.json`
- `table.odp.masters.json`
- `table.odp.metadata.json`
- `table.odp.shape0.json`
- `table.odp.slide0.json`
- `table.odp.slide0.structure.json`
- `table.odp.slide0.text.txt`
- `table.odp.tables.json`
- `table.odp.text.json`

## Scope (honest)

- **Shipped here:** byte-based ODP detection; the ODF-manifest-backed
  discovery model; the `draw:page` slide inventory in document order;
  shapes (text frames, pictures, custom shapes, groups, embedded tables);
  run-level text; notes pages; master pages; styles; the `Pictures/*`
  media inventory; native `odp-slide`/`odp-shape`/`odp-notes`/
  `odp-masters`/`odp-media`/`odp-tables`/`odp-find`; common
  `metadata`/`text`/`table`/`cell`/`search-match`.
- **Exactness** is inherited from the Phase-12.2 ZIP member raw spans: the
  ODP model is derived (`Q_gen`) and never on the exactness path.
- **Not claimed here:** slide rendering, animation, transitions, OLE
  embeddings, chart data, and the economic court.
- **Never run on the host:** every command above ran in the pinned
  `doc-baseline` container.
