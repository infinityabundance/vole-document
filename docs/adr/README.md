# Architecture Decision Records

Frozen decisions that shape the implementation. An ADR is not marketing; it is
the written rationale that must survive chat history.

| ADR | Title | Status |
|---|---|---|
| [0001](0001-exact-bytes-only.md) | `EXACT_BYTES` is the only normative profile | Accepted |
| [0002](0002-one-crate.md) | One crate, module separation, no micro-workspace | Accepted |
| [0003](0003-docker-only.md) | Docker-only, digest-pinned reproducibility | Accepted |
| [0004](0004-wire-format.md) | Length-delimited records; no serde/bincode on the wire | Accepted (provisional format) |
| [0005](0005-bounded-dra.md) | Bounded, non-Turing-complete DRA with coverage certificate | Accepted |
| [0006](0006-rans-substrate.md) | rANS is a substrate; entropy seed is a capsule | Accepted (impl Phase 2) |
| [0007](0007-deflate-replay.md) | Exact DEFLATE replay is a per-stream candidate | Accepted (impl Phase 6) |
| [0008](0008-entropyfs-optional.md) | EntropyFS is optional; standalone form is sacred | Accepted (impl Phase 9) |
| [0009](0009-pdf-byte-authority.md) | PDF physical bytes are the authority; oracles are not | Accepted (impl Phase 3–8) |
| [0010](0010-typed-channels-rejected.md) | Typed lexical channels are rejected by complete cost | Accepted — recorded negative result (Phase 4) |
| [0011](0011-layout-prediction-framing.md) | Layout prediction is exact but loses to DRA op framing | Accepted — recorded negative result (Phase 5) |
| [0012](0012-packed-framing-threshold.md) | Packed segment framing is the layout threshold | Accepted (Phase 5.7) |
| [0013](0013-layout-rans-not-profitable.md) | Layout + rANS does not beat whole-file order-0 rANS | Accepted — recorded negative result (Phase 5.8) |
| [0014](0014-lgpl-cabac-dependency.md) | `preflate-rs` pulls an LGPL-3.0-or-later dependency (`cabac`) | Accepted — documented policy exception (Phase 6) |
| [0015](0015-deflate-replay-result.md) | Exact DEFLATE replay wins on shared plaintext with a large/weakly-coded appearance | Accepted — first measured positive for a PDF structural candidate (Phase 6) |
| [0016](0016-replay-resource-bound.md) | Decode-time DEFLATE replay is statically resource-bounded | Accepted (Phase 6.7) |
| [0017](0017-generic-lossless-baselines.md) | Generic lossless compressors are the whole-file comparator; VOLE's whole-file lanes lose to them (0/27) | Accepted — recorded methodology and negative result (Phase 7.0c) |
| [0018](0018-partial-materialization.md) | Partial materialization is a scoped random-access query-cost win on decode CPU, with no I/O win in v1 | Accepted — scoped positive result (Phase 7.3) |
| [0019](0019-seek-based-io.md) | Seek-based partial I/O makes a late observation view a measured bytes-read win (versus non-seekable sequential codecs only) | Accepted — scoped positive result, with recorded early/small-descriptor and seekable/blocked losses (Phase 8.3) |
| [0020](0020-content-addressed-store.md) | A content-addressed object store; three accounting universes | Accepted (design; cohort measurement Phase 9.3) |
| [0021](0021-cross-document-sharing-result.md) | Cross-document sharing does not beat generic chunk dedup (measured) | Accepted — recorded negative result (Phase 9.3) |
| [0022](0022-encoder-only-search-governance.md) | Encoder-only search governance, with zero decode authority | Accepted — mechanism implemented; parametric search is a recorded negative (Phase 10.1) |
| [0023](0023-consolidated-findings.md) | The current representation stack does not beat purpose-built baselines on any measured axis (superseding consolidated decision) | Accepted — top-level negative-results consolidation (Phase 10.2) |
| [0024](0024-document-field-authority.md) | The document is a persistent procedural field; authority is layered and never confused | Accepted (Phase 11.0) |
| [0025](0025-procedural-seed-dag.md) | A VOLE-owned, content-addressed, immutable procedural seed DAG stored one blob per node | Accepted (Phase 11.0) |
| [0026](0026-observation-query-provenance.md) | Typed observation algebra, provenance on every answer, EXPLAIN/EXPLAIN ANALYZE | Accepted (Phase 11.0) |
| [0027](0027-cost-accounting.md) | Four accounting universes; multi-objective cost; lifetime-cost headline | Accepted (Phase 11.0) |
| [0028](0028-finer-than-object-sharing.md) | Finer-than-object shareable units still lose to content-defined chunking (measured) | Accepted — recorded negative result (Phase 11.14) |
| [0029](0029-multi-format-authority-model.md) | Three distinct representational layers for a multi-format field; no lossy universal AST | Accepted (Phase 12.0) |
| [0030](0030-zip-physical-layer.md) | A byte-authoritative ZIP physical layer shared by DOCX/EPUB | Accepted (Phase 12.0) |
| [0031](0031-common-observation-model.md) | A shared observation vocabulary, not a shared schema; native selectors first-class | Accepted (Phase 12.0) |
| [0032](0032-docx-adapter-scope.md) | DOCX adapter scope: OPC semantic discovery, WML subset, stories, versioned profiles | Accepted (Phase 12.0) |
| [0033](0033-epub-adapter-scope.md) | EPUB adapter scope: OCF, package, spine-first order, bounded XHTML; no invented pages | Accepted (Phase 12.0) |
| [0034](0034-cross-document-identity-sharing.md) | Cross-document identity and sharing: state, not bytes; exact versioned content identity | Accepted (Phase 12.0) |
| [0035](0035-phase12-lifetime-benchmark.md) | Phase-12 lifetime benchmark: pre-registered ablations, accounting boundary, go/no-go | Accepted (Phase 12.0) |
| [0036](0036-pdf-length-revision-size.md) | PDF `/Length` + revision proceduralization is byte-exact but loses to the whole-file rANS lane and to generic compressors (0 wins over 28 files) | Accepted — recorded negative result (Phase 13.1) |
| [0037](0037-pdf-grammar-templates.md) | A bounded COS-token phrase grammar is byte-exact and becomes the best VOLE lane on 4/28 files (34,505 B auto-winner gain) but loses to generic compressors on all 7 it proposes | Accepted — mixed (scoped VOLE-ladder positive; loss vs generic) (Phase 13.2) |
| [0038](0038-odt-adapter-scope.md) | ODT adapter scope: ODF package, semantic content-part discovery, bounded OpenDocument subset, versioned profile; exactness is not conformance | Accepted (Phase 13.3) |
| [0039](0039-partial-materialization-checkpoints.md) | Byte-level partial-materialization checkpoints are byte-exact and advisory but redundant with the observation index and do not pay their framing (0 wins; +259 to +1,159 B per byte-range query) | Accepted — recorded negative result (Phase 13.4) |
| [0040](0040-benign-doctype.md) | A benign `DOCTYPE` (no internal subset) is accepted and ignored in the bounded-XML policy; a declaration with an internal subset is still refused | Accepted (Phase 13.7) |

## Adding an ADR

Copy the shape of an existing file: **Status**, **Context**, **Decision**,
**Consequences**. Record rejected alternatives. Never delete an ADR; supersede it
with a new one that cites the old.
