#!/bin/sh
# Phase-8.4 seekable/blocked random-access baseline court.
#
# The honest random-access comparison the Phase-8.3 bytes-read court was missing:
# against **seekable/blocked archive formats**, not against non-seekable
# sequential gzip/zstd/xz prefixes. For a pre-registered byte range it measures,
# for each format, the bytes a random-access reader must actually read to serve
# the range and the time to decode it, and byte-compares the extracted region
# against the source.
#
# Formats (built here by the pinned `baseline` image):
#   * bgzip  -l 9  (BGZF blocks) + its `.gzi` index;
#   * xz --block-size=64KiB | 1MiB | 4MiB (independently decodable blocks) +
#     the xz Stream Footer/Index;
#   * pixz (indexed parallel xz, 16 MiB blocks) read through its xz Index/Footer.
#
# Cost definition (bytes read): the covering compressed block(s) plus whatever
# index/framing locates them (the `.gzi` file for BGZF; the xz Stream Footer +
# Index field for xz/pixz). This is a *seek* cost, never a sequential prefix.
#
# Honesty rules: every extracted slice is `cmp`-identical to the source; xz/pixz
# blocks are decoded *in isolation* (via `tools/xz-block-reframe.pl`) so block
# independence is demonstrated, not assumed; BGZF blocks are decoded as lone
# gzip members. The VOLE row is the instrumented `bytes_read` of the seeked
# `view` on the same range.
#
# Usage (inside the `baseline` service):
#   sh tools/seekable-baselines.sh OUT.jsonl SOURCE.pdf FILE.seek.voldoc WORKDIR
set -eu

cd /work

OUT=$1
SOURCE=$2
VOLDOC=$3
WORK=$4

BIN=${VOLE_BIN:-./target/debug/vole-document}
mkdir -p "$WORK"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
: > "$OUT"

# --- Build the seekable/blocked archives ------------------------------------
BGZ=$WORK/large.pdf.bgz
GZI=$WORK/large.pdf.bgz.gzi
XZ64=$WORK/large.pdf.64KiB.xz
XZ1M=$WORK/large.pdf.1MiB.xz
XZ4M=$WORK/large.pdf.4MiB.xz
PXZ=$WORK/large.pdf.pxz

echo "building seekable baselines in $WORK ..." >&2
bgzip -l 9 -c -i -I "$GZI" "$SOURCE" > "$BGZ"
xz --block-size=64KiB -c "$SOURCE" > "$XZ64"
xz --block-size=1MiB  -c "$SOURCE" > "$XZ1M"
xz --block-size=4MiB  -c "$SOURCE" > "$XZ4M"
pixz -t -i "$SOURCE" -o "$PXZ"

ARCH="\"bgzip\":$(wc -c < "$BGZ"),\"xz64k\":$(wc -c < "$XZ64"),\"xz1m\":$(wc -c < "$XZ1M"),\"xz4m\":$(wc -c < "$XZ4M"),\"pixz\":$(wc -c < "$PXZ"),\"voldoc\":$(wc -c < "$VOLDOC"),\"source\":$(wc -c < "$SOURCE")"

tfield() { grep -F "$2" "$1" 2>/dev/null | head -1 | sed 's/.*: //' || true; }

# run_timed NAME CMD... : run CMD under /usr/bin/time -v, stdout to $TMP/out.NAME.
run_timed() {
  _name=$1; shift
  rm -f "$TMP/time.$_name"
  /usr/bin/time -v -o "$TMP/time.$_name" "$@" > "$TMP/out.$_name" 2>/dev/null
}

# perf NAME -> perf members (no braces) for a run_timed result.
perf() {
  _t="$TMP/time.$1"
  printf '"user_s":%s,"sys_s":%s,"wall":"%s","max_rss_kb":%s' \
    "$(tfield "$_t" "User time")" "$(tfield "$_t" "System time")" \
    "$(tfield "$_t" "Elapsed")" "$(tfield "$_t" "Maximum resident")"
}

# cmp_slice FIRST COMPARES $TMP/out.$2 (decoded blocks) against the source range.
cmp_slice() {
  _first=$1; _name=$2
  tail -c "+$((A - _first + 1))" "$TMP/out.$_name" | head -c "$LEN" > "$TMP/exp.bin"
  tail -c "+$((A + 1))" "$SOURCE" | head -c "$LEN" | cmp -s - "$TMP/exp.bin"
}

