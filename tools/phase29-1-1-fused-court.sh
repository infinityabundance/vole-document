#!/usr/bin/env sh
# Phase 29.1.1 — the generalized fused kernel correctness court (dev lane).
#
# Self-contained in `dev`: it uses the committed Arrow/Parquet/CSV fixtures and a
# generated fixed-width table (no python3 in this lane), the Rust test target for
# the kernel, and the shipped CLI for end-to-end checks. It asserts:
#
#   * fused `count`/`sum`/`min`/`max`/`avg`/`grouped` match hand-computed values on
#     Arrow, Parquet, CSV/TSV, and fixed-width sources (H2 correctness);
#   * equality and inclusive-range predicates behave exactly like `gt`/`any`;
#   * nulls are excluded per SQL semantics (nullable Arrow and Parquet fixtures);
#   * a text / decimal / non-numeric / string-dictionary column is a **typed
#     decline**, never a wrong answer;
#   * a float column with `grouped` is a typed decline;
#   * `materialize --exact` still closes the source byte-for-byte after the source
#     and the descriptor are deleted (the kernel does not touch exactness).
#
#   docker compose run --rm --no-TTY dev sh tools/phase29-1-1-fused-court.sh
#
# Env: BIN (default target/debug/vole-document), SKIP_TESTS=1 to skip cargo test.

set -u
cd /work
export LC_ALL=C PYTHONDONTWRITEBYTECODE=1

BIN=${BIN:-target/debug/vole-document}
AFIX=tools/fixtures/arrow
PFIX=tools/fixtures/parquet
CFIX=tools/fixtures/csv
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
fails=0

if [ "${SKIP_TESTS:-0}" != "1" ]; then
    echo "== cargo test (fused kernel) =="
    if cargo test --all-features --locked -j 1 --test fused_kernel > "$T/test.log" 2>&1; then
        tail -3 "$T/test.log"
    else
        tail -30 "$T/test.log"
        echo "phase29-1-1: fused_kernel tests FAILED" >&2
        exit 1
    fi
fi

[ -x "$BIN" ] || {
    echo "phase29-1-1: $BIN missing; build it first:" >&2
    echo "  docker compose run --rm --no-TTY dev cargo build --all-features --locked -j 1" >&2
    exit 1
}

pass() { echo "  ok   $*"; }
fail() { echo "  FAIL $*" >&2; fails=$((fails + 1)); }

fmt_of() {
    case "$1" in
        *.arrow) echo arrow ;;
        *.parquet) echo parquet ;;
        *.csv|*.tsv) echo csv ;;
        *.txt) echo fixedwidth ;;
        *) echo arrow ;;
    esac
}

build_field() { # <path> <store-name> -> hex
    "$BIN" field-build "$1" --store "$T/$2.store" --profile runtime --packed \
        > "$T/$2.build.json" 2> "$T/$2.err" || return 1
    grep -o '"field":"[0-9a-f]\{64\}"' "$T/$2.build.json" | head -1 | cut -d'"' -f4
}

# The value of a JSON key that is a bare number or a quoted decimal integer.
jint() { grep -o "\"$2\":\"\?-\?[0-9]\+" "$1" | head -1 | grep -o -- '-\?[0-9]\+$'; }
jbool() { grep -o "\"$2\":\(true\|false\)" "$1" | head -1 | cut -d: -f2; }

# expect_num <label> <fixture> <store> <column> <pred> <op> <field> <want> [extra...]
expect_num() {
    label=$1; fx=$2; name=$3; col=$4; pred=$5; op=$6; field=$7; want=$8
    shift 8
    fmt=$(fmt_of "$fx")
    hex=$(build_field "$fx" "$name") || { fail "$label: field-build"; return; }
    [ -n "$hex" ] || { fail "$label: no field id"; return; }
    out="$T/$name.json"
    "$BIN" fused-scan --store "$T/$name.store" --field "$hex" --packed \
        --format "$fmt" --column "$col" --pred "$pred" --op "$op" \
        --repeat 1 "$@" > "$out" 2> "$T/$name.err" || { fail "$label: fused-scan rc=$?"; return; }
    if [ "$(jbool "$out" declined)" = "true" ]; then fail "$label: unexpected decline"; return; fi
    got=$(jint "$out" "$field")
    if [ "$got" = "$want" ]; then pass "$label: $field=$got"; else fail "$label: $field=$got want=$want"; fi
}

