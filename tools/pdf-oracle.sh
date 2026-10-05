#!/bin/sh
# Phase 3.7 differential oracle: our byte-authoritative PDF scanner against qpdf.
#
# qpdf is used strictly as an *oracle* (an independent structural reference),
# never as byte authority. This court:
#   1. generates a small deterministic corpus of real, valid PDFs (classic xref,
#      two-page, and an incremental update) with exact /Length and startxref
#      offsets;
#   2. runs our scanner's machine-readable view (`pdf-inspect`) to get our
#      object-number set;
#   3. runs `qpdf --check` and asserts the file is reported valid;
#   4. enumerates qpdf's object-number set with `qpdf --show-xref` (lines of the
#      form `N/G: ...`) and compares it with ours;
#   5. records matched / our_only / qpdf_only and page count from `pdfinfo`.
#
# A disagreement is a FINDING to report (and exits nonzero), never something to
# hide. Run from inside the tools image, cwd /work:
#
#   docker compose run --rm --no-TTY tools sh tools/pdf-oracle.sh [OUTDIR]
#
# OUTDIR defaults to evidence/scratch/pdfs; oracle.jsonl is written there.
set -eu

cd /work

# Optional first argument: the output directory (default evidence/scratch/pdfs).
# `oracle.jsonl`, the generated corpus, and scratch files all live there.
OUTDIR="${1:-evidence/scratch/pdfs}"
WORK="$OUTDIR"
BIN="${BIN:-$WORK/vole-document}"
CORPUS="$WORK/corpus"
JSONL="$WORK/oracle.jsonl"
SCRATCH="$WORK/oracle-scratch"

rm -rf "$CORPUS" "$SCRATCH"
mkdir -p "$CORPUS" "$SCRATCH"
: > "$JSONL"

fail=0

# json_list "1 2 3 ": render a space-separated number list as a JSON array,
# yielding [] when empty.
json_list() {
  if [ -z "$1" ]; then
    printf '[]'
  else
    printf '["%s"]' "$(printf '%s' "$1" | sed 's/ $//; s/ /","/g')"
  fi
}

# ---------------------------------------------------------------------------
# Corpus generation
# ---------------------------------------------------------------------------

LAST_XREF=0
OBJLIST=""

# emit_obj NUM DATA: append DATA (printf %b interpreted) to $f and record the
# object's start offset in $OBJLIST.
emit_obj() {
  _num=$1
  _data=$2
  _off=$(wc -c < "$f" | tr -d ' ')
  printf '%b' "$_data" >> "$f"
  printf '%s %s\n' "$_num" "$_off" >> "$OBJLIST"
}

# write_xref_classic MAX: append a classic xref table + trailer + startxref +
# %%EOF covering objects 1..MAX (all present). Sets LAST_XREF.
write_xref_classic() {
  _max=$1
  _xref=$(wc -c < "$f" | tr -d ' ')
  LAST_XREF=$_xref
  printf 'xref\n0 %d\n' "$((_max + 1))" >> "$f"
  printf '%010d 65535 f \n' 0 >> "$f"
  _n=1
  while [ "$_n" -le "$_max" ]; do
    _off=$(awk -v k="$_n" '$1 == k { print $2 }' "$OBJLIST")
    printf '%010d 00000 n \n' "$_off" >> "$f"
    _n=$((_n + 1))
  done
  printf 'trailer\n<< /Size %d /Root 1 0 R >>\n' "$((_max + 1))" >> "$f"
  printf 'startxref\n%s\n%%%%EOF\n' "$_xref" >> "$f"
}

