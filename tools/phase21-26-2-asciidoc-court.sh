#!/bin/sh
# Phase 21.26.2 — the AsciiDoc (Asciidoctor input language) prose adapter court.
#
# Pre-registered (Phase-21 subphase 21.26.2). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture (including the Opaque, Markdown, and
#       reStructuredText controls), `materialize(field) == source` (length + SHA-256 +
#       cmp) after the source file AND the standalone descriptor are deleted, in a
#       fresh process. The court FAILS unless this is 100% on every fixture.
#   H2 (surface + preserved representation) — the AsciiDoc-specific model is exposed:
#       exact block/inline source spans, a level-0 document title and `==`+ sections
#       with their exact `=` marker and recorded level, document attributes
#       (`:name: value` / `:name!:`) with attribute references (`{name}`) surfaced
#       literally (never expanded), block attribute lines attached to the following
#       block, delimited blocks (listing/literal/example/sidebar/quote/open/
#       passthrough) with their exact delimiter and verbatim content,
#       unordered/ordered/description lists with nesting, tables (`|===` with `|` cell
#       markers), admonitions (`NOTE:` and `[NOTE]`), and inline markup (strong/
#       emphasis/mono/passthrough/superscript/subscript/mark) plus the `link:`,
#       `image:`, `include::`, `xref:` and bare-URL macros preserved verbatim. Common
#       metadata/text/heading/block/search-match and native
#       adoc-heading/adoc-block/adoc-attribute/adoc-inline/adoc-find answer.
#   H3 (declines + boundaries) — an out-of-range native heading and an unsupported
#       common pair (the common `table`, which is not in AsciiDoc's capability set)
#       decline typed (rc 6), never a silent empty answer; a malformed `--adoc-heading`
#       argument is a usage error (rc 2); plain prose stays `opaque` (rc 6, never a
#       panic); a **Markdown** document (a GFM table + ATX heading) stays `markdown`
#       and is never stolen by AsciiDoc; a **reStructuredText** document stays `rst`.
#
# Runs in the pinned `dev` service using only POSIX `sh`, coreutils, git, and the
# shipped binary (no python3, no jq):
#   docker compose run --rm --no-TTY dev sh tools/phase21-26-2-asciidoc-court.sh

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
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-26-2-asciidoc-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-26-2
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
if ! cargo build --locked --all-features > "$RAW/build.log" 2>&1; then
    tail -40 "$RAW/build.log" >&2
    echo "PHASE 21.26.2 ASCIIDOC COURT: FAIL — all-features build failed" >&2
    exit 1
fi
tail -2 "$RAW/build.log"

echo "=== generate fixtures (POSIX printf only) ==="
mk() { /usr/bin/printf '%b' "$2" > "$1"; }

# --- NORMAL: AsciiDoc documents (each carries an AsciiDoc-specific signal) ----
# basic.adoc: a level-0 document title, two sections (`==`/`===`), and a body with
# inline markup.
mk "$WORK/corpus/basic.adoc" '= AsciiDoc Primer\n\nA paragraph with *strong*, _emphasis_, and `mono` text.\n\n== First Section\n\nA body paragraph with +pass+ and ^sup^ and ~sub~ and #mark#.\n\n=== Subsection\n\nMore body text.\n'
# attributes.adoc: two document attributes (`:name:value`, the no-space spelling that
# avoids the reST field-list overlap) and a literal `{ref}`.
mk "$WORK/corpus/attributes.adoc" '= Attributes\n\n:author:Jane Doe\n:version:1.0\n\nThe attribute {author} stays literal.\n'
# delimited.adoc: every delimited block kind (bodies span two lines).
mk "$WORK/corpus/delimited.adoc" '= Delimited\n\n----\nlisting one\nlisting two\n----\n\n....\nliteral one\nliteral two\n....\n\n====\nexample one\nexample two\n====\n\n****\nsidebar one\nsidebar two\n****\n\n____\nquote one\nquote two\n____\n\n--\nopen one\nopen two\n--\n\n++++\npassthrough one\npassthrough two\n++++\n'
# lists.adoc: unordered, ordered, and description lists plus a `[source,rust]` block
# attribute attached to a listing block.
mk "$WORK/corpus/lists.adoc" '= Lists\n\n* item one\n* item two\n\n. first\n. second\n\nterm:: a definition body\n\n[source,rust]\n----\nfn main() {}\n----\n'
# tables.adoc: a `[cols="2"]`-attributed `|===` table with `|` cell markers.
mk "$WORK/corpus/tables.adoc" '= Tables\n\n[cols="2"]\n|===\n| Name | Value\n| alpha | 1\n| beta | 2\n|===\n'
# inline.adoc: every inline markup kind plus every macro.
mk "$WORK/corpus/inline.adoc" '= Inline\n\nText with *strong*, _emphasis_, `mono`, +pass+, ^sup^, ~sub~, #mark#, and ##marked##.\n\nSee link:https://example.org[Example] and image:logo.png[Logo] and include::chapter.adoc[] and xref:sec-1[One] and https://bare.example[Site] and {name}.\n'
# admonitions.adoc: a `NOTE:`/`WARNING:` paragraph and a `[TIP]` block admonition.
mk "$WORK/corpus/admonitions.adoc" '= Admonitions\n\nNOTE: This is a note.\n\n[TIP]\nA block tip body.\n\nWARNING: Careful now.\n'

