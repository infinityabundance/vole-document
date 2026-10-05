# ADR-0003: Docker-only, digest-pinned reproducibility

- **Status:** Accepted (Phase 0)
- **Date:** 2026-10-05

## Context

"Works on my machine" is unacceptable for an archival format whose claims depend
on exact bytes. Toolchain drift changes parser behavior, float formatting, and
format output.

## Decision

Never run `cargo`/`rustc`/`rustfmt`/`clippy`/tests/benches/fuzzing or PDF oracles
on the host. Use the pinned services in `compose.yaml`, built from base images
pinned by digest in `Dockerfile`:

- `dev` — `rust:1.99.0-slim-bookworm@sha256:452176c0…`
- `msrv` — `rust:1.89-slim-bookworm@sha256:d7fc7de7…`
- `tools` — `debian:bookworm-slim@sha256:3783cc01…` (qpdf/Poppler/MuPDF/Ghostscript)

Each receipt records base-image digests, `rustc`/`cargo` versions, `Cargo.lock`
SHA-256, git commit and dirty state, and CPU architecture.

## Consequences

- Restricted sandboxes may need `network_mode: host` for DNS/egress; that is a
  runtime concern, not a semantic one.
- CI must invoke the same Docker paths as local development.
