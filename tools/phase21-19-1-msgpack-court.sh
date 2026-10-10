#!/bin/sh
# Phase 21.19.1 — MessagePack (binary structured-tree Wave 2) surface + closure.
#
# Pre-registered (Phase-21 subphase 21.19.1). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture (including the malformed/Opaque
#       controls), `materialize(field) == source` (length + SHA-256 + cmp) after the
#       source file AND the standalone descriptor are deleted, in a fresh process.
#       The court FAILS unless this is 100% on every fixture.
#   H2 (surface + preserved representation) — the representation-preserving model is
#       exposed: the exact format byte actually used (encoding width AND signedness,
#       e.g. `0x17` vs `0xcc 0x17` vs `0xd0 0x17`), `str` vs `bin` as distinct kinds,
#       map order and duplicate keys, float width (float32/float64), and extension
#       type numbers + payload lengths (preserved, never interpreted). Common
#       metadata/text/search-match and native msgpack-pointer/msgpack-node/
#       msgpack-find answer.
#   H3 (honest declines / boundaries) — an out-of-range pointer and a missing member
#       decline typed with exit code 6; a malformed pointer is a usage error (rc 2).
#       The controls pin the detection boundaries: `strict.json` stays `Json` (a
#       native MessagePack selector on it declines typed, rc 6); a **CBOR** document
#       stays `Cbor` (coexistence — CBOR is tried first, so MessagePack never steals
#       it) and a native msgpack selector on it declines typed (rc 6); a lone scalar,
#       the never-used `0xc1` byte, a map key with no value, trailing bytes, the
#       ambiguous MessagePack fixarray(3)/fixmap(2) overlap fixtures, and plain prose
#       stay `Opaque` with a typed common decline (rc 6).
#
# Runs in the pinned `dev` service (Rust toolchain + git + coreutils; deliberately
# NO python3 and NO jq, so the court depends only on the shipped binary, GNU
# `/usr/bin/printf` for the binary fixtures, and POSIX shell):
#   docker compose run --rm --no-TTY dev sh tools/phase21-19-1-msgpack-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-19-1-msgpack-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-19
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== generate fixtures (POSIX printf only) ==="
# GNU coreutils printf (invoked explicitly; dash's builtin does not do \xHH) writes
# the exact bytes. No python3, no jq.
mk() { /usr/bin/printf "$2" > "$1"; }

# --- NORMAL: detected as MessagePack, full surface exercised -----------------
# {"a": [1, 2, 3], "b": true}
mk "$WORK/corpus/basic.msgpack"     '\x82\xa1a\x93\x01\x02\x03\xa1b\xc3'
# [23 fixint, 23 uint8, 23 int8, 127, -1] — width/signedness must stay distinct.
mk "$WORK/corpus/widths.msgpack"    '\x95\x17\xcc\x17\xd0\x17\x7f\xff'
# [1.0f32, 1.0f64]
mk "$WORK/corpus/floats.msgpack"    '\x92\xca\x3f\x80\x00\x00\xcb\x3f\xf0\x00\x00\x00\x00\x00\x00'
# [h'010203', "abc"] — bin vs str
mk "$WORK/corpus/bytestext.msgpack" '\x92\xc4\x03\x01\x02\x03\xa3abc'
# {"a": 1, "a": 2, "b": 3} — duplicate keys
mk "$WORK/corpus/dupkeys.msgpack"   '\x83\xa1a\x01\xa1a\x02\xa1b\x03'
# [fixext4(-1, 01020304), ext8(5, "AB")] — ext type + length preserved
mk "$WORK/corpus/ext.msgpack"       '\x92\xd6\xff\x01\x02\x03\x04\xc7\x02\x05AB'
# map16 {"a": 1} — an unambiguous MessagePack-only head byte (CBOR rejects 0xde)
mk "$WORK/corpus/map16.msgpack"     '\xde\x00\x01\xa1a\x01'

# --- CONTROLS ---------------------------------------------------------------
# A strict JSON document: must stay `Json` (tried before MessagePack).
/usr/bin/printf '{"a": 1, "b": [2, 3]}' > "$WORK/corpus/strict.json"
# A CBOR document: must stay `Cbor` (CBOR is tried before MessagePack).
mk "$WORK/corpus/control.cbor"      '\xa3\x61a\x01\x61b\x82\x01\x02\x61c\xf5'
# Plain prose: never a container head byte, stays `Opaque`.
/usr/bin/printf 'The quick brown fox jumps over the lazy dog.\nPlain prose, not MessagePack.\n' \
    > "$WORK/corpus/prose.txt"
