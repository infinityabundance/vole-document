#!/usr/bin/env bash
# ADR-0060 — permanent cross-field identity court.
#
# Durable adversarial court for the invariant "an observation's output is a pure
# function of the field's content id (NodeId), independent of the store it lives in
# and of what else the store contains". It FAILS on any cross-field aliasing or
# wrong-document answer. Cases (details in tools/fixtures/identity-court.py):
#
#   interleave  — PDF + DOCX/XLSX/PPTX/ODS/ODP + JSON + YAML built and observed
#                 interleaved in ONE store; each observation must equal the same
#                 document built alone, and field ids must not depend on store
#                 contents;
#   programs    — identical source via two reconstruction programs (direct
#                 `field-build --profile runtime` vs searching `encode` ->
#                 `field-ingest`) in one store: identical observations, no aliasing;
#   edit        — `field-edit` one PDF page, then re-observe: the original field is
#                 unchanged and NO other field is affected;
#   cache-clear — `cache --clear` then re-observe: the same answers;
#   crash       — SIGKILL builds mid-way, reopen the store: no wrong-document bytes
#                 are ever served and a post-crash rebuild is exact.
#
# Runs in the pinned, hard-capped `doc-baseline` service (dev toolchain + python3
# + sqlite3); never the host:
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/identity-court.sh

set -uo pipefail
cd /work
export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1

BIN=${VOLE_BIN:-target/debug/vole-document}
if [ ! -x "$BIN" ]; then BIN=target/debug/vole-document; fi
SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN="evidence/campaigns/${STAMP}-identity-${SHA}"
RAW="$CAMPAIGN/raw"
WORK="evidence/scratch/identity"

rm -rf "$CAMPAIGN"
mkdir -p "$RAW" "$WORK/fixtures"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2
[ -x "$BIN" ] || { echo "identity-court: $BIN missing; refusing to run" >&2; exit 1; }

echo "=== regenerate fixtures (stdlib only) ==="
python3 tools/fixtures/make-xlsx.py >/dev/null
python3 tools/fixtures/make-pptx.py >/dev/null
python3 tools/fixtures/make-ods.py >/dev/null
python3 tools/fixtures/make-odp.py >/dev/null
python3 tools/fixtures/make-json.py >/dev/null
python3 tools/fixtures/make-yaml.py >/dev/null
python3 tools/fixtures/make-docx.py >/dev/null
mkdir -p "$WORK/pdfs"
"$BIN" pdf-make-samples "$WORK/pdfs" > "$RAW/pdf_samples.json"

cp "$WORK/pdfs/classic.pdf"          "$WORK/fixtures/doc.pdf"
cp tools/fixtures/docx/basic.docx    "$WORK/fixtures/doc.docx"
cp tools/fixtures/xlsx/single.xlsx   "$WORK/fixtures/doc.xlsx"
cp tools/fixtures/pptx/basic.pptx    "$WORK/fixtures/doc.pptx"
cp tools/fixtures/ods/basic.ods      "$WORK/fixtures/doc.ods"
cp tools/fixtures/odp/basic.odp      "$WORK/fixtures/doc.odp"
cp tools/fixtures/json/basic.json    "$WORK/fixtures/doc.json"
cp tools/fixtures/yaml/anchors.yaml  "$WORK/fixtures/doc.yaml"

{
    printf 'fixture\tbytes\tsha256\n'
    for f in doc.pdf doc.docx doc.xlsx doc.pptx doc.ods doc.odp doc.json doc.yaml; do
        printf '%s\t%s\t%s\n' "$f" "$(wc -c < "$WORK/fixtures/$f" | tr -d ' ')" \
            "$(sha256sum "$WORK/fixtures/$f" | cut -d' ' -f1)"
    done
} | tee "$RAW/fixtures.tsv"

echo "=== run the court ==="
set +e
python3 tools/fixtures/identity-court.py \
    --bin "$BIN" --work "$WORK" --out "$RAW" --fixtures "$WORK/fixtures"
RC=$?
set -e
VERDICT=$(python3 -c "import json;print(json.load(open('$RAW/case.json'))['verdict'])")

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "ADR-0060 permanent cross-field identity court",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "measured_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "tree_state": "$(git status --porcelain | tr '\n' ' ' | sed 's/"/\\"/g')",
  "service": "doc-baseline",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "python": "$(python3 --version)",
  "arch": "$(uname -m)",
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "bin": "$BIN",
  "bin_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "driver_sha256": "$(sha256sum tools/fixtures/identity-court.py | cut -d' ' -f1)",
  "fixtures_tsv_sha256": "$(sha256sum "$RAW/fixtures.tsv" | cut -d' ' -f1)",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

