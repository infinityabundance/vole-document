#!/usr/bin/env bash
# Phase 29.1 — decision Experiment 2: the generalized fused kernel across formats.
#
# Pre-registered hypotheses:
#
#   H1 (exactness)  — for every fixture/format, VOLE `materialize --exact == source`
#       (length + SHA-256 + cmp) after the source AND the descriptor are deleted.
#   H2 (correctness) — the fused `count`/`sum`/`min`/`max`/`avg`/`grouped` equals
#       the pyarrow AND DuckDB oracle on the same logical data, per format.
#   H3 (attribution) — the fused kernel also beats the shipped whole-column text
#       path in the SAME process (derived cache off) for `count`.
#   H4 (economics)   — headline is the in-process steady-state median; cold is
#       separate; ADR-0054 paired per-(fixture,format,question) estimator with a
#       fixed-seed cluster bootstrap, MDE, every raw sample retained.
#
# Lanes: fused-VOLE (per format), current-VOLE observe path (Arrow/Parquet count),
# pyarrow (reads the real Arrow/Parquet/CSV), DuckDB (reads the real data).
#
#   docker compose run --rm --no-TTY analytical bash tools/phase29-1-fused-econ-court.sh
#
# Env: REPS (5), ITERS (9), WARMUP (3), PROFILE (release).

set -u
cd /work
export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1
# POSIX tab (dash's `sh` does not support bash's `$'\t'`).
TAB=$(printf '\t')

DRIVER=tools/fixtures/phase29-1-fused-econ.py
SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
# `CAMPAIGN_NAME` defaults to the Phase-29.1 name so existing callers are unchanged;
# a follow-on phase (e.g. 30.1) seals its own receipt by overriding it.
CAMPAIGN="evidence/campaigns/${STAMP}-${CAMPAIGN_NAME:-phase29-1-fused}-${SHA}"
RAW="$CAMPAIGN/raw"
WORK="evidence/scratch/phase29-1-econ"
REPS=${REPS:-5}
ITERS=${ITERS:-9}
WARMUP=${WARMUP:-3}
# Human-readable labels for the sealed receipt (defaults preserve Phase-29.1 output).
PHASE_TITLE=${PHASE_TITLE:-"Phase 29.1 — generalized fused kernel (Arrow/Parquet/CSV)"}
PHASE_DESC=${PHASE_DESC:-"29.1 — generalized fused kernel (Arrow/Parquet/CSV) decision Experiment 2"}

PROFILE=${PROFILE:-release}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-${VOLE_BIN:-target/debug/vole-document}} ;;
esac

rm -rf "$CAMPAIGN" "$WORK"
mkdir -p "$RAW/fused" "$RAW/pyarrow" "$RAW/duckdb" "$WORK/corpus" "$WORK/ref" "$WORK/lanes"

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
echo "-- measuring $BIN ($PROFILE; VOLE substrate: --profile runtime --packed)"

echo "=== regenerate deterministic corpus (.arrow + .parquet + .csv) ==="
python3 tools/fixtures/make-arrow.py --econ "$WORK/corpus" | tee "$RAW/corpus.txt"
cp -a "$WORK/corpus"/. "$WORK/ref/"

echo "=== plan ==="
python3 - "$WORK" "$RAW" <<'PY'
import os, sys
work, raw = sys.argv[1], sys.argv[2]
FIX_ALL = {
    "e_small": {"num": ("id", "int", 100), "flt": ("score", "float", 0), "group": "id"},
    "e_multi": {"num": ("k", "int", 1500), "group": "k"},
    "e_large": {"num": ("id", "int", 500000), "flt": ("score", "float", 0)},
    "e_null":  {"num": ("id", "int", 3), "flt": ("score", "float", 0), "group": "id"},
}
# `ECON_FIXTURES` is an opt-in, backward-compatible filter so a follow-on phase
# (e.g. 30.1) can seal a large-input-only matrix from the same court. Empty /
# unset -> every fixture (the Phase-29.1 default is unchanged).
_sel = os.environ.get("ECON_FIXTURES", "")
FIX = {k: v for k, v in FIX_ALL.items() if not _sel or k in _sel.split(",")}
if not FIX:
    raise SystemExit("ECON_FIXTURES=%r selected no fixture" % _sel)
