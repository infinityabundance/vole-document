#!/usr/bin/env bash
# Phase 28.5 — decision Experiment 1: the fused Arrow predicate/aggregate kernel.
#
# Pre-registered hypotheses:
#
#   H1 (exactness) — for every fixture, VOLE `materialize --exact == source`
#       (length + SHA-256 + cmp) after the source AND the descriptor are deleted.
#   H2 (kernel correctness) — the fused kernel's count equals the shipped
#       observation path's count AND pyarrow's AND DuckDB's, per fixture; the
#       evidence variant's matching-row coordinates equal pyarrow's
#       `indices_nonzero` prefix.
#   H3 (fused is not merely in-process) — fused also beats the shipped `observe`
#       path run in the SAME process (derived cache off), so the win is the fused
#       kernel, not just avoiding a fresh CLI process.
#   H4 (economics) — the headline is the in-process steady-state median; cold is
#       reported separately; the estimator is the paired per-fixture ratio with a
#       fixed-seed cluster bootstrap (median + geomean, 95% CI, MDE), every raw
#       sample retained.
#
# The four lanes answer the SAME logical question over the SAME logical Arrow data:
#   Q2  = count(*) where numeric column > K   (all four lanes)
#   Q2E = the matching rows' coordinates + that count (fused-VOLE vs pyarrow)
#
# Runs in the pinned, hard-capped `analytical` service (dev toolchain + python3 +
# hash-pinned duckdb + hash-pinned pyarrow); never the host:
#
#   docker compose run --rm --no-TTY analytical bash tools/phase28-5-fused-arrow-econ-court.sh
#
# Env: REPS (default 9), ITERS (default 15), WARMUP (default 3), EVID_CAP (1024),
#      PROFILE (release|debug). No cap is ever raised here.

set -u
cd /work
export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1

BASE=tools/fixtures/phase21-16-arrow-baseline.py
DUCK=tools/fixtures/phase21-16-arrow-duckdb.py
PYARROW=tools/fixtures/phase21-16-arrow-pyarrow.py
FUSED=tools/fixtures/phase28-5-fused-arrow.py
SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN="evidence/campaigns/${STAMP}-phase28-5-fused-arrow-${SHA}"
RAW="$CAMPAIGN/raw"
WORK="evidence/scratch/phase28-5-econ"
REPS=${REPS:-9}
ITERS=${ITERS:-15}
WARMUP=${WARMUP:-3}
EVID_CAP=${EVID_CAP:-1024}

PROFILE=${PROFILE:-release}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-${VOLE_BIN:-target/debug/vole-document}} ;;
esac

rm -rf "$CAMPAIGN" "$WORK"
mkdir -p "$RAW/fused" "$RAW/fusedcold" "$RAW/currentcold" "$RAW/steady" \
         "$RAW/pyarrowq2e" "$RAW/coldpy" "$RAW/qanswers" \
         "$WORK/corpus" "$WORK/ref"

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

