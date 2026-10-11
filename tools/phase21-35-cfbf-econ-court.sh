#!/usr/bin/env bash
# Phase 21.35 — the CFBF/OLE2 compound-file economic court.
# Lane C is deliberately absent (no faithful Python-stdlib CFBF decoder): every
# question is recorded as a `comparator_absent` typed decline, never fabricated.
# Thin wrapper over the shared engine (tools/fixtures/textfmt-court.sh).
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-35-cfbf-econ-court.sh
set -uo pipefail
cd /work
export ECON_STAMP="${ECON_STAMP:-2026-10-11}"
exec bash tools/fixtures/textfmt-court.sh cfbf
