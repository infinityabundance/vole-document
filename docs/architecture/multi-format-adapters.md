# Multi-format adapters

Phase 12 makes the field format-universal across PDF, DOCX and EPUB: three
native inverse compilers converge on one `DocumentField` with common
observations and retained native structure. Phase 13.3 adds a fourth, ODT
(OpenDocument), Phase 21.1 a fifth, XLSX (SpreadsheetML), Phase 21.2 a sixth,
PPTX (PresentationML), and Phase 21.3 a seventh, ODS (OpenDocument Spreadsheet), and Phase 21.4 an eighth,
ODP (OpenDocument Presentation). Phase 21 Wave 2 then adds the
structured-tree (JSON, YAML, XML, TOML, JSONL), tabular (CSV/TSV), prose/web
(Markdown, HTML), messaging (EML/MIME), and analytical (Parquet, Arrow IPC)
adapters — each not a package, with the whole source as the exact leaf.
“Universal” means the observation
vocabulary is shared, not that every format is supported.

## Three representational layers (ADR-0029)

They stay distinct, reference each other, and are never conflated:

1. **Exact physical source state** — PDF spans; ZIP local headers / compressed
   member spans / data descriptors / central directory / ZIP64 / EOCD; XML member
   bytes. Normative; reconstructs `original_bytes`.
2. **Format-native procedural state** — PDF object/stream/revision; OPC
   part/relationship and WordprocessingML paragraph/table/story; EPUB
   package/manifest/spine/nav/XHTML. Normative for native observations.
3. **Shared observation vocabulary** — a small common selector/representation set
   plus per-format escape hatches. Never a lossy universal AST.

A shared Rust `Selector` enum is interface reuse, not shared semantics.

## Byte-authoritative ZIP layer (ADR-0030)

DOCX (OPC) and EPUB (OCF) share one physical scanner analogous to the PDF
scanner. It covers `[0, N)` (`Prefix · LocalHeader · MemberData ·
DataDescriptor · CentralDirectory · … · Eocd · Trailing · Unclassified`) with a
`validate()` that rejects any gap/overlap/wrong total. A member's identity is
`(archive ordinal, local-header offset)`, distinct from the advisory logical part
name; duplicate names are never normalized. The exact leaf is the raw compressed
span — no unzip/rezip. The `zip`/`rawzip` crates are oracle-only. Reject-vs-
opaque split: a broken physical cover is a typed reject; a semantics/resource
problem preserves the exact bytes and declines only the decode.

## DOCX adapter (ADR-0032)

- Main part discovered **semantically** from `_rels/.rels` (the officeDocument
  relationship), never a hardcoded `/word/document.xml`.
- WordprocessingML subset: paragraphs/runs/text, styles with heading identity via
  resolved `outlineLvl`, sections, headers/footers, notes, comments, bookmarks,
  hyperlinks, fields, tracked changes, drawings/resources, numbering. Unknown
  namespaces are preserved, never interpreted.
- Stories are explicit (`Main`, `Header`, `Footer`, `Footnote`, `Endnote`,
  `Comment`, `TextBox`, `Glossary`); text/find observations are scoped to one
  story and never silently mixed. Tables are first-class.
- Versioned extraction profiles (`DocxExtractProfile`); the profile identity is
  hashed into the canonical selector.
- XML is derived-only (`quick-xml`, no DTD/entities, UTF-8); exact XML bytes stay
  `Q_ref`.

## EPUB adapter (ADR-0033)

- OCF container: `mimetype` (first, stored, exactly 20 bytes) and
  `META-INF/container.xml` → rootfile(s); the package document is located
  semantically, never from a hardcoded `OEBPS/content.opf`.
- Spine-first reading order: `SpineItem(n)` is the format-native coordinate.
  Reflowable EPUB has **no intrinsic pages**; `Page(n)` exists only where a
  page-list nav, `epub:type="pagebreak"`, or a fixed-layout viewport defines it,
  and is never synthesized.
- Bounded XHTML observations (headings, paragraphs, lists, tables, links,
  resources, fragment ids, semantic sections; SVG/MathML preserved). No script
  execution, no remote fetch.
