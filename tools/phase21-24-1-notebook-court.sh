#!/bin/sh
# Phase 21.24.1 — Jupyter notebook (`.ipynb`, nbformat; JSON physical bytes + a bounded
# semantic sub-detection) surface + exact closure.
#
# Pre-registered (Phase-21 subphase 21.24.1). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture (including the JSON/Opaque controls),
#       `materialize(field) == source` (length + SHA-256 + cmp) after the source
#       file AND the standalone descriptor are deleted, in a fresh process. The
#       court FAILS unless this is 100% on every fixture.
#   H2 (surface + preserved representation) — the representation-preserving model is
#       exposed: the exact `nbformat`/`nbformat_minor`; the exact `cell_type` string;
#       the exact `source` representation (a string vs an array of lines, never
#       re-joined); the exact `execution_count`; cell/output order; `metadata`; and
#       `attachments`. Every output type (`stream`/`execute_result`/`display_data`/
#       `error`) is reported with its fields. Common metadata/text/search-match and
#       native notebook-nbformat/notebook-cell/notebook-cell-type/notebook-cell-source/
#       notebook-cell-output/notebook-find answer.
#   H3 (honest declines / boundaries) — an out-of-range cell/output and a cell with no
#       `source` decline typed (rc 6); a malformed `--notebook-cell-output` argument is
#       a usage error (rc 2). The controls pin the detection boundary: a plain JSON
#       document, a JSON document that merely has a `cells` key, and a JSON document
#       whose `cells` are not objects all stay `Json`; prose stays `Opaque`. A notebook
#       selector on a Json/Opaque field declines typed (rc 6).
#
# Runs in the pinned `dev` service (Rust toolchain + git + coreutils; deliberately
# NO python3 and NO jq, so the court depends only on the shipped binary, GNU
# `/usr/bin/printf` for the fixtures, and POSIX shell):
#   docker compose run --rm --no-TTY dev sh tools/phase21-24-1-notebook-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="2026-10-10"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-24-1-notebook-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-24
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
if ! cargo build --locked --all-features > "$RAW/build.log" 2>&1; then
    tail -40 "$RAW/build.log" >&2
    echo "PHASE 21.24.1 NOTEBOOK COURT: FAIL — all-features build failed" >&2
    exit 1
fi
tail -2 "$RAW/build.log"

echo "=== generate fixtures (POSIX printf only) ==="
mk() { /usr/bin/printf '%b' "$2" > "$1"; }

# --- NORMAL: detected as a notebook, full surface exercised -------------------
# An nbformat 4 notebook: a markdown cell (line-array source + attachments), a code
# cell (string source + one output of each type), and a raw cell.
mk "$WORK/corpus/main.ipynb" \
'{"cells":[{"cell_type":"markdown","metadata":{"id":"m1"},"source":["# Title\\n","Some *text*.\\n"],"attachments":{"img.png":{"image/png":"aGk="}}},{"cell_type":"code","execution_count":7,"metadata":{"collapsed":false},"source":"print(1 + 1)\\n","outputs":[{"output_type":"stream","name":"stdout","text":["2\\n"]},{"output_type":"execute_result","execution_count":7,"data":{"text/plain":"2"},"metadata":{}},{"output_type":"display_data","data":{"image/png":"aGk="},"metadata":{}},{"output_type":"error","ename":"ValueError","evalue":"boom","traceback":["Traceback (most recent call last):","ValueError: boom"]}]},{"cell_type":"raw","metadata":{},"source":"raw text\\n"}],"metadata":{"kernelspec":{"display_name":"Python 3","name":"python3"},"language_info":{"name":"python","version":"3.11"}},"nbformat":4,"nbformat_minor":5}'
# An nbformat 3 notebook (older schema: an `input` field, no `source`).
mk "$WORK/corpus/v3.ipynb" \
'{"metadata":{},"nbformat":3,"nbformat_minor":0,"worksheets":[{"cells":[]}],"cells":[{"cell_type":"code","input":"1+1","outputs":[],"language":"python"}]}'
# A minimal nbformat 4 notebook (empty cell list).
mk "$WORK/corpus/min.ipynb" '{"nbformat":4,"cells":[]}'

