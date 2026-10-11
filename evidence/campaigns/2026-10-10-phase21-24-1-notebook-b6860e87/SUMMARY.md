# Phase 21.24.1 — Jupyter notebook (nbformat) court

**Question.** Does the notebook adapter close exactly and expose a
representation-preserving model (the exact nbformat/nbformat_minor; the exact
cell_type string; the exact source representation — a string vs an array of
lines, never re-joined; the exact execution_count; cell/output order; metadata;
attachments; and every output type with its fields) on top of the whole-source
exact leaf — while keeping the bounded semantic sub-detection boundary (before
the generic JSON detector) and declining malformed/ambiguous inputs typed?

**Method.** Each self-authored fixture is generated in-court with GNU
`/usr/bin/printf`, ingested via `field-build`, observed, then — after the
**source file and the standalone descriptor are deleted** — rematerialized
exactly in a fresh process and compared with `length + SHA-256 + cmp`. An
out-of-range cell/output and a cell with no `source` are required to decline
typed; the plain-JSON / bare-cells / non-object-cells controls and prose pin the
detection boundaries. The court runs in the pinned `dev` service using only
POSIX `sh`, coreutils, git, and the shipped binary (no python3, no jq).

## Exactness (after source + descriptor deletion)

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `main.ipynb` | notebook | 819 | 819 | true | true | true | 6 | -1 |
| `v3.ipynb` | notebook | 153 | 153 | true | true | true | 6 | -1 |
| `min.ipynb` | notebook | 25 | 25 | true | true | true | 6 | -1 |
| `plain.json` | json | 17 | 17 | true | true | true | -1 | 6 |
| `cells.json` | json | 32 | 32 | true | true | true | -1 | 6 |
| `unrelated.json` | json | 30 | 30 | true | true | true | -1 | 6 |
| `prose.txt` | opaque | 50 | 50 | true | true | true | -1 | 6 |

## Counts

```
fixtures 7
notebook_fixtures 3
exact_ok 7
exact_fail 0
surface_fail 0
typed_decline_cells_ok 3
typed_decline_outputs_ok 3
typed_decline_source_less_ok 1
json_controls_ok 3
usage_ok 1
opaque_controls_ok 1
opaque_controls 1
binary cd870560aef649e962225c65c7c87e08d3d6c5b0721dade5c4017427c0d98b7d
```

## Per-fixture observation files (raw/)

- `build.log`
- `cells.json.notebook-decline.json`
- `main.ipynb.cell1.json`
- `main.ipynb.celltype0.bin`
- `main.ipynb.celltype0.json`
- `main.ipynb.find.json`
- `main.ipynb.metadata.json`
- `main.ipynb.nbformat.bin`
- `main.ipynb.nbformat.json`
- `main.ipynb.out0.json`
- `main.ipynb.out1.json`
- `main.ipynb.out2.json`
- `main.ipynb.out3.json`
- `main.ipynb.search.json`
- `main.ipynb.source0.json`
- `main.ipynb.source1.json`
- `main.ipynb.text.json`
- `min.ipynb.find.json`
- `min.ipynb.metadata.json`
- `min.ipynb.nbformat.bin`
- `min.ipynb.nbformat.json`
- `min.ipynb.search.json`
- `min.ipynb.text.json`
- `plain.json.notebook-decline.json`
- `prose.txt.metadata.json`
- `results.tsv`
- `unrelated.json.notebook-decline.json`
- `v3.ipynb.find.json`
- `v3.ipynb.metadata.json`
- `v3.ipynb.nbformat.bin`
- `v3.ipynb.nbformat.json`
- `v3.ipynb.search.json`
- `v3.ipynb.source-decline.json`
- `v3.ipynb.text.json`

## Scope (honest)

- **Shipped here:** byte-based bounded semantic notebook detection; one bounded
  notebook model **reusing the shared JSON parser** (never a second JSON
  parser); native `notebook-nbformat`/`notebook-cell`/`notebook-cell-type`/
  `notebook-cell-source`/`notebook-cell-output`/`notebook-find`; common
  `metadata`/`text`/`search-match`.
- **Precedence:** after the strong magic-byte binaries (PDF/ZIP/Parquet/Arrow)
  and after GeoJSON; immediately before the generic JSON detector. A notebook is
  a more specific claim than a bare JSON value, so it is tried first.
- **Detection boundary:** a notebook's physical bytes are JSON. A notebook is
  claimed only when the root object has a plain non-negative integer-literal
  `nbformat` (>= 1) and an array `cells`, every cell an object with a string
  `cell_type`, and every present recognized field nbformat-shaped (a
  string-or-array-of-strings `source`, a number-or-null `execution_count`, object
  `metadata`/`attachments`, an array `outputs` of objects with a string
  `output_type`). A plain JSON document, a JSON document that merely has a
  `cells` key, a JSON document whose `cells` are not objects, and a non-integer
  `nbformat` all stay `Json`; prose stays `Opaque`.
- **Recorded negatives (not distinguished):** `cell_type`/`output_type` strings
  are preserved but not restricted to the known set; `nbformat` is not restricted
  to a version; a `source` string is never split and a `source` array is never
  joined (the two forms cannot be conflated); a number literal is never reparsed
  (so `4.0`/`4e0`/`-0` is not an integer `nbformat` and stays `Json`).
- **Declines:** an out-of-range cell/output, a cell with no `source`, a
  malformed `--notebook-cell-output` argument, a cap breach, and a non-notebook
  source are typed (`InvalidNotebookStructure` rc 38, unsupported-feature rc 6,
  resource-limit rc 8, or usage rc 2); such input stays `Json`/`Opaque` when
  detection declines.
- **Exactness** is the whole source (a RAW-like `DocumentExact`): the notebook
  model is derived (`Q_gen`) and never on the exactness path (ADR-0060: the
  model node depends on the `sha256(source)` root).
- **Not claimed here:** executing a notebook, validating every nbformat schema
  rule, the economic court (separate script).
- **Never run on the host:** every command above ran in the pinned `dev`
  container.
