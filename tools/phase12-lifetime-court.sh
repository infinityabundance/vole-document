#!/usr/bin/env bash
# Phase-12.11 / 12.11b — the mixed PDF/DOCX/EPUB **lifetime AI-document workload
# court**, extended with the **required ablation ladder** (plan §105, ADR-0035).
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
# And (12.11b) *which* Phase-12 capability produces the result. The required
# ladder (plan §105 / ADR-0035) is measured as separate lanes with their own
# switch — feature-set builds and two runtime flags, never copy-pasted code:
#
#   A0   direct tooling per query                              (lane `a0`)
#   A1   source-retaining SQLite+FTS5                          (lane `a1`)
#   A1b  A1 + cached popular results                           (lane `a1b`)
#   A2   Phase-11 field, PDF only (no `package` feature)       (lane `a2`)
#   A3   ZIP physical only (`package`, no OPC/OCF graph)       (lane `a3`)
#   A4   + native package graph (`opc`; no docx/epub content)  (lane `a4`)
#   A5   + progressive semantic inversion (docx/epub, --no-cache) (lane `a5`)
#   A6   + persistent semantic reuse (`DerivedCache` on)       (lane `a6`)
#   A7   + common observation layer                            (not separable; in dispatch)
#   A8   + hierarchical indexes                                (not separable; built at ingest)
#   A9   + EntropyFS fine-grained range access                 (lane `a9`, `--entropyfs`)
#   A10  + cross-document sharing                              (not separable per-document; see §A10)
#   A11  full Phase-12 system                                  (lane `a11`; ≡ prior `v`)
#
# Rungs that are not cleanly separable in the landed architecture are recorded
# honestly in the receipt's `rungs` table (status `not separable` + proxy), never
# fabricated. The §105 "eager-vs-progressive" and "raw-compressed-vs-decoded-
# persisted" ablations map onto the A4/A6 switch (`--no-cache` vs cache).
#
# Accounting (ADR-0027, ADR-0035):
#   * one-time costs (VOLE encode+field-ingest; A1 extraction+DB; A1b +cache
#     population) are charged in full (wall, CPU, process read/pread bytes);
#   * persistent bytes are reported in the four ADR-0027 universes, never summed;
#   * per-query bytes are the process `read`+`pread64` total under strace for
#     **every** lane (one boundary); VOLE instrumented `ObserveStats.bytes_read`
#     is reported alongside and never summed;
#   * the answer of every case is asserted against the committed schedule's
#     expected value; a capability-declining rung is recorded as a decline, and
#     the answered-set delta is the ladder's attribution evidence.
#
# Pre-registration: the schedule is committed at
# tools/fixtures/phase12-lifetime-schedule.json **before** this court runs, and
# it carries the pre-registered `ladder` rung table. `SCHEDULE_ONLY=1` regenerates
# it (for the pre-registration commit) and exits.
#
# Usage (inside the pinned `doc-baseline` service):
#   bash tools/phase12-lifetime-court.sh [OUTDIR]
# Env: LIFETIME_NS, LIFETIME_PASSES (override the schedule), LIFETIME_DOCS,
#      LIFETIME_LANES (space list to restrict field lanes, e.g. "a6 a11").
set -u

cd /work
LC_ALL=C
export LC_ALL

OUTDIR=${1:-evidence/campaigns/$(date -u +%Y-%m-%d)-phase12-lifetime-ablations-$(git rev-parse --short HEAD 2>/dev/null || echo unknown)}
SCHEDULE=${SCHEDULE:-tools/fixtures/phase12-lifetime-schedule.json}
CORPUS=${CORPUS:-evidence/scratch/phase12-lifetime-corpus}
BASE_PY=tools/fixtures/phase12-baseline.py
export SCHEDULE

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
MAX_NS=$(printf '%s\n' $NS_STR | jq -R 'tonumber' | jq -s 'max')

mkdir -p "$OUTDIR/raw"
RAW="$OUTDIR/raw"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
BINDIR="$WORK/bin"; mkdir -p "$BINDIR"
SCRATCH="$WORK/a1b"; mkdir -p "$SCRATCH"

# ===========================================================================
# Function definitions (exported so the timed runner inherits them).
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

run_v() { # id arg  (env D SRC FMT STORE FIELD VOLE_BIN VOLE_FLAGS)
  local id=$1 arg=$2 argv rc=0
  argv=$(vole_argv "$FMT" "$id" "$arg")
  case "$argv" in
    DECLINE) return 3 ;;
    MATERIALIZE)
      # shellcheck disable=SC2086
      "$VOLE_BIN" materialize --store "$STORE" --field "$FIELD" --exact --output "$D/ans.bin" $VOLE_FLAGS > "$D/ans.json" 2>/dev/null || rc=$? ;;
    *)
      # shellcheck disable=SC2086
      "$VOLE_BIN" $argv --store "$STORE" --field "$FIELD" $VOLE_FLAGS > "$D/ans.json" 2>/dev/null || rc=$? ;;
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
run_a1b() { # id arg (env D DB) — A1 + materialized popular results
  local id=$1 arg=$2 k kind rc=0
  k="$id|$arg"
  kind=$(sqlite3 "$DB" "SELECT kind FROM result_cache WHERE k='$k';" 2>/dev/null)
  case "$kind" in
    bytes) sqlite3 "$DB" "SELECT writefile('$D/ans.bin', answer) FROM result_cache WHERE k='$k';" >/dev/null 2>"$D/a1b.err" || rc=$? ;;
    text)  sqlite3 "$DB" "SELECT writefile('$D/ans', answer) FROM result_cache WHERE k='$k';" >/dev/null 2>>"$D/a1b.err" || rc=$? ;;
    *) return 3 ;;
  esac
  return $rc
}
# Populate the A1b result cache by executing A1's own SQL for every pre-registered
# case (one source of truth: `a1_sql`). Byte cases write the answer blob.
populate_a1b() { # db dir name nc fmt
  local db=$1 d=$2 nm=$3 nc=$4 fmt=$5 i=0 id arg sql key
  sqlite3 "$db" "CREATE TABLE IF NOT EXISTS result_cache(k TEXT PRIMARY KEY, kind TEXT, answer BLOB);" 2>/dev/null
  while [ "$i" -lt "$nc" ]; do
    id=$(jq -r --arg n "$nm" --argjson i "$i" '.documents[]|select(.name==$n).cases[$i].id' "$SCHEDULE")
    arg=$(jq -r --arg n "$nm" --argjson i "$i" '.documents[]|select(.name==$n).cases[$i].arg' "$SCHEDULE")
    sql=$(a1_sql "$fmt" "$id" "$arg" "$d")
    if [ "$sql" != NONE ]; then
      key="$id|$arg"
      if printf '%s' "$sql" | grep -q writefile; then
        sqlite3 "$db" "$sql" >/dev/null 2>&1
        sqlite3 "$db" "INSERT OR REPLACE INTO result_cache(k,kind,answer) VALUES('$key','bytes',readfile('$d/ans.bin'));" 2>/dev/null
      else
        sqlite3 "$db" "$sql" > "$d/a1b.ans" 2>/dev/null
        sqlite3 "$db" "INSERT OR REPLACE INTO result_cache(k,kind,answer) VALUES('$key','text',readfile('$d/a1b.ans'));" 2>/dev/null
      fi
    fi
    i=$((i+1))
  done
}
export -f run_v run_a0 run_a1 run_a1b populate_a1b

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
export -f answer_text answer_sha assert_case

