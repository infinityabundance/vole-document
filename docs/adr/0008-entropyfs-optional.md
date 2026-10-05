# ADR-0008: EntropyFS is optional; the standalone form is sacred

- **Status:** Accepted (Phase 0); implementation is Phase 9
- **Date:** 2026-10-05

## Context

Sharing immutable objects across documents is valuable, but an archival artifact
must decode without a mount, daemon, network, or DSFB.

## Decision

- The standalone `.voldoc` form requires **no** EntropyFS, DSFB, network, or
  external tool.
- Use the embeddable `entropyfs = "=0.7.17"` `engine::Engine`
  (`Engine::create/open`, `put_blob`/`get_blob`/`read_blob_range`, BLAKE3
  `BlobId`) with `default-features = false` — never the FUSE/ublk frontends.
- Since EntropyFS exposes a concrete `Engine` (no `ObjectStore` trait), VOLE-Document
  defines its own `ObjectStore`-style abstraction and writes a thin adapter.
- Note: `dsfb` is a hard (non-optional) dependency of EntropyFS; enabling the
  `entropyfs-store` feature therefore pulls DSFB as a dependency, but DSFB still
  retains **zero** decode authority.
- A store-backed descriptor may hold immutable external content IDs; the generic
  materializer depends on an object resolver, not on EntropyFS specifically.
  Standalone ↔ store-backed conversion must materialize identical bytes.

## Consequences

- Three accounting universes stay distinct: standalone, unique-reachable, and
  amortized. A tiny root reference is never reported as a small standalone file.
- A store directory and an exclusive lock are required (one engine per store).
