#!/usr/bin/env bash
# Phase-12.12 — the mixed-format **LLM working-set court**.
#
# For the same pre-registered question per document, three representations can be
# handed to a language model:
#
#   B0  the whole extracted document   (Poppler for PDF; the named DOCX/EPUB
#                                       stdlib profiles for DOCX/EPUB);
#   B1  the local extract              (page-local Poppler; the first story /
#                                       spine paragraph block);
#   V   the VOLE observation payload   (`observe --page/--block --kind text`).
#
# This court reports, per document: UTF-8 bytes, **tokens under the pinned,
# named tokenizer** (bert-base-uncased, WordPiece vocab 30522, loaded by the
# hash-verified `tokenizers==0.20.3` runtime from the vendored asset; the SHA-256
# is checked at court time), the `context_waste_ratio` (B0 bytes / V bytes) and
# its token-space counterpart, and the win/tie/loss verdict of V vs B1 and B0.
#
# No token number is reported without the tokenizer that produced it; a smaller V
# is a working-set measurement, never a text-quality claim.
#
# Runs inside the pinned `llm-workingset` service. Usage:
#   bash tools/phase12-llm-court.sh [OUTDIR]
set -u

cd /work
LC_ALL=C
export LC_ALL

OUTDIR=${1:-evidence/campaigns/$(date -u +%Y-%m-%d)-phase12-llm-$(git rev-parse --short HEAD 2>/dev/null || echo unknown)}
SCHEDULE=${SCHEDULE:-tools/fixtures/phase12-lifetime-schedule.json}
CORPUS=${CORPUS:-evidence/scratch/phase12-lifetime-corpus}
BIN=${VOLE_BIN:-./target/debug/vole-document}
BASE_PY=tools/fixtures/phase12-baseline.py

TOK_JSON=${LLM_TOKENIZER_JSON:-tools/tokenizers/bert-base-uncased.tokenizer.json}
TOK_SHA=${LLM_TOKENIZER_SHA:-tools/tokenizers/bert-base-uncased.tokenizer.json.sha256}
TOK_NAME=${LLM_TOKENIZER_NAME:-bert-base-uncased}
TOK_PY=${LLM_TOKENIZE_PY:-tools/llm-tokenize.py}

for f in "$SCHEDULE" "$CORPUS/ground_truth.json" "$TOK_JSON" "$TOK_SHA" "$TOK_PY"; do
  [ -f "$f" ] || { echo "phase12-llm-court: missing $f" >&2; exit 2; }
done

if [ ! -x "$BIN" ] || ! "$BIN" --help 2>&1 | grep -q 'field-ingest'; then
  echo "phase12-llm-court: building the all-features binary" >&2
  cargo build --locked --all-features 1>&2
fi

mkdir -p "$OUTDIR/raw"
RAW="$OUTDIR/raw"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

EXPECT_SHA=$(cut -d' ' -f1 < "$TOK_SHA")

# --- tokenizer identity (verified from the asset before any count) -----------
TOK_ID=$(python3 "$TOK_PY" --tokenizer "$TOK_JSON" --expect-sha256 "$EXPECT_SHA" --name "$TOK_NAME")
TOK_OK=$(printf '%s' "$TOK_ID" | jq -r '.ok')
if [ "$TOK_OK" != true ]; then
  echo "phase12-llm-court: tokenizer identity check failed:" >&2
  printf '%s\n' "$TOK_ID" >&2
  exit 5
fi
printf '%s' "$TOK_ID" > "$RAW/tokenizer.json"

# --- environment -------------------------------------------------------------
RUSTC_V=$(rustc --version 2>/dev/null | sed 's/^rustc //' || echo unknown)
CARGO_V=$(cargo --version 2>/dev/null | sed 's/^cargo //' || echo unknown)
PY_V=$(python3 --version 2>&1 | sed 's/^Python //')
ARCH=$(uname -m)
COMMIT=$(git rev-parse HEAD 2>/dev/null || echo unknown)
COMMIT_SHORT=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
DIRTY=$(git status --porcelain 2>/dev/null | head -40 | tr '\n' ';' || echo unknown)
LOCK_SHA=$(sha256sum Cargo.lock 2>/dev/null | cut -d' ' -f1 || echo unknown)
BASE_STABLE=$(sed -n 's/^ARG BASE_STABLE=//p' Dockerfile 2>/dev/null | head -1)
IMAGE_ID=${HOST_IMAGE_ID:-unrecorded}
POPPLER_V=$(pdftotext -v 2>&1 | head -1 | sed 's/^pdftotext version //')
JQ_V=$(jq --version)

