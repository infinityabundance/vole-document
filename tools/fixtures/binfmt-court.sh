#!/usr/bin/env bash
# Phase 21.18 / 21.19 — the CBOR and MessagePack ECONOMIC courts (shared engine).
#
# One engine, parameterised by FORMAT (`cbor` | `msgpack`), so the two courts cannot
# drift apart. Pre-registered. Hypotheses:
#
#   H1 (byte-exactness / Q6) — for every corpus fixture (including the malformed and
#       Opaque controls), VOLE `materialize --exact == source` (length + SHA-256 +
#       `cmp`) after the source file AND the standalone descriptor are deleted, in a
#       fresh process. The court FAILS unless this is 100 %.
#   H2 (contract questions) — the three lanes (VOLE; a **source-retaining** SQLite
#       baseline that keeps the raw bytes and queries `json_extract`/`json_type`/
#       `json_tree` on a binary->strict-JSON normalization; a **conventional
#       binary->host-value load** via a vendored pure-Python decoder) answer the SAME
#       twelve questions Q1–Q12 (Q1–Q8 as in the JSON family, plus the binary-only
#       Q9 encoding width/format byte, Q10 byte-vs-text, Q11 tag/extension type, Q12
#       float width) where they can, and every question a lane cannot answer is a
#       TYPED decline, never a silent empty answer.
#   H3 (cross-lane agreement) — where VOLE and a comparator both answer, comparable
#       values agree (a scalar value+class, a node class, a duplicate-key count, a
#       lexical-find match set, a materialized length+SHA). VOLE answers the
#       representation questions (Q2/Q8/Q9/Q10/Q11/Q12) and the duplicate-key count
#       that the conventional load declines; the SQLite lane answers Q4 and Q6 too.
#       Those gaps are recorded, never papered over.
#   H4 (economics) — build/storage/cold/warm are measured per lane with a stated
#       estimator (paired per-fixture ratios, median + geometric mean, fixed-seed
#       cluster bootstrap by fixture, ratio-of-sums reported separately), every raw
#       sample retained. Wall times are microseconds (`us`).
#
# Honest scope: this is a self-authored deterministic corpus; only Q6 is a
# byte-authority claim. The conventional load is a binary->host-value load (it drops
# spans/width/signedness/byte-vs-text/duplicates/tag/ext/float-width), NOT a
# representation-preserving decoder: such a decoder could in principle match VOLE on
# several of those, and no claim is made against one. The strict-JSON SQLite lane
# cannot represent a non-text map key or `NaN`; that limitation is recorded, not
# hidden.
#
# Fairness / conventions: RELEASE build (`PROFILE=release`); VOLE substrate
# `field-build --profile runtime --packed`, `--packed` on every observe/batch/
# materialize. Runs in the pinned, hard-capped `doc-baseline` service (dev toolchain
# + python3 + sqlite3); never the host:
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-18-cbor-court.sh
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase21-19-msgpack-court.sh

set -uo pipefail
cd /work
export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1

FORMAT=${1:?usage: binfmt-court.sh cbor|msgpack}
case "$FORMAT" in
    cbor)    PHASE=21.18; PHASETAG=21-18; FMTNAME=CBOR;        GEN=tools/fixtures/make-cbor.py ;;
    msgpack) PHASE=21.19; PHASETAG=21-19; FMTNAME=MessagePack; GEN=tools/fixtures/make-msgpack.py ;;
    *) echo "unknown format: $FORMAT" >&2; exit 2 ;;
esac

