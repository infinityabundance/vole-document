#!/bin/sh
# Phase-11.10/11.13 — LLM working-set court.
#
# The question is: "what is on page N?" The representation handed to a model can
# be one of:
#
#   B0  the whole extracted document (pdftotext of every page);
#   B1  the page-local extract (pdftotext -f N -l N);
#   V   the VOLE `observe --page N --kind text` answer.
#
# This script reports, for each candidate, the UTF-8 byte size, the **token
# count under a pinned, named tokenizer**, the `context_waste_ratio`
# (whole-document bytes / page-local VOLE bytes), the token-space counterpart of
# that ratio, and `answer_relevant_tokens` (the page-local B1 tokens — the least
# context that still names the page).
#
# ## Tokens are now genuinely measured
#
# The tokenizer is the vendored HuggingFace `bert-base-uncased` `tokenizer.json`
# (`tools/tokenizers/`), loaded by the pinned `tokenizers==0.20.3` runtime inside
# the `llm-workingset` image. The asset's SHA-256 is verified at court time and
# its identity (name, implementation version, config, digest) is emitted next to
# every count. **No token number is ever reported without the tokenizer that
# produced it**, and a smaller V is a working-set measurement, not a text-quality
# claim.
#
# Usage (inside a service with poppler + python3 + the pinned tokenizer, i.e.
# the `llm-workingset` service):
#   sh tools/field-llm-workingset.sh OUT.json PDF PAGE STORE FIELD
#
# Optional environment (defaults match the vendored asset):
#   LLM_TOKENIZER_JSON  path to the tokenizer.json asset
#   LLM_TOKENIZER_SHA   path to a `sha256sum`-format digest file, or the digest
#   LLM_TOKENIZER_NAME  human name recorded in the receipt
#   LLM_TOKENIZE_PY     path to the loader
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

LLM_TOKENIZER_JSON=${LLM_TOKENIZER_JSON:-tools/tokenizers/bert-base-uncased.tokenizer.json}
LLM_TOKENIZER_SHA=${LLM_TOKENIZER_SHA:-tools/tokenizers/bert-base-uncased.tokenizer.json.sha256}
LLM_TOKENIZER_NAME=${LLM_TOKENIZER_NAME:-bert-base-uncased}
LLM_TOKENIZE_PY=${LLM_TOKENIZE_PY:-tools/llm-tokenize.py}

[ -f "$PDF" ] || { echo "no such PDF: $PDF" >&2; exit 2; }

# The recorded digest is either a file (`sha256sum` format) or a bare hex string.
if [ -f "$LLM_TOKENIZER_SHA" ]; then
  EXPECT_SHA=$(cut -d' ' -f1 < "$LLM_TOKENIZER_SHA")
else
  EXPECT_SHA=$LLM_TOKENIZER_SHA
fi

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

# V — VOLE page text observation. `observe` emits a valid JSON object; `jq -j`
# writes the `.text` payload with no added trailing newline, so the V bytes on
# disk equal the reported `bytes_returned` (cross-checked below).
V_RAW=$("$BIN" observe --store "$STORE" --field "$FIELD" --page "$PAGE" --kind text 2>/dev/null || true)
V_STATS=$(printf '%s' "$V_RAW" | jq -c '.stats' 2>/dev/null || echo '{}')
V=$(printf '%s' "$V_RAW" | jq -r '.stats.bytes_returned' 2>/dev/null || echo 0)
[ -n "$V" ] && [ "$V" != null ] || V=0
printf '%s' "$V_RAW" | jq -j '.text' > "$TMP/v.txt" 2>/dev/null || : > "$TMP/v.txt"
V_DISK=$(wc -c < "$TMP/v.txt" | tr -d ' ')
if [ "$V_DISK" = "$V" ]; then V_BYTES_MATCH=true; else V_BYTES_MATCH=false; fi

# Tokens require the pinned runtime, installed in the `llm-workingset` image —
# the intended home of this court. Other callers (e.g. `db-baseline` running
# `tools/field-court.sh`) have no Python, so the script *degrades to bytes only*
# with an explicit reason instead of failing. A token number is still never
# reported without the tokenizer that produced it.
TOK_AVAILABLE=false
if command -v python3 >/dev/null 2>&1 && [ -f "$LLM_TOKENIZER_JSON" ] && [ -f "$LLM_TOKENIZE_PY" ] \
   && python3 -c 'import tokenizers' >/dev/null 2>&1; then
  TOK_AVAILABLE=true
fi

