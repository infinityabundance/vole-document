#!/bin/sh
# Phase-11.9 A1 — fair *preprocessed* SQLite baseline for page-local text.
#
# The honest conventional baseline the Phase-11 plan calls A1: extract each
# page's text once with the same oracle the A0 lane uses (`pdftotext`, Poppler),
# load it into an indexed SQLite table, and then answer the same page-text query
# from the database. The one-time extraction cost and the resulting database
# bytes are charged in full; no preprocessing is hidden.
#
# Extraction uses `pdftotext -f P -l P` per page (so each page row is
# page-local in *content*, even though Poppler still has to locate the page) and
# inserts the exact extracted bytes with SQLite's `readfile()` (no shell
# quoting, no re-encoding). The query is timed twice: once plainly (wall clock)
# and once under `strace -f -e trace=read,pread64` (bytes read by syscalls).
#
# A SQLite win here is expected and is recorded, not hidden: a narrow indexed
# lookup reads a few KB, while a VOLE `observe` re-reads its descriptor blob.
#
# Usage (inside a service that has sqlite3 + poppler + strace, e.g. db-baseline):
#   sh tools/field-baseline-db.sh OUT.json PDF PAGE [MAXPAGES]
set -eu

cd /work
LC_ALL=C
export LC_ALL

OUT=$1
PDF=$2
PAGE=$3
MAXPAGES=${4:-0}
[ -n "$PDF" ] && [ -n "$PAGE" ] || { echo "usage: field-baseline-db.sh OUT.json PDF PAGE [MAXPAGES]" >&2; exit 2; }
[ -f "$PDF" ] || { echo "no such PDF: $PDF" >&2; exit 2; }

SQLITE_V=$(sqlite3 --version | awk '{print $1}')
SRC_BYTES=$(stat -c %s "$PDF")
SRC_SHA=$(sha256sum "$PDF" | cut -d' ' -f1)

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
DB="$TMP/pages.db"
PAGEDIR="$TMP/pages"
mkdir -p "$PAGEDIR"

# Page count from Poppler itself (the oracle's own view of the document).
NPAGES=$(pdfinfo "$PDF" | awk -F: '/^Pages:/{gsub(/ /,"",$2); print $2}')
case "$NPAGES" in ''|*[!0-9]*) NPAGES=0 ;; esac
if [ "$MAXPAGES" -gt 0 ] 2>/dev/null && [ "$MAXPAGES" -lt "$NPAGES" ]; then
  NPAGES=$MAXPAGES
fi

# ---------------------------------------------------------------------------
# One-time preprocessing: pdftotext per page -> file -> indexed SQLite table.
# ---------------------------------------------------------------------------
T0=$(date +%s%N)
i=1
while [ "$i" -le "$NPAGES" ]; do
  pdftotext -f "$i" -l "$i" "$PDF" "$PAGEDIR/p$i.txt" 2>/dev/null || : > "$PAGEDIR/p$i.txt"
  i=$((i + 1))
done
{
  echo "PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF;"
  echo "CREATE TABLE pages(page INTEGER PRIMARY KEY, text BLOB);"
  i=1
  while [ "$i" -le "$NPAGES" ]; do
    echo "INSERT INTO pages(page,text) VALUES($i, readfile('$PAGEDIR/p$i.txt'));"
    i=$((i + 1))
  done
  echo "CREATE INDEX idx_pages_page ON pages(page);"
} | sqlite3 "$DB" >/dev/null
T1=$(date +%s%N)
PREPROCESS_MS=$(( (T1 - T0) / 1000000 ))
DB_BYTES=$(stat -c %s "$DB")

# ---------------------------------------------------------------------------
# Query: the same page-text lookup, timed plainly (wall) and under strace.
# ---------------------------------------------------------------------------
Q0=$(date +%s%N)
QTEXT_LEN=$(sqlite3 "$DB" "SELECT length(text) FROM pages WHERE page=$PAGE;" | head -1)
QTEXT_SHA=$(sqlite3 "$DB" "SELECT text FROM pages WHERE page=$PAGE;" | sha256sum | cut -d' ' -f1)
Q1=$(date +%s%N)
QUERY_MS=$(( (Q1 - Q0) / 1000000 ))

strace -f -e trace=read,pread64 -o "$TMP/q.strace" \
  sqlite3 "$DB" "SELECT text FROM pages WHERE page=$PAGE;" >/dev/null 2>&1 || true
QR_CALLS=$(awk '/^[0-9 ]*(read|pread64)\(/{c++} /^[0-9 ]*<\.\.\. (read|pread64) resumed>/{c++} END{print c+0}' "$TMP/q.strace")
QR_BYTES=$(awk 'match($0,/^[0-9 ]*(read|pread64)\(/){if(match($0,/ = [0-9]+$/)){s+=substr($0,RSTART+2)+0}} match($0,/^[0-9 ]*<\.\.\. (read|pread64) resumed>/){if(match($0,/ = [0-9]+$/)){s+=substr($0,RSTART+2)+0}} END{print s+0}' "$TMP/q.strace")

jq -n \
  --arg tool "sqlite3" \
  --arg sqlite_version "$SQLITE_V" \
  --arg pdf "$PDF" \
  --argjson src_bytes "$SRC_BYTES" \
  --arg src_sha256 "$SRC_SHA" \
  --argjson pages "$NPAGES" \
  --argjson preprocess_wall_ms "$PREPROCESS_MS" \
  --argjson db_bytes "$DB_BYTES" \
  --argjson page "$PAGE" \
  --argjson query_wall_ms "$QUERY_MS" \
  --argjson query_read_pread_calls "$QR_CALLS" \
  --argjson query_read_pread_bytes "$QR_BYTES" \
  --argjson query_text_len "${QTEXT_LEN:-0}" \
  --arg query_text_sha256 "$QTEXT_SHA" \
  '{
     tool: $tool,
     sqlite_version: $sqlite_version,
     oracle: "pdftotext (poppler)",
     source_pdf: $pdf,
     source_bytes: $src_bytes,
     source_sha256: $src_sha256,
     pages_extracted: $pages,
     preprocessing: {wall_ms: $preprocess_wall_ms, db_bytes: $db_bytes},
     query: {
       page: $page,
       wall_ms: $query_wall_ms,
       read_pread_calls: $query_read_pread_calls,
       read_pread_bytes: $query_read_pread_bytes,
       text_len: $query_text_len,
       text_sha256: $query_text_sha256
     }
   }' > "$OUT"

cat "$OUT"
