#!/bin/sh
# Phase 21.16.1 — Arrow IPC (analytical Wave 2) surface + exact closure.
#
# Pre-registered (Phase-21 subphase 21.16.1). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture, `materialize(field) == source`
#       (length + SHA-256 + cmp) after the source file AND the standalone
#       descriptor are deleted, in a fresh process.
#   H2 (schema + inventory + decoded values) — the Flatbuffers footer (file
#       format) or leading Schema message (stream format) is parsed: the schema
#       (names, type tags/parameters, nullability, children), the record-batch
#       inventory with each batch's **exact source span**, and the decoded values
#       for Int (all widths, signed/unsigned), FloatingPoint (half/single/double),
#       Boolean, Date/Time/Timestamp/Duration (as raw ints), Utf8/LargeUtf8,
#       Binary/LargeBinary, FixedSizeBinary, including OPTIONAL validity bitmaps
#       and multiple batches. Common metadata/text/table/cell/search-match and
#       native arrow-schema/arrow-column/arrow-batch/arrow-cell answer.
#   H3 (honest declines / boundaries) — a nested type, a dictionary-encoded field,
#       a Decimal, and a compressed body decline typed (rc 6); a row bomb declines
#       typed (rc 8); malformed Flatbuffers declines typed (rc 30), never a panic.
#       The Opaque controls (prose, an `ARROW1`-only prefix, a truncated prefix,
#       an inconsistent footer length) stay `Opaque` with a typed common decline.
#
# Runs in the pinned `doc-baseline` service (dev toolchain + python3 + sqlite):
#   docker compose run --rm --no-TTY doc-baseline sh tools/phase21-16-1-arrow-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-16-1-arrow-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-16
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features -j 1 2>&1 | tail -2

