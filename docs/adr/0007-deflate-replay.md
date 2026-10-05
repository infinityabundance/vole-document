# ADR-0007: Exact DEFLATE replay is a per-stream candidate

- **Status:** Accepted (Phase 0); implementation is Phase 6
- **Date:** 2026-10-05

## Context

A compressed bitstream is not uniquely determined by its plaintext. `preflate-rs`
recovers the original DEFLATE bitstream from plaintext plus compact correction
state — exactly the codec-replay layer the architecture calls for — but it has
sharp edges.

## Decision

- Integrate `preflate-rs = "=0.7.6"` (MSRV 1.89) as one **candidate**, never a
  blanket transformation. VOLE-Document finds the stream spans; preflate's generic
  container scanning is not used for discovery.
- Use the raw-DEFLATE entry points (`preflate_whole_deflate_stream` /
  `recreate_whole_deflate_stream`, plus the streaming pair for large inputs).
- Persist the correction state as **opaque bytes** (it is a private,
  version-coupled bitcode+CABAC layout with no stable public deserializer) plus a
  format-version tag and the original stream length.
- Require `reconstructed_stream == original_stream` **before** the candidate may
  enter the document-level size court.
- Isolate and bound reconstruction: it can `unwrap()` internally on hostile state,
  so run it in a bounded stage with external time/memory caps and always keep a
  store-raw fallback.

## Consequences

- Replay is a byte-cost competition per stream; losses are recorded, not hidden.
- Nothing about DEFLATE replay enters normative decode except the recorded
  correction bytes and the declared coder version.