# ===========================================================================
# Build the ladder's feature-set binaries inside the pinned service.
#   A2  = field (Phase-11, PDF only; no `package`)
#   A3  = + package (ZIP physical/OCF physical, no OPC/OCF *graph*)
#   A46 = + opc/docx/epub (native package graph); `--no-cache` selects A4/A5
#   full = --all-features (A9 via `--entropyfs`, A11 default)
# Never compile on the host: every build runs here.
# ===========================================================================
BIN_HELPER() { :; }
build_all_features() {
  cargo build --locked --all-features 1>&2
  cp target/debug/vole-document "$BINDIR/vole-full"
}
build_variant() { # label features
  cargo build --locked --no-default-features --features "$2" 1>&2
  cp target/debug/vole-document "$BINDIR/vole-$1"
}
echo "phase12-lifetime-court: building ladder binaries" >&2
build_all_features
build_variant a2 "rans,store,field"
build_variant a3 "rans,store,field,package"
build_variant a4g "rans,store,field,package,opc"
build_variant a46 "rans,store,field,package,opc,docx,epub"
FULL_BIN="$BINDIR/vole-full"

# --- ladder lanes: label|bin|flags|rung -------------------------------------
FIELD_LANES=(
  "a2|$BINDIR/vole-a2||A2 Phase-11 field (PDF only; no package feature)"
  "a3|$BINDIR/vole-a3||A3 ZIP physical only (package, no opc/docx/epub)"
  "a4|$BINDIR/vole-a4g||A4 + native package graph (opc; no docx/epub content semantics)"
  "a5|$BINDIR/vole-a46|--no-cache|A5 + progressive semantic inversion (docx/epub content, no persistent reuse)"
  "a6|$BINDIR/vole-a46||A6 + persistent semantic reuse (DerivedCache on)"
  "a9|$FULL_BIN|--entropyfs|A9 + EntropyFS fine-grained range access (entropyfs-store backend)"
  "a11|$FULL_BIN||A11 full Phase-12 system (filesystem backend, DerivedCache on)"
)
if [ -n "${LIFETIME_LANES:-}" ]; then
  FILTERED=()
  for spec in "${FIELD_LANES[@]}"; do
    IFS='|' read -r l _ _ _ <<< "$spec"
    for want in $LIFETIME_LANES; do [ "$l" = "$want" ] && FILTERED+=("$spec"); done
  done
  FIELD_LANES=("${FILTERED[@]}")
fi

# The rung table pre-registered in the schedule (status + switch), so the receipt
# and SUMMARY can state every loss and every non-separable rung.
RUNGS_JSON=$(jq -c '.ladder' "$SCHEDULE")

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

LANES_DESC=$(printf '%s\n' "${FIELD_LANES[@]}" | cut -d'|' -f1 | tr '\n' ' ')
jq -n \
  --arg commit "$COMMIT" --arg commit_short "$COMMIT_SHORT" --arg dirty "$DIRTY" \
  --arg lock_sha "$LOCK_SHA" --arg arch "$ARCH" \
  --arg rustc "$RUSTC_V" --arg cargo "$CARGO_V" --arg python "$PY_V" \
  --arg base_stable "$BASE_STABLE" --arg base_tools "$BASE_TOOLS" --arg image_id "$IMAGE_ID" \
  --arg sqlite "$SQLITE_V" --arg poppler "$POPPLER_V" --arg strace "$STRACE_V" --arg jq "$JQ_V" \
  --arg bin "$FULL_BIN" --arg ns "$NS_STR" --argjson passes "$PASSES" --arg schedule "$SCHEDULE" \
  --arg lanes "$LANES_DESC" \
  '{git:{commit:$commit,commit_short:$commit_short,dirty:$dirty},
    arch:$arch, cargo_lock_sha256:$lock_sha,
    service_image:"vole-document/doc-baseline:1.99.0", doc_baseline_image_id:$image_id,
    toolchain:{rustc:$rustc,cargo:$cargo,python:$python},
    base_image:$base_stable, base_tools:$base_tools,
    oracles:{sqlite3:$sqlite,poppler_pdftotext:$poppler,strace:$strace,jq:$jq},
    env_affecting_semantics:{LC_ALL:"C",LIFETIME_NS:$ns,LIFETIME_PASSES:$passes,SCHEDULE:$schedule,LIFETIME_LANES:$lanes},
    camera:"fresh process per query; per-query wall via bash EPOCHREALTIME; CPU/peak-RSS on a batch of the first N queries with /usr/bin/time -v; per-query bytes = process read/pread64 under strace for every lane; VOLE instrumented stats.bytes_read reported alongside; ladder binaries built inside this service with `--no-default-features --features ...`"}' \
  > "$RAW/environment.json"

