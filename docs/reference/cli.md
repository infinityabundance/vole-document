# CLI reference

The `vole-document` binary is the only user-facing entry point. Commands are
selected by the first argument; the *format* of an input is detected from its
bytes, never from the file extension or a flag.

Feature-gated command groups appear only when built in: the entropy stack
(`rans`, default), the object store (`store`, default), and the document field
(`field`, default; DOCX/EPUB/ODT need `package,opc,docx,epub,odt`).

## Core container

```text
vole-document encode      [--force KIND] INPUT  OUTPUT.voldoc
vole-document decode      INPUT.voldoc   OUTPUT
vole-document materialize INPUT.voldoc   OUTPUT
vole-document verify      INPUT.voldoc
vole-document inspect     INPUT.voldoc
vole-document capabilities [ROOT]
```

- `encode` runs the complete-cost court and writes the smallest exact
  descriptor. `--force KIND` restricts the court to one candidate family for
  honest per-mechanism ablation; it never bypasses exactness, and it fails with
  a typed usage error if the input does not propose that kind.
- `decode` and `materialize` are synonyms and produce the exact source bytes.
- `verify` (re)checks integrity and the coverage certificate without writing.
- `inspect` prints descriptor structure. `capabilities ROOT` detects `ROOT`'s
  document format from bytes and prints the supported selectors/representations.

`KIND` for `encode --force`:

```text
raw | rle | byte-rans | pdf-physical | pdf-channels | pdf-layout |
pdf-layout-rans | pdf-deflate-replay | pdf-deflate-replay-rans |
pdf-deflate-replay-rans-indexed | pdf-length-revision | pdf-cos-template
```

## Partial views (`rans`)

```text
vole-document view INPUT.voldoc [OUTPUT] --byte-range A:L | --pdf-object N:G |
    --pdf-stream N:G | --pdf-revision I [--stats]
```

Serves one byte range, indirect object, encoded stream, or revision from the
advisory seek `DIRECTORY` and observation index, checking every served slice
against the full materialization. Exactly one selector is required. A partial
read is an *observation* (`integrity_verified == false`); `materialize`/`decode`/
`verify` remain the archival authority.

## Object store (`store`)

```text
vole-document decode --store STORE_DIR INPUT.voldoc OUTPUT
vole-document store put     INPUT.voldoc STORE_DIR
vole-document store account STORE_DIR ROOT...
vole-document store gc      STORE_DIR ROOT...
```

`--store` externalizes the descriptor's objects into a content-addressed store;
`store account` reports the three accounting universes (standalone,
unique-reachable, amortized); `store gc` is a mark-and-sweep over the referenced
roots.

## Persistent document field (`field`)

