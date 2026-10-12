#!/bin/sh
# Phase 28.7 — Experiment 3: cross-revision dependency reuse (MEASUREMENT ONLY).
#
# ## Question
#
#   On authentic document *revision families*, how much **verified work** is
#   reusable across revisions under a fragment-level dependency graph, how much
#   index memory does that need, and does it beat a full rebuild and a properly
#   optimized **versioned content-addressed store**?
#
# ## This ships NO production change
#
#   The dependency graph is a side SIMULATION. Production storage is untouched:
#   no wire format, no planner change, no decoder change. The court builds each
#   revision with the real `field-build --profile runtime --packed` (so the
#   full-rebuild lane is a MEASURED quantity), verifies `materialize --exact`,
#   and derives the graph lanes (content-defined chunking, semantic edges,
#   index accounting) in Python.
#
# ## Three lanes (per family, cumulative)
#
#   L1 full    : rebuild each revision from scratch; sum measured store bytes,
#                node executions and build latency.
#   L2 cas     : fragment-level content-addressed store (content-defined
#                chunking, dedup by content id) with NO semantic dependency
#                edges -> physical bytes dedup, observations must recompute.
#   L3 graph   : L2 + semantic edges (style tables, shared-string tables) ->
#                an observation whose full closure is unchanged is REUSED.
#
# A negative (the index/maintenance cost consumes the advantage) is a valid
# result and is recorded, not hidden.
#
# ## Verify
#
#   docker compose run --rm --no-TTY doc-baseline sh tools/phase28-7-revision-reuse-court.sh
#
# Env: PROFILE (release|debug), OP_TIMEOUT (default 600).

set -u
cd /work
export LC_ALL=C PYTHONDONTWRITEBYTECODE=1

FIX=tools/fixtures/phase28-7-revision-reuse.py
RUNSTATC=tools/fixtures/phase28-3-runstat.c

PROFILE=${PROFILE:-release}
OP_TIMEOUT=${OP_TIMEOUT:-600}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-target/debug/vole-document} ;;
esac

if [ -x "$BIN" ] && [ -n "${BIN_SKIP_BUILD:-}" ]; then
    echo "-- using prebuilt $BIN (BIN_SKIP_BUILD set)" >&2
else
    echo "== phase 28.7 revision-reuse court (MEASUREMENT ONLY) ==" >&2
    echo "-- building ($PROFILE) binary" >&2
    # shellcheck disable=SC2086
    if ! cargo build $BUILD_ARGS -j 1 >&2; then
        echo "phase28-7: build FAILED ($BUILD_ARGS); refusing to measure" >&2
        exit 1
    fi
fi
[ -x "$BIN" ] || { echo "phase28-7: $BIN missing; refusing to measure" >&2; exit 1; }
command -v cc >/dev/null 2>&1 || { echo "phase28-7: need cc" >&2; exit 1; }

SHORT=$(git rev-parse --short HEAD 2>/dev/null || echo nogit)
STAMP=$(date -u +%Y-%m-%d)
CAMP=${CAMP:-evidence/campaigns/${STAMP}-phase28-7-revision-reuse-${SHORT}}
RAW=$CAMP/raw
OUT=$CAMP/out
WORK=${WORK:-evidence/scratch/phase28-7-court}
rm -rf "$CAMP" "$WORK"
mkdir -p "$RAW" "$OUT" "$RAW/measured" "$RAW/build" "$WORK" "$WORK/store"
CORPUS=$WORK/corpus
ABS=$(pwd)

RUNSTAT=/tmp/phase28-7-runstat
cc -O2 -o "$RUNSTAT" "$RUNSTATC" || { echo "runstat build failed" >&2; exit 1; }

