#!/usr/bin/env bash
# Phase 21.17 — the JSON5 / JSONC ECONOMIC court (VOLE vs two conventional lanes).
#
# Pre-registered. Hypotheses:
#
#   H1 (byte-exactness / Q6) — for every corpus fixture, VOLE
#       `materialize --exact == source` (length + SHA-256 + `cmp`) after the source
#       file AND the standalone descriptor are deleted, in a fresh process. The
#       court FAILS unless this is 100 %.
#   H2 (contract questions) — the three lanes (VOLE; a **source-retaining** SQLite
#       baseline that keeps the raw bytes and queries `json_extract`/`json_type`/
#       `json_tree` on a JSON5->strict-JSON normalization; a **conventional
#       JSON5->object load** via a vendored pure-Python loader) answer the SAME
#       twelve questions Q1–Q12 (Q1–Q8 as in the JSON court, plus the JSON5-only
#       Q9 comment spans, Q10 key spans, Q11 recorded dialect, Q12 exact numeric
#       spelling) where they can, and every question a lane cannot answer is a
#       TYPED decline, never a silent empty answer.
#   H3 (cross-lane agreement) — where VOLE and a comparator both answer, comparable
#       values agree (a scalar value, a node kind, a duplicate-key count, a
#       lexical-find match set, a materialized length+SHA). VOLE answers the
#       representation questions (Q2/Q8/Q9/Q10/Q12) that the conventional load
#       declines, and the value questions (Q1/Q5) on value (spelling is compared
#       separately). Those gaps are recorded, never papered over.
#   H4 (economics) — build/storage/cold/warm are measured per lane with a stated
#       estimator (paired per-fixture ratios, median + geometric mean, fixed-seed
#       cluster bootstrap by fixture, ratio-of-sums reported separately), every raw
#       sample retained. Wall times are microseconds (`us`).
#
# Honest scope: this is a self-authored deterministic corpus; only Q6 is a
# byte-authority claim. The conventional loader is a JSON5->object load (it drops
# spans/comments/duplicates/spelling), NOT a span-preserving scanner: a
# span-preserving loader could in principle match VOLE on structure, and no claim
# is made against one. The strict-JSON SQLite lane cannot even represent JSON5
# `Infinity`/`NaN`; that limitation is recorded, not hidden.
#
# Fairness / conventions: RELEASE build (`PROFILE=release`); VOLE substrate
# `field-build --profile runtime --packed`, `--packed` on every observe/batch/
# materialize. Runs in the pinned, hard-capped `doc-baseline` service (dev
# toolchain + python3 + sqlite3); never the host:
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-17-json5-court.sh

set -uo pipefail
cd /work
export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1

BASE=tools/fixtures/phase21-17-json5-baseline.py
VOLE=tools/fixtures/phase21-17-json5-vole.py
SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-17-json5-econ-${SHA}"
RAW="$CAMPAIGN/raw"
WORK="evidence/scratch/phase21-17-econ"
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
python3 tools/fixtures/make-json5.py --corpus "$WORK/corpus" | tee "$RAW/corpus.txt"

# Lane inputs (JSON5/JSONC-parseable) vs controls (boundary pins, not measured).
LANE_FIXTURES="basic.json5 jsonc.jsonc numbers.json5 strings.json5 unicode.json5 comments.json5 dup.json5 deep.json5 large.json5"
CONTROLS="strict.json bomb.json5 malformed.json5 prose.txt"

for f in $LANE_FIXTURES; do cp "$WORK/corpus/$f" "$WORK/ref/"; done

{
    printf 'fixture\tsrc_bytes\tsrc_sha256\n'
    while IFS=$'\t' read -r name bytes sha; do
        [ -n "$name" ] || continue
        case " $LANE_FIXTURES " in *" $name "*) printf '%s\t%s\t%s\n' "$name" "$bytes" "$sha" ;; esac
    done < "$RAW/corpus.txt"
} > "$RAW/fixtures.tsv"

FIXTURES=$(awk 'NR>1{print $1}' "$RAW/fixtures.tsv")

now_ns() { date +%s%N; }

