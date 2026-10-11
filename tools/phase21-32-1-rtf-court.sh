#!/bin/sh
# Phase 21.32 — the RTF (Rich Text Format) adapter court.
#
# Pre-registered (Phase-21 subphase 21.32). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture (including the Opaque controls),
#       `materialize(field) == source` (length + SHA-256 + cmp) after the source file
#       AND the standalone descriptor are deleted, in a fresh process. The court FAILS
#       unless this is 100% on every fixture.
#   H2 (surface + preserved representation) — the representation-preserving RTF model is
#       exposed: the **header** (`\rtf` version, charset, `\deffN`); every **group** with
#       its exact brace span, parent, depth, and destination kind; every **control word**
#       (with its numeric parameter) and **control symbol**; every `\'hh` **hex escape**
#       (spelling preserved, decoded under the declared charset) and `\uN` **Unicode
#       escape** (with the active `\ucN` fallback count); every **destination**
#       (`\fonttbl`/`\colortbl`/`\stylesheet`/`\info`, ...) verbatim; every plain **text
#       run** byte-exactly. Native rtf-header/rtf-group/rtf-control/rtf-hex/rtf-unicode/
#       rtf-text/rtf-dest/rtf-find and common metadata/text/search-match answer.
#   H3 (declines + boundaries) — an out-of-range native index, a non-canonical/unknown
#       destination reference, and an unsupported common pair decline typed (rc 6 / rc 2),
#       never a silent empty answer. The boundary controls pin detection: a leading-`{`
#       junk blob, plain prose, an RTF magic with no version digit, and an unbalanced
#       brace all stay `opaque` (a native rtf-header request on the junk blob declines
#       typed, rc 6).
#
# Runs in the pinned `dev` service using only POSIX `sh`, coreutils, git, and the shipped
# binary (no python3, no jq):
#   docker compose run --rm --no-TTY dev sh tools/phase21-32-1-rtf-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
# The `dev` lane is hard-capped on memory; keep the debug-info symbol footprint bounded so
# a full all-features link fits (symbols only — no semantics change). Overridable by the
# caller, and recorded in the receipt.
: "${CARGO_PROFILE_DEV_DEBUG:=0}"
export CARGO_PROFILE_DEV_DEBUG
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="2026-10-10"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-32-1-rtf-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-32-1
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
if ! cargo build --locked --all-features > "$RAW/build.log" 2>&1; then
    tail -40 "$RAW/build.log" >&2
    echo "PHASE 21.32.1 RTF COURT: FAIL — all-features build failed" >&2
    exit 1
fi
tail -2 "$RAW/build.log"

echo "=== generate fixtures (POSIX here-doc + /usr/bin/printf; no python3, no jq) ==="

# --- NORMAL: RTF --------------------------------------------------------------
# doc.rtf exercises every construct the adapter records. It must end exactly at the root
# `}` (detection requires the outermost group to close at end of input), so the command
# substitution strips the here-doc's trailing newline.
DOC=$(cat <<'RTFEOF'
{\rtf1\ansi\deff0
{\fonttbl{\f0\fnil Arial;}{\f1\fnil Times;}}
{\colortbl;\red0\green0\blue0;}
{\stylesheet{\s0 Normal;}}
{\info{\title Doc}{\author Ada}}
Hello \b bold\b0 \fs24 big\fs20 and caf\'e9
\u233? \uc0\u8212
\{x\} \\ \~ \- \_
{grouped text}
}
RTFEOF
)
printf '%s' "$DOC" > "$WORK/corpus/doc.rtf"

# --- CONTROLS (detection boundaries) ----------------------------------------
# A leading-`{` junk blob -> opaque (the Opaque control; a native rtf selector declines).
/usr/bin/printf '%s' '{ not rtf }' > "$WORK/corpus/junk.txt"
# Plain prose -> opaque.
/usr/bin/printf '%s\n' 'This is just prose text.' 'No RTF structure at all.' \
    > "$WORK/corpus/prose.txt"
# A `{\rtfX` header with no version digit -> opaque.
/usr/bin/printf '%s' '{\rtfX junk}' > "$WORK/corpus/rtfx.rtf"
# An unbalanced brace -> opaque.
/usr/bin/printf '%s' '{\rtf1\ansi {x}' > "$WORK/corpus/broken.rtf"

