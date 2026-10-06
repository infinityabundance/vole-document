#!/usr/bin/env bash
# Phase-12.11 — the mixed PDF/DOCX/EPUB **lifetime AI-document workload court**.
#
# Question (plan §62/§63, ADR-0035): on a pre-registered, deterministic mixed
# corpus and a frozen observation schedule, does the Phase-12 VOLE field answer
# repeatedly with a lower *cumulative lifetime cost* than
#
#   A0  direct per-query tooling (Poppler for PDF; pinned stdlib zipfile+XML for
#       DOCX/EPUB), no persistence; and
#   A1  a competent one-time-preprocessed SQLite+FTS5 cache **that retains the
#       source in full** (so exact recovery is possible),
#
# after every one-time cost is charged in full and amortized over
# N in {1,10,100,1000}, in fresh processes?
#
# Accounting (ADR-0027, ADR-0035):
#   * one-time costs (VOLE encode+field-ingest; A1 extraction+DB; A0 none) are
#     charged in full (wall, CPU, process read/pread bytes);
#   * persistent bytes are reported in the four ADR-0027 universes, never summed;
#   * per-query bytes are the process `read`+`pread64` total under strace for
#     **all three** systems (one boundary); VOLE's instrumented
#     `ObserveStats.bytes_read` is reported alongside and never summed;
#   * both systems get cold and warm passes; the answer of every case is
#     asserted against the committed schedule's expected value;
#   * crossover N per (format, surface, metric) is reported; "never" within
#     N<=1000 is a first-class verdict; every loss is recorded.
#
# Pre-registration: the schedule is committed at
# tools/fixtures/phase12-lifetime-schedule.json **before** this court runs.
# `SCHEDULE_ONLY=1` regenerates it (for the pre-registration commit) and exits.
#
# Usage (inside the pinned `doc-baseline` service):
#   bash tools/phase12-lifetime-court.sh [OUTDIR]
# Env: LIFETIME_NS, LIFETIME_PASSES (override the schedule), LIFETIME_DOCS.
set -u

cd /work
LC_ALL=C
export LC_ALL

OUTDIR=${1:-evidence/campaigns/$(date -u +%Y-%m-%d)-phase12-lifetime-$(git rev-parse --short HEAD 2>/dev/null || echo unknown)}
SCHEDULE=${SCHEDULE:-tools/fixtures/phase12-lifetime-schedule.json}
CORPUS=${CORPUS:-evidence/scratch/phase12-lifetime-corpus}
BIN=${VOLE_BIN:-./target/debug/vole-document}
BASE_PY=tools/fixtures/phase12-baseline.py

# --- pre-registration mode: regenerate the committed schedule, then exit -----
if [ "${SCHEDULE_ONLY:-0}" = 1 ]; then
  python3 tools/fixtures/phase12-schedule.py "$CORPUS" "$SCHEDULE"
  exit $?
fi

if [ ! -f "$SCHEDULE" ]; then
  echo "phase12-lifetime-court: missing committed schedule $SCHEDULE" >&2
  echo "run with SCHEDULE_ONLY=1 first (pre-register before measuring)" >&2
  exit 2
fi
if [ ! -f "$CORPUS/ground_truth.json" ]; then
  echo "phase12-lifetime-court: missing corpus; generate it first:" >&2
  echo "  docker compose run --rm --no-TTY producers python3 tools/fixtures/phase12-corpus-gen.py $CORPUS" >&2
  exit 2
fi

NS_STR=$(jq -r '.ns | join(" ")' "$SCHEDULE")
PASSES=$(jq -r '.passes' "$SCHEDULE")
[ -n "${LIFETIME_NS:-}" ] && NS_STR=$LIFETIME_NS
[ -n "${LIFETIME_PASSES:-}" ] && PASSES=$LIFETIME_PASSES

if [ ! -x "$BIN" ] || ! "$BIN" --help 2>&1 | grep -q 'field-ingest'; then
  echo "phase12-lifetime-court: building the all-features binary ($BIN)" >&2
  cargo build --locked --all-features 1>&2
fi

mkdir -p "$OUTDIR/raw"
RAW="$OUTDIR/raw"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

# ===========================================================================
# Functions (exported so the timed runner inherits them via `export -f`).
# ===========================================================================
sha() { sha256sum "$1" 2>/dev/null | cut -d' ' -f1; }
bytes() { stat -c %s "$1" 2>/dev/null || echo 0; }
dusb() { du -sb "$1" 2>/dev/null | cut -f1; }
now_us() { echo $(( ${EPOCHREALTIME/./} )); }
jqf() { jq -r "$1" "$2" 2>/dev/null; }
rp_bytes() {
  awk 'match($0,/^[0-9 ]*(read|pread64)\(/){if(match($0,/ = [0-9]+$/)){s+=substr($0,RSTART+2)+0}} match($0,/^[0-9 ]*<\.\.\. (read|pread64) resumed>/){if(match($0,/ = [0-9]+$/)){s+=substr($0,RSTART+2)+0}} END{print s+0}' "$1"
}
tv_cpu_ms() { awk -F': ' '/User time \(seconds\)/{u=$2} /System time \(seconds\)/{s=$2} END{printf "%.3f", (u+s)*1000}' "$1"; }
tv_rss_kb() { awk -F': ' '/Maximum resident set size/{print $2}' "$1" | tr -d ' '; }
export -f sha bytes dusb now_us jqf rp_bytes tv_cpu_ms tv_rss_kb

