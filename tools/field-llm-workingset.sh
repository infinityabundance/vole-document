#!/bin/sh
# Phase-11.10 — LLM working-set court.
#
# The question is: "what is on page N?" The representation handed to a model can
# be one of:
#
#   B0  the whole extracted document (pdftotext of every page);
#   B1  the page-local extract (pdftotext -f N -l N);
#   V   the VOLE `observe --page N --kind text` answer.
#
# This script reports the UTF-8 byte size of each candidate and the
# `context_waste_ratio` (whole-document bytes / page-local VOLE bytes): how many
# times more context a whole-document baseline feeds a model than the
# page-scoped VOLE answer.
#
# ## Tokens
#
# Token counts are only meaningful against a *pinned* tokenizer. No pinned
# offline tokenizer (a vendored BPE/`.model` file with a recorded SHA-256) is
# available in this image, and installing an unpinned model would make the
# number non-reproducible, so this court **does not claim token counts**. It
# reports UTF-8 bytes only and sets `tokens: null` with an explicit reason.
#
# Usage (inside a service with poppler + the VOLE CLI, e.g. db-baseline):
#   sh tools/field-llm-workingset.sh OUT.json PDF PAGE STORE FIELD
set -eu

cd /work
LC_ALL=C
export LC_ALL

OUT=$1
PDF=$2
PAGE=$3
STORE=$4
FIELD=$5
BIN=${VOLE_BIN:-./target/debug/vole-document}

[ -f "$PDF" ] || { echo "no such PDF: $PDF" >&2; exit 2; }

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# B0 — whole extracted document.
pdftotext "$PDF" "$TMP/b0.txt" 2>/dev/null || : > "$TMP/b0.txt"
B0=$(wc -c < "$TMP/b0.txt" | tr -d ' ')
B0_SHA=$(sha256sum "$TMP/b0.txt" | cut -d' ' -f1)

# B1 — page-local extract.
pdftotext -f "$PAGE" -l "$PAGE" "$PDF" "$TMP/b1.txt" 2>/dev/null || : > "$TMP/b1.txt"
B1=$(wc -c < "$TMP/b1.txt" | tr -d ' ')
B1_SHA=$(sha256sum "$TMP/b1.txt" | cut -d' ' -f1)

# V — VOLE page text observation. The `observe` JSON is now valid (the missing
# comma before `"stats"` was fixed in 63f43fb), so it parses standalone with jq;
# `bytes_returned` is the page-text byte count.
V_RAW=$("$BIN" observe --store "$STORE" --field "$FIELD" --page "$PAGE" --kind text 2>/dev/null || true)
V_STATS=$(printf '%s' "$V_RAW" | jq -c '.stats' 2>/dev/null || echo '{}')
V=$(printf '%s' "$V_RAW" | jq -r '.stats.bytes_returned' 2>/dev/null || echo 0)
[ -n "$V" ] && [ "$V" != null ] || V=0

# context_waste_ratio, stated precisely: bytes of whole-document extract divided
# by bytes of the page-scoped VOLE answer. >1 means the whole-document baseline
# hands the model that many times more context than VOLE's page-local answer.
jq -n \
  --arg pdf "$PDF" \
  --argjson page "$PAGE" \
  --argjson b0_bytes "$B0" --arg b0_sha256 "$B0_SHA" \
  --argjson b1_bytes "$B1" --arg b1_sha256 "$B1_SHA" \
  --argjson v_bytes "$V" \
  --arg v_stats "$V_STATS" \
  '
  def ratio($x;$y): if $y > 0 then ($x / $y) else null end;
  {
    question: ("what is on page " + ($page|tostring)),
    baselines: {
      B0_whole_document: {oracle: "pdftotext (poppler)", bytes: $b0_bytes, sha256: $b0_sha256},
      B1_page_local:     {oracle: "pdftotext -f/-l (poppler)", bytes: $b1_bytes, sha256: $b1_sha256},
      V_vole_page_text:  {repr: "observe --page --kind text", bytes: $v_bytes, stats: $v_stats}
    },
    context_waste_ratio: ratio($b0_bytes; $v_bytes),
    ratios: {
      B0_over_V: ratio($b0_bytes; $v_bytes),
      B1_over_V: ratio($b1_bytes; $v_bytes),
      B0_over_B1: ratio($b0_bytes; $b1_bytes)
    },
    tokens: null,
    tokens_reason: "no pinned offline tokenizer (no vendored BPE/model with a recorded SHA-256); UTF-8 bytes reported only, token counts not claimed",
    honest_losses: (
      [ if $v_bytes > $b1_bytes then "V_page_text_larger_than_B1_page_local" else empty end,
        if $v_bytes > $b0_bytes then "V_page_text_larger_than_B0_whole_document" else empty end ]
    )
  }' > "$OUT"

cat "$OUT"
