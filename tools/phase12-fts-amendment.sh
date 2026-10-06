#!/usr/bin/env bash
# Phase-12 FTS5 amendment court (12.11 follow-up).
#
# The 12.11/12.11b lifetime receipt's A1 lane *created* an FTS5 external-content
# table but its search SQL used `blocks.text LIKE '%q%'`, so the claim that the
# search surface was "FTS5/BM25" was not backed by a receipt. This amendment makes
# the A1 search genuinely FTS5 (`fts MATCH`) in `tools/phase12-lifetime-court.sh`
# (`a1_sql`), keeps the old LIKE query as `a1_like_sql`, and measures both shadows
# of the *same* search case on the *same* committed corpus/schedule at
# N ∈ {1,10,100,1000}, so the FTS5-vs-LIKE difference is explicit.
#
# It measures, per document/variant/N, the process `read`+`pread64` total under
# `strace` for N sequential `sqlite3` invocations (fresh process per query, exactly
# the lifetime court's A1 camera) and the wall time of the *same* instrumented
# batch (both variants pay the identical strace overhead, so the comparison is
# fair; the wall is labelled as strace-instrumented). Both variants' answers are
# asserted against the committed schedule's expected marker.
#
# Usage (inside the pinned `doc-baseline` service):
#   bash tools/phase12-fts-amendment.sh [OUTDIR]
set -u

cd /work
LC_ALL=C
export LC_ALL

OUTDIR=${1:-evidence/campaigns/$(date -u +%Y-%m-%d)-phase12-fts5-amendment-$(git rev-parse --short HEAD 2>/dev/null || echo unknown)}
RAW="$OUTDIR/raw"

# Snapshot provenance *before* this script creates its own receipt directory.
COMMIT=$(git rev-parse HEAD 2>/dev/null || echo unknown)
COMMIT_SHORT=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)
DIRTY=$(git status --porcelain 2>/dev/null | head -40 | tr '\n' ';' || echo unknown)
RUN_UTC=$(date -u +%Y-%m-%dT%H:%M:%SZ)

rm -rf "$OUTDIR"
mkdir -p "$RAW/db"

SCHEDULE=${SCHEDULE:-tools/fixtures/phase12-lifetime-schedule.json}
CORPUS=${CORPUS:-evidence/scratch/phase12-lifetime-corpus}
NS_STR="1 10 100 1000"

now_us() { echo $(( ${EPOCHREALTIME/./} )); }
rp_bytes() {
  awk 'match($0,/^[0-9 ]*(read|pread64)\(/){if(match($0,/ = [0-9]+$/)){s+=substr($0,RSTART+2)+0}} match($0,/^[0-9 ]*<\.\.\. (read|pread64) resumed>/){if(match($0,/ = [0-9]+$/)){s+=substr($0,RSTART+2)+0}} END{print s+0}' "$1"
}

if [ ! -f "$CORPUS/ground_truth.json" ]; then
  echo "phase12-fts-amendment: generating corpus (stdlib)" >&2
  python3 tools/fixtures/phase12-corpus-gen.py "$CORPUS"
fi

FAIL=0
: > "$RAW/correctness.tsv"
: > "$RAW/search_bytes.tsv.tmp"

