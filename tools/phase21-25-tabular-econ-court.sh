#!/usr/bin/env bash
# Phase 21.25 — the PSV (pipe) + fixed-width tabular ECONOMIC court.
# Thin wrapper over the shared engine (tools/fixtures/textfmt-court.sh).
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-25-tabular-econ-court.sh
set -uo pipefail
cd /work
export ECON_STAMP="${ECON_STAMP:-2026-10-10}"
exec bash tools/fixtures/textfmt-court.sh tabular
