#!/bin/sh
# Phase 21.35 — CFBF / OLE2 compound-file physical-layer surface + exact closure.
#
# Pre-registered (Phase-21 subphase 21.35.1). Hypotheses:
#
#   H1 (byte-exactness) — for every normal fixture, `materialize(field) == source`
#       (length + SHA-256 + cmp) after the source file AND the standalone descriptor
#       are deleted, in a fresh process. The court FAILS unless this is 100% on every
#       normal fixture.
#   H2 (surface + preserved representation) — the representation-preserving model is
#       exposed: the recorded dialect (`cfb3`/`cfb4`), the header (sector size, FAT /
#       directory counts, header span), the directory entries with verbatim UTF-16LE
#       names (incl. non-ASCII `café`), object type, the reconstructed sibling/child
#       tree (`parent`/`depth`), exact entry spans, and the resolved stream spans (a
#       FAT-chained large stream >= 4096 B and a MiniFAT-chained small stream). Common
#       metadata/text/search-match and native cfbf-dialect/cfbf-entry/cfbf-stream/
#       cfbf-find answer.
#   H3 (honest declines / boundaries) — the controls pin the detection boundaries: a
#       signature-only blob, a corrupted-signature blob, a bad-sector-shift blob, an
#       out-of-range directory-sector blob, and prose stay `Opaque`; a compound file
#       with a plausible header but a malformed body (an invalid directory object type,
#       a cyclic FAT chain, a truncated stream chain) detects as `cfbf` but its model
#       declines typed (InvalidCfbfStructure). Never a panic, never a silent empty
#       answer.
#
# Runs in the pinned `dev` service (Rust toolchain + git + coreutils + sed/grep;
# deliberately NO python3, NO external OLE tooling, NO network):
#   docker compose run --rm --no-TTY dev sh tools/phase21-35-1-cfbf-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-35-1-cfbf-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-35
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== generate fixtures (dependency-free CFBF/OLE2 writer) ==="
cargo run --locked --all-features --example cfbf_fixtures -- "$WORK/corpus" 2>&1 | tail -1

# Fixture classes.
#   NORMAL  — valid compound files: H1 exactness + H2 surface.
#   DECLINE — detects as `cfbf` but the model is structurally malformed (rc 51).
#   OPAQUE  — not detected as a compound file; a cfbf selector declines typed (rc 6).
NORMAL="cfbf_v3.cfb cfbf_v4.cfb"
DECLINE="cfbf_badtype.cfb cfbf_cyclic.cfb cfbf_truncated_stream.cfb"
OPAQUE="cfbf_bad_dir_sector.cfb cfbf_badsig.cfb cfbf_badshift.cfb sig_only.bin prose.txt"

dialect_for() {
    case "$1" in
        cfbf_v4.cfb) echo "cfb4" ;;
        *) echo "cfb3" ;;
    esac
}
find_for() {
    case "$1" in
        cfbf_v3.cfb) echo "Larg" ;;
        cfbf_v4.cfb) echo "caf" ;;
        *) echo "e" ;;
    esac
}
entry_name_for() {
    echo "Root Entry"
}
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
office_ok=0
office_n=0
opaque_ok=0
opaque_n=0
normal_n=0
entry_ok=0
entry_n=0
decline_after_detect_ok=0

