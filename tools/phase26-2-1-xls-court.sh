#!/bin/sh
# Phase 26.2 — legacy Excel 97–2003 (`.xls`, BIFF8) semantic surface + exact closure.
#
# Pre-registered (Phase-26 subphase 26.2.1). Hypotheses:
#
#   H1 (byte-exactness) — for every normal fixture, `materialize(field) == source`
#       (length + SHA-256 + cmp) after the source file AND the standalone descriptor
#       are deleted, in a fresh process. The court FAILS unless this is 100% on every
#       normal fixture.
#   H2 (surface + preserved representation) — the representation-preserving model is
#       exposed on top of the reused CFBF/OLE2 physical layer: the BIFF8 record stream
#       (the `BOF`/`EOF` framing and version, and each record's exact span), the
#       ordered sheets (`BOUNDSHEET` name + `BOF` stream position), the `SST` shared
#       string table (with `CONTINUE` reassembly of a continued string), and each cell
#       of every supported kind (`LABELSST`/`NUMBER`/`RK`/`BOOLERR`/`FORMULA`+`STRING`/
#       `BLANK`/`MULBLANK`) with its row/column, exact record span, and exact stored
#       value spelling (the `RK` compact numeric encoding is decoded), plus native
#       `xls-biff`/`xls-sheet`/`xls-cell`/`xls-record`/`xls-find` and common
#       `metadata`/`text`/`search-match`.
#   H3 (declines + boundaries) — a `.xls` with a plausible BIFF8 `BOF` but a malformed
#       body declines typed (rc 53); a generic compound file, and a `.xls` with a bad
#       `BOF` version or an unrecognized substream type, stay `cfbf` and an `xls`
#       selector declines typed (rc 6); prose stays `opaque` (rc 6). Never a panic.
#
# Dependency-free (the fixtures are written by a small shipped Rust writer;
# deliberately NO python3, NO Excel/LibreOffice, NO external OLE tooling, NO network):
#   docker compose run --rm --no-TTY dev sh tools/phase26-2-1-xls-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase26-2-1-xls-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase26-2
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== generate fixtures (dependency-free .xls writer) ==="
cargo run --locked --all-features --example xls_fixtures -- "$WORK/corpus" 2>&1 | tail -1

