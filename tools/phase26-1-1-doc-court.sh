#!/bin/sh
# Phase 26.1 — legacy Word 97–2003 (`.doc`, MS-DOC) semantic surface + exact closure.
#
# Pre-registered (Phase-26 subphase 26.1.1). Hypotheses:
#
#   H1 (byte-exactness) — for every normal fixture, `materialize(field) == source`
#       (length + SHA-256 + cmp) after the source file AND the standalone descriptor
#       are deleted, in a fresh process. The court FAILS unless this is 100% on every
#       normal fixture.
#   H2 (surface + preserved representation) — the representation-preserving model is
#       exposed on top of the reused CFBF/OLE2 physical layer: the Word FIB
#       (`wIdent` `0xA5EC`, `nFib`/version, the `fWhichTblStm` table selection,
#       `fcMin`/`fcMac`, the `ccp*` section counts, `fcClx`/`lcbClx`) and the piece
#       table (each piece's CP range, resolved `fc`, compressed flag, exact
#       `WordDocument`-stream byte span, and decoded text), with a mix of a
#       **compressed** (CP1252) piece and an **uncompressed** (UTF-16LE) piece, in
#       both a `0Table` and a `1Table` variant (the non-selected table carries a decoy
#       CLX). Common metadata/text/search-match and native
#       doc-fib/doc-piece/doc-text/doc-find answer.
#   H3 (honest declines / boundaries) — the controls pin the detection boundaries: a
#       generic compound file with no Word FIB, a `.doc` with a bad `wIdent`, and a
#       `.doc` with an implausible `nFib` (Word 6/95) all stay `Cfbf` (a valid compound
#       file the CFBF layer accepts) and a doc selector on them declines typed (rc 6);
#       prose stays `Opaque` (rc 6). A `.doc` with a plausible FIB but a malformed
#       body (an out-of-range `fcClx`/`fcMin`, a non-monotonic CP array, a PCD `fc` outside
#       the `WordDocument` stream) detects as `doc` but its model declines typed
#       (InvalidDocStructure, rc 52). Never a panic, never a silent empty answer.
#
# Runs in the pinned `dev` service (Rust toolchain + git + coreutils + sed/grep;
# deliberately NO python3, NO Word/LibreOffice/`wvWare`, NO external OLE tooling, NO
# network):
#   docker compose run --rm --no-TTY dev sh tools/phase26-1-1-doc-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase26-1-1-doc-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase26-1
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== generate fixtures (dependency-free .doc writer) ==="
cargo run --locked --all-features --example doc_fixtures -- "$WORK/corpus" 2>&1 | tail -1

# Fixture classes.
#   NORMAL   — valid `.doc`: H1 exactness + H2 surface.
#   DECLINE  — detects as `doc` but the model is structurally malformed (rc 52).
#   CFBF_CTL — a valid compound file that is NOT a Word 97–2003 document (format
#              `cfbf`); a doc selector declines typed (rc 6).
#   OPAQUE   — not a compound file at all; a doc selector declines typed (rc 6).
NORMAL="doc_0table.doc doc_1table.doc"
DECLINE="doc_bad_clx_range.doc doc_fcmin_range.doc doc_nonmonotonic.doc doc_piece_outside.doc"
CFBF_CTL="doc_bad_ident.doc doc_bad_nfib.doc generic_cfbf.bin"
OPAQUE="prose.txt"

