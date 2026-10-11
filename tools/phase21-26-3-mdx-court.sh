#!/bin/sh
# Phase 21.26.3 — the MDX (Markdown + JSX/ESM) adapter court.
#
# Pre-registered (Phase-21 subphase 21.26.3). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture (including the Opaque, Markdown, and HTML
#       controls), `materialize(field) == source` (length + SHA-256 + cmp) after the
#       source file AND the standalone descriptor are deleted, in a fresh process. The
#       court FAILS unless this is 100% on every fixture.
#   H2 (surface + preserved representation) — the MDX-specific model is exposed on top
#       of the reused Markdown model: exact ESM `import`/`export` statement spans
#       (default/named/multi-line), JSX elements and fragments with their exact spans,
#       attributes, and nested children (recorded as extents, never executed and never
#       parsed as JavaScript), MDX `{ … }` expressions inline and block (brace-balanced,
#       including braces inside strings), a whole-line comment expression, and the full
#       Markdown surface (headings, paragraphs, lists, fenced code, tables, links,
#       reference definitions, footnotes). Common metadata/text/heading/block/
#       search-match and native
#       mdx-heading/mdx-block/mdx-esm/mdx-jsx/mdx-expression/mdx-find answer.
#   H3 (declines + boundaries) — an out-of-range native ESM/JSX/expression and an
#       unsupported common pair (the common `table`) decline typed (rc 6), never a
#       silent empty answer; a malformed `--mdx-heading` argument is a usage error
#       (rc 2); plain prose stays `opaque` (rc 6, never a panic); a **Markdown**
#       document (an ATX heading) stays `markdown`; a Markdown document whose **inline
#       code** contains JSX-looking text stays `markdown`; a Markdown document with an
#       **inline** `{x}` expression stays `markdown`; a plain **HTML** document stays
#       `html` and is never stolen by MDX.
#
# Runs in the pinned `dev` service using only POSIX `sh`, coreutils, git, and the
# shipped binary (no python3, no jq):
#   docker compose run --rm --no-TTY dev sh tools/phase21-26-3-mdx-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
# The `dev` lane is hard-capped on memory; keep the debug-info symbol footprint
# bounded so a full all-features link fits (symbols only — no semantics change).
# Overridable by the caller, and recorded in the receipt.
: "${CARGO_PROFILE_DEV_DEBUG:=0}"
export CARGO_PROFILE_DEV_DEBUG
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="2026-10-10"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-26-3-mdx-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-26-3
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
if ! cargo build --locked --all-features > "$RAW/build.log" 2>&1; then
    tail -40 "$RAW/build.log" >&2
    echo "PHASE 21.26.3 MDX COURT: FAIL — all-features build failed" >&2
    exit 1
fi
tail -2 "$RAW/build.log"

echo "=== generate fixtures (POSIX printf only) ==="
mk() { /usr/bin/printf '%b' "$2" > "$1"; }

# --- NORMAL: MDX documents (each carries an MDX-specific signal) -------------
# esm.mdx: a multi-line named import, an `export default`, and an `export const`.
mk "$WORK/corpus/esm.mdx" 'import React, {\n  useState,\n} from "react"\nexport default function App() { return null }\nexport const VERSION = "1.0"\n\n# ESM\n\nBody paragraph.\n'
# jsx.mdx: a component with attributes and a nested namespaced child plus a child
# expression, and a fragment.
mk "$WORK/corpus/jsx.mdx" '# JSX\n\n<Widget name="w" count={3}>\n  <Item.Child />\n  {value}\n</Widget>\n\n<>\nfrag\n</>\n'
# expr.mdx: an ESM import, a whole-line block expression, a comment expression, and an
# inline expression.
mk "$WORK/corpus/expr.mdx" 'import config from "./c"\n\n# Expr\n\n{frontmatter.title}\n\n{/* a comment */}\n\nText with {inline.value} inside.\n'
# mixed.mdx: JSX with Markdown children interleaved with prose and a block expression.
mk "$WORK/corpus/mixed.mdx" '# Mixed\n\nA paragraph before.\n\n<Card title={x}>\n  **bold** child text\n</Card>\n\n{summary}\n\nAfter.\n'
# surface.mdx: the full Markdown surface plus a JSX component signal.
mk "$WORK/corpus/surface.mdx" '# Surface\n\nA paragraph with *emphasis*, `code`, and [a link](http://ex).\n\n- item one\n- item two\n\n```rust\nlet x = 1;\n```\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n<Meta />\n\n[ref]: http://ref\n\nsee [text][ref].\n\n[^n]: a note\n'

