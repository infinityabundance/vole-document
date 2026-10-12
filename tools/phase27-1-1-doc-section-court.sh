#!/bin/sh
# Phase 27.1 — legacy Word 97–2003 (`.doc`, MS-DOC) **depth**: the footnote / header /
# annotation / endnote / textbox CP sections materialized as separate typed answers.
#
# Pre-registered (Phase-27 subphase 27.1.1). Hypotheses:
#
#   H1 (byte-exactness) — for every normal fixture, `materialize(field) == source`
#       (length + SHA-256 + cmp) after the source file AND the standalone descriptor
#       are deleted, in a fresh process. The court FAILS unless this is 100% on every
#       normal fixture. The extended `DocModel` still depends on the exact
#       `sha256(source)` root (ADR-0060); sections are derived, never exact.
#   H2 (sectioned surface + preserved representation) — the single contiguous CP space
#       of the piece table is partitioned by the cumulative `ccp*` counts into
#       main/footnote/header/annotation/endnote/textbox/hdrtextbox, and each section is
#       exposed as its own span-preserving answer: its name and FIB `ccp*` field, its CP
#       range, the ordered pieces clipped to it (piece index, clipped CP range, exact
#       `WordDocument`-stream byte span, compressed flag, and decoded text), and the
#       section's concatenated decoded text. The fixture's pieces **cross section
#       boundaries** (main/footnote, footnote/header, header/annotation, with one
#       uncompressed piece), so the clipping is observable. The main text
#       (`[0, ccpText)`) and the existing `doc-fib`/`doc-piece`/`doc-text`/`doc-find`
#       answers are unchanged. A section whose `ccp*` count is zero (`hdrtextbox`) is a
#       typed **empty** answer (rc 0, `"empty":true`, no pieces, empty text) — never an
#       error.
#   H3 (honest declines / boundaries) — the controls pin the detection boundary and the
#       extended subset's edges: a generic compound file with no Word FIB, a `.doc` with
#       a bad `wIdent`, and a `.doc` with an implausible `nFib` (Word 6/95) all stay
#       `Cfbf` (a valid compound file the CFBF layer accepts) and a doc selector on them
#       declines typed (rc 6); prose stays `Opaque` (rc 6); a `.doc` with a plausible
#       FIB but a malformed body (an out-of-range `fcClx`/`fcMin`, a non-monotonic CP
#       array, a PCD `fc` outside the `WordDocument` stream) detects as `doc` but its
#       model — and therefore every section selector — declines typed
#       (InvalidDocStructure, rc 52); an unknown `--doc-section` name is a usage error
#       (rc 2). Never a panic, never a silent/empty answer that hides a decline.
#
# Runs in the pinned `dev` service (Rust toolchain + git + coreutils + sed/grep;
# deliberately NO python3, NO Word/LibreOffice/`wvWare`, NO external OLE tooling, NO
# network):
#   docker compose run --rm --no-TTY dev sh tools/phase27-1-1-doc-section-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase27-1-1-doc-section-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase27-1
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== generate fixtures (dependency-free .doc writer) ==="
cargo run --locked --all-features --example doc_fixtures -- "$WORK/corpus" 2>&1 | tail -1

# Fixture classes.
#   NORMAL   — valid `.doc`: H1 exactness + H2 surface (incl. all seven CP sections).
#   DECLINE  — detects as `doc` but the model is structurally malformed (rc 52).
#   CFBF_CTL — a valid compound file that is NOT a Word 97–2003 document (format
#              `cfbf`); a doc selector declines typed (rc 6).
#   OPAQUE   — not a compound file at all; a doc selector declines typed (rc 6).
NORMAL="doc_0table.doc doc_1table.doc doc_sections.doc"
DECLINE="doc_bad_clx_range.doc doc_fcmin_range.doc doc_nonmonotonic.doc doc_piece_outside.doc"
CFBF_CTL="doc_bad_ident.doc doc_bad_nfib.doc generic_cfbf.bin"
OPAQUE="prose.txt"

