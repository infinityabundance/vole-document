#!/bin/sh
# Phase 28.6 — decision Experiment 2: an exact-authority fragment locality
# prototype (MEASUREMENT ONLY).
#
# ## Question
#
#   For narrow requests on LARGE documents, how many **actually required** bytes
#   does the current VOLE physical closure read versus a fragment-level
#   directory's minimum, and versus page-level SQLite? Does format-native +
#   fixed-size fragmentation admit a materially narrower closure, and would the
#   directory's metadata overhead consume the advantage?
#
# ## This ships NO new persistent wire format
#
#   The fragment directory is a side ANALYSIS: per source it builds candidate
#   fragmentations (fixed 4/16/64/256 KiB and format-native structural
#   boundaries) and, per pre-registered narrow request, reports the minimum set
#   of fragments whose verified bytes are actually required. Nothing here is a
#   wire format, a planner change, or a decoder change — it decides whether
#   28.8+ (physical fragmentation) is justified.
#
# ## Three lanes (per request)
#
#   current VOLE  : `observe-batch` cold; the instrumented `bytes_read`
#                   (descriptor+manifest+index+seed — the honest physical total,
#                   exactly Phase 22.5's VOLE number) and, as an independent
#                   cross-check, the process bytes read from the store counted
#                   from `strace -e trace=read,pread64 -yy`.
#   fragment floor: the prototype above (ideal required bytes; each fixed/native
#                   scheme's fragments intersected; the directory bytes it implies
#                   at 48 B/entry).
#   page SQLite   : the Phase-22.5 page-level control, local. A source-retaining
#                   one-blob table; the required source region is read with
#                   `PRAGMA mmap_size=0` under `strace pread64`; bytes = pages
#                   touched. Its whole-`.db` size is reported as a control.
#
# The historical control is Phase 22.5 (`docs/phases/phase-22-5-results.md`;
# `evidence/campaigns/2026-10-08-phase22-5-remote-*`): VOLE selective read a
# median 3.15x more bytes than page-level SQLite. Those numbers are repeated in
# the receipt so the new result is read against them, not instead of them.
#
# ## Verify
#
#   docker compose run --rm --no-TTY doc-baseline sh tools/phase28-6-fragment-locality-court.sh
#
# Env: PROFILE (release|debug), OP_TIMEOUT (default 600).

set -u
cd /work
export LC_ALL=C PYTHONDONTWRITEBYTECODE=1

FRAG=tools/fixtures/phase28-6-fragments.py
SQLITE=tools/fixtures/phase28-6-sqlite.py

PROFILE=${PROFILE:-release}
OP_TIMEOUT=${OP_TIMEOUT:-600}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-target/debug/vole-document} ;;
esac

if [ -x "$BIN" ] && [ -n "${BIN_SKIP_BUILD:-}" ]; then
    echo "-- using prebuilt $BIN (BIN_SKIP_BUILD set)" >&2
else
    echo "== phase 28.6 fragment-locality court (MEASUREMENT ONLY) ==" >&2
    echo "-- building ($PROFILE) binary" >&2
    # shellcheck disable=SC2086
    if ! cargo build $BUILD_ARGS -j 1 >&2; then
        echo "phase28-6: build FAILED ($BUILD_ARGS); refusing to measure" >&2
        exit 1
    fi
fi
[ -x "$BIN" ] || { echo "phase28-6: $BIN missing; refusing to measure" >&2; exit 1; }

command -v strace >/dev/null 2>&1 || { echo "phase28-6: need strace" >&2; exit 1; }
command -v cc >/dev/null 2>&1 || { echo "phase28-6: need cc" >&2; exit 1; }
command -v qpdf >/dev/null 2>&1 || { echo "phase28-6: need qpdf (pdf reachability)" >&2; exit 1; }

SHORT=$(git rev-parse --short HEAD 2>/dev/null || echo nogit)
STAMP=$(date -u +%Y-%m-%d)
CAMP=${CAMP:-evidence/campaigns/${STAMP}-phase28-6-fragment-locality-${SHORT}}
RAW=$CAMP/raw
OUT=$CAMP/out
WORK=${WORK:-evidence/scratch/phase28-6-court}
rm -rf "$CAMP" "$WORK"
mkdir -p "$RAW" "$OUT" "$RAW/q" "$WORK" "$WORK/store"
CORPUS=$WORK/corpus
ABS=$(pwd)

RUNSTAT=/tmp/phase28-6-runstat
cc -O2 -o "$RUNSTAT" tools/fixtures/phase28-3-runstat.c || { echo "runstat build failed" >&2; exit 1; }

