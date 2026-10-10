#!/bin/sh
# Phase 21.5.3 (FIX 4) — acquire the pinned real-world multi-format samples.
#
# Downloads every row in tools/realcorpus/formats-manifest.tsv into a gitignored
# cache (`realformats-v1/documents/<format>/<id>`) and verifies the recorded byte
# length AND SHA-256 against the manifest. The network is never trusted: a
# mismatch fails the run. Runs in the pinned, network-capable, capped `realcorpus`
# service; never on the host:
#
#   docker compose run --rm --no-TTY realcorpus sh tools/realcorpus/fetch-formats.sh [--check]
#
#   --check   verify already-present bytes only; do not download missing ones.
#
# Exit codes: 0 all requested rows present + verified; 1 a hash/length mismatch or
# a failed download; 2 usage/manifest error.

set -u
cd /work
MANIFEST=${MANIFEST:-tools/realcorpus/formats-manifest.tsv}
CORPUS=${CORPUS:-realformats-v1}
CHECK=0
for a in "$@"; do
    case "$a" in
        --check) CHECK=1 ;;
        *) echo "fetch-formats: unknown arg $a" >&2; exit 2 ;;
    esac
done
[ -f "$MANIFEST" ] || { echo "fetch-formats: manifest not found: $MANIFEST" >&2; exit 2; }

mkdir -p "$CORPUS/documents"
RC=0
T0=$(date +%s%N)
N=0; OK=0; MISS=0; BAD=0
# Skip the header; iterate tab-separated rows.
tail -n +2 "$MANIFEST" | while IFS="$(printf '\t')" read -r id format bytes sha url source license redistributable notes complexity; do
    [ -n "$id" ] || continue
    N=$((N + 1))
    dir="$CORPUS/documents/$format"
    dest="$dir/$id"
    mkdir -p "$dir"
    need=1
    if [ -f "$dest" ]; then
        gb=$(wc -c < "$dest" | tr -d ' ')
        gs=$(sha256sum "$dest" | cut -d' ' -f1)
        if [ "$gb" = "$bytes" ] && [ "$gs" = "$sha" ]; then
            need=0
        fi
    fi
    if [ "$need" -eq 1 ]; then
        if [ "$CHECK" -eq 1 ]; then
            echo "MISSING $id ($format) — run without --check to fetch" >&2
            continue
        fi
        tmp="$dest.tmp.$$"
        if ! curl -sSL --fail --max-time 120 -o "$tmp" "$url"; then
            echo "FAIL download $id <- $url" >&2
            rm -f "$tmp"
            continue
        fi
        gb=$(wc -c < "$tmp" | tr -d ' ')
        gs=$(sha256sum "$tmp" | cut -d' ' -f1)
        if [ "$gb" != "$bytes" ] || [ "$gs" != "$sha" ]; then
            echo "FAIL verify $id: got len=$gb sha=$gs expected len=$bytes sha=$sha" >&2
            rm -f "$tmp"
            continue
        fi
        mv "$tmp" "$dest"
    fi
    echo "OK $id $format $bytes $sha"
done
T1=$(date +%s%N)
echo "fetch-formats: manifest rows processed (see per-row OK/FAIL above); elapsed_ms=$(( (T1 - T0) / 1000000 ))"
# A second pass re-verifies what ended up on disk; a missing/mismatched row is fatal.
tail -n +2 "$MANIFEST" | while IFS="$(printf '\t')" read -r id format bytes sha url source license redistributable notes complexity; do
    [ -n "$id" ] || continue
    dest="$CORPUS/documents/$format/$id"
    if [ ! -f "$dest" ]; then
        echo "STILL-MISSING $id"; continue
    fi
    gb=$(wc -c < "$dest" | tr -d ' ')
    gs=$(sha256sum "$dest" | cut -d' ' -f1)
    if [ "$gb" != "$bytes" ] || [ "$gs" != "$sha" ]; then
        echo "STILL-BAD $id"; continue
    fi
done > /tmp/fetch-formats-final.txt
if grep -q . /tmp/fetch-formats-final.txt; then
    cat /tmp/fetch-formats-final.txt >&2
    echo "fetch-formats: some rows are missing or mismatched (see above)" >&2
    RC=1
else
    echo "fetch-formats: all manifest rows present and verified"
fi
exit $RC
