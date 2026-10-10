# Format support

Capability matrix for the implemented formats. `✓` = supported and
answerable; `—` = a typed decline (`unsupported-feature`, exit 6), never a
silent empty answer. The format is detected from bytes, never a file name.

## Exactness and detection

| Property | PDF | DOCX | EPUB | ODT |
|---|---|---|---|---|
| Byte-based detection | `%PDF-` + indirect object + `%%EOF` | OPC ZIP package | OCF ZIP container | ODF ZIP package (`mimetype`/manifest) |
| Physical authority | owned lexer + span scanner | shared byte-authoritative ZIP | shared byte-authoritative ZIP | shared byte-authoritative ZIP |
| `materialize == original` (length + SHA-256 + `cmp`) | ✓ | ✓ | ✓ | ✓ |
| Exact after source **and** descriptor deletion, fresh process | ✓ | ✓ | ✓ | ✓ |

## Common observation vocabulary

The shared vocabulary is added for all four formats (ADR-0031); a common
observation is a *projection*, never the document model.

| Common observation | PDF | DOCX | EPUB | ODT |
|---|---|---|---|---|
| `metadata` | ✓ (descriptor identity) | ✓ (story structure) | ✓ (package summary) | ✓ (content summary) |
| `text` | ✓ | ✓ | ✓ | ✓ |
| `find` (lexical search) | ✓ | ✓ | ✓ | ✓ |
| `heading` | — | ✓ | ✓ | ✓ |
| `block` (paragraph-ish) | — | ✓ | ✓ | ✓ |
| `table` / `cell` | — | ✓ | ✓ | ✓ |
| `resource` | — | ✓ | ✓ | ✓ |
| `link` | — | ✓ | ✓ | ✓ |

`metadata` is a shared *selector name* with per-format semantics, not a shared
meaning: the PDF lane returns its own `source_len`/`source_sha256`, DOCX a story
structure, EPUB a package summary, ODT a content summary.

## Native observations

| Native surface | PDF | DOCX | EPUB | ODT |
|---|---|---|---|---|
| Object / stream / revision | ✓ | — | — | — |
| Story | — | ✓ | — | — |
| Paragraph / heading / table / cell | — | ✓ | ✓ | ✓ |
| Package part / relationship | — | ✓ | ✓ | ✓ (`odt-part`) |
| Manifest / spine item / nav | — | — | ✓ | ✓ (ODF manifest) |
| Members (raw / decoded) | — | ✓ | ✓ | ✓ |
| Page | ✓ (intrinsic) | — (no pagination) | only when the publication defines it | — (no pagination) |
| Byte range | ✓ | ✓ | ✓ | ✓ |

Reflowable EPUB has no intrinsic pages, DOCX has no intrinsic pagination, and ODT
has no intrinsic pagination; `Page(n)` is never synthesized (ADR-0033/0038).

## Exactness after removal — receipts

