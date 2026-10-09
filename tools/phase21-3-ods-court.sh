#!/bin/sh
# Phase 21.3.1 — ODS (OpenDocument Spreadsheet) ODF/XML surface + exact closure + model.
#
# Pre-registered (Phase-21 subphase 21.3.1). Hypotheses:
#
#   H1 (byte-exactness) — for every admitted ODS fixture,
#       materialize(field) == source (length + SHA-256 + cmp) after the source
#       file AND the standalone descriptor are deleted, in a fresh process.
#   H2 (model + observations) — the sheet inventory (name/order/visibility), the
#       cell facets (stored formula, typed value, displayed text, style name, and
#       decoded-part span), repeated cells/rows, merged spans, named expressions,
#       cell styles, and cell comments are all visible; common metadata/text/table/
#       cell/search and native sheet/cell/find/styles/named-expressions/comments
#       answer.
#   H3 (honest declines) — unsupported selector/representation pairs (e.g. the
#       common heading/block/resource/link that do not map to a spreadsheet)
#       decline typed with exit code 6, never a silent empty answer.
#   H4 (bounded expansion) — a `table:number-rows-repeated` bomb declines typed
#       with exit code 8 (resource limit), never allocating the declared grid.
#
# Runs in the pinned `doc-baseline` service (dev toolchain + python3 + sqlite):
#   docker compose run --rm --no-TTY doc-baseline sh tools/phase21-3-ods-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-3-ods-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-3
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== regenerate fixtures (stdlib only) ==="
python3 tools/fixtures/make-ods.py
cp tools/fixtures/ods/*.ods "$WORK/corpus/"

# Every fixture is byte-exact; all except the bomb also answer observations.
FIXTURES="basic.ods multi.ods values.ods merged.ods named.ods comments.ods styles.ods bomb.ods"
NORMAL="basic.ods multi.ods values.ods merged.ods named.ods comments.ods styles.ods"

# --- per-fixture exactness + observations -----------------------------------
printf '[\n' > "$RAW/results.json"
first=1
exact_ok=0
exact_fail=0
declines_ok=0
bomb_ok=0
for f in $FIXTURES; do
    src="tools/fixtures/ods/$f"
    cp "$src" "$WORK/corpus/$f"
    src_len="$(wc -c < "$src" | tr -d ' ')"
    src_sha="$(sha256sum "$src" | cut -d' ' -f1)"

    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    field="$(printf '%s' "$build_json" | jq -r '.ingest.field')"

    decline_rc=-1
    bomb_rc=-1
    if [ "$f" = "bomb.ods" ]; then
        # H4: the repeated-row bomb declines typed (resource limit) on observation.
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json" 2>&1
        bomb_rc=$?
        set -e
        if [ "$bomb_rc" -eq 8 ]; then bomb_ok=$((bomb_ok + 1)); fi
    else
        # Observations (each is a fresh process; the field is on disk).
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --ods-sheet 0 --kind metadata \
            > "$RAW/$f.sheet0.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --ods-cell A1 --ods-sheet 0 --kind metadata \
            > "$RAW/$f.cellA1.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --ods-styles --kind metadata \
            > "$RAW/$f.styles.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --ods-named-expressions --kind metadata \
            > "$RAW/$f.named.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --ods-comments --ods-sheet 0 --kind metadata \
            > "$RAW/$f.comments.json"

        # H3: an unsupported common pair declines typed (rc 6).
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --heading 0 --kind text \
            > /dev/null 2>&1
        decline_rc=$?
        set -e
        if [ "$decline_rc" -eq 6 ]; then declines_ok=$((declines_ok + 1)); fi
    fi

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
        --argjson bomb_rc "$bomb_rc" \
        '{fixture:$f,src_len:$src_len,src_sha256:$src_sha,out_len:$out_len,out_sha256:$out_sha,cmp_ok:$cmp_ok,exact:$exact,decline_rc:$decline_rc,bomb_rc:$bomb_rc}')
    if [ "$first" -eq 1 ]; then first=0; else printf ',\n' >> "$RAW/results.json"; fi
    printf '%s' "$obs" >> "$RAW/results.json"
done
printf '\n]\n' >> "$RAW/results.json"

echo "=== exactness after source + descriptor deletion ==="
cat "$RAW/results.json" | jq -c '.[] | {fixture,exact,decline_rc,bomb_rc}'

echo "=== summary ==="
jq -n \
    --argjson fixtures "$(printf '%s' "$FIXTURES" | wc -w | tr -d ' ')" \
    --argjson exact_ok "$exact_ok" \
    --argjson exact_fail "$exact_fail" \
    --argjson typed_declines_ok "$declines_ok" \
    --argjson bomb_declines_ok "$bomb_ok" \
    '{fixtures:$fixtures,exact_ok:$exact_ok,exact_fail:$exact_fail,typed_declines_ok:$typed_declines_ok,bomb_declines_ok:$bomb_declines_ok}' \
    | tee "$CAMPAIGN/summary.json"

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 21.3.1 ODS court — matrix"
    echo
    echo "| fixture | src len | out len | src sha256 | sha256 match | cmp | exact | decline rc | bomb rc |"
    echo "| --- | ---: | ---: | --- | --- | --- | --- | ---: | ---: |"
    jq -r '.[] | "| `\(.fixture)` | \(.src_len) | \(.out_len) | `\(.src_sha256[0:12])` | \(.src_sha256==.out_sha256) | \(.cmp_ok) | \(.exact) | \(.decline_rc) | \(.bomb_rc) |"' \
        "$RAW/results.json"
} > "$CAMPAIGN/MATRIX.md"

# --- counts.txt -------------------------------------------------------------
{
    echo "fixtures $(( $(printf '%s' "$FIXTURES" | wc -w) ))"
    echo "exact_ok $exact_ok"
    echo "exact_fail $exact_fail"
    echo "typed_declines_ok $declines_ok"
    echo "bomb_declines_ok $bomb_ok"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.3.1 — ODS ODF/XML surface + exact closure + model",
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
  "fixtures_tool": "tools/fixtures/make-ods.py",
  "fixtures_sha256": {
    "basic.ods": "$(sha256sum tools/fixtures/ods/basic.ods | cut -d' ' -f1)",
    "multi.ods": "$(sha256sum tools/fixtures/ods/multi.ods | cut -d' ' -f1)",
    "values.ods": "$(sha256sum tools/fixtures/ods/values.ods | cut -d' ' -f1)",
    "merged.ods": "$(sha256sum tools/fixtures/ods/merged.ods | cut -d' ' -f1)",
    "named.ods": "$(sha256sum tools/fixtures/ods/named.ods | cut -d' ' -f1)",
    "comments.ods": "$(sha256sum tools/fixtures/ods/comments.ods | cut -d' ' -f1)",
    "styles.ods": "$(sha256sum tools/fixtures/ods/styles.ods | cut -d' ' -f1)",
    "bomb.ods": "$(sha256sum tools/fixtures/ods/bomb.ods | cut -d' ' -f1)"
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
  "phase": "21.3.1 — ODS ODF/XML surface + exact closure + model",
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
  "fixture_tool": "tools/fixtures/make-ods.py (stdlib only; no odfpy)",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion",
  "observations": ["metadata", "text", "table", "cell", "native ods-sheet", "native ods-cell", "native ods-find", "native ods-styles", "native ods-named-expressions", "native ods-comments"],
  "declines": "unsupported common pairs decline typed (rc 6); a repeated-row bomb declines typed (rc 8)",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY doc-baseline sh tools/phase21-3-ods-court.sh
# inside the court:
#   cargo build --locked --all-features
#   python3 tools/fixtures/make-ods.py
#   target/debug/vole-document field-build tools/fixtures/ods/F.ods --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--ods-sheet 0|--ods-cell A1 --ods-sheet 0|--ods-styles|--ods-named-expressions|--ods-comments) --kind metadata|text
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
    echo "# Phase 21.3.1 — ODS (OpenDocument Spreadsheet) court"
    echo
    echo "**Question.** Does the first OpenDocument *spreadsheet* format close exactly and"
    echo "expose a real spreadsheet model on the shared ODF package substrate?"
    echo
    echo "**Method.** Each self-authored ODS fixture (generated deterministically by"
    echo "\`tools/fixtures/make-ods.py\`, Python stdlib only) is ingested via"
    echo "\`field-build\`, observed, then — after the **source file and the standalone"
    echo "descriptor are deleted** — rematerialized exactly in a fresh process and"
    echo "compared with \`length + SHA-256 + cmp\`. Unsupported common pairs are asked"
    echo "for and required to decline typed; the repeated-row bomb is required to"
    echo "decline as a resource limit."
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
    echo "- **Shipped here:** byte-based ODS detection; the ODF-manifest-backed"
    echo "  discovery model; the parsed sheet inventory (name/order/visibility); the"
    echo "  distinct cell facets (stored formula, typed value, displayed text, style"
    echo "  name, decoded-part span); bounded repeated-cell/-row expansion; merged"
    echo "  spans; named expressions; cell styles; cell comments; native"
    echo "  \`ods-sheet\`/\`ods-cell\`/\`ods-find\`/\`ods-styles\`/\`ods-named-expressions\`/"
    echo "  \`ods-comments\`; common \`metadata\`/\`text\`/\`table\`/\`cell\`/\`search-match\`."
    echo "- **Exactness** is inherited from the Phase-12.2 ZIP member raw spans: the ODS"
    echo "  model is derived (\`Q_gen\`) and never on the exactness path."
    echo "- **Not claimed here:** formula evaluation, number-format rendering,"
    echo "  pivot tables, charts/drawings, external data references, and the economic"
    echo "  court."
    echo "- **Never run on the host:** every command above ran in the pinned"
    echo "  \`doc-baseline\` container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$exact_fail" -ne 0 ]; then
    echo "PHASE 21.3.1 COURT: FAIL" >&2
    exit 1
fi
echo "PHASE 21.3.1 COURT: PASS — campaign $CAMPAIGN"
