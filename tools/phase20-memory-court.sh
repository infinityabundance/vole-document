#!/usr/bin/env bash
# Phase 20.2 — large-source memory architecture for the direct build.
#
# ## Why this court exists
#
# Phase 19.3 measured the direct build (`field-build --profile runtime --packed
# --sync=batch`) over the full frozen `real100-v1` and found the next binding
# constraint is MEMORY, not wall: `nasa-pdf-0001` (a 408,854,600 B source) peaked
# at 2342 MiB RSS (~5.7x source) against the 6 GiB lane cap. A source of ~1 GiB
# would therefore not fit. This court records an RSS-vs-source curve, and the
# per-stage bisect that attributes the peak, before and after this phase's
# change, and proves the change is output-preserving (descriptor SHA-256 equal).
#
# ## What it does, per source
#
#   * `field-build SRC --store DIR --profile runtime --packed --sync=batch` under
#     `/usr/bin/time -v` (wall + peak RSS) via `timeout`;
#   * the descriptor SHA-256 (`.descriptor_sha256`) the build reports;
#   * `materialize --exact` to a file, then length + SHA-256 + `cmp` against the
#     source (the phase prime directive: all three must hold);
#   * RSS/source ratio.
#
# The synthetic >=1 GiB source is deterministic (a repeated fixed 1 MiB block, so
# the bytes are reproducible) and opaque (no PDF/ZIP magic), so it exercises the
# large-source RAW floor and the scan-decline path without any format claim.
#
# ## Lane
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase20-memory-court.sh
#   docker compose run --rm --no-TTY doc-baseline sh -c 'SYNTH_MB=0 bash tools/phase20-memory-court.sh'
#   docker compose run --rm --no-TTY doc-baseline sh -c 'SYNTH_MB=64 bash tools/phase20-memory-court.sh'
set -uo pipefail

cd /work
export LC_ALL=C

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=${CAMPAIGN:-evidence/campaigns/${STAMP}-phase20-memory-${SHA}}
RAW=$CAMPAIGN/raw
WORK=${WORK:-evidence/scratch/phase20-memory/work}
mkdir -p "$RAW" "$WORK"

OP_TIMEOUT=${OP_TIMEOUT:-900}
SYNTH_MB=${SYNTH_MB:-1024}          # >=1 GiB deterministic synthetic source; 0 = skip
SYNTH_TAG=synth-opaque-1024m

PROFILE=${PROFILE:-release}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-target/debug/vole-document} ;;
esac

echo "== phase20.2 large-source memory court ==" >&2
echo "-- building ($PROFILE) binary" >&2
# shellcheck disable=SC2086
if ! cargo build $BUILD_ARGS >&2; then
    echo "phase20-memory: build FAILED ($BUILD_ARGS); refusing to measure" >&2
    exit 1
fi
[ -x "$BIN" ] || { echo "phase20-memory: $BIN missing after build" >&2; exit 1; }

# The frozen size span: the real100-v1 documents that bracket 1 MiB .. 409 MiB,
# plus small DOCX/EPUB members so the package ingest path is covered too.
DOCS_TSV=$RAW/docs.tsv
if [ -n "${DOCS:-}" ] && [ -f "${DOCS:-}" ]; then
    cp "$DOCS" "$DOCS_TSV"
else
    cat >"$DOCS_TSV" <<'EOF'
nist-docx-0013	real100-v1/documents/nist/docx/nist-docx-0013.docx
nist-docx-0005	real100-v1/documents/nist/docx/nist-docx-0005.docx
nist-epub-0007	real100-v1/documents/nist/epub/nist-epub-0007.epub
nist-epub-0005	real100-v1/documents/nist/epub/nist-epub-0005.epub
nist-pdf-0003	real100-v1/documents/nist/pdf/nist-pdf-0003.pdf
nist-pdf-0017	real100-v1/documents/nist/pdf/nist-pdf-0017.pdf
nasa-pdf-0010	real100-v1/documents/nasa/pdf/nasa-pdf-0010.pdf
nasa-pdf-0023	real100-v1/documents/nasa/pdf/nasa-pdf-0023.pdf
nasa-pdf-0022	real100-v1/documents/nasa/pdf/nasa-pdf-0022.pdf
nasa-epub-0006	real100-v1/documents/nasa/epub/nasa-epub-0006.epub
nasa-pdf-0012	real100-v1/documents/nasa/pdf/nasa-pdf-0012.pdf
nasa-pdf-0024	real100-v1/documents/nasa/pdf/nasa-pdf-0024.pdf
nasa-pdf-0003	real100-v1/documents/nasa/pdf/nasa-pdf-0003.pdf
nasa-pdf-0002	real100-v1/documents/nasa/pdf/nasa-pdf-0002.pdf
nasa-pdf-0001	real100-v1/documents/nasa/pdf/nasa-pdf-0001.pdf
EOF
fi

