#!/bin/sh
# Phase-9.3 cohort with deliberate cross-document sharing.
#
# Runs entirely inside the pinned `tools` image (qpdf 11.3.0, Ghostscript
# 10.00.0; no python, no rust). Every byte is locally generated from our own
# deterministic inputs; no third-party document bytes are used.
#
# Strata (each a directory under evidence/corpus/phase9/):
#   p7/            copies of the byte-reproducible Phase-7 producer corpus subset
#   repeat-bin/    N byte-identical copies of one opaque binary (RAW-winner)
#   repeat-pdf/    N byte-identical copies of one PDF (channels-winner)
#   shared-payload/N PDFs embedding the SAME stream payload, distinct tags
#   shared-bin/    N binaries sharing one payload, distinct 8-byte prefixes
#   dup-resource/  one document duplicating a resource internally (hand-base2)
#   reexport/      qpdf --deterministic-id re-exports (same content, re-serialized)
#   one-changed/   base vs one changed byte inside the single stream object
#   incremental/   rev-0..rev-K appending objects + xref + /Prev
#   shifted/       base vs one byte inserted early in the stream (CDC control)
#
# Determinism: inputs are hand-built or produced by pinned tools with fixed
# flags (`qpdf --deterministic-id`); perturbations are pure functions of bytes;
# no timestamps and no RNG. Ghostscript outputs are excluded (per-run /ID).
#
# The regenerable bytes are gitignored (.gitignore: evidence/corpus/**/*.pdf,
# *.bin). cohort.json is the committed ledger.
#
# Prerequisite: evidence/corpus/phase7 must exist (run `tools/pdf-corpus.sh`).
#
# Usage: docker compose run --rm --no-TTY tools sh tools/store-cohort.sh
set -eu

cd /work
LC_ALL=C
export LC_ALL

OUT=evidence/corpus/phase9
P7=evidence/corpus/phase7
TMP="$OUT/.tmp"
mkdir -p "$OUT" "$TMP"

QPDF_V="$(qpdf --version | awk '{print $3; exit}')"
echo "tools: qpdf=$QPDF_V"

if [ ! -f "$P7/hand-base1.pdf" ]; then
  echo "store-cohort: missing $P7/hand-base1.pdf; run tools/pdf-corpus.sh first" >&2
  exit 1
fi

LEDGER="$TMP/cohort-entries.jsonl"
: > "$LEDGER"

sha_of() { sha256sum "$1" | cut -d' ' -f1; }

# record NAME STRATUM PROVENANCE
record() {
  _name=$1; _stratum=$2; _prov=$3
  _sha="$(sha_of "$OUT/$_name")"
  _bytes=$(wc -c < "$OUT/$_name" | tr -d ' ')
  printf '{"name":"%s","stratum":"%s","sha256":"%s","bytes":%s,"provenance":"%s"}\n' \
    "$_name" "$_stratum" "$_sha" "$_bytes" "$_prov" >> "$LEDGER"
}

# ---------------------------------------------------------------------------
# Deterministic payloads
# ---------------------------------------------------------------------------
# gen_text LINES OUT — text-like page content.
gen_text() {
  _lines=$1; _out=$2
  : > "$_out"
  printf 'BT\n/F1 11 Tf\n72 740 Td\n14 TL\n' >> "$_out"
  _i=1
  while [ "$_i" -le "$_lines" ]; do
    if [ $((_i % 3)) -eq 0 ]; then
      printf '(Line %s: repeated text shared across pages, amount %s.00) Tj T*\n' "$_i" "$((_i * 7 % 1000))" >> "$_out"
    else
      printf '(Line %s: the quick brown fox jumps over the lazy dog) Tj T*\n' "$_i" >> "$_out"
    fi
    _i=$((_i + 1))
  done
  printf 'ET\n' >> "$_out"
}