BASE=tools/fixtures/binfmt-baseline.py
VOLE=tools/fixtures/binfmt-vole.py
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
                python3 "$VOLE" run --format "$FORMAT" --bin "$BIN" --store "$VDIR/store" --field "$FIELD" \
                    --plan "$PLAN" --outdir "$VDIR/run$rep" --source "$WORK/ref/$f" --packed >/dev/null
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
printf 'fixture\texpect_fmt\tfmt\texpect_rc\trc\tok\n' > "$RAW/controls.tsv"
CTL_OK=0; CTL_N=0
for c in $CONTROLS; do
    echo "=== control $c ==="
    expect_fmt=$(python3 -c "import json;print(json.load(open('$RAW/plan.json'))['controls']['$c'])")
    build_json=$("$BIN" field-build "$WORK/corpus/$c" --store "$WORK/ctlstore" --voldoc "$WORK/$c.voldoc" --profile runtime --packed 2>/dev/null)
    fmt=$(python3 -c "import json,sys;d=json.loads(sys.argv[1]);print(d.get('format') or d.get('ingest',{}).get('format',''))" "$build_json" 2>/dev/null || echo "")
    FIELD=$(python3 -c "import json,sys;d=json.loads(sys.argv[1]);print(d['ingest']['field'])" "$build_json" 2>/dev/null || echo "")
    if [ "$expect_fmt" = "opaque" ]; then
        # The common observation declines typed (rc 6), never a panic.
        expect_rc=6
        "$BIN" observe --store "$WORK/ctlstore" --field "$FIELD" --packed --metadata --kind metadata > /dev/null 2>&1
        rc=$?
        ok=false
        [ "$fmt" = "$expect_fmt" ] && [ "$rc" -eq 6 ] && ok=true
    else
        # A detected format: the FOREIGN native selector declines typed (rc 6), and
        # the OWN native selector (where one exists) succeeds.
        expect_rc=6
        "$BIN" observe --store "$WORK/ctlstore" --field "$FIELD" --packed --cbor-pointer "/a" --kind metadata > /dev/null 2>&1; cb_rc=$?
        "$BIN" observe --store "$WORK/ctlstore" --field "$FIELD" --packed --msgpack-pointer "/a" --kind metadata > /dev/null 2>&1; mp_rc=$?
        rc=99; own_ok=false
        case "$expect_fmt" in
            json)    rc=$mp_rc; [ "$cb_rc" -eq 6 ] && own_ok=true ;;
            cbor)    rc=$mp_rc; [ "$cb_rc" -eq 0 ] && own_ok=true ;;
            msgpack) rc=$cb_rc; [ "$mp_rc" -eq 0 ] && own_ok=true ;;
        esac
        ok=false
        [ "$fmt" = "$expect_fmt" ] && [ "$rc" -eq 6 ] && [ "$own_ok" = true ] && ok=true
    fi
    [ "$ok" = true ] && CTL_OK=$((CTL_OK + 1))
    CTL_N=$((CTL_N + 1))
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$c" "$expect_fmt" "$fmt" "$expect_rc" "$rc" "$ok" >> "$RAW/controls.tsv"
    [ "$ok" = true ] || echo "FINDING: control $c fmt=$fmt (want $expect_fmt) rc=$rc (want $expect_rc)" >&2

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
  "phase": "${PHASE} — ${FMTNAME} economic court (VOLE vs source-retaining strict-JSON SQLite + conventional ${FMTNAME} load)",
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
  "cbor_min_sha256": "$(sha256sum tools/fixtures/cbor_min.py | cut -d' ' -f1)",
  "msgpack_min_sha256": "$(sha256sum tools/fixtures/msgpack_min.py | cut -d' ' -f1)",
  "baseline_sha256": "$(sha256sum $BASE | cut -d' ' -f1)",
  "vole_probe_sha256": "$(sha256sum $VOLE | cut -d' ' -f1)",
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
docker compose run --rm --no-TTY doc-baseline bash tools/phase${PHASE}-${FORMAT}-court.sh
# PROFILE defaults to release (target/release/vole-document).
# Inside the court:
#   cargo build $BUILD_ARGS
#   python3 $GEN --corpus \$WORK/corpus
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

# --- receipt facts ----------------------------------------------------------
if [ "$FORMAT" = "cbor" ]; then
    SURFACE="every major type; the encoding width actually used (0x17 vs 0x1817) and signedness (uint vs negint); byte string vs text string as distinct kinds; tag numbers preserved (never resolved); map order and duplicate keys; float width (half/single/double); definite vs indefinite-length items"
    DECLINES="an out-of-range pointer and a missing member decline typed (rc 6); a malformed pointer is a usage error (rc 2); the strict-JSON control stays Json and a native CBOR selector on it declines typed; the lone-scalar, truncated, map-key-without-value, unterminated, trailing-byte, ambiguous fixarray(3)/fixmap(2), and prose controls stay Opaque and a common observation declines typed (rc 6), never a panic"
    DETECTION="CBOR has no magic bytes, so detection is conservative: the self-described-CBOR tag 55799 (0xd9 0xd9 0xf7), or a full-input well-formed parse whose root is a container/tag reaching at least three nodes; ambiguous or trivial inputs fall back to Opaque rather than being guessed"
    RC_CODES="0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 32 invalid-cbor-structure"
