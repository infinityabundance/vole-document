# ADR-0006: rANS is a substrate; the entropy seed is a capsule

- **Status:** Accepted (Phase 0); implementation is Phase 2
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

## Consequences

- Every entropy channel is priced including its model bytes ("never free").
- Typed channels compete against RAW/RLE/bitpack/fixed-width; rANS is not assumed
  to win.
