#!/bin/sh
# Phase-7.0 producer-stratified Flate corpus.
#
# Runs entirely inside the pinned `tools` image (qpdf 11.3.0, Ghostscript
# 10.00.0; no python, no rust). It:
#   1. writes 1-2 small valid base PDFs BY HAND in shell — correct /Length,
#      classic xref, startxref, a page with a sizeable content stream, and at
#      least one embedded `/FlateDecode` stream. The embedded Flate stream is a
#      real zlib stream built by hand from RFC 1951 *stored* blocks plus a
#      computed Adler-32; no third-party compressor is used to create the base.
#   2. produces variants with genuinely distinct producers:
#        * Ghostscript `pdfwrite` at `/default`, `/prepress`, `/printer`,
#          `/ebook`, `/screen` (one output per setting);
#        * qpdf `--compress-streams=y --object-streams=generate
#          --stream-data=compress`, `--linearize`, `--object-streams=preserve`,
#          and a `--compress-streams=n` control.
#   3. includes any dev-generated synthetic samples that were placed under
#      `evidence/corpus/phase7/_synthetic/` beforehand:
#        docker compose run --rm --no-TTY dev sh -c \
#          './target/debug/vole-document pdf-make-samples evidence/corpus/phase7/_synthetic'
#   4. runs `qpdf --check` on every produced PDF and records the exact command,
#      producer, producer version, and SHA-256 of every file in
#      `evidence/corpus/phase7/provenance.json`.
#
# The regenerable `.pdf` bytes are gitignored; the script and provenance.json
# are committed. No third-party document bytes are used anywhere.
#
# Usage (from the host):
#   docker compose run --rm --no-TTY tools sh tools/pdf-corpus.sh
set -eu

cd /work
LC_ALL=C
export LC_ALL

OUT=evidence/corpus/phase7
SYN="$OUT/_synthetic"
TMP="$OUT/.tmp"
mkdir -p "$OUT" "$SYN" "$TMP"

QPDF_V="$(qpdf --version | awk '{print $3; exit}')"
GS_V="$(gs --version)"
echo "tool versions: qpdf=$QPDF_V ghostscript=$GS_V"

PROVENANCE="$OUT/provenance.json"
ENTRIES="$TMP/provenance-entries.jsonl"
: > "$ENTRIES"

# SHA-256 of a file, lowercase hex.
sha_of() {
  sha256sum "$1" | cut -d' ' -f1
}

# record NAME PRODUCER PRODUCER_VERSION COMMAND: append a provenance entry.
record() {
  _name=$1; _producer=$2; _pver=$3; _cmd=$4
  _sha="$(sha_of "$OUT/$_name")"
  printf '{"name":"%s","sha256":"%s","producer":"%s","producer_version":"%s","command":"%s","license":"locally-generated"}\n' \
    "$_name" "$_sha" "$_producer" "$_pver" "$_cmd" >> "$ENTRIES"
}

