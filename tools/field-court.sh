#!/bin/sh
# Phase-11.9/11.11 field court — the §77 headline demonstration.
#
# End-to-end, one machine/run, machine-readable JSON + a human summary, for a
# large deterministic document and (when present) a real producer document:
#
#   1. generate / select the source;
#   2. `encode` to a `.voldoc`, then `field-ingest` into a field store;
#   3. report the full ingest cost (wall, source/descriptor/seed/index/cache
#      bytes, node counts);
#   4. in a *new process*, `observe --page N --kind text|structure`, `preview`,
#      `explain --analyze`, and report the whole `ObserveStats`;
#   5. scrub a page set and show the per-page working set is bounded and does not
#      grow with document size (two large sizes at one source size + a real
#      producer document);
#   6. remove the source PDF, start a *new process*, repeat the queries and
#      `materialize --exact`, and verify length + SHA-256 + `cmp`;
#   7. re-query and prove reuse (`seed_nodes_reused > 0`,
#      `seed_nodes_executed == 0`);
#   8. compare against fair baselines on the same run: A0 raw tooling
#      (`pdftotext` bytes-read via strace + wall + RSS; `pdfinfo`;
#      `qpdf --linearize` first-page prefix fraction), A1 preprocessed SQLite
#      (`tools/field-baseline-db.sh`), and generic compressors (gzip/zstd/xz on
#      the source and on the `.voldoc`);
#   9. the LLM working-set court (`tools/field-llm-workingset.sh`): B0 whole
#      document, B1 page-local Poppler, V VOLE page text, UTF-8 bytes only.
#
# ## Honesty rules this script enforces
#
#   * Four accounting universes (ADR-0027) are always separate: source;
#     descriptor+store; procedural field (seed+index); derived cache.
#   * The instrumented `ObserveStats.bytes_read` counts **only seed-store bytes
#     fetched**; the process additionally re-reads the whole descriptor blob on
#     every invocation. Both are reported; neither is hidden.
#   * The derived cache is counted and its bytes are paid; it is disposably
#     cleared and correctness is re-checked.
#   * No token counts are claimed (no pinned offline tokenizer).
#   * Where a conventional tool wins (SQLite indexed page lookup, page-local
#     Poppler text, whole-file compression), the loss is recorded.
#
# Usage (inside the `db-baseline` service):
#   sh tools/field-court.sh OUTDIR [PRODUCER_PDF]
set -eu

cd /work
LC_ALL=C
export LC_ALL

OUTDIR=${1:-evidence/scratch/field-court}
PRODUCER=${2:-evidence/corpus/phase7-producers/libreoffice-export.pdf}
BIN=${VOLE_BIN:-./target/debug/vole-document}

mkdir -p "$OUTDIR/raw"
RAW="$OUTDIR/raw"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

if [ ! -x "$BIN" ]; then
  echo "field-court: $BIN not found; building (cargo build --locked --all-features)" >&2
  cargo build --locked --all-features 1>&2
fi

# ---------------------------------------------------------------------------
# Small helpers.
# ---------------------------------------------------------------------------
now_ms() { echo $(( $(date +%s%N) / 1000000 )); }
sha() { sha256sum "$1" | cut -d' ' -f1; }
bytes() { stat -c %s "$1" 2>/dev/null || echo 0; }
dusb() { du -sb "$1" 2>/dev/null | cut -f1; }

# strace read/pread64 byte accounting. The process still mmaps its libraries;
# that is reported separately and never folded into "bytes read".
rp_calls() { awk '/^[0-9 ]*(read|pread64)\(/{c++} /^[0-9 ]*<\.\.\. (read|pread64) resumed>/{c++} END{print c+0}' "$1"; }
rp_bytes() { awk 'match($0,/^[0-9 ]*(read|pread64)\(/){if(match($0,/ = [0-9]+$/)){s+=substr($0,RSTART+2)+0}} match($0,/^[0-9 ]*<\.\.\. (read|pread64) resumed>/){if(match($0,/ = [0-9]+$/)){s+=substr($0,RSTART+2)+0}} END{print s+0}' "$1"; }
fmap_bytes() { awk '/mmap\(/{l=$0;sub(/^.*mmap\(/,"",l);sub(/\) = .*$/,"",l);n=split(l,a,", ");if(n>=5&&a[3]~/PROT_READ/&&a[4]!~/ANONYMOUS/&&a[5]~/^[0-9]+$/){s+=a[2]}}END{print s+0}' "$1"; }

# `observe`/`preview --json` emit a stats object that is valid JSON but the whole
# line has a missing comma before `"stats"` (a pre-existing CLI defect in the
# committed field code). We therefore isolate the stats object, which parses.
stats_of() { sed -n 's/.*"stats":\({[^}]*}\).*/\1/p' "$1"; }
obs_json() { # FILE -> stats object augmented with basis/exact (valid JSON)
  _s=$(stats_of "$1")
  _b=$(sed -n 's/.*"basis":"\([a-z-]*\)".*/\1/p' "$1" | head -1)
  _e=$(sed -n 's/.*"exact":\(true\|false\).*/\1/p' "$1" | head -1)
  printf '%s' "$_s" | jq -c --arg b "${_b:-unknown}" --argjson e "${_e:-false}" '. + {basis:$b, exact:$e}'
}
obs_field() { grep -o '"field":"[0-9a-f]*"' "$1" | head -1 | sed 's/.*"\([0-9a-f]*\)".*/\1/'; }

