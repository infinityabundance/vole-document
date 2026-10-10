#!/usr/bin/env bash
# Phase 21.20-21.24 — the CONFIG / FEED / GEOJSON / GIS / NOTEBOOK ECONOMIC courts
# (shared engine).
#
# One engine, parameterised by FORMAT (config|feed|geojson|gis|notebook), so the
# five courts cannot drift apart. Pre-registered. Hypotheses:
#
#   H1 (byte-exactness / Q6) — for every corpus fixture (including the malformed and
#       Opaque controls), VOLE `materialize --exact == source` (length + SHA-256 +
#       `cmp`) after the source file AND the standalone descriptor are deleted, in a
#       fresh process. The court FAILS unless this is 100 %.
#   H2 (contract questions) — the three lanes (VOLE; a **source-retaining** store that
#       keeps the raw bytes AND a conventional extraction; a **conventional
#       decode-to-host-values load**: configparser-style for config,
#       `xml.etree.ElementTree` for feed/gis, `json` for geojson/notebook) answer the
#       SAME twelve questions Q1–Q12 where they can, and every question a lane cannot
#       answer is a TYPED decline, never a silent empty answer.
#   H3 (cross-lane agreement) — where VOLE and a comparator both answer, comparable
#       values agree (a canonical value, a kind, a duplicate count, a match set, a
#       materialized length+SHA). VOLE answers the representation questions (source
#       spans, exact token spelling, attribute/quote/continuation markers) that the
#       conventional load declines. Those gaps are recorded, never papered over.
#   H4 (economics) — build/storage/cold/warm are measured per lane with a stated
#       estimator (paired per-fixture ratios, median + geometric mean, fixed-seed
#       cluster bootstrap by fixture, ratio-of-sums reported separately), every raw
#       sample retained. Wall times are microseconds (`us`).
#
# Honest scope: a self-authored deterministic corpus; only Q6 is a byte-authority
# claim. The conventional load is deliberately the weaker comparator. Runs in the
# pinned, hard-capped `doc-baseline` service (dev toolchain + python3 + sqlite3);
# never the host:
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-20-config-court.sh
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-21-feed-court.sh
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-22-geojson-court.sh
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-23-gis-court.sh
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-24-notebook-court.sh

set -uo pipefail
cd /work
export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1

FORMAT=${1:?usage: textfmt-court.sh config|feed|geojson|gis|notebook}
case "$FORMAT" in
    config)   PHASE=21.20; PHASETAG=21-20; FMTNAME=config ;;
    feed)     PHASE=21.21; PHASETAG=21-21; FMTNAME=RSS/Atom ;;
    geojson)  PHASE=21.22; PHASETAG=21-22; FMTNAME=GeoJSON ;;
    gis)      PHASE=21.23; PHASETAG=21-23; FMTNAME=KML/GPX ;;
    notebook) PHASE=21.24; PHASETAG=21-24; FMTNAME=Jupyter-notebook ;;
    *) echo "unknown format: $FORMAT" >&2; exit 2 ;;
esac

BASE=tools/fixtures/textfmt_econ.py
VOLE=tools/fixtures/textfmt-vole.py
GEN=tools/fixtures/make-${FORMAT}.py
SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN="evidence/campaigns/${STAMP}-phase${PHASETAG}-${FORMAT}-econ-${SHA}"
RAW="$CAMPAIGN/raw"
WORK="evidence/scratch/phase${PHASETAG}-${FORMAT}-econ"
REPS=${REPS:-3}

PROFILE=${PROFILE:-release}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-${VOLE_BIN:-target/debug/vole-document}} ;;
esac

