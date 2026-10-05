#!/bin/sh
# Phase-7.0c generic-compressor baseline ladder.
#
# For every input file this records the source size and the smallest *lossless*
# complete-file size for:
#
#   * generic compressors: gzip -9, zstd -19 --long=27, xz -9e, brotli -q 11;
#   * VOLE lanes: RAW, RLE, BYTE_RANS, every forced structural kind
#     (pdf-physical, pdf-channels, pdf-layout, pdf-layout-rans,
#      pdf-deflate-replay, pdf-deflate-replay-rans) and the unforced auto winner
#     (complete serialized `.voldoc` size).
#
# Honesty rules:
#   * Every generic result is round-trip verified (`<decompress> | cmp` against
#     the source) before it is scored; a failure is recorded as `null`, never
#     silently accepted.
#   * A forced VOLE kind the input does not propose is a typed `Usage` decline,
#     recorded as `null`.
#   * The auto winner is `verify`ed and `decode`d + `cmp`ed byte-exact.
#
# Output: one JSON object to OUT containing the per-file `rows` (the baseline
# table, each row also carrying its ascending candidate `rank`) and a `summary`
# that states, per corpus, on how many files the best VOLE lane beats each
# generic compressor. A compact table is echoed to stdout.
#
# Usage (inside the `baseline` service):
#   sh tools/baselines.sh OUT.json FILE_OR_DIR...
set -eu

cd /work

OUT=$1
shift

BIN=${VOLE_BIN:-./target/debug/vole-document}
CORPUS=${BASELINE_CORPUS:-corpus}

if [ ! -x "$BIN" ]; then
  echo "baseline: $BIN not found; building (cargo build --locked --all-features)" >&2
  cargo build --locked --all-features 1>&2
fi

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
ROWS="$TMP/rows.jsonl"
: > "$ROWS"

# ---------------------------------------------------------------------------
# Generic compressors. Each echoes a byte count, or `null` if the round-trip
# does not reconstruct the source byte-for-byte.
# ---------------------------------------------------------------------------
gzip_size() {
  _in=$1; _z="$TMP/g.gz"; _d="$TMP/g.out"
  if ! gzip -9 -c "$_in" > "$_z" 2>/dev/null; then echo null; return 0; fi
  if gzip -dc "$_z" > "$_d" 2>/dev/null && cmp -s "$_d" "$_in"; then
    wc -c < "$_z" | tr -d ' '
  else
    echo null
  fi
  rm -f "$_z" "$_d"
}

zstd_size() {
  _in=$1; _z="$TMP/z.zst"; _d="$TMP/z.out"
  if ! zstd -19 --long=27 -q -c "$_in" > "$_z" 2>/dev/null; then echo null; return 0; fi
  if zstd -d --long=27 -q -c "$_z" > "$_d" 2>/dev/null && cmp -s "$_d" "$_in"; then
    wc -c < "$_z" | tr -d ' '
  else
    echo null
  fi
  rm -f "$_z" "$_d"
}

xz_size() {
  _in=$1; _z="$TMP/x.xz"; _d="$TMP/x.out"
  if ! xz -9e -c "$_in" > "$_z" 2>/dev/null; then echo null; return 0; fi
  if xz -dc "$_z" > "$_d" 2>/dev/null && cmp -s "$_d" "$_in"; then
    wc -c < "$_z" | tr -d ' '
  else
    echo null
  fi
  rm -f "$_z" "$_d"
}

brotli_size() {
  _in=$1; _z="$TMP/b.br"; _d="$TMP/b.out"
  if ! brotli -q 11 -c "$_in" > "$_z" 2>/dev/null; then echo null; return 0; fi
  if brotli -d -c "$_z" > "$_d" 2>/dev/null && cmp -s "$_d" "$_in"; then
    wc -c < "$_z" | tr -d ' '
  else
    echo null
  fi
  rm -f "$_z" "$_d"
}

# ---------------------------------------------------------------------------
# VOLE lanes.
# ---------------------------------------------------------------------------
extract_len() {
  _v=$(printf '%s' "$1" | sed -n 's/.*"encoded_len":\([0-9][0-9]*\).*/\1/p')
  if [ -n "$_v" ]; then printf '%s' "$_v"; else printf 'null'; fi
}

extract_candidate() {
  printf '%s' "$1" | sed -n 's/.*"candidate":"\([A-Z_]*\)".*/\1/p'
}

# run_force FILE KIND — complete serialized size of a forced lane, or `null`.
run_force() {
  _f=$1; _kind=$2
  _base=$(printf '%s' "$_f" | tr '/' '_')
  _out="$TMP/$_base.$_kind.voldoc"
  rm -f "$_out"
  set +e
  _json=$($BIN encode --force "$_kind" "$_f" "$_out" 2>/dev/null)
  _rc=$?
  set -e
  if [ "$_rc" -eq 0 ]; then extract_len "$_json"; else printf 'null'; fi
  rm -f "$_out"
}

