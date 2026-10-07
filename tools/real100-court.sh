#!/usr/bin/env bash
# real100-v1 frontier court (Phase 13 / corpus).
#
# Runs the FROZEN architecture over the FROZEN 100-document NASA/NIST corpus and
# maps, per workload stratum, which lane wins:
#
#   V  = the Phase-11/12 procedural DocumentField (as frozen; auto encoder)
#   A1 = one-time-preprocessed, source-retaining SQLite + FTS5 (12.11 lane)
#   A0 = direct per-query tooling (Poppler/pdfinfo; stdlib zipfile + XML)
#
# It does NOT assert content (real documents have unknown content): it records,
# per (document, workload, lane), whether the lane ANSWERED or DECLINED and its
# wall cost, then aggregates a frontier map. The corpus and the op schedule are
# pre-registered and frozen; no VOLE result feeds selection or tuning.
#
# Resource bounds (mandatory): every lane is a capped compose service; each
# document has a wall budget PERDOC and each op a shorter OP_TIMEOUT, both
# recorded as timeouts in the raw table rather than silently dropped.
#
# Run in the pinned `doc-baseline` service:
#   docker compose run --rm --no-TTY doc-baseline bash tools/real100-court.sh
set -uo pipefail

cd /work
export LC_ALL=C

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=evidence/campaigns/${STAMP}-real100-${TAG:-frontier}-${SHA}
RAW=$CAMPAIGN/raw
WORK=${WORK:-evidence/scratch/real100-frontier-work}
mkdir -p "$RAW" "$WORK"

MANIFEST=${MANIFEST:-real100-v1/manifest.tsv}
CORPUS=${CORPUS:-real100-v1/documents}
SCHEDULE=$RAW/schedule.json
BIN=${VOLE_BIN:-}
BASE=tools/fixtures/phase12-baseline.py
OP_TIMEOUT=${OP_TIMEOUT:-180}
REPEAT_N=${REPEAT_N:-5}
LIMIT=${LIMIT:-0}   # 0 = the full frozen population; >0 caps docs (smoke / bounded run)
# Build profile. Phase 15 repairs the court: the real100 numbers were taken on an
# unoptimized debug binary. `PROFILE=release` builds `--release` and uses
# `target/release/vole-document`.
PROFILE=${PROFILE:-debug}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; DEFAULT_BIN=target/release/vole-document ;;
    *)       BUILD_ARGS="--locked --all-features";           DEFAULT_BIN=target/debug/vole-document ;;
esac
BIN=${BIN:-$DEFAULT_BIN}

echo "== real100 frontier court ==" >&2
echo "-- building ($PROFILE) binary" >&2
# shellcheck disable=SC2086
cargo build $BUILD_ARGS >&2

echo "-- verifying the frozen corpus (SHA-256 + length + format)" >&2
if ! sh tools/realcorpus/verify.sh --corpus real100-v1 >"$RAW/verify.txt" 2>&1; then
    echo "real100-court: corpus verification FAILED; refusing to measure" >&2
    tail -20 "$RAW/verify.txt" >&2
    exit 1
fi

python3 tools/fixtures/real100-schedule.py "$MANIFEST" "$CORPUS" "$SCHEDULE" >"$RAW/schedule.meta.json" 2>"$RAW/schedule.err"

now_ms() { echo $(( ${EPOCHREALTIME/./} / 1000 )); }

# --- lane command builders -------------------------------------------------
v_argv() { # fmt workload -> echo a VOLE argv, or DECLINE
  case "$1:$2" in
    pdf:text_once) echo "observe --page 1 --kind text" ;;
    docx:text_once|epub:text_once) echo "observe --block 0 --kind text" ;;
    docx:heading|epub:heading) echo "observe --heading 0 --kind text" ;;
    docx:table|epub:table) echo "observe --table 0 --kind text" ;;
    docx:resource|epub:resource) echo "observe --resource 0 --kind metadata" ;;
    *:metadata) echo "observe --metadata --kind metadata" ;;
    *) echo "DECLINE" ;;
  esac
}
a0_argv() { # fmt workload -> PY <case> <arg> | PDFTOTEXT | PDFINFO | DECLINE
  case "$1:$2" in
    pdf:text_once) echo "PDFTOTEXT" ;;
    docx:text_once|epub:text_once) echo "PY block 0" ;;
    docx:heading|epub:heading) echo "PY heading 0" ;;
    docx:table|epub:table) echo "PY table 0" ;;
    docx:resource|epub:resource) echo "PY resource-meta 0" ;;
    pdf:metadata) echo "PDFINFO" ;;
    *:metadata) echo "PY metadata -" ;;
    *) echo "DECLINE" ;;
  esac
}
a1_sql() { # fmt workload -> SQL | DECLINE
  case "$1:$2" in
    pdf:heading|pdf:table|pdf:resource) echo "DECLINE" ;;
    pdf:text_once) echo "SELECT text FROM blocks WHERE doc_id=1 AND unit='page' AND ordering=1;" ;;
    docx:text_once|epub:text_once) echo "SELECT text FROM blocks WHERE doc_id=1 AND block_id=1;" ;;
    *:heading) echo "SELECT b.text FROM blocks b JOIN headings h ON h.block_id=b.block_id WHERE h.doc_id=1 ORDER BY h.heading_id LIMIT 1;" ;;
    *:table) echo "SELECT text FROM blocks WHERE doc_id=1 AND kind='table' ORDER BY block_id LIMIT 1;" ;;
    *:resource) echo "SELECT path||' '||member_bytes FROM resources WHERE doc_id=1 AND resource_id=1;" ;;
    *:metadata) echo "SELECT group_concat(key||'='||value,';') FROM metadata WHERE doc_id=1;" ;;
    *) echo "DECLINE" ;;
  esac
}

