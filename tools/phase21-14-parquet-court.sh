#!/usr/bin/env bash
# Phase 21.14 — the Parquet economic court (completes subphase 21.14).
#
# Pre-registered. Hypotheses:
#
#   H1 (byte-exactness / Q6) — for every corpus fixture, VOLE
#       `materialize --exact == source` (length + SHA-256 + `cmp`) after the source
#       file AND the standalone descriptor are deleted, in a fresh process. The court
#       FAILS unless this is 100 %.
#   H2 (contract questions) — the three lanes (VOLE, a source-retaining SQLite
#       baseline, a DuckDB/Parquet analytical baseline) answer the SAME six
#       questions Q1–Q6 where they can, and every question a lane cannot answer is a
#       TYPED decline, never a silent empty answer.
#   H3 (cross-lane agreement) — where all lanes answer, comparable values agree (a
#       column's values, a predicate count, a lexical-find count, a column chunk's
#       min/max statistics). VOLE answers Q6 (exact raw bytes) and Q3 (the exact
#       chunk span); the baselines decline or partially answer those, and the gaps
#       are recorded, never papered over.
#   H4 (economics) — build/storage/cold/warm are measured per lane with a stated
#       estimator (paired per-fixture ratios, median + geometric mean, fixed-seed
#       cluster bootstrap by fixture; ratio-of-sums reported separately), every raw
#       sample retained. Wall times are microseconds (`us`).
#
# DuckDB/Parquet is the **mandatory** analytical comparator and will very likely win
# the analytical axes (projection, predicate execution, compressed pages, selective
# reads); the court says so plainly. It is not a source-retaining store, so Q6 is a
# typed `not-native` decline. VOLE is an archival field with typed observations, not
# a query engine; its predicate/find counts are a decoded column plus a count in the
# driver.
#
# Honest scope: a self-authored corpus; only Q6 is a byte-authority claim.
#
# Fairness: the comparators (SQLite C, DuckDB, Python) are unaffected by the Rust
# profile, so the court defaults to a RELEASE build (`PROFILE=release`); VOLE uses
# `field-build --profile runtime --packed`, with `--packed` on every
# observe/observe-batch/materialize.
#
# Runs in the pinned, hard-capped `analytical` service (dev toolchain + python3 +
# sqlite3 + hash-pinned duckdb); never the host:
#
#   docker compose run --rm --no-TTY analytical bash tools/phase21-14-parquet-court.sh

set -uo pipefail
cd /work
export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1

BASE=tools/fixtures/phase21-14-parquet-baseline.py
DUCK=tools/fixtures/phase21-14-parquet-duckdb.py
VOLE=tools/fixtures/phase21-14-parquet-vole.py
SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-14-parquet-econ-${SHA}"
RAW="$CAMPAIGN/raw"
WORK="evidence/scratch/phase21-14-econ"
REPS=${REPS:-3}

PROFILE=${PROFILE:-release}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-${VOLE_BIN:-target/debug/vole-document}} ;;
esac

rm -rf "$CAMPAIGN" "$WORK"
mkdir -p "$RAW/qanswers" "$WORK/corpus" "$WORK/ref"

echo "=== build ($PROFILE) ==="
# shellcheck disable=SC2086
if ! cargo build $BUILD_ARGS -j 1 2>&1 | tail -2; then
    echo "court: cargo build FAILED ($BUILD_ARGS); refusing to measure" >&2
    exit 1
fi
[ -x "$BIN" ] || { echo "court: $BIN missing; refusing to measure" >&2; exit 1; }
echo "-- measuring $BIN ($PROFILE; VOLE substrate: --profile runtime --packed)"

echo "=== regenerate deterministic corpus ==="
python3 tools/fixtures/make-parquet.py --corpus "$WORK/corpus" | tee "$RAW/corpus.txt"

