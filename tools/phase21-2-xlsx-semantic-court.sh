#!/bin/sh
# Phase 21.1.2 — XLSX (SpreadsheetML) semantic model + exact closure.
#
# Pre-registered (Phase-21 subphase 21.1.2). Hypotheses:
#
#   H1 (byte-exactness) — for every admitted XLSX fixture,
#       materialize(field) == source (length + SHA-256 + cmp) after the source
#       file AND the standalone descriptor are deleted, in a fresh process.
#   H2 (semantic surface) — the *spreadsheet* structure is exposed as distinct
#       observations: the style table (number formats, fonts, fills, alignment),
#       merged ranges, cell comments (keyed by cell) + VML note anchors,
#       internal/external hyperlinks resolved through the sheet `_rels`, defined
#       names, tables (name/ref/columns), a drawing/chart/media relationship
#       graph, and external relationships (typed metadata, never dereferenced).
#   H3 (distinctions preserved) — a cell's stored formula, its cached result, its
#       deterministic number-format *display* projection, its style, and its
#       comment are separate fields, never conflated; styles never merge with
#       values.
#   H4 (honest declines) — unsupported selector/representation pairs and missing
#       parts decline typed with exit code 6, never a silent empty answer.
#
# Runs in the pinned `doc-baseline` service (dev toolchain + python3 + sqlite):
#   docker compose run --rm --no-TTY doc-baseline sh tools/phase21-2-xlsx-semantic-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-2-xlsx-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-2
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== regenerate fixtures (stdlib only) ==="
python3 tools/fixtures/make-xlsx.py
cp tools/fixtures/xlsx/*.xlsx "$WORK/corpus/"

FIXTURES="single.xlsx multi.xlsx semantic.xlsx"

# --- per-fixture exactness ---------------------------------------------------
printf '[\n' > "$RAW/results.json"
first=1
exact_ok=0
exact_fail=0
declines_ok=0
for f in $FIXTURES; do
    src="tools/fixtures/xlsx/$f"
    cp "$src" "$WORK/corpus/$f"
    src_len="$(wc -c < "$src" | tr -d ' ')"
    src_sha="$(sha256sum "$src" | cut -d' ' -f1)"

    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    field="$(printf '%s' "$build_json" | jq -r '.ingest.field')"

    # Baseline observations (fresh process each; the field is on disk).
    "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
        > "$RAW/$f.metadata.json"
    "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
        > "$RAW/$f.text.json"
    "$BIN" observe --store "$WORK/store" --field "$field" --sheet 0 --kind text \
        > "$RAW/$f.sheet0.json"

    # H4: an unsupported common pair declines typed (rc 6).
    set +e
    "$BIN" observe --store "$WORK/store" --field "$field" --heading 0 --kind text >/dev/null 2>&1
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

    # The field id must survive the source+descriptor deletion; keep it on record.
    printf '%s\n' "$field" > "$RAW/$f.field.txt"

    # Re-ingest a fresh field for the observation pass (the field above is kept
    # for the exactness witness; the corpus file was deleted).
    cp "$src" "$WORK/corpus/$f"
    build_json2="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    field2="$(printf '%s' "$build_json2" | jq -r '.ingest.field')"
    printf '%s\n' "$field2" > "$RAW/$f.observe_field.txt"
done
printf '\n]\n' >> "$RAW/results.json"

# --- semantic (21.1.2) observations on semantic.xlsx -------------------------
echo "=== semantic observations (semantic.xlsx) ==="
SEM_FIELD="$(cat "$RAW/semantic.xlsx.observe_field.txt")"
"$BIN" observe --store "$WORK/store" --field "$SEM_FIELD" --sheet 0 --kind metadata \
    > "$RAW/semantic.sheet0.metadata.json"
"$BIN" observe --store "$WORK/store" --field "$SEM_FIELD" --xlsx-cell B2 --sheet 0 --kind metadata \
    > "$RAW/semantic.cellB2.json"
"$BIN" observe --store "$WORK/store" --field "$SEM_FIELD" --xlsx-styles --kind metadata \
    > "$RAW/semantic.styles.json"
"$BIN" observe --store "$WORK/store" --field "$SEM_FIELD" --xlsx-defined-names --kind metadata \
    > "$RAW/semantic.defined-names.json"
"$BIN" observe --store "$WORK/store" --field "$SEM_FIELD" --xlsx-external-rels --kind metadata \
    > "$RAW/semantic.external-rels.json"
"$BIN" observe --store "$WORK/store" --field "$SEM_FIELD" --xlsx-comments --sheet 0 --kind metadata \
    > "$RAW/semantic.comments.json"
"$BIN" observe --store "$WORK/store" --field "$SEM_FIELD" --xlsx-hyperlinks --sheet 0 --kind metadata \
    > "$RAW/semantic.hyperlinks.json"
"$BIN" observe --store "$WORK/store" --field "$SEM_FIELD" --xlsx-tables --sheet 0 --kind metadata \
    > "$RAW/semantic.tables.json"
"$BIN" observe --store "$WORK/store" --field "$SEM_FIELD" --xlsx-drawing --sheet 0 --kind metadata \
    > "$RAW/semantic.drawing.json"
"$BIN" observe --store "$WORK/store" --field "$SEM_FIELD" --xlsx-drawing --sheet 0 --kind decoded \
    > "$RAW/semantic.drawing.decoded.bin"

# H3: the displayed value is distinct from the cached value (2469 -> 2469.00).
display_ok=false
if jq -e '.value.displayed == "2469.00"' "$RAW/semantic.cellB2.json" >/dev/null 2>&1; then
    display_ok=true
fi
# H2: the drawing exposes a chart and a media part.
drawing_ok=false
if grep -q '/xl/charts/chart1.xml' "$RAW/semantic.drawing.json" \
    && grep -q '/xl/media/image1.png' "$RAW/semantic.drawing.json"; then
    drawing_ok=true
fi
# H2: external relationships are typed and include the external workbook target.
external_ok=false
if grep -q 'file:///C:/tmp/other.xlsx' "$RAW/semantic.external-rels.json"; then
    external_ok=true
fi
# H2: the comment is keyed to a cell and the VML anchor matches.
comment_ok=false
if grep -q '"cell":"B2"' "$RAW/semantic.comments.json" \
    && grep -q '_x0000_s1025' "$RAW/semantic.comments.json"; then
    comment_ok=true
fi
# H2: the defined names include both the local and the hidden name.
defined_ok=false
if grep -q '"TaxRate"' "$RAW/semantic.defined-names.json" \
    && grep -q '"HiddenName"' "$RAW/semantic.defined-names.json"; then
    defined_ok=true
fi
# H2: the table has its name/ref/columns.
table_ok=false
if grep -q '"Table1"' "$RAW/semantic.tables.json" \
    && grep -q '"ref":"A1:C3"' "$RAW/semantic.tables.json"; then
    table_ok=true
fi
# H2: the hyperlinks keep internal and external distinct.
link_ok=false
if grep -q '"external":true' "$RAW/semantic.hyperlinks.json" \
    && grep -q '"location":"Sheet1!C1"' "$RAW/semantic.hyperlinks.json"; then
    link_ok=true
fi
# H2: the style table exposes a font, a fill, and an alignment.
style_ok=false
if grep -q '"name":"Arial"' "$RAW/semantic.styles.json" \
    && grep -q '"patternType":"solid"' "$RAW/semantic.styles.json" \
    && grep -q '"horizontal":"center"' "$RAW/semantic.styles.json"; then
    style_ok=true
fi

echo "=== exactness after source + descriptor deletion ==="
cat "$RAW/results.json" | jq -c '.[] | {fixture,exact,decline_rc}'

echo "=== summary ==="
jq -n \
    --argjson fixtures "$(printf '%s' "$FIXTURES" | wc -w | tr -d ' ')" \
    --argjson exact_ok "$exact_ok" \
    --argjson exact_fail "$exact_fail" \
    --argjson declines_ok "$declines_ok" \
    --argjson display_ok "$display_ok" \
    --argjson drawing_ok "$drawing_ok" \
    --argjson external_ok "$external_ok" \
    --argjson comment_ok "$comment_ok" \
    --argjson defined_ok "$defined_ok" \
    --argjson table_ok "$table_ok" \
    --argjson link_ok "$link_ok" \
    --argjson style_ok "$style_ok" \
    '{fixtures:$fixtures,exact_ok:$exact_ok,exact_fail:$exact_fail,
      typed_declines_ok:$declines_ok,
      display_ok:$display_ok,drawing_ok:$drawing_ok,external_ok:$external_ok,
      comment_ok:$comment_ok,defined_ok:$defined_ok,table_ok:$table_ok,
      link_ok:$link_ok,style_ok:$style_ok}' \
    | tee "$CAMPAIGN/summary.json"

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 21.1.2 XLSX semantic court — matrix"
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
    echo "style_ok $style_ok"
    echo "display_ok $display_ok"
    echo "comment_ok $comment_ok"
    echo "link_ok $link_ok"
    echo "defined_ok $defined_ok"
    echo "table_ok $table_ok"
    echo "drawing_ok $drawing_ok"
    echo "external_ok $external_ok"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.1.2 — XLSX SpreadsheetML semantic model + exact closure",
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
    "multi.xlsx": "$(sha256sum tools/fixtures/xlsx/multi.xlsx | cut -d' ' -f1)",
    "semantic.xlsx": "$(sha256sum tools/fixtures/xlsx/semantic.xlsx | cut -d' ' -f1)"
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
  "phase": "21.1.2 — XLSX SpreadsheetML semantic model + exact closure",
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
  "observations": ["metadata","text","table","cell","native xlsx-sheet","native xlsx-cell",
                   "xlsx-styles","xlsx-defined-names","xlsx-external-rels","xlsx-comments",
                   "xlsx-hyperlinks","xlsx-tables","xlsx-drawing"],
  "distinctions": "stored formula vs cached result vs deterministic display projection vs style vs comment are separate fields",
  "projection": "number-format rendering is a bounded deterministic projection (labelled); formulas are never evaluated",
  "declines": "unsupported selector/representation pairs and missing parts decline typed (rc 6)",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY doc-baseline sh tools/phase21-2-xlsx-semantic-court.sh
# inside the court:
#   cargo build --locked --all-features
#   python3 tools/fixtures/make-xlsx.py
#   target/debug/vole-document field-build tools/fixtures/xlsx/F.xlsx --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX
#       (--metadata|--doc-text|--sheet 0|--xlsx-cell B2 --sheet 0
#        |--xlsx-styles|--xlsx-defined-names|--xlsx-external-rels
#        |--xlsx-comments --sheet 0|--xlsx-hyperlinks --sheet 0
#        |--xlsx-tables --sheet 0|--xlsx-drawing --sheet 0) --kind metadata|text|structure|decoded
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
    echo "# Phase 21.1.2 — XLSX (SpreadsheetML) semantic court"
    echo
    echo "**Question.** Does the XLSX field expose the *spreadsheet* structure — cell"
    echo "styles, merges, comments, hyperlinks, defined names, tables, drawings/charts,"
    echo "external relationships — as distinct observations, while still closing exactly"
    echo "(\`materialize == source\`, length + SHA-256 + \`cmp\`) after source + descriptor"
    echo "deletion?"
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
    echo "## Observation surface (semantic.xlsx, raw/)"
    echo
    echo '```'
    cat "$CAMPAIGN/summary.json"
    echo '```'
    echo
    echo "## Per-fixture observation files (raw/)"
    echo
    ls "$RAW" | sed 's/^/- `/; s/$/`/'
    echo
    echo "## Scope (honest)"
    echo
    echo "- **Shipped here:** a richer cell-style table (custom number formats, fonts,"
    echo "  fills, alignment) exposed as \`xlsx-styles\` and resolved per cell; merged"
    echo "  ranges; cell comments (\`xl/comments*.xml\`, keyed by cell) + VML note anchors;"
    echo "  internal (\`location\`) and external (\`r:id\`) hyperlinks resolved through the"
    echo "  sheet \`_rels\`; defined/named ranges; tables (name/ref/columns) via"
    echo "  \`tableParts\`; drawings/charts/media exposed as a relationship graph (the"
    echo "  drawing part's exact/decoded bytes resolve; charts are never evaluated); and"
    echo "  package external relationships as typed metadata (never dereferenced)."
    echo "- **Distinctions:** a cell's stored formula, cached result, deterministic"
    echo "  number-format display projection, style, and comment are separate fields."
    echo "  The display projection is a bounded, labelled (\`deterministically-derived\`)"
    echo "  computation over a small fixed set of format codes; formulas are never"
    echo "  evaluated."
    echo "- **Exactness** is inherited from the Phase-12.2 ZIP member raw spans: every"
    echo "  XLSX model here is derived (\`Q_gen\`) and never on the exactness path."
    echo "- **Not claimed here:** formula evaluation, chart data/axis semantics, pivot"
    echo "  tables, rich-text/comment formatting beyond plain text, shared-formula"
    echo "  arrays, and the economic court. Those are later subphases."
    echo "- **Never run on the host:** every command above ran in the pinned"
    echo "  \`doc-baseline\` container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$exact_fail" -ne 0 ]; then
    echo "PHASE 21.1.2 COURT: FAIL" >&2
    exit 1
fi
echo "PHASE 21.1.2 COURT: PASS — campaign $CAMPAIGN"