# ---------------------------------------------------------------------------
# Metadata (a receipt records its environment, never assumes it)
# ---------------------------------------------------------------------------
{
    echo "# Phase 28.6 receipt metadata"
    echo "generated_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "git_commit: $(git rev-parse HEAD 2>/dev/null || echo nogit)"
    echo "git_branch: $(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo nogit)"
    echo "git_dirty: $(test -n "$(git --no-optional-locks status --porcelain 2>/dev/null)" && echo yes || echo no)"
    echo "root: $ABS"
    echo "rustc: $(rustc -V 2>/dev/null || echo n/a)"
    echo "cargo: $(cargo -V 2>/dev/null || echo n/a)"
    echo "python3: $(python3 --version 2>&1)"
    echo "qpdf: $(qpdf --version 2>/dev/null | head -1)"
    echo "strace: $(strace --version 2>/dev/null | head -1)"
    echo "sqlite3: $(sqlite3 --version 2>/dev/null)"
    echo "arch: $(uname -m)"
    echo "nproc: $(nproc 2>/dev/null || echo '?')"
    echo "cargo_lock_sha256: $(sha256sum Cargo.lock | cut -d' ' -f1)"
    echo "binary: $BIN"
    echo "binary_sha256: $(sha256sum "$BIN" | cut -d' ' -f1)"
    echo "build_args: cargo build $BUILD_ARGS -j 1"
    echo "service: doc-baseline (mem_limit == memswap_limit == 6g, pids_limit 4096, cpus 8)"
    echo "base_image: rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e (doc-baseline: latest of that base)"
    echo "substrate: field-build --profile runtime --packed"
    echo "directory_entry_model: 48 B/entry (u64 offset + u32 length + sha256, padded)"
    echo "phase22_5_historical_control: VOLE selective median 3.15x page-level SQLite bytes; 0 wins / 17 losses / 7 unresolved; docs/phases/phase-22-5-results.md"
    echo "notes: measurement only; no wire format, no planner change, no decoder change."
} > "$CAMP/METADATA.txt"

echo "== regenerating / pinning the corpus ==" >&2
python3 "$FRAG" corpus --out "$CORPUS" > "$RAW/corpus.tsv" 2>"$RAW/corpus.err" \
    || { echo "phase28-6: corpus generation failed" >&2; cat "$RAW/corpus.err" >&2; exit 1; }
cat "$RAW/corpus.tsv" >&2

# ---------------------------------------------------------------------------
# Per-format narrow request (pre-registered)
# ---------------------------------------------------------------------------
request_for() {
    case "$1" in
        pdf)     echo "--page 1 --kind text" ;;
        docx)    echo "--block 0 --kind text" ;;
        json)    echo "--json-pointer /items/19999 --kind text" ;;
        csv)     echo "--csv-range 10:0:12:3 --kind text" ;;
        parquet) echo "--parquet-cell 0:0 --kind text" ;;
    esac
}
require_for() {
    case "$1" in
        pdf)     echo "page-1 text (heuristic projection)" ;;
        docx)    echo "block-0 text (deterministically derived)" ;;
        json)    echo "JSON pointer /items/19999 (deterministically derived)" ;;
        csv)     echo "CSV records 10..12 (deterministically derived)" ;;
        parquet) echo "Parquet cell 0:0 (deterministically derived)" ;;
    esac
}

printf 'fmt\tsrc_bytes\tsrc_sha256\tstore_bytes\trequest\tvole_rc\tvole_bytes_read\tvole_phys_bytes\tvole_wall_us\tvole_rss_kb\tvole_ret\trequired_bytes\trequired_ranges\texact_ok\n' > "$RAW/observations.tsv"
printf 'fmt\tscheme\tn_fragments\thit_fragments\tfloor_bytes\tdirectory_bytes\n' > "$RAW/schemes.tsv"
printf 'fmt\tlayout\tdb_bytes\tregion_extract_ok\treads\tdistinct_ranges\tpage_bytes\tpages\n' > "$RAW/sqlite.tsv"