```text
vole-document field-build  INPUT --store DIR [--profile runtime] [--workers N]
                           [--voldoc OUT.voldoc] [--entropyfs | --packed] [--sync=batch|each]
vole-document field-ingest INPUT.voldoc --store DIR [--workers N] [--entropyfs | --packed] [--sync=batch|each]
vole-document field-edit   --store DIR --field HEX --page N --content FILE [--entropyfs | --packed]
vole-document observe      --store DIR --field HEX [--entropyfs | --packed] [--promote[=BYTES]]
    (--page N | --object N | --stream N | --revision N | --revisions | --byte-range A..B |
    --metadata | --doc-text | --heading N | --block N | --table N | --cell T:R:C |
    --resource N | --link N | --spine-item N | --text PATTERN |
    --sheet N | --xlsx-cell A1 [--sheet N] | --xlsx-styles | --xlsx-defined-names |
    --xlsx-external-rels | --xlsx-comments | --xlsx-hyperlinks | --xlsx-tables | --xlsx-drawing |
    --slide N | --pptx-shape N | --pptx-notes N | --pptx-layouts | --pptx-masters |
    --pptx-theme | --pptx-media N | --pptx-tables | --pptx-find PATTERN |
    --ods-sheet N | --ods-cell A1 [--ods-sheet N] | --ods-styles | --ods-named-expressions |
    --ods-comments | --ods-find PATTERN |
    --odp-slide N | --odp-shape N | --odp-notes N | --odp-masters | --odp-media N |
    --odp-tables | --odp-find PATTERN |
    --json-pointer P | --json-node P | --json-find PATTERN |
    --json5-pointer P | --json5-node P | --json5-comments | --json5-find PATTERN |
    --cbor-pointer P | --cbor-node P | --cbor-find PATTERN |
    --msgpack-pointer P | --msgpack-node P | --msgpack-find PATTERN |
    --config-line N | --config-entry N | --config-section NAME | --config-find PATTERN |
    --feed-channel | --feed-field NAME | --feed-entry N | --feed-entry-field N:NAME | --feed-find PATTERN |
    --geojson-type | --geojson-feature N | --geojson-geometry N | --geojson-coordinates N |
    --geojson-property N:NAME | --geojson-find PATTERN |
    --gis-root | --gis-field NAME | --gis-record N | --gis-record-field N:NAME | --gis-point N |
    --gis-find PATTERN |
    --notebook-nbformat | --notebook-cell N | --notebook-cell-type N | --notebook-cell-source N |
    --notebook-cell-output N:M | --notebook-find PATTERN |
    --yaml-path P | --yaml-node | --yaml-documents | --yaml-anchor NAME | --yaml-find PATTERN |
    --csv-row N | --csv-cell R:C | --csv-cell R:COLNAME | --csv-header | --csv-range | --csv-find PATTERN |
    --fixedwidth-row N | --fixedwidth-cell R:C | --fixedwidth-header | --fixedwidth-columns |
    --fixedwidth-range R1:C1:R2:C2 | --fixedwidth-find PATTERN |
    --md-heading N | --md-block N | --md-code | --md-link N | --md-find PATTERN |
    --rst-heading N | --rst-block N | --rst-directive N | --rst-inline N | --rst-find PATTERN |
    --adoc-heading N | --adoc-block N | --adoc-attribute N | --adoc-inline N | --adoc-find PATTERN |
    --mdx-heading N | --mdx-block N | --mdx-esm N | --mdx-jsx N | --mdx-expression N |
    --mdx-find PATTERN |
    --mhtml-root | --mhtml-resource N | --mhtml-location | --mhtml-find PATTERN |
    --xml-path P | --xml-element | --xml-attr NAME | --xml-namespaces | --xml-find PATTERN |
    --html-path P | --html-element | --html-attr NAME | --html-scripts | --html-find PATTERN |
    --toml-path P | --toml-table | --toml-find PATTERN |
    --jsonl-line N | --jsonl-pointer P | --jsonl-find PATTERN |
    --logstream-line N | --logstream-field N:ROLE | --logstream-find PATTERN |
    --pkgmeta-section N | --pkgmeta-entry N | --pkgmeta-key SECTION:KEY | --pkgmeta-find PATTERN |
    --apispec-dialect | --apispec-version | --apispec-object N | --apispec-ref N | --apispec-find PATTERN |
    --bib-dialect | --bib-entry N | --bib-field N:FIELD | --bib-key KEY | --bib-find PATTERN |
    --rtf-header | --rtf-group N | --rtf-control N | --rtf-hex N | --rtf-unicode N |
    --rtf-text N | --rtf-dest NAME[:N] | --rtf-find PATTERN |
    --container-dialect | --container-entry N | --container-find PATTERN |
    --hdf-dialect | --hdf-object N | --hdf-dimension N | --hdf-attribute N | --hdf-find PATTERN |
    --cfbf-dialect | --cfbf-entry N | --cfbf-stream N | --cfbf-find PATTERN |
    --eml-header NAME | --eml-part N | --eml-attachments | --eml-body | --eml-find PATTERN |
    --doc-fib | --doc-piece N | --doc-body | --doc-section NAME | --doc-find PATTERN |
    --xls-biff | --xls-sheet N | --xls-cell N | --xls-record N | --xls-text | --xls-find PATTERN |
    --ppt-document | --ppt-slide N | --ppt-run N | --ppt-record N | --ppt-list NAME |
    --ppt-text | --ppt-find PATTERN |
    --msg-summary | --msg-property N | --msg-recipient N | --msg-attachment N | --msg-text |
    --msg-find PATTERN |
    --parquet-schema | --parquet-column | --parquet-row-group | --parquet-cell |
    --arrow-schema | --arrow-column | --arrow-batch | --arrow-cell)
    --kind KIND
vole-document observe-batch --store DIR --field HEX [--entropyfs | --packed] [--promote[=BYTES]]
    [--requests FILE|-] [--repeat N]
vole-document find         --store DIR --field HEX --text PATTERN [--entropyfs | --packed]
vole-document explain      --store DIR --field HEX <selector> --kind KIND [--analyze] [--entropyfs | --packed]
vole-document preview      --store DIR --field HEX --page N [--json] [--entropyfs | --packed]
vole-document materialize  --store DIR --field HEX --exact --output FILE [--entropyfs | --packed]
vole-document field-incremental --store DIR --field HEX [--lane full|cas|incremental]
    [--index DIR] [--format auto|csv|json|docx|xlsx] [--verify] [--max-observations N]
    [--entropyfs | --packed]
vole-document cache        --store DIR [--clear] [--entropyfs | --packed]
vole-document field-store-stats --store DIR [--entropyfs | --packed]
```