vole_argv() { # fmt id arg
  local fmt=$1 id=$2 arg=$3
  case "$id" in
    narrow-text|adjacent-region|paragraph-context)
      if [ "$fmt" = pdf ]; then echo "observe --page 1 --kind text"; else echo "observe --block ${arg#block:} --kind text"; fi ;;
    heading-section) echo "observe --heading 0 --kind text" ;;
    table-cell) echo "observe --cell ${arg#cell:} --kind text" ;;
    expand-table) echo "observe --table 0 --kind text" ;;
    resource) echo "observe --resource 0 --kind metadata" ;;
    metadata) echo "observe --metadata --kind metadata" ;;
    search) echo "find --text ${arg#search:}" ;;
    full-source) echo "MATERIALIZE" ;;
    exact-member)
      if [ "$fmt" = pdf ]; then echo "observe --byte-range 0..64 --kind exact"; else echo "observe --resource 0 --kind decoded"; fi ;;
    native-provenance)
      if [ "$fmt" = pdf ]; then echo "observe --page 1 --kind text"; else echo "observe --block 0 --kind text"; fi ;;
    *) echo "DECLINE" ;;
  esac
}
a0_py_case() { # id arg
  case "$1" in
    narrow-text|adjacent-region|paragraph-context) echo "block ${2#block:}" ;;
    heading-section) echo "heading 0" ;;
    table-cell) echo "cell ${2#cell:}" ;;
    expand-table) echo "table 0" ;;
    resource) echo "resource-meta 0" ;;
    metadata) echo "metadata -" ;;
    search) echo "search ${2#search:}" ;;
    exact-member) echo "resource-bytes 0" ;;
    *) echo "UNSUPPORTED" ;;
  esac
}
a1_sql() { # fmt id arg dir
  local fmt=$1 id=$2 arg=$3 d=$4
  case "$id" in
    narrow-text|adjacent-region|paragraph-context)
      if [ "$fmt" = pdf ]; then echo "SELECT text FROM blocks WHERE doc_id=1 AND unit='page' AND ordering=1;"; else echo "SELECT text FROM blocks WHERE doc_id=1 AND block_id=$((${arg#block:}+1));"; fi ;;
    heading-section) echo "SELECT b.text FROM blocks b JOIN headings h ON h.block_id=b.block_id WHERE h.doc_id=1 ORDER BY h.heading_id LIMIT 1;" ;;
    table-cell) echo "SELECT text FROM table_cells WHERE table_id=1 AND r=$(echo "${arg#cell:}" | cut -d: -f2) AND c=$(echo "${arg#cell:}" | cut -d: -f3);" ;;
    expand-table) echo "SELECT text FROM blocks WHERE doc_id=1 AND kind='table' ORDER BY block_id LIMIT 1;" ;;
    resource) echo "SELECT path||' '||member_bytes FROM resources WHERE doc_id=1 AND resource_id=1;" ;;
    metadata) echo "SELECT group_concat(key||'='||value,';') FROM metadata WHERE doc_id=1;" ;;
    search) echo "SELECT text FROM blocks WHERE doc_id=1 AND text LIKE '%${arg#search:}%' LIMIT 1;" ;;
    full-source) echo "SELECT writefile('$d/ans.bin', payload) FROM source_blob WHERE doc_id=1;" ;;
    exact-member)
      if [ "$fmt" = pdf ]; then echo "SELECT writefile('$d/ans.bin', substr(payload,1,64)) FROM source_blob WHERE doc_id=1;";
      else echo "SELECT writefile('$d/ans.bin', substr(payload, payload_offset+1, payload_length)) FROM source_blob JOIN resources ON resources.doc_id=source_blob.doc_id WHERE source_blob.doc_id=1 AND resource_id=1;"; fi ;;
    *) echo "NONE" ;;
  esac
}
export -f vole_argv a0_py_case a1_sql

run_v() { # id arg  (env D SRC FMT STORE FIELD)
  local id=$1 arg=$2 argv rc=0
  argv=$(vole_argv "$FMT" "$id" "$arg")
  case "$argv" in
    DECLINE) return 3 ;;
    MATERIALIZE) "$BIN" materialize --store "$STORE" --field "$FIELD" --exact --output "$D/ans.bin" > "$D/ans.json" 2>/dev/null || rc=$? ;;
    *) # shellcheck disable=SC2086
       "$BIN" $argv --store "$STORE" --field "$FIELD" > "$D/ans.json" 2>/dev/null || rc=$? ;;
  esac
  return $rc
}
run_a0() { # id arg (env D SRC FMT)
  local id=$1 arg=$2 rc=0 mapped case arg2
  [ "$id" = full-source ] && { cat "$SRC" > "$D/ans.bin"; return 0; }
  if [ "$FMT" = pdf ]; then
    case "$id" in
      narrow-text|adjacent-region) pdftotext -f 1 -l 1 "$SRC" "$D/ans" 2>/dev/null || rc=$? ;;
      metadata) pdfinfo "$SRC" > "$D/ans" 2>/dev/null || rc=$? ;;
      search) pdftotext "$SRC" - 2>/dev/null | grep -F -m1 "${arg#search:}" > "$D/ans" 2>/dev/null; rc=0 ;;
      exact-member) dd if="$SRC" of="$D/ans.bin" bs=1 count=64 2>/dev/null || rc=$? ;;
      *) rc=3 ;;
    esac
    return $rc
  fi
  mapped=$(a0_py_case "$id" "$arg")
  [ "$mapped" = UNSUPPORTED ] && return 3
  qcase="${mapped%% *}"; arg2="${mapped#* }"; [ "$arg2" = "$qcase" ] && arg2=""
  if [ "$id" = exact-member ]; then
    python3 "$BASE_PY" query --format "$FMT" --source "$SRC" --case "$qcase" --arg "$arg2" --out "$D/ans.bin" --metrics "$D/ans.metrics.json" >/dev/null 2>&1 || rc=$?
  else
    python3 "$BASE_PY" query --format "$FMT" --source "$SRC" --case "$qcase" --arg "$arg2" --out "$D/ans" --metrics "$D/ans.metrics.json" >/dev/null 2>&1 || rc=$?
  fi
  return $rc
}
run_a1() { # id arg (env D FMT DB)
  local id=$1 arg=$2 sql rc=0
  sql=$(a1_sql "$FMT" "$id" "$arg" "$D")
  [ "$sql" = NONE ] && return 3
  sqlite3 "$DB" "$sql" > "$D/ans" 2>"$D/a1.err" || rc=$?
  return $rc
}
export -f run_v run_a0 run_a1

