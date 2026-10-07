#!/bin/sh
# real100-v1 acquisition entry point. A thin wrapper over `acquire.py add`:
#
#   tools/realcorpus/fetch.sh --corpus real100-v1 \
#     --id nasa-pdf-tm-0001 --agency nasa --format pdf \
#     --url https://ntrs.nasa.gov/api/citations/<id>/downloads/<file>.pdf \
#     --landing-page https://ntrs.nasa.gov/citations/<id> \
#     --title "..." --publication-id "NASA/TM-..." --year 1998 \
#     --document-family "..." --document-type TM \
#     --structural-tags "table-heavy;figure-heavy" \
#     --cross-format-family-id cf-nasa-0001 \
#     --rights-status "US-Gov-Public-Domain" --redistributable true
#
# Canonical invocation (never the host):
#   docker compose run --rm --no-TTY realcorpus \
#     tools/realcorpus/fetch.sh --corpus real100-v1 ...
#
# Run `fetch.sh --help` for the full flag list (it forwards to acquire.py).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
exec python3 "$here/acquire.py" add "$@"