if [ "$TOK_AVAILABLE" = true ]; then
  # One pinned loader invocation, three files, one JSON object.
  TOK=$(python3 "$LLM_TOKENIZE_PY" \
    --tokenizer "$LLM_TOKENIZER_JSON" \
    --expect-sha256 "$EXPECT_SHA" \
    --name "$LLM_TOKENIZER_NAME" \
    --files "$TMP/b0.txt" "$TMP/b1.txt" "$TMP/v.txt")
  TOK_OK=$(printf '%s' "$TOK" | jq -r '.ok')
  if [ "$TOK_OK" != true ]; then
    echo "field-llm-workingset: tokenizer failed:" >&2
    printf '%s\n' "$TOK" >&2
    exit 5
  fi
  B0_TOK=$(printf '%s' "$TOK" | jq -r '.counts[0].tokens')
  B1_TOK=$(printf '%s' "$TOK" | jq -r '.counts[1].tokens')
  V_TOK=$(printf '%s' "$TOK" | jq -r '.counts[2].tokens')
  B0_DECODED_OK=$(printf '%s' "$TOK" | jq -r '.counts[0].valid_utf8')
  B1_DECODED_OK=$(printf '%s' "$TOK" | jq -r '.counts[1].valid_utf8')
  V_DECODED_OK=$(printf '%s' "$TOK" | jq -r '.counts[2].valid_utf8')
  printf '%s' "$TOK" > "$TMP/tok.json"
  TOKENS_REASON=null
else
  B0_TOK=null; B1_TOK=null; V_TOK=null
  B0_DECODED_OK=null; B1_DECODED_OK=null; V_DECODED_OK=null
  printf '{"tokenizer":null}' > "$TMP/tok.json"
  TOKENS_REASON='"pinned tokenizer not installed in this image; UTF-8 bytes only (run the token court inside the llm-workingset service)"'
fi

# context_waste_ratio, stated precisely: bytes of whole-document extract divided
# by bytes of the page-scoped VOLE answer. >1 means the whole-document baseline
# hands the model that many times more context than VOLE's page-local answer.
# The token-space counterpart (`token_ratios`) is the same comparison under the
# named tokenizer (null when the tokenizer is unavailable).
jq -n \
  --arg pdf "$PDF" \
  --argjson page "$PAGE" \
  --argjson tok_available "$TOK_AVAILABLE" \
  --argjson tokens_reason "$TOKENS_REASON" \
  --argjson b0_bytes "$B0" --arg b0_sha256 "$B0_SHA" --argjson b0_tokens "$B0_TOK" \
  --argjson b1_bytes "$B1" --arg b1_sha256 "$B1_SHA" --argjson b1_tokens "$B1_TOK" \
  --argjson v_bytes "$V" --argjson v_tokens "$V_TOK" \
  --arg v_stats "$V_STATS" \
  --argjson v_bytes_match "$V_BYTES_MATCH" \
  --argjson b0_decoded_ok "$B0_DECODED_OK" \
  --argjson b1_decoded_ok "$B1_DECODED_OK" \
  --argjson v_decoded_ok "$V_DECODED_OK" \
  --slurpfile tok "$TMP/tok.json" \
  '
  def ratio($x;$y): if ($x != null and $y != null and $y > 0) then ($x / $y) else null end;
  {
    question: ("what is on page " + ($page|tostring)),
    tokenizer: ($tok[0].tokenizer),
    tokens_measured: $tok_available,
    tokens_reason: $tokens_reason,
    baselines: {
      B0_whole_document: {oracle: "pdftotext (poppler)", bytes: $b0_bytes,
                          sha256: $b0_sha256, tokens: $b0_tokens, text_valid_utf8: $b0_decoded_ok},
      B1_page_local:     {oracle: "pdftotext -f/-l (poppler)", bytes: $b1_bytes,
                          sha256: $b1_sha256, tokens: $b1_tokens, text_valid_utf8: $b1_decoded_ok},
      V_vole_page_text:  {repr: "observe --page --kind text", bytes: $v_bytes,
                          tokens: $v_tokens, text_valid_utf8: $v_decoded_ok,
                          bytes_returned_matches_payload: $v_bytes_match, stats: $v_stats}
    },
    context_waste_ratio: ratio($b0_bytes; $v_bytes),
    ratios: {
      B0_over_V: ratio($b0_bytes; $v_bytes),
      B1_over_V: ratio($b1_bytes; $v_bytes),
      B0_over_B1: ratio($b0_bytes; $b1_bytes)
    },
    token_context_waste_ratio: ratio($b0_tokens; $v_tokens),
    token_ratios: {
      B0_over_V: ratio($b0_tokens; $v_tokens),
      B1_over_V: ratio($b1_tokens; $v_tokens),
      B0_over_B1: ratio($b0_tokens; $b1_tokens)
    },
    answer_relevant_tokens: $b1_tokens,
    tokens: {B0: $b0_tokens, B1: $b1_tokens, V: $v_tokens},
    honest_losses: (
      [ if $v_bytes > $b1_bytes then "V_page_text_larger_than_B1_page_local" else empty end,
        if $v_bytes > $b0_bytes then "V_page_text_larger_than_B0_whole_document" else empty end,
        if ($v_tokens != null and $v_tokens > $b1_tokens) then "V_tokens_exceed_B1_page_local" else empty end,
        if ($v_tokens != null and $v_tokens > $b0_tokens) then "V_tokens_exceed_B0_whole_document" else empty end ]
    )
  }' > "$OUT"

cat "$OUT"
