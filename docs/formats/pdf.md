# PDF

VOLE's native format and the one the Phase-11 field was built on. PDF physical
bytes are the authority; qpdf/Poppler/MuPDF are differential oracles only
(ADR-0009).

## Authority boundary

The owned scanner covers the whole file with a contiguous span partition and is
the byte authority. A file is treated as a PDF only when the bytes show a `%PDF-`
header **and** a complete indirect object **and** a `%%EOF`; the extension is
never authority. Anything else falls back to the opaque exact lane. An object
inside an object stream has no physical `N G obj` marker, so a physical scan
legitimately enumerates fewer objects than a semantic tool.

## Physical representation

An owned lexer plus a conservative structural scanner (`%PDF-`, `obj`/`endobj`,
`stream`/`endstream`, `xref`, `trailer`, `startxref`, `%%EOF`), `/Length`
resolution, and an append-only incremental revision map (`/Prev` chain, `/Size`
never decreasing). The `stream` + EOL payload is an opaque span, so
`/FlateDecode` stream bytes are a byte-authoritative span the scanner owns.

## Native inverse representation

Objects, streams, revisions, the page tree, and `/ObjStm`-hosted page objects are
recovered as native procedural state; page content can be deepened to operators
and text runs (Stage C). Content is recovered as observations, not as a size
mechanism.

## Supported observations

`metadata`, `text`, `find` (the common surface PDF answers); native `object`,
`stream`, `revision`, intrinsic `page`, and physical `byte-range`.

## Unsupported observations

The widened common selectors `heading`, `block`, `table`, `cell`, `resource`,
`link` are typed declines for PDF (exit 6) — PDF has no such native structure to
project. No title observation is exposed.

## Exact reconstruction

`materialize(descriptor) == original_bytes` and
`materialize(field_root) == original_bytes` (length + SHA-256 + `cmp`), including
after the source and descriptor are deleted, in a fresh process. The PDF
no-regression gate (`N6`) confirms the widened multi-format surface did not
change the PDF lane (A2 vs A11: 32/32 exact, 0 regressions).

## Security limits

No PDF JavaScript, actions, or embedded executables are executed; encrypted
content is opaque by default and passwords never enter normative decode; signed
byte ranges round-trip identically. DEFLATE replay runs in a process-isolated,
memory- and time-capped worker (ADR-0016). See [Security](../SECURITY.md).

## Known limitations

Whole-file size loses to generic lossless compressors (0/27). PDF↔DOCX resource
sharing does not exist: the PDF adapter extracts no embedded resource blob.
Structural PDF proceduralization is measured and closes the ledger's PDF
`PROPOSED` items: `/Length`/revision ([ADR-0036](../adr/0036-pdf-length-revision-size.md))
is a recorded negative, and a bounded COS grammar/template
(`pdf-cos-template`, [ADR-0037](../adr/0037-pdf-grammar-templates.md)) becomes the
best VOLE lane on 4/28 files but never beats a generic compressor.

## Relevant ADRs

[0006](../adr/0006-rans-substrate.md), [0007](../adr/0007-deflate-replay.md),
[0009](../adr/0009-pdf-byte-authority.md),
[0010](../adr/0010-typed-channels-rejected.md),
[0011](../adr/0011-layout-prediction-framing.md),
[0012](../adr/0012-packed-framing-threshold.md),
[0013](../adr/0013-layout-rans-not-profitable.md),
[0015](../adr/0015-deflate-replay-result.md),
[0016](../adr/0016-replay-resource-bound.md),
[0036](../adr/0036-pdf-length-revision-size.md),
[0037](../adr/0037-pdf-grammar-templates.md).

## Evidence

- Physical authority: `evidence/campaigns/2026-10-05-phase3-486aa17/`.
- Structural courts: `evidence/campaigns/2026-10-05-phase4-3840bc4/`,
  `…-phase5-7193001/`, `…-phase5-4521778/`, `…-phase5-8-cf8048d/`,
  `…-phase6-0d0bb79/`, `…-phase7-court-99dc72e/`.
- Field results: [phase-11-results.md](../phases/phase-11-results.md).
- No-regression: `evidence/campaigns/2026-10-06-phase12-pdf-noregression-0d23a02/`.
