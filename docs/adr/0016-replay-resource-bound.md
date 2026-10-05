# ADR-0016: Decode-time DEFLATE replay is bounded by a VOLE replay-profile admission limit

- **Status:** Accepted (Phase 6.7; re-framed in Phase 7.0; F2 contained on the
  decode path in Phase 7.1b)
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

**There is no finite DEFLATE maximum to appeal to.** RFC 1951 ([RFC1951] §3.2.4)
permits a compressed data set to be a series of blocks of arbitrary sizes,
including arbitrarily many **empty non-final stored blocks**. A legal bitstream
that inflates to *zero* bytes can therefore be arbitrarily large, so no finite
function `f` bounds `compressed_size` by `f(decompressed_size)`. In particular,
`2*P + 1024` is **not** a theorem about legal DEFLATE. It is a useful *algorithmic*
expansion figure — roughly a literals-only encoding of `P` bytes — that is a good
faith estimate of what a normal compressor emits for a `P`-byte plaintext, but a
hostile bitstream is free to exceed it without violating RFC 1951.

Consequently, a decoder that only behaved correctly for RFC-legal input could not
use `2*P + 1024` as a rejection rule. VOLE-Document instead makes a deliberate
**admission decision**: it declines to exact-replay descriptors outside a bounded
**replay profile**, and records that the decline is a policy choice rather than a
claim about the format.

## Decision

- **Replay profile.** Definition (`src/dra/program.rs`):

  ```text
  replay_profile_limit(P, limits) =
      min(limits.max_output_bytes,
          limits.max_replay_bytes,
          P * REPLAY_OUTPUT_RATIO_PERCENT / 100 + REPLAY_OUTPUT_SLACK)
  ```

  with `REPLAY_OUTPUT_RATIO_PERCENT = 200` and `REPLAY_OUTPUT_SLACK = 1024`
  (saturating). The ratio and slack are VOLE policy, not RFC constants.
  `Limits::max_replay_bytes` (default `1 << 34`, strict `1 << 26`) is a
  VOLE-specific admission cap on a single `DEFLATE_REPLAY` output.
- **Analysis** (`Program::analyze`) computes the plaintext source length `P` — the
  referenced object length, or the decoded channel length — and rejects
  `declared_output_len > replay_profile_limit(P, limits)` with `InvalidGraph`. This
  happens **before** the replay engine is invoked, so an out-of-profile claim never
  reaches `preflate`. The separate `max_output_bytes` budget check is retained as
  the outer materialization bound.
- **Evaluation** (`Program::eval`) additionally rejects a `plaintext.len()` or
  `corrections.len()` above `max_record_len` with `ResourceLimit`, so the two
  inputs handed to the engine are themselves bounded by the record limit that
  applies to every object.
- An unknown `replay_codec` (see ADR-0007) fails closed as `UnsupportedFeature`
  before any of the above, and a descriptor containing the op declares the
  mandatory `FEATURE_DEFLATE_REPLAY` bit, so a build without the feature never
  runs replay at all.
- **Process isolation (Phase 7.1b, contains fuzz finding F2).** The static
  replay-profile bound and `catch_unwind` address the *declared* output and
  *panics*, not the third-party decoder's *internal* allocation. Fuzzing (Phase
  7.1) found F2: a 33-byte hostile `(plaintext, corrections)` pair drives a
  multi-gigabyte allocation inside `preflate-rs` 0.7.6 (observed peak RSS
  2532 MiB under the pinned `fuzz` service; the third-party decoder allocates
  from attacker-controlled correction state, e.g. a reconstructed reference
  length). On the **decode path** only, `Program::eval` now calls
  `replay_bounded`, which runs `recreate_whole_deflate_stream` in a child process
  under an `RLIMIT_AS` address-space cap (`ulimit -v`) and a wall-clock timeout
  (`VOLE_REPLAY_MEM_MB`, `VOLE_REPLAY_TIMEOUT_MS`) and returns a typed
  `CodecReplay` on abort, timeout, or malformed reply. The in-process
  `replay_raw` remains for the encoder's own `try_replay` verification and the
  fuzz targets.
- **Library fallback.** `replay_bounded` uses the executable named by
  `VOLE_REPLAY_WORKER`, or one installed by the CLI itself via
  `install_default_replay_worker` (a safe setter, because `std::env::set_var` is
  `unsafe` under Rust 2024 and this crate forbids `unsafe`). If **no** worker is
  configured the library falls back to the in-process `replay_raw`; a
  configured-but-broken worker is a typed error and never silently falls back.

## Consequences

- A descriptor whose declared replay length exceeds the VOLE replay profile is
  rejected deterministically at analysis, before allocation; the fail-closed error
  class is `InvalidGraph`. The message states explicitly that the limit is a VOLE
  policy bound and that RFC 1951 permits unbounded empty non-final blocks.
- The bound is **conservative and static**: it can admit a declared length larger
  than any stream `preflate` would actually produce, and it may **decline a
  legitimate exact replay** whose real output exceeds the profile (for example a
  heavily fragmented or stored-block-heavy bitstream). That decline is an
  intentional, honest admission cost of the policy — not evidence that the stream
  is illegal.
- The whole-source SHA-256 court and `materialize` length check remain the final
  correctness authority; the static bound is a resource-safety gate, not a
  correctness gate.
- **F2 is contained, not eliminated.** Process isolation bounds the *decoder's*
  exposure: a malformed `.vcdoc` can no longer amplify memory in the decoder
  process, because the third-party allocation happens in a child with a hard
  address-space cap and a timeout. It is **not** a proof that `preflate` is
  bounded. The residual is a consumer that decodes untrusted `.vcdoc` without a
  worker path (`VOLE_REPLAY_WORKER` unset and no installed default, e.g. a
  library embedder): that path is in-process and still exposed. The CLI installs
  itself as the default worker, so its `decode`/`verify` are isolated by default.
- This is the primary resource bound for replay precisely because `preflate-rs`
  0.7.6 exposes no incremental sink. A **stronger** guarantee would come from
  either of:
  - a bound **specific to** `REPLAY_DEFLATE_PREFLATE_0_7_6`, proving that the
    outputs this exact preflate 0.7.6 layout can emit satisfy
    `len <= replay_profile_limit(P)` (a property of the implementation, not of
    RFC 1951); or
  - a future **VOLE-owned streaming replay** that decodes into a caller-supplied
    sink and enforces the limit on the output *as it is produced*, so the profile
    becomes a true runtime cap rather than a pre-check on a declared length. That
    engine would carry a new `replay_codec` id and a new universe (ADR-0007).

## References

- RFC 1951: DEFLATE Compressed Data Format Specification version 1.3
  (<https://www.rfc-editor.org/rfc/rfc1951>), in particular §3.2.4 (non-compressed
  / stored blocks) for arbitrarily many empty non-final blocks.
- ADR-0007: exact DEFLATE replay is a per-stream candidate (opcode, correction
  representation, and the `replay_codec` tag)
- ADR-0014: `preflate-rs` pulls LGPL-3.0-or-later `cabac`
- `src/dra/program.rs` (`replay_profile_limit`, `REPLAY_OUTPUT_RATIO_PERCENT`,
  `REPLAY_OUTPUT_SLACK`, `analyze`, `eval`); `src/limits.rs` (`max_replay_bytes`);
  `src/codec/deflate.rs` (`replay_raw`, `replay_bounded`, `run_worker_stdio`,
  `REPLAY_WORKER_SUBCOMMAND`); `tests/replay_isolation.rs`; `SECURITY.md`

[RFC1951]: https://www.rfc-editor.org/rfc/rfc1951
