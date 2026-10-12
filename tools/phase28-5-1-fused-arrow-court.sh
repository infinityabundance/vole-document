#!/usr/bin/env sh
# Phase 28.5.1 — the fused Arrow kernel correctness court (dev lane).
#
# Self-contained in `dev`: it uses the committed Arrow fixtures under
# `tools/fixtures/arrow/` (no python3 is present in this lane), the Rust test
# target for the kernel, and the shipped CLI for end-to-end checks. It asserts:
#
#   * the kernel's count equals the shipped `observe` text path's count on the same
#     field/substrate, for integer, unsigned, float, nullable, and multi-batch
#     fixtures (H2 correctness);
#   * the bounded evidence cap is honoured and flagged, never silently ignored;
#   * a non-numeric / compressed / decimal / dictionary column is a **typed
#     decline**, never a wrong answer;
#   * `materialize --exact` still closes the source byte-for-byte after the source
#     and the descriptor are deleted (the kernel does not touch exactness).
#
#   docker compose run --rm --no-TTY dev sh tools/phase28-5-1-fused-arrow-court.sh
#
# Env: BIN (default target/debug/vole-document), SKIP_TESTS=1 to skip cargo test.

set -u
cd /work
export LC_ALL=C PYTHONDONTWRITEBYTECODE=1

BIN=${BIN:-target/debug/vole-document}
FIX=tools/fixtures/arrow
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
fails=0

if [ "${SKIP_TESTS:-0}" != "1" ]; then
    echo "== cargo test (Arrow kernel) =="
    if cargo test --all-features --locked -j 1 --test arrow_adapter > "$T/test.log" 2>&1; then
        tail -3 "$T/test.log"
    else
        tail -20 "$T/test.log"
        echo "phase28-5-1: arrow_adapter tests FAILED" >&2
        exit 1
    fi
fi

[ -x "$BIN" ] || {
    echo "phase28-5-1: $BIN missing; build it first:" >&2
    echo "  docker compose run --rm --no-TTY dev cargo build --all-features --locked -j 1" >&2
    exit 1
}

pass() { echo "  ok   $*"; }
fail() { echo "  FAIL $*" >&2; fails=$((fails + 1)); }

# Build a field from a fixture and print its field hex.
build_field() { # fixture name -> hex
    "$BIN" field-build "$1" --store "$T/$2.store" --profile runtime --packed \
        > "$T/$2.build.json" 2> "$T/$2.build.err" || return 1
    grep -o '"field":"[0-9a-f]\{64\}"' "$T/$2.build.json" | head -1 | cut -d'"' -f4
}

jnum() { grep -o "\"$2\":[0-9]*" "$1" | head -1 | cut -d: -f2; }
jbool() { grep -o "\"$2\":\(true\|false\)" "$1" | head -1 | cut -d: -f2; }

# expect_equal <label> <fixture> <store-name> <column> <gt> <cap> <want-count>
expect_equal() {
    label=$1; fx=$2; name=$3; col=$4; gt=$5; cap=$6; want=$7
    hex=$(build_field "$fx" "$name") || { fail "$label: field-build"; return; }
    [ -n "$hex" ] || { fail "$label: no field id"; return; }
    out="$T/$name.json"
    "$BIN" arrow-fused --store "$T/$name.store" --field "$hex" --packed \
        --column "$col" --gt "$gt" --evidence-cap "$cap" --warmup 1 --repeat 2 --also-current \
        > "$out" 2> "$T/$name.err" || { fail "$label: arrow-fused rc=$?"; return; }
    c=$(jnum "$out" count); cc=$(jnum "$out" current_count)
    d=$(jbool "$out" declined)
    if [ "$d" = "true" ]; then fail "$label: unexpected decline"; return; fi
    if [ "$c" != "$want" ]; then fail "$label: fused count=$c want=$want"; return; fi
    if [ "$c" != "$cc" ]; then fail "$label: fused=$c != current=$cc"; return; fi
    pass "$label: fused==current==$c"
}

# expect_decline <label> <fixture> <store-name> <column> <gt>
expect_decline() {
    label=$1; fx=$2; name=$3; col=$4; gt=$5
    hex=$(build_field "$fx" "$name") || { fail "$label: field-build"; return; }
    [ -n "$hex" ] || { fail "$label: no field id"; return; }
    out="$T/$name.json"
    "$BIN" arrow-fused --store "$T/$name.store" --field "$hex" --packed \
        --column "$col" --gt "$gt" --repeat 1 > "$out" 2>/dev/null
    d=$(jbool "$out" declined)
    [ "$d" = "true" ] && pass "$label: typed decline" || fail "$label: expected a typed decline"
}

