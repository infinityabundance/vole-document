#!/bin/sh
# Phase 21.18.1 — CBOR (RFC 8949, binary structured-tree Wave 2) surface + closure.
#
# Pre-registered (Phase-21 subphase 21.18.1). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture (including the malformed/Opaque
#       controls), `materialize(field) == source` (length + SHA-256 + cmp) after the
#       source file AND the standalone descriptor are deleted, in a fresh process.
#       The court FAILS unless this is 100% on every fixture.
#   H2 (surface + preserved representation) — the representation-preserving model is
#       exposed: every major type, the encoding width actually used (`0x17` vs
#       `0x1817`), byte string vs text string as distinct kinds, tag numbers (never
#       resolved), map order and duplicate keys, float width (half/single/double),
#       and definite vs indefinite-length items. Common metadata/text/search-match and
#       native cbor-pointer/cbor-node/cbor-find answer.
#   H3 (honest declines / boundaries) — an out-of-range pointer and a missing member
#       decline typed with exit code 6; a malformed pointer is a usage error (rc 2).
#       The controls pin the detection boundaries: `strict.json` stays `Json` (a
#       native CBOR selector on it declines typed, rc 6); a lone scalar, a truncated
#       item, a map key with no value, an unterminated item, a MessagePack fixarray/
#       fixmap, and plain prose stay `Opaque` with a typed common decline (rc 6).
#
# Runs in the pinned `dev` service (Rust toolchain + git + coreutils; deliberately
# NO python3 and NO jq, so the court depends only on the shipped binary, GNU
# `/usr/bin/printf` for the binary fixtures, and POSIX shell):
#   docker compose run --rm --no-TTY dev sh tools/phase21-18-1-cbor-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-18-1-cbor-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-18
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== generate fixtures (POSIX printf only) ==="
# GNU coreutils printf (invoked explicitly; dash's builtin does not do \xHH) writes
# the exact bytes. No python3, no jq.
mk() { /usr/bin/printf "$2" > "$1"; }

# --- NORMAL: detected as CBOR, full surface exercised -----------------------
# {"a": 1, "b": [1, 2], "c": true}
mk "$WORK/corpus/basic.cbor"     '\xa3\x61\x61\x01\x61\x62\x82\x01\x02\x61\x63\xf5'
# [23 (0x17), 23 (0x18 0x17)] — width must stay distinct
mk "$WORK/corpus/widths.cbor"    '\x82\x17\x18\x17'
# [1.5h, 1.0f, 1.0d]
mk "$WORK/corpus/floats.cbor"    '\x83\xf9\x3e\x00\xfa\x3f\x80\x00\x00\xfb\x3f\xf0\x00\x00\x00\x00\x00\x00'
# 55799({"t": 1(<uint>)}) — self-described tag + tag 1
mk "$WORK/corpus/tags.cbor"      '\xd9\xd9\xf7\xa1\x61\x74\xc1\x1a\x51\x4b\x67\x00'
# [h'010203', "abc"] — byte vs text string
mk "$WORK/corpus/bytestext.cbor" '\x82\x43\x01\x02\x03\x63\x61\x62\x63'
# {"a": 1, "a": 2} — duplicate keys
mk "$WORK/corpus/dupkeys.cbor"   '\xa2\x61\x61\x01\x61\x61\x02'
# [_ "ab", {_ "x": 1 }] — indefinite array/text/map
mk "$WORK/corpus/indef.cbor"     '\x9f\x7f\x61\x61\x61\x62\xff\xbf\x61\x78\x01\xff\xff'
# large.cbor: definite array(2000) of uint 1 in the 1-byte form (0x18 0x01)
big=""
i=0
while [ "$i" -lt 2000 ]; do big="${big}\\x18\\x01"; i=$((i + 1)); done
{ /usr/bin/printf '\x99\x07\xd0'; /usr/bin/printf "$big"; } > "$WORK/corpus/large.cbor"

