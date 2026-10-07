#!/bin/sh
# real100-v1 acquisition driver. Reads the frozen selection
# (real100-v1/sources/real100_selection.tsv, produced by select-real100.py) and
# acquires each document. Idempotent: any id already present in the manifest is
# skipped, so the run can be resumed after a context loss. A source that is
# unreachable or fails a byte check is reported and skipped (never substituted,
# never faked).
#
#   docker compose run --rm --no-TTY realcorpus sh tools/realcorpus/select-real100.sh
#
# Parsing lives in acquire.py (a tab is IFS-whitespace, so POSIX `read` would
# collapse the empty optional columns).
set -u
here=$(cd "$(dirname "$0")" && pwd)
CORPUS=${CORPUS:-/work/real100-v1}
SEL=${1:-$CORPUS/sources/real100_selection.tsv}
[ -f "$SEL" ] || { echo "selection not found: $SEL" >&2; exit 2; }
exec python3 "$here/acquire.py" add-tsv --corpus "$CORPUS" --selection "$SEL"