# ---------------------------------------------------------------------------
# zlib_store IN OUT — build a real zlib (RFC 1950) stream by hand.
#
# The DEFLATE payload is a sequence of RFC 1951 *stored* (BTYPE=00) blocks of
# up to 65535 bytes, each with LEN/NLEN, followed by the big-endian Adler-32 of
# the input. Structurally valid, and deliberately weakly coded (a level-0-like
# appearance) so the exact-replay correction ratio is observable. Bytes are
# emitted as octal escapes (which survive shell command substitution) and
# converted back to raw bytes one line at a time.
# ---------------------------------------------------------------------------
zlib_store() {
  _in=$1; _out=$2
  : > "$_out"
  od -An -v -tu1 "$_in" | awk '
    { for (i = 1; i <= NF; i++) b[++n] = $i + 0 }
    function emit(v) {
      line = line sprintf("\\%03o", v)
      if (length(line) >= 256) { print line; line = "" }
    }
    function block(fin, len, start,   k, lo, hi, nl) {
      lo = len % 256; hi = int(len / 256); nl = 65535 - len
      emit(fin ? 1 : 0)
      emit(lo); emit(hi)
      emit(nl % 256); emit(int(nl / 256) % 256)
      for (k = 0; k < len; k++) emit(b[start + k + 1])
    }
    END {
      # CMF=0x78 (deflate, 32K window), FLG=0x01 (FCHECK valid, FLEVEL=0).
      emit(0x78); emit(0x01)
      a = 1; bb = 0
      for (i = 1; i <= n; i++) { a = (a + b[i]) % 65521; bb = (bb + a) % 65521 }
      adler = bb * 65536 + a
      off = 0
      if (n == 0) {
        block(1, 0, 0)
      } else {
        while (off < n) {
          len = n - off; if (len > 65535) len = 65535
          block((off + len >= n) ? 1 : 0, len, off)
          off += len
        }
      }
      emit(int(adler / 16777216) % 256)
      emit(int(adler / 65536) % 256)
      emit(int(adler / 256) % 256)
      emit(adler % 256)
      if (length(line) > 0) print line
    }
  ' | while IFS= read -r _line; do printf '%b' "$_line"; done >> "$_out"
}

# gen_content SEED LINES OUT — a deterministic, sizeable page content stream.
gen_content() {
  _seed=$1; _lines=$2; _out=$3
  : > "$_out"
  printf 'BT\n/F1 11 Tf\n72 740 Td\n14 TL\n' >> "$_out"
  _i=1
  while [ "$_i" -le "$_lines" ]; do
    if [ $((_i % 3)) -eq 0 ]; then
      printf '(%s) Tj T*\n' "Line $_i of $_seed: repeated text shared across pages, amount $((_i * 7 % 1000)).00" >> "$_out"
    else
      printf '(%s) Tj T*\n' "Line $_i of $_seed: the quick brown fox jumps over the lazy dog $_i" >> "$_out"
    fi
    _i=$((_i + 1))
  done
  printf 'ET\n' >> "$_out"
}

# ---------------------------------------------------------------------------
# PDF writer helpers (classic xref, exact offsets and /Length)
# ---------------------------------------------------------------------------
base_pdf_header() { # FILE
  printf '%%PDF-1.5\n' > "$1"
}

obj_plain() { # FILE NUM DATA(%b escapes) OBJLIST
  _f=$1; _num=$2; _data=$3; _list=$4
  _off=$(wc -c < "$_f" | tr -d ' ')
  printf '%b' "$_data" >> "$_f"
  printf '%s %s\n' "$_num" "$_off" >> "$_list"
}

obj_stream() { # FILE NUM DICT_SUFFIX CONTENT_FILE OBJLIST
  _f=$1; _num=$2; _extra=$3; _content=$4; _list=$5
  _len=$(wc -c < "$_content" | tr -d ' ')
  _off=$(wc -c < "$_f" | tr -d ' ')
  printf '%s 0 obj\n<< /Length %s%s >>\nstream\n' "$_num" "$_len" "$_extra" >> "$_f"
  cat "$_content" >> "$_f"
  printf '\nendstream\nendobj\n' >> "$_f"
  printf '%s %s\n' "$_num" "$_off" >> "$_list"
}

write_xref() { # FILE OBJLIST MAX
  _f=$1; _list=$2; _max=$3
  _xref=$(wc -c < "$_f" | tr -d ' ')
  printf 'xref\n0 %d\n' "$((_max + 1))" >> "$_f"
  printf '%010d 65535 f \n' 0 >> "$_f"
  _n=1
  while [ "$_n" -le "$_max" ]; do
    _off=$(awk -v k="$_n" '$1 == k { print $2 }' "$_list")
    printf '%010d 00000 n \n' "$_off" >> "$_f"
    _n=$((_n + 1))
  done
  printf 'trailer\n<< /Size %d /Root 1 0 R >>\n' "$((_max + 1))" >> "$_f"
  printf 'startxref\n%s\n%%%%EOF\n' "$_xref" >> "$_f"
}

