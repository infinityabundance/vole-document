#!/usr/bin/env bash
# Phase 17 item 1 — direct source → field ingestion (no candidate search).
#
# ## Question
#
# Today the only way to build a field is `encode SRC OUT.voldoc` (the whole
# candidate portfolio: PDF DEFLATE replay, BYTE_RANS, RLE, RAW, ...) followed by
# `field-ingest OUT.voldoc --store DIR` (which inverts the winning program). The
# field is not a compression product, and its observations are derived by
# re-scanning the *materialized source*, not by reading the winning program — so
# the search is pure overhead for a runtime ingest. This court measures the new
# `field-build SRC --store DIR` direct path (one fixed, court-proved program)
# against the current two-step path.
#
# ## What it does, per document
#
#   * current: `encode` then `field-ingest`   -> the exact authority + the field
#   * direct:  `field-build` (one process)    -> the exact authority + the field
#   * wall time (ms) and peak RSS (KB) for each step, via GNU `/usr/bin/time -v`
#   * exactness BOTH lanes: `materialize --exact` length + SHA-256 + `cmp`, plus
#     `verify` on the current authority and `decode` on the direct authority
#   * observation equality over a fixed, format-agnostic schedule: it runs the
#     same `observe` CLI on both stores and compares the normalized answer
#     (dropping the field id and the read/stat counters, which are not the
#     answer). The one *known* difference is recorded explicitly: the common
#     PDF `metadata` projection embeds the chosen program's `object_count` /
#     `graph_ops` structural counters, so a fixed program reports its own.
#
# ## Population (documented, bounded, deterministic)
#
# A fixed default subset spanning pdf/docx/epub and the <100KiB / 100KiB-1MiB /
# 1-10MiB / 10-50MiB size classes. Override with `IDS="id1 id2 ..."`.
#
# ## Lane
#
# Never the host: the digest-pinned, hard-capped `doc-baseline` service
# (mem_limit == memswap_limit == 6g, pids_limit 4096, cpus 8):
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase17-direct-field-court.sh
#   docker compose run --rm --no-TTY doc-baseline sh -c 'IDS="nist-pdf-0004" bash tools/phase17-direct-field-court.sh'
set -uo pipefail

cd /work
export LC_ALL=C

SHA=$(git --no-pager rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=evidence/campaigns/${STAMP}-phase17-direct-field-${SHA}${TAG:+-$TAG}
RAW=$CAMPAIGN/raw
WORK=${WORK:-evidence/scratch/phase17-direct-field}
rm -rf "$RAW" "$WORK"
mkdir -p "$RAW" "$WORK"

MANIFEST=${MANIFEST:-real100-v1/manifest.tsv}
CORPUS=${CORPUS:-real100-v1/documents}
OP_TIMEOUT=${OP_TIMEOUT:-300}
DEFAULT_IDS="nist-pdf-0002 nist-pdf-0004 nasa-pdf-0007 nist-docx-0010 nist-docx-0001 nist-epub-0008 nist-epub-0004 nasa-epub-0011 nasa-epub-0006"
IDS=${IDS:-$DEFAULT_IDS}

PROFILE=${PROFILE:-release}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-${VOLE_BIN:-target/debug/vole-document}} ;;
esac

echo "== phase17.1 direct source -> field court ==" >&2
echo "-- building ($PROFILE) binary" >&2
# shellcheck disable=SC2086
if ! cargo build $BUILD_ARGS >&2; then
    echo "phase17-direct-field: build FAILED ($BUILD_ARGS); refusing to measure" >&2
    exit 1
fi
[ -x "$BIN" ] || { echo "phase17-direct-field: $BIN missing; refusing to measure" >&2; exit 1; }

# --- subset selection -------------------------------------------------------
python3 - "$MANIFEST" "$CORPUS" $IDS >"$RAW/subset.tsv" <<'PY'
import sys
manifest, corpus = sys.argv[1], sys.argv[2]
ids = sys.argv[3:]
EXT = {"pdf": "pdf", "docx": "docx", "epub": "epub"}
rows = []
with open(manifest) as f:
    hdr = f.readline().rstrip("\n").split("\t")
    for line in f:
        if not line.strip():
            continue
        rows.append(dict(zip(hdr, line.rstrip("\n").split("\t"))))
by_id = {r["id"]: r for r in rows}
for i in ids:
    r = by_id.get(i)
    if not r:
        sys.stderr.write("unknown id %s\n" % i)
        continue
    fmt = r["format"]
    path = "%s/%s/%s/%s.%s" % (corpus, r["agency"], fmt, i, EXT[fmt])
    print("|".join([i, r["agency"], fmt, r["size_class"], r["byte_len"], r["sha256"], path]))
