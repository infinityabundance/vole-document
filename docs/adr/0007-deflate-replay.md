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

## Implementation notes (Phase 6, verified)

Empirical verification of the crate contract (Docker probe, `research/` only):

- `preflate-rs` operates on **raw DEFLATE** (RFC 1951). A full zlib stream is
  rejected (`Err(NonZeroPadding)`); the caller must strip the 2-byte zlib header
  and 4-byte Adler-32 trailer and re-emit them itself.
- Round trips were bit-exact for repetitive, random, short, and PDF-like inputs;
  corrections were byte-identical across process runs (**deterministic**).
- Reconstruction **panics** on many mutated correction blobs and returns wrong
  bytes for many others. Therefore decode wraps `recreate_whole_deflate_stream`
  in `catch_unwind` and relies on the whole-source SHA-256 court; a caught panic
  becomes a typed `CodecReplay` error.
- Analysis itself can panic on very large, highly repetitive input in a build
  with overflow checks; encode isolates and declines it.
- Default `PreflateConfig` is `max_chain_length = 4096`,
  `plain_text_limit = 128 MiB`, `verify_compression = true`; the plaintext limit
  is overridden from `Limits`, and a non-fully-consumed result is declined.
- License: `preflate-rs` → `cabac` is LGPL-3.0-or-later. See **ADR-0014**.