echo "=== regenerate deterministic corpus (.arrow + .parquet projection) ==="
python3 tools/fixtures/make-arrow.py --econ "$WORK/corpus" | tee "$RAW/corpus.txt"
{
    printf 'fixture\tsrc_bytes\tsrc_sha256\n'
    while IFS="$(printf '\t')" read -r name bytes sha; do
        printf '%s\t%s\t%s\n' "$name" "$bytes" "$sha"
    done < "$RAW/corpus.txt"
} > "$RAW/fixtures.tsv"
cp -a "$WORK/corpus"/* "$WORK/ref/"
FIXTURES=$(awk 'NR>1{print $1}' "$RAW/fixtures.tsv")

now_ns() { date +%s%N; }

# --- per-fixture plan (FROZEN): "COL_NUM GT COL_STR SUBSTR" -------------------
plan_for() {
    case "$1" in
        e_small.arrow) echo "0 100 1 row-001" ;;
        e_multi.arrow) echo "0 1500 1 t0150" ;;
        e_large.arrow) echo "0 500000 2 record-0000100" ;;
        *)             echo "0 0 0 z" ;;
    esac
}

printf 'fixture\tvole_ok\tlen_ok\tsha_ok\tcmp_ok\tmat_us\n' > "$RAW/exact.tsv"
printf '{"fused_eq_current":{' > "$RAW/exact.json.tmp"

first=1
for f in $FIXTURES; do
    echo "=== $f ==="
    read -r COLN GT COLS SUB <<EOF
$(plan_for "$f")
EOF
    PLAN=$(python3 -c 'import json,sys;print(json.dumps({"col_num":int(sys.argv[1]),"gt":int(sys.argv[2]),"col_str":int(sys.argv[3]),"substr":sys.argv[4]}))' \
        "$COLN" "$GT" "$COLS" "$SUB")

    VDIR="$WORK/lanes/$f.vole"
    SDIR="$WORK/lanes/$f.sqlite"
    DDIR="$WORK/lanes/$f.duckdb"
    PDIR="$WORK/lanes/$f.pyarrow"
    mkdir -p "$VDIR" "$DDIR" "$PDIR"

    # -- VOLE field (once) ----------------------------------------------------
    "$BIN" field-build "$WORK/corpus/$f" --store "$VDIR/store" --voldoc "$VDIR/desc.voldoc" \
        --profile runtime --packed > "$VDIR/build.json" 2> "$VDIR/build.err" || {
        echo "court: field-build failed for $f" >&2; exit 1; }
    FIELD=$(python3 -c "import json;print(json.load(open('$VDIR/build.json'))['ingest']['field'])")
    [ -n "$FIELD" ] || { echo "court: no VOLE field for $f" >&2; exit 1; }

    # -- comparator lane builds (once) ----------------------------------------
    rm -rf "$DDIR/dd" "$PDIR/pd"
    python3 "$DUCK" build --source "$WORK/ref/$f" --dir "$DDIR/dd" > "$DDIR/build.json" 2> "$DDIR/build.err"
    python3 "$PYARROW" build --source "$WORK/ref/$f" --dir "$PDIR/pd" > "$PDIR/build.json" 2> "$PDIR/build.err"

    # -- interleaved repeats --------------------------------------------------
    for rep in $(seq 1 "$REPS"); do
        # fused kernel (in-process steady) interleaved with the shipped observe path
        "$BIN" arrow-fused --store "$VDIR/store" --field "$FIELD" --packed \
            --column "$COLN" --gt "$GT" --evidence-cap 0 \
            --warmup "$WARMUP" --repeat "$ITERS" --also-current \
            > "$RAW/fused/$f.Q2.$rep.json" 2> "$RAW/fused/$f.Q2.$rep.err"
        "$BIN" arrow-fused --store "$VDIR/store" --field "$FIELD" --packed \
            --column "$COLN" --gt "$GT" --evidence-cap "$EVID_CAP" \
            --warmup "$WARMUP" --repeat "$ITERS" --also-current \
            > "$RAW/fused/$f.Q2E.$rep.json" 2> "$RAW/fused/$f.Q2E.$rep.err"
        # fused kernel cold (a fresh CLI process per question)
        t0=$(now_ns)
        "$BIN" arrow-fused --store "$VDIR/store" --field "$FIELD" --packed \
            --column "$COLN" --gt "$GT" --evidence-cap 0 --warmup 0 --repeat 1 \
            > "$RAW/fusedcold/$f.$rep.raw.json" 2>/dev/null
        t1=$(now_ns)
        us=$(( (t1 - t0) / 1000 ))
        python3 - "$RAW/fusedcold/$f.$rep.raw.json" "$us" "$RAW/fusedcold/$f.$rep.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
d["us"] = int(sys.argv[2])
json.dump(d, open(sys.argv[3], "w"), sort_keys=True)
PY
        rm -f "$RAW/fusedcold/$f.$rep.raw.json"
        # current-VOLE deployed path: fresh CLI process per question
        python3 "$FUSED" currentcold --bin "$BIN" --store "$VDIR/store" --field "$FIELD" \
            --packed --column "$COLN" --gt "$GT" --reps 1 \
            --outdir "$RAW/currentcold.$f.$rep.dir" >/dev/null 2>&1
        mv "$RAW/currentcold.$f.$rep.dir/1.json" "$RAW/currentcold/$f.$rep.json"
        rmdir "$RAW/currentcold.$f.$rep.dir"
        # pyarrow / DuckDB steady Q2 (persistent reader / registered Arrow table)
        python3 "$FUSED" pyarrow-q2 --dir "$PDIR/pd" --plan "$PLAN" \
            --iters "$ITERS" --warmup "$WARMUP" --outdir "$RAW/steady.$f.pyarrow.$rep.dir" >/dev/null 2>&1
        mv "$RAW/steady.$f.pyarrow.$rep.dir/q2.json" "$RAW/steady/$f.pyarrow.$rep.json"
        rmdir "$RAW/steady.$f.pyarrow.$rep.dir"
        python3 "$FUSED" duckdb-q2 --dir "$DDIR/dd" --plan "$PLAN" \
            --iters "$ITERS" --warmup "$WARMUP" --threads 4 --outdir "$RAW/steady.$f.duckdb.$rep.dir" >/dev/null 2>&1
        mv "$RAW/steady.$f.duckdb.$rep.dir/q2.json" "$RAW/steady/$f.duckdb.$rep.json"
        rmdir "$RAW/steady.$f.duckdb.$rep.dir"
        # pyarrow matching-row coordinates (Q2E) + cold-process Q2 (fresh interpreter)
        python3 "$FUSED" pyarrow-q2e --dir "$PDIR/pd" --plan "$PLAN" --iters "$ITERS" \
            --warmup "$WARMUP" --cap "$EVID_CAP" --outdir "$RAW/pyarrowq2e.$f.$rep.dir" >/dev/null 2>&1
        mv "$RAW/pyarrowq2e.$f.$rep.dir/q2e.json" "$RAW/pyarrowq2e/$f.$rep.json"
        rmdir "$RAW/pyarrowq2e.$f.$rep.dir"
        for lane in pyarrow duckdb; do
            t0=$(now_ns)
            if [ "$lane" = pyarrow ]; then
                python3 "$PYARROW" query --dir "$PDIR/pd" --q Q2 --plan "$PLAN" \
                    --out "$RAW/qanswers/$f.Q2.pyarrow.json" >/dev/null 2>&1
            else
                python3 "$DUCK" query --dir "$DDIR/dd" --q Q2 --plan "$PLAN" \
                    --out "$RAW/qanswers/$f.Q2.duckdb.json" >/dev/null 2>&1
            fi
            t1=$(now_ns)
            us=$(( (t1 - t0) / 1000 ))
            printf '{"us":%s}\n' "$us" > "$RAW/coldpy/$f.$lane.$rep.json"
        done
        # the pyarrow Q2 answer (once) for cross-lane agreement is written above.
    done

    # the fused / current-cli Q2 answers (rep 1) for agreement
    cp "$RAW/fused/$f.Q2.1.json" "$RAW/qanswers/$f.Q2.fused.json"
    cp "$RAW/currentcold/$f.1.json" "$RAW/qanswers/$f.Q2.current_cli.json"

    # ---- exactness: delete the source AND the descriptor, then rematerialize --
    SRC_BYTES=$(awk -F'\t' -v fx="$f" 'NR>1 && $1==fx {print $2}' "$RAW/fixtures.tsv")
    SRC_SHA=$(awk -F'\t' -v fx="$f" 'NR>1 && $1==fx {print $3}' "$RAW/fixtures.tsv")
    rm -f "$WORK/corpus/$f" "$VDIR/desc.voldoc"
    t0=$(now_ns)
    "$BIN" materialize --store "$VDIR/store" --field "$FIELD" --exact --packed \
        --output "$WORK/$f.vole.out" >/dev/null 2>&1; v_rc=$?
    t1=$(now_ns)
    v_us=$(( (t1 - t0) / 1000 ))
    v_len=$(wc -c < "$WORK/$f.vole.out" 2>/dev/null | tr -d ' ')
    v_sha=$(sha256sum "$WORK/$f.vole.out" 2>/dev/null | cut -d' ' -f1)
    len_ok=false; [ "$v_len" = "$SRC_BYTES" ] && len_ok=true
    sha_ok=false; [ "$v_sha" = "$SRC_SHA" ] && sha_ok=true
    cmp_ok=false; cmp -s "$WORK/ref/$f" "$WORK/$f.vole.out" && cmp_ok=true
    vole_ok=false
    [ "$v_rc" -eq 0 ] && [ "$len_ok" = true ] && [ "$sha_ok" = true ] && [ "$cmp_ok" = true ] && vole_ok=true
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$f" "$vole_ok" "$len_ok" "$sha_ok" "$cmp_ok" "$v_us" >> "$RAW/exact.tsv"
    [ "$vole_ok" = true ] || echo "FINDING: VOLE not byte-exact for $f" >&2

    f_cnt=$(python3 -c "import json;print(json.load(open('$RAW/fused/$f.Q2.1.json')).get('count'))")
    c_cnt=$(python3 -c "import json;print(json.load(open('$RAW/currentcold/$f.1.json')).get('count'))")
    [ "$first" -eq 1 ] || printf ',' >> "$RAW/exact.json.tmp"
    first=0
    python3 - "$f" "$f_cnt" "$c_cnt" "$RAW/exact.json.tmp" <<'PY'
import json, sys
fx, a, b = sys.argv[1], sys.argv[2], sys.argv[3]
with open(sys.argv[4], "a") as f:
    f.write('%s:{"fused":%s,"current":%s,"equal":%s}' % (
        json.dumps(fx), a, b, "true" if a == b else "false"))
PY
done
printf '}}\n' >> "$RAW/exact.json.tmp"
python3 - "$RAW/exact.json.tmp" "$RAW/exact.json" <<'PY'
import json, sys
body = open(sys.argv[1]).read()
with open(sys.argv[2], "w") as f:
    f.write(body)
PY
rm -f "$RAW/exact.json.tmp"

DUCKDB_VERSION=$(python3 -c 'import duckdb; print(duckdb.__version__)')
PYARROW_VERSION=$(python3 -c 'import pyarrow; print(pyarrow.__version__)')
FIX_JSON=$(python3 -c "import json;rows=[l.split('\t') for l in open('$RAW/fixtures.tsv').read().splitlines()[1:] if l];print(json.dumps({r[0]:{'len':int(r[1]),'sha256':r[2]} for r in rows},sort_keys=True))")
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "28.5 — fused Arrow predicate/aggregate kernel (decision Experiment 1)",
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
  "nproc": "$(nproc 2>/dev/null || echo '?')",
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "bin": "$BIN",
  "bin_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "profile": "$PROFILE",
  "build_args": "$BUILD_ARGS",
  "vole_substrate": "--profile runtime --packed",
  "fixtures_len_sha256": $FIX_JSON,
  "reps": $REPS,
  "iters": $ITERS,
  "warmup": $WARMUP,
  "evidence_cap": $EVID_CAP,
  "lanes": ["fused", "current_inproc", "current_cli", "pyarrow", "duckdb"],
  "questions": {"Q2": "count(*) where numeric column > K", "Q2E": "Q2 + matching-row coordinates/evidence"},
  "headline_metric": "in-process steady-state median",
  "current_inproc": "the shipped observe path in the SAME process (derived cache off): attributes the win to the kernel, not to avoiding process spawn",
  "pyarrow_q2e": "pyarrow.compute.indices_nonzero over the real Arrow data (the equal contract for the coordinates)",
  "historical_control": "evidence/campaigns/2026-10-12-phase28-2-arrow-econ-f7e0a961 (shipped VOLE steady median = 2.25x DuckDB, 29.1x pyarrow on the six-question envelope)",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY analytical bash tools/phase28-5-fused-arrow-econ-court.sh
# PROFILE defaults to release (target/release/vole-document).
# Inside the court:
#   cargo build --release --locked --all-features
#   python3 tools/fixtures/make-arrow.py --econ $WORK/corpus
#   VOLE: $BIN field-build F.arrow --store STORE --voldoc F.voldoc --profile runtime --packed
#         $BIN arrow-fused --store STORE --field HEX --packed --column N --gt K \
#             [--evidence-cap C] --warmup W --repeat R --also-current   # Q2 / Q2E
#         $BIN arrow-fused ... --warmup 0 --repeat 1                    # cold process
#         python3 tools/fixtures/phase28-5-fused-arrow.py currentcold --bin $BIN ...
#   pyarrow: python3 tools/fixtures/phase21-16-arrow-pyarrow.py build --source F.arrow --dir PD
#            python3 tools/fixtures/phase28-5-fused-arrow.py pyarrow-q2  --dir PD --plan PLAN ...
#            python3 tools/fixtures/phase28-5-fused-arrow.py pyarrow-q2e --dir PD --plan PLAN ...
#            python3 tools/fixtures/phase21-16-arrow-pyarrow.py query --dir PD --q Q2 ...  (cold)
#   DuckDB:  python3 tools/fixtures/phase21-16-arrow-duckdb.py build --source F.arrow --dir DD
#            python3 tools/fixtures/phase28-5-fused-arrow.py duckdb-q2 --dir DD --plan PLAN --threads 4 ...
#            python3 tools/fixtures/phase21-16-arrow-duckdb.py query --dir DD --q Q2 ...  (cold)
#   aggregate: python3 tools/fixtures/phase28-5-fused-arrow.py collect --raw RAW
#              python3 tools/fixtures/phase28-5-fused-arrow.py aggregate --raw RAW --campaign CAMPAIGN --env ENV
EOF

echo "=== collect ==="
python3 "$FUSED" collect --raw "$RAW"

# Bound oversized raw evidence (large verbatim dumps -> length+SHA-256+prefix).
python3 tools/fixtures/phase21-16-bound.py "$RAW"

echo "=== aggregate ==="
python3 "$FUSED" aggregate --raw "$RAW" --campaign "$CAMPAIGN" --env "$CAMPAIGN/environment.json"
agg_rc=$?

EXACT_OK=$(awk -F'\t' 'NR>1 && $2=="true"' "$RAW/exact.tsv" | wc -l | tr -d ' ')
EXACT_N=$(awk -F'\t' 'NR>1' "$RAW/exact.tsv" | wc -l | tr -d ' ')
VERDICT=$(python3 -c "import json;print(json.load(open('$CAMPAIGN/receipt.json'))['verdict'])")

status="PASS"
if [ "$agg_rc" -ne 0 ] || [ "$EXACT_OK" -ne "$EXACT_N" ] || [ "$EXACT_N" -eq 0 ]; then
    status="FAIL"
fi

{
    echo "# Phase 28.5 — fused Arrow predicate/aggregate kernel (decision Experiment 1)"
    echo
    echo "**Question.** Does one bounded native kernel that reads the substrate Arrow bytes,"
    echo "decodes only the addressed numeric column, applies the predicate in native form, and"
    echo "aggregates directly — optionally attaching exact source evidence — beat the shipped"
    echo "VOLE path end-to-end, and where does it stand against pyarrow and DuckDB?"
    echo
    echo "**Measurement.** Headline: in-process steady-state median. Cold-process is separate."
    echo "Estimator: paired per-fixture ratio, median + geometric mean, fixed-seed cluster"
    echo "bootstrap 95% CI, and MDE (ADR-0054). Every raw sample is retained under \`$RAW\`."
    echo "The \`fused\` vs \`current_inproc\` lane pair runs in the SAME process with the derived"
    echo "cache off, so the win is the kernel, not process startup."
    echo
    cat "$CAMPAIGN/MATRIX.md"
    echo
    cat "$CAMPAIGN/decision.md"
    echo
    echo "## Verdict"
    echo
    echo "- VOLE exactness: **${EXACT_OK}/${EXACT_N}**."
    echo "- Decision: **$VERDICT**."
    echo "- Campaign: \`$CAMPAIGN\`."
    echo "- Profile: \`$PROFILE\`; VOLE substrate \`--profile runtime --packed\`."
    echo "- Never run on the host: every command ran in the pinned, capped \`analytical\` container."
} > "$CAMPAIGN/SUMMARY.md"

# Keep the machine verdict honest: the court FAILS only when exactness breaks.
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
    echo "PHASE 28.5 FUSED ARROW ECON COURT: PASS — exactness ${EXACT_OK}/${EXACT_N} — decision $VERDICT — $CAMPAIGN"
    exit 0
fi
chown -R 1000:1000 "$CAMPAIGN" "$WORK" 2>/dev/null || true
echo "PHASE 28.5 FUSED ARROW ECON COURT: FAIL — exactness ${EXACT_OK}/${EXACT_N} — $CAMPAIGN" >&2
exit 1