else
    SURFACE="the exact format byte actually used (encoding width AND signedness: 0x17 fixint vs 0xcc uint8 vs 0xd0 int8); str vs bin as distinct kinds; map order and duplicate keys; float width (float32/float64); extension type numbers + payload lengths preserved, never interpreted"
    DECLINES="an out-of-range pointer and a missing member decline typed (rc 6); a malformed pointer is a usage error (rc 2); the strict-JSON control stays Json and a native MessagePack selector on it declines typed; the CBOR control stays Cbor (coexistence) and a native MessagePack selector on it declines typed; the lone-scalar, 0xc1-byte, map-key-without-value, trailing-bytes, ambiguous fixarray(3)/fixmap(2), and prose controls stay Opaque and a common observation declines typed (rc 6), never a panic"
    DETECTION="MessagePack has no magic bytes, so detection is conservative: a full-input well-formed parse of exactly one item whose root is a container reaching at least three items and eight bytes, OR the same with an unambiguous MessagePack-only head byte (0xdc..=0xdf); CBOR's detector is tried first, so an input well-formed under both grammars is classified Cbor, never stolen"
    RC_CODES="0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 33 invalid-msgpack-structure"
fi

echo "=== aggregate ==="
python3 "$BASE" aggregate --format "$FORMAT" --raw "$RAW" --campaign "$CAMPAIGN" --env "$CAMPAIGN/environment.json"
verdict_rc=$?

cat > "$CAMPAIGN/receipt.json.tmp" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "${PHASE} — ${FMTNAME} economic court",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "measured_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "tree_state": "$(git status --porcelain | tr '\n' ' ' | sed 's/"/\\"/g')",
  "service": "doc-baseline",
  "profile": "$PROFILE",
  "bin": "$BIN",
  "bin_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "surface": "$SURFACE",
  "declines": "$DECLINES",
  "detection_boundary": "$DETECTION",
  "rc_codes": "$RC_CODES",
  "lane_fixtures": "$LANE_FIXTURES",
  "controls": "$CONTROLS",
  "observations": ["metadata", "text", "search-match", "native pointer (exact + metadata)"]
}
EOF

EXACT_OK=$(awk -F'\t' 'NR>1 && $2=="true"' "$RAW/exact.tsv" | wc -l | tr -d ' ')
EXACT_N=$(awk -F'\t' 'NR>1' "$RAW/exact.tsv" | wc -l | tr -d ' ')
echo
if [ "$verdict_rc" -eq 0 ] && [ "$EXACT_OK" -eq "$EXACT_N" ] && [ "$EXACT_N" -gt 0 ] && [ "$CTL_OK" -eq "$CTL_N" ]; then
    echo "PHASE ${PHASE} ${FMTNAME} COURT: PASS — VOLE exactness ${EXACT_OK}/${EXACT_N}, controls ${CTL_OK}/${CTL_N} — campaign $CAMPAIGN"
    python3 - "$CAMPAIGN" <<'PYEOF'
import json, sys
campaign = sys.argv[1]
rc = json.load(open(campaign + "/receipt.json"))
extra = json.load(open(campaign + "/receipt.json.tmp"))
rc.update(extra)
rc["verdict"] = "PASS"
json.dump(rc, open(campaign + "/receipt.json", "w"), indent=2, sort_keys=True)
PYEOF
    rm -f "$CAMPAIGN/receipt.json.tmp"
    exit 0
fi
echo "PHASE ${PHASE} ${FMTNAME} COURT: FAIL — VOLE exactness ${EXACT_OK}/${EXACT_N}, controls ${CTL_OK}/${CTL_N} — campaign $CAMPAIGN" >&2
python3 - "$CAMPAIGN" <<'PYEOF'
import json, sys
campaign = sys.argv[1]
rc = json.load(open(campaign + "/receipt.json"))
extra = json.load(open(campaign + "/receipt.json.tmp"))
rc.update(extra)
rc["verdict"] = "FAIL"
json.dump(rc, open(campaign + "/receipt.json", "w"), indent=2, sort_keys=True)
PYEOF
rm -f "$CAMPAIGN/receipt.json.tmp"
exit 1
