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

ARG BASE_STABLE=rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e
ARG BASE_MSRV=rust:1.89-slim-bookworm@sha256:d7fc7de78bb8c1469933aeecbf801314d30d7d6e9f0578bba4cfa285bfa37fe6
ARG BASE_TOOLS=debian:bookworm-slim@sha256:3783cc01769c7b2b1b83a5c5ad96c815348e28ed7da68e2e3687004faa906251

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
