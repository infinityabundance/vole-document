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
| 2.1 | Canonical model normalization | symbol histogram → normalized frequency table (deterministic rounding, `scale_bits`, zero-freq handling); model serialize/parse + validation | `src/entropy/model.rs` ✅ **done** |
| 2.2 | Scalar byte rANS codec | order-0 encode/decode on `ryg_rans_rs::byte` **safe manual** API; typed errors for every hostile case | `src/entropy/rans.rs` ✅ **done** |
| 2.3 | Entropy descriptor + container/DRA wiring | `EntropyChannelDescriptor`; MODEL + ENTROPY_CHANNEL records; `DECODE_CHANNEL` DRA op; materialize decodes channels | `src/entropy/mod.rs`, `src/entropy/codec.rs`, `src/container/descriptor.rs`, `src/dra/*`, `src/materialize/mod.rs` ✅ **done** |
| 2.4 | RLE candidate | `REPEAT_LAST`-based run-length candidate in the court | `src/encode/candidates.rs`, `src/adapter/opaque/mod.rs` ✅ **done** |
| 2.5 | BYTE_RANS candidate | encode channel + capsule into descriptor; complete-cost accounting (model + payload charged) | `src/encode/candidates.rs`, `src/container/descriptor.rs` ✅ **done** |
| 2.6 | Negative controls + gates | RAW wins on incompressible/small; override rules; deterministic tie-break | `tests/entropy.rs` ✅ **done** |
| 2.7 | Parity & corruption goldens | scalar authority; model/state round-trip; corruption ⇒ typed error | `tests/entropy.rs`, `src/entropy/rans.rs` ✅ **done** (reference-oracle parity + goldens in `tests/goldens.rs`) |
| 2.8 | Property / fuzz tests | never-panic + bounded-failure over entropy parsers | `fuzz/`, `tests/entropy.rs` ✅ **done** (`tests/property.rs`, `tools/soak-fuzz.sh`) |
| 2.9 | Evidence campaign | cumulative ladder + leave-one-out + negative controls + attribution | `tools/phase2-court.sh`, `evidence/` ✅ **done** |
| 2.10 | Docs + freeze | SPEC/PROJECT_STATE/README/ADR-0006/CHANGELOG updated with measured results | docs ✅ **done** |

## Acceptance gates (predeclared)

1. Exact: every admitted descriptor materializes byte-for-byte.
2. Hostile-safe: malformed entropy descriptors/capsules ⇒ typed error, never panic.
3. Complete cost: model bytes and framing are charged; no "free model".
4. Negative control: on incompressible input RAW (or a trivial lane) wins.
5. Determinism: same input ⇒ same descriptor bytes.
6. Parity: scalar decode is the reference; any accelerated path is bit-identical.
7. Honest result: if rANS loses broadly, that is recorded, not hidden.

## Measured results

Campaign `2026-10-05-phase2-f6af30b` (verdict **PASS**), receipt at
`evidence/campaigns/2026-10-05-phase2-f6af30b/`. All corpus items were encoded
with both the CORE binary (RAW + RLE) and the FULL binary (RAW + RLE +
BYTE_RANS), decoded by the producing binary, `cmp`-compared, and `verify`-ed.
Lengths are serialized `.voldoc` byte counts; the winner is the complete-cost
court's choice with model bytes charged.

### Cumulative ladder (9-file mixed corpus)

```text
sum_source = 590081
sum_core   = 464474   (RAW + RLE)
sum_full   = 291304   (RAW + RLE + BYTE_RANS)
delta      = 173170   (sum_core - sum_full)
```

The delta is fully attributed to the two `BYTE_RANS` wins:

| file | core_len | full_len | delta |
|---|---:|---:|---:|
| `text-256k.bin` | 262453 | 143746 | 118707 |
| `skewed.bin` | 65845 | 11382 | 54463 |
| **sum** | | | **173170** |

### Negative controls

| file | core_len | full_len | winner | ok |
|---|---:|---:|---|---|
| `empty.bin` | 272 | 272 | RLE | true |
| `one.bin` | 278 | 278 | RLE | true |
| `rand-det-64k.bin` | 65845 | 65845 | RAW | true |
| `rand-urandom-64k.bin` | 65845 | 65845 | RAW | true |

All negative controls passed: order-0 rANS loses to RAW/RLE on tiny and
high-entropy inputs once the 516-byte canonical model is charged. `RLE` wins the
runs (`zeros-64k.bin` 65536 → 283; `runs.bin` 65536 → 3088). This is a scoped
order-0 result on one deterministic corpus, not a general compression claim.
