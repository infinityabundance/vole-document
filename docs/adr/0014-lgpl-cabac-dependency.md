# ADR-0014: `preflate-rs` pulls an LGPL-3.0-or-later dependency (`cabac`)

- **Status:** Accepted — documented dependency-policy exception
- **Date:** 2026-10-05

## Context

Phase 6 integrates `preflate-rs = "=0.7.6"` as the exact DEFLATE replay engine
(ADR-0007). Running the dependency gate (`cargo deny check`) in the pinned
`policy` container surfaced two license facts that were not in the earlier
allow-list:

- `unicode-ident` is `(MIT OR Apache-2.0) AND Unicode-3.0` — a standard,
  permissive proc-macro dependency of `syn`.
- `cabac = 0.15.0`, a **normal, non-optional** dependency of `preflate-rs`, is
  licensed **LGPL-3.0-or-later**.

`cabac` is compiled into any artifact that enables the `deflate-replay` feature.
Rust links statically by default, so the LGPL applies to the combined binary in
the way LGPL does for statically linked works, not merely as a dynamically linked
library.

## Decision

- Allow `Unicode-3.0` in `deny.toml` (permissive).
- Allow `LGPL-3.0-or-later` in `deny.toml` as an **explicit, commented exception**
  tied to `preflate-rs` → `cabac`, and document the consequence rather than hide
  it.
- Keep `deflate-replay` an **optional, OPT-IN cargo feature**, *not* part of the
  default feature set: the default build is `default = ["rans"]`, so it is
  **permissive-only** and pulls no copyleft code. Enable replay explicitly with
  `--features deflate-replay` (or `--all-features`). A distributor that builds the
  default feature set, or the explicitly permissive-only
  `--no-default-features --features rans`, ships an artifact containing no LGPL
  code; a descriptor that requires replay then fails closed with
  `UnsupportedFeature` rather than being reinterpreted.
- Record the exception in the README, CHANGELOG, and this ADR so no downstream
  user is surprised.

## Consequences

- The crate's own source remains `MIT OR Apache-2.0`. The **combined binary** built
  with the `deflate-replay` feature additionally contains LGPL-3.0-or-later code
  (`cabac`). Redistributing that binary carries LGPL obligations.
- Replacing `preflate-rs` with a permissively licensed exact-DEFLATE-replay engine
  would remove the exception; none is known today. If one appears, revisit this
  ADR and supersede it.
- `cargo deny` remains fail-closed for any *other* license: only these two named
  licenses are added to the allow-list.
