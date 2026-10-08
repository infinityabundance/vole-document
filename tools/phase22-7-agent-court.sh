#!/usr/bin/env bash
# Phase 22.7 (review item P6) — the document-agent economic court.
#
# ## Question
#
#   Across the WHOLE document-agent workload, what is the cost per CORRECT,
#   GROUNDED task, and does VOLE cost >=2x less than a conventional tuned
#   baseline that is given the SAME task-level optimizations?
#
# ## Method (frozen)
#
#   A deterministic *scripted* document-inspection agent runs the same frozen
#   multi-step workflow per task — (1) locate a fact by search, (2) read the
#   containing unit, (3) answer with a source span — against two backends on the
#   same documents:
#
#     VOLE      field-build (runtime/RAW) + find + observe / observe-batch
#     baseline  a conventional per-unit extraction (Poppler text lines for PDF;
#               the OPC XML for DOCX/EPUB) loaded into SQLite with byte spans,
#               searched with LIKE, read by row
#
#   Correctness is an exact (whitespace-normalised) match of the agent's answer
#   to a FROZEN expected unit text derived from the document itself; grounding
#   requires a non-null source span whose bytes, re-materialised from the
#   backend's own source-of-truth, contain the anchor. Answer correctness is
#   fixed BEFORE any token/cost claim (a cheaper wrong answer is not a win).
#
#   There is NO real LLM. The model-input measure is the token count of the
#   EXACT transcript the scripted agent would feed forward (search result + read
#   result), tokenised with the pinned offline tokenizer. If the pinned runtime
#   is absent from the lane, a clearly-labelled deterministic proxy is used and
#   the receipt says so.
#
# ## Lane (never the host)
#
#   # The tokenizer lives in `llm-workingset` (pinned offline tokenizers 0.20.3):
#   docker compose run --rm --no-TTY llm-workingset bash tools/phase22-7-agent-court.sh
#   # The requested default lane also runs it (uses the labelled token proxy):
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase22-7-agent-court.sh
#   # Bounded / explicit population:
#   docker compose run --rm --no-TTY llm-workingset sh -c \
#     'LIMIT=3 bash tools/phase22-7-agent-court.sh'
#   docker compose run --rm --no-TTY llm-workingset sh -c \
#     'IDS="nist-docx-0005 nist-epub-0008" bash tools/phase22-7-agent-court.sh'
set -uo pipefail

cd /work
export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
SHAFULL=$(git rev-parse HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=${CAMPAIGN:-evidence/campaigns/${STAMP}-phase22-7-agent-${SHA}${TAG:+-$TAG}}
RAW=$CAMPAIGN/raw
WORK=${WORK:-evidence/scratch/phase22-7-agent-work}
mkdir -p "$RAW/build" "$WORK"

HARNESS=${HARNESS:-tools/fixtures/phase22-7-agent.py}
LIMIT=${LIMIT:-0}
IDS=${IDS:-}
TASKS_PER_DOC=${TASKS_PER_DOC:-3}
CORPUS=${CORPUS:-real100-v1/documents}
PROFILE=${PROFILE:-release}
TOK_JSON=${LLM_TOKENIZER_JSON:-tools/tokenizers/bert-base-uncased.tokenizer.json}
TOK_SHA_FILE=${LLM_TOKENIZER_SHA:-tools/tokenizers/bert-base-uncased.tokenizer.json.sha256}
TOK_NAME=${LLM_TOKENIZER_NAME:-bert-base-uncased}

case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-${VOLE_BIN:-target/debug/vole-document}} ;;
esac

echo "== phase22.7 document-agent economic court ==" >&2
echo "-- building ($PROFILE) binary" >&2
# shellcheck disable=SC2086
if ! cargo build $BUILD_ARGS >&2; then
    echo "phase22-7-agent: build FAILED ($BUILD_ARGS); refusing to measure" >&2
    exit 1
fi
[ -x "$BIN" ] || { echo "phase22-7-agent: $BIN missing after build" >&2; exit 1; }