FMTS = ["arrow", "parquet", "csv"]
rows = []
for fx, spec in FIX.items():
    for fmt in FMTS:
        src = os.path.join(work, "corpus", fx + "." + fmt)
        cells = []
        col, kind, gt = spec["num"]
        for op in ["count", "sum", "min", "max", "avg"]:
            cells.append((col, kind, op, gt))
        if "group" in spec:
            cells.append((spec["group"], "int", "grouped", gt))
        if "flt" in spec:
            col, kind, gt = spec["flt"]
            for op in ["count", "sum", "min", "max", "avg"]:
                cells.append((col, kind, op, gt))
        for (col, kind, op, gt) in cells:
            rows.append((fx, fmt, "%s:%s" % (col, op), col, kind, op, str(gt), src))
with open(os.path.join(raw, "plan.tsv"), "w") as f:
    f.write("fixture\tformat\tqid\tcolumn\tkind\top\tgt\tsrc\n")
    for r in rows:
        f.write("\t".join(r) + "\n")
print(len(rows), "plan rows")
PY

printf 'fixture\tformat\tvole_ok\tlen_ok\tsha_ok\tcmp_ok\tmat_us\n' > "$RAW/exact.tsv"
printf 'fixture\tformat\tqid\tlane\trep\tns\n' > "$RAW/cold.tsv"

echo "=== measure (reps=$REPS iters=$ITERS warmup=$WARMUP) ==="
awk -F'\t' 'NR>1{print $1"|"$2}' "$RAW/plan.tsv" | sort -u | while IFS='|' read -r fx fmt; do
    src="$WORK/corpus/$fx.$fmt"
    VDIR="$WORK/lanes/$fx.$fmt"
    mkdir -p "$VDIR"
    "$BIN" field-build "$src" --store "$VDIR/store" --voldoc "$VDIR/desc.voldoc" \
        --profile runtime --packed > "$VDIR/build.json" 2> "$VDIR/build.err" || {
        echo "court: field-build failed for $fx.$fmt" >&2; exit 1; }
    FIELD=$(python3 -c "import json;print(json.load(open('$VDIR/build.json'))['ingest']['field'])")
    echo "-- $fx.$fmt field=${FIELD%????????????????????????????????????????????????????????????}"

    awk -F'\t' -v fx="$fx" -v fmt="$fmt" 'NR>1 && $1==fx && $2==fmt {print}' "$RAW/plan.tsv" \
    | while IFS="$TAB" read -r _fx _fmt qid col kind op gt _src; do
        for rep in $(seq 1 "$REPS"); do
            extra=""
            if [ "$op" = "count" ] && { [ "$fmt" = "arrow" ] || [ "$fmt" = "parquet" ]; }; then
                extra="--also-current"
            fi
            t0=$(date +%s%N)
            "$BIN" fused-scan --store "$VDIR/store" --field "$FIELD" --packed \
                --format "$fmt" --column "$col" --kind "$kind" \
                --pred "gt:$gt" --op "$op" \
                --warmup "$WARMUP" --repeat "$ITERS" $extra \
                > "$RAW/fused/$fx.$fmt.$qid.$rep.json" 2> "$RAW/fused/$fx.$fmt.$qid.$rep.err"
            t1=$(date +%s%N)
            printf '%s\t%s\t%s\tfused\t%s\t%s\n' "$fx" "$fmt" "$qid" "$rep" "$((t1 - t0))" >> "$RAW/cold.tsv"
            t0=$(date +%s%N)
            python3 "$DRIVER" pyarrow --source "$src" --format "$fmt" --column "$col" \
                --kind "$kind" --op "$op" --gt "$gt" --iters "$ITERS" --warmup "$WARMUP" \
                --out "$RAW/pyarrow/$fx.$fmt.$qid.$rep.json" >/dev/null 2>"$RAW/pyarrow/$fx.$fmt.$qid.$rep.err" || true
            t1=$(date +%s%N)
            printf '%s\t%s\t%s\tpyarrow\t%s\t%s\n' "$fx" "$fmt" "$qid" "$rep" "$((t1 - t0))" >> "$RAW/cold.tsv"
            t0=$(date +%s%N)
            python3 "$DRIVER" duckdb --source "$src" --format "$fmt" --column "$col" \
                --kind "$kind" --op "$op" --gt "$gt" --iters "$ITERS" --warmup "$WARMUP" \
                --threads 4 --out "$RAW/duckdb/$fx.$fmt.$qid.$rep.json" >/dev/null 2>"$RAW/duckdb/$fx.$fmt.$qid.$rep.err" || true
            t1=$(date +%s%N)
            printf '%s\t%s\t%s\tduckdb\t%s\t%s\n' "$fx" "$fmt" "$qid" "$rep" "$((t1 - t0))" >> "$RAW/cold.tsv"
        done
    done

    # exactness: delete the source AND the descriptor, then rematerialize.
    t0=$(date +%s%N)
    rm -f "$WORK/corpus/$fx.$fmt" "$VDIR/desc.voldoc"
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