PY
echo "-- subset ($(wc -l <"$RAW/subset.tsv" | tr -d ' ') docs)" >&2
cat "$RAW/subset.tsv" >&2

# --- corpus verification (read-only) ---------------------------------------
echo "-- verifying the subset (SHA-256 + length)" >&2
verify_rc=0
while IFS='|' read -r id _a _f _s _bl sha path; do
    [ -n "$id" ] || continue
    if [ ! -f "$path" ]; then echo "   missing $path" >&2; verify_rc=1; continue; fi
    got_sha=$(sha256sum "$path" | cut -d' ' -f1)
    got_len=$(stat -c %s "$path")
    if [ "$got_sha" != "$sha" ] || [ "$got_len" != "$_bl" ]; then
        echo "   verify FAIL $id" >&2; verify_rc=1
    fi
done <"$RAW/subset.tsv"
if [ "$verify_rc" -ne 0 ]; then
    echo "phase17-direct-field: corpus verification FAILED; refusing to measure" >&2
    exit 1
fi

# --- helpers ----------------------------------------------------------------
cat >"$WORK/norm.py" <<'PY'
import json, sys
try:
    o = json.loads(sys.stdin.read())
except Exception:
    print("PARSE_ERROR")
    sys.exit(0)
o.pop("field", None)
o.pop("stats", None)          # read/wall counters are not the answer
v = o.get("value")
if isinstance(v, dict):       # common document metadata: descriptor-shape counters
    v.pop("object_count", None)
    v.pop("graph_ops", None)
print(json.dumps(o, sort_keys=True))
PY

# run_timed LABEL OUT_TSV CMD...: echo "rc ms rss_kb" on stdout.
run_timed() {
    local label=$1 out=$2; shift 2
    local start end rc rss
    start=$(date +%s%N)
    timeout --signal=KILL "$OP_TIMEOUT" /usr/bin/time -v -o "$WORK/t_$label.txt" \
        "$@" >"$WORK/o_$label.txt" 2>"$WORK/e_$label.txt"
    rc=$?
    end=$(date +%s%N)
    rss=$(awk -F': ' '/Maximum resident set size/{print $2}' "$WORK/t_$label.txt" 2>/dev/null)
    [ -n "$rss" ] || rss=0
    printf '%s %s %s\n' "$rc" "$(( (end - start) / 1000000 ))" "$rss" >"$out"
}

jget() { # jget FILE dotted.path
    python3 - "$1" "$2" <<'PY'
import json, sys
o = json.load(open(sys.argv[1]))
for k in sys.argv[2].split("."):
    o = o[k]
print(o)
PY
}

# The observation schedule (CLI grammar without --store/--field). Format-agnostic:
# a selector a lane cannot answer declines typed, and both lanes must decline
# identically.
SCHEDULE=(
    "--metadata --kind metadata"
    "--doc-text --kind text"
    "--page 1 --kind text"
    "--page 1 --kind structure"
    "--page 1 --kind operators"
    "--byte-range 0..64 --kind exact"
    "--object 1 --kind exact"
    "--stream 1 --kind decoded"
    "--table 0 --kind text"
    "--resource 0 --kind metadata"
    "--text the --kind text"
)

printf 'id\tfmt\tsize_class\tsrc_bytes\tcur_enc_bytes\tdir_enc_bytes\tenc_rc\tenc_ms\tenc_rss_kb\ting_rc\ting_ms\ting_rss_kb\tcur_ms\tcur_rss_kb\tdir_rc\tdir_ms\tdir_rss_kb\tspeedup\n' >"$RAW/build.tsv"
printf 'id\tfmt\tsrc\tcur_exact\tcur_len\tcur_sha\tdir_exact\tdir_len\tdir_sha\tdir_auth_exact\tdir_auth_len\tdir_auth_sha\n' >"$RAW/exact.tsv"
printf 'id\tfmt\tselector\trep\tcur_rc\tdir_rc\tequal\tnote\n' >"$RAW/observe.tsv"