| Court | Result | Receipt |
|---|---|---|
| Source + descriptor removal (all three formats) | 38/38 | `evidence/campaigns/2026-10-06-phase12-removal-dc4d5a3/` |
| DOCX/EPUB logical triplet equivalence | 96/96 | `evidence/campaigns/2026-10-06-phase12-triplet-dc4d5a3/` |
| PDF no-regression (A2 vs A11) | 32/32 exact, 0 regressions | `evidence/campaigns/2026-10-06-phase12-pdf-noregression-0d23a02/` |
| Hostile ZIP/OPC/OCF/XML fixtures | 315/315 assertions | `evidence/campaigns/2026-10-06-phase12-security-33f6d04/` |
| ODT adapter (detection/content/profiles/exactness/removal/decline) | 9/9 | `evidence/campaigns/2026-10-07-phase13-odt-95c486d/` |
| XLSX adapter (OPC surface + exact closure) | exact 2/2 | `evidence/campaigns/2026-10-08-phase21-1-xlsx-4d26514/` |
| XLSX semantic model + exact closure | exact 3/3 | `evidence/campaigns/2026-10-09-phase21-2-xlsx-5802be9/` |
| XLSX economic court (vs source-retaining SQLite + DuckDB/Parquet) | exact 8/8 | `evidence/campaigns/2026-10-09-phase21-3-xlsx-b2400f1/` |
| PPTX adapter (OPC/PresentationML + exact closure + model) | exact 5/5 | `evidence/campaigns/2026-10-09-phase21-2-pptx-8aab956/` |
| PPTX economic court (vs source-retaining SQLite) | exact 8/8 | `evidence/campaigns/2026-10-09-phase21-3-pptx-054ce93/` |
| ODS adapter (ODF surface + model) | exact 8/8 | `evidence/campaigns/2026-10-09-phase21-3-ods-ef26d97/` |
| ODS economic court (vs source-retaining SQLite + DuckDB/Parquet) | exact 8/8 | `evidence/campaigns/2026-10-09-phase21-3-2-ods-3dd5827/` |
| ODP adapter / economic court | exact 6/6 · 8/8 | `evidence/campaigns/2026-10-09-phase21-4-1-odp-957a800/`, `…/2026-10-09-phase21-4-odp-econ-957a800/` |
| JSON adapter / economic court | exact 8/8 · 7/7 | `evidence/campaigns/2026-10-09-phase21-5-1-json-0d94667/`, `…/2026-10-09-phase21-5-json-econ-0d94667/` |
| YAML adapter / economic court | exact 10/10 · 8/8 | `evidence/campaigns/2026-10-09-phase21-6-1-yaml-ceab8ec/`, `…/2026-10-09-phase21-6-yaml-econ-ceab8ec/` |
| CSV/TSV adapter / economic court (SQLite + DuckDB) | exact 10/10 · 6/6 | `evidence/campaigns/2026-10-09-phase21-7-1-csv-49f523ba/`, `…/2026-10-09-phase21-7-csv-econ-49f523ba/` |
| Markdown adapter / economic court (SQLite + render) | exact 12/12 · 9/9 | `evidence/campaigns/2026-10-10-phase21-8-1-markdown-0e6a97c3/`, `…/2026-10-10-phase21-8-markdown-econ-0e6a97c3/` |
| XML adapter / economic court (SQLite + ElementTree) | exact 10/10 · 6/6 | `evidence/campaigns/2026-10-10-phase21-9-1-xml-4e3c3fbb/`, `…/2026-10-10-phase21-9-xml-econ-4e3c3fbb/` |
| HTML adapter / economic court (SQLite + html.parser) | exact 10/10 · 6/6 | `evidence/campaigns/2026-10-10-phase21-10-1-html-f04d7545/`, `…/2026-10-10-phase21-10-html-econ-f04d7545/` |
| TOML adapter / economic court (SQLite + tomllib) | exact 10/10 · 7/7 | `evidence/campaigns/2026-10-10-phase21-11-1-toml-eb045c17/`, `…/2026-10-10-phase21-11-toml-econ-eb045c17/` |
| JSONL adapter / economic court (SQLite + per-line load) | exact 9/9 · 6/6 | `evidence/campaigns/2026-10-10-phase21-12-1-jsonl-92e34118/`, `…/2026-10-10-phase21-12-jsonl-econ-92e34118/` |
| EML/MIME adapter / economic court (SQLite + `email`) | exact 7/7 · 6/6 | `evidence/campaigns/2026-10-10-phase21-13-1-eml-02dc88a7/`, `…/2026-10-10-phase21-13-eml-econ-02dc88a7/` |
| Parquet adapter / economic court (SQLite + DuckDB/Parquet) | exact 13/13 · 7/7 | `evidence/campaigns/2026-10-10-phase21-14-1-parquet-bfe32a33/`, `…/2026-10-10-phase21-14-parquet-econ-bfe32a33/` |
| Arrow IPC adapter / economic court (SQLite + DuckDB projection) | exact 18/18 · 3/3 | `evidence/campaigns/2026-10-10-phase21-16-1-arrow-4fdc7ad0/`, `…/2026-10-10-phase21-16-arrow-econ-4fdc7ad0/` |
| Cross-field identity court (ADR-0060) | 12/12 | `evidence/campaigns/2026-10-09-identity-3400385/`, re-sealed `…/2026-10-10-identity-7f007e2e/` |
| Real-format smoke (independently sourced) | 12/12 exact | `evidence/campaigns/2026-10-09-realformats-smoke-3400385/` |
| Stratified real-world multi-format court (52 real + 8 hostile) | 60/60 byte-exact | `evidence/campaigns/2026-10-10-realformats-stratified-3529688e/` (prior `…/2026-10-10-realformats-stratified-92edce73/`) |
| JSON comparator repair (modern SQLite+JSONB, span-preserving baseline) | exact 7/7 | `evidence/campaigns/2026-10-09-phase21-5-json-econ-3400385/` |
| YAML span-preserving comparator (parity with the JSON court) | exact 8/8 | `evidence/campaigns/2026-10-10-phase21-6-yaml-econ-92edce73/` |
| CSV O(n²)→O(n) metadata fix (`csv_common_metadata`) | large.csv metadata ~16.5 min → ~0.5 s | `evidence/campaigns/2026-10-10-phase21-7-1-csv-3006e56a/`, `…/2026-10-10-phase21-7-csv-econ-3006e56a/` |

