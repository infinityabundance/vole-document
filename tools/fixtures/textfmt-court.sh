#!/usr/bin/env bash
# Phase 21.20-21.35 + 27.4 — the CONFIG / FEED / GEOJSON / GIS / NOTEBOOK / TABULAR
# (PSV + fixed-width) / RST / ASCIIDOC / MDX / MHTML / LOGSTREAM / PKGMETA /
# APISPEC / BIB / RTF / CONTAINER / HDF / CFBF / legacy-OFFICE (doc/xls/ppt/msg)
# ECONOMIC courts (shared engine).
#
# One engine, parameterised by FORMAT
# (config|feed|geojson|gis|notebook|tabular|rst|asciidoc|mdx|…|doc|xls|ppt|msg),
# so the courts cannot drift apart. Pre-registered. Hypotheses:
#
#   H1 (byte-exactness / Q6) — for every corpus fixture (including the malformed and
#       Opaque controls), VOLE `materialize --exact == source` (length + SHA-256 +
#       `cmp`) after the source file AND the standalone descriptor are deleted, in a
#       fresh process. The court FAILS unless this is 100 %.
#   H2 (contract questions) — the lanes (VOLE; a **source-retaining** store that
#       keeps the raw bytes AND a conventional extraction; a **conventional
#       decode-to-host-values load**: configparser-style for config,
#       `xml.etree.ElementTree` for feed/gis, `json` for geojson/notebook; and, for
#       the tabular format only, the mandatory **DuckDB/Parquet** analytical
#       comparator of ADR-0059) answer the
#       SAME twelve questions Q1–Q12 where they can, and every question a lane
#       cannot answer is a TYPED decline, never a silent empty answer.
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
# pinned, hard-capped `doc-baseline` service (dev toolchain + python3 + sqlite3) —
# or `analytical` (same base + hash-pinned DuckDB) for the tabular format, which
# additionally carries the mandatory DuckDB/Parquet comparator; never the host:
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-20-config-court.sh
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-21-feed-court.sh
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-22-geojson-court.sh
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-23-gis-court.sh
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-24-notebook-court.sh
#   docker compose run --rm --no-TTY analytical  bash tools/phase21-25-tabular-econ-court.sh
#     (the tabular format also carries the mandatory DuckDB/Parquet comparator, ADR-0059)
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-26-1-rst-econ-court.sh
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-26-2-asciidoc-econ-court.sh
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-26-3-mdx-econ-court.sh
#
# Phase 27.4 — the four legacy CFBF-backed Office formats. Lane C (the
# conventional decode-to-host-values load) is `comparator_absent`: there is no
# faithful pure-Python/stdlib decoder for `.doc`/`.xls`/`.ppt`/`.msg`, so every
# Lane-C question is a typed `comparator_absent` decline, exactly like HDF5/CFBF.
# The source-retaining SQLite lane keeps the raw bytes (byte authority) and
# declines every structural question typed:
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase27-4-doc-econ-court.sh
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase27-4-xls-econ-court.sh
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase27-4-ppt-econ-court.sh
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase27-4-msg-econ-court.sh

set -uo pipefail
cd /work
export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1

