# ADR-0009: PDF physical bytes are the authority; oracles are not

- **Status:** Accepted — physical scanner implemented (Phase 3); structural passes Phases 4–8
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

## Implementation (Phase 3)

The physical authority is now implemented in `src/adapter/pdf/`:

- **Owned lexer** (`lexer.rs`, `span.rs`) — a hostile-safe lexical partition of
  the input into a contiguous, non-overlapping span cover of `[0, len)`:
  whitespace, comments, literal strings (escapes and nesting), hex strings,
  names, delimiters, and regular tokens. Structural keywords are recognised only
  on `Regular` lexemes, so an `endobj`/`stream`/`xref` spelling inside a string
  or comment can never be mistaken for structure.
- **Physical scanner** (`physical.rs`) — a conservative structural pass over the
  lexical cover: `%PDF-` header, `N G obj`/`endobj`, `stream`/`endstream`
  (payload treated as opaque raw bytes), classic `xref`/`trailer`, `startxref`,
  `%%EOF`, comments, and whitespace. Direct and indirect `/Length` are resolved
  with CRLF/LF handling; an object's `/Type` yields a conservative structural
  role (`XRefStream`, `ObjectStream`, otherwise `Generic`). Anything ambiguous
  stays `Unclassified`/`ObjBody`; bytes are never invented or dropped.
- **Revision map** (`physical.rs`) — `%%EOF`-delimited, append-only revisions in
  file order, each with its recorded `startxref` and resolved `/Prev`; `/Size`
  is treated as never decreasing.
- **Detection** (`adapter.rs`) — validated from the bytes: a `%PDF-` header
  **and** at least one complete indirect object **and** at least one `%%EOF`.
  Extensions are never authority; failure declines to the opaque exact lane.
- **Candidate** (`adapter.rs`) — `PDF_PHYSICAL` persists the exact physical
  partition as one literal `INLINE` op per span, in ascending offset order. Span
  kinds are **analysis metadata** recomputable by `scan`, not stored authority;
  the literal bytes are the authority.

The qpdf differential court (`tools/pdf-oracle.sh`, qpdf 11.3) confirms 100%
object-number agreement (classic 4/4, two-page 6/6, incremental 5/5) and
`qpdf --check` validity. Divergence is expected for objects compressed inside
object streams, which carry no physical `N G obj` marker: a physical scanner
enumerates fewer objects than qpdf's semantic view. qpdf remains an oracle,
never the byte authority.

As predicted by acceptance gate 6, the literal PDF candidate **loses to RAW** in
Phase 3 (RAW won all 9 corpus items; `PDF_PHYSICAL` won 0): it carries no
structural compression, so per-span overhead loses once complete cost is
charged. Structural compression (xref/`/Length` proceduralization, stream
replay, typed residuals) is Phase 5+. Campaign `2026-10-05-phase3-486aa17`.
