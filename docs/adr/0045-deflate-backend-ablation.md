# ADR-0045: DEFLATE backend ablation — enable `miniz_oxide` SIMD; `zlib-rs` meets the bar and is recommended; `zune-inflate` is disqualified

- **Status:** Accepted — SIMD enabled; `zlib-rs` recommended (not yet adopted); `zune-inflate` disqualified (Phase 15.5)
- **Date:** 2026-10-07

## Context

The shipped inflate backend is `miniz_oxide` (scalar). Safe-Rust alternatives
exist, and `miniz_oxide` has an optional `simd` adler-32 path that was off. The
question is which *correct* backend is fastest, under a bar fixed **before**
measuring. The exactness path never calls an inflater (a stream's exact leaf is
its raw compressed span), so this is a derived-state (`Q_gen`) performance choice
only, and output identity is the gate.

## Decision

Pre-register the adoption bar **≥ 1.25× GB/s AND ≤ 1.10× peak RSS** versus
`miniz_oxide` scalar, and require byte-identity with the `miniz_oxide` reference
for every candidate. Measure over the real `real100-v1` compressed members.

## Consequences

- **Measured result (receipt
  `evidence/campaigns/2026-10-07-phase15-deflate-e676166/`):** **52,498 members,
  88 documents, 481 MiB compressed / 2,768 MiB decoded** (one candidate per
  process, so peak RSS is clean).

  | candidate | GB/s | vs miniz | mismatches | RSS vs miniz | bar |
  | --- | ---: | ---: | ---: | ---: | --- |
  | miniz (scalar) | 1.231 | reference | 0 | reference | — |
  | miniz-simd | 1.366 | 1.11× | 0 | 1.00× | below bar |
  | **zlib-rs** | **1.938** | **1.58×** | **0** | **1.00×** | **MEETS** |
  | zune-inflate | 1.922 | 1.56× | **255** | 0.93× | **DISQUALIFIED** |

- **`miniz-simd` is enabled.** The optional `simd` adler-32 path swaps only the
  checksum hasher, never the DEFLATE bit decoder, so decoded bytes are identical;
  it is a free, output-preserving 1.11×.
- **`zlib-rs` meets the bar and is the recommended backend swap** — byte-identical,
  RSS-neutral, 1.58×. It is **not yet adopted**: the court measures only, and
  swapping the shipped inflater is a separate, deliberate change guarded by this
  byte-identity witness.
- **`zune-inflate` is disqualified for incorrectness** — it decoded 255 members
  differently from the reference. A candidate that does not reproduce the
  reference bytes cannot be adopted regardless of speed.
- **Related:** the scalar PDF `find_endstream` substring scan was replaced with a
  reused `memchr::memmem::Finder` (feature `memmem-scan`); differential tests
  assert equality with the retained scalar oracle, so no `.voldoc` byte changes.
- **Limits.** 88 of 100 documents contributed members; 12,001 members were
  declined (unsupported filters) and are counted, not hidden. The lane has no
  `perf`; the harness reports its own peak RSS, which `/usr/bin/time -v` confirms.

## References

- `Cargo.toml` (`memmem-scan`, `miniz-simd`, `deflate-ablation`; deps `memchr`,
  `zlib-rs`, `zune-inflate`); `examples/deflate_ablation.rs`;
  `src/adapter/pdf/lexer.rs`; `docs/phases/phase-15-results.md` (15.5)
- `evidence/campaigns/2026-10-07-phase15-deflate-e676166/`
- ADR-0007/0015/0016: DEFLATE replay (a different axis — reconstruction of the
  exact bitstream — never an inflate backend choice)
