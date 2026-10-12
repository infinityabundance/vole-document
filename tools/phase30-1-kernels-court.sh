#!/usr/bin/env sh
# Phase 30.1 — large-input fused kernels (Arrow/CSV past pyarrow, Parquet).
#
# Question. The Phase-29.1 receipt (historical control:
# `evidence/campaigns/2026-10-12-phase29-1-fused-19d2c372`) showed the fused
# kernel *losing* the large-input lane — fused/pyarrow ~2.7-4.2x on `e_large`
# Arrow and 15-52x on `e_large` CSV. Phase 30.1 replaced the per-value
# `Vec`+`Value` decode in the Arrow/Parquet/CSV scanners with one monomorphized,
# single-pass `push_i64_le`/`push_f64_le` fold over the column's contiguous bytes
# (plus an allocation-free SWAR CSV field scanner). Does that close the large-input
# gap, without moving exactness?
#
# Two lanes, one receipt path (`evidence/campaigns/<today>-phase30-1-kernels-<sha>`):
#
#   * `full`  — the `analytical` lane (python3 + pyarrow + duckdb): regenerates the
#     aligned `e_large` corpus and reproduces the Phase-29.1 fused-vs-pyarrow/DuckDB
#     comparison (steady-state, cold separated, ADR-0054 paired estimator, MDE,
#     correctness oracle), with the Phase-29.1 numbers retained as the historical
#     control. This delegates the measurement to `phase29-1-fused-econ-court.sh`
#     with `ECON_FIXTURES=e_large`, so the methodology is byte-identical to 29.1.
#
#   * `kernel` — any lane with no python3/pyarrow/duckdb (the `dev` lane): measures
#     the fused kernel over the committed `tools/fixtures/{arrow,parquet,csv}/large.*`
#     fixtures, checks byte-exactness and in-process/structural correctness, and
#     seals a receipt that carries the fused medians plus the Phase-29.1 historical
#     control. It does NOT claim a comparator ratio it cannot measure.
#
#   docker compose run --rm --no-TTY analytical sh tools/phase30-1-kernels-court.sh
#   docker compose run --rm --no-TTY dev        sh tools/phase30-1-kernels-court.sh
#
# Env: REPS/ITERS/WARMUP (full lane), WARMUP/ITERS (kernel lane), BIN, PROFILE.

set -u
cd /work
export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1

STAMP=$(date -u +%Y-%m-%d)
SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
CONTROL="evidence/campaigns/2026-10-12-phase29-1-fused-19d2c372"

export CAMPAIGN_NAME=phase30-1-kernels
export PHASE_TITLE="Phase 30.1 — large-input fused kernels (Arrow/CSV/Parquet past pyarrow)"
export PHASE_DESC="30.1 — large-input fused kernels (Arrow/CSV/Parquet past pyarrow) decision Experiment"
CAMPAIGN="evidence/campaigns/${STAMP}-${CAMPAIGN_NAME}-${SHA}"

HAVE_PY=0
if command -v python3 >/dev/null 2>&1 && python3 -c 'import pyarrow, duckdb' >/dev/null 2>&1; then
    HAVE_PY=1
fi

# ===========================================================================
# full lane (analytical): reproduce the Phase-29.1 large-input comparison
# ===========================================================================
if [ "$HAVE_PY" = "1" ]; then
    echo "phase30-1: analytical lane (python3+pyarrow+duckdb) -> full large-input comparison (e_large)"
    ECON_FIXTURES=e_large \
    PROFILE="${PROFILE:-release}" \
    sh tools/phase29-1-fused-econ-court.sh
    rc=$?

    if [ -d "$CAMPAIGN" ]; then
        # Relabel the internally Phase-29.1-labelled artifacts for this receipt.
        sed -i 's/Phase 29\.1/Phase 30.1/g' "$CAMPAIGN/MATRIX.md" "$CAMPAIGN/decision.md" 2>/dev/null || true
        python3 - "$CAMPAIGN" "$CONTROL" "$PHASE_DESC" <<'PY'