# --- per-document measurement ----------------------------------------------
while IFS='|' read -r id agency fmt size_class byte_len sha path; do
    [ -n "$id" ] || continue
    echo "== $id ($fmt, $size_class, $byte_len B) ==" >&2
    D=$WORK/$id
    mkdir -p "$D"
    CUR_AUTH=$D/current.voldoc
    CUR_STORE=$D/current-store
    DIR_AUTH=$D/direct.voldoc
    DIR_STORE=$D/direct-store

    # --- current path: encode + field-ingest ------------------------------
    run_timed "$id.enc" "$D/enc.t" "$BIN" encode "$path" "$CUR_AUTH"
    read -r ENC_RC ENC_MS ENC_RSS <"$D/enc.t"
    ENC_BYTES=NA
    if [ "$ENC_RC" -eq 0 ]; then
        ENC_BYTES=$(jget "$WORK/o_$id.enc.txt" encoded_len 2>/dev/null || echo NA)
    fi
    ING_RC=99; ING_MS=0; ING_RSS=0; CUR_FIELD=""
    if [ "$ENC_RC" -eq 0 ]; then
        run_timed "$id.ing" "$D/ing.t" "$BIN" field-ingest "$CUR_AUTH" --store "$CUR_STORE"
        read -r ING_RC ING_MS ING_RSS <"$D/ing.t"
        if [ "$ING_RC" -eq 0 ]; then
            CUR_FIELD=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["field"])' "$WORK/o_$id.ing.txt" 2>/dev/null)
        fi
    fi
    CUR_MS=$((ENC_MS + ING_MS))
    CUR_RSS=$ENC_RSS; [ "$ING_RSS" -gt "$CUR_RSS" ] && CUR_RSS=$ING_RSS

    # --- direct path: field-build (one process) ---------------------------
    run_timed "$id.dir" "$D/dir.t" "$BIN" field-build "$path" --store "$DIR_STORE" --voldoc "$DIR_AUTH"
    read -r DIR_RC DIR_MS DIR_RSS <"$D/dir.t"
    DIR_FIELD=""; DIR_BYTES=NA
    if [ "$DIR_RC" -eq 0 ]; then
        DIR_FIELD=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["ingest"]["field"])' "$WORK/o_$id.dir.txt" 2>/dev/null)
        DIR_BYTES=$(jget "$WORK/o_$id.dir.txt" encoded_len 2>/dev/null || echo NA)
    fi

    if [ "$DIR_MS" -gt 0 ] && [ "$CUR_MS" -gt 0 ]; then
        SPEEDUP=$(python3 -c "print('%.3f' % (float($CUR_MS)/float($DIR_MS)))")
    else
        SPEEDUP=NA
    fi
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$id" "$fmt" "$size_class" "$byte_len" \
        "$ENC_BYTES" "$DIR_BYTES" \
        "$ENC_RC" "$ENC_MS" "$ENC_RSS" "$ING_RC" "$ING_MS" "$ING_RSS" \
        "$CUR_MS" "$CUR_RSS" "$DIR_RC" "$DIR_MS" "$DIR_RSS" "$SPEEDUP" >>"$RAW/build.tsv"

    # --- exactness --------------------------------------------------------
    CUR_EXACT=NA; CUR_LEN=NA; CUR_SHA=NA; DIR_EXACT=NA; DIR_LEN=NA; DIR_SHA=NA
    AUTH_EXACT=NA; AUTH_LEN=NA; AUTH_SHA=NA

    # current authority verifies, and the current field materializes exactly.
    if [ "$ENC_RC" -eq 0 ]; then
        timeout --signal=KILL "$OP_TIMEOUT" "$BIN" verify "$CUR_AUTH" >/dev/null 2>&1 || true
    fi
    if [ -n "$CUR_FIELD" ]; then
        timeout --signal=KILL "$OP_TIMEOUT" "$BIN" materialize --store "$CUR_STORE" --field "$CUR_FIELD" --exact --output "$D/cur.mat" >/dev/null 2>&1
        if [ -f "$D/cur.mat" ]; then
            CUR_LEN=$(stat -c %s "$D/cur.mat")
            CUR_SHA=$(sha256sum "$D/cur.mat" | cut -d' ' -f1)
            if cmp -s "$path" "$D/cur.mat"; then CUR_EXACT=1; else CUR_EXACT=0; fi
        fi
    fi

    # direct field materializes exactly, and the direct authority decodes exactly.
    if [ -n "$DIR_FIELD" ]; then
        timeout --signal=KILL "$OP_TIMEOUT" "$BIN" materialize --store "$DIR_STORE" --field "$DIR_FIELD" --exact --output "$D/dir.mat" >/dev/null 2>&1
        if [ -f "$D/dir.mat" ]; then
            DIR_LEN=$(stat -c %s "$D/dir.mat")
            DIR_SHA=$(sha256sum "$D/dir.mat" | cut -d' ' -f1)
            if cmp -s "$path" "$D/dir.mat"; then DIR_EXACT=1; else DIR_EXACT=0; fi
        fi
    fi
    if [ "$DIR_RC" -eq 0 ] && [ -f "$DIR_AUTH" ]; then
        timeout --signal=KILL "$OP_TIMEOUT" "$BIN" decode "$DIR_AUTH" "$D/dir.dec" >/dev/null 2>&1
        if [ -f "$D/dir.dec" ]; then
            AUTH_LEN=$(stat -c %s "$D/dir.dec")
            AUTH_SHA=$(sha256sum "$D/dir.dec" | cut -d' ' -f1)
            if cmp -s "$path" "$D/dir.dec"; then AUTH_EXACT=1; else AUTH_EXACT=0; fi
        fi
    fi
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$id" "$fmt" "$sha" "$CUR_EXACT" "$CUR_LEN" "$CUR_SHA" \
        "$DIR_EXACT" "$DIR_LEN" "$DIR_SHA" "$AUTH_EXACT" "$AUTH_LEN" "$AUTH_SHA" >>"$RAW/exact.tsv"

    # --- observation equality ---------------------------------------------
    if [ -n "$CUR_FIELD" ] && [ -n "$DIR_FIELD" ]; then
        for entry in "${SCHEDULE[@]}"; do
            # shellcheck disable=SC2086
            timeout --signal=KILL "$OP_TIMEOUT" "$BIN" observe --store "$CUR_STORE" --field "$CUR_FIELD" $entry \
                >"$D/cur.obs" 2>"$D/cur.obs.err"; CRA=$?
            # shellcheck disable=SC2086
            timeout --signal=KILL "$OP_TIMEOUT" "$BIN" observe --store "$DIR_STORE" --field "$DIR_FIELD" $entry \
                >"$D/dir.obs" 2>"$D/dir.obs.err"; DRB=$?
            sel=$(printf '%s' "$entry" | awk '{print $1}')
            rep=$(printf '%s' "$entry" | awk '{print $3}')
            NOTE=""
            if [ "$CRA" -ne 0 ] || [ "$DRB" -ne 0 ]; then
                if [ "$CRA" -eq "$DRB" ]; then EQ=decline-equal; else EQ=decline-mismatch; fi
            else
                if diff -q <(python3 "$WORK/norm.py" <"$D/cur.obs") <(python3 "$WORK/norm.py" <"$D/dir.obs") >/dev/null; then
                    EQ=1
                else
                    EQ=0
                    NOTE=$(paste -d' ' <(python3 "$WORK/norm.py" <"$D/cur.obs") <(python3 "$WORK/norm.py" <"$D/dir.obs") | head -c 400)
                fi
                if [ "$sel" = "--metadata" ]; then
                    NOTE="shape-counters cur=$(python3 -c 'import json,sys;v=json.load(open(sys.argv[1]))["value"];print(str(v["object_count"])+"/"+str(v["graph_ops"]))' "$D/cur.obs" 2>/dev/null) dir=$(python3 -c 'import json,sys;v=json.load(open(sys.argv[1]))["value"];print(str(v["object_count"])+"/"+str(v["graph_ops"]))' "$D/dir.obs" 2>/dev/null)"
                fi
            fi
            printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "$sel" "$rep" "$CRA" "$DRB" "$EQ" "$NOTE" >>"$RAW/observe.tsv"
        done
    fi
