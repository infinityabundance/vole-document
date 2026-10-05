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
  **replay-codec tag** and the original stream length. The tag names the exact
  reconstruction semantics on the wire (`REPLAY_DEFLATE_PREFLATE_0_7_6` = `1`);
  an unknown tag fails closed as `UnsupportedFeature`, so the opaque `preflate`
  representation is **experimental and version-coupled**, never silently promoted
  to a frozen archival standard. A future VOLE-owned replay engine gets a new
  codec id and a new universe.
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

### Wire tag and format status (Phase 6.7)

`Op::DeflateReplay` (`0x0A`) carries a leading `replay_codec: u8`. The only
value this build implements is `REPLAY_DEFLATE_PREFLATE_0_7_6 = 1`, which binds
the `corrections` blob to **`preflate` 0.7.6's private bitcode+CABAC layout**.
This is deliberately *not* a frozen v1 contract:

- The tag is read first; any other value returns
  `ErrorClass::UnsupportedFeature` (never `InvalidGraph`), so a document that
  names a codec this build does not implement fails closed instead of being
  replayed with the wrong semantics.
- The tag arrived with **DRA version 8** and the universe
  `…;dra-8;…+deflate-replay-preflate-0.7.6-experimental`; the `-experimental`
  suffix states on the wire that the correction blob tracks a specific third-party
  version rather than a stable VOLE-owned format.
- A future permissively licensed, VOLE-owned replay codec would be a new codec id
  (and a new universe string), not a silent reinterpretation of this one.
See ADR-0016 for the decode-time resource bound that accompanies the tag.