# gen_bytes SEED LEN OUT — deterministic pseudo-random bytes (MINSTD, exact in
# double precision so mawk and gawk agree), written as raw bytes in LC_ALL=C.
gen_bytes() {
  _seed=$1; _len=$2; _out=$3
  awk -v seed="$_seed" -v len="$_len" 'BEGIN{
    state = 1
    for (i = 1; i <= length(seed); i++) state = (state * 131 + index("abcdefghijklmnopqrstuvwxyz0123456789", substr(seed, i, 1))) % 2147483647
    if (state <= 0) state = 1
    for (n = 0; n < len; n++) { state = (state * 16807) % 2147483647; printf "%c", state % 256 }
  }' > "$_out"
}

zlib_store() { # IN OUT — RFC 1950 zlib stream of RFC 1951 stored blocks.
  _in=$1; _out=$2
  : > "$_out"
  od -An -v -tu1 "$_in" | awk '
    { for (i = 1; i <= NF; i++) b[++n] = $i + 0 }
    function emit(v) { line = line sprintf("\\%03o", v); if (length(line) >= 256) { print line; line = "" } }
    function block(fin, len, start,   k, lo, hi, nl) {
      lo = len % 256; hi = int(len / 256); nl = 65535 - len
      emit(fin ? 1 : 0); emit(lo); emit(hi); emit(nl % 256); emit(int(nl / 256) % 256)
      for (k = 0; k < len; k++) emit(b[start + k + 1])
    }
    END {
      emit(120); emit(1)
      a = 1; bb = 0
      for (i = 1; i <= n; i++) { a = (a + b[i]) % 65521; bb = (bb + a) % 65521 }
      adler = bb * 65536 + a
      off = 0
      if (n == 0) { block(1, 0, 0) }
      else { while (off < n) { len = n - off; if (len > 65535) len = 65535; block((off + len >= n) ? 1 : 0, len, off); off += len } }
      emit(int(adler / 16777216) % 256); emit(int(adler / 65536) % 256); emit(int(adler / 256) % 256); emit(adler % 256)
      if (length(line) > 0) print line
    }
  ' | while IFS= read -r _line; do printf '%b' "$_line"; done >> "$_out"
}

# ---------------------------------------------------------------------------
# PDF writers (classic xref, exact offsets and /Length)
# ---------------------------------------------------------------------------
# pdf_one_stream OUT TAG STREAM_A
pdf_one_stream() {
  _f=$1; _tag=$2; _a=$3
  _list="$TMP/ol.$_tag"; : > "$_list"
  printf '%%PDF-1.5\n%% %s\n' "$_tag" > "$_f"
  printf '1 %s\n' "$(wc -c < "$_f" | tr -d ' ')" >> "$_list"
  printf '1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n' >> "$_f"
  printf '2 %s\n' "$(wc -c < "$_f" | tr -d ' ')" >> "$_list"
  printf '2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n' >> "$_f"
  printf '3 %s\n' "$(wc -c < "$_f" | tr -d ' ')" >> "$_list"
  printf '3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>\nendobj\n' >> "$_f"
  printf '4 %s\n' "$(wc -c < "$_f" | tr -d ' ')" >> "$_list"
  printf '4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n' >> "$_f"
  _len=$(wc -c < "$_a" | tr -d ' ')
  printf '5 %s\n' "$(wc -c < "$_f" | tr -d ' ')" >> "$_list"
  printf '5 0 obj\n<< /Length %s /Filter /FlateDecode >>\nstream\n' "$_len" >> "$_f"
  cat "$_a" >> "$_f"
  printf '\nendstream\nendobj\n' >> "$_f"
  _xref=$(wc -c < "$_f" | tr -d ' ')
  printf 'xref\n0 6\n0000000000 65535 f \n' >> "$_f"
  _n=1
  while [ "$_n" -le 5 ]; do
    _o=$(awk -v k="$_n" '$1 == k { print $2 }' "$_list")
    printf '%010d 00000 n \n' "$_o" >> "$_f"
    _n=$((_n + 1))
  done
  printf 'trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n%s\n%%%%EOF\n' "$_xref" >> "$_f"
}