done <"$RAW/subset.tsv"

# --- aggregate --------------------------------------------------------------
python3 - "$RAW" <<'PY' | tee "$RAW/summary.txt"
import csv, statistics, sys
raw = sys.argv[1]
def load(name):
    with open("%s/%s" % (raw, name)) as f:
        return list(csv.DictReader(f, delimiter="\t"))
b = load("build.tsv")
e = load("exact.tsv")
o = load("observe.tsv")
def num(x):
    try: return float(x)
    except Exception: return None
ok = [r for r in b if r["enc_rc"] == "0" and r["ing_rc"] == "0" and r["dir_rc"] == "0"]
print("rows=%d complete=%d" % (len(b), len(ok)))
cur = [num(r["cur_ms"]) for r in ok if num(r["cur_ms"]) is not None]
drc = [num(r["dir_ms"]) for r in ok if num(r["dir_ms"]) is not None]
if cur and drc:
    print("current sum_ms=%.0f median_ms=%.0f  direct sum_ms=%.0f median_ms=%.0f  speedup sum=%.3fx median=%.3fx" % (
        sum(cur), statistics.median(cur), sum(drc), statistics.median(drc),
        sum(cur)/sum(drc), statistics.median(cur)/statistics.median(drc)))
cur_rss = [num(r["cur_rss_kb"]) for r in ok]
dir_rss = [num(r["dir_rss_kb"]) for r in ok]
if cur_rss and dir_rss:
    print("peak RSS median cur=%.0f KB dir=%.0f KB" % (statistics.median(cur_rss), statistics.median(dir_rss)))