# --- CONTROLS ---------------------------------------------------------------
# Plain JSON: not a notebook → stays `Json`.
mk "$WORK/corpus/plain.json" '{"a":1,"b":[2,3]}'
# A JSON doc that merely has a `cells` key but no `nbformat` → stays `Json`.
mk "$WORK/corpus/cells.json" '{"cells":[{"cell_type":"code"}]}'
# A JSON doc whose `cells` are not objects → stays `Json`.
mk "$WORK/corpus/unrelated.json" '{"nbformat":4,"cells":[1,2,3]}'
# Plain prose: never a notebook → stays `Opaque`.
mk "$WORK/corpus/prose.txt" 'The quick brown fox.\nPlain prose, not a notebook.\n'

NORMAL="main.ipynb v3.ipynb min.ipynb"
JSONCTL="plain.json cells.json unrelated.json"
OPAQUE="prose.txt"
ALL="$NORMAL $JSONCTL $OPAQUE"

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
printf 'fixture\tformat\tsrc_len\tsrc_sha\tout_len\tout_sha\tcmp\texact\tdecline_rc\tctl_rc\n' >> "$TSV"

exact_ok=0
exact_fail=0
surface_fail=0
decline_cell_ok=0
decline_output_ok=0
decline_source_ok=0
jsonctl_ok=0
opaque_ok=0
opaque_n=0
normal_n=0
usage_ok=0

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
    ctl_rc=-1

    if is_in "$f" "$NORMAL"; then
        normal_n=$((normal_n + 1))
        [ "$fmt" = "notebook" ] || { echo "FINDING: $f not detected as notebook (fmt=$fmt)" >&2; surface_fail=$((surface_fail + 1)); }

        # H2: common metadata / text / search-match.
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --text "cell_type" --kind text \
            > "$RAW/$f.search.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"format":"notebook"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata missing format" >&2; surface_fail=$((surface_fail + 1)); }

        # H2: native notebook-nbformat.
        "$BIN" observe --store "$WORK/store" --field "$field" --notebook-nbformat --kind text \
            > "$RAW/$f.nbformat.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --notebook-nbformat --kind exact \
            > "$RAW/$f.nbformat.bin" 2>&1 || surface_fail=$((surface_fail + 1))

        # H2: native notebook-find (reuses the JSON match vocabulary).
        "$BIN" observe --store "$WORK/store" --field "$field" --notebook-find cell_type --kind text \
            > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))

        case "$f" in
            main.ipynb)
                grep -q '"nbformat":4' "$RAW/$f.metadata.json" || { echo "FINDING: main nbformat" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"nbformat_minor":5' "$RAW/$f.metadata.json" || { echo "FINDING: main nbformat_minor" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"cells":3' "$RAW/$f.metadata.json" || { echo "FINDING: main cell count" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"outputs":4' "$RAW/$f.metadata.json" || { echo "FINDING: main output count" >&2; surface_fail=$((surface_fail + 1)); }
                [ "$(json_str "$(cat "$RAW/$f.nbformat.json")" text)" = "4" ] || { echo "FINDING: main nbformat text" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"value_hex":"34"' "$RAW/$f.nbformat.bin" || { echo "FINDING: main nbformat exact" >&2; surface_fail=$((surface_fail + 1)); }

                # A cell descriptor reports its type, source form, execution_count, outputs.
                "$BIN" observe --store "$WORK/store" --field "$field" --notebook-cell 1 --kind metadata \
                    > "$RAW/$f.cell1.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"cell_type":"code"' "$RAW/$f.cell1.json" || { echo "FINDING: main cell type" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"source_form":"string"' "$RAW/$f.cell1.json" || { echo "FINDING: main cell source form" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"execution_count":7' "$RAW/$f.cell1.json" || { echo "FINDING: main execution_count" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"outputs":4' "$RAW/$f.cell1.json" || { echo "FINDING: main cell outputs" >&2; surface_fail=$((surface_fail + 1)); }

                # The exact cell_type token.
                "$BIN" observe --store "$WORK/store" --field "$field" --notebook-cell-type 0 --kind text \
                    > "$RAW/$f.celltype0.json" 2>&1 || surface_fail=$((surface_fail + 1))
                "$BIN" observe --store "$WORK/store" --field "$field" --notebook-cell-type 0 --kind exact \
                    > "$RAW/$f.celltype0.bin" 2>&1 || surface_fail=$((surface_fail + 1))
                [ "$(json_str "$(cat "$RAW/$f.celltype0.json")" text)" = "markdown" ] || { echo "FINDING: main cell_type text" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"value_hex":"226d61726b646f776e22"' "$RAW/$f.celltype0.bin" || { echo "FINDING: main cell_type exact" >&2; surface_fail=$((surface_fail + 1)); }

                # A string source is decoded; a line-array source keeps its form.
                "$BIN" observe --store "$WORK/store" --field "$field" --notebook-cell-source 1 --kind text \
                    > "$RAW/$f.source1.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -qF '"text":"print(1 + 1)\n"' "$RAW/$f.source1.json" || { echo "FINDING: main string source" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --notebook-cell-source 0 --kind metadata \
                    > "$RAW/$f.source0.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"form":"lines"' "$RAW/$f.source0.json" || { echo "FINDING: main lines source form" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"elements":2' "$RAW/$f.source0.json" || { echo "FINDING: main lines source elements" >&2; surface_fail=$((surface_fail + 1)); }

                # One output of each type.
                "$BIN" observe --store "$WORK/store" --field "$field" --notebook-cell-output 1:0 --kind metadata \
                    > "$RAW/$f.out0.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"output_type":"stream"' "$RAW/$f.out0.json" || { echo "FINDING: main stream type" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"name":"stdout"' "$RAW/$f.out0.json" || { echo "FINDING: main stream name" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"text_form":"lines"' "$RAW/$f.out0.json" || { echo "FINDING: main stream text form" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --notebook-cell-output 1:1 --kind metadata \
                    > "$RAW/$f.out1.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"output_type":"execute_result"' "$RAW/$f.out1.json" || { echo "FINDING: main execute_result" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"data_keys":\["text/plain"\]' "$RAW/$f.out1.json" || { echo "FINDING: main data_keys" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --notebook-cell-output 1:2 --kind metadata \
                    > "$RAW/$f.out2.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"output_type":"display_data"' "$RAW/$f.out2.json" || { echo "FINDING: main display_data" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --notebook-cell-output 1:3 --kind metadata \
                    > "$RAW/$f.out3.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"output_type":"error"' "$RAW/$f.out3.json" || { echo "FINDING: main error type" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"ename":"ValueError"' "$RAW/$f.out3.json" || { echo "FINDING: main error ename" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"traceback":2' "$RAW/$f.out3.json" || { echo "FINDING: main error traceback" >&2; surface_fail=$((surface_fail + 1)); }

                grep -q '"role":"key"' "$RAW/$f.find.json" || { echo "FINDING: main find missing key role" >&2; surface_fail=$((surface_fail + 1)); } ;;
            v3.ipynb)
                [ "$(json_str "$(cat "$RAW/$f.nbformat.json")" text)" = "3" ] || { echo "FINDING: v3 nbformat text" >&2; surface_fail=$((surface_fail + 1)); } ;;
            min.ipynb)
                [ "$(json_str "$(cat "$RAW/$f.nbformat.json")" text)" = "4" ] || { echo "FINDING: min nbformat text" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"cells":0' "$RAW/$f.metadata.json" || { echo "FINDING: min cell count" >&2; surface_fail=$((surface_fail + 1)); } ;;
        esac

        # H3: an out-of-range cell and an out-of-range output decline typed (rc 6).
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --notebook-cell 999 --kind metadata \
            > /dev/null 2>&1
        decline_rc=$?
        set -e
        [ "$decline_rc" -eq 6 ] && decline_cell_ok=$((decline_cell_ok + 1)) || { echo "FINDING: $f out-of-range cell rc=$decline_rc (want 6)" >&2; }
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --notebook-cell-output 999:0 --kind metadata \
            > /dev/null 2>&1
        o_rc=$?
        set -e
        [ "$o_rc" -eq 6 ] && decline_output_ok=$((decline_output_ok + 1)) || { echo "FINDING: $f out-of-range output rc=$o_rc (want 6)" >&2; }

        # H3: a cell with no `source` (nbformat 3's `input`) declines typed (rc 6).
        if [ "$f" = "v3.ipynb" ]; then
            set +e
            "$BIN" observe --store "$WORK/store" --field "$field" --notebook-cell-source 0 --kind text \
                > "$RAW/$f.source-decline.json" 2>&1
            s_rc=$?
            set -e
            [ "$s_rc" -eq 6 ] && decline_source_ok=$((decline_source_ok + 1)) || { echo "FINDING: v3 source-less cell rc=$s_rc (want 6)" >&2; surface_fail=$((surface_fail + 1)); }
        fi

    elif is_in "$f" "$JSONCTL"; then
        # The boundary: a plain/bare-cells/non-object-cells JSON document stays `Json`.
        [ "$fmt" = "json" ] || { echo "FINDING: control $f detected as $fmt (want json)" >&2; surface_fail=$((surface_fail + 1)); }
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --notebook-nbformat --kind text \
            > "$RAW/$f.notebook-decline.json" 2>&1
        ctl_rc=$?
        set -e
        [ "$ctl_rc" -eq 6 ] && jsonctl_ok=$((jsonctl_ok + 1)) || { echo "FINDING: JSON control $f notebook selector rc=$ctl_rc (want 6)" >&2; }

    elif is_in "$f" "$OPAQUE"; then
        [ "$fmt" = "opaque" ] || { echo "FINDING: control $f detected as $fmt (want opaque)" >&2; surface_fail=$((surface_fail + 1)); }
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1
        ctl_rc=$?
        set -e
        [ "$ctl_rc" -eq 6 ] && opaque_ok=$((opaque_ok + 1)) || { echo "FINDING: opaque control $f metadata rc=$ctl_rc (want 6)" >&2; }
        opaque_n=$((opaque_n + 1))
    fi

    # A malformed `--notebook-cell-output` argument is a usage error (rc 2) — checked once.
    if [ "$f" = "main.ipynb" ]; then
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --notebook-cell-output abc --kind metadata \
            > /dev/null 2>&1
        u_rc=$?
        set -e
        [ "$u_rc" -eq 2 ] && usage_ok=$((usage_ok + 1)) || echo "FINDING: malformed --notebook-cell-output rc=$u_rc (want 2)" >&2
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
        "$decline_rc" "$ctl_rc" >> "$TSV"
done

echo "=== exactness after source + descriptor deletion ==="
cat "$TSV"

fixtures_n="$(printf '%s\n' $ALL | wc -w | tr -d ' ')"

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 21.24.1 Notebook court — matrix"
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
    echo "notebook_fixtures $normal_n"
    echo "exact_ok $exact_ok"
    echo "exact_fail $exact_fail"
    echo "surface_fail $surface_fail"
    echo "typed_decline_cells_ok $decline_cell_ok"
    echo "typed_decline_outputs_ok $decline_output_ok"
    echo "typed_decline_source_less_ok $decline_source_ok"
    echo "json_controls_ok $jsonctl_ok"
    echo "usage_ok $usage_ok"
    echo "opaque_controls_ok $opaque_ok"
    echo "opaque_controls $opaque_n"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.24.1 — Jupyter notebook (nbformat) surface + exact closure",
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
    || [ "$decline_cell_ok" -ne "$normal_n" ] || [ "$decline_output_ok" -ne "$normal_n" ] \
    || [ "$decline_source_ok" -ne 1 ] \
    || [ "$jsonctl_ok" -ne 3 ] || [ "$usage_ok" -ne 1 ] || [ "$opaque_ok" -ne "$opaque_n" ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.24.1 — Jupyter notebook (nbformat) surface + exact closure",
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
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion, for every fixture including the JSON/Opaque controls",
  "observations": ["metadata", "text", "search-match", "native notebook-nbformat", "native notebook-cell", "native notebook-cell-type", "native notebook-cell-source", "native notebook-cell-output", "native notebook-find"],
  "surface": "the exact nbformat/nbformat_minor; the exact cell_type string; the exact source representation (a string vs an array of lines, never re-joined); the exact execution_count; cell/output order; metadata; attachments; and every output type (stream/execute_result/display_data/error) with its fields",
  "declines": "an out-of-range cell/output and a cell with no source decline typed (rc 6); a malformed --notebook-cell-output argument is a usage error (rc 2); the plain-JSON / bare-cells / non-object-cells controls stay Json and a native notebook selector on each declines typed (rc 6); prose stays Opaque and a common observation declines typed (rc 6), never a panic",
  "detection_boundary": "a notebook's physical bytes are JSON, so detection is a bounded semantic sub-test run before the generic JSON detector: the source must parse as exactly one JSON value that is an nbformat-shaped object — a plain non-negative integer-literal nbformat (>= 1), an array cells, every cell an object with a string cell_type, and every present recognized field nbformat-shaped (a string-or-array-of-strings source, a number-or-null execution_count, object metadata/attachments, an array outputs of objects with a string output_type). A plain JSON document, a JSON document that merely has a cells key, a JSON document whose cells are not objects, and a non-integer nbformat (e.g. 4.0/1e2/-0) all stay Json; prose stays Opaque.",
  "cannot_distinguish": "cell_type/output_type strings are preserved but not restricted to the known set (an unrecognized value is recorded as given); nbformat is not restricted to a version (1..4 and beyond are all accepted); a source string is never split and a source array is never joined, so the two representations cannot be conflated; a number literal is never reparsed (nbformat must be a plain integer literal).",
  "precedence": "after the strong magic-byte binaries (PDF/ZIP/Parquet/Arrow) and after GeoJSON; immediately before the generic JSON detector; before JSON5/JSONL/YAML/TOML/CSV/Markdown/XML/HTML and everything else",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 38 invalid-notebook-structure",
  "adr_0060": "the NotebookModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase21-24-1-notebook-court.sh
# inside the court:
#   cargo build --locked --all-features
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--text PAT|--notebook-nbformat|--notebook-cell N|--notebook-cell-type N|--notebook-cell-source N|--notebook-cell-output N:M|--notebook-find PAT) --kind metadata|text|structure|exact
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
    echo "# Phase 21.24.1 — Jupyter notebook (nbformat) court"
    echo
    echo "**Question.** Does the notebook adapter close exactly and expose a"
    echo "representation-preserving model (the exact nbformat/nbformat_minor; the exact"
    echo "cell_type string; the exact source representation — a string vs an array of"
    echo "lines, never re-joined; the exact execution_count; cell/output order; metadata;"
    echo "attachments; and every output type with its fields) on top of the whole-source"
    echo "exact leaf — while keeping the bounded semantic sub-detection boundary (before"
    echo "the generic JSON detector) and declining malformed/ambiguous inputs typed?"
    echo
    echo "**Method.** Each self-authored fixture is generated in-court with GNU"
    echo "\`/usr/bin/printf\`, ingested via \`field-build\`, observed, then — after the"
    echo "**source file and the standalone descriptor are deleted** — rematerialized"
    echo "exactly in a fresh process and compared with \`length + SHA-256 + cmp\`. An"
    echo "out-of-range cell/output and a cell with no \`source\` are required to decline"
    echo "typed; the plain-JSON / bare-cells / non-object-cells controls and prose pin the"
    echo "detection boundaries. The court runs in the pinned \`dev\` service using only"
    echo "POSIX \`sh\`, coreutils, git, and the shipped binary (no python3, no jq)."
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
    echo "- **Shipped here:** byte-based bounded semantic notebook detection; one bounded"
    echo "  notebook model **reusing the shared JSON parser** (never a second JSON"
    echo "  parser); native \`notebook-nbformat\`/\`notebook-cell\`/\`notebook-cell-type\`/"
    echo "  \`notebook-cell-source\`/\`notebook-cell-output\`/\`notebook-find\`; common"
    echo "  \`metadata\`/\`text\`/\`search-match\`."
    echo "- **Precedence:** after the strong magic-byte binaries (PDF/ZIP/Parquet/Arrow)"
    echo "  and after GeoJSON; immediately before the generic JSON detector. A notebook is"
    echo "  a more specific claim than a bare JSON value, so it is tried first."
    echo "- **Detection boundary:** a notebook's physical bytes are JSON. A notebook is"
    echo "  claimed only when the root object has a plain non-negative integer-literal"
    echo "  \`nbformat\` (>= 1) and an array \`cells\`, every cell an object with a string"
    echo "  \`cell_type\`, and every present recognized field nbformat-shaped (a"
    echo "  string-or-array-of-strings \`source\`, a number-or-null \`execution_count\`, object"
    echo "  \`metadata\`/\`attachments\`, an array \`outputs\` of objects with a string"
    echo "  \`output_type\`). A plain JSON document, a JSON document that merely has a"
    echo "  \`cells\` key, a JSON document whose \`cells\` are not objects, and a non-integer"
    echo "  \`nbformat\` all stay \`Json\`; prose stays \`Opaque\`."
    echo "- **Recorded negatives (not distinguished):** \`cell_type\`/\`output_type\` strings"
    echo "  are preserved but not restricted to the known set; \`nbformat\` is not restricted"
    echo "  to a version; a \`source\` string is never split and a \`source\` array is never"
    echo "  joined (the two forms cannot be conflated); a number literal is never reparsed"
    echo "  (so \`4.0\`/\`4e0\`/\`-0\` is not an integer \`nbformat\` and stays \`Json\`)."
    echo "- **Declines:** an out-of-range cell/output, a cell with no \`source\`, a"
    echo "  malformed \`--notebook-cell-output\` argument, a cap breach, and a non-notebook"
    echo "  source are typed (\`InvalidNotebookStructure\` rc 38, unsupported-feature rc 6,"
    echo "  resource-limit rc 8, or usage rc 2); such input stays \`Json\`/\`Opaque\` when"
    echo "  detection declines."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the notebook"
    echo "  model is derived (\`Q_gen\`) and never on the exactness path (ADR-0060: the"
    echo "  model node depends on the \`sha256(source)\` root)."
    echo "- **Not claimed here:** executing a notebook, validating every nbformat schema"
    echo "  rule, the economic court (separate script)."
    echo "- **Never run on the host:** every command above ran in the pinned \`dev\`"
    echo "  container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" != "PASS" ]; then
    echo "PHASE 21.24.1 NOTEBOOK COURT: FAIL — campaign $CAMPAIGN (exact_fail=$exact_fail surface_fail=$surface_fail dc=$decline_cell_ok do=$decline_output_ok ds=$decline_source_ok jsonctl=$jsonctl_ok usage=$usage_ok opaque=$opaque_ok/$opaque_n)" >&2
    exit 1
fi
echo "PHASE 21.24.1 NOTEBOOK COURT: PASS — campaign $CAMPAIGN"
