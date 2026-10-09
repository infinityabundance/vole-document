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
#   debian:bookworm-slim       -> oracle tooling base (Phase 3+) and, from
#                                 Phase 7.0b, the generator-family `producers` base
#                                 (same digest as `tools`)
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

# ---------------------------------------------------------------------------
# Generator-family PDF corpus (Phase 7.0b). Deliberately a SEPARATE stage so the
# fast `tools` semantic gate stays small; only the opt-in `producers` service
# builds it. Unlike qpdf/Ghostscript (transformers), these are real *authoring*
# generators with distinct DEFLATE behaviour:
#   * ReportLab   (python3-reportlab)          — Python PDF canvas; pageCompression
#   * Cairo       (python3-cairo + python3-gi) — vector PDF surface (pycairo)
#   * LibreOffice (libreoffice-writer-nogui)   — office-suite headless export
#   * pdfTeX      (texlive-latex-base + texlive-fonts-recommended)
# Plus qpdf (--deterministic-id post-normalization) and git/ca-certificates.
# Size/build tradeoff (measured on BASE_TOOLS): ~38 s cold apt install, the
# image grows to ~740 MB on disk (dev is ~1.02 GB; tools ~264 MB). Kept because
# LibreOffice and pdfTeX are precisely the *authoring-application* families the
# Phase-7.0b question needs, and because this stage never affects any other gate.
# ---------------------------------------------------------------------------
FROM ${BASE_TOOLS} AS producers
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      python3 \
      python3-reportlab \
      python3-cairo \
      python3-gi \
      libreoffice-writer-nogui \
      texlive-latex-base \
      texlive-fonts-recommended \
      qpdf \
      git \
      ca-certificates \
      coreutils \
 && rm -rf /var/lib/apt/lists/* \
 && git config --system --add safe.directory /work
WORKDIR /work

# ---------------------------------------------------------------------------
# Generic-compressor baseline ladder (Phase 7.0c) and the Phase-7.3
# partial-materialization query court. Derives from the pinned `dev` toolchain
# (same base digest, so rustc/cargo match the measurement binary) and adds the
# generic compressors the honest comparison needs: gzip, zstd, xz and brotli,
# plus jq to reduce the JSON table. For the query-cost court it also adds `pv`
# (counts the compressed bytes a sequential decoder must read and the
# decompressed bytes it must inflate to reach an offset) and GNU `time`
# (`/usr/bin/time -v` for wall/CPU seconds and peak RSS). It is deliberately NOT
# part of any fast gate; only the opt-in `baseline` service builds it.
# `tools/baselines.sh` runs the compressors and the VOLE lanes over complete
# files; `tools/partial-court.sh` runs the random-access query head-to-head.
# Phase 8 adds `strace` so `tools/seek-court.sh` can cross-check the seek
# reader's instrumented `bytes_read` against the actual descriptor-file read
# syscalls (never the loader's or a pipe's read-ahead).
# Phase 8.4 adds the honest **seekable/blocked** random-access baselines: `bgzip`
# (from the `tabix` package, BGZF block-gzip + `.gzi` index) and `pixz` (an
# indexed, parallel xz with `-x start:size` random access), plus the equivalent
# `xz --block-size=…` streams. These are what a purpose-built random-access
# format actually costs to seek; the non-blocked gzip/zstd/xz prefixes are the
# *sequential* baseline and are separately labelled as such.
# Phase 9.3 adds `borgbackup` (pinned `1.2.4-1`): a fixed-parameter buzhash
# content-defined-chunk dedup store, the honest cross-document baseline for the
# content-addressed object store. `tools/chunk-dedup.sh` runs it with frozen
# `--chunker-params 19,23,21,4095 --compression none` and reports the unique
# stored bytes (dedup isolated from chunk compression).
# ---------------------------------------------------------------------------
FROM dev AS baseline
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      gzip \
      zstd \
      xz-utils \
      brotli \
      jq \
      coreutils \
      pv \
      time \
      strace \
      tabix \
      pixz \
      borgbackup=1.2.4-1 \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /work

# ---------------------------------------------------------------------------
# Phase-11.9 fair-baseline court. Derives from `baseline` (same pinned base
# digest as `dev`/`baseline`, so rustc/cargo match the measurement binary) and
# adds the *preprocessed* conventional baseline's tools: `sqlite3` (the one-time
# per-page extraction into an indexed table) plus the raw PDF tooling the A0
# lane needs (`poppler-utils` for `pdftotext`/`pdfinfo`, `qpdf` for
# `--linearize`). One image therefore runs the whole `tools/field-court.sh`
# end-to-end (VOLE CLI + strace + compressors + PDF oracles + sqlite), which
# keeps every number on one base digest.
# ---------------------------------------------------------------------------
FROM baseline AS db-baseline
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      sqlite3 \
      poppler-utils \
      qpdf \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /work

# ---------------------------------------------------------------------------
# Phase-12.11 multi-format lifetime court. Derives from `db-baseline` (the same
# pinned base digest, so rustc/cargo/sqlite3/Poppler match every other
# measurement lane) and adds only `python3` (Debian bookworm = Python 3.11.2)
# so the DOCX/EPUB A0/A1 lanes can extract text/structure with the **stdlib**
# `zipfile` + `xml.etree.ElementTree` (no third-party parser, no floating
# dependency). The task's <8-document mixed corpus and the four-universe
# footprint are measured here; nothing runs on the host.
# ---------------------------------------------------------------------------
FROM db-baseline AS doc-baseline
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      python3 \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /work

# ---------------------------------------------------------------------------
# Phase-21.1.3 analytical comparator. Derives from `doc-baseline` (same pinned
# base digest and python3) and adds a **hash-pinned** DuckDB runtime — the
# analytical baseline the Phase-21 plan *mandates* alongside SQLite for the
# tabular formats (CSV/TSV, XLSX, ODS). Those engines already embody columnar
# projection, predicate pushdown, compressed pages, and metadata indexes, so
# winning only against SQLite there could just mean the wrong competitor was
# chosen (the Phase-22 "maximize the competitor first" rule). DuckDB reads and
# writes Parquet natively, so no `pyarrow` dependency is required.
#
# Hash-pinned cp311 wheels (bookworm ships Python 3.11.2; duckdb 1.5.6):
#   x86_64  73b108c04c932b36c2fa4e41110cc1c3c8cd510eb49f065f92d050be8e6929fd
#   aarch64 56c0f71c6bee982e9c30568bb12371bf66b26bf129c75d8d7f60bc69d6590a2c
#
# Phase 21.5.3 correction (FIX 1): the JSON economic court's SQLite lane must use a
# *modern* SQLite that actually has the `jsonb` binary representation (JSONB exists
# only from SQLite 3.45.0, 2024-04-15). The bookworm system SQLite is 3.40.1
# (2022-12-28), so the original receipt's "uses json1/jsonb" wording was false. The
# `pysqlite3-binary` wheel bundles a recent SQLite in a single hash-verified module:
# the installed 0.5.4.post2 cp311 wheel reports `sqlite_version` **3.51.1**, so the
# baseline can honestly use `jsonb(...)` and `json_tree`/`json_each`. Pinned by
# SHA-256 (cp311 manylinux2014_x86_64 wheel:
#   3060a56666ede382c9af3e4b086e30c9ffb65133b3fa606c2d1b9fbff512f241).
# NOTE: upstream publishes ONLY x86_64 wheels for pysqlite3-binary (verified from
# the PyPI JSON index: every release is `*_x86_64.whl`; there is no aarch64 wheel),
# so this pin is x86_64-only. The court records the module name + `sqlite_version`
# + wheel SHA-256 in its receipt, and `analytical` is the pinned x86_64 lane.
# ---------------------------------------------------------------------------
FROM doc-baseline AS analytical
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      python3-pip \
 && rm -rf /var/lib/apt/lists/*
RUN printf 'duckdb==1.5.6 \
  --hash=sha256:73b108c04c932b36c2fa4e41110cc1c3c8cd510eb49f065f92d050be8e6929fd \
  --hash=sha256:56c0f71c6bee982e9c30568bb12371bf66b26bf129c75d8d7f60bc69d6590a2c\n' \
      > /tmp/duckdb-requirements.txt \
 && pip3 install --break-system-packages --no-cache-dir --no-deps --require-hashes -r /tmp/duckdb-requirements.txt \
 && rm -f /tmp/duckdb-requirements.txt
RUN printf 'pysqlite3-binary==0.5.4.post2 \
  --hash=sha256:3060a56666ede382c9af3e4b086e30c9ffb65133b3fa606c2d1b9fbff512f241\n' \
      > /tmp/pysqlite3-requirements.txt \
 && pip3 install --break-system-packages --no-cache-dir --no-deps --require-hashes -r /tmp/pysqlite3-requirements.txt \
 && rm -f /tmp/pysqlite3-requirements.txt
WORKDIR /work

# ---------------------------------------------------------------------------
# real100-v1 corpus-acquisition harness. A pinned, network-capable stage on the
# same `debian:bookworm-slim` digest as `tools` (never the host). It carries:
#   * `curl` + `ca-certificates` — HTTPS download with redirects and retries;
#   * `python3` (stdlib only) — provenance, hashing, OPC (ZIP) inspection;
#   * `poppler-utils` + `qpdf` — the *structural probe* (page count, extracted
#     text length, image/font inventory, object streams, xref streams) used to
#     assign grounded `structural_tags`. These are read-only inspectors of the
#     downloaded bytes; they never validate or transform a document, and no
#     codec result is consulted.
# The image stays small and, like every lane, is hard-capped in compose.yaml.
# ---------------------------------------------------------------------------
FROM ${BASE_TOOLS} AS realcorpus
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      curl \
      ca-certificates \
      python3 \
      coreutils \
      poppler-utils \
      qpdf \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /work

# ---------------------------------------------------------------------------
# Phase-11.13 LLM working-set *token* court (priority #8). Derives from the
# pinned `dev` toolchain (same base digest as `dev`/`baseline`, so rustc/cargo
# match the measurement binary) and adds the two things the token court needs
# beyond a plain dev image:
#   * Poppler (`pdftotext`, the B0/B1 oracle) and `jq` (the receipt assembler);
#   * a *pinned* real tokenizer: the HuggingFace `tokenizers` runtime at an
#     exact version, installed from a hash-pinned wheel, plus the vendored
#     `bert-base-uncased` `tokenizer.json` committed in the repository
#     (tools/tokenizers/, SHA-256 recorded in PROVENANCE.txt and re-verified at
#     court time). The court never touches the network: the model asset is on
#     disk, so the token count is offline-deterministic.
#
# Hash-pinned cp311 wheels (bookworm ships Python 3.11.2):
#   x86_64  453c7769d22231960ee0e883d1005c93c68015025a5e4ae56275406d94a3c907
#   aarch64 ef820880d5e4e8484e2fa54ff8d297bb32519eaa7815694dc835ace9130a3eea
# Both are the manylinux2014 wheels for tokenizers 0.20.3. `--no-deps`:
# `import tokenizers` + `Tokenizer.from_file` do not need `huggingface-hub`
# (that is only pulled in by `from_pretrained`), so the runtime is exactly one
# hash-verified wheel.
# ---------------------------------------------------------------------------
FROM dev AS llm-workingset
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      poppler-utils \
      jq \
      python3 \
      python3-pip \
 && rm -rf /var/lib/apt/lists/*
RUN printf 'tokenizers==0.20.3 \
  --hash=sha256:453c7769d22231960ee0e883d1005c93c68015025a5e4ae56275406d94a3c907 \
  --hash=sha256:ef820880d5e4e8484e2fa54ff8d297bb32519eaa7815694dc835ace9130a3eea\n' \
      > /tmp/tokenizers-requirements.txt \
 && pip3 install --break-system-packages --no-cache-dir --no-deps --require-hashes -r /tmp/tokenizers-requirements.txt \
 && rm -f /tmp/tokenizers-requirements.txt
WORKDIR /work
