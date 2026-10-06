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
| [0019](0019-seek-based-io.md) | Seek-based partial I/O makes a late observation view a measured bytes-read win | Accepted — scoped positive result, with recorded early/small-descriptor losses (Phase 8.3) |
| [0020](0020-content-addressed-store.md) | A content-addressed object store; three accounting universes | Accepted (design; cohort measurement Phase 9.3) |

## Adding an ADR

Copy the shape of an existing file: **Status**, **Context**, **Decision**,
**Consequences**. Record rejected alternatives. Never delete an ADR; supersede it
with a new one that cites the old.
