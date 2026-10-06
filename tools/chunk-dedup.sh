#!/bin/sh
# Phase-9.3 generic content-defined-chunk dedup baseline.
#
# The fair baseline for a cross-document content-addressed store is a generic
# *content-defined-chunk* dedup store, not only per-file LZ. This script runs
# `borg` with **frozen** parameters and `--compression=none`, so dedup is
# isolated from chunk compression, and reports the repository's unique stored
# bytes (the deduplicated content size).
#
#   tool:    borgbackup (Debian bookworm pin 1.2.4-1)
#   chunker: `--chunker-params 19,23,21,4095` = (min_exp, max_exp,
#            hash_mask_bits, hash_window_size) -> 2**19 = 512 KiB min chunk,
#            2**23 = 8 MiB max chunk, 2**21 = 2 MiB target, 4095 B rolling-hash
#            window. Frozen so boundaries are reproducible.
#   compress: none
#
# Determinism: the chunker is a fixed-parameter rolling hash with no randomness,
# so two runs over the same bytes produce the same chunk boundaries and the same
# `unique_csize`. The script verifies this by running twice and comparing, and
# reports `deterministic` accordingly.
#
# Usage (inside the `baseline` service):
#   sh tools/chunk-dedup.sh COHORT_DIR OUT.json [CHUNKER_PARAMS] [COMPRESSION]
# CHUNKER_PARAMS defaults to the frozen borg default `19,23,21,4095`; the court
# sweeps several and keeps the smallest, so the CDC comparison is the strongest
# available, not a strawman. COMPRESSION defaults to `none` (dedup isolated from
# chunk compression); the court also runs `zstd,19` as a compressed cross-check.
set -eu

cd /work

DIR=$1
OUT=$2

BORG_VER="$(borg --version | awk '{print $2}')"
PARAMS=${3:-19,23,21,4095}
COMP=${4:-none}

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

export BORG_BASE_DIR="$TMP/base"
export BORG_CACHE_DIR="$TMP/cache"
export BORG_CONFIG_DIR="$TMP/config"
export BORG_SECURITY_DIR="$TMP/security"
export BORG_KEYS_DIR="$TMP/keys"
mkdir -p "$BORG_BASE_DIR" "$BORG_CACHE_DIR" "$BORG_CONFIG_DIR" "$BORG_SECURITY_DIR" "$BORG_KEYS_DIR"

# run_borg SUFFIX -> writes repo JSON to $TMP/info-$SUFFIX.json, echoes unique_csize
run_borg() {
  _sfx=$1
  _repo="$TMP/repo-$_sfx"
  borg init --encryption=none "$_repo" >/dev/null 2>&1
  borg create --compression "$COMP" --chunker-params "$PARAMS" --stats \
    "$_repo::cohort" "$DIR" >/dev/null 2>&1
  borg compact "$_repo" >/dev/null 2>&1
  borg info --json "$_repo" > "$TMP/info-$_sfx.json"
  sed -n 's/.*"unique_csize":[[:space:]]*\([0-9][0-9]*\).*/\1/p' "$TMP/info-$_sfx.json" | head -1
}

UNIQ1=$(run_borg a)
UNIQ2=$(run_borg b)

DET=false
if [ "$UNIQ1" = "$UNIQ2" ]; then DET=true; fi

extract() { sed -n "s/.*\"$1\":[[:space:]]*\([0-9][0-9]*\).*/\1/p" "$TMP/info-a.json" | head -1; }
TOTAL_SIZE=$(extract total_size)
TOTAL_CSIZE=$(extract total_csize)
TOTAL_CHUNKS=$(extract total_chunks)
UNIQ_CHUNKS=$(extract total_unique_chunks)

if [ "$TOTAL_SIZE" = "" ]; then TOTAL_SIZE=0; fi
if [ "$TOTAL_CSIZE" = "" ]; then TOTAL_CSIZE=0; fi
if [ "$TOTAL_CHUNKS" = "" ]; then TOTAL_CHUNKS=0; fi
if [ "$UNIQ_CHUNKS" = "" ]; then UNIQ_CHUNKS=0; fi

printf '{"tool":"borg","borg_version":"%s","chunker_params":"%s","compression":"%s","dir":"%s","borg_unique_bytes":%s,"borg_unique_bytes_run2":%s,"borg_deterministic":%s,"borg_total_size":%s,"borg_total_csize":%s,"borg_total_chunks":%s,"borg_unique_chunks":%s}\n' \
  "$BORG_VER" "$PARAMS" "$COMP" "$DIR" "$UNIQ1" "$UNIQ2" "$DET" "$TOTAL_SIZE" "$TOTAL_CSIZE" "$TOTAL_CHUNKS" "$UNIQ_CHUNKS" > "$OUT"

cat "$OUT"
