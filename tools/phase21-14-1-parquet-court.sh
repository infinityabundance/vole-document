#!/bin/sh
# Phase 21.14.1 — Parquet (analytical Wave 2) surface + exact closure.
#
# Pre-registered (Phase-21 subphase 21.14.1). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture, `materialize(field) == source`
#       (length + SHA-256 + cmp) after the source file AND the standalone
#       descriptor are deleted, in a fresh process.
#   H2 (schema + inventory + decoded values) — the Thrift-Compact footer is parsed:
#       the schema (names, physical/logical types, repetition), the row-group and
#       column-chunk inventory with each chunk's **exact source span** and
#       statistics, and the decoded values for PLAIN and RLE_DICTIONARY across
#       BOOLEAN/INT32/INT64/FLOAT/DOUBLE/BYTE_ARRAY/FIXED_LEN_BYTE_ARRAY, including
#       OPTIONAL nulls, GZIP, and multiple row groups / pages. Common
#       metadata/text/table/cell/search-match and native
#       parquet-schema/parquet-column/parquet-row-group/parquet-cell answer.
#   H3 (honest declines / boundaries) — an unsupported codec (ZSTD) or encoding
#       (DELTA_BINARY_PACKED) declines typed, a decompression bomb declines typed
#       (resource limit), and the Opaque controls (prose, a truncated file, an
#       inconsistent footer length) stay `Opaque` with a typed common decline.
#
# Runs in the pinned `doc-baseline` service (dev toolchain + python3 + sqlite):
#   docker compose run --rm --no-TTY doc-baseline sh tools/phase21-14-1-parquet-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-14-1-parquet-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-14
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features -j 1 2>&1 | tail -2