# --- per-fixture plan (FROZEN; JSON5 pointers, chosen to exercise each feature) --
plan_for() {
    case "$1" in
        basic.json5)
            echo '{"value_ptr":"/unquoted","span_ptr":"/hex","kind_ptr":"/hex","dup_ptr":"/hex","array_ptr":"/trailing/0","find_pat":"trail","token_ptr":"/unquoted","keyspan_ptr":"","num_ptr":"/hex"}' ;;
        jsonc.jsonc)
            echo '{"value_ptr":"/compilerOptions/target","span_ptr":"/compilerOptions/target","kind_ptr":"/compilerOptions/strict","dup_ptr":"/compilerOptions/target","array_ptr":"/compilerOptions/target","find_pat":"target","token_ptr":"/compilerOptions/target","keyspan_ptr":"/compilerOptions","num_ptr":"/compilerOptions/target"}' ;;
        numbers.json5)
            echo '{"value_ptr":"/dot","span_ptr":"/hex","kind_ptr":"/inf","dup_ptr":"/nan","array_ptr":"/exp","find_pat":"inf","token_ptr":"/dot","keyspan_ptr":"","num_ptr":"/hex"}' ;;
        strings.json5)
            echo '{"value_ptr":"/single","span_ptr":"/single","kind_ptr":"/hex","dup_ptr":"/nul","array_ptr":"/cont","find_pat":"line","token_ptr":"/single","keyspan_ptr":"","num_ptr":"/hex"}' ;;
        unicode.json5)
            echo '{"value_ptr":"/esc","span_ptr":"/esc","kind_ptr":"/nb","dup_ptr":"/esc","array_ptr":"/esc","find_pat":"esc","token_ptr":"/esc","keyspan_ptr":"","num_ptr":"/nb"}' ;;
        comments.json5)
            echo '{"value_ptr":"/b","span_ptr":"/b","kind_ptr":"/a","dup_ptr":"/d","array_ptr":"/b","find_pat":"comment","token_ptr":"/b","keyspan_ptr":"","num_ptr":"/a"}' ;;
        dup.json5)
            echo '{"value_ptr":"/a","span_ptr":"/a","kind_ptr":"/a","dup_ptr":"/a","array_ptr":"/c","find_pat":"a","token_ptr":"/a","keyspan_ptr":"","num_ptr":"/a"}' ;;
        deep.json5)
            echo '{"value_ptr":"/label","span_ptr":"/label","kind_ptr":"/n","dup_ptr":"/label","array_ptr":"/label","find_pat":"bottom","token_ptr":"/label","keyspan_ptr":"","num_ptr":"/n"}' ;;
        large.json5)
            echo '{"value_ptr":"/5/name","span_ptr":"/5/name","kind_ptr":"/5/ok","dup_ptr":"/5/name","array_ptr":"/5/name","find_pat":"item-5","token_ptr":"/5/name","keyspan_ptr":"/5","num_ptr":"/5/value"}' ;;
        *) echo '{}' ;;
    esac
}

printf 'fixture\tlane\trep\trc\tus\n' > "$RAW/build.tsv"
printf 'fixture\tlane\tbytes\tfiles\n' > "$RAW/storage.tsv"
printf 'fixture\tlane\tq\trep\trc\tus\n' > "$RAW/cold.tsv"
printf 'fixture\tlane\tq\trep\trc\tus\n' > "$RAW/warm.tsv"
printf 'fixture\tvole_ok\tsqlite_ok\tconv_ok\tlen_ok\tsha_ok\tcmp_ok\tv_us\ts_us\tc_us\n' > "$RAW/exact.tsv"

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

# Rotating lane order per repetition so no lane is systematically first/last.
lane_order() { # rep
    case $(( ($1 - 1) % 3 )) in
        0) echo "vole sqlite conv" ;;
        1) echo "conv vole sqlite" ;;
        *) echo "sqlite conv vole" ;;
    esac
}

QS="Q1 Q2 Q3 Q4 Q5 Q6 Q7 Q8 Q9 Q10 Q11 Q12"

