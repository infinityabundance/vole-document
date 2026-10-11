#!/bin/sh
# Phase 21.26.1 — the reStructuredText (reST, Docutils) prose adapter court.
#
# Pre-registered (Phase-21 subphase 21.26.1). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture (including the Opaque and Markdown
#       controls), `materialize(field) == source` (length + SHA-256 + cmp) after the
#       source file AND the standalone descriptor are deleted, in a fresh process.
#       The court FAILS unless this is 100% on every fixture.
#   H2 (surface + preserved representation) — the reST-specific model is exposed:
#       exact block/inline source spans, section titles with their exact
#       underline/overline adornment (char and length) and a recorded hierarchy,
#       paragraphs, explicit markup (`.. ` comments, `.. directive::` directives
#       preserved verbatim, substitution definitions, footnotes/citations, hyperlink
#       targets), field/option/definition lists, literal (`::`) and doctest (`>>> `)
#       blocks, bullet/enumerated lists with nesting, inline markup, and grid/simple
#       tables. Common metadata/text/heading/block/search-match and native
#       rst-heading/rst-block/rst-directive/rst-inline/rst-find answer.
#   H3 (declines + boundaries) — an unsupported common pair (the common `table`,
#       which is not in reST's capability set) and an out-of-range native title
#       decline typed (rc 6), never a silent empty answer; a malformed `--rst-heading`
#       argument is a usage error (rc 2); plain prose stays `opaque` (rc 6, never a
#       panic); a **Markdown** document (a GFM table + ATX heading) stays `markdown`
#       and is never stolen by reST.
#
# Runs in the pinned `dev` service using only POSIX `sh`, coreutils, git, and the
# shipped binary (no python3, no jq):
#   docker compose run --rm --no-TTY dev sh tools/phase21-26-1-rst-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="2026-10-10"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-26-1-rst-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-26
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
if ! cargo build --locked --all-features > "$RAW/build.log" 2>&1; then
    tail -40 "$RAW/build.log" >&2
    echo "PHASE 21.26.1 RST COURT: FAIL — all-features build failed" >&2
    exit 1
fi
tail -2 "$RAW/build.log"

echo "=== generate fixtures (POSIX printf only) ==="
mk() { /usr/bin/printf '%b' "$2" > "$1"; }

# --- NORMAL: reStructuredText documents (each carries a reST-specific signal) --
# basic.rst: a `=` title, an inline-rich paragraph, a `^` subtitle, a body.
mk "$WORK/corpus/basic.rst" 'Title\n=====\n\nA paragraph with **strong**, *emphasis*, ``literal``, and `role`:code: text.\n\nSection Two\n^^^^^^^^^^^\n\nA body paragraph.\n'
# explicit.rst: explicit markup — comment, hyperlink target, directive, footnote,
# and a substitution definition (all preserved verbatim, never executed).
mk "$WORK/corpus/explicit.rst" 'Explicit\n========\n\n.. a comment line\n\n.. _tgt: https://example.org\n\n.. note:: directive body\n\n.. [1] footnote body\n\n.. |sub| replace:: replacement\n\nbody text.\n'
# lists.rst: bullet list, ordered list, a nested list, a definition list, a field
# list, and an option list.
mk "$WORK/corpus/lists.rst" 'Lists\n=====\n\n- alpha\n- beta\n\n1. one\n2. two\n\n- outer\n  - inner\n\nTerm\n  definition body\n\n:author: me\n\n-a, --all  do all\n'
# tables.rst: a grid table and a simple table.
mk "$WORK/corpus/tables.rst" 'Tables\n======\n\n+---+---+\n| a | b |\n+---+---+\n| 1 | 2 |\n+---+---+\n\n=====  =====\ncolA   colB\n=====  =====\n1      2\n=====  =====\n'
# inline.rst: every inline kind.
mk "$WORK/corpus/inline.rst" 'Inline\n======\n\n**strong** *emphasis* ``literal`` `role`:code: |sub| [1]_ `lbl`_ and name_\n'
# literal.rst: a literal block (`::`) and a doctest block.
mk "$WORK/corpus/literal.rst" 'Literal\n=======\n\nA paragraph ending here::\n\n    indented literal body\n\n>>> print(1)\n1\n'

