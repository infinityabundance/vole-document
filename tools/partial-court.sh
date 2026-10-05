#!/bin/sh
# Phase-7.3 partial-materialization query court.
#
# The decisive measurement of the partial-materialization pivot: random-access /
# query cost, not whole-file size. For a pre-registered query set over output
# positions in the source, it measures:
#
#   * VOLE `view`: the served bytes plus every `ObservationStats` field
#     (ops/channels/objects touched, entropy bytes decoded, descriptor bytes
#     traversed, work amplification) and wall/CPU/peak-RSS via `/usr/bin/time -v`;
#   * gzip -9 / zstd -19 --long=27 / xz -9e: the honest cost to reach the same
#     output range by sequential decompression, i.e. the decompressed bytes that
#     must be inflated, the compressed bytes that must be read (counted by `pv`),
#     and wall/CPU/peak-RSS.
#
# Honesty rules:
#   * Every VOLE slice is byte-compared against the source (`cmp`), so a partial
#     result that differs from the full materialization slice is a failure.
#   * The sequential decoders are timed by `/usr/bin/time -v` directly (a FIFO
#     feeds a consumer that closes after the target offset), so the reported
#     CPU/RSS are the decoder's, not a shell's.
#   * `compressed_bytes_read` is measured at the pipe, so it carries a bounded
#     read-ahead (one pipe buffer + the decoder's internal buffer); it is a
#     close upper bound, not an exact seek figure.
#
# Usage (inside the `baseline` service):
#   sh tools/partial-court.sh OUT.jsonl SOURCE.pdf FILE.voldoc GZ ZST XZ
set -eu

cd /work

OUT=$1
SOURCE=$2
VOLDOC=$3
GZ=$4
ZST=$5
XZ=$6

BIN=${VOLE_BIN:-./target/debug/vole-document}
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
: > "$OUT"

# Extract a `/usr/bin/time -v` field value by label substring.
field() {
  grep -F "$2" "$1" | head -1 | sed 's/.*: //'
}

# One codec sub-object: decompressed bytes processed (n), compressed bytes read
# (pv), CPU/wall seconds and peak RSS.
codec_json() {
  _t=$1; _p=$2; _n=$3
  _u=$(field "$_t" "User time")
  _s=$(field "$_t" "System time")
  _w=$(field "$_t" "Elapsed")
  _m=$(field "$_t" "Maximum resident")
  _c=$(cat "$_p" 2>/dev/null || echo 0)
  printf '{"decompressed_bytes":%s,"compressed_bytes_read":%s,"user_s":%s,"sys_s":%s,"wall":"%s","max_rss_kb":%s}' \
    "$_n" "${_c:-0}" "${_u:-0}" "${_s:-0}" "${_w:-0:00.00}" "${_m:-0}"
}

# measure NAME DECODER_CMD COMPRESSED_FILE N — time the decoder directly and
# count the compressed bytes it reads. DECODER_CMD is word-split on purpose.
measure() {
  _name=$1; _dec=$2; _comp=$3; _n=$4
  rm -f "$TMP/o.fifo"
  mkfifo "$TMP/o.fifo"
  ( head -c "$_n" < "$TMP/o.fifo" > /dev/null ) &
  _hp=$!
  # shellcheck disable=SC2086
  timeout 600 sh -c "pv -b -n '$_comp' 2>'$TMP/pv.$_name' | /usr/bin/time -v -o '$TMP/time.$_name' $_dec > '$TMP/o.fifo'" || true
  wait "$_hp" 2>/dev/null || true
}

# One full query record. $1 = query id, $2 = selector flag value, $3 = a byte
# range "A:LEN" (its resolved range is read back from the VOLE stats).
run_query() {
  _id=$1
  _selector=$2
  _bytes=$3

  rm -f "$TMP/obs.bin" "$TMP/vtime.txt"
  /usr/bin/time -v -o "$TMP/vtime.txt" \
    "$BIN" view "$VOLDOC" "$TMP/obs.bin" "$_selector" --stats > "$TMP/stats.json" 2>/dev/null

  _a=$(sed -n 's/.*"range_start":\([0-9]*\).*/\1/p' "$TMP/stats.json")
  _len=$(sed -n 's/.*"range_len":\([0-9]*\).*/\1/p' "$TMP/stats.json")
  _n=$((_a + _len))

  # Correctness: the served slice must equal the source slice.
  tail -c "+$((_a + 1))" "$SOURCE" | head -c "$_len" > "$TMP/exp.bin"
  if cmp -s "$TMP/exp.bin" "$TMP/obs.bin"; then _ok=true; else _ok=false; fi

  # Sequential baselines must produce the same prefix count.
  measure gzip "gzip -dc" "$GZ" "$_n"
  measure zstd "zstd -d --long=27 -q -c" "$ZST" "$_n"
  measure xz "xz -dc" "$XZ" "$_n"

  _vstats=$(cat "$TMP/stats.json")
  _vu=$(field "$TMP/vtime.txt" "User time")
  _vs=$(field "$TMP/vtime.txt" "System time")
  _vw=$(field "$TMP/vtime.txt" "Elapsed")
  _vm=$(field "$TMP/vtime.txt" "Maximum resident")

  printf '{"query":"%s","selector":"%s","byte_range":"%s","range_start":%s,"range_len":%s,"correct":%s,"voldoc":%s,"vole_time":{"user_s":%s,"sys_s":%s,"wall":"%s","max_rss_kb":%s},"gzip":%s,"zstd":%s,"xz":%s}\n' \
    "$_id" "$_selector" "$_bytes" "$_a" "$_len" "$_ok" "$_vstats" \
    "$_vu" "$_vs" "$_vw" "$_vm" \
    "$(codec_json "$TMP/time.gzip" "$TMP/pv.gzip" "$_n")" \
    "$(codec_json "$TMP/time.zstd" "$TMP/pv.zstd" "$_n")" \
    "$(codec_json "$TMP/time.xz" "$TMP/pv.xz" "$_n")" >> "$OUT"

  printf '  %-26s a=%-10s len=%-9s vole_ok=%s\n' "$_id" "$_a" "$_len" "$_ok" >&2
}

# --- Pre-registered query set (frozen before measuring) ---------------------
# Byte ranges: 256 B at {0, 1, 8, 16, 24, 31} MiB.
for off in 0 1048576 8388608 16777216 25165824 32505856; do
  run_query "byte-$off" "--byte-range=$off:256" "$off:256"
done

# PDF queries: 8 encoded-stream spans and 4 indirect-object spans, distributed
# across the 800 streams (content object N = 5 + 2*i, page object N = 4 + 2*i).
for i in 40 144 256 360 464 560 664 760; do
  obj=$((5 + 2 * i))
  run_query "stream-$obj" "--pdf-stream=$obj:0" "resolved-by-selector"
done
for i in 40 256 464 664; do
  obj=$((4 + 2 * i))
  run_query "object-$obj" "--pdf-object=$obj:0" "resolved-by-selector"
done

echo "partial query court written to $OUT" >&2
