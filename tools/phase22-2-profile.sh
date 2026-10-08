#!/usr/bin/env bash
# Phase 22.2 (review item P1) — the DECISIVE warm-session profiling gate.
#
# NEW file; read-only with respect to every frozen phase-18/19/20/22 court and
# script. It builds each document's packed store ONCE, warms it, then attributes
# one warm `observe-batch` session across stages with independent instruments:
#
#   1. the ENV-GATED internal stage profiler (`VOLE_PROFILE_OPEN=1`, off by
#      default) — store/field open, manifest, descriptor read+parse, and the
#      per-observation breakdown: probe (selector resolution), index-node read
#      (+BLAKE3 verify), index-node decode, materialize (procedural node + typed
#      model decode), dispatch (evaluation core), serialize (answer JSON), plus
#      index node/descent counts;
#   2. `/usr/bin/time -v` — wall, user, sys, peak RSS, minor/major page faults;
#   3. `strace -c -f` — syscall-time and call-count attribution;
#   4. an allocation-counting LD_PRELOAD shim (the crate forbids `unsafe`, so it
#      cannot install a counting global allocator): exact allocation COUNT and
#      requested BYTES for the whole process;
#   5. the observation JSON `bytes_read` / `index_bytes_read` / `seed_bytes_read`
#      / `descriptor_bytes_read` classes — the physical bytes fetched per class.
#
# It reports the same breakdown against the fs (non-packed) seed backend for a
# contrast pair, so the "packed store" cost is not confused with the layout.
#
# Never the host: the digest-pinned, hard-capped `doc-baseline` service,
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase22-2-profile.sh
set -uo pipefail

cd /work
export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1

BIN=${BIN:-target/release/vole-document}
OP_TIMEOUT=${OP_TIMEOUT:-180}
MANIFEST=${MANIFEST:-real100-v1/manifest.tsv}
CORPUS=${CORPUS:-real100-v1/documents}
REPS=${REPS:-5}
DEFAULT_IDS="nist-pdf-0002 nist-pdf-0004 nist-pdf-0016 nist-pdf-0017 nist-docx-0005 nist-docx-0008 nist-docx-0009 nist-docx-0014 nist-epub-0003 nist-epub-0006 nist-epub-0008 nist-epub-0009"
IDS=${IDS:-$DEFAULT_IDS}
FS_CONTRAST=${FS_CONTRAST:-"nist-docx-0008 nist-epub-0009 nist-pdf-0004"}
WORK=${WORK:-evidence/scratch/phase22-2-profile}
OUT=${OUT:-$WORK/out}
SHIM=${SHIM:-/tmp/libmallocc.so}
rm -rf "$WORK"; mkdir -p "$WORK" "$OUT"

[ -x "$BIN" ] || { echo "phase22-2-profile: $BIN missing" >&2; exit 1; }

cc -O2 -shared -fPIC -o "$SHIM" tools/fixtures/phase22-2-malloc-count.c -ldl \
    || { echo "phase22-2-profile: shim build failed" >&2; exit 1; }

obs_for() {
    case "$1" in
        pdf)  echo "bytes text metadata revision" ;;
        *)    echo "bytes text doc-text heading table resource metadata revision" ;;
    esac
}
vole_args() {
    case "$2" in
        bytes)    echo "--byte-range 0..64 --kind exact" ;;
        text)     if [ "$1" = pdf ]; then echo "--page 1 --kind text"; else echo "--block 0 --kind text"; fi ;;
        doc-text) echo "--doc-text --kind text" ;;
        heading)  echo "--heading 0 --kind text" ;;
        table)    echo "--table 0 --kind text" ;;
        resource) echo "--resource 0 --kind metadata" ;;
        metadata) echo "--metadata --kind metadata" ;;
        revision) echo "--revisions --kind lineage" ;;
        *)        echo "DECLINE" ;;
    esac
}

python3 - "$MANIFEST" "$CORPUS" $IDS >"$WORK/subset.tsv" <<'PY'
import sys
manifest, corpus = sys.argv[1], sys.argv[2]
ids = sys.argv[3:]
EXT = {"pdf": "pdf", "docx": "docx", "epub": "epub"}
rows = []
with open(manifest) as f:
    hdr = f.readline().rstrip("\n").split("\t")
    for line in f:
        if line.strip():
            rows.append(dict(zip(hdr, line.rstrip("\n").split("\t"))))
by_id = {r["id"]: r for r in rows}
for i in ids:
    r = by_id.get(i)
    if not r:
        continue
    fmt = r["format"]
    path = "%s/%s/%s/%s.%s" % (corpus, r["agency"], fmt, i, EXT[fmt])
    print("|".join([i, r["agency"], fmt, r["size_class"], r["byte_len"], r["sha256"], path]))
PY

printf 'id\tfmt\tbackend\tblen\tstore_bytes\tdescriptor_len\topen_us\tloop_us\tdescriptor_parse_us\tmanifest_us\tdescriptor_read_us\tindex_open_us\tobserve_us\tprobe_us\tlookup_calls\tindex_nodes\tindex_read_us\tindex_parse_us\tdispatch_us\tmaterialize_us\tserialize_us\twall_ms\tuser_ms\tsys_ms\trss_kb\tminflt\tmajflt\tallocs\talloc_bytes\tsyscall_calls\tindex_openat\tindex_distinct\n' >"$OUT/stages.tsv"

