#!/usr/bin/env bash
# Phase 21.22 — the GeoJSON (RFC 7946) ECONOMIC court.
# Thin wrapper over the shared engine (tools/fixtures/textfmt-court.sh).
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-22-geojson-court.sh
set -uo pipefail
cd /work
exec bash tools/fixtures/textfmt-court.sh geojson