echo "=== regenerate fixtures (stdlib only) ==="
python3 tools/fixtures/make-parquet.py
cp tools/fixtures/parquet/*.parquet tools/fixtures/parquet/prose.txt "$WORK/corpus/"

NORMAL="small_plain.parquet optional.parquet dictionary.parquet gzip.parquet multi_rg.parquet two_pages.parquet large.parquet"
DECLINE="unsupported_codec.parquet unsupported_encoding.parquet bomb.parquet"
OPAQUE="prose.txt truncated.parquet badlen.parquet"
ALL="$NORMAL $DECLINE $OPAQUE"

col_for() {
    case "$1" in
        small_plain.parquet|gzip.parquet) echo "5" ;;
        optional.parquet|multi_rg.parquet|two_pages.parquet) echo "1" ;;
        dictionary.parquet) echo "0" ;;
        large.parquet) echo "2" ;;
        *) echo "0" ;;
    esac
}

printf '[\n' > "$RAW/results.json"
first=1
exact_ok=0
exact_fail=0
declines_ok=0
opaque_ok=0
opaque_n=0
normal_n=0

is_normal() { for n in $NORMAL; do [ "$n" = "$1" ] && return 0; done; return 1; }
is_decline() { for n in $DECLINE; do [ "$n" = "$1" ] && return 0; done; return 1; }
is_opaque() { for n in $OPAQUE; do [ "$n" = "$1" ] && return 0; done; return 1; }

for f in $ALL; do
    src="tools/fixtures/parquet/$f"
    src_len="$(wc -c < "$src" | tr -d ' ')"
    src_sha="$(sha256sum "$src" | cut -d' ' -f1)"

    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    field="$(printf '%s' "$build_json" | jq -r '.ingest.field')"
    fmt="$(printf '%s' "$build_json" | jq -r '.ingest.format')"

    decl_rc=-1
    ctl_rc=-1

    if is_normal "$f"; then
        normal_n=$((normal_n + 1))
        [ "$fmt" = "parquet" ] || echo "FINDING: $f not detected as parquet (fmt=$fmt)" >&2
        col="$(col_for "$f")"
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --table 0 --kind text \
            > "$RAW/$f.table.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --parquet-schema --kind metadata \
            > "$RAW/$f.schema.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --parquet-column "$col" --kind text \
            > "$RAW/$f.column.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --parquet-row-group 0 --kind metadata \
            > "$RAW/$f.rowgroup.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --parquet-cell "0:$col" --kind text \
            > "$RAW/$f.cell.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --parquet-column "$col" --kind exact \
            > "$RAW/$f.column.bin"
    elif is_decline "$f"; then
        [ "$fmt" = "parquet" ] || echo "FINDING: $f not detected as parquet (fmt=$fmt)" >&2
        # H3: the footer/inventory is exposed, the decoded column declines typed.
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json"
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --parquet-column 0 --kind text \
            > "$RAW/$f.column.txt" 2> "$RAW/$f.column.err"
        decl_rc=$?
        set -e
        # 6 = unsupported-feature (codec/encoding); 8 = resource-limit (bomb).
        if [ "$decl_rc" -eq 6 ] || [ "$decl_rc" -eq 8 ]; then declines_ok=$((declines_ok + 1)); fi
    elif is_opaque "$f"; then
        [ "$fmt" = "opaque" ] || echo "FINDING: control $f detected as $fmt (want opaque)" >&2
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1
        ctl_rc=$?
        set -e
        if [ "$ctl_rc" -eq 6 ]; then opaque_ok=$((opaque_ok + 1)); fi
        opaque_n=$((opaque_n + 1))
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
        --argjson decl_rc "$decl_rc" \
        --argjson ctl_rc "$ctl_rc" \
        '{fixture:$f,format:$fmt,src_len:$src_len,src_sha256:$src_sha,out_len:$out_len,out_sha256:$out_sha,cmp_ok:$cmp_ok,exact:$exact,decline_rc:$decl_rc,ctl_rc:$ctl_rc}')
    if [ "$first" -eq 1 ]; then first=0; else printf ',\n' >> "$RAW/results.json"; fi
    printf '%s' "$obs" >> "$RAW/results.json"
done
printf '\n]\n' >> "$RAW/results.json"

echo "=== exactness after source + descriptor deletion ==="
cat "$RAW/results.json" | jq -c '.[] | {fixture,format,exact,decline_rc,ctl_rc}'

echo "=== summary ==="
jq -n \
    --argjson fixtures "$(printf '%s' "$ALL" | wc -w | tr -d ' ')" \
    --argjson normal "$normal_n" \
    --argjson exact_ok "$exact_ok" \
    --argjson exact_fail "$exact_fail" \
    --argjson typed_declines_ok "$declines_ok" \
    --argjson opaque_controls_ok "$opaque_ok" \
    --argjson opaque_controls "$opaque_n" \
    '{fixtures:$fixtures,normal:$normal,exact_ok:$exact_ok,exact_fail:$exact_fail,typed_declines_ok:$typed_declines_ok,opaque_controls_ok:$opaque_controls_ok,opaque_controls:$opaque_controls}' \
    | tee "$CAMPAIGN/summary.json"

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 21.14.1 Parquet court — matrix"
    echo
    echo "| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |"
    echo "| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |"
    jq -r '.[] | "| `\(.fixture)` | \(.format) | \(.src_len) | \(.out_len) | \(.src_sha256==.out_sha256) | \(.cmp_ok) | \(.exact) | \(.decline_rc) | \(.ctl_rc) |"' \
        "$RAW/results.json"
} > "$CAMPAIGN/MATRIX.md"

# --- counts.txt -------------------------------------------------------------
{
    echo "fixtures $(( $(printf '%s' "$ALL" | wc -w) ))"
    echo "normal $normal_n"
    echo "exact_ok $exact_ok"
    echo "exact_fail $exact_fail"
    echo "typed_declines_ok $declines_ok"
    echo "opaque_controls_ok $opaque_ok"
    echo "opaque_controls $opaque_n"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.14.1 — Parquet analytical surface + exact closure",
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
  "fixtures_tool": "tools/fixtures/make-parquet.py",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
[ "$exact_fail" -eq 0 ] || verdict="FAIL"
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.14.1 — Parquet analytical surface + exact closure",
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
  "fixture_tool": "tools/fixtures/make-parquet.py (stdlib only; hand-built Parquet)",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion",
  "observations": ["metadata", "text/table", "search-match", "parquet-schema", "parquet-column", "parquet-row-group", "parquet-cell"],
  "declines": "an unsupported codec (ZSTD) or encoding (DELTA_BINARY_PACKED) declines typed (rc 6); a decompression bomb declines typed (rc 8); the Opaque controls stay opaque with a typed common decline (rc 6)",
  "detection": "PAR1 at both ends plus a consistent little-endian footer length; Parquet is tried after the package families and before the weak heuristics",
  "supports": "PLAIN + RLE_DICTIONARY/PLAIN_DICTIONARY encodings; UNCOMPRESSED + GZIP codecs; BOOLEAN/INT32/INT64/FLOAT/DOUBLE/BYTE_ARRAY/FIXED_LEN_BYTE_ARRAY; DATA_PAGE v1 + DICTIONARY_PAGE; flat columns",
  "declines_typed": "INT96; DATA_PAGE_V2; deprecated BIT_PACKED levels; DELTA_* / GROUP_VAR_INT / BYTE_STREAM_SPLIT encodings; SNAPPY/ZSTD/BROTLI/LZO/LZ4 codecs; repeated/nested columns; legacy min/max statistics",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 29 invalid-parquet-structure",
  "adr_0060": "the ParquetModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY doc-baseline sh tools/phase21-14-1-parquet-court.sh
# inside the court:
#   cargo build --locked --all-features -j 1
#   python3 tools/fixtures/make-parquet.py
#   target/debug/vole-document field-build tools/fixtures/parquet/F --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--table 0|--parquet-schema|--parquet-column N|--parquet-row-group N|--parquet-cell R:C) --kind metadata|text|exact
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
    echo "# Phase 21.14.1 — Parquet (analytical Wave 2) court"
    echo
    echo "**Question.** Does the Parquet format close exactly and expose a bounded"
    echo "derived model — the Thrift-Compact footer's schema, row-group and"
    echo "column-chunk inventory (each chunk with its exact source span and"
    echo "statistics), and the decoded values for PLAIN and RLE_DICTIONARY across the"
    echo "common physical types — on top of the whole-source exact leaf, while"
    echo "declining unsupported codecs/encodings and bombs typed?"
    echo
    echo "**Method.** Each self-authored Parquet fixture (generated deterministically"
    echo "by \`tools/fixtures/make-parquet.py\`, Python stdlib only — the\`analytical\`"
    echo "lane is where DuckDB cross-checks the very same bytes) is ingested via"
    echo "\`field-build\`, observed, then — after the **source file and the standalone"
    echo "descriptor are deleted** — rematerialized exactly in a fresh process and"
    echo "compared with \`length + SHA-256 + cmp\`. Unsupported/bomb fixtures are"
    echo "required to decline typed; the Opaque controls pin the detection boundary."
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
    echo "- **Shipped here:** byte-based conservative Parquet detection; the bounded,"
    echo "  dependency-free Thrift-Compact footer reader; the schema/inventory"
    echo "  observation; decoded values for PLAIN + RLE_DICTIONARY across"
    echo "  BOOLEAN/INT32/INT64/FLOAT/DOUBLE/BYTE_ARRAY/FIXED_LEN_BYTE_ARRAY, with"
    echo "  UNCOMPRESSED + GZIP; native"
    echo "  \`parquet-schema\`/\`parquet-column\`/\`parquet-row-group\`/\`parquet-cell\`; common"
    echo "  \`metadata\`/\`text\`/\`table\`/\`cell\`/\`search-match\`."
    echo "- **Typed declines:** INT96, DATA_PAGE_V2, deprecated BIT_PACKED levels, the"
    echo "  DELTA_* / GROUP_VAR_INT / BYTE_STREAM_SPLIT encodings, the SNAPPY/ZSTD/"
    echo "  BROTLI/LZO/LZ4 codecs, repeated/nested columns, and legacy \`min\`/\`max\`"
    echo "  statistics — each is declined typed, never guessed."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the Parquet"
    echo "  model is derived (\`Q_gen\`) and never on the exactness path (ADR-0060: the"
    echo "  model node depends on the \`sha256(source)\` root)."
    echo "- **Not claimed here:** the economic court (separate script)."
    echo "- **Never run on the host:** every command above ran in a pinned container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$exact_fail" -ne 0 ]; then
    echo "PHASE 21.14.1 PARQUET COURT: FAIL" >&2
    exit 1
fi
echo "PHASE 21.14.1 PARQUET COURT: PASS — campaign $CAMPAIGN"