FORMAT=${1:?usage: textfmt-court.sh config|feed|geojson|gis|notebook|tabular|rst|asciidoc|mdx}
case "$FORMAT" in
    config)   PHASE=21.20; PHASETAG=21-20; FMTNAME=config ;;
    feed)     PHASE=21.21; PHASETAG=21-21; FMTNAME=RSS/Atom ;;
    geojson)  PHASE=21.22; PHASETAG=21-22; FMTNAME=GeoJSON ;;
    gis)      PHASE=21.23; PHASETAG=21-23; FMTNAME=KML/GPX ;;
    notebook) PHASE=21.24; PHASETAG=21-24; FMTNAME=Jupyter-notebook ;;
    tabular)  PHASE=21.25; PHASETAG=21-25; FMTNAME="PSV + fixed-width" ;;
    rst)      PHASE=21.26.1; PHASETAG=21-26-1; FMTNAME=reStructuredText ;;
    asciidoc) PHASE=21.26.2; PHASETAG=21-26-2; FMTNAME=AsciiDoc ;;
    mdx)      PHASE=21.26.3; PHASETAG=21-26-3; FMTNAME=MDX ;;
    mhtml)    PHASE=21.27; PHASETAG=21-27; FMTNAME=MHTML ;;
    logstream) PHASE=21.28; PHASETAG=21-28; FMTNAME="log stream" ;;
    pkgmeta)  PHASE=21.29; PHASETAG=21-29; FMTNAME="package metadata" ;;
    apispec)  PHASE=21.30; PHASETAG=21-30; FMTNAME="API specification" ;;
    bib)      PHASE=21.31; PHASETAG=21-31; FMTNAME=bibliography ;;
    rtf)      PHASE=21.32; PHASETAG=21-32; FMTNAME=RTF ;;
    container) PHASE=21.33; PHASETAG=21-33; FMTNAME="archive container (ZIP/TAR)" ;;
    hdf)      PHASE=21.34; PHASETAG=21-34; FMTNAME="scientific container (NetCDF/HDF5)" ;;
    cfbf)     PHASE=21.35; PHASETAG=21-35; FMTNAME="CFBF/OLE2 compound file" ;;
    doc)      PHASE=27.4; PHASETAG=27-4; FMTNAME="legacy Word 97–2003 (.doc, MS-DOC)" ;;
    xls)      PHASE=27.4; PHASETAG=27-4; FMTNAME="legacy Excel 97–2003 (.xls, BIFF8)" ;;
    ppt)      PHASE=27.4; PHASETAG=27-4; FMTNAME="legacy PowerPoint 97–2003 (.ppt, MS-PPT)" ;;
    msg)      PHASE=27.4; PHASETAG=27-4; FMTNAME="legacy Outlook message (.msg, MS-OXMSG)" ;;
    *) echo "unknown format: $FORMAT" >&2; exit 2 ;;
esac

BASE=tools/fixtures/textfmt_econ.py
VOLE=tools/fixtures/textfmt-vole.py
GEN=tools/fixtures/make-${FORMAT}.py
SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP="${ECON_STAMP:-$(date -u +%Y-%m-%d)}"
CAMPAIGN="evidence/campaigns/${STAMP}-phase${PHASETAG}-${FORMAT}-econ-${SHA}"
RAW="$CAMPAIGN/raw"
WORK="evidence/scratch/phase${PHASETAG}-${FORMAT}-econ"
REPS=${REPS:-3}

PROFILE=${PROFILE:-release}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-${VOLE_BIN:-target/debug/vole-document}} ;;
esac

# The tabular court carries the DuckDB/Parquet comparator (ADR-0059), so it runs
# in the `analytical` service (same pinned base + Python + hash-pinned DuckDB);
# every other format runs in `doc-baseline` and keeps its two-lane set.
COURT_SERVICE=doc-baseline
[ "$FORMAT" = "tabular" ] && COURT_SERVICE=analytical