# Deterministic synthetic source (repeated fixed 1 MiB pseudo-random block).
if [ "$SYNTH_MB" -gt 0 ]; then
    SYNTH=$WORK/$SYNTH_TAG.bin
    if [ ! -f "$SYNTH" ] || [ "$(stat -c %s "$SYNTH")" != "$(( SYNTH_MB * 1024 * 1024 ))" ]; then
        echo "-- generating ${SYNTH_MB} MiB deterministic synthetic source" >&2
        python3 - "$SYNTH" "$SYNTH_MB" <<'PY'
import sys
path, mb = sys.argv[1], int(sys.argv[2])
# xorshift64 block, deterministic and format-neutral (no %PDF / PK magic).
state = 0x2545F4914F6CDD1D
blk = bytearray(1024 * 1024)
for i in range(len(blk)):
    state ^= (state << 13) & 0xFFFFFFFFFFFFFFFF
    state ^= state >> 7
    state ^= (state << 17) & 0xFFFFFFFFFFFFFFFF
    blk[i] = state & 0xFF
with open(path, "wb") as f:
    for _ in range(mb):
        f.write(blk)
PY
    fi
    printf '%s\t%s\n' "$SYNTH_TAG" "$SYNTH" >>"$DOCS_TSV"
fi

echo "-- population: $(wc -l <"$DOCS_TSV" | tr -d ' ') sources" >&2

now_ms() { echo $(( ${EPOCHREALTIME/./} / 1000 )); }
rss_kb() { awk -F': ' '/Maximum resident set size/{print $2}' "$1" 2>/dev/null; }

printf 'id\tsrc_bytes\trc\twall_ms\trss_kb\trss_ratio\tdescriptor_sha\texact_rc\texact_ok\texact_len\texact_sha\n' >"$RAW/build.tsv"

while IFS=$'\t' read -r id path; do
    [ -n "$id" ] || continue
    [ -f "$path" ] || { echo "MISSING $path" >&2; continue; }
    blen=$(stat -c %s "$path")
    d="$WORK/$id"; rm -rf "$d"; mkdir -p "$d"
    echo "== $id ($blen B) ==" >&2

    t0=$(now_ms)
    timeout "$OP_TIMEOUT" /usr/bin/time -v "$BIN" field-build "$path" \
        --store "$d/vstore" --profile runtime --packed --sync=batch \
        >"$d/build.json" 2>"$d/build.time"; rc=$?
    t1=$(now_ms); wall_ms=$(( t1 - t0 ))
    rss=$(rss_kb "$d/build.time"); rss=${rss:-0}
    ratio=$(awk -v r="$rss" -v b="$blen" 'BEGIN{ if(b>0) printf "%.3f", r*1024/b; else print 0 }')
    dsha=$(jq -r '.descriptor_sha256 // empty' "$d/build.json" 2>/dev/null)
    field=$(jq -r '.ingest.field // empty' "$d/build.json" 2>/dev/null)
    echo "   field-build rc=$rc ms=$wall_ms rss_kb=${rss} ratio=$ratio desc=${dsha:-none}" >&2

    # Exact closure: length + SHA-256 + byte compare against the source.
    exact_rc=3; exact_ok=0; elen=0; esha=""
    if [ "$rc" -eq 0 ] && [ -n "$field" ]; then
        timeout "$OP_TIMEOUT" "$BIN" materialize --store "$d/vstore" --field "$field" \
            --exact --output "$d/exact.bin" --packed >/dev/null 2>"$d/exact.err"; exact_rc=$?
        elen=$(stat -c %s "$d/exact.bin" 2>/dev/null || echo 0)
        esha=$(sha256sum "$d/exact.bin" 2>/dev/null | cut -d' ' -f1)
        if [ "$exact_rc" -eq 0 ] && [ "$elen" = "$blen" ] && cmp -s "$d/exact.bin" "$path"; then
            exact_ok=1
        fi
    fi

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$id" "$blen" "$rc" "$wall_ms" "$rss" "$ratio" "$dsha" \
        "$exact_rc" "$exact_ok" "$elen" "$esha" >>"$RAW/build.tsv"

    # keep small evidence; drop the (large) store + materialized bytes
    ed="$RAW/perdoc/$id"; mkdir -p "$ed"
    for f in "$d"/build.json "$d"/build.time "$d"/exact.err; do [ -f "$f" ] && cp "$f" "$ed/"; done
    rm -rf "$d"