- Versioned `EpubExtractProfile`; exact-byte preservation and EPUB conformance
  are separate outcomes.

## ODT adapter (ADR-0038)

- OpenDocument (ODF) package: the mandatory stored `mimetype`
  (`application/vnd.oasis.opendocument.text`) and `META-INF/manifest.xml`. ODF is
  **not** OPC (no `[Content_Types].xml`, no `officeDocument` relationship), so —
  like EPUB — the adapter reuses the ZIP layer and the shared XML policy but not
  the OPC graph. The main content part is located **semantically** from the ODF
  manifest (never a hardcoded `content.xml`).
- Bounded OpenDocument content model (`office:body`/`office:text`): paragraphs,
  headings (`text:h` + `text:outline-level`), spans, lists, tables
  (column/row spans, covered cells), links, bookmarks, notes, images/resources,
  tracked changes (`text:changed-region` kinds), and sections. No intrinsic pages:
  `Page(n)` is never synthesized.
- Versioned `OdtExtractProfile` (tracked changes Final/Original/All, notes
  include/exclude, hidden, tabs, breaks); common vocabulary plus native
  `odt-part`/`odt-paragraph`/`odt-heading`/`odt-table`/`odt-cell`/`odt-list`/
  `odt-find`.
- Progressive inversion: only the requested part is parsed, on demand, and the
  canonical derived model is persisted and reused; exact leaves stay the 12.2
  member raw spans. A missing/malformed manifest is a typed decline with
  exactness preserved.

## XLSX adapter (ADR-0059)

- OPC package on the shared ZIP layer (`[Content_Types].xml`, `_rels/.rels`,
  `xl/workbook.xml`, `xl/worksheets/sheetN.xml`, `xl/styles.xml`,
  `xl/sharedStrings.xml`, `xl/comments*.xml`, `xl/tables/*`, `xl/drawings/*`,
  `xl/charts/*`, `xl/media/*`). Gated behind the **non-default** `xlsx = ["opc"]`
  feature; a build without it reports XLSX `Opaque`.
- **Five distinct cell fields, never conflated:** stored formula (`<f>`), cached
  result (`<v>`), a bounded deterministic **displayed value** (labelled
  `deterministically-derived`; **formulas never evaluated**), resolved style
  (`cellXfs`: number format, font, fill, alignment), and the exact XML span.
- Bounded SpreadsheetML model: sheets (order/name/visibility), rows/cells (shared
  vs inline, booleans, errors), merged ranges (their `ref` strings), comments
  (+VML note anchors), internal/external hyperlinks via the sheet `_rels`,
  defined/named ranges, tables, drawings/charts/media as a relationship graph,
  and package external relationships as typed metadata never dereferenced.
- Detection is byte-based and mutually exclusive with DOCX via a **positive**
  WordprocessingML main-part signal, so a Word document that embeds a workbook is
  not misclassified. Cell references use checked arithmetic and bounded
  coordinates; the sheet-text projection is bounded before it is built.
- Progressive inversion; exact leaves stay the 12.2 member raw spans. No intrinsic
  pages: `Page(n)` is a typed decline.

## PPTX adapter (ADR-0059/0060)

- OPC package; gated behind the **non-default** `pptx = ["opc"]` feature.
  Presentation part resolved semantically from `[Content_Types].xml` + rels; slide
  order from `p:sldIdLst` (never `slideN.xml` order).
- Shape tree: text shapes/run-level text, pictures (`a:blip` → media), graphic
  frames (embedded `a:tbl` / chart rel), groups (bounded recursion), connectors;
  notes, layouts, masters, themes, media, tables. Embedded-table text is part of
  the deck text projection.
- Chart data is not parsed (the reference is exposed); a decoded slide/shape XML
  digest is not exposed. Progressive inversion; exact leaves stay the 12.2 member
  raw spans.

## ODS adapter (ADR-0060)

- OpenDocument (ODF) package over the shared ZIP layer (not OPC), like ODT; main
  part from `META-INF/manifest.xml`; non-default `ods` feature.