# --- CONTROLS ---------------------------------------------------------------
# Plain prose: no AsciiDoc structural mark -> stays Opaque.
mk "$WORK/corpus/prose.txt" 'This is just some prose text.\nIt has several lines, with punctuation,\nbut no AsciiDoc title, section, table, or delimited block at all.\n'
# A Markdown document (ATX heading + GFM table) must stay Markdown.
mk "$WORK/corpus/markdown.md" '# Heading\n\nA paragraph with [link](http://x).\n\n| a | b |\n| - | - |\n| 1 | 2 |\n'
# A reStructuredText document must stay Rst (reST is tried before AsciiDoc).
mk "$WORK/corpus/rest.rst" 'Title\n=====\n\n.. note:: a directive body\n'
# A spaced-attribute AsciiDoc-shaped source is a reST field list, so reST (tried
# first) claims it: an explicit boundary control.
mk "$WORK/corpus/attributes_spaced.adoc" '= Attributes\n\n:author: Jane Doe\n\nThe attribute {author} stays literal.\n'

NORMAL="basic.adoc attributes.adoc delimited.adoc lists.adoc tables.adoc inline.adoc admonitions.adoc"
CONTROL="prose.txt markdown.md rest.rst attributes_spaced.adoc"
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
printf 'fixture\tformat\tsrc_len\tsrc_sha\tout_len\tout_sha\tcmp\texact\tdecline_rc\tcontrol_rc\n' >> "$TSV"