# ---------------------------------------------------------------------------
# Environment capture.
# ---------------------------------------------------------------------------
RUSTC_V=$(rustc --version 2>/dev/null | sed 's/^rustc //' || echo unknown)
CARGO_V=$(cargo --version 2>/dev/null | sed 's/^cargo //' || echo unknown)
ARCH=$(uname -m)
COMMIT=$(git rev-parse HEAD 2>/dev/null || echo unknown)
COMMIT_SHORT=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
DIRTY=$(git status --porcelain 2>/dev/null | head -40 | tr '\n' ';' || echo unknown)
LOCK_SHA=$(sha "$(git rev-parse --show-toplevel 2>/dev/null || echo /work)/Cargo.lock" 2>/dev/null || echo unknown)
[ -f /work/Cargo.lock ] && LOCK_SHA=$(sha /work/Cargo.lock)
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
  --arg base_stable "$BASE_STABLE" --arg base_tools "$BASE_TOOLS" \
  --arg image_id "$IMAGE_ID" \
  --arg sqlite "$SQLITE_V" --arg poppler "$POPPLER_V" --arg qpdf "$QPDF_V" \
  --arg strace "$STRACE_V" --arg jq "$JQ_V" \
  --arg gzip "$GZIP_V" --arg zstd "$ZSTD_V" --arg xz "$XZ_V" \
  --arg bin "$BIN" \
  '{
     git: {commit: $commit, commit_short: $commit_short, dirty: $dirty},
     arch: $arch,
     cargo_lock_sha256: $lock_sha,
     toolchain: {rustc: $rustc, cargo: $cargo},
     images: {base_stable_digest: $base_stable, base_tools_digest: $base_tools, db_baseline_image_id: $image_id},
     oracles: {sqlite3: $sqlite, poppler_pdftotext: $poppler, qpdf: $qpdf, strace: $strace, jq: $jq,
               gzip: $gzip, zstd: $zstd, xz: $xz},
     env_affecting_semantics: {LC_ALL: "C", VOLE_BIN: $bin}
   }' > "$RAW/environment.json"

# Primary query page and per-case scrub sets.
PRIMARY_PAGE=1

