# Format support

Capability matrix for the three implemented formats. `✓` = supported and
answerable; `—` = a typed decline (`unsupported-feature`, exit 6), never a
silent empty answer. The format is detected from bytes, never a file name.

## Exactness and detection

| Property | PDF | DOCX | EPUB |
|---|---|---|---|
| Byte-based detection | `%PDF-` + indirect object + `%%EOF` | OPC ZIP package | OCF ZIP container |
| Physical authority | owned lexer + span scanner | shared byte-authoritative ZIP | shared byte-authoritative ZIP |
| `materialize == original` (length + SHA-256 + `cmp`) | ✓ | ✓ | ✓ |
| Exact after source **and** descriptor deletion, fresh process | ✓ | ✓ | ✓ |

## Common observation vocabulary

The shared vocabulary is added for all three formats (ADR-0031); a common
observation is a *projection*, never the document model.

| Common observation | PDF | DOCX | EPUB |
|---|---|---|---|
| `metadata` | ✓ (descriptor identity) | ✓ (story structure) | ✓ (package summary) |
| `text` | ✓ | ✓ | ✓ |
| `find` (lexical search) | ✓ | ✓ | ✓ |
| `heading` | — | ✓ | ✓ |
| `block` (paragraph-ish) | — | ✓ | ✓ |
| `table` / `cell` | — | ✓ | ✓ |
| `resource` | — | ✓ | ✓ |
| `link` | — | ✓ | ✓ |

`metadata` is a shared *selector name* with per-format semantics, not a shared
meaning: the PDF lane returns its own `source_len`/`source_sha256`, DOCX a story
structure, EPUB a package summary.

## Native observations

| Native surface | PDF | DOCX | EPUB |
|---|---|---|---|
| Object / stream / revision | ✓ | — | — |
| Story | — | ✓ | — |
| Package part / relationship | — | ✓ | ✓ |
| Manifest / spine item / nav | — | — | ✓ |
| Members (raw / decoded) | — | ✓ | ✓ |
| Page | ✓ (intrinsic) | — (no pagination) | only when the publication defines it |
| Byte range | ✓ | ✓ | ✓ |

Reflowable EPUB has no intrinsic pages, and DOCX has no intrinsic pagination;
`Page(n)` is never synthesized from viewport size or heuristics (ADR-0033).

## Exactness after removal — receipts

| Court | Result | Receipt |
|---|---|---|
| Source + descriptor removal (all three formats) | 38/38 | `evidence/campaigns/2026-10-06-phase12-removal-dc4d5a3/` |
| DOCX/EPUB logical triplet equivalence | 96/96 | `evidence/campaigns/2026-10-06-phase12-triplet-dc4d5a3/` |
| PDF no-regression (A2 vs A11) | 32/32 exact, 0 regressions | `evidence/campaigns/2026-10-06-phase12-pdf-noregression-0d23a02/` |
| Hostile ZIP/OPC/OCF/XML fixtures | 315/315 assertions | `evidence/campaigns/2026-10-06-phase12-security-33f6d04/` |

See [Status ledger](../project/status.md) for the full mechanism table and
[Conformance](../reference/conformance.md) for the courts.

## Feature gates

The default build is `default = ["rans", "store", "field"]`. The ZIP/DOCX/EPUB
adapters need `package,opc,docx,epub`. `deflate-replay` is opt-in (pulls LGPL
`cabac`). A descriptor that needs a capability the build lacks sets a mandatory
feature bit and fails closed with `unsupported-feature` (exit 6).

## Not supported

ODT and any other container beyond PDF/DOCX/EPUB are `PROPOSED`, not
implemented (see [Roadmap](../project/roadmap.md)). Any source that is not a
recognized format still round-trips exactly through the opaque `RAW` lane.