- `field-build` is the **direct** source → field path: it runs one court over ONE
  fixed, non-searched exactness program (`--profile runtime`), stores the exact
  authority, and inverts it into the field in a single process — no compression
  portfolio search. Measured 2.04× faster and 3.4× lower peak RSS than
  `encode` + `field-ingest`, with identical exactness and observations (ADR-0051).
- The XLSX selectors (`--sheet`, `--xlsx-cell`, `--xlsx-styles`, `--xlsx-comments`,
  `--xlsx-hyperlinks`, `--xlsx-tables`, `--xlsx-drawing`, `--xlsx-defined-names`,
  `--xlsx-external-rels`) need the non-default `xlsx` feature; a build without it
  reports XLSX `Opaque`. A cell's stored formula, cached result, displayed value,
  style, and XML span are separate fields; formulas are never evaluated (ADR-0059).
- The PPTX selectors (`--slide`, `--pptx-shape`, `--pptx-notes`, `--pptx-layouts`,
  `--pptx-masters`, `--pptx-theme`, `--pptx-media`, `--pptx-tables`, `--pptx-find`)
  need the non-default `pptx` feature. Slide order comes from `p:sldIdLst`, never
  `slideN.xml` file order; chart data is not evaluated (`--slide` exposes the
  chart *reference*).
- The ODS selectors (`--ods-sheet`, `--ods-cell`, `--ods-styles`,
  `--ods-named-expressions`, `--ods-comments`, `--ods-find`) need the non-default
  `ods` feature. A cell's stored formula, typed value, displayed text, style and
  XML span are separate fields; formulas are never evaluated.
- The ODP selectors (`--odp-slide`, `--odp-shape`, `--odp-notes`, `--odp-masters`,
  `--odp-media`, `--odp-tables`, `--odp-find`) need the non-default `odp` feature.
  Slide order is `draw:page` document order, never page-name/file order.
- The JSON selectors (`--json-pointer` RFC 6901, `--json-node`, `--json-find`) need
  the non-default, dependency-free `json` feature. The adapter preserves
  representation: numeric/escape spelling, member order, duplicate keys, and exact
  token source spans.
- The YAML selectors (`--yaml-path`, `--yaml-node`, `--yaml-documents`,
  `--yaml-anchor NAME`, `--yaml-find`) need the non-default `yaml` feature. Anchors
  and aliases are exposed as a graph (never expanded); tags, scalar styles, merge
  keys and comments are preserved.
- The CSV/TSV selectors (`--csv-row`, `--csv-cell`, `--csv-header`, `--csv-range`,
  `--csv-find`) need the non-default `csv` feature. Record/field bytes and the
  dialect are preserved; there is no CSV index, so the row/cell selectors are
  O(offset) bounded-memory scans.
- The Markdown selectors (`--md-heading`, `--md-block`, `--md-code`, `--md-link`,
  `--md-find`) need the non-default, dependency-free `markdown` feature. Blocks
  carry exact source spans and bytes; the source is never re-flowed.
- The XML selectors (`--xml-path`, `--xml-element`, `--xml-attr`,
  `--xml-namespaces`, `--xml-find`) need the non-default `xml` feature. A DTD
  internal subset is refused, so no entity is ever resolved.
- The HTML selectors (`--html-path`, `--html-element`, `--html-attr`,
  `--html-scripts`, `--html-find`) need the non-default, dependency-free `html`
  feature. `script`/`style` content is captured as raw bytes, never executed; a
  document-level HTML marker wins HTML over the XML fallback.
- The TOML selectors (`--toml-path`, `--toml-table`, `--toml-find`) need the
  non-default, dependency-free `toml` feature. Duplicate keys and table
  redefinitions are typed declines, never silently collapsed.