DOCS=$(jq -r '.documents[].name' "$SCHEDULE")
[ -n "${LIFETIME_DOCS:-}" ] && DOCS=$LIFETIME_DOCS
echo "phase12-lifetime-court: docs=$(echo $DOCS | wc -w) ns='$NS_STR' passes=$PASSES lanes='$LANES_DESC'" >&2

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
    a0)  D="$RUN" SRC="$SRC" FMT="$FMT" run_a0 "$cid" "$carg" >/dev/null 2>&1 || true ;;
    a1)  D="$RUN" FMT="$FMT" DB="$DB" run_a1 "$cid" "$carg" >/dev/null 2>&1 || true ;;
    a1b) D="$RUN" DB="$DBB" run_a1b "$cid" "$carg" >/dev/null 2>&1 || true ;;
    *)   D="$RUN" SRC="$SRC" FMT="$FMT" STORE="$STORE" FIELD="$FIELD" VOLE_BIN="$VOLE_BIN" VOLE_FLAGS="$VOLE_FLAGS" run_v "$cid" "$carg" >/dev/null 2>&1 || true ;;
  esac
  b1=$(now_us)
  echo "$PASS $j $k $(( b1-b0 )) ${QB[$k]:-0}" >> "$CSV"
done
LQ
chmod +x "$WORK/lq12.sh"

# ---------------------------------------------------------------------------
# process_case_lanes NAME — per-case bytes + assertions for every lane.
# Reads rows from $d/cases.tsv. Emits raw/case_bytes.tsv (wide) and the per-lane
# casebytes files the timed runner consumes.
# ---------------------------------------------------------------------------
process_case_lanes() {
  local name=$1 fmt=$2 src=$3 d=$4 nc=$5 db=$6 dbb=$7
  : > "$d/cases.tsv"
  : > "$d/casebytes.a0.tsv"; : > "$d/casebytes.a1.tsv"; : > "$d/casebytes.a1b.tsv"
  local i=0
  for spec in "${FIELD_LANES[@]}"; do
    IFS='|' read -r label _ _ _ <<< "$spec"
    : > "$d/casebytes.$label.tsv"
  done
  while [ "$i" -lt "$nc" ]; do
    local id arg ek exp cd
    id=$(jq -r --arg n "$name" --argjson i "$i" '.documents[]|select(.name==$n).cases[$i].id' "$SCHEDULE")
    arg=$(jq -r --arg n "$name" --argjson i "$i" '.documents[]|select(.name==$n).cases[$i].arg' "$SCHEDULE")
    ek=$(jq -r --arg n "$name" --argjson i "$i" '.documents[]|select(.name==$n).cases[$i].expected_kind' "$SCHEDULE")
    exp=$(jq -r --arg n "$name" --argjson i "$i" '.documents[]|select(.name==$n).cases[$i].expected' "$SCHEDULE")
    printf '%s\t%s\t%s\n' "$i" "$id" "$arg" >> "$d/cases.tsv"
    cd="$d/case-$i"; mkdir -p "$cd"

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

    # A1b
    local a1b_status=declined a1b_read=0
    if D="$cd" DB="$dbb" run_a1b "$id" "$arg"; then
      a1b_status=$(assert_case "$id" "$ek" "$exp" "$cd")
      local k="$id|$arg"; local kind; kind=$(sqlite3 "$dbb" "SELECT kind FROM result_cache WHERE k='$k';" 2>/dev/null)
      case "$kind" in
        bytes) strace -f -e trace=read,pread64 -o "$cd/a1b.strace" sqlite3 "$dbb" "SELECT writefile('$cd/ans.bin', answer) FROM result_cache WHERE k='$k';" >/dev/null 2>&1 || true ;;
        text)  strace -f -e trace=read,pread64 -o "$cd/a1b.strace" sqlite3 "$dbb" "SELECT writefile('$cd/ans', answer) FROM result_cache WHERE k='$k';" >/dev/null 2>&1 || true ;;
      esac
      a1b_read=$(rp_bytes "$cd/a1b.strace")
    fi

    printf '%s\t%s\t%s\t%s\t%s' "$name" "$i" "$id" "$a0_read" "$a1_read" > "$d/row.tsv"
    printf '\t%s' "$a1b_read" >> "$d/row.tsv"
    printf '%s\t%s' "$i" "$a0_read" >> "$d/casebytes.a0.tsv"
    printf '%s\t%s' "$i" "$a1_read" >> "$d/casebytes.a1.tsv"
    printf '%s\t%s' "$i" "$a1b_read" >> "$d/casebytes.a1b.tsv"
    printf '%s\t%s\t%s\t%s\t%s\n' "$name" "$id" "A0:$a0_status" "A1:$a1_status" "A1b:$a1b_status" >> "$RAW/assertions.tsv"

    # Ladder field lanes
    for spec in "${FIELD_LANES[@]}"; do
      IFS='|' read -r label lbin lflags _ <<< "$spec"
      local store="$d/store-$label" field_file="$d/field-$label"
      local v_status=declined v_read=0 v_inst=0 v_ret=0
      local field; field=$(cat "$field_file" 2>/dev/null)
      if D="$cd" SRC="$src" FMT="$fmt" STORE="$store" FIELD="$field" VOLE_BIN="$lbin" VOLE_FLAGS="$lflags" run_v "$id" "$arg"; then
        v_status=$(assert_case "$id" "$ek" "$exp" "$cd")
        [ -f "$cd/ans.json" ] && { v_inst=$(jqf '.stats.bytes_read' "$cd/ans.json"); v_ret=$(jqf '.stats.bytes_returned' "$cd/ans.json"); }
        local vargv; vargv=$(vole_argv "$fmt" "$id" "$arg")
        if [ "$vargv" = MATERIALIZE ]; then
          # shellcheck disable=SC2086
          strace -f -e trace=read,pread64 -o "$cd/v.$label.strace" "$lbin" materialize --store "$store" --field "$field" --exact --output /dev/null $lflags >/dev/null 2>&1 || true
        else
          # shellcheck disable=SC2086
          strace -f -e trace=read,pread64 -o "$cd/v.$label.strace" "$lbin" $vargv --store "$store" --field "$field" $lflags >/dev/null 2>&1 || true
        fi
        v_read=$(rp_bytes "$cd/v.$label.strace")
      fi
      printf '\t%s' "$v_read" >> "$d/row.tsv"
      printf '%s\t%s\n' "$i" "$v_read" >> "$d/casebytes.$label.tsv"
      printf '%s\t%s\t%s\t%s\t%s\n' "$name" "$id" "$label:${v_status%%:*}" "$v_inst" "$v_ret" >> "$RAW/assertions.tsv"
    done
    printf '\n' >> "$d/row.tsv"
    i=$((i+1))
  done
  cat "$d/row.tsv" >> "$RAW/case_bytes.tsv"
}