ce = [num(r["cur_enc_bytes"]) for r in ok if num(r["cur_enc_bytes"]) is not None]
de = [num(r["dir_enc_bytes"]) for r in ok if num(r["dir_enc_bytes"]) is not None]
if ce and de:
    print("authority bytes sum cur=%.0f dir=%.0f (direct/current=%.3fx)" % (sum(ce), sum(de), (sum(de)/sum(ce) if sum(ce) else 0)))
byfmt = {}
for r in ok:
    byfmt.setdefault(r["fmt"], []).append((num(r["cur_ms"]), num(r["dir_ms"])))
for fmt, rows in sorted(byfmt.items()):
    c = sum(x for x, _ in rows); d = sum(y for _, y in rows)
    print("  %-4s n=%d cur=%.0fms dir=%.0fms speedup=%.2fx" % (fmt, len(rows), c, d, (c/d if d else 0)))
ex = sum(1 for r in e if r["cur_exact"] == "1")
dx = sum(1 for r in e if r["dir_exact"] == "1")
ax = sum(1 for r in e if r["dir_auth_exact"] == "1")
print("exactness: current %d/%d  direct-field %d/%d  direct-authority-decode %d/%d" % (ex, len(e), dx, len(e), ax, len(e)))
eqs = sum(1 for r in o if r["equal"] == "1")
deq = sum(1 for r in o if r["equal"] == "decline-equal")
bad = [r for r in o if r["equal"] not in ("1", "decline-equal")]
print("observations: equal=%d decline-equal=%d divergent=%d (of %d)" % (eqs, deq, len(bad), len(o)))
for r in bad:
    print("  DIVERGENT %s %s %s: %s" % (r["id"], r["selector"], r["rep"], r["note"][:160]))
PY

BIN_SHA=$(sha256sum "$BIN" | cut -d' ' -f1)
cat >"$CAMPAIGN/environment.json" <<JSON
{
  "campaign": "$CAMPAIGN",
  "phase": "17.1 — direct source -> field ingestion (no candidate search)",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_commit": "$(git --no-pager rev-parse HEAD 2>/dev/null || echo unknown)",
  "git_dirty": "$(git --no-pager status --porcelain | tr '\n' ';')",
  "arch": "$(uname -m)",
  "service": "doc-baseline",
  "profile": "$PROFILE",
  "bin": "$BIN",
  "bin_sha256": "$BIN_SHA",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "manifest": "$MANIFEST",
  "manifest_sha256": "$(sha256sum "$MANIFEST" | cut -d' ' -f1)",
  "subset": "$IDS",
  "op_timeout_s": $OP_TIMEOUT,
  "env_affecting_semantics": {"LC_ALL": "C"}
}
JSON

{
  echo "# Phase 17.1 direct source -> field court — command log"
  echo "# generated $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "docker compose run --rm --no-TTY doc-baseline bash tools/phase17-direct-field-court.sh"
  echo "IDS='$IDS' OP_TIMEOUT=$OP_TIMEOUT PROFILE=$PROFILE"
  echo
  echo "# current path (per document):"
  echo "$BIN encode SRC OUT.voldoc"
  echo "$BIN field-ingest OUT.voldoc --store DIR"
  echo "# direct path (per document):"
  echo "$BIN field-build SRC --store DIR --voldoc OUT.voldoc"
  echo "# exactness:"
  echo "$BIN materialize --store DIR --field FIELD --exact --output OUT   # + sha256sum + cmp"
  echo "$BIN decode DIRECT.voldoc OUT                                   # direct authority"
  echo "$BIN verify CURRENT.voldoc"
  echo "# observations (both stores, one process each):"
  echo "$BIN observe --store DIR --field FIELD <selector> --kind <rep>"
  echo "# normalization: python3 norm.py drops field id + stats; metadata also drops"
  echo "# the two descriptor-shape counters (object_count, graph_ops)."
} >"$CAMPAIGN/commands.txt"

echo "-- evidence sealed under $CAMPAIGN" >&2