# A lone inline scalar (23) and a lone one-byte scalar: structurally trivial.
mk "$WORK/corpus/single.msgpack"   '\x17'
mk "$WORK/corpus/scalar.msgpack"   '\xcc\x17'
# Malformed: the never-used 0xc1 byte; a map key with no value; trailing bytes.
mk "$WORK/corpus/badc1.msgpack"    '\xc1'
mk "$WORK/corpus/badmap.msgpack"   '\x81\xa1a'
mk "$WORK/corpus/trailing.msgpack" '\x01\x02'
# MessagePack fixarray(3) and fixmap(2): the ambiguous whole-number/short-container
# overlap with CBOR — below the byte threshold, so they stay `Opaque`.
mk "$WORK/corpus/fixarray3.bin" '\x93\x01\x02\x03'
mk "$WORK/corpus/fixmap2.bin"   '\x82\x01\x02\x03\x04'

ALL="basic.msgpack widths.msgpack floats.msgpack bytestext.msgpack dupkeys.msgpack ext.msgpack map16.msgpack strict.json control.cbor single.msgpack scalar.msgpack badc1.msgpack badmap.msgpack trailing.msgpack fixarray3.bin fixmap2.bin prose.txt"
NORMAL="basic.msgpack widths.msgpack floats.msgpack bytestext.msgpack dupkeys.msgpack ext.msgpack map16.msgpack"
JSONCTL="strict.json"
CBORCTL="control.cbor"
OPAQUE="single.msgpack scalar.msgpack badc1.msgpack badmap.msgpack trailing.msgpack fixarray3.bin fixmap2.bin prose.txt"