table_for() {
    case "$1" in
        doc_1table.doc) echo "1Table" ;;
        *) echo "0Table" ;;
    esac
}
pieces_for() {
    case "$1" in
        doc_sections.doc) echo "7" ;;
        *) echo "3" ;;
    esac
}
main_grep() {
    case "$1" in
        doc_sections.doc) echo "AAABBB" ;;
        *) echo "Hello, w" ;;
    esac
}
find_pat() {
    case "$1" in
        doc_sections.doc) echo "AAAB" ;;
        *) echo "Hello" ;;
    esac
}
# The first piece of each normal fixture (its span, exact byte length, and hex) differs:
# `doc_sections.doc` piece 0 is the compressed `AAAB` (4 bytes), the others the
# compressed `Hello, ` (7 bytes).
piece0_span() {
    case "$1" in
        doc_sections.doc) echo "[898,902]" ;;
        *) echo "[898,905]" ;;
    esac
}
piece0_bytes_len() {
    case "$1" in
        doc_sections.doc) echo 4 ;;
        *) echo 7 ;;
    esac
}
piece0_hex() {
    case "$1" in
        doc_sections.doc) echo 41414142 ;;
        *) echo 48656c6c6f2c20 ;;
    esac
}
# Extract one JSON string field from a single-line JSON object (greedy, no jq).
json_str() {
    printf '%s' "$1" | sed -n "s/.*\"$2\":\"\([^\"]*\)\".*/\1/p"
}
# The expected concatenated text of each CP section of `doc_sections.doc`.
sec_text() {
    case "$1" in
        main) echo "AAABBB" ;;
        footnote) echo "F1F2" ;;
        header) echo "HDR" ;;
        annotation) echo "CMT" ;;
        endnote) echo "END" ;;
        textbox) echo "TBX" ;;
        hdrtextbox) echo "" ;;
    esac
}

TSV="$RAW/results.tsv"
: > "$TSV"
printf 'fixture\tformat\tsrc_len\tsrc_sha\tout_len\tout_sha\tcmp\texact\tdecline_rc\tctl_rc\n' >> "$TSV"

exact_ok=0
exact_fail=0
surface_fail=0
opaque_ok=0
opaque_n=0
normal_n=0
piece_ok=0
piece_n=0
decline_after_detect_ok=0
decline_n=0
cfbf_ctl_ok=0
cfbf_ctl_n=0
sections_ok=0
sections_n=0
boundary_ok=0
boundary_n=0
empty_ok=0
empty_n=0
usage_ok=0
usage_n=0

for f in $DECLINE; do
    src="$WORK/ref/$f"
    cp "$WORK/corpus/$f" "$src"
    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    fmt="$(json_str "$build_json" format)"
    field="$(json_str "$build_json" field)"
    decline_n=$((decline_n + 1))
    [ "$fmt" = "doc" ] || { echo "FINDING: $f detected as $fmt (want doc)" >&2; surface_fail=$((surface_fail + 1)); }
    set +e
    "$BIN" observe --store "$WORK/store" --field "$field" --doc-fib --kind text \
        > "$RAW/$f.decline.txt" 2>&1
    ctl_rc=$?
    set -e
    if [ "$ctl_rc" -eq 52 ]; then decline_after_detect_ok=$((decline_after_detect_ok + 1)); else echo "FINDING: $f structural decline rc=$ctl_rc (want 52)" >&2; fi
    # A section selector on a structurally malformed document declines the same way.
    set +e
    "$BIN" observe --store "$WORK/store" --field "$field" --doc-section main --kind text \
        > "$RAW/$f.section.decline.txt" 2>&1
    sec_rc=$?
    set -e
    if [ "$sec_rc" -ne 52 ]; then echo "FINDING: $f section decline rc=$sec_rc (want 52)" >&2; surface_fail=$((surface_fail + 1)); fi
    printf '%s\t%s\t-\t-\t-\t-\t-\t-\t-\t%s\n' "$f" "$fmt" "$ctl_rc" >> "$TSV"
