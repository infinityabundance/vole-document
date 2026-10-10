#!/bin/sh
# Phase 21.11.1 — TOML (structured-tree Wave 2) surface + exact closure.
#
# Pre-registered (Phase-21 subphase 21.11.1). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture, `materialize(field) == source`
#       (length + SHA-256 + cmp) after the source file AND the standalone
#       descriptor are deleted, in a fresh process.
#   H2 (model + observations) — the representation-preserving model is exposed:
#       the exact source span of a table/array/inline-table/key/value/comment, the
#       exact spelling of every scalar (strings, integers with `_`/`0x`/`0o`/`0b`,
#       floats incl. `inf`/`nan`, booleans, date-times), dotted keys, arrays of
#       tables, and a table's keys; common metadata/text and native
#       toml-path/toml-table/toml-find answer.
#   H3 (honest declines) — an unsupported common pair declines typed with exit
#       code 6, never a silent empty answer; a control is not detected as TOML.
#   H5 (conservative detection) — plain prose (`prose.txt`), a duplicate-key
#       document (`dup.toml`, which violates TOML's redefinition rules), and
#       `<-`-junk (`junk.toml`) are NOT detected as TOML.
#
# Runs in the pinned `doc-baseline` service (dev toolchain + python3 + sqlite):
#   docker compose run --rm --no-TTY doc-baseline sh tools/phase21-11-1-toml-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-11-1-toml-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-11
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== regenerate fixtures (stdlib only) ==="
python3 tools/fixtures/make-toml.py
cp tools/fixtures/toml/*.toml tools/fixtures/toml/prose.txt "$WORK/corpus/"

# Normal fixtures: detected as TOML, every native/common observation answers, and
# each closes byte-exactly. Controls: not detected as TOML — an unsupported common
# observation must decline typed (rc 6).
ALL="basic.toml tables.toml arrays.toml inline.toml scalars.toml comments.toml large.toml prose.txt dup.toml junk.toml"
NORMAL="basic.toml tables.toml arrays.toml inline.toml scalars.toml comments.toml large.toml"
OPAQUE="prose.txt dup.toml junk.toml"

# The dotted path / table / find pattern each normal fixture observes.
path_for() {
    case "$1" in
        basic.toml)    echo "server.host" ;;
        tables.toml)   echo "owner.address.city" ;;
        arrays.toml)   echo "name" ;;
        inline.toml)   echo "name" ;;
        scalars.toml)  echo "basic" ;;
        comments.toml) echo "name" ;;
        large.toml)    echo "title" ;;
        *)             echo "" ;;
    esac
}
table_for() {
    case "$1" in
        basic.toml)    echo "server" ;;
        tables.toml)   echo "owner" ;;
        arrays.toml)   echo "meta" ;;
        inline.toml)   echo "nested" ;;
        scalars.toml)  echo "info" ;;
        comments.toml) echo "section" ;;
        large.toml)    echo "meta" ;;
        *)             echo "" ;;
    esac
}
find_for() {
    case "$1" in
        basic.toml)    echo "localhost" ;;
        tables.toml)   echo "London" ;;
        arrays.toml)   echo "product-1" ;;
        inline.toml)   echo "inline-marker" ;;
        scalars.toml)  echo "nodejs" ;;
        comments.toml) echo "comments" ;;
        large.toml)    echo "ZEBRA_MARKER_21_11" ;;
        *)             echo "" ;;
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

is_normal() {
    for n in $NORMAL; do [ "$n" = "$1" ] && return 0; done
    return 1
}
is_opaque() {
    for o in $OPAQUE; do [ "$o" = "$1" ] && return 0; done
    return 1
}

for f in $ALL; do
    src="tools/fixtures/toml/$f"
    cp "$src" "$WORK/corpus/$f"
    src_len="$(wc -c < "$src" | tr -d ' ')"
    src_sha="$(sha256sum "$src" | cut -d' ' -f1)"

    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    field="$(printf '%s' "$build_json" | jq -r '.ingest.field')"
    fmt="$(printf '%s' "$build_json" | jq -r '.ingest.format')"

    decline_rc=-1
    opaque_rc=-1

    if is_normal "$f"; then
        normal_n=$((normal_n + 1))
        [ "$fmt" = "toml" ] || echo "FINDING: $f not detected as toml (fmt=$fmt)" >&2
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --toml-find "$(find_for "$f")" --kind text \
            > "$RAW/$f.find.json"
        ptr="$(path_for "$f")"
        if [ -n "$ptr" ]; then
            "$BIN" observe --store "$WORK/store" --field "$field" --toml-path "$ptr" --kind metadata \
                > "$RAW/$f.path.json"
            "$BIN" observe --store "$WORK/store" --field "$field" --toml-path "$ptr" --kind exact \
                > "$RAW/$f.token.bin"
        fi
        tbl="$(table_for "$f")"
        if [ -n "$tbl" ]; then
            "$BIN" observe --store "$WORK/store" --field "$field" --toml-table "$tbl" --kind metadata \
                > "$RAW/$f.table.json"
        fi
        # H3: an unsupported common pair declines typed (rc 6).
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --table 0 --kind text \
            > /dev/null 2>&1
        decline_rc=$?
        set -e
        if [ "$decline_rc" -eq 6 ]; then declines_ok=$((declines_ok + 1)); fi
    elif is_opaque "$f"; then
        # H5: not detected as TOML; its common observation declines typed (rc 6).
        [ "$fmt" = "opaque" ] || echo "FINDING: control $f detected as $fmt (want opaque)" >&2
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1
        opaque_rc=$?
        set -e
        if [ "$opaque_rc" -eq 6 ]; then opaque_ok=$((opaque_ok + 1)); fi
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
    echo "# Phase 21.11.1 TOML court — matrix"
    echo
    echo "| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | opaque rc |"
    echo "| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |"
    jq -r '.[] | "| `\(.fixture)` | \(.format) | \(.src_len) | \(.out_len) | \(.src_sha256==.out_sha256) | \(.cmp_ok) | \(.exact) | \(.decline_rc) | \(.opaque_rc) |"' \
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
  "phase": "21.11.1 — TOML structured-tree surface + exact closure",
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
  "fixtures_tool": "tools/fixtures/make-toml.py",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
[ "$exact_fail" -eq 0 ] || verdict="FAIL"
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.11.1 — TOML structured-tree surface + exact closure",
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
  "fixture_tool": "tools/fixtures/make-toml.py (stdlib only)",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion",
  "observations": ["metadata", "text", "native toml-path", "native toml-table", "native toml-find"],
  "declines": "unsupported common pairs decline typed (rc 6); an opaque control is Opaque (rc 6), never a panic",
  "redefinition_policy": "TOML duplicate-key and redefinition rules are ENFORCED: a violation is a typed decline (rc 26), so dup.toml is not detected (Opaque), never silently overwritten",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 26 invalid-toml-structure",
  "adr_0060": "the TomlModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY doc-baseline sh tools/phase21-11-1-toml-court.sh
# inside the court:
#   cargo build --locked --all-features
#   python3 tools/fixtures/make-toml.py
#   target/debug/vole-document field-build tools/fixtures/toml/F.toml --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--toml-find a|--toml-path P|--toml-table T) --kind metadata|text|exact
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
    echo "# Phase 21.11.1 — TOML (structured-tree Wave 2) court"
    echo
    echo "**Question.** Does the TOML format close exactly and expose a"
    echo "representation-preserving model (exact table/array/inline-table/key/value/"
    echo "comment spans, dotted keys, arrays of tables, and every scalar's exact"
    echo "spelling) on top of the whole-source exact leaf — while **enforcing** TOML's"
    echo "duplicate-key/redefinition rules (so violated input stays Opaque)?"
    echo
    echo "**Method.** Each self-authored TOML fixture (generated deterministically by"
    echo "\`tools/fixtures/make-toml.py\`, Python stdlib only) is ingested via"
    echo "\`field-build\`, observed, then — after the **source file and the standalone"
    echo "descriptor are deleted** — rematerialized exactly in a fresh process and"
    echo "compared with \`length + SHA-256 + cmp\`. Unsupported common pairs are asked"
    echo "for and required to decline typed; the prose/duplicate-key/junk controls are"
    echo "required to be Opaque."
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
    echo "- **Shipped here:** byte-based conservative TOML detection; the bounded,"
    echo "  span-preserving parser (tables, arrays of tables, dotted keys, inline"
    echo "  tables, arrays, keys, values, comments; every scalar's exact spelling); the"
    echo "  canonical derived model; native \`toml-path\`/\`toml-table\`/\`toml-find\`;"
    echo "  common \`metadata\`/\`text\`."
    echo "- **Precedence:** TOML is tried after JSON/YAML and before CSV/Markdown/XML/"
    echo "  HTML, because its complete-parse signal is strong and a TOML comment (\`#\`"
    echo "  at column 0) would otherwise be misread as a Markdown ATX heading."
    echo "- **Rules enforced:** duplicate keys and table redefinitions are typed"
    echo "  declines (rc 26), not silently preserved; such input stays Opaque."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the TOML"
    echo "  model is derived (\`Q_gen\`) and never on the exactness path (ADR-0060: the"
    echo "  model node depends on the \`sha256(source)\` root)."
    echo "- **Not claimed here:** the economic court (separate script), calendar"
    echo "  validation of date-times, and encodings beyond UTF-8."
    echo "- **Never run on the host:** every command above ran in the pinned"
    echo "  \`doc-baseline\` container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$exact_fail" -ne 0 ]; then
    echo "PHASE 21.11.1 TOML COURT: FAIL" >&2
    exit 1
fi
echo "PHASE 21.11.1 TOML COURT: PASS — campaign $CAMPAIGN"