exact_ok=0
exact_fail=0
surface_fail=0
normal_n=0
control_n=0
decline_ok=0
usage_ok=0
boundary_ok=0
opaque_ok=0

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

    decline_rc=-1
    control_rc=-1

    if is_in "$f" "$NORMAL"; then
        normal_n=$((normal_n + 1))
        [ "$fmt" = "asciidoc" ] || { echo "FINDING: $f detected as $fmt (want asciidoc)" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"format":"asciidoc"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata missing format" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --adoc-heading 0 --kind metadata \
            > "$RAW/$f.heading.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --adoc-block 0 --kind exact \
            > "$RAW/$f.block.exact.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --adoc-block 0 --kind metadata \
            > "$RAW/$f.block.meta.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --adoc-find a --kind text \
            > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --adoc-attribute 0 --kind metadata \
            > "$RAW/$f.attribute.json" 2>&1 || { :; }

        # A native inline span resolves when the document carries inline markup.
        if is_in "$f" "basic.adoc attributes.adoc inline.adoc tables.adoc"; then
            "$BIN" observe --store "$WORK/store" --field "$field" --adoc-inline 0 --kind metadata \
                > "$RAW/$f.inline.json" 2>&1 || { echo "FINDING: $f adoc-inline" >&2; surface_fail=$((surface_fail + 1)); }
            grep -q '"inline":0' "$RAW/$f.inline.json" || { echo "FINDING: $f adoc-inline ordinal" >&2; surface_fail=$((surface_fail + 1)); }
        fi

        # Common selectors: heading/block/search-match resolve for an AsciiDoc document.
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
            basic.adoc)
                grep -q '"doc_title":1' "$RAW/$f.metadata.json" || { echo "FINDING: basic doc_title" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"sections":2' "$RAW/$f.metadata.json" || { echo "FINDING: basic sections" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"max_section_level":2' "$RAW/$f.metadata.json" || { echo "FINDING: basic hierarchy" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"document_title":true' "$RAW/$f.heading.json" || { echo "FINDING: basic doc title level" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"strong":1' "$RAW/$f.metadata.json" || { echo "FINDING: basic strong" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"marks":1' "$RAW/$f.metadata.json" || { echo "FINDING: basic mark" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --adoc-heading 1 --kind metadata \
                    > "$RAW/$f.heading1.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"level":1' "$RAW/$f.heading1.json" || { echo "FINDING: basic section level" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"marker":"=="' "$RAW/$f.heading1.json" || { echo "FINDING: basic section marker" >&2; surface_fail=$((surface_fail + 1)); } ;;
            attributes.adoc)
                grep -q '"attributes":2' "$RAW/$f.metadata.json" || { echo "FINDING: attributes count" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"attribute_refs":1' "$RAW/$f.metadata.json" || { echo "FINDING: attributes refs" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"name":"author"' "$RAW/$f.attribute.json" || { echo "FINDING: attributes name" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"value":"Jane Doe"' "$RAW/$f.attribute.json" || { echo "FINDING: attributes value" >&2; surface_fail=$((surface_fail + 1)); } ;;
            delimited.adoc)
                grep -q '"listings":1' "$RAW/$f.metadata.json" || { echo "FINDING: delim listing" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"literals":1' "$RAW/$f.metadata.json" || { echo "FINDING: delim literal" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"examples":1' "$RAW/$f.metadata.json" || { echo "FINDING: delim example" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"sidebars":1' "$RAW/$f.metadata.json" || { echo "FINDING: delim sidebar" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"quotes":1' "$RAW/$f.metadata.json" || { echo "FINDING: delim quote" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"opens":1' "$RAW/$f.metadata.json" || { echo "FINDING: delim open" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"passthrough_blocks":1' "$RAW/$f.metadata.json" || { echo "FINDING: delim passthrough" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"delimited":7' "$RAW/$f.metadata.json" || { echo "FINDING: delim total" >&2; surface_fail=$((surface_fail + 1)); } ;;
            lists.adoc)
                grep -q '"list_items":5' "$RAW/$f.metadata.json" || { echo "FINDING: lists items" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"block_attrs":1' "$RAW/$f.metadata.json" || { echo "FINDING: lists block_attrs" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"listings":1' "$RAW/$f.metadata.json" || { echo "FINDING: lists listing" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --adoc-block 1 --kind text \
                    > "$RAW/$f.block1.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q 'item one' "$RAW/$f.block1.json" || { echo "FINDING: lists item text" >&2; surface_fail=$((surface_fail + 1)); } ;;
            tables.adoc)
                grep -q '"tables":1' "$RAW/$f.metadata.json" || { echo "FINDING: tables count" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"table_cells":6' "$RAW/$f.metadata.json" || { echo "FINDING: tables cells" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --adoc-block 2 --kind metadata \
                    > "$RAW/$f.table.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"rows":3' "$RAW/$f.table.json" || { echo "FINDING: tables rows" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"cols":2' "$RAW/$f.table.json" || { echo "FINDING: tables cols" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q 'attrs' "$RAW/$f.table.json" || { echo "FINDING: tables attrs" >&2; surface_fail=$((surface_fail + 1)); } ;;
            inline.adoc)
                grep -q '"strong":1' "$RAW/$f.metadata.json" || { echo "FINDING: inline strong" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"marks":1' "$RAW/$f.metadata.json" || { echo "FINDING: inline mark" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"mark_double":1' "$RAW/$f.metadata.json" || { echo "FINDING: inline mark_double" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"links":1' "$RAW/$f.metadata.json" || { echo "FINDING: inline links" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"images":1' "$RAW/$f.metadata.json" || { echo "FINDING: inline images" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"includes":1' "$RAW/$f.metadata.json" || { echo "FINDING: inline includes" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"xrefs":1' "$RAW/$f.metadata.json" || { echo "FINDING: inline xrefs" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"urls":1' "$RAW/$f.metadata.json" || { echo "FINDING: inline urls" >&2; surface_fail=$((surface_fail + 1)); } ;;
            admonitions.adoc)
                grep -q '"admonitions":3' "$RAW/$f.metadata.json" || { echo "FINDING: admonitions count" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"block_admonitions":1' "$RAW/$f.metadata.json" || { echo "FINDING: admonitions block" >&2; surface_fail=$((surface_fail + 1)); } ;;
        esac

        # H3: an out-of-range native heading declines typed (rc 6).
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --adoc-heading 99 --kind metadata \
            > /dev/null 2>&1
        decline_rc=$?
        set -e
        [ "$decline_rc" -eq 6 ] && decline_ok=$((decline_ok + 1)) || { echo "FINDING: $f out-of-range heading rc=$decline_rc (want 6)" >&2; surface_fail=$((surface_fail + 1)); }

        # H3: an unsupported common pair declines typed (rc 6).
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --table 0 --kind text \
            > /dev/null 2>&1
        control_rc=$?
        set -e
        [ "$control_rc" -eq 6 ] || { echo "FINDING: $f common table rc=$control_rc (want 6)" >&2; surface_fail=$((surface_fail + 1)); }

    elif is_in "$f" "$CONTROL"; then
        control_n=$((control_n + 1))
        case "$f" in
            markdown.md)
                # A valid Markdown document must not be stolen by AsciiDoc.
                [ "$fmt" = "markdown" ] || { echo "FINDING: $f detected as $fmt (want markdown)" >&2; surface_fail=$((surface_fail + 1)); }
                boundary_ok=$((boundary_ok + 1)) ;;
            rest.rst)
                # A valid reStructuredText document must not be stolen by AsciiDoc.
                [ "$fmt" = "rst" ] || { echo "FINDING: $f detected as $fmt (want rst)" >&2; surface_fail=$((surface_fail + 1)); }
                boundary_ok=$((boundary_ok + 1)) ;;
            attributes_spaced.adoc)
                # The spaced `:name: value` form is a reST field list; reST is tried
                # first, so the source is admitted as Rst and never stolen by AsciiDoc.
                [ "$fmt" = "rst" ] || { echo "FINDING: $f detected as $fmt (want rst)" >&2; surface_fail=$((surface_fail + 1)); }
                boundary_ok=$((boundary_ok + 1)) ;;
            prose.txt)
                [ "$fmt" = "opaque" ] || { echo "FINDING: $f detected as $fmt (want opaque)" >&2; surface_fail=$((surface_fail + 1)); }
                set +e
                "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
                    > "$RAW/$f.metadata.json" 2>&1
                control_rc=$?
                set -e
                [ "$control_rc" -eq 6 ] && opaque_ok=$((opaque_ok + 1)) || { echo "FINDING: opaque $f metadata rc=$control_rc (want 6)" >&2; }
                boundary_ok=$((boundary_ok + 1)) ;;
        esac
    fi

    # A malformed `--adoc-heading` argument is a usage error (rc 2), once.
    if [ "$f" = "basic.adoc" ]; then
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --adoc-heading abc --kind metadata \
            > /dev/null 2>&1
        u_rc=$?
        set -e
        [ "$u_rc" -eq 2 ] && usage_ok=$((usage_ok + 1)) || echo "FINDING: malformed --adoc-heading rc=$u_rc (want 2)" >&2
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

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$f" "$fmt" "$src_len" "$src_sha" "$out_len" "$out_sha" "$cmp_ok" "$exact" \
        "$decline_rc" "$control_rc" >> "$TSV"
done

echo "=== exactness after source + descriptor deletion ==="
cat "$TSV"

fixtures_n="$(printf '%s\n' $ALL | wc -w | tr -d ' ')"

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 21.26.2 AsciiDoc court — matrix"
    echo
    echo "| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |"
    echo "| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |"
    # shellcheck disable=SC2162
    while IFS='	' read -r a b c d e g h i j k; do
        [ "$a" = "fixture" ] && continue
        [ -z "$a" ] && continue
        if [ "$d" = "$g" ]; then sh=true; else sh=false; fi
        echo "| \`$a\` | $b | $c | $e | $sh | $h | $i | $j | $k |"
    done < "$TSV"
} > "$CAMPAIGN/MATRIX.md"

# --- counts.txt -------------------------------------------------------------
{
    echo "fixtures $fixtures_n"
    echo "asciidoc_fixtures $normal_n"
    echo "control_fixtures $control_n"
    echo "exact_ok $exact_ok"
    echo "exact_fail $exact_fail"
    echo "surface_fail $surface_fail"
    echo "typed_declines_ok $decline_ok"
    echo "boundary_ok $boundary_ok"
    echo "opaque_controls_ok $opaque_ok"
    echo "usage_ok $usage_ok"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.26.2 — AsciiDoc (Asciidoctor) prose surface + exact closure + model",
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
    || [ "$decline_ok" -ne "$normal_n" ] \
    || [ "$boundary_ok" -ne 4 ] || [ "$opaque_ok" -ne 1 ] || [ "$usage_ok" -ne 1 ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.26.2 — AsciiDoc (Asciidoctor) prose surface + exact closure + model",
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
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion, for every fixture including the Opaque, Markdown, reStructuredText, and spaced-attribute controls",
  "asciidoc": "a bounded, line-based AsciiDoc-subset parser preserving exact block/inline source spans: a level-0 document title and ==+ sections with their exact = marker and recorded level, paragraphs, document attributes (:name: value / :name!:) with attribute references ({name}) surfaced literally (never expanded), block attribute lines attached to the following block, delimited blocks (listing/literal/example/sidebar/quote/open/passthrough) with their exact delimiter and verbatim content, unordered/ordered/description lists with nesting, tables (|=== with | cell markers), admonitions (NOTE: and [NOTE]), and inline markup (strong/emphasis/mono/passthrough/superscript/subscript/mark) plus the link:, image:, include::, xref: and bare-URL macros preserved verbatim; never re-flowed or normalized",
  "observations": ["metadata", "text", "heading", "block", "search-match", "native adoc-heading", "native adoc-block", "native adoc-attribute", "native adoc-inline", "native adoc-find"],
  "declines": "an out-of-range native heading declines typed (rc 6); an unsupported common pair (table) declines typed (rc 6); a malformed --adoc-heading argument is a usage error (rc 2); plain prose stays opaque (rc 6), never a panic",
  "detection_boundary": "AsciiDoc detection requires an AsciiDoc-SPECIFIC structural signal: a level-0 document title (= ) on the first non-blank line followed by at least one further block, a ==+ section, a |=== table, a complete delimited block, or a block attribute line [...] followed by a block. Markdown and reStructuredText are tried first, so a Markdown document stays Markdown and a reST document stays Rst; plain prose stays Opaque.",
  "cannot_distinguish": "an example (====), passthrough (++++), or literal (....) block whose delimiter character is '=', '+', or '.' and whose body is a SINGLE non-blank line is indistinguishable from a reST overline section title (====\\nTitle\\n====); reST is tried first, so such a source is admitted as Rst (or stays Opaque) and is never reclassified as AsciiDoc. A document attribute written in the canonical spaced form (:name: value) or the unset form (:name!:) is a reStructuredText field list, so reST (tried before AsciiDoc) claims it; only the no-space :name:value spelling stays AsciiDoc (the attributes_spaced.adoc control demonstrates this). A source that is ONLY :name: value-shaped lines is also claimed earlier by YAML/config. The AsciiDoc adapter still parses both attribute spellings inside a document that it is handed directly.",
  "precedence": "AsciiDoc detection runs immediately after reStructuredText and before fixed-width; the dispatcher already tries PDF/ZIP/JSON/YAML/config/CSV/Markdown ahead of it",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 41 invalid-asciidoc-structure",
  "adr_0060": "the AsciidocModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase21-26-2-asciidoc-court.sh
# inside the court:
#   cargo build --locked --all-features
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--heading N|--block N|--text PAT|--adoc-heading N|--adoc-block N|--adoc-attribute N|--adoc-inline N|--adoc-find PAT) --kind metadata|text|exact
#   rm source F.voldoc
#   target/debug/vole-document materialize --store STORE --field HEX --exact --output OUT ; cmp OUT source
# Regression controls (AsciiDoc must not break Markdown/reST):
#   docker compose run --rm --no-TTY dev sh tools/phase21-8-1-markdown-court.sh
#   docker compose run --rm --no-TTY dev sh tools/phase21-26-1-rst-court.sh
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
    echo "# Phase 21.26.2 — AsciiDoc (Asciidoctor) court"
    echo
    echo "**Question.** Does the third Wave-2 prose format (the Asciidoctor input"
    echo "language) close exactly and expose a representation-preserving AsciiDoc model"
    echo "(exact block and inline spans, a level-0 document title and ==+ sections with"
    echo "their exact marker and recorded level, document attributes with literal"
    echo "attribute references, block attribute lines attached to the following block,"
    echo "every delimited block kind with its exact delimiter and verbatim content, list"
    echo "nesting, tables, admonitions, and inline markup plus the link:/image:/include::/"
    echo "xref:/bare-URL macros) on top of the whole-source exact leaf, while keeping an"
    echo "AsciiDoc-**specific** detection boundary (plain prose stays Opaque; a Markdown"
    echo "document stays Markdown; a reStructuredText document stays Rst)?"
    echo
    echo "**Method.** Each self-authored fixture is generated in-court with GNU"
    echo "\`/usr/bin/printf\`, ingested via \`field-build\`, observed, then — after the"
    echo "**source file and the standalone descriptor are deleted** — rematerialized"
    echo "exactly in a fresh process and compared with \`length + SHA-256 + cmp\`. An"
    echo "out-of-range native heading and an unsupported common pair are required to"
    echo "decline typed; a malformed native argument is a usage error; the prose,"
    echo "Markdown, and reST controls pin the detection boundaries. The court runs in the"
    echo "pinned \`dev\` service using only POSIX \`sh\`, coreutils, git, and the shipped"
    echo "binary (no python3, no jq)."
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
    echo "- **Shipped here:** byte-based, conservative, AsciiDoc-specific detection; the"
    echo "  bounded, line-based AsciiDoc-subset parser (exact block and inline spans and"
    echo "  bytes, document title/sections with their marker and level, document"
    echo "  attributes with literal attribute references, block attribute lines attached"
    echo "  to the following block, every delimited block kind with its delimiter and"
    echo "  verbatim content, unordered/ordered/description lists with nesting, tables,"
    echo "  admonitions, inline markup and macros); the canonical derived model; native"
    echo "  \`adoc-heading\`/\`adoc-block\`/\`adoc-attribute\`/\`adoc-inline\`/\`adoc-find\`;"
    echo "  common \`metadata\`/\`text\`/\`heading\`/\`block\`/\`search-match\`."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the AsciiDoc"
    echo "  model is derived (\`Q_gen\`) and never on the exactness path (ADR-0060: the"
    echo "  model node depends on the \`sha256(source)\` root, so no source-reading node"
    echo "  aliases another field's source)."
    echo "- **The source is never re-flowed or rendered.** A block's exact bytes are"
    echo "  literally \`source[span]\`; the canonical text projection is the source itself."
    echo "- **The supported subset is bounded.** AsciiDoc has no magic bytes, so detection"
    echo "  requires an AsciiDoc-specific signal. Attribute expansion, include/link"
    echo "  resolution, the full cell/row-spanning table grammar, and structure nested"
    echo "  inside delimited blocks are left as literal text rather than guessed."
    echo "- **The reST boundary is honest:** an \`====\`/\`++++\`/\`....\` block whose body is a"
    echo "  single non-blank line is a reST overline title, so reST (tried first) admits it"
    echo "  and it is never reclassified as AsciiDoc; the canonical spaced attribute form"
    echo "  \`:name: value\` and the unset form \`:name!:\` are reST field lists, so reST"
    echo "  claims them (the \`attributes_spaced.adoc\` control demonstrates this) and only"
    echo "  the no-space \`:name:value\` spelling stays AsciiDoc; a source that is only"
    echo "  \`:name: value\` lines is claimed by YAML/config first."
    echo "- **Not claimed here:** a full Asciidoctor conformance oracle."
    echo "- **Never run on the host:** every command above ran in the pinned \`dev\`"
    echo "  container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" != "PASS" ]; then
    echo "PHASE 21.26.2 ASCIIDOC COURT: FAIL" >&2
    exit 1
fi
echo "PHASE 21.26.2 ASCIIDOC COURT: PASS — campaign $CAMPAIGN"
