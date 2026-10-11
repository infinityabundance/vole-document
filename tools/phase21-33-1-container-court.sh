#!/bin/sh
# Phase 21.33 — generic archive-container (ZIP / TAR) surface + exact closure.
#
# Pre-registered (Phase-21 subphase 21.33.1). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture, `materialize(field) == source`
#       (length + SHA-256 + cmp) after the source file AND the standalone
#       descriptor are deleted, in a fresh process. The court FAILS unless this is
#       100% on every fixture.
#   H2 (surface + preserved representation) — the representation-preserving model
#       is exposed: the ordered member list with raw names (incl. non-UTF-8), the
#       ZIP compression method (stored/deflate), both declared sizes, the CRC-32,
#       the exact member spans; the TAR member `typeflag`/mode/linkname and the
#       GNU long-name (`L`)/PAX (`x`) extensions folded in; the recorded dialect
#       (`zip`/`tar`). Common metadata/text/search-match and native
#       container-dialect/container-entry/container-find answer.
#   H3 (honest declines / boundaries) — the controls pin the detection boundaries:
#       an office `.docx`/`.odt` source still detects as its office format and a
#       container selector on it declines typed; a random `PK`-junk blob, prose, a
#       ZIP with a bad EOCD, a truncated ZIP, and a TAR with a bad checksum stay
#       `Opaque`; a truncated TAR (valid first header, truncated member) detects as
#       `container` but a container observation declines typed (InvalidContainer-
#       Structure). Never a panic, never a silent empty answer.
#
# Runs in the pinned `dev` service (Rust toolchain + git + coreutils + sed/grep +
# GNU tar; deliberately NO python3, NO `zip`:
#   docker compose run --rm --no-TTY dev sh tools/phase21-33-1-container-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-33-1-container-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-33
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== generate fixtures (dependency-free ZIP/TAR writer) ==="
cargo run --locked --all-features --example container_fixtures -- "$WORK/corpus" 2>&1 | tail -1

# Fixture classes.
#   NORMAL  — plain containers: H1 exactness + H2 surface.
#   ENTRY_OK / ENTRY_DECLINE — the native container-entry index behaviour.
#   OFFICE  — the office ZIP family must be detected first (NAME:FORMAT).
#   OPAQUE  — not detected as a container; a container selector declines typed.
#   DECLINE — detects as `container` but the model is structurally malformed.
NORMAL="zip_stored.zip zip_multi.zip zip_nonutf8.zip zip_empty.zip tar_ustar.tar tar_extended.tar"
ENTRY_OK="zip_stored.zip zip_multi.zip zip_nonutf8.zip tar_ustar.tar tar_extended.tar"
ENTRY_DECLINE="zip_empty.zip"
OFFICE="office.docx:docx office.odt:odt"
OPAQUE="pk_junk.bin prose.txt bad_eocd.zip bad_checksum.tar truncated.zip"
DECLINE="truncated.tar"

dialect_for() {
    case "$1" in
        tar_ustar.tar|tar_extended.tar|truncated.tar) echo "tar" ;;
        *) echo "zip" ;;
    esac
}
entry_name_for() {
    case "$1" in
        zip_stored.zip|tar_ustar.tar) echo "hello.txt" ;;
        zip_multi.zip) echo "a.txt" ;;
        zip_nonutf8.zip) echo "caf" ;;
        tar_extended.tar) echo "this/is/a/very/long" ;;
        *) echo "" ;;
    esac
}
find_for() {
    case "$1" in
        zip_stored.zip) echo "hello" ;;
        zip_multi.zip) echo "alpha" ;;
        zip_nonutf8.zip) echo "caf" ;;
        tar_ustar.tar) echo "nested" ;;
        tar_extended.tar) echo "overridden" ;;
        zip_empty.zip) echo "z" ;;
        *) echo "z" ;;
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
declines_ok=0
office_ok=0
office_n=0
opaque_ok=0
opaque_n=0
normal_n=0
entry_ok=0
entry_n=0
entry_decline_ok=0
decline_after_detect_ok=0

for spec in $OFFICE; do
    f="${spec%%:*}"
    want="${spec##*:}"
    src="$WORK/ref/$f"
    cp "$WORK/corpus/$f" "$src"
    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    fmt="$(json_str "$build_json" format)"
    field="$(json_str "$build_json" field)"
    office_n=$((office_n + 1))
    [ "$fmt" = "$want" ] || { echo "FINDING: office control $f detected as $fmt (want $want)" >&2; surface_fail=$((surface_fail + 1)); }
    # A container selector on an office package must decline typed (rc 6).
    set +e
    "$BIN" observe --store "$WORK/store" --field "$field" --container-dialect --kind text \
        > "$RAW/$f.container-decline.txt" 2>&1
    ctl_rc=$?
    set -e
    if [ "$ctl_rc" -eq 6 ]; then office_ok=$((office_ok + 1)); else echo "FINDING: office control $f container selector rc=$ctl_rc (want 6)" >&2; fi
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
    "$BIN" observe --store "$WORK/store" --field "$field" --container-dialect --kind text \
        > "$RAW/$f.container-decline.txt" 2>&1
    ctl_rc=$?
    set -e
    if [ "$ctl_rc" -eq 6 ]; then opaque_ok=$((opaque_ok + 1)); else echo "FINDING: opaque control $f container selector rc=$ctl_rc (want 6)" >&2; fi
    printf '%s\t%s\t-\t-\t-\t-\t-\t-\t-\t%s\n' "$f" "$fmt" "$ctl_rc" >> "$TSV"
