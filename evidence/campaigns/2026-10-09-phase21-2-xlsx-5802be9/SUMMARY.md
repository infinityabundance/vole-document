# Phase 21.1.2 — XLSX (SpreadsheetML) semantic court

**Question.** Does the XLSX field expose the *spreadsheet* structure — cell
styles, merges, comments, hyperlinks, defined names, tables, drawings/charts,
external relationships — as distinct observations, while still closing exactly
(`materialize == source`, length + SHA-256 + `cmp`) after source + descriptor
deletion?

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
| `semantic.xlsx` | 7027 | 7027 | `d641caf10f9d` | true | true | true | 6 |

| contract check | result |
| --- | --- |
| merged-range REF exposed (semantic A5:B5) | true |
| no-drawing explicit absence (single.xlsx, rc 0) | true |
| unsupported-pair typed decline rc 6 (all 3 fixtures) | true |

## Counts

```
fixtures 3
exact_ok 3
exact_fail 0
typed_declines_ok 3
style_ok true
display_ok true
comment_ok true
link_ok true
defined_ok true
table_ok true
drawing_ok true
external_ok true
merge_ref_ok true
no_drawing_rc 0
no_drawing_ok true
binary 108232e0839426d7cf1a0c621f1ce7affe007938b4d9e0de2b0527ce12558c71
```

## Observation surface (semantic.xlsx, raw/)

```
{
  "fixtures": 3,
  "exact_ok": 3,
  "exact_fail": 0,
  "typed_declines_ok": 3,
  "merge_ref_ok": true,
  "no_drawing_rc": 0,
  "no_drawing_ok": true,
  "display_ok": true,
  "drawing_ok": true,
  "external_ok": true,
  "comment_ok": true,
  "defined_ok": true,
  "table_ok": true,
  "link_ok": true,
  "style_ok": true
}
```

## Per-fixture observation files (raw/)

- `multi.xlsx.field.txt`
- `multi.xlsx.metadata.json`
- `multi.xlsx.observe_field.txt`
- `multi.xlsx.sheet0.json`
- `multi.xlsx.text.json`
- `results.json`
- `semantic.cellB2.json`
- `semantic.comments.json`
- `semantic.defined-names.json`
- `semantic.drawing.decoded.bin`
- `semantic.drawing.json`
- `semantic.external-rels.json`
- `semantic.hyperlinks.json`
- `semantic.sheet0.metadata.json`
- `semantic.styles.json`
- `semantic.tables.json`
- `semantic.xlsx.field.txt`
- `semantic.xlsx.metadata.json`
- `semantic.xlsx.observe_field.txt`
- `semantic.xlsx.sheet0.json`
- `semantic.xlsx.text.json`
- `single.drawing.none.json`
- `single.xlsx.field.txt`
- `single.xlsx.metadata.json`
- `single.xlsx.observe_field.txt`
- `single.xlsx.sheet0.json`
- `single.xlsx.text.json`

## Scope (honest)

- **Shipped here:** a richer cell-style table (custom number formats, fonts,
  fills, alignment) exposed as `xlsx-styles` and resolved per cell; merged
  ranges; cell comments (`xl/comments*.xml`, keyed by cell) + VML note anchors;
  internal (`location`) and external (`r:id`) hyperlinks resolved through the
  sheet `_rels`; defined/named ranges; tables (name/ref/columns) via
  `tableParts`; drawings/charts/media exposed as a relationship graph (the
  drawing part's exact/decoded bytes resolve; charts are never evaluated); and
  package external relationships as typed metadata (never dereferenced).
- **Distinctions:** a cell's stored formula, cached result, deterministic
  number-format display projection, style, and comment are separate fields.
  The display projection is a bounded, labelled (`deterministically-derived`)
  computation — a bounded grammar over a small set of number-format codes;
  formulas are never evaluated.
- **Declines and absences (typed, never silent):** unsupported selector/
  representation pairs decline typed (rc 6); requesting BYTES for a part that
  does not exist declines typed; an element that *references* a missing part
  (e.g. a `tableParts` rel whose target is absent) declines typed as invalid
  package structure (rc 20); and a metadata observation for an OPTIONAL part
  that is simply absent (no styles part, no drawing element, no comments rel)
  answers an explicit absence at rc 0 (a `present:false` / `null` value) —
  an answer, not a decline.
- **Merged ranges** are exposed as their `ref` strings (a JSON array) with a
  machine-readable `merge_count`, not as a bare count.
- **Exactness** is inherited from the Phase-12.2 ZIP member raw spans: every
  XLSX model here is derived (`Q_gen`) and never on the exactness path.
- **Not claimed here:** formula evaluation, chart data/axis semantics, pivot
  tables, rich-text/comment formatting beyond plain text, shared-formula
  arrays, and the economic court. Those are later subphases.
- **Never run on the host:** every command above ran in the pinned
  `doc-baseline` container.
