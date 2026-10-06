# ADR-0034 — Cross-document identity and sharing: state, not bytes

Status: accepted (Phase 12.0).
Extends ADR-0021 (object-granularity sharing), ADR-0028 (finer-than-object
sharing), ADR-0025 (seed DAG identity). Cites plan §DEC-8, §34–§37, §91;
research H §1–§7, J §2, J §7.

> **Post-review amendment (Phase 12.15 skeptic, `15b5729`).** The landed 12.8
> court (`evidence/campaigns/2026-10-06-phase12-share-e7ef693/`) reports the
> `retained_inverse_work_fraction` this ADR defines, computed from node
> executions + cold input bytes (not source size, as required), and correctly
> separates `nodes_id_shared` from `nodes_reused`. However, the three controls
> this decision **mandates** — a post-`cache --clear` receipt, an OS-level
> witness, and a CDC-baseline comparison — are **absent**. The `0.339907`
> fraction is therefore a single warm, in-process observation, and the `N3`
> go/no-go test (ADR-0035) is **not evaluated**. See
> [`docs/reviews/phase-12-skeptic-review.md`](../reviews/phase-12-skeptic-review.md)
> (F8).

## Context

Phase 9 and Phase 11.14 both measured that shareable units — coarse objects and
finer-than-object units — **lose to generic content-defined chunking** (CDC)
(ADR-0021/0028). The Phase-9 mistake was measuring reuse as *bytes* at the wrong
granularity: a fraction derived from source size called a loss a win. Phase 12
wants cross-document reuse across PDF/DOCX/EPUB, so it must not repeat that error.

## Decision

* **Three identity kinds, none interchangeable.** *Content identity*
  (`BLAKE3-256(exact bytes)`, e.g. `NodeId`) is global and shareable. *Logical-
  occurrence identity* (a document root + selector / occurrence path) is
  per-document and **not** shareable. *Physical-source identity*
  `(source_sha256, offset, len)` is a span in one immutable source, **not**
  shareable.
* **Sharing requires exact versioned content identity.** A `NodeId` matches only
  when bytes *and* `materializer_id` + version + deps + params match. No second
  "logical equivalence" key may map distinct bytes to one store entry.
* **Cross-format semantic dedup is out of scope** until canonicalization,
  versioning, provenance completeness and round-trip preservation are *proven*. At
  most it is a `Q_gen` `Inferred`/`Heuristic` observation — never an exact node,
  never on the `materialize` path (a shared node maps different bytes to one entry,
  the Phase-9 error with worse consequences).
* **Reuse is measured as state/work, not bytes:**
  `retained_inverse_work_fraction = reused_persisted_inverse_work /
  total_inverse_work_required_by_cold_reconstruction`, where work = node executions
  plus the exact cold input bytes each execution would have read. Receipt it warm
  **and** post-`cache --clear`, cross-check with an OS-level witness, and compare
  against a CDC baseline. "No genuine reuse" is an **admissible loss**.
* Keep `nodes_id_shared` (a representation fact, 0 bytes written) distinct from
  `nodes_reused` (work served from the persisted store). Materializer / registry
  (code) sharing is never counted as data reuse.
* **Guard against the Phase-9 object-granularity mistake:** per-member, per-part
  and per-XML-token nodes must each pay their own byte cost and be scored on reuse
  work; a fraction computed from source size is inadmissible.

## Consequences

* Cross-document reuse falls out of `NodeId` equality with no new keying; identical
  subtrees share one seed blob and one cache entry.
* The realistic reuse surface is *decoded members and identical resources*, not raw
  compressed members (per-entry LFH metadata defeats those) and not canonical XML
  (unproven). Any value must be demonstrated, never assumed.

**Rejected:** a byte-based reuse metric; cross-format semantic hashing /
canonicalization dedup; a second logical-equivalence key; counting code reuse as
data reuse; assuming raw compressed-member identity is common; a cache-only warm
claim without the post-`cache --clear` control.