# --- CONTROLS ---------------------------------------------------------------
# Plain prose: no MDX and no Markdown structural signal -> stays Opaque.
mk "$WORK/corpus/prose.txt" 'This is just some prose text.\nIt has several lines, with punctuation,\nbut no heading, list, or any markup at all.\n'
# A Markdown document (ATX heading) must stay Markdown.
mk "$WORK/corpus/markdown.md" '# Heading\n\nA paragraph with [link](http://x).\n'
# A Markdown document whose **inline code** contains JSX-looking text must stay
# Markdown (the MDX scanner protects inline code spans).
mk "$WORK/corpus/markdown_code.md" '# Heading\n\nuse `<Foo />` here\n'
# A Markdown document with an **inline** `{x}` expression must stay Markdown.
mk "$WORK/corpus/inline_expr.md" '# Heading\n\nvalue is {x} here\n'
# A plain HTML document must stay Html (never stolen by MDX).
mk "$WORK/corpus/html.html" '<!doctype html>\n<html><body><h1>Hi</h1></body></html>\n'
# A brace-bearing JSON-shaped blob with no Markdown structural mark must stay Opaque
# (a whole-line {…} alone is not an MDX signal).
mk "$WORK/corpus/braces.txt" '{"a":1}\n{oops}\n'

NORMAL="esm.mdx jsx.mdx expr.mdx mixed.mdx surface.mdx"
CONTROL="prose.txt markdown.md markdown_code.md inline_expr.md html.html braces.txt"
ALL="$NORMAL $CONTROL"

is_in() {
    for x in $2; do [ "$x" = "$1" ] && return 0; done
    return 1
}
# Extract one JSON string field from a single-line JSON object (greedy, no jq).
json_str() {
    printf '%s' "$1" | sed -n "s/.*\"$2\":\"\([^\"]*\)\".*/\1/p"
}

TSV="$RAW/results.tsv"
: > "$TSV"
printf 'fixture\tformat\tsrc_len\tsrc_sha\tout_len\tout_sha\tcmp\texact\tcontrol_rc\n' >> "$TSV"

exact_ok=0
exact_fail=0
surface_fail=0
normal_n=0
control_n=0
boundary_ok=0
opaque_ok=0
usage_ok=0
decline_ok=0

