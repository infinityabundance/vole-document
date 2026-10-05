# ADR-0006: rANS is a substrate; the entropy seed is a capsule

- **Status:** Accepted — implemented (Phase 2)
- **Date:** 2026-10-05

## Context

The phrase "entropy seed" is dangerous if read as "one integer reconstructs
arbitrary data". In finite-width rANS the complete representation is the final
state **plus** the renormalization payload and the exact probability model /
context schedule.

## Decision

- Integrate the native Rust substrate `ryg-rans-rs = "=0.5.1"` through its
  **safe manual** API (`ryg_rans_rs::byte::{rans_byte_dec_init,
  rans_byte_dec_advance_symbol, rans_byte_dec_renorm}`, each returning
  `Result<_, DecodeError>`). The panicking `alloc_utils::decode` convenience path
  is **banned** on the normative hostile-input path.
- Define a VOLE-owned entropy semantic layer: a stable descriptor declaring coder
  family, state width, renormalization convention, `scale_bits`, frequency
  normalization, symbol ordering, lane count, stream direction, model encoding,
  payload length, symbol count, decoded length, and integrity.
- The "entropy seed" is a complete **capsule** carrying model identity/bytes,
  normalized frequencies, state(s), renormalization payload, counts, and
  integrity — never a scalar.
- The scalar implementation is the reference authority; SIMD/interleaved
  backends are admitted only after a bit-for-bit parity court.

## Implementation (Phase 2)

Landed in Phase 2 (`0.1.0-alpha.2`, campaign `2026-10-05-phase2-f6af30b`):

- **Safe manual API only.** The codec (`src/entropy/rans.rs`) uses
  `ryg_rans_rs::byte::{rans_byte_enc_put_symbol, rans_byte_enc_flush,
  rans_byte_dec_get, rans_byte_dec_advance_symbol}`. `ryg_rans_rs::alloc_utils`
  is not used; untrusted values are range-checked or use checked arithmetic, and
  the decoder never unwinds on malformed input.
- **Capsule fields.** A channel persists the model identity (`model_id`), scalar
  `initial_state`, renormalization `payload` (forward decoder-consumption order),
  `symbol_count`, and `decoded_length` — the complete decoder-entry state, never
  a scalar seed.
- **Canonical model.** A deterministic, integer-only normalization produces the
  516-byte dense `MODEL` record; a channel's `scale_bits` must agree with the
  model it names, and a missing model is rejected at parse time.
- **Hostile-safe decode with an exact-consumption integrity check.** Decoding
  bounds work before allocation and accepts a stream only when it consumes the
  payload exactly and returns the state to `RANS_BYTE_L`. That check is a
  structural invariant, not a checksum; record CRC-32C and the whole-source
  SHA-256 remain the content-integrity authorities.
- **Feature gating.** `rans` is on by default (`default = ["rans"]`); without it,
  channel-free descriptors still materialize exactly and channel-bearing ones
  return an explicit `UnsupportedFeature`.

Measured outcome (scoped, order-0 only): the campaign's `BYTE_RANS` wins are
fully attributed and its negative controls hold — rANS is not assumed to win.

## Consequences

- Every entropy channel is priced including its model bytes ("never free").
- Typed channels compete against RAW/RLE/bitpack/fixed-width; rANS is not assumed
  to win.