for name in $(jq -r '.documents[].name' "$SCHEDULE"); do
  fmt=$(jq -r --arg n "$name" '.documents[]|select(.name==$n).format' "$SCHEDULE")
  src_file=$(jq -r --arg n "$name" '.documents[]|select(.name==$n).source' "$SCHEDULE")
  src="$CORPUS/$src_file"
  arg=$(jq -r --arg n "$name" '.documents[]|select(.name==$n).cases[]|select(.id=="search").arg' "$SCHEDULE")
  ek=$(jq -r --arg n "$name" '.documents[]|select(.name==$n).cases[]|select(.id=="search").expected_kind' "$SCHEDULE")
  exp=$(jq -r --arg n "$name" '.documents[]|select(.name==$n).cases[]|select(.id=="search").expected' "$SCHEDULE")
  q=${arg#search:}
  db="$RAW/db/$name.db"
  echo "=== $name ($fmt) search:$q ===" >&2
  python3 tools/fixtures/phase12-baseline.py build --format "$fmt" --source "$src" --db "$db" > "$RAW/db/$name.build.json"

  sql_like="SELECT text FROM blocks WHERE doc_id=1 AND text LIKE '%$q%' LIMIT 1;"
  sql_uni="SELECT text FROM blocks WHERE doc_id=1 AND block_id IN (SELECT rowid FROM fts WHERE fts MATCH '$q') LIMIT 1;"
  sql_tri="SELECT text FROM blocks WHERE doc_id=1 AND block_id IN (SELECT rowid FROM fts_tri WHERE fts_tri MATCH '$q') LIMIT 1;"

  # Correctness: LIKE and the trigram FTS5 index must both answer the expected
  # marker; the unicode61 FTS5 index is recorded as-is (it may miss a marker that
  # is embedded in a larger token). Markers can appear deep in a long PDF text, so
  # the check is on the full answer; only the log is truncated.
  like_full=$(sqlite3 "$db" "$sql_like")
  uni_full=$(sqlite3 "$db" "$sql_uni")
  tri_full=$(sqlite3 "$db" "$sql_tri")
  like_ok=no; case "$like_full" in *"$exp"*) like_ok=yes;; esac
  tri_ok=no; case "$tri_full" in *"$exp"*) tri_ok=yes;; esac
  uni_ok=no; case "$uni_full" in *"$exp"*) uni_ok=yes;; esac
  [ "$like_ok" = yes ] || FAIL=$((FAIL + 1))
  [ "$tri_ok" = yes ] || FAIL=$((FAIL + 1))
  [ "$tri_full" = "$like_full" ] || FAIL=$((FAIL + 1))
  printf '%s\t%s\tlike_ok=%s\tfts_tri_ok=%s\tfts_uni_ok=%s\ttri_eq_like=%s\n' \
    "$name" "$ek" "$like_ok" "$tri_ok" "$uni_ok" "$([ "$tri_full" = "$like_full" ] && echo yes || echo no)" >> "$RAW/correctness.tsv"

  for variant in tri uni like; do
    case "$variant" in
      tri) sql="$sql_tri" ;;
      uni) sql="$sql_uni" ;;
      like) sql="$sql_like" ;;
    esac
    for n in $NS_STR; do
      st=$(mktemp)
      t0=$(now_us)
      strace -f -e trace=read,pread64 -o "$st" \
        bash -c 'd=$1; s=$2; n=$3; i=0; while [ $i -lt $n ]; do sqlite3 "$d" "$s" >/dev/null 2>&1; i=$((i+1)); done' \
        _ "$db" "$sql" "$n" >/dev/null 2>&1 || true
      t1=$(now_us)
      read_bytes=$(rp_bytes "$st")
      rm -f "$st"
      printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$fmt" "$variant" "$n" "$(( (t1-t0)/1000 ))" "$read_bytes" >> "$RAW/search_bytes.tsv.tmp"
    done
  done
done
mv "$RAW/search_bytes.tsv.tmp" "$RAW/search_bytes.tsv"

# ---------------------------------------------------------------------------
# Environment + receipt.
# ---------------------------------------------------------------------------
RUSTC_V=$(rustc --version 2>/dev/null | sed 's/^rustc //' || echo unknown)
CARGO_V=$(cargo --version 2>/dev/null | sed 's/^cargo //' || echo unknown)
ARCH=$(uname -m)
LOCK_SHA=$(sha256sum Cargo.lock 2>/dev/null | cut -d' ' -f1 || echo unknown)
BASE_STABLE=$(sed -n 's/^ARG BASE_STABLE=//p' Dockerfile 2>/dev/null | head -1)
IMAGE_ID=${HOST_IMAGE_ID:-unrecorded}
SQLITE_V=$(sqlite3 --version | awk '{print $1}')
JQ_V=$(jq --version)
NDOC=$(jq '.documents|length' "$SCHEDULE")

