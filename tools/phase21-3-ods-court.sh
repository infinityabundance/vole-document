#!/usr/bin/env bash
# Phase 21.3.2 — the ODS (OpenDocument Spreadsheet) economic court.
#
# Pre-registered. Hypotheses:
#
#   H1 (byte-exactness / Q10) — for every corpus fixture, VOLE
#       `materialize --exact == source` (length + SHA-256 + `cmp`) after the
#       source file AND the standalone descriptor are deleted, in a fresh
#       process. The court FAILS unless this is 100 %.
#   H2 (contract questions) — the three lanes (VOLE, a source-retaining SQLite
#       baseline, a DuckDB/Parquet analytical baseline) answer the SAME ten
#       questions Q1–Q10 where they can, and every question a lane cannot answer
#       is a TYPED decline, never a silent empty answer.
#   H3 (cross-lane agreement) — where all lanes answer, their comparable values
#       agree (typed cell value, stored formula, containing named range, resolved
#       cell style, content part, named-expression references, exact decoded cell
#       XML, embedded resource bytes, original bytes). VOLE is expected to decline
#       Q3 (formula dependents — no formula evaluation) and Q9 (no ODS-native
#       resource/media selector); those capability gaps are recorded, never
#       papered over.
#   H4 (economics) — build/storage/cold/warm are measured per lane with a stated
#       estimator (paired per-fixture ratios, median + geometric mean, fixed-seed
#       cluster bootstrap by fixture), every raw sample retained.
#
# ODS is a tabular/analytical format, so the DuckDB/Parquet comparator is
# REQUIRED alongside SQLite (the Phase-21 plan's Wave-2 rule). DuckDB is a
# *comparator* for columnar/tabular questions: it does NOT provide exact-source
# closure or provenance, and the court says so plainly (Q8/Q9/Q10 are typed
# declines there, and Q10's stored-blob passthrough is flagged non-native).
#
# Fairness: the comparators (SQLite C, DuckDB, Python) are unaffected by the Rust
# profile, so the court defaults to a RELEASE build (`PROFILE=release`) and
# measures `target/release/vole-document` (same convention as
# `tools/phase22-2-court.sh`). The VOLE lane uses the established economic
# substrate: `field-build --profile runtime --packed`, with `--packed` on every
# `observe`/`observe-batch`/`materialize --exact`. Wall times are microseconds
# (`us`), and every table in SUMMARY.md is labelled in microseconds.
#
# Runs in the pinned, hard-capped `analytical` service (dev toolchain + python3 +
# sqlite3 + hash-pinned duckdb 1.5.6); never the host:
#
#   docker compose run --rm --no-TTY analytical bash tools/phase21-3-ods-court.sh

set -uo pipefail
cd /work
export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1

BASE=tools/fixtures/phase21-3-ods-baseline.py
DUCK=tools/fixtures/phase21-3-ods-duckdb.py
VOLE=tools/fixtures/phase21-3-ods-vole.py
MAKE=tools/fixtures/make-ods.py
SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-3-2-ods-${SHA}"
RAW="$CAMPAIGN/raw"
WORK="evidence/scratch/phase21-3-2"
REPS=${REPS:-3}

# The comparators (SQLite C, DuckDB, Python) are unaffected by the Rust profile;
# a debug entropyfs build is not. Default to RELEASE so the comparison is fair to
# VOLE (identical convention to tools/phase22-2-court.sh).
PROFILE=${PROFILE:-release}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-${VOLE_BIN:-target/debug/vole-document}} ;;
esac

rm -rf "$CAMPAIGN" "$WORK"
mkdir -p "$RAW/qanswers" "$RAW/vole_warm" "$RAW/vole_surface" "$WORK/corpus" "$WORK/ref"

echo "=== build ($PROFILE) ==="
# shellcheck disable=SC2086
if ! cargo build $BUILD_ARGS 2>&1 | tail -2; then
    echo "court: cargo build FAILED ($BUILD_ARGS); refusing to measure" >&2
    exit 1