echo "== measuring lanes ==" >&2
tail -n +2 "$RAW/corpus.tsv" | while IFS="$(printf '\t')" read -r fmt path bytes sha; do
    [ -n "$fmt" ] || continue
    store="$WORK/store/$fmt"
    rm -rf "$store"
    echo "== $fmt ($bytes B) ==" >&2
    if ! timeout "$OP_TIMEOUT" "$BIN" field-build "$path" --store "$store" \
            --profile runtime --packed >"$RAW/q/$fmt.build.json" 2>"$RAW/q/$fmt.build.err"; then
        echo "   field-build FAILED" >&2; continue
    fi
    FIELD=$(grep -o '"field":"[0-9a-f]*"' "$RAW/q/$fmt.build.json" | head -1 | cut -d'"' -f4)
    store_bytes=$(find "$store" -type f -printf '%s\n' | awk '{s+=$1} END{print s+0}')
    req=$(request_for "$fmt")
    printf '%s\n' "$req" > "$RAW/q/$fmt.req"

    # ---- current VOLE: cold instrumented observation (+ wall/RSS) -----------
    rm -rf "$store/cache"
    "$RUNSTAT" "$BIN" observe-batch --store "$store" --field "$FIELD" --packed \
        --requests "$RAW/q/$fmt.req" >"$RAW/$fmt.answer.json" 2>"$RAW/q/$fmt.observe.err"
    vrc=$?
    rl=$(grep 'runstat wall_us' "$RAW/q/$fmt.observe.err" | tail -1)
    wall=$(printf '%s' "$rl" | grep -o 'wall_us=[0-9]*' | cut -d= -f2)
    rss=$(printf '%s' "$rl" | grep -o 'maxrss_kb=[0-9]*' | cut -d= -f2)
    vb=$(python3 -c "import json;print(json.load(open('$RAW/$fmt.answer.json'))['stats']['bytes_read'])" 2>/dev/null || echo 0)
    vr=$(python3 -c "import json;print(json.load(open('$RAW/$fmt.answer.json'))['stats']['bytes_returned'])" 2>/dev/null || echo 0)

    # ---- independent physical-read cross-check (strace, store only) ---------
    rm -rf "$store/cache"
    strace -f -e trace=read,pread64 -yy -o "$RAW/q/$fmt.strace" \
        "$BIN" observe-batch --store "$store" --field "$FIELD" --packed \
        --requests "$RAW/q/$fmt.req" >/dev/null 2>"$RAW/q/$fmt.strace.err"
    phys=$(python3 "$SQLITE" trace2bytes --trace "$RAW/q/$fmt.strace" \
        --match "$ABS/$store" --page 0 | python3 -c "import json,sys;print(json.load(sys.stdin)['bytes'])" 2>/dev/null || echo 0)

    # ---- exactness: materialize --exact == source (length + sha256 + cmp) ----
    timeout "$OP_TIMEOUT" "$BIN" materialize --store "$store" --field "$FIELD" \
        --exact --output "$WORK/$fmt.exact.bin" --packed >/dev/null 2>"$RAW/q/$fmt.exact.err"
    exact_ok=0
    if [ -f "$WORK/$fmt.exact.bin" ]; then
        ml=$(stat -c %s "$WORK/$fmt.exact.bin")
        mh=$(sha256sum "$WORK/$fmt.exact.bin" | cut -d' ' -f1)
        if [ "$ml" = "$bytes" ] && [ "$mh" = "$sha" ] && cmp -s "$WORK/$fmt.exact.bin" "$path"; then
            exact_ok=1
        fi
    fi

    # ---- fragment directory floor -------------------------------------------
    if python3 "$FRAG" analyze --fmt "$fmt" --source "$path" \
            --answer "$RAW/$fmt.answer.json" --out "$RAW/$fmt.frag.json" \
            >"$RAW/q/$fmt.frag.out" 2>"$RAW/q/$fmt.frag.err"; then
        cp "$RAW/$fmt.frag.json" "$OUT/$fmt.frag.json"
        python3 - "$RAW/$fmt.frag.json" "$RAW/schemes.tsv" "$fmt" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
with open(sys.argv[2], "a") as f:
    for name in sorted(d["schemes"]):
        s = d["schemes"][name]
        f.write("%s\t%s\t%d\t%d\t%d\t%d\n" % (
            sys.argv[3], name, s["n_fragments"], s["hit_fragments"],
            s["floor_bytes"], s["directory_bytes"]))