# answer_text id dir -> the text/JSON payload of the answer
answer_text() { # id dir
  local id=$1 d=$2
  if [ "$id" = native-provenance ] && [ -f "$d/ans.json" ]; then jqf '.provenance' "$d/ans.json"; return; fi
  if [ -f "$d/ans.json" ]; then jqf '.text // (if .value==null then "" else (.value|tostring) end)' "$d/ans.json"; return; fi
  if [ -f "$d/ans" ]; then
    case "$(head -c1 "$d/ans" 2>/dev/null)" in
      "{") jqf '.text // (if .value==null then "" else (.value|tostring) end)' "$d/ans" ;;
      *) cat "$d/ans" ;;
    esac
    return
  fi
  echo "MISSING"
}
answer_sha() { # id dir
  local d=$2
  if [ -f "$d/ans.bin" ]; then sha "$d/ans.bin"; return; fi
  if [ -f "$d/ans.json" ]; then jqf '.bytes_sha256 // .sha256' "$d/ans.json"; return; fi
  echo "MISSING"
}
assert_case() { # id ek exp dir -> "PASS"|"FAIL:..."|"declined"
  local id=$1 ek=$2 exp=$3 d=$4 got
  case "$ek" in
    sha256) got=$(answer_sha "$id" "$d"); [ "$got" = "$exp" ] && echo PASS || echo "FAIL:sha got[$got] want[$exp]" ;;
    equals) got=$(answer_text "$id" "$d"); [ "$got" = "$exp" ] && echo PASS || echo "FAIL:eq got[$got] want[$exp]" ;;
    contains) got=$(answer_text "$id" "$d"); case "$got" in *"$exp"*) echo PASS;; *) echo "FAIL:contains got[$got] want[$exp]";; esac ;;
    nonempty) got=$(answer_text "$id" "$d"); [ -n "$got" ] && [ "$got" != MISSING ] && echo PASS || echo "FAIL:empty" ;;
    declined) echo "declined" ;;
    *) echo "FAIL:badkind" ;;
  esac
}

# ===========================================================================
# Environment capture.
# ===========================================================================
RUSTC_V=$(rustc --version 2>/dev/null | sed 's/^rustc //' || echo unknown)
CARGO_V=$(cargo --version 2>/dev/null | sed 's/^cargo //' || echo unknown)
PY_V=$(python3 --version 2>&1 | sed 's/^Python //')
ARCH=$(uname -m)
COMMIT=$(git rev-parse HEAD 2>/dev/null || echo unknown)
COMMIT_SHORT=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
DIRTY=$(git status --porcelain 2>/dev/null | head -40 | tr '\n' ';' || echo unknown)
LOCK_SHA=$(sha /work/Cargo.lock 2>/dev/null || echo unknown)
BASE_STABLE=$(sed -n 's/^ARG BASE_STABLE=//p' /work/Dockerfile 2>/dev/null | head -1)
BASE_TOOLS=$(sed -n 's/^ARG BASE_TOOLS=//p' /work/Dockerfile 2>/dev/null | head -1)
IMAGE_ID=${HOST_IMAGE_ID:-unrecorded}
SQLITE_V=$(sqlite3 --version | awk '{print $1}')
POPPLER_V=$(pdftotext -v 2>&1 | head -1 | sed 's/^pdftotext version //')
STRACE_V=$(strace --version 2>&1 | head -1 | sed 's/^strace -- version //')
JQ_V=$(jq --version)