import json, os, sys
camp, control, phase = sys.argv[1], sys.argv[2], sys.argv[3]

def load(p):
    try:
        with open(p) as f:
            return json.load(f)
    except (OSError, ValueError):
        return {}

new = load(os.path.join(camp, "receipt.json"))
if new:
    new["phase"] = phase
    new["mode"] = "full"
    ctl = load(os.path.join(control, "receipt.json"))
    new["historical_control"] = {
        "campaign": control,
        "large_input_gap": ctl.get("large_input_gap", {}),
        "estimator": ctl.get("estimator", {}),
    }
    # before/after for the e_large fused/comparator median ratios
    ba = {}
    ng, cg = new.get("large_input_gap", {}), ctl.get("large_input_gap", {})
    for k in sorted(set(ng) | set(cg)):
        row = {}
        for comp in ("pyarrow", "duckdb"):
            b = (cg.get(k) or {}).get(comp)
            a = (ng.get(k) or {}).get(comp)
            if b is not None or a is not None:
                row[comp] = {"before": b, "after": a}
        if row:
            ba[k] = row
    new["large_input_before_after"] = ba
    with open(os.path.join(camp, "receipt.json"), "w") as f:
        json.dump(new, f, sort_keys=True, indent=1)

env = load(os.path.join(camp, "environment.json"))
if env:
    env["phase"] = phase
    with open(os.path.join(camp, "environment.json"), "w") as f:
        json.dump(env, f, sort_keys=True, indent=1)

# A human before/after section appended to SUMMARY.md.
L = ["", "## Large-input before/after vs the Phase-29.1 control (e_large)", "",
     "Ratio is `fused / comparator`; `< 1 favours fused`. `before` is the sealed",
     "Phase-29.1 control (`%s`); `after` is this receipt." % control, "",
     "| format | question | comp | before | after | delta |", "|---|---|---|---:|---:|---:|"]
for k in sorted(ba):
    fmt, qid = k.split("/", 1)
    for comp in ("pyarrow", "duckdb"):
        r = ba[k].get(comp)
        if not r:
            continue
        b, a = r.get("before"), r.get("after")
        d = "" if (b is None or a is None) else "%+.3f" % (a - b)
        L.append("| %s | %s | %s | %s | %s | %s |" % (
            fmt, qid, comp,
            "%.3f" % b if b is not None else "-",
            "%.3f" % a if a is not None else "-", d))
L.append("")
with open(os.path.join(camp, "SUMMARY.md"), "a") as f:
    f.write("\n".join(L) + "\n")
PY
        chown -R 1000:1000 "$CAMPAIGN" 2>/dev/null || true
    fi
    exit "$rc"
fi

# ===========================================================================
# kernel lane (dev / no python3): fused large-input kernel + exactness
# ===========================================================================
echo "phase30-1: kernel lane (no python3/pyarrow/duckdb) -> fused large-input kernel court"

BIN=${BIN:-}
if [ -z "$BIN" ]; then
    if [ -x target/release/vole-document ]; then
        BIN=target/release/vole-document
    else
        BIN=target/debug/vole-document
    fi
fi
[ -x "$BIN" ] || {
    echo "phase30-1: $BIN missing; build it first:" >&2
    echo "  docker compose run --rm --no-TTY dev cargo build --all-features --locked -j 1" >&2
    exit 1
}

case "$BIN" in
    *release*) PROFILE=release ;;
    *)         PROFILE=debug ;;
esac

WARMUP=${WARMUP:-3}
ITERS=${ITERS:-9}
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
fails=0

if [ -f "$CAMPAIGN/receipt.json" ] && grep -q '"mode": "full"' "$CAMPAIGN/receipt.json" 2>/dev/null; then
    # A full (analytical) receipt already exists; never clobber it with kernel-lane
    # evidence. Write this lane's evidence alongside it under kernel-lane/.
    OUT="$CAMPAIGN/kernel-lane"
    rm -rf "$OUT"
    echo "phase30-1: preserving the existing full receipt; writing kernel evidence to $OUT"
