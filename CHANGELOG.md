# Changelog

All notable changes are recorded here. The format is pre-1.0 and provisional.

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

- No compression headline is claimed. rANS and format-aware mechanisms are
  `PROPOSED` (Phases 2+), not `ADOPTED`.