NORMAL="doc.rtf"
CONTROL="junk.txt prose.txt rtfx.rtf broken.rtf"
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
        [ "$fmt" = "rtf" ] || { echo "FINDING: $f detected as $fmt (want rtf)" >&2; surface_fail=$((surface_fail + 1)); }

        # Common metadata / text.
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"format":"rtf"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata format" >&2; surface_fail=$((surface_fail + 1)); }
        grep -q '"fonttbl":1' "$RAW/$f.metadata.json" || { echo "FINDING: $f fonttbl count" >&2; surface_fail=$((surface_fail + 1)); }
        grep -q '"version":1' "$RAW/$f.metadata.json" || { echo "FINDING: $f version" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"text"' "$RAW/$f.text.json" || { echo "FINDING: $f doc-text" >&2; surface_fail=$((surface_fail + 1)); }

        # Native header: Text is the compact canonical string; Metadata carries the spans.
        "$BIN" observe --store "$WORK/store" --field "$field" --rtf-header --kind text \
            > "$RAW/$f.header.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"text":"rtf1 ansi deff0"' "$RAW/$f.header.json" || { echo "FINDING: $f rtf-header text" >&2; surface_fail=$((surface_fail + 1)); }
        "$BIN" observe --store "$WORK/store" --field "$field" --rtf-header --kind metadata \
            > "$RAW/$f.header.meta.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"version":1' "$RAW/$f.header.meta.json" || { echo "FINDING: $f rtf-header version" >&2; surface_fail=$((surface_fail + 1)); }

        # Native group 1 is `\fonttbl`.
        "$BIN" observe --store "$WORK/store" --field "$field" --rtf-group 1 --kind metadata \
            > "$RAW/$f.group1.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"dest":"fonttbl"' "$RAW/$f.group1.json" || { echo "FINDING: $f rtf-group dest" >&2; surface_fail=$((surface_fail + 1)); }

        # Native control 0 is `\rtf1` (ExactBytes keeps the exact spelling).
        "$BIN" observe --store "$WORK/store" --field "$field" --rtf-control 0 --kind exact \
            > "$RAW/$f.control0.bin" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"value_hex":"5c72746631"' "$RAW/$f.control0.bin" || { echo "FINDING: $f rtf-control exact (want \\rtf1)" >&2; surface_fail=$((surface_fail + 1)); }
        "$BIN" observe --store "$WORK/store" --field "$field" --rtf-control 0 --kind text \
            > "$RAW/$f.control0.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"text":"rtf"' "$RAW/$f.control0.json" || { echo "FINDING: $f rtf-control text" >&2; surface_fail=$((surface_fail + 1)); }

        # Native hex 0 is `\'e9` (spelling preserved).
        "$BIN" observe --store "$WORK/store" --field "$field" --rtf-hex 0 --kind exact \
            > "$RAW/$f.hex0.bin" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"value_hex":"5c276539"' "$RAW/$f.hex0.bin" || { echo "FINDING: $f rtf-hex exact (want \\'e9)" >&2; surface_fail=$((surface_fail + 1)); }

        # Native unicode 0 is `\u233` under the default `\uc1` fallback count.
        "$BIN" observe --store "$WORK/store" --field "$field" --rtf-unicode 0 --kind metadata \
            > "$RAW/$f.unicode0.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"code":233' "$RAW/$f.unicode0.json" || { echo "FINDING: $f rtf-unicode code" >&2; surface_fail=$((surface_fail + 1)); }
        grep -q '"uc":1' "$RAW/$f.unicode0.json" || { echo "FINDING: $f rtf-unicode uc" >&2; surface_fail=$((surface_fail + 1)); }

        # Native destination `fonttbl` (ExactBytes is the whole group, braces included).
        "$BIN" observe --store "$WORK/store" --field "$field" --rtf-dest fonttbl --kind exact \
            > "$RAW/$f.dest.bin" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"value_hex":"7b5c666f6e7474626c' "$RAW/$f.dest.bin" || { echo "FINDING: $f rtf-dest exact (want {\\fonttbl...)" >&2; surface_fail=$((surface_fail + 1)); }

        # Native find and common search-match.
        "$BIN" observe --store "$WORK/store" --field "$field" --rtf-find bold --kind text \
            > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"matches":\[' "$RAW/$f.find.json" || { echo "FINDING: $f rtf-find" >&2; surface_fail=$((surface_fail + 1)); }
        "$BIN" observe --store "$WORK/store" --field "$field" --text Hello --kind text \
            > "$RAW/$f.common.search.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"matches":\[' "$RAW/$f.common.search.json" || { echo "FINDING: $f common search-match" >&2; surface_fail=$((surface_fail + 1)); }

        # H3: out-of-range indices, a bad destination reference, and an unsupported common
        # pair decline typed (rc 6), and a non-canonical reference is a usage error (rc 2).
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --rtf-group 9999 --kind metadata > /dev/null 2>&1
        d_rc1=$?
        "$BIN" observe --store "$WORK/store" --field "$field" --rtf-control 9999 --kind metadata > /dev/null 2>&1
        d_rc2=$?
        "$BIN" observe --store "$WORK/store" --field "$field" --rtf-hex 9999 --kind metadata > /dev/null 2>&1
        d_rc3=$?
        "$BIN" observe --store "$WORK/store" --field "$field" --rtf-unicode 9999 --kind metadata > /dev/null 2>&1
        d_rc4=$?
        "$BIN" observe --store "$WORK/store" --field "$field" --rtf-text 9999 --kind metadata > /dev/null 2>&1
        d_rc5=$?
        "$BIN" observe --store "$WORK/store" --field "$field" --rtf-dest noSuch --kind text > /dev/null 2>&1
        d_rc6=$?
        "$BIN" observe --store "$WORK/store" --field "$field" --rtf-dest fonttbl:01 --kind text > /dev/null 2>&1
        d_rc7=$?
        "$BIN" observe --store "$WORK/store" --field "$field" --table 0 --kind text > /dev/null 2>&1
        d_rc8=$?
        set -e
        if [ "$d_rc1" -eq 6 ] && [ "$d_rc2" -eq 6 ] && [ "$d_rc3" -eq 6 ] \
            && [ "$d_rc4" -eq 6 ] && [ "$d_rc5" -eq 6 ] && [ "$d_rc6" -eq 2 ] \
            && [ "$d_rc7" -eq 2 ] && [ "$d_rc8" -eq 6 ]; then
            decline_ok=$((decline_ok + 1))
        else
            echo "FINDING: typed declines rc=$d_rc1/$d_rc2/$d_rc3/$d_rc4/$d_rc5/$d_rc6/$d_rc7/$d_rc8 (want 6/6/6/6/6/2/2/6)" >&2
        fi
    else
        control_n=$((control_n + 1))
        case "$f" in
            junk.txt)
                [ "$fmt" = "opaque" ] || { echo "FINDING: $f detected as $fmt (want opaque)" >&2; surface_fail=$((surface_fail + 1)); }
                set +e
                "$BIN" observe --store "$WORK/store" --field "$field" --rtf-header --kind metadata \
                    > "$RAW/$f.rtf-decline.json" 2>&1
                control_rc=$?
                set -e
                [ "$control_rc" -eq 6 ] && opaque_ok=$((opaque_ok + 1)) || { echo "FINDING: opaque $f rtf-header rc=$control_rc (want 6)" >&2; }
                boundary_ok=$((boundary_ok + 1)) ;;
            prose.txt)
                [ "$fmt" = "opaque" ] || { echo "FINDING: $f detected as $fmt (want opaque)" >&2; surface_fail=$((surface_fail + 1)); }
                boundary_ok=$((boundary_ok + 1)) ;;
            rtfx.rtf)
                [ "$fmt" = "opaque" ] || { echo "FINDING: $f detected as $fmt (want opaque)" >&2; surface_fail=$((surface_fail + 1)); }
                boundary_ok=$((boundary_ok + 1)) ;;
            broken.rtf)
                [ "$fmt" = "opaque" ] || { echo "FINDING: $f detected as $fmt (want opaque)" >&2; surface_fail=$((surface_fail + 1)); }
                boundary_ok=$((boundary_ok + 1)) ;;
        esac
    fi

    # H1: delete the source AND the standalone descriptor, then rematerialize exactly in a
    # fresh process and compare (length + SHA-256 + cmp).
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
    echo "# Phase 21.32.1 RTF court — matrix"
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
    echo "rtf_fixtures $normal_n"
    echo "control_fixtures $control_n"
    echo "exact_ok $exact_ok"
    echo "exact_fail $exact_fail"
    echo "surface_fail $surface_fail"
    echo "boundary_ok $boundary_ok"
    echo "opaque_control_ok $opaque_ok"
    echo "typed_declines_ok $decline_ok"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.32.1 — RTF surface + exact closure + model",
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
  "fixtures_tool": "POSIX shell here-doc + GNU /usr/bin/printf (no python3, no jq); generated in-court, never committed",
  "env_affecting_semantics": {"LC_ALL": "C", "CARGO_PROFILE_DEV_DEBUG": "$CARGO_PROFILE_DEV_DEBUG"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
if [ "$exact_fail" -ne 0 ] || [ "$surface_fail" -ne 0 ] \
    || [ "$boundary_ok" -ne 4 ] || [ "$opaque_ok" -ne 1 ] \
    || [ "$decline_ok" -ne 1 ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.32.1 — RTF surface + exact closure + model",
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
  "fixture_tool": "POSIX sh (here-doc + /usr/bin/printf), in-court; not committed",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion, for every fixture including the Opaque controls",
  "rtf": "a bounded, span-preserving RTF adapter: the header (the exact \\\\rtf version control word and its numeric version, the charset control word, and \\\\deffN); every group '{ ... }' with its exact brace span (both braces included), parent index, and nesting depth (the outermost group is depth 1); every control word (\\\\word with its optional numeric parameter, e.g. \\\\fs24) and control symbol (\\\\\\\\, \\\\{, \\\\}, \\\\~, \\\\-, \\\\_, \\\\*) with the exact spelling, name/parameter spans, and whether a terminating delimiter space followed; every \\\\'hh hex escape (the spelling preserved with the decoded byte under the declared charset) and \\\\uN Unicode escape (with the active \\\\ucN fallback count — the literal form and the fallback bytes are preserved, never dropped); every destination (\\\\* ignore, \\\\fonttbl, \\\\colortbl, \\\\stylesheet, \\\\info, \\\\pict, \\\\object) verbatim with exact spans; and every plain text run byte-exactly (RTF uses a space to terminate a control word, so no whitespace is ever normalized). Nothing is rendered, normalized, or re-serialized",
  "observations": ["metadata", "text", "search-match", "native rtf-header", "native rtf-group", "native rtf-control", "native rtf-hex", "native rtf-unicode", "native rtf-text", "native rtf-dest", "native rtf-find"],
  "declines": "an out-of-range group/control/hex/unicode/text index and an unsupported common pair decline typed (rc 6); an unknown destination name and a non-canonical ordinal are usage errors (rc 2); a native rtf selector on a non-RTF document declines typed (rc 6), never a panic",
  "detection_boundary": "RTF detection is content-only (NEVER a file name) and strong-magic based: the source must begin (after an optional UTF-8 BOM) with '{' '\\\\' 'r' 't' 'f' followed by an ASCII DIGIT — the header magic '{<backslash>rtf<digit>' — AND its group braces must balance under a bounded scan that respects the '\\\\\\\\'/'{'/'}' control symbols, the \\\\'hh hex spelling, control words (with their optional numeric parameter), and \\\\binN raw-binary data, with the outermost group closing exactly at end of input",
  "cannot_distinguish": "detection is a magic + brace-balance test, not a full-grammar test: a byte string that merely begins with '{<backslash>rtf<digit>' and has balanced braces is admitted as RTF even if its control words are otherwise meaningless, and the derived model may then decline typed. A byte string with no '{<backslash>rtf<digit>' magic stays opaque even if it is otherwise RTF-shaped (the magic is required). The adapter does not render (no final text layout), does not resolve fonttbl/stylesheet/info semantics, does not execute any control word, and does not map the mac/pc/pca code pages beyond reporting the raw decoded byte (only the ansi 0x80-0x9F window is remapped to Windows-1252). A leading-'{' junk blob, plain prose, a '{<backslash>rtfX' header with no version digit, and an unbalanced brace all stay opaque",
  "precedence": "RTF detection runs with the strong-magic detectors, AFTER the binary magics (PDF/ZIP/Parquet/Arrow) and the bounded JSON sub-detectors (GeoJSON/notebook/package-metadata/API-spec/bibliography) and BEFORE the generic JSON detector; an RTF source begins with '{', which is not a valid JSON document, not a config KEY=VALUE line, and not a Markdown/RST structural mark",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 48 invalid-rtf-structure",
  "adr_0060": "the RtfModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase21-32-1-rtf-court.sh
# inside the court:
#   cargo build --locked --all-features
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--text PAT|--rtf-header|--rtf-group N|--rtf-control N|--rtf-hex N|--rtf-unicode N|--rtf-text N|--rtf-dest NAME[:N]|--rtf-find PAT|--table N) --kind metadata|text|exact
#   rm source F.voldoc
#   target/debug/vole-document materialize --store STORE --field HEX --exact --output OUT ; cmp OUT source
# Regression controls (rtf must not break the shared detectors):
#   docker compose run --rm --no-TTY dev sh tools/phase21-5-1-json-court.sh
#   docker compose run --rm --no-TTY dev sh tools/phase21-31-1-bib-court.sh
# Full gate (dev service):
#   cargo fmt --all --check
#   cargo clippy --all-targets --all-features -- -D warnings
#   cargo test --locked --all-features
#   cargo test --locked
#   MSRV: docker compose run --rm --no-TTY msrv cargo build --locked --all-features
EOF

# --- SUMMARY.md -------------------------------------------------------------
{
    echo "# Phase 21.32.1 — RTF adapter court"
    echo
    echo "**Question.** Does an RTF document close exactly and expose a"
    echo "representation-preserving model (the header, each group's exact brace span and"
    echo "depth, each control word/symbol, each \`\\'hh\` hex escape and \`\\uN\` Unicode"
    echo "escape with its \`\\ucN\` fallback count, each destination verbatim, and each plain"
    echo "text run byte-exactly) on top of the whole-source exact leaf, while keeping a"
    echo "conservative content-only detection boundary (RTF only on the strong"
    echo "\`{\\rtf<digit>\` magic plus balanced braces; a leading-\`{\` junk blob and prose"
    echo "stay \`opaque\`)?"
    echo
    echo "**Method.** Each self-authored fixture is generated in-court with a POSIX"
    echo "here-doc and GNU \`/usr/bin/printf\`, ingested via \`field-build\`, observed, then —"
    echo "after the **source file and the standalone descriptor are deleted** —"
    echo "rematerialized exactly in a fresh process and compared with \`length + SHA-256 +"
    echo "cmp\`. An out-of-range index, a bad destination reference, and an unsupported"
    echo "common pair are required to decline typed; the leading-\`{\` junk blob, prose, a"
    echo "\`{\\rtfX\` header with no version digit, and an unbalanced brace pin the detection"
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
    echo "- **Shipped here:** content-only, conservative RTF detection (the strong"
    echo "  \`{\\rtf<digit>\` magic plus a bounded brace-balance scan); the bounded,"
    echo "  span-preserving header/group/control/hex/unicode/text/destination model; native"
    echo "  \`rtf-header\`/\`rtf-group\`/\`rtf-control\`/\`rtf-hex\`/\`rtf-unicode\`/\`rtf-text\`/"
    echo "  \`rtf-dest\`/\`rtf-find\`; common \`metadata\`/\`text\`/\`search-match\`."
    echo "- **Precedence:** RTF detection runs after the binary magics and the bounded JSON"
    echo "  sub-detectors and before the generic JSON detector, so a \`{\`-leading document is"
    echo "  never stolen by JSON."
    echo "- **Boundary (honest):** detection is a magic + brace-balance test, not a"
    echo "  full-grammar test; a blob that begins with \`{\\rtf<digit>\` and has balanced"
    echo "  braces is admitted as RTF even if its control words are meaningless. The adapter"
    echo "  does not render, does not resolve font/stylesheet/info semantics, and does not"
    echo "  map the \`\\mac\`/\`\\pc\`/\`\\pca\` code pages beyond reporting the raw decoded byte."
} > "$CAMPAIGN/SUMMARY.md"

echo "=== counts ==="
cat "$CAMPAIGN/counts.txt"

if [ "$verdict" != "PASS" ]; then
    echo "PHASE 21.32.1 RTF COURT: FAIL — see $CAMPAIGN" >&2
    exit 1
fi
echo "PHASE 21.32.1 RTF COURT: PASS — receipt at $CAMPAIGN"
