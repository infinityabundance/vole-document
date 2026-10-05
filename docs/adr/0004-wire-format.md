# ADR-0004: Length-delimited records; no serde/bincode on the wire

- **Status:** Accepted (Phase 0); format itself is **provisional, not frozen**
- **Date:** 2026-10-05

## Context

A durable archive format must not depend on a serialize framework's version,
Rust struct memory layout, or field-order choices. It must be inspectable,
hashable, and independently decodable.

## Decision

The `.voldoc` container is an explicit byte format: a 64-byte header plus
length-delimited records with CRC-32C framing. `serde`/JSON is allowed only for
human-facing diagnostics (`inspect`, receipts), never as the archival layout. The
descriptor encoding is canonical: a given `Descriptor` always serializes to the
same bytes, and `serialize(parse(x)) == x` for canonical descriptors.

Records carry a `flags` byte; unknown **mandatory** tags/feature bits fail closed,
unknown explicitly-optional records are skipped (forward compatibility).

## Freeze policy

The layout is not v1. It freezes only when the exact core, bounds, integrity, and
the PDF golden path are stable and a hostile-input format court is green. Until
then, any change bumps the universe string and/or the header minor version.

## Consequences

- Independent decoders are feasible from `SPEC.md` alone.
- Adding a record type in a backward-compatible way requires marking it optional.