jq -n \
  --arg commit "$COMMIT" --arg commit_short "$COMMIT_SHORT" --arg dirty "$DIRTY" \
  --arg lock_sha "$LOCK_SHA" --arg arch "$ARCH" \
  --arg rustc "$RUSTC_V" --arg cargo "$CARGO_V" --arg python "$PY_V" \
  --arg base_stable "$BASE_STABLE" --arg base_tools "$BASE_TOOLS" --arg image_id "$IMAGE_ID" \
  --arg sqlite "$SQLITE_V" --arg poppler "$POPPLER_V" --arg strace "$STRACE_V" --arg jq "$JQ_V" \
  --arg bin "$BIN" --arg ns "$NS_STR" --argjson passes "$PASSES" --arg schedule "$SCHEDULE" \
  '{git:{commit:$commit,commit_short:$commit_short,dirty:$dirty},
    arch:$arch, cargo_lock_sha256:$lock_sha,
    service_image:"vole-document/doc-baseline:1.99.0", doc_baseline_image_id:$image_id,
    toolchain:{rustc:$rustc,cargo:$cargo,python:$python},
    base_image:$base_stable, base_tools:$base_tools,
    oracles:{sqlite3:$sqlite,poppler_pdftotext:$poppler,strace:$strace,jq:$jq},
    env_affecting_semantics:{LC_ALL:"C",VOLE_BIN:$bin,LIFETIME_NS:$ns,LIFETIME_PASSES:$passes,SCHEDULE:$schedule},
    camera:"fresh process per query; per-query wall via bash EPOCHREALTIME; CPU/peak-RSS on a batch of the first N queries with /usr/bin/time -v; per-query bytes = process read/pread64 under strace for all three systems; VOLE instrumented stats.bytes_read reported alongside"}' \
  > "$RAW/environment.json"

DOCS=$(jq -r '.documents[].name' "$SCHEDULE")
[ -n "${LIFETIME_DOCS:-}" ] && DOCS=$LIFETIME_DOCS
echo "phase12-lifetime-court: docs=$(echo $DOCS | wc -w) ns='$NS_STR' passes=$PASSES" >&2

: > "$RAW/case_bytes.tsv"
: > "$RAW/assertions.tsv"
ALL_CUM="$WORK/cumulative.jsonl"; : > "$ALL_CUM"

# ---------------------------------------------------------------------------
# Timed runner (inherits exported functions + env).
# ---------------------------------------------------------------------------
cat > "$WORK/lq12.sh" <<'LQ'
#!/usr/bin/env bash
set -u
cd /work
declare -a IDS ARGS QB
while IFS=$'\t' read -r k id arg; do IDS[$k]=$id; ARGS[$k]=$arg; done < "$CASES"
while IFS=$'\t' read -r k b; do QB[$k]=$b; done < "$CBYTES"
: > "$CSV"
for (( j=1; j<=N; j++ )); do
  k=$(( (j-1) % NC ))
  cid=${IDS[$k]}; carg=${ARGS[$k]}
  b0=$(now_us)
  case "$SYS" in
    v)  D="$RUN" SRC="$SRC" FMT="$FMT" STORE="$STORE" FIELD="$FIELD" run_v  "$cid" "$carg" >/dev/null 2>&1 || true ;;
    a0) D="$RUN" SRC="$SRC" FMT="$FMT" run_a0 "$cid" "$carg" >/dev/null 2>&1 || true ;;
    a1) D="$RUN" FMT="$FMT" DB="$DB" run_a1 "$cid" "$carg" >/dev/null 2>&1 || true ;;
  esac
  b1=$(now_us)
  echo "$PASS $j $k $(( b1-b0 )) ${QB[$k]:-0}" >> "$CSV"
done
LQ
chmod +x "$WORK/lq12.sh"

