#!/usr/bin/env bash
# Phase 21.16 / 28.2 — the Arrow IPC economic court (Phase 28.2 re-seal).
#
# Pre-registered. Hypotheses:
#
#   H1 (byte-exactness / Q6) — for every corpus fixture, VOLE
#       `materialize --exact == source` (length + SHA-256 + `cmp`) after the source
#       file AND the standalone descriptor are deleted, in a fresh process. The court
#       FAILS unless this is 100 %.
#   H2 (contract questions) — the four lanes (VOLE, a source-retaining SQLite
#       baseline, a DuckDB analytical baseline over the **real Arrow data**, and a
#       `pyarrow` lane over the Arrow IPC file itself) answer the SAME six
#       questions Q1–Q6 where they can, and every question a lane cannot answer is a
#       TYPED decline, never a silent empty answer.
#   H3 (cross-lane agreement) — where all lanes answer, comparable values agree.
#   H4 (economics) — build/storage/**steady-state** are measured per lane with a
#       stated estimator (paired per-fixture ratios, median + geometric mean,
#       fixed-seed cluster bootstrap by fixture; ratio-of-sums reported separately),
#       every raw sample retained. Wall times are microseconds (`us`).
#
# PHASE 28.2 (the review's "28.0") — the competitor envelope:
#   * DuckDB now reads the **real Arrow data** through the Arrow C data interface
#     (`con.register` of the `pyarrow.ipc` table) — NOT a Parquet projection.
#   * SQLite gets typed columns, a covering predicate index, and a tuned FTS5
#     external-content **trigram** index for the substring question (with a
#     recorded cross-check).
#   * **In-process steady-state** is the headline; **cold-process** (fresh OS process
#     per question) and **cold-cache** (fresh connection/reader, warm interpreter) are
#     reported **separately**. Python interpreter startup no longer decides the ratio.
#   * p50/p95/p99 are recorded per question; capability accounting counts every
#     answered / typed-declined / vs-VOLE outcome.
#   * Pre-28.2 numbers are kept as an explicit **historical control** (see MATRIX).
#
# The comparators win the analytical axes (projection, predicate execution, metadata
# statistics); the court says so plainly. DuckDB and pyarrow are not source-retaining
# stores, so Q6 is a typed `not-native` decline for both; with the Arrow integration
# there is no Parquet statistics block, so Q3 is a typed `not-retained` decline for
# both. VOLE is an archival field with typed observations, not a query engine.
#
# Honest scope: a self-authored corpus; only Q6 is a byte-authority claim.
#
# Fairness: the comparators (SQLite C, DuckDB, Python) are unaffected by the Rust
# profile, so the court defaults to a RELEASE build (`PROFILE=release`); VOLE uses
# `field-build --profile runtime --packed`, with `--packed` on every
# observe/observe-batch/materialize.
#
# Runs in the pinned, hard-capped `analytical` service (dev toolchain + python3 +
# sqlite3 + hash-pinned duckdb + hash-pinned pyarrow); never the host:
#
#   docker compose run --rm --no-TTY analytical bash tools/phase21-16-arrow-court.sh

set -u
# POSIX sh (dash) has no `pipefail`; the build failure is detected explicitly below.
cd /work
export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1

BASE=tools/fixtures/phase21-16-arrow-baseline.py
DUCK=tools/fixtures/phase21-16-arrow-duckdb.py
PYARROW=tools/fixtures/phase21-16-arrow-pyarrow.py
VOLE=tools/fixtures/phase21-16-arrow-vole.py
SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN="evidence/campaigns/${STAMP}-phase28-2-arrow-econ-${SHA}"
RAW="$CAMPAIGN/raw"
WORK="evidence/scratch/phase28-2-econ"
REPS=${REPS:-9}
STEADY_ITERS=${STEADY_ITERS:-15}
# Under a heavily loaded shared host, wall-clock samples are noisy; the build/cold
# estimator is the MIN over reps, so more reps give a more robust floor. The
# steady-state headline uses p50 over STEADY_ITERS in-process samples.