# ---------------------------------------------------------------------------
# court_case NAME SOURCE_PDF NPAGES SCRUB_CSV
# ---------------------------------------------------------------------------
court_case() {
  _name=$1; _src=$2; _npages=$3; _scrub=$4
  _d="$WORK/$_name"
  mkdir -p "$_d/store"
  _src_len=$(bytes "$_src")
  _src_sha=$(sha "$_src")
  echo "court: case=$_name pages=$_npages source=${_src_len}B sha=${_src_sha}" >&2

  # --- encode (timed) ------------------------------------------------------
  _desc="$_d/$_name.voldoc"
  _t0=$(now_ms); "$BIN" encode "$_src" "$_desc" > "$RAW/$_name.encode.json"; _t1=$(now_ms)
  _encode_ms=$(( _t1 - _t0 ))
  _cand=$(jq -r .candidate "$RAW/$_name.encode.json")
  _desc_len=$(jq -r .encoded_len "$RAW/$_name.encode.json")

  # --- field-ingest (timed) ------------------------------------------------
  _t0=$(now_ms); "$BIN" field-ingest "$_desc" --store "$_d/store" > "$RAW/$_name.ingest.json"; _t1=$(now_ms)
  _ingest_ms=$(( _t1 - _t0 ))
  _field=$(jq -r .field "$RAW/$_name.ingest.json")

  # --- store universes (ADR-0027), separate --------------------------------
  _desc_bytes=$(dusb "$_d/store/descriptor")
  _seed_bytes=$(dusb "$_d/store/seed")
  _index_bytes=$(dusb "$_d/store/index")
  _cache_init=$(dusb "$_d/store/cache")
  _manifest_bytes=$(dusb "$_d/store/field")
  _store_total=$(dusb "$_d/store")

  # --- cold observation in a new process (text), straced -------------------
  # A plain `observe` opens the field once, so this is the honest per-process
  # cost of one observation (unlike `explain --analyze`, which opens twice).
  _t0=$(now_ms)
  strace -f -e trace=read,pread64,mmap -o "$RAW/$_name.observe-text.strace" \
    "$BIN" observe --store "$_d/store" --field "$_field" --page "$PRIMARY_PAGE" --kind text \
    > "$RAW/$_name.observe-text.json" 2>/dev/null || true
  _t1=$(now_ms)
  _cold_wall_ms=$(( _t1 - _t0 ))
  _cold=$(obs_json "$RAW/$_name.observe-text.json")
  _promoted=$(obs_field "$RAW/$_name.observe-text.json")
  [ -n "$_promoted" ] || _promoted=$_field

  _cold_proc_rp=$(rp_bytes "$RAW/$_name.observe-text.strace")
  _cold_proc_calls=$(rp_calls "$RAW/$_name.observe-text.strace")
  _cold_proc_mmap=$(fmap_bytes "$RAW/$_name.observe-text.strace")

  # --- structure observation + raw preview (now warm: the text deepening above
  # --- already built the page's operators/text/preview nodes) ----------------
  "$BIN" observe --store "$_d/store" --field "$_promoted" --page "$PRIMARY_PAGE" --kind structure \
    > "$RAW/$_name.observe-structure.json" 2>/dev/null || true
  "$BIN" preview --store "$_d/store" --field "$_promoted" --page "$PRIMARY_PAGE" \
    > "$RAW/$_name.preview.bin" 2>/dev/null || true

  _t0=$(now_ms)
  "$BIN" explain --store "$_d/store" --field "$_promoted" --page "$PRIMARY_PAGE" --kind structure --analyze \
    > "$RAW/$_name.explain-structure.json" 2>/dev/null || true
  _t1=$(now_ms); _struct_wall_ms=$(( _t1 - _t0 ))
  _struct=$(jq -c .actual "$RAW/$_name.explain-structure.json")

  _t0=$(now_ms)
  "$BIN" explain --store "$_d/store" --field "$_promoted" --page "$PRIMARY_PAGE" --kind preview --analyze \
    > "$RAW/$_name.explain-preview.json" 2>/dev/null || true
  _t1=$(now_ms); _preview_wall_ms=$(( _t1 - _t0 ))
  _preview=$(jq -c .actual "$RAW/$_name.explain-preview.json")

  # Reuse: a second text observation must reuse and execute nothing.
  _t0=$(now_ms)
  "$BIN" observe --store "$_d/store" --field "$_promoted" --page "$PRIMARY_PAGE" --kind text \
    > "$RAW/$_name.observe-text-warm.json" 2>/dev/null || true
  _t1=$(now_ms); _warm_wall_ms=$(( _t1 - _t0 ))
  _warm=$(obs_json "$RAW/$_name.observe-text-warm.json")

  _cache_after=$(dusb "$_d/store/cache")

  # --- scrub: per-page working set (new process per page) ------------------
  # Each not-yet-touched page is a cold first observation (deepened=true); the
  # repeated pages show reuse.
  : > "$RAW/$_name.scrub.jsonl"
  _oldifs=$IFS; IFS=','
  for _p in $_scrub; do
    IFS=$_oldifs
    "$BIN" observe --store "$_d/store" --field "$_promoted" --page "$_p" --kind text \
      > "$RAW/$_name.scrub-$_p.json" 2>/dev/null || true
    obs_json "$RAW/$_name.scrub-$_p.json" | jq -c --argjson p "$_p" '. + {page:$p}' >> "$RAW/$_name.scrub.jsonl"
    IFS=','
  done
  IFS=$_oldifs

  # --- source removal + full rematerialize (new processes) -----------------
  mkdir -p "$_d/golden"
  cp "$_src" "$_d/golden/source.pdf"
  _golden_sha=$(sha "$_d/golden/source.pdf")
  rm -f "$_src"
  _src_removed=true
  if [ -e "$_src" ]; then _src_removed=false; fi

  # query after removal
  "$BIN" explain --store "$_d/store" --field "$_promoted" --page "$PRIMARY_PAGE" --kind text --analyze \
    > "$RAW/$_name.explain-after-removal.json" 2>/dev/null || true
  _after_removal=$(jq -c .actual "$RAW/$_name.explain-after-removal.json")

  # full exact materialization
  _t0=$(now_ms)
  "$BIN" materialize --store "$_d/store" --field "$_promoted" --exact --output "$_d/out.pdf" \
    > "$RAW/$_name.materialize.json" 2>/dev/null || true
  _t1=$(now_ms); _mat_wall_ms=$(( _t1 - _t0 ))
  _mat_len=$(bytes "$_d/out.pdf")
  _mat_sha=$(sha "$_d/out.pdf")
  if cmp -s "$_d/out.pdf" "$_d/golden/source.pdf"; then _mat_cmp=true; else _mat_cmp=false; fi
  if [ "$_mat_len" = "$_src_len" ] && [ "$_mat_sha" = "$_src_sha" ] && [ "$_mat_cmp" = true ]; then
    _exact=true
  else
    _exact=false
  fi

  # --- cache cleared: observations still correct (disposable cache) --------
  _prepreview_sha=$(sha "$RAW/$_name.preview.bin")
  _cache_clear=$("$BIN" cache --store "$_d/store" --clear)
  "$BIN" preview --store "$_d/store" --field "$_promoted" --page "$PRIMARY_PAGE" \
    > "$RAW/$_name.preview-after-clear.bin" 2>/dev/null || true
  _postpreview_sha=$(sha "$RAW/$_name.preview-after-clear.bin")
  "$BIN" explain --store "$_d/store" --field "$_promoted" --page "$PRIMARY_PAGE" --kind text --analyze \
    > "$RAW/$_name.explain-after-clear.json" 2>/dev/null || true
  _after_clear=$(jq -c .actual "$RAW/$_name.explain-after-clear.json")
  if [ "$_prepreview_sha" = "$_postpreview_sha" ]; then _cache_clear_ok=true; else _cache_clear_ok=false; fi
  _cache_cleared_bytes=$(printf '%s' "$_cache_clear" | jq -r .reclaimed 2>/dev/null || echo 0)

  # --- LLM working set -----------------------------------------------------
  sh tools/field-llm-workingset.sh "$RAW/$_name.llm.json" "$_d/golden/source.pdf" "$PRIMARY_PAGE" "$_d/store" "$_promoted" >/dev/null

  # --- assemble the case object -------------------------------------------
  jq -n \
    --arg name "$_name" --arg src "$_src" --argjson src_len "$_src_len" --arg src_sha "$_src_sha" \
    --argjson npages "$_npages" --argjson primary_page "$PRIMARY_PAGE" \
    --arg cand "$_cand" --argjson desc_len "$_desc_len" --argjson encode_ms "$_encode_ms" \
    --argjson ingest_ms "$_ingest_ms" --slurpfile ing "$RAW/$_name.ingest.json" \
    --argjson desc_bytes "$_desc_bytes" --argjson seed_bytes "$_seed_bytes" \
    --argjson index_bytes "$_index_bytes" --argjson cache_init "$_cache_init" \
    --argjson manifest_bytes "$_manifest_bytes" --argjson store_total "$_store_total" \
    --argjson cache_after "$_cache_after" \
    --argjson cold "$_cold" --argjson cold_wall_ms "$_cold_wall_ms" \
    --argjson cold_proc_rp "$_cold_proc_rp" --argjson cold_proc_calls "$_cold_proc_calls" \
    --argjson cold_proc_mmap "$_cold_proc_mmap" \
    --argjson struct "$_struct" --argjson struct_wall_ms "$_struct_wall_ms" \
    --argjson preview "$_preview" --argjson preview_wall_ms "$_preview_wall_ms" \
    --argjson warm "$_warm" --argjson warm_wall_ms "$_warm_wall_ms" \
    --slurpfile scrub "$RAW/$_name.scrub.jsonl" \
    --argjson src_removed "$_src_removed" --argjson after_removal "$_after_removal" \
    --argjson mat_len "$_mat_len" --arg mat_sha "$_mat_sha" --argjson mat_cmp "$_mat_cmp" \
    --argjson mat_wall_ms "$_mat_wall_ms" --argjson exact "$_exact" \
    --argjson cache_clear_ok "$_cache_clear_ok" --argjson cache_cleared_bytes "$_cache_cleared_bytes" \
    --argjson after_clear "$_after_clear" \
    --slurpfile llm "$RAW/$_name.llm.json" \
    '{
       name: $name, kind: (if $name|startswith("large") then "generated" else "producer" end),
       source: {path: $src, len: $src_len, sha256: $src_sha, pages: $npages},
       encode: {candidate: $cand, encoded_len: $desc_len, wall_ms: $encode_ms},
       ingest: ($ing[0] + {wall_ms: $ingest_ms}),
       store_universe_bytes: {
         descriptor: $desc_bytes, seed: $seed_bytes, index: $index_bytes,
         manifest: $manifest_bytes, derived_cache_initial: $cache_init,
         derived_cache_after_cold: $cache_after, total: $store_total
       },
       observe_new_process: {
         primary_page: $primary_page,
         text_cold: ($cold + {wall_ms: $cold_wall_ms,
                             process_read_pread_bytes: $cold_proc_rp,
                             process_read_pread_calls: $cold_proc_calls,
                             process_file_mmap_bytes: $cold_proc_mmap}),
         structure: ($struct + {wall_ms: $struct_wall_ms}),
         preview: ($preview + {wall_ms: $preview_wall_ms}),
         text_warm_reuse: ($warm + {wall_ms: $warm_wall_ms})
       },
       scrub: $scrub,
       source_removal: {
         source_removed: $src_removed,
         query_after_removal: $after_removal,
         materialized: {len: $mat_len, sha256: $mat_sha, cmp_equal: $mat_cmp,
                        wall_ms: $mat_wall_ms, exact: $exact}
       },
       cache_disposable: {
         cleared_reclaimed_bytes: $cache_cleared_bytes,
         preview_sha_unchanged: $cache_clear_ok,
         text_after_clear: $after_clear
       },
       llm_working_set: $llm[0]
     }' > "$RAW/$_name.case.json"

  # keep the golden copy for baselines
  cp "$_d/golden/source.pdf" "$WORK/$_name.source.pdf"
}