jq -n \
  --arg commit "$COMMIT" --arg commit_short "$COMMIT_SHORT" --arg dirty "$DIRTY" \
  --arg lock_sha "$LOCK_SHA" --arg arch "$ARCH" \
  --arg rustc "$RUSTC_V" --arg cargo "$CARGO_V" --arg python "$PY_V" \
  --arg base "$BASE_STABLE" --arg image_id "$IMAGE_ID" \
  --arg poppler "$POPPLER_V" --arg jq "$JQ_V" --arg bin "$BIN" \
  --slurpfile tok "$RAW/tokenizer.json" \
  '{git:{commit:$commit,commit_short:$commit_short,dirty:$dirty},
    arch:$arch,cargo_lock_sha256:$lock_sha,
    service_image:"vole-document/llm-workingset:1.99.0", llm_workingset_image_id:$image_id,
    toolchain:{rustc:$rustc,cargo:$cargo,python:$python},
    base_image:$base,
    oracles:{poppler_pdftotext:$poppler,jq:$jq},
    env_affecting_semantics:{LC_ALL:"C",VOLE_BIN:$bin},
    tokenizer:$tok[0].tokenizer}' > "$RAW/environment.json"

# --- questions ---------------------------------------------------------------
# Pre-registered per format; the same question is asked of every system.
#   pdf   "what is on page 1?"
#   docx  "what is the first body paragraph (block 1)?"
#   epub  "what is the first spine paragraph (block 1)?"
: > "$RAW/cases.jsonl"
: > "$RAW/assertions.tsv"

DOCS=$(jq -r '.documents[].name' "$SCHEDULE")
[ -n "${LLM_DOCS:-}" ] && DOCS=$LLM_DOCS

