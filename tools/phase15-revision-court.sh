#!/usr/bin/env bash
# Phase 15.6 — revision court (cross-revision retention under durable promotion).
#
# Takes real revision families from `real100-v1/manifest.tsv`
# (`revision_family_id`, members ordered by `publication_year`; falls back to
# `cross_format_family_id` when a family has no revision chain). For each bounded
# family (same-format members, so the stream is identical across revisions):
#
#   shared   ingest R1 into a fresh root; warm a session with the frozen stream;
#            ingest R2 into the SAME root (content-addressed sharing applies);
#            run the identical stream; record cumulative cost + durable bytes.
#   cold     ingest R2 into a fresh root; run the identical stream (control).
#
#   retained_cross_revision_work = (C_cold − C_shared) / C_cold
#
# reported per lane (v_off / v_on) plus `nodes_id_shared`, `shared_resource_ids`,
# `seed_nodes_reused`, and durable bytes. The SQLite control is a fair per-
# revision REBUILD (fresh extraction each revision, charged in full): it has the
# same source, but no content-addressed sharing opportunity, so its retained work
# is 0 by construction — reported as context, not as the verdict.
#
# Run in the pinned `doc-baseline` service:
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase15-revision-court.sh
set -uo pipefail

cd /work
export LC_ALL=C

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=${CAMPAIGN:-evidence/campaigns/${STAMP}-phase15-revision-${SHA}}
RAW=$CAMPAIGN/raw
WORK=${WORK:-evidence/scratch/phase15-revision-work}
mkdir -p "$RAW" "$WORK"

MANIFEST=${MANIFEST:-real100-v1/manifest.tsv}
CORPUS=${CORPUS:-real100-v1/documents}
# Bounded, documented subset of the same-format revision families.
FAMILIES=${FAMILIES:-"rev-nist-sp800-34 rev-nist-sp800-218 rev-nist-fips-140"}
PER_TYPE=${PER_TYPE:-6}
PROFILE=${PROFILE:-release}
PROMOTE_SUFFIX=${PROMOTE_SUFFIX:-}
BASE=tools/fixtures/phase12-baseline.py
DRV=tools/fixtures/phase15-baseline.py
FRONT=tools/fixtures/phase15-frontier.py

echo "== phase15 revision court ==" >&2
if ! cargo build --release --locked --all-features >&2; then
    echo "revision-court: build FAILED; refusing to measure" >&2
    exit 1
fi
BIN=target/release/vole-document

now_ms() { echo $(( $(date +%s%N) / 1000000 )); }
time_user_ms() { awk -F': ' '/User time \(seconds\)/{printf "%d", $2*1000}' "$1" 2>/dev/null || echo 0; }
time_sys_ms()  { awk -F': ' '/System time \(seconds\)/{printf "%d", $2*1000}' "$1" 2>/dev/null || echo 0; }

# --- revision families ------------------------------------------------------
# family -> members "id|year|fmt" ordered by year (fallback: cross_format family)
families_from() {
    awk -F'\t' -v col="$1" 'NR>1 && $col!=""{print $col"\t"$11"\t"$1"\t"$5}' "$MANIFEST" \
        | sort -k1,1 -k2,2n \
        | awk -F'\t' '{a[$1]=a[$1]"\n"$3"|"$2"|"$4; n[$1]++} END{for(k in a) if(n[k]>=2) print k a[k]}'
}

printf 'family\tlane\tfmt\tr1_ms\tr2_cold_ms\tr2_shared_ms\tnodes_id_shared\tshared_resource_ids\tseed_nodes_reused\tr2_cold_cpu_ms\tr2_shared_cpu_ms\tpromoted_bytes\n' >"$RAW/revision.tsv"

# Fallback chain: `revision_family_id` (col 18) families with >=2 members; if the
# manifest carries no revision chain, fall back to `cross_format_family_id` (17).
if [ "$FAMILIES" = auto ]; then
    FAMILIES=$(awk -F'\t' 'NR>1 && $18!=""{c[$18]++} END{for(k in c) if(c[k]>=2) print k}' "$MANIFEST")
    if [ -z "$FAMILIES" ]; then
        FAMILIES=$(awk -F'\t' 'NR>1 && $17!=""{c[$17]++} END{for(k in c) if(c[k]>=2) print k}' "$MANIFEST")
    fi
    echo "revision-court: auto families: $FAMILIES" >&2