# ---------------------------------------------------------------------------
# process_doc NAME
# ---------------------------------------------------------------------------
process_doc() {
  local name=$1
  local fmt src d
  fmt=$(jq -r --arg n "$name" '.documents[]|select(.name==$n).format' "$SCHEDULE")
  src="$CORPUS/$(jq -r --arg n "$name" '.documents[]|select(.name==$n).source' "$SCHEDULE")"
  d="$WORK/$name"; mkdir -p "$d/store" "$d/run"
  local nc; nc=$(jq -r --arg n "$name" '.documents[]|select(.name==$n).cases|length' "$SCHEDULE")
  local src_len src_sha; src_len=$(bytes "$src"); src_sha=$(sha "$src")
  echo "=== $name fmt=$fmt src=${src_len}B cases=$nc ===" >&2

  # ---- V one-time ---------------------------------------------------------
  local desc="$d/$name.voldoc" field
  local e0 e1 enc_ms ing_ms enc_cpu ing_cpu
  e0=$(now_us)
  /usr/bin/time -v -o "$d/enc.time" "$BIN" encode --force raw "$src" "$desc" > "$RAW/$name.encode.json" 2>/dev/null
  e1=$(now_us); enc_ms=$(( (e1-e0)/1000 )); enc_cpu=$(tv_cpu_ms "$d/enc.time")
  e0=$(now_us)
  /usr/bin/time -v -o "$d/ing.time" "$BIN" field-ingest "$desc" --store "$d/store" > "$RAW/$name.ingest.json" 2>/dev/null
  e1=$(now_us); ing_ms=$(( (e1-e0)/1000 )); ing_cpu=$(tv_cpu_ms "$d/ing.time")
  field=$(jqf '.field' "$RAW/$name.ingest.json")
  local v_one_ms=$(( enc_ms + ing_ms ))
  local v_one_cpu; v_one_cpu=$(awk -v a="$enc_cpu" -v b="$ing_cpu" 'BEGIN{printf "%.3f",a+b}')
  strace -f -e trace=read,pread64 -o "$d/v.enc.strace" "$BIN" encode --force raw "$src" "$d/s.voldoc" >/dev/null 2>&1 || true
  strace -f -e trace=read,pread64 -o "$d/v.ing.strace" "$BIN" field-ingest "$d/s.voldoc" --store "$d/sstore" >/dev/null 2>&1 || true
  local v_one_read=$(( $(rp_bytes "$d/v.enc.strace") + $(rp_bytes "$d/v.ing.strace") ))
  rm -rf "$d/s.voldoc" "$d/sstore"

  # ---- A1 one-time --------------------------------------------------------
  local db="$d/dbcache.db"
  local p0 p1 a1_ms a1_cpu
  p0=$(now_us)
  /usr/bin/time -v -o "$d/a1build.time" python3 "$BASE_PY" build --format "$fmt" --source "$src" --db "$db" --metrics "$RAW/$name.a1build.metrics.json" > "$RAW/$name.a1build.json" 2>/dev/null
  p1=$(now_us); a1_ms=$(( (p1-p0)/1000 )); a1_cpu=$(tv_cpu_ms "$d/a1build.time")
  strace -f -e trace=read,pread64 -o "$d/a1build.strace" python3 "$BASE_PY" build --format "$fmt" --source "$src" --db "$d/a1s.db" --metrics "$d/a1s.metrics.json" >/dev/null 2>&1 || true
  local a1_one_read; a1_one_read=$(rp_bytes "$d/a1build.strace")
  rm -f "$d/a1s.db" "$d/a1s.db-wal" "$d/a1s.db-shm"
  sqlite3 "$db" "PRAGMA wal_checkpoint(TRUNCATE);" >/dev/null 2>&1 || true
  local db_bytes wal_bytes shm_bytes a1_persistent
  db_bytes=$(bytes "$db"); wal_bytes=$(bytes "$db-wal"); shm_bytes=$(bytes "$db-shm")
  a1_persistent=$(( db_bytes + wal_bytes + shm_bytes ))

  # ---- V store universes are measured AFTER the timed schedule (see below), so
  #      the warm derived cache is inside the persistent footprint. ------------
  local desc_b=0 seed_b=0 index_b=0 cache_b=0 manifest_b=0 store_b=0

  # ---- per-case bytes + assertions (uncharged pre-pass) -------------------
  : > "$d/cases.tsv"
  : > "$d/casebytes.a0.tsv"; : > "$d/casebytes.a1.tsv"; : > "$d/casebytes.v.tsv"
  : > "$d/v.instrumented.tsv"
  local i=0
  while [ "$i" -lt "$nc" ]; do
    local id arg ek exp
    id=$(jq -r --arg n "$name" --argjson i "$i" '.documents[]|select(.name==$n).cases[$i].id' "$SCHEDULE")
    arg=$(jq -r --arg n "$name" --argjson i "$i" '.documents[]|select(.name==$n).cases[$i].arg' "$SCHEDULE")
    ek=$(jq -r --arg n "$name" --argjson i "$i" '.documents[]|select(.name==$n).cases[$i].expected_kind' "$SCHEDULE")
    exp=$(jq -r --arg n "$name" --argjson i "$i" '.documents[]|select(.name==$n).cases[$i].expected' "$SCHEDULE")
    printf '%s\t%s\t%s\n' "$i" "$id" "$arg" >> "$d/cases.tsv"
    local cd="$d/case-$i"; mkdir -p "$cd"

    # A0
    local a0_status=declined a0_read=0
    if D="$cd" SRC="$src" FMT="$fmt" run_a0 "$id" "$arg"; then
      a0_status=$(assert_case "$id" "$ek" "$exp" "$cd")
      if [ "$id" = full-source ]; then strace -f -e trace=read,pread64 -o "$cd/a0.strace" cat "$src" >/dev/null 2>&1 || true
      elif [ "$fmt" = pdf ]; then
        case "$id" in
          narrow-text|adjacent-region) strace -f -e trace=read,pread64 -o "$cd/a0.strace" pdftotext -f 1 -l 1 "$src" - >/dev/null 2>&1 || true ;;
          metadata) strace -f -e trace=read,pread64 -o "$cd/a0.strace" pdfinfo "$src" >/dev/null 2>&1 || true ;;
          search) strace -f -e trace=read,pread64 -o "$cd/a0.strace" sh -c "pdftotext '$src' - | grep -F -m1 '${arg#search:}'" >/dev/null 2>&1 || true ;;
          exact-member) strace -f -e trace=read,pread64 -o "$cd/a0.strace" dd if="$src" of=/dev/null bs=1 count=64 >/dev/null 2>&1 || true ;;
        esac
      else
        local mp c2 a2; mp=$(a0_py_case "$id" "$arg"); c2="${mp%% *}"; a2="${mp#* }"; [ "$a2" = "$c2" ] && a2=""
        strace -f -e trace=read,pread64 -o "$cd/a0.strace" python3 "$BASE_PY" query --format "$fmt" --source "$src" --case "$c2" --arg "$a2" --out /dev/null >/dev/null 2>&1 || true
      fi
      a0_read=$(rp_bytes "$cd/a0.strace")
    fi

    # A1
    local a1_status=declined a1_read=0
    if D="$cd" FMT="$fmt" DB="$db" run_a1 "$id" "$arg"; then
      a1_status=$(assert_case "$id" "$ek" "$exp" "$cd")
      local sql; sql=$(a1_sql "$fmt" "$id" "$arg" "$cd")
      strace -f -e trace=read,pread64 -o "$cd/a1.strace" sqlite3 "$db" "$sql" >/dev/null 2>&1 || true
      a1_read=$(rp_bytes "$cd/a1.strace")
    fi

    # V
    local v_status=declined v_read=0 v_inst=0 v_ret=0
    if D="$cd" SRC="$src" FMT="$fmt" STORE="$d/store" FIELD="$field" run_v "$id" "$arg"; then
      v_status=$(assert_case "$id" "$ek" "$exp" "$cd")
      [ -f "$cd/ans.json" ] && { v_inst=$(jqf '.stats.bytes_read' "$cd/ans.json"); v_ret=$(jqf '.stats.bytes_returned' "$cd/ans.json"); }
      local vargv; vargv=$(vole_argv "$fmt" "$id" "$arg")
      if [ "$vargv" = MATERIALIZE ]; then
        strace -f -e trace=read,pread64 -o "$cd/v.strace" "$BIN" materialize --store "$d/store" --field "$field" --exact --output /dev/null >/dev/null 2>&1 || true
      else
        # shellcheck disable=SC2086
        strace -f -e trace=read,pread64 -o "$cd/v.strace" "$BIN" $vargv --store "$d/store" --field "$field" >/dev/null 2>&1 || true
      fi
      v_read=$(rp_bytes "$cd/v.strace")
    fi

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$i" "$id" "$a0_read" "$a1_read" "$v_read" "${v_inst:-0}" "${v_ret:-0}" >> "$RAW/case_bytes.tsv"
    printf '%s\t%s\t%s\n' "$i" "$a0_read" >> "$d/casebytes.a0.tsv"
    printf '%s\t%s\n' "$i" "$a1_read" >> "$d/casebytes.a1.tsv"
    printf '%s\t%s\n' "$i" "$v_read" >> "$d/casebytes.v.tsv"
    printf '%s\t%s\n' "$i" "${v_inst:-0}" >> "$d/v.instrumented.tsv"
    printf '%s\t%s\t%s\t%s\t%s\n' "$name" "$id" "A0:$a0_status" "A1:$a1_status" "V:$v_status" >> "$RAW/assertions.tsv"
    i=$((i+1))
  done

  # ---- timed schedule -----------------------------------------------------
  export BIN BASE_PY FMT="$fmt" SRC="$src" DB="$db" STORE="$d/store" FIELD="$field" RUN="$d/run" NC="$nc" CASES="$d/cases.tsv"
  local sys pass n
  for sys in a0 a1 v; do
    export SYS="$sys" CBYTES="$d/casebytes.$sys.tsv"
    for (( pass=1; pass<=PASSES; pass++ )); do
      export PASS="$pass"
      for n in $NS_STR; do
        local csv="$RAW/$name.$sys.p$pass.n$n.csv" agg="$d/$sys.p$pass.n$n.time"
        export N="$n" CSV="$csv"
        /usr/bin/time -v -o "$agg" bash "$WORK/lq12.sh" >/dev/null 2>"$RAW/$name.runner.$sys.p$pass.n$n.err" || true
        local cpm rss otm otb otc
        cpm=$(tv_cpu_ms "$agg" 2>/dev/null || echo 0); rss=$(tv_rss_kb "$agg" 2>/dev/null || echo 0)
        case "$sys" in
          v) otm=$v_one_ms; otb=$v_one_read; otc=$v_one_cpu ;;
          a1) otm=$a1_ms; otb=$a1_one_read; otc=$a1_cpu ;;
          a0) otm=0; otb=0; otc=0 ;;
        esac
        local cw cb
        cw=$(awk '{s+=$4} END{printf "%.3f", s/1000.0}' "$csv" 2>/dev/null || echo 0)
        cb=$(awk '{s+=$5} END{printf "%.0f", s}' "$csv" 2>/dev/null || echo 0)
        printf '{"name":"%s","format":"%s","system":"%s","pass":%d,"n":%d,"cumulative_wall_ms":%.3f,"cumulative_cpu_ms":%.3f,"peak_rss_kb":%d,"cumulative_process_read_bytes":%.0f,"one_time_wall_ms":%.3f,"one_time_cpu_ms":%.3f,"one_time_process_read_bytes":%d}\n' \
          "$name" "$fmt" "$sys" "$pass" "$n" \
          "$(awk -v a="$otm" -v b="$cw" 'BEGIN{printf "%.3f", a+b}')" "$(awk -v a="$otc" -v b="$cpm" 'BEGIN{printf "%.3f", a+b}')" "${rss:-0}" \
          "$(awk -v a="$otb" -v b="$cb" 'BEGIN{printf "%.0f", a+b}')" "$otm" "$otc" "$otb" >> "$ALL_CUM"
      done
    done
  done

  # ---- per-doc JSON -------------------------------------------------------
  # (store universes are re-measured here, after the timed schedule, so the
  #  warm derived cache is inside the persistent footprint)
  desc_b=$(dusb "$d/store/descriptor"); seed_b=$(dusb "$d/store/seed")
  index_b=$(dusb "$d/store/index"); cache_b=$(dusb "$d/store/cache")
  manifest_b=$(dusb "$d/store/field"); store_b=$(dusb "$d/store")
  jq -n \
    --arg name "$name" --arg fmt "$fmt" --arg src "$src" \
    --argjson src_len "$src_len" --arg src_sha "$src_sha" \
    --argjson nc "$nc" --argjson ns "$(printf '%s\n' $NS_STR | jq -R 'tonumber' | jq -s '.')" --argjson passes "$PASSES" \
    --arg field "$field" \
    --argjson v_one_ms "$v_one_ms" --argjson v_one_cpu "$v_one_cpu" --argjson v_one_read "$v_one_read" \
    --argjson a1_ms "$a1_ms" --argjson a1_cpu "$a1_cpu" --argjson a1_one_read "$a1_one_read" \
    --argjson db_bytes "$db_bytes" --argjson wal_bytes "$wal_bytes" --argjson shm_bytes "$shm_bytes" --argjson a1_persistent "$a1_persistent" \
    --argjson desc_b "$desc_b" --argjson seed_b "$seed_b" --argjson index_b "$index_b" \
    --argjson cache_b "$cache_b" --argjson manifest_b "$manifest_b" --argjson store_b "$store_b" \
    --argjson enc_ms "$enc_ms" --argjson enc_cpu "$enc_cpu" --argjson ing_ms "$ing_ms" --argjson ing_cpu "$ing_cpu" \
    --slurpfile ing "$RAW/$name.ingest.json" \
    --slurpfile a1m "$RAW/$name.a1build.metrics.json" \
    '{
      name:$name, format:$fmt,
      source:{path:$src,len:$src_len,sha256:$src_sha},
      schedule:{cases:$nc, ns:$ns, passes:$passes},
      vole:{
        encode:{wall_ms:$enc_ms,cpu_ms:$enc_cpu},
        ingest:{wall_ms:$ing_ms,cpu_ms:$ing_cpu,field:$field},
        one_time:{wall_ms:$v_one_ms,cpu_ms:$v_one_cpu,process_read_bytes:$v_one_read},
        ingest_report:$ing[0],
        store_universe_bytes:{descriptor:$desc_b,seed:$seed_b,index:$index_b,manifest:$manifest_b,derived_cache:$cache_b,total:$store_b}
      },
      a0:{one_time:{wall_ms:0,cpu_ms:0,process_read_bytes:0}},
      a1:{preprocessing:{wall_ms:$a1_ms,cpu_ms:$a1_cpu,process_read_bytes:$a1_one_read},
          db_bytes:$db_bytes,wal_bytes:$wal_bytes,shm_bytes:$shm_bytes,
          persistent_bytes:$a1_persistent,retained_source_bytes:$src_len,
          extraction_metrics:$a1m[0]}
    }' > "$RAW/$name.json" 2>"$RAW/$name.jqerr"
  [ -s "$RAW/$name.json" ] || { echo "FATAL: empty json for $name" >&2; cat "$RAW/$name.jqerr" >&2; exit 1; }
}