# --- CONTROLS ---------------------------------------------------------------
# Plain prose: no reST structural mark -> stays Opaque.
mk "$WORK/corpus/prose.txt" 'This is just some prose text.\nIt has several lines, with punctuation,\nbut no reST directive, adornment, table, or field at all.\n'
# A Markdown document (ATX heading + GFM table) must stay Markdown.
mk "$WORK/corpus/markdown.md" '# Heading\n\nA paragraph with [link](http://x).\n\n| a | b |\n| - | - |\n| 1 | 2 |\n'

NORMAL="basic.rst explicit.rst lists.rst tables.rst inline.rst literal.rst"
CONTROL="prose.txt markdown.md"
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
        [ "$fmt" = "rst" ] || { echo "FINDING: $f detected as $fmt (want rst)" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"format":"rst"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata missing format" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --rst-heading 0 --kind metadata \
            > "$RAW/$f.heading.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --rst-block 0 --kind exact \
            > "$RAW/$f.block.exact.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --rst-block 0 --kind metadata \
            > "$RAW/$f.block.meta.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --rst-find a --kind text \
            > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))

        # Common selectors: heading/block/search-match resolve for a reST document.
        "$BIN" observe --store "$WORK/store" --field "$field" --heading 0 --kind text \
            > "$RAW/$f.common.heading.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --block 0 --kind text \
            > "$RAW/$f.common.block.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --text a --kind text \
            > "$RAW/$f.common.search.json" 2>&1 || surface_fail=$((surface_fail + 1))

        # The whole-document text is the source itself (never re-flowed).
        grep -q '"text"' "$RAW/$f.text.json" || { echo "FINDING: $f doc-text" >&2; surface_fail=$((surface_fail + 1)); }

        case "$f" in
            basic.rst)
                grep -q '"titles":2' "$RAW/$f.metadata.json" || { echo "FINDING: basic titles" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"max_title_level":2' "$RAW/$f.metadata.json" || { echo "FINDING: basic hierarchy" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"adornment":"====="' "$RAW/$f.heading.json" || { echo "FINDING: basic adornment" >&2; surface_fail=$((surface_fail + 1)); }
                [ "$(json_str "$(cat "$RAW/$f.block.exact.json")" value_hex)" = "5469746c650a3d3d3d3d3d" ] || { echo "FINDING: basic block0 exact" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"strong":1' "$RAW/$f.metadata.json" || { echo "FINDING: basic strong" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"interpreted":1' "$RAW/$f.metadata.json" || { echo "FINDING: basic interpreted" >&2; surface_fail=$((surface_fail + 1)); } ;;
            explicit.rst)
                grep -q '"directives":1' "$RAW/$f.metadata.json" || { echo "FINDING: explicit directives" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"footnotes":1' "$RAW/$f.metadata.json" || { echo "FINDING: explicit footnotes" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"targets":1' "$RAW/$f.metadata.json" || { echo "FINDING: explicit targets" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --rst-directive 0 --kind text \
                    > "$RAW/$f.directive.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q 'note:: directive body' "$RAW/$f.directive.text.json" || { echo "FINDING: explicit directive verbatim" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --rst-directive 0 --kind metadata \
                    > "$RAW/$f.directive.meta.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"name":"note"' "$RAW/$f.directive.meta.json" || { echo "FINDING: explicit directive name" >&2; surface_fail=$((surface_fail + 1)); } ;;
            lists.rst)
                grep -q '"list_items":5' "$RAW/$f.metadata.json" || { echo "FINDING: lists items" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"fields":1' "$RAW/$f.metadata.json" || { echo "FINDING: lists fields" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"options":1' "$RAW/$f.metadata.json" || { echo "FINDING: lists options" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"definitions":1' "$RAW/$f.metadata.json" || { echo "FINDING: lists definitions" >&2; surface_fail=$((surface_fail + 1)); } ;;
            tables.rst)
                grep -q '"grid_tables":1' "$RAW/$f.metadata.json" || { echo "FINDING: tables grid" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"simple_tables":1' "$RAW/$f.metadata.json" || { echo "FINDING: tables simple" >&2; surface_fail=$((surface_fail + 1)); } ;;
            inline.rst)
                grep -q '"strong":1' "$RAW/$f.metadata.json" || { echo "FINDING: inline strong" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"literals":1' "$RAW/$f.metadata.json" || { echo "FINDING: inline literals" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"substitutions":1' "$RAW/$f.metadata.json" || { echo "FINDING: inline substitutions" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"footnote_refs":1' "$RAW/$f.metadata.json" || { echo "FINDING: inline footnote refs" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"hyperlink_refs":2' "$RAW/$f.metadata.json" || { echo "FINDING: inline hyperlink refs" >&2; surface_fail=$((surface_fail + 1)); } ;;
            literal.rst)
                grep -q '"literal_blocks":1' "$RAW/$f.metadata.json" || { echo "FINDING: literal blocks" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"doctests":1' "$RAW/$f.metadata.json" || { echo "FINDING: literal doctests" >&2; surface_fail=$((surface_fail + 1)); } ;;
        esac

        # A native inline span resolves when the document carries inline markup.
        if is_in "$f" "basic.rst inline.rst tables.rst"; then
            "$BIN" observe --store "$WORK/store" --field "$field" --rst-inline 0 --kind metadata \
                > "$RAW/$f.inline.json" 2>&1 || { echo "FINDING: $f rst-inline" >&2; surface_fail=$((surface_fail + 1)); }
            grep -q '"inline":0' "$RAW/$f.inline.json" || { echo "FINDING: $f rst-inline ordinal" >&2; surface_fail=$((surface_fail + 1)); }
        fi

        # H3: an out-of-range native title declines typed (rc 6).
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --rst-heading 99 --kind metadata \
            > /dev/null 2>&1
        decline_rc=$?
        set -e
        [ "$decline_rc" -eq 6 ] && decline_ok=$((decline_ok + 1)) || { echo "FINDING: $f out-of-range title rc=$decline_rc (want 6)" >&2; surface_fail=$((surface_fail + 1)); }

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
                # A valid Markdown document must not be stolen by reST.
                [ "$fmt" = "markdown" ] || { echo "FINDING: $f detected as $fmt (want markdown)" >&2; surface_fail=$((surface_fail + 1)); }
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

    # A malformed `--rst-heading` argument is a usage error (rc 2), once.
    if [ "$f" = "basic.rst" ]; then
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --rst-heading abc --kind metadata \
            > /dev/null 2>&1
        u_rc=$?
        set -e
        [ "$u_rc" -eq 2 ] && usage_ok=$((usage_ok + 1)) || echo "FINDING: malformed --rst-heading rc=$u_rc (want 2)" >&2
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
    echo "# Phase 21.26.1 reStructuredText court — matrix"
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
    echo "rst_fixtures $normal_n"
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
  "phase": "21.26.1 — reStructuredText (Docutils) prose surface + exact closure + model",
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
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
if [ "$exact_fail" -ne 0 ] || [ "$surface_fail" -ne 0 ] \
    || [ "$decline_ok" -ne "$normal_n" ] \
    || [ "$boundary_ok" -ne 2 ] || [ "$opaque_ok" -ne 1 ] || [ "$usage_ok" -ne 1 ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.26.1 — reStructuredText (Docutils) prose surface + exact closure + model",
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
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion, for every fixture including the Opaque and Markdown controls",
  "rst": "a bounded, line-based Docutils-subset parser preserving exact block/inline source spans: section titles with their exact underline/overline adornment (char and length) and a recorded hierarchy, paragraphs, explicit markup (comments, directives preserved verbatim, substitution definitions, footnotes/citations, hyperlink targets), field/option/definition lists, literal (::) and doctest (>>>) blocks, bullet/enumerated lists with nesting, inline markup (strong/emphasis/literal/interpreted/substitution/footnote/hyperlink/anonymous references), and grid/simple tables; never re-flowed or normalized",
  "observations": ["metadata", "text", "heading", "block", "search-match", "native rst-heading", "native rst-block", "native rst-directive", "native rst-inline", "native rst-find"],
  "declines": "an out-of-range native title declines typed (rc 6); an unsupported common pair (table) declines typed (rc 6); a malformed --rst-heading argument is a usage error (rc 2); plain prose stays opaque (rc 6), never a panic",
  "detection_boundary": "reST detection requires a reST-SPECIFIC structural signal: an explicit markup start (.. comment/directive/target/footnote/substitution), a grid or simple table, a field list, or a section adornment whose char is not a Markdown construct. Markdown is tried first, so a Markdown document is never stolen; plain prose stays Opaque.",
  "cannot_distinguish": "an adornment of '-', '*', or '_' (a run of length >= 3) is a Markdown thematic break, an adornment of '~' (>= 3) is a Markdown fence opener, and an adornment of '#' is a Markdown ATX heading; those documents are admitted as Markdown (or stay Opaque) and are never reclassified as reST. A tiny standalone field list (:name: value) or directive (.. name:: body) is claimed by YAML first (YAML precedes reST in the dispatcher order), so it is not detected as reST (the reST adapter's own structural-signal predicate still recognizes both).",
  "precedence": "reStructuredText detection runs immediately after Markdown and before fixed-width; the dispatcher already tries PDF/ZIP/JSON/YAML/CSV/TOML/config ahead of it",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 40 invalid-rst-structure",
  "adr_0060": "the RstModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase21-26-1-rst-court.sh
# inside the court:
#   cargo build --locked --all-features
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--heading N|--block N|--text PAT|--rst-heading N|--rst-block N|--rst-directive N|--rst-inline N|--rst-find PAT) --kind metadata|text|exact
#   rm source F.voldoc
#   target/debug/vole-document materialize --store STORE --field HEX --exact --output OUT ; cmp OUT source
# Regression control (reST must not break Markdown):
#   docker compose run --rm --no-TTY dev sh tools/phase21-8-1-markdown-court.sh
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
    echo "# Phase 21.26.1 — reStructuredText (Docutils) court"
    echo
    echo "**Question.** Does the next Wave-2 prose format (the Docutils input language)"
    echo "close exactly and expose a representation-preserving reST model (exact block"
    echo "and inline spans, section titles with their exact adornment and recorded"
    echo "hierarchy, directives preserved verbatim, targets/footnotes/substitutions,"
    echo "field/option/definition lists, literal and doctest blocks, list nesting, and"
    echo "grid/simple tables) on top of the whole-source exact leaf, while keeping a"
    echo "reST-**specific** detection boundary (plain prose and a Markdown document are"
    echo "never stolen)?"
    echo
    echo "**Method.** Each self-authored fixture is generated in-court with GNU"
    echo "\`/usr/bin/printf\`, ingested via \`field-build\`, observed, then — after the"
    echo "**source file and the standalone descriptor are deleted** — rematerialized"
    echo "exactly in a fresh process and compared with \`length + SHA-256 + cmp\`. An"
    echo "out-of-range native title and an unsupported common pair are required to decline"
    echo "typed; a malformed native argument is a usage error; the prose and Markdown"
    echo "controls pin the detection boundaries. The court runs in the pinned \`dev\`"
    echo "service using only POSIX \`sh\`, coreutils, git, and the shipped binary (no"
    echo "python3, no jq)."
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
    echo "- **Shipped here:** byte-based, conservative, reST-specific detection; the"
    echo "  bounded, line-based Docutils-subset parser (exact block and inline spans and"
    echo "  bytes, section titles and their adornment/hierarchy, explicit markup,"
    echo "  directives preserved verbatim, targets/footnotes/substitutions,"
    echo "  field/option/definition lists, literal and doctest blocks, bullet/enumerated"
    echo "  lists with nesting, inline markup, and grid/simple tables); the canonical"
    echo "  derived model; native \`rst-heading\`/\`rst-block\`/\`rst-directive\`/\`rst-inline\`/"
    echo "  \`rst-find\`; common \`metadata\`/\`text\`/\`heading\`/\`block\`/\`search-match\`."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the reST"
    echo "  model is derived (\`Q_gen\`) and never on the exactness path (ADR-0060: the"
    echo "  model node depends on the \`sha256(source)\` root, so no source-reading node"
    echo "  aliases another field's source)."
    echo "- **The source is never re-flowed or rendered.** A block's exact bytes are"
    echo "  literally \`source[span]\`; the canonical text projection is the source itself."
    echo "- **The supported subset is bounded.** reST has no magic bytes, so detection"
    echo "  requires a reST-specific signal. The full directive option/argument grammar,"
    echo "  multi-line section titles, and nested structure inside directives/lists are"
    echo "  left as literal text rather than guessed."
    echo "- **The Markdown boundary is honest:** a \`- \\* _\` adornment (>= 3) is a"
    echo "  Markdown thematic break, \`~\` (>= 3) is a Markdown fence, and \`#\` is a"
    echo "  Markdown ATX heading, so those documents are never reclassified as reST; and a"
    echo "  tiny \`:name: value\` / \`.. name:: body\` source is claimed by YAML first."
    echo "- **Not claimed here:** a full Docutils conformance oracle."
    echo "- **Never run on the host:** every command above ran in the pinned \`dev\`"
    echo "  container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" != "PASS" ]; then
    echo "PHASE 21.26.1 RST COURT: FAIL" >&2
    exit 1
fi
echo "PHASE 21.26.1 RST COURT: PASS — campaign $CAMPAIGN"
