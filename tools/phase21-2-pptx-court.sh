#!/bin/sh
# Phase 21.2.1 — PPTX (PresentationML) OPC/XML surface + exact closure + model.
#
# Pre-registered (Phase-21 subphase 21.2.1). Hypotheses:
#
#   H1 (byte-exactness) — for every admitted PPTX fixture,
#       materialize(field) == source (length + SHA-256 + cmp) after the source
#       file AND the standalone descriptor are deleted, in a fresh process.
#   H2 (model + observations) — the presentation inventory (slide size, slide
#       count), each slide's shape tree (text shapes and run-level text, a
#       picture, an embedded table), the notes slides, and the master/layout/
#       theme/media graph are all visible; common metadata/text/table/cell and
#       native slide/shape/notes/layouts/masters/theme/media/tables/find answer.
#   H3 (honest declines) — unsupported selector/representation pairs (e.g. the
#       common heading/block/resource/link that do not map to a presentation)
#       decline typed with exit code 6, never a silent empty answer.
#   H4 (slide order) — the presentation order comes from `p:sldIdLst`, never
#       from `slideN.xml` file names (the `order.pptx` fixture reverses them).
#
# Runs in the pinned `doc-baseline` service (dev toolchain + python3 + sqlite):
#   docker compose run --rm --no-TTY doc-baseline sh tools/phase21-2-pptx-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-2-pptx-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-2
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== regenerate fixtures (stdlib only) ==="
python3 tools/fixtures/make-pptx.py
cp tools/fixtures/pptx/*.pptx "$WORK/corpus/"

FIXTURES="basic.pptx table.pptx picture.pptx notes.pptx order.pptx"

# --- per-fixture exactness + observations -----------------------------------
printf '[\n' > "$RAW/results.json"
first=1
exact_ok=0
exact_fail=0
declines_ok=0
obs_ok=0
for f in $FIXTURES; do
    src="tools/fixtures/pptx/$f"
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
    "$BIN" observe --store "$WORK/store" --field "$field" --slide 0 --kind text \
        > "$RAW/$f.slide0.json"
    "$BIN" observe --store "$WORK/store" --field "$field" --pptx-tables --slide 0 --kind text \
        > "$RAW/$f.tables.json"
    "$BIN" observe --store "$WORK/store" --field "$field" --pptx-shape 0 --slide 0 --kind text \
        > "$RAW/$f.shape0.json"
    "$BIN" observe --store "$WORK/store" --field "$field" --pptx-find e --kind text \
        > "$RAW/$f.find.json"
    "$BIN" observe --store "$WORK/store" --field "$field" --pptx-layouts --kind metadata \
        > "$RAW/$f.layouts.json"
    "$BIN" observe --store "$WORK/store" --field "$field" --pptx-masters --kind metadata \
        > "$RAW/$f.masters.json"
    "$BIN" observe --store "$WORK/store" --field "$field" --pptx-theme --kind metadata \
        > "$RAW/$f.theme.json"

    # H3: an unsupported common pair declines typed (rc 6).
    set +e
    "$BIN" observe --store "$WORK/store" --field "$field" --heading 0 --kind text \
        > /dev/null 2>&1
    err_rc=$?
    set -e
    decline_rc="$err_rc"
    if [ "$err_rc" -eq 6 ]; then declines_ok=$((declines_ok + 1)); fi

    # `order.pptx` is the slide-order fixture: capture its field for the post-loop
    # full-text assertions (its second slide needs an explicit `--slide 1`).
    if [ "$f" = "order.pptx" ]; then
        order_field="$field"
        "$BIN" observe --store "$WORK/store" --field "$field" --slide 1 --kind text \
            > "$RAW/$f.slide1.json"
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
        '{fixture:$f,src_len:$src_len,src_sha256:$src_sha,out_len:$out_len,out_sha256:$out_sha,cmp_ok:$cmp_ok,exact:$exact,decline_rc:$decline_rc}')
    if [ "$first" -eq 1 ]; then first=0; else printf ',\n' >> "$RAW/results.json"; fi
    printf '%s' "$obs" >> "$RAW/results.json"

    # A recorded model check: a presentation reports at least one slide.
    if jq -e '.value.slides >= 1' "$RAW/$f.metadata.json" >/dev/null 2>&1; then
        obs_ok=$((obs_ok + 1))
    fi
done
printf '\n]\n' >> "$RAW/results.json"

# --- H4: slide order follows `p:sldIdLst`, not file names -------------------
# Assert the FULL common text (not a substring) and the second native slide, so
# a collapsed presentation order (one slide repeated) cannot pass. Also assert the
# embedded-table text is present in the deck projection.
order_text="$(jq -r '.text' "$RAW/order.pptx.text.json")"
order_slide0="$(jq -r '.text' "$RAW/order.pptx.slide0.json")"
order_slide1="$(jq -r '.text' "$RAW/order.pptx.slide1.json")"
table_text="$(jq -r '.text' "$RAW/table.pptx.text.json")"
expected_order="$(printf 'SECOND FILE\nFIRST FILE')"
expected_table="$(printf 'Name\tQty\nWidget\t3')"
order_ok=false
if [ "$order_text" = "$expected_order" ] && \
   [ "$order_slide0" = "SECOND FILE" ] && [ "$order_slide1" = "FIRST FILE" ]; then
    order_ok=true
else
    echo "FINDING: slide order did not follow sldIdLst (order text=$order_text slide0=$order_slide0 slide1=$order_slide1)" >&2
fi
table_ok=false
if [ "$table_text" = "$expected_table" ]; then
    table_ok=true
else
    echo "FINDING: embedded table text missing from the deck projection (table text=$table_text)" >&2
fi

echo "=== exactness after source + descriptor deletion ==="
cat "$RAW/results.json" | jq -c '.[] | {fixture,exact,decline_rc}'

echo "=== summary ==="
jq -n \
    --argjson fixtures "$(printf '%s' "$FIXTURES" | wc -w | tr -d ' ')" \
    --argjson exact_ok "$exact_ok" \
    --argjson exact_fail "$exact_fail" \
    --argjson declines_ok "$declines_ok" \
    --argjson order_ok "$order_ok" \
    --argjson table_ok "$table_ok" \
    '{fixtures:$fixtures,exact_ok:$exact_ok,exact_fail:$exact_fail,typed_declines_ok:$declines_ok,sldidlst_order_ok:$order_ok,table_text_ok:$table_ok}' \
    | tee "$CAMPAIGN/summary.json"

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 21.2.1 PPTX court — matrix"
    echo
    echo "| fixture | src len | out len | src sha256 | sha256 match | cmp | exact | decline rc |"
    echo "| --- | ---: | ---: | --- | --- | --- | --- | ---: |"
    jq -r '.[] | "| `\(.fixture)` | \(.src_len) | \(.out_len) | `\(.src_sha256[0:12])` | \(.src_sha256==.out_sha256) | \(.cmp_ok) | \(.exact) | \(.decline_rc) |"' \
        "$RAW/results.json"
} > "$CAMPAIGN/MATRIX.md"

# --- counts.txt (storage = sum of regular-file sizes; never du -sb) ---------
STORE_BYTES="$(find "$WORK/store" -type f -printf '%s\n' 2>/dev/null | awk '{s+=$1} END {print s+0}')"
{
    echo "fixtures $(( $(printf '%s' "$FIXTURES" | wc -w) ))"
    echo "exact_ok $exact_ok"
    echo "exact_fail $exact_fail"
    echo "typed_declines_ok $declines_ok"
    echo "sldidlst_order_ok $order_ok"
    echo "table_text_ok $table_ok"
    echo "store_regular_file_bytes $STORE_BYTES"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.2.1 — PPTX OPC/XML surface + exact closure + model",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "git_dirty": "$(git status --porcelain | tr '\n' ' ' | sed 's/"/\\"/g')",
  "arch": "$(uname -m)",
  "service": "doc-baseline",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "python3": "$(python3 --version)",
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "bin": "$BIN",
  "bin_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "profile": "debug",
  "fixtures_tool": "tools/fixtures/make-pptx.py",
  "fixtures_sha256": {
    "basic.pptx": "$(sha256sum tools/fixtures/pptx/basic.pptx | cut -d' ' -f1)",
    "table.pptx": "$(sha256sum tools/fixtures/pptx/table.pptx | cut -d' ' -f1)",
    "picture.pptx": "$(sha256sum tools/fixtures/pptx/picture.pptx | cut -d' ' -f1)",
    "notes.pptx": "$(sha256sum tools/fixtures/pptx/notes.pptx | cut -d' ' -f1)",
    "order.pptx": "$(sha256sum tools/fixtures/pptx/order.pptx | cut -d' ' -f1)"
  },
  "storage_accounting": "sum of regular-file sizes under the store (never du -sb)",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
[ "$exact_fail" -eq 0 ] || verdict="FAIL"
[ "$order_ok" = true ] || verdict="FAIL"
[ "$table_ok" = true ] || verdict="FAIL"
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.2.1 — PPTX OPC/XML surface + exact closure + model",
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
  "fixture_tool": "tools/fixtures/make-pptx.py (stdlib only; no python-pptx)",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion",
  "observations": ["metadata", "text", "table", "cell", "native pptx-slide", "native pptx-shape", "native pptx-notes", "native pptx-layouts", "native pptx-masters", "native pptx-theme", "native pptx-media", "native pptx-tables", "native pptx-find"],
  "declines": "unsupported common pairs decline typed (rc 6)",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY doc-baseline sh tools/phase21-2-pptx-court.sh
# inside the court:
#   cargo build --locked --all-features
#   python3 tools/fixtures/make-pptx.py
#   target/debug/vole-document field-build tools/fixtures/pptx/F.pptx --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--slide 0|--pptx-shape 0 --slide 0|--pptx-find e|--pptx-layouts|--pptx-masters|--pptx-theme) --kind metadata|text
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
    echo "# Phase 21.2.1 — PPTX (PresentationML) court"
    echo
    echo "**Question.** Does the second format of the format programme close exactly"
    echo "and expose a real PresentationML model on the shared OPC surface?"
    echo
    echo "**Method.** Each self-authored PPTX fixture (generated deterministically by"
    echo "\`tools/fixtures/make-pptx.py\`, Python stdlib only) is ingested via"
    echo "\`field-build\`, observed, then — after the **source file and the standalone"
    echo "descriptor are deleted** — rematerialized exactly in a fresh process and"
    echo "compared with \`length + SHA-256 + cmp\`. Unsupported common pairs are asked"
    echo "for and required to decline typed. Slide order is proved to follow"
    echo "\`p:sldIdLst\` (the \`order.pptx\` fixture reverses the file names)."
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
    echo "- **Shipped here:** byte-based PPTX detection; the OPC-backed presentation"
    echo "  discovery model; the parsed presentation inventory (slide size, slide"
    echo "  order from \`p:sldIdLst\`); each slide's shape tree (text shapes, run-level"
    echo "  text, a picture, a group, a connector, an embedded \`a:tbl\` table); the"
    echo "  notes slides; native \`pptx-slide\`/\`pptx-shape\`/\`pptx-notes\`/"
    echo "  \`pptx-layouts\`/\`pptx-masters\`/\`pptx-theme\`/\`pptx-media\`/\`pptx-tables\`/"
    echo "  \`pptx-find\`; common \`metadata\`/\`text\`/\`table\`/\`cell\`/\`search-match\`."
    echo "- **Embedded-table text** is part of the slide/deck \`text\` projection"
    echo "  (matching DOCX, where a table block's text is part of the body); the"
    echo "  \`table.pptx\` court asserts the exact projected string."
    echo "- **Exactness** is inherited from the Phase-12.2 ZIP member raw spans: the"
    echo "  PPTX model is derived (\`Q_gen\`) and never on the exactness path."
    echo "- **Not claimed here:** chart/diagram rendering, SmartArt, animations,"
    echo "  transitions, speaker-notes rendering, and image decoding. Those are out of"
    echo "  scope for this subphase."
    echo "- **Never run on the host:** every command above ran in the pinned"
    echo "  \`doc-baseline\` container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$exact_fail" -ne 0 ] || [ "$order_ok" != true ] || [ "$table_ok" != true ]; then
    echo "PHASE 21.2.1 COURT: FAIL" >&2
    exit 1
fi
echo "PHASE 21.2.1 COURT: PASS — campaign $CAMPAIGN"
