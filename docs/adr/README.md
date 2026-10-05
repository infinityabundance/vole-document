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

## Adding an ADR

Copy the shape of an existing file: **Status**, **Context**, **Decision**,
**Consequences**. Record rejected alternatives. Never delete an ADR; supersede it
with a new one that cites the old.
