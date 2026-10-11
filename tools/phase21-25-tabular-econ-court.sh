#!/usr/bin/env bash
# Phase 21.25 — the PSV (pipe) + fixed-width tabular ECONOMIC court.
# Thin wrapper over the shared engine (tools/fixtures/textfmt-court.sh).
#
# This is the tabular format, so the shared engine adds the mandatory
# DuckDB/Parquet analytical comparator alongside the two conventional lanes
# (source-retaining SQLite + a conventional load), per ADR-0059. It therefore
# runs in the `analytical` service (same pinned base as `doc-baseline` plus the
# hash-pinned DuckDB wheel); see tools/phase21-7-csv-court.sh for the precedent.
#
#   docker compose run --rm --no-TTY analytical bash tools/phase21-25-tabular-econ-court.sh
set -uo pipefail
cd /work
export ECON_STAMP="${ECON_STAMP:-2026-10-10}"
exec bash tools/fixtures/textfmt-court.sh tabular