- `office:body/office:spreadsheet`: sheets, rows, cells (typed value + displayed
  text + stored formula + style, never conflated), merges, repeated cells/rows
  (bounded — a bomb declines typed before allocation), named expressions, styles,
  comments.
- Formulas never evaluated; no ODS-native resource/media selector (recorded gap).
  Progressive inversion; exact leaves stay the 12.2 member raw spans.

## ODP adapter (ADR-0060)

- OpenDocument (ODF) package over the shared ZIP layer (not OPC), like ODT/ODS;
  main part from `META-INF/manifest.xml`; non-default `odp` feature.
- `office:body/office:presentation`: slides = `draw:page` in **document order**
  (never page-name/file order); shapes (text boxes/run-level text, images ->
  `Pictures/` via `xlink:href`, groups bounded, `draw:table`), notes, masters,
  styles, media. Embedded-table text is part of the deck text projection.
- Chart data/rendering not interpreted. Progressive inversion; exact leaves stay
  the 12.2 member raw spans.

## JSON adapter — structured tree (Phase 21 Wave 2)

- The first **structured-tree** format; not a package — the whole source is the
  document (RAW authority). Non-default, dependency-free `json` feature.
- A bounded, **representation-preserving** parser: exact token source spans,
  object member order, numeric spelling (`1e3`), string escape spelling
  (`\u00e9`), and duplicate keys kept distinct. `--json-pointer` (RFC 6901),
  `--json-node`, `--json-find`; common metadata/text/find.
- Conservative detection (the whole source must parse as one JSON value;
  malformed → Opaque). The model node depends on the `DocumentExact` root
  (ADR-0060).

## YAML adapter — structured tree (Phase 21 Wave 2)

- The second **structured-tree** format; not a package — the whole source is the
  document (RAW authority). Non-default, dependency-free `yaml` feature.
- A bounded YAML subset that **preserves representation**: exact spans, anchors and
  aliases as a graph (never expanded), tags as literal text, multiple documents,
  scalar styles (plain/single/double/literal/folded), merge keys (`<<`), mapping
  order, duplicate keys, comment spans. `--yaml-path`/`--yaml-node`/`--yaml-documents`/
  `--yaml-anchor`/`--yaml-find`; common metadata/text/find.
- Conservative detection (a mapping/sequence at every document root; plain text →
  Opaque). The model node depends on the `DocumentExact` root (ADR-0060).

## CSV/TSV adapter — tabular (Phase 21 Wave 2)

- The tabular format; not a package — the whole source is the document (RAW
  authority). Non-default, dependency-free `csv` feature.
- RFC 4180 CSV + TSV: quoted fields (`""`), embedded delimiters/newlines/quotes,
  CRLF/LF/CR, BOM, header. **Preserves exact bytes**: record/field spans + dialect;
  quoting/whitespace not normalized. Bounded memory (no full-file structure).
  `--csv-row`/`--csv-cell`/`--csv-header`/`--csv-range`/`--csv-find`.
- Conservative detection (no magic bytes; prose → Opaque). No CSV index — row/cell
  reads are O(offset) scans. Model node depends on the `DocumentExact` root
  (ADR-0060). A tabular format requires the DuckDB/Parquet comparator (ADR-0059),
  which wins storage/ingest/indexed reads on the tested corpus (recorded).

## Markdown adapter — prose (Phase 21 Wave 2)

- Prose format; not a package — the whole source is the document (RAW
  authority). Non-default, dependency-free `markdown` feature.
- A bounded, line-based CommonMark subset that keeps exact block/inline source
  spans and bytes (the source is never re-flowed): ATX headings, paragraphs,
  ordered/unordered/nested lists, fenced/indented code with language tags,
  blockquotes, GFM tables, inline/reference links and images, reference
  definitions, footnotes, and YAML/TOML front matter. Native
  `md-heading`/`md-block`/`md-code`/`md-link`/`md-find`; common
  metadata/text/heading/block/find.
- Conservative detection (a structural mark is required; plain prose → Opaque).
  The model node depends on the `DocumentExact` root (ADR-0060).

## XML adapter — structured tree (Phase 21 Wave 2)

- Structured-tree format; not a package — the whole source is the document (RAW
  authority). Non-default `xml = ["dep:quick-xml"]` feature.
