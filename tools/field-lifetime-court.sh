#!/usr/bin/env bash
# Phase-11.12 priority #7 — the 1 / 10 / 100 / 1,000-query *lifetime-cost* court,
# with crossover detection, against a fair preprocessed baseline.
#
# This is a *complement* to tools/field-court.sh (the §77 headline court). Where
# field-court.sh measures one cold/warm observation per case, this court measures
# the **cumulative lifetime cost** of answering N observations for
# N in {1,10,100,1000} and reports where (if anywhere) VOLE's cumulative cost
# drops below a conventional system's.
#
# Systems, all on the same corpus, same query set, same accounting:
#
#   VOLE field (Phase 11)   one-time `encode` + `field-ingest` (charged in full:
#                           wall, CPU, bytes read, bytes written) then N
#                           observations answered in a *fresh process each*
#                           (process startup charged). A warm pass (the same
#                           schedule answered a second time, derived cache warm)
#                           is measured too, so the marginal amortisation of the
#                           one-time work is visible.
#   A0 raw PDF tooling      no preprocessing; each query is
#                           `pdftotext -f P -l P` (Poppler) with read/pread
#                           bytes (strace) + wall + RSS.
#   A1 preprocessed SQLite  one-time per-page `pdftotext` -> indexed SQLite table
#                           (charged in full), then the same N queries.
#
# The comparable lane is **page text**, the only surface A0/A1 can answer. VOLE
# additionally answers a 5-surface pre-registered query set (page text, page
# structure, preview, object bytes, decoded stream); that lane is reported for
# VOLE alone because the conventional baselines cannot answer those surfaces.
#
# Honesty rules enforced here (ADR-0027, ADR-0017):
#   * VOLE's ingest is charged in full and never hidden. The SQLite baseline gets
#     its one-time extraction charged in full too. No store root is compared to a
#     whole file. Four accounting universes stay separate.
#   * `total_bytes_read` is defined **per system** and stated: for VOLE it is the
#     instrumented `ObserveStats.bytes_read` (the document/procedural byte classes
#     it actually fetched: descriptor+manifest+index+seed); for A0/A1 it is the
#     process `read`/`pread64` byte total under `strace` (which also includes
#     library reads). The differing definitions are never conflated, and a VOLE
#     process-level strace cross-check is recorded alongside.
#   * CPU is measured on an **aggregate batch** (all N queries wrapped in one
#     `/usr/bin/time -v` run) because a single sub-millisecond query is below the
#     10 ms resolution of `/usr/bin/time`; the batch CPU is reported as cumulative.
#     Peak RSS is the batch maximum. The wrapping shell's own CPU is a small
#     constant included for every system.
#   * Wins AND losses are recorded; a crossover that does not exist is stated
#     plainly.
#
# Usage (inside the `db-baseline` service):
#   bash tools/field-lifetime-court.sh OUTDIR
# Env:
#   LIFETIME_NS      space-separated N values (default "1 10 100 1000")
#   LIFETIME_PASSES  number of passes over the schedule (default 2: cold + warm)
#   LIFETIME_DOCS    space-separated explicit PDFs; default = all
#                    evidence/corpus/phase7-producers/*.pdf plus one generated
#                    pdf-make-large document.
#   LIFETIME_NO_GENERATED  set to 1 to skip the generated document
set -euo pipefail

cd /work
LC_ALL=C
export LC_ALL

OUTDIR=${1:-evidence/scratch/field-lifetime}
NS_STR=${LIFETIME_NS:-"1 10 100 1000"}
PASSES=${LIFETIME_PASSES:-2}
BIN=${VOLE_BIN:-./target/debug/vole-document}

mkdir -p "$OUTDIR/raw"
RAW="$OUTDIR/raw"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

if [ ! -x "$BIN" ]; then
  echo "field-lifetime-court: $BIN not found; building" >&2
  cargo build --locked --all-features 1>&2
fi

# ---------------------------------------------------------------------------
# Helpers (adapted from tools/field-court.sh).
# ---------------------------------------------------------------------------
bytes() { stat -c %s "$1" 2>/dev/null || echo 0; }
sha() { sha256sum "$1" | cut -d' ' -f1; }
dusb() { du -sb "$1" 2>/dev/null | cut -f1; }
# read/pread64 byte total under `strace -f -e trace=read,pread64`.
rp_bytes() {
  awk 'match($0,/^[0-9 ]*(read|pread64)\(/){if(match($0,/ = [0-9]+$/)){s+=substr($0,RSTART+2)+0}} match($0,/^[0-9 ]*<\.\.\. (read|pread64) resumed>/){if(match($0,/ = [0-9]+$/)){s+=substr($0,RSTART+2)+0}} END{print s+0}' "$1"
}
now_us() { echo $(( ${EPOCHREALTIME/./} )); }   # microseconds, no fork
json_num() { local s=$1 k=$2; if [[ $s =~ \"$k\":([0-9]+) ]]; then echo "${BASH_REMATCH[1]}"; else echo 0; fi; }
# /usr/bin/time -v parsers.
tv_cpu_ms() { awk -F': ' '/User time \(seconds\)/{u=$2} /System time \(seconds\)/{s=$2} END{printf "%.3f", (u+s)*1000}' "$1"; }
tv_rss_kb() { awk -F': ' '/Maximum resident set size/{print $2}' "$1" | tr -d ' '; }

# Aggregate query runner (fresh process per query). Env: MODE BIN STORE FIELD
# SCHED SRC DB N OBJ STREAM PAGDIR NPAGES.
cat > "$WORK/lq.sh" <<'LQ'
#!/usr/bin/env bash
set -u
i=1
while [ "$i" -le "$N" ]; do
  p=$(( (i-1) % SCHED + 1 ))
  case "$MODE" in
    vole-text) "$BIN" observe --store "$STORE" --field "$FIELD" --page "$p" --kind text >/dev/null 2>&1 ;;
    vole-multi)
      s=$(( (i-1) % 5 )); mp=$(( ((i-1)/5) % SCHED + 1 ))
      case "$s" in
        0) "$BIN" observe --store "$STORE" --field "$FIELD" --page "$mp" --kind text >/dev/null 2>&1 ;;
        1) "$BIN" observe --store "$STORE" --field "$FIELD" --page "$mp" --kind structure >/dev/null 2>&1 ;;
        2) "$BIN" observe --store "$STORE" --field "$FIELD" --page "$mp" --kind preview >/dev/null 2>&1 ;;
        3) [ -n "$OBJ" ] && "$BIN" observe --store "$STORE" --field "$FIELD" --object "$OBJ" --kind exact >/dev/null 2>&1 ;;
        4) [ -n "$STREAM" ] && "$BIN" observe --store "$STORE" --field "$FIELD" --stream "$STREAM" --kind decoded >/dev/null 2>&1 ;;
      esac ;;
    a0) pdftotext -f "$p" -l "$p" "$SRC" - >/dev/null 2>&1 ;;
    a1) sqlite3 "$DB" "SELECT text FROM pages WHERE page=$p;" >/dev/null 2>&1 ;;
    prep)
      q=1
      while [ "$q" -le "$NPAGES" ]; do
        pdftotext -f "$q" -l "$q" "$SRC" "$PAGDIR/p$q.txt" 2>/dev/null || : > "$PAGDIR/p$q.txt"
        q=$((q+1))
      done
      {
        echo "PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF;"
        echo "CREATE TABLE pages(page INTEGER PRIMARY KEY, text BLOB);"
        q=1
        while [ "$q" -le "$NPAGES" ]; do
          echo "INSERT INTO pages(page,text) VALUES($q, readfile('$PAGDIR/p$q.txt'));"
          q=$((q+1))
        done
        echo "CREATE INDEX idx_pages_page ON pages(page);"
      } | sqlite3 "$DB" >/dev/null ;;
  esac
  i=$((i+1))