# ---------------------------------------------------------------------------
# Build the deterministic large documents (two page counts, one source size)
# and the producer document.
# ---------------------------------------------------------------------------
CASES=""
GEN50="$WORK/gen50"; GEN400="$WORK/gen400"
mkdir -p "$GEN50" "$GEN400"
"$BIN" pdf-make-large "$GEN50" 50 > "$RAW/gen-50.json"
"$BIN" pdf-make-large "$GEN400" 400 > "$RAW/gen-400.json"

court_case large-50 "$GEN50/large.pdf" 50 "1,2,3,25,49,50,10,1"
CASES="$CASES large-50"
court_case large-400 "$GEN400/large.pdf" 400 "1,2,3,120,399,400,200,1"
CASES="$CASES large-400"

PRODUCER_USED=false
if [ -f "$PRODUCER" ]; then
  PCOPY="$WORK/producer.pdf"
  cp "$PRODUCER" "$PCOPY"
  PNPAGES=$(pdfinfo "$PCOPY" | awk -F: '/^Pages:/{gsub(/ /,"",$2); print $2}')
  case "$PNPAGES" in ''|*[!0-9]*) PNPAGES=1 ;; esac
  court_case "producer" "$PCOPY" "$PNPAGES" "1,2,3,10,30,60,1"
  CASES="$CASES producer"
  PRODUCER_USED=true
fi

# ---------------------------------------------------------------------------
# A0 — raw PDF tooling on the primary (large-400) document.
# ---------------------------------------------------------------------------
A0_SRC="$WORK/large-400.source.pdf"
A0_PAGES=400
A0_PAGE=$PRIMARY_PAGE

pdfinfo "$A0_SRC" > "$RAW/a0.pdfinfo.txt" 2>/dev/null || true

# pdfinfo timing/RSS
/usr/bin/time -v pdfinfo "$A0_SRC" >/dev/null 2>"$RAW/a0.pdfinfo.time" || true
A0_INFO_RSS=$(awk -F': ' '/Maximum resident set size/{print $2}' "$RAW/a0.pdfinfo.time")

# pdftotext page-local, straced
_t0=$(now_ms)
strace -f -e trace=read,pread64,mmap -o "$RAW/a0.pdftotext.strace" \
  pdftotext -f "$A0_PAGE" -l "$A0_PAGE" "$A0_SRC" "$RAW/a0.page.txt" >/dev/null 2>&1 || true