See [Status ledger](../project/status.md) for the full mechanism table and
[Conformance](../reference/conformance.md) for the courts.

## XLSX (SpreadsheetML)

XLSX enters through the shared OPC layer (ADR-0030) with a **SpreadsheetML**
native model. The `xlsx` feature is **non-default** (`xlsx = ["opc"]`); detection
is byte-based and mutually exclusive with DOCX (a positive WordprocessingML
main-part signal, so a Word document that *embeds* an Excel workbook is still
`docx`).

| Property | XLSX |
|---|---|
| Byte-based detection | OPC ZIP package declaring the SpreadsheetML workbook main content type (`...spreadsheetml.sheet.main+xml`) |
| Physical authority | shared byte-authoritative ZIP; the exact leaf is the raw member span (no unzip/rezip) |
| `materialize == original` (length + SHA-256 + `cmp`) | ✓, incl. after source + descriptor deletion in a fresh process |
| Common observations | `metadata`, `text`, `table`, `cell` |
| Native observations | `xlsx-sheet` (`--sheet`), `xlsx-cell` (`--xlsx-cell A1`), `xlsx-styles`, `xlsx-comments`, `xlsx-hyperlinks`, `xlsx-tables`, `xlsx-drawing`, `xlsx-defined-names`, `xlsx-external-rels` |
| `Page(n)` | — typed decline (a spreadsheet has no intrinsic pagination) |

The **crucial distinctions** are never conflated: a cell's *stored formula*,
its *cached result*, its deterministic number-format *displayed value*, its
*style*, and its underlying *XML span* are separate fields with different
provenance. The displayed value is a bounded, labelled
(`deterministically-derived`) projection; formulas are **never evaluated**.
Every non-exactness answer is a derived projection (`exact == false`); only
the materialized workbook is a byte-authority claim. Declines are typed: an
unsupported selector/representation pair, a BYTES request for a missing part,
and a reference to a missing part all decline typed; a metadata observation for
an optional part that is simply absent answers an explicit absence at exit 0.
See [Formats/XLSX](../formats/xlsx.md) and [ADR-0059](../adr/0059-xlsx-adapter-and-analytical-comparator.md).

## PPTX (PresentationML)

PPTX enters through the shared OPC layer (ADR-0030) with a **PresentationML**
native model. The `pptx = ["opc"]` feature is **non-default**; detection is
byte-based (a positive PresentationML main-part content type) and mutually
exclusive with DOCX/XLSX, so a Word or Excel document that embeds a deck is not
misclassified.

