#!/usr/bin/env sh
# Phase 30.2 — push the fused CSV kernel past pyarrow (read-inclusive).
#
# Question. Phase 30.1 closed the large-input gap for Arrow/Parquet but left the
# `e_large` CSV fused/pyarrow ratio at ~35x (Phase-30.1 receipt
# `evidence/campaigns/2026-10-12-phase30-1-kernels-2ec78328`). Reading that receipt's
# harness shows the pyarrow *CSV* lane timed only the in-memory aggregation
# (`pyarrow.compute` over an already-parsed `Table`): `pyarrow.csv.read_csv` sat
# outside the timed lambda. That is fair for Arrow/Parquet (a zero-copy columnar
# scan) but for CSV it compares VOLE's parse+aggregate against pyarrow's
# aggregate-only. Phase 30.2 does two things:
#
#   1. It replaces the scalar SWAR field scan with the SIMD `memchr3` search and
#      adds a **bounded, deterministic, record-aligned multi-threaded** delimited
#      scan (`--workers N`; fixed chunk boundaries, ordered partial-accumulator
#      merge). This is the shipped `csv` fused path.
#   2. It measures the honest, apples-to-apples CSV comparator: pyarrow
#      `read_csv(bytes) + aggregate` and DuckDB `read_csv_auto` (read-inclusive),
#      while still reporting the Phase-30.1 aggregation-only number for continuity.
#
# Correctness oracle is pyarrow/DuckDB on the same logical data (count/sum/min/max/
# avg/grouped agree exactly; floats within 1e-9 relative). Exactness
# (`materialize --exact == source`) is checked for every CSV fixture.
#
#   docker compose run --rm --no-TTY analytical sh tools/phase30-2-csv-court.sh
#
# Env: REPS (5), ITERS (9), WARMUP (3), WORKERS (0 = available_parallelism),
#      PROFILE (release), BIN.

set -u
cd /work
export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1
# POSIX tab (dash's `sh` has no bash `$'\t'`).
TAB=$(printf '\t')

DRIVER=tools/fixtures/phase30-2-csv.py
SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN="evidence/campaigns/${STAMP}-phase30-2-csv-${SHA}"
RAW="$CAMPAIGN/raw"
WORK="evidence/scratch/phase30-2-csv"
CONTROL="evidence/campaigns/2026-10-12-phase30-1-kernels-2ec78328"
REPS=${REPS:-5}
ITERS=${ITERS:-9}
WARMUP=${WARMUP:-3}
WORKERS=${WORKERS:-0}
PHASE_TITLE="Phase 30.2 — fused CSV kernel past pyarrow (read-inclusive)"
PHASE_DESC="30.2 — fused CSV kernel past pyarrow (bounded parallel scan, read-inclusive)"

PROFILE=${PROFILE:-release}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-${VOLE_BIN:-target/debug/vole-document}} ;;
esac

rm -rf "$CAMPAIGN" "$WORK"
mkdir -p "$RAW/fused" "$RAW/fused_serial" "$RAW/pyarrow" "$RAW/duckdb" \
         "$WORK/corpus" "$WORK/ref" "$WORK/lanes"

echo "=== build ($PROFILE) ==="
# shellcheck disable=SC2086
cargo build $BUILD_ARGS -j 1 > "$WORK/build.log" 2>&1
build_rc=$?
tail -2 "$WORK/build.log"
if [ "$build_rc" -ne 0 ]; then
    echo "court: cargo build FAILED ($BUILD_ARGS); refusing to measure" >&2
    exit 1
fi
[ -x "$BIN" ] || { echo "court: $BIN missing; refusing to measure" >&2; exit 1; }
echo "-- measuring $BIN ($PROFILE; VOLE substrate: --profile runtime --packed; --workers $WORKERS)"

echo "=== regenerate deterministic CSV corpus ==="
python3 tools/fixtures/make-arrow.py --econ "$WORK/corpus" | tee "$RAW/corpus.txt"
cp -a "$WORK/corpus"/. "$WORK/ref/"

