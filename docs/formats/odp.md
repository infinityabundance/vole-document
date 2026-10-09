# ODP

ODP (OpenDocument Presentation) enters through the bounded OpenDocument inverse
compiler over the shared byte-authoritative ZIP layer (ADR-0030/0038), exactly
like ODT and ODS. ODF is **not** OPC, so the adapter reuses the ZIP physical layer
and the bounded-XML policy but does not route through the OPC graph. The adapter
is gated behind the **non-default** `odp = ["opc"]` feature.

## Authority boundary

The exact physical source is the ZIP member cover. The main content part is
located **semantically** from `META-INF/manifest.xml`, never from a hardcoded path
alone. Detection is byte-based: the mandatory stored `mimetype` equals
`application/vnd.oasis.opendocument.presentation`, or the manifest declares that
media type — mutually exclusive with ODT (text) and ODS (spreadsheet).
Decode-time OpenDocument *conformance* is a differential judgement reported as
typed issues and never gates exactness.

## Physical representation

`mimetype` (first entry, stored), `META-INF/manifest.xml`, `content.xml`,
`styles.xml`, `meta.xml`, `Pictures/*`. Members are identified by `(archive
ordinal, local-header offset)`; the exact leaf is the raw compressed span (no
unzip/rezip). A missing or malformed manifest is a typed decline.

## Native inverse representation

The `office:body`/`office:presentation` content model. **Slides are `draw:page`
in document order**, never a page-name or file order. On each slide: `draw:frame`
(with `draw:text-box` text paragraphs/runs, and `draw:image` → a `Pictures/`
media part via `xlink:href`), `draw:custom-shape`, groups (`draw:g`, bounded
recursion), `draw:table` (rows/cells), and `presentation:placeholder`. Speaker
notes (`presentation:notes`), master pages (`style:master-page`, referenced by
`presentation:master-page-name`), and styles are resolved from the content/styles
parts. An embedded table's cell text is part of the slide/deck text projection and
is also exposed structurally.

## Supported observations

Common selectors: `metadata`, `text`, `table`, `cell`, `find`. Native:
`--odp-slide N` (metadata/text/structure), `--odp-shape` (slide + flat shape
index), `--odp-notes N`, `--odp-masters`, `--odp-media` (decoded resource bytes),
`--odp-tables`, `--odp-find`, plus raw or decoded members.

## Unsupported observations

`Page(n)` is a typed decline — a presentation's pages are its slides, addressed by
`--odp-slide`, and are never synthesized. Slide rendering, animation/transitions,
OLE embeddings, and chart data are not interpreted.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any `.odp`,
including packages the adapter declines to interpret natively, and after the
source **and** descriptor are deleted in a fresh process (the 21.4.1 court and the
21.4.2 economic court, exactness **6/6** and **8/8**). Exactness is inherited from
the Phase-12.2 ZIP member raw spans; the model is derived (`Q_gen`) and is **never
on the exactness path**.

## Security limits

XML is derived-only (bounded depth/events/nodes/attributes/text); group recursion
is depth-bounded (`max_odp_group_depth`); slide/shape/text-run/media/table/notes/
master counts are bounded by `max_odp_*` limits. No `PathBuf` is ever built from a
member name; `xlink:href`/external targets are inert strings and never fetched. See
[Security](../SECURITY.md).

## Known limitations

This is a **bounded** OpenDocument presentation subset, not a rendering engine.
ODF does not standardize slide hidden state; the adapter reads a declared
`presentation:visibility` convention. It does not render slides or resolve
master-page theme inheritance into effective per-shape formatting. The economic
court is measured on a **self-authored deterministic corpus**; only the
materialized deck is a byte-authority claim, every other observation is a derived
projection.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0030](../adr/0030-zip-physical-layer.md),
[0031](../adr/0031-common-observation-model.md),
[0038](../adr/0038-odt-adapter-scope.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.4.1 ODP court: `tests/odp_adapter.rs`; campaign
  `evidence/campaigns/2026-10-09-phase21-4-1-odp-957a800/`.
- Phase 21.4.2 economic court: `tools/phase21-4-odp-court.sh`; campaign
  `evidence/campaigns/2026-10-09-phase21-4-odp-econ-957a800/`.
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