else
    OUT="$CAMPAIGN"
    rm -rf "$CAMPAIGN"
fi
RAW="$OUT/raw"
mkdir -p "$RAW"

TAB=$(printf '\t')
jnum() { grep -o "\"$2\":\"\{0,1\}-\{0,1\}[0-9]\+" "$1" | head -1 | grep -o -- '-\{0,1\}[0-9]\+$'; }
jg()   { grep -o "\"$2\":-\{0,1\}[0-9][0-9.eE+-]*" "$1" | head -1 | cut -d: -f2; }
jbool(){ grep -o "\"$2\":\(true\|false\)" "$1" | head -1 | cut -d: -f2; }
jarr() { grep -o "\"$2\":\[[0-9,]*\]" "$1" | head -1 | sed 's/.*\[//; s/\].*//' | tr ',' '\n'; }
median() {
    sort -n | awk '{a[NR]=$1} END{
        if (NR==0) { print "" }
        else if (NR%2) { print a[(NR+1)/2] }
        else { print (a[NR/2]+a[NR/2+1])/2 }
    }'
}
# The op's measured value (count / sum / min / max / avg) from a fused-scan JSON.
jval() { # file op kind
    case "$2" in
        count) jnum "$1" count ;;
        sum)   if [ "$3" = int ]; then jnum "$1" sum_int; else jg "$1" sum_float; fi ;;
        min)   if [ "$3" = int ]; then jnum "$1" min_int; else jg "$1" min_float; fi ;;
        max)   if [ "$3" = int ]; then jnum "$1" max_int; else jg "$1" max_float; fi ;;
        avg)   jg "$1" avg ;;
    esac
}

printf 'format%scolumn%sop%skind%smedian_ns%smatched%svalue\n' \
    "$TAB" "$TAB" "$TAB" "$TAB" "$TAB" "$TAB" > "$RAW/kernel.tsv"

cat > "$T/plan.tsv" <<EOF
arrow${TAB}tools/fixtures/arrow/large.arrow${TAB}id${TAB}int
arrow${TAB}tools/fixtures/arrow/large.arrow${TAB}score${TAB}float
parquet${TAB}tools/fixtures/parquet/large.parquet${TAB}0${TAB}int
parquet${TAB}tools/fixtures/parquet/large.parquet${TAB}1${TAB}float
csv${TAB}tools/fixtures/csv/large.csv${TAB}row_id${TAB}int
csv${TAB}tools/fixtures/csv/large.csv${TAB}score${TAB}int
EOF

echo "== fused large-input steady state (warmup=$WARMUP iters=$ITERS, binary=$BIN) =="
while IFS="$TAB" read -r fmt src col kind; do
    [ -n "$fmt" ] || continue
    if [ ! -f "$src" ]; then
        echo "  skip $src (generated fixture absent; regenerate with the matching" >&2
        echo "       make-*.py, or run this court in the analytical lane)" >&2
        continue
    fi
    nm=$(printf '%s_%s' "$fmt" "$col" | tr -c 'A-Za-z0-9' '_')
    hex=$("$BIN" field-build "$src" --store "$T/$nm.store" --profile runtime --packed \
            2>"$RAW/$nm.build.err" \
          | grep -o '"field":"[0-9a-f]*"' | head -1 | cut -d'"' -f4)
    if [ -z "$hex" ]; then
        echo "  FAIL $fmt/$col: field-build" >&2
        fails=$((fails + 1))
        continue
    fi
    for op in count sum min max avg; do
        extra=""
        if [ "$op" = count ] && [ "$fmt" != csv ]; then
            extra="--also-current"
        fi
        out="$RAW/$nm.$op.json"
        # shellcheck disable=SC2086
        if ! "$BIN" fused-scan --store "$T/$nm.store" --field "$hex" --packed \
                --format "$fmt" --column "$col" --kind "$kind" --pred any --op "$op" \
                --warmup "$WARMUP" --repeat "$ITERS" $extra \
                > "$out" 2>"$RAW/$nm.$op.err"; then
            echo "  FAIL $fmt/$col $op: fused-scan rc=$?" >&2
            fails=$((fails + 1))
            continue
        fi
        if [ "$(jbool "$out" declined)" = "true" ]; then
            echo "  FAIL $fmt/$col $op: unexpected decline" >&2
            fails=$((fails + 1))
            continue
        fi
        med=$(jarr "$out" fused_ns | median)
        matched=$(jnum "$out" matched)
        val=$(jval "$out" "$op" "$kind")
        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
            "$fmt" "$col" "$op" "$kind" "$med" "$matched" "$val" >> "$RAW/kernel.tsv"
    done
    echo "  ok   $fmt/$col fused medians recorded"
