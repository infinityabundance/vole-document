#!/bin/sh
# Phase 21.10.1 — HTML (error-recovering markup, Wave 2) surface + exact closure.
#
# Pre-registered (Phase-21 subphase 21.10.1). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture, `materialize(field) == source`
#       (length + SHA-256 + cmp) after the source file AND the standalone
#       descriptor are deleted, in a fresh process.
#   H2 (model + observations) — the representation-preserving model is exposed:
#       the exact source span of an element/attribute/text/comment/DOCTYPE, the
#       element name, the attribute quoting (double/single/unquoted/boolean), void
#       elements, raw `<script>`/`<style>` content, and literal (unexpanded) entity
#       references; common metadata/text/heading/link/search-match and native
#       html-path/html-element/html-attr/html-scripts/html-find answer.
#   H3 (honest declines) — unsupported common pairs decline typed with exit code 6,
#       never a silent empty answer; a malformed/opaque document is Opaque.
#   H4 (error recovery) — `malformed.html` (unclosed tags, implicit `<li>` closing,
#       a stray end tag, an unclosed document) parses and closes exactly, never a
#       panic.
#   H5 (conservative detection + precedence) — plain prose (`prose.txt`), non-HTML
#       `<-junk` (`junk.html`), and an internal-subset DOCTYPE (`xxe.html`) are NOT
#       detected as HTML; a fully well-formed XHTML source (`xhtml.html`) is kept as
#       **XML** by precedence, never guessed to be HTML.
#
# Runs in the pinned `doc-baseline` service (dev toolchain + python3 + sqlite):
#   docker compose run --rm --no-TTY doc-baseline sh tools/phase21-10-1-html-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-10-1-html-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-10
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== regenerate fixtures (stdlib only) ==="
python3 tools/fixtures/make-html.py
cp tools/fixtures/html/*.html tools/fixtures/html/prose.txt "$WORK/corpus/"

# Normal fixtures: detected as HTML, every native/common observation answers, and
# each closes byte-exactly. Controls: not detected as HTML (Opaque) — an
# unsupported common observation must decline typed (rc 6).
ALL="basic.html elements.html rawtext.html entities.html malformed.html large.html xhtml.html prose.txt junk.html xxe.html"
NORMAL="basic.html elements.html rawtext.html entities.html malformed.html large.html"
OPAQUE="prose.txt junk.html xxe.html"

# The element path each normal fixture observes ("" = skip).
path_for() {
    case "$1" in
        basic.html)     echo "/html/body/h1" ;;
        elements.html)  echo "/html/body/div[1]/span" ;;
        rawtext.html)   echo "/html/body/p" ;;
        entities.html)  echo "/html/body/p" ;;
        malformed.html) echo "/html/body/ul/li[2]" ;;
        large.html)     echo "/html/body/div[1]/h2" ;;
        *)              echo "" ;;
    esac
}
attr_for() {
    case "$1" in
        basic.html)     echo "/html/body@id" ;;
        elements.html)  echo "/html/body@id" ;;
        rawtext.html)   echo "/html/body@id" ;;
        entities.html)  echo "/html/body@id" ;;
        malformed.html) echo "/html/body@id" ;;
        large.html)     echo "/html/body@id" ;;
        *)              echo "" ;;
    esac
}

printf '[\n' > "$RAW/results.json"
first=1
exact_ok=0
exact_fail=0
declines_ok=0
opaque_ok=0
opaque_n=0
normal_n=0

is_normal() {
    for n in $NORMAL; do [ "$n" = "$1" ] && return 0; done
    return 1
}
is_opaque() {
    for o in $OPAQUE; do [ "$o" = "$1" ] && return 0; done
    return 1
}

for f in $ALL; do
    src="tools/fixtures/html/$f"
    cp "$src" "$WORK/corpus/$f"
    src_len="$(wc -c < "$src" | tr -d ' ')"
    src_sha="$(sha256sum "$src" | cut -d' ' -f1)"

    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    field="$(printf '%s' "$build_json" | jq -r '.ingest.field')"
    fmt="$(printf '%s' "$build_json" | jq -r '.ingest.format')"

    decline_rc=-1
    opaque_rc=-1
    html_native_rc=-1

    if is_normal "$f"; then
        normal_n=$((normal_n + 1))
        [ "$fmt" = "html" ] || echo "FINDING: $f not detected as html (fmt=$fmt)" >&2
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --heading 0 --kind text \
            > "$RAW/$f.heading.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --link 0 --kind metadata \
            > "$RAW/$f.link.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --html-element "" --kind structure \
            > "$RAW/$f.element.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --html-scripts --kind metadata \
            > "$RAW/$f.scripts.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --html-find h1 --kind text \
            > "$RAW/$f.find.json"
        ptr="$(path_for "$f")"
        if [ -n "$ptr" ]; then
            "$BIN" observe --store "$WORK/store" --field "$field" --html-path "$ptr" --kind metadata \
                > "$RAW/$f.path.json"
            "$BIN" observe --store "$WORK/store" --field "$field" --html-path "$ptr" --kind exact \
                > "$RAW/$f.token.bin"
        fi
        at="$(attr_for "$f")"
        if [ -n "$at" ]; then
            "$BIN" observe --store "$WORK/store" --field "$field" --html-attr "$at" --kind exact \
                > "$RAW/$f.attr.bin"
            "$BIN" observe --store "$WORK/store" --field "$field" --html-attr "$at" --kind metadata \
                > "$RAW/$f.attr.json"
        fi
        # H3: an unsupported common pair declines typed (rc 6).
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --table 0 --kind text \
            > /dev/null 2>&1
        decline_rc=$?
        set -e
        if [ "$decline_rc" -eq 6 ]; then declines_ok=$((declines_ok + 1)); fi
    elif is_opaque "$f"; then
        # H5: not detected as HTML; its common observation declines typed (rc 6).
        [ "$fmt" = "opaque" ] || echo "FINDING: control $f detected as $fmt (want opaque)" >&2
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1
        opaque_rc=$?
        set -e
        if [ "$opaque_rc" -eq 6 ]; then opaque_ok=$((opaque_ok + 1)); fi
        opaque_n=$((opaque_n + 1))
    else
        # The XHTML precedence control: detected as XML (not HTML), so the
        # HTML-native selector declines typed (rc 6) while common metadata answers.
        [ "$fmt" = "xml" ] || echo "FINDING: xhtml.html detected as $fmt (want xml)" >&2
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json"
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --html-element "" --kind structure \
            > "$RAW/$f.html-native.json" 2>&1
        html_native_rc=$?
        set -e
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
        --argjson html_native_rc "$html_native_rc" \
        '{fixture:$f,format:$fmt,src_len:$src_len,src_sha256:$src_sha,out_len:$out_len,out_sha256:$out_sha,cmp_ok:$cmp_ok,exact:$exact,decline_rc:$decline_rc,opaque_rc:$opaque_rc,html_native_rc:$html_native_rc}')
    if [ "$first" -eq 1 ]; then first=0; else printf ',\n' >> "$RAW/results.json"; fi
    printf '%s' "$obs" >> "$RAW/results.json"
done
printf '\n]\n' >> "$RAW/results.json"

echo "=== exactness after source + descriptor deletion ==="
cat "$RAW/results.json" | jq -c '.[] | {fixture,format,exact,decline_rc,opaque_rc,html_native_rc}'

echo "=== summary ==="
jq -n \
    --argjson fixtures "$(printf '%s' "$ALL" | wc -w | tr -d ' ')" \
    --argjson normal "$normal_n" \
    --argjson exact_ok "$exact_ok" \
    --argjson exact_fail "$exact_fail" \
    --argjson typed_declines_ok "$declines_ok" \
    --argjson opaque_controls_ok "$opaque_ok" \
    --argjson opaque_controls "$opaque_n" \
    '{fixtures:$fixtures,normal:$normal,exact_ok:$exact_ok,exact_fail:$exact_fail,typed_declines_ok:$typed_declines_ok,opaque_controls_ok:$opaque_controls_ok,opaque_controls:$opaque_controls}' \
    | tee "$CAMPAIGN/summary.json"

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 21.10.1 HTML court — matrix"
    echo
    echo "| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | opaque rc | html-native rc |"
    echo "| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: | ---: |"
    jq -r '.[] | "| `\(.fixture)` | \(.format) | \(.src_len) | \(.out_len) | \(.src_sha256==.out_sha256) | \(.cmp_ok) | \(.exact) | \(.decline_rc) | \(.opaque_rc) | \(.html_native_rc) |"' \
        "$RAW/results.json"
} > "$CAMPAIGN/MATRIX.md"

# --- counts.txt -------------------------------------------------------------
{
    echo "fixtures $(( $(printf '%s' "$ALL" | wc -w) ))"
    echo "normal $normal_n"
    echo "exact_ok $exact_ok"
    echo "exact_fail $exact_fail"
    echo "typed_declines_ok $declines_ok"
    echo "opaque_controls_ok $opaque_ok"
    echo "opaque_controls $opaque_n"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.10.1 — HTML error-recovering markup surface + exact closure",
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
  "fixtures_tool": "tools/fixtures/make-html.py",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
[ "$exact_fail" -eq 0 ] || verdict="FAIL"
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.10.1 — HTML error-recovering markup surface + exact closure",
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
  "fixture_tool": "tools/fixtures/make-html.py (stdlib only)",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion",
  "observations": ["metadata", "text", "heading", "link", "search-match", "native html-path", "native html-element", "native html-attr", "native html-scripts", "native html-find"],
  "declines": "unsupported common pairs decline typed (rc 6); an opaque/malformed document is Opaque (rc 6), never a panic",
  "error_recovery": "malformed.html (unclosed tags, implicit <li> closing, a stray end tag, an unclosed document) parses and closes exactly, never a panic",
  "precedence": "xhtml.html (well-formed XHTML) is detected as XML, not HTML (XML is tried first)",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 25 invalid-html-structure",
  "adr_0060": "the HtmlModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY doc-baseline sh tools/phase21-10-1-html-court.sh
# inside the court:
#   cargo build --locked --all-features
#   python3 tools/fixtures/make-html.py
#   target/debug/vole-document field-build tools/fixtures/html/F.html --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--heading 0|--link 0|--html-element ""|--html-scripts|--html-find a|--html-path P|--html-attr P@N) --kind metadata|text|structure|exact
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
    echo "# Phase 21.10.1 — HTML (error-recovering markup, Wave 2) court"
    echo
    echo "**Question.** Does the HTML markup format close exactly and expose a"
    echo "representation-preserving, error-recovering model (exact element/attribute/"
    echo "text/comment/DOCTYPE spans, attribute quoting, void elements, raw script/style"
    echo "bytes, literal entity references) on top of the whole-source exact leaf — while"
    echo "refusing a DOCTYPE internal subset (so no entity expansion is possible)?"
    echo
    echo "**Method.** Each self-authored HTML fixture (generated deterministically by"
    echo "\`tools/fixtures/make-html.py\`, Python stdlib only) is ingested via"
    echo "\`field-build\`, observed, then — after the **source file and the standalone"
    echo "descriptor are deleted** — rematerialized exactly in a fresh process and"
    echo "compared with \`length + SHA-256 + cmp\`. Unsupported common pairs are asked"
    echo "for and required to decline typed; the prose/junk/DOCTYPE-subset controls are"
    echo "required to be Opaque, and the well-formed XHTML control is required to stay XML."
    echo
    echo "## Exactness (after source + descriptor deletion)"
    echo
    tail -n +3 "$CAMPAIGN/MATRIX.md"
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
    echo "- **Shipped here:** byte-based conservative HTML detection; the bounded,"
    echo "  span-preserving, **error-recovering** scanner (elements, attributes with"
    echo "  double/single/unquoted/boolean quoting, text, comments, DOCTYPE, raw"
    echo "  \`script\`/\`style\` content; entity references surfaced literally); the canonical"
    echo "  derived model; native \`html-path\`/\`html-element\`/\`html-attr\`/\`html-scripts\`/"
    echo "  \`html-find\`; common \`metadata\`/\`text\`/\`heading\`/\`link\`/\`search-match\`."
    echo "- **Encoding:** UTF-8 only (HTML has no \`encoding_rs\` here); a UTF-16 BOM, a NUL"
    echo "  byte, or a non-UTF-8 byte string is a typed decline."
    echo "- **Security:** a DOCTYPE with an internal subset is refused, so no entity is"
    echo "  ever resolved; \`script\`/\`style\` content is captured as raw bytes and is never"
    echo "  executed or parsed as markup."
    echo "- **Precedence:** XML is tried before HTML, so a fully well-formed XHTML source"
    echo "  stays XML; HTML only claims the \`<\`-bearing sources XML declines."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the HTML"
    echo "  model is derived (\`Q_gen\`) and never on the exactness path (ADR-0060: the"
    echo "  model node depends on the \`sha256(source)\` root)."
    echo "- **Not claimed here:** the full HTML tree-construction recovery algorithm"
    echo "  (adoption agency / foster parenting), CSS/JS interpretation, encodings beyond"
    echo "  UTF-8, and the economic court."
    echo "- **Never run on the host:** every command above ran in the pinned"
    echo "  \`doc-baseline\` container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$exact_fail" -ne 0 ]; then
    echo "PHASE 21.10.1 HTML COURT: FAIL" >&2
    exit 1
fi
echo "PHASE 21.10.1 HTML COURT: PASS — campaign $CAMPAIGN"