# ===========================================================================
# Run.
# ===========================================================================
for name in $DOCS; do process_doc "$name"; done

# ===========================================================================
# receipt.json
# ===========================================================================
DOCFILES=""
for name in $DOCS; do DOCFILES="$DOCFILES $RAW/$name.json"; done
# shellcheck disable=SC2086
jq -s '[.[]|select(has("name"))]' $DOCFILES > "$WORK/docs.json"
cp "$ALL_CUM" "$RAW/cumulative.jsonl"

jq -n \
  --slurpfile env "$RAW/environment.json" \
  --slurpfile docs "$WORK/docs.json" \
  --slurpfile cum "$WORK/cumulative.jsonl" \
  --slurpfile sched "$SCHEDULE" \
  '{
    campaign:"phase12-lifetime-court",
    phase:"Phase 12.11 — mixed PDF/DOCX/EPUB lifetime AI-document workload court (A0/A1 vs V)",
    environment:$env[0],
    schedule:$sched[0],
    accounting_universes_adr0027:{
      source:"original document bytes (archival reality)",
      descriptor_store:"exact .voldoc descriptor + externalized objects (normative reconstruction)",
      procedural_field:"seed DAG nodes + hierarchical index bytes (normative for observations)",
      derived_cache:"disposable observation caches (never normative)"
    },
    boundary_note:"Per-query bytes are the process read/pread64 total under strace for A0, A1 and V alike (one boundary). VOLE ObserveStats.bytes_read is reported separately in raw/case_bytes.tsv and is never summed with process reads. CPU/peak-RSS are aggregate measurements over the first N queries.",
    documents:$docs[0],
    cumulative:$cum
  }' > "$OUTDIR/receipt.json"