# --- CONTROLS ---------------------------------------------------------------
# A strict JSON document: must stay `Json` (tried before CBOR).
/usr/bin/printf '{"a": 1, "b": [2, 3]}' > "$WORK/corpus/strict.json"
# Plain prose: never a container head byte, stays `Opaque`.
/usr/bin/printf 'The quick brown fox jumps over the lazy dog.\nPlain prose, not CBOR.\n' \
    > "$WORK/corpus/prose.txt"
# A lone inline scalar (23) and a lone one-byte scalar: structurally trivial.
mk "$WORK/corpus/single.cbor"  '\x17'
mk "$WORK/corpus/scalar.cbor"  '\x18\x17'
# Malformed: a map key with no value; an unterminated indefinite array.
mk "$WORK/corpus/badmap.cbor"  '\xa1\x61\x61\x61'
mk "$WORK/corpus/unterm.cbor"  '\x9f\x01\x02\x03'
# MessagePack fixarray(3) and fixmap(2): not a complete well-formed CBOR container.
mk "$WORK/corpus/msgpack_fixarray.bin" '\x93\x01\x02\x03'
mk "$WORK/corpus/msgpack_fixmap.bin"   '\x82\x01\x02\x03\x04'

ALL="basic.cbor widths.cbor floats.cbor tags.cbor bytestext.cbor dupkeys.cbor indef.cbor large.cbor strict.json single.cbor scalar.cbor badmap.cbor unterm.cbor msgpack_fixarray.bin msgpack_fixmap.bin prose.txt"
NORMAL="basic.cbor widths.cbor floats.cbor tags.cbor bytestext.cbor dupkeys.cbor indef.cbor large.cbor"
JSONCTL="strict.json"
OPAQUE="single.cbor scalar.cbor badmap.cbor unterm.cbor msgpack_fixarray.bin msgpack_fixmap.bin prose.txt"

ptr_for() {
    case "$1" in
        basic.cbor)     echo "/b/1" ;;
        widths.cbor)    echo "/1" ;;
        floats.cbor)    echo "/2" ;;
        tags.cbor)      echo "/t" ;;
        bytestext.cbor) echo "/1" ;;
        dupkeys.cbor)   echo "/a" ;;
        indef.cbor)     echo "/0" ;;
        large.cbor)     echo "/1999" ;;
        *)              echo "/" ;;
    esac
}
find_for() {
    case "$1" in
        tags.cbor)      echo "t" ;;
        bytestext.cbor) echo "abc" ;;
        dupkeys.cbor)   echo "a" ;;
        *)              echo "z" ;;
    esac
}
# An out-of-range pointer that must decline typed (rc 6). For arrays an
# out-of-range index; for maps a missing member.
decline_for() {
    case "$1" in
        large.cbor)  echo "/9999999" ;;
        widths.cbor|floats.cbor|bytestext.cbor|indef.cbor) echo "/99" ;;
        *)           echo "/nope" ;;
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
json_ctl_ok=0
usage_ok=0
opaque_ok=0
opaque_n=0
normal_n=0

