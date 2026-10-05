# ADR-0002: One crate, module separation, no micro-workspace

- **Status:** Accepted (Phase 0)
- **Date:** 2026-10-05

## Context

The architecture spans a container, a reconstruction algebra, an entropy layer,
adapters, and a store. A cargo workspace of many crates would multiply version
and feature-management overhead before any of the layers are proven.

## Decision

Build a single native Rust package `vole-document` with modules for architectural
separation (`container`, `dra`, `encode`, `materialize`, `adapter`, …). The crate
uses `#![forbid(unsafe_code)]`. Optional backends are cargo **features**, but no
feature combination may silently change `.voldoc` semantics: the encoded file
declares its own mandatory semantic features.

## Consequences

- Simple builds, one `Cargo.lock`, one MSRV.
- Architectural boundaries are enforced by module privacy and review, not by the
  crate graph; the subagent protocol supplies independent scrutiny.