DUCKDB_VERSION=$(python3 -c 'import duckdb; print(duckdb.__version__)')
PYARROW_VERSION=$(python3 -c 'import pyarrow; print(pyarrow.__version__)')
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
  "duckdb_version": "$DUCKDB_VERSION",
  "pyarrow_version": "$PYARROW_VERSION",
  "arch": "$(uname -m)",
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "bin": "$BIN",
  "bin_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "profile": "$PROFILE",
  "build_args": "$BUILD_ARGS",
  "vole_substrate": "--profile runtime --packed",
  "reps": $REPS,
  "iters": $ITERS,
  "warmup": $WARMUP,
  "lanes": ["fused", "current", "pyarrow", "duckdb"],
  "headline_metric": "in-process steady-state median",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY analytical bash tools/phase29-1-fused-econ-court.sh
# PROFILE defaults to release (target/release/vole-document).
# Inside the court:
#   cargo build --release --locked --all-features
#   python3 tools/fixtures/make-arrow.py --econ $WORK/corpus     # .arrow + .parquet + .csv
#   VOLE: $BIN field-build F --store S --voldoc D --profile runtime --packed
#         $BIN fused-scan --store S --field HEX --packed --format FMT --column C \
#             --kind int|float --pred gt:K --op OP [--also-current] --warmup W --repeat R
#   pyarrow: python3 $DRIVER pyarrow --source F --format FMT --column C --kind K --op OP --gt G ...
#   DuckDB:  python3 $DRIVER duckdb  --source F --format FMT --column C --kind K --op OP --gt G ...
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

{
    echo "# $PHASE_TITLE"
    echo
    echo "**Question.** Does one bounded, validity-aware fused kernel — one versioned"
    echo "logical plan (projection + equality/range predicate + accumulator), with a"
    echo "physical scanner per format — beat the shipped VOLE path and stand against"
    echo "pyarrow and DuckDB, with **exact** agreement to the oracle on count/sum/min/"
    echo "max/avg/grouped across Arrow, Parquet, and CSV?"
    echo
    echo "**Measurement.** Headline: in-process steady-state median. Cold is separate."
    echo "Estimator: paired per-(fixture,format,question) ratio, median + geometric mean,"
    echo "fixed-seed cluster bootstrap 95% CI, and MDE (ADR-0054). Every raw sample is"
    echo "retained under \`$RAW\`."
    echo
    cat "$CAMPAIGN/MATRIX.md"
    echo
    cat "$CAMPAIGN/decision.md"
    echo
    echo "## Verdict"
    echo
    echo "- VOLE exactness: **${EXACT_OK}/${EXACT_N}** (fixture×format)."
    echo "- Oracle agreement: see MATRIX.md."
    echo "- Decision: **$VERDICT**."
    echo "- Campaign: \`$CAMPAIGN\`."
    echo "- Profile: \`$PROFILE\`; VOLE substrate \`--profile runtime --packed\`."
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
    echo "PHASE 29.1 FUSED ECON COURT: PASS — exactness ${EXACT_OK}/${EXACT_N} — verdict $VERDICT — $CAMPAIGN"
    exit 0
fi
chown -R 1000:1000 "$CAMPAIGN" "$WORK" 2>/dev/null || true
echo "PHASE 29.1 FUSED ECON COURT: FAIL — exactness ${EXACT_OK}/${EXACT_N} — $CAMPAIGN" >&2
exit 1