# ---------------------------------------------------------------------------
# process_doc NAME
# ---------------------------------------------------------------------------
process_doc() {
  local name=$1
  local fmt src d
  fmt=$(jq -r --arg n "$name" '.documents[]|select(.name==$n).format' "$SCHEDULE")
  src="$CORPUS/$(jq -r --arg n "$name" '.documents[]|select(.name==$n).source' "$SCHEDULE")"
  d="$WORK/$name"; mkdir -p "$d"
  local nc; nc=$(jq -r --arg n "$name" '.documents[]|select(.name==$n).cases|length' "$SCHEDULE")
  local src_len src_sha; src_len=$(bytes "$src"); src_sha=$(sha "$src")
  echo "=== $name fmt=$fmt src=${src_len}B cases=$nc ===" >&2

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

  # ---- A1b one-time (= A1 + copy + result-cache population) ---------------
  local dbb="$d/a1bcache.db"
  local q0 q1 pop_ms pop_cpu a1b_ms a1b_cpu a1b_one_read
  q0=$(now_us)
  cp "$db" "$dbb"
  /usr/bin/time -v -o "$d/a1bpop.time" bash -c 'populate_a1b "$1" "$2" "$3" "$4" "$5"' _ "$dbb" "$d" "$name" "$nc" "$fmt" >/dev/null 2>&1
  q1=$(now_us); pop_ms=$(( (q1-q0)/1000 )); pop_cpu=$(tv_cpu_ms "$d/a1bpop.time")
  a1b_ms=$(( a1_ms + pop_ms ))
  a1b_cpu=$(awk -v a="$a1_cpu" -v b="$pop_cpu" 'BEGIN{printf "%.3f",a+b}')
  ( export SCHEDULE
    strace -f -e trace=read,pread64 -o "$d/a1bpop.strace" bash -c 'cp "$1" "$2"; populate_a1b "$2" "$3" "$4" "$5" "$6"' _ "$db" "$dbb" "$d" "$name" "$nc" "$fmt" >/dev/null 2>&1 || true )
  a1b_one_read=$(( a1_one_read + $(rp_bytes "$d/a1bpop.strace") ))
  sqlite3 "$dbb" "PRAGMA wal_checkpoint(TRUNCATE);" >/dev/null 2>&1 || true
  local dbb_bytes a1b_persistent
  dbb_bytes=$(bytes "$dbb"); a1b_persistent=$(( dbb_bytes + $(bytes "$dbb-wal") + $(bytes "$dbb-shm") ))

  # ---- ladder field lanes: one-time encode + ingest -----------------------
  for spec in "${FIELD_LANES[@]}"; do
    IFS='|' read -r label lbin lflags lrung <<< "$spec"
    local store="$d/store-$label"
    mkdir -p "$store"
    local e0 e1 enc_ms ing_ms enc_cpu ing_cpu field
    e0=$(now_us)
    /usr/bin/time -v -o "$d/$label.enc.time" "$lbin" encode --force raw "$src" "$d/$label.voldoc" > "$RAW/lane.$name.$label.encode.json" 2>/dev/null
    e1=$(now_us); enc_ms=$(( (e1-e0)/1000 )); enc_cpu=$(tv_cpu_ms "$d/$label.enc.time")
    e0=$(now_us)
    /usr/bin/time -v -o "$d/$label.ing.time" "$lbin" field-ingest "$d/$label.voldoc" --store "$store" $lflags > "$RAW/lane.$name.$label.ingest.json" 2>/dev/null
    e1=$(now_us); ing_ms=$(( (e1-e0)/1000 )); ing_cpu=$(tv_cpu_ms "$d/$label.ing.time")
    field=$(jqf '.field' "$RAW/lane.$name.$label.ingest.json")
    echo "$field" > "$d/field-$label"
    local one_ms=$(( enc_ms + ing_ms ))
    local one_cpu; one_cpu=$(awk -v a="$enc_cpu" -v b="$ing_cpu" 'BEGIN{printf "%.3f",a+b}')
    # shellcheck disable=SC2086
    strace -f -e trace=read,pread64 -o "$d/$label.enc.strace" "$lbin" encode --force raw "$src" "$d/$label.s.voldoc" >/dev/null 2>&1 || true
    # shellcheck disable=SC2086
    strace -f -e trace=read,pread64 -o "$d/$label.ing.strace" "$lbin" field-ingest "$d/$label.s.voldoc" --store "$d/$label.sstore" $lflags >/dev/null 2>&1 || true
    local one_read=$(( $(rp_bytes "$d/$label.enc.strace") + $(rp_bytes "$d/$label.ing.strace") ))
    rm -rf "$d/$label.s.voldoc" "$d/$label.sstore"
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$label" "$enc_ms" "$enc_cpu" "$ing_ms" "$ing_cpu" "$one_ms" "$one_cpu" "$one_read" >> "$d/lane_times.tsv"
  done

  # ---- per-case bytes + assertions (uncharged pre-pass) -------------------
  process_case_lanes "$name" "$fmt" "$src" "$d" "$nc" "$db" "$dbb"

  # ---- timed schedule -----------------------------------------------------
  local systems="a0 a1 a1b"
  for spec in "${FIELD_LANES[@]}"; do IFS='|' read -r label _ _ _ <<< "$spec"; systems="$systems $label"; done
  local sys pass n
  for sys in $systems; do
    local lbin="" lflags=""
    if [ "$sys" != a0 ] && [ "$sys" != a1 ] && [ "$sys" != a1b ]; then
      for spec in "${FIELD_LANES[@]}"; do
        IFS='|' read -r label lb lf _ <<< "$spec"; [ "$label" = "$sys" ] && { lbin=$lb; lflags=$lf; }
      done
    fi
    local cbytes="$d/casebytes.$sys.tsv"
    [ -f "$cbytes" ] || cbytes="$d/casebytes.v.tsv"
    export SYS="$sys" CBYTES="$cbytes" BIN="$FULL_BIN" BASE_PY FMT="$fmt" SRC="$src" DB="$db" DBB="$dbb" RUN="$d/run-$sys" NC="$nc" CASES="$d/cases.tsv"
    mkdir -p "$d/run-$sys"
    export STORE="$d/store-$sys" FIELD="$(cat "$d/field-$sys" 2>/dev/null)" VOLE_BIN="$lbin" VOLE_FLAGS="$lflags"
    for (( pass=1; pass<=PASSES; pass++ )); do
      export PASS="$pass"
      for n in $NS_STR; do
        local csv="$RAW/$name.$sys.p$pass.n$n.csv" agg="$d/$sys.p$pass.n$n.time"
        export N="$n" CSV="$csv"
        /usr/bin/time -v -o "$agg" bash "$WORK/lq12.sh" >/dev/null 2>"$RAW/$name.runner.$sys.p$pass.n$n.err" || true
        local cpm rss cw cb otm otb otc
        cpm=$(tv_cpu_ms "$agg" 2>/dev/null || echo 0); rss=$(tv_rss_kb "$agg" 2>/dev/null || echo 0)
        cw=$(awk '{s+=$4} END{printf "%.3f", s/1000.0}' "$csv" 2>/dev/null || echo 0)
        cb=$(awk '{s+=$5} END{printf "%.0f", s}' "$csv" 2>/dev/null || echo 0)
        # one-time costs per system (charged in full and amortized by N)
        case "$sys" in
          a1)  otm=$a1_ms; otb=$a1_one_read; otc=$a1_cpu ;;
          a1b) otm=$a1b_ms; otb=$a1b_one_read; otc=$a1b_cpu ;;
          a0)  otm=0; otb=0; otc=0 ;;
          *)
            local lt one_ms one_cpu one_read
            lt=$(awk -v L="$sys" -F'\t' '$1==L{print}' "$d/lane_times.tsv")
            one_ms=$(echo "$lt" | cut -f6); one_cpu=$(echo "$lt" | cut -f7); one_read=$(echo "$lt" | cut -f8)
            otm=${one_ms:-0}; otc=${one_cpu:-0}; otb=${one_read:-0} ;;
        esac
        printf '{"name":"%s","format":"%s","system":"%s","pass":%d,"n":%d,"cumulative_wall_ms":%.3f,"cumulative_cpu_ms":%.3f,"peak_rss_kb":%d,"cumulative_process_read_bytes":%.0f,"one_time_wall_ms":%.3f,"one_time_cpu_ms":%.3f,"one_time_process_read_bytes":%d}\n' \
          "$name" "$fmt" "$sys" "$pass" "$n" \
          "$(awk -v a="$otm" -v b="$cw" 'BEGIN{printf "%.3f", a+b}')" "$(awk -v a="$otc" -v b="$cpm" 'BEGIN{printf "%.3f", a+b}')" "${rss:-0}" \
          "$(awk -v a="$otb" -v b="$cb" 'BEGIN{printf "%.0f", a+b}')" "$otm" "$otc" "$otb" >> "$ALL_CUM"
      done
    done
  done

  # ---- persistent universes + per-lane JSON -------------------------------
  local lanes_json="$WORK/lanes.$name.json"
  printf '{' > "$lanes_json"
  local first=1
  for spec in "${FIELD_LANES[@]}"; do
    IFS='|' read -r label lbin lflags lrung <<< "$spec"
    local store="$d/store-$label"
    local desc_b seed_b index_b cache_b manifest_b store_b
    desc_b=$(dusb "$store/descriptor"); seed_b=$(dusb "$store/seed")
    index_b=$(dusb "$store/index"); cache_b=$(dusb "$store/cache")
    manifest_b=$(dusb "$store/field"); store_b=$(dusb "$store")
    local lt; lt=$(awk -v L="$label" -F'\t' '$1==L{print}' "$d/lane_times.tsv")
    local l_enc_ms l_enc_cpu l_ing_ms l_ing_cpu l_one_ms l_one_cpu l_one_read
    l_enc_ms=$(echo "$lt" | cut -f2); l_enc_cpu=$(echo "$lt" | cut -f3)
    l_ing_ms=$(echo "$lt" | cut -f4); l_ing_cpu=$(echo "$lt" | cut -f5)
    l_one_ms=$(echo "$lt" | cut -f6); l_one_cpu=$(echo "$lt" | cut -f7); l_one_read=$(echo "$lt" | cut -f8)
    local answered declined afail
    # Answer counts are computed from the raw assertions file (one row per lane).
    answered=$(awk -F'\t' -v n="$name" -v L="$label" '$1==n && $3==L":PASS"{c++} END{print c+0}' "$RAW/assertions.tsv")
    declined=$(awk -F'\t' -v n="$name" -v L="$label" '$1==n && $3==L":declined"{c++} END{print c+0}' "$RAW/assertions.tsv")
    afail=$(awk -F'\t' -v n="$name" -v L="$label" '$1==n && $3 ~ ("^"L":FAIL"){c++} END{print c+0}' "$RAW/assertions.tsv")
    local obj="$RAW/lane.$name.$label.json"
    jq -n \
      --arg lane "$label" --arg rung "$lrung" --arg bin "$lbin" --arg flags "$lflags" \
      --arg name "$name" --arg fmt "$fmt" \
      --argjson answered "$answered" --argjson declined "$declined" --argjson afail "$afail" \
      --argjson l_enc_ms "${l_enc_ms:-0}" --argjson l_enc_cpu "${l_enc_cpu:-0}" \
      --argjson l_ing_ms "${l_ing_ms:-0}" --argjson l_ing_cpu "${l_ing_cpu:-0}" \
      --argjson l_one_ms "${l_one_ms:-0}" --argjson l_one_cpu "${l_one_cpu:-0}" --argjson l_one_read "${l_one_read:-0}" \
      --argjson desc_b "${desc_b:-0}" --argjson seed_b "${seed_b:-0}" --argjson index_b "${index_b:-0}" \
      --argjson cache_b "${cache_b:-0}" --argjson manifest_b "${manifest_b:-0}" --argjson store_b "${store_b:-0}" \
      --slurpfile ing "$RAW/lane.$name.$label.ingest.json" \
      '{lane:$lane, rung:$rung, bin:$bin, flags:$flags,
        one_time:{wall_ms:$l_one_ms,cpu_ms:$l_one_cpu,process_read_bytes:$l_one_read},
        encode:{wall_ms:$l_enc_ms,cpu_ms:$l_enc_cpu},
        ingest:{wall_ms:$l_ing_ms,cpu_ms:$l_ing_cpu},
        ingest_report:$ing[0],
        store_universe_bytes:{descriptor:$desc_b,seed:$seed_b,index:$index_b,manifest:$manifest_b,derived_cache:$cache_b,total:$store_b},
        answered_cases:$answered, declined_cases:$declined, failed_cases:$afail}' > "$obj"
    [ "$first" = 1 ] || printf ',' >> "$lanes_json"
    first=0
    printf '"%s":' "$label" >> "$lanes_json"
    cat "$obj" >> "$lanes_json"
    echo "lane $name/$label: answered=$answered declined=$declined store=${store_b}B" >&2
  done
  printf '}' >> "$lanes_json"

  # ---- per-doc JSON -------------------------------------------------------
  jq -n \
    --arg name "$name" --arg fmt "$fmt" --arg src "$src" \
    --argjson src_len "$src_len" --arg src_sha "$src_sha" \
    --argjson nc "$nc" --argjson ns "$(printf '%s\n' $NS_STR | jq -R 'tonumber' | jq -s '.')" --argjson passes "$PASSES" \
    --argjson a1_ms "$a1_ms" --argjson a1_cpu "$a1_cpu" --argjson a1_one_read "$a1_one_read" \
    --argjson db_bytes "$db_bytes" --argjson wal_bytes "$wal_bytes" --argjson shm_bytes "$shm_bytes" --argjson a1_persistent "$a1_persistent" \
    --argjson a1b_ms "$a1b_ms" --argjson a1b_cpu "$a1b_cpu" --argjson a1b_one_read "$a1b_one_read" \
    --argjson dbb_bytes "$dbb_bytes" --argjson a1b_persistent "$a1b_persistent" \
    --slurpfile a1m "$RAW/$name.a1build.metrics.json" \
    --slurpfile lanes "$lanes_json" \
    '{
      name:$name, format:$fmt,
      source:{path:$src,len:$src_len,sha256:$src_sha},
      schedule:{cases:$nc, ns:$ns, passes:$passes},
      a0:{one_time:{wall_ms:0,cpu_ms:0,process_read_bytes:0}},
      a1:{preprocessing:{wall_ms:$a1_ms,cpu_ms:$a1_cpu,process_read_bytes:$a1_one_read},
          db_bytes:$db_bytes,wal_bytes:$wal_bytes,shm_bytes:$shm_bytes,
          persistent_bytes:$a1_persistent,retained_source_bytes:$src_len,
          extraction_metrics:$a1m[0]},
      a1b:{preprocessing:{wall_ms:$a1b_ms,cpu_ms:$a1b_cpu,process_read_bytes:$a1b_one_read},
          db_bytes:$dbb_bytes,persistent_bytes:$a1b_persistent,retained_source_bytes:$src_len},
      lanes:$lanes[0],
      vole:null
    }' > "$RAW/$name.json" 2>"$RAW/$name.jqerr"
  # The `vole` alias must point at a lane that actually ran in this configuration.
  if [ -z "${LIFETIME_LANES:-}" ] || printf '%s' "$LANES_DESC" | grep -qw a11; then
    jq -n --slurpfile d "$RAW/$name.json" '$d[0] | .vole = .lanes.a11' > "$RAW/$name.json.tmp" \
      && mv "$RAW/$name.json.tmp" "$RAW/$name.json"
  else
    jq -n --slurpfile d "$RAW/$name.json" '$d[0] | .vole = (.lanes | to_entries | .[0].value)' > "$RAW/$name.json.tmp" \
      && mv "$RAW/$name.json.tmp" "$RAW/$name.json"
  fi
  [ -s "$RAW/$name.json" ] || { echo "FATAL: empty json for $name" >&2; cat "$RAW/$name.jqerr" >&2; exit 1; }
}