# --- per-format native selectors (own/foreign), frozen -----------------------
case "$FORMAT" in
    config)   OWN="--config-line 0 --kind structure";         FOREIGN="--feed-channel --kind structure" ;;
    feed)     OWN="--feed-channel --kind structure";          FOREIGN="--geojson-type --kind metadata" ;;
    geojson)  OWN="--geojson-type --kind metadata";           FOREIGN="--gis-root --kind structure" ;;
    gis)      OWN="--gis-root --kind structure";               FOREIGN="--notebook-nbformat --kind metadata" ;;
    notebook) OWN="--notebook-nbformat --kind metadata";      FOREIGN="--config-line 0 --kind structure" ;;
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
python3 "$GEN" --corpus "$WORK/corpus" | tee "$RAW/corpus.txt"
python3 "$BASE" plan --format "$FORMAT" > "$RAW/plan.json"

LANE_FIXTURES=$(python3 -c "import json;print(' '.join(json.load(open('$RAW/plan.json'))['lane_fixtures']))")
CONTROLS=$(python3 -c "import json;print(' '.join(json.load(open('$RAW/plan.json'))['controls']))")

for f in $LANE_FIXTURES; do cp "$WORK/corpus/$f" "$WORK/ref/"; done

{
    printf 'fixture\tsrc_bytes\tsrc_sha256\n'
    while IFS=$'\t' read -r name bytes sha; do
        [ -n "$name" ] || continue
        case " $LANE_FIXTURES " in *" $name "*) printf '%s\t%s\t%s\n' "$name" "$bytes" "$sha" ;; esac
    done < "$RAW/corpus.txt"
} > "$RAW/fixtures.tsv"

now_ns() { date +%s%N; }
plan_for() { python3 -c "import json;print(json.dumps(json.load(open('$RAW/plan.json'))['plans']['$1']))"; }

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
        sqlite) python3 "$BASE" build --format "$FORMAT" --lane sqlite --source "$WORK/corpus/$f" --out "$dir" > "$dir/build.json" 2> "$dir/build.err" ;;
        conv)   python3 "$BASE" build --format "$FORMAT" --lane conv --source "$WORK/corpus/$f" --out "$dir" > "$dir/build.json" 2> "$dir/build.err" ;;
    esac
}

lane_order() { # rep
    case $(( ($1 - 1) % 3 )) in
        0) echo "vole sqlite conv" ;;
        1) echo "conv vole sqlite" ;;
        *) echo "sqlite conv vole" ;;
    esac
}

QS="Q1 Q2 Q3 Q4 Q5 Q6 Q7 Q8 Q9 Q10 Q11 Q12"

for f in $LANE_FIXTURES; do
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
                python3 "$VOLE" run --format "$FORMAT" --bin "$BIN" --store "$VDIR/store" \
                    --field "$FIELD" --plan "$PLAN" --outdir "$VDIR/run$rep" \
                    --source "$WORK/ref/$f" --packed >/dev/null
                for q in $QS; do
                    us=$(python3 -c "import json;print(json.load(open('$VDIR/run$rep/cold_us.json')).get('$q',0))" 2>/dev/null || echo 0)
                    printf '%s\t%s\t%s\t%s\t0\t%s\n' "$f" "$lane" "$q" "$rep" "$us" >> "$RAW/cold.tsv"
                    wus=$(python3 -c "import json;print(json.load(open('$VDIR/run$rep/warm_us.json')).get('$q',0))" 2>/dev/null || echo 0)
                    printf '%s\t%s\t%s\t%s\t0\t%s\n' "$f" "$lane" "$q" "$rep" "$wus" >> "$RAW/warm.tsv"
                    if [ "$rep" -eq 1 ]; then
                        cp "$VDIR/run$rep/qanswers/$q.json" "$RAW/qanswers/$f.$q.vole.json" 2>/dev/null || true
                        cp "$VDIR/run$rep/warm/$q.json" "$RAW/vole_warm/$f.$q.json" 2>/dev/null || true
                    fi
                done
            else
                case "$lane" in sqlite) dir="$SDIR" ;; conv) dir="$CDIR" ;; esac
                for q in $QS; do
                    t0=$(now_ns)
                    python3 "$BASE" query --format "$FORMAT" --lane "$lane" --dir "$dir" --q "$q" --plan "$PLAN" \
                        --out "$dir/run${rep}_$q.json" >/dev/null 2>&1; rc=$?
                    t1=$(now_ns)
                    us=$(( (t1 - t0) / 1000 ))
                    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$f" "$lane" "$q" "$rep" "$rc" "$us" >> "$RAW/cold.tsv"
                    if [ "$rep" -eq 1 ]; then cp "$dir/run${rep}_$q.json" "$RAW/qanswers/$f.$q.$lane.json" 2>/dev/null || true; fi
                done
                python3 "$BASE" session --format "$FORMAT" --lane "$lane" --dir "$dir" \
                    --queries "Q1,Q2,Q3,Q4,Q5,Q6,Q7,Q8,Q9,Q10,Q11,Q12" \
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
    python3 "$BASE" materialize --format "$FORMAT" --lane sqlite --dir "$SDIR" --out "$WORK/$f.sql.out" >/dev/null 2>&1; s_rc=$?
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
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$f" "$vole_ok" "$sql_ok" "$len_ok" "$sha_ok" "$cmp_ok" "$v_us" "$s_us" >> "$RAW/exact.tsv"
    [ "$vole_ok" = true ] || echo "FINDING: VOLE not byte-exact for $f" >&2