done
LQ

# emit_cum CSV NS ONE_TIME_MS ONE_TIME_BYTES ONE_TIME_CPU_MS LANE CPUMAP
# CSV rows: "i wall_us bytes_read bytes_returned"; CPUMAP: "n:cpu_ms:rss_kb ...".
emit_cum() {
  awk -v ns="$2" -v otm="$3" -v otb="$4" -v otc="$5" -v lane="$6" -v cmap="$7" '
    BEGIN {
      n = split(ns, N, " ");
      m = split(cmap, CE, " ");
      for (k=1;k<=m;k++) if (CE[k] != "") { split(CE[k], a, ":"); cpu[a[1]]=a[2]; rss[a[1]]=a[3] }
    }
    {
      i=$1; w=$2; b=$3; r=$4; cw+=w; cb+=b; cr+=r;
      for (k=1;k<=n;k++) if (i==N[k]) {
        printf("{\"lane\":\"%s\",\"n\":%d,\"one_time_wall_ms\":%.3f,\"cumulative_wall_ms\":%.3f,\"cumulative_bytes_read\":%.0f,\"cumulative_bytes_returned\":%.0f,\"cumulative_cpu_ms\":%.3f,\"peak_rss_kb\":%.0f,\"marginal_wall_ms\":%.3f,\"marginal_bytes_read\":%.0f,\"marginal_bytes_returned\":%.0f}\n",
          lane, N[k], otm + 0.0, otm + cw/1000.0, otb+cb, cr, (otc + 0.0) + cpu[N[k]], rss[N[k]], w/1000.0, b, r);
      }
    }' "$1"
}

# Discover a valid object/stream selector number (bounded; setup, not charged).
discover_selector() { # $1 store $2 field $3 flag $4 kind $5 max
  local n=1
  while [ "$n" -le "$5" ]; do
    if "$BIN" observe --store "$1" --field "$2" --"$3" "$n" --kind "$4" >/dev/null 2>&1; then echo "$n"; return 0; fi
    n=$((n+1))
  done
  echo ""
}

# ---------------------------------------------------------------------------
# Environment capture.
# ---------------------------------------------------------------------------
RUSTC_V=$(rustc --version 2>/dev/null | sed 's/^rustc //' || echo unknown)
CARGO_V=$(cargo --version 2>/dev/null | sed 's/^cargo //' || echo unknown)
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
QPDF_V=$(qpdf --version | head -1 | sed 's/^qpdf version //')
STRACE_V=$(strace --version 2>&1 | head -1 | sed 's/^strace -- version //')
JQ_V=$(jq --version)
GZIP_V=$(gzip --version | head -1 | sed 's/^gzip //')
ZSTD_V=$(zstd --version 2>&1 | sed -n 's/.*v\([0-9][0-9.]*\).*/\1/p' | head -1)
XZ_V=$(xz --version | head -1 | sed 's/^xz (XZ Utils) //')

jq -n \
  --arg commit "$COMMIT" --arg commit_short "$COMMIT_SHORT" --arg dirty "$DIRTY" \
  --arg lock_sha "$LOCK_SHA" --arg arch "$ARCH" \
  --arg rustc "$RUSTC_V" --arg cargo "$CARGO_V" \
  --arg base_stable "$BASE_STABLE" --arg base_tools "$BASE_TOOLS" --arg image_id "$IMAGE_ID" \
  --arg sqlite "$SQLITE_V" --arg poppler "$POPPLER_V" --arg qpdf "$QPDF_V" \
  --arg strace "$STRACE_V" --arg jq "$JQ_V" \
  --arg gzip "$GZIP_V" --arg zstd "$ZSTD_V" --arg xz "$XZ_V" \
  --arg bin "$BIN" --arg ns "$NS_STR" --argjson passes "$PASSES" \
  '{
     git: {commit: $commit, commit_short: $commit_short, dirty: $dirty},
     arch: $arch,
     cargo_lock_sha256: $lock_sha,
     toolchain: {rustc: $rustc, cargo: $cargo},
     images: {base_stable_digest: $base_stable, base_tools_digest: $base_tools, db_baseline_image_id: $image_id},
     oracles: {sqlite3: $sqlite, poppler_pdftotext: $poppler, qpdf: $qpdf, strace: $strace, jq: $jq,
               gzip: $gzip, zstd: $zstd, xz: $xz},
     env_affecting_semantics: {LC_ALL: "C", VOLE_BIN: $bin, LIFETIME_NS: $ns, LIFETIME_PASSES: $passes},
     camera: "fresh process per query; wall measured with bash EPOCHREALTIME (no fork); CPU/peak-RSS measured on an aggregate batch with /usr/bin/time -v"
   }' > "$RAW/environment.json"

# ---------------------------------------------------------------------------
# Corpus.
# ---------------------------------------------------------------------------
DOCS=()
if [ -n "${LIFETIME_DOCS:-}" ]; then
  # shellcheck disable=SC2206
  DOCS=( ${LIFETIME_DOCS} )
