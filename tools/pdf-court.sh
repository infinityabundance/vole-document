#!/bin/sh
# Phase-7.0 complete-cost court over the producer corpus.
#
# Runs the real `vole-document` CLI (built in the `dev` service with
# `--all-features`) over every corpus file and records the *serialized*
# `.voldoc` size for each lane:
#
#   * the ordinary complete-cost court:   encode FILE OUT
#   * forced single lanes:                encode --force KIND FILE OUT
#       KIND in raw, byte-rans, pdf-deflate-replay, pdf-deflate-replay-rans
#
# A forced kind the input does not propose exits with a typed Usage error
# (`candidate ... is not proposed for this input`); that honest decline is
# recorded as `null`, never silently substituted.
#
# For the auto winner the script also runs `verify` and byte-compares a full
# `decode` against the source. It emits one JSON object per input file to
# stdout (JSONL). `encoded_len` is the complete serialized length, so it is the
# figure the standalone-compression court compares.
#
# Usage (from the host, inside the dev service):
#   docker compose run --rm --no-TTY dev \
#     sh -c './target/debug/vole-document ...'   # (driver below)
#   sh tools/pdf-court.sh OUT_DIR FILE...
set -eu

OUT_DIR=$1
shift

mkdir -p "$OUT_DIR"
BIN=./target/debug/vole-document

# extract_len JSON — pull the integer `encoded_len` from an encode report.
extract_len() {
  _v=$(printf '%s' "$1" | sed -n 's/.*"encoded_len":\([0-9][0-9]*\).*/\1/p')
  if [ -n "$_v" ]; then printf '%s' "$_v"; else printf 'null'; fi
}

# extract_candidate JSON — pull the winning `candidate` kind name.
extract_candidate() {
  printf '%s' "$1" | sed -n 's/.*"candidate":"\([A-Z_]*\)".*/\1/p'
}

# run_force FILE KIND — hmm, KIND is the CLI spelling.
run_force() {
  _f=$1; _kind=$2
  _base=$(printf '%s' "$_f" | tr '/' '_')
  _out="$OUT_DIR/.$_base.$_kind.voldoc"
  rm -f "$_out"
  set +e
  _json=$($BIN encode --force "$_kind" "$_f" "$_out" 2>/tmp/court-err.txt)
  _rc=$?
  set -e
  if [ "$_rc" -eq 0 ]; then
    _v=$(extract_len "$_json")
  else
    _v=null
  fi
  rm -f "$_out"
  printf '%s' "$_v"
}

for _f in "$@"; do
  _name=$(basename "$_f")
  _auto_out="$OUT_DIR/.$_name.auto.voldoc"
  rm -f "$_auto_out"
  _auto_json=$($BIN encode "$_f" "$_auto_out")
  _auto_len=$(extract_len "$_auto_json")
  _auto_cand=$(extract_candidate "$_auto_json")
  _src_len=$(printf '%s' "$_auto_json" | sed -n 's/.*"source_len":\([0-9][0-9]*\).*/\1/p')

  # Independent exactness checks on the auto winner.
  set +e
  $BIN verify "$_auto_out" >/dev/null 2>&1
  _verify_rc=$?
  $BIN decode "$_auto_out" "$OUT_DIR/.$_name.dec" >/dev/null 2>&1
  _decode_rc=$?
  cmp -s "$OUT_DIR/.$_name.dec" "$_f"
  _cmp_rc=$?
  set -e
  if [ "$_verify_rc" -eq 0 ]; then _verify=true; else _verify=false; fi
  if [ "$_decode_rc" -eq 0 ] && [ "$_cmp_rc" -eq 0 ]; then _rt=true; else _rt=false; fi

  _raw=$(run_force "$_f" raw)
  _byte_rans=$(run_force "$_f" byte-rans)
  _deflate=$(run_force "$_f" pdf-deflate-replay)
  _deflate_rans=$(run_force "$_f" pdf-deflate-replay-rans)

  rm -f "$_auto_out" "$OUT_DIR/.$_name.dec"

  printf '{"file":"%s","source_len":%s,"auto_candidate":"%s","auto_len":%s,' \
    "$_name" "$_src_len" "$_auto_cand" "$_auto_len"
  printf '"raw":%s,"byte_rans":%s,"deflate_replay":%s,"deflate_replay_rans":%s,' \
    "$_raw" "$_byte_rans" "$_deflate" "$_deflate_rans"
  printf '"verify_ok":%s,"roundtrip_ok":%s}\n' "$_verify" "$_rt"
done