# --- per-lane op runner: sets RC and WALL (ms) -----------------------------
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
run_a0() { # d fmt workload src
  local d=$1 fmt=$2 w=$3 src=$4 arg t0 t1
  arg=$(a0_argv "$fmt" "$w")
  case "$arg" in
    DECLINE) RC=3; WALL=0; return ;;
    PDFTOTEXT) t0=$(now_ms); timeout "$OP_TIMEOUT" pdftotext -f 1 -l 1 "$src" "$d/a0.$w" 2>"$d/a0.$w.err"; RC=$?; t1=$(now_ms); WALL=$(( t1-t0 )); return ;;
    PDFINFO)   t0=$(now_ms); timeout "$OP_TIMEOUT" pdfinfo "$src" >"$d/a0.$w" 2>"$d/a0.$w.err"; RC=$?; t1=$(now_ms); WALL=$(( t1-t0 )); return ;;
    PY*) # shellcheck disable=SC2086
         set -- $arg; t0=$(now_ms); timeout "$OP_TIMEOUT" python3 "$BASE" query --format "$fmt" --source "$src" --case "$2" --arg "$3" --out "$d/a0.$w" >"$d/a0.$w.metrics" 2>"$d/a0.$w.err"; RC=$?; t1=$(now_ms); WALL=$(( t1-t0 )); return ;;
  esac
}
run_a1() { # d fmt workload
  local d=$1 fmt=$2 w=$3 sql t0 t1
  sql=$(a1_sql "$fmt" "$w")
  if [ "$sql" = DECLINE ]; then RC=3; WALL=0; return; fi
  t0=$(now_ms)
  timeout "$OP_TIMEOUT" sqlite3 "$d/a1.db" "$sql" >"$d/a1.$w" 2>"$d/a1.$w.err"
  RC=$?
  t1=$(now_ms); WALL=$(( t1 - t0 ))
}

submit() { # docid agency fmt sizeclass workload lane rc wall
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" "$5" "$6" "$7" "$8" >>"$RAW/ops.tsv"
}

printf 'id\tagency\tfmt\tsclass\tworkload\tlane\trc\twall_ms\n' >"$RAW/ops.tsv"
printf 'id\tagency\tfmt\tsclass\tblen\tvenc_rc\tvenc_ms\tving_rc\tving_ms\tv_desc_bytes\tv_store_bytes\tv_transient_bytes\ta1_build_rc\ta1_build_ms\ta1_bytes\n' >"$RAW/onetime.tsv"
printf 'id\tagency\tfmt\tsclass\tv_ok\tv_rc\tv_wall\ta1_ok\ta1_rc\ta1_wall\ta0_ok\ta0_wall\n' >"$RAW/exact.tsv"

