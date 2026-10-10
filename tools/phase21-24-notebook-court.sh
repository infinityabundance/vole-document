#!/usr/bin/env bash
# Phase 21.24 — the Jupyter notebook (.ipynb) ECONOMIC court.
# Thin wrapper over the shared engine (tools/fixtures/textfmt-court.sh).
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-24-notebook-court.sh
set -uo pipefail
cd /work
exec bash tools/fixtures/textfmt-court.sh notebook