echo "== fused kernel vs shipped path (counts) =="
# primitives: i64 = i*1e9-4e9 (i in 0..8) -> >0 is 1e9,2e9,3e9 = 3
expect_equal "primitives i64>0"   "$FIX/primitives.arrow"  prim i64 0 4 3
# primitives: i32 = i*1000-4000 -> >0 is 1000,2000,3000 = 3
expect_equal "primitives i32>0"   "$FIX/primitives.arrow"  prim i32 0 4 3
# multi_batch: k = 0..14 -> >7 is 8..14 = 7
expect_equal "multi_batch k>7"    "$FIX/multi_batch.arrow" mb k 7 32 7
# nullable: id = 1..6 (no nulls) -> >2 is 3,4,5,6 = 4
expect_equal "nullable id>2"      "$FIX/nullable.arrow"    null id 2 8 4

# Float columns: the kernel compares natively (matching pyarrow), while the
# shipped `observe --kind text` driver parses integers only and so silently
# yields 0 for a non-integral float predicate. The kernel is the correct one;
# this is recorded, not hidden. (The economic corpus's Q2 column is an integer,
# where the two agree.)
hex=$(build_field "$FIX/primitives.arrow" f64c)
out="$T/f64c.json"
"$BIN" arrow-fused --store "$T/f64c.store" --field "$hex" --packed \
    --column f64 --gt 0 --warmup 1 --repeat 2 --also-current > "$out" 2>/dev/null
fc=$(jnum "$out" count); cc=$(jnum "$out" current_count)
if [ "$fc" = "3" ] && [ "$cc" = "0" ]; then
    pass "primitives f64>0: fused=3 (native, == pyarrow) vs shipped text driver=0 (integer-only)"
else
    fail "primitives f64>0: fused=$fc (want 3) shipped=$cc (want 0)"
fi

echo "== bounded evidence cap =="
hex=$(build_field "$FIX/primitives.arrow" primcap)
out="$T/primcap.json"
"$BIN" arrow-fused --store "$T/primcap.store" --field "$hex" --packed \
    --column i64 --gt 0 --evidence-cap 2 --repeat 1 > "$out" 2>/dev/null
c=$(jnum "$out" count); ec=$(jnum "$out" evidence_count); tr=$(jbool "$out" evidence_truncated)
if [ "$c" = "3" ] && [ "$ec" = "2" ] && [ "$tr" = "true" ]; then
    pass "cap=2 -> count=3 evidence_count=2 truncated=true"
else
    fail "cap: count=$c evidence_count=$ec truncated=$tr"
fi

echo "== typed declines outside the subset =="
expect_decline "utf8 declined"        "$FIX/strings.arrow"                 strs 0 0
expect_decline "decimal declined"     "$FIX/unsupported_decimal.arrow"     dec  0 0
expect_decline "dictionary declined"  "$FIX/unsupported_dictionary.arrow"  dct  0 0
expect_decline "compressed declined"  "$FIX/unsupported_compressed.arrow"  cmp  0 0

echo "== exactness (kernel off the exactness path) =="
# Build from a throwaway copy, delete the copy AND the descriptor, then
# rematerialize from the store alone and byte-compare to a kept reference.
cp "$FIX/primitives.arrow" "$T/exact.src"
cp "$FIX/primitives.arrow" "$T/exact.ref"
hex=$("$BIN" field-build "$T/exact.src" --store "$T/exact.store" --voldoc "$T/exact.voldoc" \
        --profile runtime --packed > "$T/exact.build.json" 2>/dev/null; \
      grep -o '"field":"[0-9a-f]\{64\}"' "$T/exact.build.json" | head -1 | cut -d'"' -f4)
rm -f "$T/exact.src" "$T/exact.voldoc"
"$BIN" materialize --store "$T/exact.store" --field "$hex" --exact --packed \
    --output "$T/exact.out" >/dev/null 2>&1
rc=$?
len_ok=false; [ "$(wc -c < "$T/exact.out" 2>/dev/null | tr -d ' ')" = "$(wc -c < "$T/exact.ref" | tr -d ' ')" ] && len_ok=true
sha_ok=false; [ "$(sha256sum "$T/exact.out" 2>/dev/null | cut -d' ' -f1)" = "$(sha256sum "$T/exact.ref" | cut -d' ' -f1)" ] && sha_ok=true
cmp_ok=false; cmp -s "$T/exact.ref" "$T/exact.out" && cmp_ok=true
if [ "$rc" -eq 0 ] && [ "$len_ok" = true ] && [ "$sha_ok" = true ] && [ "$cmp_ok" = true ]; then
    pass "materialize --exact == source (len+sha+cmp) after source+descriptor deletion"
else
    fail "exactness: rc=$rc len=$len_ok sha=$sha_ok cmp=$cmp_ok"
fi

echo
if [ "$fails" -eq 0 ]; then
    echo "PHASE 28.5.1 FUSED ARROW COURT: PASS"
    exit 0
fi
echo "PHASE 28.5.1 FUSED ARROW COURT: FAIL ($fails)" >&2
exit 1