# --- per-format native selectors (own/foreign), frozen -----------------------
case "$FORMAT" in
    config)   OWN="--config-line 0 --kind structure";         FOREIGN="--feed-channel --kind structure" ;;
    feed)     OWN="--feed-channel --kind structure";          FOREIGN="--geojson-type --kind metadata" ;;
    geojson)  OWN="--geojson-type --kind metadata";           FOREIGN="--gis-root --kind structure" ;;
    gis)      OWN="--gis-root --kind structure";               FOREIGN="--notebook-nbformat --kind metadata" ;;
    notebook) OWN="--notebook-nbformat --kind metadata";     FOREIGN="--config-line 0 --kind structure" ;;
    tabular)  OWN="--fixedwidth-row 0 --kind structure";      FOREIGN="--rst-heading 0 --kind structure" ;;
    rst)      OWN="--rst-heading 0 --kind structure";         FOREIGN="--adoc-heading 0 --kind structure" ;;
    asciidoc) OWN="--adoc-heading 0 --kind structure";        FOREIGN="--mdx-heading 0 --kind structure" ;;
    mdx)      OWN="--mdx-heading 0 --kind structure";         FOREIGN="--adoc-heading 0 --kind structure" ;;
    mhtml)    OWN="--mhtml-root --kind metadata";             FOREIGN="--logstream-line 0 --kind metadata" ;;
    logstream) OWN="--logstream-line 0 --kind metadata";      FOREIGN="--pkgmeta-section 0 --kind metadata" ;;
    pkgmeta)  OWN="--pkgmeta-section 0 --kind metadata";       FOREIGN="--apispec-object 0 --kind metadata" ;;
    apispec)  OWN="--apispec-object 0 --kind metadata";        FOREIGN="--bib-entry 0 --kind metadata" ;;
    bib)      OWN="--bib-entry 0 --kind metadata";             FOREIGN="--rtf-header --kind text" ;;
    rtf)      OWN="--rtf-header --kind metadata";              FOREIGN="--container-dialect --kind text" ;;
    container) OWN="--container-dialect --kind text";           FOREIGN="--hdf-dialect --kind text" ;;
    hdf)      OWN="--hdf-dialect --kind text";                 FOREIGN="--cfbf-dialect --kind text" ;;
    cfbf)     OWN="--cfbf-dialect --kind text";                FOREIGN="--mhtml-root --kind metadata" ;;
    doc)      OWN="--doc-fib --kind structure";                 FOREIGN="--xls-biff --kind structure" ;;
    xls)      OWN="--xls-biff --kind structure";                FOREIGN="--ppt-document --kind structure" ;;
    ppt)      OWN="--ppt-document --kind structure";             FOREIGN="--msg-summary --kind structure" ;;
    msg)      OWN="--msg-summary --kind structure";              FOREIGN="--doc-fib --kind structure" ;;
esac

WRAPFILE="tools/phase${PHASETAG}-${FORMAT}-court.sh"
case "$FORMAT" in
    tabular|rst|asciidoc|mdx|mhtml|logstream|pkgmeta|apispec|bib|rtf|container|hdf|cfbf|doc|xls|ppt|msg)
        WRAPFILE="tools/phase${PHASETAG}-${FORMAT}-econ-court.sh" ;;
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
        duckdb) python3 "$BASE" build --format "$FORMAT" --lane duckdb --source "$WORK/corpus/$f" --out "$dir" > "$dir/build.json" 2> "$dir/build.err" ;;
    esac
}

# The tabular format carries the mandatory DuckDB/Parquet comparator as a third
# comparator lane (ADR-0059); every other format keeps its two-lane set.
if [ "$FORMAT" = "tabular" ]; then
    LANES="vole sqlite conv duckdb"
else
    LANES="vole sqlite conv"
fi

lane_order() { # rep
    if [ "$FORMAT" = "tabular" ]; then
        case $(( ($1 - 1) % 4 )) in
            0) echo "vole sqlite conv duckdb" ;;
            1) echo "duckdb vole sqlite conv" ;;
            2) echo "conv duckdb vole sqlite" ;;
            *) echo "sqlite conv duckdb vole" ;;
        esac
    else
        case $(( ($1 - 1) % 3 )) in
            0) echo "vole sqlite conv" ;;
            1) echo "conv vole sqlite" ;;
            *) echo "sqlite conv vole" ;;
        esac
    fi
}

QS="Q1 Q2 Q3 Q4 Q5 Q6 Q7 Q8 Q9 Q10 Q11 Q12"

