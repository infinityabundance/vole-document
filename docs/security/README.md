# Security notes

The normative threat model and hostile-input contract live in
[`/SECURITY.md`](../SECURITY.md). This directory holds format- and
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

## Phase 3+ (PDF) — implemented

- No PDF JavaScript, actions, or embedded executables are ever executed.
- External XML entities and network fetches are disabled.
- Encrypted content is opaque by default; passwords never enter normative decode.
- Signed byte ranges round-trip identically; signing is not a decoder concern.
- Parser recovery is an encoder convenience only and never invents bytes.

## Phase 11–12 (persistent field + ZIP/OPC/OCF/XML) — implemented

The persistent field and the Phase-12 ZIP/OPC/OCF/XML adapters extend the
hostile-input surface. The normative contract remains
[`/SECURITY.md`](../SECURITY.md). The Phase-12 campaign
`2026-10-06-phase12-security-33f6d04` ran **315/315** court assertions over 45
hostile fixtures (16 reject / 22 opaque-preserve-and-decline / 7 accept; every
accepted fixture still `materialize --exact`), a 15/15 library court, and 8 new
bounded fuzz targets (`zip_scan`, `zip_decode`, `opc_rels`, `docx_wml`,
`epub_package`, `epub_content`, `xml_part`, `common_observe`), each `exit=0` with
no new crash, hang or amplification. Bounded properties:

- **ZIP** is parsed by a physical span scanner; ZIP64 and data descriptors are
  bounded, and a member that cannot be covered exactly is declined, never guessed.
- **OPC/OCF** relationship and content-type graphs reject cycles and bound depth;
  a package that cannot be resolved exactly is declined or preserved opaque.
- **DOCX/EPUB XML** is parsed with external entities and network fetches disabled;
  a part outside the bounded subset is preserved opaque, not approximated.
- The hostile-input class tally and gate status are recorded in
  [`docs/phases/phase-12-results.md`](../phases/phase-12-results.md); the decline-rate
  threshold `N4` remains **not evaluated**.
