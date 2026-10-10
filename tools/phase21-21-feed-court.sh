#!/usr/bin/env bash
# Phase 21.21 — the RSS/Atom feed ECONOMIC court.
# Thin wrapper over the shared engine (tools/fixtures/textfmt-court.sh).
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-21-feed-court.sh
set -uo pipefail
cd /work
exec bash tools/fixtures/textfmt-court.sh feed
