#!/bin/sh
# Phase-8 seek-based partial I/O court.
#
# The decisive measurement of Phase 8: does a seeked `view` become a
# **bytes-read** win (not just a decode-CPU win) by reading only the records a
# query needs, instead of `fs::read`ing the whole descriptor?
#
# For the frozen 18-query set (6 byte-ranges at 0/1/8/16/24/31 MiB + 8
# `--pdf-stream` + 4 `--pdf-object`) this records, per query:
#
#   * VOLE `view` (seek path): the instrumented `bytes_read` from
#     ObservationStats (a CountingReader inside the reader), its syscall-level
#     cross-check (`strace -e trace=read,pread64 -P <descriptor>`: bytes returned
#     and read-call count attributed to the descriptor *file*, never the loader or
#     a pipe), wall/CPU/peak-RSS via `/usr/bin/time -v`, and the Phase-7 usage
#     stats (ops_evaluated/ops_total, channels_decoded, objects_fetched,
#     descriptor_bytes_traversed, work_amplification);
#   * gzip -9 / zstd -19 --long=27 / xz -9e: the honest cost to reach the same
#     output offset by sequential decompression — the **decompressed bytes
#     processed** (`a + len`) and the **compressed bytes that must be read**
#     (counted by `pv` at the pipe), with wall/CPU/peak-RSS.
#
# Honesty rules:
#   * Every VOLE slice is byte-compared (`cmp`) against the source slice.
#   * `bytes_read` (instrumented) is the primary metric; the `strace` figure is a
#     cross-check on the same file and may differ by the tiny header peek the CLI
#     performs before choosing the seek path. Both are reported.
#   * The sequential decoders are timed directly (a FIFO feeds a consumer that
#     closes after the target offset), so CPU/RSS are the decoder's.
#   * `compressed_bytes_read` is measured at the pipe, so it carries a bounded
#     read-ahead (one pipe buffer + the decoder's internal buffer); it is a close
#     upper bound, not an exact seek figure.
#
# Usage (inside the `baseline` service):
#   sh tools/seek-court.sh OUT.jsonl SOURCE.pdf FILE.voldoc GZ ZST XZ
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

# Trace only the read/pread64 syscalls that touch the descriptor file and report
# "BYTES CALLS": bytes returned and the number of read calls attributed to it.
strace_probe() {
  _sel=$1
  rm -f "$TMP/strace.txt"
  strace -f -e trace=read,pread64 -P "$VOLDOC" -o "$TMP/strace.txt" \
    "$BIN" view "$VOLDOC" /dev/null "$_sel" --stats >/dev/null 2>&1 || true
  awk '/ = / { n=split($0,a," = "); v=a[n]+0; if (v>0) { b+=v; c++ } }
       END { printf "%d %d", b+0, c+0 }' "$TMP/strace.txt"
}

# One full query record. $1 = query id, $2 = selector flag value, $3 = a byte
# range "A:L" (its resolved range is read back from the VOLE stats).
run_query() {
  _id=$1
  _selector=$2
  _bytes=$3

  rm -f "$TMP/obs.bin" "$TMP/vtime.txt" "$TMP/stats.json"
  /usr/bin/time -v -o "$TMP/vtime.txt" \
    "$BIN" view "$VOLDOC" "$TMP/obs.bin" "$_selector" --stats > "$TMP/stats.json" 2>/dev/null

  _a=$(sed -n 's/.*"range_start":\([0-9]*\).*/\1/p' "$TMP/stats.json")
  _len=$(sed -n 's/.*"range_len":\([0-9]*\).*/\1/p' "$TMP/stats.json")
  _n=$((_a + _len))

  # Correctness: the served slice must equal the source slice.
  tail -c "+$((_a + 1))" "$SOURCE" | head -c "$_len" > "$TMP/exp.bin"
  if cmp -s "$TMP/exp.bin" "$TMP/obs.bin"; then _ok=true; else _ok=false; fi

  # Syscall-level cross-check, attributed to the descriptor file only.
  _st=$(strace_probe "$_selector")
  _sb=${_st%% *}
  _sc=${_st##* }

  # Sequential baselines must produce the same prefix count.
  measure gzip "gzip -dc" "$GZ" "$_n"
  measure zstd "zstd -d --long=27 -q -c" "$ZST" "$_n"
  measure xz "xz -dc" "$XZ" "$_n"

  _vstats=$(cat "$TMP/stats.json")
  _vu=$(field "$TMP/vtime.txt" "User time")
  _vs=$(field "$TMP/vtime.txt" "System time")
  _vw=$(field "$TMP/vtime.txt" "Elapsed")
  _vm=$(field "$TMP/vtime.txt" "Maximum resident")

  printf '{"query":"%s","selector":"%s","byte_range":"%s","range_start":%s,"range_len":%s,"correct":%s,"voldoc":%s,"strace":{"bytes_read":%s,"read_calls":%s},"vole_time":{"user_s":%s,"sys_s":%s,"wall":"%s","max_rss_kb":%s},"gzip":%s,"zstd":%s,"xz":%s}\n' \
    "$_id" "$_selector" "$_bytes" "$_a" "$_len" "$_ok" "$_vstats" \
    "${_sb:-0}" "${_sc:-0}" \
    "$_vu" "$_vs" "$_vw" "$_vm" \
    "$(codec_json "$TMP/time.gzip" "$TMP/pv.gzip" "$_n")" \
    "$(codec_json "$TMP/time.zstd" "$TMP/pv.zstd" "$_n")" \
    "$(codec_json "$TMP/time.xz" "$TMP/pv.xz" "$_n")" >> "$OUT"

  printf '  %-26s a=%-10s len=%-9s vole_ok=%s bytes_read=%s strace_b=%s strace_calls=%s\n' \
    "$_id" "$_a" "$_len" "$_ok" \
    "$(sed -n 's/.*"bytes_read":\([0-9]*\).*/\1/p' "$TMP/stats.json")" "${_sb:-0}" "${_sc:-0}" >&2
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

echo "seek query court written to $OUT" >&2
