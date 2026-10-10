#!/usr/bin/env bash
# Phase 21.12 — the JSONL economic court (completes subphase 21.12).
#
# Pre-registered. Hypotheses:
#
#   H1 (byte-exactness / Q8) — for every corpus fixture, VOLE
#       `materialize --exact == source` (length + SHA-256 + `cmp`) after the source
#       file AND the standalone descriptor are deleted, in a fresh process. The
#       court FAILS unless this is 100 %.
#   H2 (contract questions) — three lanes (VOLE; a source-retaining SQLite
#       baseline; a conventional JSON→object `json` baseline) answer the SAME eight
#       questions Q1–Q8 where they can, and every question a lane cannot answer is a
#       TYPED decline, never a silent empty answer.
#   H3 (cross-lane agreement) — where all lanes answer, comparable values agree
#       (a field value in record N, the per-line top-level kind of record N, a
#       scalar value, a lexical-find count, the record count). VOLE answers Q2/Q5 (a
#       record value's exact source span and a scalar's exact spelling); the
#       baselines decline those, because a conventional JSON load drops every source
#       offset and parses numbers/escapes into host values. Those gaps are recorded,
#       never papered over.
#   H4 (economics) — build/storage/cold/warm are measured per lane with a stated
#       estimator (paired per-fixture ratios, median + geometric mean, fixed-seed
#       cluster bootstrap by fixture; ratio-of-sums reported separately), every raw
#       sample retained. Wall times are microseconds (`us`).
#
# JSONL is a line/event stream, so the comparators are a conventional JSON→object
# load (plus a source-retaining SQLite baseline for byte-authority): neither
# provides exact-source spans or exact scalar spelling (Q2/Q5 are typed declines)
# nor, for the conventional load, the original bytes (Q8).
#
# Honest scope: this is a self-authored corpus; only Q8 is a byte-authority claim.
#
# Fairness: the comparators (Python, SQLite) are unaffected by the Rust profile, so
# the court defaults to a RELEASE build (`PROFILE=release`); VOLE uses
# `field-build --profile runtime --packed`, with `--packed` on every
# observe/observe-batch/materialize.
#
# Runs in the pinned, hard-capped `doc-baseline` service (dev toolchain + python3 +
# sqlite3); never the host:
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-12-jsonl-court.sh

set -uo pipefail
cd /work
export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1

BASE=tools/fixtures/phase21-12-jsonl-baseline.py
VOLE=tools/fixtures/phase21-12-jsonl-vole.py
SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-12-jsonl-econ-${SHA}"
RAW="$CAMPAIGN/raw"
WORK="evidence/scratch/phase21-12-econ"
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
python3 tools/fixtures/make-jsonl.py --corpus "$WORK/corpus" | tee "$RAW/corpus.txt"
cp "$WORK/corpus/basic.ndjson" "$WORK/corpus/shapes.ndjson" "$WORK/corpus/unicode.ndjson" \
   "$WORK/corpus/crlf.ndjson" "$WORK/corpus/blank.ndjson" "$WORK/corpus/large.ndjson" \
   "$WORK/ref/"

# fixtures.tsv: the six contract fixtures (the controls — single.json/malformed/
# bag — are not JSONL-lane inputs and are handled by the 21.12.1 court).
{
    printf 'fixture\tsrc_bytes\tsrc_sha256\n'
    while IFS=$'\t' read -r name bytes sha; do
        case "$name" in
            single.json|malformed.ndjson|bag.ndjson) continue ;;
        esac
        printf '%s\t%s\t%s\n' "$name" "$bytes" "$sha"
    done < "$RAW/corpus.txt"
} > "$RAW/fixtures.tsv"

FIXTURES=$(awk 'NR>1{print $1}' "$RAW/fixtures.tsv")

now_ns() { date +%s%N; }

# --- per-fixture plan (FROZEN): "VAL_SPEC|BOOL_SPEC|KIND_LINE|FIND" -----------
plan_for() {
    case "$1" in
        basic.ndjson)   echo "1:/name|0:/active|0|beta" ;;
        shapes.ndjson)  echo "2:|1:/0|0|string" ;;
        unicode.ndjson) echo "0:/s|2:/mixed|0|emoji" ;;
        crlf.ndjson)    echo "0:/a|2:/c|0|b" ;;
        blank.ndjson)   echo "0:/x|1:/y|0|x" ;;
        large.ndjson)   echo "0:/name|0:/ok|0|item-0" ;;
        *)              echo "0:|0:|0|" ;;
    esac
}