done

for f in $CFBF_CTL; do
    src="$WORK/ref/$f"
    cp "$WORK/corpus/$f" "$src"
    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    fmt="$(json_str "$build_json" format)"
    field="$(json_str "$build_json" field)"
    cfbf_ctl_n=$((cfbf_ctl_n + 1))
    [ "$fmt" = "cfbf" ] || { echo "FINDING: CFBF control $f detected as $fmt (want cfbf)" >&2; surface_fail=$((surface_fail + 1)); }
    set +e
    "$BIN" observe --store "$WORK/store" --field "$field" --doc-section footnote --kind text \
        > "$RAW/$f.decline.txt" 2>&1
    ctl_rc=$?
    set -e
    if [ "$ctl_rc" -eq 6 ]; then cfbf_ctl_ok=$((cfbf_ctl_ok + 1)); else echo "FINDING: CFBF control $f doc section rc=$ctl_rc (want 6)" >&2; fi
    printf '%s\t%s\t-\t-\t-\t-\t-\t-\t-\t%s\n' "$f" "$fmt" "$ctl_rc" >> "$TSV"
done

for f in $OPAQUE; do
    src="$WORK/ref/$f"
    cp "$WORK/corpus/$f" "$src"
    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    fmt="$(json_str "$build_json" format)"
    field="$(json_str "$build_json" field)"
    opaque_n=$((opaque_n + 1))
    [ "$fmt" = "opaque" ] || { echo "FINDING: opaque control $f detected as $fmt (want opaque)" >&2; surface_fail=$((surface_fail + 1)); }
    set +e
    "$BIN" observe --store "$WORK/store" --field "$field" --doc-section main --kind text \
        > "$RAW/$f.decline.txt" 2>&1
    ctl_rc=$?
    set -e
    if [ "$ctl_rc" -eq 6 ]; then opaque_ok=$((opaque_ok + 1)); else echo "FINDING: opaque control $f doc section rc=$ctl_rc (want 6)" >&2; fi
    printf '%s\t%s\t-\t-\t-\t-\t-\t-\t-\t%s\n' "$f" "$fmt" "$ctl_rc" >> "$TSV"
done