# pdf_two_streams OUT TAG STREAM_A STREAM_B — objects 5 and 6 share geometry.
pdf_two_streams() {
  _f=$1; _tag=$2; _a=$3; _b=$4
  _list="$TMP/ol.$_tag"; : > "$_list"
  printf '%%PDF-1.5\n%% %s\n' "$_tag" > "$_f"
  printf '1 %s\n' "$(wc -c < "$_f" | tr -d ' ')" >> "$_list"
  printf '1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n' >> "$_f"
  printf '2 %s\n' "$(wc -c < "$_f" | tr -d ' ')" >> "$_list"
  printf '2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n' >> "$_f"
  printf '3 %s\n' "$(wc -c < "$_f" | tr -d ' ')" >> "$_list"
  printf '3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents [5 0 R 6 0 R] >>\nendobj\n' >> "$_f"
  printf '4 %s\n' "$(wc -c < "$_f" | tr -d ' ')" >> "$_list"
  printf '4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n' >> "$_f"
  _la=$(wc -c < "$_a" | tr -d ' ')
  printf '5 %s\n' "$(wc -c < "$_f" | tr -d ' ')" >> "$_list"
  printf '5 0 obj\n<< /Length %s /Filter /FlateDecode >>\nstream\n' "$_la" >> "$_f"
  cat "$_a" >> "$_f"; printf '\nendstream\nendobj\n' >> "$_f"
  _lb=$(wc -c < "$_b" | tr -d ' ')
  printf '6 %s\n' "$(wc -c < "$_f" | tr -d ' ')" >> "$_list"
  printf '6 0 obj\n<< /Length %s /Filter /FlateDecode >>\nstream\n' "$_lb" >> "$_f"
  cat "$_b" >> "$_f"; printf '\nendstream\nendobj\n' >> "$_f"
  _xref=$(wc -c < "$_f" | tr -d ' ')
  printf 'xref\n0 7\n0000000000 65535 f \n' >> "$_f"
  _n=1
  while [ "$_n" -le 6 ]; do
    _o=$(awk -v k="$_n" '$1 == k { print $2 }' "$_list")
    printf '%010d 00000 n \n' "$_o" >> "$_f"
    _n=$((_n + 1))
  done
  printf 'trailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n%s\n%%%%EOF\n' "$_xref" >> "$_f"
}

# pdf_incremental DIR K — rev-0 (one stream) plus K appended revisions, each
# adding one stream object + xref section + trailer /Prev.
pdf_incremental() {
  _dir=$1; _k=$2
  mkdir -p "$_dir"
  gen_text 400 "$TMP/inc-content.txt"
  zlib_store "$TMP/inc-content.txt" "$TMP/inc-0.z"
  pdf_one_stream "$_dir/rev-0.pdf" "rev-0" "$TMP/inc-0.z"
  _prev_xref=$(sed -n '/^startxref$/{n;p;}' "$_dir/rev-0.pdf")
  _n=1
  while [ "$_n" -le "$_k" ]; do
    _from=$((_n - 1))
    cp "$_dir/rev-$_from.pdf" "$_dir/rev-$_n.pdf"
    _objnum=$((5 + _n))
    gen_text $((400 + _n * 7)) "$TMP/inc-content-$_n.txt"
    zlib_store "$TMP/inc-content-$_n.txt" "$TMP/inc-$_n.z"
    _len=$(wc -c < "$TMP/inc-$_n.z" | tr -d ' ')
    _off=$(wc -c < "$_dir/rev-$_n.pdf" | tr -d ' ')
    printf '%s 0 obj\n<< /Length %s /Filter /FlateDecode >>\nstream\n' "$_objnum" "$_len" >> "$_dir/rev-$_n.pdf"
    cat "$TMP/inc-$_n.z" >> "$_dir/rev-$_n.pdf"
    printf '\nendstream\nendobj\n' >> "$_dir/rev-$_n.pdf"
    _xref=$(wc -c < "$_dir/rev-$_n.pdf" | tr -d ' ')
    printf 'xref\n%s 1\n%010d 00000 n \ntrailer\n<< /Size %s /Root 1 0 R /Prev %s >>\nstartxref\n%s\n%%%%EOF\n' \
      "$_objnum" "$_off" "$((_objnum + 1))" "$_prev_xref" "$_xref" >> "$_dir/rev-$_n.pdf"
    _prev_xref=$_xref
    _n=$((_n + 1))
  done
}

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------
echo "=== payloads ==="
gen_text 4000 "$TMP/payload.text"          # ~ 200 KiB text
gen_bytes phase9 131072 "$TMP/payload.rand"