printf 'fixture\tlane\trep\trc\tus\n' > "$RAW/build.tsv"
printf 'fixture\tlane\tbytes\tfiles\n' > "$RAW/storage.tsv"
printf 'fixture\tlane\tq\trep\trc\tus\n' > "$RAW/cold.tsv"
printf 'fixture\tlane\tq\trep\trc\tus\n' > "$RAW/warm.tsv"
printf 'fixture\tvole_ok\tsqlite_ok\tconv_ok\tlen_ok\tsha_ok\tcmp_ok\tv_us\ts_us\n' > "$RAW/exact.tsv"

build_lane() { # fixture lane dir
    local f="$1" lane="$2" dir="$3"
    rm -rf "$dir"; mkdir -p "$dir"
    case "$lane" in
        vole)   "$BIN" field-build "$WORK/corpus/$f" --store "$dir/store" --voldoc "$dir/desc.voldoc" \
                    --profile runtime --packed > "$dir/build.json" 2> "$dir/build.err" ;;
        sqlite) python3 "$BASE" build --lane sqlite --source "$WORK/corpus/$f" --out "$dir" > "$dir/build.json" 2> "$dir/build.err" ;;
        conv)   python3 "$BASE" build --lane conv --source "$WORK/corpus/$f" --out "$dir" > "$dir/build.json" 2> "$dir/build.err" ;;
    esac
}

for f in $FIXTURES; do
    echo "=== $f ==="
    IFS='|' read -r VAL_SPEC BOOL_SPEC KIND_LINE FIND_PAT <<EOF
$(plan_for "$f")
EOF
    PLAN=$(python3 -c 'import json,sys
v,b,k,pat = sys.argv[1:5]
print(json.dumps({"val_spec": v, "bool_spec": b, "kind_line": k, "find": pat}))' \
        "$VAL_SPEC" "$BOOL_SPEC" "$KIND_LINE" "$FIND_PAT")

    VDIR="$WORK/lanes/$f.vole"
    SDIR="$WORK/lanes/$f.sqlite"
    CDIR="$WORK/lanes/$f.conv"

    # ---- build (best-of-N, interleaved lane order per rep) ------------------
    for rep in $(seq 1 "$REPS"); do
        if [ $((rep % 2)) -eq 1 ]; then order="vole sqlite conv"; else order="conv sqlite vole"; fi
        for lane in $order; do
            case "$lane" in vole) dir="$VDIR" ;; sqlite) dir="$SDIR" ;; conv) dir="$CDIR" ;; esac
            t0=$(now_ns)
            build_lane "$f" "$lane" "$dir"; rc=$?
            t1=$(now_ns)
            us=$(( (t1 - t0) / 1000 ))
            printf '%s\t%s\t%s\t%s\t%s\n' "$f" "$lane" "$rep" "$rc" "$us" >> "$RAW/build.tsv"
            [ "$rc" -eq 0 ] || echo "FINDING: build failed $f/$lane rc=$rc" >&2
        done
    done

    # ---- storage (sum of regular-file sizes; never du -sb) ------------------
    for lane in vole sqlite conv; do
        case "$lane" in vole) dir="$VDIR" ;; sqlite) dir="$SDIR" ;; conv) dir="$CDIR" ;; esac
        bytes=$(find "$dir" -type f -printf '%s\n' | awk '{s+=$1} END{print s+0}')
        files=$(find "$dir" -type f | wc -l | tr -d ' ')
        printf '%s\t%s\t%s\t%s\n' "$f" "$lane" "$bytes" "$files" >> "$RAW/storage.tsv"
    done

    FIELD=$(python3 -c "import json,sys;print(json.load(open('$VDIR/build.json'))['ingest']['field'])" 2>/dev/null || echo "")

    # ---- query phase (cold + warm), interleaved lane order per rep ----------
    for rep in $(seq 1 "$REPS"); do
        if [ $((rep % 2)) -eq 1 ]; then order="vole sqlite conv"; else order="conv sqlite vole"; fi
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
                case "$lane" in sqlite) dir="$SDIR" ;; conv) dir="$CDIR" ;; esac
                for q in Q1 Q2 Q3 Q4 Q5 Q6 Q7 Q8; do
                    t0=$(now_ns)
                    python3 "$BASE" query --lane "$lane" --dir "$dir" --q "$q" --plan "$PLAN" \
                        --out "$dir/run${rep}_$q.json" >/dev/null 2>&1; rc=$?
                    t1=$(now_ns)
                    us=$(( (t1 - t0) / 1000 ))
                    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$f" "$lane" "$q" "$rep" "$rc" "$us" >> "$RAW/cold.tsv"
                    if [ "$rep" -eq 1 ]; then cp "$dir/run${rep}_$q.json" "$RAW/qanswers/$f.$q.$lane.json"; fi
                done
                python3 "$BASE" session --lane "$lane" --dir "$dir" --queries Q1,Q2,Q3,Q4,Q5,Q6,Q7,Q8 \
                    --plan "$PLAN" --out "$dir/sess$rep.json" >/dev/null 2>&1
                for q in Q1 Q2 Q3 Q4 Q5 Q6 Q7 Q8; do
                    wus=$(python3 -c "import json;b=json.load(open('$dir/sess$rep.json'))['batch'];print(next((x['us'] for x in b if x['q']=='$q'),0))" 2>/dev/null || echo 0)
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
    python3 "$BASE" materialize --lane sqlite --dir "$SDIR" --out "$WORK/$f.sql.out" >/dev/null 2>&1; s_rc=$?
    t1=$(now_ns)
    s_us=$(( (t1 - t0) / 1000 ))
    python3 "$BASE" materialize --lane conv --dir "$CDIR" --out "$WORK/$f.conv.out" >/dev/null 2>&1; c_rc=$?

    v_len=$(wc -c < "$WORK/$f.vole.out" 2>/dev/null | tr -d ' ')
    v_sha=$(sha256sum "$WORK/$f.vole.out" 2>/dev/null | cut -d' ' -f1)
    len_ok=false; [ "$v_len" = "$SRC_BYTES" ] && len_ok=true
    sha_ok=false; [ "$v_sha" = "$SRC_SHA" ] && sha_ok=true
    cmp_ok=false; cmp -s "$WORK/ref/$f" "$WORK/$f.vole.out" && cmp_ok=true
    vole_ok=false
    if [ "$v_rc" -eq 0 ] && [ "$len_ok" = true ] && [ "$sha_ok" = true ] && [ "$cmp_ok" = true ]; then vole_ok=true; fi
    sql_ok=false; [ "$s_rc" -eq 0 ] && cmp -s "$WORK/ref/$f" "$WORK/$f.sql.out" && sql_ok=true
    conv_ok=false; [ "$c_rc" -eq 0 ] && cmp -s "$WORK/ref/$f" "$WORK/$f.conv.out" && conv_ok=true
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$f" "$vole_ok" "$sql_ok" "$conv_ok" "$len_ok" "$sha_ok" "$cmp_ok" "$v_us" "$s_us" >> "$RAW/exact.tsv"
    [ "$vole_ok" = true ] || echo "FINDING: VOLE not byte-exact for $f" >&2