for f in $ALL; do
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

    decline_rc=-1
    ctl_rc=-1

    if is_in "$f" "$NORMAL"; then
        normal_n=$((normal_n + 1))
        [ "$fmt" = "cbor" ] || { echo "FINDING: $f not detected as cbor (fmt=$fmt)" >&2; surface_fail=$((surface_fail + 1)); }

        # H2: common metadata / text + native pointer (exact + metadata) / node / find.
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --cbor-pointer "$(ptr_for "$f")" --kind exact \
            > "$RAW/$f.ptr.bin" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --cbor-pointer "$(ptr_for "$f")" --kind metadata \
            > "$RAW/$f.ptr.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --cbor-node "" --kind structure \
            > "$RAW/$f.node.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --cbor-find "$(find_for "$f")" --kind text \
            > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))
        [ -s "$RAW/$f.ptr.bin" ] || { echo "FINDING: $f pointer exact bytes empty" >&2; surface_fail=$((surface_fail + 1)); }

        # Common metadata must report the format and the root kind.
        grep -q '"format":"cbor"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata missing format" >&2; surface_fail=$((surface_fail + 1)); }

        # Per-fixture representation assertions.
        case "$f" in
            basic.cbor)
                grep -q '"top_type":"map"' "$RAW/$f.metadata.json" || { echo "FINDING: basic top_type not map" >&2; surface_fail=$((surface_fail + 1)); } ;;
            widths.cbor)
                "$BIN" observe --store "$WORK/store" --field "$field" --cbor-pointer /0 --kind metadata > "$RAW/$f.p0.json" 2>&1
                "$BIN" observe --store "$WORK/store" --field "$field" --cbor-pointer /1 --kind metadata > "$RAW/$f.p1.json" 2>&1
                grep -q '"info":23' "$RAW/$f.p0.json" || { echo "FINDING: widths /0 width not 23" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"info":24' "$RAW/$f.p1.json" || { echo "FINDING: widths /1 width not 24" >&2; surface_fail=$((surface_fail + 1)); } ;;
            floats.cbor)
                "$BIN" observe --store "$WORK/store" --field "$field" --cbor-pointer /0 --kind metadata > "$RAW/$f.p0.json" 2>&1
                "$BIN" observe --store "$WORK/store" --field "$field" --cbor-pointer /2 --kind metadata > "$RAW/$f.p2.json" 2>&1
                grep -q '"float":"half"' "$RAW/$f.p0.json" || { echo "FINDING: floats /0 not half" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"float":"double"' "$RAW/$f.p2.json" || { echo "FINDING: floats /2 not double" >&2; surface_fail=$((surface_fail + 1)); } ;;
            tags.cbor)
                grep -q '"kind":"tag"' "$RAW/$f.ptr.json" || { echo "FINDING: tags /t not a tag" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"tag":1' "$RAW/$f.ptr.json" || { echo "FINDING: tags /t tag number not 1" >&2; surface_fail=$((surface_fail + 1)); } ;;
            bytestext.cbor)
                "$BIN" observe --store "$WORK/store" --field "$field" --cbor-pointer /0 --kind metadata > "$RAW/$f.p0.json" 2>&1
                "$BIN" observe --store "$WORK/store" --field "$field" --cbor-pointer /1 --kind metadata > "$RAW/$f.p1.json" 2>&1
                grep -q '"kind":"bytes"' "$RAW/$f.p0.json" || { echo "FINDING: bytestext /0 not bytes" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"kind":"text"' "$RAW/$f.p1.json" || { echo "FINDING: bytestext /1 not text" >&2; surface_fail=$((surface_fail + 1)); } ;;
            dupkeys.cbor)
                grep -q '"matches":2' "$RAW/$f.ptr.json" || { echo "FINDING: dupkeys /a matches != 2" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"duplicate_keys":\["a"\]' "$RAW/$f.node.json" || { echo "FINDING: dupkeys node not reporting duplicate keys" >&2; surface_fail=$((surface_fail + 1)); } ;;
            indef.cbor)
                grep -q '"indefinite":true' "$RAW/$f.ptr.json" || { echo "FINDING: indef /0 not indefinite" >&2; surface_fail=$((surface_fail + 1)); } ;;
        esac

        # H3: an out-of-range pointer declines typed (rc 6).
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --cbor-pointer "$(decline_for "$f")" --kind metadata \
            > /dev/null 2>&1
        decline_rc=$?
        set -e
        [ "$decline_rc" -eq 6 ] && declines_ok=$((declines_ok + 1)) || { echo "FINDING: $f decline rc=$decline_rc (want 6)" >&2; }

    elif [ "$f" = "$JSONCTL" ]; then
        # H3: strict JSON stays `Json`; a native CBOR selector on it declines (rc 6).
        [ "$fmt" = "json" ] || { echo "FINDING: control $f detected as $fmt (want json)" >&2; surface_fail=$((surface_fail + 1)); }
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --cbor-pointer "/a" --kind metadata \
            > "$RAW/$f.cbor-decline.json" 2>&1
        ctl_rc=$?
        set -e
        [ "$ctl_rc" -eq 6 ] && json_ctl_ok=$((json_ctl_ok + 1)) || { echo "FINDING: strict JSON cbor selector rc=$ctl_rc (want 6)" >&2; }

    elif is_in "$f" "$OPAQUE"; then
        # H3: not detected as CBOR; the common observation declines typed (rc 6).
        [ "$fmt" = "opaque" ] || { echo "FINDING: control $f detected as $fmt (want opaque)" >&2; surface_fail=$((surface_fail + 1)); }
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1
        ctl_rc=$?
        set -e
        [ "$ctl_rc" -eq 6 ] && opaque_ok=$((opaque_ok + 1)) || { echo "FINDING: opaque control $f metadata rc=$ctl_rc (want 6)" >&2; }
        opaque_n=$((opaque_n + 1))
    fi

    # A malformed pointer is a usage error (rc 2) — checked once, on basic.cbor.
    if [ "$f" = "basic.cbor" ]; then
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --cbor-pointer "no-slash" --kind metadata \
            > /dev/null 2>&1
        u_rc=$?
        set -e
        [ "$u_rc" -eq 2 ] && usage_ok=$((usage_ok + 1)) || echo "FINDING: malformed pointer rc=$u_rc (want 2)" >&2
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
        "$decline_rc" "$ctl_rc" >> "$TSV"
