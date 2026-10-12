#!/bin/sh
# Phase 27.2 — legacy PowerPoint 97–2003 (`.ppt`, MS-PPT) `SlideListWithText` depth:
# materialize the master / notes / handout text lists as separate typed answers.
#
# Pre-registered (Phase-27 subphase 27.2.1). Hypotheses:
#
#   H1 (byte-exactness) — for every normal fixture, `materialize(field) == source`
#       (length + SHA-256 + cmp) after the source file AND the standalone descriptor
#       are deleted, in a fresh process. The court FAILS unless this is 100% on every
#       normal fixture. The widened model must not touch the exactness path.
#   H2 (surface + preserved representation) — every `SlideListWithText` container
#       (`recType 4080`) is materialized as its own typed answer, distinguished by
#       `recInstance`: `slides` (0), `masters` (1), `notes` (2), `handout` (3). For each
#       list the answer carries the exact container span, the ordered `SlidePersistAtom`
#       entries (`persistIdRef`, `cTexts`, exact span), and for each entry the ordered
#       text runs (`TextCharsAtom` UTF-16LE / `TextBytesAtom` CP1252) with their exact
#       atom/header spans and decoded text, plus native `ppt-document`/`ppt-slide`/
#       `ppt-run`/`ppt-record`/`ppt-list`/`ppt-find` and common `metadata`/`text`/
#       `search-match`. The slides-list answers (`ppt-slide`/`ppt-run`/`ppt-record`) are
#       unchanged. An absent or empty list is a typed **empty** answer (H2 includes the
#       empty-list case).
#   H3 (declines + boundaries) — a `.ppt` with a plausible `Document` record but a
#       malformed body declines typed (rc 54); **including a malformed auxiliary list**
#       (an odd-length `TextCharsAtom` in a notes list) that the previous subphase
#       silently skipped and now declines. A generic compound file, and a
#       `PowerPoint Document` stream whose first record is not a `Document`, stay
#       `cfbf` and a `ppt` selector declines typed (rc 6); prose stays `opaque`
#       (rc 6). Never a panic.
#
# Dependency-free (the fixtures are written by a small shipped Rust writer;
# deliberately NO python3, NO PowerPoint/LibreOffice, NO external OLE tooling, NO
# network):
#   docker compose run --rm --no-TTY dev sh tools/phase27-2-1-ppt-lists-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase27-2-1-ppt-lists-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase27-2-1
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== generate fixtures (dependency-free .ppt writer) ==="
cargo run --locked --all-features --example ppt_fixtures -- "$WORK/corpus" 2>&1 | tail -1

# Fixture classes.
#   NORMAL   — valid `.ppt`: H1 exactness + H2 surface (slides + masters + notes +
#              handout lists).
#   DECLINE  — detects as `ppt` but the model is structurally malformed (rc 54).
#   CFBF_CTL — a valid compound file that is NOT a PowerPoint document (format `cfbf`);
#              a ppt selector declines typed (rc 6).
#   OPAQUE   — not a compound file at all; a ppt selector declines typed (rc 6).
NORMAL="ppt_valid.ppt ppt_lists.ppt"
DECLINE="ppt_bad_document.ppt ppt_overrun.ppt ppt_text_mismatch.ppt ppt_truncated.ppt ppt_bad_current_user.ppt ppt_bad_aux_list.ppt"
CFBF_CTL="generic_cfbf.bin ppt_bad_first_record.ppt"
OPAQUE="prose.txt"

