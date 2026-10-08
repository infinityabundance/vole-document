#!/usr/bin/env bash
# Phase 16 item 4: the surgical residency experiment on real100-v1.
#
# This is a TRIMMED COPY of the frozen `tools/real100-court.sh`. It keeps the
# same 100-document population, the same encode + field-ingest one-time lane, and
# the exact cold-vs-resident `text_repeat` semantics (`v` = REPEAT_N separate
# cold processes; `v_r` = ONE `observe-batch` process doing REPEAT_N observations)
# so its per-size-class numbers are directly comparable to
# `evidence/campaigns/2026-10-07-real100-release-resident-78f7ea8/`.
#
# It drops the lanes irrelevant to the residency question (A0 direct tooling, A1
# SQLite/FTS builds+queries, the per-op heading/table/resource/metadata rows, the
# exact-reconstruction lane, the informational `session_mixed` row) purely to fit
# the remeasurement in one bounded run. The frozen court files are NOT modified:
# `tools/real100-court.sh`, `tools/fixtures/real100-frontier.py`, and `real100-v1/`
# are untouched, and the raw ops.tsv schema is preserved.
#
# Run in the pinned `doc-baseline` service:
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase16-resident-probe-court.sh
set -uo pipefail

cd /work
export LC_ALL=C

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=evidence/campaigns/${STAMP}-real100-${TAG:-resident-probe}-${SHA}
RAW=$CAMPAIGN/raw
WORK=${WORK:-evidence/scratch/real100-resident-probe-work}
mkdir -p "$RAW" "$WORK"

MANIFEST=${MANIFEST:-real100-v1/manifest.tsv}
CORPUS=${CORPUS:-real100-v1/documents}
SCHEDULE=$RAW/schedule.json
OP_TIMEOUT=${OP_TIMEOUT:-180}
REPEAT_N=${REPEAT_N:-5}
LIMIT=${LIMIT:-0}   # 0 = the full frozen population; >0 caps docs (smoke / bounded run)

PROFILE=${PROFILE:-release}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-${VOLE_BIN:-target/debug/vole-document}} ;;
esac

echo "== phase16 resident-probe court ==" >&2
echo "-- building ($PROFILE) binary" >&2
# shellcheck disable=SC2086
if ! cargo build $BUILD_ARGS >&2; then
    echo "phase16-resident-probe-court: build FAILED ($BUILD_ARGS); refusing to measure" >&2
    exit 1
