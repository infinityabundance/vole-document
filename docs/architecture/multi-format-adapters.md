# Multi-format adapters

Phase 12 makes the field format-universal across PDF, DOCX and EPUB: three
native inverse compilers converge on one `DocumentField` with common
observations and retained native structure. Phase 13.3 adds a fourth, ODT
(OpenDocument), Phase 21.1 a fifth, XLSX (SpreadsheetML), and Phase 21.2 a sixth,
PPTX (PresentationML). “Universal” means the observation
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