- The JSONL selectors (`--jsonl-line`, `--jsonl-pointer`, `--jsonl-find`) need the
  non-default `jsonl` feature (which implies `json`). Each non-blank line must be
  exactly one JSON value.
- The EML selectors (`--eml-header`, `--eml-part`, `--eml-attachments`,
  `--eml-body`, `--eml-find`) need the non-default, dependency-free `eml`
  feature. Header spans/order/duplicates and the resolved MIME tree are preserved.
- The Parquet selectors (`--parquet-schema`, `--parquet-column`,
  `--parquet-row-group`, `--parquet-cell`) need the non-default, dependency-free
  `parquet` feature. Unsupported codecs/encodings/types and bombs decline typed.
- The Arrow selectors (`--arrow-schema`, `--arrow-column`, `--arrow-batch`,
  `--arrow-cell`) need the non-default, dependency-free `arrow` feature. The
  detector admits the Arrow IPC **file** format (`ARROW1` magic with a consistent
  footer) and the magic-less encapsulated-message **stream** framing; the ordered
  message inventory (Schema / each batch / end-of-stream, exact spans contiguous
  from offset 0 and covering the whole source) is served through the common
  `metadata` observation. Nested types, dictionary-encoded fields, big-endian
  bodies, and `BodyCompression` decline typed.
- The analytical formats (Parquet, Arrow IPC) and the tabular CSV/TSV court carry
  the mandatory **DuckDB/Parquet** comparator; it wins the analytical axes, which
  is recorded rather than hidden (ADR-0059).
- The JSON5/JSONC selectors (`--json5-pointer` RFC 6901, `--json5-node`,
  `--json5-comments`, `--json5-find`) need `json5 = ["json"]`; CBOR
  (`--cbor-pointer`, `--cbor-node`, `--cbor-find`) needs `cbor = ["json"]`; and
  MessagePack (`--msgpack-pointer`, `--msgpack-node`, `--msgpack-find`) needs
  `msgpack = ["json"]` — all non-default and dependency-free, reusing the JSON
  match vocabulary. Each preserves its exact encoding form (the JSON5/JSONC
  dialect, CBOR major type/width/tag, MessagePack format byte/`str`-vs-`bin`).
- The config selectors (`--config-line`, `--config-entry`, `--config-section`,
  `--config-find`) need the non-default, dependency-free `config` feature
  (INI/`.env`/Java properties). The feed selectors (`--feed-channel`,
  `--feed-field`, `--feed-entry`, `--feed-entry-field`, `--feed-find`) need
  `feed = ["xml"]`; GeoJSON (`--geojson-type`, `--geojson-feature`,
  `--geojson-geometry`, `--geojson-coordinates`, `--geojson-property`,
  `--geojson-find`) needs `geojson = ["json"]`; KML/GPX (`--gis-root`,
  `--gis-field`, `--gis-record`, `--gis-record-field`, `--gis-point`,
  `--gis-find`) needs `gis = ["xml"]`; and the Jupyter notebook
  (`--notebook-nbformat`, `--notebook-cell`, `--notebook-cell-type`,
  `--notebook-cell-source`, `--notebook-cell-output`, `--notebook-find`) needs
  `notebook = ["json"]` — all non-default.
- The fixed-width selectors (`--fixedwidth-row`, `--fixedwidth-cell`,
  `--fixedwidth-header`, `--fixedwidth-columns`, `--fixedwidth-range`,
  `--fixedwidth-find`) need the non-default `fixedwidth = ["csv"]` feature; the
  PSV (pipe) dialect lives inside the existing `csv` feature.
- The prose-extra selectors need non-default features: reStructuredText
  (`--rst-heading`, `--rst-block`, `--rst-directive`, `--rst-inline`,
  `--rst-find`) needs `rst`; AsciiDoc (`--adoc-heading`, `--adoc-block`,
  `--adoc-attribute`, `--adoc-inline`, `--adoc-find`) needs `asciidoc`; and MDX
  (`--mdx-heading`, `--mdx-block`, `--mdx-esm`, `--mdx-jsx`, `--mdx-expression`,
  `--mdx-find`) needs `mdx = ["markdown"]`, reusing the Markdown parser/model.
  MHTML (`--mhtml-root`, `--mhtml-resource`, `--mhtml-location`, `--mhtml-find`)
  needs `mhtml = ["eml", "html"]`, reusing the EML MIME layer and the HTML
  scanner (before the generic EML detector).