# ---------------------------------------------------------------------------
# Metadata (a receipt records its environment, never assumes it)
# ---------------------------------------------------------------------------
{
    echo "# Phase 28.7 receipt metadata"
    echo "generated_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "git_commit: $(git rev-parse HEAD 2>/dev/null || echo nogit)"
    echo "git_branch: $(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo nogit)"
    echo "git_dirty: $(test -n "$(git --no-optional-locks status --porcelain 2>/dev/null)" && echo yes || echo no)"
    echo "root: $ABS"
    echo "rustc: $(rustc -V 2>/dev/null || echo n/a)"
    echo "cargo: $(cargo -V 2>/dev/null || echo n/a)"
    echo "python3: $(python3 --version 2>&1)"
    echo "arch: $(uname -m)"
    echo "nproc: $(nproc 2>/dev/null || echo '?')"
    echo "cargo_lock_sha256: $(sha256sum Cargo.lock | cut -d' ' -f1)"
    echo "binary: $BIN"
    echo "binary_sha256: $(sha256sum "$BIN" | cut -d' ' -f1)"
    echo "build_args: cargo build $BUILD_ARGS -j 1"
    echo "service: doc-baseline (mem_limit == memswap_limit == 6g, pids_limit 4096, cpus 8)"
    echo "substrate: field-build --profile runtime --packed"
    echo "content_id: BLAKE2b-256 (stdlib stand-in for the store's BLAKE3)"
    echo "chunking: content-defined (gear hash, min 2048 / avg 8192 / max 32768); containers use member-aligned structural fragments"
    echo "index_model: chunk entry 48 B; dependency edge 16 B; semantic ctx entry 48 B; obs identity node (optional) 48 B"
    echo "identity_separation (ADR-0060): exact revision SHA-256, physical fragment content id, semantic context signature, and derived-observation content id are DISTINCT namespaces; no reuse weakens a node's dependency on its DocumentExact root"
    echo "phase15_6_context: the existing content-addressed descriptor store retained ~0 cross-revision work (phase15-revision-court); with 97.2% of persistent bytes being the whole descriptor (phase 22.6), a change invalidates it wholesale -- fragmenting the authority is what localizes a revision"
    echo "notes: measurement only; no wire format, no planner change, no decoder change."
} > "$CAMP/METADATA.txt"
python3 - "$RAW/metadata.json" "$ABS" <<'PY'
import json, sys, subprocess, os
out, root = sys.argv[1], sys.argv[2]
def sh(c):
    try: return subprocess.run(c, shell=True, capture_output=True, text=True).stdout.strip()
    except Exception: return ""
json.dump({
  "git_commit": sh("git rev-parse HEAD"), "git_branch": sh("git rev-parse --abbrev-ref HEAD"),
  "git_dirty": bool(sh("git --no-optional-locks status --porcelain")),
  "root": root, "rustc": sh("rustc -V"), "cargo": sh("cargo -V"),
  "python3": sh("python3 --version"), "arch": sh("uname -m"),
  "cargo_lock_sha256": sh("sha256sum Cargo.lock").split(" ")[0],
  "service": "doc-baseline", "profile": os.environ.get("PROFILE", "release"),
}, open(out, "w"), indent=1, sort_keys=True)
PY

echo "== regenerating / pinning revision families ==" >&2
python3 "$FIX" families --out "$CORPUS" > "$RAW/families.tsv" 2>"$RAW/families.err" \
    || { echo "phase28-7: family generation failed" >&2; cat "$RAW/families.err" >&2; exit 1; }
cat "$RAW/families.tsv" >&2

# ---------------------------------------------------------------------------
# Measure each revision with the real binary (full-rebuild lane)
# ---------------------------------------------------------------------------
printf 'family\tfmt\tchange\trev\tbytes\tsha256\tstore_bytes\tbuild_us\tnode_count\texact_ok\n' > "$RAW/measured.tsv"
EXACT_OK=0
EXACT_N=0

tail -n +2 "$RAW/families.tsv" | while IFS="$(printf '\t')" read -r fam fmt change rev path bytes sha; do
    [ -n "$fam" ] || continue
    store="$WORK/store/$fam-$rev"
    rm -rf "$store"
    echo "== $fam r$rev ($fmt, $bytes B) ==" >&2
    "$RUNSTAT" "$BIN" field-build "$path" --store "$store" --profile runtime --packed \
        >"$RAW/build/$fam.r$rev.json" 2>"$RAW/build/$fam.r$rev.err"
    brc=$?
    rl=$(grep 'runstat wall_us' "$RAW/build/$fam.r$rev.err" | tail -1)
    build_us=$(printf '%s' "$rl" | grep -o 'wall_us=[0-9]*' | cut -d= -f2)
    [ -n "$build_us" ] || build_us=0
    store_bytes=0
    [ -d "$store" ] && store_bytes=$(find "$store" -type f -printf '%s\n' 2>/dev/null | awk '{s+=$1} END{print s+0}')
    node_count=$(grep -o '"node_count":[0-9]*' "$RAW/build/$fam.r$rev.json" | head -1 | cut -d: -f2)
    index_node_count=$(grep -o '"index_node_count":[0-9]*' "$RAW/build/$fam.r$rev.json" | head -1 | cut -d: -f2)
    field=$(grep -o '"field":"[0-9a-f]*"' "$RAW/build/$fam.r$rev.json" | head -1 | cut -d'"' -f4)

    # exactness: materialize --exact == source (length + sha256 + cmp)
    exact_ok=0
    if [ "$brc" = 0 ] && [ -n "$field" ]; then
        timeout "$OP_TIMEOUT" "$BIN" materialize --store "$store" --field "$field" \
            --exact --output "$WORK/$fam.r$rev.exact.bin" --packed \
            >/dev/null 2>"$RAW/build/$fam.r$rev.exact.err"
        if [ -f "$WORK/$fam.r$rev.exact.bin" ]; then
            ml=$(stat -c %s "$WORK/$fam.r$rev.exact.bin")
            mh=$(sha256sum "$WORK/$fam.r$rev.exact.bin" | cut -d' ' -f1)
            if [ "$ml" = "$bytes" ] && [ "$mh" = "$sha" ] && cmp -s "$WORK/$fam.r$rev.exact.bin" "$path"; then
                exact_ok=1
            fi
        fi
    fi

    python3 - "$RAW/measured/$fam.r$rev.json" "$fam" "$rev" "$store_bytes" "$build_us" \
        "$node_count" "$index_node_count" "$exact_ok" <<'PY'