done

echo "=== exactness after source + descriptor deletion ==="
cat "$TSV"

fixtures_n="$(printf '%s\n' $ALL | wc -w | tr -d ' ')"

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 21.18.1 CBOR court — matrix"
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
    echo "json_control_ok $json_ctl_ok"
    echo "usage_ok $usage_ok"
    echo "opaque_controls_ok $opaque_ok"
    echo "opaque_controls $opaque_n"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.18.1 — CBOR (RFC 8949) binary structured-tree surface + exact closure",
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
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
if [ "$exact_fail" -ne 0 ] || [ "$surface_fail" -ne 0 ] \
    || [ "$declines_ok" -ne "$normal_n" ] || [ "$json_ctl_ok" -ne 1 ] \
    || [ "$usage_ok" -ne 1 ] || [ "$opaque_ok" -ne "$opaque_n" ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.18.1 — CBOR (RFC 8949) binary structured-tree surface + exact closure",
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
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion, for every fixture including the Opaque controls",
  "observations": ["metadata", "text", "search-match", "native cbor-pointer", "native cbor-node", "native cbor-find"],
  "surface": "every major type; the encoding width actually used (0x17 vs 0x1817); byte string vs text string as distinct kinds; tag numbers preserved (never resolved); map order and duplicate keys; float width (half/single/double); definite vs indefinite-length items",
  "declines": "an out-of-range pointer and a missing member decline typed (rc 6); a malformed pointer is a usage error (rc 2); the strict-JSON control stays Json and a native CBOR selector on it declines typed; the lone-scalar, truncated, map-key-without-value, unterminated, MessagePack fixarray/fixmap, and prose controls stay Opaque and a common observation declines typed (rc 6), never a panic",
  "detection_boundary": "CBOR has no magic bytes, so detection is conservative: the self-described-CBOR tag 55799 (0xd9 0xd9 0xf7), or a full-input well-formed parse whose root is a container/tag and which reaches at least three nodes. A container head byte is always >= 0x80, so no pure-ASCII document is claimed. Cannot distinguish MessagePack's whole-number/short-container prefix overlap; ambiguous or trivial inputs fall back to Opaque rather than being guessed (a Phase-21.19 MessagePack adapter must share the seam).",
  "precedence": "CBOR is tried after the strong magic-byte binaries (PDF/ZIP/Parquet/Arrow) and after the JSON family (JSON/JSON5/JSONL), and before the remaining textual heuristics (EML/YAML/TOML/CSV/Markdown/XML/HTML)",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 32 invalid-cbor-structure",
  "adr_0060": "the CborModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase21-18-1-cbor-court.sh
# inside the court:
#   cargo build --locked --all-features
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--cbor-pointer P|--cbor-node ""|--cbor-find PAT) --kind metadata|text|structure|exact
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
    echo "# Phase 21.18.1 — CBOR (RFC 8949) court"
    echo
    echo "**Question.** Does the binary structured-tree CBOR format close exactly and"
    echo "expose a representation-preserving model (every major type, the encoding width"
    echo "actually used, byte-vs-text strings, tag numbers never resolved, map order and"
    echo "duplicate keys, float width, and definite/indefinite-length items) on top of the"
    echo "whole-source exact leaf — while keeping the conservative no-magic-byte detection"
    echo "boundary and declining malformed/ambiguous inputs typed?"
    echo
    echo "**Method.** Each self-authored fixture is generated in-court with GNU"
    echo "\`/usr/bin/printf\`, ingested via \`field-build\`, observed, then — after the"
    echo "**source file and the standalone descriptor are deleted** — rematerialized"
    echo "exactly in a fresh process and compared with \`length + SHA-256 + cmp\`. An"
    echo "out-of-range pointer is required to decline typed; the strict-JSON,"
    echo "lone-scalar, truncated, map-key-without-value, unterminated, MessagePack, and"
    echo "prose controls pin the detection boundaries. The court runs in the pinned"
    echo "\`dev\` service using only POSIX \`sh\`, coreutils, git, and the shipped binary"
    echo "(no python3, no jq)."
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
    echo "- **Shipped here:** byte-based conservative CBOR detection; the bounded CBOR"
    echo "  parser producing a representation-preserving arena (kind, exact span, encoding"
    echo "  width, tag number, float width, definite/indefinite form, ordered children);"
    echo "  native \`cbor-pointer\`/\`cbor-node\`/\`cbor-find\`; common"
    echo "  \`metadata\`/\`text\`/\`search-match\`."
    echo "- **Precedence:** after the strong magic-byte binaries (PDF/ZIP/Parquet/Arrow)"
    echo "  and the JSON family (JSON/JSON5/JSONL), before the textual heuristics."
    echo "- **Detection boundary:** CBOR has **no magic bytes**. An input is claimed only"
    echo "  on the self-described-CBOR tag \`55799\`, or a full-input well-formed parse"
    echo "  whose root is a container/tag reaching at least three nodes; a container head"
    echo "  byte is always \`>= 0x80\`, so no pure-ASCII document is ever claimed. A lone"
    echo "  scalar, a truncated item, a map key with no value, an unterminated item, a"
    echo "  MessagePack source, and prose all stay \`Opaque\` rather than being guessed."
    echo "- **Recorded negative:** the whole-number/short-container prefix overlaps"
    echo "  MessagePack's \`fixint\`/\`fixarray\` encodings; the two cannot always be told"
    echo "  apart, so ambiguous inputs are not guessed. A Phase-21.19 MessagePack adapter"
    echo "  must share this seam."
    echo "- **Declines:** a malformed head/value/map/string and an out-of-range pointer"
    echo "  are typed (\`InvalidCborStructure\` rc 32, or unsupported-feature rc 6); such"
    echo "  input stays \`Opaque\`."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the CBOR"
    echo "  model is derived (\`Q_gen\`) and never on the exactness path (ADR-0060: the"
    echo "  model node depends on the \`sha256(source)\` root)."
    echo "- **Not claimed here:** the economic court (separate script)."
    echo "- **Never run on the host:** every command above ran in the pinned \`dev\`"
    echo "  container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" != "PASS" ]; then
    echo "PHASE 21.18.1 CBOR COURT: FAIL — campaign $CAMPAIGN (exact_fail=$exact_fail surface_fail=$surface_fail declines_ok=$declines_ok json_ctl_ok=$json_ctl_ok usage_ok=$usage_ok opaque_ok=$opaque_ok/$opaque_n)" >&2
    exit 1
fi
echo "PHASE 21.18.1 CBOR COURT: PASS — campaign $CAMPAIGN"
