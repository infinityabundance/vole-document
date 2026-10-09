#!/usr/bin/env bash
# Phase 21.5.2 — the JSON economic court (completes subphase 21.5).
#
# Pre-registered. Hypotheses:
#
#   H1 (byte-exactness / Q6) — for every corpus fixture, VOLE
#       `materialize --exact == source` (length + SHA-256 + `cmp`) after the source
#       file AND the standalone descriptor are deleted, in a fresh process. The
#       court FAILS unless this is 100 %.
#   H2 (contract questions) — the two lanes (VOLE and a source-retaining SQLite
#       baseline using `json1`/`jsonb`) answer the SAME eight questions Q1–Q8 where
#       they can, and every question a lane cannot answer is a TYPED decline.
#   H3 (cross-lane agreement) — where both lanes answer, comparable values agree
#       (a value at a pointer, a node kind, an array element, key existence, a
#       lexical find set, exact closure). VOLE declines what its surface cannot
#       answer; SQLite declines what it does not model (source spans, duplicate
#       keys, token spelling). Those gaps are recorded, never papered over.
#   H4 (economics) — build/storage/cold/warm are measured per lane with a stated
#       estimator (paired per-fixture ratios, median + geometric mean, fixed-seed
#       cluster bootstrap by fixture), every raw sample retained.
#
# Honest scope: this is a self-authored corpus; only Q6 is a byte-authority claim.
# Preserving representation (spelling/order/duplicate keys/spans) is where VOLE
# claims value; SQLite's `jsonb` is a serious comparator.
#
# Fairness / conventions: RELEASE build (`PROFILE=release`); VOLE substrate
# `field-build --profile runtime --packed`, `--packed` on every observe/batch/
# materialize. Wall times are microseconds (`us`). Runs in the pinned, hard-capped
# `doc-baseline` service; never the host:
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-5-json-court.sh

set -uo pipefail
cd /work
export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1

BASE=tools/fixtures/phase21-5-json-baseline.py
VOLE=tools/fixtures/phase21-5-json-vole.py
SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-5-json-econ-${SHA}"
RAW="$CAMPAIGN/raw"
WORK="evidence/scratch/phase21-5-json"
REPS=${REPS:-3}

PROFILE=${PROFILE:-release}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-${VOLE_BIN:-target/debug/vole-document}} ;;
esac

rm -rf "$CAMPAIGN" "$WORK"
mkdir -p "$RAW/qanswers" "$RAW/vole_warm" "$WORK/corpus" "$WORK/ref"

echo "=== build ($PROFILE) ==="
# shellcheck disable=SC2086
if ! cargo build $BUILD_ARGS 2>&1 | tail -2; then
    echo "court: cargo build FAILED ($BUILD_ARGS); refusing to measure" >&2
    exit 1
fi
[ -x "$BIN" ] || { echo "court: $BIN missing; refusing to measure" >&2; exit 1; }
echo "-- measuring $BIN ($PROFILE; VOLE substrate: --profile runtime --packed)"

