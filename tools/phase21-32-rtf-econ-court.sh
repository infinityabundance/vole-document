#!/usr/bin/env bash
# Phase 21.32 — the RTF economic court.
# Thin wrapper over the shared engine (tools/fixtures/textfmt-court.sh).
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-32-rtf-econ-court.sh
set -uo pipefail
cd /work
export ECON_STAMP="${ECON_STAMP:-2026-10-11}"
exec bash tools/fixtures/textfmt-court.sh rtf
