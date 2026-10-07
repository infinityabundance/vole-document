# Multi-format adapters

Phase 12 makes the field format-universal across PDF, DOCX and EPUB: three
native inverse compilers converge on one `DocumentField` with common
observations and retained native structure. “Universal” means the observation
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
[0035](../adr/0035-phase12-lifetime-benchmark.md).