done

# --- controls: detection boundaries + H1 over the whole corpus ---------------
printf 'fixture\texpect_fmt\tfmt\texpect_rc\trc\tcommon_rc\town_rc\tforeign_rc\tok\n' > "$RAW/controls.tsv"
CTL_OK=0; CTL_N=0
for c in $CONTROLS; do
    echo "=== control $c ==="
    expect_fmt=$(python3 -c "import json;print(json.load(open('$RAW/plan.json'))['controls']['$c'])")
    build_json=$("$BIN" field-build "$WORK/corpus/$c" --store "$WORK/ctlstore" --voldoc "$WORK/$c.voldoc" --profile runtime --packed 2>/dev/null)
    fmt=$(python3 -c "import json,sys;d=json.loads(sys.argv[1]);print(d.get('format') or d.get('ingest',{}).get('format',''))" "$build_json" 2>/dev/null || echo "")
    FIELD=$(python3 -c "import json,sys;d=json.loads(sys.argv[1]);print(d['ingest']['field'])" "$build_json" 2>/dev/null || echo "")

    "$BIN" observe --store "$WORK/ctlstore" --field "$FIELD" --packed --metadata --kind metadata > /dev/null 2>&1; common_rc=$?
    # shellcheck disable=SC2086
    "$BIN" observe --store "$WORK/ctlstore" --field "$FIELD" --packed $OWN > /dev/null 2>&1; own_rc=$?
    # shellcheck disable=SC2086
    "$BIN" observe --store "$WORK/ctlstore" --field "$FIELD" --packed $FOREIGN > /dev/null 2>&1; foreign_rc=$?

    if [ "$expect_fmt" = "opaque" ]; then
        expect_common=6
    else
        expect_common=0
    fi
    if [ "$expect_fmt" = "$FORMAT" ]; then
        expect_own=0
    else
        expect_own=6
    fi
    rc=$own_rc
    expect_rc=$expect_own
    ok=false
    if [ "$fmt" = "$expect_fmt" ] && [ "$common_rc" -eq "$expect_common" ] \
        && [ "$own_rc" -eq "$expect_own" ] && [ "$foreign_rc" -eq 6 ]; then
        ok=true
    fi
    [ "$ok" = true ] && CTL_OK=$((CTL_OK + 1))
    CTL_N=$((CTL_N + 1))
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$c" "$expect_fmt" "$fmt" "$expect_rc" "$rc" "$common_rc" "$own_rc" "$foreign_rc" "$ok" >> "$RAW/controls.tsv"
    [ "$ok" = true ] || echo "FINDING: control $c fmt=$fmt (want $expect_fmt) common=$common_rc (want $expect_common) own=$own_rc (want $expect_own) foreign=$foreign_rc (want 6)" >&2

    # H1 over the controls too: delete source + descriptor, rematerialize, compare.
    cp "$WORK/corpus/$c" "$WORK/ref/$c"
    src_len=$(wc -c < "$WORK/ref/$c" | tr -d ' ')
    src_sha=$(sha256sum "$WORK/ref/$c" | cut -d' ' -f1)
    rm -f "$WORK/corpus/$c" "$WORK/$c.voldoc"
    t0=$(now_ns)
    "$BIN" materialize --store "$WORK/ctlstore" --field "$FIELD" --exact --packed --output "$WORK/$c.out" >/dev/null 2>&1; v_rc=$?
    t1=$(now_ns)
    v_us=$(( (t1 - t0) / 1000 ))
    out_len=$(wc -c < "$WORK/$c.out" 2>/dev/null | tr -d ' ')
    out_sha=$(sha256sum "$WORK/$c.out" 2>/dev/null | cut -d' ' -f1)
    len_ok=false; [ "$out_len" = "$src_len" ] && len_ok=true
    sha_ok=false; [ "$out_sha" = "$src_sha" ] && sha_ok=true
    cmp_ok=false; cmp -s "$WORK/ref/$c" "$WORK/$c.out" && cmp_ok=true
    vole_ok=false
    if [ "$v_rc" -eq 0 ] && [ "$len_ok" = true ] && [ "$sha_ok" = true ] && [ "$cmp_ok" = true ]; then vole_ok=true; fi
    printf '%s\t%s\t-\t%s\t%s\t%s\t%s\t-\n' "$c" "$vole_ok" "$len_ok" "$sha_ok" "$cmp_ok" "$v_us" >> "$RAW/exact.tsv"
    [ "$vole_ok" = true ] || echo "FINDING: VOLE not byte-exact for control $c" >&2
