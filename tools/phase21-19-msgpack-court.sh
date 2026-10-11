#!/usr/bin/env bash
# Phase 21.19 — the MessagePack ECONOMIC court.
#
# Thin wrapper over the shared binary-format economic engine
# (`tools/fixtures/binfmt-court.sh`), which holds the pre-registered hypotheses
# (H1 byte-exactness, H2 contract questions, H3 cross-lane agreement, H4 economics),
# the corpus, the three lanes, the Q1–Q12 contract, the ADR-0054 estimator, and the
# receipt format. This wrapper exists only so the two courts have their own named,
# self-documenting entry point.
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-19-msgpack-court.sh

set -uo pipefail
exec bash tools/fixtures/binfmt-court.sh msgpack