fi
[ -x "$BIN" ] || { echo "court: $BIN missing; refusing to measure" >&2; exit 1; }
echo "-- measuring $BIN ($PROFILE; VOLE substrate: --profile runtime --packed)"

echo "=== regenerate deterministic corpus ==="
python3 "$MAKE" --corpus "$WORK/corpus" | tee "$RAW/corpus.txt"
cp -a "$WORK/corpus"/*.ods "$WORK/ref/"

# fixtures.tsv: fixture  src_bytes  src_sha256
{
    printf 'fixture\tsrc_bytes\tsrc_sha256\n'
    while IFS=$'\t' read -r name bytes sha; do
        [ -n "$name" ] && printf '%s\t%s\t%s\n' "$name" "$bytes" "$sha"
    done < "$RAW/corpus.txt"
} > "$RAW/fixtures.tsv"

FIXTURES=$(awk 'NR>1{print $1}' "$RAW/fixtures.tsv")

now_ns() { date +%s%N; }

# --- per-fixture plan (FROZEN; chosen to exercise each feature) --------------
# sheet cell dep q4cell find
plan_for() {
    case "$1" in
        c01-basic.ods)    echo "0 D2 B2 C5 item" ;;
        c02-typed.ods)    echo "0 E1 A1 A1 label" ;;
        c03-repeated.ods) echo "0 B1 A1 A1 x" ;;
        c04-merged.ods)   echo "0 A1 A1 A1 merged" ;;
        c05-named.ods)    echo "0 A1 A1 B2 5" ;;
        c06-comments.ods) echo "0 B1 A1 A1 cell" ;;
        c07-styles.ods)   echo "0 C1 A1 A1 s" ;;
        c08-large.ods)    echo "0 G3000 A6000 A1 123" ;;
        *)                echo "0 A1 A1 A1 a" ;;
    esac
}

# --- TSV headers -------------------------------------------------------------
printf 'fixture\tlane\trep\trc\tus\n' > "$RAW/build.tsv"
printf 'fixture\tlane\tbytes\tfiles\n' > "$RAW/storage.tsv"
printf 'fixture\tlane\tq\trep\trc\tus\n' > "$RAW/cold.tsv"
printf 'fixture\tlane\tq\trep\trc\tus\n' > "$RAW/warm.tsv"
printf 'fixture\tvole_ok\tsqlite_ok\tlen_ok\tsha_ok\tcmp_ok\tv_us\ts_us\n' > "$RAW/exact.tsv"
printf 'fixture\tsurface\trc\tok\n' > "$RAW/vole_surface.tsv"

build_lane() { # fixture lane dir
    local f="$1" lane="$2" dir="$3"
    rm -rf "$dir"; mkdir -p "$dir"
    case "$lane" in
        vole)   "$BIN" field-build "$WORK/corpus/$f" --store "$dir/store" --voldoc "$dir/desc.voldoc" \
                    --profile runtime --packed > "$dir/build.json" 2> "$dir/build.err" ;;
        sqlite) python3 "$BASE" build --source "$WORK/corpus/$f" --db "$dir/x.sqlite" --through 5 > "$dir/build.json" 2> "$dir/build.err" ;;
        duckdb) python3 "$DUCK" build --source "$WORK/corpus/$f" --dir "$dir/dd" > "$dir/build.json" 2> "$dir/build.err" ;;
    esac
}

for f in $FIXTURES; do
    echo "=== $f ==="
    read -r SHEET CELL DEP Q4 FIND <<EOF
$(plan_for "$f")
EOF
    PLAN="{\"sheet\":$SHEET,\"cell\":\"$CELL\",\"dep\":\"$DEP\",\"q4cell\":\"$Q4\",\"find\":\"$FIND\"}"

    VDIR="$WORK/lanes/$f.vole"
    SDIR="$WORK/lanes/$f.sqlite"
    DDIR="$WORK/lanes/$f.duckdb"

    # ---- build (best-of-N, interleaved lane order per rep) ------------------
    for rep in $(seq 1 "$REPS"); do
        if [ $((rep % 2)) -eq 1 ]; then order="vole sqlite duckdb"; else order="duckdb sqlite vole"; fi
        for lane in $order; do
            case "$lane" in
                vole) dir="$VDIR" ;; sqlite) dir="$SDIR" ;; duckdb) dir="$DDIR" ;;
            esac
            t0=$(now_ns)
            build_lane "$f" "$lane" "$dir"; rc=$?
            t1=$(now_ns)
            us=$(( (t1 - t0) / 1000 ))
            printf '%s\t%s\t%s\t%s\t%s\n' "$f" "$lane" "$rep" "$rc" "$us" >> "$RAW/build.tsv"
            [ "$rc" -eq 0 ] || echo "FINDING: build failed $f/$lane rc=$rc" >&2
        done
    done

    # ---- storage (sum of regular-file sizes; never du -sb) ------------------
    for lane in vole sqlite duckdb; do
        case "$lane" in vole) dir="$VDIR" ;; sqlite) dir="$SDIR" ;; duckdb) dir="$DDIR" ;; esac
        bytes=$(find "$dir" -type f -printf '%s\n' | awk '{s+=$1} END{print s+0}')
        files=$(find "$dir" -type f | wc -l | tr -d ' ')
        printf '%s\t%s\t%s\t%s\n' "$f" "$lane" "$bytes" "$files" >> "$RAW/storage.tsv"
    done

    FIELD=$(python3 -c "import json,sys;print(json.load(open('$VDIR/build.json'))['ingest']['field'])" 2>/dev/null || echo "")

    # ---- query phase (cold + warm), interleaved lane order per rep ----------
    for rep in $(seq 1 "$REPS"); do
        if [ $((rep % 2)) -eq 1 ]; then order="vole sqlite duckdb"; else order="duckdb sqlite vole"; fi
        for lane in $order; do
            if [ "$lane" = "vole" ]; then
                [ -n "$FIELD" ] || { echo "FINDING: no VOLE field for $f" >&2; continue; }
                python3 "$VOLE" run --bin "$BIN" --store "$VDIR/store" --field "$FIELD" \
                    --plan "$PLAN" --outdir "$VDIR/run$rep" --source "$WORK/corpus/$f" --packed >/dev/null
                for q in Q1 Q2 Q3 Q4 Q5 Q6 Q7 Q8 Q9 Q10; do
                    us=$(python3 -c "import json;print(json.load(open('$VDIR/run$rep/cold_us.json')).get('$q',0))")
                    printf '%s\t%s\t%s\t%s\t0\t%s\n' "$f" "$lane" "$q" "$rep" "$us" >> "$RAW/cold.tsv"
                    wus=$(python3 -c "import json;print(json.load(open('$VDIR/run$rep/warm_us.json')).get('$q',0))" 2>/dev/null || echo 0)
                    printf '%s\t%s\t%s\t%s\t0\t%s\n' "$f" "$lane" "$q" "$rep" "$wus" >> "$RAW/warm.tsv"
                    if [ "$rep" -eq 1 ]; then
                        cp "$VDIR/run$rep/qanswers/$q.json" "$RAW/qanswers/$f.$q.vole.json"
                        cp "$VDIR/run$rep/warm/$q.json" "$RAW/vole_warm/$f.$q.json" 2>/dev/null || true
                    fi
                done
                if [ "$rep" -eq 1 ]; then
                    cp "$VDIR/run$rep/surface.json" "$RAW/vole_surface/$f.json" 2>/dev/null || true
                    python3 - "$RAW/vole_surface/$f.json" "$f" >> "$RAW/vole_surface.tsv" <<'PY'
import json, sys
p, fx = sys.argv[1], sys.argv[2]
try:
    d = json.load(open(p))
except OSError:
    sys.exit(0)
for name in sorted(d):
    s = d[name]
    print(f"{fx}\t{name}\t{s.get('rc')}\t{1 if s.get('ok') else 0}")
PY
                fi
            elif [ "$lane" = "sqlite" ]; then
                for q in Q1 Q2 Q3 Q4 Q5 Q6 Q7 Q8 Q9 Q10; do
                    t0=$(now_ns)
                    python3 "$BASE" query --db "$SDIR/x.sqlite" --q "$q" --plan "$PLAN" \
                        --out "$SDIR/run${rep}_$q.json" >/dev/null 2>&1; rc=$?
                    t1=$(now_ns)
                    us=$(( (t1 - t0) / 1000 ))
                    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$f" "$lane" "$q" "$rep" "$rc" "$us" >> "$RAW/cold.tsv"
                    if [ "$rep" -eq 1 ]; then cp "$SDIR/run${rep}_$q.json" "$RAW/qanswers/$f.$q.sqlite.json"; fi
                done
                python3 "$BASE" session --db "$SDIR/x.sqlite" --queries Q1,Q2,Q3,Q4,Q5,Q6,Q7,Q8,Q9,Q10 \
                    --plan "$PLAN" --out "$SDIR/sess$rep.json" >/dev/null 2>&1
                for q in Q1 Q2 Q3 Q4 Q5 Q6 Q7 Q8 Q9 Q10; do
                    wus=$(python3 -c "import json;b=json.load(open('$SDIR/sess$rep.json'))['batch'];print(next((x['us'] for x in b if x['q']=='$q'),0))" 2>/dev/null || echo 0)
                    printf '%s\t%s\t%s\t%s\t0\t%s\n' "$f" "$lane" "$q" "$rep" "$wus" >> "$RAW/warm.tsv"
                done
            else
                for q in Q1 Q2 Q3 Q4 Q5 Q6 Q7 Q8 Q9 Q10; do
                    t0=$(now_ns)
                    python3 "$DUCK" query --dir "$DDIR/dd" --q "$q" --plan "$PLAN" \
                        --out "$DDIR/run${rep}_$q.json" >/dev/null 2>&1; rc=$?
                    t1=$(now_ns)
                    us=$(( (t1 - t0) / 1000 ))
                    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$f" "$lane" "$q" "$rep" "$rc" "$us" >> "$RAW/cold.tsv"
                    if [ "$rep" -eq 1 ]; then cp "$DDIR/run${rep}_$q.json" "$RAW/qanswers/$f.$q.duckdb.json"; fi
                done
                python3 "$DUCK" session --dir "$DDIR/dd" --queries Q1,Q2,Q3,Q4,Q5,Q6,Q7,Q8,Q9,Q10 \
                    --plan "$PLAN" --out "$DDIR/sess$rep.json" >/dev/null 2>&1
                for q in Q1 Q2 Q3 Q4 Q5 Q6 Q7 Q8 Q9 Q10; do
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

# --- toolchain / environment -------------------------------------------------
DUCKDB_VERSION=$(python3 -c 'import duckdb; print(duckdb.__version__)')
t0=$(now_ns); python3 -c pass; t1=$(now_ns); PY_STARTUP_US=$(( (t1 - t0) / 1000 ))
FIX_JSON=$(python3 -c "import json,csv;rows=list(csv.DictReader(open('$RAW/fixtures.tsv'),delimiter='\t'));print(json.dumps({r['fixture']:{'len':int(r['src_bytes']),'sha256':r['src_sha256']} for r in rows},sort_keys=True))")
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.3.2 — ODS economic court (VOLE vs SQLite vs DuckDB)",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "measured_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "tree_state": "$(git status --porcelain | tr '\n' ' ' | sed 's/"/\\"/g')",
  "service": "analytical",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "derives_from": "doc-baseline (same pinned base digest)",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "python": "$(python3 --version)",
  "sqlite3": "$(sqlite3 --version)",
  "duckdb_version": "$DUCKDB_VERSION",
  "duckdb_wheel_sha256": "73b108c04c932b36c2fa4e41110cc1c3c8cd510eb49f065f92d050be8e6929fd (cp311 x86_64); 56c0f71c6bee982e9c30568bb12371bf66b26bf129c75d8d7f60bc69d6590a2c (cp311 aarch64)",
  "arch": "$(uname -m)",
  "python_startup_us": $PY_STARTUP_US,
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "bin": "$BIN",
  "bin_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "profile": "$PROFILE",
  "build_args": "$BUILD_ARGS",
  "vole_substrate": "--profile runtime --packed (packed seed segments in store/fieldpack/)",
  "fixtures_len_sha256": $FIX_JSON,
  "reps": $REPS,
  "storage_accounting": "sum of regular-file sizes (find -type f -printf '%s'); du -sb never used (ADR-0049)",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY analytical bash tools/phase21-3-ods-court.sh
# PROFILE defaults to release (target/release/vole-document); the comparators are
# unaffected by the Rust profile (same convention as tools/phase22-2-court.sh).
# Inside the court:
#   cargo build $BUILD_ARGS            # release: --release --locked --all-features
#   python3 tools/fixtures/make-ods.py --corpus $WORK/corpus
#   VOLE:   $BIN field-build F.ods --store STORE --voldoc F.voldoc --profile runtime --packed
#           python3 tools/fixtures/phase21-3-ods-vole.py run --bin $BIN --store STORE --field HEX --plan ... --outdir ... --packed
#           rm source + descriptor ; $BIN materialize --store STORE --field HEX --exact --packed --output OUT ; cmp OUT source
#   SQLite: python3 tools/fixtures/phase21-3-ods-baseline.py build --source F.ods --db D --through 5
#           python3 tools/fixtures/phase21-3-ods-baseline.py query --db D --q Qn --plan ... --out OUT
#           python3 tools/fixtures/phase21-3-ods-baseline.py session --db D --queries Q1..Q10 --plan ... --out OUT
#           python3 tools/fixtures/phase21-3-ods-baseline.py materialize --db D --out OUT
#   DuckDB: python3 tools/fixtures/phase21-3-ods-duckdb.py build --source F.ods --dir DD
#           python3 tools/fixtures/phase21-3-ods-duckdb.py query --dir DD --q Qn --plan ... --out OUT
#           python3 tools/fixtures/phase21-3-ods-duckdb.py session --dir DD --queries Q1..Q10 --plan ... --out OUT
#   aggregate: python3 tools/fixtures/phase21-3-ods-baseline.py aggregate --raw RAW --campaign CAMPAIGN --env ENV
# Warm session: observe-batch accepts --packed in this tree, so the warm lane is
#   served from the SAME packed substrate as the cold lane.
# Full dev gate:
#   cargo fmt --all --check
#   cargo clippy --all-targets --all-features -- -D warnings
#   cargo test --locked --all-features
EOF

echo "=== aggregate ==="
python3 "$BASE" aggregate --raw "$RAW" --campaign "$CAMPAIGN" --env "$CAMPAIGN/environment.json"
verdict_rc=$?

EXACT_OK=$(awk -F'\t' 'NR>1 && $2=="true"' "$RAW/exact.tsv" | wc -l | tr -d ' ')
EXACT_N=$(awk -F'\t' 'NR>1' "$RAW/exact.tsv" | wc -l | tr -d ' ')
echo
if [ "$verdict_rc" -eq 0 ] && [ "$EXACT_OK" -eq "$EXACT_N" ] && [ "$EXACT_N" -gt 0 ]; then
    echo "PHASE 21.3.2 ODS COURT: PASS — VOLE exactness ${EXACT_OK}/${EXACT_N} — campaign $CAMPAIGN"
    exit 0
fi
echo "PHASE 21.3.2 ODS COURT: FAIL — VOLE exactness ${EXACT_OK}/${EXACT_N} — campaign $CAMPAIGN" >&2
exit 1
