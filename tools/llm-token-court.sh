#!/bin/sh
# Phase-11.13 — LLM working-set *token* court (priority #8).
#
# The §11.10 court reported UTF-8 bytes only and set `tokens: null` because no
# pinned offline tokenizer existed. This court closes that honestly: it measures
# the same three baseline classes —
#
#   B0  whole extracted document (pdftotext of every page);
#   B1  page-local extract (pdftotext -f N -l N);
#   V   the VOLE `observe --page N --kind text` answer
#
# — in UTF-8 bytes *and* in tokens under a pinned, named tokenizer
# (`bert-base-uncased`, WordPiece 30522, loaded by the hash-pinned
# `tokenizers==0.20.3` runtime from the vendored asset in tools/tokenizers/,
# whose SHA-256 is verified at court time).
#
# It answers one question plainly: **does the VOLE page answer reduce the tokens
# a model must read for "what is on page N", relative to the conventional
# page-local extract?** Wins, ties and losses are all recorded. A smaller V is a
# working-set measurement under this tokenizer, never a text-quality claim.
#
# Usage (inside the `llm-workingset` service):
#   sh tools/llm-token-court.sh OUTDIR [PRODUCER_DIR]
set -eu

cd /work
LC_ALL=C
export LC_ALL

OUTDIR=${1:-evidence/scratch/llm-token-court}
PRODUCER_DIR=${2:-evidence/corpus/phase7-producers}
BIN=${VOLE_BIN:-./target/debug/vole-document}

LLM_TOKENIZER_JSON=${LLM_TOKENIZER_JSON:-tools/tokenizers/bert-base-uncased.tokenizer.json}
LLM_TOKENIZER_SHA=${LLM_TOKENIZER_SHA:-tools/tokenizers/bert-base-uncased.tokenizer.json.sha256}
LLM_TOKENIZER_NAME=${LLM_TOKENIZER_NAME:-bert-base-uncased}
LLM_TOKENIZE_PY=${LLM_TOKENIZE_PY:-tools/llm-tokenize.py}

PRIMARY_PAGE=${PRIMARY_PAGE:-1}

mkdir -p "$OUTDIR/raw"
RAW="$OUTDIR/raw"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

# The required gates run `cargo test --no-default-features` last, which rebuilds
# `target/debug/vole-document` *without* the `field`/`store` features. Detect a
# binary that cannot serve the field court and rebuild the all-features one.
if [ ! -x "$BIN" ] || ! "$BIN" --help 2>&1 | grep -q 'field-ingest'; then
  echo "llm-token-court: building the all-features binary ($BIN)" >&2
  rm -f "$BIN"
  cargo build --locked --all-features 1>&2
fi

now_ms() { echo $(( $(date +%s%N) / 1000000 )); }
sha() { sha256sum "$1" | cut -d' ' -f1; }
bytes() { stat -c %s "$1" 2>/dev/null || echo 0; }
pages() { pdfinfo "$1" 2>/dev/null | awk '/^Pages:/{print $2}' || echo 0; }

# ---------------------------------------------------------------------------
# Environment capture (everything a receipt must pin).
# ---------------------------------------------------------------------------
RUSTC_V=$(rustc --version 2>/dev/null | sed 's/^rustc //' || echo unknown)
CARGO_V=$(cargo --version 2>/dev/null | sed 's/^cargo //' || echo unknown)
PY_V=$(python3 --version 2>&1 | sed 's/^Python //')
ARCH=$(uname -m)
COMMIT=$(git rev-parse HEAD 2>/dev/null || echo unknown)
COMMIT_SHORT=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
DIRTY=$(git status --porcelain 2>/dev/null | head -40 | tr '\n' ';' || echo unknown)
LOCK_SHA=$(sha /work/Cargo.lock 2>/dev/null || echo unknown)
BASE_STABLE=$(sed -n 's/^ARG BASE_STABLE=//p' /work/Dockerfile 2>/dev/null | head -1)
BASE_TOOLS=$(sed -n 's/^ARG BASE_TOOLS=//p' /work/Dockerfile 2>/dev/null | head -1)
IMAGE_REF=vole-document/llm-workingset:1.99.0
IMAGE_ID=${HOST_IMAGE_ID:-unrecorded}
POPPLER_V=$(pdftotext -v 2>&1 | head -1 | sed 's/^pdftotext version //')
JQ_V=$(jq --version)

# Tokenizer identity, verified from the asset on disk (no files => identity only).
TOK_ID=$(python3 "$LLM_TOKENIZE_PY" \
  --tokenizer "$LLM_TOKENIZER_JSON" \
  --expect-sha256 "$(cut -d' ' -f1 < "$LLM_TOKENIZER_SHA")" \
  --name "$LLM_TOKENIZER_NAME")
