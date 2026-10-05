# ADR-0001: `EXACT_BYTES` is the only normative profile

- **Status:** Accepted (Phase 0)
- **Date:** 2026-10-05

## Context

Document systems commonly conflate "the same document" with "the same bytes".
PDF re-save, XML canonicalization, and AST round-trips all produce *equivalent*
documents that are not byte-identical. The prior-art paper names three profiles
(`EXACT_BYTES`, `CANONICAL_DOCUMENT`, `SEMANTIC_DOCUMENT`) and forbids mixing
their ratios.

## Decision

Only `EXACT_BYTES` is normative and implemented. The header `exactness_profile`
must equal `0`; any other value fails closed with `UnsupportedFeature`. No
canonical or semantic profile exists until it can be named, versioned, and
measured separately.

## Consequences

- Every accepted stream satisfies `materialize(D) == X` byte-for-byte.
- We cannot claim "compression" from semantic dedup or canonicalization.
- A future canonical/semantic profile must be a distinct universe and must never
  be reported as archival preservation.