PY
        rb=$(python3 -c "import json;print(json.load(open('$RAW/$fmt.frag.json'))['required_bytes'])")
        rn=$(python3 -c "import json;print(json.load(open('$RAW/$fmt.frag.json'))['required_n_ranges'])")
        python3 -c "import json;json.dump(json.load(open('$RAW/$fmt.frag.json'))['required_ranges'], open('$RAW/q/$fmt.ranges.json','w'))"
    else
        echo "   fragment analyze FAILED" >&2; head -3 "$RAW/q/$fmt.frag.err" >&2; continue
    fi

    # ---- page-level SQLite control (Phase 22.5 model, local) ----------------
    # Two envelopes: `pages` = one row per 4 KiB page (the honest page-level
    # control: a required byte range maps to explicit pages), and `blob` = the
    # Phase-22.5 source_blob analogue (a single BLOB whose overflow chain must
    # be walked). Both are reported.
    for layout in pages blob; do
        db="$WORK/$fmt.$layout.db"
        python3 "$SQLITE" build --source "$path" --db "$db" --layout "$layout" \
            >"$RAW/q/$fmt.sqlite.$layout.build.json" 2>"$RAW/q/$fmt.sqlite.$layout.build.err"
        db_bytes=$(stat -c %s "$db" 2>/dev/null || echo 0)
        strace -f -e trace=pread64 -yy -o "$RAW/q/$fmt.sqlite.$layout.strace" \
            python3 "$SQLITE" query --db "$db" --ranges "$RAW/q/$fmt.ranges.json" \
            --layout "$layout" >"$RAW/q/$fmt.sqlite.$layout.query.json" \
            2>"$RAW/q/$fmt.sqlite.$layout.query.err"
        qok=$(python3 -c "import json;print(json.load(open('$RAW/q/$fmt.sqlite.$layout.query.json'))['got'])" 2>/dev/null || echo 0)
        python3 "$SQLITE" trace2bytes --trace "$RAW/q/$fmt.sqlite.$layout.strace" \
            --match "$db" --page 4096 >"$RAW/q/$fmt.sqlite.$layout.trace.json" 2>/dev/null
        sreads=$(python3 -c "import json;print(json.load(open('$RAW/q/$fmt.sqlite.$layout.trace.json'))['reads'])" 2>/dev/null || echo 0)
        sdist=$(python3 -c "import json;print(json.load(open('$RAW/q/$fmt.sqlite.$layout.trace.json'))['distinct_ranges'])" 2>/dev/null || echo 0)
        sbytes=$(python3 -c "import json;print(json.load(open('$RAW/q/$fmt.sqlite.$layout.trace.json'))['bytes'])" 2>/dev/null || echo 0)
        spages=$(python3 -c "import json;print(json.load(open('$RAW/q/$fmt.sqlite.$layout.trace.json'))['pages'])" 2>/dev/null || echo 0)
        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$fmt" "$layout" "$db_bytes" "$qok" "$sreads" "$sdist" "$sbytes" "$spages" >> "$RAW/sqlite.tsv"
    done

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$fmt" "$bytes" "$sha" "$store_bytes" "$req" "$vrc" "${vb:-0}" "${phys:-0}" \
        "${wall:-0}" "${rss:-0}" "${vr:-0}" "$rb" "$rn" "$exact_ok" >> "$RAW/observations.tsv"

    rm -rf "$store/cache"
done

# ---------------------------------------------------------------------------
# Bound raw evidence: gzip the syscall traces; drop any that stay > 9 MiB so a
# receipt never carries an oversized artifact (the derived trace2bytes JSON and
# the merged regions remain).
# ---------------------------------------------------------------------------
for t in "$RAW"/q/*.strace; do
    [ -f "$t" ] || continue
    gzip -f "$t" 2>/dev/null || true
done
find "$RAW" -type f -name '*.strace.gz' -size +9M -print -delete 2>/dev/null >> "$RAW/dropped-large-traces.txt"

# ---------------------------------------------------------------------------
# Aggregate → SUMMARY.md + receipt.json
# ---------------------------------------------------------------------------
python3 tools/fixtures/phase28-6-aggregate.py "$RAW" "$OUT" "$CAMP" "$SHORT"

# ---------------------------------------------------------------------------
# Decision (recorded in the receipt and printed for the report)
# ---------------------------------------------------------------------------
python3 tools/fixtures/phase28-6-decide.py "$CAMP/receipt.json" > "$CAMP/DECISION.txt"
cat "$CAMP/DECISION.txt" >&2

# ---------------------------------------------------------------------------
# Exactness gate: a receipt is only valid if every document reproduced exactly.
# ---------------------------------------------------------------------------
EXACT=$(python3 -c "import json;print(json.load(open('$CAMP/receipt.json'))['exact_ok'])")
NDOCS=$(python3 -c "import json;print(json.load(open('$CAMP/receipt.json'))['n_docs'])")
if [ "$EXACT" != "$NDOCS" ] || [ "$NDOCS" -lt 5 ]; then
    echo "phase28-6: EXACTNESS GATE FAILED: $EXACT/$NDOCS materialize --exact == source" >&2
    exit 1
fi
echo "phase28-6: exactness gate OK ($EXACT/$NDOCS)" >&2

# ---------------------------------------------------------------------------
# Evidence-size gate: the sealed receipt must carry no file > 10 MiB.
# ---------------------------------------------------------------------------
OVER=$(find "$CAMP" -type f -size +10M 2>/dev/null | head -5)
if [ -n "$OVER" ]; then
    echo "phase28-6: EVIDENCE-SIZE GATE FAILED (>10 MiB):" >&2
    printf '%s\n' "$OVER" >&2
    exit 1
fi

echo "== phase 28.6 court complete: $CAMP ==" >&2
ls -la "$CAMP" >&2
