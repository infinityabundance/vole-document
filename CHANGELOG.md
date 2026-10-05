# Changelog

All notable changes are recorded here. The format is pre-1.0 and provisional.

## [0.1.0-alpha.3] — unreleased

### Added

- Phase 3 — byte-authoritative PDF physical authority:
  - Owned PDF lexical scanner (whitespace, comments, literal/hex strings with
    escapes and nesting, names, delimiters, regular tokens) producing a
    contiguous, non-overlapping span cover of `[0, len)`.
  - Conservative physical structure scanner: `%PDF-` header, `obj`/`endobj`,
    `stream`/`endstream`, `xref`, `trailer`, `startxref`, `%%EOF`, and comments;
    stream payloads are treated as opaque bytes when `/Length` is unusable.
  - Direct/indirect `/Length` resolution with CRLF/LF handling, and an
    append-only revision map (`/Prev` chain; `/Size` never decreases).
  - xref-stream and object-stream structural role detection.
  - Validated PDF detection (a `%PDF-` header **and** an indirect object **and**
    `%%EOF`); file extensions are never authority.
  - `PdfPhysical` view plus a `PDF_PHYSICAL` candidate that persists the physical
    partition as one literal `INLINE` op per span, with conservative opaque
    fallback. Span kinds are deterministic analysis metadata recomputable by
    `scan`.
- `source_format = 1` (PDF) and the Phase-3 universe string
  `vole-document;universe;phase-3;exact-bytes;dra-2;opaque+entropy+pdf`.
- Courts: `tests/pdf.rs` (total coverage, forced physical exactness, validated
  detection, revision mapping, hostile random bytes) and the qpdf differential
  oracle `tools/pdf-oracle.sh`.

### Measured

- Campaign `2026-10-05-phase3-486aa17` (verdict PASS): deterministic 9-item
  corpus (7 valid PDFs plus `malformed.pdf` and `notpdf.bin` negative controls).
  Coverage `all_covered = true` — 171 spans, 19 objects, 8 revisions; exactness
  `all_exact = true` (`materialize(descriptor) == original_bytes` for every
  item). qpdf 11.3 oracle: 100% object-number agreement (classic 4/4, two-page
  6/6, incremental 5/5), `qpdf --check` valid, `pdfinfo` pages 1/2/1. Court
  outcome: RAW won all 9 items and `PDF_PHYSICAL` won 0 — the expected Phase-3
  result, because the literal physical lane carries no structural compression
  yet (that is Phase 5). Receipt under
  `evidence/campaigns/2026-10-05-phase3-486aa17/`.

### Notes

- The wire format remains **PROVISIONAL** and is not frozen v1. The PDF physical
  authority is implemented and measured; PDF structural compression and
  xref/`/Length` proceduralization remain `PROPOSED` (Phases 5+).

## [0.1.0-alpha.2] — unreleased

### Added

- Phase 2 — native rANS floor (order-0 / typed byte channels):
  - Canonical integer-only entropy model (`MODEL`, 516-byte dense table over the
    byte alphabet) with deterministic normalization and validation.
  - Scalar order-0 byte rANS codec built on the `ryg-rans-rs` **safe manual**
    API; the panicking convenience decoder is not used.
  - Complete entropy **capsule** (model + decoder state + renormalization payload
    + counts) as a typed `ENTROPY_CHANNEL` (33-byte header + payload); never a
    scalar "seed".
  - `DECODE_CHANNEL` DRA op (DRA version 2) and materialization of channels.
  - `BYTE_RANS` and `RLE` candidates competing on complete serialized cost, with
    model bytes charged like any other bytes.
  - Feature `rans` (`default = ["rans"]`); `--no-default-features` keeps the exact
    RAW/RLE floor and returns an explicit `UnsupportedFeature` for
    channel-bearing descriptors instead of a silent reinterpretation.
- Phase-2 universe string
  `vole-document;universe;phase-2;exact-bytes;dra-2;opaque+entropy`.
- Courts: entropy acceptance gates (`tests/entropy.rs`), reference-oracle parity
  and frozen goldens (`tests/goldens.rs`), deterministic property/mutation
  fuzzing (`tests/property.rs`), and the soak script `tools/soak-fuzz.sh`.

### Measured

- Campaign `2026-10-05-phase2-f6af30b` (verdict PASS): on a 9-file mixed corpus,
  `sum_source = 590081`, `sum_core = 464474` (RAW + RLE), `sum_full = 291304`
  (RAW + RLE + BYTE_RANS), `delta = 173170`, fully attributed to the two
  `BYTE_RANS` wins. Negative controls hold (RAW on random, RLE on tiny); model
  cost is charged. This is a scoped order-0 result, not a general compression
  claim. Receipt under `evidence/campaigns/2026-10-05-phase2-f6af30b/`.

### Notes

- The wire format remains **PROVISIONAL** and is not frozen v1. PDF and other
  format-aware mechanisms remain `PROPOSED` (Phases 3+).

## [0.1.0-alpha.1] — unreleased

### Added

- Exact `.voldoc` container core (Phase 1):
  - 64-byte fixed header with CRC-32C self-check and fail-closed version/feature
    negotiation.
  - Length-delimited records with per-record CRC-32C.
  - Typed errors with stable, documented CLI exit codes.
  - Centralized resource [`Limits`].
  - SHA-256 whole-source archival identity.
- Document Reconstruction Algebra literal subset (`EMIT_OBJECT`, `INLINE`,
  `REPEAT_LAST`) with a derived **coverage certificate** validated before
  allocation.
- RAW exact opaque adapter (the correctness floor for every file type).
- Complete-cost candidate court with decode-before-commit: the winner is priced
  from its serialized bytes and byte-compared against the source.
- CLI: `encode`, `decode`, `materialize`, `verify`, `inspect`, `capabilities`.
- Docker-only, digest-pinned toolchain gates: stable (Rust 1.99.0), MSRV
  (Rust 1.89.0), and a PDF oracle tools image (qpdf/Poppler/MuPDF/Ghostscript).
- Courts: exact, malformed (hostile input), conformance.
- Phase 0 research synthesis and ADRs.

### Notes

- No compression headline was claimed in Phase 1. rANS and format-aware
  mechanisms were `PROPOSED` (Phases 2+), not `ADOPTED`.
