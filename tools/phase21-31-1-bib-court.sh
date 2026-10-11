#!/bin/sh
# Phase 21.31 — the bibliography adapter court (BibTeX / RIS / CSL-JSON).
#
# Pre-registered (Phase-21 subphase 21.31). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture (including the plain-JSON-array and Opaque
#       controls), `materialize(field) == source` (length + SHA-256 + cmp) after the
#       source file AND the standalone descriptor are deleted, in a fresh process. The
#       court FAILS unless this is 100% on every fixture.
#   H2 (surface + preserved representation) — the representation-preserving bibliography
#       model is exposed for all three dialects: the **recorded dialect** (`bibtex` /
#       `ris` / `csl_json`); every entry/item (BibTeX entry key and entry type *as
#       written*; RIS `TY`; CSL-JSON item `id`); every field with its exact value span
#       (BibTeX brace/quoted/number/macro forms kept distinct and duplicate fields
#       reported; the RIS two-space `XX  - ` separator and a multi-line continuation;
#       CSL-JSON member tokens). Native bib-dialect/bib-entry/bib-field/bib-key/bib-find
#       and common metadata/text/search-match answer.
#   H3 (declines + boundaries) — an out-of-range entry, an absent field/key, and an
#       unsupported common pair decline typed (rc 6), never a silent empty answer. The
#       boundary controls pin detection: a plain JSON array of non-CSL objects stays
#       `json` (a native bib selector on it declines typed, rc 6), and plain prose stays
#       `opaque` (a common metadata request on it declines typed, rc 6).
#
# Runs in the pinned `dev` service using only POSIX `sh`, coreutils, git, and the
# shipped binary (no python3, no jq):
#   docker compose run --rm --no-TTY dev sh tools/phase21-31-1-bib-court.sh

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
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-31-1-bib-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-31-1
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
if ! cargo build --locked --all-features > "$RAW/build.log" 2>&1; then
    tail -40 "$RAW/build.log" >&2
    echo "PHASE 21.31.1 BIB COURT: FAIL — all-features build failed" >&2
    exit 1
fi
tail -2 "$RAW/build.log"

echo "=== generate fixtures (POSIX printf only) ==="
mk() { /usr/bin/printf '%b' "$2" > "$1"; }

# --- NORMAL: bibliographies -------------------------------------------------
# refs.bib: a BibTeX entry with a braced/quoted/number/macro value, a duplicate field
# (note), and an @string entry (the concatenation and @comment paths are unit-tested).
mk "$WORK/corpus/refs.bib" '@article{smith2020,\n  author = {Smith, John},\n  title = "A Study",\n  year = 2020,\n  month = jan,\n  note = {first},\n  note = {second}\n}\n@string{ acm = "ACM" }\n'
# refs.ris: an RIS entry with a multi-line continuation on the TI field.
mk "$WORK/corpus/refs.ris" 'TY  - JOUR\nAU  - Smith, John\nTI  - A Study\n      continued here\nPY  - 2020\nER  - \n'
# refs.json: a CSL-JSON array of two items.
mk "$WORK/corpus/refs.json" '[{"id":"smith2020","type":"article-journal","title":"A Study"},{"id":"doe2019","type":"book","title":"Things"}]\n'

# --- CONTROLS (boundaries) --------------------------------------------------
# A plain JSON array of non-CSL objects -> json.
mk "$WORK/corpus/array.json" '[{"a":1},{"b":2}]\n'
# Plain prose -> opaque.
mk "$WORK/corpus/prose.txt" 'This is just prose text.\nIt has several lines, with punctuation,\nbut no bibliography structure at all.\n'