table_for() {
    case "$1" in
        doc_1table.doc) echo "1Table" ;;
        *) echo "0Table" ;;
    esac
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
opaque_ok=0
opaque_n=0
normal_n=0
piece_ok=0
piece_n=0
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
    [ "$fmt" = "doc" ] || { echo "FINDING: $f detected as $fmt (want doc)" >&2; surface_fail=$((surface_fail + 1)); }
    set +e
    "$BIN" observe --store "$WORK/store" --field "$field" --doc-fib --kind text \
        > "$RAW/$f.decline.txt" 2>&1
    ctl_rc=$?
    set -e
    if [ "$ctl_rc" -eq 52 ]; then decline_after_detect_ok=$((decline_after_detect_ok + 1)); else echo "FINDING: $f structural decline rc=$ctl_rc (want 52)" >&2; fi
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
    "$BIN" observe --store "$WORK/store" --field "$field" --doc-fib --kind text \
        > "$RAW/$f.decline.txt" 2>&1
    ctl_rc=$?
    set -e
    if [ "$ctl_rc" -eq 6 ]; then cfbf_ctl_ok=$((cfbf_ctl_ok + 1)); else echo "FINDING: CFBF control $f doc selector rc=$ctl_rc (want 6)" >&2; fi
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
    "$BIN" observe --store "$WORK/store" --field "$field" --doc-fib --kind text \
        > "$RAW/$f.decline.txt" 2>&1
    ctl_rc=$?
    set -e
    if [ "$ctl_rc" -eq 6 ]; then opaque_ok=$((opaque_ok + 1)); else echo "FINDING: opaque control $f doc selector rc=$ctl_rc (want 6)" >&2; fi
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
    "$BIN" observe --store "$WORK/store" --field "$field" --doc-find "Hello" --kind metadata \
        > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))

    grep -q '"format":"doc"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata missing format" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"nFib":193' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata missing nFib 193" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"version":"word97"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata version != word97" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q "\"table\":\"$want_table\"" "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata table != $want_table" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"pieces":3' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata piece count != 3" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"text":"word97"' "$RAW/$f.fib.txt" || { echo "FINDING: $f fib text != word97" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"selector":"doc-text"' "$RAW/$f.body.json" || { echo "FINDING: $f native doc-text selector missing" >&2; surface_fail=$((surface_fail + 1)); }
    # The document text is the main-text pieces only — the decoy must never win.
    grep -q 'Hello, w' "$RAW/$f.text.json" || { echo "FINDING: $f document text missing" >&2; surface_fail=$((surface_fail + 1)); }
    if grep -q 'DECOY' "$RAW/$f.text.json"; then echo "FINDING: $f document text picked the decoy table" >&2; surface_fail=$((surface_fail + 1)); fi
    grep -q '"matches":\[{' "$RAW/$f.find.json" || { echo "FINDING: $f find returned no matches" >&2; surface_fail=$((surface_fail + 1)); }

    # H2: the piece list — a compressed piece, an uncompressed piece, and exact bytes.
    piece_n=$((piece_n + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --doc-piece 0 --kind metadata \
        > "$RAW/$f.piece0.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --doc-piece 0 --kind exact \
        > "$RAW/$f.piece0.bin" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --doc-piece 1 --kind metadata \
        > "$RAW/$f.piece1.json" 2>&1 || surface_fail=$((surface_fail + 1))
    grep -q '"compressed":true' "$RAW/$f.piece0.json" || { echo "FINDING: $f piece0 not compressed" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"cp_start":0' "$RAW/$f.piece0.json" || { echo "FINDING: $f piece0 cp_start != 0" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"span":\[898,905\]' "$RAW/$f.piece0.json" || { echo "FINDING: $f piece0 span != [898,905]" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"compressed":false' "$RAW/$f.piece1.json" || { echo "FINDING: $f piece1 should be uncompressed" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"units":6' "$RAW/$f.piece1.json" || { echo "FINDING: $f piece1 units != 6" >&2; surface_fail=$((surface_fail + 1)); }
    # The compressed piece's exact bytes are the seven CP1252 bytes "Hello, "
    # (the `--kind exact` answer carries `bytes_len`, `bytes_sha256`, and `value_hex`).
    if grep -q '"bytes_len":7' "$RAW/$f.piece0.bin" && grep -q '48656c6c6f2c20' "$RAW/$f.piece0.bin"; then
        piece_ok=$((piece_ok + 1))
    else
        echo "FINDING: $f piece0 exact bytes are not 'Hello, '" >&2
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

echo "=== exactness after source + descriptor deletion ==="
cat "$TSV"

fixtures_n=$((normal_n + opaque_n + cfbf_ctl_n))

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 26.1.1 legacy Word (.doc) court — matrix"
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
    echo "pieces_ok $piece_ok"
    echo "pieces_n $piece_n"
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
  "phase": "26.1.1 — legacy Word 97–2003 (.doc, MS-DOC) semantic surface + exact closure on the CFBF/OLE2 physical layer",
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
    || [ "$opaque_ok" -ne "$opaque_n" ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "26.1.1 — legacy Word 97–2003 (.doc, MS-DOC) semantic surface + exact closure on the CFBF/OLE2 physical layer",
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
  "observations": ["metadata", "text", "search-match", "native doc-fib", "native doc-piece", "native doc-text", "native doc-find"],
  "surface": "the Word FIB (wIdent 0xA5EC, nFib/version word97, the fWhichTblStm table selection 0Table/1Table, fcMin/fcMac, the ccp* section counts ccpText/ccpFtn/ccpHdd, fcClx/lcbClx) and the piece table (each piece's CP range, resolved fc, compressed flag, and exact WordDocument-stream byte span, plus decoded text), on top of the reused CFBF/OLE2 physical layer",
  "declines": "a .doc with a plausible FIB but a malformed body (an out-of-range fcClx, an out-of-range fcMin, a non-monotonic CP array, a PCD fc outside the WordDocument stream) detects as doc and its model declines typed InvalidDocStructure (rc 52); a generic compound file, a .doc with a bad wIdent, and a .doc with an implausible nFib (Word 6/95) stay Cfbf and a doc selector declines typed (rc 6); prose stays Opaque (rc 6); never a panic",
  "precedence": "the DOC detector is built on top of the CFBF detector and runs before the generic Cfbf claim, so a .doc becomes Doc while a generic compound file stays Cfbf; a non-CFBF source is unaffected",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 52 invalid-doc-structure",
  "adr_0060": "the DocModel node depends on the exact source root (DocumentExact, keyed by sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase26-1-1-doc-court.sh
# inside the court:
#   cargo build --locked --all-features
#   cargo run --locked --all-features --example doc_fixtures -- CORPUS
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--doc-fib|--doc-piece 0|--doc-find PAT) --kind metadata|text|exact
#   rm source F.voldoc
#   target/debug/vole-document materialize --store STORE --field HEX --exact --output OUT ; cmp OUT source
# Full gate (dev service):
#   cargo fmt --all --check
#   cargo clippy --all-targets --all-features -- -D warnings
#   cargo test --locked --all-features   (use CARGO_PROFILE_TEST_DEBUG=0 if the all-features link OOM-kills)
#   cargo test --locked --features doc
#   CFBF regression control: sh tools/phase21-35-1-cfbf-court.sh
#   MSRV: docker compose run --rm --no-TTY msrv cargo build --locked --all-features
EOF

# --- SUMMARY.md -------------------------------------------------------------
{
    echo "# Phase 26.1.1 — legacy Word 97–2003 (\`.doc\`, MS-DOC) court"
    echo
    echo "**Question.** Does a legacy Word 97–2003 document close exactly and expose a"
    echo "representation-preserving semantic model (the Word FIB and the piece table, with"
    echo "every piece's CP range, resolved \`fc\`, compressed flag, and exact"
    echo "\`WordDocument\`-stream byte span, and the decoded text) **on top of the reused"
    echo "CFBF/OLE2 physical layer**, while staying honest at the detection boundary and"
    echo "declining malformed inputs typed?"
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
    echo "- **Supported:** detection on top of the CFBF layer (the \`WordDocument\` stream"
    echo "  begins with the Word FIB magic \`0xA5EC\` with a plausible Word 97–2003 \`nFib\`"
    echo "  and the table stream exists); the FIB (\`wIdent\`, \`nFib\`/version, the"
    echo "  \`fWhichTblStm\` table selection, \`fcMin\`/\`fcMac\`, \`cbMac\`, the \`ccp*\` section"
    echo "  counts, \`fcClx\`/\`lcbClx\`); the piece table (the \`Prc\` records and the \`Pcdt\`"
    echo "  \`PlcPcd\`: the CP array and the 8-byte PCDs), each piece's \`fc\`, compressed"
    echo "  flag, and exact \`WordDocument\`-stream byte span, and its decoded text; the main"
    echo "  document text over \`[0, ccpText)\`; native"
    echo "  \`doc-fib\`/\`doc-piece\`/\`doc-text\`/\`doc-find\`; common"
    echo "  \`metadata\`/\`text\`/\`search-match\`."
    echo "- **Boundary:** a generic compound file, a \`.doc\` with a bad \`wIdent\`, and a"
    echo "  \`.doc\` with an implausible \`nFib\` (Word 6/95) stay \`Cfbf\`; prose stays"
    echo "  \`Opaque\`; a \`.doc\` with a plausible FIB but a malformed body is admitted and"
    echo "  its model declines typed."
    echo "- **Declines:** a bad \`wIdent\`, an implausible \`nFib\`, a missing selected table"
    echo "  stream, an out-of-range \`fcClx\`/\`lcbClx\` or \`fcMin\`/\`fcMac\`, a CLX past the"
    echo "  table stream, an absent \`Pcdt\` or an \`lcb\` that is not \`12n+4\`, a CP array that"
    echo "  is not monotonic (or does not start at 0), a PCD \`fc\` whose piece falls outside"
    echo "  the \`WordDocument\` stream, and a piece-table last CP that disagrees with the"
    echo "  declared \`ccp*\` totals are typed declines (\`InvalidDocStructure\`)."
    echo "- **Not claimed here:** Word 6/95 (an older FIB), Word 2007+ \`.docx\` (an OPC"
    echo "  ZIP, not a compound file), and the other legacy CFBF semantic models"
    echo "  (XLS/PPT/MSG, later subphases); the economic court (separate script)."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the DOC model"
    echo "  is derived and never on the exactness path (ADR-0060: the model node depends on"
    echo "  the \`sha256(source)\` root)."
    echo "- **Never run on the host:** every command above ran in the pinned \`dev\`"
    echo "  container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" != "PASS" ]; then
    echo "PHASE 26.1.1 DOC COURT: FAIL — campaign $CAMPAIGN (exact_ok=$exact_ok/$normal_n surface_fail=$surface_fail pieces_ok=$piece_ok/$piece_n declines=$decline_after_detect_ok/$decline_n cfbf_ctl=$cfbf_ctl_ok/$cfbf_ctl_n opaque=$opaque_ok/$opaque_n)" >&2
    exit 1
fi
echo "PHASE 26.1.1 DOC COURT: PASS — campaign $CAMPAIGN"