| Property | PPTX |
|---|---|
| Byte-based detection | OPC ZIP package declaring the PresentationML main content type (`...presentationml.presentation.main+xml`) |
| Physical authority | shared byte-authoritative ZIP; exact leaf is the raw member span |
| `materialize == original` (length + SHA-256 + `cmp`) | ✓, incl. after source + descriptor deletion in a fresh process |
| Common observations | `metadata`, `text`, `table`, `cell` |
| Native observations | `pptx-slide` (`--slide N`), `pptx-shape`, `pptx-notes`, `pptx-layouts`, `pptx-masters`, `pptx-theme`, `pptx-media`, `pptx-tables`, `pptx-find` |
| `Page(n)` | — typed decline (slides are addressed by `--slide`, never synthesized) |

Slide order comes from `p:sldIdLst`, never `slideN.xml` file-name order. Every
non-exactness answer is a derived projection (`exact == false`); chart data is not
parsed (the slide exposes the chart *reference*). See [Formats/PPTX](../formats/pptx.md).

## ODS (OpenDocument Spreadsheet)

ODS enters through the bounded OpenDocument (ODF) inverse over the shared ZIP
layer (not OPC), exactly like ODT. The `ods = ["opc"]` feature is **non-default**.

| Property | ODS |
|---|---|
| Byte-based detection | ODF package whose mandatory `mimetype` (or `META-INF/manifest.xml`) declares `application/vnd.oasis.opendocument.spreadsheet` |
| Physical authority | shared byte-authoritative ZIP; exact leaf is the raw member span |
| `materialize == original` (length + SHA-256 + `cmp`) | ✓, incl. after source + descriptor deletion in a fresh process |
| Common observations | `metadata`, `text`, `table`, `cell`, `find` |
| Native observations | `ods-sheet` (`--ods-sheet N`), `ods-cell`, `ods-styles`, `ods-named-expressions`, `ods-comments`, `ods-find` |
| `Page(n)` | — typed decline (no intrinsic pagination) |

A cell's stored formula, typed value, displayed text, style, and XML span are
separate fields; formulas are never evaluated. `table:number-columns-repeated`/
`-rows-repeated` expansion is bounded (a bomb declines typed before allocation).
See [Formats/ODS](../formats/ods.md).

## ODP (OpenDocument Presentation)

ODP enters through the bounded OpenDocument (ODF) inverse over the shared ZIP
layer (not OPC), like ODT/ODS. The `odp = ["opc"]` feature is **non-default**.

| Property | ODP |
|---|---|
| Byte-based detection | ODF package whose mandatory `mimetype` (or manifest) declares `application/vnd.oasis.opendocument.presentation` |
| Physical authority | shared byte-authoritative ZIP; exact leaf is the raw member span |
| `materialize == original` (length + SHA-256 + `cmp`) | ✓, incl. after source + descriptor deletion in a fresh process |
| Common observations | `metadata`, `text`, `table`, `cell`, `find` |
| Native observations | `odp-slide` (`--odp-slide N`), `odp-shape`, `odp-notes`, `odp-masters`, `odp-media`, `odp-tables`, `odp-find` |
| `Page(n)` | — typed decline (slides are addressed by `--odp-slide`) |

Slide order is `draw:page` **document order**, never page-name/file order. Chart
data and rendering are not interpreted. See [Formats/ODP](../formats/odp.md).

This completes the six office formats from two shared package substrates (OPC:
DOCX/XLSX/PPTX; ODF: ODT/ODS/ODP). The Wave-2 families follow, across the
structured-tree, tabular, prose/web, messaging and analytical layers below.

## JSON (structured tree)

JSON is the first **structured-tree** format (Wave 2). It is not a package: the
whole source is the document, so the exact leaf is the source itself. The
`json = []` feature is **non-default and dependency-free**.

| Property | JSON |
|---|---|
| Byte-based detection | not PDF/ZIP, and the *whole* source parses as exactly one JSON value under the caps (conservative; malformed → Opaque) |
| Physical authority | the whole source (RAW authority) |
| `materialize == original` (length + SHA-256 + `cmp`) | ✓, incl. after source + descriptor deletion in a fresh process |
| Common observations | `metadata`, `text`, `find` |
| Native observations | `json-pointer` (`--json-pointer P`, RFC 6901), `json-node`, `json-find` |
| Representation preserved | numeric spelling (`1e3`), string escape spelling (`\u00e9`), member order, **duplicate keys kept distinct**, exact token source spans |