done

# --- toolchain / environment -------------------------------------------------
t0=$(now_ns); python3 -c pass; t1=$(now_ns); PY_STARTUP_US=$(( (t1 - t0) / 1000 ))
SQ_INFO=$(python3 "$BASE" version --format "$FORMAT")
SQLITE_VER=$(python3 -c "import json,sys;print(json.loads(sys.argv[1])['sqlite_version'])" "$SQ_INFO")
FIX_JSON=$(python3 -c "import json,csv;rows=list(csv.DictReader(open('$RAW/fixtures.tsv'),delimiter='\t'));print(json.dumps({r['fixture']:{'len':int(r['src_bytes']),'sha256':r['src_sha256']} for r in rows},sort_keys=True))")
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "${PHASE} — ${FMTNAME} economic court",
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
  "format": "$FORMAT",
  "econ_module_sha256": "$(sha256sum $BASE | cut -d' ' -f1)",
  "vole_probe_sha256": "$(sha256sum $VOLE | cut -d' ' -f1)",
  "generator_sha256": "$(sha256sum $GEN | cut -d' ' -f1)",
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
  "own_selector": "$OWN",
  "foreign_selector": "$FOREIGN",
  "fixtures_len_sha256": $FIX_JSON,
  "reps": $REPS,
  "storage_accounting": "sum of regular-file sizes (find -type f -printf '%s'); du -sb never used (ADR-0049)",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

cat > "$CAMPAIGN/commands.txt" <<EOF
docker compose run --rm --no-TTY doc-baseline bash tools/phase${PHASETAG}-${FORMAT}-court.sh
# PROFILE defaults to release (target/release/vole-document).
# Inside the court:
#   cargo build $BUILD_ARGS
#   python3 $GEN --corpus \$WORK/corpus
#   python3 $BASE plan --format $FORMAT
#   VOLE:   \$BIN field-build F --store STORE --voldoc F.voldoc --profile runtime --packed
#           python3 $VOLE run --format $FORMAT --bin \$BIN --store STORE --field HEX --plan ... --outdir ... --source SRC --packed
#           rm source + descriptor ; \$BIN materialize --store STORE --field HEX --exact --packed --output OUT ; cmp OUT source
#   sqlite: python3 $BASE build --format $FORMAT --lane sqlite --source F --out D
#           python3 $BASE query --format $FORMAT --lane sqlite --dir D --q Qn --plan ... --out OUT
#           python3 $BASE session --format $FORMAT --lane sqlite --dir D --queries Q1,...,Q12 --plan ... --out OUT
#           python3 $BASE materialize --format $FORMAT --lane sqlite --dir D --out OUT
#   conv:   python3 $BASE build --format $FORMAT --lane conv --source F --out D
#           python3 $BASE query --format $FORMAT --lane conv --dir D --q Qn --plan ... --out OUT
#   aggregate: python3 $BASE aggregate --format $FORMAT --raw RAW --campaign CAMPAIGN --env ENV
# Full dev gate:
#   cargo fmt --all --check
#   cargo clippy --all-targets --all-features -- -D warnings
#   cargo test --locked --all-features
EOF