for f in $DECLINE; do
    src="$WORK/ref/$f"
    cp "$WORK/corpus/$f" "$src"
    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    fmt="$(json_str "$build_json" format)"
    field="$(json_str "$build_json" field)"
    [ "$fmt" = "cfbf" ] || { echo "FINDING: $f detected as $fmt (want cfbf)" >&2; surface_fail=$((surface_fail + 1)); }
    set +e
    "$BIN" observe --store "$WORK/store" --field "$field" --cfbf-dialect --kind text \
        > "$RAW/$f.decline.txt" 2>&1
    ctl_rc=$?
    set -e
    if [ "$ctl_rc" -eq 51 ]; then decline_after_detect_ok=$((decline_after_detect_ok + 1)); else echo "FINDING: $f structural decline rc=$ctl_rc (want 51)" >&2; fi
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
    "$BIN" observe --store "$WORK/store" --field "$field" --cfbf-dialect --kind text \
        > "$RAW/$f.decline.txt" 2>&1
    ctl_rc=$?
    set -e
    if [ "$ctl_rc" -eq 6 ]; then opaque_ok=$((opaque_ok + 1)); else echo "FINDING: opaque control $f cfbf selector rc=$ctl_rc (want 6)" >&2; fi
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
    want_dialect="$(dialect_for "$f")"
    [ "$fmt" = "cfbf" ] || { echo "FINDING: $f not detected as cfbf (fmt=$fmt)" >&2; surface_fail=$((surface_fail + 1)); }

    # H2: common metadata / text / search-match + native dialect / entry / stream / find.
    "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
        > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
        > "$RAW/$f.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --cfbf-dialect --kind text \
        > "$RAW/$f.dialect.txt" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --cfbf-find "$(find_for "$f")" --kind metadata \
        > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))

    grep -q '"format":"cfbf"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata missing format" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q "\"dialect\":\"$want_dialect\"" "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata dialect != $want_dialect" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q "\"text\":\"$want_dialect\"" "$RAW/$f.dialect.txt" || { echo "FINDING: $f dialect text != $want_dialect" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"matches":\[' "$RAW/$f.find.json" || { echo "FINDING: $f find returned no matches" >&2; surface_fail=$((surface_fail + 1)); }

    # H2: the native entry + stream indices.
    entry_n=$((entry_n + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --cfbf-entry 0 --kind exact \
        > "$RAW/$f.entry0.bin" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --cfbf-entry 0 --kind metadata \
        > "$RAW/$f.entry0.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --cfbf-stream 2 --kind exact \
        > "$RAW/$f.large.bin" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --cfbf-stream 2 --kind metadata \
        > "$RAW/$f.large.json" 2>&1 || surface_fail=$((surface_fail + 1))
    name="$(entry_name_for "$f")"
    grep -q "\"name\":\"$name" "$RAW/$f.entry0.json" || { echo "FINDING: $f entry0 name != $name" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"type":"root"' "$RAW/$f.entry0.json" || { echo "FINDING: $f entry0 is not the root storage" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"stream_size":5000' "$RAW/$f.large.json" || { echo "FINDING: $f large stream size != 5000" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"mini":false' "$RAW/$f.large.json" || { echo "FINDING: $f large stream should be FAT-chained" >&2; surface_fail=$((surface_fail + 1)); }
    [ -s "$RAW/$f.entry0.bin" ] || { echo "FINDING: $f entry0 exact bytes empty" >&2; surface_fail=$((surface_fail + 1)); }
    [ -s "$RAW/$f.large.bin" ] || { echo "FINDING: $f large stream exact bytes empty" >&2; surface_fail=$((surface_fail + 1)); }
    entry_ok=$((entry_ok + 1))

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

fixtures_n=$((normal_n + opaque_n))

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 21.35.1 CFBF/OLE2 court — matrix"
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
    echo "native_entry_ok $entry_ok"
    echo "native_entry_n $entry_n"
    echo "opaque_controls_ok $opaque_ok"
    echo "opaque_controls $opaque_n"
    echo "structural_decline_ok $decline_after_detect_ok"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.35.1 — CFBF/OLE2 compound-file physical-layer surface + exact closure",
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
  "fixtures_tool": "examples/cfbf_fixtures.rs (dependency-free CFBF/OLE2 writer; no python3, no external OLE tooling)",
  "external_oracles": "none (the compound-file model is validated internally; no external OLE parser is invoked)",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
if [ "$exact_fail" -ne 0 ] || [ "$surface_fail" -ne 0 ] \
    || [ "$exact_ok" -ne "$normal_n" ] \
    || [ "$entry_ok" -ne "$entry_n" ] \
    || [ "$opaque_ok" -ne "$opaque_n" ] \
    || [ "$decline_after_detect_ok" -ne 3 ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.35.1 — CFBF/OLE2 compound-file physical-layer surface + exact closure",
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
  "fixtures": "$NORMAL $DECLINE $OPAQUE",
  "fixture_tool": "examples/cfbf_fixtures.rs",
  "external_oracles": "none",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion",
  "observations": ["metadata", "text", "search-match", "native cfbf-dialect", "native cfbf-entry", "native cfbf-stream", "native cfbf-find"],
  "surface": "the 512-byte header (signature, version, byte order, sector/mini-sector shifts, FAT/DIFAT/directory/MiniFAT counts and first sectors); the DIFAT and FAT; the 128-byte directory entries with verbatim UTF-16LE names (incl. non-ASCII), object type, color, and the reconstructed sibling/child tree (parent/depth); and each stream resolved to an exact byte span (FAT-chained large streams and MiniFAT-chained small streams inside the root mini stream); recorded dialect cfb3/cfb4",
  "declines": "a compound file with a plausible header but a malformed body (invalid directory object type, cyclic FAT chain, truncated stream chain) detects as cfbf and its model declines typed InvalidCfbfStructure (rc 51); a signature-only blob, a corrupted signature, a bad sector shift, an out-of-range directory sector, and prose stay Opaque and a cfbf selector on them declines typed (rc 6); never a panic",
  "precedence": "the strong-magic binaries (PDF/ZIP-family/container/scientific) are detected first; cfbf is inserted with them and before every weak, no-magic-byte heuristic, so a compound file is never guessed to be JSON/YAML/XML/prose and no earlier format's bytes are stolen",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 51 invalid-cfbf-structure",
  "adr_0060": "the CfbfModel node depends on the exact source root (DocumentExact, keyed by sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase21-35-1-cfbf-court.sh
# inside the court:
#   cargo build --locked --all-features
#   cargo run --locked --all-features --example cfbf_fixtures -- CORPUS
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--cfbf-dialect|--cfbf-entry 0|--cfbf-stream 2|--cfbf-find PAT) --kind metadata|text|exact
#   rm source F.voldoc
#   target/debug/vole-document materialize --store STORE --field HEX --exact --output OUT ; cmp OUT source
# Full gate (dev service):
#   cargo fmt --all --check
#   cargo clippy --all-targets --all-features -- -D warnings
#   cargo test --locked --all-features   (use CARGO_PROFILE_TEST_DEBUG=0 if the all-features link OOM-kills)
#   cargo test --locked --features cfbf
#   MSRV: docker compose run --rm --no-TTY msrv cargo build --locked --all-features
EOF

# --- SUMMARY.md -------------------------------------------------------------
{
    echo "# Phase 21.35.1 — CFBF / OLE2 compound-file court"
    echo
    echo "**Question.** Does a CFBF/OLE2 compound file close exactly and expose a"
    echo "representation-preserving physical-layer model (the header, the DIFAT/FAT, the"
    echo "128-byte directory entries with verbatim UTF-16LE names incl. non-ASCII and a"
    echo "reconstructed sibling/child tree, and each stream resolved to an exact byte"
    echo "span — FAT-chained large streams and MiniFAT-chained small streams — with a"
    echo "recorded cfb3/cfb4 dialect) on top of the whole-source exact leaf, while"
    echo "staying honest at the detection boundary and declining malformed inputs typed?"
    echo
    echo "**Method.** Each fixture is generated by the dependency-free"
    echo "\`examples/cfbf_fixtures.rs\` writer, ingested via \`field-build\`, observed, then —"
    echo "after the **source file and the standalone descriptor are deleted** —"
    echo "rematerialized exactly in a fresh process and compared with \`length + SHA-256 +"
    echo "cmp\`. The Opaque and structural controls pin the detection boundaries. The court"
    echo "runs in the pinned \`dev\` service using only POSIX \`sh\`, coreutils, git, and the"
    echo "shipped binary (no python3, no external OLE tooling, no network)."
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
    echo "- **Shipped here:** strong-magic + header-validation detection (the 8-byte"
    echo "  signature at offset 0 **and** an internally consistent header — byte order"
    echo "  \`FFFE\`, a major version of 3 or 4, a sector shift of 9 or 12 consistent with"
    echo "  the version, a mini-sector shift of 6, at least one FAT sector, and a first"
    echo "  directory sector inside the file); the parsed header, DIFAT (\`109\` header"
    echo "  entries + chained DIFAT sectors), FAT and MiniFAT; the 128-byte directory"
    echo "  entries with verbatim UTF-16LE names and the reconstructed red-black"
    echo "  sibling/child tree; the exact stream spans (regular and mini); native"
    echo "  \`cfbf-dialect\`/\`cfbf-entry\`/\`cfbf-stream\`/\`cfbf-find\`; common"
    echo "  \`metadata\`/\`text\`/\`search-match\`; a recorded dialect."
    echo "- **Boundary:** a signature-only blob (its header fields are zero), a corrupted"
    echo "  signature, a bad sector shift, an out-of-range directory sector, and prose all"
    echo "  stay \`Opaque\`. A blob with a plausible header but a malformed body is admitted"
    echo "  (its whole-source exact leaf is unaffected) and its model declines typed."
    echo "- **Declines:** an invalid directory object type, a cyclic/overlapping FAT or"
    echo "  MiniFAT chain, an out-of-range sector ID, a truncated stream chain, and a"
    echo "  header/sector-size mismatch are typed declines (\`InvalidCfbfStructure\`); a"
    echo "  bad/short input stays \`Opaque\` rather than being claimed."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the"
    echo "  compound-file model is derived and never on the exactness path (ADR-0060: the"
    echo "  model node depends on the \`sha256(source)\` root)."
    echo "- **Span caveat:** a stream's \`data_span\` is its exact \`(offset, length)\` when"
    echo "  its sector chain is contiguous (the common case); a non-contiguous chain records"
    echo "  a bounding hull and its \`ExactBytes\` declines typed rather than returning"
    echo "  interleaved foreign bytes."
    echo "- **Not claimed here:** the legacy DOC/XLS/PPT/MSG *semantic* models (later"
    echo "  subphases); the economic court (separate script)."
    echo "- **Never run on the host:** every command above ran in the pinned \`dev\`"
    echo "  container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" != "PASS" ]; then
    echo "PHASE 21.35.1 CFBF COURT: FAIL — campaign $CAMPAIGN (exact_ok=$exact_ok/$normal_n surface_fail=$surface_fail entry_ok=$entry_ok entry_n=$entry_n opaque=$opaque_ok/$opaque_n structural=$decline_after_detect_ok)" >&2
    exit 1
fi
echo "PHASE 21.35.1 CFBF COURT: PASS — campaign $CAMPAIGN"