PROFILE=${PROFILE:-release}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-${VOLE_BIN:-target/debug/vole-document}} ;;
esac

rm -rf "$CAMPAIGN" "$WORK"
mkdir -p "$RAW/qanswers" "$RAW/plans" "$WORK/corpus" "$WORK/ref"

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

printf 'fixture\tlane\trep\trc\tus\n' > "$RAW/build.tsv"
printf 'fixture\tlane\tbytes\tfiles\n' > "$RAW/storage.tsv"
printf 'fixture\tlane\tq\trep\trc\tus\n' > "$RAW/cold.tsv"
printf 'fixture\tlane\tq\tmin\tp50\tp95\tp99\tmedian\tn\n' > "$RAW/steady.tsv"
printf 'fixture\tlane\tq\ti\tus\n' > "$RAW/steady.samples.tsv"
printf 'fixture\tlane\tq\tus\n' > "$RAW/coldcache.tsv"
printf 'fixture\tvole_ok\tsqlite_ok\tlen_ok\tsha_ok\tcmp_ok\tv_us\ts_us\n' > "$RAW/exact.tsv"

# Emit steady-state rows (fixture lane q min p50 p95 p99 median n) from a driver's
# steady JSON (schema: {"batch": [{"q","min","p50","p95","p99","n"}, ...]}).
emit_steady() { # fixture lane json
    python3 - "$3" "$1" "$2" "$RAW/steady.tsv" "$RAW/steady.samples.tsv" <<'PY'
import json, os, sys
p, fx, lane, out, sout = sys.argv[1:]
b = []
if os.path.exists(p):
    try:
        b = json.load(open(p)).get("batch", [])
    except ValueError:
        b = []
with open(out, "a") as f:
    for x in b:
        f.write("%s\t%s\t%s\t%d\t%.1f\t%.1f\t%.1f\t%.1f\t%d\n" % (
            fx, lane, x["q"], x["min"], x["p50"], x["p95"], x["p99"], x["median"], x["n"]))
with open(sout, "a") as f:
    for x in b:
        for i, s in enumerate(x.get("samples", [])):
            f.write("%s\t%s\t%s\t%d\t%d\n" % (fx, lane, x["q"], i, s))
PY
}

# Emit cold-cache rows from a driver's cold-cache JSON (schema {"coldcache": {q: us}}).
emit_coldcache() { # fixture lane json
    python3 - "$3" "$1" "$2" "$RAW/coldcache.tsv" <<'PY'
import json, os, sys
p, fx, lane, out = sys.argv[1:]
c = {}
if os.path.exists(p):
    try:
        c = json.load(open(p)).get("coldcache", {})
    except ValueError:
        c = {}
with open(out, "a") as f:
    for q in ("Q1", "Q2", "Q3", "Q4", "Q5", "Q6"):
        if q in c:
            f.write("%s\t%s\t%s\t%s\n" % (fx, lane, q, c[q]))
PY
}