# expect_decline <label> <fixture> <store> <column> <pred> <op> [extra...]
expect_decline() {
    label=$1; fx=$2; name=$3; col=$4; pred=$5; op=$6
    shift 6
    fmt=$(fmt_of "$fx")
    hex=$(build_field "$fx" "$name") || { fail "$label: field-build"; return; }
    [ -n "$hex" ] || { fail "$label: no field id"; return; }
    out="$T/$name.json"
    "$BIN" fused-scan --store "$T/$name.store" --field "$hex" --packed \
        --format "$fmt" --column "$col" --pred "$pred" --op "$op" --repeat 1 "$@" \
        > "$out" 2>/dev/null
    d=$(jbool "$out" declined)
    [ "$d" = "true" ] && pass "$label: typed decline" || fail "$label: expected a typed decline"
}

echo "== Arrow: operators vs hand-computed values =="
# primitives i64 = i*1e9 - 4e9 (i in 0..8): >0 -> 1e9,2e9,3e9.
expect_num "arrow i64 count gt" "$AFIX/primitives.arrow" a1 i64 gt:0 count count 3
expect_num "arrow i64 sum gt"   "$AFIX/primitives.arrow" a2 i64 gt:0 sum sum_int 6000000000
expect_num "arrow i64 min gt"   "$AFIX/primitives.arrow" a3 i64 gt:0 min min_int 1000000000
expect_num "arrow i64 max gt"   "$AFIX/primitives.arrow" a4 i64 gt:0 max max_int 3000000000
expect_num "arrow i64 avg cnt"  "$AFIX/primitives.arrow" a5 i64 gt:0 avg count 3
# i32 = i*1000-4000: >0 -> 1000,2000,3000 (3, sum 6000).
expect_num "arrow i32 sum gt"   "$AFIX/primitives.arrow" a6 i32 gt:0 sum sum_int 6000
# eq / range behave identically to gt on the matching subset.
expect_num "arrow i64 eq 2e9"   "$AFIX/primitives.arrow" a7 i64 eq:2000000000 count count 1
expect_num "arrow i64 range"    "$AFIX/primitives.arrow" a8 i64 range:1000000000:2000000000 count count 2
expect_num "arrow i64 any"      "$AFIX/primitives.arrow" a9 i64 any count count 8
# multi_batch k = 0..14 -> >7 is 7 rows; grouped any -> 15 singleton groups.
expect_num "arrow multi gt"     "$AFIX/multi_batch.arrow" a10 k gt:7 count count 7
expect_num "arrow multi grouped" "$AFIX/multi_batch.arrow" a11 k any grouped count 15

echo "== Arrow: nulls are excluded (SQL semantics) =="
expect_num "arrow nullable id count"   "$AFIX/nullable.arrow" n1 id gt:2 count count 4
# nullable score = [1.5, -, -2.0, 3.25, -, 0.0]: any -> 4 values, 2 nulls.
expect_num "arrow nullable score count" "$AFIX/nullable.arrow" n2 score any count count 4
expect_num "arrow nullable score nulls" "$AFIX/nullable.arrow" n3 score any count nulls 2
expect_num "arrow nullable score gt0"   "$AFIX/nullable.arrow" n4 score gt:0 count count 2

echo "== Arrow: typed declines outside the subset =="
expect_decline "arrow utf8 declined"    "$AFIX/strings.arrow"                d1 s any count
expect_decline "arrow decimal declined" "$AFIX/unsupported_decimal.arrow"    d2 dec any count
expect_decline "arrow dict declined"    "$AFIX/unsupported_dictionary.arrow" d3 0 any count
expect_decline "arrow grouped on float" "$AFIX/primitives.arrow"             d4 f64 any grouped

