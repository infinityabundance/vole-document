#!/usr/bin/env bash
# Phase 18.4 supporting mechanism probe — WHY `field-build --packed` does not
# collapse the contract court's ingest-write term on the bind-mounted /work.
#
#   * fs backend: `PackedSeedStore` into `seed/` — one file per node, one `fsync`.
#   * packed backend: `PackedSeedStore` into `fieldpack/` — one segment, but
#     `insert` calls `f.sync_data()` per node (`src/store/pack.rs`), i.e. one
#     `fdatasync` per seed node.
#
# So the two backends issue ~the same number of durability syncs; the packed
# store only removes the per-node file CREATE (mk_dir/openat/write). On the slow
# bind mount the sync latency dominates, so packed ~= fs. In /tmp (fast sync)
# both collapse to ~1.3 s, showing the term is the sync latency.
#
# Never on the host. Usage:
#   docker compose run --rm --no-TTY doc-baseline \
#     bash tools/phase18-contract-packed-mechanism.sh [OUTFILE]
set -uo pipefail
cd /work
BIN=${BIN:-target/release/vole-document}
F=${F:-real100-v1/documents/nist/pdf/nist-pdf-0017.pdf}
OUT=${1:-/dev/stdout}
W=${WORK:-evidence/scratch/phase18-contract-packed-mechanism}
rm -rf "$W"; mkdir -p "$W"

now_ms() { echo $(( ${EPOCHREALTIME/./} / 1000 )); }

{
    echo "# Phase 18.4 packed-store ingest-write mechanism probe"
    echo "# bin: $BIN ; source: $F ($(stat -c %s "$F") bytes)"
    echo "# date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo
    for loc in work tmp; do
        for mode in fs packed; do
            if [ "$loc" = work ]; then st="$W/$mode"; else st="/tmp/p18-4-$mode"; fi
            extra=""; [ "$mode" = packed ] && extra="--packed"
            rm -rf "$st"
            t0=$(now_ms)
            strace -f -c -o "$W/$loc.$mode.strace" "$BIN" field-build "$F" \
                --store "$st" --profile runtime $extra >/dev/null 2>/dev/null
            rc=$?
            t1=$(now_ms)
            files=$(find "$st" -type f 2>/dev/null | wc -l | tr -d ' ')
            echo "## $loc/$mode rc=$rc wall_ms=$(( t1 - t0 )) files=$files"
            grep -E "fdatasync|fsync|mkdir|openat|write|total" "$W/$loc.$mode.strace" || true
            echo
        done
    done
} >"$OUT" 2>&1

rm -rf "$W"
cat "$OUT"