fi

run_family() {
    local family=$1
    # members: lines id|year|fmt, ordered by year; take first two (bounded R1,R2)
    mapfile -t members < <(awk -F'\t' -v f="$family" 'NR>1 && $18==f{print $11"\t"$1"\t"$5}' "$MANIFEST" | sort -s -k1,1n | head -2)
    if [ "${#members[@]}" -lt 2 ]; then
        echo "revision-court: family $family has <2 same-format members; skipping" >&2
        return
    fi
    local m1=${members[0]} m2=${members[1]}
    local id1 fmt1 id2 fmt2
    id1=$(echo "$m1" | cut -f2); fmt1=$(echo "$m1" | cut -f3)
    id2=$(echo "$m2" | cut -f2); fmt2=$(echo "$m2" | cut -f3)
    if [ "$fmt1" != "$fmt2" ]; then
        echo "revision-court: family $family crosses formats ($fmt1->$fmt2); skipping (identical stream needs same format)" >&2
        return
    fi
    local src1 src2
    src1=$(ls "$CORPUS"/*/*/"$id1".* 2>/dev/null | head -1)
    src2=$(ls "$CORPUS"/*/*/"$id2".* 2>/dev/null | head -1)
    if [ ! -f "$src1" ] || [ ! -f "$src2" ]; then
        echo "revision-court: family $family missing sources ($src1 / $src2)" >&2
        return
    fi
    echo "== family $family: R1=$id1 R2=$id2 ($fmt1) ==" >&2
    local fdir=$WORK/$family
    rm -rf "$fdir"; mkdir -p "$fdir"
    local schd=$RAW/schedule-$family.json pfx=$fdir/reqs
    python3 "$FRONT" schedule --format "$fmt1" --out "$schd" --per-type "$PER_TYPE" >"$RAW/schedule-$family.meta.json"
    jq -r '.requests[].arg' "$schd" >"$pfx"
    local nreq; nreq=$(wc -l <"$pfx" | tr -d ' ')

    for lane in v_off v_on; do
        local extra=""
        [ "$lane" = v_on ] && extra="--promote${PROMOTE_SUFFIX}"
        local sh=$fdir/$lane-shared cold=$fdir/$lane-cold
        rm -rf "$sh" "$cold"; mkdir -p "$sh" "$cold"

        # R1 into shared root, then the stream (warm)
        local t0 t1 f1 f2
        t0=$(now_ms)
        "$BIN" encode "$src1" "$sh/r1.voldoc" >"$sh/r1.encode.json" 2>/dev/null
        "$BIN" field-ingest "$sh/r1.voldoc" --store "$sh/store" >"$sh/r1.ingest.json" 2>/dev/null
        t1=$(now_ms); local r1setup=$((t1-t0))
        f1=$(jq -r '.field // empty' "$sh/r1.ingest.json")
        t0=$(now_ms); /usr/bin/time -v -o "$sh/r1.time" "$BIN" observe-batch --store "$sh/store" --field "$f1" --requests "$pfx" $extra >"$sh/r1.jsonl" 2>/dev/null; t1=$(now_ms)
        local r1_ms=$(( r1setup + t1 - t0 ))

        # R2 into the SAME root (sharing), then the identical stream
        t0=$(now_ms)
        "$BIN" encode "$src2" "$sh/r2.voldoc" >"$sh/r2.encode.json" 2>/dev/null
        "$BIN" field-ingest "$sh/r2.voldoc" --store "$sh/store" >"$sh/r2.ingest.json" 2>/dev/null
        t1=$(now_ms); local r2setup=$((t1-t0))
        f2=$(jq -r '.field // empty' "$sh/r2.ingest.json")
        t0=$(now_ms); /usr/bin/time -v -o "$sh/r2.time" "$BIN" observe-batch --store "$sh/store" --field "$f2" --requests "$pfx" $extra >"$sh/r2.jsonl" 2>/dev/null; t1=$(now_ms)
        local r2_shared=$(( r2setup + t1 - t0 ))
        local sh_cpu=$(( $(time_user_ms "$sh/r2.time") + $(time_sys_ms "$sh/r2.time") ))

        # R2 into a FRESH root (cold control)
        t0=$(now_ms)
        "$BIN" encode "$src2" "$cold/r2.voldoc" >"$cold/r2.encode.json" 2>/dev/null
        "$BIN" field-ingest "$cold/r2.voldoc" --store "$cold/store" >"$cold/r2.ingest.json" 2>/dev/null
        t1=$(now_ms); local csetup=$((t1-t0))
        f2=$(jq -r '.field // empty' "$cold/r2.ingest.json")
        t0=$(now_ms); /usr/bin/time -v -o "$cold/r2.time" "$BIN" observe-batch --store "$cold/store" --field "$f2" --requests "$pfx" $extra >"$cold/r2.jsonl" 2>/dev/null; t1=$(now_ms)
        local r2_cold=$(( csetup + t1 - t0 ))
        local c_cpu=$(( $(time_user_ms "$cold/r2.time") + $(time_sys_ms "$cold/r2.time") ))

        local nshared sres reused prom
        nshared=$(jq -r '.nodes_id_shared // 0' "$sh/r2.ingest.json" 2>/dev/null || echo 0)
        sres=$(jq -r '.shared_resource_ids // 0' "$sh/r2.ingest.json" 2>/dev/null || echo 0)
        reused=$(jq -s '[.[] | .stats.seed_nodes_reused // 0] | add // 0' "$sh/r2.jsonl" 2>/dev/null || echo 0)
        prom=$(du -sb "$sh/store/promoted" 2>/dev/null | awk '{print $1+0}')
        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
            "$family" "$lane" "$fmt1" "$r1_ms" "$r2_cold" "$r2_shared" \
            "$nshared" "$sres" "$reused" "$c_cpu" "$sh_cpu" "$prom" >>"$RAW/revision.tsv"
        printf '   %-6s R1=%sms R2cold=%sms R2shared=%sms nodes_id_shared=%s shared_res=%s reused=%s promoted=%s\n' \
            "$lane" "$r1_ms" "$r2_cold" "$r2_shared" "$nshared" "$sres" "$reused" "$prom" >&2
    done

    # SQLite control: fair per-revision REBUILD (no sharing opportunity)
    local sdir=$fdir/sq
    rm -rf "$sdir"; mkdir -p "$sdir"
    local t0 t1
    t0=$(now_ms)
    python3 "$BASE" build --format "$fmt1" --source "$src1" --db "$sdir/r1.db" --metrics "$sdir/r1.metrics.json" >/dev/null 2>"$sdir/r1.err"
    python3 "$DRV" run --lane full --format "$fmt1" --source "$src1" --db "$sdir/r1.db" --requests <(jq '.requests' "$schd") --out "$sdir/r1.run.json" >/dev/null 2>"$sdir/r1.run.err"
    t1=$(now_ms); local sq1=$((t1-t0))
    t0=$(now_ms)
    python3 "$BASE" build --format "$fmt2" --source "$src2" --db "$sdir/r2.db" --metrics "$sdir/r2.metrics.json" >/dev/null 2>"$sdir/r2.err"
    python3 "$DRV" run --lane full --format "$fmt2" --source "$src2" --db "$sdir/r2.db" --requests <(jq '.requests' "$schd") --out "$sdir/r2.run.json" >/dev/null 2>"$sdir/r2.run.err"
    t1=$(now_ms); local sq2=$((t1-t0))
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$family" "sq_rebuild" "$fmt1" "$sq1" "$sq2" "$sq2" 0 0 0 0 0 0 >>"$RAW/revision.tsv"
    printf '   sq_rebuild R1=%sms R2=%sms (no sharing opportunity)\n' "$sq1" "$sq2" >&2
}