is_in() {
    for x in $2; do [ "$x" = "$1" ] && return 0; done
    return 1
}
# Extract one JSON string field from a single-line JSON object (greedy, no jq).
json_str() {
    printf '%s' "$1" | sed -n "s/.*\"$2\":\"\([^\"]*\)\".*/\1/p"
}
# The byte offset of the first occurrence of a literal in a file (GNU grep -bo).
byte_at() {
    grep -bo "$2" "$1" | head -1 | cut -d: -f1
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
run_ok=0
run_n=0
list_ok=0
list_n=0
decline_after_detect_ok=0
decline_n=0
cfbf_ctl_ok=0
cfbf_ctl_n=0

for f in $DECLINE; do
    src="$WORK/ref/$f"
    cp "$WORK/corpus/$f" "$src"
    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    fmt="$(json_str "$build_json" format)"
    field="$(json_str "$build_json" field)"
    decline_n=$((decline_n + 1))
    [ "$fmt" = "ppt" ] || { echo "FINDING: $f detected as $fmt (want ppt)" >&2; surface_fail=$((surface_fail + 1)); }
    set +e
    "$BIN" observe --store "$WORK/store" --field "$field" --ppt-document --kind text \
        > "$RAW/$f.decline.txt" 2>&1
    ctl_rc=$?
    set -e
    if [ "$ctl_rc" -eq 54 ]; then decline_after_detect_ok=$((decline_after_detect_ok + 1)); else echo "FINDING: $f structural decline rc=$ctl_rc (want 54)" >&2; fi
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
    "$BIN" observe --store "$WORK/store" --field "$field" --ppt-document --kind text \
        > "$RAW/$f.decline.txt" 2>&1
    ctl_rc=$?
    set -e
    if [ "$ctl_rc" -eq 6 ]; then cfbf_ctl_ok=$((cfbf_ctl_ok + 1)); else echo "FINDING: CFBF control $f ppt selector rc=$ctl_rc (want 6)" >&2; fi
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
    "$BIN" observe --store "$WORK/store" --field "$field" --ppt-document --kind text \
        > "$RAW/$f.decline.txt" 2>&1
    ctl_rc=$?
    set -e
    if [ "$ctl_rc" -eq 6 ]; then opaque_ok=$((opaque_ok + 1)); else echo "FINDING: opaque control $f ppt selector rc=$ctl_rc (want 6)" >&2; fi
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
    [ "$fmt" = "ppt" ] || { echo "FINDING: $f not detected as ppt (fmt=$fmt)" >&2; surface_fail=$((surface_fail + 1)); }

    # H2: common metadata / text / search-match + native document / slide / run /
    # record / find / list (all four instances).
    "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
        > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --ppt-document --kind text \
        > "$RAW/$f.document.txt" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --ppt-slide 0 --kind metadata \
        > "$RAW/$f.slide0.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --ppt-slide 1 --kind text \
        > "$RAW/$f.slide1.txt" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --ppt-run 0 --kind metadata \
        > "$RAW/$f.run0.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --ppt-run 1 --kind metadata \
        > "$RAW/$f.run1.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --ppt-record 0 --kind metadata \
        > "$RAW/$f.record0.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --ppt-record 0 --kind exact \
        > "$RAW/$f.record0.bin" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --ppt-find "Hello" --kind metadata \
        > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --ppt-text --kind text \
        > "$RAW/$f.text.txt" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --ppt-list slides --kind metadata \
        > "$RAW/$f.list_slides.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --ppt-list masters --kind metadata \
        > "$RAW/$f.list_masters.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --ppt-list notes --kind metadata \
        > "$RAW/$f.list_notes.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --ppt-list handout --kind metadata \
        > "$RAW/$f.list_handout.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --ppt-list masters --kind text \
        > "$RAW/$f.list_masters.txt" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --ppt-list notes --kind text \
        > "$RAW/$f.list_notes.txt" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --ppt-list handout --kind text \
        > "$RAW/$f.list_handout.txt" 2>&1 || surface_fail=$((surface_fail + 1))

    grep -q '"format":"ppt"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata missing format" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"stream":"PowerPoint Document"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata stream != PowerPoint Document" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"slides":2' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata slide count != 2" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"runs":2' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata run count != 2" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"chars":1' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata char-run count != 1" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"bytes":1' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata byte-run count != 1" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"text":"powerpoint97-2003"' "$RAW/$f.document.txt" || { echo "FINDING: $f document text != powerpoint97-2003" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"persist_id_ref":1' "$RAW/$f.slide0.json" || { echo "FINDING: $f slide0 persist_id_ref != 1" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"list":"slides"' "$RAW/$f.slide0.json" || { echo "FINDING: $f slide0 list != slides" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"kind":"chars"' "$RAW/$f.run0.json" || { echo "FINDING: $f run0 not a chars run" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"kind":"bytes"' "$RAW/$f.run1.json" || { echo "FINDING: $f run1 not a bytes run" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"id_hex":"0x03E8"' "$RAW/$f.record0.json" || { echo "FINDING: $f record0 is not the Document record" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"type":"document"' "$RAW/$f.record0.json" || { echo "FINDING: $f record0 type != document" >&2; surface_fail=$((surface_fail + 1)); }
    # The Document record's exact bytes begin `0F 00 E8 03` (recVer 0xF, recType 1000).
    if grep -q '0f00e803' "$RAW/$f.record0.bin"; then
        :
    else
        echo "FINDING: $f record0 exact bytes are not the Document header" >&2
        surface_fail=$((surface_fail + 1))
    fi
    # The slides-only fixture's Document payload is exactly 124 bytes (record 132); the
    # four-list fixture's Document is larger (its extra list containers).
    if [ "$f" = "ppt_valid.ppt" ]; then
        grep -q '"bytes_len":132' "$RAW/$f.record0.bin" || { echo "FINDING: $f record0 bytes_len != 132" >&2; surface_fail=$((surface_fail + 1)); }
    fi
    grep -q '"matches":\[{' "$RAW/$f.find.json" || { echo "FINDING: $f find returned no matches" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q 'Hello' "$RAW/$f.text.txt" || { echo "FINDING: $f text missing the UTF-16LE run" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q 'Caf' "$RAW/$f.text.txt" || { echo "FINDING: $f text missing the CP1252 run" >&2; surface_fail=$((surface_fail + 1)); }

    # H2: the slides list (recInstance 0) is unchanged and non-empty.
    grep -q '"list":"slides"' "$RAW/$f.list_slides.json" || { echo "FINDING: $f list_slides name != slides" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"rec_instance":0' "$RAW/$f.list_slides.json" || { echo "FINDING: $f list_slides rec_instance != 0" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"empty":false' "$RAW/$f.list_slides.json" || { echo "FINDING: $f list_slides empty != false" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"persist_id_ref":1' "$RAW/$f.list_slides.json" || { echo "FINDING: $f list_slides missing persist 1" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"persist_id_ref":2' "$RAW/$f.list_slides.json" || { echo "FINDING: $f list_slides missing persist 2" >&2; surface_fail=$((surface_fail + 1)); }

    # H2: the masters list (recInstance 1) is modeled.
    grep -q '"list":"masters"' "$RAW/$f.list_masters.json" || { echo "FINDING: $f list_masters name != masters" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"rec_instance":1' "$RAW/$f.list_masters.json" || { echo "FINDING: $f list_masters rec_instance != 1" >&2; surface_fail=$((surface_fail + 1)); }

    # H2: each run decodes to its exact text (the CP1252 high byte maps to U+20AC).
    run_n=$((run_n + 1))
    if grep -q 'Hello Ω' "$RAW/$f.run0.json" && grep -q 'Café €' "$RAW/$f.run1.json"; then
        run_ok=$((run_ok + 1))
    else
        echo "FINDING: $f run text did not decode as expected" >&2
        surface_fail=$((surface_fail + 1))
    fi

    if [ "$f" = "ppt_valid.ppt" ]; then
        # A slides-only file: the masters/notes/handout lists are **absent** → typed
        # empty answers (never an error), and the metadata counts stay at the Phase-26.3
        # values.
        list_n=$((list_n + 1))
        if grep -q '"empty":true' "$RAW/$f.list_masters.json" \
            && grep -q '"empty":true' "$RAW/$f.list_notes.json" \
            && grep -q '"empty":true' "$RAW/$f.list_handout.json" \
            && grep -q '"slides":0' "$RAW/$f.list_masters.json" \
            && grep -q '"list":"handout"' "$RAW/$f.list_handout.json"; then
            list_ok=$((list_ok + 1))
        else
            echo "FINDING: $f absent aux lists are not typed empty answers" >&2
            surface_fail=$((surface_fail + 1))
        fi
        grep -q '"lists":1' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata lists != 1" >&2; surface_fail=$((surface_fail + 1)); }
    fi

    if [ "$f" = "ppt_lists.ppt" ]; then
        # All four lists are modeled with their own slides, runs, and decoded text.
        list_n=$((list_n + 1))
        grep -q '"lists":4' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata lists != 4" >&2; surface_fail=$((surface_fail + 1)); }
        notes_ok=true
        grep -q '"persist_id_ref":3' "$RAW/$f.list_masters.json" || notes_ok=false
        grep -q 'Master Title' "$RAW/$f.list_masters.txt" || notes_ok=false
        grep -q '"list":"notes"' "$RAW/$f.list_notes.json" || notes_ok=false
        grep -q '"persist_id_ref":4' "$RAW/$f.list_notes.json" || notes_ok=false
        grep -q '"kind":"chars"' "$RAW/$f.list_notes.json" || notes_ok=false
        grep -q '"kind":"bytes"' "$RAW/$f.list_notes.json" || notes_ok=false
        grep -q 'Speaker notes' "$RAW/$f.list_notes.txt" || notes_ok=false
        grep -q 'Pöints' "$RAW/$f.list_notes.txt" || notes_ok=false
        # Run order within the notes list: the chars run precedes the bytes run.
        co="$(byte_at "$RAW/$f.list_notes.json" 'Speaker notes')"
        bo="$(byte_at "$RAW/$f.list_notes.json" 'Pöints')"
        [ -n "$co" ] && [ -n "$bo" ] && [ "$co" -lt "$bo" ] || notes_ok=false
        grep -q '"persist_id_ref":5' "$RAW/$f.list_handout.json" || notes_ok=false
        grep -q 'Handout footer' "$RAW/$f.list_handout.txt" || notes_ok=false
        if [ "$notes_ok" = true ]; then
            list_ok=$((list_ok + 1))
        else
            echo "FINDING: $f aux-list answers are incomplete or out of order" >&2
            surface_fail=$((surface_fail + 1))
        fi
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

echo "=== exactness after source + descriptor deletion ==="
cat "$TSV"

fixtures_n=$((normal_n + opaque_n + cfbf_ctl_n))

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 27.2.1 legacy PowerPoint (.ppt) SlideListWithText court — matrix"
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
    echo "normal $normal_n"
    echo "exact_ok $exact_ok"
    echo "exact_fail $exact_fail"
    echo "surface_fail $surface_fail"
    echo "runs_ok $run_ok"
    echo "runs_n $run_n"
    echo "list_ok $list_ok"
    echo "list_n $list_n"
    echo "structural_declines_ok $decline_after_detect_ok"
    echo "structural_declines $decline_n"
    echo "cfbf_controls_ok $cfbf_ctl_ok"
    echo "cfbf_controls $cfbf_ctl_n"
    echo "opaque_controls_ok $opaque_ok"
    echo "opaque_controls $opaque_n"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "27.2.1 — legacy PowerPoint 97–2003 (.ppt, MS-PPT) SlideListWithText depth (slides + masters + notes + handout)",
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
  "fixtures_tool": "examples/ppt_fixtures.rs (dependency-free .ppt/MS-PPT-in-CFBF writer; no python3, no PowerPoint/LibreOffice, no external OLE tooling)",
  "external_oracles": "none (the PPT model is validated internally; no external PowerPoint parser is invoked)",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
if [ "$exact_fail" -ne 0 ] || [ "$surface_fail" -ne 0 ] \
    || [ "$exact_ok" -ne "$normal_n" ] \
    || [ "$run_ok" -ne "$run_n" ] \
    || [ "$list_ok" -ne "$list_n" ] \
    || [ "$decline_after_detect_ok" -ne "$decline_n" ] \
    || [ "$cfbf_ctl_ok" -ne "$cfbf_ctl_n" ] \
    || [ "$opaque_ok" -ne "$opaque_n" ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "27.2.1 — legacy PowerPoint 97–2003 (.ppt, MS-PPT) SlideListWithText depth (slides + masters + notes + handout)",
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
  "fixture_tool": "examples/ppt_fixtures.rs",
  "external_oracles": "none",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion",
  "observations": ["metadata", "text", "search-match", "native ppt-document", "native ppt-slide", "native ppt-run", "native ppt-record", "native ppt-list (slides|masters|notes|handout)", "native ppt-find"],
  "surface": "every SlideListWithText container (recType 4080), distinguished by recInstance — slides (0), masters (1), notes (2), handout (3) — is materialized as its own typed answer: the exact container span, the ordered SlidePersistAtom entries (persistIdRef, cTexts, exact span), and for each entry the ordered text runs (TextCharsAtom UTF-16LE / TextBytesAtom CP1252) with their exact atom/header spans and decoded text. The slides-list projection (ppt-slide/ppt-run/ppt-record, canonical text) is byte-for-byte unchanged",
  "empty_lists": "an absent SlideListWithText instance and a present-but-empty list are typed empty answers (slides 0, runs 0, empty true), never errors; the list's container span is still reported when the container is present",
  "declines": "a .ppt with a plausible Document record but a malformed body (a bad Document container length, an overrunning recLen, an odd-length TextCharsAtom, a truncated record header, an out-of-range Current User offset, and — newly modeled — an odd-length TextCharsAtom inside a notes list) detects as ppt and its model declines typed InvalidPptStructure (rc 54); a generic compound file and a PowerPoint Document stream whose first record is not a Document stay cfbf and a ppt selector declines typed (rc 6); prose stays opaque (rc 6); never a panic",
  "precedence": "the PPT detector is built on top of the CFBF detector and runs after the Word and Excel detectors and before the generic Cfbf claim, so a .ppt becomes Ppt while a generic compound file stays Cfbf; a non-CFBF source (including a .pptx OPC ZIP) is unaffected",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 54 invalid-ppt-structure",
  "record_framing": "the MS-PPT RecordHeader is 8 bytes: w=u16 little-endian (recVer = w & 0x0F, recInstance = (w>>4) & 0x0FFF), recType=u16, recLen=u32; recVer 0xF marks a container. The Document record (recType 1000, recVer 0xF) opens with 0F 00 E8 03",
  "model_version": "the derived PptModel wire version is 2 (the SlideListWithText list arena is now the source of truth; the slides/runs projection is derived from it). The model is derived, never on the exactness path",
  "adr_0060": "the PptModel node depends on the exact source root (DocumentExact, keyed by sha256(source)); no source-reading node aliases another field's source. The ppt-list answer reads through the same model node and exact root",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase27-2-1-ppt-lists-court.sh
# inside the court:
#   cargo build --locked --all-features
#   cargo run --locked --all-features --example ppt_fixtures -- CORPUS
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX
#       (--metadata|--ppt-text|--ppt-document|--ppt-slide 0|--ppt-slide 1|--ppt-run 0|--ppt-run 1|--ppt-record 0|--ppt-list slides|--ppt-list masters|--ppt-list notes|--ppt-list handout|--ppt-find PAT)
#       --kind metadata|text|exact
#   rm source F.voldoc
#   target/debug/vole-document materialize --store STORE --field HEX --exact --output OUT ; cmp OUT source
# Full gate (dev service):
#   cargo fmt --all --check
#   cargo clippy --all-targets --all-features -- -D warnings
#   cargo test --locked --all-features   (use CARGO_PROFILE_TEST_DEBUG=0 if the all-features link OOM-kills)
#   cargo test --locked --features ppt
#   PPT regression control: sh tools/phase26-3-1-ppt-court.sh
#   DOC regression control: sh tools/phase26-1-1-doc-court.sh
#   CFBF regression control: sh tools/phase21-35-1-cfbf-court.sh
#   MSRV: docker compose run --rm --no-TTY msrv cargo build --locked --all-features
EOF

# --- SUMMARY.md -------------------------------------------------------------
{
    echo "# Phase 27.2.1 — legacy PowerPoint 97–2003 (\`.ppt\`, MS-PPT) list court"
    echo
    echo "**Question.** Does every \`SlideListWithText\` container — the **slides** (0),"
    echo "**masters** (1), **notes** (2), and **handout** (3) lists — close exactly and"
    echo "materialize as its own typed, span-preserving answer (the exact container span, the"
    echo "ordered \`SlidePersistAtom\` entries, and for each the ordered \`TextCharsAtom\`"
    echo "(UTF-16LE) / \`TextBytesAtom\` (CP1252) runs with their exact spans and decoded"
    echo "text), while the slides-list answers stay byte-for-byte unchanged and malformed"
    echo "inputs (including a malformed auxiliary list) decline typed?"
    echo
    echo "**Method.** Each fixture is generated by the dependency-free"
    echo "\`examples/ppt_fixtures.rs\` writer, ingested via \`field-build\`, observed, then —"
    echo "after the **source file and the standalone descriptor are deleted** —"
    echo "rematerialized exactly in a fresh process and compared with \`length + SHA-256 +"
    echo "cmp\`. The CFBF, Opaque, and structural controls pin the detection boundaries. The"
    echo "court runs in the pinned \`dev\` service using only POSIX \`sh\`, coreutils, git, and"
    echo "the shipped binary (no python3, no PowerPoint tooling, no network)."
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
    ls "$RAW" | sed 's/^/  - /'
    echo
    echo "## Scope (honest)"
    echo
    echo "- **Supported:** the \`SlideListWithText\` lists \`slides\`/\`masters\`/\`notes\`/"
    echo "  \`handout\` (by \`recInstance\`) with their ordered \`SlidePersistAtom\` entries and"
    echo "  text runs, each with its exact span and decoded text; the exact container span;"
    echo "  native \`ppt-list\`; the unchanged \`ppt-slide\`/\`ppt-run\`/\`ppt-record\`/\`ppt-find\`;"
    echo "  common \`metadata\`/\`text\`/\`search-match\`."
    echo "- **Empty:** an absent list and a present-but-empty list are typed empty answers"
    echo "  (\`slides\`:0, \`runs\`:0, \`empty\`:true), never errors."
    echo "- **Boundary:** a generic compound file, and a \`PowerPoint Document\` stream whose"
    echo "  first record is not a \`Document\`, stay \`Cfbf\`; prose stays \`Opaque\`; a \`.pptx\`"
    echo "  (an OPC ZIP) never reaches the CFBF detector at all; a \`.ppt\` with a plausible"
    echo "  \`Document\` record but a malformed body is admitted and its model declines typed."
    echo "- **Declines:** a malformed body — including a malformed **auxiliary** list"
    echo "  (an odd-length \`TextCharsAtom\` in a notes list), which the previous subphase"
    echo "  silently skipped — is a typed decline (\`InvalidPptStructure\`)."
    echo "- **Not claimed here:** the \`Current User\` stream's *content* beyond the"
    echo "  \`UserEditAtom\` offset, embedded OLE objects, pictures, and the other legacy CFBF"
    echo "  semantic models; the economic court (separate script)."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the PPT model"
    echo "  is derived and never on the exactness path (ADR-0060: the model node depends on"
    echo "  the \`sha256(source)\` root)."
    echo "- **Never run on the host:** every command above ran in the pinned \`dev\`"
    echo "  container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" != "PASS" ]; then
    echo "PHASE 27.2.1 PPT LISTS COURT: FAIL — campaign $CAMPAIGN (exact_ok=$exact_ok/$normal_n surface_fail=$surface_fail runs_ok=$run_ok/$run_n lists_ok=$list_ok/$list_n declines=$decline_after_detect_ok/$decline_n cfbf_ctl=$cfbf_ctl_ok/$cfbf_ctl_n opaque=$opaque_ok/$opaque_n)" >&2
    exit 1
fi
echo "PHASE 27.2.1 PPT LISTS COURT: PASS — campaign $CAMPAIGN"