TOK_OK=$(printf '%s' "$TOK_ID" | jq -r '.ok')
if [ "$TOK_OK" != true ]; then
  echo "llm-token-court: tokenizer identity check failed:" >&2
  printf '%s\n' "$TOK_ID" >&2
  exit 5
fi
printf '%s' "$TOK_ID" > "$RAW/tokenizer.json"

jq -n \
  --arg commit "$COMMIT" --arg commit_short "$COMMIT_SHORT" --arg dirty "$DIRTY" \
  --arg lock_sha "$LOCK_SHA" --arg arch "$ARCH" \
  --arg rustc "$RUSTC_V" --arg cargo "$CARGO_V" --arg python "$PY_V" \
  --arg base_stable "$BASE_STABLE" --arg base_tools "$BASE_TOOLS" \
  --arg image_ref "$IMAGE_REF" --arg image_id "$IMAGE_ID" \
  --arg poppler "$POPPLER_V" --arg jq "$JQ_V" \
  --arg bin "$BIN" \
  '{
     git: {commit: $commit, commit_short: $commit_short, dirty: $dirty},
     arch: $arch,
     cargo_lock_sha256: $lock_sha,
     toolchain: {rustc: $rustc, cargo: $cargo, python: $python},
     images: {base_stable_digest: $base_stable, base_tools_digest: $base_tools,
              image_ref: $image_ref, llm_workingset_image_id: $image_id},
     oracles: {poppler_pdftotext: $poppler, jq: $jq},
     env_affecting_semantics: {LC_ALL: "C", VOLE_BIN: $bin, PRIMARY_PAGE: "1"},
     tokenizer: $tok[0].tokenizer
   }' --slurpfile tok "$RAW/tokenizer.json" > "$RAW/environment.json"

# ---------------------------------------------------------------------------
# Cases.
# ---------------------------------------------------------------------------
: > "$RAW/cases.jsonl"

