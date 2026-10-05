# Security notes

The normative threat model and hostile-input contract live in
[`/SECURITY.md`](../../SECURITY.md). This directory holds format- and
phase-specific security notes as they accrue.

## Phase 1 (exact core)

Attack surfaces and their bounds:

| Surface | Bound |
|---|---|
| Header | 64 bytes; CRC-32C self-check; version/feature/profile fail closed |
| Records | length-delimited; `max_record_len`, `max_record_count`; per-record CRC-32C |
| Graph | `max_graph_ops`; checked arithmetic; output bounded by `max_output_bytes` before allocation |
| Coverage | contiguous union must equal `declared_source_len`; gaps/overlaps rejected pre-materialization |
| Repeat expansion | `max_repeat_count`; consecutive/leading `REPEAT_LAST` rejected |
| Objects | `max_object_count`; object references bounds-checked |

Properties proven by `tests/malformed.rs`:

- Every single-byte mutation of a valid descriptor is detected.
- Truncation at any length is rejected.
- Unknown mandatory records/features/versions fail closed.
- Over-expanding graphs are rejected by the output limit before allocation.

## Phase 3+ (PDF) — planned

- No PDF JavaScript, actions, or embedded executables are ever executed.
- External XML entities and network fetches are disabled.
- Encrypted content is opaque by default; passwords never enter normative decode.
- Signed byte ranges round-trip identically; signing is not a decoder concern.
- Parser recovery is an encoder convenience only and never invents bytes.
