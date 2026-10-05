# Phase 2 — Native rANS floor (subphase plan)

Goal: make rANS a real, bounded, hostile-safe entropy **substrate** beneath typed
channels, with complete-cost competition against RAW/RLE, and prove honest
negative controls. An rANS "seed" is always a full capsule, never a scalar.

Execution rules (frozen for this phase):

- All compilation/testing runs in the pinned Docker `dev` service. Never on host.
- Work proceeds in the subphases below, in order. Each subphase: implement, test
  in Docker, commit, push to `staging`. No skipped parts.
- Subagents run one at a time; each is read/write-scoped to disjoint files and
  must run its tests via `docker compose run --rm --no-TTY dev cargo test`.
- A candidate is admitted only if it is byte-exact and strictly cheaper than the
  best fallback on **complete** serialized cost (model bytes included).

## Subphases

| # | Subphase | Deliverable | Files (disjoint) |
|---|---|---|---|
| 2.1 | Entropy semantic layer | coder descriptor + `EntropySeedCapsule` types; MODEL + ENTROPY_CHANNEL record wiring (encode/parse, fail-closed) | `src/entropy/mod.rs`, `src/entropy/codec.rs`, `src/container/descriptor.rs` |
| 2.2 | Canonical model normalization | symbol histogram → normalized frequency table (deterministic rounding, `scale_bits`, zero-freq handling); model serialize/parse + integrity | `src/entropy/model.rs` |
| 2.3 | Scalar byte rANS codec | order-0 encode/decode on `ryg_rans_rs::byte` **safe manual** API; typed errors for every hostile case | `src/entropy/rans.rs` |
| 2.4 | RLE candidate | `REPEAT_LAST`-based run-length candidate in the court | `src/encode/candidates.rs`, `src/adapter/opaque/mod.rs` |
| 2.5 | BYTE_RANS candidate | encode channel + capsule into descriptor; complete-cost accounting (model + payload charged) | `src/encode/candidates.rs`, `src/container/descriptor.rs` |
| 2.6 | Negative controls + gates | RAW wins on incompressible/small; override rules; deterministic tie-break | `tests/entropy.rs` |
| 2.7 | Parity & corruption goldens | scalar authority; model/state round-trip; corruption ⇒ typed error | `tests/entropy.rs`, `src/entropy/rans.rs` |
| 2.8 | Property / fuzz tests | never-panic + bounded-failure over entropy parsers | `fuzz/`, `tests/entropy.rs` |
| 2.9 | Evidence campaign | cumulative ladder + leave-one-out + negative controls + attribution | `tools/phase2-court.sh`, `evidence/` |
| 2.10 | Docs + freeze | SPEC/PROJECT_STATE/README/ADR-0006/CHANGELOG updated with measured results | docs |

## Acceptance gates (predeclared)

1. Exact: every admitted descriptor materializes byte-for-byte.
2. Hostile-safe: malformed entropy descriptors/capsules ⇒ typed error, never panic.
3. Complete cost: model bytes and framing are charged; no "free model".
4. Negative control: on incompressible input RAW (or a trivial lane) wins.
5. Determinism: same input ⇒ same descriptor bytes.
6. Parity: scalar decode is the reference; any accelerated path is bit-identical.
7. Honest result: if rANS loses broadly, that is recorded, not hidden.