for name in $DOCS; do
  fmt=$(jq -r --arg n "$name" '.documents[]|select(.name==$n).format' "$SCHEDULE")
  src="$CORPUS/$name"
  d="$WORK/$name"; mkdir -p "$d/store"
  echo "phase12-llm-court: $name fmt=$fmt" >&2

  # ingest
  "$BIN" encode --force raw "$src" "$d/d.voldoc" > "$RAW/$name.encode.json" 2>/dev/null
  "$BIN" field-ingest "$d/d.voldoc" --store "$d/store" > "$RAW/$name.ingest.json" 2>/dev/null
  field=$(jq -r '.field' "$RAW/$name.ingest.json")

  # B0 / B1 / V payloads
  case "$fmt" in
    pdf)
      question="what is on page 1?"
      pdftotext "$src" "$d/b0.txt" 2>/dev/null || : > "$d/b0.txt"
      pdftotext -f 1 -l 1 "$src" "$d/b1.txt" 2>/dev/null || : > "$d/b1.txt"
      "$BIN" observe --store "$d/store" --field "$field" --page 1 --kind text > "$d/v.json" 2>/dev/null || echo '{}' > "$d/v.json"
      b0_profile="poppler-pdftotext-whole"; b1_profile="poppler-pdftotext-f1-l1"
      ;;
    docx)
      question="what is the first body paragraph (block 1)?"
      python3 "$BASE_PY" query --format docx --source "$src" --case doc-text --out "$d/b0.json" >/dev/null 2>&1 || echo '{}' > "$d/b0.json"
      jq -j '.text' "$d/b0.json" > "$d/b0.txt" 2>/dev/null || : > "$d/b0.txt"
      python3 "$BASE_PY" query --format docx --source "$src" --case block --arg 1 --out "$d/b1.json" >/dev/null 2>&1 || echo '{}' > "$d/b1.json"
      jq -j '.text' "$d/b1.json" > "$d/b1.txt" 2>/dev/null || : > "$d/b1.txt"
      "$BIN" observe --store "$d/store" --field "$field" --block 1 --kind text > "$d/v.json" 2>/dev/null || echo '{}' > "$d/v.json"
      b0_profile="docx-story-txt-v1 (stdlib zipfile+ElementTree, all blocks)"; b1_profile="docx-block-txt-v1 (first body paragraph block)"
      ;;
    epub)
      question="what is the first spine paragraph (block 1)?"
      python3 "$BASE_PY" query --format epub --source "$src" --case doc-text --out "$d/b0.json" >/dev/null 2>&1 || echo '{}' > "$d/b0.json"
      jq -j '.text' "$d/b0.json" > "$d/b0.txt" 2>/dev/null || : > "$d/b0.txt"
      python3 "$BASE_PY" query --format epub --source "$src" --case block --arg 1 --out "$d/b1.json" >/dev/null 2>&1 || echo '{}' > "$d/b1.json"
      jq -j '.text' "$d/b1.json" > "$d/b1.txt" 2>/dev/null || : > "$d/b1.txt"
      "$BIN" observe --store "$d/store" --field "$field" --block 1 --kind text > "$d/v.json" 2>/dev/null || echo '{}' > "$d/v.json"
      b0_profile="epub-spine-txt-v1 (stdlib zipfile+ElementTree, all blocks)"; b1_profile="epub-block-txt-v1 (first spine paragraph block)"
      ;;
  esac
  jq -j '.text' "$d/v.json" > "$d/v.txt" 2>/dev/null || : > "$d/v.txt"

  # tokens
  TOK=$(python3 "$TOK_PY" --tokenizer "$TOK_JSON" --expect-sha256 "$EXPECT_SHA" --name "$TOK_NAME" \
        --files "$d/b0.txt" "$d/b1.txt" "$d/v.txt")
  [ "$(printf '%s' "$TOK" | jq -r '.ok')" = true ] || { echo "tokenizer failed for $name" >&2; printf '%s\n' "$TOK" >&2; exit 5; }
  b0_b=$(printf '%s' "$TOK" | jq -r '.counts[0].bytes'); b0_t=$(printf '%s' "$TOK" | jq -r '.counts[0].tokens')
  b1_b=$(printf '%s' "$TOK" | jq -r '.counts[1].bytes'); b1_t=$(printf '%s' "$TOK" | jq -r '.counts[1].tokens')
  v_b=$(printf '%s' "$TOK" | jq -r '.counts[2].bytes');  v_t=$(printf '%s' "$TOK" | jq -r '.counts[2].tokens')

  jq -n \
    --arg name "$name" --arg fmt "$fmt" --arg question "$question" \
    --arg b0p "$b0_profile" --arg b1p "$b1_profile" \
    --argjson b0b "$b0_b" --argjson b0t "$b0_t" \
    --argjson b1b "$b1_b" --argjson b1t "$b1_t" \
    --argjson vb "$v_b" --argjson vt "$v_t" \
    --slurpfile v "$d/v.json" \
    '{name:$name,format:$fmt,question:$question,
      baselines:{
        B0_whole_document:{profile:$b0p,bytes:$b0b,tokens:$b0t},
        B1_local_extract:{profile:$b1p,bytes:$b1b,tokens:$b1t},
        V_vole_observation:{repr:$v[0].selector,bytes:$vb,tokens:$vt,
                            provenance:$v[0].provenance}} }' >> "$RAW/cases.jsonl"

  printf '%s\t%s\t%s\t%s\t%s\n' "$name" "$fmt" \
    "B0:${b0_t}" "B1:${b1_t}" "V:${v_t}" >> "$RAW/assertions.tsv"
done

jq -s '.' "$RAW/cases.jsonl" > "$RAW/cases.json"

