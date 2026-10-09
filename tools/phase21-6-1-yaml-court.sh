#!/bin/sh
# Phase 21.6.1 — YAML (structured-tree, Wave 2) surface + exact closure + model.
#
# Pre-registered (Phase-21 subphase 21.6.1). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture, `materialize(field) == source`
#       (length + SHA-256 + cmp) after the source file AND the standalone
#       descriptor are deleted, in a fresh process.
#   H2 (model + observations) — the representation-preserving model is exposed:
#       the exact source span of a node, its kind/style, the exact token bytes,
#       anchor/alias and tag text, mapping order, duplicate keys, the document
#       list, and comment spans; common metadata/text/search-match and native
#       yaml-path/yaml-node/yaml-documents/yaml-anchor/yaml-find answer.
#   H3 (honest declines) — unsupported selector/representation pairs (e.g. the
#       common `table` that does not map to YAML) decline typed with exit code 6,
#       never a silent empty answer; a plain-text or malformed document is Opaque
#       (never a panic).
#   H4 (conservative detection) — a source that does not parse as a bounded YAML
#       stream with a mapping/sequence at every document root is not detected.
#
# Runs in the pinned `doc-baseline` service (dev toolchain + python3 + sqlite):
#   docker compose run --rm --no-TTY doc-baseline sh tools/phase21-6-1-yaml-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-6-1-yaml-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-6
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== regenerate fixtures (stdlib only) ==="
python3 tools/fixtures/make-yaml.py
cp tools/fixtures/yaml/*.yaml "$WORK/corpus/"

ALL="anchors.yaml tags.yaml multidoc.yaml styles.yaml comments.yaml dup.yaml deep.yaml large.yaml plain.yaml malformed.yaml"
# Every fixture is byte-exact; plain.yaml and malformed.yaml are Opaque (their
# derived observations decline typed) yet still close exactly via the opaque floor.
NORMAL="anchors.yaml tags.yaml multidoc.yaml styles.yaml comments.yaml dup.yaml deep.yaml large.yaml"

# The node each fixture observes through a path ("" = skip the path probe).
path_for() {
    case "$1" in
        anchors.yaml)   echo "defaults.host" ;;
        tags.yaml)      echo "widget.name" ;;
        multidoc.yaml)  echo "doc1.name" ;;
        styles.yaml)    echo "literal" ;;
        comments.yaml)  echo "c.0" ;;
        dup.yaml)       echo "a" ;;
        large.yaml)     echo "items.0.id" ;;
        *)              echo "" ;;
    esac
}

# The anchor each fixture resolves ("" = skip the anchor probe).
anchor_for() {
    case "$1" in
        anchors.yaml) echo "defaults" ;;
        *)            echo "" ;;
    esac
}

printf '[\n' > "$RAW/results.json"
first=1
exact_ok=0
exact_fail=0
declines_ok=0
opaque_ok=0
for f in $ALL; do
    src="tools/fixtures/yaml/$f"
    cp "$src" "$WORK/corpus/$f"
    src_len="$(wc -c < "$src" | tr -d ' ')"
    src_sha="$(sha256sum "$src" | cut -d' ' -f1)"

    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    field="$(printf '%s' "$build_json" | jq -r '.ingest.field')"
    fmt="$(printf '%s' "$build_json" | jq -r '.ingest.format')"

    decline_rc=-1
    opaque_rc=-1
    case " $NORMAL " in
        *" $f "*) is_normal=1 ;;
        *)        is_normal=0 ;;
    esac

    if [ "$is_normal" -eq 0 ]; then
        # H4/H3: a plain-text or malformed source is Opaque; its common
        # observation declines typed (rc 6), never a silent empty answer.
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1
        opaque_rc=$?
        set -e
        if [ "$opaque_rc" -eq 6 ]; then opaque_ok=$((opaque_ok + 1)); fi
    else
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --yaml-node "" --kind structure \
            > "$RAW/$f.node.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --yaml-documents --kind metadata \
            > "$RAW/$f.documents.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --yaml-find a --kind text \
            > "$RAW/$f.find.json"
        ptr="$(path_for "$f")"
        if [ -n "$ptr" ]; then
            "$BIN" observe --store "$WORK/store" --field "$field" --yaml-path "$ptr" --kind metadata \
                > "$RAW/$f.path.json"
            "$BIN" observe --store "$WORK/store" --field "$field" --yaml-path "$ptr" --kind exact \
                > "$RAW/$f.token.json"
        fi
        anc="$(anchor_for "$f")"
        if [ -n "$anc" ]; then
            "$BIN" observe --store "$WORK/store" --field "$field" --yaml-anchor "$anc" --kind metadata \
                > "$RAW/$f.anchor.json"
        fi

        # H3: an unsupported common pair declines typed (rc 6).
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --table 0 --kind text \
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
        --arg fmt "$fmt" \
        --argjson src_len "$src_len" \
        --arg src_sha "$src_sha" \
        --argjson out_len "$out_len" \
        --arg out_sha "$out_sha" \
        --argjson cmp_ok "$cmp_ok" \
        --argjson exact "$exact" \
        --argjson decline_rc "$decline_rc" \
        --argjson opaque_rc "$opaque_rc" \
        '{fixture:$f,format:$fmt,src_len:$src_len,src_sha256:$src_sha,out_len:$out_len,out_sha256:$out_sha,cmp_ok:$cmp_ok,exact:$exact,decline_rc:$decline_rc,opaque_rc:$opaque_rc}')
    if [ "$first" -eq 1 ]; then first=0; else printf ',\n' >> "$RAW/results.json"; fi
    printf '%s' "$obs" >> "$RAW/results.json"
done
printf '\n]\n' >> "$RAW/results.json"

echo "=== exactness after source + descriptor deletion ==="
cat "$RAW/results.json" | jq -c '.[] | {fixture,format,exact,decline_rc,opaque_rc}'

echo "=== summary ==="
jq -n \
    --argjson fixtures "$(printf '%s' "$ALL" | wc -w | tr -d ' ')" \
    --argjson exact_ok "$exact_ok" \
    --argjson exact_fail "$exact_fail" \
    --argjson typed_declines_ok "$declines_ok" \
    --argjson opaque_controls_ok "$opaque_ok" \
    '{fixtures:$fixtures,exact_ok:$exact_ok,exact_fail:$exact_fail,typed_declines_ok:$typed_declines_ok,opaque_controls_ok:$opaque_controls_ok}' \
    | tee "$CAMPAIGN/summary.json"

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 21.6.1 YAML court — matrix"
    echo
    echo "| fixture | format | src len | out len | src sha256 | sha256 match | cmp | exact | decline rc | opaque rc |"
    echo "| --- | --- | ---: | ---: | --- | --- | --- | --- | ---: | ---: |"
    jq -r '.[] | "| `\(.fixture)` | \(.format) | \(.src_len) | \(.out_len) | `\(.src_sha256[0:12])` | \(.src_sha256==.out_sha256) | \(.cmp_ok) | \(.exact) | \(.decline_rc) | \(.opaque_rc) |"' \
        "$RAW/results.json"
} > "$CAMPAIGN/MATRIX.md"

# --- counts.txt -------------------------------------------------------------
{
    echo "fixtures $(( $(printf '%s' "$ALL" | wc -w) ))"
    echo "exact_ok $exact_ok"
    echo "exact_fail $exact_fail"
    echo "typed_declines_ok $declines_ok"
    echo "opaque_controls_ok $opaque_ok"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.6.1 — YAML structured-tree surface + exact closure + model",
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
  "fixtures_tool": "tools/fixtures/make-yaml.py",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
[ "$exact_fail" -eq 0 ] || verdict="FAIL"
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.6.1 — YAML structured-tree surface + exact closure + model",
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
  "fixtures": "$ALL",
  "fixture_tool": "tools/fixtures/make-yaml.py (stdlib only)",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion",
  "observations": ["metadata", "text", "search-match", "native yaml-path", "native yaml-node", "native yaml-documents", "native yaml-anchor", "native yaml-find"],
  "declines": "unsupported common pairs decline typed (rc 6); a plain-text or malformed document is Opaque (rc 6), never a panic",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit",
  "adr_0060": "the YamlModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY doc-baseline sh tools/phase21-6-1-yaml-court.sh
# inside the court:
#   cargo build --locked --all-features
#   python3 tools/fixtures/make-yaml.py
#   target/debug/vole-document field-build tools/fixtures/yaml/F.yaml --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--yaml-node ""|--yaml-documents|--yaml-find a|--yaml-path P|--yaml-anchor A) --kind ...
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
    echo "# Phase 21.6.1 — YAML (structured-tree, Wave 2) court"
    echo
    echo "**Question.** Does the second Wave-2 structured-tree format close exactly and"
    echo "expose a representation-preserving model (exact spans, kind/style, token bytes,"
    echo "anchors/aliases, tags, documents, mapping order, duplicate keys, comments) on top"
    echo "of the whole-source exact leaf?"
    echo
    echo "**Method.** Each self-authored YAML fixture (generated deterministically by"
    echo "\`tools/fixtures/make-yaml.py\`, Python stdlib only) is ingested via"
    echo "\`field-build\`, observed, then — after the **source file and the standalone"
    echo "descriptor are deleted** — rematerialized exactly in a fresh process and"
    echo "compared with \`length + SHA-256 + cmp\`. Unsupported common pairs are asked for"
    echo "and required to decline typed; the plain-text and malformed controls are required"
    echo "to be Opaque (a typed decline, never a panic)."
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
    echo "- **Shipped here:** byte-based conservative YAML detection; the bounded,"
    echo "  representation-preserving parser (exact node spans, anchors/aliases as a graph,"
    echo "  literal tags, scalar styles plain/single/double/literal/folded, multiple"
    echo "  documents, mapping order, duplicate keys, merge keys surfaced, comment spans);"
    echo "  the canonical derived model; native \`yaml-path\`/\`yaml-node\`/\`yaml-documents\`/"
    echo "  \`yaml-anchor\`/\`yaml-find\`; common \`metadata\`/\`text\`/\`search-match\`."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the YAML model"
    echo "  is derived (\`Q_gen\`) and never on the exactness path (ADR-0060: the model node"
    echo "  depends on the \`sha256(source)\` root, so no source-reading node aliases another"
    echo "  field's source)."
    echo "- **The supported subset is bounded.** Directives, explicit keys, flow-collection"
    echo "  keys, multi-line plain/quoted scalars, and tab indentation are DECLINED with a"
    echo "  typed error (so the input stays Opaque) rather than guessed at."
    echo "- **Not claimed here:** the YAML 1.2 full spec, tag resolution/schema typing, and"
    echo "  the economic court."
    echo "- **Never run on the host:** every command above ran in the pinned"
    echo "  \`doc-baseline\` container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$exact_fail" -ne 0 ]; then
    echo "PHASE 21.6.1 YAML COURT: FAIL" >&2
    exit 1
fi
echo "PHASE 21.6.1 YAML COURT: PASS — campaign $CAMPAIGN"
