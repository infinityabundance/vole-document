#!/usr/bin/env bash
# Phase 27.4 — the legacy PowerPoint 97–2003 (`.ppt`, MS-PPT) economic court.
# Lane C (the conventional decode-to-host-values load) is deliberately absent:
# there is no faithful Python-stdlib MS-PPT decoder, so every question is
# recorded as a `comparator_absent` typed decline, never fabricated. The
# source-retaining SQLite lane keeps the raw bytes (byte authority) and declines
# every structural question typed. Thin wrapper over the shared engine
# (tools/fixtures/textfmt-court.sh).
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase27-4-ppt-econ-court.sh
set -uo pipefail
cd /work
export ECON_STAMP="${ECON_STAMP:-2026-10-12}"
exec bash tools/fixtures/textfmt-court.sh ppt