- A bounded, span-preserving scanner: elements, attributes, text, CDATA,
  comments, PIs, DOCTYPE, namespace declarations (entity references literal).
  Native `xml-path`/`xml-element`/`xml-attr`/`xml-namespaces`/`xml-find`.
- **Security:** a benign DOCTYPE is accepted and never fetched; a DTD internal
  subset is **refused** (no XXE, no billion-laughs). A document-level HTML marker
  wins HTML. The model node depends on the `DocumentExact` root (ADR-0060).

## HTML adapter — error-recovering markup (Phase 21 Wave 2)

- Markup format; not a package — the whole source is the document (RAW
  authority). Non-default, dependency-free `html` feature.
- A bounded, span-preserving, error-recovering scanner: elements, attributes
  (all quoting forms), text, comments, DOCTYPE, and raw `script`/`style` bytes;
  entity references literal. Native
  `html-path`/`html-element`/`html-attr`/`html-scripts`/`html-find`; common
  metadata/text/heading/link/find.
- UTF-8 only; a DTD internal subset is refused; `script`/`style` is never
  executed. The model node depends on the `DocumentExact` root (ADR-0060).

## TOML adapter — structured tree (Phase 21 Wave 2)

- Structured-tree config format; not a package — the whole source is the
  document (RAW authority). Non-default, dependency-free `toml` feature.
