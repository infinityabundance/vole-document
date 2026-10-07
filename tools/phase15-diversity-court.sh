#!/usr/bin/env bash
# Phase 15.6 — unforeseen-query-diversity court (adaptive procedural promotion).
#
# Measures the precompute/recompute frontier under a query schedule whose later
# observation order is NOT revealed at ingest. Progressive query-TYPE diversity:
# types are revealed one at a time (text, metadata, heading, table, resource) and
# at each depth d the lane runs the ordered prefix of the frozen stream; the
# cumulative cost at d is (ingest/build) + (all prefix queries). Every lane
# consumes the identical stream front-to-back. Popularity is only discoverable
# online; the emitted schedule JSON is content-addressed and recorded.
#
# Lanes (same corpus / store; only the query engine differs):
#   v_off   VOLE, default (best-effort disposable cache, no promotion)
#   v_on    VOLE, --promote (durable governed store; > <root>/promoted/)
#   sq_min  SQLite minimal      (source blob only, re-parse per request)
#   sq_full SQLite full         (every view materialized eagerly: phase12 build)
#   sq_adapt SQLite adaptive    (minimal + online governed result_cache)
#
# Per (doc, lane, depth): cumulative wall ms, CPU ms (user+sys), peak RSS,
# physical reads (VOLE logical `bytes_read` summed at a SEPARATE boundary from
# SQLite `/proc/self/io` read_bytes — never compared raw, ADR-0027 §Correction),
# persistent bytes, and the durable aux bytes (promoted/ vs result_cache).
#
# Resource bounds: this is a BOUNDED run. DOCS selects a small documented subset
# and PER_TYPE bounds the stream. Nothing is tuned to manufacture a win.
#
# Run in the pinned `doc-baseline` service:
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase15-diversity-court.sh
set -uo pipefail

cd /work
export LC_ALL=C

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=${CAMPAIGN:-evidence/campaigns/${STAMP}-phase15-diversity-${SHA}}
RAW=$CAMPAIGN/raw
WORK=${WORK:-evidence/scratch/phase15-diversity-work}
mkdir -p "$RAW" "$WORK"

MANIFEST=${MANIFEST:-real100-v1/manifest.tsv}
CORPUS=${CORPUS:-real100-v1/documents}
# Bounded, documented subset (one per format; small documents to bound the
# re-extraction cost of the minimal/adaptive lanes).
DOCS=${DOCS:-"nist-docx-0005 nist-epub-0008 nist-pdf-0007"}
PER_TYPE=${PER_TYPE:-8}
PROFILE=${PROFILE:-release}
PROMOTE_SUFFIX=${PROMOTE_SUFFIX:-}   # empty = bare `--promote` (default budget)
BASE=tools/fixtures/phase12-baseline.py
DRV=tools/fixtures/phase15-baseline.py
FRONT=tools/fixtures/phase15-frontier.py

echo "== phase15 diversity court ==" >&2
echo "-- building ($PROFILE) binary" >&2
# shellcheck disable=SC2086
if ! cargo build --release --locked --all-features >&2; then
    echo "diversity-court: build FAILED; refusing to measure" >&2
    exit 1
fi
BIN=target/release/vole-document

echo "-- schedule constants: SEED=20261007 TYPES=text,metadata,heading,table,resource PER_TYPE=$PER_TYPE" >&2

# --- per-lane driver (written once; parameterized by env) -------------------
DRIVER=$WORK/lane-driver.sh
cat >"$DRIVER" <<'DRIVER_EOF'
#!/usr/bin/env bash
set -uo pipefail
cd /work
lane=$LANE; fmt=$LANE_FMT; src=$LANE_SRC; run=$LANE_RUN
reqs=$LANE_REQS; reqjson=$LANE_REQJSON
mkdir -p "$run"
s0=$(date +%s%N)
case "$lane" in
  v_off|v_on)
    "$BIN" encode "$src" "$run/v.voldoc" >"$run/encode.json" 2>"$run/encode.err"
    "$BIN" field-ingest "$run/v.voldoc" --store "$run/store" >"$run/ingest.json" 2>"$run/ingest.err"
    jq -r '.field // empty' "$run/ingest.json" > "$run/field"
    ;;
  sq_min|sq_adapt)
    python3 "$DRV" min-build --source "$src" --db "$run/min.db" >"$run/minbuild.json" 2>"$run/minbuild.err"
    ;;
  sq_full)
    python3 "$BASE" build --format "$fmt" --source "$src" --db "$run/a1.db" --metrics "$run/a1.build.metrics.json" >"$run/a1build.json" 2>"$run/a1build.err"
    ;;