for f in $LANE_FIXTURES; do
    echo "=== $f ==="
    PLAN=$(plan_for "$f")
    VDIR="$WORK/lanes/$f.vole"
    SDIR="$WORK/lanes/$f.sqlite"
    CDIR="$WORK/lanes/$f.conv"
    DDDIR="$WORK/lanes/$f.duckdb"

    # ---- build (best-of-N, rotating lane order per rep) ---------------------
    for rep in $(seq 1 "$REPS"); do
        for lane in $(lane_order "$rep"); do
            case "$lane" in vole) dir="$VDIR" ;; sqlite) dir="$SDIR" ;; conv) dir="$CDIR" ;; duckdb) dir="$DDDIR" ;; esac
            t0=$(now_ns)
            build_lane "$f" "$lane" "$dir"; rc=$?
            t1=$(now_ns)
            us=$(( (t1 - t0) / 1000 ))
            printf '%s\t%s\t%s\t%s\t%s\n' "$f" "$lane" "$rep" "$rc" "$us" >> "$RAW/build.tsv"
            [ "$rc" -eq 0 ] || echo "FINDING: build failed $f/$lane rc=$rc" >&2
        done
    done

    # ---- storage (sum of regular-file sizes; never du -sb) ------------------
    for lane in $LANES; do
        case "$lane" in vole) dir="$VDIR" ;; sqlite) dir="$SDIR" ;; conv) dir="$CDIR" ;; duckdb) dir="$DDDIR" ;; esac
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
                case "$lane" in sqlite) dir="$SDIR" ;; conv) dir="$CDIR" ;; duckdb) dir="$DDDIR" ;; esac
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

    expect_spec=$(python3 -c "import json;print(json.load(open('$RAW/plan.json'))['controls']['$c'])")
    # A control may declare its expected generic-observation return code as
    # `FMT@RC` (e.g. an OPC office package needs an explicit extraction profile and
    # declines the generic metadata observation with rc 20). Plain `FMT` keeps the
    # historical rule (opaque -> 6, otherwise 0).
    expect_fmt=${expect_spec%%@*}
    if [ "$expect_spec" != "$expect_fmt" ]; then
        expect_common=${expect_spec##*@}
    elif [ "$expect_fmt" = "opaque" ]; then
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
    # A native selector on a foreign/opaque input must DECLINE. Some families use
    # the generic unsupported-feature code (6); the tabular family uses the
    # structured invalid-csv-structure (23) / invalid-fixedwidth-structure (39)
    # codes instead. Both are typed declines, so accept any non-zero rc here and a
    # zero rc where the format is the expected one.
    own_ok=false
    if [ "$expect_own" -eq 0 ]; then
        [ "$own_rc" -eq 0 ] && own_ok=true
    else
        [ "$own_rc" -ne 0 ] && own_ok=true
    fi
    if [ "$fmt" = "$expect_fmt" ] && [ "$common_rc" -eq "$expect_common" ] \
        && [ "$own_ok" = true ] && [ "$foreign_rc" -eq 6 ]; then
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

# The tabular court runs in the `analytical` service (DuckDB 1.5.6 + Python
# stdlib), so record that service and the pinned DuckDB version honestly. Every
# other format keeps the hand-written environment.json above unchanged.
if [ "$FORMAT" = "tabular" ]; then
python3 - "$CAMPAIGN/environment.json" <<'PYEOF'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
import duckdb
d["service"] = "analytical"
d["derives_from"] = "doc-baseline (same pinned base digest)"
d["duckdb_version"] = duckdb.__version__
json.dump(d, open(p, "w"), indent=2, sort_keys=True)
PYEOF
fi

cat > "$CAMPAIGN/commands.txt" <<EOF
docker compose run --rm --no-TTY $COURT_SERVICE bash $WRAPFILE
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

if [ "$FORMAT" = "tabular" ]; then
cat >> "$CAMPAIGN/commands.txt" <<EOF
#   duckdb: python3 $BASE build --format $FORMAT --lane duckdb --source F --out D
#           python3 $BASE query --format $FORMAT --lane duckdb --dir D --q Qn --plan ... --out OUT
#           python3 $BASE session --format $FORMAT --lane duckdb --dir D --queries Q1,...,Q12 --plan ... --out OUT
EOF
fi

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
    tabular)
        SURFACE='PSV is the CSV adapter third recorded delimiter (pipe), preserving exact record/field spans, original quoting, and ragged rows; the fixed-width adapter exposes the inferred per-column start/end positions and widths, the uniform record width, every record exact content span, and every field exact padded bytes'
        DECLINES='an out-of-range fixed-width record and an out-of-range cell decline typed (rc 6); a comma/tab table stays csv; a GFM pipe table stays markdown; variable-length prose and a single-space aligned blob stay Opaque (rc 6), never a panic'
        DETECTION='the pipe delimiter is tried after comma and tab and declined on a Markdown delimiter row; fixed-width requires >= 3 sampled records of identical byte width, >= 2 non-empty columns, and interior whitespace gaps >= 2 columns wide, and is declined on any source that also parses as a delimited or Markdown table'
        RC_CODES='0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit'
        ;;
    rst)
        SURFACE='section titles with their exact underline/overline adornment and a recorded hierarchy; paragraphs; explicit markup (comments, hyperlink targets, directives, footnotes, substitution definitions); field/option/definition lists; literal and doctest blocks; bullet/enumerated lists with nesting; inline markup; grid/simple tables'
        DECLINES='an out-of-range native title and an unsupported common pair (the common table) decline typed (rc 6); a Markdown document stays markdown and is never stolen by reST; plain prose stays Opaque (rc 6), never a panic'
        DETECTION='reST detection is byte-based and conservative: a section adornment whose char is not a Markdown signal, an explicit-markup construct, a grid/simple table, a field list, or a literal/doctest block; a Markdown document is never reclassified'
        RC_CODES='0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit'
        ;;
    asciidoc)
        SURFACE='a level-0 document title and = sections with their exact marker and recorded level; document attributes and literal references; delimited blocks preserved verbatim; unordered/ordered/description lists; |=== tables; admonitions; inline markup plus the link/image/include/xref/URL macros'
        DECLINES='an out-of-range native heading and an unsupported common pair (the common table) decline typed (rc 6); Markdown stays markdown and reStructuredText stays rst; plain prose stays Opaque (rc 6), never a panic'
        DETECTION='ASCIIDoc detection requires an AsciiDoc-specific structural mark (a document title, a section, a delimited block, a |=== table, an attribute list, or an admonition); a Markdown or reST document is never stolen (reST is tried first)'
        RC_CODES='0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit'
        ;;
    mdx)
        SURFACE='ESM import/export statement spans; JSX elements and fragments with attributes and nested children (recorded as extents, never executed); MDX expressions inline/block/comment (brace-balanced); and the full Markdown surface'
        DECLINES='an out-of-range native ESM/JSX/expression and an unsupported common pair (the common table) decline typed (rc 6); a Markdown document stays markdown (including when inline code or an inline expression looks like MDX); plain HTML stays html; plain prose stays Opaque (rc 6), never a panic'
        DETECTION='MDX detection requires an ESM/JSX/expression signal on top of Markdown block structure; inline code spans and inline expressions alone do not promote a Markdown document to MDX, and a plain HTML document stays html'
        RC_CODES='0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit'
        ;;
    mhtml)
        SURFACE='the MIME multipart/related envelope (Snapshot-Content-Location, Content-Base), the ordered parts and their exact spans, the ordered sub-resources keyed by Content-Location/Content-ID, and the root HTML part (the start= Content-ID, else the first text/html part, else the first part); quoted-printable/base64 parts are decoded for observations while the encoded bytes stay in the source'
        DECLINES='an out-of-range native resource declines typed (rc 6); an unsupported common pair (table) declines typed (rc 6); a malformed --mhtml-resource is a usage error (rc 2); plain prose stays opaque, a plain HTML document stays html, and a plain MIME message stays eml (rc 6), never a panic'
        DETECTION='MHTML requires a multipart/related MIME root plus an MHTML-SPECIFIC signal (a Snapshot-Content-Location/Content-Base, or a From+Subject envelope with a text/html part); a plain multipart/related email, a plain MIME message, and a plain HTML document are never stolen'
        RC_CODES='0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 43 invalid-mhtml-structure'
        ;;
    logstream)
        SURFACE='a generic timestamp/level/message dialect, RFC 5424 (HEADER + structured-data), and RFC 3164 BSD; record order, exact line spans, per-field roles and spans, line terminators (lf/crlf), blank-line accounting and the recorded record dialect'
        DECLINES='an out-of-range record and an unsupported common pair decline typed (rc 6); a malformed --logstream-field reference is a usage error (rc 2); plain prose stays opaque, a single JSON value stays json, a JSON stream stays jsonl, and a colon-less BSD stream stays yaml (rc 6), never a panic'
        DETECTION='log-stream detection is byte-based and conservative (>= 2 non-blank lines, EVERY non-blank line matching a dialect) and runs after the structured/tabular/prose families and before the fixed-width heuristic; a single log-looking line inside prose never claims the file'
        RC_CODES='0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 44 invalid-logstream-structure'
        ;;
    pkgmeta)
        SURFACE='npm package.json, package-lock.json, Cargo.toml, Cargo.lock and PEP 621 pyproject.toml: the recorded dialect, ordered sections with their roles, kinds, names and exact spans, the ordered entries, and inline-table/array dependency specs preserved'
        DECLINES='an out-of-range section/entry, an absent section/key, and an unsupported common pair decline typed (rc 6); a malformed --pkgmeta-key reference is a usage error (rc 2); a generic JSON/TOML document and plain prose decline typed (rc 6), never a panic'
        DETECTION='package-metadata detection is content-only (never a file name) and runs before the generic JSON and TOML detectors; it requires a strong manifest shape (npm name+version+third key, an npm lockfile, a Cargo [package]/Cargo.lock, or a pyproject [build-system]/[project]/[tool.poetry])'
        RC_CODES='0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 45 invalid-pkgmeta-structure'
        ;;
    apispec)
        SURFACE='a JSON Schema, OpenAPI 3.x, Swagger 2.0 and AsyncAPI document: the recorded dialect, the version string, ordered objects with their roles and member counts, exact object spans, and the ordered $ref targets'
        DECLINES='an out-of-range object/$ref and an unsupported common pair decline typed (rc 6); a generic JSON object, a JSON-Schema-shaped object with no $schema, and plain prose decline typed (rc 6), never a panic'
        DETECTION='API-spec detection is content-only (never a file name) and runs before the generic JSON detector; it requires a strong root-level string marker (a json-schema.org $schema URI, an openapi 3.x string, swagger "2.0", or a non-empty asyncapi string)'
        RC_CODES='0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 46 invalid-apispec-structure'
        ;;
    bib)
        SURFACE='BibTeX (@type{key, fields in braced/quoted/number/macro form, duplicate fields), RIS (TY..ER with multi-line continuations) and CSL-JSON (an id+type item array): the recorded dialect, ordered entries, their keys/ids and types, ordered field names and kinds, and the exact field-value tokens'
        DECLINES='an out-of-range entry, an absent field/key, and an unsupported common pair decline typed (rc 6); a plain JSON array of non-CSL objects and plain prose decline typed (rc 6), never a panic'
        DETECTION='bibliography detection is content-only (never a file name): a line-anchored @type{ entry sequence (BibTeX), an RIS TY..ER sequence, or a bounded CSL-JSON test run before the generic JSON detector'
        RC_CODES='0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 47 invalid-bib-structure'
        ;;
    rtf)
        SURFACE='the RTF header, the font/colour/stylesheet/info destination tables, control words with numeric parameters, group nesting, destination groups, unicode escapes and hex escapes (spelling preserved), escapes/symbols, and the exact document text'
        DECLINES='an out-of-range group/control/hex/unicode/text index and an unsupported common pair decline typed (rc 6); an unknown destination name and a non-canonical ordinal are usage errors (rc 2); a leading-brace junk blob, an RTF header with no version digit, an unbalanced brace and prose stay opaque (rc 6), never a panic'
        DETECTION='RTF detection is content-only and strong-magic based: the source must begin (after an optional UTF-8 BOM) with the header magic (brace, backslash, rtf, version digit) AND its group braces must balance under a bounded scan that respects control symbols, hex escapes, control words and raw-binary data, closing exactly at end of input'
        RC_CODES='0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 48 invalid-rtf-structure'
        ;;
    container)
        SURFACE='the ordered member list with raw names (including non-UTF-8), the ZIP compression method (stored/deflate), both declared sizes and CRC-32, exact member spans; the TAR typeflag/mode/linkname with GNU long-name (L) and PAX (x) extensions folded in; the recorded dialect (zip/tar)'
        DECLINES='an out-of-range member index declines typed (rc 6); a container selector on an office package or an Opaque control declines typed (rc 6); a truncated TAR detects as container and its model declines typed InvalidContainerStructure (rc 49); never a panic'
        DETECTION='ZIP detection requires a PK local/EOCD signature; TAR detection requires a ustar/PAX/GNU header with a valid checksum; a docx/odt office package still detects as its office format and a container selector on it declines typed'
        RC_CODES='0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 49 invalid-container-structure'
        ;;
    hdf)
        SURFACE='NetCDF classic (CDF-1/CDF-2) dimensions, global/variable attributes, and variables with their types, shapes and record flag; HDF5-subset superblock, groups and datasets with types, shapes and spans; the recorded dialect (netcdf/hdf5)'
        DECLINES='a filtered dataset and a v2 object header decline typed (rc 6); a malformed NetCDF header / bad tag and an out-of-range index decline typed (rc 50 / rc 6); an hdf selector on an Opaque control declines typed (rc 6); never a panic'
        DETECTION='content-only, never a file name: NetCDF = the 4-byte CDF magic at offset 0; HDF5 = the 8-byte signature at offset 0 OR 512 (user-block form), with a superblock version in 0..=3 that fits inside the source'
        RC_CODES='0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 50 invalid-hdf-structure'
        ;;
    cfbf)
        SURFACE='the 512-byte header (signature, version, byte order, sector/mini-sector shifts, FAT/DIFAT/directory/MiniFAT counts and first sectors); the DIFAT and FAT; the 128-byte directory entries with verbatim UTF-16LE names (including non-ASCII), object type, colour and the reconstructed sibling/child tree; and each stream resolved to an exact byte span; the recorded dialect cfb3/cfb4'
        DECLINES='a compound file with a plausible header but a malformed body (invalid directory object type, cyclic FAT chain, truncated stream chain) detects as cfbf and declines typed InvalidCfbfStructure (rc 51); a signature-only blob, a corrupted signature, a bad sector shift, an out-of-range directory sector and prose stay opaque (rc 6); never a panic'
        DETECTION='CFBF detection is content-only and strong-magic based: the 8-byte D0CF11E0A1B11AE1 signature at offset 0, a v3/v4 major version, a valid byte order and a plausible sector/mini-sector shift'
        RC_CODES='0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 51 invalid-cfbf-structure'
        ;;
    doc)
        SURFACE='the reused CFBF/OLE2 physical layer plus the Word 97–2003 (MS-DOC) semantic model: the FIB (wIdent 0xA5EC, nFib/version, the fWhichTblStm table selection, fcMin/fcMac, the ccp* section counts, fcClx/lcbClx), the piece table (each piece CP range, resolved fc, compressed flag, exact WordDocument-stream byte span, decoded text, and the exact bytes), the CP sections (main/footnote/header/annotation/endnote/textbox/hdrtextbox), and the document text'
        DECLINES='an out-of-range --doc-piece/--doc-section and a doc selector on a foreign format decline typed (rc 6); a doc with a bad wIdent/nFib detects as cfbf and a native doc selector on it declines typed (rc 6); a compound file whose body is structurally invalid but detects as doc declines typed InvalidDocStructure (rc 52); prose stays opaque (rc 6), never a panic'
        DETECTION='content-only, never a file name: the source must be a valid CFBF/OLE2 compound file with a WordDocument stream whose FIB carries wIdent 0xA5EC and a plausible Word 97 nFib; a generic compound file and a bad-ident/bad-nFib doc stay cfbf and are never stolen'
        RC_CODES='0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 52 invalid-doc-structure'
        ;;
    xls)
        SURFACE='the reused CFBF/OLE2 physical layer plus the Excel 97–2003 (BIFF8) semantic model: the BIFF header (version, Workbook/Book stream selection, root substream type, record count, stream span), the BOUNDSHEET inventory (name, BOF stream position, sheet type, visibility state, spans), the cells (kind, row/column, exact record span, exact stored value spelling), the sheet-ordered canonical text, and the ordered BIFF records'
        DECLINES='an out-of-range --xls-sheet/--xls-cell/--xls-record declines typed (rc 6); an xls with a bad BOF version or an unrecognized substream type detects as cfbf and a native xls selector on it declines typed (rc 6); a structurally invalid workbook that detects as xls declines typed InvalidXlsStructure (rc 53); prose stays opaque (rc 6), never a panic'
        DETECTION='content-only, never a file name: the source must be a CFBF/OLE2 compound file with a Workbook/Book stream whose first record is a BIFF8 BOF (version 0x0600, globals substream); a generic compound file and a bad-BOF/bad-subtype workbook stay cfbf and are never stolen'
        RC_CODES='0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 53 invalid-xls-structure'
        ;;
    ppt)
        SURFACE='the reused CFBF/OLE2 physical layer plus the PowerPoint 97–2003 (MS-PPT) semantic model: the Document header (record/slide/run counts, Current User presence, stream span), the ordered MS-PPT records, the slides (persistIdRef, cTexts, list, spans), the text runs (slide, kind, exact atom span, decoded text, exact bytes), and every SlideListWithText list (slides/masters/notes/handout)'
        DECLINES='an out-of-range --ppt-slide/--ppt-run/--ppt-record/--ppt-list and an out-of-range list name decline typed (rc 6); a first record that is not a Document container detects as cfbf and a native ppt selector on it declines typed (rc 6); a structurally invalid presentation that detects as ppt declines typed InvalidPptStructure (rc 54); prose stays opaque (rc 6), never a panic'
        DETECTION='content-only, never a file name: the source must be a CFBF/OLE2 compound file with a PowerPoint Document stream whose first record is the Document container (recType 1000); a generic compound file and a non-Document first record stay cfbf and are never stolen'
        RC_CODES='0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 54 invalid-ppt-structure'
        ;;
    msg)
        SURFACE='the reused CFBF/OLE2 physical layer plus the Outlook message (MS-OXMSG) semantic model: the __properties_version1.0 header and its fixed-size property entries (tag, id, property name, type, flags, value bytes, span, owner), the __substg1.0_* substreams, the one-level recipient and attachment sub-storages with their own property lists and substreams, and the canonical decoded text'
        DECLINES='an out-of-range --msg-property/--msg-recipient/--msg-attachment declines typed (rc 6); a compound file with no root __properties_version1.0 detects as cfbf and a native msg selector on it declines typed (rc 6); a structurally invalid message that detects as msg declines typed InvalidMsgStructure (rc 55); prose stays opaque (rc 6), never a panic'
        DETECTION='content-only, never a file name: the source must be a CFBF/OLE2 compound file whose root storage carries a __properties_version1.0 stream with a valid 32-byte message header; a generic compound file and a missing-properties file stay cfbf and are never stolen'
        RC_CODES='0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 55 invalid-msg-structure'
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
    "service": rc.get("environment", {}).get("service", "doc-baseline"),
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
