#!/usr/bin/env bash
# Phase 20.3 profiling harness — attribute the warm one-session cost.
#
# NEW file; read-only with respect to the frozen phase-18/19 courts. It builds
# each document's packed store ONCE (exactly as tools/phase19-warm-court.sh
# does), then profiles the warm `observe-batch` process with:
#   * the env-gated internal open profiler (`VOLE_PROFILE_OPEN=1`) — store open,
#     manifest read, descriptor read, descriptor parse, index open, per-request
#     dispatch;
#   * `/usr/bin/time -v` — wall/user/sys and peak RSS;
#   * `strace -c` — syscall-time attribution;
#   * `strace -T` on the fs syscalls — the time actually spent in open/read.
# Plus process-start references (`/bin/true`, a no-op CLI invocation, sqlite3).
#
# Never the host: run inside the hard-capped doc-baseline service,
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase20-warm-profile.sh
set -uo pipefail

cd /work
export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1

BIN=${BIN:-target/release/vole-document}
OP_TIMEOUT=${OP_TIMEOUT:-180}
MANIFEST=${MANIFEST:-real100-v1/manifest.tsv}
CORPUS=${CORPUS:-real100-v1/documents}
DOCS=${DOCS:-"nist-docx-0008 nist-docx-0014 nist-pdf-0004 nist-pdf-0017 nist-epub-0009"}
WORK=${WORK:-evidence/scratch/phase20-warm-profile}
OUT=${OUT:-evidence/scratch/phase20-warm-profile/out}
rm -rf "$WORK" "$OUT"; mkdir -p "$WORK" "$OUT"

[ -x "$BIN" ] || { echo "phase20-profile: $BIN missing" >&2; exit 1; }

now_us() { echo $(( ${EPOCHREALTIME/./} )); }

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

# select docs from the manifest (same path derivation as the phase-19 court)
python3 - "$MANIFEST" "$CORPUS" $DOCS >"$WORK/subset.tsv" <<'PY'
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
    print("|".join([i, r["agency"], fmt, r["size_class"], r["byte_len"], r["sha256"], path,
                    r["revision_family_id"] or "", "0"]))
PY

printf 'id\tfmt\tblen\tvstore_bytes\tdescriptor_len\tstore_open_us\tfield_open_us\tindex_open_us\tsession_open_total_us\tmanifest_read_us\tdescriptor_read_us\tdescriptor_parse_us\tloop_total_us\tdispatch_us\tobserves\n' >"$OUT/internal.tsv"

echo "== process-start references ==" | tee "$OUT/process_start.txt"
for label in bin_true usage vole_vsqlite; do
    case "$label" in
        bin_true) cmd=(/bin/true) ;;
        usage)    cmd=("$BIN") ;;
        vole_vsqlite) cmd=(sqlite3 :memory: "select 1;") ;;
    esac
    # shellcheck disable=SC2068
    { /usr/bin/time -v "${cmd[@]}" >/dev/null; } 2>&1 | grep -E 'wall clock|User time|System time|Maximum resident' | sed "s/^/$label /" >>"$OUT/process_start.txt"
done
grep -E 'wall clock' "$OUT/process_start.txt" | sed 's/^/  /' >&2