build_lane() { # fixture lane dir
    local f="$1" lane="$2" dir="$3"
    local pf="${f%.arrow}.parquet"
    rm -rf "$dir"; mkdir -p "$dir"
    case "$lane" in
        vole)   "$BIN" field-build "$WORK/corpus/$f" --store "$dir/store" --voldoc "$dir/desc.voldoc" \
                    --profile runtime --packed > "$dir/build.json" 2> "$dir/build.err" ;;
        sqlite) python3 "$BASE" build --source "$WORK/ref/$f" --table-source "$WORK/ref/$pf" --db "$dir/x.sqlite" --plan "${4:-}" > "$dir/build.json" 2> "$dir/build.err" ;;
        duckdb) python3 "$DUCK" build --source "$WORK/ref/$f" --dir "$dir/dd" > "$dir/build.json" 2> "$dir/build.err" ;;
        pyarrow) python3 "$PYARROW" build --source "$WORK/ref/$f" --dir "$dir/pd" > "$dir/build.json" 2> "$dir/build.err" ;;
    esac
}

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

    for rep in $(seq 1 "$REPS"); do
        if [ $((rep % 2)) -eq 1 ]; then order="vole sqlite duckdb pyarrow"; else order="pyarrow duckdb sqlite vole"; fi
        for lane in $order; do
            case "$lane" in vole) dir="$VDIR" ;; sqlite) dir="$SDIR" ;; duckdb) dir="$DDIR" ;; pyarrow) dir="$PDIR" ;; esac
            t0=$(now_ns)
            build_lane "$f" "$lane" "$dir" "$PLAN"; rc=$?
            t1=$(now_ns)
            us=$(( (t1 - t0) / 1000 ))
            printf '%s\t%s\t%s\t%s\t%s\n' "$f" "$lane" "$rep" "$rc" "$us" >> "$RAW/build.tsv"
            [ "$rc" -eq 0 ] || echo "FINDING: build failed $f/$lane rc=$rc" >&2
        done
    done

    for lane in vole sqlite duckdb pyarrow; do
        case "$lane" in vole) dir="$VDIR" ;; sqlite) dir="$SDIR" ;; duckdb) dir="$DDIR" ;; pyarrow) dir="$PDIR" ;; esac
        bytes=$(find "$dir" -type f -printf '%s\n' | awk '{s+=$1} END{print s+0}')
        files=$(find "$dir" -type f | wc -l | tr -d ' ')
        printf '%s\t%s\t%s\t%s\n' "$f" "$lane" "$bytes" "$files" >> "$RAW/storage.tsv"
    done

    FIELD=$(python3 -c "import json,sys;print(json.load(open('$VDIR/build.json'))['ingest']['field'])" 2>/dev/null || echo "")

    for rep in $(seq 1 "$REPS"); do
        if [ $((rep % 2)) -eq 1 ]; then order="vole sqlite duckdb pyarrow"; else order="pyarrow duckdb sqlite vole"; fi
        for lane in $order; do
            if [ "$lane" = "vole" ]; then
                [ -n "$FIELD" ] || { echo "FINDING: no VOLE field for $f" >&2; continue; }
                python3 "$VOLE" run --bin "$BIN" --store "$VDIR/store" --field "$FIELD" \
                    --plan "$PLAN" --outdir "$VDIR/run$rep" --source "$WORK/corpus/$f" --packed >/dev/null
                for q in Q1 Q2 Q3 Q4 Q5 Q6; do
                    us=$(python3 -c "import json;print(json.load(open('$VDIR/run$rep/cold_us.json')).get('$q',0))" 2>/dev/null || echo 0)
                    printf '%s\t%s\t%s\t%s\t0\t%s\n' "$f" "$lane" "$q" "$rep" "$us" >> "$RAW/cold.tsv"
                    if [ "$rep" -eq 1 ]; then
                        cp "$VDIR/run$rep/qanswers/$q.json" "$RAW/qanswers/$f.$q.vole.json"
                    fi
                done
            elif [ "$lane" = "sqlite" ]; then
                for q in Q1 Q2 Q3 Q4 Q5 Q6; do
                    t0=$(now_ns)
                    python3 "$BASE" query --db "$SDIR/x.sqlite" --q "$q" --plan "$PLAN" \
                        --out "$SDIR/run${rep}_$q.json" >/dev/null 2>&1; rc=$?
                    t1=$(now_ns)
                    us=$(( (t1 - t0) / 1000 ))
                    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$f" "$lane" "$q" "$rep" "$rc" "$us" >> "$RAW/cold.tsv"
                    if [ "$rep" -eq 1 ]; then cp "$SDIR/run${rep}_$q.json" "$RAW/qanswers/$f.$q.sqlite.json"; fi
                done
            elif [ "$lane" = "duckdb" ]; then
                for q in Q1 Q2 Q3 Q4 Q5 Q6; do
                    t0=$(now_ns)
                    python3 "$DUCK" query --dir "$DDIR/dd" --q "$q" --plan "$PLAN" \
                        --out "$DDIR/run${rep}_$q.json" >/dev/null 2>&1; rc=$?
                    t1=$(now_ns)
                    us=$(( (t1 - t0) / 1000 ))
                    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$f" "$lane" "$q" "$rep" "$rc" "$us" >> "$RAW/cold.tsv"
                    if [ "$rep" -eq 1 ]; then cp "$DDIR/run${rep}_$q.json" "$RAW/qanswers/$f.$q.duckdb.json"; fi
                done
            else
                for q in Q1 Q2 Q3 Q4 Q5 Q6; do
                    t0=$(now_ns)
                    python3 "$PYARROW" query --dir "$PDIR/pd" --q "$q" --plan "$PLAN" \
                        --out "$PDIR/run${rep}_$q.json" >/dev/null 2>&1; rc=$?
                    t1=$(now_ns)
                    us=$(( (t1 - t0) / 1000 ))
                    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$f" "$lane" "$q" "$rep" "$rc" "$us" >> "$RAW/cold.tsv"
                    if [ "$rep" -eq 1 ]; then cp "$PDIR/run${rep}_$q.json" "$RAW/qanswers/$f.$q.pyarrow.json"; fi
                done
            fi
        done
    done

    # ---- Phase 28.2: steady-state (headline) + cold-cache + recorded plans ---
    if [ -n "$FIELD" ]; then
        python3 "$VOLE" steady --bin "$BIN" --store "$VDIR/store" --field "$FIELD" \
            --plan "$PLAN" --outdir "$VDIR/steady" --packed \
            --iters "$STEADY_ITERS" --warmup 2 >/dev/null 2>&1
        emit_steady "$f" "vole" "$VDIR/steady/steady_us.json"
    fi
    # VOLE's unit of execution is a fresh CLI process per question, so its cold-cache
    # coincides with its cold-process; the cold rows are reused as the cold-cache column.
    awk -F'\t' -v fx="$f" 'NR>1 && $1==fx && $2=="vole" && $4==1 {print $1"\t"$2"\t"$3"\t"$6}' \
        "$RAW/cold.tsv" >> "$RAW/coldcache.tsv"

    python3 "$BASE" steady --db "$SDIR/x.sqlite" --plan "$PLAN" --out "$SDIR/steady.json" \
        --iters "$STEADY_ITERS" --warmup 3 >/dev/null 2>&1
    emit_steady "$f" "sqlite" "$SDIR/steady.json"
    python3 "$BASE" coldcache --db "$SDIR/x.sqlite" --plan "$PLAN" --out "$SDIR/coldcache.json" >/dev/null 2>&1
    emit_coldcache "$f" "sqlite" "$SDIR/coldcache.json"
    python3 "$BASE" explain --db "$SDIR/x.sqlite" --plan "$PLAN" --out "$RAW/plans/$f.sqlite.explain.json" >/dev/null 2>&1

    python3 "$DUCK" steady --dir "$DDIR/dd" --plan "$PLAN" --out "$DDIR/steady.json" \
        --iters "$STEADY_ITERS" --warmup 3 >/dev/null 2>&1
    emit_steady "$f" "duckdb" "$DDIR/steady.json"
    python3 "$DUCK" coldcache --dir "$DDIR/dd" --plan "$PLAN" --out "$DDIR/coldcache.json" >/dev/null 2>&1
    emit_coldcache "$f" "duckdb" "$DDIR/coldcache.json"
    python3 "$DUCK" explain --dir "$DDIR/dd" --plan "$PLAN" --out "$RAW/plans/$f.duckdb.explain.json" >/dev/null 2>&1

    python3 "$PYARROW" steady --dir "$PDIR/pd" --plan "$PLAN" --out "$PDIR/steady.json" \
        --iters "$STEADY_ITERS" --warmup 3 >/dev/null 2>&1
    emit_steady "$f" "pyarrow" "$PDIR/steady.json"
    python3 "$PYARROW" coldcache --dir "$PDIR/pd" --plan "$PLAN" --out "$PDIR/coldcache.json" >/dev/null 2>&1
    emit_coldcache "$f" "pyarrow" "$PDIR/coldcache.json"
    python3 "$PYARROW" explain --dir "$PDIR/pd" --plan "$PLAN" --out "$RAW/plans/$f.pyarrow.explain.json" >/dev/null 2>&1

    # ---- exactness: delete the source AND the descriptor, then rematerialize --
    SRC_BYTES=$(awk -F'\t' -v fx="$f" 'NR>1 && $1==fx {print $2}' "$RAW/fixtures.tsv")
    SRC_SHA=$(awk -F'\t' -v fx="$f" 'NR>1 && $1==fx {print $3}' "$RAW/fixtures.tsv")
    rm -f "$WORK/corpus/$f" "$VDIR/desc.voldoc"

    t0=$(now_ns)
    "$BIN" materialize --store "$VDIR/store" --field "$FIELD" --exact --packed --output "$WORK/$f.vole.out" >/dev/null 2>&1; v_rc=$?
    t1=$(now_ns)
    v_us=$(( (t1 - t0) / 1000 ))
    t0=$(now_ns)
    python3 "$BASE" materialize --db "$SDIR/x.sqlite" --out "$WORK/$f.sql.out" >/dev/null 2>&1; s_rc=$?
    t1=$(now_ns)
    s_us=$(( (t1 - t0) / 1000 ))

    v_len=$(wc -c < "$WORK/$f.vole.out" 2>/dev/null | tr -d ' ')
    v_sha=$(sha256sum "$WORK/$f.vole.out" 2>/dev/null | cut -d' ' -f1)
    len_ok=false; [ "$v_len" = "$SRC_BYTES" ] && len_ok=true
    sha_ok=false; [ "$v_sha" = "$SRC_SHA" ] && sha_ok=true
    cmp_ok=false; cmp -s "$WORK/ref/$f" "$WORK/$f.vole.out" && cmp_ok=true
    vole_ok=false
    if [ "$v_rc" -eq 0 ] && [ "$len_ok" = true ] && [ "$sha_ok" = true ] && [ "$cmp_ok" = true ]; then vole_ok=true; fi
    sql_ok=false; cmp -s "$WORK/ref/$f" "$WORK/$f.sql.out" && sql_ok=true
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$f" "$vole_ok" "$sql_ok" "$len_ok" "$sha_ok" "$cmp_ok" "$v_us" "$s_us" >> "$RAW/exact.tsv"
    [ "$vole_ok" = true ] || echo "FINDING: VOLE not byte-exact for $f" >&2