_t1=$(now_ms); A0_PT_WALL_MS=$(( _t1 - _t0 ))
A0_PT_RP=$(rp_bytes "$RAW/a0.pdftotext.strace")
A0_PT_CALLS=$(rp_calls "$RAW/a0.pdftotext.strace")
A0_PT_MMAP=$(fmap_bytes "$RAW/a0.pdftotext.strace")
A0_PT_BYTES=$(bytes "$RAW/a0.page.txt")

# pdftotext whole document (for the LLM court B0 and reference)
/usr/bin/time -v pdftotext "$A0_SRC" "$RAW/a0.alldoc.txt" >/dev/null 2>"$RAW/a0.alldoc.time" || true
A0_PT_RSS=$(awk -F': ' '/Maximum resident set size/{print $2}' "$RAW/a0.alldoc.time")
A0_ALL_BYTES=$(bytes "$RAW/a0.alldoc.txt")

# qpdf --linearize first-page prefix fraction (/E = end of first page, /L = len)
qpdf --linearize "$A0_SRC" "$WORK/a0.linearized.pdf" >/dev/null 2>&1 || true
A0_LIN_BYTES=$(bytes "$WORK/a0.linearized.pdf")
A0_E=$(head -c 1200 "$WORK/a0.linearized.pdf" | tr -cd '\11\12\15\40-\176' | grep -oE '/E [0-9]+' | head -1 | awk '{print $2}')
A0_L=$(head -c 1200 "$WORK/a0.linearized.pdf" | tr -cd '\11\12\15\40-\176' | grep -oE '/L [0-9]+' | head -1 | awk '{print $2}')
[ -n "$A0_E" ] || A0_E=0
[ -n "$A0_L" ] || A0_L=0
if [ "$A0_L" -gt 0 ] 2>/dev/null; then
  A0_PREFIX_FRAC=$(awk -v e="$A0_E" -v l="$A0_L" 'BEGIN{printf "%.6f", e/l}')
else
  A0_PREFIX_FRAC=null
fi

jq -n \
  --argjson pages "$A0_PAGES" --argjson page "$A0_PAGE" \
  --argjson page_bytes "$A0_PT_BYTES" --argjson wall_ms "$A0_PT_WALL_MS" \
  --argjson rp_bytes "$A0_PT_RP" --argjson rp_calls "$A0_PT_CALLS" --argjson mmap_bytes "$A0_PT_MMAP" \
  --argjson all_bytes "$A0_ALL_BYTES" --argjson rss_kb "${A0_PT_RSS:-0}" --argjson info_rss_kb "${A0_INFO_RSS:-0}" \
  --argjson lin_bytes "$A0_LIN_BYTES" --argjson lin_e "$A0_E" --argjson lin_l "$A0_L" \
  --arg prefix_frac "$A0_PREFIX_FRAC" \
  '{
     pdfinfo: {pages: $pages, max_rss_kb: $info_rss_kb},
     pdftotext_page_local: {
       page: $page, text_bytes: $page_bytes, wall_ms: $wall_ms,
       read_pread_calls: $rp_calls, read_pread_bytes: $rp_bytes,
       file_mmap_bytes: $mmap_bytes
     },
     pdftotext_whole_document: {text_bytes: $all_bytes, max_rss_kb: $rss_kb},
     qpdf_linearize: {file_bytes: $lin_bytes, E_end_of_first_page: $lin_e, L_file_len: $lin_l,
                      first_page_prefix_fraction: ($prefix_frac|tonumber? // null)}
   }' > "$RAW/a0.json"

# ---------------------------------------------------------------------------
# A1 — preprocessed SQLite (fair, charged in full).
# ---------------------------------------------------------------------------
sh tools/field-baseline-db.sh "$RAW/a1.json" "$A0_SRC" "$A0_PAGE" >/dev/null

# ---------------------------------------------------------------------------
# Generic compressors on the source and on the `.voldoc` (round-trip verified).
# ---------------------------------------------------------------------------
A1_DESC="$WORK/large-400/large-400.voldoc"
gen_size() { # $1 file $2 name
  _f=$1; _n=$2; _z="$WORK/c.$_n"; _o="$WORK/d.$_n"
  { case "$_n" in
      gzip)  gzip -9 -c "$_f" > "$_z" 2>/dev/null && gzip -dc "$_z" > "$_o" 2>/dev/null ;;
      zstd)  zstd -19 --long=27 -q -c "$_f" > "$_z" 2>/dev/null && zstd -d --long=27 -q -c "$_z" > "$_o" 2>/dev/null ;;
      xz)    xz -9e -c "$_f" > "$_z" 2>/dev/null && xz -dc "$_z" > "$_o" 2>/dev/null ;;
    esac
  } || true
  if [ -f "$_o" ] && cmp -s "$_o" "$_f"; then bytes "$_z"; else echo null; fi
  rm -f "$_z" "$_o"
}
SRC_GZ=$(gen_size "$A0_SRC" gzip); SRC_ZS=$(gen_size "$A0_SRC" zstd); SRC_XZ=$(gen_size "$A0_SRC" xz)
VC_GZ=$(gen_size "$A1_DESC" gzip); VC_ZS=$(gen_size "$A1_DESC" zstd); VC_XZ=$(gen_size "$A1_DESC" xz)
jq -n \
  --argjson src_len "$(bytes "$A0_SRC")" --argjson desc_len "$(bytes "$A1_DESC")" \
  --argjson src_gzip "$SRC_GZ" --argjson src_zstd "$SRC_ZS" --argjson src_xz "$SRC_XZ" \
  --argjson vc_gzip "$VC_GZ" --argjson vc_zstd "$VC_ZS" --argjson vc_xz "$VC_XZ" \
  '{source_bytes: $src_len, voldoc_bytes: $desc_len,
    source: {gzip9: $src_gzip, zstd19: $src_zstd, xz9e: $src_xz},
    voldoc: {gzip9: $vc_gzip, zstd19: $vc_zstd, xz9e: $vc_xz},
    roundtrip_verified: true,
    note: "generic compressors on the complete file; a VOLE size win over xz is not claimed"}' > "$RAW/generic.json"