The representation-preservation is the point: SQLite's `json1`/`jsonb` normalizes
spelling and collapses duplicate keys, where VOLE surfaces them. See
[Formats/JSON](../formats/json.md).

## YAML (structured tree)

YAML is the second structured-tree format (Wave 2); the `yaml = []` feature is
**non-default and dependency-free**. Detection is conservative (the whole source
must parse as YAML with a mapping/sequence at every document root; a bare scalar
or plain text → Opaque).

| Property | YAML |
|---|---|
| Physical authority | the whole source (RAW authority) |
| `materialize == original` (length + SHA-256 + `cmp`) | ✓, incl. after source + descriptor deletion in a fresh process |
| Common observations | `metadata`, `text`, `find` |
| Native observations | `yaml-path`, `yaml-node`, `yaml-documents`, `yaml-anchor` (`--yaml-anchor NAME`), `yaml-find` |
| Representation preserved | exact spans; **anchors/aliases as a graph** (never expanded); tags; multiple documents; scalar styles (`|`, `>`, quoted); merge keys (`<<`); mapping order; duplicate keys; comment spans |

A normalizing "YAML→JSON" pipeline expands anchors/aliases and merges `<<`, drops
tags/styles/comments — the adapter surfaces them instead. See
[Formats/YAML](../formats/yaml.md).

## CSV / TSV (tabular)

CSV/TSV is the tabular family (Wave 2); the `csv = []` feature is **non-default and
dependency-free**. Detection is conservative (CSV has no magic bytes): a delimiter
(comma or tab) is accepted only if it yields a modal field count ≥ 2 over a strict
majority of a sampled prefix; prose/one-column/malformed → Opaque.

| Property | CSV / TSV |
|---|---|
| Physical authority | the whole source (RAW authority) |
| `materialize == original` (length + SHA-256 + `cmp`) | ✓, incl. after source + descriptor deletion in a fresh process |
| Common observations | `metadata`, `text`, `table`, `cell`, `find` |
| Native observations | `csv-row`, `csv-cell` (`R:C` or `R:COLNAME`), `csv-header`, `csv-range`, `csv-find` |
| Representation preserved | exact record/field byte spans; dialect (delimiter/quote/line-terminator); quoting, CRLF, BOM kept |

**Honest cost:** VOLE has **no CSV index** — `csv-row`/`csv-cell` are O(offset)
bounded-memory scans, and the mandatory **DuckDB/Parquet** comparator wins storage,
ingest, and indexed point reads on the tested corpus. See
[Formats/CSV](../formats/csv.md).

## Markdown (prose)

Markdown is the Wave-2 prose format (`markdown = []`, **non-default and
dependency-free**). It has **no magic bytes** and plain prose is a valid
Markdown paragraph, so detection requires a **structural mark** (ATX heading,
fenced code, front matter, table, reference/footnote definition); plain prose
stays Opaque.

| Property | Markdown |
|---|---|
| Physical authority | the whole source (RAW authority) |
| `materialize == original` (length + SHA-256 + `cmp`) | ✓, incl. after source + descriptor deletion in a fresh process |
| Common observations | `metadata`, `text`, `heading`, `block`, `find` |
| Native observations | `md-heading`, `md-block`, `md-code`, `md-link`, `md-find` |
| Representation preserved | exact block/inline byte spans and bytes; the source is never re-flowed; headings/levels, lists, fenced code + language, blockquotes, GFM tables, links/images, reference definitions, footnotes, front matter |

See [Formats/Markdown](../formats/markdown.md).

## XML (structured tree)

XML carries namespaces/attributes/DTD policy (`xml = ["dep:quick-xml"]`,
**non-default**). Detection is conservative and byte-based; a bare XML tree that
only *mentions* `<html>` as a non-root descendant stays Xml. A benign DOCTYPE is
accepted and never fetched; a **DTD internal subset is refused** (no entity is
ever resolved).