echo "=== p7/ (Phase-7 producer subset, byte-reproducible) ==="
mkdir -p "$OUT/p7"
for _f in hand-base1 hand-base2 qpdf-compress qpdf-linearize qpdf-nocompress qpdf-preserve-objectstreams; do
  cp "$P7/$_f.pdf" "$OUT/p7/$_f.pdf"
  record "p7/$_f.pdf" "p7" "tools/pdf-corpus.sh (copied); qpdf=$QPDF_V"
done

echo "=== repeat-bin/ ==="
mkdir -p "$OUT/repeat-bin"
_i=0
while [ "$_i" -lt 4 ]; do
  cp "$TMP/payload.rand" "$OUT/repeat-bin/repeat-$_i.bin"
  record "repeat-bin/repeat-$_i.bin" "repeat-bin" "cp of gen_bytes(phase9,131072)"
  _i=$((_i + 1))
done

echo "=== repeat-pdf/ ==="
mkdir -p "$OUT/repeat-pdf"
_i=0
while [ "$_i" -lt 4 ]; do
  cp "$OUT/p7/hand-base1.pdf" "$OUT/repeat-pdf/repeat-$_i.pdf"
  record "repeat-pdf/repeat-$_i.pdf" "repeat-pdf" "cp of p7/hand-base1.pdf"
  _i=$((_i + 1))
done

echo "=== shared-payload/ ==="
mkdir -p "$OUT/shared-payload"
zlib_store "$TMP/payload.text" "$TMP/payload.text.z"
_i=0
while [ "$_i" -lt 5 ]; do
  pdf_one_stream "$OUT/shared-payload/shared-$_i.pdf" "shared-tag-$_i" "$TMP/payload.text.z"
  record "shared-payload/shared-$_i.pdf" "shared-payload" "tools/store-cohort.sh: pdf_one_stream(tag=shared-tag-$_i, zlib_store(payload.text))"
  _i=$((_i + 1))
done

echo "=== shared-bin/ ==="
mkdir -p "$OUT/shared-bin"
_i=0
while [ "$_i" -lt 5 ]; do
  { printf 'tag%05d' "$_i"; cat "$TMP/payload.rand"; } > "$OUT/shared-bin/shared-$_i.bin"
  record "shared-bin/shared-$_i.bin" "shared-bin" "tools/store-cohort.sh: 8-byte tag || gen_bytes(phase9,131072)"
  _i=$((_i + 1))
done

echo "=== dup-resource/ ==="
mkdir -p "$OUT/dup-resource"
cp "$OUT/p7/hand-base2.pdf" "$OUT/dup-resource/dup-resource.pdf"
record "dup-resource/dup-resource.pdf" "dup-resource" "cp of p7/hand-base2.pdf (two streams share one plaintext)"

echo "=== reexport/ ==="
mkdir -p "$OUT/reexport"
qpdf --deterministic-id --compress-streams=n --stream-data=uncompress "$OUT/p7/hand-base1.pdf" "$OUT/reexport/reexport-nocompress.pdf"
qpdf --deterministic-id --object-streams=generate --compress-streams=y --stream-data=compress "$OUT/p7/hand-base1.pdf" "$OUT/reexport/reexport-objectstreams.pdf"
qpdf --deterministic-id --linearize "$OUT/p7/hand-base1.pdf" "$OUT/reexport/reexport-linearize.pdf"
for _f in reexport-nocompress reexport-objectstreams reexport-linearize; do
  record "reexport/$_f.pdf" "reexport" "qpdf --deterministic-id (qpdf=$QPDF_V) of p7/hand-base1.pdf"