# ---------------------------------------------------------------------------
# Frozen document population: >=3 formats, >=6 docs, mixed size classes.
# (pdf/docx/epub from real100-v1; small enough that a full run is bounded.)
# ---------------------------------------------------------------------------
DOCLIST="$RAW/docs.tsv"
cat > "$DOCLIST" <<'DOCS'
nist-pdf-0001	pdf	real100-v1/documents/nist/pdf/nist-pdf-0001.pdf
nist-pdf-0016	pdf	real100-v1/documents/nist/pdf/nist-pdf-0016.pdf
nist-pdf-0004	pdf	real100-v1/documents/nist/pdf/nist-pdf-0004.pdf
nist-docx-0001	docx	real100-v1/documents/nist/docx/nist-docx-0001.docx
nist-docx-0005	docx	real100-v1/documents/nist/docx/nist-docx-0005.docx
nist-docx-0013	docx	real100-v1/documents/nist/docx/nist-docx-0013.docx
nist-epub-0001	epub	real100-v1/documents/nist/epub/nist-epub-0001.epub
nist-epub-0003	epub	real100-v1/documents/nist/epub/nist-epub-0003.epub
nist-epub-0008	epub	real100-v1/documents/nist/epub/nist-epub-0008.epub
DOCS

if [ -n "$IDS" ]; then
    printf '%s\n' $IDS | LC_ALL=C sort > "$RAW/ids.txt"
    awk -F'\t' 'NR==FNR{want[$1]=1;next} $1 in want' "$RAW/ids.txt" "$DOCLIST" > "$RAW/docs.filtered.tsv"
    mv "$RAW/docs.filtered.tsv" "$DOCLIST"
fi
if [ "$LIMIT" -gt 0 ]; then
    head -n "$LIMIT" "$DOCLIST" > "$RAW/docs.limited.tsv"
    mv "$RAW/docs.limited.tsv" "$DOCLIST"
fi

# ---------------------------------------------------------------------------
# Tokenizer identity (recorded whether or not the runtime is present).
# ---------------------------------------------------------------------------
TOK_SHA="none"
[ -f "$TOK_SHA_FILE" ] && TOK_SHA=$(cut -d' ' -f1 < "$TOK_SHA_FILE")
TOK_PRESENT=no
python3 -c 'import tokenizers' 2>/dev/null && TOK_PRESENT=yes

# ---------------------------------------------------------------------------
# Run the court.
# ---------------------------------------------------------------------------
echo "-- docs:" >&2; cat "$DOCLIST" >&2
python3 "$HARNESS" run \
    --outdir "$CAMPAIGN" --docs "$DOCLIST" --vbin "$BIN" \
    --tokenizer "$TOK_JSON" --expect-sha256 "$TOK_SHA" --tokname "$TOK_NAME" \
    --work "$WORK" --tasks-per-doc "$TASKS_PER_DOC" \
    || { echo "phase22-7-agent: harness run FAILED" >&2; exit 2; }

# ---------------------------------------------------------------------------
# Provenance
# ---------------------------------------------------------------------------
BASE_IMAGE=$(grep -m1 '^ARG BASE_STABLE=' Dockerfile | sed 's/^ARG BASE_STABLE=//')
CARGO_LOCK_SHA=$(sha256sum Cargo.lock 2>/dev/null | awk '{print $1}')
BIN_SHA=$(sha256sum "$BIN" 2>/dev/null | awk '{print $1}')
MANIFEST_SHA=$( [ -f real100-v1/manifest.tsv ] && sha256sum real100-v1/manifest.tsv | awk '{print $1}' || echo none)
RUSTC=$(rustc --version 2>/dev/null || echo unknown)
CARGO=$(cargo --version 2>/dev/null || echo unknown)
SQLITE=$(python3 -c 'import sqlite3;print(sqlite3.sqlite_version)' 2>/dev/null || echo none)
PYVER=$(python3 --version 2>/dev/null || echo unknown)
ARCH=$(uname -m)
UTC=$(date -u +%Y-%m-%dT%H:%M:%SZ)
DIRTY=$(git --no-optional-locks status --short 2>/dev/null | tr '\n' ';')
SERVICE=${SERVICE:-${COMPOSE_SERVICE:-llm-workingset}}
POPPLER=$(pdftotext -v 2>&1 | head -1 | sed 's/^pdftotext version //')
TOK_VER=$(python3 -c 'import tokenizers;print(tokenizers.__version__)' 2>/dev/null || echo absent)