NORMAL="refs.bib refs.ris refs.json"
CONTROL="array.json prose.txt"
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
json_control_ok=0
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
        [ "$fmt" = "bib" ] || { echo "FINDING: $f detected as $fmt (want bib)" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"format":"bib"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata format" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"text"' "$RAW/$f.text.json" || { echo "FINDING: $f doc-text" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --bib-dialect --kind text \
            > "$RAW/$f.dialect.json" 2>&1 || surface_fail=$((surface_fail + 1))

        "$BIN" observe --store "$WORK/store" --field "$field" --bib-entry 0 --kind metadata \
            > "$RAW/$f.entry0.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"dialect"' "$RAW/$f.entry0.json" || { echo "FINDING: $f entry0" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --bib-find Study --kind text \
            > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"matches":\[' "$RAW/$f.find.json" || { echo "FINDING: $f find" >&2; surface_fail=$((surface_fail + 1)); }

        # Common search-match resolves.
        "$BIN" observe --store "$WORK/store" --field "$field" --text Study --kind text \
            > "$RAW/$f.common.search.json" 2>&1 || surface_fail=$((surface_fail + 1))

        case "$f" in
            refs.bib)
                grep -q '"dialect":"bibtex"' "$RAW/$f.metadata.json" || { echo "FINDING: bibtex dialect" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"text":"bibtex"' "$RAW/$f.dialect.json" || { echo "FINDING: bibtex dialect text" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"key":"smith2020"' "$RAW/$f.entry0.json" || { echo "FINDING: bibtex entry key" >&2; surface_fail=$((surface_fail + 1)); }
                # Field 0:title keeps the quoted value's delimiters ("A Study").
                "$BIN" observe --store "$WORK/store" --field "$field" --bib-field 0:title --kind exact \
                    > "$RAW/$f.field.bin" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"value_hex":"224120537475647922"' "$RAW/$f.field.bin" || { echo "FINDING: bibtex title exact (want \\\"A Study\\\")" >&2; surface_fail=$((surface_fail + 1)); }
                # The duplicate field is reported.
                "$BIN" observe --store "$WORK/store" --field "$field" --bib-entry 0 --kind metadata \
                    > "$RAW/$f.entry0.dup.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"duplicates":1' "$RAW/$f.entry0.dup.json" || { echo "FINDING: bibtex duplicate field" >&2; surface_fail=$((surface_fail + 1)); }
                # Key lookup by citation key.
                "$BIN" observe --store "$WORK/store" --field "$field" --bib-key smith2020 --kind text \
                    > "$RAW/$f.key.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"text":"smith2020"' "$RAW/$f.key.json" || { echo "FINDING: bibtex key" >&2; surface_fail=$((surface_fail + 1)); } ;;
            refs.ris)
                grep -q '"dialect":"ris"' "$RAW/$f.metadata.json" || { echo "FINDING: ris dialect" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"text":"ris"' "$RAW/$f.dialect.json" || { echo "FINDING: ris dialect text" >&2; surface_fail=$((surface_fail + 1)); }
                # Field 0:TI is the exact value (delimiters-free for RIS).
                "$BIN" observe --store "$WORK/store" --field "$field" --bib-field 0:TI --kind exact \
                    > "$RAW/$f.field.bin" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"value_hex":"41205374756479' "$RAW/$f.field.bin" || { echo "FINDING: ris TI exact (want A Study...)" >&2; surface_fail=$((surface_fail + 1)); } ;;
            refs.json)
                grep -q '"dialect":"csl_json"' "$RAW/$f.metadata.json" || { echo "FINDING: csl dialect" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"text":"csl_json"' "$RAW/$f.dialect.json" || { echo "FINDING: csl dialect text" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"id":"smith2020"' "$RAW/$f.entry0.json" || { echo "FINDING: csl item id" >&2; surface_fail=$((surface_fail + 1)); }
                # Field 0:title returns the JSON string token (with quotes).
                "$BIN" observe --store "$WORK/store" --field "$field" --bib-field 0:title --kind exact \
                    > "$RAW/$f.field.bin" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"value_hex":"224120537475647922"' "$RAW/$f.field.bin" || { echo "FINDING: csl title exact (want \\\"A Study\\\")" >&2; surface_fail=$((surface_fail + 1)); }
                # Key lookup by item id.
                "$BIN" observe --store "$WORK/store" --field "$field" --bib-key doe2019 --kind text \
                    > "$RAW/$f.key.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"text":"doe2019"' "$RAW/$f.key.json" || { echo "FINDING: csl key" >&2; surface_fail=$((surface_fail + 1)); } ;;
        esac

        # H3: an out-of-range entry, an absent field/key, and an unsupported common
        # pair decline typed (rc 6), once.
        if [ "$f" = "refs.bib" ]; then
            set +e
            "$BIN" observe --store "$WORK/store" --field "$field" --bib-entry 99 --kind metadata \
                > /dev/null 2>&1
            d_rc1=$?
            "$BIN" observe --store "$WORK/store" --field "$field" --bib-field 0:nope --kind text \
                > /dev/null 2>&1
            d_rc2=$?
            "$BIN" observe --store "$WORK/store" --field "$field" --bib-key nope --kind text \
                > /dev/null 2>&1
            d_rc3=$?
            "$BIN" observe --store "$WORK/store" --field "$field" --table 0 --kind text \
                > /dev/null 2>&1
            d_rc4=$?
            set -e
            [ "$d_rc1" -eq 6 ] && [ "$d_rc2" -eq 6 ] && [ "$d_rc3" -eq 6 ] && [ "$d_rc4" -eq 6 ] \
                && decline_ok=$((decline_ok + 1)) \
                || echo "FINDING: typed declines rc=$d_rc1/$d_rc2/$d_rc3/$d_rc4 (want 6/6/6/6)" >&2
        fi
    else
        control_n=$((control_n + 1))
        case "$f" in
            array.json)
                [ "$fmt" = "json" ] || { echo "FINDING: $f detected as $fmt (want json)" >&2; surface_fail=$((surface_fail + 1)); }
                set +e
                "$BIN" observe --store "$WORK/store" --field "$field" --bib-entry 0 --kind metadata \
                    > "$RAW/$f.bib-decline.json" 2>&1
                control_rc=$?
                set -e
                [ "$control_rc" -eq 6 ] && json_control_ok=$((json_control_ok + 1)) || { echo "FINDING: $f bib-entry rc=$control_rc (want 6)" >&2; }
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
        if [ "$f" != "prose.txt" ]; then
            "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
                > "$RAW/$f.metadata.json" 2>&1 || true
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

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$f" "$fmt" "$src_len" "$src_sha" "$out_len" "$out_sha" "$cmp_ok" "$exact" \
        "$control_rc" >> "$TSV"