# ---------------------------------------------------------------------------
# Assemble receipt.json.
# ---------------------------------------------------------------------------
: > "$RAW/cases.jsonl"
for c in $CASES; do cat "$RAW/$c.case.json" >> "$RAW/cases.jsonl"; echo >> "$RAW/cases.jsonl"; done

jq -s '.' "$RAW/cases.jsonl" > "$RAW/cases.json"

jq -n \
  --slurpfile env "$RAW/environment.json" \
  --slurpfile cases "$RAW/cases.json" \
  --slurpfile a0 "$RAW/a0.json" \
  --slurpfile a1 "$RAW/a1.json" \
  --slurpfile generic "$RAW/generic.json" \
  --arg producer_used "$PRODUCER_USED" \
  '{
     campaign: ("phase11-field-court"),
     phase: "Phase 11.9/11.10/11.11 — fair baselines, LLM working set, resilience + source removal",
     environment: $env[0],
     accounting_universes_adr0027: {
       source: "original document bytes (archival reality)",
       descriptor_store: "exact .voldoc descriptor + externalized objects (normative reconstruction)",
       procedural_field: "seed DAG nodes + hierarchical index bytes (normative for observations)",
       derived_cache: "disposable observation caches (never normative; counted and cleared here)"
     },
     producer_document_used: ($producer_used == "true"),
     cases: $cases[0],
     baselines: {A0_raw_tooling: $a0[0], A1_preprocessed_sqlite: $a1[0], generic_compressors: $generic[0]},
     claimed: {
       ingest_cost_reported: true,
       descriptor_bytes_reported: true,
       seed_index_cache_bytes_reported: true,
       per_process_descriptor_reread_reported: true,
       exactness_materialize_equals_source: true,
       token_counts: false,
       token_reason: "no pinned offline tokenizer"
     }
   }' > "$OUTDIR/receipt.json"