jq -n \
  --arg commit "$COMMIT" --arg commit_short "$COMMIT_SHORT" --arg branch "$BRANCH" --arg dirty "$DIRTY" \
  --arg lock_sha "$LOCK_SHA" --arg arch "$ARCH" --arg rustc "$RUSTC_V" --arg cargo "$CARGO_V" \
  --arg base_stable "$BASE_STABLE" --arg image_id "$IMAGE_ID" --arg sqlite "$SQLITE_V" --arg jq "$JQ_V" \
  --arg run_utc "$RUN_UTC" --argjson ndoc "$NDOC" --argjson fail "$FAIL" --arg ns "$NS_STR" \
  '{git:{commit:$commit,commit_short:$commit_short,branch:$branch,dirty:$dirty},
    arch:$arch, cargo_lock_sha256:$lock_sha, run_utc:$run_utc,
    service_image:"vole-document/doc-baseline:1.99.0", doc_baseline_image_id:$image_id,
    toolchain:{rustc:$rustc,cargo:$cargo}, base_image:$base_stable,
    oracles:{sqlite3:$sqlite,jq:$jq}, ns:$ns, documents:$ndoc, failures:$fail,
    camera:"fresh sqlite3 process per query under strace; process read+pread64 total; wall is the strace-instrumented batch for both variants",
    fts5_tables:["fts (unicode61 token index)","fts_tri (trigram substring index, used by the A1 search lane)"],
    query_fts_tri:"SELECT text FROM blocks WHERE doc_id=1 AND block_id IN (SELECT rowid FROM fts_tri WHERE fts_tri MATCH Q) LIMIT 1",
    query_fts_uni:"SELECT text FROM blocks WHERE doc_id=1 AND block_id IN (SELECT rowid FROM fts WHERE fts MATCH Q) LIMIT 1",
    query_like:"SELECT text FROM blocks WHERE doc_id=1 AND text LIKE %Q% LIMIT 1"}' \
  > "$OUTDIR/environment.json"