for f in $NORMAL; do
    src="$WORK/ref/$f"
    cp "$WORK/corpus/$f" "$src"
    src_len="$(wc -c < "$src" | tr -d ' ')"
    src_sha="$(sha256sum "$src" | cut -d' ' -f1)"

    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    field="$(json_str "$build_json" field)"
    fmt="$(json_str "$build_json" format)"

    normal_n=$((normal_n + 1))
    want_table="$(table_for "$f")"
    want_pieces="$(pieces_for "$f")"
    want_main="$(main_grep "$f")"
    want_find="$(find_pat "$f")"
    [ "$fmt" = "doc" ] || { echo "FINDING: $f not detected as doc (fmt=$fmt)" >&2; surface_fail=$((surface_fail + 1)); }

    # H2: common metadata / text / search-match + native fib / piece / text / find.
    "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
        > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
        > "$RAW/$f.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --doc-fib --kind text \
        > "$RAW/$f.fib.txt" 2>&1 || surface_fail=$((surface_fail + 1))
    # The native document-text selector (canonical `doc-text`, CLI `--doc-body`).
    "$BIN" observe --store "$WORK/store" --field "$field" --doc-body --kind text \
        > "$RAW/$f.body.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --doc-find "$want_find" --kind metadata \
        > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))

    grep -q '"format":"doc"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata missing format" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"nFib":193' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata missing nFib 193" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"version":"word97"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata version != word97" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q "\"table\":\"$want_table\"" "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata table != $want_table" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q "\"pieces\":$want_pieces" "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata piece count != $want_pieces" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"text":"word97"' "$RAW/$f.fib.txt" || { echo "FINDING: $f fib text != word97" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"selector":"doc-text"' "$RAW/$f.body.json" || { echo "FINDING: $f native doc-text selector missing" >&2; surface_fail=$((surface_fail + 1)); }
    # The document text is the main-text pieces only — the decoy must never win.
    grep -q "$want_main" "$RAW/$f.text.json" || { echo "FINDING: $f document text missing ($want_main)" >&2; surface_fail=$((surface_fail + 1)); }
    if grep -q 'DECOY' "$RAW/$f.text.json"; then echo "FINDING: $f document text picked the decoy table" >&2; surface_fail=$((surface_fail + 1)); fi
    grep -q '"matches":\[{' "$RAW/$f.find.json" || { echo "FINDING: $f find returned no matches" >&2; surface_fail=$((surface_fail + 1)); }

    # H2: the existing piece selectors are unchanged (main-text pieces only).
    piece_n=$((piece_n + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --doc-piece 0 --kind metadata \
        > "$RAW/$f.piece0.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --doc-piece 0 --kind exact \
        > "$RAW/$f.piece0.bin" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --doc-piece 1 --kind metadata \
        > "$RAW/$f.piece1.json" 2>&1 || surface_fail=$((surface_fail + 1))
    grep -q '"compressed":true' "$RAW/$f.piece0.json" || { echo "FINDING: $f piece0 not compressed" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"cp_start":0' "$RAW/$f.piece0.json" || { echo "FINDING: $f piece0 cp_start != 0" >&2; surface_fail=$((surface_fail + 1)); }
    grep -Fq "\"span\":$(piece0_span "$f")" "$RAW/$f.piece0.json" || { echo "FINDING: $f piece0 span wrong" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"compressed":false' "$RAW/$f.piece1.json" || { echo "FINDING: $f piece1 should be uncompressed" >&2; surface_fail=$((surface_fail + 1)); }
    # The compressed piece's exact bytes are its CP1252 bytes.
    if grep -q "\"bytes_len\":$(piece0_bytes_len "$f")" "$RAW/$f.piece0.bin" && grep -q "$(piece0_hex "$f")" "$RAW/$f.piece0.bin"; then
        piece_ok=$((piece_ok + 1))
    else
        echo "FINDING: $f piece0 exact bytes are wrong" >&2
        surface_fail=$((surface_fail + 1))
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
        "-" "-" >> "$TSV"
done

# ---------------------------------------------------------------------------
# H2 (depth): the seven CP sections of `doc_sections.doc` as separate answers.
# ---------------------------------------------------------------------------
build_json="$("$BIN" field-build "$WORK/ref/doc_sections.doc" --store "$WORK/store" \
    --voldoc "$WORK/sec.voldoc")"
sec_field="$(json_str "$build_json" field)"

for spec in "main 0 6 2" "footnote 6 10 2" "header 10 13 2" \
    "annotation 13 16 2" "endnote 16 19 1" "textbox 19 22 1" "hdrtextbox 22 22 0"; do
    # shellcheck disable=SC2086
    set -- $spec
    name="$1"; cs="$2"; ce="$3"; np="$4"
    want="$(sec_text "$name")"
    sections_n=$((sections_n + 1))
    ok=1
    "$BIN" observe --store "$WORK/store" --field "$sec_field" --doc-section "$name" --kind text \
        > "$RAW/doc_sections.$name.text.json" 2>&1 || ok=0
    "$BIN" observe --store "$WORK/store" --field "$sec_field" --doc-section "$name" --kind metadata \
        > "$RAW/doc_sections.$name.json" 2>&1 || ok=0
    grep -q "\"selector\":\"doc-section:$name\"" "$RAW/doc_sections.$name.text.json" || ok=0
    grep -q "\"text\":\"$want\"" "$RAW/doc_sections.$name.text.json" || ok=0
    grep -q "\"section\":\"$name\"" "$RAW/doc_sections.$name.json" || ok=0
    grep -q "\"cp_start\":$cs,\"cp_end\":$ce" "$RAW/doc_sections.$name.json" || ok=0
    grep -q "\"pieces\":$np" "$RAW/doc_sections.$name.json" || ok=0
    if [ "$name" = "hdrtextbox" ]; then
        grep -q '"empty":true' "$RAW/doc_sections.$name.json" || ok=0
    else
        grep -q '"empty":false' "$RAW/doc_sections.$name.json" || ok=0
    fi
    if [ "$ok" = 1 ]; then
        sections_ok=$((sections_ok + 1))
    else
        echo "FINDING: section $name failed (cp=$cs:$ce pieces=$np text='$want')" >&2
        surface_fail=$((surface_fail + 1))
    fi
done

# H2 (depth): the pieces that cross section boundaries are clipped exactly — the piece
# index, the clipped CP range, the compressed flag, the exact WordDocument-stream byte
# span of the clipped portion, and the decoded text.
boundary_check() { # name fragment
    boundary_n=$((boundary_n + 1))
    if grep -Fq "$2" "$RAW/doc_sections.$1.json"; then
        boundary_ok=$((boundary_ok + 1))
    else
        echo "FINDING: section $1 missing clipped piece '$2'" >&2
        surface_fail=$((surface_fail + 1))
    fi
}
boundary_check main '{"piece":0,"cp_start":0,"cp_end":4,"compressed":true,"units":4,"span":[898,902]'
boundary_check main '{"piece":1,"cp_start":4,"cp_end":6,"compressed":false,"units":2,"span":[902,906]'
boundary_check footnote '{"piece":1,"cp_start":6,"cp_end":8,"compressed":false,"units":2,"span":[906,910]'
boundary_check footnote '{"piece":2,"cp_start":8,"cp_end":10,"compressed":true,"units":2,"span":[910,912]'
boundary_check header '{"piece":2,"cp_start":10,"cp_end":11,"compressed":true,"units":1,"span":[912,913]'
boundary_check header '{"piece":3,"cp_start":11,"cp_end":13,"compressed":true,"units":2,"span":[913,915]'
boundary_check annotation '{"piece":3,"cp_start":13,"cp_end":14,"compressed":true,"units":1,"span":[915,916]'
boundary_check annotation '{"piece":4,"cp_start":14,"cp_end":16,"compressed":true,"units":2,"span":[916,918]'
boundary_check endnote '{"piece":5,"cp_start":16,"cp_end":19,"compressed":true,"units":3,"span":[918,921]'
boundary_check textbox '{"piece":6,"cp_start":19,"cp_end":22,"compressed":true,"units":3,"span":[921,924]'

# H2 (depth): the empty section is a typed empty answer — no pieces, no text, and no
# error — not a decline.
empty_n=$((empty_n + 1))
set +e
"$BIN" observe --store "$WORK/store" --field "$sec_field" --doc-section hdrtextbox --kind metadata \
    > "$RAW/doc_sections.hdrtextbox.metadata2.json" 2>&1
empty_rc=$?
set -e
if [ "$empty_rc" -eq 0 ] \
    && grep -q '"empty":true' "$RAW/doc_sections.hdrtextbox.metadata2.json" \
    && grep -q '"pieces":0' "$RAW/doc_sections.hdrtextbox.metadata2.json" \
    && grep -q '"text_units":0' "$RAW/doc_sections.hdrtextbox.metadata2.json" \
    && grep -q '"section_pieces":\[\]' "$RAW/doc_sections.hdrtextbox.metadata2.json"; then
    empty_ok=$((empty_ok + 1))
else
    echo "FINDING: hdrtextbox empty section is not a typed empty answer (rc=$empty_rc)" >&2
    surface_fail=$((surface_fail + 1))
fi

# H3 (boundary): an unknown section name is a usage error, never a silent answer.
usage_n=$((usage_n + 1))
set +e
"$BIN" observe --store "$WORK/store" --field "$sec_field" --doc-section bogus --kind text \
    > "$RAW/unknown_section.txt" 2>&1
usage_rc=$?
set -e
if [ "$usage_rc" -eq 2 ]; then usage_ok=$((usage_ok + 1)); else echo "FINDING: unknown --doc-section rc=$usage_rc (want 2)" >&2; surface_fail=$((surface_fail + 1)); fi

echo "=== exactness after source + descriptor deletion ==="
cat "$TSV"

fixtures_n=$((normal_n + opaque_n + cfbf_ctl_n))

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 27.1.1 legacy Word (.doc) section court — matrix"
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

# --- sections.tsv -----------------------------------------------------------
{
    printf 'section\tcp_start\tcp_end\tpieces\ttext\tsource\n'
    for spec in "main 0 6 2" "footnote 6 10 2" "header 10 13 2" \
        "annotation 13 16 2" "endnote 16 19 1" "textbox 19 22 1" "hdrtextbox 22 22 0"; do
        # shellcheck disable=SC2086
        set -- $spec
        name="$1"; cp_end="$3"; cp_start="$2"
        txt="$(sec_text "$name")"
        printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
            "$name" "$cp_start" "$cp_end" "$4" "$txt" "doc_sections.$name.json" >> "$RAW/sections.tsv"
    done
    cat "$RAW/sections.tsv"
} > "$CAMPAIGN/sections.txt"

# --- counts.txt -------------------------------------------------------------
{
    echo "fixtures $fixtures_n"
    echo "normal $normal_n"
    echo "exact_ok $exact_ok"
    echo "exact_fail $exact_fail"
    echo "surface_fail $surface_fail"
    echo "pieces_ok $piece_ok"
    echo "pieces_n $piece_n"
    echo "structural_declines_ok $decline_after_detect_ok"
    echo "structural_declines $decline_n"
    echo "cfbf_controls_ok $cfbf_ctl_ok"
    echo "cfbf_controls $cfbf_ctl_n"
    echo "opaque_controls_ok $opaque_ok"
    echo "opaque_controls $opaque_n"
    echo "sections_ok $sections_ok"
    echo "sections_n $sections_n"
    echo "section_boundary_pieces_ok $boundary_ok"
    echo "section_boundary_pieces_n $boundary_n"
    echo "empty_section_ok $empty_ok"
    echo "empty_section_n $empty_n"
    echo "unknown_section_usage_ok $usage_ok"
    echo "unknown_section_usage_n $usage_n"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "27.1.1 — legacy Word 97–2003 (.doc, MS-DOC) depth: footnote/header/annotation/endnote/textbox/hdrtextbox CP sections as separate span-preserving answers",
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
  "fixtures_tool": "examples/doc_fixtures.rs (dependency-free .doc/CFBF writer; no python3, no Word/LibreOffice/wvWare, no external OLE tooling)",
  "external_oracles": "none (the DOC model is validated internally; no external Word parser is invoked)",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
if [ "$exact_fail" -ne 0 ] || [ "$surface_fail" -ne 0 ] \
    || [ "$exact_ok" -ne "$normal_n" ] \
    || [ "$piece_ok" -ne "$piece_n" ] \
    || [ "$decline_after_detect_ok" -ne "$decline_n" ] \
    || [ "$cfbf_ctl_ok" -ne "$cfbf_ctl_n" ] \
    || [ "$opaque_ok" -ne "$opaque_n" ] \
    || [ "$sections_ok" -ne "$sections_n" ] \
    || [ "$boundary_ok" -ne "$boundary_n" ] \
    || [ "$empty_ok" -ne "$empty_n" ] \
    || [ "$usage_ok" -ne "$usage_n" ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "27.1.1 — legacy Word 97–2003 (.doc, MS-DOC) depth: footnote/header/annotation/endnote/textbox/hdrtextbox CP sections as separate span-preserving answers",
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
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "fixtures": "$NORMAL $DECLINE $CFBF_CTL $OPAQUE",
  "fixture_tool": "examples/doc_fixtures.rs",
  "external_oracles": "none",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion",
  "observations": ["metadata", "text", "search-match", "native doc-fib", "native doc-piece", "native doc-text", "native doc-section (main/footnote/header/annotation/endnote/textbox/hdrtextbox)", "native doc-find"],
  "surface": "the CP space partitioned by the cumulative ccp* counts into main/footnote/header/annotation/endnote/textbox/hdrtextbox; each section is a separate span-preserving Q_gen answer (its name and FIB ccp* field, its CP range, the ordered pieces clipped to it each with the piece index, clipped CP range, exact WordDocument-stream byte span, compressed flag, and decoded text, and the section's concatenated decoded text); pieces that cross section boundaries (main/footnote, footnote/header, header/annotation, incl. one uncompressed piece) appear clipped in every section they overlap; an empty section (hdrtextbox) is a typed empty answer; the main text [0, ccpText) and the FIB/piece/main-text selectors are unchanged",
  "declines": "a .doc with a plausible FIB but a malformed body (an out-of-range fcClx, an out-of-range fcMin, a non-monotonic CP array, a PCD fc outside the WordDocument stream) detects as doc and its model — and every section selector — declines typed InvalidDocStructure (rc 52); an unknown --doc-section name is a usage error (rc 2); a generic compound file, a .doc with a bad wIdent, and a .doc with an implausible nFib (Word 6/95) stay Cfbf and a doc selector declines typed (rc 6); prose stays Opaque (rc 6); never a panic",
  "precedence": "the DOC detector is built on top of the CFBF detector and runs before the generic Cfbf claim, so a .doc becomes Doc while a generic compound file stays Cfbf; a non-CFBF source is unaffected",
  "rc_codes": "0 ok; 2 usage; 6 unsupported-feature; 8 resource-limit; 52 invalid-doc-structure",
  "adr_0060": "the DocModel node depends on the exact source root (DocumentExact, keyed by sha256(source)); the sectioned answers are derived from that same node and never on the exactness path",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase27-1-1-doc-section-court.sh
# inside the court:
#   cargo build --locked --all-features
#   cargo run --locked --all-features --example doc_fixtures -- CORPUS
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX --doc-section NAME --kind text|metadata
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--doc-fib|--doc-piece 0|--doc-find PAT) --kind metadata|text|exact
#   rm source F.voldoc
#   target/debug/vole-document materialize --store STORE --field HEX --exact --output OUT ; cmp OUT source
# Full gate (dev service):
#   cargo fmt --all --check
#   cargo clippy --all-targets --all-features -- -D warnings
#   cargo test --locked --all-features   (use CARGO_PROFILE_TEST_DEBUG=0 if the all-features link OOM-kills)
#   cargo test --locked --features doc
#   Regression controls: sh tools/phase26-1-1-doc-court.sh ; sh tools/phase21-35-1-cfbf-court.sh
#   MSRV: docker compose run --rm --no-TTY msrv cargo build --locked --all-features
EOF

# --- SUMMARY.md -------------------------------------------------------------
{
    echo "# Phase 27.1.1 — legacy Word 97–2003 (\`.doc\`, MS-DOC) section depth"
    echo
    echo "**Question.** Does the legacy DOC model materialize the footnote / header /"
    echo "annotation / endnote / textbox / hdrtextbox CP sections as separate,"
    echo "span-preserving (\`Q_gen\`) answers — with the CP space partitioned by the"
    echo "cumulative \`ccp*\` counts, pieces that cross section boundaries clipped exactly,"
    echo "and an empty section a typed empty answer — while the whole source still closes"
    echo "byte-exactly and every malformed input still declines typed?"
    echo
    echo "**Method.** Each fixture is generated by the dependency-free"
    echo "\`examples/doc_fixtures.rs\` writer, ingested via \`field-build\`, observed, then —"
    echo "after the **source file and the standalone descriptor are deleted** —"
    echo "rematerialized exactly in a fresh process and compared with \`length + SHA-256 +"
    echo "cmp\`. The CFBF, Opaque, and structural controls pin the detection boundaries. The"
    echo "court runs in the pinned \`dev\` service using only POSIX \`sh\`, coreutils, git, and"
    echo "the shipped binary (no python3, no Word tooling, no network)."
    echo
    echo "## Exactness (after source + descriptor deletion)"
    echo
    tail -n +3 "$CAMPAIGN/MATRIX.md"
    echo
    echo "## Sectioned answers (doc_sections.doc)"
    echo
    echo '```'
    cat "$CAMPAIGN/sections.txt"
    echo '```'
    echo
    echo "## Counts"
    echo
    echo '```'
    cat "$CAMPAIGN/counts.txt"
    echo '```'
    echo
    echo "## Per-fixture observation files (raw/)"
    echo
    ls "$RAW" | sed 's/^/  - /'
    echo
    echo "## Scope (honest)"
    echo
    echo "- **Supported (depth):** the single contiguous CP space partitioned by the"
    echo "  cumulative \`ccp*\` counts into \`main\`/\`footnote\`/\`header\`/\`annotation\`/"
    echo "  \`endnote\`/\`textbox\`/\`hdrtextbox\`; each section as a separate answer carrying"
    echo "  its name and FIB \`ccp*\` field, its CP range, the ordered pieces clipped to it"
    echo "  (piece index, clipped CP range, exact \`WordDocument\`-stream byte span,"
    echo "  compressed flag, decoded text), and the concatenated decoded text; pieces that"
    echo "  cross section boundaries appear clipped in every section they overlap; a"
    echo "  zero-count section is a typed empty answer."
    echo "- **Supported (unchanged):** detection on top of the CFBF layer; the FIB; the"
    echo "  piece table; each piece's \`fc\`, compressed flag, and exact stream span; the"
    echo "  main document text over \`[0, ccpText)\`; native"
    echo "  \`doc-fib\`/\`doc-piece\`/\`doc-text\`/\`doc-find\`; common"
    echo "  \`metadata\`/\`text\`/\`search-match\`."
    echo "- **Boundary:** a generic compound file, a \`.doc\` with a bad \`wIdent\`, and a"
    echo "  \`.doc\` with an implausible \`nFib\` (Word 6/95) stay \`Cfbf\`; prose stays"
    echo "  \`Opaque\`; a \`.doc\` with a plausible FIB but a malformed body is admitted and"
    echo "  its model (and every section selector) declines typed."
    echo "- **Declines:** a malformed body (\`fcClx\`/\`fcMin\` out of range, a non-monotonic"
    echo "  CP array, a PCD \`fc\` outside the \`WordDocument\` stream, a piece-table last CP"
    echo "  disagreeing with the declared \`ccp*\` totals) is \`InvalidDocStructure\`; an"
    echo "  unknown \`--doc-section\` name is a usage error."
    echo "- **Not claimed here:** the section text is decoded CP1252/UTF-16 text — the exact"
    echo "  bytes remain available through the piece spans (and the whole source through the"
    echo "  exact leaf); the header/footer **story** mapping (which header belongs to which"
    echo "  section) and the textbox/header-textbox **anchors** are outside this subset."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the DOC model"
    echo "  and its sections are derived and never on the exactness path (ADR-0060: the"
    echo "  model node depends on the \`sha256(source)\` root)."
    echo "- **Never run on the host:** every command above ran in the pinned \`dev\`"
    echo "  container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" != "PASS" ]; then
    echo "PHASE 27.1.1 DOC SECTION COURT: FAIL — campaign $CAMPAIGN (exact_ok=$exact_ok/$normal_n surface_fail=$surface_fail pieces_ok=$piece_ok/$piece_n sections_ok=$sections_ok/$sections_n boundary_ok=$boundary_ok/$boundary_n empty_ok=$empty_ok/$empty_n usage_ok=$usage_ok/$usage_n declines=$decline_after_detect_ok/$decline_n cfbf_ctl=$cfbf_ctl_ok/$cfbf_ctl_n opaque=$opaque_ok/$opaque_n)" >&2
    exit 1
fi
echo "PHASE 27.1.1 DOC SECTION COURT: PASS — campaign $CAMPAIGN"