done

DUCKDB_VERSION=$(python3 -c 'import duckdb; print(duckdb.__version__)')
PYARROW_VERSION=$(python3 -c 'import pyarrow; print(pyarrow.__version__)')
FIX_JSON=$(python3 -c "import json;rows=[l.split('\t') for l in open('$RAW/fixtures.tsv').read().splitlines()[1:] if l];print(json.dumps({r[0]:{'len':int(r[1]),'sha256':r[2]} for r in rows},sort_keys=True))")
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "28.2 — Arrow IPC economic court (competitor envelope; VOLE vs SQLite vs DuckDB vs pyarrow)",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "measured_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "tree_state": "$(git status --porcelain | tr '\n' ' ' | sed 's/"/\\"/g')",
  "service": "analytical",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "python": "$(python3 --version)",
  "sqlite3": "$(sqlite3 --version)",
  "duckdb_version": "$DUCKDB_VERSION",
  "pyarrow_version": "$PYARROW_VERSION",
  "pyarrow_basis": "hash-pinned cp311 manylinux_2_28 x86_64 wheel, --no-deps (pyarrow 25.0.1); the lane opens the Arrow IPC file directly",
  "duckdb_arrow": "Phase 28.2: DuckDB reads the REAL Arrow data through the Arrow C data interface (con.register of the pyarrow.ipc table), not a Parquet projection; PRAGMA threads=4",
  "envelope": {
    "headline": "in-process steady-state (warm connection/state); per-question median",
    "separate": ["cold-process (fresh OS process per question)", "cold-cache (fresh connection/reader, warm interpreter)"],
    "sqlite": "typed columns; covering predicate index; FTS5 external-content trigram index for the substring question (case_sensitive_like=ON) with an instr() cross-check; cache_size=64MiB; mmap; WITHOUT ROWID considered and declined",
    "duckdb": "real Arrow via the Arrow C data interface; column projection; predicate execution; prepared statements; threads=4; no zonemap pruning (Arrow stores no statistics)",
    "pyarrow": "Arrow IPC opened directly; column projection; vector kernels; persistent reader",
    "vole": "unchanged archival field (typed observations); steady-state = repeated warm CLI invocations"
  },
  "arch": "$(uname -m)",
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "bin": "$BIN",
  "bin_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "profile": "$PROFILE",
  "build_args": "$BUILD_ARGS",
  "vole_substrate": "--profile runtime --packed",
  "fixtures_len_sha256": $FIX_JSON,
  "reps": $REPS,
  "steady_iters": $STEADY_ITERS,
  "historical_control": "evidence/campaigns/2026-10-12-phase21-16-arrow-econ-efb51887 (pre-28.2, 26.6)",
  "storage_accounting": "sum of regular-file sizes (find -type f -printf '%s'); du -sb never used",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY analytical bash tools/phase21-16-arrow-court.sh
