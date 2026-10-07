#!/usr/bin/env bash
# phase15 DEFLATE-ablation court (Phase 15.5).
#
# Runs `examples/deflate_ablation.rs` — ONE candidate per process, so the
# harness's own `peak_rss_kib` (and an independent `/usr/bin/time -v`) are clean —
# over the REAL compressed members of the frozen `real100-v1` corpus (PDF
# FlateDecode zlib streams + ZIP method-8 DEFLATE members).
#
# Candidates:
#   miniz       — miniz_oxide 0.8.9, scalar adler (built without `miniz-simd`)
#   miniz-simd  — miniz_oxide 0.8.9 with the `simd-adler32` path (separate build)
#   zlib-rs     — zlib_rs 0.5.5, safe Rust `Inflate` API only
#   zune-inflate— zune_inflate 0.2.54
#
# Correctness gate: every candidate's decoded bytes must equal the miniz_oxide
# reference byte-for-byte over the whole corpus (the harness reports `mismatches`).
# Adoption bar (pre-registered, from the 15.5 design): a non-miniz candidate is
# adopted only at >= 1.25x GB/s AND peak RSS <= 1.10x. This court only MEASURES;
# it does not switch the shipped inflater.
#
# Run in the pinned `doc-baseline` service (or `dev`, which is the same base):
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase15-deflate-court.sh
# Overrides: LIMIT=<n> (0 = all members), REPS=<n>.
set -uo pipefail

cd /work
export LC_ALL=C

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=evidence/campaigns/${STAMP}-phase15-deflate-${SHA}
RAW=$CAMPAIGN/raw
mkdir -p "$RAW"

CORPUS=${CORPUS:-real100-v1/documents}
LIMIT=${LIMIT:-0}
REPS=${REPS:-5}
LIMIT_ARG=""
[ "$LIMIT" -gt 0 ] && LIMIT_ARG="--limit $LIMIT"

echo "== phase15 deflate-ablation court ==" >&2

echo "-- building (scalar miniz + zlib-rs + zune-inflate)" >&2
if ! cargo build --release --locked --features deflate-ablation --example deflate_ablation >&2; then
    echo "phase15-deflate: scalar build FAILED; refusing to measure" >&2
    exit 1
fi
SCALAR=target/release/examples/deflate_ablation
[ -x "$SCALAR" ] || { echo "phase15-deflate: $SCALAR missing" >&2; exit 1; }

# candidate tag -> binary; CAND defaults to TAG but the SIMD binary still takes
# `--candidate miniz` (it reports config=simd).
run_one() { # tag bin [cand]
    local tag=$1 bin=$2 cand=${3:-$1} out="$RAW/$1.json"
    echo "   -- $tag (candidate=$cand)" >&2
    # shellcheck disable=SC2086
    /usr/bin/time -v "$bin" --candidate "$cand" --corpus "$CORPUS" $LIMIT_ARG \
        --reps "$REPS" --out "$out" >"$RAW/$tag.stdout" 2>"$RAW/$tag.time"
}

run_one miniz         "$SCALAR"
run_one zlib-rs       "$SCALAR"
run_one zune-inflate  "$SCALAR"

# The SIMD-adler build is a SEPARATE target dir: cargo caches by feature set, and a
# shared target directory was observed to hand back the scalar example unchanged
# for the simd feature set (config stayed `scalar`), collapsing the two arms.
echo "-- building (miniz SIMD adler, separate target dir)" >&2
if ! CARGO_TARGET_DIR=/tmp/vole-deflate-simd cargo build --release --locked \
        --features deflate-ablation,miniz-simd --example deflate_ablation >&2; then
    echo "phase15-deflate: simd build FAILED; refusing to measure" >&2
    exit 1
fi
SIMD=/tmp/vole-deflate-simd/release/examples/deflate_ablation
[ -x "$SIMD" ] || { echo "phase15-deflate: $SIMD missing" >&2; exit 1; }
run_one miniz-simd    "$SIMD" miniz

printf '{\n' >"$RAW/environment.json"
printf '  "campaign": "%s",\n' "$CAMPAIGN" >>"$RAW/environment.json"
printf '  "phase": "15.5 - DEFLATE decompression ablation over real100-v1 members",\n' >>"$RAW/environment.json"
printf '  "utc": "%s",\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >>"$RAW/environment.json"
printf '  "git_commit": "%s",\n' "$(git rev-parse HEAD 2>/dev/null || echo unknown)" >>"$RAW/environment.json"
printf '  "git_dirty": "%s",\n' "$(git status --porcelain 2>/dev/null | tr '\n' ';' | sed 's/"/\\"/g')" >>"$RAW/environment.json"
printf '  "arch": "%s",\n' "$(uname -m)" >>"$RAW/environment.json"
printf '  "service": "doc-baseline",\n' >>"$RAW/environment.json"
printf '  "rustc": "%s",\n' "$(rustc --version 2>/dev/null | sed 's/"/\\"/g')" >>"$RAW/environment.json"
printf '  "cargo": "%s",\n' "$(cargo --version 2>/dev/null | sed 's/"/\\"/g')" >>"$RAW/environment.json"
printf '  "cargo_lock_sha256": "%s",\n' "$(sha256sum Cargo.lock | cut -d" " -f1)" >>"$RAW/environment.json"
printf '  "manifest_sha256": "%s",\n' "$(sha256sum real100-v1/manifest.tsv | cut -d" " -f1)" >>"$RAW/environment.json"
printf '  "limit": %s, "reps": %s,\n' "$LIMIT" "$REPS" >>"$RAW/environment.json"
printf '  "env_affecting_semantics": {"LC_ALL": "C"}\n' >>"$RAW/environment.json"
printf '}\n' >>"$RAW/environment.json"

echo "-- aggregating" >&2
python3 tools/fixtures/phase15-deflate.py "$RAW" "$CAMPAIGN" | tee "$RAW/summary.txt"

echo "phase15 deflate-ablation court: done — $CAMPAIGN"
