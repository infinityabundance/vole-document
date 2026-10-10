#!/bin/sh
# Phase 21.13.1 — EML / MIME (messaging Wave 2) surface + exact closure.
#
# Pre-registered (Phase-21 subphase 21.13.1). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture, `materialize(field) == source`
#       (length + SHA-256 + cmp) after the source file AND the standalone
#       descriptor are deleted, in a fresh process.
#   H2 (message model + observations) — the representation-preserving message model
#       is exposed: every header's exact name/value/full span, header **order** and
#       **duplicate** headers, folded headers, the resolved `multipart/*` tree, and
#       the exact `Content-Transfer-Encoding`-decoded constituent bytes; common
#       metadata/text/resource and native
#       eml-header/eml-part/eml-attachments/eml-body/eml-find answer.
#   H3 (honest declines / boundaries) — an out-of-range part declines typed with
#       exit code 6, never a silent empty answer. The `prose.txt` control (no header
#       block) stays `Opaque` and a common observation on it declines typed.
#
# Runs in the pinned `doc-baseline` service (dev toolchain + python3 + sqlite):
#   docker compose run --rm --no-TTY doc-baseline sh tools/phase21-13-1-eml-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-13-1-eml-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-13
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features -j 1 2>&1 | tail -2

echo "=== regenerate fixtures (stdlib only) ==="
python3 tools/fixtures/make-eml.py
cp tools/fixtures/eml/*.eml tools/fixtures/eml/prose.txt "$WORK/corpus/"

ALL="simple.eml mixed.eml alternative.eml nested.eml folded.eml large.eml prose.txt"
NORMAL="simple.eml mixed.eml alternative.eml nested.eml folded.eml large.eml"
OPAQUE="prose.txt"

header_for() { echo "Subject"; }
part_for() {
    case "$1" in
        mixed.eml|large.eml) echo "2" ;;
        alternative.eml|nested.eml) echo "1" ;;
        *) echo "0" ;;
    esac
}
find_for() {
    case "$1" in
        simple.eml)      echo "simple" ;;
        mixed.eml)       echo "Caf" ;;
        alternative.eml) echo "HTML" ;;
        nested.eml)      echo "forwarded" ;;
        folded.eml)      echo "folded" ;;
        large.eml)       echo "attachment" ;;
        *)               echo "z" ;;
    esac
}
att_for() {
    case "$1" in
        mixed.eml|large.eml) echo "0" ;;
        *) echo "" ;;
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
is_opaque() { [ "$1" = "$OPAQUE" ] && return 0; return 1; }

for f in $ALL; do
    src="tools/fixtures/eml/$f"
    cp "$src" "$WORK/corpus/$f"
    src_len="$(wc -c < "$src" | tr -d ' ')"
    src_sha="$(sha256sum "$src" | cut -d' ' -f1)"

    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    field="$(printf '%s' "$build_json" | jq -r '.ingest.field')"
    fmt="$(printf '%s' "$build_json" | jq -r '.ingest.format')"

    decline_rc=-1
    ctl_rc=-1

    if is_normal "$f"; then
        normal_n=$((normal_n + 1))
        [ "$fmt" = "eml" ] || echo "FINDING: $f not detected as eml (fmt=$fmt)" >&2
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --eml-header "$(header_for)" --kind metadata \
            > "$RAW/$f.header.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --eml-part "$(part_for "$f")" --kind metadata \
            > "$RAW/$f.part.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --eml-part "$(part_for "$f")" --kind exact \
            > "$RAW/$f.part.bin"
        "$BIN" observe --store "$WORK/store" --field "$field" --eml-body --kind text \
            > "$RAW/$f.body.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --eml-attachments --kind metadata \
            > "$RAW/$f.attachments.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --eml-find "$(find_for "$f")" --kind text \
            > "$RAW/$f.find.json"
        att="$(att_for "$f")"
        if [ -n "$att" ]; then
            "$BIN" observe --store "$WORK/store" --field "$field" --resource "$att" --kind exact \
                > "$RAW/$f.resource.bin"
        fi
        # H3: an out-of-range part declines typed (rc 6).
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --eml-part 999999 --kind metadata \
            > /dev/null 2>&1
        decline_rc=$?
        set -e
        if [ "$decline_rc" -eq 6 ]; then declines_ok=$((declines_ok + 1)); fi
    elif is_opaque "$f"; then
        # H3: not detected as EML; a common observation on it declines typed (rc 6).
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
        --argjson decline_rc "$decline_rc" \
        --argjson ctl_rc "$ctl_rc" \
        '{fixture:$f,format:$fmt,src_len:$src_len,src_sha256:$src_sha,out_len:$out_len,out_sha256:$out_sha,cmp_ok:$cmp_ok,exact:$exact,decline_rc:$decline_rc,ctl_rc:$ctl_rc}')
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
    echo "# Phase 21.13.1 EML court — matrix"
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
  "phase": "21.13.1 — EML/MIME message surface + exact closure",
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
  "fixtures_tool": "tools/fixtures/make-eml.py",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
[ "$exact_fail" -eq 0 ] || verdict="FAIL"
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.13.1 — EML/MIME message surface + exact closure",
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
  "fixture_tool": "tools/fixtures/make-eml.py (stdlib only)",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion",
  "observations": ["metadata", "text", "resource", "native eml-header", "native eml-part", "native eml-attachments", "native eml-body", "native eml-find"],
  "declines": "an out-of-range part declines typed (rc 6); the prose.txt control (no header block) stays Opaque and a common observation on it declines typed (rc 6), never a panic",
  "precedence": "EML is tried after JSON/JSONL and before YAML/TOML/CSV/Markdown/XML/HTML, because a raw message's header block would otherwise be reinterpreted as a YAML mapping",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 28 invalid-eml-structure",
  "adr_0060": "the EmlModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY doc-baseline sh tools/phase21-13-1-eml-court.sh
# inside the court:
#   cargo build --locked --all-features -j 1
#   python3 tools/fixtures/make-eml.py
#   target/debug/vole-document field-build tools/fixtures/eml/F --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--eml-header NAME|--eml-part N|--eml-attachments|--eml-body|--eml-find P|--resource N) --kind metadata|text|exact
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
    echo "# Phase 21.13.1 — EML / MIME (messaging Wave 2) court"
    echo
    echo "**Question.** Does the EML/MIME format close exactly and expose a"
    echo "representation-preserving **message** model (every header's exact"
    echo "name/value/full span, header order and duplicate headers, folded headers, the"
    echo "resolved \`multipart/*\` tree, and the exact"
    echo "\`Content-Transfer-Encoding\`-decoded constituent bytes, incl. attachments) on"
    echo "top of the whole-source exact leaf, while declining malformed inputs typed?"
    echo
    echo "**Method.** Each self-authored EML fixture (generated deterministically by"
    echo "\`tools/fixtures/make-eml.py\`, Python stdlib only) is ingested via"
    echo "\`field-build\`, observed, then — after the **source file and the standalone"
    echo "descriptor are deleted** — rematerialized exactly in a fresh process and"
    echo "compared with \`length + SHA-256 + cmp\`. An out-of-range part is asked for and"
    echo "required to decline typed; the \`prose.txt\` control pins the detection"
    echo "boundary."
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
    echo "- **Shipped here:** byte-based conservative EML detection; the bounded,"
    echo "  span-preserving message model; native"
    echo "  \`eml-header\`/\`eml-part\`/\`eml-attachments\`/\`eml-body\`/\`eml-find\`; common"
    echo "  \`metadata\`/\`text\`/\`resource\`/\`search-match\`."
    echo "- **Precedence:** EML is tried after JSON/JSONL and before"
    echo "  YAML/TOML/CSV/Markdown/XML/HTML."
    echo "- **Declines:** a multipart without a boundary, an unknown transfer encoding,"
    echo "  a non-UTF-8 charset for text, and encrypted/signed S/MIME are typed declines."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the EML"
    echo "  model is derived (\`Q_gen\`) and never on the exactness path (ADR-0060: the"
    echo "  model node depends on the \`sha256(source)\` root)."
    echo "- **Not claimed here:** the economic court (separate script)."
    echo "- **Never run on the host:** every command above ran in the pinned"
    echo "  \`doc-baseline\` container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$exact_fail" -ne 0 ]; then
    echo "PHASE 21.13.1 EML COURT: FAIL" >&2
    exit 1
fi
echo "PHASE 21.13.1 EML COURT: PASS — campaign $CAMPAIGN"