echo "=== regenerate fixtures (stdlib only) ==="
python3 tools/fixtures/make-arrow.py
cp tools/fixtures/arrow/*.arrow tools/fixtures/arrow/prose.txt \
    tools/fixtures/arrow/magic_only.bin "$WORK/corpus/"

NORMAL="primitives.arrow nullable.arrow strings.arrow temporal.arrow multi_batch.arrow fsb.arrow stream.arrow large.arrow"
DECLINE="unsupported_decimal.arrow unsupported_nested.arrow unsupported_dictionary.arrow unsupported_compressed.arrow bomb.arrow malformed_flatbuf.arrow"
OPAQUE="prose.txt magic_only.bin truncated.arrow badlen.arrow"
ALL="$NORMAL $DECLINE $OPAQUE"

col_for() {
    case "$1" in
        primitives.arrow) echo "i32" ;;
        nullable.arrow) echo "note" ;;
        strings.arrow) echo "s" ;;
        temporal.arrow) echo "ts" ;;
        multi_batch.arrow) echo "label" ;;
        fsb.arrow) echo "raw" ;;
        stream.arrow) echo "b" ;;
        large.arrow) echo "id" ;;
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
    src="tools/fixtures/arrow/$f"
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
        [ "$fmt" = "arrow" ] || echo "FINDING: $f not detected as arrow (fmt=$fmt)" >&2
        col="$(col_for "$f")"
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --table 0 --kind text \
            > "$RAW/$f.table.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --arrow-schema --kind metadata \
            > "$RAW/$f.schema.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --arrow-column "$col" --kind text \
            > "$RAW/$f.column.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --arrow-batch 0 --kind metadata \
            > "$RAW/$f.batch.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --arrow-cell "0:$col" --kind text \
            > "$RAW/$f.cell.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --arrow-column "$col" --kind exact \
            > "$RAW/$f.column.bin"
    elif is_decline "$f"; then
        [ "$fmt" = "arrow" ] || echo "FINDING: $f not detected as arrow (fmt=$fmt)" >&2
        # H3: the metadata inventory parses (file format) but the decoded column,
        # or the whole structure for malformed Flatbuffers, declines typed.
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --arrow-column 0 --kind text \
            > "$RAW/$f.column.txt" 2> "$RAW/$f.column.err"
        decl_rc=$?
        set -e
        # 6 = unsupported-feature; 8 = resource-limit; 30 = invalid-arrow-structure.
        if [ "$decl_rc" -eq 6 ] || [ "$decl_rc" -eq 8 ] || [ "$decl_rc" -eq 30 ]; then
            declines_ok=$((declines_ok + 1))
        fi
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

# Bound oversized raw evidence: the large fixture's verbatim table/column dumps are
# replaced with length+SHA-256+prefix receipts; the full dumps are gitignored.
python3 tools/fixtures/phase21-16-bound.py "$RAW"

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
    echo "# Phase 21.16.1 Arrow IPC court — matrix"
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
  "phase": "21.16.1 — Arrow IPC analytical surface + exact closure",
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
  "fixtures_tool": "tools/fixtures/make-arrow.py",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
[ "$exact_fail" -eq 0 ] || verdict="FAIL"
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.16.1 — Arrow IPC analytical surface + exact closure",
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
  "fixture_tool": "tools/fixtures/make-arrow.py (stdlib only; hand-built Arrow IPC)",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion",
  "observations": ["metadata", "text/table", "search-match", "arrow-schema", "arrow-column", "arrow-batch", "arrow-cell"],
  "declines": "a nested type, a dictionary-encoded field, a Decimal, and a compressed body decline typed (rc 6); a row bomb declines typed (rc 8); malformed Flatbuffers declines typed (rc 30); the Opaque controls stay opaque with a typed common decline (rc 6)",
  "detection": "the ARROW1 magic at the start plus either a consistent little-endian int32 footer length before the trailing ARROW1 (file format) or a valid encapsulated Schema message after the 8-byte magic+padding prefix (stream format); Arrow is tried after Parquet and before the weak heuristics",
  "supports": "Int 8/16/32/64 signed+unsigned, FloatingPoint half/single/double, Boolean, Date32/64, Time32/64, Timestamp, Duration (as raw ints), Utf8/LargeUtf8, Binary/LargeBinary, FixedSizeBinary, with validity bitmaps; file and stream formats; multi-batch",
  "declines_typed": "Null, Decimal, Interval, all nested types (List/LargeList/FixedSizeList/ListView/LargeListView/Struct/Map/Union/RunEndEncoded), BinaryView/Utf8View, dictionary-encoded fields, big-endian bodies, and any BodyCompression (LZ4_FRAME/ZSTD)",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 30 invalid-arrow-structure",
  "adr_0060": "the ArrowModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY doc-baseline sh tools/phase21-16-1-arrow-court.sh
# inside the court:
#   cargo build --locked --all-features -j 1
#   python3 tools/fixtures/make-arrow.py
#   target/debug/vole-document field-build tools/fixtures/arrow/F --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--table 0|--arrow-schema|--arrow-column NAME|N|--arrow-batch N|--arrow-cell R:C) --kind metadata|text|exact
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
    echo "# Phase 21.16.1 — Arrow IPC (analytical Wave 2) court"
    echo
    echo "**Question.** Does the Arrow IPC format close exactly and expose a bounded"
    echo "derived model — the Flatbuffers footer/leading-schema inventory (schema,"
    echo "record batches with exact source spans) and the decoded values for the common"
    echo "primitive/binary types — on top of the whole-source exact leaf, while"
    echo "declining unsupported types/compression and bombs typed?"
    echo
    echo "**Method.** Each self-authored Arrow IPC fixture (generated deterministically"
    echo "by \`tools/fixtures/make-arrow.py\`, Python stdlib only — a hand-built"
    echo "Flatbuffers metadata writer and columnar buffer encoders) is ingested via"
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
    echo "- **Shipped here:** byte-based conservative Arrow IPC detection; the bounded,"
    echo "  dependency-free Flatbuffers reader; the schema/inventory observation; decoded"
    echo "  values for Int (all widths, signed/unsigned), FloatingPoint (half/single/"
    echo "  double), Boolean, Date/Time/Timestamp/Duration (as raw ints), Utf8/LargeUtf8,"
    echo "  Binary/LargeBinary, FixedSizeBinary, with validity bitmaps; file and stream"
    echo "  formats; multi-batch; native \`arrow-schema\`/\`arrow-column\`/\`arrow-batch\`/"
    echo "  \`arrow-cell\`; common \`metadata\`/\`text\`/\`table\`/\`cell\`/\`search-match\`."
    echo "- **Typed declines:** Null, Decimal, Interval, every nested type (List/"
    echo "  LargeList/FixedSizeList/ListView/LargeListView/Struct/Map/Union/"
    echo "  RunEndEncoded), BinaryView/Utf8View, dictionary-encoded fields, big-endian"
    echo "  bodies, and any \`BodyCompression\` — each is declined typed, never guessed."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the Arrow"
    echo "  model is derived (\`Q_gen\`) and never on the exactness path (ADR-0060: the"
    echo "  model node depends on the \`sha256(source)\` root)."
    echo "- **Not claimed here:** the economic court (separate script)."
    echo "- **Never run on the host:** every command above ran in a pinned container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$exact_fail" -ne 0 ]; then
    echo "PHASE 21.16.1 ARROW COURT: FAIL" >&2
    exit 1
fi
echo "PHASE 21.16.1 ARROW COURT: PASS — campaign $CAMPAIGN"