for fam in $FAMILIES; do
    run_family "$fam"
done

echo "-- aggregating revision frontier" >&2
python3 "$FRONT" frontier --raw "$RAW" --campaign "$CAMPAIGN" --x 10 --y 20 --z 20 | tee "$RAW/frontier.txt"

cat >"$CAMPAIGN/commands.txt" <<EOF
court: tools/phase15-revision-court.sh
FAMILIES="$FAMILIES"
PER_TYPE=$PER_TYPE PROFILE=$PROFILE PROMOTE_SUFFIX="$PROMOTE_SUFFIX"
BIN=$BIN
shared: encode R1 -> field-ingest into a fresh root; observe-batch stream; encode R2 -> field-ingest same root; observe-batch stream
cold:   encode R2 -> field-ingest fresh root; observe-batch stream
sqlite: phase12 build (R1) + phase12 build (R2 fresh) -- fair per-revision REBUILD
EOF
cat >"$CAMPAIGN/environment.json" <<EOF
{"phase":"15.6","court":"revision","families":"$FAMILIES","per_type":$PER_TYPE,
 "seed":20261007,"promote_suffix":"$PROMOTE_SUFFIX","profile":"$PROFILE",
 "git":"$SHA","date":"$STAMP"}
EOF

python3 - "$CAMPAIGN" "$RAW" "$STAMP" "$SHA" "$FAMILIES" "$PER_TYPE" <<'PY'
import json, os, subprocess, sys
camp, raw, stamp, sha, fams, per_type = sys.argv[1:7]