# ---------------------------------------------------------------------------
# SUMMARY — FTS5 vs LIKE, per document and total.
# ---------------------------------------------------------------------------
{
  echo "# Phase 12.11 — FTS5 amendment (A1 search surface: LIKE vs FTS5 unicode61 vs FTS5 trigram)"
  echo
  echo "Generated by \`tools/phase12-fts-amendment.sh\` inside the pinned, capped \`doc-baseline\` service."
  echo
  echo "Commit: \`$COMMIT_SHORT\` (branch \`$BRANCH\`, dirty: \`$DIRTY\`) · run (UTC) $RUN_UTC.  "
  echo "sqlite3 \`$SQLITE_V\` (FTS5 available), rustc \`$RUSTC_V\`; base \`$BASE_STABLE\`."
  echo
  echo "**Correction.** The 12.11/12.11b A1 lane *created* an FTS5 table but searched with"
  echo "\`blocks.text LIKE '%Q%'\`; no receipt measured FTS5. This amendment adds a real"
  echo "FTS5 surface to the A1 baseline and measures it at N ∈ {1,10,100,1000}. Two FTS5"
  echo "tokenizers are measured: \`unicode61\` (whole-token) and \`trigram\` (substring). The"
  echo "A1 search lane now uses \`fts_tri MATCH\` because the schedule's markers can be"
  echo "embedded in a larger token (e.g. \`theXF13A\`), which a whole-token index misses."
  echo
  echo "Correctness (\`raw/correctness.tsv\`): LIKE and FTS5-trigram answer the expected"
  echo "marker for all $NDOC documents; FTS5-unicode61 is recorded as-is (it misses"
  echo "embedded-token markers). Per-query read bytes are the strace \`read\`+\`pread64\`"
  echo "total for a batch of N fresh \`sqlite3\` processes (the lifetime court's A1 camera);"
  echo "wall is that instrumented batch, so all variants pay identical strace overhead."
  echo
  echo "## Per-document read bytes and wall (N = 1, 10, 100, 1000)"
  echo
  echo "| document | fmt | N | LIKE B | FTS5-trigram B | FTS5-unicode61 B | Δ(tri−LIKE) B | tri wall ms | like wall ms |"
  echo "|---|---|---:|---:|---:|---:|---:|---:|---:|"
  awk -F'\t' '
    { k=$1"\t"$2"\t"$4"\t"$3; b[k]=$6; w[k]=$5 }
    END { for (x in b) { split(x,a,"\t"); doc=a[1]; fmt=a[2]; nn=a[3]; v=a[4];
      if (v!="tri") continue;
      base=a[1]"\t"a[2]"\t"a[3]"\t";
      tri=b[x]; like=b[base"like"]; uni=b[base"uni"];
      if (tri==""||like=="") continue;
      printf "| %s | %s | %s | %s | %s | %s | %d | %s | %s |\n", doc, fmt, nn, like, tri, (uni==""?"-":uni), tri-like, w[x], w[base"like"] } }' \
    "$RAW/search_bytes.tsv" | sort -t'|' -k2,2 -k4,4n
  echo
  echo "## Totals at N=1000 ($NDOC documents)"
  awk -F'\t' '$4==1000 { b[$3]+=$6; w[$3]+=$5 }
    END { printf "| variant | total read B | total wall ms |\n|---|---:|---:|\n| LIKE | %d | %d |\n| FTS5-trigram | %d | %d |\n| FTS5-unicode61 | %d | %d |\n", b["like"], w["like"], b["tri"], w["tri"], b["uni"], w["uni"] }' \
    "$RAW/search_bytes.tsv"
  echo
  echo "**Reading.** FTS5 resolves the marker through an inverted index; LIKE scans"
  echo "\`blocks.text\`. On this small self-authored corpus both are dominated by the"
  echo "per-query \`sqlite3\` process + database open, so the absolute numbers are close;"
  echo "the table reports the measured difference, not an assumption. No claim is made"
  echo "that FTS5 is faster in general."
  echo
  echo "Assertion failures: **$FAIL** (must be 0)."
} > "$OUTDIR/SUMMARY.md"

cat > "$OUTDIR/commands.txt" <<EOF
# Phase-12 FTS5 amendment court — exact commands
# Commit under test: $COMMIT (branch $BRANCH); dirty: $DIRTY
# Service image: vole-document/doc-baseline:1.99.0 (id $IMAGE_ID); base $BASE_STABLE
# rustc $RUSTC_V, cargo $CARGO_V, sqlite3 $SQLITE_V; Cargo.lock sha256: $LOCK_SHA
# Run (UTC): $RUN_UTC
# All commands run inside the pinned, capped doc-baseline image; nothing on the host.

docker compose run --rm --no-TTY -e HOST_IMAGE_ID=$IMAGE_ID \\
    doc-baseline bash tools/phase12-fts-amendment.sh $OUTDIR

# Inside:
#   python3 tools/fixtures/phase12-corpus-gen.py evidence/scratch/phase12-lifetime-corpus   (if absent)
#   python3 tools/fixtures/phase12-baseline.py build --format FMT --source SRC --db DB
#   sqlite3 DB "SELECT text FROM blocks WHERE doc_id=1 AND block_id IN (SELECT rowid FROM fts_tri WHERE fts_tri MATCH 'Q') LIMIT 1;"
#   sqlite3 DB "SELECT text FROM blocks WHERE doc_id=1 AND text LIKE '%Q%' LIMIT 1;"
EOF

( cd "$OUTDIR" && sha256sum SUMMARY.md commands.txt environment.json raw/*.tsv > raw.sha256 ) 2>/dev/null || true

echo "phase12-fts-amendment: documents=$NDOC ns='$NS_STR' failures=$FAIL" >&2
echo "wrote $OUTDIR" >&2
[ "$FAIL" -eq 0 ]
