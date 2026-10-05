# ADR-0016: Decode-time DEFLATE replay is statically resource-bounded

- **Status:** Accepted (Phase 6.7)
- **Date:** 2026-10-05

## Context

`DEFLATE_REPLAY` (ADR-0007) reconstructs a raw DEFLATE bitstream from a
`(plaintext, corrections)` pair at decode time. `preflate-rs = "=0.7.6"` offers no
bounded *streaming* reconstruction sink: `recreate_whole_deflate_stream` returns
one in-memory buffer, so a hostile descriptor that declares an enormous
`declared_output_len` could drive an unbounded allocation before the whole-source
SHA-256 court ever runs. ADR-0007 already isolates the call with `catch_unwind`,
but `catch_unwind` addresses **panics**, not **memory growth**; a resource bound is
a separate requirement.

The declared length is not a free parameter the decoder may trust. A raw DEFLATE
stream carries, at most, one Huffman-coded literal per output byte (length/distance
matches can only *shrink* the stream relative to that encoding), and DEFLATE caps
Huffman code lengths at 15 bits. A stream that inflates to `P` bytes therefore
cannot exceed the size of a literals-only encoding of `P` bytes: `2*P + 1024`
bytes, where the slack covers block headers, end-of-block codes, stored-block
overhead, and small-window cases. A declared output above this bound is
**impossible**, not merely large.

## Decision

- Analysis (`Program::analyze`) computes the plaintext source length `P` — the
  referenced object length, or the decoded channel length — and rejects
  `declared_output_len > max_raw_deflate_len(P)` with `InvalidGraph`, where
  `max_raw_deflate_len(P) = 2*P + 1024` (saturating). This happens **before** the
  replay engine is invoked, so an impossible claim never reaches `preflate`.
- Evaluation (`Program::eval`) additionally rejects a `plaintext.len()` or
  `corrections.len()` above `max_record_len` with `ResourceLimit`, so the two
  inputs handed to the engine are themselves bounded by the record limit that
  applies to every object.
- The existing `max_output_bytes` check remains the outer budget bound.
- An unknown `replay_codec` (see ADR-0007) fails closed as `UnsupportedFeature`
  before any of the above, and a descriptor containing the op declares the
  mandatory `FEATURE_DEFLATE_REPLAY` bit, so a build without the feature never
  runs replay at all.

## Consequences

- A descriptor with an impossible declared replay length is rejected
  deterministically at analysis, before allocation; the fail-closed error class is
  `InvalidGraph`.
- The bound is **conservative and static**: it can admit a declared length that is
  larger than any stream `preflate` would actually produce, but it can never admit
  a length that is physically impossible, so it cannot reject a legitimate exact
  replay.
- The whole-source SHA-256 court and `materialize` length check remain the final
  correctness authority; the static bound is a resource-safety gate, not a
  correctness gate.
- This is the primary resource bound for replay precisely because `preflate-rs`
  0.7.6 exposes no incremental sink. A future VOLE-owned replay engine can add a
  **true streaming bound** (decode into a caller-supplied sink that stops at the
  declared length) and supersede this static approximation; that engine would also
  carry a new `replay_codec` id and a new universe (ADR-0007).

## References

- ADR-0007: exact DEFLATE replay is a per-stream candidate (opcode, correction
  representation, and the `replay_codec` tag)
- ADR-0014: `preflate-rs` pulls LGPL-3.0-or-later `cabac`
- `src/dra/program.rs` (`max_raw_deflate_len`, `analyze`, `eval`); `src/codec/deflate.rs`