echo "=== regenerate deterministic corpus ==="
python3 tools/fixtures/make-json.py --corpus "$WORK/corpus" | tee "$RAW/corpus.txt"
cp -a "$WORK/corpus"/*.json "$WORK/ref/"

{
    printf 'fixture\tsrc_bytes\tsrc_sha256\n'
    while IFS=$'\t' read -r name bytes sha; do
        [ -n "$name" ] || continue
        case "$name" in
            malformed.json) continue ;; # Opaque control: not a JSON-parseable lane input
        esac
        printf '%s\t%s\t%s\n' "$name" "$bytes" "$sha"
    done < "$RAW/corpus.txt"
} > "$RAW/fixtures.tsv"

FIXTURES=$(awk 'NR>1{print $1}' "$RAW/fixtures.tsv")

now_ns() { date +%s%N; }

# --- per-fixture plan (FROZEN; chosen to exercise each feature) --------------
# "VALUE_SPAN_KIND_DUP_ARRAY_TOKEN_FIND"
plan_for() {
    case "$1" in
        basic.json)   echo "/count|/name|/nested|/count|/nested/a/0|/count|basic" ;;
        unicode.json) echo "/accent|/emoji|/raw|/raw|/nope|/accent|accent" ;;
        numbers.json) echo "/neg|/big|/zero|/neg|/nope|/float|xyz" ;;
        dup.json)     echo "/b|/a|/c|/a|/nope|/b|a" ;;
        deep.json)    echo "/k|/k|/k|/k|/k|/k|k" ;;
        scalar.json)  echo '||||||1' ;;
        large.json)   echo "/items/0/id|/items/0/name|/items/0|/marker|/items/0/name|/items/0/id|ZEBRA_MARKER_21_5" ;;
        *)            echo "|||""|||" ;;
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
    esac
}

for f in $FIXTURES; do
    echo "=== $f ==="
    IFS='|' read -r VALUE_PTR SPAN_PTR KIND_PTR DUP_PTR ARRAY_PTR TOKEN_PTR FIND_PAT <<EOF
$(plan_for "$f")
EOF
    PLAN=$(python3 -c 'import json,sys;print(json.dumps({"value_ptr":sys.argv[1],"span_ptr":sys.argv[2],"kind_ptr":sys.argv[3],"dup_ptr":sys.argv[4],"array_ptr":sys.argv[5],"token_ptr":sys.argv[6],"find_pat":sys.argv[7]}))' \
        "$VALUE_PTR" "$SPAN_PTR" "$KIND_PTR" "$DUP_PTR" "$ARRAY_PTR" "$TOKEN_PTR" "$FIND_PAT")

    VDIR="$WORK/lanes/$f.vole"
    SDIR="$WORK/lanes/$f.sqlite"

    # ---- build (best-of-N, interleaved lane order per rep) ------------------
    for rep in $(seq 1 "$REPS"); do
        if [ $((rep % 2)) -eq 1 ]; then order="vole sqlite"; else order="sqlite vole"; fi
        for lane in $order; do
            case "$lane" in vole) dir="$VDIR" ;; sqlite) dir="$SDIR" ;; esac
            t0=$(now_ns)
            build_lane "$f" "$lane" "$dir"; rc=$?
            t1=$(now_ns)
            us=$(( (t1 - t0) / 1000 ))
            printf '%s\t%s\t%s\t%s\t%s\n' "$f" "$lane" "$rep" "$rc" "$us" >> "$RAW/build.tsv"
            [ "$rc" -eq 0 ] || echo "FINDING: build failed $f/$lane rc=$rc" >&2
        done
    done

    # ---- storage (sum of regular-file sizes; never du -sb) ------------------
    for lane in vole sqlite; do
        case "$lane" in vole) dir="$VDIR" ;; sqlite) dir="$SDIR" ;; esac
        bytes=$(find "$dir" -type f -printf '%s\n' | awk '{s+=$1} END{print s+0}')
        files=$(find "$dir" -type f | wc -l | tr -d ' ')
        printf '%s\t%s\t%s\t%s\n' "$f" "$lane" "$bytes" "$files" >> "$RAW/storage.tsv"
    done

    FIELD=$(python3 -c "import json,sys;print(json.load(open('$VDIR/build.json'))['ingest']['field'])" 2>/dev/null || echo "")

    # ---- query phase (cold + warm), interleaved lane order per rep ----------
    for rep in $(seq 1 "$REPS"); do
        if [ $((rep % 2)) -eq 1 ]; then order="vole sqlite"; else order="sqlite vole"; fi
        for lane in $order; do
            if [ "$lane" = "vole" ]; then
                [ -n "$FIELD" ] || { echo "FINDING: no VOLE field for $f" >&2; continue; }
                python3 "$VOLE" run --bin "$BIN" --store "$VDIR/store" --field "$FIELD" \
                    --plan "$PLAN" --outdir "$VDIR/run$rep" --source "$WORK/corpus/$f" --packed >/dev/null
                for q in Q1 Q2 Q3 Q4 Q5 Q6 Q7 Q8; do
                    us=$(python3 -c "import json;print(json.load(open('$VDIR/run$rep/cold_us.json')).get('$q',0))" 2>/dev/null || echo 0)
                    printf '%s\t%s\t%s\t%s\t0\t%s\n' "$f" "$lane" "$q" "$rep" "$us" >> "$RAW/cold.tsv"
                    wus=$(python3 -c "import json;print(json.load(open('$VDIR/run$rep/warm_us.json')).get('$q',0))" 2>/dev/null || echo 0)
                    printf '%s\t%s\t%s\t%s\t0\t%s\n' "$f" "$lane" "$q" "$rep" "$wus" >> "$RAW/warm.tsv"
                    if [ "$rep" -eq 1 ]; then
                        cp "$VDIR/run$rep/qanswers/$q.json" "$RAW/qanswers/$f.$q.vole.json"
                        cp "$VDIR/run$rep/warm/$q.json" "$RAW/vole_warm/$f.$q.json" 2>/dev/null || true
                    fi
                done
            else
                for q in Q1 Q2 Q3 Q4 Q5 Q6 Q7 Q8; do
                    t0=$(now_ns)
                    python3 "$BASE" query --db "$SDIR/x.sqlite" --q "$q" --plan "$PLAN" \
                        --out "$SDIR/run${rep}_$q.json" >/dev/null 2>&1; rc=$?
                    t1=$(now_ns)
                    us=$(( (t1 - t0) / 1000 ))
                    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$f" "$lane" "$q" "$rep" "$rc" "$us" >> "$RAW/cold.tsv"
                    if [ "$rep" -eq 1 ]; then cp "$SDIR/run${rep}_$q.json" "$RAW/qanswers/$f.$q.sqlite.json"; fi
                done
                python3 "$BASE" session --db "$SDIR/x.sqlite" --queries Q1,Q2,Q3,Q4,Q5,Q6,Q7,Q8 \
                    --plan "$PLAN" --out "$SDIR/sess$rep.json" >/dev/null 2>&1
                for q in Q1 Q2 Q3 Q4 Q5 Q6 Q7 Q8; do
                    wus=$(python3 -c "import json;b=json.load(open('$SDIR/sess$rep.json'))['batch'];print(next((x['us'] for x in b if x['q']=='$q'),0))" 2>/dev/null || echo 0)
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
t0=$(now_ns); python3 -c pass; t1=$(now_ns); PY_STARTUP_US=$(( (t1 - t0) / 1000 ))
FIX_JSON=$(python3 -c "import json,csv;rows=list(csv.DictReader(open('$RAW/fixtures.tsv'),delimiter='\t'));print(json.dumps({r['fixture']:{'len':int(r['src_bytes']),'sha256':r['src_sha256']} for r in rows},sort_keys=True))")
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.5.1 — JSON economic court (VOLE vs source-retaining SQLite)",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "measured_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "tree_state": "$(git status --porcelain | tr '\n' ' ' | sed 's/"/\\"/g')",
  "service": "doc-baseline",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "python": "$(python3 --version)",
  "sqlite3": "$(sqlite3 --version)",
  "sqlite_json1": "$(python3 -c 'import sqlite3;print(sqlite3.sqlite_version)')",
  "arch": "$(uname -m)",
  "python_startup_us": $PY_STARTUP_US,
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "bin": "$BIN",
  "bin_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "profile": "$PROFILE",
  "build_args": "$BUILD_ARGS",
  "vole_substrate": "--profile runtime --packed",
  "fixtures_len_sha256": $FIX_JSON,
  "reps": $REPS,
  "storage_accounting": "sum of regular-file sizes (find -type f -printf '%s'); du -sb never used (ADR-0049)",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

cat > "$CAMPAIGN/commands.txt" <<EOF
docker compose run --rm --no-TTY doc-baseline bash tools/phase21-5-json-court.sh
# PROFILE defaults to release (target/release/vole-document).
# Inside the court:
#   cargo build --release --locked --all-features
#   python3 tools/fixtures/make-json.py --corpus \$WORK/corpus
#   VOLE:   \$BIN field-build F.json --store STORE --voldoc F.voldoc --profile runtime --packed
#           python3 tools/fixtures/phase21-5-json-vole.py run --bin \$BIN --store STORE --field HEX --plan ... --outdir ... --packed
#           rm source + descriptor ; \$BIN materialize --store STORE --field HEX --exact --packed --output OUT ; cmp OUT source
#   SQLite: python3 tools/fixtures/phase21-5-json-baseline.py build --source F.json --db D
#           python3 tools/fixtures/phase21-5-json-baseline.py query --db D --q Qn --plan ... --out OUT
#           python3 tools/fixtures/phase21-5-json-baseline.py session --db D --queries Q1..Q8 --plan ... --out OUT
#           python3 tools/fixtures/phase21-5-json-baseline.py materialize --db D --out OUT
#   aggregate: python3 tools/fixtures/phase21-5-json-baseline.py aggregate --raw RAW --campaign CAMPAIGN --env ENV
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
    echo "PHASE 21.5.2 JSON COURT: PASS — VOLE exactness ${EXACT_OK}/${EXACT_N} — campaign $CAMPAIGN"
    exit 0
fi
echo "PHASE 21.5.2 JSON COURT: FAIL — VOLE exactness ${EXACT_OK}/${EXACT_N} — campaign $CAMPAIGN" >&2
exit 1