parselast() { # file key -> last numeric value of key
    grep -o "$2=[0-9]*" "$1" 2>/dev/null | tail -1 | cut -d= -f2
}
# Isolate the LAST complete profiled run: a run ends at its `request_loop_total_us=`
# line (the descriptor-read / manifest lines for that run precede it).
lastrun() {
    awk '/request_loop_total_us=/{block=block $0 "\n"; a=block; block=""; next} {block=block $0 "\n"} END{printf "%s", a}' "$1"
}
sumkey() { # block key -> sum of that key over all matches (space-tokenized, so
    # `dispatch_us` never matches inside `observe_dispatch_us`)
    printf '%s' "$1" | tr ' ' '\n' | grep "^$2=[0-9]*" | cut -d= -f2 | awk '{s+=$1} END{print s+0}'
}

run_backend() { # id fmt path backend
    local id="$1" fmt="$2" path="$3" backend="$4"
    local d="$WORK/$id.$backend"
    mkdir -p "$d"
    local packed=""
    [ "$backend" = packed ] && packed="--packed"
    rm -rf "$d/vstore"
    timeout "$OP_TIMEOUT" "$BIN" field-build "$path" --store "$d/vstore" \
        --profile runtime $packed >"$d/build.json" 2>"$d/build.err"
    local field
    field=$(jq -r '.ingest.field // empty' "$d/build.json" 2>/dev/null)
    [ -n "$field" ] || { echo "   $id/$backend build FAILED" >&2; return 1; }
    local sbytes dlen
    sbytes=$(find "$d/vstore" -type f -printf '%s\n' 2>/dev/null | awk '{s+=$1} END{print s+0}')
    dlen=$(parse_dlen "$d/vstore")
    local req="$d/reqs"; : >"$req"
    for obs in $(obs_for "$fmt"); do printf '%s\n' "$(vole_args "$fmt" "$obs")" >>"$req"; done

    # untimed warm-up
    timeout "$OP_TIMEOUT" "$BIN" observe-batch --store "$d/vstore" $packed \
        --field "$field" --requests "$req" >/dev/null 2>&1

    # (1) internal stage profiler, last run
    : >"$d/prof.err"
    for _ in $(seq 1 "$REPS"); do
        VOLE_PROFILE_OPEN=1 timeout "$OP_TIMEOUT" "$BIN" observe-batch --store "$d/vstore" \
            $packed --field "$field" --requests "$req" >/dev/null 2>>"$d/prof.err"
    done
    local blk; blk=$(lastrun "$d/prof.err")
    local open_us loop_us dparse_us man_us dread_us iopen_us
    open_us=$(sumkey "$blk" session_open_total_us)
    loop_us=$(sumkey "$blk" request_loop_total_us)
    dparse_us=$(sumkey "$blk" descriptor_parse_us)
    man_us=$(sumkey "$blk" manifest_read_us)
    dread_us=$(sumkey "$blk" descriptor_read_us)
    iopen_us=$(sumkey "$blk" index_open_us)
    local observe_us probe_us lookup_calls index_nodes index_read_us index_parse_us dispatch_us materialize_us serialize_us
    observe_us=$(sumkey "$blk" observe_us)
    probe_us=$(sumkey "$blk" probe_us)
    lookup_calls=$(sumkey "$blk" lookup_calls)
    index_nodes=$(sumkey "$blk" index_nodes)
    index_read_us=$(sumkey "$blk" index_read_us)
    index_parse_us=$(sumkey "$blk" index_parse_us)
    dispatch_us=$(sumkey "$blk" dispatch_us)
    materialize_us=$(sumkey "$blk" materialize_us)
    serialize_us=$(sumkey "$blk" serialize_us)

    # (2) /usr/bin/time -v  (median block of REPS) plus a fine EPOCHREALTIME wall
    for _ in $(seq 1 "$REPS"); do
        { /usr/bin/time -v "$BIN" observe-batch --store "$d/vstore" $packed \
            --field "$field" --requests "$req" >/dev/null; } 2>>"$d/timev.err"
    done
    local wall_ms_fine
    for _ in $(seq 1 "$REPS"); do
        t0=$(( ${EPOCHREALTIME/./} ))
        "$BIN" observe-batch --store "$d/vstore" $packed --field "$field" --requests "$req" >/dev/null 2>&1
        t1=$(( ${EPOCHREALTIME/./} ))
        echo $(( (t1 - t0) / 1000 )) >>"$d/wall.ms"
    done
    wall_ms_fine=$(sort -n "$d/wall.ms" | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}')
    local wall_ms user_s sys_s rss minflt majflt
    wall_ms=$(awk '/wall clock/{n=split($NF,a,":"); if(n==3) ms=(a[1]*3600+a[2]*60+a[3])*1000; else ms=(a[1]*60+a[2])*1000; print ms}' "$d/timev.err" | sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}')
    wall_ms=${wall_ms_fine:-$wall_ms}
    user_s=$(awk '/User time/{print $NF}' "$d/timev.err" | sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}')
    sys_s=$(awk '/System time/{print $NF}' "$d/timev.err" | sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}')
    rss=$(awk '/Maximum resident/{print $NF}' "$d/timev.err" | sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}')
    minflt=$(awk '/Minor .*faults/{print $NF}' "$d/timev.err" | sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}')
    majflt=$(awk '/Major .*faults/{print $NF}' "$d/timev.err" | sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}')

    # (3) strace -c  + index-openat attribution (how many times each index node is opened)
    timeout "$OP_TIMEOUT" strace -c -f -o "$d/strace.txt" "$BIN" observe-batch --store "$d/vstore" \
        $packed --field "$field" --requests "$req" >/dev/null 2>&1 || true
    local scalls
    scalls=$(awk '/^[0-9]/ && NF>=4 {s+=$(NF-1)} END{print s+0}' "$d/strace.txt")
    timeout "$OP_TIMEOUT" strace -f -e trace=openat -o "$d/openat.txt" "$BIN" observe-batch --store "$d/vstore" \
        $packed --field "$field" --requests "$req" >/dev/null 2>&1 || true
    local index_openat index_distinct
    index_openat=$(grep -c '/index/' "$d/openat.txt" 2>/dev/null || echo 0)
    index_distinct=$(grep '/index/' "$d/openat.txt" 2>/dev/null | grep -o '/index/[a-f0-9/]*' | sort -u | wc -l | tr -d ' ')

    # (4) allocation-counting shim
    local acount abytes
    LD_PRELOAD="$SHIM" timeout "$OP_TIMEOUT" "$BIN" observe-batch --store "$d/vstore" $packed \
        --field "$field" --requests "$req" >/dev/null 2>"$d/malloc.err" || true
    acount=$(grep -o 'allocs=[0-9]*' "$d/malloc.err" | cut -d= -f2)
    abytes=$(grep -o 'bytes=[0-9]*' "$d/malloc.err" | cut -d= -f2)

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$id" "$fmt" "$backend" "$(echo "$path" | xargs stat -c %s)" "$sbytes" "$dlen" \
        "${open_us:-0}" "${loop_us:-0}" "${dparse_us:-0}" "${man_us:-0}" "${dread_us:-0}" "${iopen_us:-0}" \
        "$observe_us" "$probe_us" "${lookup_calls:-0}" "${index_nodes:-0}" "$index_read_us" "$index_parse_us" \
        "$dispatch_us" "$materialize_us" "${serialize_us:-0}" \
        "${wall_ms:-0}" "${user_s:-0}" "${sys_s:-0}" "${rss:-0}" "${minflt:-0}" "${majflt:-0}" \
        "${acount:-0}" "${abytes:-0}" "${scalls:-0}" "${index_openat:-0}" "${index_distinct:-0}" >>"$OUT/stages.tsv"
    cp "$d/prof.err" "$OUT/$id.$backend.prof.err"
    cp "$d/strace.txt" "$OUT/$id.$backend.strace.txt"
    cp "$d/openat.txt" "$OUT/$id.$backend.openat.txt"
    cp "$d/timev.err" "$OUT/$id.$backend.timev.txt"
    cp "$d/malloc.err" "$OUT/$id.$backend.malloc.txt"
    echo "   $id/$backend open=${open_us:-?} loop=${loop_us:-?} idxread=${index_read_us} nodes=${index_nodes} allocs=${acount:-?} wall=${wall_ms:-?}ms" >&2
    rm -rf "$d/vstore"
}