for f in $FIXTURES; do
    echo "=== $f ==="
    PLAN=$(plan_for "$f")

    VDIR="$WORK/lanes/$f.vole"
    SDIR="$WORK/lanes/$f.sqlite"
    CDIR="$WORK/lanes/$f.conv"

    # ---- build (best-of-N, rotating lane order per rep) ---------------------
    for rep in $(seq 1 "$REPS"); do
        for lane in $(lane_order "$rep"); do
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

    FIELD=$(python3 -c "import json;print(json.load(open('$VDIR/build.json'))['ingest']['field'])" 2>/dev/null || echo "")

    # ---- query phase (cold + warm), rotating lane order per rep -------------
    for rep in $(seq 1 "$REPS"); do
        for lane in $(lane_order "$rep"); do
            if [ "$lane" = "vole" ]; then
                [ -n "$FIELD" ] || { echo "FINDING: no VOLE field for $f" >&2; continue; }
                python3 "$VOLE" run --bin "$BIN" --store "$VDIR/store" --field "$FIELD" \
                    --plan "$PLAN" --outdir "$VDIR/run$rep" --source "$WORK/ref/$f" --packed >/dev/null
                for q in $QS; do
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
                for q in $QS; do
                    t0=$(now_ns)
                    python3 "$BASE" query --lane "$lane" --dir "$dir" --q "$q" --plan "$PLAN" \
                        --out "$dir/run${rep}_$q.json" >/dev/null 2>&1; rc=$?
                    t1=$(now_ns)
                    us=$(( (t1 - t0) / 1000 ))
                    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$f" "$lane" "$q" "$rep" "$rc" "$us" >> "$RAW/cold.tsv"
                    if [ "$rep" -eq 1 ]; then cp "$dir/run${rep}_$q.json" "$RAW/qanswers/$f.$q.$lane.json"; fi
                done
                python3 "$BASE" session --lane "$lane" --dir "$dir" --queries "Q1,Q2,Q3,Q4,Q5,Q6,Q7,Q8,Q9,Q10,Q11,Q12" \
                    --plan "$PLAN" --out "$dir/sess$rep.json" >/dev/null 2>&1
                for q in $QS; do
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

    v_len=$(wc -c < "$WORK/$f.vole.out" 2>/dev/null | tr -d ' ')
    v_sha=$(sha256sum "$WORK/$f.vole.out" 2>/dev/null | cut -d' ' -f1)
    len_ok=false; [ "$v_len" = "$SRC_BYTES" ] && len_ok=true
    sha_ok=false; [ "$v_sha" = "$SRC_SHA" ] && sha_ok=true
    cmp_ok=false; cmp -s "$WORK/ref/$f" "$WORK/$f.vole.out" && cmp_ok=true
    vole_ok=false
    if [ "$v_rc" -eq 0 ] && [ "$len_ok" = true ] && [ "$sha_ok" = true ] && [ "$cmp_ok" = true ]; then vole_ok=true; fi
    sql_ok=false; [ "$s_rc" -eq 0 ] && cmp -s "$WORK/ref/$f" "$WORK/$f.sql.out" && sql_ok=true
    conv_ok=false
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$f" "$vole_ok" "$sql_ok" "$conv_ok" "$len_ok" "$sha_ok" "$cmp_ok" "$v_us" "$s_us" "0" >> "$RAW/exact.tsv"
    [ "$vole_ok" = true ] || echo "FINDING: VOLE not byte-exact for $f" >&2
done

# --- controls: the detection boundaries stay pinned --------------------------
printf 'fixture\texpect_fmt\tfmt\texpect_decline_rc\tdecline_rc\tok\n' > "$RAW/controls.tsv"
CTL_OK=0; CTL_N=0
for c in $CONTROLS; do
    echo "=== control $c ==="
    build_json=$("$BIN" field-build "$WORK/corpus/$c" --store "$WORK/ctlstore" --voldoc "$WORK/$c.voldoc" 2>/dev/null)
    fmt=$(python3 -c "import json,sys;d=json.loads(sys.argv[1]);print(d.get('format') or d.get('ingest',{}).get('format',''))" "$build_json" 2>/dev/null || echo "")
    FIELD=$(python3 -c "import json,sys;d=json.loads(sys.argv[1]);print(d['ingest']['field'])" "$build_json" 2>/dev/null || echo "")
    case "$c" in
        strict.json) expect_fmt=json ;;
        *)           expect_fmt=opaque ;;
    esac
    set +e
    "$BIN" observe --store "$WORK/ctlstore" --field "$FIELD" --metadata --kind metadata > /dev/null 2>&1
    rc=$?
    set -e
    if [ "$expect_fmt" = "json" ]; then
        # A native JSON5 selector on a strict-JSON control declines typed (rc 6).
        set +e
        "$BIN" observe --store "$WORK/ctlstore" --field "$FIELD" --json5-pointer "/a" --kind metadata > /dev/null 2>&1
        rc=$?
        set -e
    fi
    ok=false
    [ "$fmt" = "$expect_fmt" ] && [ "$rc" -eq 6 ] && ok=true
    [ "$ok" = true ] && CTL_OK=$((CTL_OK + 1))
    CTL_N=$((CTL_N + 1))
    printf '%s\t%s\t%s\t6\t%s\t%s\n' "$c" "$expect_fmt" "$fmt" "$rc" "$ok" >> "$RAW/controls.tsv"
    [ "$ok" = true ] || echo "FINDING: control $c fmt=$fmt (want $expect_fmt) rc=$rc" >&2
