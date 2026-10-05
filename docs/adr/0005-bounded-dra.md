# ADR-0005: A bounded, non-Turing-complete DRA with a coverage certificate

- **Status:** Accepted (Phase 1)
- **Date:** 2026-10-05

## Context

"Store a program that regenerates the document" invites arbitrary code execution,
unbounded work, and unbounded expansion. The paper instead specifies a finite,
versioned reconstruction algebra with statically boundable work.

## Decision

The Document Reconstruction Algebra is a tiny instruction set
(`EMIT_OBJECT`, `INLINE`, `REPEAT_LAST`) with deterministic semantics, checked
arithmetic, declared output length, and no ambient authority (no I/O, clocks, RNG,
network, or subprocesses). Consecutive `REPEAT_LAST` and a leading `REPEAT_LAST`
are rejected so expansion stays statically bounded and unambiguous.

Coverage is a **derived, checked invariant**, not stored bytes: during parse the
program is analyzed using object *lengths* (no allocation) and the coverage map
must be contiguous and cover exactly `[0, declared_source_len)`. A gap or an
overlap is a `CoverageViolation`, rejected before materialization.

## Consequences

- The decoder rejects over-expanding or malformed graphs before allocating.
- The "parser forgot a source distinction" bug class is caught at the
  representation boundary rather than after the fact.
- The opcode set grows only when a mechanism pays its own byte cost.