- The line/event-stream selectors (`--logstream-line`, `--logstream-field`,
  `--logstream-find`) need the non-default, dependency-free `logstream` feature;
  package metadata (`--pkgmeta-section`, `--pkgmeta-entry`, `--pkgmeta-key`,
  `--pkgmeta-find`) needs `pkgmeta = ["json", "toml"]`; API/spec
  (`--apispec-dialect`, `--apispec-version`, `--apispec-object`, `--apispec-ref`,
  `--apispec-find`) needs `apispec = ["json"]`; and bibliography (`--bib-dialect`,
  `--bib-entry`, `--bib-field`, `--bib-key`, `--bib-find`) needs
  `bib = ["json"]`.
- The word-processor/container selectors: RTF (`--rtf-header`, `--rtf-group`,
  `--rtf-control`, `--rtf-hex`, `--rtf-unicode`, `--rtf-text`, `--rtf-dest`,
  `--rtf-find`) needs the non-default, dependency-free `rtf` feature; the
  archive container (`--container-dialect`, `--container-entry`,
  `--container-find`) needs `container = ["package"]`; the scientific container
  (`--hdf-dialect`, `--hdf-object`, `--hdf-dimension`, `--hdf-attribute`,
  `--hdf-find`) needs `hdf`; and the CFBF/OLE2 physical layer (`--cfbf-dialect`,
  `--cfbf-entry`, `--cfbf-stream`, `--cfbf-find`) needs `cfbf` — all non-default.
- The legacy Office/MSG semantic selectors (Phase 26/27) build on `cfbf` and are
  non-default: `--doc-fib`/`--doc-piece`/`--doc-body`/`--doc-section`/`--doc-find`
  need `doc`; `--xls-biff`/`--xls-sheet`/`--xls-cell`/`--xls-record`/`--xls-text`/
  `--xls-find` need `xls`; `--ppt-document`/`--ppt-slide`/`--ppt-run`/
  `--ppt-record`/`--ppt-list`/`--ppt-text`/`--ppt-find` need `ppt`; and
  `--msg-summary`/`--msg-property`/`--msg-recipient`/`--msg-attachment`/
  `--msg-text`/`--msg-find` need `msg` (each `= ["cfbf"]`). `--doc-section NAME`
  selects one of `main`/`footnote`/`header`/`annotation`/`endnote`/`textbox`/
  `hdrtextbox`; `--ppt-list NAME` one of `slides`/`masters`/`notes`/`handout`. The
  native DOC text selector is `--doc-body` (canonical `doc-text`). Each model
  declines out-of-subset constructs typed and never panics.
- `field-ingest` inverse-proceduralizes one `.voldoc` into a persistent field
  and prints its field id (`HEX`) and roots. The format is detected from bytes.
- `observe` / `find` / `explain` / `preview` answer typed observations with
  provenance; `--kind` is one of
  `metadata|text|structure|operators|encoded|decoded|exact|preview|full|lineage`.
- `--revisions --kind lineage` answers a PDF's revision lineage (header, revision
  count, ordered indices, byte spans, resolved `startxref`/`/Prev`, per-revision
  object/stream membership) from an ingest-once node; `--revision N --kind lineage`
  scopes it to one revision. It is a PDF-native observation: every non-PDF format
  is a **typed decline** (ADR-0052).
- `observe-batch` serves many observations in **one** process (one JSON answer
  per line; each request line is the per-observation flag grammar *without*
  `--store`/`--field`). A resident session, not a wire/decoder change
  (ADR-0042).
- `explain --analyze` executes the plan and reports the actual work (bytes read
  by class, nodes executed vs reused, decodes, wall/CPU).
- `materialize --exact` reconstructs the original bytes from the field alone.
- `field-incremental` is the Phase-29 revision-aware incremental-observation
  instrument (`src/field/incremental.rs`): it reuses revision N's verified derived
  observations for revision N+1's field **only** where every **physical and
  semantic** dependency is unchanged, keyed by a versioned durable index
  (`VOLEINC1`). `--lane cas` is a physical-only versioned content-addressed
  **control** (the lane that diverges wrong on a style/shared-string change);
  `--lane full` rebuilds everything. It emits an answering-integrity JSON receipt
  and is **observe-only** — it never touches the exactness path. Outside the
  supported subset (an unparseable container, a non-stored member, an
  observation count over the bound) it declines typed.