# --- receipt facts (per format) ---------------------------------------------
# Single-quoted so backticks and backslashes are literal (no command substitution).
case "$FORMAT" in
    config)
        SURFACE='line order and kind; exact spans for lines, keys, separators, values, and comments; the section/entry/comment vocabulary; = vs : separator spelling; export, single/double quoting, empty values, the Java .properties trailing-backslash continuation, and \uXXXX escapes preserved as spelling; duplicate keys reported not collapsed; the recorded dialect (ini/env/properties)'
        DECLINES='an out-of-range line/entry and an unknown section decline typed (rc 6); the strict-TOML control stays Toml and a native config selector on it declines typed; the strict-JSON control stays Json; the pure KEY=VALUE overlap, prose, and a shebang script stay Opaque and a common observation declines typed (rc 6), never a panic'
        DETECTION='config-family detection is byte-based and conservative: an INI [section] header, an .env export signal, or a Java-properties-only construct (trailing-backslash continuation, \uXXXX, or a : separator); the pure KEY=VALUE env/properties overlap is never guessed'
        RC_CODES='0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit'
        ;;
    feed)
        SURFACE='RSS 2.0 channel/item and Atom feed/entry; channel/feed-level fields; entry order; exact element spans; element attributes in source order with exact spelling; CDATA and entities; duplicate entry elements; the recorded dialect (rss/atom)'
        DECLINES='an out-of-range entry and an unknown field decline typed (rc 6); a non-feed XML control stays Xml and a native feed selector on it declines typed; the strict-JSON control stays Json; malformed XML and prose stay Opaque and a common observation declines typed (rc 6), never a panic'
        DETECTION='feed detection requires an RSS rss/channel or Atom feed root; other XML stays Xml and is never stolen'
        RC_CODES='0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit'
        ;;
    geojson)
        SURFACE='a FeatureCollection and a bare Feature; Point/LineString/Polygon/GeometryCollection; exact numeric coordinate spelling; properties member order and duplicate keys; foreign (non-core) members preserved; id/bbox; the root type'
        DECLINES='an out-of-range feature/geometry and an unknown property decline typed (rc 6); a plain JSON control (including a wrong type) stays Json and a native GeoJSON selector declines typed; malformed JSON and prose stay Opaque and a common observation declines typed (rc 6), never a panic'
        DETECTION='GeoJSON requires a JSON object with a recognized type (Feature/FeatureCollection/a geometry class) and GeoJSON structure; a plain JSON document stays Json and is never stolen'
        RC_CODES='0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit'
        ;;
    gis)
        SURFACE='KML Document/Folder/Placemark and GPX wpt/rte/trk; point coordinates (KML coordinates text vs GPX lat/lon attributes); element namespace prefixes; name/description fields; duplicate record fields; the recorded dialect (kml/gpx)'
        DECLINES='an out-of-range record/point and an unknown field decline typed (rc 6); a non-GIS XML control stays Xml and a native GIS selector declines typed; the strict-JSON control stays Json; malformed XML and prose stay Opaque and a common observation declines typed (rc 6), never a panic'
        DETECTION='GIS detection requires a kml or gpx root element; other XML stays Xml and is never stolen'
        RC_CODES='0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit'
        ;;
    notebook)
        SURFACE='nbformat major/minor; code/markdown/raw cells; source as a string vs a line array preserved distinctly; execution_count present/absent/null; the four output kinds; cell order; duplicate members; exact source-token bytes'
        DECLINES='an out-of-range cell/output declines typed (rc 6); a plain JSON control and a JSON document missing nbformat stay Json and a native notebook selector declines typed; malformed JSON and prose stay Opaque and a common observation declines typed (rc 6), never a panic'
        DETECTION='notebook detection requires a JSON object with nbformat and a cells array whose cell objects carry a cell_type; a plain JSON document stays Json and is never stolen'
        RC_CODES='0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit'
        ;;