done

# --- toolchain / environment -------------------------------------------------
t0=$(now_ns); python3 -c pass; t1=$(now_ns); PY_STARTUP_US=$(( (t1 - t0) / 1000 ))
SQLITE_INFO=$(python3 "$BASE" version)
SQLITE_VER=$(python3 -c "import json,sys;print(json.loads(sys.argv[1])['sqlite_version'])" "$SQLITE_INFO")
FIX_JSON=$(python3 -c "import json,csv;rows=list(csv.DictReader(open('$RAW/fixtures.tsv'),delimiter='\t'));print(json.dumps({r['fixture']:{'len':int(r['src_bytes']),'sha256':r['src_sha256']} for r in rows},sort_keys=True))")
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.17 — JSON5/JSONC economic court (VOLE vs source-retaining strict-JSON SQLite + conventional JSON5 load)",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "measured_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "tree_state": "$(git status --porcelain | tr '\n' ' ' | sed 's/"/\\"/g')",
  "service": "doc-baseline",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "python": "$(python3 --version)",
  "sqlite3_cli": "$(sqlite3 --version)",
  "sqlite_module": "sqlite3",
  "sqlite_version": "$SQLITE_VER",
  "json5mini_sha256": "$(sha256sum tools/fixtures/json5mini.py | cut -d' ' -f1)",
  "arch": "$(uname -m)",
  "python_startup_us": $PY_STARTUP_US,
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "bin": "$BIN",
  "bin_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "profile": "$PROFILE",
  "build_args": "$BUILD_ARGS",
  "vole_substrate": "--profile runtime --packed",
  "lane_fixtures": "$LANE_FIXTURES",
  "controls": "$CONTROLS",
  "fixtures_len_sha256": $FIX_JSON,
  "reps": $REPS,
  "storage_accounting": "sum of regular-file sizes (find -type f -printf '%s'); du -sb never used (ADR-0049)",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

cat > "$CAMPAIGN/commands.txt" <<EOF
docker compose run --rm --no-TTY doc-baseline bash tools/phase21-17-json5-court.sh
# PROFILE defaults to release (target/release/vole-document).
# Inside the court:
#   cargo build --release --locked --all-features
#   python3 tools/fixtures/make-json5.py --corpus \$WORK/corpus
#   VOLE:   \$BIN field-build F --store STORE --voldoc F.voldoc --profile runtime --packed
#           python3 tools/fixtures/phase21-17-json5-vole.py run --bin \$BIN --store STORE --field HEX --plan ... --outdir ... --source SRC --packed
#           rm source + descriptor ; \$BIN materialize --store STORE --field HEX --exact --packed --output OUT ; cmp OUT source
#   sqlite: python3 tools/fixtures/phase21-17-json5-baseline.py build --lane sqlite --source F --out D
#           python3 tools/fixtures/phase21-17-json5-baseline.py query --lane sqlite --dir D --q Qn --plan ... --out OUT
#           python3 tools/fixtures/phase21-17-json5-baseline.py session --lane sqlite --dir D --queries Q1,...,Q12 --plan ... --out OUT
#           python3 tools/fixtures/phase21-17-json5-baseline.py materialize --lane sqlite --dir D --out OUT
#   conv:   python3 tools/fixtures/phase21-17-json5-baseline.py build --lane conv --source F --out D
#           python3 tools/fixtures/phase21-17-json5-baseline.py query --lane conv --dir D --q Qn --plan ... --out OUT
#   aggregate: python3 tools/fixtures/phase21-17-json5-baseline.py aggregate --raw RAW --campaign CAMPAIGN --env ENV
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
if [ "$verdict_rc" -eq 0 ] && [ "$EXACT_OK" -eq "$EXACT_N" ] && [ "$EXACT_N" -gt 0 ] && [ "$CTL_OK" -eq "$CTL_N" ]; then
    echo "PHASE 21.17 JSON5 COURT: PASS — VOLE exactness ${EXACT_OK}/${EXACT_N}, controls ${CTL_OK}/${CTL_N} — campaign $CAMPAIGN"
    exit 0
fi
echo "PHASE 21.17 JSON5 COURT: FAIL — VOLE exactness ${EXACT_OK}/${EXACT_N}, controls ${CTL_OK}/${CTL_N} — campaign $CAMPAIGN" >&2
exit 1
