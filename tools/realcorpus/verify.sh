#!/bin/sh
# real100-v1 verification gate. For every corpus it:
#   1. re-checks every manifest row against the bytes on disk
#      (SHA-256 + byte length + format-by-bytes);
#   2. regenerates SHA256SUMS from the committed documents;
#   3. optionally prints the diversity compliance table.
#
# Canonical invocation (never the host):
#   docker compose run --rm --no-TTY realcorpus tools/realcorpus/verify.sh
#   docker compose run --rm --no-TTY realcorpus \
#     tools/realcorpus/verify.sh --corpus real100-v1 --fetch-missing --diversity
#
# With no --corpus it verifies real100-v1 and real100-v1/pilot. A row whose
# bytes are absent counts as a failure unless --fetch-missing is given; rows
# marked redistributable=false are kept out of the committed documents tree,
# so they normally need --fetch-missing.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)

repos=""
fetch_missing=""
diversity=""
while [ $# -gt 0 ]; do
    case "$1" in
        --corpus) repos="$repos $2"; shift 2 ;;
        --fetch-missing) fetch_missing="--fetch-missing"; shift ;;
        --diversity) diversity="yes"; shift ;;
        -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
        *) echo "verify.sh: unknown argument: $1" >&2; exit 2 ;;
    esac
done
[ -n "$repos" ] || repos="real100-v1 real100-v1/pilot"

rc=0
for repo in $repos; do
    case "$repo" in
        /*) dir="$repo" ;;
        *)  dir="$root/$repo" ;;
    esac
    if [ ! -d "$dir" ]; then
        echo "skip (missing directory): $repo"
        continue
    fi
    echo "=== verify $repo ==="
    if ! python3 "$here/acquire.py" verify --corpus "$dir" $fetch_missing; then
        rc=1
    fi
    if [ -d "$dir/documents" ]; then
        ( cd "$dir" && find documents -type f -print | LC_ALL=C sort \
            | xargs -r sha256sum > SHA256SUMS )
        n=$(wc -l < "$dir/SHA256SUMS" | tr -d ' ')
        echo "wrote $repo/SHA256SUMS ($n files)"
    fi
    if [ -n "$diversity" ]; then
        echo "--- diversity ($repo) ---"
        python3 "$here/check-diversity.py" --corpus "$dir"
    fi
done
exit $rc