| Property | XML |
|---|---|
| Physical authority | the whole source (RAW authority) |
| `materialize == original` (length + SHA-256 + `cmp`) | ✓, incl. after source + descriptor deletion in a fresh process |
| Common observations | `metadata`, `text`, `find` |
| Native observations | `xml-path`, `xml-element`, `xml-attr`, `xml-namespaces`, `xml-find` |
| Representation preserved | exact element/attribute/text/CDATA/comment/PI spans; namespaces; entity references literal (never resolved) |

**Declined:** XPath beyond the native path selector, schema/DTD validation, C14N,
XInclude, encodings beyond UTF-8. See [Formats/XML](../formats/xml.md).

## HTML (error-recovering markup)

HTML is the Wave-2 error-recovering markup format (`html = []`, **non-default and
dependency-free**). A document-level HTML marker (`<!doctype html>` or an `<html>`
root) **wins HTML over the generic XML fallback**, so XHTML is HTML, not XML.

| Property | HTML |
|---|---|
| Physical authority | the whole source (RAW authority) |
| `materialize == original` (length + SHA-256 + `cmp`) | ✓, incl. after source + descriptor deletion in a fresh process |
| Common observations | `metadata`, `text`, `heading`, `link`, `find` |
| Native observations | `html-path`, `html-element`, `html-attr`, `html-scripts`, `html-find` |
| Representation preserved | exact element/attribute/text/comment/DOCTYPE spans; raw `script`/`style` bytes; entity references literal |

**Declined:** the full tree-construction recovery algorithm (adoption agency /
foster parenting), CSS/JS interpretation, encodings beyond UTF-8. `script`/`style`
content is captured as raw bytes and never executed. See
[Formats/HTML](../formats/html.md).

## TOML (structured tree)

TOML is the Wave-2 config/structured-tree format (`toml = []`, **non-default and
dependency-free**). The *whole* source must parse as TOML; the strong
complete-parse signal means TOML is tried before CSV/Markdown/XML/HTML (a
column-0 `#` comment would otherwise look like a Markdown heading).

| Property | TOML |
|---|---|
| Physical authority | the whole source (RAW authority) |
| `materialize == original` (length + SHA-256 + `cmp`) | ✓, incl. after source + descriptor deletion in a fresh process |
| Common observations | `metadata`, `text` |
| Native observations | `toml-path`, `toml-table`, `toml-find` |
| Representation preserved | exact key/value/table spans; every scalar's exact spelling |

**Declined:** duplicate keys and table redefinitions (exit 26), calendar
validation of date-times, encodings beyond UTF-8. See
[Formats/TOML](../formats/toml.md).

## JSONL / NDJSON (line/event stream)

JSONL is the Wave-2 line/event-stream format (`jsonl = ["json"]`, **non-default**);
it reuses the shared JSON parser. JSON is tried first (a single value, even across
lines, stays Json); JSONL then requires **each non-blank line to parse as exactly
one JSON value**.

| Property | JSONL |
|---|---|
| Physical authority | the whole source (RAW authority) |
| `materialize == original` (length + SHA-256 + `cmp`) | ✓, incl. after source + descriptor deletion in a fresh process |
| Common observations | `metadata`, `text` |
| Native observations | `jsonl-line`, `jsonl-pointer`, `jsonl-find` |
| Representation preserved | per-line record spans; each record parsed by the shared JSON parser (numeric/escape spelling, member order, duplicate keys preserved) |

**Declined:** fewer than two records, a malformed line, an over-cap budget, a
non-newline-separated bag. See [Formats/JSONL](../formats/jsonl.md).

## EML / MIME (messaging)

EML/MIME is the Wave-2 messaging format (`eml = []`, **non-default and
dependency-free**). A leading Unix-mbox `From ` envelope is skipped before the
header scan; EML is tried after JSON/JSONL and before
YAML/TOML/CSV/Markdown/XML/HTML.

