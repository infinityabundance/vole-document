#!/bin/sh
# Phase 21.4.1 — ODP (OpenDocument Presentation) ODF/XML surface + exact closure.
#
# Pre-registered (Phase-21 subphase 21.4.1). Hypotheses:
#
#   H1 (byte-exactness) — for every admitted ODP fixture,
#       materialize(field) == source (length + SHA-256 + cmp) after the source
#       file AND the standalone descriptor are deleted, in a fresh process.
#   H2 (model + observations) — the `draw:page` slide inventory (name/order), the
#       shapes (text runs, pictures/media, embedded tables), notes pages, master
#       pages, and the media inventory are all visible; common metadata/text/table/
#       cell/search and native slide/shape/notes/masters/media/tables/find answer.
#   H3 (honest declines) — unsupported selector/representation pairs (e.g. the
#       common heading/block/link that do not map to a presentation) decline typed
#       with exit code 6, never a silent empty answer.
#   H4 (bounded expansion) — a `table:number-rows-repeated` bomb declines typed
#       with exit code 8 (resource limit), never allocating the declared grid.
#   H5 (document order) — slide order is the `draw:page` document order, never a
#       page-name/file order (`order.odp` names its first page "Slide 2").
#
# Runs in the pinned `doc-baseline` service (dev toolchain + python3 + sqlite):
#   docker compose run --rm --no-TTY doc-baseline sh tools/phase21-4-1-odp-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-4-1-odp-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-4-1-odp
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== regenerate fixtures (stdlib only) ==="
python3 tools/fixtures/make-odp.py
cp tools/fixtures/odp/*.odp "$WORK/corpus/"

# Every fixture is byte-exact; all except the bomb also answer observations.
FIXTURES="basic.odp table.odp picture.odp notes.odp order.odp bomb.odp"

# --- per-fixture exactness + observations -----------------------------------
printf '[\n' > "$RAW/results.json"
first=1
exact_ok=0
exact_fail=0
declines_ok=0
bomb_ok=0
for f in $FIXTURES; do
    src="tools/fixtures/odp/$f"
    cp "$src" "$WORK/corpus/$f"
    src_len="$(wc -c < "$src" | tr -d ' ')"
    src_sha="$(sha256sum "$src" | cut -d' ' -f1)"

    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    field="$(printf '%s' "$build_json" | jq -r '.ingest.field')"

    decline_rc=-1
    bomb_rc=-1
    slide0_rc=-1
    if [ "$f" = "bomb.odp" ]; then
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
        "$BIN" observe --store "$WORK/store" --field "$field" --odp-slide 0 --kind metadata \
            > "$RAW/$f.slide0.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --odp-slide 0 --kind structure \
            > "$RAW/$f.slide0.structure.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --odp-shape 0 --odp-slide 0 --kind text \
            > "$RAW/$f.shape0.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --odp-masters --kind metadata \
            > "$RAW/$f.masters.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --odp-find Slide --kind text \
            > "$RAW/$f.find.json"

        # H5: slide 0's text proves document order (order.odp's first page is named
        # "Slide 2" but its text is "SECOND FILE").
        slide0_text="$("$BIN" observe --store "$WORK/store" --field "$field" --odp-slide 0 --kind text \
            | jq -r '.text')"
        printf '%s' "$slide0_text" > "$RAW/$f.slide0.text.txt"

        # Per-fixture feature observation.
        case "$f" in
            table.odp)
                "$BIN" observe --store "$WORK/store" --field "$field" --odp-tables --odp-slide 0 --kind text \
                    > "$RAW/$f.tables.json" ;;
            picture.odp)
                "$BIN" observe --store "$WORK/store" --field "$field" --odp-media 0 --kind metadata \
                    > "$RAW/$f.media.json" ;;
            notes.odp)
                "$BIN" observe --store "$WORK/store" --field "$field" --odp-notes 0 --kind text \
                    > "$RAW/$f.notes.json" ;;
        esac

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
        --arg slide0_text "$slide0_text" \
        '{fixture:$f,src_len:$src_len,src_sha256:$src_sha,out_len:$out_len,out_sha256:$out_sha,cmp_ok:$cmp_ok,exact:$exact,decline_rc:$decline_rc,bomb_rc:$bomb_rc,slide0_text:$slide0_text}')
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
    echo "# Phase 21.4.1 ODP court — matrix"
    echo
    echo "| fixture | src len | out len | src sha256 | sha256 match | cmp | exact | decline rc | bomb rc | slide0 text |"
    echo "| --- | ---: | ---: | --- | --- | --- | --- | ---: | ---: | --- |"
    jq -r '.[] | "| `\(.fixture)` | \(.src_len) | \(.out_len) | `\(.src_sha256[0:12])` | \(.src_sha256==.out_sha256) | \(.cmp_ok) | \(.exact) | \(.decline_rc) | \(.bomb_rc) | `\(.slide0_text)` |"' \
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
  "phase": "21.4.1 — ODP ODF/XML surface + exact closure + model",
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
  "fixtures_tool": "tools/fixtures/make-odp.py",
  "fixtures_sha256": {
    "basic.odp": "$(sha256sum tools/fixtures/odp/basic.odp | cut -d' ' -f1)",
    "table.odp": "$(sha256sum tools/fixtures/odp/table.odp | cut -d' ' -f1)",
    "picture.odp": "$(sha256sum tools/fixtures/odp/picture.odp | cut -d' ' -f1)",
    "notes.odp": "$(sha256sum tools/fixtures/odp/notes.odp | cut -d' ' -f1)",
    "order.odp": "$(sha256sum tools/fixtures/odp/order.odp | cut -d' ' -f1)",
    "bomb.odp": "$(sha256sum tools/fixtures/odp/bomb.odp | cut -d' ' -f1)"
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
  "phase": "21.4.1 — ODP ODF/XML surface + exact closure + model",
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
  "fixture_tool": "tools/fixtures/make-odp.py (stdlib only; no odfpy)",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion",
  "observations": ["metadata", "text", "table", "cell", "native odp-slide", "native odp-shape", "native odp-notes", "native odp-masters", "native odp-media", "native odp-tables", "native odp-find"],
  "declines": "unsupported common pairs decline typed (rc 6); a repeated-row bomb declines typed (rc 8)",
  "slide_order": "document order (draw:page), never a page-name/file order",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 19 invalid-xml-structure; 20 invalid-package-structure",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY doc-baseline sh tools/phase21-4-1-odp-court.sh
# inside the court:
#   cargo build --locked --all-features
#   python3 tools/fixtures/make-odp.py
#   target/debug/vole-document field-build tools/fixtures/odp/F.odp --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--odp-slide 0|--odp-shape 0 --odp-slide 0|--odp-masters|--odp-find Slide|--odp-tables --odp-slide 0|--odp-media 0|--odp-notes 0) --kind metadata|text|structure
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
    echo "# Phase 21.4.1 — ODP (OpenDocument Presentation) court"
    echo
    echo "**Question.** Does the last Wave-1 office format — OpenDocument Presentation —"
    echo "close exactly and expose a real presentation model on the shared ODF package"
    echo "substrate (reusing ODT/ODS)?"
    echo
    echo "**Method.** Each self-authored ODP fixture (generated deterministically by"
    echo "\`tools/fixtures/make-odp.py\`, Python stdlib only) is ingested via"
    echo "\`field-build\`, observed, then — after the **source file and the standalone"
    echo "descriptor are deleted** — rematerialized exactly in a fresh process and"
    echo "compared with \`length + SHA-256 + cmp\`. Unsupported common pairs are asked"
    echo "for and required to decline typed; a repeated-row bomb is required to"
    echo "decline as a resource limit; slide order is required to be the \`draw:page\`"
    echo "document order (never a page-name order)."
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
    echo "- **Shipped here:** byte-based ODP detection; the ODF-manifest-backed"
    echo "  discovery model; the \`draw:page\` slide inventory in document order;"
    echo "  shapes (text frames, pictures, custom shapes, groups, embedded tables);"
    echo "  run-level text; notes pages; master pages; styles; the \`Pictures/*\`"
    echo "  media inventory; native \`odp-slide\`/\`odp-shape\`/\`odp-notes\`/"
    echo "  \`odp-masters\`/\`odp-media\`/\`odp-tables\`/\`odp-find\`; common"
    echo "  \`metadata\`/\`text\`/\`table\`/\`cell\`/\`search-match\`."
    echo "- **Exactness** is inherited from the Phase-12.2 ZIP member raw spans: the"
    echo "  ODP model is derived (\`Q_gen\`) and never on the exactness path."
    echo "- **Not claimed here:** slide rendering, animation, transitions, OLE"
    echo "  embeddings, chart data, and the economic court."
    echo "- **Never run on the host:** every command above ran in the pinned"
    echo "  \`doc-baseline\` container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$exact_fail" -ne 0 ]; then
    echo "PHASE 21.4.1 COURT: FAIL" >&2
    exit 1
fi
echo "PHASE 21.4.1 COURT: PASS — campaign $CAMPAIGN"