cat >"$CAMPAIGN/environment.json" <<JSON
{
  "campaign": "$CAMPAIGN",
  "phase": "22.7 (P6) — document-agent economic court",
  "utc": "$UTC",
  "git_commit": "$SHAFULL",
  "git_dirty": "$DIRTY",
  "arch": "$ARCH",
  "service": "$SERVICE",
  "base_image": "$BASE_IMAGE",
  "rustc": "$RUSTC",
  "cargo": "$CARGO",
  "sqlite3": "$SQLITE",
  "python": "$PYVER",
  "poppler_pdftotext": "$POPPLER",
  "cargo_lock_sha256": "$CARGO_LOCK_SHA",
  "manifest": "real100-v1/manifest.tsv",
  "manifest_sha256": "$MANIFEST_SHA",
  "bin": "$BIN",
  "bin_sha256": "$BIN_SHA",
  "profile": "$PROFILE",
  "corpus": "$CORPUS",
  "tokenizer": {
    "asset_path": "$TOK_JSON",
    "asset_sha256_expected": "$TOK_SHA",
    "runtime_present": "$TOK_PRESENT",
    "runtime_version": "$TOK_VER",
    "name": "$TOK_NAME"
  },
  "env_affecting_semantics": {"LC_ALL": "C", "PYTHONDONTWRITEBYTECODE": "1"}
}
JSON

cat >"$CAMPAIGN/receipt.json" <<JSON
{
  "campaign": "$CAMPAIGN",
  "phase": "22.7 (P6) — document-agent economic court",
  "utc": "$UTC",
  "measured_commit": "$SHAFULL",
  "tree_state": "$DIRTY",
  "service": "$SERVICE",
  "profile": "$PROFILE",
  "bin": "$BIN",
  "bin_sha256": "$BIN_SHA",
  "base_image": "$BASE_IMAGE",
  "rustc": "$RUSTC",
  "cargo": "$CARGO",
  "sqlite3": "$SQLITE",
  "python": "$PYVER",
  "harness": "$HARNESS",
  "court": "tools/phase22-7-agent-court.sh",
  "gate": ">=2x lower cost per correct grounded task",
  "workflow": "locate (search) -> read containing unit -> answer with source span; same frozen tasks for both backends",
  "correctness": "normalised exact match of the agent answer to the frozen expected unit text (derived from the document)",
  "grounding": "non-null source span whose bytes (re-materialised from the backend's own source-of-truth) contain the anchor",
  "model_input": "token count of the exact transcript fed forward, counted with the pinned bert-base-uncased tokenizer when available; else a labelled deterministic proxy",
  "is_llm": "NO real LLM — a deterministic scripted agent; token counts are a stand-in for model input",
  "rc_codes": "0 ok; 2 harness failure; 9 not attempted; 124 timeout; 137 SIGKILL/OOM"
}
JSON

cat >"$CAMPAIGN/commands.txt" <<CMDS
# Phase 22.7 (P6) — document-agent economic court
# Commit under test: $SHAFULL (branch $(git rev-parse --abbrev-ref HEAD 2>/dev/null)); dirty: $DIRTY
# Service: $SERVICE; base $BASE_IMAGE; rustc $RUSTC; cargo $CARGO; python $PYVER; sqlite $SQLITE; poppler $POPPLER
# Tokenizer asset: $TOK_JSON sha256=$TOK_SHA (expected $TOK_SHA); runtime present=$TOK_PRESENT ($TOK_VER)
# ALL commands run inside the pinned, capped compose service; nothing on the host.
#   docker compose run --rm --no-TTY llm-workingset bash tools/phase22-7-agent-court.sh
# The court, inside the container:
#   cargo build $BUILD_ARGS            # release, all-features, locked
#   python3 $HARNESS run --outdir $CAMPAIGN --docs $RAW/docs.tsv --vbin $BIN \\
#       --tokenizer $TOK_JSON --expect-sha256 $TOK_SHA --tokname $TOK_NAME \\
#       --work $WORK --tasks-per-doc $TASKS_PER_DOC
# Per document:
#   \$BIN field-build SRC --store DIR/vstore --profile runtime --packed   (VOLE doc-prep)
#   \$BIN observe-batch --store DIR/vstore --field FIELD --packed --requests REQ   (VOLE locate: --text A --kind text)
#   \$BIN observe-batch ... --requests REQ   (VOLE read: --page P | --block K --kind text; K from a
#           deterministic --block metadata enumeration mapping the search coordinate to the block ordinal)
#   python: reference extraction (pdftotext -layout / OPC XML) + SQLite units; locate = case-sensitive
#           instr(text,?) (VOLE find is case-sensitive too); read = SELECT text,span by unit_id   (baseline)
CMDS