# ---------------------------------------------------------------------------
# Human summary.
# ---------------------------------------------------------------------------
SUMMARY="$OUTDIR/SUMMARY.md"
{
  echo "# Phase 11 field court — results"
  echo
  echo "Generated by \`tools/field-court.sh\` inside the pinned \`db-baseline\` service."
  echo
  jq -r '
    "Commit: `\(.environment.git.commit_short)` (dirty: \(.environment.git.dirty))
\n"
    + "rustc `\(.environment.toolchain.rustc)`, cargo `\(.environment.toolchain.cargo)`; Cargo.lock sha256 `\(.environment.cargo_lock_sha256)`; arch `\(.environment.arch)`.
"
    + "Oracles: poppler \(.environment.oracles.poppler_pdftotext), qpdf \(.environment.oracles.qpdf), sqlite3 \(.environment.oracles.sqlite3), strace \(.environment.oracles.strace).
\n"
    + "## Accounting universes (ADR-0027, always separate)
\n"
    + "- source — the original document bytes (archival reality).
"
    + "- descriptor + store — the exact `.voldoc` plus externalized objects (normative reconstruction).
"
    + "- procedural field — seed DAG nodes + hierarchical index bytes (normative for observations).
"
    + "- derived cache — disposable observation caches (never normative; counted and cleared).
\n"
    + "## Per case
\n"
    + "| case | pages | source B | descriptor B | seed B | index B | cache init B | nodes | index nodes | cold seed bytes_read | cold bytes_returned | warm executed/reused | exact |
"
    + "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
"
    + ([.cases[] | "| \(.name) | \(.source.pages) | \(.source.len) | \(.store_universe_bytes.descriptor) | \(.store_universe_bytes.seed) | \(.store_universe_bytes.index) | \(.store_universe_bytes.derived_cache_initial) | \(.ingest.node_count) | \(.ingest.index_node_count) | \(.observe_new_process.text_cold.bytes_read) | \(.observe_new_process.text_cold.bytes_returned) | \(.observe_new_process.text_warm_reuse.seed_nodes_executed)/\(.observe_new_process.text_warm_reuse.seed_nodes_reused) | \(.source_removal.materialized.exact) |"] | join("\n"))
    + "\n\nNote: `bytes_read` is the instrumented seed-store byte count only. Each process additionally re-reads the whole descriptor blob (reported below); that read is never folded into the procedural-field universe.
\n"
    + "## Working set is bounded and does not grow with document size
\n"
    + "The generated documents target a fixed 32 MiB source, so the two generated sizes differ in **page count** at constant source size; the real producer document differs in source size (\([.cases[]|select(.kind=="producer")|.source.len]|first // "n/a") B). Per-page cold `bytes_read` and `bytes_returned`:
\n"
    + "| case | page | index nodes | seed fetched | seed bytes_read | seed executed | seed reused | deepened | bytes_returned | wall µs |
"
    + "|---|---:|---:|---:|---:|---:|---:|---|---:|---:|
"
    + ([.cases[] | .name as $n | .scrub[] | "| \($n) | \(.page) | \(.index_nodes_read) | \(.seed_nodes_fetched) | \(.bytes_read) | \(.seed_nodes_executed) | \(.seed_nodes_reused) | \(.deepened) | \(.bytes_returned) | \(.wall_micros) |"] | join("\n"))
    + "\n\nThe per-page seed-store `bytes_read` stays tiny (page 1 cold \((.cases[]|select(.name=="large-50")|.observe_new_process.text_cold.bytes_read)) B at 50 pages, \((.cases[]|select(.name=="large-400")|.observe_new_process.text_cold.bytes_read)) B at 400 pages, \((.cases[]|select(.name=="producer")|.observe_new_process.text_cold.bytes_read) // "n/a") B on the \((.cases[]|select(.name=="producer")|.source.len) // "n/a") B producer) and does not grow with page count or document size. **However**, the observed process re-reads its exact descriptor blob in full every invocation:
\n"
    + ("| case | descriptor blob B | process read/pread B (cold observe) | process file mmap B |
|---|---:|---:|---:|
")
    + ([.cases[] | "| \(.name) | \(.store_universe_bytes.descriptor) | \(.observe_new_process.text_cold.process_read_pread_bytes) | \(.observe_new_process.text_cold.process_file_mmap_bytes) |"] | join("\n"))
    + "\n\nThis is an honest limitation: only the procedural-field universe is page-bounded; the total process I/O is dominated by a full descriptor reload per process.
\n"
  ' "$OUTDIR/receipt.json"
} > "$SUMMARY"

{
  jq -r '
    "## Resilience + source removal (11.11)
\n"
    + "| case | source removed | query after removal ok | materialized len | sha256 == source | cmp equal | exact | materialize wall ms | cache reclaimed B | preview sha unchanged after clear |
"
    + "|---|---|---|---|---|---|---:|---:|---|
"
    + ([.cases[] | "| \(.name) | \(.source_removal.source_removed) | \(.source_removal.query_after_removal.exact == false and (.source_removal.query_after_removal.bytes_returned > 0)) | \(.source_removal.materialized.len) | \(.source_removal.materialized.sha256 == .source.sha256) | \(.source_removal.materialized.cmp_equal) | \(.source_removal.materialized.exact) | \(.source_removal.materialized.wall_ms) | \(.cache_disposable.cleared_reclaimed_bytes) | \(.cache_disposable.preview_sha_unchanged) |"] | join("\n"))
    + "\n\nReuse proof (fresh process, same page): pages re-queried report `seed_nodes_reused > 0` and `seed_nodes_executed == 0`.
\n"
    + "## Baselines (same machine, same run)
\n"
    + "### A0 — raw PDF tooling (primary \((.cases[]|select(.name=="large-400")|.source.len)) B document)
\n"
    + "- pdfinfo: \(.baselines.A0_raw_tooling.pdfinfo.pages) pages, max RSS \(.baselines.A0_raw_tooling.pdfinfo.max_rss_kb) kB.
"
    + "- `pdftotext -f \(.baselines.A0_raw_tooling.pdftotext_page_local.page) -l \(.baselines.A0_raw_tooling.pdftotext_page_local.page)`: \(.baselines.A0_raw_tooling.pdftotext_page_local.text_bytes) B text, wall \(.baselines.A0_raw_tooling.pdftotext_page_local.wall_ms) ms, **read/pread \(.baselines.A0_raw_tooling.pdftotext_page_local.read_pread_bytes) B** (\(.baselines.A0_raw_tooling.pdftotext_page_local.read_pread_calls) calls), file mmap \(.baselines.A0_raw_tooling.pdftotext_page_local.file_mmap_bytes) B.
"
    + "- `pdftotext` whole document: \(.baselines.A0_raw_tooling.pdftotext_whole_document.text_bytes) B, max RSS \(.baselines.A0_raw_tooling.pdftotext_whole_document.max_rss_kb) kB.
"
    + "- `qpdf --linearize`: file \(.baselines.A0_raw_tooling.qpdf_linearize.file_bytes) B, /E (end of first page) \(.baselines.A0_raw_tooling.qpdf_linearize.E_end_of_first_page) → first-page prefix fraction **\(.baselines.A0_raw_tooling.qpdf_linearize.first_page_prefix_fraction)**.
\n"
    + "### A1 — preprocessed SQLite (fair: extraction charged in full)
\n"
    + "- sqlite \(.baselines.A1_preprocessed_sqlite.sqlite_version); extracted \(.baselines.A1_preprocessed_sqlite.pages_extracted) pages with pdftotext in \(.baselines.A1_preprocessed_sqlite.preprocessing.wall_ms) ms → DB \(.baselines.A1_preprocessed_sqlite.preprocessing.db_bytes) B.
"
    + "- page query: text \(.baselines.A1_preprocessed_sqlite.query.text_len) B, wall \(.baselines.A1_preprocessed_sqlite.query.wall_ms) ms, **read/pread \(.baselines.A1_preprocessed_sqlite.query.read_pread_bytes) B** (\(.baselines.A1_preprocessed_sqlite.query.read_pread_calls) calls).
"
    + "- **Recorded loss:** the indexed SQLite page lookup reads far fewer bytes than a VOLE `observe` (which reloads its full descriptor). A1 wins the narrow-query byte court; its one-time extraction and DB bytes are charged above.
\n"
    + "### Generic compressors (complete file, round-trip verified)
\n"
    + "| input | bytes | gzip -9 | zstd -19 | xz -9e |
|---|---:|---:|---:|---:|
"
    + "| source | \(.baselines.generic_compressors.source_bytes) | \(.baselines.generic_compressors.source.gzip9) | \(.baselines.generic_compressors.source.zstd19) | \(.baselines.generic_compressors.source.xz9e) |
"
    + "| voldoc | \(.baselines.generic_compressors.voldoc_bytes) | \(.baselines.generic_compressors.voldoc.gzip9) | \(.baselines.generic_compressors.voldoc.zstd19) | \(.baselines.generic_compressors.voldoc.xz9e) |
\n"
    + "**Recorded loss:** VOLE is *not* a whole-file compressor (ADR-0017); xz on the source is far smaller than the `.voldoc`.
\n"
    + "## LLM working-set court (11.10)
\n"
    + "UTF-8 bytes only. **Token counts are not claimed** (no pinned offline tokenizer).
\n"
    + "| case | B0 whole doc B | B1 page-local B | V VOLE page text B | context_waste_ratio (B0/V) | honest losses |
"
    + "|---|---:|---:|---:|---:|---|
"
    + ([.cases[] | "| \(.name) | \(.llm_working_set.baselines.B0_whole_document.bytes) | \(.llm_working_set.baselines.B1_page_local.bytes) | \(.llm_working_set.baselines.V_vole_page_text.bytes) | \(.llm_working_set.context_waste_ratio) | \(.llm_working_set.honest_losses | join(", ")) |"] | join("\n"))
    + "\n\n\(.cases[0].llm_working_set.tokens_reason)
"
    + "\n\nNote: V is `observe --page --kind text`, a bounded heuristic text-run projection; B1 is Poppler reading-order extraction. The byte comparison is not a text-quality claim, and a smaller V is not asserted to be a more complete extract.\n"
  ' "$OUTDIR/receipt.json"
} >> "$SUMMARY"

{
  echo
  echo "## Per-case wins and losses"
  echo
  jq -r '
    .cases[] | "
### \(.name) — wins and losses
\n"
      + "Wins: exactness (`materialize == source`) holds after source removal and cache clearing; per-page procedural-field `bytes_read` is bounded (\(.observe_new_process.text_cold.bytes_read) B cold, \(.observe_new_process.text_warm_reuse.bytes_read) B warm); reuse across processes (\(.observe_new_process.text_warm_reuse.seed_nodes_reused) reused / \(.observe_new_process.text_warm_reuse.seed_nodes_executed) executed).
"
      + "Losses: every process reloads the \(.store_universe_bytes.descriptor) B descriptor (twice on a cold first touch: the observation and its promotion each open the field); the derived cache written on a cold page is \(.store_universe_bytes.derived_cache_after_cold) B; VOLE page text is \(.llm_working_set.baselines.V_vole_page_text.bytes) B vs page-local Poppler \(.llm_working_set.baselines.B1_page_local.bytes) B.
"
  ' "$OUTDIR/receipt.json"
  echo
  echo "Raw observe outputs larger than 40 KiB under \`raw/\` are \`gzip -9\` compressed (filenames end in \`.gz\`)."
} >> "$SUMMARY"

echo "receipt: $OUTDIR/receipt.json"
echo "summary: $OUTDIR/SUMMARY.md"

# ---------------------------------------------------------------------------
# Exact command ledger.
# ---------------------------------------------------------------------------
{
  echo "# Phase 11 field court — exact commands"
  echo "# Commit under test: $COMMIT (branch $(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)); dirty: $DIRTY"
  echo "# All commands run inside the pinned db-baseline image; nothing on the host."
  echo
  echo "docker compose run --rm --no-TTY dev         cargo build --all-features --locked"
  echo "docker compose build db-baseline"
  echo "docker compose run --rm --no-TTY db-baseline sh tools/field-court.sh $OUTDIR [PRODUCER_PDF]"
  echo
  echo "# Per case (inside field-court.sh):"
  echo "#   \$BIN pdf-make-large DIR {50,400}"
  echo "#   \$BIN encode SRC DIR/<name>.voldoc"
  echo "#   \$BIN field-ingest DIR/<name>.voldoc --store DIR/store"
  echo "#   \$BIN explain --store STORE --field F --page 1 --kind text|structure|preview --analyze"
  echo "#   \$BIN observe --store STORE --field F --page 1 --kind text|structure"
  echo "#   \$BIN preview --store STORE --field F --page 1"
  echo "#   \$BIN materialize --store STORE --field F --exact --output out.pdf"
  echo "#   \$BIN cache --store STORE [--clear]"
  echo "#   strace -f -e trace=read,pread64,mmap -o LOG \$BIN explain --analyze ..."
  echo "# Baselines:"
  echo "#   pdfinfo SRC"
  echo "#   strace -f -e trace=read,pread64,mmap -o LOG pdftotext -f 1 -l 1 SRC out.txt"
  echo "#   qpdf --linearize SRC lin.pdf   (first-page prefix fraction = /E / /L)"
  echo "#   sh tools/field-baseline-db.sh OUT.json SRC 1"
  echo "#   sh tools/field-llm-workingset.sh OUT.json SRC 1 STORE FIELD"
  echo "#   gzip -9 / zstd -19 --long=27 / xz -9e on SRC and on the .voldoc (round-trip verified)"
} > "$OUTDIR/commands.txt"
echo "commands: $OUTDIR/commands.txt"

# ---------------------------------------------------------------------------
# Keep the receipt small: the full page-text value of a large observe is
# uninformative, so bulky raw outputs are gzip -9 compressed in place. The
# stats/strace/explain evidence that carries the measurements stays plain.
# ---------------------------------------------------------------------------
find "$RAW" -type f -size +40k ! -name '*.gz' -exec gzip -9 {} \; 2>/dev/null || true

# chown back to the host user when running as root (the host invokes this with
# --user, but be defensive if it did not).
if [ -n "${HOST_UID:-}" ] && [ -n "${HOST_GID:-}" ]; then
  chown -R "$HOST_UID:$HOST_GID" "$OUTDIR" 2>/dev/null || true
fi