for f in $ALL; do
    # The reference copy is never deleted; the corpus copy is ingested and removed.
    src="$WORK/ref/$f"
    cp "$WORK/corpus/$f" "$src"
    src_len="$(wc -c < "$src" | tr -d ' ')"
    src_sha="$(sha256sum "$src" | cut -d' ' -f1)"

    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    field="$(json_str "$build_json" field)"
    fmt="$(json_str "$build_json" format)"

    control_rc=-1

    if is_in "$f" "$NORMAL"; then
        normal_n=$((normal_n + 1))
        [ "$fmt" = "mdx" ] || { echo "FINDING: $f detected as $fmt (want mdx)" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"format":"mdx"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata missing format" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --mdx-heading 0 --kind metadata \
            > "$RAW/$f.heading.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --mdx-block 0 --kind exact \
            > "$RAW/$f.block.exact.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --mdx-block 0 --kind metadata \
            > "$RAW/$f.block.meta.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --mdx-find a --kind text \
            > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))

        # Common selectors: heading/block/search-match resolve for an MDX document.
        "$BIN" observe --store "$WORK/store" --field "$field" --heading 0 --kind text \
            > "$RAW/$f.common.heading.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --block 0 --kind text \
            > "$RAW/$f.common.block.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --text a --kind text \
            > "$RAW/$f.common.search.json" 2>&1 || surface_fail=$((surface_fail + 1))

        # The whole-document text is the source itself (never re-flowed).
        grep -q '"text"' "$RAW/$f.text.json" || { echo "FINDING: $f doc-text" >&2; surface_fail=$((surface_fail + 1)); }
        # The exact block-0 observation carries the exact bytes.
        grep -q '"value_hex"' "$RAW/$f.block.exact.json" || { echo "FINDING: $f block0 exact" >&2; surface_fail=$((surface_fail + 1)); }

        case "$f" in
            esm.mdx)
                grep -q '"esm":3' "$RAW/$f.metadata.json" || { echo "FINDING: esm count" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"has_esm":true' "$RAW/$f.metadata.json" || { echo "FINDING: esm signal" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --mdx-esm 0 --kind metadata \
                    > "$RAW/$f.esm0.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"kind":"import"' "$RAW/$f.esm0.json" || { echo "FINDING: esm0 import" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --mdx-esm 1 --kind metadata \
                    > "$RAW/$f.esm1.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"kind":"export"' "$RAW/$f.esm1.json" || { echo "FINDING: esm1 export" >&2; surface_fail=$((surface_fail + 1)); } ;;
            jsx.mdx)
                grep -q '"jsx":3' "$RAW/$f.metadata.json" || { echo "FINDING: jsx count" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"has_component":true' "$RAW/$f.metadata.json" || { echo "FINDING: component signal" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"has_fragment":true' "$RAW/$f.metadata.json" || { echo "FINDING: fragment signal" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"expressions":1' "$RAW/$f.metadata.json" || { echo "FINDING: jsx expr count" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --mdx-jsx 0 --kind metadata \
                    > "$RAW/$f.jsx0.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"name":"Widget"' "$RAW/$f.jsx0.json" || { echo "FINDING: jsx0 name" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"component":true' "$RAW/$f.jsx0.json" || { echo "FINDING: jsx0 component" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"jsx_attr":true' "$RAW/$f.jsx0.json" || { echo "FINDING: jsx0 attr" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"attrs":2' "$RAW/$f.jsx0.json" || { echo "FINDING: jsx0 attrs" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"children":1' "$RAW/$f.jsx0.json" || { echo "FINDING: jsx0 children" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --mdx-jsx 1 --kind metadata \
                    > "$RAW/$f.jsx1.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"namespaced":true' "$RAW/$f.jsx1.json" || { echo "FINDING: jsx1 namespaced" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"self_closing":true' "$RAW/$f.jsx1.json" || { echo "FINDING: jsx1 self-closing" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --mdx-jsx 2 --kind metadata \
                    > "$RAW/$f.jsx2.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"kind":"fragment"' "$RAW/$f.jsx2.json" || { echo "FINDING: jsx2 fragment" >&2; surface_fail=$((surface_fail + 1)); } ;;
            expr.mdx)
                grep -q '"expressions":3' "$RAW/$f.metadata.json" || { echo "FINDING: expr count" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"has_block_expression":true' "$RAW/$f.metadata.json" || { echo "FINDING: block expr signal" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --mdx-esm 0 --kind metadata \
                    > "$RAW/$f.esm0.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"kind":"import"' "$RAW/$f.esm0.json" || { echo "FINDING: expr esm0" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --mdx-expression 1 --kind metadata \
                    > "$RAW/$f.expr1.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"comment":true' "$RAW/$f.expr1.json" || { echo "FINDING: expr comment" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --mdx-expression 0 --kind metadata \
                    > "$RAW/$f.expr0.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"block":true' "$RAW/$f.expr0.json" || { echo "FINDING: expr0 block" >&2; surface_fail=$((surface_fail + 1)); } ;;
            mixed.mdx)
                grep -q '"jsx":1' "$RAW/$f.metadata.json" || { echo "FINDING: mixed jsx count" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"expressions":1' "$RAW/$f.metadata.json" || { echo "FINDING: mixed expr count" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --mdx-jsx 0 --kind metadata \
                    > "$RAW/$f.jsx0.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"name":"Card"' "$RAW/$f.jsx0.json" || { echo "FINDING: mixed jsx0 name" >&2; surface_fail=$((surface_fail + 1)); } ;;
            surface.mdx)
                grep -q '"headings":1' "$RAW/$f.metadata.json" || { echo "FINDING: surface headings" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"tables":1' "$RAW/$f.metadata.json" || { echo "FINDING: surface tables" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"code_blocks":1' "$RAW/$f.metadata.json" || { echo "FINDING: surface code" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"list_items":2' "$RAW/$f.metadata.json" || { echo "FINDING: surface list" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"ref_defs":1' "$RAW/$f.metadata.json" || { echo "FINDING: surface ref" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"footnotes":1' "$RAW/$f.metadata.json" || { echo "FINDING: surface footnote" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"jsx":1' "$RAW/$f.metadata.json" || { echo "FINDING: surface jsx" >&2; surface_fail=$((surface_fail + 1)); } ;;
        esac

        # H3: an out-of-range native selector and an unsupported common pair decline
        # typed (rc 6), once.
        if [ "$f" = "jsx.mdx" ]; then
            set +e
            "$BIN" observe --store "$WORK/store" --field "$field" --mdx-esm 99 --kind metadata \
                > /dev/null 2>&1
            d_rc1=$?
            "$BIN" observe --store "$WORK/store" --field "$field" --table 0 --kind text \
                > /dev/null 2>&1
            d_rc2=$?
            set -e
            [ "$d_rc1" -eq 6 ] && [ "$d_rc2" -eq 6 ] && decline_ok=$((decline_ok + 1)) \
                || echo "FINDING: typed declines rc=$d_rc1/$d_rc2 (want 6/6)" >&2
        fi
    else
        control_n=$((control_n + 1))
        case "$f" in
            prose.txt)
                [ "$fmt" = "opaque" ] || { echo "FINDING: $f detected as $fmt (want opaque)" >&2; surface_fail=$((surface_fail + 1)); }
                set +e
                "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
                    > "$RAW/$f.metadata.json" 2>&1
                control_rc=$?
                set -e
                [ "$control_rc" -eq 6 ] && opaque_ok=$((opaque_ok + 1)) || { echo "FINDING: opaque $f metadata rc=$control_rc (want 6)" >&2; }
                boundary_ok=$((boundary_ok + 1)) ;;
            markdown.md)
                [ "$fmt" = "markdown" ] || { echo "FINDING: $f detected as $fmt (want markdown)" >&2; surface_fail=$((surface_fail + 1)); }
                boundary_ok=$((boundary_ok + 1)) ;;
            markdown_code.md)
                [ "$fmt" = "markdown" ] || { echo "FINDING: $f detected as $fmt (want markdown)" >&2; surface_fail=$((surface_fail + 1)); }
                boundary_ok=$((boundary_ok + 1)) ;;
            inline_expr.md)
                [ "$fmt" = "markdown" ] || { echo "FINDING: $f detected as $fmt (want markdown)" >&2; surface_fail=$((surface_fail + 1)); }
                boundary_ok=$((boundary_ok + 1)) ;;
            html.html)
                [ "$fmt" = "html" ] || { echo "FINDING: $f detected as $fmt (want html)" >&2; surface_fail=$((surface_fail + 1)); }
                boundary_ok=$((boundary_ok + 1)) ;;
            braces.txt)
                [ "$fmt" = "opaque" ] || { echo "FINDING: $f detected as $fmt (want opaque)" >&2; surface_fail=$((surface_fail + 1)); }
                boundary_ok=$((boundary_ok + 1)) ;;
        esac
    fi

    # A malformed `--mdx-heading` argument is a usage error (rc 2), once.
    if [ "$f" = "esm.mdx" ]; then
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --mdx-heading abc --kind metadata \
            > /dev/null 2>&1
        u_rc=$?
        set -e
        [ "$u_rc" -eq 2 ] && usage_ok=$((usage_ok + 1)) || echo "FINDING: malformed --mdx-heading rc=$u_rc (want 2)" >&2
    fi

    # H1: delete the source AND the standalone descriptor, then rematerialize exactly
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

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$f" "$fmt" "$src_len" "$src_sha" "$out_len" "$out_sha" "$cmp_ok" "$exact" \
        "$control_rc" >> "$TSV"
done

echo "=== exactness after source + descriptor deletion ==="
cat "$TSV"

fixtures_n="$(printf '%s\n' $ALL | wc -w | tr -d ' ')"

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 21.26.3 MDX court — matrix"
    echo
    echo "| fixture | format | src len | out len | sha256 match | cmp | exact | control rc |"
    echo "| --- | --- | ---: | ---: | --- | --- | --- | ---: |"
    # shellcheck disable=SC2162
    while IFS='	' read -r a b c d e g h i j; do
        [ "$a" = "fixture" ] && continue
        [ -z "$a" ] && continue
        if [ "$d" = "$g" ]; then sh=true; else sh=false; fi
        echo "| \`$a\` | $b | $c | $e | $sh | $h | $i | $j |"
    done < "$TSV"
} > "$CAMPAIGN/MATRIX.md"

# --- counts.txt -------------------------------------------------------------
{
    echo "fixtures $fixtures_n"
    echo "mdx_fixtures $normal_n"
    echo "control_fixtures $control_n"
    echo "exact_ok $exact_ok"
    echo "exact_fail $exact_fail"
    echo "surface_fail $surface_fail"
    echo "boundary_ok $boundary_ok"
    echo "opaque_controls_ok $opaque_ok"
    echo "typed_declines_ok $decline_ok"
    echo "usage_ok $usage_ok"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.26.3 — MDX (Markdown + JSX/ESM) surface + exact closure + model",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "git_dirty": "$(git status --porcelain | tr '\n' ' ' | sed 's/"/\\"/g')",
  "arch": "$(uname -m)",
  "service": "dev",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "bin": "$BIN",
  "bin_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "profile": "debug",
  "fixtures_tool": "POSIX shell + GNU /usr/bin/printf (no python3, no jq); generated in-court, never committed",
  "env_affecting_semantics": {"LC_ALL": "C", "CARGO_PROFILE_DEV_DEBUG": "$CARGO_PROFILE_DEV_DEBUG"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
if [ "$exact_fail" -ne 0 ] || [ "$surface_fail" -ne 0 ] \
    || [ "$boundary_ok" -ne 6 ] || [ "$opaque_ok" -ne 1 ] \
    || [ "$decline_ok" -ne 1 ] || [ "$usage_ok" -ne 1 ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.26.3 — MDX (Markdown + JSX/ESM) surface + exact closure + model",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "measured_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "tree_state": "$(git status --porcelain | tr '\n' ' ' | sed 's/"/\\"/g')",
  "service": "dev",
  "profile": "debug",
  "bin": "$BIN",
  "bin_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "fixtures": "$ALL",
  "fixture_tool": "POSIX shell + GNU /usr/bin/printf (in-court; not committed)",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion, for every fixture including the Opaque, Markdown, inline-code-Markdown, inline-expression-Markdown, brace-blob, and HTML controls",
  "mdx": "a superset adapter that REUSES the Markdown parser/model (never a second prose parser) and layers exact-span MDX constructs on top: top-level ESM import/export statements (default/named/multi-line), JSX elements and fragments with their attributes and nested children (recorded as extents, never executed and never parsed as JavaScript), and MDX { … } expressions inline and block (brace-balanced with string literals, escapes, and comments respected); the full Markdown surface (headings, paragraphs, lists, fenced code, tables, links, reference definitions, footnotes, front matter) is preserved by the reused model; never re-flowed or normalized",
  "observations": ["metadata", "text", "heading", "block", "search-match", "native mdx-heading", "native mdx-block", "native mdx-esm", "native mdx-jsx", "native mdx-expression", "native mdx-find"],
  "declines": "an out-of-range native ESM/JSX/expression declines typed (rc 6); an unsupported common pair (table) declines typed (rc 6); a malformed --mdx-heading argument is a usage error (rc 2); plain prose stays opaque (rc 6), never a panic",
  "detection_boundary": "MDX detection is tried BEFORE generic Markdown and before HTML, and requires an MDX-SPECIFIC signal on top of a successful Markdown parse: a top-level ESM import/export statement, a JSX component (a capitalized/namespaced element name) or fragment, a JSX-specific attribute (a brace value or spread), or a whole-line MDX block expression {…}. The scanner skips fenced/indented code, front matter, and inline code spans, so a JSX/ESM construct inside code never makes a Markdown document MDX. A plain Markdown document stays Markdown, a plain HTML document stays Html, and plain prose stays Opaque.",
  "cannot_distinguish": "a plain HTML element with only lowercase tags and quoted attributes (<div class=\"x\">…</div>) is indistinguishable from the same JSX and carries no MDX signal, so MDX declines it and it stays Html; only a capitalized/namespaced name or a JSX-specific attribute distinguishes JSX from HTML. An INLINE {…} expression alone does not admit a document as MDX (plain prose is full of balanced braces), so a Markdown document with an inline {x} stays Markdown; only a whole-line block expression (or a component/ESM/JSX-attribute signal) admits it. A component/capitalized tag inside a Markdown table cell likewise admits MDX (a documented over-approximation).",
  "precedence": "MDX detection runs immediately before Markdown and before HTML/XML; it also declines anything already claimed by JSON/YAML/CSV/HTML/XML/RST/AsciiDoc (defence in depth)",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 42 invalid-mdx-structure",
  "adr_0060": "the MdxModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase21-26-3-mdx-court.sh
# inside the court:
#   cargo build --locked --all-features
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--heading N|--block N|--text PAT|--mdx-heading N|--mdx-block N|--mdx-esm N|--mdx-jsx N|--mdx-expression N|--mdx-find PAT|--table N) --kind metadata|text|exact
#   rm source F.voldoc
#   target/debug/vole-document materialize --store STORE --field HEX --exact --output OUT ; cmp OUT source
# Regression controls (MDX must not break Markdown/HTML):
#   docker compose run --rm --no-TTY dev sh tools/phase21-8-1-markdown-court.sh
#   docker compose run --rm --no-TTY doc-baseline sh tools/phase21-26-2-asciidoc-court.sh
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
    echo "# Phase 21.26.3 — MDX (Markdown + JSX/ESM) court"
    echo
    echo "**Question.** Does MDX — Markdown with JSX and ESM layered on top — close"
    echo "exactly and expose a representation-preserving model (exact ESM/JSX/expression"
    echo "spans, JSX attributes and nested children, fragments, brace-balanced inline and"
    echo "block expressions, and the full reused Markdown surface) on top of the"
    echo "whole-source exact leaf, while keeping an MDX-**specific** detection boundary"
    echo "(plain prose stays Opaque; a Markdown document stays Markdown, even with"
    echo "JSX-looking inline code or an inline \`{x}\`; a plain HTML document stays Html)?"
    echo
    echo "**Method.** Each self-authored fixture is generated in-court with GNU"
    echo "\`/usr/bin/printf\`, ingested via \`field-build\`, observed, then — after the"
    echo "**source file and the standalone descriptor are deleted** — rematerialized"
    echo "exactly in a fresh process and compared with \`length + SHA-256 + cmp\`. An"
    echo "out-of-range native selector and an unsupported common pair are required to"
    echo "decline typed; a malformed native argument is a usage error; the prose,"
    echo "Markdown, inline-code, inline-expression, and HTML controls pin the detection"
    echo "boundaries. The court runs in the pinned \`dev\` service using only POSIX \`sh\`,"
    echo "coreutils, git, and the shipped binary (no python3, no jq)."
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
    echo "- **Shipped here:** byte-based, conservative, MDX-specific detection before"
    echo "  Markdown and HTML; the reuse of the Markdown parser/model for the prose"
    echo "  surface; the exact-span ESM/JSX/expression arenas; the canonical derived"
    echo "  model; native \`mdx-heading\`/\`mdx-block\`/\`mdx-esm\`/\`mdx-jsx\`/"
    echo "  \`mdx-expression\`/\`mdx-find\`; common \`metadata\`/\`text\`/\`heading\`/\`block\`/"
    echo "  \`search-match\`."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the MDX model"
    echo "  is derived (\`Q_gen\`) and never on the exactness path (ADR-0060: the model node"
    echo "  depends on the \`sha256(source)\` root, so no source-reading node aliases another"
    echo "  field's source)."
    echo "- **The source is never re-flowed or rendered.** A block's, element's, or"
    echo "  expression's exact bytes are literally \`source[span]\`; the canonical text"
    echo "  projection is the source itself."
    echo "- **JSX is never executed and never parsed as JavaScript**; only extents, names,"
    echo "  attribute counts, and child counts are recorded."
    echo "- **The boundary is honest:** a lowercase, quoted-attribute-only HTML element is"
    echo "  indistinguishable from the same JSX and stays Html; an inline \`{…}\` alone does"
    echo "  not admit MDX; JSX-looking text inside fenced/indented code or inline code spans"
    echo "  is never an MDX signal."
    echo "- **Not claimed here:** a full MDX/JSX conformance oracle, nor JavaScript"
    echo "  evaluation."
    echo "- **Never run on the host:** every command above ran in the pinned \`dev\`"
    echo "  container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" != "PASS" ]; then
    echo "PHASE 21.26.3 MDX COURT: FAIL" >&2
    exit 1
fi
echo "PHASE 21.26.3 MDX COURT: PASS — campaign $CAMPAIGN"