- `--workers N` parallelizes independently decodable ingest work (`N`=absent or
  `1` is serial, `0` means `available_parallelism`); it requires the **non-default**
  `parallel` feature, is speed-neutral on the tested corpus, and is exactly
  deterministic across worker counts (ADR-0044).
- `--packed` replaces the `seed/` namespace with a packed `fieldpack/` store
  (ADR-0043); it is mutually exclusive with `--entropyfs`. `--sync=batch|each`
  selects the packed writer's durability policy (ADR-0053): `batch` (**default**)
  syncs once per segment — records are made durable at seal or before a manifest
  is published — while `each` restores one `fdatasync` per seed node. Both
  backends serve every read command including `observe-batch`.
- `--promote[=BYTES]` opts into the durable, byte-budgeted promotion layer over
  reused intermediates (Phase 15.6). It is **off by default**, never on the
  exactness path, and was refuted on the tested corpus (ADR-0046).
- `--entropyfs` requires a build with the `entropyfs-store` feature.

## Features relevant to the field

- `parallel` (non-default) enables `--workers`; `field` implies `memmem-scan`
  (`memchr`).
- The shipped inflate backend is **`zlib-rs`** (adopted in Phase 16.1; the
  byte-identity witness is `tests/deflate_backend_equivalence.rs`), owned by one
  helper in `src/field/inflate.rs`.
- `miniz-simd` enables `miniz_oxide`'s SIMD adler-32 path (output-preserving).
- `deflate-ablation` builds the `examples/deflate_ablation.rs` harness and pulls
  the measurement-only candidate backends (`zlib-rs`, `zune-inflate`); it adds no
  decoder behavior (ADR-0045, fulfilled by 16.1).

## Fine-unit sharing (`field`)

```text
vole-document share report INPUT.voldoc...
vole-document share-account --store DIR INPUT.voldoc...
vole-document share externalize --store DIR INPUT.voldoc OUTPUT.voldoc
```

## Tooling subcommands

```text
vole-document pdf-inspect INPUT
vole-document pdf-make-samples DIR
vole-document pdf-make-large DIR [OBJECTS]
vole-document deflate-stats INPUT...        # only with deflate-replay
vole-document arrow-fused --store DIR --field HEX --column SPEC --gt N \
    [--evidence-cap K] [--warmup W] [--repeat R] [--also-current]
vole-document fused-scan --store DIR --field HEX --format FORMAT --column SPEC \
    --pred PRED --op OP [--kind int|float] [--group-cap K] [--warmup W] \
    [--repeat R] [--also-current] [--entropyfs | --packed]
vole-document coarse-index --store DIR --field HEX [--entropyfs | --packed]
vole-document coarse-read  --store DIR --field HEX [--use-map] --requests FILE [-]
    [--entropyfs | --packed]
```

These generate deterministic corpora and diagnostics for the courts; `pdf-make-large`
assembles a valid classic-xref PDF of `OBJECTS` distinct zlib streams (≥32 MiB).
`arrow-fused` is the **Phase-28.5 decision instrument** (not a shipped default
mechanism): it runs the bounded native single-column predicate/aggregate kernel
and reports per-iteration microseconds as JSON, and `--also-current` interleaves
the shipped in-process `observe` path in the same process for the paired ablation.
A column whose type the kernel does not model (utf8/binary/bool/temporal/
compressed/nested) declines typed.
`fused-scan` is the **Phase-29.1 generalized** kernel: `FORMAT` is
`arrow|parquet|csv|fixedwidth`, `PRED` is `any|gt:K|eq:K|range:LO:HI`, `OP` is
`count|sum|min|max|avg|grouped`; it is validity-aware (SQL null semantics), decodes
only the addressed column, and declines typed on any unsupported type/encoding.
`coarse-index` builds and persists the Phase-29.2 coarse structural directory
(`VCI1`) for a field; `coarse-read` serves pre-registered narrow requests from
whole native units of the verbatim authority with real `pread` range reads
(`--use-map` replays the planned units). Both degrade to the full read, never a
wrong answer.

## Exit codes

```text
0 ok                  2 usage               3 io
4 invalid-container   5 unsupported-version 6 unsupported-feature
7 integrity-mismatch  8 resource-limit      9 invalid-graph
10 invalid-model      15 coverage-violation 16 reconstruction-mismatch
70 internal-invariant
```

File output is written atomically: a temporary sibling is written, fsynced, then
renamed into place, so a partial or failed operation never replaces the
destination.
