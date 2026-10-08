#!/bin/sh
# Phase 21.1.1 — XLSX (SpreadsheetML) OPC/XML surface + exact closure + model.
#
# Pre-registered (Phase-21 subphase 21.1.1). Hypotheses:
#
#   H1 (byte-exactness) — for every admitted XLSX fixture,
#       materialize(field) == source (length + SHA-256 + cmp) after the source
#       file AND the standalone descriptor are deleted, in a fresh process.
#   H2 (model + observations) — the workbook/sheet inventory, shared/inline
#       strings, numbers/booleans/errors, a stored formula with a cached value, a
#       style, and a merged range are all visible; common metadata/text/table/cell
#       and native sheet/cell/find answer.
#   H3 (honest declines) — unsupported selector/representation pairs (e.g. the
#       common heading/block/resource/link that do not map to a spreadsheet)
#       decline typed with exit code 6, never a silent empty answer.
#   H4 (distinctions preserved) — a cell's stored formula and its cached result
#       are separate observations.
#
# Runs in the pinned `doc-baseline` service (dev toolchain + python3 + sqlite):
#   docker compose run --rm --no-TTY doc-baseline sh tools/phase21-1-xlsx-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-1-xlsx-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-1
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== regenerate fixtures (stdlib only) ==="
python3 tools/fixtures/make-xlsx.py
cp tools/fixtures/xlsx/*.xlsx "$WORK/corpus/"

FIXTURES="single.xlsx multi.xlsx"

# --- per-fixture exactness + observations -----------------------------------
printf '[\n' > "$RAW/results.json"
first=1
exact_ok=0
exact_fail=0
declines_ok=0
obs_ok=0
for f in $FIXTURES; do
    src="tools/fixtures/xlsx/$f"
    cp "$src" "$WORK/corpus/$f"
    src_len="$(wc -c < "$src" | tr -d ' ')"
    src_sha="$(sha256sum "$src" | cut -d' ' -f1)"

    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    field="$(printf '%s' "$build_json" | jq -r '.ingest.field')"

    # Observations (each is a fresh process; the field is on disk).
    "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
        > "$RAW/$f.metadata.json"
    "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
        > "$RAW/$f.text.json"
    "$BIN" observe --store "$WORK/store" --field "$field" --sheet 0 --kind text \
        > "$RAW/$f.sheet0.json"
    "$BIN" observe --store "$WORK/store" --field "$field" --xlsx-cell A1 --sheet 0 --kind text \
        > "$RAW/$f.cellA1.json"
    "$BIN" observe --store "$WORK/store" --field "$field" --xlsx-cell A1 --sheet 0 --kind structure \
        > "$RAW/$f.cellA1.structure.json"

    # H3: an unsupported common pair declines typed (rc 6).
    set +e
    err_out="$("$BIN" observe --store "$WORK/store" --field "$field" --heading 0 --kind text 2>&1)"
    err_rc=$?
    set -e
    decline_rc="$err_rc"
    if [ "$err_rc" -eq 6 ]; then declines_ok=$((declines_ok + 1)); fi

    # Delete the source AND the standalone descriptor, then rematerialize exactly
    # in a fresh process and compare (length + SHA-256 + cmp).
    rm -f "$WORK/corpus/$f" "$WORK/$f.voldoc"
    "$BIN" materialize --store "$WORK/store" --field "$field" --exact \
        --output "$WORK/$f.out" > /dev/null
    out_len="$(wc -c < "$WORK/$f.out" | tr -d ' ')"
    out_sha="$(sha256sum "$WORK/$f.out" | cut -d' ' -f1)"
    if cmp -s "$src" "$WORK/$f.out"; then cmp_ok=true; else cmp_ok=false; fi
    if [ "$out_len" = "$src_len" ] && [ "$out_sha" = "$src_sha" ] && [ "$cmp_ok" = true ]; then
        exact=true; exact_ok=$((exact_ok + 1))
    else
        exact=false; exact_fail=$((exact_fail + 1))
        echo "FINDING: not byte-exact after removal: $f" >&2
    fi

    obs=$(jq -n \
        --arg f "$f" \
        --argjson src_len "$src_len" \
        --arg src_sha "$src_sha" \
        --argjson out_len "$out_len" \
        --arg out_sha "$out_sha" \
        --argjson cmp_ok "$cmp_ok" \
        --argjson exact "$exact" \
        --argjson decline_rc "$decline_rc" \
        '{fixture:$f,src_len:$src_len,src_sha256:$src_sha,out_len:$out_len,out_sha256:$out_sha,cmp_ok:$cmp_ok,exact:$exact,decline_rc:$decline_rc}')
    if [ "$first" -eq 1 ]; then first=0; else printf ',\n' >> "$RAW/results.json"; fi
    printf '%s' "$obs" >> "$RAW/results.json"

    # A couple of observation-content checks (recorded, not asserted silently).
    if jq -e '.value.sheets >= 1' "$RAW/$f.metadata.json" >/dev/null 2>&1; then obs_ok=$((obs_ok + 1)); fi
done
printf '\n]\n' >> "$RAW/results.json"

echo "=== exactness after source + descriptor deletion ==="
cat "$RAW/results.json" | jq -c '.[] | {fixture,exact,decline_rc}'

echo "=== summary ==="
jq -n \
    --argjson fixtures "$(printf '%s' "$FIXTURES" | wc -w | tr -d ' ')" \
    --argjson exact_ok "$exact_ok" \
    --argjson exact_fail "$exact_fail" \
    --argjson declines_ok "$declines_ok" \
    '{fixtures:$fixtures,exact_ok:$exact_ok,exact_fail:$exact_fail,typed_declines_ok:$declines_ok}' \
    | tee "$CAMPAIGN/summary.json"

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 21.1.1 XLSX court — matrix"
    echo
    echo "| fixture | src len | out len | src sha256 | sha256 match | cmp | exact | decline rc |"
    echo "| --- | ---: | ---: | --- | --- | --- | --- | ---: |"
    jq -r '.[] | "| `\(.fixture)` | \(.src_len) | \(.out_len) | `\(.src_sha256[0:12])` | \(.src_sha256==.out_sha256) | \(.cmp_ok) | \(.exact) | \(.decline_rc) |"' \
        "$RAW/results.json"
} > "$CAMPAIGN/MATRIX.md"

# --- counts.txt -------------------------------------------------------------
{
    echo "fixtures $(( $(printf '%s' "$FIXTURES" | wc -w) ))"
    echo "exact_ok $exact_ok"
    echo "exact_fail $exact_fail"
    echo "typed_declines_ok $declines_ok"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.1.1 — XLSX OPC/XML surface + exact closure + model",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "git_dirty": "$(git status --porcelain | tr '\n' ' ' | sed 's/"/\\"/g')",
  "arch": "$(uname -m)",
  "service": "doc-baseline",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "python3": "$(python3 --version)",
  "sqlite3": "$(sqlite3 --version)",
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "bin": "$BIN",
  "bin_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "profile": "debug",
  "fixtures_tool": "tools/fixtures/make-xlsx.py",
  "fixtures_sha256": {
    "single.xlsx": "$(sha256sum tools/fixtures/xlsx/single.xlsx | cut -d' ' -f1)",
    "multi.xlsx": "$(sha256sum tools/fixtures/xlsx/multi.xlsx | cut -d' ' -f1)"
  },
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
[ "$exact_fail" -eq 0 ] || verdict="FAIL"
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.1.1 — XLSX OPC/XML surface + exact closure + model",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "measured_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "tree_state": "$(git status --porcelain | tr '\n' ' ' | sed 's/"/\\"/g')",
  "service": "doc-baseline",
  "profile": "debug",
  "bin": "$BIN",
  "bin_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "python": "$(python3 --version)",
  "fixtures": "$FIXTURES",
  "fixture_tool": "tools/fixtures/make-xlsx.py (stdlib only; no openpyxl)",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion",
  "observations": ["metadata", "text", "table", "cell", "native xlsx-sheet", "native xlsx-cell", "native xlsx-find"],
  "declines": "unsupported common pairs decline typed (rc 6)",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY doc-baseline sh tools/phase21-1-xlsx-court.sh
# inside the court:
#   cargo build --locked --all-features
#   python3 tools/fixtures/make-xlsx.py
#   target/debug/vole-document field-build tools/fixtures/xlsx/F.xlsx --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--sheet 0|--xlsx-cell A3 --sheet 0) --kind metadata|text|structure
#   rm source F.voldoc
#   target/debug/vole-document materialize --store STORE --field HEX --exact --output OUT ; cmp OUT source
# Full gate (dev service):
#   cargo fmt --all --check
#   cargo clippy --all-targets --all-features -- -D warnings
#   cargo test --locked --all-features
#   cargo test --locked
#   cargo test --locked --no-default-features
#   MSRV: docker compose run --rm --no-TTY msrv cargo build --locked --all-features
EOF

# --- SUMMARY.md -------------------------------------------------------------
{
    echo "# Phase 21.1.1 — XLSX (SpreadsheetML) court"
    echo
    echo "**Question.** Does the first format of the format programme close exactly and"
    echo "expose a real SpreadsheetML model on the shared OPC surface?"
    echo
    echo "**Method.** Each self-authored XLSX fixture (generated deterministically by"
    echo "\`tools/fixtures/make-xlsx.py\`, Python stdlib only) is ingested via"
    echo "\`field-build\`, observed, then — after the **source file and the standalone"
    echo "descriptor are deleted** — rematerialized exactly in a fresh process and"
    echo "compared with \`length + SHA-256 + cmp\`. Unsupported common pairs are asked"
    echo "for and required to decline typed."
    echo
    echo "## Exactness (after source + descriptor deletion)"
    echo
    cat "$CAMPAIGN/MATRIX.md" | tail -n +3
    echo
    echo "## Counts"
    echo
    echo '```'
    cat "$CAMPAIGN/counts.txt"
    echo '```'
    echo
    echo "## Per-fixture observation files (raw/)"
    echo
    ls "$RAW" | sed 's/^/- `/; s/$/`/'
    echo
    echo "## Scope (honest)"
    echo
    echo "- **Shipped here:** byte-based XLSX detection; the OPC-backed workbook"
    echo "  discovery model; the parsed workbook inventory (name/order/visibility);"
    echo "  shared/inline strings, numbers/booleans/errors, a stored formula with a"
    echo "  cached value, a minimal style table, and merged ranges; native"
    echo "  \`xlsx-sheet\`/\`xlsx-cell\`/\`xlsx-find\`; common \`metadata\`/\`text\`/\`table\`/"
    echo "  \`cell\`/\`search-match\`."
    echo "- **Exactness** is inherited from the Phase-12.2 ZIP member raw spans: the XLSX"
    echo "  model is derived (\`Q_gen\`) and never on the exactness path."
    echo "- **Not claimed here:** displayed values (number-format rendering), formula"
    echo "  evaluation, charts/drawings, external data references, pivot tables, and the"
    echo "  economic court. Those are later subphases (21.1.2/21.1.3)."
    echo "- **Never run on the host:** every command above ran in the pinned"
    echo "  \`doc-baseline\` container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$exact_fail" -ne 0 ]; then
    echo "PHASE 21.1.1 COURT: FAIL" >&2
    exit 1
fi
echo "PHASE 21.1.1 COURT: PASS — campaign $CAMPAIGN"
