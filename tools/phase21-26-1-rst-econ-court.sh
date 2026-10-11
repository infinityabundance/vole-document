#!/usr/bin/env bash
# Phase 21.26.1 — the reStructuredText prose ECONOMIC court.
# Thin wrapper over the shared engine (tools/fixtures/textfmt-court.sh).
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-26-1-rst-econ-court.sh
set -uo pipefail
cd /work
export ECON_STAMP="${ECON_STAMP:-2026-10-10}"
exec bash tools/fixtures/textfmt-court.sh rst