echo "=== plan ==="
python3 - "$WORK" "$RAW" <<'PY'
import os, sys
work, raw = sys.argv[1], sys.argv[2]
# CSV-only plan. `perf` cells feed the estimator; `correct` cells only feed the
# oracle-agreement check (grouped + NULL/empty-field semantics).
FIX = {
    "e_large": {"num": ("id", "int", 500000), "flt": ("score", "float", 0), "group": "perf"},
    "e_multi": {"num": ("k", "int", 1500), "gcol": "k", "group": "correct"},
    "e_null":  {"num": ("id", "int", 3), "flt": ("score", "float", 0), "group": "correct"},
}
FMTS = ["csv"]
rows = []
for fx, spec in FIX.items():
    for fmt in FMTS:
        src = os.path.join(work, "corpus", fx + "." + fmt)
        cells = []
        col, kind, gt = spec["num"]
        for op in ["count", "sum", "min", "max", "avg"]:
            cells.append((col, kind, op, gt))
        if "gcol" in spec:
            cells.append((spec["gcol"], "int", "grouped", spec["num"][2]))
        if "flt" in spec:
            col, kind, gt = spec["flt"]
            for op in ["count", "sum", "min", "max", "avg"]:
                cells.append((col, kind, op, gt))
        for (col, kind, op, gt) in cells:
            rows.append((fx, fmt, "%s:%s" % (col, op), col, kind, op, str(gt), spec["group"], src))
with open(os.path.join(raw, "plan.tsv"), "w") as f:
    f.write("fixture\tformat\tqid\tcolumn\tkind\top\tgt\tgroup\tsrc\n")
    for r in rows:
        f.write("\t".join(r) + "\n")
print(len(rows), "plan rows")
PY

printf 'fixture\tformat\tvole_ok\tlen_ok\tsha_ok\tcmp_ok\tmat_us\n' > "$RAW/exact.tsv"
printf 'fixture\tformat\tqid\tlane\trep\tns\n' > "$RAW/cold.tsv"

echo "=== measure (reps=$REPS iters=$ITERS warmup=$WARMUP workers=$WORKERS) ==="
awk -F'\t' 'NR>1{print $1}' "$RAW/plan.tsv" | sort -u | while read -r fx; do
    fmt=csv
    src="$WORK/corpus/$fx.$fmt"
    VDIR="$WORK/lanes/$fx.$fmt"
    mkdir -p "$VDIR"
    "$BIN" field-build "$src" --store "$VDIR/store" --voldoc "$VDIR/desc.voldoc" \
        --profile runtime --packed > "$VDIR/build.json" 2> "$VDIR/build.err" || {
        echo "court: field-build failed for $fx.$fmt" >&2; exit 1; }
    FIELD=$(python3 -c "import json;print(json.load(open('$VDIR/build.json'))['ingest']['field'])")
    echo "-- $fx.$fmt"

    awk -F'\t' -v fx="$fx" 'NR>1 && $1==fx {print}' "$RAW/plan.tsv" \
    | while IFS="$TAB" read -r _fx _fmt qid col kind op gt _grp _src; do
        for rep in $(seq 1 "$REPS"); do
            t0=$(date +%s%N)
            "$BIN" fused-scan --store "$VDIR/store" --field "$FIELD" --packed \
                --format "$fmt" --column "$col" --kind "$kind" \
                --pred "gt:$gt" --op "$op" --workers "$WORKERS" \
                --warmup "$WARMUP" --repeat "$ITERS" \
                > "$RAW/fused/$fx.$fmt.$qid.$rep.json" 2> "$RAW/fused/$fx.$fmt.$qid.$rep.err"
            t1=$(date +%s%N)
            printf '%s\t%s\t%s\tfused\t%s\t%s\n' "$fx" "$fmt" "$qid" "$rep" "$((t1 - t0))" >> "$RAW/cold.tsv"

            "$BIN" fused-scan --store "$VDIR/store" --field "$FIELD" --packed \
                --format "$fmt" --column "$col" --kind "$kind" \
                --pred "gt:$gt" --op "$op" --workers 1 \
                --warmup "$WARMUP" --repeat "$ITERS" \
                > "$RAW/fused_serial/$fx.$fmt.$qid.$rep.json" 2> "$RAW/fused_serial/$fx.$fmt.$qid.$rep.err"

            t0=$(date +%s%N)
            python3 "$DRIVER" oracle pyarrow --source "$src" --column "$col" \
                --kind "$kind" --op "$op" --gt "$gt" --iters "$ITERS" --warmup "$WARMUP" \
                --out "$RAW/pyarrow/$fx.$fmt.$qid.$rep.json" >/dev/null 2>"$RAW/pyarrow/$fx.$fmt.$qid.$rep.err" || true
            t1=$(date +%s%N)
            printf '%s\t%s\t%s\tpyarrow\t%s\t%s\n' "$fx" "$fmt" "$qid" "$rep" "$((t1 - t0))" >> "$RAW/cold.tsv"

            t0=$(date +%s%N)
            python3 "$DRIVER" oracle duckdb --source "$src" --column "$col" \
                --kind "$kind" --op "$op" --gt "$gt" --iters "$ITERS" --warmup "$WARMUP" \
                --threads 4 --out "$RAW/duckdb/$fx.$fmt.$qid.$rep.json" >/dev/null 2>"$RAW/duckdb/$fx.$fmt.$qid.$rep.err" || true
            t1=$(date +%s%N)
            printf '%s\t%s\t%s\tduckdb\t%s\t%s\n' "$fx" "$fmt" "$qid" "$rep" "$((t1 - t0))" >> "$RAW/cold.tsv"
        done
    done

    # exactness: delete the source AND the descriptor, then rematerialize.
    t0=$(date +%s%N)
    rm -f "$src" "$VDIR/desc.voldoc"
    "$BIN" materialize --store "$VDIR/store" --field "$FIELD" --exact --packed \
        --output "$WORK/$fx.$fmt.out" >/dev/null 2>&1; rc=$?
    t1=$(date +%s%N)
    v_us=$(( (t1 - t0) / 1000 ))
    v_len=$(wc -c < "$WORK/$fx.$fmt.out" 2>/dev/null | tr -d ' ')
    ref_len=$(wc -c < "$WORK/ref/$fx.$fmt" | tr -d ' ')
    v_sha=$(sha256sum "$WORK/$fx.$fmt.out" 2>/dev/null | cut -d' ' -f1)
    ref_sha=$(sha256sum "$WORK/ref/$fx.$fmt" | cut -d' ' -f1)
    len_ok=false; [ "$v_len" = "$ref_len" ] && len_ok=true
    sha_ok=false; [ "$v_sha" = "$ref_sha" ] && sha_ok=true
    cmp_ok=false; cmp -s "$WORK/ref/$fx.$fmt" "$WORK/$fx.$fmt.out" && cmp_ok=true
    vole_ok=false
    [ "$rc" -eq 0 ] && [ "$len_ok" = true ] && [ "$sha_ok" = true ] && [ "$cmp_ok" = true ] && vole_ok=true
    printf '%s.%s\t%s\t%s\t%s\t%s\t%s\n' "$fx" "$fmt" "$vole_ok" "$len_ok" "$sha_ok" "$cmp_ok" "$v_us" >> "$RAW/exact.tsv"
    [ "$vole_ok" = true ] || echo "FINDING: VOLE not byte-exact for $fx.$fmt" >&2
