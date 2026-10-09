# Phase 21.2.1 — PPTX (PresentationML) court

**Question.** Does the second format of the format programme close exactly
and expose a real PresentationML model on the shared OPC surface?

**Method.** Each self-authored PPTX fixture (generated deterministically by
`tools/fixtures/make-pptx.py`, Python stdlib only) is ingested via
`field-build`, observed, then — after the **source file and the standalone
descriptor are deleted** — rematerialized exactly in a fresh process and
compared with `length + SHA-256 + cmp`. Unsupported common pairs are asked
for and required to decline typed. Slide order is proved to follow
`p:sldIdLst` (the `order.pptx` fixture reverses the file names).

## Exactness (after source + descriptor deletion)

| fixture | src len | out len | src sha256 | sha256 match | cmp | exact | decline rc |
| --- | ---: | ---: | --- | --- | --- | --- | ---: |
| `basic.pptx` | 5173 | 5173 | `f7bc75b22018` | true | true | true | 6 |
| `table.pptx` | 4279 | 4279 | `006cf59580f9` | true | true | true | 6 |
| `picture.pptx` | 4406 | 4406 | `488ab8309673` | true | true | true | 6 |
| `notes.pptx` | 5348 | 5348 | `685dcf91e407` | true | true | true | 6 |
| `order.pptx` | 5080 | 5080 | `9ff794588eb8` | true | true | true | 6 |

## Counts

```
fixtures 5
exact_ok 5
exact_fail 0
typed_declines_ok 5
sldidlst_order_ok true
table_text_ok true
store_regular_file_bytes 108285
binary 4b06d8871ee5da256d6ec64ed72fd8ce57b6ca8f78afc21d4f60b8b67b5cc538
```

## Per-fixture observation files (raw/)

- `basic.pptx.find.json`
- `basic.pptx.layouts.json`
- `basic.pptx.masters.json`
- `basic.pptx.metadata.json`
- `basic.pptx.shape0.json`
- `basic.pptx.slide0.json`
- `basic.pptx.tables.json`
- `basic.pptx.text.json`
- `basic.pptx.theme.json`
- `notes.pptx.find.json`
- `notes.pptx.layouts.json`
- `notes.pptx.masters.json`
- `notes.pptx.metadata.json`
- `notes.pptx.shape0.json`
- `notes.pptx.slide0.json`
- `notes.pptx.tables.json`
- `notes.pptx.text.json`
- `notes.pptx.theme.json`
- `order.pptx.find.json`
- `order.pptx.layouts.json`
- `order.pptx.masters.json`
- `order.pptx.metadata.json`
- `order.pptx.shape0.json`
- `order.pptx.slide0.json`
- `order.pptx.slide1.json`
- `order.pptx.tables.json`
- `order.pptx.text.json`
- `order.pptx.theme.json`
- `picture.pptx.find.json`
- `picture.pptx.layouts.json`
- `picture.pptx.masters.json`
- `picture.pptx.metadata.json`
- `picture.pptx.shape0.json`
- `picture.pptx.slide0.json`
- `picture.pptx.tables.json`
- `picture.pptx.text.json`
- `picture.pptx.theme.json`
- `results.json`
- `table.pptx.find.json`
- `table.pptx.layouts.json`
- `table.pptx.masters.json`
- `table.pptx.metadata.json`
- `table.pptx.shape0.json`
- `table.pptx.slide0.json`
- `table.pptx.tables.json`
- `table.pptx.text.json`
- `table.pptx.theme.json`

## Scope (honest)

- **Shipped here:** byte-based PPTX detection; the OPC-backed presentation
  discovery model; the parsed presentation inventory (slide size, slide
  order from `p:sldIdLst`); each slide's shape tree (text shapes, run-level
  text, a picture, a group, a connector, an embedded `a:tbl` table); the
  notes slides; native `pptx-slide`/`pptx-shape`/`pptx-notes`/
  `pptx-layouts`/`pptx-masters`/`pptx-theme`/`pptx-media`/`pptx-tables`/
  `pptx-find`; common `metadata`/`text`/`table`/`cell`/`search-match`.
- **Embedded-table text** is part of the slide/deck `text` projection
  (matching DOCX, where a table block's text is part of the body); the
  `table.pptx` court asserts the exact projected string.
- **Exactness** is inherited from the Phase-12.2 ZIP member raw spans: the
  PPTX model is derived (`Q_gen`) and never on the exactness path.
- **Not claimed here:** chart/diagram rendering, SmartArt, animations,
  transitions, speaker-notes rendering, and image decoding. Those are out of
  scope for this subphase.
- **Never run on the host:** every command above ran in the pinned
  `doc-baseline` container.