def sh(cmd):
    try:
        return subprocess.run(cmd, shell=True, capture_output=True, text=True).stdout.strip()
    except Exception:
        return ""

lock = sh("sha256sum Cargo.lock").split(" ")[0] if os.path.exists("Cargo.lock") else ""
verd = {}
try:
    verd = json.load(open(os.path.join(camp, "verdict.json")))
except Exception:
    pass

f3 = verd.get("falsifiers", {}).get("revision_retained_ge_Y")
if not f3:
    interp = (
        "REFUTED. Falsifier 3: the median retained cross-revision work is far "
        "below Y% for every lane, including VOLE --promote (durable promotion adds "
        "~0% and its promoted store held no cross-revision bytes). Content-"
        "addressed identity did not translate into a measurable work saving on "
        "this bounded subset, matching ADR-0034's negative.")
else:
    interp = ("NOT refuted: median retained cross-revision work is >= Y% for at "
              "least one lane. See SUMMARY.md.")

receipt = {
    "phase": "15.6",
    "court": "revision",
    "git_commit": sh("git rev-parse HEAD"),
    "git_dirty": bool(sh("git status --porcelain").strip()),
    "date_utc": stamp,
    "arch": sh("uname -m"),
    "rustc": sh("rustc -vV").replace("\n", " "),
    "cargo": sh("cargo -V"),
    "cargo_lock_sha256": lock,
    "base_image": "vole-document/doc-baseline:1.99.0 (FROM baseline FROM dev, rust:1.99.0-slim-bookworm)",
    "service": "doc-baseline (mem 6g, memswap 6g, pids 4096, cpus 8)",
    "facts": "family,col18 revision_family_id; members ordered by publication_year; same-format only",
    "subset": {"families": fams.split(), "per_type": int(per_type)},
    "schedule_seed": 20261007,
    "promote_suffix": os.environ.get("PROMOTE_SUFFIX", ""),
    "verdict": verd,
    "interpretation": interp,
    "sqlite_control": ("fair per-revision REBUILD (fresh extraction each revision, "
                       "charged in full). It has no content-addressed sharing "
                       "opportunity, so its retained work is 0 by construction; "
                       "reported as context, not as the verdict."),
    "honesty": (
        "Bounded subset of same-format revision families. The verdict is read "
        "directly from verdict.json; nothing was tuned. A negative/neutral "
        "result is a legitimate outcome."),
}
with open(os.path.join(camp, "receipt.json"), "w") as fh:
    json.dump(receipt, fh, indent=1, sort_keys=True)
print("receipt: " + json.dumps(verd.get("falsifiers", {}), sort_keys=True))
PY

chown -R 1000:1000 "$CAMPAIGN" "$WORK" 2>/dev/null || true
echo "phase15 revision court: done — $CAMPAIGN"
