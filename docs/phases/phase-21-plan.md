# Phase 21 — Format programme

> **IN PROGRESS.** Subphase **21.1 (XLSX) is complete and shipped** on the
> `staging` line — **21.1.1** (OPC/SpreadsheetML surface + exact closure),
> **21.1.2** (the spreadsheet semantic model: styles, merges, comments,
> hyperlinks, defined names, tables, drawings/charts, external relationships, and
> a bounded deterministic *displayed-value* projection kept distinct from the
> stored formula and cached result), **21.1.2b** (a hardening pass an adversarial
> audit required: overflow/coordinate guards, precise DOCX-vs-XLSX detection, and
> precise typed-decline wording), and **21.1.3** (the XLSX economic court against
> a source-retaining SQLite baseline **and** a DuckDB/Parquet comparator).
> Subphases **21.2 (PPTX), 21.3 (ODS), 21.4 (ODP)** remain planned and not
> started. The Wave-2 families below are a target, never a result. The whole
> programme is judged by the capability/cost frontier on the same contract —
> **not by format count**.

Branch: `staging` @ `v0.1.0-alpha.30` (Phase 21.1). The Phase-22 economic
programme (22.1–22.7) is complete and released in `v0.1.0-alpha.29`.

## Why

Phase 12–13 delivered four formats (PDF, DOCX, EPUB, ODT) from two shared
substrates — a byte-authoritative ZIP/OPC layer and a bounded-XML policy. Phase
21 is the deliberate programme that completes the office families those
substrates already imply, and then extends the **same logical observation API**
across radically different physical formats by adding two more shared layers (a
structured tree and a tabular layer). The point is not format count: it is that
the *same* typed observation vocabulary — with per-answer provenance recording
where the answer came from — works across formats whose physical structure has
nothing in common.

## The order (frozen)

Two office package substrates yield **six office formats** first; the wider
families follow.

### Wave 1 — office, from two shared package substrates

| Subphase | Format | Package substrate | Semantic model |
|---|---|---|---|
| 21.1 | **XLSX** | OPC (shared with DOCX/PPTX) | SpreadsheetML |
| 21.2 | **PPTX** | OPC | PresentationML |
| 21.3 | **ODS** | ODF (shared with ODT/ODP) | OpenDocument Spreadsheet |
| 21.4 | **ODP** | ODF | OpenDocument Presentation |

DOCX (OPC/WordprocessingML) and ODT (ODF/OpenDocument Text) already exist, so
Wave 1 completes the six office formats — **DOCX/XLSX/PPTX** from one OPC
substrate and **ODT/ODS/ODP** from one ODF substrate.

### Wave 2 — the wider families (order within a family is indicative)

| Family | Formats |
|---|---|
| Structured | JSON, JSONL/NDJSON, YAML, TOML, XML |
| Tabular | CSV, TSV, PSV, fixed-width |
| Docs / web | Markdown, MDX, reStructuredText, AsciiDoc, HTML, XHTML, MHTML |
| Messaging | EML, MIME; later MSG |
| Config | INI, `.env`, properties |
| Data interchange | JSON5, JSONC, CBOR, MessagePack |
| Scientific / analytical | Parquet, Arrow IPC, HDF5, NetCDF |
| Logs / events | (line/event streams) |
| Package metadata | `Cargo.toml`, `package.json`, `pyproject.toml`, lockfiles |
| Feeds | RSS, Atom |
| GIS | GeoJSON, KML, GPX |
| Notebook | `.ipynb` |
| API / spec | OpenAPI, JSON Schema, AsyncAPI |
| Bibliography | BibTeX, CSL-JSON, RIS |
| Legacy | RTF; later DOC/XLS/PPT via a shared **CFBF/OLE** layer |
| Containers | ZIP, TAR |

### Wave 2 — the near order

Within Wave 2 the immediate order is chosen for **maximum reuse of the two new
shared layers** (the structured tree and the tabular layer), so each format pays
mostly for its own physical parser and span policy:

```text
JSON → YAML → CSV/TSV → Markdown → XML → HTML → TOML → JSONL
```

Reasons the order matters, briefly: **JSON** (path / JSON Pointer / exact token
spans) and **YAML** (anchors, aliases, tags, merge keys — which a JSON-normalizing
pipeline destroys) exercise the *structured* layer hardest; **CSV/TSV** give a
cheap **high-volume** court (100 MB / 1 GB / 10 GB) for ingest throughput
`GB/s`, index size, and selective reads; **Markdown/HTML/XML** are the wide-web
and documentation surface; **TOML/JSONL** are cheap completions.