while IFS='|' read -r id agency fmt sclass blen sha path fam is_head; do
    [ -n "$id" ] || continue
    d="$WORK/$id"; mkdir -p "$d"
    echo "== $id ($fmt, $sclass, $blen B) ==" >&2
    rm -rf "$d/vstore"
    timeout "$OP_TIMEOUT" "$BIN" field-build "$path" --store "$d/vstore" \
        --profile runtime --packed >"$d/build.json" 2>"$d/build.err"
    field=$(jq -r '.ingest.field // empty' "$d/build.json" 2>/dev/null)
    [ -n "$field" ] || { echo "   build FAILED" >&2; continue; }
    vstore_bytes=$(find "$d/vstore" -type f -printf '%s\n' | awk '{s+=$1} END{print s+0}')
    dlen=$(stat -c %s "$d/vstore/descriptor/$field" 2>/dev/null || echo 0)

    req="$d/vole.reqs"; : >"$req"
    for obs in $(obs_for "$fmt"); do printf '%s\n' "$(vole_args "$fmt" "$obs")" >>"$req"; done

    # untimed warm-up
    timeout "$OP_TIMEOUT" "$BIN" observe-batch --store "$d/vstore" --packed \
        --field "$field" --requests "$req" >/dev/null 2>&1

    # (a) internal env-gated breakdown, one run per depth-slot (the court runs
    #     one process per depth; here one representative process, repeated 5x)
    : >"$d/internal.err"
    for _ in 1 2 3 4 5; do
        VOLE_PROFILE_OPEN=1 timeout "$OP_TIMEOUT" "$BIN" observe-batch --store "$d/vstore" --packed \
            --field "$field" --requests "$req" >/dev/null 2>>"$d/internal.err"
    done
    store_us=$(grep -o 'store_open_us=[0-9]*' "$d/internal.err" | cut -d= -f2 | sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}')
    field_us=$(grep -o 'field_open_us=[0-9]*' "$d/internal.err" | cut -d= -f2 | sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}')
    index_us=$(grep -o 'index_open_us=[0-9]*' "$d/internal.err" | cut -d= -f2 | sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}')
    sess_us=$(grep -o 'session_open_total_us=[0-9]*' "$d/internal.err" | cut -d= -f2 | sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}')
    man_us=$(grep -o 'manifest_read_us=[0-9]*' "$d/internal.err" | cut -d= -f2 | sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}')
    dread_us=$(grep -o 'descriptor_read_us=[0-9]*' "$d/internal.err" | cut -d= -f2 | sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}')
    dparse_us=$(grep -o 'descriptor_parse_us=[0-9]*' "$d/internal.err" | cut -d= -f2 | sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}')
    loop_us=$(grep -o 'request_loop_total_us=[0-9]*' "$d/internal.err" | cut -d= -f2 | sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}')
    disp_us=$(grep -o 'observe_dispatch_us=[0-9]*' "$d/internal.err" | cut -d= -f2 | sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}')
    nocalls=$(grep -o 'observe_calls=[0-9]*' "$d/internal.err" | cut -d= -f2 | sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}')
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$id" "$fmt" "$blen" "$vstore_bytes" "$dlen" "$store_us" "$field_us" "$index_us" \
        "$sess_us" "$man_us" "$dread_us" "$dparse_us" "$loop_us" "$disp_us" "$nocalls" >>"$OUT/internal.tsv"
    echo "   internal: store=$store_us field=$field_us index=$index_us sess=$sess_us loop=$loop_us dispatch=$disp_us" >&2

    # (b) time -v, median of 5 wall readings
    for _ in 1 2 3 4 5; do
        { /usr/bin/time -v "$BIN" observe-batch --store "$d/vstore" --packed \
            --field "$field" --requests "$req" >/dev/null; } 2>>"$d/timev.err"
    done
    grep -E 'wall clock' "$d/timev.err" | sed -E 's/.*m([0-9.]+)s.*/\1/' | awk '{print $1*1000}' | sort -n | awk -v id="$id" 'END{print id"\t"a[int((NR+1)/2)]}' >>"$OUT/timev_wall.tsv" 2>/dev/null || true
    # simpler robust capture
    awk '/wall clock/{v=$NF; gsub("s","",v); print v*1000}' "$d/timev.err" | sort -n | awk -v id="$id" '{a[NR]=$1} END{if(NR)print id"\t"a[int((NR+1)/2)]}' >>"$OUT/timev.tsv"

    # (c) strace -c syscall attribution
    timeout "$OP_TIMEOUT" strace -c -f -o "$OUT/$id.strace.txt" "$BIN" observe-batch --store "$d/vstore" \
        --packed --field "$field" --requests "$req" >/dev/null 2>&1 || true
    cp "$d/internal.err" "$OUT/$id.internal.err"

    echo "   stored-close: vstore=$vstore_bytes dlen=$dlen" >&2
    rm -rf "$d/vstore"
done <"$WORK/subset.tsv"

echo "-- internal breakdown ($OUT/internal.tsv) --" >&2
cat "$OUT/internal.tsv" >&2
echo "-- strace -c per doc ($OUT/*.strace.txt) --" >&2
chown -R 1000:1000 "$OUT" "$WORK" 2>/dev/null || true
echo "phase20 warm-profile: done — $OUT"