parse_dlen() { # descriptor dir: sum of descriptor blob sizes
    find "$1/descriptor" -type f -printf '%s\n' 2>/dev/null | awk '{s+=$1} END{print s+0}'
}

echo "== phase22.2 profiling gate (REPS=$REPS, docs=$(echo $IDS | wc -w)) ==" >&2
while IFS='|' read -r id agency fmt sclass blen sha path; do
    [ -n "$id" ] || continue
    echo "== $id ($fmt, $sclass, $blen B) ==" >&2
    run_backend "$id" "$fmt" "$path" packed
    for c in $FS_CONTRAST; do
        [ "$c" = "$id" ] && run_backend "$id" "$fmt" "$path" fs
    done
done <"$WORK/subset.tsv"

# process-start floor
echo "-- process-start floor --" >&2
for label in bin_true usage; do
    case "$label" in
        bin_true) cmd=(/bin/true) ;;
        usage)    cmd=("$BIN") ;;
    esac
    # shellcheck disable=SC2068
    { /usr/bin/time -v "${cmd[@]}" >/dev/null; } 2>&1 | grep -E 'wall clock|User time|System time|Maximum resident' | sed "s/^/$label /" >>"$OUT/process_start.txt"
done
cat "$OUT/process_start.txt" >&2 || true

chown -R 1000:1000 "$OUT" "$WORK" 2>/dev/null || true
echo "phase22.2 profile: done — $OUT/stages.tsv"
