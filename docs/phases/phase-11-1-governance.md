# Phase 11.1 — Search governance: deterministic by default, zero decode authority

Status: accepted (Phase 11.1).

## What Phase 10 established

Phase 10 built an encoder-only search governor with **zero decode authority** and
measured it against an exhaustive candidate oracle and a fixed heuristic. The
result was a **recorded negative**: `all_equal = true`, `median_benefit_permille
= 0` (ADR-0022). The published `dsfb 0.1.2` crate is "Drift-Slew Fusion
Bootstrap" state estimation — a floating-point `f64` observer over
`rand`/`rand_distr` — with **no** candidate-family, residual, or search-directive
API. It cannot govern a representation search, and the project's own
`dsfb-search` feature is deliberately dependency-free (`dsfb-search = []`, not
`["dep:dsfb"]`).

## Phase 11 decision

* **Default policy is deterministic and fixed.** Phase 11's inverse compiler,
  observation planner, and cache use fixed, integer, reproducible rules. Nothing
  in the decode or observation path depends on a search process.
* **DSFB is declined**, not integrated. There is no `dsfb` crate in the decode
  dependency path and no persisted state a decoder must rediscover.
* **The reusable mechanism, if ever needed.** The only reusable idea from Phase
  10 is the *contract*, not the crate: a suggestion is merely another candidate,
  and every candidate must independently pass the coverage, exactness, and
  complete-cost courts. Phase 11 adopts this as a principle should a candidate
  space large enough to need bounding ever appear; it does not adopt the crate.

## The gate

`tests/field_authority.rs` enforces the boundary mechanically: no file in
`src/materialize`, `src/container`, or `src/dra` may reference `field::`,
`SeedStore`, `seed::`, or `dsfb` outside comments. With the entire `field`
feature deleted (`--no-default-features`), `materialize(descriptor) ==
original_bytes` still holds byte-exactly — the field is an additional plane, never
a prerequisite.
