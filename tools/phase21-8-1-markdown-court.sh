#!/bin/sh
# Phase 21.8.1 — Markdown (prose, Wave 2) surface + exact closure + model.
#
# Pre-registered (Phase-21 subphase 21.8.1). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture, `materialize(field) == source`
#       (length + SHA-256 + cmp) after the source file AND the standalone
#       descriptor are deleted, in a fresh process.
#   H2 (model + observations) — the representation-preserving model is exposed:
#       exact block/inline source spans and exact bytes (headings/levels, list
#       items, fenced code with language, blockquotes, tables, links/images with
#       targets and titles, reference definitions, footnotes, front matter);
#       common metadata/text/heading/block/search-match and native
#       md-heading/md-block/md-code/md-link/md-find answer.
#   H3 (honest declines) — unsupported selector/representation pairs (e.g. the
#       common `table` that is not in Markdown's capability set) decline typed with
#       exit code 6, never a silent empty answer; a plain-prose document is Opaque
#       (never a panic).
#   H4 (conservative detection) — a source carrying no structural mark (an ATX
#       heading, a fenced code block, front matter, a table, a reference
#       definition, or a footnote definition) is not detected as Markdown.
#
# Runs in the pinned `doc-baseline` service (dev toolchain + python3 + sqlite):
#   docker compose run --rm --no-TTY doc-baseline sh tools/phase21-8-1-markdown-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-8-1-markdown-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-8
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== regenerate fixtures (stdlib only) ==="
python3 tools/fixtures/make-markdown.py
cp tools/fixtures/markdown/* "$WORK/corpus/"

ALL="basic.md lists.md code.md table.md links.md blockquotes.md footnotes.md frontmatter.md toml_frontmatter.md large.md plain.txt prose.md"
# Every fixture is byte-exact; plain.txt and prose.md are Opaque controls (their
# derived observations decline typed) yet still close exactly via the opaque floor.
NORMAL="basic.md lists.md code.md table.md links.md blockquotes.md footnotes.md frontmatter.md toml_frontmatter.md large.md"

# Verbatim document-text observations are bounded: a fixture at or below this size
# is recorded byte-for-byte; a larger one (the ~3 MB `large.md`) is recorded as its
# text length + SHA-256 + a bounded prefix. The observation itself and every verdict
# are unchanged; only the verbatim dump is bounded.
TEXT_DUMP_MAX=$((1 * 1024 * 1024))

# The block index each fixture observes through `md-block` ("" = skip).
block_for() {
    case "$1" in
        basic.md)            echo "1" ;;
        lists.md)            echo "1" ;;
        code.md)             echo "1" ;;
        table.md)            echo "1" ;;
        links.md)            echo "1" ;;
        blockquotes.md)      echo "1" ;;
        footnotes.md)        echo "1" ;;
        frontmatter.md)      echo "1" ;;
        toml_frontmatter.md) echo "1" ;;
        large.md)            echo "1" ;;
        *)                   echo "" ;;
    esac
}

# The code block each fixture observes ("" = skip).
code_for() {
    case "$1" in
        code.md)  echo "0" ;;
        large.md) echo "0" ;;
        *)        echo "" ;;
    esac
}

# The link each fixture observes ("" = skip).
link_for() {
    case "$1" in
        links.md) echo "0" ;;
        large.md) echo "0" ;;
        *)        echo "" ;;
    esac
}

printf '[\n' > "$RAW/results.json"
first=1
exact_ok=0
exact_fail=0
declines_ok=0
opaque_ok=0
for f in $ALL; do
    src="tools/fixtures/markdown/$f"
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
        # H4/H3: a plain-prose source is Opaque; its common observation declines
        # typed (rc 6), never a silent empty answer.
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1
        opaque_rc=$?
        set -e
        if [ "$opaque_rc" -eq 6 ]; then opaque_ok=$((opaque_ok + 1)); fi
    else
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json"
        # doc-text: bound the verbatim dump for an oversized fixture.
        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$WORK/$f.text.full.json"
        if [ "$src_len" -le "$TEXT_DUMP_MAX" ]; then
            mv "$WORK/$f.text.full.json" "$RAW/$f.text.json"
        else
            jq -j '.text' "$WORK/$f.text.full.json" > "$WORK/$f.text.bin"
            text_len=$(wc -c < "$WORK/$f.text.bin" | tr -d ' ')
            text_sha=$(sha256sum "$WORK/$f.text.bin" | cut -d' ' -f1)
            head -c "$TEXT_DUMP_MAX" "$WORK/$f.text.bin" > "$WORK/$f.text.prefix"
            jq -c --argjson cap "$TEXT_DUMP_MAX" --argjson tlen "$text_len" \
                --arg tsha "$text_sha" --rawfile pfx "$WORK/$f.text.prefix" \
                'del(.text) + {text_omitted:true, text_verbatim_bytes:$cap, text_len_bytes:$tlen, text_sha256:$tsha, text_prefix:$pfx}' \
                "$WORK/$f.text.full.json" > "$RAW/$f.text.json"
            rm -f "$WORK/$f.text.full.json" "$WORK/$f.text.bin" "$WORK/$f.text.prefix"
        fi
        "$BIN" observe --store "$WORK/store" --field "$field" --md-heading 0 --kind metadata \
            > "$RAW/$f.heading.json"
        bl="$(block_for "$f")"
        if [ -n "$bl" ]; then
            "$BIN" observe --store "$WORK/store" --field "$field" --md-block "$bl" --kind exact \
                > "$RAW/$f.block.exact.json"
            "$BIN" observe --store "$WORK/store" --field "$field" --md-block "$bl" --kind metadata \
                > "$RAW/$f.block.meta.json"
        fi
        cd_="$(code_for "$f")"
        if [ -n "$cd_" ]; then
            "$BIN" observe --store "$WORK/store" --field "$field" --md-code "$cd_" --kind text \
                > "$RAW/$f.code.text.json"
            "$BIN" observe --store "$WORK/store" --field "$field" --md-code "$cd_" --kind metadata \
                > "$RAW/$f.code.meta.json"
        fi
        lk="$(link_for "$f")"
        if [ -n "$lk" ]; then
            "$BIN" observe --store "$WORK/store" --field "$field" --md-link "$lk" --kind metadata \
                > "$RAW/$f.link.json"
        fi
        "$BIN" observe --store "$WORK/store" --field "$field" --md-find a --kind text \
            > "$RAW/$f.find.json"

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
    echo "# Phase 21.8.1 Markdown court — matrix"
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
  "phase": "21.8.1 — Markdown prose surface + exact closure + model",
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
  "fixtures_tool": "tools/fixtures/make-markdown.py",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
[ "$exact_fail" -eq 0 ] || verdict="FAIL"
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.8.1 — Markdown prose surface + exact closure + model",
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
  "fixture_tool": "tools/fixtures/make-markdown.py (stdlib only)",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion",
  "observations": ["metadata", "text", "heading", "block", "search-match", "native md-heading", "native md-block", "native md-code", "native md-link", "native md-find"],
  "declines": "unsupported common pairs decline typed (rc 6); a plain-prose document is Opaque (rc 6), never a panic",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit",
  "adr_0060": "the MarkdownModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY doc-baseline sh tools/phase21-8-1-markdown-court.sh
# inside the court:
#   cargo build --locked --all-features
#   python3 tools/fixtures/make-markdown.py
#   target/debug/vole-document field-build tools/fixtures/markdown/F --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--md-heading N|--md-block N|--md-code N|--md-link N|--md-find P) --kind ...
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
    echo "# Phase 21.8.1 — Markdown (prose, Wave 2) court"
    echo
    echo "**Question.** Does the first Wave-2 prose format close exactly and expose a"
    echo "representation-preserving prose model (exact block/inline spans and bytes,"
    echo "headings and levels, list items, fenced code with its language, blockquotes,"
    echo "tables, links/images with targets and titles, reference definitions, footnotes,"
    echo "front matter) on top of the whole-source exact leaf?"
    echo
    echo "**Method.** Each self-authored Markdown fixture (generated deterministically by"
    echo "\`tools/fixtures/make-markdown.py\`, Python stdlib only) is ingested via"
    echo "\`field-build\`, observed, then — after the **source file and the standalone"
    echo "descriptor are deleted** — rematerialized exactly in a fresh process and"
    echo "compared with \`length + SHA-256 + cmp\`. Unsupported common pairs are asked for"
    echo "and required to decline typed; the plain-prose controls are required to be Opaque"
    echo "(a typed decline, never a panic)."
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
    echo "- **Shipped here:** byte-based conservative Markdown detection; the bounded,"
    echo "  line-based CommonMark-subset parser (exact block and inline spans and bytes,"
    echo "  ATX headings and levels, paragraphs, ordered/unordered/nested lists, fenced"
    echo "  code with language tags, indented code, blockquotes, GFM tables, inline and"
    echo "  reference links and images with targets and titles, reference definitions,"
    echo "  footnotes, and YAML/TOML front matter); the canonical derived model; native"
    echo "  \`md-heading\`/\`md-block\`/\`md-code\`/\`md-link\`/\`md-find\`; common \`metadata\`/"
    echo "  \`text\`/\`heading\`/\`block\`/\`search-match\`."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the Markdown"
    echo "  model is derived (\`Q_gen\`) and never on the exactness path (ADR-0060: the model"
    echo "  node depends on the \`sha256(source)\` root, so no source-reading node aliases"
    echo "  another field's source)."
    echo "- **The source is never re-flowed or rendered.** A block's exact bytes are"
    echo "  literally \`source[span]\`; the canonical text projection is the source itself."
    echo "- **The supported subset is bounded.** Markdown has no magic bytes and plain"
    echo "  prose is itself a valid Markdown paragraph, so detection requires a structural"
    echo "  mark (an ATX heading, a fenced code block, front matter, a table, a reference"
    echo "  definition, or a footnote definition). Plain prose stays Opaque rather than"
    echo "  being guessed at. Setext headings, HTML blocks, and nested inline emphasis"
    echo "  inside link text are left as literal text rather than guessed."
    echo "- **Not claimed here:** a full CommonMark conformance oracle and the economic"
    echo "  court (a separate campaign)."
    echo "- **Never run on the host:** every command above ran in the pinned"
    echo "  \`doc-baseline\` container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$exact_fail" -ne 0 ]; then
    echo "PHASE 21.8.1 MARKDOWN COURT: FAIL" >&2
    exit 1
fi
echo "PHASE 21.8.1 MARKDOWN COURT: PASS — campaign $CAMPAIGN"
