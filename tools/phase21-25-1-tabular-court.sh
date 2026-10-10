#!/bin/sh
# Phase 21.25.1 — the tabular-extra adapter court: PSV (pipe-separated values) as
# the CSV adapter's third recorded dialect, and the **new** fixed-width
# (column-position) adapter.
#
# Pre-registered (Phase-21 subphase 21.25.1). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture (including the delimited/Markdown/Opaque
#       controls), `materialize(field) == source` (length + SHA-256 + cmp) after the
#       source file AND the standalone descriptor are deleted, in a fresh process.
#       The court FAILS unless this is 100% on every fixture.
#   H2 (surface + preserved representation) — PSV is the CSV adapter's **third
#       recorded delimiter** (`pipe`), preserving exact record/field spans, original
#       quoting/`""`/embedded delimiters, ragged rows, BOM, and the header exactly as
#       comma/tab do; the fixed-width adapter exposes the inferred per-column
#       start/end positions and widths, the uniform record width, every record's exact
#       content span and every field's exact padded bytes, the terminator, a BOM, and
#       the header row. Common metadata/text/table/cell/search-match and native
#       csv-* and fixedwidth-* selectors answer.
#   H3 (declines + boundaries) — an unsupported common pair (`heading`) declines typed
#       (rc 6); an out-of-range fixed-width record declines typed (rc 6); a delimited
#       table stays `csv` with the right dialect; a GFM pipe table stays `markdown`
#       (never stolen by the PSV dialect); variable-length prose and a single-space
#       aligned blob stay `opaque` (a typed decline, never a panic).
#
# Runs in the pinned `dev` service using only POSIX `sh`, coreutils, git, and the
# shipped binary (no python3, no jq):
#   docker compose run --rm --no-TTY dev sh tools/phase21-25-1-tabular-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="2026-10-10"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-25-1-tabular-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-25
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
if ! cargo build --locked --all-features > "$RAW/build.log" 2>&1; then
    tail -40 "$RAW/build.log" >&2
    echo "PHASE 21.25.1 TABULAR COURT: FAIL — all-features build failed" >&2
    exit 1
fi
tail -2 "$RAW/build.log"

echo "=== generate fixtures (POSIX printf only) ==="
mk() { /usr/bin/printf '%b' "$2" > "$1"; }

# --- NORMAL: fixed-width (column-position) tables ---------------------------
# basic.fw: three records of identical width 10; columns [0,5) and [7,10),
# separated by a two-wide all-space gap at positions 5 and 6.
mk "$WORK/corpus/basic.fw" 'Name   Age\nAlice   30\nBob     25\n'
# crlf.fw: the same table with CRLF terminators.
mk "$WORK/corpus/crlf.fw" 'Name   Age\r\nAlice   30\r\nBob     25\r\n'
# three.fw: three records of width 13; columns [0,3), [5,8), [10,13).
mk "$WORK/corpus/three.fw" 'ID   Nm   Age\n001  Ali  030\n002  Bob  025\n'
# bom.fw: a leading UTF-8 BOM prefix.
mk "$WORK/corpus/bom.fw" '\357\273\277Name   Age\nAlice   30\nBob     25\n'

# --- NORMAL: delimited tables (the CSV adapter's comma/tab/pipe dialects) ----
mk "$WORK/corpus/comma.csv" 'name,age\nalice,30\nbob,25\n'
mk "$WORK/corpus/tab.tsv" 'name\tage\nalice\t30\nbob\t25\n'
mk "$WORK/corpus/pipe.psv" 'name|note|age\nalice|blue|30\nbob|red|25\n'

# --- CONTROLS ---------------------------------------------------------------
# A GFM/Markdown pipe table carries a delimiter row: it must stay Markdown.
mk "$WORK/corpus/markdown.md" '| a | b |\n| --- | --- |\n| c | d |\n'
# Variable-length prose: not uniform-width, stays Opaque.
mk "$WORK/corpus/prose.txt" 'The quick brown fox\njumps over the lazy\ndog near the river\n'
# A single-space-separated aligned blob: too ambiguous, stays Opaque.
mk "$WORK/corpus/spaced.txt" 'abcd efgh\nijkl mnop\nqrst uvwx\n'