court_case() {
  _name=$1; _src=$2; _kind=$3
  _d="$WORK/$_name"
  mkdir -p "$_d/store"
  _src_len=$(bytes "$_src"); _src_sha=$(sha "$_src"); _npages=$(pages "$_src")
  echo "llm-token-court: case=$_name kind=$_kind pages=$_npages source=${_src_len}B" >&2

  _desc="$_d/$_name.voldoc"
  _t0=$(now_ms); "$BIN" encode "$_src" "$_desc" > "$RAW/$_name.encode.json"; _t1=$(now_ms)
  _encode_ms=$(( _t1 - _t0 ))
  _t0=$(now_ms); "$BIN" field-ingest "$_desc" --store "$_d/store" > "$RAW/$_name.ingest.json"; _t1=$(now_ms)
  _ingest_ms=$(( _t1 - _t0 ))
  _field=$(jq -r .field "$RAW/$_name.ingest.json")

  sh tools/field-llm-workingset.sh "$RAW/$_name.llm.json" "$_src" "$PRIMARY_PAGE" "$_d/store" "$_field" >/dev/null

  jq -n \
    --arg name "$_name" --arg kind "$_kind" \
    --arg src "$_src" --argjson src_len "$_src_len" --arg src_sha "$_src_sha" \
    --argjson npages "$_npages" --arg field "$_field" \
    --argjson encode_ms "$_encode_ms" --argjson ingest_ms "$_ingest_ms" \
    --slurpfile enc "$RAW/$_name.encode.json" \
    --slurpfile llm "$RAW/$_name.llm.json" \
    '{
       name: $name, kind: $kind,
       source: {path: $src, len: $src_len, sha256: $src_sha, pages: $npages},
       encode: {candidate: ($enc[0].candidate // null), encoded_len: ($enc[0].encoded_len // null), wall_ms: $encode_ms},
       field: $field, ingest_wall_ms: $ingest_ms,
       llm: $llm[0]
     }' >> "$RAW/cases.jsonl"
}

# Generated documents (deterministic; same sizes as the §11.10 court).
"$BIN" pdf-make-large "$WORK/gen50" 50 > "$RAW/gen-50.json"
"$BIN" pdf-make-large "$WORK/gen400" 400 > "$RAW/gen-400.json"
court_case large-50 "$WORK/gen50/large.pdf" generated
court_case large-400 "$WORK/gen400/large.pdf" generated

# Real producer documents, when present (regenerable, gitignored bytes).
for _p in "$PRODUCER_DIR"/libreoffice-export.pdf "$PRODUCER_DIR"/cairo-vector.pdf \
          "$PRODUCER_DIR"/pdftex-doc.pdf "$PRODUCER_DIR"/reportlab-multipage.pdf; do
  [ -f "$_p" ] || continue
  _b=$(basename "$_p" .pdf)
  court_case "$_b" "$_p" producer
done

jq -s '.' "$RAW/cases.jsonl" > "$RAW/cases.json"

# ---------------------------------------------------------------------------
# Verdict: per case, V vs B1 (page-local Poppler) and V vs B0 (whole doc).
# ---------------------------------------------------------------------------
jq -n --slurpfile c "$RAW/cases.json" '
  def verdict($v; $b): if $v < $b then "win" elif $v == $b then "tie" else "loss" end;
  ($c[0] | map({
     name: .name, kind: .kind,
     B0_bytes: .llm.baselines.B0_whole_document.bytes,
     B1_bytes: .llm.baselines.B1_page_local.bytes,
     V_bytes: .llm.baselines.V_vole_page_text.bytes,
     B0_tokens: .llm.baselines.B0_whole_document.tokens,
     B1_tokens: .llm.baselines.B1_page_local.tokens,
     V_tokens: .llm.baselines.V_vole_page_text.tokens,
     token_ratio_B0_over_V: .llm.token_ratios.B0_over_V,
     token_ratio_B1_over_V: .llm.token_ratios.B1_over_V,
     verdict_vs_B1_tokens: verdict(.llm.baselines.V_vole_page_text.tokens; .llm.baselines.B1_page_local.tokens),
     verdict_vs_B0_tokens: verdict(.llm.baselines.V_vole_page_text.tokens; .llm.baselines.B0_whole_document.tokens),
     losses: .llm.honest_losses
   })) as $rows
  | {
      per_case: $rows,
      vs_B1: {
        win: ($rows | map(select(.verdict_vs_B1_tokens=="win")) | length),
        tie: ($rows | map(select(.verdict_vs_B1_tokens=="tie")) | length),
        loss: ($rows | map(select(.verdict_vs_B1_tokens=="loss")) | length)
      },
      vs_B0: {
        win: ($rows | map(select(.verdict_vs_B0_tokens=="win")) | length),
        tie: ($rows | map(select(.verdict_vs_B0_tokens=="tie")) | length),
        loss: ($rows | map(select(.verdict_vs_B0_tokens=="loss")) | length)
      },
      any_token_reduction: (($rows | map(select(.verdict_vs_B1_tokens=="win")) | length) > 0),
      all_token_reduction: (($rows | map(select(.verdict_vs_B1_tokens=="loss" or .verdict_vs_B1_tokens=="tie")) | length) == 0)
    }' > "$RAW/verdict.json"

# ---------------------------------------------------------------------------
# receipt.json + SUMMARY.md + commands.txt
# ---------------------------------------------------------------------------
jq -n \
  --slurpfile env "$RAW/environment.json" \
  --slurpfile cases "$RAW/cases.json" \
  --slurpfile verdict "$RAW/verdict.json" \
  '{
    campaign: "phase11-llm-tokens",
    phase: "Phase 11.13 — LLM working-set token court with a pinned tokenizer",
    environment: $env[0],
    tokenizer: $env[0].tokenizer,
    cases: $cases[0],
    verdict: $verdict[0],
    claim_scope: "Token counts are bert-base-uncased (WordPiece, vocab 30522) under the pinned tokenizers 0.20.3 runtime; they are not a claim about any other model. A smaller V is a working-set measurement, not a text-quality claim.",
    accounting_note: "B0/B1 are independent Poppler text extracts; V is the VOLE `observe --page --kind text` payload (a bounded heuristic text-run projection, basis=heuristic, not Poppler reading order)."
  }' > "$OUTDIR/receipt.json"

# commands.txt
cat > "$OUTDIR/commands.txt" <<EOF
# Phase 11.13 LLM working-set token court — exact commands
# Commit under test: $COMMIT (branch $(git rev-parse --abbrev-ref HEAD 2>/dev/null)); dirty: $DIRTY
# Tokenizer: $LLM_TOKENIZER_NAME; asset sha256: $(cut -d' ' -f1 < "$LLM_TOKENIZER_SHA")
# All commands run inside the pinned, capped llm-workingset image; nothing on the host.

docker compose build llm-workingset
docker compose run --rm --no-TTY dev sh -c 'cargo build --all-features --locked'
docker compose run --rm --no-TTY -e HOST_IMAGE_ID=<llm-workingset image id> llm-workingset \\
    sh tools/llm-token-court.sh $OUTDIR