# ---------------------------------------------------------------------------
# Human-readable provenance / scope appended to SUMMARY.md
# ---------------------------------------------------------------------------
{
  echo "## Method and provenance"
  echo
  echo "- Commit under test: \`$SHAFULL\` (branch \`$(git rev-parse --abbrev-ref HEAD 2>/dev/null)\`); dirty: \`$DIRTY\`."
  echo "- Service: \`$SERVICE\`; base image \`$BASE_IMAGE\`; \`$RUSTC\`; \`$CARGO\`; \`$PYVER\`; sqlite \`$SQLITE\`; poppler \`$POPPLER\`."
  echo "- Tokenizer: \`$TOK_JSON\` sha256 \`$TOK_SHA\`; runtime present: \`$TOK_PRESENT\` (\`$TOK_VER\`)."
  echo
  echo "## What is REAL vs STAND-IN"
  echo
  echo "- **Real:** the VOLE binary, \`field-build\`/\`find\`/\`observe-batch\`; the baseline Poppler/SQLite"
  echo "  pipeline; the documents; the reference extraction; the correctness and grounding checks;"
  echo "  build/query wall, storage bytes and peak RSS."
  echo "- **Stand-in:** there is **no LLM**. The \"agent\" is a deterministic script; the number of"
  echo "  model-input tokens is the token count of the exact transcript the script feeds forward,"
  echo "  under the pinned tokenizer (or the labelled proxy if the runtime is absent). Real-model"
  echo "  behaviour (tool-choice, retries, correctness of a generated answer) is NOT measured."
  echo "- **Stated prices** for the monetary column are illustrative (see summary.json); they are not"
  echo "  any vendor's list price. The physical cost vector is reported beside them."
  echo
  echo "## What this does NOT prove"
  echo
  echo "- It is not a result from a real LLM agent; it does not measure generated-answer quality."
  echo "- Per-format generality is limited to pdf/docx/epub and the frozen sample; unresolved where"
  echo "  the sample cannot support a claim."
  echo "- It does not reopen the falsified cross-document dedup / adaptive-promotion results."
  echo
} >> "$CAMPAIGN/SUMMARY.md"

echo "-- done — $CAMPAIGN" >&2
echo "campaign=$CAMPAIGN"
python3 - "$CAMPAIGN/summary.json" <<'PY'
import json,sys
s=json.load(open(sys.argv[1]))
p=s["pooled"]
print("tokenizer:", s["tokenizer"]["kind"], s["tokenizer"]["name"])
print("pooled VOLE  : correct %d/%d grounded %d/%d corr+grnd %d/%d cost/cg %s" % (
  p["vole"]["correct"],p["vole"]["tasks"],p["vole"]["grounded"],p["vole"]["tasks"],
  p["vole"]["correct_grounded"],p["vole"]["tasks"],p["vole"]["cost_per_correct_grounded"]))
print("pooled BASE  : correct %d/%d grounded %d/%d corr+grnd %d/%d cost/cg %s" % (
  p["baseline"]["correct"],p["baseline"]["tasks"],p["baseline"]["grounded"],p["baseline"]["tasks"],
  p["baseline"]["correct_grounded"],p["baseline"]["tasks"],p["baseline"]["cost_per_correct_grounded"]))
print("pooled ratio VOLE/base:", p["cost_ratio_vole_over_baseline"], "gate_met:", s["gate_met"])
for r in s["regions"]:
    print("  region %-6s ratio %s verdict %s" % (r["region"], r["cost_ratio_vole_over_baseline"], r["verdict"]))
PY