import json, sys
out, fam, rev, sb, bu, nc, inc, eo = sys.argv[1:9]
json.dump({"family": fam, "rev": int(rev), "store_bytes": int(sb or 0),
           "build_us": int(bu or 0), "node_count": int(nc or 0),
           "index_node_count": int(inc or 0), "exact_ok": int(eo or 0)},
          open(out, "w"), sort_keys=True)
PY
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$fam" "$fmt" "$change" "$rev" "$bytes" "$sha" "$store_bytes" "$build_us" "$node_count" "$exact_ok" \
        >> "$RAW/measured.tsv"
    rm -rf "$store/cache"
done

# exactness gate inputs (count from the measured table)
EXACT_N=$(tail -n +2 "$RAW/measured.tsv" | wc -l | tr -d ' ')
EXACT_OK=$(tail -n +2 "$RAW/measured.tsv" | awk -F'\t' '$10==1{n++} END{print n+0}')
python3 - "$RAW/exactness.json" "$EXACT_OK" "$EXACT_N" <<'PY'
import json, sys
json.dump({"ok": int(sys.argv[2]), "n": int(sys.argv[3])}, open(sys.argv[1], "w"))
PY

echo "== building the dependency graph and the three lanes ==" >&2
python3 "$FIX" digest --families "$RAW/families.tsv" --measured "$RAW/measured" \
    --out "$RAW/digest.json" >/dev/null 2>"$RAW/digest.err" \
    || { echo "phase28-7: digest failed" >&2; cat "$RAW/digest.err" >&2; exit 1; }

python3 "$FIX" aggregate --raw "$RAW" --out "$OUT" --campaign "$CAMP" --short "$SHORT" \
    >/dev/null 2>"$RAW/aggregate.err" \
    || { echo "phase28-7: aggregate failed" >&2; cat "$RAW/aggregate.err" >&2; exit 1; }

cp "$RAW/measured.tsv" "$OUT/measured.tsv"
cp "$RAW/digest.json" "$OUT/digest.json"

python3 "$FIX" decide --receipt "$CAMP/receipt.json" > "$CAMP/DECISION.txt" 2>"$RAW/decide.err" \
    || { echo "phase28-7: decide failed" >&2; cat "$RAW/decide.err" >&2; exit 1; }
cat "$CAMP/DECISION.txt" >&2

# ---------------------------------------------------------------------------
# Gates
# ---------------------------------------------------------------------------
if [ "$EXACT_OK" != "$EXACT_N" ] || [ "$EXACT_N" -lt 12 ]; then
    echo "phase28-7: EXACTNESS GATE FAILED: $EXACT_OK/$EXACT_N materialize --exact == source" >&2
    exit 1
fi
echo "phase28-7: exactness gate OK ($EXACT_OK/$EXACT_N revisions)" >&2

OVER=$(find "$CAMP" -type f -size +10M 2>/dev/null | head -5)
if [ -n "$OVER" ]; then
    echo "phase28-7: EVIDENCE-SIZE GATE FAILED (>10 MiB):" >&2
    printf '%s\n' "$OVER" >&2
    exit 1
fi
if find "$CAMP" -name '__pycache__' -o -name '*.pyc' 2>/dev/null | grep -q .; then
    echo "phase28-7: found __pycache__/.pyc in evidence" >&2
    exit 1
fi

chown -R 1000:1000 "$CAMP" "$WORK" 2>/dev/null || true
echo "== phase 28.7 court complete: $CAMP ==" >&2
ls -la "$CAMP" >&2
