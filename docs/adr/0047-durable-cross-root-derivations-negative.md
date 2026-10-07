# ADR-0047: Durable cross-root derivations do not survive — `N3` violated again (not built)

- **Status:** Accepted — recorded negative result (Phase 15.7)
- **Date:** 2026-10-07

## Context

Phase 12 recorded `N3` as violated: cross-document *durable work* reuse falls to
`0.0` after `cache --clear`, even though representation identity is shared
(ADR-0034/0035). Phase 15.7 asked whether a stronger, **canonical derivation
identity** `(algorithm-version, dependency NodeIds, parameters)` — so that
identical inputs share *computed* state, not merely representation identity —
satisfies `N3` on the real publication/revision families. The design predicted a
violation; the court tests that prediction.

## Decision

Measure first. Run the `N3` control on the real `real100-v1` families with one
shared `FieldStore` per family, and implement a durable `DerivationStore` **only**
if the reuse condition holds. No Rust change is made by the court.

## Consequences

- **Measured result (receipt
  `evidence/campaigns/2026-10-07-phase15-crossroot-7429d61/`):** **`N3` is
  VIOLATED.**
  - **23** families, **56/56** members ingested, one shared `FieldStore` per
    family.
  - **Cross-member derived reuse = 0 nodes** (warm in-order reuse 30 = empty-cache
    floor 30).
  - Post-`cache --clear` fresh-process reuse **above** the intra-observation floor
    = **0** (no durable output store). The counter is live: a cache-intact fresh
    process reused 85.
  - Representation identity **is** shared: **44** nodes id-shared, **4** shared
    resources (`79,720 B`).
  - For contrast, chunk-level borg CDC saved **32,158,196** source bytes versus
    **0** derived-output bytes saved.
- **No Rust change was made.** The court is measurement-first: it shows a durable
  `DerivationStore` is **not warranted**, so building one would be unjustified
  complexity.
- **No compression claim.** A shared blob is scored as state/work, never as a
  store-size fraction. Exactness is out of scope for this court; it measures
  sharing and reuse.
- **Limits.** The only baseline is borg CDC (no SQLite comparison was reproduced);
  one frozen corpus; no population claim.

## References

- `tools/phase15-crossroot-court.sh`, `tools/fixtures/phase15-crossroot.py`
- `evidence/campaigns/2026-10-07-phase15-crossroot-7429d61/` (`N3.txt`,
  `receipt.json`, `raw/summary.json`)
- `docs/phases/phase-15-results.md` (15.7)
- ADR-0034/0035: cross-document identity sharing and the `N3` condition