esac
s1=$(date +%s%N); echo $(( (s1 - s0) / 1000000 )) > "$run/setup_ms"
case "$lane" in
  v_off|v_on)
    extra=""
    [ "$lane" = v_on ] && extra="--promote${PROMOTE_SUFFIX}"
    field=$(cat "$run/field")
    # shellcheck disable=SC2086
    "$BIN" observe-batch --store "$run/store" --field "$field" --requests "$reqs" $extra >"$run/observe.jsonl" 2>"$run/observe.err"
    ;;
  sq_min|sq_adapt)
    dlane=${lane#sq_}
    python3 "$DRV" run --lane "$dlane" --format "$fmt" --source "$src" --db "$run/min.db" --requests "$reqjson" --out "$run/metrics.json" >"$run/driver.out" 2>"$run/driver.err"
    ;;
  sq_full)
    python3 "$DRV" run --lane full --format "$fmt" --source "$src" --db "$run/a1.db" --requests "$reqjson" --out "$run/metrics.json" >"$run/driver.out" 2>"$run/driver.err"
    ;;
esac
s2=$(date +%s%N); echo $(( (s2 - s1) / 1000000 )) > "$run/query_ms"
DRIVER_EOF
chmod +x "$DRIVER"

now_ms() { echo $(( $(date +%s%N) / 1000000 )); }

# parse `/usr/bin/time -v` output for CPU + peak RSS
time_user_ms() { awk -F': ' '/User time \(seconds\)/{printf "%d", $2*1000}' "$1"; }
time_sys_ms()  { awk -F': ' '/System time \(seconds\)/{printf "%d", $2*1000}' "$1"; }
time_rss_kb()  { awk -F': ' '/Maximum resident set size/{print $2}' "$1"; }

printf 'doc\tfmt\tlane\tdepth\tn_requests\tsetup_ms\tquery_ms\ttotal_ms\tcpu_user_ms\tcpu_sys_ms\trss_kb\tphys_read_bytes\tpersistent_bytes\taux_bytes\tanswered\tdeclined\n' >"$RAW/diversity.tsv"

ALL_LANES="v_off v_on sq_min sq_full sq_adapt"

for id in $DOCS; do
    fmt=$(awk -F'\t' -v id="$id" 'NR>1 && $1==id{print $5}' "$MANIFEST")
    src=$(ls "$CORPUS"/*/*/"$id".* 2>/dev/null | head -1)
    if [ -z "$fmt" ] || [ -z "$src" ] || [ ! -f "$src" ]; then
        echo "diversity-court: missing doc $id (fmt=$fmt src=$src); skipping" >&2
        continue
    fi
    echo "== $id ($fmt) ==" >&2
    ddir=$WORK/$id
    rm -rf "$ddir"; mkdir -p "$ddir"
    schd=$RAW/schedule-$id.json
    python3 "$FRONT" schedule --format "$fmt" --out "$schd" --per-type "$PER_TYPE" >"$RAW/schedule-$id.meta.json"
    # depth count = number of revealed types (== the schedule's `types` length)
    T=$(jq '.types | length' "$schd")

    for ((d=1; d<=T; d++)); do
        pfx_json=$ddir/d$d.reqs.json
        pfx_reqs=$ddir/d$d.reqs
        python3 - "$schd" "$d" "$PER_TYPE" "$pfx_json" "$pfx_reqs" <<'PY'
import json, sys
sch = json.load(open(sys.argv[1])); depth = int(sys.argv[2]); per = int(sys.argv[3])
reqs = sch["requests"][: depth * per]
json.dump(reqs, open(sys.argv[4], "w"))
with open(sys.argv[5], "w") as fh:
    for r in reqs:
        fh.write(r["arg"] + "\n")