# write_base1 — one page whose /Contents is an array: object 5 (uncompressed
# text) and object 6 (a hand-built stored-block Flate stream).
write_base1() {
  _f="$OUT/hand-base1.pdf"
  _list="$TMP/objlist.base1"; : > "$_list"
  base_pdf_header "$_f"
  obj_plain "$_f" 1 '1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n' "$_list"
  obj_plain "$_f" 2 '2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n' "$_list"
  obj_plain "$_f" 3 '3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents [5 0 R 6 0 R] >>\nendobj\n' "$_list"
  obj_plain "$_f" 4 '4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n' "$_list"

  gen_content base1 text 700 "$TMP/base1.p5.txt"
  obj_stream "$_f" 5 '' "$TMP/base1.p5.txt" "$_list"

  gen_content base1 flate 900 "$TMP/base1.p6.txt"
  zlib_store "$TMP/base1.p6.txt" "$TMP/base1.p6.z"
  obj_stream "$_f" 6 ' /Filter /FlateDecode' "$TMP/base1.p6.z" "$_list"

  write_xref "$_f" "$_list" 6
}

# write_base2 — one page whose /Contents array holds two FlateDecode streams
# that share the *same* plaintext (object 5 and object 6): the shared-plaintext
# geometry the Phase-6 win region needs, both weakly coded (stored blocks).
write_base2() {
  _f="$OUT/hand-base2.pdf"
  _list="$TMP/objlist.base2"; : > "$_list"
  base_pdf_header "$_f"
  obj_plain "$_f" 1 '1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n' "$_list"
  obj_plain "$_f" 2 '2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n' "$_list"
  obj_plain "$_f" 3 '3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents [5 0 R 6 0 R] >>\nendobj\n' "$_list"
  obj_plain "$_f" 4 '4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n' "$_list"

  gen_content base2 shared 1200 "$TMP/base2.p5.txt"
  zlib_store "$TMP/base2.p5.txt" "$TMP/base2.p5.z"
  obj_stream "$_f" 5 ' /Filter /FlateDecode' "$TMP/base2.p5.z" "$_list"
  obj_stream "$_f" 6 ' /Filter /FlateDecode' "$TMP/base2.p5.z" "$_list"

  write_xref "$_f" "$_list" 6
}

# ---------------------------------------------------------------------------
# Producer variants
# ---------------------------------------------------------------------------
# gs_variant SETTING OUTNAME INPUT
gs_variant() {
  _setting=$1; _outname=$2; _in=$3
  _cmd="gs -q -dNOPAUSE -dBATCH -sDEVICE=pdfwrite -dCompressPages=true -dUseFlateCompression=true -dPDFSETTINGS=/$_setting -sOutputFile=$OUT/$_outname $_in"
  eval "$_cmd"
  record "$_outname" "ghostscript" "$GS_V" "$_cmd"
}

# qpdf_variant OUTNAME INPUT ARGS...
qpdf_variant() {
  _outname=$1; _in=$2; shift 2
  _cmd="qpdf $* $_in $OUT/$_outname"
  eval "$_cmd"
  record "$_outname" "qpdf" "$QPDF_V" "$_cmd"
}

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------
echo "=== writing hand-built base PDFs ==="
write_base1
write_base2
record "hand-base1.pdf" "hand (VOLE shell writer, stored-block zlib)" "tools/pdf-corpus.sh" "hand-written classic-xref PDF; object 6 /FlateDecode is a hand-built stored-block zlib stream"
record "hand-base2.pdf" "hand (VOLE shell writer, stored-block zlib)" "tools/pdf-corpus.sh" "hand-written classic-xref PDF; objects 5 and 6 share one plaintext as two hand-built stored-block Flate streams"
ls -l "$OUT"/hand-*.pdf