**Analytical comparators are required, not optional.** For the **tabular/
analytical** formats (CSV/TSV, XLSX, ODS) the court **must** include a
**DuckDB/Parquet** baseline as well as SQLite, because those engines already
embody columnar projection, predicate pushdown, compressed pages, and metadata
indexes — winning only against SQLite there could just mean the wrong competitor
was chosen (the Phase-22 "maximize the competitor first" rule, applied per
format). Parquet and Arrow IPC themselves are the **most adversarial** later
targets for exactly this reason.

## The reuse architecture

### Office: extend the package layer, not the semantic model

The OPC layer already owns the *physical* services DOCX uses (byte-authoritative
ZIP member scan, `[Content_Types].xml`, `_rels` relationships, semantic part
discovery, URI resolution, bounded XML, resource blobs, exact part spans,
provenance). Phase 21 **extends that layer into shared physical services for all
OPC formats**, changing only the semantic model per format — WordprocessingML
(DOCX), **SpreadsheetML (XLSX)**, **PresentationML (PPTX)**. Likewise an ODF
package layer serves ODT, **ODS**, and **ODP** (the mandatory stored `mimetype`
and `META-INF/manifest.xml`, semantic main-part discovery, versioned extraction
profiles) with only the content model differing. The exact leaf remains the raw
member span; no unzip/rezip and no conformance-by-re-save.

### Wave 2: a generalized structured-tree layer

JSON, YAML, TOML, XML and HTML share one logical model: a **structured tree**
whose observations are `path`, `node`, `scalar`, `mapping`, `sequence`,
`attribute`, `source span`, `parent`, `children`, `find`, and `exact bytes`.
Each format supplies its own physical parser and its own span policy (YAML
anchors/aliases, TOML table arrays, XML namespaces and DTD policy, HTML
error-recovery rules), but the observation API is the same and provenance
records which physical construct produced the answer.

### Wave 2: a generalized tabular layer

CSV, TSV, PSV, XLSX and ODS share one logical model: `sheet`/`table`, `row`,
`column`, `cell`, `range`, `header`, `formula`/`value`, `type`, and `source
coordinate`. A CSV cell and an XLSX cell answer the *same* logical observation
against radically different physical media; provenance says which.

These layers do not replace the office semantic models — XLSX still exposes
SpreadsheetML-native structure — they add a **shared logical projection on top**,
exactly as the common vocabulary does today (ADR-0031).

## 21.1 XLSX — in detail (the first subphase)

XLSX enters through the shared OPC layer; its semantic model is SpreadsheetML.
The physical authority, ZIP-cover validation, relationship graph, and exact part
spans are the existing OPC services.

**Proposed observation surface.** Workbook (sheets, order, visibility,
very-hidden), rows, cells, **stored formulas**, **cached results**, **displayed
value**, styles (number format, font, fill, alignment), comments, hyperlinks,
merged ranges, shared strings, named ranges, tables, charts, drawings, and
external relationships (external workbook / external data references).

**The crucial distinctions (never conflated).** A cell's *stored formula*, its
*cached result* (the value Excel last computed), its *displayed value* (which
depends on number format and recalculation), its *style*, and its underlying
*XML span* are **five different observations with different provenance**. None
may be silently substituted for another, and none may be presented as "the
value". Exact source closure is preserved: the workbook materializes
byte-for-byte (length + SHA-256 + `cmp`) after source **and** descriptor
deletion in a fresh process, and every derived answer keeps a span back to the
original bytes.

**The XLSX economic court.** Against a source-retaining SQLite baseline, on
**contract-equivalent terms** (same escalating capability contract as Phase 16+
C0–C5, extended for spreadsheet coordinates), ask ten questions and require the
same answer on both lanes where both can answer:

| Q | Question |
|---|---|
| Q1 | cell value |
| Q2 | cell formula |
| Q3 | formula dependents |
| Q4 | containing table |
| Q5 | cell style |
| Q6 | worksheet relationship |
| Q7 | chart referencing the table |
| Q8 | exact XML span |
| Q9 | embedded resource |
| Q10 | original bytes |

Each lane states exactly what it can derive and what it declines; declines are
typed, never silent. As everywhere in this repo, the court is deliberately mixed
and the negatives are recorded. The proposed subphases are **21.1.1** OPC/XML
surface + exact closure, then **21.1.2** the spreadsheet semantic model, then
**21.1.3** the economic court — each with a sealed receipt under
`evidence/campaigns/`.

## Acceptance (per subphase, unchanged)

Exactness is the invariant (`materialize == source`, length + SHA-256 + `cmp`);
Docker only, digest-pinned and memory-capped; one crate; one branch per phase
with commit-and-push per subphase; a sealed receipt per court; negatives and
partials recorded; no cap raised to make a run pass; no claim without evidence.
New formats are additions — none may weaken or reinterpret an existing shipped
observation.