# ===========================================================================
# Crossovers + losses + SUMMARY.md
# ===========================================================================
{
  echo "# Phase 12.11 — mixed-format lifetime AI-document workload court"
  echo
  echo "Generated by \`tools/phase12-lifetime-court.sh\` inside the pinned \`doc-baseline\` service."
  echo "Corpus: 12 deterministic documents (alpha/bravo/charlie/delta x PDF/DOCX/EPUB) from"
  echo "\`tools/fixtures/phase12-corpus-gen.py\`; schedule pre-registered in"
  echo "\`tools/fixtures/phase12-lifetime-schedule.json\` (round-robin, frozen before measurement)."
  echo "delta is a deliberately large, incompressible variant where the source-retaining"
  echo "baseline's covering-index point lookups are expected to beat VOLE's per-observation"
  echo "full-descriptor read at large N."
  echo
  jq -r '"Commit: `\(.environment.git.commit_short)` (dirty: \(.environment.git.dirty))  \n" +
    "Service image `vole-document/doc-baseline:1.99.0` (id `\(.environment.doc_baseline_image_id)`), base `\(.environment.base_image)`.  \n" +
    "rustc `\(.environment.toolchain.rustc)`, cargo `\(.environment.toolchain.cargo)`, python `\(.environment.toolchain.python)`; Cargo.lock sha256 `\(.environment.cargo_lock_sha256)`; arch `\(.environment.arch)`.  \n" +
    "Oracles: sqlite3 \(.environment.oracles.sqlite3), poppler \(.environment.oracles.poppler_pdftotext), strace \(.environment.oracles.strace).  \n" +
    "Schedule: \(.schedule.documents|length) documents, ns=\(.schedule.ns|join(",")), passes=\(.schedule.passes)."' "$OUTDIR/receipt.json"
  echo
  echo "## Per-document one-time and persistent cost"
  echo
  echo "| document | fmt | src B | V one-time ms | V one-time read B | A1 one-time ms | A1 one-time read B | V store B | A1 db B | A1 retained src B |"
  echo "|---|---|---:|---:|---:|---:|---:|---:|---:|---:|"
  jq -r '.documents[] | "| \(.name) | \(.format) | \(.source.len) | \(.vole.one_time.wall_ms) | \(.vole.one_time.process_read_bytes) | \(.a1.preprocessing.wall_ms) | \(.a1.preprocessing.process_read_bytes) | \(.vole.store_universe_bytes.total) | \(.a1.db_bytes) | \(.a1.retained_source_bytes) |"' "$OUTDIR/receipt.json"
  echo
  echo "## Crossover (warm pass): where the ordering flips (or never)"
  echo
  echo "\`V wins\` is the range of N at which V's cumulative is strictly below the baseline;"
  echo "\`A1 leads from\` is the first N at which the baseline's cumulative is <= V's (empty ="
  echo "the baseline never leads). A one-point range means V led only at that N."
  echo
  echo "| document | fmt | metric | V wins vs A0 | V wins vs A1 | A1 leads from |"
  echo "|---|---|---|---|---|---|"
  jq -r '
    (.schedule.passes) as $P
    | .documents[] as $d
    | ($d.name) as $nm
    | ([.cumulative[]|select(.name==$nm and .pass==$P and .system=="v")]|sort_by(.n)) as $vv
    | ([.cumulative[]|select(.name==$nm and .pass==$P and .system=="a0")]|sort_by(.n)) as $v0
    | ([.cumulative[]|select(.name==$nm and .pass==$P and .system=="a1")]|sort_by(.n)) as $v1
    | def rng($a;$b;k): ([range(0;($a|length)) as $i|select(($a[$i][k]//1e18) < ($b[$i][k]//1e18))|$a[$i].n]) as $w
        | if ($w|length)==0 then "never" elif $w[-1]==$a[-1].n then "N>="+($w[0]|tostring)+" (all)" else "N="+($w[0]|tostring)+".."+($w[-1]|tostring) end;
      def lead($a;$b;k): ([range(0;($a|length)) as $i|select(($b[$i][k]//1e18) <= ($a[$i][k]//1e18))|$b[$i].n][0] // "never");
      ["cumulative_wall_ms","cumulative_cpu_ms","cumulative_process_read_bytes"][] as $k
      | "| \($d.name) | \($d.format) | \($k) | \(rng($vv;$v0;$k)) | \(rng($vv;$v1;$k)) | \(lead($vv;$v1;$k)) |"
  ' "$OUTDIR/receipt.json"
  echo
  echo "Queries answered in a fresh process each; one-time costs charged in full and amortized."
  echo
  echo "## Cumulative numbers at N=1 and N=max (warm pass)"
  echo
  echo "| document | system | N | wall ms | cpu ms | peak RSS kB | process read B |"
  echo "|---|---|---:|---:|---:|---:|---:|"
  jq -r '(.schedule.passes) as $P | (.schedule.ns) as $NS | (($NS|max)) as $MAX
    | .cumulative[] | select(.pass==$P and (.n==1 or .n==$MAX))
    | "| \(.name) | \(.system) | \(.n) | \(.cumulative_wall_ms) | \(.cumulative_cpu_ms) | \(.peak_rss_kb) | \(.cumulative_process_read_bytes) |"' "$OUTDIR/receipt.json"
  echo
  echo "## All recorded losses and declines"
  echo
  jq -r '(.schedule.passes) as $P
    | .documents[] as $d
    | ($d.name) as $nm
    | ([.cumulative[]|select(.name==$nm and .pass==$P and .system=="v")]|sort_by(.n)|last) as $vmax
    | ([.cumulative[]|select(.name==$nm and .pass==$P and .system=="a1")]|sort_by(.n)|last) as $a1max
    | "- \($d.name): V one-time process read \($d.vole.one_time.process_read_bytes) B (\((($d.vole.one_time.process_read_bytes/$d.source.len)*100|round/100))x source); V store \($d.vole.store_universe_bytes.total) B vs A1 db \($d.a1.db_bytes) B."
  ' "$OUTDIR/receipt.json"
  echo
  echo "Assertion failures (should be empty) and declines:"
  echo
  awk -F'\t' '$3!="A0:PASS" || $4!="A1:PASS" || $5!="V:PASS" {printf "- %s %s %s %s %s\n",$1,$2,$3,$4,$5}' "$RAW/assertions.tsv"
} > "$OUTDIR/SUMMARY.md"

echo "phase12-lifetime-court: wrote $OUTDIR" >&2