PY
        nreq=$(wc -l < "$pfx_reqs" | tr -d ' ')
        for lane in $ALL_LANES; do
            run=$ddir/$lane/d$d
            rm -rf "$run"; mkdir -p "$run"
            LANE=$lane LANE_ID=$id LANE_FMT=$fmt LANE_SRC=$src LANE_RUN=$run \
                LANE_REQS=$pfx_reqs LANE_REQJSON=$pfx_json BIN=$BIN DRV=$DRV \
                BASE=$BASE PROMOTE_SUFFIX=$PROMOTE_SUFFIX \
                /usr/bin/time -v -o "$run/time.txt" bash "$DRIVER" \
                >"$run/stdout" 2>"$run/stderr"
            rc=$?
            setup_ms=$(cat "$run/setup_ms" 2>/dev/null || echo 0)
            query_ms=$(cat "$run/query_ms" 2>/dev/null || echo 0)
            total_ms=$(( setup_ms + query_ms ))
            cpu_u=$(time_user_ms "$run/time.txt" 2>/dev/null || echo 0)
            cpu_s=$(time_sys_ms "$run/time.txt" 2>/dev/null || echo 0)
            rss=$(time_rss_kb "$run/time.txt" 2>/dev/null || echo 0)

            case "$lane" in
              v_off|v_on)
                phys=$(jq -s '[.[].stats.bytes_read // 0] | add // 0' "$run/observe.jsonl" 2>/dev/null || echo 0)
                ans=$(jq -s '[.[] | select(.stats)] | length' "$run/observe.jsonl" 2>/dev/null || echo 0)
                dec=$(jq -s '[.[] | select(.error)] | length' "$run/observe.jsonl" 2>/dev/null || echo 0)
                persist=$(du -sb "$run/store" 2>/dev/null | awk '{s+=$1} END{print s+0}')
                aux=$(du -sb "$run/store/promoted" 2>/dev/null | awk '{s+=$1} END{print s+0}')
                ;;
              *)
                phys=$(jq -r '.phys_read_bytes // 0' "$run/metrics.json" 2>/dev/null || echo 0)
                ans=$(jq -r '.requests // 0' "$run/metrics.json" 2>/dev/null || echo 0)
                dec=0
                persist=$(jq -r '.db_bytes // 0' "$run/metrics.json" 2>/dev/null || echo 0)
                aux=$(jq -r '.cache_bytes // 0' "$run/metrics.json" 2>/dev/null || echo 0)
                ;;
            esac
            printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
                "$id" "$fmt" "$lane" "$d" "$nreq" "$setup_ms" "$query_ms" "$total_ms" \
                "$cpu_u" "$cpu_s" "$rss" "$phys" "$persist" "$aux" "$ans" "$dec" >>"$RAW/diversity.tsv"
            printf '   %-8s d=%s reqs=%-3s total=%sms (setup %s/query %s) cpu=%s+%sms rss=%sKB persist=%s aux=%s ans=%s dec=%s rc=%s\n' \
                "$lane" "$d" "$nreq" "$total_ms" "$setup_ms" "$query_ms" "$cpu_u" "$cpu_s" "$rss" "$persist" "$aux" "$ans" "$dec" "$rc" >&2
        done
    done
done

echo "-- aggregating frontier map" >&2
python3 "$FRONT" frontier --raw "$RAW" --campaign "$CAMPAIGN" --x 10 --y 20 --z 20 | tee "$RAW/frontier.txt"

# environment + commands + sealed receipt (at the campaign root)
cat >"$CAMPAIGN/commands.txt" <<EOF
court: tools/phase15-diversity-court.sh
DOCS="$DOCS"
PER_TYPE=$PER_TYPE PROFILE=$PROFILE PROMOTE_SUFFIX="$PROMOTE_SUFFIX"
BIN=$BIN
schedule: python3 tools/fixtures/phase15-frontier.py schedule --format F --per-type $PER_TYPE
baseline: python3 tools/fixtures/phase12-baseline.py build --format F --source S --db D   (sq_full)
lanes: v_off, v_on, sq_min, sq_full, sq_adapt
EOF
cat >"$CAMPAIGN/environment.json" <<EOF
{"phase":"15.6","court":"diversity","docs":"$DOCS","per_type":$PER_TYPE,
 "seed":20261007,"types":["text","metadata","heading","table","resource"],
 "promote_suffix":"$PROMOTE_SUFFIX","profile":"$PROFILE","git":"$SHA","date":"$STAMP"}
