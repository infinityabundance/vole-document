# Phase 21 — Format programme

> **WAVE 1 COMPLETE; WAVE-2 LISTED FORMATS COMPLETE.** Subphases **21.1 (XLSX)**, **21.2 (PPTX)**, **21.3 (ODS)** and
> **21.4 (ODP)** are complete and shipped — the **six office
> formats** from two shared package substrates (OPC: DOCX/XLSX/PPTX; ODF:
> ODT/ODS/ODP):
> **21.1.1** (OPC/SpreadsheetML surface + exact closure),
> **21.1.2** (the spreadsheet semantic model: styles, merges, comments,
> hyperlinks, defined names, tables, drawings/charts, external relationships, and
> a bounded deterministic *displayed-value* projection kept distinct from the
> stored formula and cached result), **21.1.2b** (a hardening pass an adversarial
> audit required), **21.1.3** (the XLSX economic court against SQLite **and**
> DuckDB/Parquet), **21.2.1** (the PPTX/PresentationML adapter: slide order from
> `p:sldIdLst` — plus a correctness fix to source-scoped node identity an audit
> forced), **21.2.3** (the PPTX economic court), **21.3.1** (the ODS adapter),
> **21.3.2** (the ODS economic court vs SQLite **and** DuckDB/Parquet),
> **21.4.1** (the ODP adapter: slide order from `draw:page` document order), and
> **21.4.2** (the ODP economic court). The whole programme is judged by the
> capability/cost frontier on the same contract — **not by format count**.
>
> **Wave 2 listed formats are complete.** Eleven structured/tabular/docs/web/messaging/
> analytical formats beyond the office set were added on the `phase21-wave2`
> branch, each with an adapter + sealed economic court: **JSON** (21.5),
> **YAML** (21.6), **CSV/TSV** (21.7), **Markdown** (21.8), **XML** (21.9),
> **HTML** (21.10), **TOML** (21.11), **JSONL/NDJSON** (21.12), **EML/MIME**
> (21.13), **Parquet** (21.14) and **Arrow IPC** (21.16). **The next eight
> Wave-2 subphases (21.17–21.24) have since shipped** on the same branch, each
> with an adapter court + a sealed economic court: **JSON5/JSONC** (21.17),
> **CBOR** (21.18), **MessagePack** (21.19), **config / INI / `.env` /
> properties** (21.20), **RSS/Atom** (21.21), **GeoJSON** (21.22), **KML/GPX**
> (21.23) and the **Jupyter notebook / `.ipynb`** (21.24). Review-driven
> corrections also landed on this branch: the **JSON comparator** was repaired
> (modern SQLite+JSONB, fixed duplicate-key accounting, an independent
> span-preserving baseline), the **YAML comparator** got the same
> span-preserving treatment, the **CSV metadata** path became O(n) instead of
> O(n²), a permanent **cross-field identity court** was added (ADR-0060), the
> **stratified real-world court** sealed (52 real + 8 hostile, 60/60 byte-exact),
> and a **detection repair** fixed five real-world detection root causes. The
> 21.20–21.24 economic courts also found and fixed a **GeoJSON JSON-validity
> defect** (7 observation projections emitted invalid JSON; fixed in
> `src/field/observe.rs`, commit `ba4df0d4`). The remaining Wave-2 families
> (21.25–21.35) are not started.
>
> Branch: `phase21-wave2` @ `v0.1.0-alpha.33`+ (Wave 2). Wave 1 shipped as
> `v0.1.0-alpha.30`, JSON as `v0.1.0-alpha.31`, YAML as `v0.1.0-alpha.32` on
> `main`. The Phase-22 economic programme (22.1–22.7) is complete and released in
> `v0.1.0-alpha.29`.

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

### Wave 2 — the remaining order (frozen)

Beyond the eleven shipped Wave-2 formats, the remaining families are decomposed
into one subphase each. **21.17–21.24 have since shipped** (marked below); the
rest (21.25–21.35) are not started. The order is again chosen for **maximum reuse** — the
structured-tree, tabular, XML, JSON and MIME layers are already paid for, so a
format enters by reusing one of them and paying mostly for its own span policy.
Each subphase is the established unit: **adapter → tests → adapter court →
economic court → docs**, with a sealed receipt per court, and it may be closed as
an honest negative if the court declines it.

```text
21.17  JSON5 / JSONC        (structured extra; reuses the JSON parser + span policy)  — SHIPPED
21.18  CBOR                 (binary structured; new byte/integer/float policy)         — SHIPPED
21.19  MessagePack          (binary structured; new byte/integer/float policy)         — SHIPPED
21.20  INI / .env / properties   (config; trivial line grammar)                          — SHIPPED
21.21  RSS / Atom           (feeds; reuses the XML parser + span policy)               — SHIPPED
21.22  GeoJSON              (GIS; reuses the JSON parser + geo semantics)              — SHIPPED
21.23  KML / GPX            (GIS; reuses the XML parser + geo semantics)               — SHIPPED
21.24  Jupyter notebook     (.ipynb; reuses the JSON parser + cell/output model)       — SHIPPED
21.25  PSV / fixed-width    (tabular extra; reuses the tabular layer)
21.26  reST / AsciiDoc / MDX (docs; reuses the prose/line layer)
21.27  MHTML                (messaging + web; reuses the MIME + HTML layers)
21.28  syslog / log streams (line/event stream; reuses the JSONL line policy)
21.29  package metadata      (package.json / pyproject.toml / Cargo.toml / lockfiles)
21.30  JSON Schema / OpenAPI (API/spec; reuses the JSON parser + a schema model)
21.31  BibTeX / CSL-JSON / RIS (bibliography)
21.32  RTF                  (legacy prose; new control-word grammar)
21.33  ZIP / TAR containers (containers; reuses the package physical layer)
21.34  HDF5 / NetCDF         (scientific; new binary container policy)
21.35  CFBF / OLE            (legacy DOC/XLS/PPT/MSG via one shared physical layer)
```

The JSON-based members (21.17, 21.22, 21.24, 21.29, 21.30) need a **semantic
sub-detection** step: their physical bytes are JSON, so the generic JSON
classification must be refined by a bounded semantic test (a JSON5/JSONC dialect
marker, a GeoJSON `type`, a notebook `cells`/`nbformat`, a known package key set,
a schema `$schema`/`openapi` key) — a plain JSON document stays `Json`. The
binary/scientific/legacy members (21.18, 21.19, 21.32, 21.33, 21.34, 21.35) each
pay honestly for a new physical parser and a typed decline on anything they do not
fully support, so an unsupported input stays `Opaque`.

**Analytical comparators are required, not optional.** For the tabular/analytical
formats the court must include a **DuckDB/Parquet** baseline as well as SQLite, and
for any format whose conventional peer is already efficient on the same question
the court must say so and record the negative rather than claim a win.

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