else
  for f in /work/evidence/corpus/phase7-producers/*.pdf; do
    [ -f "$f" ] && DOCS+=("$f")
  done
  if [ "${LIFETIME_NO_GENERATED:-0}" != "1" ]; then
    mkdir -p "$WORK/gen"
    "$BIN" pdf-make-large "$WORK/gen" 50 > "$RAW/generated.json"
    DOCS+=("$WORK/gen/large.pdf")
  fi
fi
echo "corpus: ${DOCS[*]}" >&2

NMAX=0; for n in $NS_STR; do [ "$n" -gt "$NMAX" ] && NMAX=$n; done

# ---------------------------------------------------------------------------
# One document, all three systems.
# ---------------------------------------------------------------------------
process_doc() {
  local src=$1 name=$2
  local d="$WORK/$name"
  mkdir -p "$d/store"
  local src_len src_sha npages
  src_len=$(bytes "$src"); src_sha=$(sha "$src")
  npages=$(pdfinfo "$src" 2>/dev/null | awk -F: '/^Pages:/{gsub(/ /,"",$2); print $2}')
  case "$npages" in ''|*[!0-9]*) npages=1 ;; esac
  echo "=== doc=$name src=${src_len}B pages=$npages" >&2

  # ---- VOLE one-time: encode + field-ingest (charged in full) --------------
  local desc="$d/$name.voldoc"
  local t0 t1 enc_ms ing_ms enc_cpu ing_cpu
  t0=$(now_us)
  /usr/bin/time -v -o "$d/enc.time" "$BIN" encode "$src" "$desc" > "$RAW/$name.encode.json"
  t1=$(now_us); enc_ms=$(( (t1-t0)/1000 )); enc_cpu=$(tv_cpu_ms "$d/enc.time")
  local desc_len cand
  desc_len=$(bytes "$desc"); cand=$(jq -r '.candidate // "unknown"' "$RAW/$name.encode.json" 2>/dev/null || echo unknown)
  t0=$(now_us)
  /usr/bin/time -v -o "$d/ing.time" "$BIN" field-ingest "$desc" --store "$d/store" > "$RAW/$name.ingest.json"
  t1=$(now_us); ing_ms=$(( (t1-t0)/1000 )); ing_cpu=$(tv_cpu_ms "$d/ing.time")
  local field
  field=$(jq -r .field "$RAW/$name.ingest.json")
  local one_time_wall_ms=$(( enc_ms + ing_ms ))
  local one_time_cpu_ms
  one_time_cpu_ms=$(awk -v a="$enc_cpu" -v b="$ing_cpu" 'BEGIN{printf "%.3f", a+b}')

  # ---- VOLE one-time bytes read (strace encode + ingest) -------------------
  strace -f -e trace=read,pread64 -o "$RAW/$name.encode.strace" \
    "$BIN" encode "$src" "$d/strace-desc.voldoc" >/dev/null 2>&1 || true
  strace -f -e trace=read,pread64 -o "$RAW/$name.ingest.strace" \
    "$BIN" field-ingest "$d/strace-desc.voldoc" --store "$d/strace-store" >/dev/null 2>&1 || true
  local enc_read ing_read one_time_read
  enc_read=$(rp_bytes "$RAW/$name.encode.strace")
  ing_read=$(rp_bytes "$RAW/$name.ingest.strace")
  one_time_read=$(( enc_read + ing_read ))
  rm -rf "$d/strace-desc.voldoc" "$d/strace-store"

  # ---- selector discovery (setup; not charged) -----------------------------
  local obj stream
  obj=$(discover_selector "$d/store" "$field" object exact 64)
  stream=$(discover_selector "$d/store" "$field" stream decoded 300)

  # ---- per-page byte/length maps for the baselines -------------------------
  declare -A A0_RB A0_TL A1_RB A1_TL
  local cap=$npages; [ "$cap" -gt 500 ] && cap=500
  local p
  for (( p=1; p<=cap; p++ )); do
    strace -f -e trace=read,pread64 -o "$d/a0-$p.strace" \
      pdftotext -f "$p" -l "$p" "$src" "$d/a0-$p.txt" >/dev/null 2>&1 || true
    A0_RB[$p]=$(rp_bytes "$d/a0-$p.strace"); A0_TL[$p]=$(bytes "$d/a0-$p.txt")
  done

  # ---- A1 one-time preprocessing (timed, CPU measured, charged in full) ----
  local pagdir="$d/a1pages"; mkdir -p "$pagdir"
  local db="$d/pages.db"
  local pp0 pp1 prep_ms prep_cpu
  pp0=$(now_us)
  MODE=prep BIN="$BIN" SRC="$src" PAGDIR="$pagdir" NPAGES="$npages" DB="$db" N=1 SCHED=1 \
    /usr/bin/time -v -o "$d/prep.time" bash "$WORK/lq.sh" >/dev/null 2>&1 || true
  pp1=$(now_us); prep_ms=$(( (pp1-pp0)/1000 )); prep_cpu=$(tv_cpu_ms "$d/prep.time")
  local db_bytes; db_bytes=$(bytes "$db")
  local a1_one_time_read=0
  for (( p=1; p<=cap; p++ )); do a1_one_time_read=$(( a1_one_time_read + A0_RB[$p] )); done
  # A1 per-page query read bytes + text length.
  for (( p=1; p<=cap; p++ )); do
    strace -f -e trace=read,pread64 -o "$d/a1-$p.strace" \
      sqlite3 "$db" "SELECT text FROM pages WHERE page=$p;" > "$d/a1-$p.bin" 2>/dev/null || true
    A1_RB[$p]=$(rp_bytes "$d/a1-$p.strace"); A1_TL[$p]=$(bytes "$d/a1-$p.bin")
  done

  # ---- query schedule (pre-registered) -------------------------------------
  local sched=$npages; [ "$sched" -gt 10 ] && sched=10

  # ---- VOLE text lane: PASSES passes, fresh process per query --------------
  local vole_ctx=(BIN="$BIN" STORE="$d/store" FIELD="$field" SCHED="$sched" OBJ="${obj:-}" STREAM="${stream:-}")
  : > "$RAW/$name.vole.text.csv"
  local pass i page t0b t1b w br bret o
  for (( pass=1; pass<=PASSES; pass++ )); do
    for (( i=1; i<=NMAX; i++ )); do
      page=$(( (i-1) % sched + 1 ))
      t0b=$(now_us)
      if "$BIN" observe --store "$d/store" --field "$field" --page "$page" --kind text > "$d/obs.json" 2>/dev/null; then
        t1b=$(now_us); o=$(<"$d/obs.json")
        br=$(json_num "$o" bytes_read); bret=$(json_num "$o" bytes_returned)
      else
        t1b=$(now_us); br=0; bret=0
      fi
      echo "$pass $i $page $(( t1b-t0b )) $br $bret" >> "$RAW/$name.vole.text.csv"
    done
  done

  # ---- VOLE multi-surface lane (VOLE alone) --------------------------------
  : > "$RAW/$name.vole.multi.csv"
  if [ -n "$obj" ] || [ -n "$stream" ]; then
    for (( pass=1; pass<=PASSES; pass++ )); do
      for (( i=1; i<=NMAX; i++ )); do
        local s=$(( (i-1) % 5 )); local mp=$(( ((i-1)/5) % sched + 1 )); local ok=1
        t0b=$(now_us)
        case "$s" in
          0) "$BIN" observe --store "$d/store" --field "$field" --page "$mp" --kind text > "$d/obs.json" 2>/dev/null || ok=0 ;;
          1) "$BIN" observe --store "$d/store" --field "$field" --page "$mp" --kind structure > "$d/obs.json" 2>/dev/null || ok=0 ;;
          2) "$BIN" observe --store "$d/store" --field "$field" --page "$mp" --kind preview > "$d/obs.json" 2>/dev/null || ok=0 ;;
          3) if [ -n "$obj" ]; then "$BIN" observe --store "$d/store" --field "$field" --object "$obj" --kind exact > "$d/obs.json" 2>/dev/null || ok=0; else ok=0; fi ;;
          4) if [ -n "$stream" ]; then "$BIN" observe --store "$d/store" --field "$field" --stream "$stream" --kind decoded > "$d/obs.json" 2>/dev/null || ok=0; else ok=0; fi ;;
        esac
        t1b=$(now_us)
        if [ "$ok" = 1 ]; then o=$(<"$d/obs.json"); br=$(json_num "$o" bytes_read); bret=$(json_num "$o" bytes_returned); else br=0; bret=0; fi
        echo "$pass $i $s $(( t1b-t0b )) $br $bret" >> "$RAW/$name.vole.multi.csv"
      done
    done
  fi

  # ---- VOLE strace cross-check (cold + warm text observation) --------------
  strace -f -e trace=read,pread64 -o "$RAW/$name.vole.cold.strace" \
    "$BIN" observe --store "$d/store" --field "$field" --page 1 --kind text >/dev/null 2>&1 || true
  strace -f -e trace=read,pread64 -o "$RAW/$name.vole.warm.strace" \
    "$BIN" observe --store "$d/store" --field "$field" --page 1 --kind text >/dev/null 2>&1 || true

  # ---- A0 raw tooling lane -------------------------------------------------
  : > "$RAW/$name.a0.text.csv"
  for (( pass=1; pass<=PASSES; pass++ )); do
    for (( i=1; i<=NMAX; i++ )); do
      page=$(( (i-1) % sched + 1 ))
      t0b=$(now_us); pdftotext -f "$page" -l "$page" "$src" - >/dev/null 2>/dev/null || true; t1b=$(now_us)
      echo "$pass $i $page $(( t1b-t0b )) ${A0_RB[$page]} ${A0_TL[$page]}" >> "$RAW/$name.a0.text.csv"
    done
  done

  # ---- A1 preprocessed SQLite lane -----------------------------------------
  : > "$RAW/$name.a1.text.csv"
  for (( pass=1; pass<=PASSES; pass++ )); do
    for (( i=1; i<=NMAX; i++ )); do
      page=$(( (i-1) % sched + 1 ))
      t0b=$(now_us); sqlite3 "$db" "SELECT text FROM pages WHERE page=$page;" >/dev/null 2>/dev/null || true; t1b=$(now_us)
      echo "$pass $i $page $(( t1b-t0b )) ${A1_RB[$page]} ${A1_TL[$page]}" >> "$RAW/$name.a1.text.csv"
    done
  done

  # ---- store universes (ADR-0027), separate --------------------------------
  local desc_b seed_b index_b cache_b manifest_b store_b
  desc_b=$(dusb "$d/store/descriptor"); seed_b=$(dusb "$d/store/seed")
  index_b=$(dusb "$d/store/index"); cache_b=$(dusb "$d/store/cache")
  manifest_b=$(dusb "$d/store/field"); store_b=$(dusb "$d/store")

  # ---- generic compressors (whole-file context; VOLE loses, ADR-0017) ------
  gen_size() { local f=$1 n=$2 z="$d/c.$2" out="$d/d.$2"
    { case "$n" in
        gzip) gzip -9 -c "$f" > "$z" 2>/dev/null && gzip -dc "$z" > "$out" 2>/dev/null ;;
        zstd) zstd -19 --long=27 -q -c "$f" > "$z" 2>/dev/null && zstd -d --long=27 -q -c "$z" > "$out" 2>/dev/null ;;
        xz)   xz -9e -c "$f" > "$z" 2>/dev/null && xz -dc "$z" > "$out" 2>/dev/null ;;
      esac; } || true
    if [ -f "$out" ] && cmp -s "$out" "$f"; then bytes "$z"; else echo null; fi
    rm -f "$z" "$out"
  }
  jq -n \
    --argjson src_bytes "$src_len" --argjson desc_bytes "$desc_len" \
    --argjson src_gzip "$(gen_size "$src" gzip)" --argjson src_zstd "$(gen_size "$src" zstd)" --argjson src_xz "$(gen_size "$src" xz)" \
    --argjson vc_gzip "$(gen_size "$desc" gzip)" --argjson vc_zstd "$(gen_size "$desc" zstd)" --argjson vc_xz "$(gen_size "$desc" xz)" \
    '{source_bytes:$src_bytes, voldoc_bytes:$desc_bytes,
      source:{gzip9:$src_gzip, zstd19:$src_zstd, xz9e:$src_xz},
      voldoc:{gzip9:$vc_gzip, zstd19:$vc_zstd, xz9e:$vc_xz},
      roundtrip_verified:true,
      note:"generic compressors on the complete file; a VOLE size win over xz is not claimed (ADR-0017)"}' \
    > "$RAW/$name.generic.json"

  # ---- aggregate CPU + peak RSS per N (warm pass) --------------------------
  local n cmap
  cmap_text=""
  for n in $NS_STR; do
    MODE=vole-text BIN="$BIN" STORE="$d/store" FIELD="$field" SCHED="$sched" N="$n" \
      /usr/bin/time -v -o "$d/agg.time" bash "$WORK/lq.sh" >/dev/null 2>&1 || true
    cmap_text="$cmap_text $n:$(tv_cpu_ms "$d/agg.time"):$(tv_rss_kb "$d/agg.time")"
  done
  cmap_multi=""
  if [ -n "$obj" ] || [ -n "$stream" ]; then
    for n in $NS_STR; do
      MODE=vole-multi BIN="$BIN" STORE="$d/store" FIELD="$field" SCHED="$sched" OBJ="${obj:-}" STREAM="${stream:-}" N="$n" \
        /usr/bin/time -v -o "$d/agg.time" bash "$WORK/lq.sh" >/dev/null 2>&1 || true
      cmap_multi="$cmap_multi $n:$(tv_cpu_ms "$d/agg.time"):$(tv_rss_kb "$d/agg.time")"
    done
  fi
  cmap_a0=""; for n in $NS_STR; do
    MODE=a0 SRC="$src" SCHED="$sched" N="$n" \
      /usr/bin/time -v -o "$d/agg.time" bash "$WORK/lq.sh" >/dev/null 2>&1 || true
    cmap_a0="$cmap_a0 $n:$(tv_cpu_ms "$d/agg.time"):$(tv_rss_kb "$d/agg.time")"
  done
  cmap_a1=""; for n in $NS_STR; do
    MODE=a1 DB="$db" SCHED="$sched" N="$n" \
      /usr/bin/time -v -o "$d/agg.time" bash "$WORK/lq.sh" >/dev/null 2>&1 || true
    cmap_a1="$cmap_a1 $n:$(tv_cpu_ms "$d/agg.time"):$(tv_rss_kb "$d/agg.time")"
  done

  # ---- lane JSONL ----------------------------------------------------------
  : > "$RAW/$name.vole.cum.jsonl"; : > "$RAW/$name.a0.cum.jsonl"; : > "$RAW/$name.a1.cum.jsonl"
  local PL
  for (( pass=1; pass<=PASSES; pass++ )); do
    PL="pass$pass"
    awk -v want="$pass" '$1==want' "$RAW/$name.vole.text.csv" | awk '{print $2, $4, $5, $6}' > "$d/vt.csv"
    emit_cum "$d/vt.csv" "$NS_STR" "$one_time_wall_ms" "$one_time_read" "$one_time_cpu_ms" "text.$PL" "$cmap_text" >> "$RAW/$name.vole.cum.jsonl"
    if [ -s "$RAW/$name.vole.multi.csv" ]; then
      awk -v want="$pass" '$1==want' "$RAW/$name.vole.multi.csv" | awk '{print $2, $4, $5, $6}' > "$d/vm.csv"
      emit_cum "$d/vm.csv" "$NS_STR" "$one_time_wall_ms" "$one_time_read" "$one_time_cpu_ms" "multisurface.$PL" "$cmap_multi" >> "$RAW/$name.vole.cum.jsonl"
    fi
    awk -v want="$pass" '$1==want' "$RAW/$name.a0.text.csv" | awk '{print $2, $4, $5, $6}' > "$d/a0.csv"
    emit_cum "$d/a0.csv" "$NS_STR" 0 0 0 "text.$PL" "$cmap_a0" >> "$RAW/$name.a0.cum.jsonl"
    awk -v want="$pass" '$1==want' "$RAW/$name.a1.text.csv" | awk '{print $2, $4, $5, $6}' > "$d/a1.csv"
    emit_cum "$d/a1.csv" "$NS_STR" "$prep_ms" "$a1_one_time_read" "$prep_cpu" "text.$PL" "$cmap_a1" >> "$RAW/$name.a1.cum.jsonl"
  done

  # ---- assemble this document's JSON --------------------------------------
  local multi_available=true
  [ -n "$obj" ] && [ -n "$stream" ] || multi_available=false
  jq -n \
    --arg name "$name" --arg src "$src" --argjson src_len "$src_len" --arg src_sha "$src_sha" \
    --argjson npages "$npages" --argjson sched "$sched" \
    --arg cand "$cand" --argjson enc_ms "$enc_ms" --argjson enc_cpu "$enc_cpu" --argjson ing_ms "$ing_ms" --argjson ing_cpu "$ing_cpu" \
    --argjson desc_len "$desc_len" --argjson enc_read "$enc_read" --argjson ing_read "$ing_read" \
    --argjson one_time_wall_ms "$one_time_wall_ms" --argjson one_time_cpu_ms "$one_time_cpu_ms" --argjson one_time_read "$one_time_read" \
    --slurpfile ing "$RAW/$name.ingest.json" \
    --argjson desc_b "$desc_b" --argjson seed_b "$seed_b" --argjson index_b "$index_b" \
    --argjson cache_b "$cache_b" --argjson manifest_b "$manifest_b" --argjson store_b "$store_b" \
    --arg obj "${obj:-}" --arg stream "${stream:-}" --argjson multi_available "$multi_available" \
    --argjson cold_rp "$(rp_bytes "$RAW/$name.vole.cold.strace")" \
    --argjson warm_rp "$(rp_bytes "$RAW/$name.vole.warm.strace")" \
    --slurpfile vcum "$RAW/$name.vole.cum.jsonl" \
    --slurpfile a0cum "$RAW/$name.a0.cum.jsonl" \
    --slurpfile a1cum "$RAW/$name.a1.cum.jsonl" \
    --argjson prep_ms "$prep_ms" --argjson prep_cpu "$prep_cpu" --argjson db_bytes "$db_bytes" --argjson a1_one_time_read "$a1_one_time_read" \
    --slurpfile generic "$RAW/$name.generic.json" \
    '{
       name:$name,
       kind:(if $name|startswith("large") then "generated" else "producer" end),
       source:{path:$src, len:$src_len, sha256:$src_sha, pages:$npages, text_lane_pages:$sched},
       selector_discovery:{object:($obj|tonumber? // null), stream:($stream|tonumber? // null),
                           multisurface_available:$multi_available,
                           method:"bounded scan of selector numbers (object 1..64, stream 1..300) until a valid observation resolves; setup, not charged"},
       vole:{
         encode:{candidate:$cand, encoded_len:$desc_len, wall_ms:$enc_ms, cpu_ms:$enc_cpu, read_bytes:$enc_read},
         ingest:($ing[0] + {wall_ms:$ing_ms, cpu_ms:$ing_cpu, read_bytes:$ing_read}),
         one_time:{wall_ms:$one_time_wall_ms, cpu_ms:$one_time_cpu_ms, read_bytes:$one_time_read},
         store_universe_bytes:{descriptor:$desc_b, seed:$seed_b, index:$index_b, manifest:$manifest_b,
                               derived_cache:$cache_b, total:$store_b},
         lanes:($vcum | group_by(.lane) | map({key:.[0].lane, value:.}) | from_entries),
         strace_crosscheck:{cold_process_read_pread_bytes:$cold_rp, warm_process_read_pread_bytes:$warm_rp,
                            note:"process read/pread total includes library reads; the instrumented stats.bytes_read does not"}
       },
       a0:{
         tool:"pdftotext -f P -l P (Poppler)",
         one_time:{wall_ms:0, cpu_ms:0, read_bytes:0},
         lanes:($a0cum | group_by(.lane) | map({key:.[0].lane, value:.}) | from_entries)
       },
       a1:{
         tool:"sqlite3 preprocessed page-text table",
         preprocessing:{pages_extracted:$npages, wall_ms:$prep_ms, cpu_ms:$prep_cpu, db_bytes:$db_bytes, read_bytes:$a1_one_time_read},
         one_time:{wall_ms:$prep_ms, cpu_ms:$prep_cpu, read_bytes:$a1_one_time_read},
         lanes:($a1cum | group_by(.lane) | map({key:.[0].lane, value:.}) | from_entries)
       },
       generic:$generic[0]
     }' > "$RAW/$name.json" 2> "$RAW/$name.jqerr"
  [ -s "$RAW/$name.json" ] || { echo "FATAL: empty per-doc json for $name" >&2; cat "$RAW/$name.jqerr" >&2; exit 1; }

  { for (( p=1; p<=cap; p++ )); do echo "{\"page\":$p,\"pdftotext_read_bytes\":${A0_RB[$p]},\"pdftotext_text_bytes\":${A0_TL[$p]},\"sqlite_query_read_bytes\":${A1_RB[$p]},\"sqlite_text_bytes\":${A1_TL[$p]}}"; done; } > "$RAW/$name.per_page.jsonl"

  unset A0_RB A0_TL A1_RB A1_TL
  DOCNAMES="$DOCNAMES $name"
}

DOCNAMES=""
for src in "${DOCS[@]}"; do
  [ -f "$src" ] || continue
  process_doc "$src" "$(basename "$src" .pdf)"
done

# ---------------------------------------------------------------------------
# Assemble receipt.json with the crossover analysis.
# ---------------------------------------------------------------------------
DOCFILES=()
for nm in $DOCNAMES; do DOCFILES+=("$RAW/$nm.json"); done

jq -n \
  --slurpfile env "$RAW/environment.json" \
  --argjson docs "$(jq -s '.' "${DOCFILES[@]}" | jq '[.[] | select(has("name"))]')" \
  --argjson ns "$(printf '%s\n' $NS_STR | jq -R 'tonumber' | jq -s '.')" \
  --argjson passes "$PASSES" \
  '
  def cross($v; $b; $k):
    [ range(0; ($v|length)) as $i | select(($v[$i][$k] // 1e18) < ($b[$i][$k] // 1e18)) | $v[$i].n ][0] // null;
  ($passes|tostring) as $P |
  {
    campaign:"phase11-lifetime-court",
    phase:"Phase 11.12 priority #7 — 1/10/100/1000-query lifetime-cost court with crossover detection",
    environment:$env[0],
    ns:$ns,
    passes:$passes,
    accounting_universes_adr0027:{
      source:"original document bytes (archival reality)",
      descriptor_store:"exact .voldoc descriptor + externalized objects (normative reconstruction)",
      procedural_field:"seed DAG nodes + hierarchical index bytes (normative for observations)",
      derived_cache:"disposable observation caches (never normative; written lazily by a cold observation)"
    },
    metric_definitions:{
      one_time_wall_ms:"VOLE = encode + field-ingest; A0 = 0; A1 = per-page pdftotext + SQLite build",
      total_wall_ms:"one_time_wall_ms + sum of the per-query process wall (fresh process per query; startup charged)",
      total_bytes_read:"one_time_read_bytes + query reads. VOLE query reads = instrumented ObserveStats.bytes_read (descriptor+manifest+index+seed). A0/A1 query reads = process read/pread64 total under strace (includes library reads). One-time read: VOLE = strace(encode)+strace(field-ingest); A1 = sum over pages of the same pdftotext page read measured for A0; A0 = 0.",
      total_bytes_returned:"sum of the observation output bytes (VOLE bytes_returned; A0 pdftotext text bytes; A1 SQLite text length)",
      cumulative_cpu_ms:"one_time_cpu_ms + CPU of an aggregate batch run of the first N queries under /usr/bin/time -v (a single query is below the 10ms time resolution); includes the small constant CPU of the wrapping shell",
      peak_rss_kb:"peak RSS of the aggregate batch run of the first N queries",
      persistent_bytes:"VOLE = field store total (descriptor+seed+index+cache); A0 = source PDF (must be kept); A1 = SQLite db (source droppable)"
    },
    documents:$docs,
    crossover:(
      [ $docs[] | . as $d
        | ($d.vole.lanes["text.pass"+$P] // $d.vole.lanes["text.pass1"]) as $v
        | ($d.a0.lanes["text.pass"+$P]) as $a0
        | ($d.a1.lanes["text.pass"+$P]) as $a1
        | {
            name:$d.name,
            pass:"warm (final pass)",
            A0_raw_pdftotext:{
              wall_first_cross_n:cross($v;$a0;"cumulative_wall_ms"),
              bytes_first_cross_n:cross($v;$a0;"cumulative_bytes_read"),
              cpu_first_cross_n:cross($v;$a0;"cumulative_cpu_ms")
            },
            A1_preprocessed_sqlite:{
              wall_first_cross_n:cross($v;$a1;"cumulative_wall_ms"),
              bytes_first_cross_n:cross($v;$a1;"cumulative_bytes_read"),
              cpu_first_cross_n:cross($v;$a1;"cumulative_cpu_ms")
            }
          }
      ]
    )
  }' > "$OUTDIR/receipt.json"
echo "receipt: $OUTDIR/receipt.json"

# ---------------------------------------------------------------------------
# Markdown summary.
# ---------------------------------------------------------------------------
SUMMARY="$OUTDIR/SUMMARY.md"
{
  echo "# Phase 11 lifetime-cost court — results"
  echo
  echo "Generated by \`tools/field-lifetime-court.sh\` inside the pinned \`db-baseline\` service."
  echo "Measures the **cumulative lifetime cost** of answering N page-text queries for"
  echo "N ∈ {$NS_STR} for VOLE (Phase 11), A0 (raw \`pdftotext\`), and A1 (preprocessed SQLite),"
  echo "each with its one-time cost charged in full, then reports the crossover (or its absence)."
  echo "A \"pass\" answers the schedule once in a fresh process per query; pass 1 is cold and the"
  echo "final pass is warm. CPU is an aggregate batch measurement (\`/usr/bin/time -v\`); a single"
  echo "sub-millisecond query is below its 10 ms resolution. VOLE additionally answers a 5-surface"
  echo "pre-registered set (text, structure, preview, object bytes, decoded stream), reported for"
  echo "VOLE alone because the conventional baselines cannot answer those surfaces."
  echo
  jq -r '
    "Commit: `\(.environment.git.commit_short)` (dirty: \(.environment.git.dirty))  \n"
    + "rustc `\(.environment.toolchain.rustc)`, cargo `\(.environment.toolchain.cargo)`; Cargo.lock sha256 `\(.environment.cargo_lock_sha256)`; arch `\(.environment.arch)`.  \n"
    + "Oracles: poppler \(.environment.oracles.poppler_pdftotext), sqlite3 \(.environment.oracles.sqlite3), strace \(.environment.oracles.strace).  \n"
    + "Env: LIFETIME_NS=`\(.environment.env_affecting_semantics.LIFETIME_NS)`, passes=\(.passes).
"' "$OUTDIR/receipt.json"
  echo
  echo "## Crossover table (VOLE vs each baseline, warm pass)"
  echo
  echo "| document | src B | pages | one-time VOLE ms | one-time A1 ms | <A0 wall | <A1 wall | <A0 bytes | <A1 bytes | <A0 cpu | <A1 cpu |"
  echo "|---|---:|---:|---:|---:|---|---|---|---|---|---|"
  jq -r --argjson passes "$PASSES" '
    ($passes|tostring) as $P
    | .documents[] as $d
    | ($d.vole.lanes["text.pass"+$P] // $d.vole.lanes["text.pass1"]) as $v
    | ($d.a0.lanes["text.pass"+$P]) as $a0
    | ($d.a1.lanes["text.pass"+$P]) as $a1
    | def cx($b;$k): ([range(0;($v|length)) as $i|select(($v[$i][$k] // 1e18)<($b[$i][$k] // 1e18))|$v[$i].n][0] // "none");
      "| \($d.name) | \($d.source.len) | \($d.source.pages) | \($d.vole.one_time.wall_ms) | \($d.a1.preprocessing.wall_ms) | \(cx($a0;"cumulative_wall_ms")) | \(cx($a1;"cumulative_wall_ms")) | \(cx($a0;"cumulative_bytes_read")) | \(cx($a1;"cumulative_bytes_read")) | \(cx($a0;"cumulative_cpu_ms")) | \(cx($a1;"cumulative_cpu_ms")) |"
  ' "$OUTDIR/receipt.json"
  echo
  echo "\`none\` means VOLE's cumulative cost never drops below that baseline within the tested N set."
  echo
  echo "## Verdicts and losses"
  echo
  jq -r --argjson passes "$PASSES" '
    ($passes|tostring) as $P
    | .documents[] as $d
    | ($d.vole.lanes["text.pass"+$P]) as $v
    | ($d.a0.lanes["text.pass"+$P]) as $a0
    | ($d.a1.lanes["text.pass"+$P]) as $a1
    | ($v|last) as $vl | ($a0|last) as $a0l | ($a1|last) as $a1l
    | def cx($b;$k): ([range(0;($v|length)) as $i|select(($v[$i][$k] // 1e18)<($b[$i][$k] // 1e18))|$v[$i].n][0]);
    def at($b;$k): ([range(0;($v|length)) as $i|select(($v[$i][$k] // 1e18)<($b[$i][$k] // 1e18))|$v[$i].n][0] | if .==null then "never" else ("N="+(tostring)) end);
    def frac: (($d.vole.one_time.wall_ms / $vl.cumulative_wall_ms)*10000|round/100 | tostring);
      "### \($d.name) — \($d.source.len) B, \($d.source.pages) pages\n\n"
      + "- **Wall crossover.** VOLE first beats A0 raw tooling at **\(at($a0;"cumulative_wall_ms"))**; "
      + "VOLE first beats A1 preprocessed SQLite at **\(at($a1;"cumulative_wall_ms"))**. "
      + "At N=\($vl.n): VOLE \($vl.cumulative_wall_ms) ms, A0 \($a0l.cumulative_wall_ms) ms, A1 \($a1l.cumulative_wall_ms) ms.\n"
      + "- **Bytes-read crossover.** VOLE first beats A0 at **\(at($a0;"cumulative_bytes_read"))** and A1 at **\(at($a1;"cumulative_bytes_read"))**. "
      + "At N=\($vl.n): VOLE \($vl.cumulative_bytes_read) B, A0 \($a0l.cumulative_bytes_read) B, A1 \($a1l.cumulative_bytes_read) B.\n"
      + "- **CPU crossover.** VOLE first beats A0 at **\(at($a0;"cumulative_cpu_ms"))** and A1 at **\(at($a1;"cumulative_cpu_ms"))**.\n"
      + "- **One-time cost and amortisation.** VOLE ingest+encode \($d.vole.one_time.wall_ms) ms / \($d.vole.one_time.cpu_ms) ms CPU / \($d.vole.one_time.read_bytes) B read; "
      + "A1 preprocessing \($d.a1.preprocessing.wall_ms) ms / \($d.a1.preprocessing.read_bytes) B read; A0 none. "
      + "At N=\($vl.n) the one-time cost is **\(frac)%** of VOLE cumulative wall.\n"
      + "- **Persistent bytes.** VOLE \($d.vole.store_universe_bytes.total) B vs A1 \($d.a1.preprocessing.db_bytes) B (source \($d.source.len) B).\n"
  ' "$OUTDIR/receipt.json"
  echo
  echo "## All recorded losses (VOLE does not win these)"
  echo
  jq -r --argjson passes "$PASSES" '
    ($passes|tostring) as $P
    | .documents[] as $d
    | ($d.vole.lanes["text.pass"+$P]) as $v
    | ($d.a0.lanes["text.pass"+$P]) as $a0
    | ($d.a1.lanes["text.pass"+$P]) as $a1
    | ($v|last).n as $maxn
    | def never($b;$k): ([range(0;($v|length)) as $i|select(($v[$i][$k] // 1e18) < ($b[$i][$k] // 1e18))]|length) == 0;
      (if never($a1;"cumulative_wall_ms") then "- \($d.name): VOLE **never** beats A1 wall within N≤\($maxn)." else empty end),
      (if never($a1;"cumulative_bytes_read") then "- \($d.name): VOLE **never** beats A1 bytes-read within N≤\($maxn)." else empty end),
      (if never($a1;"cumulative_cpu_ms") then "- \($d.name): VOLE **never** beats A1 CPU within N≤\($maxn)." else empty end),
      "- \($d.name): one-time read B \($d.vole.one_time.read_bytes) (\((($d.vole.one_time.read_bytes/$d.source.len)*100|round/100)|tostring)× the \($d.source.len) B source); persistent store \($d.vole.store_universe_bytes.total) B; xz(source) \($d.generic.source.xz9e) B ≪ voldoc \($d.generic.voldoc_bytes) B (not a compressor, ADR-0017)."
  ' "$OUTDIR/receipt.json"
  echo
  echo "Note on definitions: VOLE \`total_bytes_read\` is the instrumented \`ObserveStats.bytes_read\` (descriptor+manifest+index+seed); A0/A1 \`total_bytes_read\` is the process \`read\`/\`pread64\` total under strace, which also includes library reads. The two are not the same measurement and are never summed together. CPU is an aggregate batch measurement."
  echo
  for nm in $DOCNAMES; do
    echo "## $nm"
    echo
    echo "### Cumulative cost at each N (warm pass)"
    echo
    echo "| N | VOLE wall ms | A0 wall ms | A1 wall ms | VOLE bytes | A0 bytes | A1 bytes | VOLE cpu ms | A0 cpu ms | A1 cpu ms | VOLE peak RSS kB |"
    echo "|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|"
    jq -r --arg nm "$nm" --argjson p "$PASSES" '
      .documents[] | select(.name==$nm) as $d
      | ($d.vole.lanes["text.pass"+($p|tostring)]) as $v
      | ($d.a0.lanes["text.pass"+($p|tostring)]) as $a0
      | ($d.a1.lanes["text.pass"+($p|tostring)]) as $a1
      | range(0;($v|length)) as $i
      | "| \($v[$i].n) | \($v[$i].cumulative_wall_ms) | \($a0[$i].cumulative_wall_ms) | \($a1[$i].cumulative_wall_ms) | \($v[$i].cumulative_bytes_read) | \($a0[$i].cumulative_bytes_read) | \($a1[$i].cumulative_bytes_read) | \($v[$i].cumulative_cpu_ms) | \($a0[$i].cumulative_cpu_ms) | \($a1[$i].cumulative_cpu_ms) | \($v[$i].peak_rss_kb) |"
    ' "$OUTDIR/receipt.json"
    echo
    echo "### Per-query marginal (warm pass) — amortisation of the one-time cost"
    echo
    echo "| N | VOLE marginal ms | VOLE marginal bytes | A1 avg ms/query | A0 avg ms/query |"
    echo "|---:|---:|---:|---:|---:|"
    jq -r --arg nm "$nm" --argjson p "$PASSES" '
      .documents[] | select(.name==$nm) as $d
      | ($d.vole.lanes["text.pass"+($p|tostring)]) as $v
      | ($d.a0.lanes["text.pass"+($p|tostring)]) as $a0
      | ($d.a1.lanes["text.pass"+($p|tostring)]) as $a1
      | range(0;($v|length)) as $i
      | "| \($v[$i].n) | \($v[$i].marginal_wall_ms) | \($v[$i].marginal_bytes_read) | \(($a1[$i].cumulative_wall_ms / $a1[$i].n)*1000 | round / 1000) | \(($a0[$i].cumulative_wall_ms / $a0[$i].n)*1000 | round / 1000) |"
    ' "$OUTDIR/receipt.json"
    echo
  done
  echo "## VOLE multi-surface lane (VOLE alone; baselines cannot answer these)"
  echo
  echo "| document | N | cumulative wall ms | cumulative bytes read | cumulative bytes returned | cumulative cpu ms | marginal ms |"
  echo "|---|---:|---:|---:|---:|---:|---:|"
  jq -r --argjson p "$PASSES" '
    .documents[] as $d
    | ($d.vole.lanes["multisurface.pass"+($p|tostring)] // []) as $m
    | $m[] | "| \($d.name) | \(.n) | \(.cumulative_wall_ms) | \(.cumulative_bytes_read) | \(.cumulative_bytes_returned) | \(.cumulative_cpu_ms) | \(.marginal_wall_ms) |"
  ' "$OUTDIR/receipt.json"
  echo
  echo "## Persistent bytes and one-time cost"
  echo
  echo "| document | system | persistent bytes | one-time wall ms | one-time cpu ms | one-time read B |"
  echo "|---|---|---:|---:|---:|---:|"
  jq -r '
    .documents[] as $d
    | "| \($d.name) | VOLE | \($d.vole.store_universe_bytes.total) | \($d.vole.one_time.wall_ms) | \($d.vole.one_time.cpu_ms) | \($d.vole.one_time.read_bytes) |",
      "| \($d.name) | A0 | \($d.source.len) | 0 | 0 | 0 |",
      "| \($d.name) | A1 | \($d.a1.preprocessing.db_bytes) | \($d.a1.preprocessing.wall_ms) | \($d.a1.preprocessing.cpu_ms) | \($d.a1.preprocessing.read_bytes) |"
  ' "$OUTDIR/receipt.json"
  echo
  echo "## VOLE store universes (ADR-0027, separate)"
  echo
  echo "| document | descriptor B | seed B | index B | manifest B | derived cache B | store total B |"
  echo "|---|---:|---:|---:|---:|---:|---:|"
  jq -r '.documents[] | "| \(.name) | \(.vole.store_universe_bytes.descriptor) | \(.vole.store_universe_bytes.seed) | \(.vole.store_universe_bytes.index) | \(.vole.store_universe_bytes.manifest) | \(.vole.store_universe_bytes.derived_cache) | \(.vole.store_universe_bytes.total) |"' "$OUTDIR/receipt.json"
  echo
  echo "## Generic compressors (whole-file context; VOLE loses, ADR-0017)"
  echo
  echo "| document | source B | xz source | voldoc B | xz voldoc |"
  echo "|---|---:|---:|---:|---:|"
  jq -r '.documents[] | "| \(.name) | \(.generic.source_bytes) | \(.generic.source.xz9e) | \(.generic.voldoc_bytes) | \(.generic.voldoc.xz9e) |"' "$OUTDIR/receipt.json"
  echo
  echo "**Recorded loss:** VOLE is not a whole-file compressor; xz on the source is far smaller than the \`.voldoc\` (ADR-0017)."
} > "$SUMMARY"

{
  echo "# Phase 11 lifetime-cost court — exact commands"
  echo "# Commit under test: $COMMIT (branch $(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)); dirty: $DIRTY"
  echo "# All commands run inside the pinned db-baseline image; nothing on the host."
  echo
  echo "docker compose run --rm --no-TTY dev         cargo build --all-features --locked"
  echo "docker compose build db-baseline"
  echo "docker compose run --rm --no-TTY db-baseline bash tools/field-lifetime-court.sh $OUTDIR"
  echo
  echo "# Per document (inside field-lifetime-court.sh):"
  echo "#   \$BIN encode SRC D.voldoc ; \$BIN field-ingest D.voldoc --store STORE   (one-time, /usr/bin/time -v)"
  echo "#   \$BIN observe --store STORE --field F --page P --kind text|structure|preview   (fresh process each)"
  echo "#   \$BIN observe --store STORE --field F --object O --kind exact"
  echo "#   \$BIN observe --store STORE --field F --stream S --kind decoded"
  echo "#   pdftotext -f P -l P SRC -                        (A0, fresh process each)"
  echo "#   pdftotext -f P -l P SRC p.txt ...; sqlite3 PAGES.db (CREATE/INSERT/INDEX)   (A1 one-time)"
  echo "#   sqlite3 PAGES.db 'SELECT text FROM pages WHERE page=P;'   (A1 query)"
  echo "#   strace -f -e trace=read,pread64   (bytes read; per-page once for A0/A1, encode+ingest once for VOLE)"
  echo "#   /usr/bin/time -v                  (cumulative CPU + peak RSS over an aggregate batch of N queries)"
  echo "#   gzip -9 / zstd -19 --long=27 / xz -9e on SRC and on the .voldoc (round-trip verified)"
  echo "# N set: $NS_STR ; passes: $PASSES"
} > "$OUTDIR/commands.txt"
echo "commands: $OUTDIR/commands.txt"; echo "summary: $SUMMARY"

find "$RAW" -type f -size +40k ! -name '*.gz' -exec gzip -9 {} \; 2>/dev/null || true
if [ -n "${HOST_UID:-}" ] && [ -n "${HOST_GID:-}" ]; then
  chown -R "$HOST_UID:$HOST_GID" "$OUTDIR" 2>/dev/null || true
fi