# gen_classic PAGES OUTNAME: a valid classic-xref PDF with PAGES pages.
# Object layout: 1 Catalog, 2 Pages, 3..(2+P) Page objects,
# (3+P)..(2+2P) content streams. Four objects for one page.
gen_classic() {
  _pages=$1
  f="$CORPUS/$2"
  OBJLIST="$SCRATCH/objlist.$2"
  : > "$f"
  : > "$OBJLIST"
  printf '%%PDF-1.4\n' >> "$f"

  emit_obj 1 '1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n'

  _kids=""
  _p=0
  while [ "$_p" -lt "$_pages" ]; do
    _kids="$_kids$((3 + _p)) 0 R "
    _p=$((_p + 1))
  done
  emit_obj 2 "2 0 obj\n<< /Type /Pages /Kids [ $_kids] /Count $_pages >>\nendobj\n"

  _p=0
  while [ "$_p" -lt "$_pages" ]; do
    _pagenum=$((3 + _p))
    _contentnum=$((3 + _pages + _p))
    emit_obj "$_pagenum" \
      "$_pagenum 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents $_contentnum 0 R >>\nendobj\n"
    _p=$((_p + 1))
  done

  _p=0
  while [ "$_p" -lt "$_pages" ]; do
    _contentnum=$((3 + _pages + _p))
    _body="BT /F1 24 Tf 72 700 Td (Page $((_p + 1)) of $_pages) Tj ET"
    _len=${#_body}
    emit_obj "$_contentnum" \
      "$_contentnum 0 obj\n<< /Length $_len >>\nstream\n$_body\nendstream\nendobj\n"
    _p=$((_p + 1))
  done

  write_xref_classic "$((2 + 2 * _pages))"
}

# gen_incremental OUTNAME: a one-page classic base plus an appended revision
# that adds object 5, with a new xref subsection and a /Prev chain.
gen_incremental() {
  gen_classic 1 "$1"
  _base_xref=$LAST_XREF

  _off5=$(wc -c < "$f" | tr -d ' ')
  printf '%b' '5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n' >> "$f"

  _newxref=$(wc -c < "$f" | tr -d ' ')
  {
    printf 'xref\n'
    printf '0 1\n'
    printf '%010d 65535 f \n' 0
    printf '5 1\n'
    printf '%010d 00000 n \n' "$_off5"
  } >> "$f"
  printf 'trailer\n<< /Size 6 /Root 1 0 R /Prev %s >>\n' "$_base_xref" >> "$f"
  printf 'startxref\n%s\n%%%%EOF\n' "$_newxref" >> "$f"
}

echo "=== generating deterministic corpus ==="
gen_classic 1 classic.pdf
gen_classic 2 twopage.pdf
gen_incremental incremental.pdf
ls -l "$CORPUS"

# ---------------------------------------------------------------------------
# Negative control: a non-PDF must be reported, not error.
# ---------------------------------------------------------------------------
NONPDF="$CORPUS/notpdf.bin"
printf 'this is plain text, definitely not a PDF\n' > "$NONPDF"
nonpdf_out=$("$BIN" pdf-inspect "$NONPDF" 2>"$SCRATCH/nonpdf.err") || {
  echo "FINDING: pdf-inspect exited nonzero on a non-PDF" >&2
  fail=1
}
case "$nonpdf_out" in
  *'"is_pdf":false'*) nonpdf_ok=true ;;
  *) nonpdf_ok=false; fail=1 ;;
esac
printf 'non-PDF control: is_pdf=false observed=%s\n' "$nonpdf_ok"
printf '{"file":"%s","control":"non_pdf","is_pdf":false,"ok":%s}\n' \
  "notpdf.bin" "$nonpdf_ok" >> "$JSONL"

# ---------------------------------------------------------------------------
# Per-PDF differential court
# ---------------------------------------------------------------------------
printf '\n=== differential oracle: our scanner vs qpdf 11.3 ===\n'
printf '%-16s %10s %10s %10s %10s %12s %8s %7s\n' \
  file our_objs qpdf_objs matched our_only qpdf_only qpdf_check pages
printf '%-16s %10s %10s %10s %10s %12s %8s %7s\n' \
  ---------------- ---------- ---------- ---------- ---------- ------------ -------- -------