{
    printf 'fixture\tsrc_bytes\tsrc_sha256\n'
    while IFS=$'\t' read -r name bytes sha; do
        case "$name" in
            unsupported_codec.parquet|unsupported_encoding.parquet|bomb.parquet|prose.txt|truncated.parquet|badlen.parquet) continue ;;
        esac
        printf '%s\t%s\t%s\n' "$name" "$bytes" "$sha"
    done < "$RAW/corpus.txt"
} > "$RAW/fixtures.tsv"
cp -a "$WORK/corpus"/*.parquet "$WORK/ref/" 2>/dev/null || true

FIXTURES=$(awk 'NR>1{print $1}' "$RAW/fixtures.tsv")

now_ns() { date +%s%N; }

# --- per-fixture plan (FROZEN): "COL_NUM GT COL_STR SUBSTR" -------------------
plan_for() {
    case "$1" in
        small_plain.parquet) echo "1 5 5 a" ;;
        optional.parquet)    echo "0 3 1 a" ;;
        dictionary.parquet)  echo "1 10 0 rg" ;;
        gzip.parquet)        echo "1 5 5 a" ;;
        multi_rg.parquet)    echo "0 10 1 row-01" ;;
        two_pages.parquet)   echo "0 100 1 t1" ;;
        large.parquet)       echo "0 500000 2 record-00001" ;;
        *)                   echo "0 0 0 z" ;;
    esac
}

printf 'fixture\tlane\trep\trc\tus\n' > "$RAW/build.tsv"
printf 'fixture\tlane\tbytes\tfiles\n' > "$RAW/storage.tsv"
printf 'fixture\tlane\tq\trep\trc\tus\n' > "$RAW/cold.tsv"
printf 'fixture\tlane\tq\trep\trc\tus\n' > "$RAW/warm.tsv"
printf 'fixture\tvole_ok\tsqlite_ok\tlen_ok\tsha_ok\tcmp_ok\tv_us\ts_us\n' > "$RAW/exact.tsv"

build_lane() { # fixture lane dir
    local f="$1" lane="$2" dir="$3"
    rm -rf "$dir"; mkdir -p "$dir"
    case "$lane" in
        vole)   "$BIN" field-build "$WORK/corpus/$f" --store "$dir/store" --voldoc "$dir/desc.voldoc" \
                    --profile runtime --packed > "$dir/build.json" 2> "$dir/build.err" ;;
        sqlite) python3 "$BASE" build --source "$WORK/corpus/$f" --db "$dir/x.sqlite" > "$dir/build.json" 2> "$dir/build.err" ;;
        duckdb) python3 "$DUCK" build --source "$WORK/corpus/$f" --dir "$dir/dd" > "$dir/build.json" 2> "$dir/build.err" ;;
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

    for rep in $(seq 1 "$REPS"); do
        if [ $((rep % 2)) -eq 1 ]; then order="vole sqlite duckdb"; else order="duckdb sqlite vole"; fi
        for lane in $order; do
            case "$lane" in vole) dir="$VDIR" ;; sqlite) dir="$SDIR" ;; duckdb) dir="$DDIR" ;; esac
            t0=$(now_ns)
            build_lane "$f" "$lane" "$dir"; rc=$?
            t1=$(now_ns)
            us=$(( (t1 - t0) / 1000 ))
            printf '%s\t%s\t%s\t%s\t%s\n' "$f" "$lane" "$rep" "$rc" "$us" >> "$RAW/build.tsv"
            [ "$rc" -eq 0 ] || echo "FINDING: build failed $f/$lane rc=$rc" >&2
        done
    done

    for lane in vole sqlite duckdb; do
        case "$lane" in vole) dir="$VDIR" ;; sqlite) dir="$SDIR" ;; duckdb) dir="$DDIR" ;; esac
        bytes=$(find "$dir" -type f -printf '%s\n' | awk '{s+=$1} END{print s+0}')
        files=$(find "$dir" -type f | wc -l | tr -d ' ')
        printf '%s\t%s\t%s\t%s\n' "$f" "$lane" "$bytes" "$files" >> "$RAW/storage.tsv"
    done

    FIELD=$(python3 -c "import json,sys;print(json.load(open('$VDIR/build.json'))['ingest']['field'])" 2>/dev/null || echo "")

    for rep in $(seq 1 "$REPS"); do
        if [ $((rep % 2)) -eq 1 ]; then order="vole sqlite duckdb"; else order="duckdb sqlite vole"; fi
        for lane in $order; do
            if [ "$lane" = "vole" ]; then
                [ -n "$FIELD" ] || { echo "FINDING: no VOLE field for $f" >&2; continue; }
                python3 "$VOLE" run --bin "$BIN" --store "$VDIR/store" --field "$FIELD" \
                    --plan "$PLAN" --outdir "$VDIR/run$rep" --source "$WORK/corpus/$f" --packed >/dev/null
                for q in Q1 Q2 Q3 Q4 Q5 Q6; do
                    us=$(python3 -c "import json;print(json.load(open('$VDIR/run$rep/cold_us.json')).get('$q',0))" 2>/dev/null || echo 0)
                    printf '%s\t%s\t%s\t%s\t0\t%s\n' "$f" "$lane" "$q" "$rep" "$us" >> "$RAW/cold.tsv"
                    wus=$(python3 -c "import json;print(json.load(open('$VDIR/run$rep/warm_us.json')).get('$q',0))" 2>/dev/null || echo 0)
                    printf '%s\t%s\t%s\t%s\t0\t%s\n' "$f" "$lane" "$q" "$rep" "$wus" >> "$RAW/warm.tsv"
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
                python3 "$BASE" session --db "$SDIR/x.sqlite" --queries Q1,Q2,Q3,Q4,Q5,Q6 \
                    --plan "$PLAN" --out "$SDIR/sess$rep.json" >/dev/null 2>&1
                for q in Q1 Q2 Q3 Q4 Q5 Q6; do
                    wus=$(python3 -c "import json;b=json.load(open('$SDIR/sess$rep.json'))['batch'];print(next((x['us'] for x in b if x['q']=='$q'),0))" 2>/dev/null || echo 0)
                    printf '%s\t%s\t%s\t%s\t0\t%s\n' "$f" "$lane" "$q" "$rep" "$wus" >> "$RAW/warm.tsv"
                done
            else
                for q in Q1 Q2 Q3 Q4 Q5 Q6; do
                    t0=$(now_ns)
                    python3 "$DUCK" query --dir "$DDIR/dd" --q "$q" --plan "$PLAN" \
                        --out "$DDIR/run${rep}_$q.json" >/dev/null 2>&1; rc=$?
                    t1=$(now_ns)
                    us=$(( (t1 - t0) / 1000 ))
                    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$f" "$lane" "$q" "$rep" "$rc" "$us" >> "$RAW/cold.tsv"
                    if [ "$rep" -eq 1 ]; then cp "$DDIR/run${rep}_$q.json" "$RAW/qanswers/$f.$q.duckdb.json"; fi
                done
                python3 "$DUCK" session --dir "$DDIR/dd" --queries Q1,Q2,Q3,Q4,Q5,Q6 \
                    --plan "$PLAN" --out "$DDIR/sess$rep.json" >/dev/null 2>&1
                for q in Q1 Q2 Q3 Q4 Q5 Q6; do
                    wus=$(python3 -c "import json;b=json.load(open('$DDIR/sess$rep.json'))['batch'];print(next((x['us'] for x in b if x['q']=='$q'),0))" 2>/dev/null || echo 0)
                    printf '%s\t%s\t%s\t%s\t0\t%s\n' "$f" "$lane" "$q" "$rep" "$wus" >> "$RAW/warm.tsv"
                done
            fi
        done
    done

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
FIX_JSON=$(python3 -c "import json;rows=[l.split('\t') for l in open('$RAW/fixtures.tsv').read().splitlines()[1:] if l];print(json.dumps({r[0]:{'len':int(r[1]),'sha256':r[2]} for r in rows},sort_keys=True))")
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.14 — Parquet economic court (VOLE vs SQLite vs DuckDB)",
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
  "arch": "$(uname -m)",
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "bin": "$BIN",
  "bin_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "profile": "$PROFILE",
  "build_args": "$BUILD_ARGS",
  "vole_substrate": "--profile runtime --packed",
  "fixtures_len_sha256": $FIX_JSON,
  "reps": $REPS,
  "storage_accounting": "sum of regular-file sizes (find -type f -printf '%s'); du -sb never used",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY analytical bash tools/phase21-14-parquet-court.sh
# PROFILE defaults to release (target/release/vole-document).
# Inside the court:
#   cargo build --release --locked --all-features
#   python3 tools/fixtures/make-parquet.py --corpus $WORK/corpus
#   VOLE:   $BIN field-build F --store STORE --voldoc F.voldoc --profile runtime --packed
#           python3 tools/fixtures/phase21-14-parquet-vole.py run --bin $BIN --store STORE --field HEX --plan ... --outdir ... --packed
#           rm source + descriptor ; $BIN materialize --store STORE --field HEX --exact --packed --output OUT ; cmp OUT source
#   SQLite: python3 tools/fixtures/phase21-14-parquet-baseline.py build --source F --db D
#           python3 tools/fixtures/phase21-14-parquet-baseline.py query --db D --q Qn --plan ... --out OUT
#           python3 tools/fixtures/phase21-14-parquet-baseline.py materialize --db D --out OUT
#   DuckDB: python3 tools/fixtures/phase21-14-parquet-duckdb.py build --source F --dir DD
#           python3 tools/fixtures/phase21-14-parquet-duckdb.py query --dir DD --q Qn --plan ... --out OUT
#   aggregate: python3 tools/fixtures/phase21-14-parquet-baseline.py aggregate --raw RAW --campaign CAMPAIGN --env ENV
# Full dev gate:
#   cargo fmt --all --check
#   cargo clippy --all-targets --all-features -- -D warnings
#   cargo test --locked --all-features
EOF

echo "=== aggregate ==="
python3 "$BASE" aggregate --raw "$RAW" --campaign "$CAMPAIGN" --env "$CAMPAIGN/environment.json"
verdict_rc=$?

# Bound oversized raw evidence: the large fixture's verbatim Q1 dumps are replaced
# with length+SHA-256+prefix receipts; the full dumps are gitignored.
python3 tools/fixtures/phase21-14-bound.py "$RAW"

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
  "phase": "21.14 — Parquet analytical economic court",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "measured_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "tree_state": "$(git status --porcelain | tr '\n' ' ' | sed 's/"/\\"/g')",
  "service": "analytical",
  "profile": "$PROFILE",
  "bin": "$BIN",
  "bin_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "duckdb_version": "$DUCKDB_VERSION",
  "lanes": ["vole", "sqlite", "duckdb"],
  "questions": {"Q1": "column values", "Q2": "predicate count", "Q3": "chunk span + min/max statistics", "Q4": "row-group count", "Q5": "lexical find count", "Q6": "exact closure"},
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion",
  "exact_ok": $EXACT_OK,
  "exact_n": $EXACT_N,
  "verdict_rule": "FAIL only if VOLE exactness is not 100% (or a cross-lane mismatch is recorded)",
  "honest_scope": "self-authored corpus; only Q6 is a byte-authority claim; DuckDB wins the analytical axes",
  "verdict": "$verdict"
}
EOF

{
    echo "# Phase 21.14 — Parquet (analytical Wave 2) economic court"
    echo
    echo "**Question.** On a self-authored Parquet corpus, how do VOLE (an archival"
    echo "field with typed observations), a source-retaining SQLite baseline, and the"
    echo "mandatory DuckDB/Parquet analytical comparator answer six contract questions,"
    echo "and does VOLE still close **exactly**?"
    echo
    echo "**Headline (recorded plainly).** DuckDB wins the analytical axes: columnar"
    echo "projection, predicate execution, compressed pages, and selective reads via"
    echo "metadata statistics are native to it, while VOLE materializes a bounded"
    echo "observation and counts in the driver. VOLE's unique claims are exactness (Q6,"
    echo "byte-authority) and the exact chunk span (Q3) from the archival footer;"
    echo "DuckDB is not a source-retaining store (Q6 declines \`not-native\`)."
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
    echo "- Never run on the host: every command ran in the pinned, capped \`analytical\` container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" = "PASS" ]; then
    echo "PHASE 21.14 PARQUET ECON COURT: PASS — VOLE exactness ${EXACT_OK}/${EXACT_N} — campaign $CAMPAIGN"
    exit 0
fi
echo "PHASE 21.14 PARQUET ECON COURT: FAIL — VOLE exactness ${EXACT_OK}/${EXACT_N} — campaign $CAMPAIGN" >&2
exit 1