done

echo "=== one-changed/ ==="
mkdir -p "$OUT/one-changed"
gen_text 4000 "$TMP/one-base.txt"
cp "$TMP/one-base.txt" "$TMP/one-changed.txt"
# Flip exactly one byte at offset 512 (same length; /Length and offsets unchanged).
printf 'Q' | dd of="$TMP/one-changed.txt" bs=1 seek=512 count=1 conv=notrunc 2>/dev/null
zlib_store "$TMP/one-base.txt" "$TMP/one-base.z"
zlib_store "$TMP/one-changed.txt" "$TMP/one-changed.z"
pdf_one_stream "$OUT/one-changed/one-base.pdf" "one-base" "$TMP/one-base.z"
pdf_one_stream "$OUT/one-changed/one-changed.pdf" "one-changed" "$TMP/one-changed.z"
record "one-changed/one-base.pdf" "one-changed" "tools/store-cohort.sh: pdf_one_stream(one-base)"
record "one-changed/one-changed.pdf" "one-changed" "tools/store-cohort.sh: pdf_one_stream(one-changed, one byte flipped at payload offset 512)"

echo "=== incremental/ ==="
pdf_incremental "$OUT/incremental" 4
_i=0
while [ "$_i" -le 4 ]; do
  record "incremental/rev-$_i.pdf" "incremental" "tools/store-cohort.sh: pdf_incremental(4) — appended object + xref + trailer /Prev per revision"
  _i=$((_i + 1))
done

echo "=== shifted/ ==="
mkdir -p "$OUT/shifted"
gen_text 4000 "$TMP/shift-base.txt"
( head -c 512 "$TMP/shift-base.txt"; printf 'Z'; tail -c +513 "$TMP/shift-base.txt" ) > "$TMP/shift-one.txt"
zlib_store "$TMP/shift-base.txt" "$TMP/shift-base.z"
zlib_store "$TMP/shift-one.txt" "$TMP/shift-one.z"
pdf_one_stream "$OUT/shifted/shift-base.pdf" "shift-base" "$TMP/shift-base.z"
pdf_one_stream "$OUT/shifted/shift-one.pdf" "shift-one" "$TMP/shift-one.z"
record "shifted/shift-base.pdf" "shifted" "tools/store-cohort.sh: pdf_one_stream(shift-base)"
record "shifted/shift-one.pdf" "shifted" "tools/store-cohort.sh: pdf_one_stream(shift-one, one byte inserted at payload offset 512)"

# ---------------------------------------------------------------------------
# Ledger
# ---------------------------------------------------------------------------
{
  printf '{\n'
  printf '  "cohort": "phase9",\n'
  printf '  "generator": "tools/store-cohort.sh",\n'
  printf '  "license": "locally-generated",\n'
  printf '  "note": "No third-party document bytes. Every file is generated by the pinned tools image from our own deterministic inputs; the bytes are gitignored and regenerable from this script.",\n'
  printf '  "tools": {"qpdf": "%s"},\n' "$QPDF_V"
  printf '  "prerequisite": "tools/pdf-corpus.sh (evidence/corpus/phase7 subset copied into p7/)",\n'
  printf '  "files": [\n'
  awk 'NR>1{printf ",\n"} {printf "    %s", $0}' "$LEDGER"
  printf '\n  ]\n'
  printf '}\n'
} > "$OUT/cohort.json"

rm -rf "$TMP"
echo "cohort ledger: $OUT/cohort.json"
find "$OUT" -type f \( -name '*.pdf' -o -name '*.bin' \) | wc -l | xargs echo "cohort files:"