done

cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "$PHASE_DESC",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "measured_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "tree_state": "$(git status --porcelain | tr '\n' ' ' | sed 's/"/\\"/g')",
  "service": "analytical",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "python": "$(python3 --version)",
  "duckdb_version": "$(python3 -c 'import duckdb; print(duckdb.__version__)')",
  "pyarrow_version": "$(python3 -c 'import pyarrow; print(pyarrow.__version__)')",
  "arch": "$(uname -m)",
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "bin": "$BIN",
  "bin_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "profile": "$PROFILE",
  "build_args": "$BUILD_ARGS",
  "vole_substrate": "--profile runtime --packed",
  "workers": "$WORKERS",
  "reps": $REPS,
  "iters": $ITERS,
  "warmup": $WARMUP,
  "lanes": ["fused", "fused_serial", "pyarrow_agg", "pyarrow_read", "duckdb_read", "duckdb_agg"],
  "historical_control": "$CONTROL",
  "headline_metric": "in-process steady-state median",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY analytical sh tools/phase30-2-csv-court.sh
# PROFILE defaults to release (target/release/vole-document).
# Inside the court:
#   cargo build --release --locked --all-features
#   python3 tools/fixtures/make-arrow.py --econ $WORK/corpus     # .arrow + .parquet + .csv
#   VOLE (parallel): $BIN fused-scan --store S --field HEX --packed --format csv \
#       --column C --kind int|float --pred gt:K --op OP --workers 0 --warmup W --repeat R
#   VOLE (serial):   ... --workers 1 ...
#   pyarrow: python3 $DRIVER oracle pyarrow --source F --column C --kind K --op OP --gt G ...
#   DuckDB:  python3 $DRIVER oracle duckdb  --source F --column C --kind K --op OP --gt G ...
#   aggregate: python3 $DRIVER collect --raw RAW --reps REPS
#              python3 $DRIVER aggregate --raw RAW --campaign CAMPAIGN --env ENV
EOF

echo "=== collect ==="
python3 "$DRIVER" collect --raw "$RAW" --reps "$REPS"

echo "=== bound raw evidence ==="
python3 tools/fixtures/phase21-16-bound.py "$RAW" || true

echo "=== aggregate ==="
python3 "$DRIVER" aggregate --raw "$RAW" --campaign "$CAMPAIGN" --env "$CAMPAIGN/environment.json"
agg_rc=$?