# --- receipt.json -----------------------------------------------------------
python3 - "$RAW/case.json" "$CAMPAIGN/receipt.json" "$CAMPAIGN/environment.json" <<'PY'
import json, sys
case = json.load(open(sys.argv[1]))
env = json.load(open(sys.argv[3]))
receipt = {
    "campaign": env["campaign"],
    "phase": case["phase"],
    "invariant": case["invariant"],
    "verdict": case["verdict"],
    "fixtures": case["fixtures"],
    "cases": case["cases"],
    "checks": case["checks"],
    "failures": case["failures"],
    "notes": case["notes"],
    "environment": env,
}
json.dump(receipt, open(sys.argv[2], "w"), indent=2, sort_keys=True)
PY

# --- commands.txt -----------------------------------------------------------
cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY doc-baseline bash tools/identity-court.sh
# inside the court:
#   cargo build --locked --all-features
#   python3 tools/fixtures/make-{xlsx,pptx,ods,odp,json,yaml,docx}.py
#   target/debug/vole-document pdf-make-samples "$WORK/pdfs"
#   python3 tools/fixtures/identity-court.py --bin target/debug/vole-document \
#       --work "$WORK" --out "$RAW" --fixtures "$WORK/fixtures"
# The driver issues, per case:
#   interleave: field-build F --store <fresh>/F --profile runtime --packed
#               field-build F --store <shared> --profile runtime --packed
#               observe --store S --field HEX --packed (--metadata|--doc-text|--byte-range 0..64 [--page 1])
#   programs:   field-build pdf --store S --profile runtime --packed
#               encode pdf S/searching.voldoc ; field-ingest S/searching.voldoc --store S --packed
#               observe/materialize both ; build a second doc (json) in S (aliasing probe)
#   edit:       field-edit --store S --field Fpdf --page 1 --content edit.bin --packed
#               observe/materialize Fpdf and F' and every other field
#   cache-clear: cache --store S --clear ; re-observe every field
#   crash:      Popen field-build <large.pdf> --store S ... ; SIGKILL mid-way ;
#               re-materialize/observe the good field ; rebuild <large.pdf> exactly
EOF

# --- SUMMARY.md -------------------------------------------------------------
{
    echo "# ADR-0060 — permanent cross-field identity court"
    echo
    echo "**Invariant.** The output of a field observation is a **pure function of the"
    echo "field's content id (NodeId)** — independent of the store it lives in and of"
    echo "what else the store contains. The court FAILS on any cross-field aliasing or"
    echo "wrong-document answer."
    echo
    echo "**Verdict: \`$VERDICT\`**"
    echo
    echo "## Cases"
    echo
    echo "| case | what it asserts |"
    echo "|---|---|"
    echo "| interleave | PDF + DOCX/XLSX/PPTX/ODS/ODP + JSON + YAML built & observed interleaved in ONE store; every observation equals the same document built alone, and field ids do not depend on store contents |"
    echo "| programs | identical source via direct \`field-build --profile runtime\` vs searching \`encode\`/\`field-ingest\` sharing a store: identical observations, no aliasing of a second document |"
    echo "| edit | \`field-edit\` one PDF page: the original field is unchanged and NO other field is affected |"
    echo "| cache-clear | \`cache --clear\` then re-observe: the same answers |"
    echo "| crash | SIGKILL a build mid-way, reopen the store: no wrong-document bytes served, post-crash rebuild exact |"
    echo
    echo "## Checks"
    echo
    echo "| check | result |"
    echo "|---|---|"
    python3 - "$RAW/case.json" <<'PY'
import json, sys
c = json.load(open(sys.argv[1]))
for k, v in c["checks"].items():
    print("| %s | %s |" % (k, "PASS" if v else "FAIL"))
PY
    echo
    echo "## Notes"
    echo
    echo '```json'
    python3 -c "import json,sys;print(json.dumps(json.load(open('$RAW/case.json'))['notes'],indent=2,sort_keys=True))"
    echo '```'
    echo
    echo "## Scope (honest)"
    echo
    echo "- This is a **fixed, self-authored** fixture set (one document per format),"
    echo "  not a real-world population; it is a permanent regression court, not a"
    echo "  coverage claim."
    echo "- \`SIGKILL\` timing is not asserted to land at a particular phase; the"
    echo "  assertion is that after each kill every *served* answer still equals the"
    echo "  pre-crash answer, and that a post-crash rebuild is exact. A partial field id"
    echo "  is unknown and is therefore not fetched."
    echo "- Nothing here is run on the host; every command ran in the pinned"
    echo "  \`doc-baseline\` container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$VERDICT" = "PASS" ] && [ "$RC" -eq 0 ]; then
    echo "IDENTITY COURT: PASS — campaign $CAMPAIGN"
    exit 0
fi
echo "IDENTITY COURT: FAIL — campaign $CAMPAIGN" >&2
exit 1