# ===========================================================================
# Run.
# ===========================================================================
for name in $DOCS; do process_doc "$name"; done

# ===========================================================================
# A10 (cross-document sharing) — ingest-side proxy.
# The per-document lifetime lanes use one store per document, so cross-document
# content sharing cannot manifest there by construction. Re-ingest every
# document's exact descriptor into ONE shared store and report the shared-resource
# counters: the honest ingest-side effect of the A10 capability.
# ===========================================================================
A10_JSON="$RAW/a10_shared_ingest.json"
SHARED="$WORK/shared_store"; mkdir -p "$SHARED"
: > "$WORK/a10.jsonl"
for name in $DOCS; do
  desc="$WORK/$name/a11.voldoc"
  [ -f "$desc" ] || desc="$WORK/$name/a6.voldoc"
  [ -f "$desc" ] || continue
  "$FULL_BIN" field-ingest "$desc" --store "$SHARED" > "$WORK/a10.$name.json" 2>/dev/null || true
  jq -c --arg n "$name" '{name:$n, shared_resource_ids:(.shared_resource_ids//0), shared_resource_bytes:(.shared_resource_bytes//0), nodes_id_shared:(.nodes_id_shared//0), seed_bytes_written:(.seed_bytes_written//0)}' "$WORK/a10.$name.json" >> "$WORK/a10.jsonl" 2>/dev/null || true