fi
PREFLIGHT=""
for cand in real100-v1/documents/nist/docx/*.docx real100-v1/documents/nasa/epub/*.epub; do
    [ -f "$cand" ] && { PREFLIGHT="$cand"; break; }
done
if [ -n "$PREFLIGHT" ]; then
    expect=$(case "$PREFLIGHT" in *.docx) echo docx ;; *) echo epub ;; esac)
    if ! "$BIN" capabilities "$PREFLIGHT" 2>/dev/null | grep -q "\"adapter\":\"$expect\""; then
        echo "phase16-resident-probe-court: preflight failed — $BIN does not support $expect; refusing to measure" >&2
        exit 1
    fi
fi

echo "-- verifying the frozen corpus (SHA-256 + length + format)" >&2
if ! sh tools/realcorpus/verify.sh --corpus real100-v1 >"$RAW/verify.txt" 2>&1; then
    echo "phase16-resident-probe-court: corpus verification FAILED; refusing to measure" >&2
    tail -20 "$RAW/verify.txt" >&2
    exit 1
fi

python3 tools/fixtures/real100-schedule.py "$MANIFEST" "$CORPUS" "$SCHEDULE" >"$RAW/schedule.meta.json" 2>"$RAW/schedule.err"

now_ms() { echo $(( ${EPOCHREALTIME/./} / 1000 )); }

v_argv() { # fmt workload -> echo a VOLE argv, or DECLINE
  case "$1:$2" in
    pdf:text_once) echo "observe --page 1 --kind text" ;;
    docx:text_once|epub:text_once) echo "observe --block 0 --kind text" ;;
    *:metadata) echo "observe --metadata --kind metadata" ;;
    *) echo "DECLINE" ;;
  esac
}

RC=0; WALL=0
run_v() { # d fmt workload
  local d=$1 fmt=$2 w=$3 argv t0 t1
  argv=$(v_argv "$fmt" "$w")
  if [ "$argv" = DECLINE ]; then RC=3; WALL=0; return; fi
  t0=$(now_ms)
  # shellcheck disable=SC2086
  timeout "$OP_TIMEOUT" "$BIN" $argv --store "$d/vstore" --field "$(cat "$d/field")" >"$d/v.$w.json" 2>"$d/v.$w.err"
  RC=$?
  t1=$(now_ms); WALL=$(( t1 - t0 ))
}

submit() { # docid agency fmt sizeclass workload lane rc wall
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" "$5" "$6" "$7" "$8" >>"$RAW/ops.tsv"
}

printf 'id\tagency\tfmt\tsclass\tworkload\tlane\trc\twall_ms\n' >"$RAW/ops.tsv"
printf 'id\tagency\tfmt\tsclass\tblen\tvenc_rc\tvenc_ms\tving_rc\tving_ms\tv_desc_bytes\tv_store_bytes\tv_transient_bytes\n' >"$RAW/onetime.tsv"

while IFS=$'\t' read -r id agency fmt path sha blen sclass tags family; do
    d="$WORK/$id"
    rm -rf "$d"; mkdir -p "$d"
    src="$path"
    echo "== $id ($fmt, $sclass) ==" >&2
    if [ ! -f "$src" ]; then
        echo "   missing source $src" >&2
        submit "$id" "$agency" "$fmt" "$sclass" "MISSING" "none" 3 0
        continue
    fi

    # ---- one-time lane costs (identical to the frozen court) --------------
    t0=$(now_ms)
    timeout "$OP_TIMEOUT" "$BIN" encode "$src" "$d/v.voldoc" >"$d/encode.json" 2>"$d/encode.err"; venc_rc=$?
    t1=$(now_ms); venc=$(( t1-t0 ))
    field=""
    ving=-1; ving_rc=-1; v_ok=0
    if [ "$venc_rc" -eq 0 ]; then
        t0=$(now_ms)
        timeout "$OP_TIMEOUT" "$BIN" field-ingest "$d/v.voldoc" --store "$d/vstore" >"$d/ingest.json" 2>"$d/ingest.err"; ving_rc=$?
        t1=$(now_ms); ving=$(( t1-t0 ))
        if [ "$ving_rc" -eq 0 ]; then
            field=$(jq -r '.field // empty' "$d/ingest.json" 2>/dev/null)
            [ -n "$field" ] && { v_ok=1; printf '%s' "$field" >"$d/field"; }
        fi
    fi
    v_desc_bytes=$(du -sb "$d/v.voldoc" 2>/dev/null | awk '{print $1+0}')
    v_store_bytes=$(du -sb "$d/vstore" 2>/dev/null | awk '{print $1+0}')
    v_transient_bytes=$(( v_desc_bytes + v_store_bytes ))
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$id" "$agency" "$fmt" "$sclass" "$blen" "$venc_rc" "$venc" "$ving_rc" "$ving" \
        "$v_desc_bytes" "$v_store_bytes" "$v_transient_bytes" >>"$RAW/onetime.tsv"

    if [ "$v_ok" -ne 1 ]; then
        submit "$id" "$agency" "$fmt" "$sclass" "text_repeat" "v" 3 0
        submit "$id" "$agency" "$fmt" "$sclass" "text_repeat" "v_r" 3 0
        continue
    fi

    # ---- warm the derived cache exactly as the frozen court does: its per-op
    # loop runs `run_v text_once` once before the `text_repeat` lane. ----------
    run_v "$d" "$fmt" text_once

    # ---- repeated text (warm), cold: REPEAT_N separate processes -----------
    total=0; lastrc=0
    for ((k=1; k<=REPEAT_N; k++)); do
        run_v "$d" "$fmt" text_once
        lastrc=$RC; total=$(( total + WALL ))
    done
    submit "$id" "$agency" "$fmt" "$sclass" "text_repeat" "v" "$lastrc" "$total"

    # ---- resident session: the same REPEAT_N observations in ONE process ---
    rline=$(v_argv "$fmt" text_once)
    if [ "$rline" != DECLINE ]; then
        printf '%s\n' "${rline#observe }" >"$d/vr.repeat.reqs"
        t0=$(now_ms)
        timeout "$OP_TIMEOUT" "$BIN" observe-batch --store "$d/vstore" --field "$field" \
            --requests "$d/vr.repeat.reqs" --repeat "$REPEAT_N" >"$d/vr.repeat.jsonl" 2>"$d/vr.repeat.err"
        RC=$?; t1=$(now_ms)
        submit "$id" "$agency" "$fmt" "$sclass" "text_repeat" "v_r" "$RC" "$(( t1-t0 ))"
    else
        submit "$id" "$agency" "$fmt" "$sclass" "text_repeat" "v_r" 3 0
    fi
done < <(jq -r --argjson lim "$LIMIT" '([.documents[]] | (if $lim > 0 then .[:$lim] else . end))[] | [.id,.agency,.format,.path,.sha256,(.byte_len|tostring),.size_class,.structural_tags,.cross_format_family_id] | @tsv' "$SCHEDULE")

echo "phase16 resident-probe court: done — $CAMPAIGN" >&2