done

# --- toolchain / environment -------------------------------------------------
t0=$(now_ns); python3 -c pass; t1=$(now_ns); PY_STARTUP_US=$(( (t1 - t0) / 1000 ))
FIX_JSON=$(python3 -c "import json,csv;rows=list(csv.DictReader(open('$RAW/fixtures.tsv'),delimiter='\t'));print(json.dumps({r['fixture']:{'len':int(r['src_bytes']),'sha256':r['src_sha256']} for r in rows},sort_keys=True))")
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.12 — JSONL economic court (VOLE vs SQLite vs conventional JSON→object)",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "measured_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "tree_state": "$(git status --porcelain | tr '\n' ' ' | sed 's/"/\\"/g')",
  "service": "doc-baseline",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "python": "$(python3 --version)",
  "sqlite3": "$(sqlite3 --version)",
  "json": "$(python3 -c 'import json;print("stdlib")')",
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

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY doc-baseline bash tools/phase21-12-jsonl-court.sh
# PROFILE defaults to release (target/release/vole-document).
# Inside the court:
#   cargo build --release --locked --all-features
#   python3 tools/fixtures/make-jsonl.py --corpus $WORK/corpus
#   VOLE:   $BIN field-build F --store STORE --voldoc F.voldoc --profile runtime --packed
#           python3 tools/fixtures/phase21-12-jsonl-vole.py run --bin $BIN --store STORE --field HEX --plan ... --outdir ... --packed
#           rm source + descriptor ; $BIN materialize --store STORE --field HEX --exact --packed --output OUT ; cmp OUT source
#   sqlite: python3 tools/fixtures/phase21-12-jsonl-baseline.py build --lane sqlite --source F --out D
#           python3 tools/fixtures/phase21-12-jsonl-baseline.py query --lane sqlite --dir D --q Qn --plan ... --out OUT
#   conv:   python3 tools/fixtures/phase21-12-jsonl-baseline.py build --lane conv --source F --out D
#           python3 tools/fixtures/phase21-12-jsonl-baseline.py query --lane conv --dir D --q Qn --plan ... --out OUT
#   aggregate: python3 tools/fixtures/phase21-12-jsonl-baseline.py aggregate --raw RAW --campaign CAMPAIGN --env ENV
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
    echo "PHASE 21.12 JSONL COURT: PASS — VOLE exactness ${EXACT_OK}/${EXACT_N} — campaign $CAMPAIGN"
    exit 0
fi
echo "PHASE 21.12 JSONL COURT: FAIL — VOLE exactness ${EXACT_OK}/${EXACT_N} — campaign $CAMPAIGN" >&2
exit 1