done

# A truncated TAR detects as `container` (its first header is valid) but its member
# is structurally incomplete, so a container observation declines typed (rc 49).
for f in $DECLINE; do
    src="$WORK/ref/$f"
    cp "$WORK/corpus/$f" "$src"
    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    fmt="$(json_str "$build_json" format)"
    field="$(json_str "$build_json" field)"
    [ "$fmt" = "container" ] || { echo "FINDING: $f detected as $fmt (want container)" >&2; surface_fail=$((surface_fail + 1)); }
    set +e
    "$BIN" observe --store "$WORK/store" --field "$field" --container-dialect --kind text \
        > "$RAW/$f.decline.txt" 2>&1
    ctl_rc=$?
    set -e
    if [ "$ctl_rc" -eq 49 ]; then decline_after_detect_ok=$((decline_after_detect_ok + 1)); else echo "FINDING: $f structural decline rc=$ctl_rc (want 49)" >&2; fi
    printf '%s\t%s\t-\t-\t-\t-\t-\t-\t-\t%s\n' "$f" "$fmt" "$ctl_rc" >> "$TSV"
done

for f in $NORMAL; do
    # The reference copy is never deleted; the corpus copy is what is ingested and
    # then removed (so the identity comparison is against retained bytes).
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
    [ "$fmt" = "container" ] || { echo "FINDING: $f not detected as container (fmt=$fmt)" >&2; surface_fail=$((surface_fail + 1)); }

    # H2: common metadata / text / search-match + native dialect / entry / find.
    "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
        > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
        > "$RAW/$f.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --container-dialect --kind text \
        > "$RAW/$f.dialect.txt" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --container-find "$(find_for "$f")" --kind metadata \
        > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))

    grep -q '"format":"container"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata missing format" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q "\"dialect\":\"$want_dialect\"" "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata dialect != $want_dialect" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q "\"text\":\"$want_dialect\"" "$RAW/$f.dialect.txt" || { echo "FINDING: $f dialect text != $want_dialect" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"matches":\[' "$RAW/$f.find.json" || { echo "FINDING: $f find returned no matches" >&2; surface_fail=$((surface_fail + 1)); }

    # H2: the native member index.
    if is_in "$f" "$ENTRY_OK"; then
        entry_n=$((entry_n + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --container-entry 0 --kind exact \
            > "$RAW/$f.entry0.bin" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --container-entry 0 --kind metadata \
            > "$RAW/$f.entry0.json" 2>&1 || surface_fail=$((surface_fail + 1))
        name="$(entry_name_for "$f")"
        if [ -n "$name" ]; then
            grep -q "\"name\":\"$name" "$RAW/$f.entry0.json" || { echo "FINDING: $f entry0 name != $name" >&2; surface_fail=$((surface_fail + 1)); }
        fi
        [ -s "$RAW/$f.entry0.bin" ] || { echo "FINDING: $f entry0 exact bytes empty" >&2; surface_fail=$((surface_fail + 1)); }
        entry_ok=$((entry_ok + 1))
    fi
    # An out-of-range member index on the empty archive declines typed (rc 6).
    if is_in "$f" "$ENTRY_DECLINE"; then
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --container-entry 0 --kind metadata \
            > "$RAW/$f.entry0-decline.txt" 2>&1
        ctl_rc=$?
        set -e
        if [ "$ctl_rc" -eq 6 ]; then entry_decline_ok=$((entry_decline_ok + 1)); else echo "FINDING: $f entry0 rc=$ctl_rc (want 6)" >&2; fi
    fi

    # H1: delete the source AND the standalone descriptor, then rematerialize
    # exactly in a fresh process and compare (length + SHA-256 + cmp).
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

fixtures_n=$((normal_n + office_n + opaque_n))

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 21.33.1 archive-container court — matrix"
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
    echo "typed_declines_ok $declines_ok"
    echo "native_entry_ok $entry_ok"
    echo "native_entry_n $entry_n"
    echo "native_entry_decline_ok $entry_decline_ok"
    echo "office_controls_ok $office_ok"
    echo "office_controls $office_n"
    echo "opaque_controls_ok $opaque_ok"
    echo "opaque_controls $opaque_n"
    echo "structural_decline_ok $decline_after_detect_ok"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.33.1 — generic archive-container (ZIP/TAR) surface + exact closure",
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
  "fixtures_tool": "examples/container_fixtures.rs (dependency-free ZIP/TAR writer; no python3, no zip)",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
if [ "$exact_fail" -ne 0 ] || [ "$surface_fail" -ne 0 ] \
    || [ "$exact_ok" -ne "$normal_n" ] \
    || [ "$entry_ok" -ne "$entry_n" ] \
    || [ "$entry_decline_ok" -ne 1 ] \
    || [ "$office_ok" -ne "$office_n" ] \
    || [ "$opaque_ok" -ne "$opaque_n" ] \
    || [ "$decline_after_detect_ok" -ne 1 ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.33.1 — generic archive-container (ZIP/TAR) surface + exact closure",
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
  "fixtures": "$NORMAL $OFFICE $OPAQUE $DECLINE",
  "fixture_tool": "examples/container_fixtures.rs",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion",
  "observations": ["metadata", "text", "search-match", "native container-dialect", "native container-entry", "native container-find"],
  "surface": "ordered member list with raw names (incl. non-UTF-8), ZIP method (stored/deflate), both declared sizes, CRC-32, exact member spans; TAR typeflag/mode/linkname with GNU long-name (L) and PAX (x) extensions folded in; recorded dialect (zip/tar)",
  "declines": "an out-of-range member index declines typed (rc 6); a container selector on an office package or an Opaque control declines typed (rc 6); a truncated TAR (valid first header, truncated member) detects as container and its model declines typed InvalidContainerStructure (rc 49); never a panic",
  "precedence": "the office ZIP family (DOCX/EPUB/ODT/ODS/ODP/XLSX/PPTX) is detected first; the generic container is inserted after it and before every weak, no-magic-byte heuristic, so an office package is never reclassified as a plain archive",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 49 invalid-container-structure",
  "adr_0060": "the ContainerModel node depends on the exact source root (DocumentExact for TAR, PackageRoot for ZIP); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase21-33-1-container-court.sh
# inside the court:
#   cargo build --locked --all-features
#   cargo run --locked --all-features --example container_fixtures -- CORPUS
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--container-dialect|--container-entry 0|--container-find PAT) --kind metadata|text|exact
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
    echo "# Phase 21.33.1 — generic archive-container (ZIP / TAR) court"
    echo
    echo "**Question.** Does a standalone ZIP or TAR archive close exactly and expose a"
    echo "representation-preserving member model (the ordered member list with raw names"
    echo "incl. non-UTF-8, the ZIP method, both declared sizes, CRC-32, and exact spans;"
    echo "the TAR typeflag/mode/linkname with the GNU long-name and PAX extensions folded"
    echo "in; and a recorded zip/tar dialect) on top of the whole-source exact leaf —"
    echo "while keeping the office-ZIP-family boundary and declining malformed inputs"
    echo "typed?"
    echo
    echo "**Method.** Each fixture is generated by the dependency-free"
    echo "\`examples/container_fixtures.rs\` writer, ingested via \`field-build\`,"
    echo "observed, then — after the **source file and the standalone descriptor are"
    echo "deleted** — rematerialized exactly in a fresh process and compared with"
    echo "\`length + SHA-256 + cmp\`. The office-format, Opaque, and structural controls"
    echo "pin the detection boundaries. The court runs in the pinned \`dev\` service using"
    echo "only POSIX \`sh\`, coreutils, git, and the shipped binary (no python3, no zip)."
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
    echo "- **Shipped here:** strong-magic + validation detection (ZIP = \`PK\\x03\\x04\` at"
    echo "  offset 0 or the empty-archive \`PK\\x05\\x06\` with a valid EOCD and coherent"
    echo "  central directory; TAR = a valid \`ustar\`/old-GNU header at 257 or a plausible"
    echo "  v7 header at 0 whose checksum matches); the reused byte-authoritative ZIP"
    echo "  physical layer; a bounded TAR parser; native"
    echo "  \`container-dialect\`/\`container-entry\`/\`container-find\`; common"
    echo "  \`metadata\`/\`text\`/\`search-match\`; a recorded dialect."
    echo "- **Precedence:** the office ZIP family is detected first; the generic container"
    echo "  is inserted after it and before every weak, no-magic-byte heuristic."
    echo "- **Declines:** a malformed EOCD, a truncated ZIP, a TAR with a bad header"
    echo "  checksum, and a structurally truncated TAR are typed declines; a bad/short"
    echo "  input stays \`Opaque\` rather than being claimed."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the container"
    echo "  model is derived and never on the exactness path (ADR-0060: the model node"
    echo "  depends on the \`sha256(source)\` root)."
    echo "- **Not claimed here:** office-package semantics (separate adapters); the"
    echo "  economic court (separate script)."
    echo "- **Never run on the host:** every command above ran in the pinned \`dev\`"
    echo "  container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" != "PASS" ]; then
    echo "PHASE 21.33.1 CONTAINER COURT: FAIL — campaign $CAMPAIGN (exact_ok=$exact_ok/$normal_n surface_fail=$surface_fail entry_ok=$entry_ok entry_decline=$entry_decline_ok office=$office_ok/$office_n opaque=$opaque_ok/$opaque_n structural=$decline_after_detect_ok)" >&2
    exit 1
fi
echo "PHASE 21.33.1 CONTAINER COURT: PASS — campaign $CAMPAIGN"