# track the smallest non-null VOLE lane.
_best_val=
_best_name=
consider() {
  _n=$1; _v=$2
  if [ "$_v" = null ]; then return 0; fi
  if [ -z "$_best_val" ] || [ "$_v" -lt "$_best_val" ]; then
    _best_val=$_v
    _best_name=$_n
  fi
}

for _f in "$@"; do
  if [ -d "$_f" ]; then
    # A corpus directory also holds ledgers (provenance.json, deflate-stats.jsonl);
    # the ladder measures the corpus artifacts only.
    _list=$(find "$_f" -type f \( -name '*.pdf' -o -name '*.bin' \) | sort)
  else
    _list=$_f
  fi
  for _file in $_list; do
    _name=$(basename "$_file")
    _dir=$(dirname "$_file")
    _disp="$_name"
    case "$_dir" in
      */_synthetic) _disp="_synthetic/$_name" ;;
    esac
    _src=$(wc -c < "$_file" | tr -d ' ')

    # generic compressors (round-trip verified)
    _gzip=$(gzip_size "$_file")
    _zstd=$(zstd_size "$_file")
    _xz=$(xz_size "$_file")
    _brotli=$(brotli_size "$_file")

    # VOLE lanes
    _auto_out="$TMP/$_name.auto.voldoc"
    rm -f "$_auto_out"
    set +e
    _auto_json=$($BIN encode "$_file" "$_auto_out" 2>/dev/null)
    _auto_rc=$?
    set -e
    if [ "$_auto_rc" -eq 0 ]; then
      _auto_len=$(extract_len "$_auto_json")
      _auto_cand=$(extract_candidate "$_auto_json")
      set +e
      $BIN verify "$_auto_out" >/dev/null 2>&1
      _vrc=$?
      $BIN decode "$_auto_out" "$TMP/$_name.dec" >/dev/null 2>&1
      _drc=$?
      cmp -s "$TMP/$_name.dec" "$_file"
      _crc=$?
      set -e
      if [ "$_vrc" -eq 0 ]; then _verify=true; else _verify=false; fi
      if [ "$_drc" -eq 0 ] && [ "$_crc" -eq 0 ]; then _rt=true; else _rt=false; fi
    else
      _auto_len=null; _auto_cand=''; _verify=false; _rt=false
    fi
    rm -f "$_auto_out" "$TMP/$_name.dec"

    _raw=$(run_force "$_file" raw)
    _rle=$(run_force "$_file" rle)
    _byte_rans=$(run_force "$_file" byte-rans)
    _phys=$(run_force "$_file" pdf-physical)
    _chans=$(run_force "$_file" pdf-channels)
    _layout=$(run_force "$_file" pdf-layout)
    _layoutrans=$(run_force "$_file" pdf-layout-rans)
    _deflate=$(run_force "$_file" pdf-deflate-replay)
    _deflaterans=$(run_force "$_file" pdf-deflate-replay-rans)

    _best_val=; _best_name=
    consider RAW "$_raw"
    consider RLE "$_rle"
    consider BYTE_RANS "$_byte_rans"
    consider PDF_PHYSICAL "$_phys"
    consider PDF_CHANNELS "$_chans"
    consider PDF_LAYOUT "$_layout"
    consider PDF_LAYOUT_RANS "$_layoutrans"
    consider PDF_DEFLATE_REPLAY "$_deflate"
    consider PDF_DEFLATE_REPLAY_RANS "$_deflaterans"
    consider AUTO "$_auto_len"
    if [ -z "$_best_val" ]; then _best_val=null; _best_name=null; fi

    printf '{"corpus":"%s","file":"%s","source_len":%s,"gzip9":%s,"zstd19":%s,"xz9e":%s,"brotli11":%s,"raw":%s,"rle":%s,"byte_rans":%s,"pdf_physical":%s,"pdf_channels":%s,"pdf_layout":%s,"pdf_layout_rans":%s,"deflate_replay":%s,"deflate_replay_rans":%s,"auto_candidate":"%s","auto_len":%s,"best_vole":%s,"best_vole_lane":"%s","verify_ok":%s,"roundtrip_ok":%s}\n' \
      "$CORPUS" "$_disp" "$_src" "$_gzip" "$_zstd" "$_xz" "$_brotli" \
      "$_raw" "$_rle" "$_byte_rans" "$_phys" "$_chans" "$_layout" "$_layoutrans" \
      "$_deflate" "$_deflaterans" "$_auto_cand" "$_auto_len" "$_best_val" "$_best_name" \
      "$_verify" "$_rt" >> "$ROWS"

    printf '%-40s src=%-8s gzip=%-8s zstd=%-8s xz=%-8s brotli=%-8s byte_rans=%-8s best_vole=%-8s(%s)\n' \
      "$_disp" "$_src" "$_gzip" "$_zstd" "$_xz" "$_brotli" "$_byte_rans" "$_best_val" "$_best_name"
  done
done

jq -s -f "$(dirname "$0")/baselines.jq" "$ROWS" > "$OUT"
echo "baseline ladder written to $OUT"
