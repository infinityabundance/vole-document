# PPTX

PPTX (Office Open XML PresentationML) enters through the shared byte-authoritative
**OPC** layer (ADR-0030/0031) with a **PresentationML** native model. The adapter
is gated behind the **non-default** `pptx = ["opc"]` feature; a build without it
cannot detect PPTX and reports it `Opaque`, so the reported capability set always
matches what the build serves.

## Authority boundary

The exact physical source is the ZIP member cover. The presentation part
(`ppt/presentation.xml`) is located **semantically** from `[Content_Types].xml`
and the OPC relationship graph, never from a hardcoded path. Decode-time
PresentationML *conformance* is a differential judgement reported as typed issues
and never gates exactness: an invalid or hostile deck is still an exact archival
object. Detection is byte-based and mutually exclusive with DOCX/XLSX — a positive
PresentationML main-part content type (`...presentationml.presentation.main+xml`,
also template/slideshow variants) — so a Word or Excel document that *embeds* a
deck is not misclassified.

## Physical representation

`[Content_Types].xml`, `_rels/.rels`, `ppt/presentation.xml` +
`ppt/_rels/presentation.xml.rels`, `ppt/slides/slideN.xml` + their `_rels`,
`ppt/slideLayouts/*`, `ppt/slideMasters/*`, `ppt/theme/*`,
`ppt/notesSlides/*`, `ppt/media/*`, `ppt/charts/*`, `ppt/embeddings/*`. Members
are identified by `(archive ordinal, local-header offset)`; the exact leaf is the
raw compressed span (no unzip/rezip). A malformed presentation/slide or a missing
part referenced by an element is a typed decline.

## Native inverse representation

The presentation model: slide size (`p:sldSz`), and the **slide order from
`p:sldIdLst`** resolved through the presentation relationships — never from
`slideN.xml` file-name order. Each slide's shape tree (`p:spTree`): text shapes
(`p:txBody` → `a:p`/`a:r`/`a:t`), pictures (`p:pic` → `a:blip/@r:embed` → media
part), graphic frames (`p:graphicFrame` → an embedded `a:tbl` table or a chart
relationship), group shapes (`p:grpSp`, bounded recursion), and connectors
(`p:cxnSp`). Speaker notes (`ppt/notesSlides/*`), layouts, masters, and themes are
resolved by relationship/content-type. An embedded table's cells are part of the
slide/deck `text` projection (matching DOCX) and are also exposed structurally.

## Supported observations

Common selectors: `metadata` (slide count, size, title), `text` (all slide text in
slide order), `table`/`cell` (a slide's embedded tables), `find`. Native:
`--slide N` (metadata/text/structure), `--pptx-shape` (slide + flat shape index),
`--pptx-notes N`, `--pptx-layouts`, `--pptx-masters`, `--pptx-theme`,
`--pptx-media` (decoded resource bytes by ordinal), `--pptx-tables`,
`--pptx-find`, plus raw or decoded members.

## Unsupported observations

`Page(n)` is a typed decline — a presentation's pages are its slides, addressed by
`--slide`, and are never synthesized. Chart **data** is not parsed or evaluated
(the slide exposes the chart *reference*, not its series); layout/master/theme
internal shape trees are listed, not interpreted; a decoded slide/shape XML digest
is not exposed (only media supports `--kind decoded`).

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any `.pptx`,
including decks the adapter declines to interpret natively, and after the source
**and** descriptor are deleted in a fresh process (the 21.2.1 court and the 21.2.3
economic court, exactness **8/8**). Exactness is inherited from the Phase-12.2 ZIP
member raw spans; the PresentationML model is derived (`Q_gen`) and is **never on
the exactness path**.

## Security limits

XML is derived-only (bounded depth/events/nodes/attributes/text); group-shape
recursion is depth-bounded (`max_pptx_group_depth`); slide/shape/text-run/media/
table/notes/layout/master counts are bounded by `max_pptx_*` limits. No `PathBuf`
is ever built from a member name; external targets are inert strings and never
fetched; no script is executed. See [Security](../SECURITY.md).

## Known limitations

This is a **bounded** PresentationML subset, not a rendering engine. It does not
evaluate charts, render slides, or resolve layout/master theme inheritance into
per-shape effective formatting. The economic court is measured on a **self-authored
deterministic corpus**, not a real-world population; only the materialized deck is
a byte-authority claim, every other observation is a derived projection. All
measurement corpora are locally generated.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0030](../adr/0030-zip-physical-layer.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0059](../adr/0059-xlsx-adapter-and-analytical-comparator.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.2.1 PPTX court: `tests/pptx_adapter.rs`; campaign
  `evidence/campaigns/2026-10-09-phase21-2-pptx-8aab956/`.
- Phase 21.2.3 economic court: `tools/phase21-3-pptx-court.sh`; campaign
  `evidence/campaigns/2026-10-09-phase21-3-pptx-054ce93/`.
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