EXACT_OK=$(awk -F'\t' 'NR>1 && $3=="true"' "$RAW/exact.tsv" | wc -l | tr -d ' ')
EXACT_N=$(awk -F'\t' 'NR>1' "$RAW/exact.tsv" | wc -l | tr -d ' ')
VERDICT=$(python3 -c "import json;print(json.load(open('$CAMPAIGN/receipt.json'))['verdict'])")

status="PASS"
if [ "$agg_rc" -ne 0 ] || [ "$EXACT_OK" -ne "$EXACT_N" ] || [ "$EXACT_N" -eq 0 ]; then
    status="FAIL"
fi

echo "=== historical control (Phase 30.1, verbatim) ==="
python3 - "$CONTROL" "$CAMPAIGN" <<'PY' || true
import json, os, sys
control, camp = sys.argv[1], sys.argv[2]
p = os.path.join(control, "receipt.json")
if not os.path.exists(p):
    raise SystemExit(0)
c = json.load(open(p))
gap = c.get("large_input_gap", {})
before = {}
for k, v in gap.items():
    fmt, q = k.split("/", 1)
    if fmt == "csv" and "pyarrow" in v:
        before[q] = v["pyarrow"]
after = json.load(open(os.path.join(camp, "receipt.json"))).get("large_csv_gap", {})
print("Phase-30.1 CSV fused/pyarrow (aggregation-only, before):")
for q in sorted(before):
    a = after.get(q, {}).get("pyarrow_agg")
    r = after.get(q, {}).get("pyarrow_read")
    print("  %-12s before=%.3f  after_agg_only=%s  after_read_inclusive=%s"
          % (q, before[q],
             ("%.3f" % a) if a is not None else "-",
             ("%.3f" % r) if r is not None else "-"))
PY

{
    echo "# $PHASE_TITLE"
    echo
    echo "**Question.** Does a bounded, deterministic, record-aligned multi-threaded"
    echo "delimited scan (with a SIMD field search) push the fused CSV kernel past"
    echo "pyarrow on the same work — parsing the CSV bytes and aggregating — without"
    echo "moving exactness or the typed declines?"
    echo
    echo "**Why read-inclusive.** The Phase-30.1 pyarrow *CSV* lane timed only the"
    echo "in-memory aggregation (\`pyarrow.compute\` over an already-parsed Table); the"
    echo "\`pyarrow.csv.read_csv\` parse sat outside the timed lambda. This court reports"
    echo "both: \`pyarrow_agg\` (the Phase-30.1 methodology, for continuity) and"
    echo "\`pyarrow_read\` (parse + aggregate, the apples-to-apples comparator), plus"
    echo "DuckDB read-inclusive and an aggregation-only control."
    echo
    echo "**Measurement.** Headline: in-process steady-state median (Phase-28.2 harness)."
    echo "Cold separated; ADR-0054 paired per-question ratio, fixed-seed cluster bootstrap"
    echo "95% CI, and MDE. Every raw sample is retained under \`$RAW\`."
    echo
    cat "$CAMPAIGN/MATRIX.md"
    echo
    cat "$CAMPAIGN/decision.md"
    echo
    echo "## Verdict"
    echo
    echo "- VOLE exactness (\`materialize --exact == source\`): **${EXACT_OK}/${EXACT_N}** (fixture)."
    echo "- Oracle agreement: see MATRIX.md (pyarrow & DuckDB, read-inclusive)."
    echo "- Decision: **$VERDICT**."
    echo "- Campaign: \`$CAMPAIGN\`."
    echo "- Profile: \`$PROFILE\`; VOLE substrate \`--profile runtime --packed\`; \`--workers $WORKERS\`."
    echo "- Historical control: \`$CONTROL\`."
    echo "- Never run on the host: every command ran in the pinned, capped \`analytical\` container."
} > "$CAMPAIGN/SUMMARY.md"

python3 - "$CAMPAIGN/receipt.json" "$status" <<'PY'
import json, sys
p, status = sys.argv[1], sys.argv[2]
d = json.load(open(p))
d["court_status"] = status
json.dump(d, open(p, "w"), sort_keys=True, indent=1)
PY

echo
if [ "$status" = "PASS" ]; then
    chown -R 1000:1000 "$CAMPAIGN" "$WORK" 2>/dev/null || true
    echo "PHASE 30.2 CSV COURT: PASS — exactness ${EXACT_OK}/${EXACT_N} — verdict $VERDICT — $CAMPAIGN"
    exit 0
fi
chown -R 1000:1000 "$CAMPAIGN" "$WORK" 2>/dev/null || true
echo "PHASE 30.2 CSV COURT: FAIL — exactness ${EXACT_OK}/${EXACT_N} — $CAMPAIGN" >&2
exit 1