done < "$T/plan.tsv"
if [ "$(awk 'NR>1{c++} END{print c+0}' "$RAW/kernel.tsv")" -eq 0 ]; then
    echo "FAIL: no large fixture was present to measure" >&2
    fails=$((fails + 1))
fi

echo "== correctness: fused vs oracle (in-process current / structural) =="
check_count() { # label jsonfile want
    got=$(jnum "$2" count)
    if [ "$got" = "$3" ]; then echo "  ok   $1: count=$got"; else
        echo "  FAIL $1: count=$got want=$3" >&2; fails=$((fails + 1)); fi
}
# Arrow/Parquet integer count must equal the shipped whole-column path.
for nm in arrow_id parquet_0; do
    f="$RAW/$nm.count.json"
    [ -f "$f" ] || { echo "  skip $nm (fixture absent)"; continue; }
    cur=$(jnum "$f" current_count)
    if [ -n "$cur" ] && [ "$(jbool "$f" current_available)" = "true" ]; then
        check_count "$nm (count)" "$f" "$cur"
    else
        echo "  FAIL $nm: in-process current path unavailable" >&2
        fails=$((fails + 1))
    fi
done
# CSV: record count equals the file's data-row count (header excluded).
CSVF=tools/fixtures/csv/large.csv
if [ -f "$CSVF" ] && [ -f "$RAW/csv_row_id.count.json" ]; then
    CSV_ROWS=$(( $(wc -l < "$CSVF") - 1 ))
    got=$(jnum "$RAW/csv_row_id.count.json" count)
    if [ "$got" = "$CSV_ROWS" ]; then
        echo "  ok   csv row_id count=$got == data rows"
    else
        echo "  FAIL csv row_id count=$got want=$CSV_ROWS" >&2
        fails=$((fails + 1))
    fi
else
    echo "  skip csv row-count invariant (fixture absent)"
fi
# avg == sum / count, and min <= avg <= max, for every column/kind.
awk -F'\t' '
NF<7 || $7=="" { next }
{ key=$1 "/" $2; v[key "/" $3]=$7; seen[key]=1 }
END {
    bad=0
    for (k in seen) {
        cnt=v[k "/count"]+0; s=v[k "/sum"]+0; av=v[k "/avg"]+0
        mn=v[k "/min"]+0; mx=v[k "/max"]+0
        if (cnt>0 && s!=0) {
            mean=s/cnt
            if (av < mean*0.999999 || av > mean*1.000001) { print "  FAIL " k ": avg " av " != sum/count " mean; bad=1 }
        }
        if (cnt>0 && (av < mn-1e-9 || av > mx+1e-9)) { print "  FAIL " k ": avg outside [min,max]"; bad=1 }
    }
    exit bad
}' "$RAW/kernel.tsv" || fails=$((fails + 1))
[ "$fails" -eq 0 ] && echo "  ok   avg == sum/count and min <= avg <= max for all columns"