done
jq -s '{mode:"all documents ingested into one shared store (filesystem backend)",
        note:"ingest-side representation fact; not a per-query lifetime metric",
        documents:.}' "$WORK/a10.jsonl" > "$A10_JSON" 2>/dev/null || echo '{"documents":[]}' > "$A10_JSON"

# ===========================================================================
# receipt.json
# ===========================================================================
DOCFILES=""
for name in $DOCS; do DOCFILES="$DOCFILES $RAW/$name.json"; done
# shellcheck disable=SC2086
jq -s '[.[]|select(has("name"))]' $DOCFILES > "$WORK/docs.json"
cp "$ALL_CUM" "$RAW/cumulative.jsonl"

# The pre-registered rung table (status + switch) is embedded verbatim.
jq -n \
  --slurpfile env "$RAW/environment.json" \
  --slurpfile docs "$WORK/docs.json" \
  --slurpfile cum "$WORK/cumulative.jsonl" \
  --slurpfile sched "$SCHEDULE" \
  --slurpfile a10 "$A10_JSON" \
  '{
    campaign:"phase12-lifetime-ablations",
    phase:"Phase 12.11b — mixed PDF/DOCX/EPUB lifetime court with the required ablation ladder (A0/A1/A1b/A2–A11)",
    environment:$env[0],
    schedule:$sched[0],
    rungs:$sched[0].ladder,
    a10_shared_ingest:$a10[0],
    accounting_universes_adr0027:{
      source:"original document bytes (archival reality)",
      descriptor_store:"exact .voldoc descriptor + externalized objects (normative reconstruction)",
      procedural_field:"seed DAG nodes + hierarchical index bytes (normative for observations)",
      derived_cache:"disposable observation caches (never normative)"
    },
    boundary_note:"Per-query bytes are the process read/pread64 total under strace for every lane (one boundary). VOLE ObserveStats.bytes_read is reported separately in raw/assertions.tsv and is never summed with process reads. CPU/peak-RSS are aggregate measurements over the first N queries.",
    documents:$docs[0],
    cumulative:$cum
  }' > "$OUTDIR/receipt.json"