| Property | EML / MIME |
|---|---|
| Physical authority | the whole source (RAW authority) |
| `materialize == original` (length + SHA-256 + `cmp`) | ✓, incl. after source + descriptor deletion in a fresh process |
| Common observations | `metadata`, `text`, `resource`, `find` |
| Native observations | `eml-header`, `eml-part`, `eml-attachments`, `eml-body`, `eml-find` |
| Representation preserved | every header's exact name/value/span, header order, **duplicate and folded headers**, the resolved `multipart/*` tree, exact `Content-Transfer-Encoding`-decoded constituent bytes |

**Declined:** a multipart without a boundary (detection declines, consistent
with parse), an unknown transfer encoding, a non-UTF-8 charset for text,
encrypted/signed S/MIME. See [Formats/EML](../formats/eml.md).

## Parquet (analytical)

Parquet is the Wave-2 analytical format (`parquet = []`, **non-default and
dependency-free** — no Thrift library).

| Property | Parquet |
|---|---|
| Byte-based detection | `PAR1` magic at both ends + a readable Thrift-Compact footer |
| Physical authority | the whole source (RAW authority) |
| `materialize == original` (length + SHA-256 + `cmp`) | ✓, incl. after source + descriptor deletion in a fresh process |
| Common observations | `metadata`, `text`, `table`, `cell`, `find` |
| Native observations | `parquet-schema`, `parquet-column`, `parquet-row-group`, `parquet-cell` |
| Representation preserved | schema; row-group/column-chunk inventory with each chunk's **exact source span** and statistics; decoded `PLAIN`/`RLE_DICTIONARY` values (`UNCOMPRESSED`/`GZIP`) |

**Declined typed:** `INT96`, `DATA_PAGE_V2`, deprecated `BIT_PACKED` levels, the
`DELTA_*`/`GROUP_VAR_INT`/`BYTE_STREAM_SPLIT` encodings, the
`SNAPPY`/`ZSTD`/`BROTLI`/`LZO`/`LZ4` codecs, repeated/nested columns, legacy
`min`/`max` statistics; a bomb declines typed before allocation. **DuckDB wins the
analytical axes** (projection, predicates, compressed pages, metadata-statistic
selective reads); VOLE's unique claims are exact closure and the exact chunk
span. See [Formats/Parquet](../formats/parquet.md).

## Arrow IPC (analytical)

Arrow IPC is the second Wave-2 analytical format (`arrow = []`, **non-default and
dependency-free**).

| Property | Arrow IPC |
|---|---|
| Byte-based detection | Arrow file magic/footer (`ARROW1`) or stream markers + a readable Flatbuffers schema/footer |
| Physical authority | the whole source (RAW authority) |
| `materialize == original` (length + SHA-256 + `cmp`) | ✓, incl. after source + descriptor deletion in a fresh process |
| Common observations | `metadata`, `text`, `table`, `cell`, `find` |
| Native observations | `arrow-schema`, `arrow-column`, `arrow-batch`, `arrow-cell` |
| Representation preserved | schema; record batches with exact source spans; decoded primitive/binary buffers (Int all widths, FloatingPoint, Boolean, Date/Time/Timestamp/Duration, Utf8/Binary, FixedSizeBinary) with validity bitmaps; file + stream, multi-batch |

**Declined typed:** `Null`, `Decimal`, `Interval`, every nested type, view types,
dictionary-encoded fields, big-endian bodies, `BodyCompression`; a bomb declines
typed. **DuckDB wins the analytical axes** and reads a **Parquet projection** of
the same logical table (the pinned wheel has no Arrow IPC file reader) — the
substitution is recorded. See [Formats/Arrow](../formats/arrow.md).