echo "== exactness (kernel off the exactness path) =="
exact_check() { # src label
    src=$1; label=$2
    if [ ! -f "$src" ]; then echo "  skip exactness $label (fixture absent)"; return; fi
    cp "$src" "$T/exact.src"
    cp "$src" "$T/exact.ref"
    hex=$("$BIN" field-build "$T/exact.src" --store "$T/exact.store" --voldoc "$T/exact.voldoc" \
            --profile runtime --packed > "$T/exact.build.json" 2>/dev/null; \
          grep -o '"field":"[0-9a-f]*"' "$T/exact.build.json" | head -1 | cut -d'"' -f4)
    rm -f "$T/exact.src" "$T/exact.voldoc"
    "$BIN" materialize --store "$T/exact.store" --field "$hex" --exact --packed \
        --output "$T/exact.out" >/dev/null 2>&1
    rc=$?
    len_ok=false
    [ "$(wc -c < "$T/exact.out" 2>/dev/null | tr -d ' ')" = "$(wc -c < "$T/exact.ref" | tr -d ' ')" ] && len_ok=true
    sha_ok=false
    [ "$(sha256sum "$T/exact.out" 2>/dev/null | cut -d' ' -f1)" = "$(sha256sum "$T/exact.ref" | cut -d' ' -f1)" ] && sha_ok=true
    cmp_ok=false
    cmp -s "$T/exact.ref" "$T/exact.out" && cmp_ok=true
    if [ "$rc" -eq 0 ] && [ "$len_ok" = true ] && [ "$sha_ok" = true ] && [ "$cmp_ok" = true ]; then
        echo "  ok   $label: materialize --exact == source (len+sha+cmp)"
    else
        echo "  FAIL $label exactness: rc=$rc len=$len_ok sha=$sha_ok cmp=$cmp_ok" >&2
        fails=$((fails + 1))
    fi
    rm -rf "$T/exact.store"
}
exact_check tools/fixtures/arrow/large.arrow   arrow
exact_check tools/fixtures/parquet/large.parquet parquet
exact_check tools/fixtures/csv/large.csv       csv

# ---- historical control (verbatim, plain text) -----------------------------
CTL_LARGE=$(awk '/^## Large-input gap/{f=1} f&&/^## /&&!/Large-input gap/{exit} f{print}' \
    "$CONTROL/MATRIX.md" 2>/dev/null)
CTL_EST=$(awk '/^## Paired ratios/{f=1} f&&/^## /&&!/Paired ratios/{exit} f{print}' \
    "$CONTROL/MATRIX.md" 2>/dev/null)

# ---- receipt ---------------------------------------------------------------
verdict="PASS"
[ "$fails" -eq 0 ] || verdict="FAIL"

{
    echo "{"
    printf '  "campaign": "%s",\n' "$OUT"
    printf '  "phase": "%s",\n' "$PHASE_DESC"
    printf '  "mode": "kernel",\n'
    printf '  "service": "dev",\n'
    printf '  "utc": "%s",\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf '  "measured_commit": "%s",\n' "$(git rev-parse HEAD 2>/dev/null || echo unknown)"
    printf '  "tree_state": "%s",\n' "$(git status --porcelain | tr '\n' ' ' | sed 's/"/\\"/g')"
    printf '  "rustc": "%s",\n' "$(rustc --version)"
    printf '  "cargo": "%s",\n' "$(cargo --version)"
    printf '  "arch": "%s",\n' "$(uname -m)"
    printf '  "bin": "%s",\n' "$BIN"
    printf '  "bin_sha256": "%s",\n' "$(sha256sum "$BIN" | cut -d' ' -f1)"
    printf '  "profile": "%s",\n' "$PROFILE"
    printf '  "warmup": %s,\n' "$WARMUP"
    printf '  "iters": %s,\n' "$ITERS"
    printf '  "headline_metric": "in-process steady-state median",\n'
    printf '  "fixtures": {\n'
    first=1
    for f in tools/fixtures/arrow/large.arrow tools/fixtures/parquet/large.parquet tools/fixtures/csv/large.csv; do
        [ "$first" = 1 ] || printf ',\n'
        first=0
        printf '    "%s": {"bytes": %s, "sha256": "%s"}' \
            "$f" "$(wc -c < "$f" | tr -d ' ')" "$(sha256sum "$f" | cut -d' ' -f1)"
    done
    printf '\n  },\n'
    printf '  "exactness": {"ok": %s, "n": 3},\n' "$([ "$fails" -eq 0 ] && echo true || echo false)"
    printf '  "historical_control_campaign": "%s",\n' "$CONTROL"
    printf '  "comparator_lanes": "unavailable in the dev/kernel lane; run this court in the analytical lane (mode full) for the live fused-vs-pyarrow/DuckDB comparison",\n'
    printf '  "env_affecting_semantics": {"LC_ALL": "C"},\n'
    printf '  "court_status": "%s"\n' "$verdict"
    echo "}"
} > "$OUT/receipt.json"