FIXED="basic.fw crlf.fw three.fw bom.fw"
DELIM="comma.csv tab.tsv pipe.psv"
CONTROL="markdown.md prose.txt spaced.txt"
ALL="$FIXED $DELIM $CONTROL"

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
fixed_n=0
delim_n=0
control_n=0
decline_ok=0
usage_ok=0
boundary_ok=0
opaque_ok=0

for f in $ALL; do
    # The reference copy is never deleted; the corpus copy is ingested and then
    # removed (so the identity comparison is against retained bytes).
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

    if is_in "$f" "$FIXED"; then
        fixed_n=$((fixed_n + 1))
        [ "$fmt" = "fixedwidth" ] || { echo "FINDING: $f detected as $fmt (want fixedwidth)" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"format":"fixedwidth"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata missing format" >&2; surface_fail=$((surface_fail + 1)); }
        grep -q '"ragged_records":0' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata ragged" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --fixedwidth-columns --kind text \
            > "$RAW/$f.columns.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --fixedwidth-columns --kind metadata \
            > "$RAW/$f.columns.meta.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --fixedwidth-header --kind metadata \
            > "$RAW/$f.header.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --fixedwidth-row 1 --kind exact \
            > "$RAW/$f.row1.exact.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --fixedwidth-row 1 --kind metadata \
            > "$RAW/$f.row1.meta.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --fixedwidth-cell 1:1 --kind exact \
            > "$RAW/$f.cell11.exact.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --fixedwidth-cell 1:1 --kind text \
            > "$RAW/$f.cell11.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --fixedwidth-range 0:0:1:1 --kind metadata \
            > "$RAW/$f.range.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --fixedwidth-find Al --kind text \
            > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))

        case "$f" in
            basic.fw)
                grep -q '"columns":2' "$RAW/$f.metadata.json" || { echo "FINDING: basic columns" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"width":10' "$RAW/$f.metadata.json" || { echo "FINDING: basic width" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"terminator":"lf"' "$RAW/$f.metadata.json" || { echo "FINDING: basic terminator" >&2; surface_fail=$((surface_fail + 1)); }
                [ "$(json_str "$(cat "$RAW/$f.columns.json")" text)" = "0:5,7:10" ] || { echo "FINDING: basic columns text" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"start":0' "$RAW/$f.columns.meta.json" || { echo "FINDING: basic col start" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"width":5' "$RAW/$f.columns.meta.json" || { echo "FINDING: basic col width" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"Name"' "$RAW/$f.header.json" || { echo "FINDING: basic header name" >&2; surface_fail=$((surface_fail + 1)); }
                [ "$(json_str "$(cat "$RAW/$f.row1.exact.json")" value_hex)" = "416c6963652020203330" ] || { echo "FINDING: basic row1 exact" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"text":"Alice"' "$RAW/$f.row1.meta.json" || { echo "FINDING: basic row1 meta" >&2; surface_fail=$((surface_fail + 1)); }
                [ "$(json_str "$(cat "$RAW/$f.cell11.exact.json")" value_hex)" = "203330" ] || { echo "FINDING: basic cell11 exact" >&2; surface_fail=$((surface_fail + 1)); }
                [ "$(json_str "$(cat "$RAW/$f.cell11.text.json")" text)" = "30" ] || { echo "FINDING: basic cell11 text" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"text":"Alice"' "$RAW/$f.range.json" || { echo "FINDING: basic range" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"record":1' "$RAW/$f.find.json" || { echo "FINDING: basic find" >&2; surface_fail=$((surface_fail + 1)); } ;;
            crlf.fw)
                grep -q '"terminator":"crlf"' "$RAW/$f.metadata.json" || { echo "FINDING: crlf terminator" >&2; surface_fail=$((surface_fail + 1)); }
                [ "$(json_str "$(cat "$RAW/$f.cell11.exact.json")" value_hex)" = "203330" ] || { echo "FINDING: crlf cell11 exact" >&2; surface_fail=$((surface_fail + 1)); } ;;
            three.fw)
                grep -q '"columns":3' "$RAW/$f.metadata.json" || { echo "FINDING: three columns" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"width":13' "$RAW/$f.metadata.json" || { echo "FINDING: three width" >&2; surface_fail=$((surface_fail + 1)); }
                [ "$(json_str "$(cat "$RAW/$f.columns.json")" text)" = "0:3,5:8,10:13" ] || { echo "FINDING: three columns text" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"Nm"' "$RAW/$f.header.json" || { echo "FINDING: three header" >&2; surface_fail=$((surface_fail + 1)); }
                [ "$(json_str "$(cat "$RAW/$f.cell11.text.json")" text)" = "Ali" ] || { echo "FINDING: three cell11 text" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --fixedwidth-cell 1:Nm --kind text \
                    > "$RAW/$f.cellnm.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
                [ "$(json_str "$(cat "$RAW/$f.cellnm.text.json")" text)" = "Ali" ] || { echo "FINDING: three cellnm" >&2; surface_fail=$((surface_fail + 1)); } ;;
            bom.fw)
                grep -q '"bom_bytes":3' "$RAW/$f.metadata.json" || { echo "FINDING: bom bytes" >&2; surface_fail=$((surface_fail + 1)); }
                [ "$(json_str "$(cat "$RAW/$f.row1.exact.json")" value_hex)" = "416c6963652020203330" ] || { echo "FINDING: bom row1 exact" >&2; surface_fail=$((surface_fail + 1)); } ;;
        esac

        # H3: an out-of-range record declines typed (rc 6).
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --fixedwidth-row 999 --kind metadata \
            > /dev/null 2>&1
        decline_rc=$?
        set -e
        [ "$decline_rc" -eq 6 ] && decline_ok=$((decline_ok + 1)) || { echo "FINDING: $f out-of-range row rc=$decline_rc (want 6)" >&2; surface_fail=$((surface_fail + 1)); }

        # H3: an unsupported common pair declines typed (rc 6).
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --heading 0 --kind text \
            > /dev/null 2>&1
        control_rc=$?
        set -e
        [ "$control_rc" -eq 6 ] || { echo "FINDING: $f heading rc=$control_rc (want 6)" >&2; surface_fail=$((surface_fail + 1)); }

    elif is_in "$f" "$DELIM"; then
        delim_n=$((delim_n + 1))
        # The delimited controls stay `csv`, with the recorded dialect.
        [ "$fmt" = "csv" ] || { echo "FINDING: $f detected as $fmt (want csv)" >&2; surface_fail=$((surface_fail + 1)); }
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
        case "$f" in
            comma.csv) grep -q '"delimiter":"comma"' "$RAW/$f.metadata.json" || { echo "FINDING: comma dialect" >&2; surface_fail=$((surface_fail + 1)); } ;;
            tab.tsv) grep -q '"delimiter":"tab"' "$RAW/$f.metadata.json" || { echo "FINDING: tab dialect" >&2; surface_fail=$((surface_fail + 1)); } ;;
            pipe.psv)
                grep -q '"delimiter":"pipe"' "$RAW/$f.metadata.json" || { echo "FINDING: pipe dialect" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"columns":3' "$RAW/$f.metadata.json" || { echo "FINDING: pipe columns" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --csv-cell 1:1 --kind exact \
                    > "$RAW/$f.cell11.exact.json" 2>&1 || surface_fail=$((surface_fail + 1))
                [ "$(json_str "$(cat "$RAW/$f.cell11.exact.json")" value_hex)" = "626c7565" ] || { echo "FINDING: pipe cell11" >&2; surface_fail=$((surface_fail + 1)); } ;;
        esac

    elif is_in "$f" "$CONTROL"; then
        control_n=$((control_n + 1))
        case "$f" in
            markdown.md)
                # The PSV dialect must not steal a GFM pipe table.
                [ "$fmt" = "markdown" ] || { echo "FINDING: $f detected as $fmt (want markdown)" >&2; surface_fail=$((surface_fail + 1)); }
                boundary_ok=$((boundary_ok + 1)) ;;
            prose.txt | spaced.txt)
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

    # A malformed `--fixedwidth-cell` argument is a usage error (rc 2), once.
    if [ "$f" = "basic.fw" ]; then
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --fixedwidth-cell abc --kind metadata \
            > /dev/null 2>&1
        u_rc=$?
        set -e
        [ "$u_rc" -eq 2 ] && usage_ok=$((usage_ok + 1)) || echo "FINDING: malformed --fixedwidth-cell rc=$u_rc (want 2)" >&2
    fi

    # H1: delete the source AND the standalone descriptor, then rematerialize
    # exactly in a fresh process and compare (length + SHA-256 + cmp).
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
    echo "# Phase 21.25.1 tabular-extra court — matrix"
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
    echo "fixedwidth_fixtures $fixed_n"
    echo "delimited_fixtures $delim_n"
    echo "control_fixtures $control_n"
    echo "exact_ok $exact_ok"
    echo "exact_fail $exact_fail"
    echo "surface_fail $surface_fail"
    echo "typed_decline_rows_ok $decline_ok"
    echo "boundary_ok $boundary_ok"
    echo "opaque_controls_ok $opaque_ok"
    echo "usage_ok $usage_ok"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.25.1 — PSV (pipe dialect) + fixed-width tabular-extra surface + exact closure",
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
    || [ "$decline_ok" -ne "$fixed_n" ] \
    || [ "$boundary_ok" -ne 3 ] || [ "$opaque_ok" -ne 2 ] || [ "$usage_ok" -ne 1 ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.25.1 — PSV (pipe dialect) + fixed-width tabular-extra surface + exact closure",
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
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion, for every fixture including the delimited/Markdown/Opaque controls",
  "psv": "the pipe delimiter is the CSV/TSV adapter's THIRD recorded dialect (one parser, three delimiters); a pipe-delimited table is claimed only with a consistent field count >= 2 across a sampled majority of >= 2 records, mirroring the comma/tab rule, and is declined when the source carries a GFM/Markdown delimiter row (so a Markdown table is never stolen)",
  "fixedwidth": "a distinct adapter (and format) for character-position columns: the inferred per-column start/end positions and widths, the uniform record width, every record's exact content span and every field's exact padded bytes, the terminator, a BOM, and the header row",
  "observations": ["metadata", "text", "table", "cell", "search-match", "native csv-row/csv-cell/csv-header/csv-range/csv-find", "native fixedwidth-row", "native fixedwidth-cell", "native fixedwidth-header", "native fixedwidth-columns", "native fixedwidth-range", "native fixedwidth-find"],
  "declines": "an out-of-range fixed-width record declines typed (rc 6); an unsupported common pair (heading) declines typed (rc 6); a malformed --fixedwidth-cell argument is a usage error (rc 2); a delimited table stays csv with the right dialect; a GFM pipe table stays markdown; variable-length prose and a single-space aligned blob stay opaque (rc 6), never a panic",
  "detection_boundary": "fixed-width is maximally conservative: >= 3 sampled records of IDENTICAL byte width, an inferred layout whose interior whitespace gaps are >= 2 columns wide, and >= 2 non-empty columns; anything that also parses as a delimited (CSV/TSV/PSV) table or as a Markdown table is declined. The pipe dialect is tried last (comma/tab first) and declined on a Markdown delimiter row.",
  "cannot_distinguish": "a fixed-width file whose trailing spaces are trimmed (variable width) is not claimed; a single-space-separated two-column layout is not claimed (the gap must be >= 2 wide); and a genuinely ambiguous aligned-text blob (equal-length lines, >= 2-wide gaps, >= 3 lines) is NOT distinguishable from a fixed-width table and IS claimed — a recorded negative. Detection samples only the first max_fixedwidth_sampled_lines_for_detection lines; parse re-validates every line and declines the whole document typed if any differs. Character positions are byte positions (multibyte UTF-8 shifts columns).",
  "precedence": "fixed-width detection runs after CSV/TSV/PSV and after Markdown (a delimited or Markdown table always keeps its format); the PSV pipe dialect is tried after comma and tab",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 39 invalid-fixedwidth-structure",
  "adr_0060": "the FixedWidthModel node (and the CsvModel node for PSV) depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase21-25-1-tabular-court.sh
# inside the court:
#   cargo build --locked --all-features
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--fixedwidth-row N|--fixedwidth-cell R:C|--fixedwidth-header|--fixedwidth-columns|--fixedwidth-range R1:C1:R2:C2|--fixedwidth-find PAT|--csv-cell R:C) --kind metadata|text|structure|exact
#   rm source F.voldoc
#   target/debug/vole-document materialize --store STORE --field HEX --exact --output OUT ; cmp OUT source
# Regression control:
#   docker compose run --rm --no-TTY dev sh tools/phase21-7-1-csv-court.sh
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
    echo "# Phase 21.25.1 — tabular-extra (PSV + fixed-width) court"
    echo
    echo "**Question.** Does the pipe delimiter close exactly as a third CSV dialect,"
    echo "and does the new fixed-width adapter close exactly while exposing a"
    echo "representation-preserving column-position table (per-column start/end"
    echo "positions and widths, the uniform record width, exact per-record and"
    echo "per-field padded spans, the terminator, a BOM, and the header row) — all on"
    echo "top of the whole-source exact leaf, while keeping a maximally conservative"
    echo "detection boundary (delimited and Markdown tables are never stolen; prose and"
    echo "ambiguous aligned text stay Opaque)?"
    echo
    echo "**Method.** Each self-authored fixture is generated in-court with GNU"
    echo "\`/usr/bin/printf\`, ingested via \`field-build\`, observed, then — after the"
    echo "**source file and the standalone descriptor are deleted** — rematerialized"
    echo "exactly in a fresh process and compared with \`length + SHA-256 + cmp\`. An"
    echo "out-of-range fixed-width record and an unsupported common pair are required to"
    echo "decline typed; the delimited/Markdown/prose controls pin the detection"
    echo "boundaries. The court runs in the pinned \`dev\` service using only POSIX"
    echo "\`sh\`, coreutils, git, and the shipped binary (no python3, no jq)."
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
    echo "- **Shipped here:** the pipe delimiter as the CSV/TSV adapter's third recorded"
    echo "  dialect (one parser, three delimiters — never a second parser); a new"
    echo "  fixed-width (column-position) adapter and format; native \`fixedwidth-row\`/"
    echo "  \`fixedwidth-cell\`/\`fixedwidth-header\`/\`fixedwidth-columns\`/"
    echo "  \`fixedwidth-range\`/\`fixedwidth-find\`; common \`metadata\`/\`text\`/\`table\`/"
    echo "  \`cell\`/\`search-match\`."
    echo "- **Detection boundary:** the pipe dialect is tried after comma and tab and is"
    echo "  declined on a GFM/Markdown delimiter row, so a Markdown pipe table is never"
    echo "  stolen. Fixed-width is claimed only with >= 3 sampled records of identical"
    echo "  byte width, >= 2 non-empty columns, and interior whitespace gaps >= 2 columns"
    echo "  wide — and only after declining any delimited or Markdown table."
    echo "- **Recorded negatives:** a trailing-space-trimmed (variable-width) fixed-width"
    echo "  file, a single-space-separated two-column layout, and any file that also"
    echo "  parses as a delimited table are NOT claimed; and a genuinely ambiguous"
    echo "  aligned-text blob (equal-length lines, >= 2-wide gaps, >= 3 lines) is NOT"
    echo "  distinguishable from a fixed-width table and IS claimed. Detection samples a"
    echo "  bounded prefix; \`parse\` re-validates every line and declines the whole"
    echo "  document typed if any differs. Character positions are byte positions."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): both the"
    echo "  \`CsvModel\` (PSV) and the \`FixedWidthModel\` are derived (\`Q_gen\`) and never on"
    echo "  the exactness path (ADR-0060: each model node depends on the \`sha256(source)\`"
    echo "  root)."
    echo "- **Not claimed here:** the economic court (separate script), a declared"
    echo "  column map, and a full fixed-width-conformance oracle."
    echo "- **Never run on the host:** every command above ran in the pinned \`dev\`"
    echo "  container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" != "PASS" ]; then
    echo "PHASE 21.25.1 TABULAR COURT: FAIL — campaign $CAMPAIGN (exact_fail=$exact_fail surface_fail=$surface_fail declines=$decline_ok/$fixed_n boundary=$boundary_ok opaque=$opaque_ok usage=$usage_ok)" >&2
    exit 1
fi
echo "PHASE 21.25.1 TABULAR COURT: PASS — campaign $CAMPAIGN"