- A bounded, span-preserving parser (tables, arrays of tables, dotted keys,
  inline tables, arrays, comments; every scalar's exact spelling). Native
  `toml-path`/`toml-table`/`toml-find`; common metadata/text. Duplicate keys and
  table redefinitions are typed declines (exit 26).
- The strong complete-parse signal places TOML before CSV/Markdown/XML/HTML. The
  model node depends on the `DocumentExact` root (ADR-0060).

## JSONL adapter — line/event stream (Phase 21 Wave 2)

- Line-delimited stream; not a package — the whole source is the document (RAW
  authority). Non-default `jsonl = ["json"]` feature (reuses the shared JSON
  parser).
- A bounded, per-line span-preserving model; native
  `jsonl-line`/`jsonl-pointer`/`jsonl-find`; common metadata/text. JSON is tried
  first; JSONL then requires each non-blank line to be exactly one JSON value.
  The model node depends on the `DocumentExact` root (ADR-0060).

## EML/MIME adapter — messaging (Phase 21 Wave 2)

- Messaging format; not a package — the whole source is the document (RAW
  authority). Non-default, dependency-free `eml` feature.
- A bounded, span-preserving message model: every header's exact name/value/span,
  header order, duplicate and folded headers, the resolved `multipart/*` tree,
  and the exact `Content-Transfer-Encoding`-decoded constituent bytes (incl.
  attachments). Native
  `eml-header`/`eml-part`/`eml-attachments`/`eml-body`/`eml-find`; common
  metadata/text/resource/find.
- A leading Unix-mbox `From ` envelope is skipped; a boundary-less multipart
  declines. The model node depends on the `DocumentExact` root (ADR-0060).

## Parquet adapter — analytical (Phase 21 Wave 2)

- Analytical columnar format; not a package — the whole source is the document
  (RAW authority). Non-default, dependency-free `parquet` feature (no Thrift
  library).
- A bounded Thrift-Compact footer reader: schema, row-group/column-chunk
  inventory with each chunk's exact source span and statistics, and decoded
  `PLAIN`/`RLE_DICTIONARY` values (`UNCOMPRESSED`/`GZIP`) across the common
  physical types. Native
  `parquet-schema`/`parquet-column`/`parquet-row-group`/`parquet-cell`.
- Unsupported codecs/encodings/types and bombs decline typed. The model node
  depends on the `DocumentExact` root (ADR-0060). The mandatory DuckDB comparator
  **wins the analytical axes** (recorded).

## Arrow IPC adapter — analytical (Phase 21 Wave 2)

- Analytical columnar IPC format; not a package — the whole source is the
  document (RAW authority). Non-default, dependency-free `arrow` feature.
- A bounded Flatbuffers reader: schema, record batches with exact source spans,
  and decoded primitive/binary buffers (Int all widths, FloatingPoint, Boolean,
  Date/Time/Timestamp/Duration, Utf8/Binary, FixedSizeBinary) with validity
  bitmaps; file + stream, multi-batch. Native
  `arrow-schema`/`arrow-column`/`arrow-batch`/`arrow-cell`.
- Nested types, views, dictionary-encoded fields, big-endian bodies and
  `BodyCompression` decline typed. The model node depends on the
  `DocumentExact` root (ADR-0060). The DuckDB comparator wins the analytical axes
  and reads a recorded Parquet projection of the same table.

## Structured-extra, binary, config, feed and GIS adapters (Phase 21 Wave 2)

The next eight Wave-2 subphases (21.17–21.24) reuse an existing physical layer
and pay mostly for their own span policy. All are non-default, and all keep the
exact leaf as the **whole source** (RAW authority) with the derived model off the
exactness path (ADR-0060).

- **JSON5 / JSONC** (`json5 = ["json"]`) reuses the JSON parser and span policy:
  comments with spans, unquoted keys, single quotes, trailing commas,
  hex/leading-dot/`Infinity`/`NaN` numbers, string continuations, the extended
  whitespace set, and a recorded `jsonc`-vs-`json5` dialect. Strict JSON stays
  `Json`. Native `json5-*`.
- **CBOR** (`cbor = ["json"]`) and **MessagePack** (`msgpack = ["json"]`) are the
  binary structured-tree formats. No magic bytes; each preserves the
  encoding/format byte, byte-vs-text, tags/extension types (never resolved), map
  order/duplicate keys, and float width. CBOR is tried before MessagePack (the
  self-described tag is the stronger signal); the ambiguous small-int/
  short-container seam is a recorded negative. Native `cbor-*` / `msgpack-*`.
- **Config (INI / `.env` / Java `.properties`)** (`config = []`) is one bounded
  key/value-line parser preserving the dialect, exact spans, `export`/quoting/
  continuations and `\uXXXX` spelling, with duplicate keys reported. The pure
  `KEY=VALUE` env-vs-properties overlap stays `Opaque`. Native `config-*`.
- **RSS / Atom** (`feed = ["xml"]`), **GeoJSON** (`geojson = ["json"]`),
  **KML / GPX** (`gis = ["xml"]`) and the **Jupyter notebook**
  (`notebook = ["json"]`) are **bounded semantic sub-detections** over an
  existing physical layer (XML or JSON), run before the generic detector of that
  layer: a feed needs `<rss><channel>` or an Atom `<feed>`+`<entry>`; GeoJSON a
  `"type"` among the nine RFC 7946 names with a consistent shape; KML/GPX the
  KML 2.2 / GPX 1.1 namespace with a structural child; a notebook an integer
  `nbformat` and nbformat-shaped `cells`. Plain XML/JSON stays `Xml`/`Json`.
  Native `feed-*` / `geojson-*` / `gis-*` / `notebook-*`.

## Shared vocabulary (ADR-0031)
Common selectors/representations are added additively: `metadata`, `text`,
`heading`, `block`, `table`, `cell`, `resource`, `link`, `find`. Native selectors
remain first-class peers. Widening the enums fails closed on unknown
pairs. Provenance carries through: a common observation is
`DeterministicallyDerived`/`Heuristic`, never exact, and `metadata` is a shared
selector *name* with per-format semantics. Capability gaps are explicit, not
silently approximated (see [Format support](../reference/format-support.md)).

## Format detection and ingestion

One format-agnostic `field-ingest` detects the format from bytes (never a file
name) and routes through the matching adapter. Every answer's provenance is
tagged `format=<fmt>;common;<native>`.

## Measured results (mixed)

| Court | Result | Receipt |
|---|---|---|
| Source + descriptor removal, all three formats | 38/38 byte-exact | `evidence/campaigns/2026-10-06-phase12-removal-dc4d5a3/` |
| DOCX/EPUB logical triplet equivalence | 96/96 | `evidence/campaigns/2026-10-06-phase12-triplet-dc4d5a3/` |
| PDF no-regression (`N6`, A2 vs A11) | 32/32 exact, 0 regressions | `evidence/campaigns/2026-10-06-phase12-pdf-noregression-0d23a02/` |
| Hostile ZIP/OPC/OCF/XML fixtures | 315/315 assertions | `evidence/campaigns/2026-10-06-phase12-security-33f6d04/` |
| Lifetime + ablation ladder | small-doc win; A1 wins large frontier | `evidence/campaigns/2026-10-06-phase12-lifetime-ablations-06db12a/` |
| XLSX adapter / semantic model | exact 2/2 · 3/3 | `evidence/campaigns/2026-10-08-phase21-1-xlsx-4d26514/`, `…/2026-10-09-phase21-2-xlsx-5802be9/` |
| XLSX economic court (vs SQLite + DuckDB/Parquet) | exact 8/8 | `evidence/campaigns/2026-10-09-phase21-3-xlsx-b2400f1/` |
| PPTX adapter / economic court | exact 5/5 · 8/8 | `evidence/campaigns/2026-10-09-phase21-2-pptx-8aab956/`, `…/2026-10-09-phase21-3-pptx-054ce93/` |
| ODS adapter / economic court | exact 8/8 · 8/8 | `evidence/campaigns/2026-10-09-phase21-3-ods-ef26d97/`, `…/2026-10-09-phase21-3-2-ods-3dd5827/` |
| ODP adapter / economic court | exact 6/6 · 8/8 | `evidence/campaigns/2026-10-09-phase21-4-1-odp-957a800/`, `…/2026-10-09-phase21-4-odp-econ-957a800/` |
| Wave-2 adapters (Markdown/XML/HTML/TOML/JSONL/EML/Parquet/Arrow) | exact 12/12·10/10·10/10·10/10·9/9·7/7·13/13·18/18 | `evidence/campaigns/2026-10-10-phase21-8-1-markdown-0e6a97c3/` … `…/2026-10-10-phase21-16-1-arrow-4fdc7ad0/` |
| Wave-2 adapters (JSON5/CBOR/MessagePack/config/feed/GeoJSON/GIS/notebook) | exact 10/10·16/16·17/17·11/11·7/7·8/8·8/8·7/7 | `evidence/campaigns/2026-10-10-phase21-17-1-json5-88b30730/` … `…/2026-10-10-phase21-24-1-notebook-b6860e87/` |
| Stratified real-world multi-format court (52 real + 8 hostile) | 60/60 byte-exact | `evidence/campaigns/2026-10-10-realformats-stratified-3529688e/` |

Interpretation. The ablation ladder attributes the small-document win to the
content adapters (A4→A5) and to persistent semantic reuse (A5→A6, which trades
bytes for CPU); EntropyFS (A9) is a loss and `A7`/`A8` are not separable. The
source-retaining SQLite+FTS5 baseline (A1) wins the large-document frontier and
wall/CPU at N=1000. The cross-format equivalence is a self-authored,
generator-defined triplet (adapter consistency, not third-party independence).

Limitations. All corpora are locally generated (841 B–61 KB); “large” means large
in that corpus. The demo’s A0/A1 baselines pay Python interpreter startup, which
flatters VOLE. Cross-document durable work reuse is a recorded negative
([Persistence and caching](persistence-and-caching.md)). See
[phase-12-results.md](../phases/phase-12-results.md) and the independent review
[phase-12-skeptic-review.md](../reviews/phase-12-skeptic-review.md).

## Relevant ADRs

[0009](../adr/0009-pdf-byte-authority.md),
[0024](../adr/0024-document-field-authority.md),
[0029](../adr/0029-multi-format-authority-model.md),
[0030](../adr/0030-zip-physical-layer.md),
[0031](../adr/0031-common-observation-model.md),
[0032](../adr/0032-docx-adapter-scope.md),
[0033](../adr/0033-epub-adapter-scope.md),
[0035](../adr/0035-phase12-lifetime-benchmark.md),
[0038](../adr/0038-odt-adapter-scope.md),
[0059](../adr/0059-xlsx-adapter-and-analytical-comparator.md),
[0060](../adr/0060-source-scoped-node-identity.md).