# Per case, inside the court:
#   \$BIN pdf-make-large DIR {50,400}
#   \$BIN encode SRC DIR/<name>.voldoc
#   \$BIN field-ingest DIR/<name>.voldoc --store DIR/store
#   sh tools/field-llm-workingset.sh OUT.json SRC 1 STORE FIELD
#     -> pdftotext SRC (B0); pdftotext -f 1 -l 1 SRC (B1);
#        \$BIN observe --store STORE --field FIELD --page 1 --kind text (V);
#        python3 tools/llm-tokenize.py --tokenizer tools/tokenizers/bert-base-uncased.tokenizer.json \\
#            --expect-sha256 \$(cat tools/tokenizers/bert-base-uncased.tokenizer.json.sha256) \\
#            --name bert-base-uncased --files b0.txt b1.txt v.txt
EOF

# SUMMARY.md
{
  echo "# Phase 11.13 — LLM working-set token court (pinned tokenizer)"
  echo
  echo "Commit under test: \`$COMMIT\` (branch \`$(git rev-parse --abbrev-ref HEAD 2>/dev/null)\`); tree dirty: \`$DIRTY\`"
  echo
  echo "Image: \`$IMAGE_REF\` (id \`$IMAGE_ID\`), base \`$BASE_STABLE\`."
  echo "Rust: \`$RUSTC_V\` / \`$CARGO_V\`; Python \`$PY_V\`; Poppler \`$POPPLER_V\`; \`$JQ_V\`."
  echo "Arch: \`$ARCH\`; \`Cargo.lock\` sha256: \`$LOCK_SHA\`."
  echo
  echo "## Tokenizer (pinned, offline)"
  echo
  echo "\`$(jq -r '.tokenizer.name' "$RAW/tokenizer.json")\`, implementation \`$(jq -r '.tokenizer.implementation' "$RAW/tokenizer.json")\` v\`$(jq -r '.tokenizer.implementation_version' "$RAW/tokenizer.json")\`."
  echo
  echo "Asset: \`$(jq -r '.tokenizer.asset_path' "$RAW/tokenizer.json")\`"
  echo "SHA-256 \`$(jq -r '.tokenizer.asset_sha256' "$RAW/tokenizer.json")\` — verified at court time: \`$(jq -r '.tokenizer.asset_verified' "$RAW/tokenizer.json")\`."
  echo
  echo '```json'
  jq '.tokenizer' "$RAW/tokenizer.json"
  echo '```'
  echo
  echo "## Per-case working set (bytes and tokens)"
  echo
  echo "| case | kind | B0 bytes | B1 bytes | V bytes | B0 tokens | B1 tokens | V tokens | V vs B1 | V vs B0 |"
  echo "|---|---|---:|---:|---:|---:|---:|---:|---|---|"
  jq -r '.per_case[] | "| \(.name) | \(.kind) | \(.B0_bytes) | \(.B1_bytes) | \(.V_bytes) | \(.B0_tokens) | \(.B1_tokens) | \(.V_tokens) | \(.verdict_vs_B1_tokens) | \(.verdict_vs_B0_tokens) |"' "$RAW/verdict.json"
  echo
  echo "## Verdict"
  echo
  echo "V vs page-local Poppler B1 (tokens): **$(jq -r '"\(.vs_B1.win) win / \(.vs_B1.tie) tie / \(.vs_B1.loss) loss"' "$RAW/verdict.json")**."
  echo "V vs whole-document B0 (tokens): **$(jq -r '"\(.vs_B0.win) win / \(.vs_B0.tie) tie / \(.vs_B0.loss) loss"' "$RAW/verdict.json")**."
  echo
  echo "Any token reduction over B1 on the tested observations: \`$(jq -r '.any_token_reduction' "$RAW/verdict.json")\`; on **all** tested observations: \`$(jq -r '.all_token_reduction' "$RAW/verdict.json")\`."
  echo
  echo "## Honest losses and caveats"
  echo
  jq -r '.per_case[] | select((.losses|length)>0) | "- **\(.name)**: V tokens \(.V_tokens) vs B1 \(.B1_tokens) / B0 \(.B0_tokens) — \(.losses|join(", "))"' "$RAW/verdict.json"
  echo
  echo "- Token counts are tokenizer-specific: \`bert-base-uncased\` (WordPiece, vocab 30522). They are **not** a claim about any other model's tokenizer."
  echo "- B0/B1 are Poppler reading-order extracts; V is VOLE's bounded heuristic text-run projection (\`basis=heuristic\`). A smaller V in bytes/tokens is a *working-set* measurement, never a text-quality claim."
  echo "- \`add_special_tokens=false\`: the count excludes the tokenizer's \`[CLS]\`/\`[SEP]\` (a constant +2 per candidate if included)."
} > "$OUTDIR/SUMMARY.md"

echo "llm-token-court: wrote $OUTDIR/receipt.json, $OUTDIR/SUMMARY.md, $OUTDIR/commands.txt, $OUTDIR/raw/" >&2
cat "$RAW/verdict.json"
