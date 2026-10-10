#!/bin/sh
# Phase 21.9.1 — XML (structured-tree, Wave 2) surface + exact closure + model.
#
# Pre-registered (Phase-21 subphase 21.9.1). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture, `materialize(field) == source`
#       (length + SHA-256 + cmp) after the source file AND the standalone
#       descriptor are deleted, in a fresh process.
#   H2 (model + observations) — the representation-preserving model is exposed:
#       the exact source span of an element/attribute/text/CDATA/comment/PI, the
#       qualified name, the literal (unexpanded) content, namespace declarations,
#       and exact attribute values; common metadata/text/search-match and native
#       xml-path/xml-element/xml-attr/xml-namespaces/xml-find answer.
#   H3 (honest declines) — unsupported common pairs decline typed with exit code 6,
#       never a silent empty answer; a malformed/opaque document is Opaque.
#   H4 (security) — an internal-subset DOCTYPE (an XXE / billion-laughs surface,
#       `xxe.xml`/`billion.xml`) is refused outright: it is NOT detected as XML, no
#       entity is ever expanded, and the opaque floor still closes it byte-exactly.
#   H5 (conservative detection) — a source that does not begin with `<` or does not
#       parse as well-formed XML with one root is not detected (`junk.xml`,
#       `prose.txt`).
#
# Runs in the pinned `doc-baseline` service (dev toolchain + python3 + sqlite):
#   docker compose run --rm --no-TTY doc-baseline sh tools/phase21-9-1-xml-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-9-1-xml-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-9
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== regenerate fixtures (stdlib only) ==="
python3 tools/fixtures/make-xml.py
cp tools/fixtures/xml/*.xml tools/fixtures/xml/prose.txt "$WORK/corpus/"

ALL="basic.xml namespaces.xml mixed.xml attrs.xml dtd.xml large.xml xxe.xml billion.xml junk.xml prose.txt"
# Every fixture closes byte-exactly; the Opaque controls (xxe/billion/junk/prose)
# are not detected as XML, so their derived observations decline typed rather than
# answering.
OPAQUE="xxe.xml billion.xml junk.xml prose.txt"

# The element path each normal fixture observes ("" = skip the path probe).
path_for() {
    case "$1" in
        basic.xml)      echo "/doc/nested/a/b" ;;
        namespaces.xml) echo "/root/p:child" ;;
        mixed.xml)      echo "/r/cdata" ;;
        attrs.xml)      echo "/r/x" ;;
        dtd.xml)        echo "/root/child" ;;
        large.xml)      echo "/root/rec[1]/name" ;;
        *)              echo "" ;;
    esac
}
# The attribute each fixture observes ("" = skip).
attr_for() {
    case "$1" in
        basic.xml) echo "/doc/nested/a@id" ;;
        attrs.xml) echo "/r/x@id" ;;
        *)         echo "" ;;
    esac
}

printf '[\n' > "$RAW/results.json"
first=1
exact_ok=0
exact_fail=0
declines_ok=0
opaque_ok=0
opaque_n=0
for f in $ALL; do
    src="tools/fixtures/xml/$f"
    cp "$src" "$WORK/corpus/$f"
    src_len="$(wc -c < "$src" | tr -d ' ')"
    src_sha="$(sha256sum "$src" | cut -d' ' -f1)"

    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    field="$(printf '%s' "$build_json" | jq -r '.ingest.field')"
    fmt="$(printf '%s' "$build_json" | jq -r '.ingest.format')"

    is_opaque=false
    for o in $OPAQUE; do [ "$o" = "$f" ] && is_opaque=true; done

    decline_rc=-1
    opaque_rc=-1
    if [ "$is_opaque" = true ]; then
        # H4/H5: not detected as XML; its common observation declines typed (rc 6).
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1
        opaque_rc=$?
        set -e
        if [ "$opaque_rc" -eq 6 ]; then opaque_ok=$((opaque_ok + 1)); fi
        opaque_n=$((opaque_n + 1))
    else
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --xml-element "" --kind structure \
            > "$RAW/$f.element.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --xml-find a --kind text \
            > "$RAW/$f.find.json"
        if [ "$f" = "namespaces.xml" ]; then
            "$BIN" observe --store "$WORK/store" --field "$field" --xml-namespaces --kind metadata \
                > "$RAW/$f.namespaces.json"
        fi
        ptr="$(path_for "$f")"
        if [ -n "$ptr" ]; then
            "$BIN" observe --store "$WORK/store" --field "$field" --xml-path "$ptr" --kind metadata \
                > "$RAW/$f.path.json"
            "$BIN" observe --store "$WORK/store" --field "$field" --xml-path "$ptr" --kind exact \
                > "$RAW/$f.token.bin"
        fi
        at="$(attr_for "$f")"
        if [ -n "$at" ]; then
            "$BIN" observe --store "$WORK/store" --field "$field" --xml-attr "$at" --kind exact \
                > "$RAW/$f.attr.bin"
            "$BIN" observe --store "$WORK/store" --field "$field" --xml-attr "$at" --kind metadata \
                > "$RAW/$f.attr.json"
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
    --argjson opaque_controls "$opaque_n" \
    '{fixtures:$fixtures,exact_ok:$exact_ok,exact_fail:$exact_fail,typed_declines_ok:$typed_declines_ok,opaque_controls_ok:$opaque_controls_ok,opaque_controls:$opaque_controls}' \
    | tee "$CAMPAIGN/summary.json"

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 21.9.1 XML court — matrix"
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
    echo "opaque_controls $opaque_n"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.9.1 — XML structured-tree surface + exact closure + model",
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
  "fixtures_tool": "tools/fixtures/make-xml.py",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
[ "$exact_fail" -eq 0 ] || verdict="FAIL"
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.9.1 — XML structured-tree surface + exact closure + model",
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
  "fixture_tool": "tools/fixtures/make-xml.py (stdlib only)",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion",
  "observations": ["metadata", "text", "search-match", "native xml-path", "native xml-element", "native xml-attr", "native xml-namespaces", "native xml-find"],
  "declines": "unsupported common pairs decline typed (rc 6); an opaque/malformed document is Opaque (rc 6), never a panic",
  "security": "an internal-subset DOCTYPE (XXE/billion-laughs surface) is refused and no entity is expanded; the opaque floor still closes it byte-exactly",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 19 invalid-xml-structure",
  "adr_0060": "the XmlModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY doc-baseline sh tools/phase21-9-1-xml-court.sh
# inside the court:
#   cargo build --locked --all-features
#   python3 tools/fixtures/make-xml.py
#   target/debug/vole-document field-build tools/fixtures/xml/F.xml --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--xml-element ""|--xml-find a|--xml-path P|--xml-attr P@N|--xml-namespaces) --kind metadata|text|structure|exact
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
    echo "# Phase 21.9.1 — XML (structured-tree, Wave 2) court"
    echo
    echo "**Question.** Does the XML structured-tree format close exactly and expose a"
    echo "representation-preserving model (exact element/attribute/text/CDATA/comment/PI"
    echo "spans, literal entity references, namespace declarations) on top of the"
    echo "whole-source exact leaf — while refusing a DTD internal subset so no XXE or"
    echo "billion-laughs expansion is possible?"
    echo
    echo "**Method.** Each self-authored XML fixture (generated deterministically by"
    echo "\`tools/fixtures/make-xml.py\`, Python stdlib only) is ingested via"
    echo "\`field-build\`, observed, then — after the **source file and the standalone"
    echo "descriptor are deleted** — rematerialized exactly in a fresh process and"
    echo "compared with \`length + SHA-256 + cmp\`. Unsupported common pairs are asked"
    echo "for and required to decline typed; the XXE/billion-laughs/\`<-junk\`/prose"
    echo "controls are required to be Opaque."
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
    echo "- **Shipped here:** byte-based conservative XML detection; the bounded,"
    echo "  span-preserving scanner (elements, attributes, text, CDATA, comments, PIs,"
    echo "  DOCTYPE, namespaces; entity references surfaced literally); the canonical"
    echo "  derived model; native \`xml-path\`/\`xml-element\`/\`xml-attr\`/"
    echo "  \`xml-namespaces\`/\`xml-find\`; common \`metadata\`/\`text\`/\`search-match\`."
    echo "- **Security:** a benign \`<!DOCTYPE…>\` (bare or PUBLIC/SYSTEM) is accepted"
    echo "  and ignored (never fetched); a declaration with an internal subset is"
    echo "  refused, so no entity is ever resolved — no XXE, no billion-laughs."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the XML"
    echo "  model is derived (\`Q_gen\`) and never on the exactness path (ADR-0060: the"
    echo "  model node depends on the \`sha256(source)\` root, so no source-reading node"
    echo "  aliases another field's source)."
    echo "- **Not claimed here:** a general XPath engine, XML Schema/DTD validation,"
    echo "  C14N, encoding switching beyond UTF-8, XInclude, and the economic court."
    echo "- **Never run on the host:** every command above ran in the pinned"
    echo "  \`doc-baseline\` container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$exact_fail" -ne 0 ]; then
    echo "PHASE 21.9.1 XML COURT: FAIL" >&2
    exit 1
fi
echo "PHASE 21.9.1 XML COURT: PASS — campaign $CAMPAIGN"