# --- verdicts ----------------------------------------------------------------
jq -n --slurpfile c "$RAW/cases.json" '
  def vd($v;$b): if $v < $b then "win" elif $v == $b then "tie" else "loss" end;
  ($c[0] | map({
     name:.name, format:.format,
     B0_bytes:.baselines.B0_whole_document.bytes, B1_bytes:.baselines.B1_local_extract.bytes, V_bytes:.baselines.V_vole_observation.bytes,
     B0_tokens:.baselines.B0_whole_document.tokens, B1_tokens:.baselines.B1_local_extract.tokens, V_tokens:.baselines.V_vole_observation.tokens,
     context_waste_ratio: (if .baselines.V_vole_observation.bytes>0 then (.baselines.B0_whole_document.bytes / .baselines.V_vole_observation.bytes) else null end),
     token_context_waste_ratio: (if .baselines.V_vole_observation.tokens>0 then (.baselines.B0_whole_document.tokens / .baselines.V_vole_observation.tokens) else null end),
     verdict_vs_B1_tokens: vd(.baselines.V_vole_observation.tokens; .baselines.B1_local_extract.tokens),
     verdict_vs_B0_tokens: vd(.baselines.V_vole_observation.tokens; .baselines.B0_whole_document.tokens),
     losses: [
       if .baselines.V_vole_observation.bytes > .baselines.B1_local_extract.bytes then "V_bytes_larger_than_B1" else empty end,
       if .baselines.V_vole_observation.bytes > .baselines.B0_whole_document.bytes then "V_bytes_larger_than_B0" else empty end,
       if .baselines.V_vole_observation.tokens > .baselines.B1_local_extract.tokens then "V_tokens_exceed_B1" else empty end,
       if .baselines.V_vole_observation.tokens > .baselines.B0_whole_document.tokens then "V_tokens_exceed_B0" else empty end ] })) as $rows
  | { per_case:$rows,
      vs_B1:{win:($rows|map(select(.verdict_vs_B1_tokens=="win"))|length), tie:($rows|map(select(.verdict_vs_B1_tokens=="tie"))|length), loss:($rows|map(select(.verdict_vs_B1_tokens=="loss"))|length)},
      vs_B0:{win:($rows|map(select(.verdict_vs_B0_tokens=="win"))|length), tie:($rows|map(select(.verdict_vs_B0_tokens=="tie"))|length), loss:($rows|map(select(.verdict_vs_B0_tokens=="loss"))|length)},
      any_token_reduction_vs_B1: (($rows|map(select(.verdict_vs_B1_tokens=="win"))|length)>0),
      all_token_reduction_vs_B1: (($rows|map(select(.verdict_vs_B1_tokens!="win"))|length)==0) }' > "$RAW/verdict.json"

# --- receipt + summary + commands --------------------------------------------
jq -n \
  --slurpfile env "$RAW/environment.json" \
  --slurpfile cases "$RAW/cases.json" \
  --slurpfile verdict "$RAW/verdict.json" \
  '{campaign:"phase12-llm-workingset",
    phase:"Phase 12.12 — mixed PDF/DOCX/EPUB LLM working-set court (pinned tokenizer)",
    environment:$env[0], tokenizer:$env[0].tokenizer,
    cases:$cases[0], verdict:$verdict[0],
    claim_scope:"Token counts are bert-base-uncased (WordPiece, vocab 30522) under the pinned tokenizers 0.20.3 runtime; they are not a claim about any other model. A smaller V is a working-set measurement, not a text-quality claim.",
    accounting_note:"B0/B1 are independent whole-document / local extracts (Poppler for PDF; the named stdlib DOCX/EPUB profiles). V is the VOLE observe payload (a bounded projection, not Poppler reading order)."}' \
  > "$OUTDIR/receipt.json"