# Fixture classes.
#   NORMAL   — valid `.xls`: H1 exactness + H2 surface.
#   DECLINE  — detects as `xls` but the model is structurally malformed (rc 53).
#   CFBF_CTL — a valid compound file that is NOT a BIFF8 workbook (format `cfbf`); an
#              xls selector declines typed (rc 6).
#   OPAQUE   — not a compound file at all; an xls selector declines typed (rc 6).
NORMAL="xls_valid.xls"
DECLINE="xls_truncated.xls xls_bad_sst_count.xls xls_bad_boundsheet.xls xls_malformed_continue.xls"
CFBF_CTL="xls_bad_bof.xls xls_bad_type.xls generic_cfbf.bin"
OPAQUE="prose.txt"

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
cell_ok=0
cell_n=0
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
    [ "$fmt" = "xls" ] || { echo "FINDING: $f detected as $fmt (want xls)" >&2; surface_fail=$((surface_fail + 1)); }
    set +e
    "$BIN" observe --store "$WORK/store" --field "$field" --xls-biff --kind text \
        > "$RAW/$f.decline.txt" 2>&1
    ctl_rc=$?
    set -e
    if [ "$ctl_rc" -eq 53 ]; then decline_after_detect_ok=$((decline_after_detect_ok + 1)); else echo "FINDING: $f structural decline rc=$ctl_rc (want 53)" >&2; fi
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
    "$BIN" observe --store "$WORK/store" --field "$field" --xls-biff --kind text \
        > "$RAW/$f.decline.txt" 2>&1
    ctl_rc=$?
    set -e
    if [ "$ctl_rc" -eq 6 ]; then cfbf_ctl_ok=$((cfbf_ctl_ok + 1)); else echo "FINDING: CFBF control $f xls selector rc=$ctl_rc (want 6)" >&2; fi
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
    "$BIN" observe --store "$WORK/store" --field "$field" --xls-biff --kind text \
        > "$RAW/$f.decline.txt" 2>&1
    ctl_rc=$?
    set -e
    if [ "$ctl_rc" -eq 6 ]; then opaque_ok=$((opaque_ok + 1)); else echo "FINDING: opaque control $f xls selector rc=$ctl_rc (want 6)" >&2; fi
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
    [ "$fmt" = "xls" ] || { echo "FINDING: $f not detected as xls (fmt=$fmt)" >&2; surface_fail=$((surface_fail + 1)); }

    # H2: common metadata / text / search-match + native biff / sheet / cell / record /
    # find.
    "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
        > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --xls-biff --kind text \
        > "$RAW/$f.biff.txt" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --xls-sheet 0 --kind metadata \
        > "$RAW/$f.sheet0.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --xls-sheet 1 --kind text \
        > "$RAW/$f.sheet1.txt" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --xls-cell 2 --kind metadata \
        > "$RAW/$f.cell2.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --xls-cell 10 --kind metadata \
        > "$RAW/$f.cell10.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --xls-record 0 --kind exact \
        > "$RAW/$f.record0.bin" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --xls-find "Alpha" --kind metadata \
        > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --xls-text --kind text \
        > "$RAW/$f.text.txt" 2>&1 || surface_fail=$((surface_fail + 1))

    grep -q '"format":"xls"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata missing format" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"version":"biff8"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata version != biff8" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"biff_version":1536' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata biff_version != 1536" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"stream":"Workbook"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata stream != Workbook" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"sheets":2' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata sheet count != 2" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"sst_unique":3' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata sst_unique != 3" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"sst_strings":3' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata sst_strings != 3" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"cells":11' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata cell count != 11" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"text":"biff8"' "$RAW/$f.biff.txt" || { echo "FINDING: $f biff text != biff8" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"name":"Alpha"' "$RAW/$f.sheet0.json" || { echo "FINDING: $f sheet0 name != Alpha" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"Beta"' "$RAW/$f.sheet1.txt" || { echo "FINDING: $f sheet1 name != Beta" >&2; surface_fail=$((surface_fail + 1)); }
    # The document text carries the sheet names and the shared strings (the third is
    # the `CONTINUE`-split one).
    grep -q 'Alpha' "$RAW/$f.text.txt" || { echo "FINDING: $f text missing Alpha" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q 'Continued' "$RAW/$f.text.txt" || { echo "FINDING: $f text missing the CONTINUE-split string" >&2; surface_fail=$((surface_fail + 1)); }

    # H2: the cell kinds, the `RK` decode, and the `CONTINUE`-split string.
    cell_n=$((cell_n + 1))
    grep -q '"kind":"rk"' "$RAW/$f.cell2.json" || { echo "FINDING: $f cell2 not an RK" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"rk":30' "$RAW/$f.cell2.json" || { echo "FINDING: $f cell2 rk raw != 30" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"value":7' "$RAW/$f.cell2.json" || { echo "FINDING: $f cell2 rk value != 7" >&2; surface_fail=$((surface_fail + 1)); }
    if grep -q '"text":"Continued"' "$RAW/$f.cell10.json"; then
        cell_ok=$((cell_ok + 1))
    else
        echo "FINDING: $f CONTINUE-split shared string did not resolve to 'Continued'" >&2
        surface_fail=$((surface_fail + 1))
    fi
    # The BOF record's exact bytes (id 0x0809, length 0x0010) are 09 08 10 00 …
    if grep -q '"bytes_len":20' "$RAW/$f.record0.bin" && grep -q '09081000' "$RAW/$f.record0.bin"; then
        :
    else
        echo "FINDING: $f record0 exact bytes are not the BOF header" >&2
        surface_fail=$((surface_fail + 1))
    fi
    grep -q '"matches":\[{' "$RAW/$f.find.json" || { echo "FINDING: $f find returned no matches" >&2; surface_fail=$((surface_fail + 1)); }

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
    echo "# Phase 26.2.1 legacy Excel (.xls) court — matrix"
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
    echo "cells_ok $cell_ok"
    echo "cells_n $cell_n"
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
  "phase": "26.2.1 — legacy Excel 97–2003 (.xls, BIFF8) semantic surface + exact closure on the CFBF/OLE2 physical layer",
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
  "fixtures_tool": "examples/xls_fixtures.rs (dependency-free .xls/BIFF8-in-CFBF writer; no python3, no Excel/LibreOffice, no external OLE tooling)",
  "external_oracles": "none (the XLS model is validated internally; no external Excel parser is invoked)",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
if [ "$exact_fail" -ne 0 ] || [ "$surface_fail" -ne 0 ] \
    || [ "$exact_ok" -ne "$normal_n" ] \
    || [ "$cell_ok" -ne "$cell_n" ] \
    || [ "$decline_after_detect_ok" -ne "$decline_n" ] \
    || [ "$cfbf_ctl_ok" -ne "$cfbf_ctl_n" ] \
    || [ "$opaque_ok" -ne "$opaque_n" ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "26.2.1 — legacy Excel 97–2003 (.xls, BIFF8) semantic surface + exact closure on the CFBF/OLE2 physical layer",
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
  "fixture_tool": "examples/xls_fixtures.rs",
  "external_oracles": "none",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion",
  "observations": ["metadata", "text", "search-match", "native xls-biff", "native xls-sheet", "native xls-cell", "native xls-record", "native xls-find"],
  "surface": "the BIFF8 record stream (the BOF/EOF framing and version, and each record's exact span), the ordered sheets (BOUNDSHEET name + BOF stream position), the SST shared string table (with CONTINUE reassembly of a continued string), and each cell of every supported kind (LABELSST/NUMBER/RK/BOOLERR/FORMULA+STRING/BLANK/MULBLANK) with its row/column, exact record span, and exact stored value spelling (the RK compact numeric encoding is decoded), on top of the reused CFBF/OLE2 physical layer",
  "declines": "a .xls with a plausible BIFF8 BOF but a malformed body (a truncated record, an SST count mismatch, an out-of-range BOUNDSHEET position, a malformed CONTINUE) detects as xls and its model declines typed InvalidXlsStructure (rc 53); a generic compound file, a .xls with a bad BOF version (BIFF5), and a .xls with an unrecognized BOF substream type stay cfbf and an xls selector declines typed (rc 6); prose stays opaque (rc 6); never a panic",
  "precedence": "the XLS detector is built on top of the CFBF detector and runs after the Word detector and before the generic Cfbf claim, so a .xls becomes Xls while a generic compound file stays Cfbf; a non-CFBF source (including a .xlsx OPC ZIP) is unaffected",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 53 invalid-xls-structure",
  "adr_0060": "the XlsModel node depends on the exact source root (DocumentExact, keyed by sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase26-2-1-xls-court.sh
# inside the court:
#   cargo build --locked --all-features
#   cargo run --locked --all-features --example xls_fixtures -- CORPUS
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX
#       (--metadata|--xls-text|--xls-biff|--xls-sheet 0|--xls-sheet 1|--xls-cell 2|--xls-cell 10|--xls-record 0|--xls-find PAT)
#       --kind metadata|text|exact
#   rm source F.voldoc
#   target/debug/vole-document materialize --store STORE --field HEX --exact --output OUT ; cmp OUT source
# Full gate (dev service):
#   cargo fmt --all --check
#   cargo clippy --all-targets --all-features -- -D warnings
#   cargo test --locked --all-features   (use CARGO_PROFILE_TEST_DEBUG=0 if the all-features link OOM-kills)
#   cargo test --locked --features xls
#   CFBF regression control: sh tools/phase21-35-1-cfbf-court.sh
#   DOC regression control: sh tools/phase26-1-1-doc-court.sh
#   MSRV: docker compose run --rm --no-TTY msrv cargo build --locked --all-features
EOF

# --- SUMMARY.md -------------------------------------------------------------
{
    echo "# Phase 26.2.1 — legacy Excel 97–2003 (\`.xls\`, BIFF8) court"
    echo
    echo "**Question.** Does a legacy Excel 97–2003 workbook close exactly and expose a"
    echo "representation-preserving semantic model (the BIFF8 record stream, the ordered"
    echo "\`BOUNDSHEET\` sheets, the \`SST\` shared-string table with \`CONTINUE\` reassembly, and"
    echo "each cell's row/column, exact record span, and exact stored value spelling, with"
    echo "the \`RK\` compact numeric encoding decoded) **on top of the reused CFBF/OLE2"
    echo "physical layer**, while staying honest at the detection boundary and declining"
    echo "malformed inputs typed?"
    echo
    echo "**Method.** Each fixture is generated by the dependency-free"
    echo "\`examples/xls_fixtures.rs\` writer, ingested via \`field-build\`, observed, then —"
    echo "after the **source file and the standalone descriptor are deleted** —"
    echo "rematerialized exactly in a fresh process and compared with \`length + SHA-256 +"
    echo "cmp\`. The CFBF, Opaque, and structural controls pin the detection boundaries. The"
    echo "court runs in the pinned \`dev\` service using only POSIX \`sh\`, coreutils, git, and"
    echo "the shipped binary (no python3, no Excel tooling, no network)."
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
    echo "- **Supported:** detection on top of the CFBF layer (the \`Workbook\`/\`Book\` stream"
    echo "  begins with a BIFF8 \`BOF\` — id \`0x0809\`, version \`0x0600\` — whose substream type"
    echo "  is a recognized worksheet/globals type); the record stream"
    echo "  (\`[u16 id][u16 len][payload]\`) with every record's exact byte span; the"
    echo "  \`BOF\`/\`EOF\` substream framing and the BIFF version; the \`BOUNDSHEET\` sheets (name"
    echo "  and \`BOF\` stream position, in order); the \`SST\` shared-string table (with"
    echo "  \`CONTINUE\` reassembly); the per-sheet cells"
    echo "  (\`LABELSST\`/\`NUMBER\`/\`RK\`/\`BOOLERR\`/\`FORMULA\`+\`STRING\`/\`BLANK\`/\`MULBLANK\`) with"
    echo "  their row/column, exact record span, and exact stored value spelling (the \`RK\`"
    echo "  compact numeric encoding is decoded); \`DIMENSIONS\`/\`ROW\`/\`FORMAT\`/\`XF\` recorded by"
    echo "  span; native \`xls-biff\`/\`xls-sheet\`/\`xls-cell\`/\`xls-record\`/\`xls-find\`; common"
    echo "  \`metadata\`/\`text\`/\`search-match\`."
    echo "- **Boundary:** a generic compound file, a \`.xls\` with a bad \`BOF\` version"
    echo "  (BIFF5/7), and a \`.xls\` with an unrecognized substream type stay \`Cfbf\`; prose"
    echo "  stays \`Opaque\`; a \`.xlsx\` (an OPC ZIP) never reaches the CFBF detector at all; a"
    echo "  \`.xls\` with a plausible BIFF8 \`BOF\` but a malformed body is admitted and its"
    echo "  model declines typed."
    echo "- **Declines:** a bad \`BOF\` version, an unrecognized substream type, a leading"
    echo "  substream that is not workbook globals, a truncated record, an out-of-range"
    echo "  \`BOUNDSHEET\` position, an \`SST\` count mismatch or a malformed \`CONTINUE\`, and a"
    echo "  \`FORMULA\` string result with no \`STRING\` record are typed declines"
    echo "  (\`InvalidXlsStructure\`)."
    echo "- **Not claimed here:** BIFF5/7 (an older \`Book\` stream with 1-byte string"
    echo "  lengths), password-encrypted/obfuscated workbooks, and the other legacy CFBF"
    echo "  semantic models (PPT/MSG, later subphases); the economic court (separate script)."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the XLS model"
    echo "  is derived and never on the exactness path (ADR-0060: the model node depends on"
    echo "  the \`sha256(source)\` root)."
    echo "- **Never run on the host:** every command above ran in the pinned \`dev\`"
    echo "  container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" != "PASS" ]; then
    echo "PHASE 26.2.1 XLS COURT: FAIL — campaign $CAMPAIGN (exact_ok=$exact_ok/$normal_n surface_fail=$surface_fail cells_ok=$cell_ok/$cell_n declines=$decline_after_detect_ok/$decline_n cfbf_ctl=$cfbf_ctl_ok/$cfbf_ctl_n opaque=$opaque_ok/$opaque_n)" >&2
    exit 1
fi
echo "PHASE 26.2.1 XLS COURT: PASS — campaign $CAMPAIGN"
