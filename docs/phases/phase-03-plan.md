# Phase 3 — PDF byte-authoritative physical authority (subphase plan)

Goal: an **owned**, conservative, span-oriented PDF physical scanner plus a
revision map, proving that every corpus PDF can be partitioned into exact byte
authorities or fall back conservatively. No aggressive structural compression
yet: the physical view is the exactness authority; the semantic view proposes
structure but never overwrites physical truth.

Execution rules (frozen):
- All compilation/testing runs in the pinned Docker `dev` service; PDF oracles in
  the pinned `tools` service. Never on host.
- Subphases proceed in order; each: implement, test in Docker, commit, push to
  `phase3`. No skipped parts.
- Subagents run one at a time, scoped to disjoint files, testing via
  `docker compose run --rm --no-TTY dev cargo test`.
- A PDF candidate is admitted only if byte-exact and strictly cheaper; a loss to
  RAW is recorded, not hidden.

## Subphases

| # | Subphase | Deliverable |
|---|---|---|
| 3.1 | PDF lexical scanner primitives | byte-exact, hostile-safe scanning of whitespace, comments, literal strings (escapes, nesting), hex strings, names, delimiters, regular tokens; span model |
| 3.2 | Physical structure scanner | `%PDF` header, `obj`/`endobj`, `stream`/`endstream`, `xref`, `trailer`, `startxref`, `%%EOF`, comments; typed spans covering exactly `[0,len)` |
| 3.3 | Stream `/Length` + revision boundaries | resolve direct/indirect `/Length`, determine exact stream byte spans, CRLF/LF handling, revision boundary detection |
| 3.4 | Revision map + xref-stream detection | model incremental updates (`/Prev` chains) as append-only revision ranges; structurally detect xref streams and object streams |
| 3.5 | `PdfPhysical` view + adapter candidate | physical view type, coverage integration, a PDF physical candidate (literal spans) + conservative fallback; forced materialization byte-exact |
| 3.6 | Deterministic PDF corpus + exact court | generator of classic-xref, xref-stream, object-stream, incremental, weird-EOL, string-trap, malformed-but-readable PDFs; every item exact or conservatively fallen back |
| 3.7 | qpdf differential oracle court | qpdf structural checks vs our scanner (object numbers/counts); qpdf is an oracle, never the authority |
| 3.8 | Phase-3 evidence campaign | coverage stats, exactness, fallback counts, oracle agreement, and the recorded court outcome (expected: RAW wins in Phase 3) |
| 3.9 | Docs + freeze + merge | SPEC/PROJECT_STATE/README/CHANGELOG/ADR-0009 updated; merge `phase3` into `main` |

## Acceptance gates (predeclared)

1. **Coverage**: every scanned PDF partitions into spans whose union is exactly
   `[0, len)` with no gaps and no overlapping authorities.
2. **Exactness**: forced physical materialization reproduces the source
   byte-for-byte, with matching SHA-256.
3. **Conservative fallback**: any unsupported/ambiguous/malformed region becomes
   a literal/raw authority; the parser never invents bytes.
4. **Hostile-safe**: malformed or adversarial PDFs never panic, hang, or allocate
   without bound.
5. **Oracle agreement**: qpdf's structural view agrees with our scanner where
   comparable; disagreements are investigated and recorded, and qpdf is never
   treated as the byte authority.
6. **Honest cost**: the PDF physical candidate is expected to lose to RAW in
   Phase 3 (no entropy/structural compression yet); this is recorded, not hidden.