> **Comparator repairs (JSON/YAML).** The JSON economic court was **repaired**
> after review (SQLite **3.51.1 with JSONB** via a pinned `pysqlite3-binary` wheel;
> duplicate-key accounting fixed with `json_tree`; an independent
> **span-preserving** baseline added). Against that serious competitor VOLE
> **matches it on every question it answers** — the earlier "wins" on spans (Q2),
> duplicate keys (Q4) and raw tokens (Q8) are **withdrawn**; its remaining
> differentiators are exact closure and economics. `[SUPERSEDED:
> evidence/campaigns/2026-10-09-phase21-5-json-econ-0d94667]`.
> The YAML court was then given the **same treatment** (21.6.2): a hash-pinned
> modern SQLite with `jsonb` **and** a span-preserving pure-Python YAML scanner;
> VOLE's representation advantages there also shrink/vanish (Q2 spanpy 8 equal;
> Q3 1 equal/7 both-decline; Q4 7 equal/1 both-decline; Q6 8 equal; Q7 1
> equal/7 both-decline; **0 mismatches**), leaving exact closure + economics.
> `[SUPERSEDED: evidence/campaigns/2026-10-09-phase21-6-yaml-econ-ceab8ec]`.

## Detection repair (Phase 21.15)

Five detection defects the stratified court surfaced were fixed (all 60 samples
remain byte-exact): **OPC/ODF** well-known control parts resolve
**case-insensitively** (`[content_types].xml`); the ODF mandatory **`mimetype` is
authoritative** over the manifest (an ODP embedding a spreadsheet no longer goes
ambiguous); an **HTML document marker wins over the generic XML fallback**
(well-formed XHTML → HTML); a Unix-mbox `From ` envelope is skipped and a
boundary-less `multipart/*` declines; and TOML accepts the **structural
terminators** `,`/`]`/`}` after a boolean. Re-sealed in the stratified campaign
`2026-10-10-realformats-stratified-3529688e` (60/60 byte-exact, 0 skipped).

## Feature gates

The default build is `default = ["rans", "store", "field"]`. The ZIP/DOCX/EPUB/ODT
adapters need `package,opc,docx,epub,odt`; the XLSX adapter needs `package,opc,xlsx`
and the PPTX adapter `package,opc,pptx` (the non-default `xlsx`/`pptx` features);
the ODS adapter needs `package,opc,ods`, the ODP adapter `package,opc,odp`, and the
JSON adapter the dependency-free `json`, and the YAML adapter `yaml`; the
CSV/TSV adapter `csv`. The Wave-2 fine-grained adapters are the dependency-free
`markdown`, `html`, `toml`, `eml`, `parquet` and `arrow` features (plus
`jsonl = ["json"]` and `xml = ["dep:quick-xml"]`), all **non-default**.
`deflate-replay` is opt-in (pulls LGPL
`cabac`). A descriptor that needs a capability the build lacks sets a mandatory
feature bit and fails closed with `unsupported-feature` (exit 6).

Phase 15 adds performance-only, non-default features that change **no** wire bytes,
no persisted artifact, and no decoder behavior:

| Feature | What it enables | Deps |
|---|---|---|
| `parallel` | `--workers N` bounded parallel ingest (ADR-0044) | `rayon` |
| `memmem-scan` | SIMD `memchr::memmem::Finder` for the PDF `find_endstream` scan (implied by `field`/`package`) | `memchr` |
| `miniz-simd` | `miniz_oxide`'s SIMD adler-32 path (output-preserving) | `simd-adler32` |
| `deflate-ablation` | the `examples/deflate_ablation.rs` harness (measures only; ADR-0045) | `zlib-rs`, `zune-inflate` |

The field CLI flags add `--workers N`, `--packed` (with `--sync=batch|each`),
and `--promote[=BYTES]`, plus the `observe-batch` command — see the [CLI](cli.md).
`--packed` and `--workers` are the two Phase-15 mechanisms that touch a persisted
artifact or a runtime path (`--sync` selects the packed writer's durability policy,
ADR-0053); `--promote` is refuted on the tested corpus and default-off (ADR-0046).

## Not supported

Beyond the six office formats (PDF, DOCX, EPUB, ODT, XLSX, PPTX, ODS, ODP) the
Phase-21 Wave 2 **listed** formats are implemented (JSON, YAML, CSV/TSV,
Markdown, XML, HTML, TOML, JSONL, EML/MIME, Parquet, Arrow IPC); the remaining
Wave-2 families (see [Roadmap](../project/roadmap.md)) are `PROPOSED`. Any source
that is not a recognized format still round-trips exactly through the opaque
`RAW` lane.
