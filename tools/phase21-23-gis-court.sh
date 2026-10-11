#!/usr/bin/env bash
# Phase 21.23 — the KML/GPX (GIS family) ECONOMIC court.
# Thin wrapper over the shared engine (tools/fixtures/textfmt-court.sh).
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-23-gis-court.sh
set -uo pipefail
cd /work
exec bash tools/fixtures/textfmt-court.sh gis