# --- main loop -------------------------------------------------------------
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

    # ---- one-time lane costs ---------------------------------------------
    t0=$(now_ms)
    timeout "$OP_TIMEOUT" "$BIN" encode "$src" "$d/v.voldoc" >"$d/encode.json" 2>"$d/encode.err"; venc_rc=$?
    t1=$(now_ms); venc=$(( t1-t0 ))
    field=""
    ving=-1; v_ok=0
    if [ "$venc_rc" -eq 0 ]; then
        t0=$(now_ms)
        timeout "$OP_TIMEOUT" "$BIN" field-ingest "$d/v.voldoc" --store "$d/vstore" >"$d/ingest.json" 2>"$d/ingest.err"; ving_rc=$?
        t1=$(now_ms); ving=$(( t1-t0 ))
        if [ "$ving_rc" -eq 0 ]; then
            field=$(jq -r '.field // empty' "$d/ingest.json" 2>/dev/null)
            [ -n "$field" ] && { v_ok=1; printf '%s' "$field" >"$d/field"; }
        fi
    fi
    t0=$(now_ms)
    timeout "$OP_TIMEOUT" python3 "$BASE" build --format "$fmt" --source "$src" --db "$d/a1.db" --metrics "$d/a1.build.metrics.json" >"$d/a1.build.json" 2>"$d/a1.build.err"; a1b_rc=$?
    t1=$(now_ms); a1b=$(( t1-t0 ))

    # Storage universes (ADR-0027 extended): the persistent footprint the field
    # needs after `field-ingest` (the store alone — the standalone `.voldoc` may be
    # deleted), the optional standalone descriptor, and the transient ingest total.
    v_desc_bytes=$(du -sb "$d/v.voldoc" 2>/dev/null | awk '{print $1+0}')
    v_store_bytes=$(du -sb "$d/vstore" 2>/dev/null | awk '{print $1+0}')
    v_transient_bytes=$(( v_desc_bytes + v_store_bytes ))
    a1bytes=$(du -sb "$d/a1.db" 2>/dev/null | awk '{s+=$1} END{print s+0}')
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$id" "$agency" "$fmt" "$sclass" "$blen" "$venc_rc" "$venc" "$ving_rc" "$ving" \
        "$v_desc_bytes" "$v_store_bytes" "$v_transient_bytes" "$a1b_rc" "$a1b" "$a1bytes" >>"$RAW/onetime.tsv"

    # ---- per-op lanes -----------------------------------------------------
    for w in text_once heading table resource metadata; do
        run_v "$d" "$fmt" "$w";   submit "$id" "$agency" "$fmt" "$sclass" "$w" v "$RC" "$WALL"
        run_a0 "$d" "$fmt" "$w" "$src"; submit "$id" "$agency" "$fmt" "$sclass" "$w" a0 "$RC" "$WALL"
        run_a1 "$d" "$fmt" "$w";   submit "$id" "$agency" "$fmt" "$sclass" "$w" a1 "$RC" "$WALL"
    done

    # ---- repeated text (warm) -------------------------------------------
    for lane in v a0 a1; do
        total=0; lastrc=0
        for ((k=1; k<=REPEAT_N; k++)); do
            case "$lane" in
                v)  run_v  "$d" "$fmt" text_once ;;
                a0) run_a0 "$d" "$fmt" text_once "$src" ;;
                a1) run_a1 "$d" "$fmt" text_once ;;
            esac
            lastrc=$RC; total=$(( total + WALL ))
        done
        submit "$id" "$agency" "$fmt" "$sclass" "text_repeat" "$lane" "$lastrc" "$total"
    done

    # ---- exact reconstruction -------------------------------------------
    # V: materialize byte-exactly from the store.
    ex_v=0; t0=$(now_ms)
    if [ "$v_ok" -eq 1 ]; then
        timeout "$OP_TIMEOUT" "$BIN" materialize --store "$d/vstore" --field "$field" --exact --output "$d/v.exact.bin" >/dev/null 2>"$d/v.exact.err"; ex_v=$?
    else ex_v=3; fi
    t1=$(now_ms); vwall=$(( t1-t0 ))
    # A1: the source blob is retained verbatim; write it out.
    ex_a1=0; t0=$(now_ms)
    timeout "$OP_TIMEOUT" sqlite3 "$d/a1.db" "SELECT writefile('$d/a1.exact.bin', payload) FROM source_blob WHERE doc_id=1;" >/dev/null 2>"$d/a1.exact.err"; ex_a1=$?
    t1=$(now_ms); a1wall=$(( t1-t0 ))
    # A0: the source file itself.
    ex_a0=0; t0=$(now_ms); cp "$src" "$d/a0.exact.bin" 2>/dev/null; ex_a0=$?; t1=$(now_ms); a0wall=$(( t1-t0 ))

    vsha=$(sha256sum "$d/v.exact.bin" 2>/dev/null | cut -d' ' -f1)
    a1sha=$(sha256sum "$d/a1.exact.bin" 2>/dev/null | cut -d' ' -f1)
    a0sha=$(sha256sum "$d/a0.exact.bin" 2>/dev/null | cut -d' ' -f1)
    [ "$vsha" = "$sha" ]  && vok=1  || vok=0
    [ "$a1sha" = "$sha" ] && a1ok=1 || a1ok=0
    [ "$a0sha" = "$sha" ] && a0ok=1 || a0ok=0
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$id" "$agency" "$fmt" "$sclass" "$vok" "$ex_v" "$vwall" "$a1ok" "$ex_a1" "$a1wall" "$a0ok" "$a0wall" >>"$RAW/exact.tsv"
    submit "$id" "$agency" "$fmt" "$sclass" "exact" v  "$ex_v"  "$vwall"
    submit "$id" "$agency" "$fmt" "$sclass" "exact" a1 "$ex_a1" "$a1wall"
    submit "$id" "$agency" "$fmt" "$sclass" "exact" a0 "$ex_a0" "$a0wall"
done < <(jq -r --argjson lim "$LIMIT" '([.documents[]] | (if $lim > 0 then .[:$lim] else . end))[] | [.id,.agency,.format,.path,.sha256,(.byte_len|tostring),.size_class,.structural_tags,.cross_format_family_id] | @tsv' "$SCHEDULE")

echo "-- aggregating frontier map" >&2
python3 tools/fixtures/real100-frontier.py "$RAW" "$CAMPAIGN" | tee "$RAW/frontier.txt"

echo "real100 frontier court: done — $CAMPAIGN"