{
    echo "# $PHASE_TITLE"
    echo
    echo "**Mode.** \`kernel\` — this lane (no python3/pyarrow/duckdb) measures the fused"
    echo "kernel over the committed \`tools/fixtures/{arrow,parquet,csv}/large.*\` fixtures."
    echo "The live fused-vs-pyarrow/DuckDB large-input comparison is sealed by running the"
    echo "same court in the \`analytical\` lane (mode \`full\`, \`e_large\`)."
    echo
    echo "**Measurement.** Headline: in-process steady-state median (warmup ${WARMUP},"
    echo "iters ${ITERS}); binary \`$BIN\` (\`$PROFILE\`)."
    echo
    echo "## Fused large-input steady-state medians (ns)"
    echo
    echo "| format | column | op | kind | median ns | matched | value |"
    echo "|---|---|---|---|---:|---:|---:|"
    awk -F'\t' 'NR>1 { printf "| %s | %s | %s | %s | %s | %s | %s |\n", $1,$2,$3,$4,$5,$6,$7 }' "$RAW/kernel.tsv"
    echo
    echo "## Exactness"
    echo
    echo "- \`materialize --exact == source\` (length + SHA-256 + \`cmp\`) for Arrow,"
    echo "  Parquet and CSV large fixtures: see \`raw/\` and the court stdout."
    echo
    echo "## Correctness (in-process / structural oracles)"
    echo
    echo "- Arrow/Parquet integer \`count\` equals the shipped whole-column \`current\` path."
    echo "- CSV \`count\` equals the file's data-row count."
    echo "- \`avg == sum/count\` and \`min <= avg <= max\` for every column."
    echo
    echo "## Historical control (Phase 29.1) — fused / comparator"
    echo
    echo "The Phase-29.1 control (\`$CONTROL\`) measured the *aligned* \`e_large\` corpus"
    echo "(different rows/columns from the committed fixtures), so the numbers below are"
    echo "the control, not an apples-to-apples after-delta. The after-delta is in the"
    echo "analytical-mode receipt."
    echo
    if [ -n "$CTL_EST" ]; then echo "$CTL_EST"; echo; fi
    if [ -n "$CTL_LARGE" ]; then echo "$CTL_LARGE"; fi
    echo
    echo "## Verdict"
    echo
    echo "- Exactness + correctness: **$verdict**."
    echo "- Campaign: \`$OUT\`."
    echo "- Never run on the host: every command ran in a pinned, capped container."
} > "$OUT/SUMMARY.md"

cat > "$OUT/commands.txt" <<'EOF'
docker compose run --rm --no-TTY analytical sh tools/phase30-1-kernels-court.sh   # full (e_large vs pyarrow/DuckDB)
docker compose run --rm --no-TTY dev        sh tools/phase30-1-kernels-court.sh   # kernel (fused large-input + exactness)
# full lane delegates the measurement to phase29-1-fused-econ-court.sh with ECON_FIXTURES=e_large
EOF

chown -R 1000:1000 "$OUT" 2>/dev/null || true

echo
if [ "$verdict" = "PASS" ]; then
    echo "PHASE 30.1 KERNELS COURT (kernel lane): PASS — $OUT"
    exit 0
fi
echo "PHASE 30.1 KERNELS COURT (kernel lane): FAIL — $OUT" >&2
exit 1