done

echo "=== exactness after source + descriptor deletion ==="
cat "$TSV"

fixtures_n="$(printf '%s\n' $ALL | wc -w | tr -d ' ')"

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 21.31.1 bibliography court — matrix"
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
    echo "bib_fixtures $normal_n"
    echo "control_fixtures $control_n"
    echo "exact_ok $exact_ok"
    echo "exact_fail $exact_fail"
    echo "surface_fail $surface_fail"
    echo "boundary_ok $boundary_ok"
    echo "opaque_control_ok $opaque_ok"
    echo "json_control_ok $json_control_ok"
    echo "typed_declines_ok $decline_ok"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.31.1 — bibliography surface + exact closure + model",
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
    || [ "$boundary_ok" -ne 2 ] || [ "$opaque_ok" -ne 1 ] \
    || [ "$json_control_ok" -ne 1 ] \
    || [ "$decline_ok" -ne 1 ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.31.1 — bibliography surface + exact closure + model",
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
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion, for every fixture including the plain-JSON-array and Opaque controls",
  "bib": "a bounded bibliography adapter covering three dialects with a RECORDED dialect: BibTeX (the entry key, the entry type and field names AS WRITTEN, the field order, every value's exact spelling and delimiters with the brace/quoted/number/macro form kept distinct, '#' concatenation, and duplicate fields preserved and REPORTED), RIS (the two-space 'XX  - ' tag separator, the multi-line continuation span, and the entry order), and CSL-JSON (the reused shared JSON arena: member order, duplicate keys, numeric/string-escape spelling, and exact token spans, plus each item's string id and type). Nothing is normalized or re-serialized",
  "observations": ["metadata", "text", "search-match", "native bib-dialect", "native bib-entry", "native bib-field", "native bib-key", "native bib-find"],
  "declines": "an out-of-range entry, an absent field/key, and an unsupported common pair decline typed (rc 6); a plain JSON array of non-CSL objects and plain prose decline typed (rc 6), never a panic",
  "detection_boundary": "bibliography detection is content-only (NEVER a file name). BibTeX and RIS are line-anchored text (a line-anchored @type{ entry sequence; an RIS TY  -  sequence whose every entry is ER  -  terminated); CSL-JSON is a bounded semantic sub-test run BEFORE the generic JSON detector: a JSON array (or single object) whose every element carries a string 'id' AND a string 'type'",
  "cannot_distinguish": "a JSON array of objects that each carry a string 'id' and a string 'type' but are not bibliographic (e.g. an array of GeoJSON Feature objects, which RFC 7946 does not admit as a document anyway) is byte-for-byte indistinguishable from CSL-JSON and IS claimed as csl_json; a SINGLE GeoJSON Feature/FeatureCollection object is claimed by the GeoJSON detector first. A plain JSON array, a JSON array of non-CSL objects, a generic JSON object, and prose all stay json/opaque. A RIS continuation line that begins with two uppercase alphanumerics and the separator is read as a new tagged line (the inherent RIS ambiguity). A BibTeX value's kind is lexical; a macro is never resolved against @string",
  "precedence": "bibliography detection runs before the generic JSON detector (a CSL-JSON array is otherwise claimed by JSON) and before the weak text heuristics (YAML/TOML/config/CSV/Markdown/RST/AsciiDoc/logstream/fixedwidth)",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 47 invalid-bib-structure",
  "adr_0060": "the BibModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase21-31-1-bib-court.sh
# inside the court:
#   cargo build --locked --all-features
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--text PAT|--bib-dialect|--bib-entry N|--bib-field N:FIELD|--bib-key KEY|--bib-find PAT|--table N) --kind metadata|text|exact
#   rm source F.voldoc
#   target/debug/vole-document materialize --store STORE --field HEX --exact --output OUT ; cmp OUT source
# Regression controls (bib must not break the reused JSON court):
#   docker compose run --rm --no-TTY dev sh tools/phase21-5-1-json-court.sh
# Full gate (dev service):
#   cargo fmt --all --check
#   cargo clippy --all-targets --all-features -- -D warnings
#   cargo test --locked --all-features
#   cargo test --locked
#   MSRV: docker compose run --rm --no-TTY msrv cargo build --locked --all-features
EOF