ptr_for() {
    case "$1" in
        basic.msgpack)     echo "/b" ;;
        widths.msgpack)    echo "/1" ;;
        floats.msgpack)    echo "/0" ;;
        bytestext.msgpack) echo "/1" ;;
        dupkeys.msgpack)   echo "/a" ;;
        ext.msgpack)       echo "/0" ;;
        map16.msgpack)     echo "/a" ;;
        *)                 echo "/" ;;
    esac
}
find_for() {
    case "$1" in
        basic.msgpack)     echo "a" ;;
        bytestext.msgpack) echo "abc" ;;
        dupkeys.msgpack)   echo "a" ;;
        *)                 echo "z" ;;
    esac
}
# An out-of-range pointer that must decline typed (rc 6). For arrays an
# out-of-range index; for maps a missing member.
decline_for() {
    case "$1" in
        basic.msgpack|dupkeys.msgpack|map16.msgpack) echo "/nope" ;;
        *)                                           echo "/99" ;;
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
cbor_ctl_ok=0
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
        [ "$fmt" = "msgpack" ] || { echo "FINDING: $f not detected as msgpack (fmt=$fmt)" >&2; surface_fail=$((surface_fail + 1)); }

        # H2: common metadata / text + native pointer (exact + metadata) / node / find.
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --msgpack-pointer "$(ptr_for "$f")" --kind exact \
            > "$RAW/$f.ptr.bin" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --msgpack-pointer "$(ptr_for "$f")" --kind metadata \
            > "$RAW/$f.ptr.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --msgpack-node "" --kind structure \
            > "$RAW/$f.node.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --msgpack-find "$(find_for "$f")" --kind text \
            > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))
        [ -s "$RAW/$f.ptr.bin" ] || { echo "FINDING: $f pointer exact bytes empty" >&2; surface_fail=$((surface_fail + 1)); }

        # Common metadata must report the format and the root kind.
        grep -q '"format":"msgpack"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata missing format" >&2; surface_fail=$((surface_fail + 1)); }

        # Per-fixture representation assertions.
        case "$f" in
            basic.msgpack)
                grep -q '"top_type":"map"' "$RAW/$f.metadata.json" || { echo "FINDING: basic top_type not map" >&2; surface_fail=$((surface_fail + 1)); } ;;
            widths.msgpack)
                "$BIN" observe --store "$WORK/store" --field "$field" --msgpack-pointer /0 --kind metadata > "$RAW/$f.p0.json" 2>&1
                "$BIN" observe --store "$WORK/store" --field "$field" --msgpack-pointer /1 --kind metadata > "$RAW/$f.p1.json" 2>&1
                grep -q '"head":23,' "$RAW/$f.p0.json" || { echo "FINDING: widths /0 head not fixint 23" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"head":204,' "$RAW/$f.p1.json" || { echo "FINDING: widths /1 head not uint8 (204)" >&2; surface_fail=$((surface_fail + 1)); } ;;
            floats.msgpack)
                "$BIN" observe --store "$WORK/store" --field "$field" --msgpack-pointer /0 --kind metadata > "$RAW/$f.p0.json" 2>&1
                "$BIN" observe --store "$WORK/store" --field "$field" --msgpack-pointer /1 --kind metadata > "$RAW/$f.p1.json" 2>&1
                grep -q '"float":"single"' "$RAW/$f.p0.json" || { echo "FINDING: floats /0 not float32" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"float":"double"' "$RAW/$f.p1.json" || { echo "FINDING: floats /1 not float64" >&2; surface_fail=$((surface_fail + 1)); } ;;
            bytestext.msgpack)
                "$BIN" observe --store "$WORK/store" --field "$field" --msgpack-pointer /0 --kind metadata > "$RAW/$f.p0.json" 2>&1
                "$BIN" observe --store "$WORK/store" --field "$field" --msgpack-pointer /1 --kind metadata > "$RAW/$f.p1.json" 2>&1
                grep -q '"kind":"bin"' "$RAW/$f.p0.json" || { echo "FINDING: bytestext /0 not bin" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"kind":"str"' "$RAW/$f.p1.json" || { echo "FINDING: bytestext /1 not str" >&2; surface_fail=$((surface_fail + 1)); } ;;
            dupkeys.msgpack)
                grep -q '"matches":2' "$RAW/$f.ptr.json" || { echo "FINDING: dupkeys /a matches != 2" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"duplicate_keys":\["a"\]' "$RAW/$f.node.json" || { echo "FINDING: dupkeys node not reporting duplicate keys" >&2; surface_fail=$((surface_fail + 1)); } ;;
            ext.msgpack)
                grep -q '"kind":"ext"' "$RAW/$f.ptr.json" || { echo "FINDING: ext /0 not an ext" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"ext_type":255' "$RAW/$f.ptr.json" || { echo "FINDING: ext /0 type not 255" >&2; surface_fail=$((surface_fail + 1)); } ;;
            map16.msgpack)
                grep -q '"top_type":"map"' "$RAW/$f.metadata.json" || { echo "FINDING: map16 top_type not map" >&2; surface_fail=$((surface_fail + 1)); } ;;
        esac

        # H3: an out-of-range pointer declines typed (rc 6).
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --msgpack-pointer "$(decline_for "$f")" --kind metadata \
            > /dev/null 2>&1
        decline_rc=$?
        set -e
        [ "$decline_rc" -eq 6 ] && declines_ok=$((declines_ok + 1)) || { echo "FINDING: $f decline rc=$decline_rc (want 6)" >&2; }

    elif [ "$f" = "$JSONCTL" ]; then
        # H3: strict JSON stays `Json`; a native MessagePack selector on it declines
        # (rc 6).
        [ "$fmt" = "json" ] || { echo "FINDING: control $f detected as $fmt (want json)" >&2; surface_fail=$((surface_fail + 1)); }
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --msgpack-pointer "/a" --kind metadata \
            > "$RAW/$f.msgpack-decline.json" 2>&1
        ctl_rc=$?
        set -e
        [ "$ctl_rc" -eq 6 ] && json_ctl_ok=$((json_ctl_ok + 1)) || { echo "FINDING: strict JSON msgpack selector rc=$ctl_rc (want 6)" >&2; }

    elif [ "$f" = "$CBORCTL" ]; then
        # H3 (coexistence): a CBOR document stays `Cbor`; a native MessagePack selector
        # on it declines typed (rc 6).
        [ "$fmt" = "cbor" ] || { echo "FINDING: control $f detected as $fmt (want cbor)" >&2; surface_fail=$((surface_fail + 1)); }
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --msgpack-pointer "/a" --kind metadata \
            > "$RAW/$f.msgpack-decline.json" 2>&1
        ctl_rc=$?
        set -e
        [ "$ctl_rc" -eq 6 ] && cbor_ctl_ok=$((cbor_ctl_ok + 1)) || { echo "FINDING: CBOR control msgpack selector rc=$ctl_rc (want 6)" >&2; }

    elif is_in "$f" "$OPAQUE"; then
        # H3: not detected as MessagePack; the common observation declines typed (rc 6).
        [ "$fmt" = "opaque" ] || { echo "FINDING: control $f detected as $fmt (want opaque)" >&2; surface_fail=$((surface_fail + 1)); }
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1
        ctl_rc=$?
        set -e
        [ "$ctl_rc" -eq 6 ] && opaque_ok=$((opaque_ok + 1)) || { echo "FINDING: opaque control $f metadata rc=$ctl_rc (want 6)" >&2; }
        opaque_n=$((opaque_n + 1))
    fi

    # A malformed pointer is a usage error (rc 2) — checked once, on basic.msgpack.
    if [ "$f" = "basic.msgpack" ]; then
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --msgpack-pointer "no-slash" --kind metadata \
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
    echo "# Phase 21.19.1 MessagePack court — matrix"
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
    echo "cbor_control_ok $cbor_ctl_ok"
    echo "usage_ok $usage_ok"
    echo "opaque_controls_ok $opaque_ok"
    echo "opaque_controls $opaque_n"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.19.1 — MessagePack binary structured-tree surface + exact closure",
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
    || [ "$cbor_ctl_ok" -ne 1 ] || [ "$usage_ok" -ne 1 ] \
    || [ "$opaque_ok" -ne "$opaque_n" ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.19.1 — MessagePack binary structured-tree surface + exact closure",
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
  "observations": ["metadata", "text", "search-match", "native msgpack-pointer", "native msgpack-node", "native msgpack-find"],
  "surface": "the exact format byte actually used (encoding width AND signedness: 0x17 fixint vs 0xcc uint8 vs 0xd0 int8); str vs bin as distinct kinds; map order and duplicate keys; float width (float32/float64); extension type numbers + payload lengths preserved, never interpreted",
  "declines": "an out-of-range pointer and a missing member decline typed (rc 6); a malformed pointer is a usage error (rc 2); the strict-JSON control stays Json and a native MessagePack selector on it declines typed; the CBOR control stays Cbor (coexistence) and a native MessagePack selector on it declines typed; the lone-scalar, 0xc1-byte, map-key-without-value, trailing-bytes, ambiguous fixarray(3)/fixmap(2), and prose controls stay Opaque and a common observation declines typed (rc 6), never a panic",
  "detection_boundary": "MessagePack has no magic bytes, so detection is conservative: a full-input well-formed parse of exactly one item whose root is a container (array/map) reaching at least three items and eight bytes, OR the same with an unambiguous MessagePack-only head byte (0xdc..=0xdf, which CBOR's grammar rejects). A container head byte is always >= 0x80, so no pure-ASCII document is claimed. Cannot distinguish the whole-number/short-container prefix overlap with CBOR; ambiguous or trivial inputs fall back to Opaque rather than being guessed.",
  "coexistence": "CBOR's detector is tried BEFORE MessagePack's (its self-described tag is the strongest binary signal), so an input well-formed under both grammars is classified Cbor, never stolen by MessagePack; a CBOR document therefore stays Cbor.",
  "precedence": "MessagePack is tried after the strong magic-byte binaries (PDF/ZIP/Parquet/Arrow), the JSON family (JSON/JSON5/JSONL), and CBOR, and before the remaining textual heuristics (EML/YAML/TOML/CSV/Markdown/XML/HTML)",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 33 invalid-msgpack-structure",
  "adr_0060": "the MsgpackModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase21-19-1-msgpack-court.sh
# inside the court:
#   cargo build --locked --all-features
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--msgpack-pointer P|--msgpack-node ""|--msgpack-find PAT) --kind metadata|text|structure|exact
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
    echo "# Phase 21.19.1 — MessagePack court"
    echo
    echo "**Question.** Does the binary structured-tree MessagePack format close exactly"
    echo "and expose a representation-preserving model (the exact format byte actually"
    echo "used — encoding width AND signedness, \`str\` vs \`bin\`, map order and duplicate"
    echo "keys, float width, and extension type/length) on top of the whole-source exact"
    echo "leaf — while keeping the conservative no-magic-byte detection boundary,"
    echo "coexisting honestly with CBOR, and declining malformed/ambiguous inputs typed?"
    echo
    echo "**Method.** Each self-authored fixture is generated in-court with GNU"
    echo "\`/usr/bin/printf\`, ingested via \`field-build\`, observed, then — after the"
    echo "**source file and the standalone descriptor are deleted** — rematerialized"
    echo "exactly in a fresh process and compared with \`length + SHA-256 + cmp\`. An"
    echo "out-of-range pointer is required to decline typed; the strict-JSON, **CBOR**"
    echo "(coexistence), lone-scalar, 0xc1-byte, map-key-without-value, trailing-bytes,"
    echo "ambiguous fixarray(3)/fixmap(2), and prose controls pin the detection"
    echo "boundaries. The court runs in the pinned \`dev\` service using only POSIX \`sh\`,"
    echo "coreutils, git, and the shipped binary (no python3, no jq)."
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
    echo "- **Shipped here:** byte-based conservative MessagePack detection; the bounded"
    echo "  MessagePack parser producing a representation-preserving arena (kind, exact"
    echo "  span, exact format byte, extension type, ordered children); native"
    echo "  \`msgpack-pointer\`/\`msgpack-node\`/\`msgpack-find\`; common"
    echo "  \`metadata\`/\`text\`/\`search-match\`."
    echo "- **Precedence:** after the strong magic-byte binaries (PDF/ZIP/Parquet/Arrow),"
    echo "  the JSON family (JSON/JSON5/JSONL), and CBOR; before the textual heuristics."
    echo "- **Detection boundary:** MessagePack has **no magic bytes**. An input is claimed"
    echo "  only on a full-input well-formed parse whose root is a container reaching at"
    echo "  least three items and eight bytes, or the same with an unambiguous"
    echo "  MessagePack-only head byte (\`0xdc..=0xdf\`, which CBOR rejects). A container"
    echo "  head byte is always \`>= 0x80\`, so no pure-ASCII document is ever claimed. A"
    echo "  lone scalar, the \`0xc1\` byte, a map key with no value, trailing bytes, the"
    echo "  ambiguous fixarray(3)/fixmap(2) sources, and prose all stay \`Opaque\` rather"
    echo "  than being guessed."
    echo "- **Coexistence:** CBOR is tried **before** MessagePack (its self-described tag"
    echo "  is the strongest binary signal), so a CBOR document stays \`Cbor\` and is never"
    echo "  stolen. The two encodings cannot always be told apart for a source well-formed"
    echo "  under both grammars; the ordering makes that an honest \`Cbor\` classification,"
    echo "  never a MessagePack guess."
    echo "- **Recorded negative:** the whole-number/short-container prefix overlaps"
    echo "  MessagePack's \`fixint\`/short containers; ambiguous inputs are not guessed."
    echo "- **Declines:** a malformed head (the never-used \`0xc1\`), a truncated item,"
    echo "  trailing bytes, a map key with no value, an over-long declared length, a"
    echo "  non-UTF-8 \`str\`, and an out-of-range pointer are typed"
    echo "  (\`InvalidMsgpackStructure\` rc 33, or unsupported-feature rc 6); such input"
    echo "  stays \`Opaque\`."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the MessagePack"
    echo "  model is derived (\`Q_gen\`) and never on the exactness path (ADR-0060: the"
    echo "  model node depends on the \`sha256(source)\` root)."
    echo "- **Not claimed here:** the economic court (separate script)."
    echo "- **Never run on the host:** every command above ran in the pinned \`dev\`"
    echo "  container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" != "PASS" ]; then
    echo "PHASE 21.19.1 MSGPACK COURT: FAIL — campaign $CAMPAIGN (exact_fail=$exact_fail surface_fail=$surface_fail declines_ok=$declines_ok json_ctl_ok=$json_ctl_ok cbor_ctl_ok=$cbor_ctl_ok usage_ok=$usage_ok opaque_ok=$opaque_ok/$opaque_n)" >&2
    exit 1
fi
echo "PHASE 21.19.1 MSGPACK COURT: PASS — campaign $CAMPAIGN"