for f in "$CORPUS"/classic.pdf "$CORPUS"/twopage.pdf "$CORPUS"/incremental.pdf; do
  name=$(basename "$f")

  # (a) our structural view.
  our_json=$("$BIN" pdf-inspect "$f")
  our_sorted="$SCRATCH/our.$name"
  printf '%s\n' "$our_json" | grep -o '"number":[0-9]*' | cut -d: -f2 | LC_ALL=C sort -u > "$our_sorted"
  our_count=$(wc -l < "$our_sorted" | tr -d ' ')

  # (b) qpdf --check: assert it reports a valid PDF.
  set +e
  qpdf --check "$f" > "$SCRATCH/check.$name.txt" 2>&1
  qc_rc=$?
  set -e
  if grep -q 'No syntax or stream encoding errors found' "$SCRATCH/check.$name.txt"; then
    qc=valid
  else
    qc=INVALID
    fail=1
    echo "FINDING: qpdf --check did not validate $name (rc=$qc_rc)" >&2
    cat "$SCRATCH/check.$name.txt" >&2
  fi

  # (c) qpdf object enumeration: `--show-xref` prints lines `N/G: ...`.
  # Prefer it if it lists `obj gen offset` style entries; fall back to --json.
  qpdf_sorted="$SCRATCH/qpdf.$name"
  qpdf --show-xref "$f" 2>"$SCRATCH/xref.$name.err" \
    | sed -n 's#^\([0-9][0-9]*\)/[0-9][0-9]*:.*#\1#p' > "$qpdf_sorted"
  if [ ! -s "$qpdf_sorted" ]; then
    echo "NOTE: --show-xref produced no entries for $name; falling back to --json" >&2
    qpdf --json "$f" 2>/dev/null \
      | grep -o '"obj":[[:space:]]*"[0-9][0-9]* [0-9][0-9]* R"' \
      | sed -n 's/.*"\([0-9][0-9]*\) [0-9][0-9]* R".*/\1/p' >> "$qpdf_sorted" || true
  fi
  LC_ALL=C sort -u "$qpdf_sorted" -o "$qpdf_sorted"
  qpdf_count=$(wc -l < "$qpdf_sorted" | tr -d ' ')

  # (d) set comparison.
  comm -12 "$our_sorted" "$qpdf_sorted" > "$SCRATCH/matched.$name"
  comm -23 "$our_sorted" "$qpdf_sorted" > "$SCRATCH/our_only.$name"
  comm -13 "$our_sorted" "$qpdf_sorted" > "$SCRATCH/qpdf_only.$name"
  matched_count=$(wc -l < "$SCRATCH/matched.$name" | tr -d ' ')
  our_only_count=$(wc -l < "$SCRATCH/our_only.$name" | tr -d ' ')
  qpdf_only_count=$(wc -l < "$SCRATCH/qpdf_only.$name" | tr -d ' ')

  matched=$(tr '\n' ' ' < "$SCRATCH/matched.$name")
  our_only=$(tr '\n' ' ' < "$SCRATCH/our_only.$name")
  qpdf_only=$(tr '\n' ' ' < "$SCRATCH/qpdf_only.$name")

  # (e) second, informational oracle signal: page count.
  pages=$(pdfinfo "$f" 2>/dev/null | sed -n 's/^Pages:[[:space:]]*//p')
  pages=${pages:-?}

  if [ "$our_only_count" -ne 0 ] || [ "$qpdf_only_count" -ne 0 ]; then
    fail=1
  fi

  printf '{"file":"%s","is_pdf":true,"our_count":%s,"qpdf_count":%s,' \
    "$name" "$our_count" "$qpdf_count" >> "$JSONL"
  printf '"matched":%s,"matched_count":%s,' \
    "$(json_list "$matched")" "$matched_count" >> "$JSONL"
  printf '"our_only":%s,"our_only_count":%s,' \
    "$(json_list "$our_only")" "$our_only_count" >> "$JSONL"
  printf '"qpdf_only":%s,"qpdf_only_count":%s,' \
    "$(json_list "$qpdf_only")" "$qpdf_only_count" >> "$JSONL"
  printf '"qpdf_check":"%s","qpdf_check_rc":%s,"pdfinfo_pages":"%s"}\n' \
    "$qc" "$qc_rc" "$pages" >> "$JSONL"

  printf '%-16s %10s %10s %10s %10s %12s %8s %7s\n' \
    "$name" "$our_count" "$qpdf_count" "$matched_count" "$our_only_count" \
    "$qpdf_only_count" "$qc" "$pages"

  if [ "$our_only_count" -ne 0 ] || [ "$qpdf_only_count" -ne 0 ]; then
    echo "FINDING: object set disagreement on $name" >&2
    echo "  our_only : $our_only" >&2
    echo "  qpdf_only: $qpdf_only" >&2
  fi
done

# ---------------------------------------------------------------------------
# Verdict
# ---------------------------------------------------------------------------
echo
echo "oracle.jsonl:"
cat "$JSONL"

if [ "$fail" -eq 0 ]; then
  echo "PDF ORACLE: PASS"
  exit 0
fi
echo "PDF ORACLE: FAIL" >&2
exit 1