echo "=== Ghostscript variants ==="
for _s in default prepress printer ebook screen; do
  gs_variant "$_s" "gs-$_s.pdf" "$OUT/hand-base1.pdf"
  echo "  gs-$_s.pdf"
done

echo "=== qpdf variants ==="
qpdf_variant "qpdf-compress.pdf" "$OUT/hand-base1.pdf" "--compress-streams=y" "--object-streams=generate" "--stream-data=compress"
qpdf_variant "qpdf-linearize.pdf" "$OUT/hand-base1.pdf" "--linearize"
qpdf_variant "qpdf-preserve-objectstreams.pdf" "$OUT/hand-base2.pdf" "--object-streams=preserve"
qpdf_variant "qpdf-nocompress.pdf" "$OUT/hand-base1.pdf" "--compress-streams=n" "--stream-data=uncompress"
echo "  qpdf variants written"

echo "=== synthetic samples (if present) ==="
_has_syn=0
for _f in "$SYN"/*.pdf "$SYN"/*.bin; do
  [ -e "$_f" ] || continue
  _has_syn=1
  _n=$(basename "$_f")
  record "_synthetic/$_n" "vole-document pdf-make-samples" "dev image (Phase-3 corpus)" \
    "docker compose run --rm --no-TTY dev ./target/debug/vole-document pdf-make-samples evidence/corpus/phase7/_synthetic"
  echo "  _synthetic/$_n"
done
if [ "$_has_syn" -eq 0 ]; then
  echo "  (none present; run pdf-make-samples into $SYN beforehand to include them)"
fi

# ---------------------------------------------------------------------------
# Validate every produced PDF with the independent qpdf oracle
# ---------------------------------------------------------------------------
echo
echo "=== qpdf --check on every produced PDF ==="
_fail=0
_total=0
_flate_total=0
for _f in "$OUT"/*.pdf "$SYN"/*.pdf; do
  [ -e "$_f" ] || continue
  _total=$((_total + 1))
  _name="${_f#"$OUT"/}"
  if qpdf --check "$_f" > "$TMP/check.txt" 2>&1; then
    _verdict=valid
  else
    _verdict=INVALID
    _fail=1
    echo "FINDING: qpdf --check failed for $_name" >&2
    cat "$TMP/check.txt" >&2
  fi
  # Count FlateDecode markers with qpdf as an independent signal (qpdf decodes
  # to a QDF form and re-emits the filters it sees).
  _flate=0
  if qpdf --qdf --object-streams=disable "$_f" "$TMP/qdf.pdf" 2>/dev/null; then
    _flate=$(grep -c '/FlateDecode' "$TMP/qdf.pdf" 2>/dev/null || true)
    _flate=${_flate:-0}
  fi
  _flate_total=$((_flate_total + _flate))
  printf '%-40s %s flate_markers=%s\n' "$_name" "$_verdict" "$_flate"
done

# ---------------------------------------------------------------------------
# Provenance ledger
# ---------------------------------------------------------------------------
{
  printf '{\n'
  printf '  "generator": "tools/pdf-corpus.sh",\n'
  printf '  "license": "locally-generated",\n'
  printf '  "note": "No third-party document bytes. Every .pdf is produced by the pinned tools image from our own deterministic inputs; the .pdf bytes are gitignored and regenerable from this script.",\n'
  printf '  "tools": {"qpdf": "%s", "ghostscript": "%s"},\n' "$QPDF_V" "$GS_V"
  printf '  "files": [\n'
  awk 'NR>1{printf ",\n"} {printf "    %s", $0}' "$ENTRIES"
  printf '\n  ]\n'
  printf '}\n'
} > "$PROVENANCE"

echo
echo "provenance: $PROVENANCE ($_total PDFs, $_flate_total indexed)"
rm -rf "$TMP"

if [ "$_fail" -ne 0 ]; then
  echo "PDF CORPUS: FAIL" >&2
  exit 1
fi
echo "PDF CORPUS: OK ($_total PDFs, all qpdf --check valid)"
