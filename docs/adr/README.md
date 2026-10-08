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
| [0041](0041-large-pdf-encode-bound.md) | Share one PDF physical scan across the candidate portfolio + stream the court; encoder time improves ~1.75× on >100 MiB PDFs, but the ~17× peak-memory bound is recorded, not solved | Accepted — partial fix (Phase 14) |
| [0042](0042-resident-session-negative.md) | A resident `DocumentFieldSession` + `observe-batch` does not beat the cold per-observation lane above ~1 MiB; the cold `narrow_probe` short-circuit is the mechanism | Accepted — recorded negative (Phase 15.2) |
| [0043](0043-packed-seed-store.md) | A segmented, offset-addressed packed `fieldpack` seed store: file/directory count 0.009× (111× fewer) at **byte parity**, latency at parity, identity unchanged | Accepted — adopted, seed namespace only (Phase 15.3) |
| [0044](0044-bounded-parallel-ingest.md) | Bounded parallel ingest (`parallel` feature, `--workers`) is exactly deterministic (10/10 field id + exact) but speed-neutral | Accepted — non-default (Phase 15.4) |
| [0045](0045-deflate-backend-ablation.md) | DEFLATE backend ablation: enable `miniz_oxide` SIMD; `zlib-rs` meets the bar (1.58×, RSS-neutral) and is recommended; `zune-inflate` is disqualified (255 mismatches) | Accepted — measured (Phase 15.5); `zlib-rs` **adopted as the shipped inflate backend in 16.1** (ADR-0045 fulfilled) |
| [0046](0046-adaptive-promotion-refuted.md) | Adaptive procedural promotion: all three pre-registered falsifiers fire; the mechanism ships opt-in and default-off | Accepted — recorded negative (Phase 15.6) |
| [0047](0047-durable-cross-root-derivations-negative.md) | Canonical derivation identity does not share computed state: `N3` violated again (0 cross-member reuse); no `DerivationStore` built | Accepted — recorded negative (Phase 15.7) |
| [0048](0048-cuda-lane-deferred.md) | The CUDA batch lane is deferred: the bandwidth gate is unopened and the pinned Docker lanes cannot see the GPU | Accepted — deferred, not measured (Phase 15.8) |
| [0049](0049-storage-accounting-correction.md) | Storage receipts must state their byte-accounting method: `du -sb` counts directory inodes (4096 B each), inflating one-file-per-node stores; file-bytes-only accounting corrects the Phase-15.3/16.2 storage headlines to parity | Accepted — correction (Phase 16.6) |
| [0050](0050-sqlite-as-substrate-question.md) | Should the document field use an embedded DB as part of its physical substrate? SQLite does not lose under an equal capability contract (16.5) and VOLE's storage edge corrects to ~0.9× (16.6); recorded as an open architectural question with the measurements that would settle it — no switch decided | Open — recorded question (Phase 16.5/16.6) |
| [0051](0051-direct-field-ingestion.md) | A direct source → field build path (`field-build --profile runtime`) with a fixed, non-searched exactness program (`RAW`, `candidates_evaluated == 1`); RAW preserves the observation surface because structure is re-scanned from the source, not read from the program. Measured 2.04× build, 3.4× lower peak RSS at 1.007× authority, exactness 9/9; recorded caveat: the PDF metadata projection's `object_count`/`graph_ops` counters are encoder-dependent | Accepted — adopted (Phase 17.1) |
| [0052](0052-revision-lineage-surface.md) | Expose PDF revision lineage (`Selector::Revisions`/`Representation::Lineage`, computed once at ingest and indexed; typed decline for non-PDF); the contract court's C4/C5 still do not close because the contract's C4 tuple is the corpus family/member/head — external metadata the PDF bytes cannot derive. Surface half fixed; contract-definition half open | Accepted — surface added; contract C4/C5 open (Phase 17.2) |
| [0053](0053-batched-packed-sync.md) | Batch the packed seed store's durability syncs to one `fsync` per segment (`SyncPolicy::Batch`, default; `--sync=batch|each`), flush before publishing a manifest, and let `observe-batch` serve `--packed`; crash recovery is a consistent prefix, exactness unchanged, and the contract build ratio moves 7.08× → **0.82×** (vs the *historical-control* configuration; a tuned envelope reads **0.219×** — Phase 22.1) | Accepted — adopted (Phase 18.5) |
| [0054](0054-repeatability-and-paired-measurement.md) | Measure with paired, interleaved repetitions of both lanes, retain every sample, quote a fixed-seed cluster bootstrap 95% CI and a ±10% tie band, and report the estimator (paired median/geometric mean vs ratio of sums) with every ratio; a performance claim must state its estimator and its interval. The build estimator-sensitivity (paired median **0.182** vs ratio of sums **~0.96**) and the warm re-read (**~1.29×**, marginal under the median, resolved under the geometric mean) are the measured basis, all against the *historical-control* configuration (a tuned envelope reads storage **0.762×**, build **0.219×**, warm a loss **1.211×** — Phase 22.1) | Accepted — measurement discipline (Phase 19.1/19.2) |
| [0055](0055-external-context-typed-external-metadata.md) | External (corpus/dataset) facts are a separate, explicitly-typed layer (`ExternalContext`, basis `ExternalMetadata`, `exact == false`) stored **beside** the document-derived field and never in the seed DAG, index, manifest, or exactness authority; removing it is one `unlink`. Supplied the same external input to both lanes, VOLE answers C4b and its tuple matches the baseline's (**12/12**), closing C4b as a *cost* question rather than a coverage gap | Accepted — C4b closes under an equal external input (Phase 20.4) |
| [0056](0056-runtime-vs-research-reconstruction-programs.md) | The **research/codec programme** (`encode` candidate portfolio) and the **runtime DocumentField programme** (`field-build --profile runtime`, fixed `RAW` + the native observation graph) are distinct; the runtime's claim is **not** that `RAW` is a novel compact reconstruction program — exact reconstruction of an intact `RAW` source is the **entry condition**, not the result. The runtime's economic mechanism (persistent dependency structure, typed exactness/provenance contracts, selective materialization, format-native access, and the cost of many observation types) must stand on its own merits, and the two must not be conflated in the IP story or diligence | Accepted — scope/presentation clarification (Phase 22 planning) |
| [0057](0057-directory-fsync-and-power-loss-proxy.md) | `fsync` the containing directory on every atomic publish (`DirSyncPolicy::Safe`, default; `--dir-sync=off` escape hatch) so a published manifest survives a power cut, and measure power loss with a barrier-log **model** that reconstructs only barrier-covered bytes and re-runs the Phase-20.1 invariants; `Batch` and `Each` are power-equivalent for the published field (flush-before-publish) | Accepted — adopted (Phase 23) |

## Adding an ADR

Copy the shape of an existing file: **Status**, **Context**, **Decision**,
**Consequences**. Record rejected alternatives. Never delete an ADR; supersede it
with a new one that cites the old.