echo "== Parquet: operators vs hand-computed values =="
expect_num "parquet i64 count gt" "$PFIX/small_plain.parquet" p1 i64 gt:0 count count 9
expect_num "parquet i64 sum gt"   "$PFIX/small_plain.parquet" p2 i64 gt:0 sum sum_int 1000000000084
expect_num "parquet i32 count gt" "$PFIX/small_plain.parquet" p3 i32 gt:0 count count 7
expect_num "parquet i32 sum gt"   "$PFIX/small_plain.parquet" p4 i32 gt:0 sum sum_int 338
expect_num "parquet multi count"  "$PFIX/multi_rg.parquet"    p5 k gt:11 count count 12
expect_num "parquet multi sum"    "$PFIX/multi_rg.parquet"    p6 k gt:11 sum sum_int 210
expect_num "parquet dict count"   "$PFIX/dictionary.parquet"  p7 code any count count 15
expect_num "parquet dict sum"     "$PFIX/dictionary.parquet"  p8 code any sum sum_int 260

echo "== Parquet: nulls are excluded =="
expect_num "parquet nullable count" "$PFIX/phase29_nullable.parquet" q1 id any count count 7
expect_num "parquet nullable nulls" "$PFIX/phase29_nullable.parquet" q2 id any count nulls 3
expect_num "parquet nullable sum"   "$PFIX/phase29_nullable.parquet" q3 id any sum sum_int 39

echo "== Parquet: typed declines =="
expect_decline "parquet string col"  "$PFIX/dictionary.parquet" d5 city any count
expect_decline "parquet unsupported" "$PFIX/unsupported_codec.parquet" d6 0 any count
expect_decline "parquet grouped flt" "$PFIX/phase29_nullable.parquet" d7 score any grouped

echo "== CSV / TSV: operators vs hand-computed values =="
expect_num "csv id count"   "$CFIX/basic.csv" c1 id gt:0 count count 3 --kind int
expect_num "csv id sum"     "$CFIX/basic.csv" c2 id any sum sum_int 6 --kind int
expect_num "csv age sum"    "$CFIX/basic.csv" c3 age any sum sum_int 96 --kind int
expect_num "csv age max"    "$CFIX/basic.csv" c4 age any max max_int 41 --kind int
expect_num "csv age range"  "$CFIX/basic.csv" c5 age range:26:40 count count 1 --kind int
expect_num "csv grouped id" "$CFIX/basic.csv" c6 id any grouped count 3 --kind int
expect_num "csv tsv count"  "$CFIX/tsv.tsv"   c7 0 any count count 3 --kind int
expect_decline "csv name as int" "$CFIX/basic.csv" d8 name any count --kind int

echo "== Fixed-width: operators vs hand-computed values =="
printf 'Name   Age\nAlice   30\nBob     25\n' > "$T/fw.txt"
expect_num "fw age count" "$T/fw.txt" f1 1 any count count 2 --kind int
expect_num "fw age sum"   "$T/fw.txt" f2 1 any sum sum_int 55 --kind int
expect_num "fw age max"   "$T/fw.txt" f3 Age any max max_int 30 --kind int
expect_decline "fw name as int" "$T/fw.txt" d9 Name any count --kind int

echo "== exactness (kernel off the exactness path) =="
exact_check() { # <fixture> <label>
    fx=$1; label=$2
    cp "$fx" "$T/exact.src"
    cp "$fx" "$T/exact.ref"
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
        pass "$label: materialize --exact == source (len+sha+cmp)"
    else
        fail "$label exactness: rc=$rc len=$len_ok sha=$sha_ok cmp=$cmp_ok"
    fi
    rm -rf "$T/exact.store"
}
exact_check "$AFIX/primitives.arrow" "arrow"
exact_check "$PFIX/small_plain.parquet" "parquet"
exact_check "$CFIX/basic.csv" "csv"

echo
if [ "$fails" -eq 0 ]; then
    echo "PHASE 29.1.1 GENERALIZED FUSED COURT: PASS"
    exit 0
fi
echo "PHASE 29.1.1 GENERALIZED FUSED COURT: FAIL ($fails)" >&2
exit 1