# extract_xz NAME FILE : probe, reframe the covering blocks, decode in isolation.
extract_xz() {
  _name=$1; _file=$2
  eval "$(perl tools/xz-seek-probe.pl "$_file" "$A" "$LEN")"
  perl tools/xz-block-reframe.pl "$_file" "$TMP/ref.$_name.xz" "$BLOCKS"
  run_timed "$_name" xz -dc "$TMP/ref.$_name.xz"
  if cmp_slice "$FIRST" "$_name"; then _ok=true; else _ok=false; fi
  printf '{"bytes_read":%s,"index":%s,"blocks":"%s","correct":%s,%s}' \
    "$COST" "$INDEX" "$BLOCKS" "$_ok" "$(perf "$_name")"
}

# extract_bgzf : probe, extract the covering BGZF block(s), decode each as a lone
# gzip member.
extract_bgzf() {
  eval "$(perl tools/bgzf-seek-probe.pl "$BGZ" "$GZI" "$A" "$LEN")"
  rm -f "$TMP/out.bgzip"
  _cmds=": > '$TMP/out.bgzip'"
  for _blk in $(echo "$BLOCKS" | tr ',' ' '); do
    _off=${_blk%%:*}; _size=${_blk##*:}
    _cmds="$_cmds; tail -c +$((_off + 1)) '$BGZ' | head -c $_size | gzip -dc >> '$TMP/out.bgzip'"
  done
  rm -f "$TMP/time.bgzip"
  if /usr/bin/time -v -o "$TMP/time.bgzip" sh -c "$_cmds" 2>/dev/null; then _ok=true; else _ok=false; fi
  if cmp_slice "$FIRST" bgzip; then _cok=true; else _cok=false; fi
  printf '{"bytes_read":%s,"index":%s,"blocks":"%s","decode_ok":%s,"correct":%s,%s}' \
    "$COST" "$INDEX" "$BLOCKS" "$_ok" "$_cok" "$(perf bgzip)"
}

# --- Pre-registered query set: 256 B at 0/1/8/16/24/31 MiB ------------------
for A in 0 1048576 8388608 16777216 25165824 32505856; do
  LEN=256

  run_timed vole "$BIN" view "$VOLDOC" "$TMP/obs.bin" "--byte-range=$A:$LEN" --stats
  VREAD=$(sed -n 's/.*"bytes_read":\([0-9]*\).*/\1/p' "$TMP/out.vole" | head -1)
  tail -c "+$((A + 1))" "$SOURCE" | head -c "$LEN" | cmp -s - "$TMP/obs.bin" && VOK=true || VOK=false

  BG=$(extract_bgzf)
  X64=$(extract_xz xz64k "$XZ64")
  X1M=$(extract_xz xz1m "$XZ1M")
  X4M=$(extract_xz xz4m "$XZ4M")
  PX=$(extract_xz pixz "$PXZ")

  printf '{"query":"byte-%s","range_start":%s,"range_len":%s,"correct":%s,"vole":{"bytes_read":%s,"correct":%s,%s},"bgzip":%s,"xz64k":%s,"xz1m":%s,"xz4m":%s,"pixz":%s,"archives":{%s}}\n' \
    "$A" "$A" "$LEN" "$VOK" "$VREAD" "$VOK" "$(perf vole)" "$BG" "$X64" "$X1M" "$X4M" "$PX" "$ARCH" >> "$OUT"

  printf '  a=%-9s len=%-4s vole=%-8s bgzip=%-8s xz64k=%-8s xz1m=%-8s xz4m=%-8s pixz=%-8s\n' \
    "$A" "$LEN" "$VREAD" \
    "$(echo "$BG" | sed 's/{"bytes_read":\([0-9]*\).*/\1/')" \
    "$(echo "$X64" | sed 's/{"bytes_read":\([0-9]*\).*/\1/')" \
    "$(echo "$X1M" | sed 's/{"bytes_read":\([0-9]*\).*/\1/')" \
    "$(echo "$X4M" | sed 's/{"bytes_read":\([0-9]*\).*/\1/')" \
    "$(echo "$PX" | sed 's/{"bytes_read":\([0-9]*\).*/\1/')" >&2
done

echo "seekable baseline court written to $OUT" >&2