# PROFILE defaults to release (target/release/vole-document).
# Inside the court:
#   cargo build --release --locked --all-features
#   python3 tools/fixtures/make-arrow.py --econ $WORK/corpus   # .arrow (+ a .parquet projection for the SQLite table load)
#   VOLE:   $BIN field-build F.arrow --store STORE --voldoc F.voldoc --profile runtime --packed
#           python3 tools/fixtures/phase21-16-arrow-vole.py run --bin $BIN --store STORE --field HEX --plan ... --outdir ... --packed
#           python3 tools/fixtures/phase21-16-arrow-vole.py steady --bin $BIN --store STORE --field HEX --plan ... --outdir ... --packed
#           rm source + descriptor ; $BIN materialize --store STORE --field HEX --exact --packed --output OUT ; cmp OUT source
#   SQLite: python3 tools/fixtures/phase21-16-arrow-baseline.py build --source F.arrow --table-source F.parquet --db D --plan ...
#           python3 tools/fixtures/phase21-16-arrow-baseline.py query|steady|coldcache|explain --db D ...
#           python3 tools/fixtures/phase21-16-arrow-baseline.py materialize --db D --out OUT
#   DuckDB: python3 tools/fixtures/phase21-16-arrow-duckdb.py build --source F.arrow --dir DD
#           python3 tools/fixtures/phase21-16-arrow-duckdb.py query|steady|coldcache|explain --dir DD ...
#   pyarrow: python3 tools/fixtures/phase21-16-arrow-pyarrow.py build --source F.arrow --dir PD
#           python3 tools/fixtures/phase21-16-arrow-pyarrow.py query|steady|coldcache|explain --dir PD ...
#   aggregate: python3 tools/fixtures/phase21-16-arrow-baseline.py aggregate --raw RAW --campaign CAMPAIGN --env ENV
EOF

