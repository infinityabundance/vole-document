#!/usr/bin/env bash
# Phase 21.34 — the scientific-container (NetCDF classic / HDF5-subset) economic
# court. Lane C decodes NetCDF classic only; every HDF5-fixture question is a
# recorded `comparator_absent` decline (no faithful Python-stdlib HDF5 decoder).
# Thin wrapper over the shared engine (tools/fixtures/textfmt-court.sh).
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-34-hdf-econ-court.sh
set -uo pipefail
cd /work
export ECON_STAMP="${ECON_STAMP:-2026-10-11}"
exec bash tools/fixtures/textfmt-court.sh hdf