# --- SUMMARY.md -------------------------------------------------------------
{
    echo "# Phase 21.31.1 — bibliography adapter court"
    echo
    echo "**Question.** Do bibliographies — BibTeX, RIS, and CSL-JSON — close exactly and"
    echo "expose a representation-preserving model (the recorded dialect; each entry/item;"
    echo "each field's exact value span with the BibTeX brace/quoted/number/macro form kept"
    echo "distinct and duplicate fields reported; the RIS two-space separator and a"
    echo "multi-line continuation; each CSL-JSON item's string \`id\` and \`type\`) on top of"
    echo "the whole-source exact leaf, while keeping a conservative content-only detection"
    echo "boundary (a plain JSON array stays \`json\`, and prose stays \`opaque\`)?"
    echo
    echo "**Method.** Each self-authored fixture is generated in-court with GNU"
    echo "\`/usr/bin/printf\`, ingested via \`field-build\`, observed, then — after the"
    echo "**source file and the standalone descriptor are deleted** — rematerialized"
    echo "exactly in a fresh process and compared with \`length + SHA-256 + cmp\`. An"
    echo "out-of-range entry, an absent field/key, and an unsupported common pair are"
    echo "required to decline typed; the plain-JSON-array and prose controls pin the"
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
    echo "- **Shipped here:** content-only, conservative bibliography detection (three"
    echo "  recorded dialects); the bounded, span-preserving entry/field/item model over the"
    echo "  reused JSON parser; native \`bib-dialect\`/\`bib-entry\`/\`bib-field\`/\`bib-key\`/"
    echo "  \`bib-find\`; common \`metadata\`/\`text\`/\`search-match\`."
    echo "- **Precedence:** bibliography detection is tried **before** the generic JSON"
    echo "  detector, so a CSL-JSON array is never stolen by JSON."
    echo "- **Boundary (honest):** a JSON array of objects that each carry a string \`id\` and"
    echo "  a string \`type\` but are not bibliographic is indistinguishable from CSL-JSON and"
    echo "  stays claimed as \`csl_json\`; a single GeoJSON Feature/FeatureCollection object is"
    echo "  claimed by GeoJSON first. A BibTeX macro is never resolved against \`@string\`."
} > "$CAMPAIGN/SUMMARY.md"

echo "=== counts ==="
cat "$CAMPAIGN/counts.txt"

if [ "$verdict" != "PASS" ]; then
    echo "PHASE 21.31.1 BIB COURT: FAIL — see $CAMPAIGN" >&2
    exit 1
fi
echo "PHASE 21.31.1 BIB COURT: PASS — receipt at $CAMPAIGN"
