# ADR-0009: PDF physical bytes are the authority; oracles are not

- **Status:** Accepted (Phase 0); implementation is Phases 3–8
- **Date:** 2026-10-05

## Context

PDF has two views: a semantic object graph and a physical byte layout with
offsets, whitespace, cross-reference structures, and incremental revisions.
Writer round-trips produce a *new* valid PDF, not the original.

## Decision

- Maintain **two** views: an owned byte-authoritative `PdfPhysical` (exact spans
  and revision history) and a derived `PdfSemantic` (objects, page tree, filters).
  The semantic view proposes structure; it never overwrites physical truth.
- **Never** materialize by re-saving through a PDF writer (`lopdf`, qpdf, etc.).
  Instead predict original physical bytes and store the exact residual.
- Treat PDF incremental updates as first-class: append-only revision chains,
  `/Prev` links, historical byte regions, and stale objects are part of the
  observation. `/Size` never decreases.
- `qpdf`, Poppler, MuPDF, and Ghostscript are **oracles** in the Docker `tools`
  court, never normative decoders and never the representation authority. qpdf's
  JSON omits byte offsets and transparently decrypts, so it cannot be the byte
  source of truth.
- Signed regions must round-trip byte-identically; encrypted content is opaque by
  default and passwords are never part of normative materialization.

## Consequences

- The strongest PDF test is `decoded PDF == input PDF` byte-for-byte, with
  independent tools then proving both sides behave identically as PDFs.
- Procedural targets (xref entries, `startxref`, `/Length`, revision chains, and
  DEFLATE decision replay) are byte-exact predictions with residual fallback.
