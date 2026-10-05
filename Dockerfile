# syntax=docker/dockerfile:1
#
# Pinned, reproducible toolchains for VOLE-Document. Every cargo/rustc/rustfmt/
# clippy/test/bench/fuzz and every PDF oracle runs inside one of these stages.
# Base images are pinned by digest. Source code is NOT baked into these images;
# it is bind-mounted by compose.yaml so a single image serves every check and
# the toolchain identity stays stable across evidence receipts.
#
# Digest provenance (recorded for receipts):
#   rust:1.99.0-slim-bookworm  -> rustc 1.99.0 (b940084d7 2026-09-28), cargo 1.99.0
#   rust:1.89-slim-bookworm    -> rustc 1.89.0 (29483883e 2025-08-04)
#   debian:bookworm-slim       -> oracle tooling base (Phase 3+)
#   rustlang/rust:nightly-bookworm-slim-2026-10-04
#                              -> rustc 1.101.0-nightly (db8f076d2 2026-10-03),
#                                 cargo 1.101.0-nightly (f3865b2a4 2026-09-29),
#                                 toolchain nightly-2026-10-04 (Phase 7 fuzz).

ARG BASE_STABLE=rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e
ARG BASE_MSRV=rust:1.89-slim-bookworm@sha256:d7fc7de78bb8c1469933aeecbf801314d30d7d6e9f0578bba4cfa285bfa37fe6
ARG BASE_TOOLS=debian:bookworm-slim@sha256:3783cc01769c7b2b1b83a5c5ad96c815348e28ed7da68e2e3687004faa906251
ARG BASE_FUZZ=rustlang/rust:nightly-bookworm-slim-2026-10-04@sha256:58f725c9459a637c19ddf24542d2aee7d93704684c9780b168e81504327ad297

# ---------------------------------------------------------------------------
# Primary sealed toolchain (Rust 1.99.0). The reference development gate.
# ---------------------------------------------------------------------------
FROM ${BASE_STABLE} AS dev
ENV CARGO_TERM_COLOR=never \
    CARGO_INCREMENTAL=0 \
    RUST_BACKTRACE=1
RUN apt-get update \
 && apt-get install -y --no-install-recommends git ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && git config --system --add safe.directory /work \
 && rustup component add rustfmt clippy
WORKDIR /work

# ---------------------------------------------------------------------------
# MSRV gate (Rust 1.89.0, the floor dictated by preflate-rs 0.7.6).
# RUSTUP_TOOLCHAIN overrides rust-toolchain.toml so the 1.99 request is ignored.
# ---------------------------------------------------------------------------
FROM ${BASE_MSRV} AS msrv
ENV CARGO_TERM_COLOR=never \
    CARGO_INCREMENTAL=0 \
    RUST_BACKTRACE=1 \
    RUSTUP_TOOLCHAIN=1.89.0
RUN apt-get update \
 && apt-get install -y --no-install-recommends git ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && git config --system --add safe.directory /work \
 && rustup component add rustfmt clippy
WORKDIR /work

# ---------------------------------------------------------------------------
# Dependency-policy gate: cargo-audit + cargo-deny, built inside the pinned
# toolchain so they match the compiler and run in Docker (never on the host).
# ---------------------------------------------------------------------------
FROM ${BASE_STABLE} AS policy
ENV CARGO_TERM_COLOR=never \
    CARGO_INCREMENTAL=0
RUN apt-get update \
 && apt-get install -y --no-install-recommends git ca-certificates pkg-config libssl-dev \
 && rm -rf /var/lib/apt/lists/* \
 && git config --system --add safe.directory /work
RUN cargo install cargo-audit cargo-deny --locked
WORKDIR /work

# ---------------------------------------------------------------------------
# Coverage-guided fuzzing (Phase 7). A pinned *dated* nightly (never a floating
# `nightly`) plus `cargo-fuzz`, so libFuzzer runs are reproducible. RUSTUP_TOOLCHAIN
# overrides rust-toolchain.toml (which requests stable 1.99.0) so the fuzz crate
# builds under the nightly that cargo-fuzz/libfuzzer require.
# ---------------------------------------------------------------------------
FROM ${BASE_FUZZ} AS fuzz
ENV CARGO_TERM_COLOR=never \
    CARGO_INCREMENTAL=0 \
    RUST_BACKTRACE=1 \
    RUSTUP_TOOLCHAIN=nightly-2026-10-04
RUN apt-get update \
 && apt-get install -y --no-install-recommends git ca-certificates build-essential \
 && rm -rf /var/lib/apt/lists/* \
 && git config --system --add safe.directory /work \
 && cargo install cargo-fuzz --locked --version 0.13.2
WORKDIR /work

# ---------------------------------------------------------------------------
# PDF / semantic oracle court (Phase 3+). Independent validators, never the
# representation authority: qpdf, Poppler, MuPDF, Ghostscript.
# ---------------------------------------------------------------------------
FROM ${BASE_TOOLS} AS tools
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      qpdf \
      poppler-utils \
      mupdf-tools \
      ghostscript \
      coreutils \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /work