done <"$DOCS_TSV"

# --- provenance -------------------------------------------------------------
RUSTC_V=$(rustc --version 2>/dev/null | sed 's/"/\\"/g')
CARGO_V=$(cargo --version 2>/dev/null | sed 's/"/\\"/g')
ARCH=$(uname -m)
COMMIT=$(git rev-parse HEAD 2>/dev/null || echo unknown)
DIRTY=$(git --no-optional-locks status --porcelain 2>/dev/null | tr '\n' ';' | sed 's/"/\\"/g')
LOCK_SHA=$(sha256sum Cargo.lock 2>/dev/null | cut -d' ' -f1)
BIN_SHA=$(sha256sum "$BIN" 2>/dev/null | cut -d' ' -f1)
PY_V=$(python3 --version 2>&1)
BASE_STABLE=rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e
cat >"$RAW/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "20.2 — large-source memory architecture for the direct build",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_commit": "$COMMIT",
  "git_dirty": "$DIRTY",
  "arch": "$ARCH",
  "service": "doc-baseline",
  "base_image": "$BASE_STABLE",
  "rustc": "$RUSTC_V",
  "cargo": "$CARGO_V",
  "python": "$PY_V",
  "cargo_lock_sha256": "$LOCK_SHA",
  "bin": "$BIN",
  "bin_sha256": "$BIN_SHA",
  "profile": "$PROFILE",
  "synth_mb": $SYNTH_MB,
  "op_timeout_s": $OP_TIMEOUT,
  "build_cmd": "field-build SRC --store DIR --profile runtime --packed --sync=batch (under /usr/bin/time -v)",
  "exactness": "materialize --exact compared to the source by length + SHA-256 + cmp",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF
cp "$RAW/environment.json" "$CAMPAIGN/environment.json"

cat >"$CAMPAIGN/commands.txt" <<'EOF'
# Phase 20.2 — large-source memory architecture for the direct build.
# Never run on the host.
docker compose run --rm --no-TTY doc-baseline bash tools/phase20-memory-court.sh
# skip the synthetic >=1 GiB source:
docker compose run --rm --no-TTY doc-baseline sh -c 'SYNTH_MB=0 bash tools/phase20-memory-court.sh'
EOF

python3 tools/fixtures/phase20-memory.py "$RAW" | tee "$RAW/summary.txt"

cat >"$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "20.2 — large-source memory architecture for the direct build",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "measured_commit": "$COMMIT",
  "tree_state": "$DIRTY",
  "profile": "$PROFILE",
  "bin": "$BIN",
  "bin_sha256": "$BIN_SHA",
  "base_image": "$BASE_STABLE",
  "service": "doc-baseline (mem_limit == memswap_limit == 6g, pids_limit 4096, cpus 8)",
  "rustc": "$RUSTC_V",
  "cargo": "$CARGO_V",
  "cargo_lock_sha256": "$LOCK_SHA",
  "court": "tools/phase20-memory-court.sh",
  "build_cmd": "field-build SRC --store DIR --profile runtime --packed --sync=batch",
  "cap": "6 GiB mem_limit, memswap_limit == mem_limit (no zram swap); NO cap was raised",
  "rss_metric": "/usr/bin/time -v Maximum resident set size (KiB)",
  "exactness": "materialize --exact: length + SHA-256 + cmp against the source",
  "wire_identity": "descriptor_sha256 per source compared before/after; equality proves the .voldoc bytes are unchanged",
  "synthetic": "deterministic repeated 1 MiB xorshift block, no PDF/ZIP magic (opaque floor)"
}
EOF

chown -R 1000:1000 "$CAMPAIGN" "$WORK" 2>/dev/null || true
echo "phase20.2 large-source memory court: done — $CAMPAIGN"