esac

echo "=== aggregate ==="
python3 "$BASE" aggregate --format "$FORMAT" --raw "$RAW" --campaign "$CAMPAIGN" --env "$CAMPAIGN/environment.json"
verdict_rc=$?

# Fact strings contain backslashes and quotes; write them raw and let Python build
# the receipt JSON (a shell heredoc cannot escape JSON safely).
printf '%s' "$SURFACE" > "$CAMPAIGN/.surface"
printf '%s' "$DECLINES" > "$CAMPAIGN/.declines"
printf '%s' "$DETECTION" > "$CAMPAIGN/.detection"
printf '%s' "$RC_CODES" > "$CAMPAIGN/.rccodes"

EXACT_OK=$(awk -F'\t' 'NR>1 && $2=="true"' "$RAW/exact.tsv" | wc -l | tr -d ' ')
EXACT_N=$(awk -F'\t' 'NR>1' "$RAW/exact.tsv" | wc -l | tr -d ' ')
CTL_N=$(awk -F'\t' 'NR>1' "$RAW/controls.tsv" | wc -l | tr -d ' ')

echo
python3 - "$CAMPAIGN" "$verdict_rc" "$EXACT_OK" "$EXACT_N" "$CTL_OK" "$CTL_N" <<'PYEOF'
import json, os, subprocess, sys
campaign, vrc, eok, en, cok, cn = sys.argv[1:7]
vrc, eok, en, cok, cn = int(vrc), int(eok), int(en), int(cok), int(cn)

def rd(name):
    with open(os.path.join(campaign, name)) as f:
        return f.read()

def sh(cmd):
    return subprocess.run(cmd, capture_output=True, text=True).stdout.strip()

rc = json.load(open(os.path.join(campaign, "receipt.json")))
extra = {
    "utc": sh(["date", "-u", "+%Y-%m-%dT%H:%M:%SZ"]),
    "measured_commit": sh(["git", "rev-parse", "HEAD"]) or "unknown",
    "tree_state": sh(["git", "status", "--porcelain"]).replace("\n", " "),
    "service": "doc-baseline",
    "surface": rd(".surface"),
    "declines": rd(".declines"),
    "detection_boundary": rd(".detection"),
    "rc_codes": rd(".rccodes"),
    "observations": ["metadata", "text", "search-match",
                     "native selectors (exact + structure)"],
}
rc.update(extra)
verdict = "PASS" if (vrc == 0 and eok == en and en > 0 and cok == cn) else "FAIL"
rc["verdict"] = verdict
json.dump(rc, open(os.path.join(campaign, "receipt.json"), "w"), indent=2, sort_keys=True)
for f in (".surface", ".declines", ".detection", ".rccodes"):
    try:
        os.remove(os.path.join(campaign, f))
    except OSError:
        pass
name = rc.get("phase", "")
if verdict == "PASS":
    print("PHASE %s COURT: PASS — VOLE exactness %d/%d, controls %d/%d — campaign %s"
          % (name, eok, en, cok, cn, campaign))
    sys.exit(0)
print("PHASE %s COURT: FAIL — VOLE exactness %d/%d, controls %d/%d — campaign %s"
      % (name, eok, en, cok, cn, campaign), file=sys.stderr)
sys.exit(1)
PYEOF