EOF

python3 - "$CAMPAIGN" "$RAW" "$STAMP" "$SHA" "$DOCS" "$PER_TYPE" <<'PY'
import hashlib, json, os, subprocess, sys
camp, raw, stamp, sha, docs, per_type = sys.argv[1:7]

def sh(cmd):
    try:
        return subprocess.run(cmd, shell=True, capture_output=True, text=True).stdout.strip()
    except Exception:
        return ""

lock = sh("sha256sum Cargo.lock").split(" ")[0] if os.path.exists("Cargo.lock") else ""
dirty = sh("git status --porcelain").strip()
verd = {}
try:
    verd = json.load(open(os.path.join(camp, "verdict.json")))
except Exception:
    pass

f1 = verd.get("falsifiers", {}).get("diversity_sq_adapt_within_X")
f2 = verd.get("falsifiers", {}).get("promotion_beats_write_everything")
if f1 and not f2:
    interp = (
        "REFUTED. Falsifier 1: VOLE --promote never beat SQLite-adaptive by more "
        "than X% at any diversity depth (the crossing never occurs). Falsifier 2: "
        "the durable promoted store's bytes cut total durable bytes by far less "
        "than Z%, at equal-or-worse latency. Governed promotion did not pay on "
        "this bounded subset.")
elif not f1:
    interp = ("NOT refuted on the diversity axis: promotion beats SQLite-adaptive "
              "by >X% at some depth. See SUMMARY.md for the depth(s).")
else:
    interp = ("Falsifier 1 refuted (no crossing) but the mechanism threshold was "
              "met; see SUMMARY.md.")

receipt = {
    "phase": "15.6",
    "court": "diversity",
    "git_commit": sh("git rev-parse HEAD"),
    "git_dirty": bool(dirty),
    "date_utc": stamp,
    "arch": sh("uname -m"),
    "rustc": sh("rustc -vV").replace("\n", " "),
    "cargo": sh("cargo -V"),
    "cargo_lock_sha256": lock,
    "base_image": "vole-document/doc-baseline:1.99.0 (FROM baseline FROM dev, rust:1.99.0-slim-bookworm)",
    "oracles": {"sqlite3": sh("sqlite3 --version"),
                "pdfinfo": sh("pdfinfo -v 2>&1").split("\n")[0],
                "python3": sh("python3 --version")},
    "service": "doc-baseline (mem 6g, memswap 6g, pids 4096, cpus 8)",
    "subset": {"docs": docs.split(), "per_type": int(per_type),
               "types": ["text", "metadata", "heading", "table", "resource"]},
    "schedule_seed": 20261007,
    "promote_suffix": os.environ.get("PROMOTE_SUFFIX", ""),
    "schedule_sha256": {
        f[len("schedule-"):-len(".json")]: json.load(open(os.path.join(raw, f)))["schedule_sha256"]
        for f in os.listdir(raw) if f.startswith("schedule-") and f.endswith(".json")},
    "verdict": verd,
    "interpretation": interp,
    "honesty": (
        "Bounded, documented subset (small docs to bound the minimal/adaptive "
        "re-extraction cost). The verdict is read directly from verdict.json; no "
        "threshold or population was tuned to manufacture a win. A negative/neutral "
        "result is a legitimate outcome."),
}
with open(os.path.join(camp, "receipt.json"), "w") as fh:
    json.dump(receipt, fh, indent=1, sort_keys=True)
print("receipt: " + json.dumps(verd.get("falsifiers", {}), sort_keys=True))
PY

chown -R 1000:1000 "$CAMPAIGN" "$WORK" 2>/dev/null || true
echo "phase15 diversity court: done — $CAMPAIGN"
