# VOLE-Document

Byte-exact procedural document storage.

VOLE-Document persists a **bounded deterministic reconstruction description** of a
document — reconstruction structure, parameters/state, typed residual channels,
and (in the entropy phases) typed rANS channels — and materializes the exact
original bytes on demand.

The governing invariant of the exact profile is uncompromising:

```text
materialize(descriptor) == original_bytes
```

Parsing successfully, producing "the same" text, the same object graph, the same
pages, the same rendering, or a canonical re-save are **not** substitutes.

> This project is deliberately *not* "a PDF optimizer that happens to use rANS".
> rANS is the entropy substrate beneath the representation, never the procedural
> model. See [`SPEC.md`](SPEC.md) and [`docs/`](docs/) for the architecture.

## Status

| Area | State | Evidence |
|---|---|---|
| Exact `.voldoc` container (framing, header, records) | **Implemented** | `src/container/`, unit + conformance courts |
| Typed errors + stable exit codes | **Implemented** | `src/error.rs` |
| Centralized resource limits | **Implemented** | `src/limits.rs` |
| CRC32C framing + SHA-256 archival identity | **Implemented** | `src/integrity.rs` |
| Document Reconstruction Algebra (literal subset) | **Implemented** | `src/dra/` |
| Coverage certificate (checked invariant) | **Implemented** | `src/dra/program.rs` |
| RAW exact opaque adapter | **Implemented** | `src/adapter/opaque/` |
| Candidate complete-cost court + decode-before-commit | **Implemented** | `src/encode/` |
| CLI (`encode`/`decode`/`verify`/`inspect`/`capabilities`) | **Implemented** | `src/main.rs` |
| Exact court over a mixed corpus | **Measured** | `evidence/campaigns/` |
| Native rANS floor (order-0 / typed byte channels) | **Measured** | campaign `2026-10-05-phase2-f6af30b` |
| RLE candidate (`REPEAT_LAST` run-length) | **Measured** | campaign `2026-10-05-phase2-f6af30b` |
| BYTE_RANS candidate (order-0 byte channel) | **Measured** | campaign `2026-10-05-phase2-f6af30b` |
| Entropy capsule (full decoder-entry state, not a seed) | **Measured** | ADR-0006; `src/entropy/` |
| PDF physical authority + adapters (Phase 3–8) | Planned | — |
| EntropyFS store-backed form (Phase 9) | Planned | — |
| DSFB search governance (Phase 10) | Planned | — |
| Partial materialization (Phase 11) | Planned | — |

"Implemented" means the mechanism exists and is tested. "Measured" means there is
a sealed campaign under `evidence/`. The Phase-1 core establishes exactness,
framing, integrity, bounds, and receipts before any entropy or format-aware
mechanism is allowed to compete; Phase 2 then measures entropy channels on that
same exactness floor.

### Phase 2 measured results

Phase 2 is **order-0 typed byte channels only** — no context model, no typed
residuals, and no format awareness. On a 9-file mixed corpus the cumulative
core→full ladder over serialized `.voldoc` bytes is:

```text
sum_source = 590081
sum_core   = 464474   (RAW + RLE)
sum_full   = 291304   (RAW + RLE + BYTE_RANS)
delta      = 173170   (sum_core - sum_full)
```

The entire delta is attributed to the two files where `BYTE_RANS` wins
(`text-256k.bin` 262144 → 143746; `skewed.bin` 65536 → 11382). `RLE` wins the
long runs (`zeros-64k.bin` 65536 → 283; `runs.bin` 65536 → 3088). Negative
controls hold: `BYTE_RANS` never wins on random 64 KiB (stored RAW at 65536 →
65845, a 309-byte fixed framing overhead) or on empty/one-byte inputs (RLE).
The canonical model's bytes are charged like any other bytes, so on tiny or
high-entropy inputs order-0 rANS loses to RAW/RLE as required — this is a scoped
measurement on one deterministic corpus, not a general compression claim.

The entropy substrate is optional in the build: `default = ["rans"]`, and a
channel-bearing descriptor decoded without the feature returns an explicit
`UnsupportedFeature`, never a silent reinterpretation.

Receipt:
[`evidence/campaigns/2026-10-05-phase2-f6af30b/`](evidence/campaigns/2026-10-05-phase2-f6af30b/).

## Quick start (Docker only)

All project commands run inside pinned containers. The host only invokes Docker.

```sh
# Build the toolchain image (pinned by digest in Dockerfile)
docker compose build dev

# Build, test, lint, format
docker compose run --rm --no-TTY dev cargo test  --all-features
docker compose run --rm --no-TTY dev cargo clippy --all-targets --all-features -- -D warnings
docker compose run --rm --no-TTY dev cargo fmt --all --check

# MSRV gate (Rust 1.89)
docker compose build msrv
docker compose run --rm --no-TTY msrv cargo build --locked

# Phase 1 exact court (writes an evidence receipt)
docker compose run --rm --no-TTY dev sh tools/phase1-court.sh
```

CLI surface (activated by the pipeline, not by extension — extensions are hints,
never authority):

```text
vole-document encode      INPUT          OUTPUT.voldoc
vole-document decode      INPUT.voldoc   OUTPUT
vole-document materialize INPUT.voldoc   OUTPUT
vole-document verify      INPUT.voldoc
vole-document inspect     INPUT.voldoc
vole-document capabilities
```

## Repository layout

```text
src/            one crate; modules for architectural separation
tests/          exact / malformed / conformance courts
tools/          court and gate scripts (run inside Docker)
docs/           architecture, ADRs, security, phase notes
evidence/       immutable campaign receipts (machine-readable)
research/       LOCAL ONLY — gitignored (paper, snapshots, subagent findings)
```

`research/` is intentionally excluded from version control. Durable findings that
matter to a phase are frozen into ADRs and phase notes and referenced from
receipts by hash.

## Licensing

Dual-licensed under either MIT or Apache-2.0, at your option. See
[`LICENSE-MIT`](LICENSE-MIT) and [`LICENSE-APACHE`](LICENSE-APACHE).
