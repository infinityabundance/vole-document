#!/usr/bin/env bash
# Phase 21.20 — the config-family (INI / .env / Java properties) ECONOMIC court.
# Thin wrapper over the shared engine (tools/fixtures/textfmt-court.sh).
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-20-config-court.sh
set -uo pipefail
cd /work
exec bash tools/fixtures/textfmt-court.sh config
