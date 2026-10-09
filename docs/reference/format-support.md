# Format support

Capability matrix for the five implemented formats. `✓` = supported and
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

## Feature gates

The default build is `default = ["rans", "store", "field"]`. The ZIP/DOCX/EPUB/ODT
adapters need `package,opc,docx,epub,odt`; the XLSX adapter needs `package,opc,xlsx`
and the PPTX adapter `package,opc,pptx` (the non-default `xlsx`/`pptx` features).
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

Containers beyond PDF/DOCX/EPUB/ODT/XLSX/PPTX (e.g. ODS, ODP) are `PROPOSED`,
not implemented (see [Roadmap](../project/roadmap.md)). Any source that is not a
recognized format still round-trips exactly through the opaque `RAW` lane.