rm -f "$WORK/lanes.jpg" 2>/dev/null

# ===========================================================================
# Crossovers + losses + SUMMARY.md
# ===========================================================================
{
  echo "# Phase 12.11b — mixed-format lifetime court **with the ablation ladder**"
  echo
  echo "Generated by \`tools/phase12-lifetime-court.sh\` inside the pinned \`doc-baseline\` service."
  echo "Corpus: 12 deterministic documents (alpha/bravo/charlie/delta x PDF/DOCX/EPUB) from"
  echo "\`tools/fixtures/phase12-corpus-gen.py\`; schedule pre-registered in"
  echo "\`tools/fixtures/phase12-lifetime-schedule.json\` (round-robin, frozen before measurement)."
  echo
  jq -r '"Commit: `\(.environment.git.commit_short)` (dirty: \(.environment.git.dirty))  \n" +
    "Service image `vole-document/doc-baseline:1.99.0` (id `\(.environment.doc_baseline_image_id)`), base `\(.environment.base_image)`.  \n" +
    "rustc `\(.environment.toolchain.rustc)`, cargo `\(.environment.toolchain.cargo)`, python `\(.environment.toolchain.python)`; Cargo.lock sha256 `\(.environment.cargo_lock_sha256)`; arch `\(.environment.arch)`.  \n" +
    "Oracles: sqlite3 \(.environment.oracles.sqlite3), poppler \(.environment.oracles.poppler_pdftotext), strace \(.environment.oracles.strace).  \n" +
    "Schedule: \(.schedule.documents|length) documents, ns=\(.schedule.ns|join(",")), passes=\(.schedule.passes).  \n" +
    "Ladder lanes run: `\(.environment.env_affecting_semantics.LIFETIME_LANES)`."' "$OUTDIR/receipt.json"
  echo
  echo "## Ablation rungs (pre-registered; which ran, which are not separable)"
  echo
  echo "| rung | status | switch / lane | note |"
  echo "|---|---|---|---|"
  jq -r '.rungs[] | "| \(.rung) | \(.status) | \(.switch) | \(.note) |"' "$OUTDIR/receipt.json"
  echo
  echo "## Mechanism attribution — answered cases and N=1000 cumulative cost per rung"
  echo
  echo "A capability-declining rung answers fewer cases and is charged **0 bytes** for a"
  echo "declined case; the answered-set delta is what attributes the mechanism."
  echo
  echo "| document | lane | rung | answered | declined | failed | N=1000 warm wall ms | cpu ms | read B | store B |"
  echo "|---|---|---|---:|---:|---:|---:|---:|---:|---:|"
  # The per-lane rows are derived from cumulative.jsonl joined with the lane JSON.
  jq -r --argjson max "$MAX_NS" \
    '($P) as $PASS
     | .documents[] as $d
     | ($d.lanes | to_entries[]) as $e
     | ([.cumulative[]|select(.name==$d.name and .system==$e.key and .pass==$PASS and .n==$max)][0] // {}) as $c
     | "| \($d.name) | \($e.key) | \($e.value.rung) | \($e.value.answered_cases) | \($e.value.declined_cases) | \($e.value.failed_cases) | \($c.cumulative_wall_ms // "-") | \($c.cumulative_cpu_ms // "-") | \($c.cumulative_process_read_bytes // "-") | \($e.value.store_universe_bytes.total) |"' \
    --argjson P "$PASSES" "$OUTDIR/receipt.json"
  echo
  echo "## Per-document one-time and persistent cost (ladder)"
  echo
  echo "| document | fmt | src B | A1 ms | A1 B | A1b ms | A1b B | a2 ms | a3 ms | a4 ms | a5 ms | a6 ms | a9 ms | a11 ms |"
  echo "|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|"
  jq -r '.documents[] | "| \(.name) | \(.format) | \(.source.len) | \(.a1.preprocessing.wall_ms) | \(.a1.persistent_bytes) | \(.a1b.preprocessing.wall_ms) | \(.a1b.persistent_bytes) | \(.lanes.a2.one_time.wall_ms // "-") | \(.lanes.a3.one_time.wall_ms // "-") | \(.lanes.a4.one_time.wall_ms // "-") | \(.lanes.a5.one_time.wall_ms // "-") | \(.lanes.a6.one_time.wall_ms // "-") | \(.lanes.a9.one_time.wall_ms // "-") | \(.lanes.a11.one_time.wall_ms // "-") |"' "$OUTDIR/receipt.json"
  echo
  echo "## Crossover (warm pass): where the ordering flips (or never)"
  echo
  echo "\`V wins\` is the range of N at which A11's cumulative is strictly below the baseline;"
  echo "\`A1 leads from\` is the first N at which the baseline's cumulative is <= A11's (empty ="
  echo "the baseline never leads). A one-point range means A11 led only at that N."
  echo
  echo "| document | fmt | metric | A11 wins vs A0 | A11 wins vs A1 | A1 leads from | A1b leads from |"
  echo "|---|---|---|---|---|---|---|"
  jq -r '
    ([.cumulative[].pass]|max) as $P
    | .documents[] as $d
    | ($d.name) as $nm
    | ([.cumulative[]|select(.name==$nm and .pass==$P and .system=="a11")]|sort_by(.n)) as $vv
    | ([.cumulative[]|select(.name==$nm and .pass==$P and .system=="a0")]|sort_by(.n)) as $v0
    | ([.cumulative[]|select(.name==$nm and .pass==$P and .system=="a1")]|sort_by(.n)) as $v1
    | ([.cumulative[]|select(.name==$nm and .pass==$P and .system=="a1b")]|sort_by(.n)) as $v1b
    | def rng($a;$b;k): ([range(0;($a|length)) as $i|select(($a[$i][k]//1e18) < ($b[$i][k]//1e18))|$a[$i].n]) as $w
        | if ($w|length)==0 then "never" elif $w[-1]==$a[-1].n then "N>="+($w[0]|tostring)+" (all)" else "N="+($w[0]|tostring)+".."+($w[-1]|tostring) end;
      def lead($a;$b;k): ([range(0;($b|length)) as $i|select(($b[$i][k]//1e18) <= ($a[$i][k]//1e18))|$b[$i].n][0] // "never");
      ["cumulative_wall_ms","cumulative_cpu_ms","cumulative_process_read_bytes"][] as $k
      | "| \($d.name) | \($d.format) | \($k) | \(rng($vv;$v0;$k)) | \(rng($vv;$v1;$k)) | \(lead($vv;$v1;$k)) | \(lead($vv;$v1b;$k)) |"
  ' "$OUTDIR/receipt.json"
  echo
  echo "## Ladder movement (N=1000 warm; A11 = full system)"
  echo
  echo "| document | metric | A0 | A1 | A1b | A2 | A3 | A4 | A5 | A6 | A9 | A11 |"
  echo "|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|"
  jq -r --argjson max "$MAX_NS" '
    ([.cumulative[].pass]|max) as $P
    | (.cumulative) as $cum
    | .documents[] as $d
    | [ "cumulative_process_read_bytes", "cumulative_cpu_ms", "cumulative_wall_ms" ][] as $k
    | (["a0","a1","a1b","a2","a3","a4","a5","a6","a9","a11"]
       | map(. as $s | ([$cum[]|select(.name==$d.name and .system==$s and .pass==$P and .n==$max)][0][$k] // "-"))
       | map(if type=="number" then (.*100|round/100|tostring) else tostring end)
       | join(" | ")) as $row
    | "| \($d.name) | \($k) | \($row) |"' "$OUTDIR/receipt.json"
  echo
  echo "## A10 — cross-document sharing (ingest-side proxy)"
  echo
  jq -r '"All 12 documents ingested into one shared store: shared_resource_ids=\([.a10_shared_ingest.documents[].shared_resource_ids]|add), shared_resource_bytes=\([.a10_shared_ingest.documents[].shared_resource_bytes]|add), nodes_id_shared=\([.a10_shared_ingest.documents[].nodes_id_shared]|add).  \n" +
    "The per-document lifetime lanes use one store per document, so A10 cannot manifest there by construction; this is an ingest-side representation fact, not a per-query lifetime metric (see the 12.8 share receipt for the reuse-work court)."' "$OUTDIR/receipt.json"
  echo
  echo "## Assertion failures (must be empty)"
  echo
  awk -F'\t' '$3 ~ /FAIL/ || $4 ~ /FAIL/ || $5 ~ /FAIL/ {print "- "$0}' "$RAW/assertions.tsv"
  echo
  echo "## Declines per document and lane (a capability gap, not a failure)"
  echo
  awk -F'\t' '{for(i=3;i<=NF;i++){if($i ~ /:declined$/){split($i,a,":"); c[$1"\t"a[1]]++}}} END{for(k in c) printf "- %s declined %d\n", k, c[k]}' "$RAW/assertions.tsv" | sort
} > "$OUTDIR/SUMMARY.md"

echo "phase12-lifetime-court: wrote $OUTDIR" >&2

cat > "$OUTDIR/commands.txt" <<EOF
# Phase 12.11b mixed-format lifetime court with the ablation ladder — exact commands
# Commit under test: $COMMIT (branch $(git rev-parse --abbrev-ref HEAD 2>/dev/null)); dirty: $DIRTY
# Service image: vole-document/doc-baseline:1.99.0 (id $IMAGE_ID); base $BASE_STABLE
# rustc $RUSTC_V, cargo $CARGO_V, python $PY_V, sqlite3 $SQLITE_V, poppler $POPPLER_V, strace $STRACE_V
# Cargo.lock sha256: $LOCK_SHA; arch $ARCH
# All commands run inside pinned, capped images; nothing on the host except git + docker.

# 1) deterministic mixed corpus (Generators reuse the 12.9 triplet builders):
docker compose run --rm --no-TTY producers python3 tools/fixtures/phase12-corpus-gen.py evidence/scratch/phase12-lifetime-corpus

# 2) pre-register the schedule + ladder BEFORE measuring:
docker compose run --rm --no-TTY doc-baseline python3 tools/fixtures/phase12-schedule.py \\
    evidence/scratch/phase12-lifetime-corpus tools/fixtures/phase12-lifetime-schedule.json

# 3) the court:
docker compose run --rm --no-TTY -e HOST_IMAGE_ID=$IMAGE_ID \\
    doc-baseline bash tools/phase12-lifetime-court.sh $OUTDIR

# Inside the court, per document (pdf/docx/epub), the ladder binaries are built in this service:
#   cargo build --locked --all-features                          -> A9/A11 (--entropyfs / default)
#   cargo build --locked --no-default-features --features rans,store,field
#   cargo build --locked --no-default-features --features rans,store,field,package
#   cargo build --locked --no-default-features --features rans,store,field,package,opc,docx,epub
# then per lane:
#   A2  (field only)            : encode + field-ingest; semantic selectors decline
#   A3  (+package)              : ZIP physical members; semantic selectors decline
#   A4  (+opc/docx/epub, --no-cache): native package graph, progressive inversion, no persistent reuse
#   A6  (+opc/docx/epub)        : native package graph, persistent DerivedCache on
#   A9  (all-features, --entropyfs): EntropyFS backend
#   A11 (all-features)          : full system (== the prior V lane)
#   A1 one-time: python3 tools/fixtures/phase12-baseline.py build --format F --source SRC --db DB
#   A1b one-time: A1 + cp DB + populate_a1b (result_cache materialized views, one SQL source of truth)
#   A0         : pdftotext/pdfinfo/dd/cat (pdf) or python3 phase12-baseline.py query (docx/epub)
#   per-query bytes: strace -f -e trace=read,pread64 (every lane)
#   CPU + peak RSS : /usr/bin/time -v on a batch of the first N queries
#   schedule   : round-robin over the committed case list, N in {1,10,100,1000}, passes 2 (cold+warm)
EOF
echo "commands: $OUTDIR/commands.txt" >&2