echo "=== aggregate ==="
python3 "$BASE" aggregate --raw "$RAW" --campaign "$CAMPAIGN" --env "$CAMPAIGN/environment.json"
verdict_rc=$?

# Bound oversized raw evidence: the large fixture's verbatim Q1 dumps are replaced
# with length+SHA-256+prefix receipts; the full dumps are gitignored.
python3 tools/fixtures/phase21-16-bound.py "$RAW"

EXACT_OK=$(awk -F'\t' 'NR>1 && $2=="true"' "$RAW/exact.tsv" | wc -l | tr -d ' ')
EXACT_N=$(awk -F'\t' 'NR>1' "$RAW/exact.tsv" | wc -l | tr -d ' ')

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
if [ "$verdict_rc" -ne 0 ] || [ "$EXACT_OK" -ne "$EXACT_N" ] || [ "$EXACT_N" -eq 0 ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "28.2 — Arrow IPC analytical economic court (competitor envelope)",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "measured_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "tree_state": "$(git status --porcelain | tr '\n' ' ' | sed 's/"/\\"/g')",
  "service": "analytical",
  "profile": "$PROFILE",
  "bin": "$BIN",
  "bin_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "duckdb_version": "$DUCKDB_VERSION",
  "pyarrow_version": "$PYARROW_VERSION",
  "lanes": ["vole", "sqlite", "duckdb", "pyarrow"],
  "questions": {"Q1": "column values", "Q2": "predicate count", "Q3": "buffer span + min/max", "Q4": "record-batch count", "Q5": "lexical find count", "Q6": "exact closure"},
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion",
  "exact_ok": $EXACT_OK,
  "exact_n": $EXACT_N,
  "headline_metric": "in-process steady-state (warm connection/state; per-question median)",
  "separate_metrics": ["cold-process", "cold-cache"],
  "verdict_rule": "FAIL only if VOLE exactness is not 100% (or a cross-lane mismatch is recorded)",
  "honest_scope": "self-authored corpus; only Q6 is a byte-authority claim; DuckDB now reads the real Arrow data through the Arrow C data interface (Phase 28.2), and the pyarrow lane opens the Arrow IPC file directly; both decline Q3 (Arrow stores no per-column statistics/span) and Q6 (not a source-retaining store)",
  "verdict": "$verdict"
}
EOF