{
  echo "# Phase 12.12 — mixed-format LLM working-set court"
  echo
  echo "Generated by \`tools/phase12-llm-court.sh\` inside the pinned \`llm-workingset\` service."
  echo "For the same pre-registered question per document, three representations are compared:"
  echo "B0 (whole extracted document), B1 (local page/story/spine extract) and V (the VOLE"
  echo "observation payload). Bytes are UTF-8; tokens are measured under the pinned, named"
  echo "tokenizer."
  echo
  jq -r '"Commit: `\(.environment.git.commit_short)` (dirty: \(.environment.git.dirty))  \n" +
    "Image `vole-document/llm-workingset:1.99.0` (id `\(.environment.llm_workingset_image_id)`), base `\(.environment.base_image)`.  \n" +
    "rustc `\(.environment.toolchain.rustc)`, python `\(.environment.toolchain.python)`; Cargo.lock sha256 `\(.environment.cargo_lock_sha256)`; arch `\(.environment.arch)`.  \n" +
    "Poppler \(.environment.oracles.poppler_pdftotext)."' "$OUTDIR/receipt.json"
  echo
  echo "## Tokenizer (pinned, offline, verified at court time)"
  echo
  jq -r '.tokenizer | "- name `\(.name)`, implementation v\(.implementation_version), asset `\(.asset_path)`  \n- SHA-256 `\(.asset_sha256)` (verified: `\(.asset_verified)`), add_special_tokens=\(.add_special_tokens), vocab \(.config.vocab_size)."' "$OUTDIR/receipt.json"
  echo
  echo "## Per-document working set (bytes and tokens)"
  echo
  echo "| document | fmt | B0 bytes | B1 bytes | V bytes | B0 tok | B1 tok | V tok | V vs B1 | V vs B0 | ctx_waste B0/V | ctx_waste tok |"
  echo "|---|---|---:|---:|---:|---:|---:|---:|---|---|---:|---:|"
  jq -r '.verdict.per_case[] | "| \(.name) | \(.format) | \(.B0_bytes) | \(.B1_bytes) | \(.V_bytes) | \(.B0_tokens) | \(.B1_tokens) | \(.V_tokens) | \(.verdict_vs_B1_tokens) | \(.verdict_vs_B0_tokens) | \((.context_waste_ratio*100|round/100)) | \((.token_context_waste_ratio*100|round/100)) |"' "$OUTDIR/receipt.json"
  echo
  echo "## Verdict (tokens, \`bert-base-uncased\`)"
  echo
  echo "V vs local extract B1: **$(jq -r '"\(.vs_B1.win) win / \(.vs_B1.tie) tie / \(.vs_B1.loss) loss"' "$RAW/verdict.json")**."
  echo "V vs whole-document B0: **$(jq -r '"\(.vs_B0.win) win / \(.vs_B0.tie) tie / \(.vs_B0.loss) loss"' "$RAW/verdict.json")**."
  echo
  echo "Any token reduction over B1: \`$(jq -r '.any_token_reduction_vs_B1' "$RAW/verdict.json")\`; on **all** tested questions: \`$(jq -r '.all_token_reduction_vs_B1' "$RAW/verdict.json")\`."
  echo
  echo "## All recorded losses"
  echo
  jq -r '.verdict.per_case[] | select((.losses|length)>0) | "- **\(.name)** (\(.format)): V \(.V_tokens) tok vs B1 \(.B1_tokens) / B0 \(.B0_tokens) — \(.losses|join(", "))"' "$OUTDIR/receipt.json"
  echo
  echo "- Token counts are tokenizer-specific (\`bert-base-uncased\`, WordPiece, vocab 30522). No \`tokens saved\` is claimed without naming this tokenizer."
  echo "- V is VOLE's bounded text projection (\`basis=heuristic\` for PDF), never Poppler reading order; a smaller V is a *working-set* measurement."
  echo "- \`add_special_tokens=false\`: the count excludes \`[CLS]\`/\`[SEP]\` (+2 per candidate if included)."
} > "$OUTDIR/SUMMARY.md"

cat > "$OUTDIR/commands.txt" <<EOF
# Phase 12.12 LLM working-set court — exact commands
# Commit under test: $COMMIT (branch $(git rev-parse --abbrev-ref HEAD 2>/dev/null)); dirty: $DIRTY
# Tokenizer: $TOK_NAME; asset sha256: $EXPECT_SHA
# All commands run inside the pinned, capped llm-workingset image; nothing on the host.

docker compose build llm-workingset
docker compose run --rm --no-TTY dev sh -c 'cargo build --all-features --locked'
docker compose run --rm --no-TTY -e HOST_IMAGE_ID=<llm-workingset image id> llm-workingset \\
    bash tools/phase12-llm-court.sh $OUTDIR

# Per document:
#   \$BIN encode SRC d.voldoc ; \$BIN field-ingest d.voldoc --store store
#   pdf : pdftotext SRC b0.txt ; pdftotext -f 1 -l 1 SRC b1.txt ; \$BIN observe --page 1 --kind text
#   docx: python3 tools/fixtures/phase12-baseline.py query --case doc-text  (B0)
#         python3 tools/fixtures/phase12-baseline.py query --case block --arg 1 (B1)
#         \$BIN observe --block 1 --kind text (V)
#   epub: same as docx with --format epub
#   python3 tools/llm-tokenize.py --tokenizer $TOK_JSON --expect-sha256 $EXPECT_SHA \\
#       --name $TOK_NAME --files b0.txt b1.txt v.txt
EOF

echo "phase12-llm-court: wrote $OUTDIR" >&2
