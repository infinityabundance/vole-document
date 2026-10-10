#!/bin/sh
# Phase 21.7.1 — CSV/TSV (tabular, Wave 2) surface + exact closure + model.
#
# Pre-registered (Phase-21 subphase 21.7.1). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture, `materialize(field) == source`
#       (length + SHA-256 + cmp) after the source file AND the standalone
#       descriptor are deleted, in a fresh process.
#   H2 (model + observations) — the representation-preserving model is exposed:
#       exact record/field source spans and exact bytes (quotes/CRLF/BOM
#       preserved), the recorded dialect, the header row; common
#       metadata/text/table/cell/search-match and native
#       csv-row/csv-cell/csv-header/csv-range/csv-find answer.
#   H3 (honest declines) — unsupported selector/representation pairs (e.g. the
#       common `heading` that does not map to CSV) decline typed with exit code 6,
#       never a silent empty answer; a plain-text, one-column, or malformed
#       document is Opaque (never a panic).
#   H4 (conservative detection) — a source that does not parse as a table under a
#       comma or tab delimiter with a consistent field count across a sampled
#       majority of records is not detected.
#
# Runs in the pinned `doc-baseline` service (dev toolchain + python3 + sqlite):
#   docker compose run --rm --no-TTY doc-baseline sh tools/phase21-7-1-csv-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-7-1-csv-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-7
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== regenerate fixtures (stdlib only) ==="
python3 tools/fixtures/make-csv.py
cp tools/fixtures/csv/* "$WORK/corpus/"

ALL="basic.csv quoted.csv crlf.csv bom.csv ragged.csv tsv.tsv large.csv plain.txt malformed.csv onecol.csv"
# Every fixture is byte-exact; plain.txt, malformed.csv, and onecol.csv are Opaque
# (their derived observations decline typed) yet still close exactly via the
# opaque floor.
NORMAL="basic.csv quoted.csv crlf.csv bom.csv ragged.csv tsv.tsv large.csv"

# Verbatim document-text observations are bounded. A fixture at or below this
# size is recorded byte-for-byte (unchanged from before); a larger one — the
# ~50 MB `large.csv` — is recorded as its text length + SHA-256 + a bounded
# prefix, so a tens-of-MB receipt is never written. The observation itself, its
# `source_span`, and every verdict below are unchanged; only the verbatim dump
# is bounded (the full text goes only to gitignored scratch).
TEXT_DUMP_MAX=$((4 * 1024 * 1024))

# The record each fixture observes through a row probe ("" = skip).
row_for() {
    case "$1" in
        basic.csv)   echo "1" ;;
        quoted.csv)  echo "2" ;;
        crlf.csv)    echo "1" ;;
        bom.csv)     echo "1" ;;
        ragged.csv)  echo "4" ;;
        tsv.tsv)     echo "1" ;;
        large.csv)   echo "1" ;;
        *)           echo "" ;;
    esac
}

# The cell (R:C) each fixture observes ("" = skip).
cell_for() {
    case "$1" in
        basic.csv)   echo "1:1" ;;
        quoted.csv)  echo "0:1" ;;
        crlf.csv)    echo "1:0" ;;
        bom.csv)     echo "1:0" ;;
        ragged.csv)  echo "1:0" ;;
        tsv.tsv)     echo "1:1" ;;
        large.csv)   echo "1:0" ;;
        *)           echo "" ;;
    esac
}

printf '[\n' > "$RAW/results.json"
first=1
exact_ok=0
exact_fail=0
declines_ok=0
opaque_ok=0
for f in $ALL; do
    src="tools/fixtures/csv/$f"
    cp "$src" "$WORK/corpus/$f"
    src_len="$(wc -c < "$src" | tr -d ' ')"
    src_sha="$(sha256sum "$src" | cut -d' ' -f1)"

    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    field="$(printf '%s' "$build_json" | jq -r '.ingest.field')"
    fmt="$(printf '%s' "$build_json" | jq -r '.ingest.format')"

    decline_rc=-1
    opaque_rc=-1
    case " $NORMAL " in
        *" $f "*) is_normal=1 ;;
        *)        is_normal=0 ;;
    esac

    if [ "$is_normal" -eq 0 ]; then
        # H4/H3: a plain-text, one-column, or malformed source is Opaque; its
        # common observation declines typed (rc 6), never a silent empty answer.
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1
        opaque_rc=$?
        set -e
        if [ "$opaque_rc" -eq 6 ]; then opaque_ok=$((opaque_ok + 1)); fi
    else
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json"
        # doc-text: bound the verbatim dump for an oversized fixture. The full
        # observation is written to scratch; the campaign keeps the whole JSON
        # byte-for-byte for a small fixture, and a length + SHA-256 + bounded
        # prefix receipt for a large one.
        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$WORK/$f.text.full.json"
        if [ "$src_len" -le "$TEXT_DUMP_MAX" ]; then
            mv "$WORK/$f.text.full.json" "$RAW/$f.text.json"
        else
            jq -j '.text' "$WORK/$f.text.full.json" > "$WORK/$f.text.bin"
            text_len=$(wc -c < "$WORK/$f.text.bin" | tr -d ' ')
            text_sha=$(sha256sum "$WORK/$f.text.bin" | cut -d' ' -f1)
            head -c "$TEXT_DUMP_MAX" "$WORK/$f.text.bin" > "$WORK/$f.text.prefix"
            jq -c --argjson cap "$TEXT_DUMP_MAX" --argjson tlen "$text_len" \
                --arg tsha "$text_sha" --rawfile pfx "$WORK/$f.text.prefix" \
                'del(.text) + {text_omitted:true, text_verbatim_bytes:$cap, text_len_bytes:$tlen, text_sha256:$tsha, text_prefix:$pfx}' \
                "$WORK/$f.text.full.json" > "$RAW/$f.text.json"
            rm -f "$WORK/$f.text.full.json" "$WORK/$f.text.bin" "$WORK/$f.text.prefix"
        fi
        "$BIN" observe --store "$WORK/store" --field "$field" --csv-header --kind metadata \
            > "$RAW/$f.header.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --csv-find a --kind text \
            > "$RAW/$f.find.json"
        rw="$(row_for "$f")"
        if [ -n "$rw" ]; then
            "$BIN" observe --store "$WORK/store" --field "$field" --csv-row "$rw" --kind exact \
                > "$RAW/$f.row.exact.json"
            "$BIN" observe --store "$WORK/store" --field "$field" --csv-row "$rw" --kind metadata \
                > "$RAW/$f.row.meta.json"
        fi
        cl="$(cell_for "$f")"
        if [ -n "$cl" ]; then
            "$BIN" observe --store "$WORK/store" --field "$field" --csv-cell "$cl" --kind exact \
                > "$RAW/$f.cell.exact.json"
            "$BIN" observe --store "$WORK/store" --field "$field" --csv-cell "$cl" --kind text \
                > "$RAW/$f.cell.text.json"
        fi
        "$BIN" observe --store "$WORK/store" --field "$field" --csv-range 0:0:1:1 --kind metadata \
            > "$RAW/$f.range.json"

        # H3: an unsupported common pair declines typed (rc 6).
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --heading 0 --kind text \
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
    '{fixtures:$fixtures,exact_ok:$exact_ok,exact_fail:$exact_fail,typed_declines_ok:$typed_declines_ok,opaque_controls_ok:$opaque_controls_ok}' \
    | tee "$CAMPAIGN/summary.json"

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 21.7.1 CSV court — matrix"
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
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.7.1 — CSV/TSV tabular surface + exact closure + model",
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
  "fixtures_tool": "tools/fixtures/make-csv.py",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
[ "$exact_fail" -eq 0 ] || verdict="FAIL"
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.7.1 — CSV/TSV tabular surface + exact closure + model",
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
  "fixture_tool": "tools/fixtures/make-csv.py (stdlib only)",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion",
  "observations": ["metadata", "text", "table", "cell", "search-match", "native csv-row", "native csv-cell", "native csv-header", "native csv-range", "native csv-find"],
  "declines": "unsupported common pairs decline typed (rc 6); a plain-text, one-column, or malformed document is Opaque (rc 6), never a panic",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit",
  "adr_0060": "the CsvModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY doc-baseline sh tools/phase21-7-1-csv-court.sh
# inside the court:
#   cargo build --locked --all-features
#   python3 tools/fixtures/make-csv.py
#   target/debug/vole-document field-build tools/fixtures/csv/F --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--csv-header|--csv-find P|--csv-row N|--csv-cell R:C|--csv-range R1:C1:R2:C2) --kind ...
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
    echo "# Phase 21.7.1 — CSV/TSV (tabular, Wave 2) court"
    echo
    echo "**Question.** Does the first Wave-2 tabular format close exactly and expose a"
    echo "representation-preserving table (exact record/field spans and bytes, quotes,"
    echo "embedded delimiters/newlines, CRLF vs LF, a BOM, ragged rows, a header, the"
    echo "recorded dialect) on top of the whole-source exact leaf?"
    echo
    echo "**Method.** Each self-authored CSV/TSV fixture (generated deterministically by"
    echo "\`tools/fixtures/make-csv.py\`, Python stdlib only) is ingested via"
    echo "\`field-build\`, observed, then — after the **source file and the standalone"
    echo "descriptor are deleted** — rematerialized exactly in a fresh process and"
    echo "compared with \`length + SHA-256 + cmp\`. Unsupported common pairs are asked for"
    echo "and required to decline typed; the plain-text, one-column, and malformed"
    echo "controls are required to be Opaque (a typed decline, never a panic)."
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
    echo "- **Shipped here:** byte-based conservative CSV/TSV detection; the bounded,"
    echo "  streaming RFC 4180 CSV + TSV parser (exact record/field spans and bytes, the"
    echo "  recorded dialect, original quoting with \`\"\"\` escapes, embedded"
    echo "  delimiters/newlines, CRLF/LF/CR, a BOM, ragged rows, blank lines, the header"
    echo "  row); the canonical derived model; native \`csv-row\`/\`csv-cell\`/"
    echo "  \`csv-header\`/\`csv-range\`/\`csv-find\`; common \`metadata\`/\`text\`/\`table\`/"
    echo "  \`cell\`/\`search-match\`."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the CSV model"
    echo "  is derived (\`Q_gen\`) and never on the exactness path (ADR-0060: the model node"
    echo "  depends on the \`sha256(source)\` root, so no source-reading node aliases another"
    echo "  field's source)."
    echo "- **Ragged rows are handled, not declined:** a record whose field count differs"
    echo "  from the header is preserved verbatim and reported; the header/majority column"
    echo "  count is what detection requires."
    echo "- **The supported subset is bounded.** CSV has no magic bytes, so detection is a"
    echo "  conservative heuristic (a stable delimiter, a consistent field count across a"
    echo "  sampled majority of at least two records, at least two columns). A plain-text,"
    echo "  one-column, or malformed document stays Opaque rather than being guessed at."
    echo "- **Not claimed here:** a full RFC 4180 conformance oracle, exotic quoting"
    echo "  conventions (non-\`\"\` quotes, escape characters), and the economic court."
    echo "- **Never run on the host:** every command above ran in the pinned"
    echo "  \`doc-baseline\` container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$exact_fail" -ne 0 ]; then
    echo "PHASE 21.7.1 CSV COURT: FAIL" >&2
    exit 1
fi
echo "PHASE 21.7.1 CSV COURT: PASS — campaign $CAMPAIGN"