{
    echo "# Phase 28.2 — Arrow IPC (analytical Wave 2) economic court — competitor envelope"
    echo
    echo "**Question.** On a self-authored Arrow IPC corpus, how do VOLE (an archival"
    echo "field with typed observations), a source-retaining SQLite baseline, a DuckDB"
    echo "analytical baseline over the **real Arrow data**, and a pyarrow lane over the"
    echo "Arrow IPC file itself answer six contract questions, and does VOLE still close"
    echo "**exactly**?"
    echo
    echo "**Headline (recorded plainly).** pyarrow — the reference Arrow reader, opening"
    echo "the \`.arrow\` file directly — and DuckDB over the **real Arrow data** (Phase 28.2:\n"
    echo "the Arrow C data interface, not a Parquet projection) both win the analytical"
    echo "computation axes (columnar projection, predicate execution), while VOLE"
    echo "materializes a bounded observation and counts in the driver. VOLE's unique claims"
    echo "are exactness (Q6, byte-authority) and the exact buffer span (Q3) from the archival"
    echo "metadata; neither DuckDB nor pyarrow is a source-retaining store (Q6 declines"
    echo "\`not-native\`) and Arrow IPC stores no per-column statistics or source span (Q3"
    echo "declines \`not-retained\`)."
    echo
    echo "**Measurement (28.2).** The headline is **in-process steady-state** (warm"
    echo "connection/state; per-question median), reported separately from **cold-process**"
    echo "(fresh OS process per question) and **cold-cache** (fresh connection/reader, warm"
    echo "interpreter). Percentiles are in the matrix. The pre-28.2 numbers are kept as an"
    echo "explicit historical control."
    echo
    echo "## Matrix"
    echo
    tail -n +2 "$CAMPAIGN/MATRIX.md"
    echo
    echo "## Verdict"
    echo
    echo "- VOLE exactness: **${EXACT_OK}/${EXACT_N}**."
    echo "- Campaign: \`$CAMPAIGN\`."
    echo "- Profile: \`$PROFILE\`; VOLE substrate \`--profile runtime --packed\`."
    echo "- Historical control (pre-28.2): \`evidence/campaigns/2026-10-12-phase21-16-arrow-econ-efb51887\`."
    echo "- Never run on the host: every command ran in the pinned, capped \`analytical\` container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" = "PASS" ]; then
    echo "PHASE 28.2 ARROW ECON COURT: PASS — VOLE exactness ${EXACT_OK}/${EXACT_N} — campaign $CAMPAIGN"
    exit 0
fi
echo "PHASE 28.2 ARROW ECON COURT: FAIL — VOLE exactness ${EXACT_OK}/${EXACT_N} — campaign $CAMPAIGN" >&2
exit 1
