#!/usr/bin/env bash
# Phase 19.1 — repeatability court for the Phase-18.5 headline numbers.
#
# ## Questions
#
#   Q1. Is the VOLE/SQLite **build** ratio actually < 1.0?
#   Q2. Is the VOLE/SQLite **warm heterogeneous query** ratio actually > 1.0
#       (a real loss), or within parity?
#
# Phase 18.4/18.5 answered these with build **0.82x** and warm **1.09x** — but
# from a SINGLE court run reduced best-of-3 (min). The store lives on a
# bind-mounted host filesystem that stalls one-sided, so min reduction hides
# variance and there was no paired/interleaved design and no interval. This
# court re-measures the SAME thing with repetitions and an interval.
#
# ## What is held fixed (so the numbers stay comparable to Phase 18)
#
#   * the SAME 12-document subset (pdf/docx/epub, both revision families),
#   * the SAME contract depths C0..C5,
#   * the SAME source-retaining SQLite lane — every SQLite build / query / blob
#     materialization is delegated to tools/fixtures/phase18-contract-packed.py,
#   * the SAME accounting: persistent bytes = SUM OF REGULAR-FILE SIZES
#     (`find -printf %s`); `du -sb` is never used (the bind mount reports huge
#     phantom directory sizes and would compare VOLE's directory store against
#     SQLite's single db file unfairly).
#
# ## What this court ADDS
#
#   * REPS=10 (override with REPS=) repetitions of BOTH lanes at every depth,
#   * interleaved execution order: odd reps VOLE-then-SQLite, even reps
#     SQLite-then-VOLE, so both lanes meet the same cache / drift conditions,
#   * EVERY individual sample retained (raw/*_samples.tsv) — no min/median
#     reduction at collection time,
#   * a per-rep warm-up before each lane's timed queries (the store is rewritten
#     each rep, so its first read would otherwise pay a cold-inode penalty),
#   * aggregation with per (document, lane, depth) and per (lane, depth) stats,
#     per paired rep ratios, and a fixed-seed cluster bootstrap 95% CI.
#
# ## Lane
#
# Never the host: the digest-pinned, hard-capped `doc-baseline` service
# (mem_limit == memswap_limit == 6g, pids_limit 4096, cpus 8):
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase19-repeat-court.sh
#   docker compose run --rm --no-TTY doc-baseline sh -c 'REPS=10 IDS="nist-pdf-0017 nist-docx-0005" bash tools/phase19-repeat-court.sh'
set -uo pipefail

cd /work
export LC_ALL=C

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=evidence/campaigns/${STAMP}-phase19-repeat-${SHA}${TAG:+-$TAG}
RAW=$CAMPAIGN/raw
WORK=${WORK:-evidence/scratch/phase19-repeat}
rm -rf "$RAW"
mkdir -p "$RAW" "$RAW/sql" "$RAW/warm" "$WORK"

MANIFEST=${MANIFEST:-real100-v1/manifest.tsv}
CORPUS=${CORPUS:-real100-v1/documents}
# SQLite lane (build/query/materialize/sqlgen) — the UNCHANGED Phase-18 fixture.
BASE=tools/fixtures/phase18-contract-packed.py
# Phase-19 aggregator (delegates the SQLite lane to BASE, adds the statistics).
REPEAT=tools/fixtures/phase19-repeat.py
OP_TIMEOUT=${OP_TIMEOUT:-180}
REPS=${REPS:-10}
BOOT=${BOOT:-20000}
SEED=${SEED:-190019}
TIE=${TIE:-0.10}
DEPTHS=${DEPTHS:-0 1 2 3 4 5}
DEFAULT_IDS="nist-pdf-0002 nist-pdf-0004 nist-pdf-0016 nist-pdf-0017 nist-docx-0005 nist-docx-0008 nist-docx-0009 nist-docx-0014 nist-epub-0003 nist-epub-0006 nist-epub-0008 nist-epub-0009"
IDS=${IDS:-$DEFAULT_IDS}
LASTDEP=${DEPTHS##* }

PROFILE=${PROFILE:-release}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-${VOLE_BIN:-target/debug/vole-document}} ;;
esac

echo "== phase19.1 repeatability court (REPS=$REPS, interleaved) ==" >&2
echo "-- building ($PROFILE) binary" >&2
# shellcheck disable=SC2086
if ! cargo build $BUILD_ARGS >&2; then
    echo "phase19-repeat: build FAILED ($BUILD_ARGS); refusing to measure" >&2
    exit 1
fi
[ -x "$BIN" ] || { echo "phase19-repeat: $BIN missing; refusing to measure" >&2; exit 1; }

PREFLIGHT=""
for cand in real100-v1/documents/nist/docx/*.docx real100-v1/documents/nasa/epub/*.epub; do
    [ -f "$cand" ] && { PREFLIGHT="$cand"; break; }
done
if [ -n "$PREFLIGHT" ]; then
    expect=$(case "$PREFLIGHT" in *.docx) echo docx ;; *) echo epub ;; esac)
    if ! "$BIN" capabilities "$PREFLIGHT" 2>/dev/null | grep -q "\"adapter\":\"$expect\""; then
        echo "phase19-repeat: preflight failed — $BIN lacks $expect" >&2
        exit 1
    fi
fi

# microsecond wall clock (EPOCHREALTIME has 6 fractional digits).
now_us() { echo $(( ${EPOCHREALTIME/./} )); }
rss_kb() { awk -F': ' '/Maximum resident set size/{print $2}' "$1"; }
# Persistent bytes = sum of REGULAR-FILE sizes (identical accounting to Phase 18).
fbytes() { find "$@" -type f -printf '%s\n' 2>/dev/null | awk '{s+=$1} END{print s+0}'; }
ffiles() { find "$1" -type f 2>/dev/null | wc -l | tr -d ' '; }
fdirs()  { find "$1" -type d 2>/dev/null | wc -l | tr -d ' '; }

# --- subset selection + revision-family head map (identical to Phase 18) -----
python3 - "$MANIFEST" "$CORPUS" $IDS >"$RAW/subset.tsv" <<'PY'
import sys
manifest, corpus = sys.argv[1], sys.argv[2]
ids = sys.argv[3:]
EXT = {"pdf": "pdf", "docx": "docx", "epub": "epub"}
rows = []
with open(manifest) as f:
    hdr = f.readline().rstrip("\n").split("\t")
    for line in f:
        if not line.strip():
            continue
        r = dict(zip(hdr, line.rstrip("\n").split("\t")))
        rows.append(r)
by_id = {r["id"]: r for r in rows}
heads = {}
for r in rows:
    fam = r["revision_family_id"]
    if not fam:
        continue
    cur = heads.get(fam)
    key = (int(r["byte_len"]),)
    if cur is None or key > cur[0]:
        heads[fam] = (key, r["id"])
for i in ids:
    r = by_id.get(i)
    if not r:
        continue
    fmt = r["format"]
    path = "%s/%s/%s/%s.%s" % (corpus, r["agency"], fmt, i, EXT[fmt])
    fam = r["revision_family_id"]
    is_head = "1" if (not fam or heads.get(fam, (None, i))[1] == i) else "0"
    print("|".join([i, r["agency"], fmt, r["size_class"], r["byte_len"], r["sha256"],
                     path, r["revision_family_id"] or "", is_head]))
PY
echo "-- subset ($(wc -l <"$RAW/subset.tsv" | tr -d ' ') docs)" >&2
cat "$RAW/subset.tsv" >&2

# --- corpus verification (read-only) ---------------------------------------
echo "-- verifying the subset (SHA-256 + length)" >&2
verify_rc=0
while IFS='|' read -r id _a _f _s _bl sha path _fam _head; do
    [ -n "$id" ] || continue
    if [ ! -f "$path" ]; then echo "   missing $path" >&2; verify_rc=1; continue; fi
    got_sha=$(sha256sum "$path" | cut -d' ' -f1)
    got_len=$(stat -c %s "$path")
    if [ "$got_sha" != "$sha" ] || [ "$got_len" != "$_bl" ]; then
        echo "   verify FAIL $id" >&2; verify_rc=1
    fi
done <"$RAW/subset.tsv"
if [ "$verify_rc" -ne 0 ]; then
    echo "phase19-repeat: corpus verification FAILED; refusing to measure" >&2
    exit 1
fi

# --- contract envelope SQL, generated once (via the phase-19 proxy) ---------
python3 "$REPEAT" sqlgen --out "$RAW/sql" --depths "$DEPTHS" >"$RAW/sqlgen.json"

# --- observation sets (identical to Phase 18) -------------------------------
obs_for() {
    case "$1" in
        pdf)  echo "bytes text metadata revision" ;;
        *)    echo "bytes text doc-text heading table resource metadata revision" ;;
    esac
}
vole_args() {
    case "$2" in
        bytes)    echo "--byte-range 0..64 --kind exact" ;;
        text)     if [ "$1" = pdf ]; then echo "--page 1 --kind text"; else echo "--block 0 --kind text"; fi ;;
        doc-text) echo "--doc-text --kind text" ;;
        heading)  echo "--heading 0 --kind text" ;;
        table)    echo "--table 0 --kind text" ;;
        resource) echo "--resource 0 --kind metadata" ;;
        metadata) echo "--metadata --kind metadata" ;;
        revision) echo "--revisions --kind lineage" ;;
        *)        echo "DECLINE" ;;
    esac
}

# --- raw tables -------------------------------------------------------------
printf 'id\tfmt\tlane\tdepth\trep\trc\tus\n' >"$RAW/build_samples.tsv"
printf 'id\tfmt\tlane\tdepth\trep\tobs\trc\tus\n' >"$RAW/cold_samples.tsv"
printf 'id\tfmt\tlane\tdepth\trep\trc\tus\trss_kb\tn\n' >"$RAW/warm_samples.tsv"
printf 'id\tfmt\tv_ok\tv_rc\tv_us\tsql_ok\tsql_rc\tsql_us\n' >"$RAW/exact.tsv"
printf 'id\trep\torder\n' >"$RAW/order.tsv"
printf 'id\tfmt\tlane\tfiles\tdirs\tbytes\tfield\tlast_depth\n' >"$RAW/store_shape.tsv"

# --- main loop --------------------------------------------------------------
while IFS='|' read -r id agency fmt sclass blen sha path fam is_head; do
    [ -n "$id" ] || continue
    d="$WORK/$id"; rm -rf "$d"; mkdir -p "$d"
    echo "== $id ($fmt, $sclass, $blen B, family=${fam:-none}, head=$is_head) ==" >&2

    field=""
    req="$d/vole.reqs"; : >"$req"
    obslist=$(obs_for "$fmt")

    for rep in $(seq 1 "$REPS"); do
        # Interleaved order: odd reps VOLE first, even reps SQLite first.
        if [ $(( rep % 2 )) -eq 1 ]; then order="vole sqlite"; else order="sqlite vole"; fi
        printf '%s\t%s\t%s\n' "$id" "$rep" "$order" >>"$RAW/order.tsv"
        echo "   rep $rep/$REPS (order: $order)" >&2

        # ---- build both lanes in the interleaved order ----
        for lane in $order; do
            if [ "$lane" = vole ]; then
                rm -rf "$d/vstore"
                t0=$(now_us)
                timeout "$OP_TIMEOUT" "$BIN" field-build "$path" --store "$d/vstore" \
                    --profile runtime --packed >"$d/build.$rep.json" 2>"$d/build.$rep.err"; rc=$?
                t1=$(now_us)
                printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "vole" "all" "$rep" "$rc" \
                    "$(( t1 - t0 ))" >>"$RAW/build_samples.tsv"
                if [ "$rc" -eq 0 ]; then
                    f=$(jq -r '.ingest.field // empty' "$d/build.$rep.json" 2>/dev/null)
                    [ -n "$f" ] && field="$f"
                fi
            else
                for dep in $DEPTHS; do
                    rm -f "$d/a1c.$dep.db" "$d/a1c.$dep.db-wal" "$d/a1c.$dep.db-shm"
                    t0=$(now_us)
                    timeout "$OP_TIMEOUT" python3 "$BASE" build --format "$fmt" --source "$path" \
                        --db "$d/a1c.$dep.db" --through "$dep" --family "$fam" --member "$id" \
                        --is-head "$is_head" --metrics "$d/a1c.$dep.metrics.json" \
                        >"$d/a1c.$dep.build.$rep.json" 2>"$d/a1c.$dep.build.$rep.err"; rc=$?
                    t1=$(now_us)
                    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "sqlite" "$dep" "$rep" "$rc" \
                        "$(( t1 - t0 ))" >>"$RAW/build_samples.tsv"
                done
            fi
        done

        # build the VOLE request file once the field is known
        if [ -n "$field" ] && [ ! -s "$req" ]; then
            for obs in $obslist; do printf '%s\n' "$(vole_args "$fmt" "$obs")" >>"$req"; done
        fi

        # ---- query both lanes in the interleaved order ----
        for lane in $order; do
            if [ "$lane" = vole ]; then
                if [ -z "$field" ]; then
                    for dep in $DEPTHS; do
                        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "vole" "$dep" "$rep" "session" 3 0 >>"$RAW/cold_samples.tsv"
                        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "vole" "$dep" "$rep" 3 0 0 0 >>"$RAW/warm_samples.tsv"
                    done
                    continue
                fi
                # per-rep warm-up (untimed) so a freshly rewritten store is not timed cold
                for obs in $obslist; do
                    # shellcheck disable=SC2086
                    timeout "$OP_TIMEOUT" "$BIN" observe $(vole_args "$fmt" "$obs") \
                        --store "$d/vstore" --field "$field" --packed >/dev/null 2>&1 || true
                done
                timeout "$OP_TIMEOUT" "$BIN" observe-batch --store "$d/vstore" --packed \
                    --field "$field" --requests "$req" >/dev/null 2>&1 || true
                for dep in $DEPTHS; do
                    envdir="$RAW/env/C$dep"; mkdir -p "$envdir"
                    for obs in $obslist; do
                        args=$(vole_args "$fmt" "$obs")
                        # keep envelopes only on the final rep (equivalence evidence)
                        if [ "$rep" -eq "$REPS" ]; then out="$envdir/$id.$fmt.$obs.vole.json"; else out=/dev/null; fi
                        t0=$(now_us)
                        # shellcheck disable=SC2086
                        timeout "$OP_TIMEOUT" "$BIN" observe $args --store "$d/vstore" \
                            --field "$field" --packed >"$out" 2>"$d/vole.$obs.err"; rc=$?
                        t1=$(now_us)
                        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "vole" "$dep" "$rep" "$obs" "$rc" \
                            "$(( t1 - t0 ))" >>"$RAW/cold_samples.tsv"
                    done
                    wout="$RAW/warm/rep$rep.C$dep.$id.$fmt.vole.jsonl"
                    t0=$(now_us)
                    timeout "$OP_TIMEOUT" "$BIN" observe-batch --store "$d/vstore" --packed \
                        --field "$field" --requests "$req" >"$wout" 2>"$d/vole.C$dep.warm.$rep.err"; rc=$?
                    t1=$(now_us)
                    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "vole" "$dep" "$rep" "$rc" \
                        "$(( t1 - t0 ))" 0 "$(grep -c . "$req")" >>"$RAW/warm_samples.tsv"
                done
            else
                # SQLite per-rep warm-up (untimed)
                for dep in $DEPTHS; do
                    stmts=""
                    for obs in $obslist; do stmts="$stmts $(cat "$RAW/sql/C$dep/$fmt.$obs.sql");"; done
                    timeout "$OP_TIMEOUT" sqlite3 -batch "$d/a1c.$dep.db" "$stmts" >/dev/null 2>&1 || true
                done
                for dep in $DEPTHS; do
                    envdir="$RAW/env/C$dep"; mkdir -p "$envdir"
                    for obs in $obslist; do
                        sqlf="$RAW/sql/C$dep/$fmt.$obs.sql"
                        if [ "$rep" -eq "$REPS" ]; then out="$envdir/$id.$fmt.$obs.a1c.json"; else out=/dev/null; fi
                        t0=$(now_us)
                        timeout "$OP_TIMEOUT" sqlite3 -batch "$d/a1c.$dep.db" "$(cat "$sqlf")" \
                            >"$out" 2>"$d/a1c.$dep.$obs.q.err"; rc=$?
                        t1=$(now_us)
                        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "sqlite" "$dep" "$rep" "$obs" "$rc" \
                            "$(( t1 - t0 ))" >>"$RAW/cold_samples.tsv"
                    done
                    stmts=""
                    for obs in $obslist; do stmts="$stmts $(cat "$RAW/sql/C$dep/$fmt.$obs.sql");"; done
                    wout="$RAW/warm/rep$rep.C$dep.$id.$fmt.a1c.jsonl"
                    t0=$(now_us)
                    timeout "$OP_TIMEOUT" /usr/bin/time -v sqlite3 -batch "$d/a1c.$dep.db" "$stmts" \
                        2>"$d/a1c.C$dep.warm.$rep.time" >"$wout"; rc=$?
                    t1=$(now_us)
                    rss=$(rss_kb "$d/a1c.C$dep.warm.$rep.time")
                    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "sqlite" "$dep" "$rep" "$rc" \
                        "$(( t1 - t0 ))" "${rss:-0}" "$(printf '%s' "$obslist" | wc -w | tr -d ' ')" >>"$RAW/warm_samples.tsv"
                done
            fi
        done
    done

    # ---- exact original closure (both lanes), from the last rep's stores ----
    v_ok=0; v_rc=3; v_us=0
    if [ -n "$field" ]; then
        t0=$(now_us)
        timeout "$OP_TIMEOUT" "$BIN" materialize --store "$d/vstore" --field "$field" \
            --exact --output "$d/v.exact.bin" --packed >/dev/null 2>"$d/v.exact.err"; v_rc=$?
        t1=$(now_us); v_us=$(( t1 - t0 ))
        vs=$(sha256sum "$d/v.exact.bin" 2>/dev/null | cut -d' ' -f1)
        vl=$(stat -c %s "$d/v.exact.bin" 2>/dev/null || echo 0)
        [ "$vs" = "$sha" ] && [ "$vl" = "$blen" ] && v_ok=1
    fi
    sql_ok=0; sql_rc=3; sql_us=0
    t0=$(now_us)
    timeout "$OP_TIMEOUT" python3 "$BASE" materialize --db "$d/a1c.$LASTDEP.db" --out "$d/sql.exact.bin" \
        >/dev/null 2>"$d/sql.exact.err"; sql_rc=$?
    t1=$(now_us); sql_us=$(( t1 - t0 ))
    ss=$(sha256sum "$d/sql.exact.bin" 2>/dev/null | cut -d' ' -f1)
    sl=$(stat -c %s "$d/sql.exact.bin" 2>/dev/null || echo 0)
    [ "$ss" = "$sha" ] && [ "$sl" = "$blen" ] && sql_ok=1
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "$v_ok" "$v_rc" "$v_us" "$sql_ok" "$sql_rc" "$sql_us" >>"$RAW/exact.tsv"

    # record the store shape (regular-file bytes/files/dirs), same accounting
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "vole" \
        "$(ffiles "$d/vstore")" "$(fdirs "$d/vstore")" "$(fbytes "$d/vstore")" \
        "$(jq -r '.ingest.field // empty' "$d/build.$REPS.json" 2>/dev/null)" "$LASTDEP" \
        >>"$RAW/store_shape.tsv"

    rm -rf "$d"
done <"$RAW/subset.tsv"

# --- provenance -------------------------------------------------------------
RUSTC_V=$(rustc --version 2>/dev/null | sed 's/"/\\"/g')
CARGO_V=$(cargo --version 2>/dev/null | sed 's/"/\\"/g')
ARCH=$(uname -m)
COMMIT=$(git rev-parse HEAD 2>/dev/null || echo unknown)
DIRTY=$(git --no-optional-locks status --porcelain 2>/dev/null | tr '\n' ';' | sed 's/"/\\"/g')
LOCK_SHA=$(sha256sum Cargo.lock 2>/dev/null | cut -d' ' -f1)
MANIFEST_SHA=$(sha256sum "$MANIFEST" | cut -d' ' -f1)
BIN_SHA=$(sha256sum "$BIN" 2>/dev/null | cut -d' ' -f1)
SQLITE_V=$(sqlite3 --version 2>/dev/null | cut -d' ' -f1)
PY_V=$(python3 --version 2>&1)
BASE_STABLE=rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e
cat >"$RAW/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "19.1 — repeatability court for the Phase-18.5 build/warm headline numbers",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_commit": "$COMMIT",
  "git_dirty": "$DIRTY",
  "arch": "$ARCH",
  "service": "doc-baseline",
  "base_image": "$BASE_STABLE",
  "rustc": "$RUSTC_V",
  "cargo": "$CARGO_V",
  "sqlite3": "$SQLITE_V",
  "python": "$PY_V",
  "cargo_lock_sha256": "$LOCK_SHA",
  "manifest": "$MANIFEST",
  "manifest_sha256": "$MANIFEST_SHA",
  "bin": "$BIN",
  "bin_sha256": "$BIN_SHA",
  "profile": "$PROFILE",
  "subset": "$DEFAULT_IDS",
  "depths": "$DEPTHS",
  "op_timeout_s": $OP_TIMEOUT,
  "reps": $REPS,
  "bootstrap_resamples": $BOOT,
  "bootstrap_seed": $SEED,
  "tie_band": $TIE,
  "interleave": "odd reps VOLE-then-SQLite; even reps SQLite-then-VOLE (raw/order.tsv)",
  "timing": "microseconds via EPOCHREALTIME; a per-rep untimed warm-up precedes each lane's timed queries",
  "accounting": "persistent bytes = sum of regular-file sizes; du -sb never used",
  "sqlite_lane": "tools/fixtures/phase18-contract-packed.py (byte-identical to Phase 18)",
  "aggregator": "tools/fixtures/phase19-repeat.py aggregate",
  "env_affecting_semantics": {"LC_ALL": "C", "VOLE_BIN": "ignored under PROFILE=release"}
}
EOF
cp "$RAW/environment.json" "$CAMPAIGN/environment.json"

cat >"$CAMPAIGN/commands.txt" <<'EOF'
# Phase 19.1 — repeatability court: N=10 interleaved, paired repetitions of BOTH
# lanes at every contract depth, every sample retained. Never on the host.
docker compose run --rm --no-TTY doc-baseline bash tools/phase19-repeat-court.sh
# bounded / explicit population:
docker compose run --rm --no-TTY doc-baseline sh -c 'REPS=10 IDS="nist-pdf-0017 nist-docx-0005" bash tools/phase19-repeat-court.sh'
EOF

# --- aggregate + seal -------------------------------------------------------
echo "-- aggregating" >&2
python3 "$REPEAT" aggregate "$RAW" "$CAMPAIGN" --reps "$REPS" --boot "$BOOT" --seed "$SEED" --tie "$TIE" \
    | tee "$RAW/summary.txt"

cat >"$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "19.1 — repeatability court for the Phase-18.5 build/warm headline numbers",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "measured_commit": "$COMMIT",
  "tree_state": "$DIRTY",
  "profile": "$PROFILE",
  "bin": "$BIN",
  "bin_sha256": "$BIN_SHA",
  "base_image": "$BASE_STABLE",
  "service": "doc-baseline (mem_limit == memswap_limit == 6g, pids_limit 4096, cpus 8)",
  "rustc": "$RUSTC_V",
  "cargo": "$CARGO_V",
  "sqlite3": "$SQLITE_V",
  "cargo_lock_sha256": "$LOCK_SHA",
  "manifest_sha256": "$MANIFEST_SHA",
  "op_timeout_s": $OP_TIMEOUT,
  "reps": $REPS,
  "bootstrap_resamples": $BOOT,
  "bootstrap_seed": $SEED,
  "tie_band": $TIE,
  "depths": "$DEPTHS",
  "court": "tools/phase19-repeat-court.sh",
  "aggregator": "tools/fixtures/phase19-repeat.py (delegates the SQLite lane to the phase-18 fixture)",
  "sqlite_baseline": "tools/fixtures/phase18-contract-packed.py (build/query/materialize/sqlgen; byte-identical to Phase 18)",
  "design": "paired, interleaved repetitions at every depth; odd reps VOLE-then-SQLite, even reps SQLite-then-VOLE; every individual sample retained in raw/build_samples.tsv, raw/cold_samples.tsv, raw/warm_samples.tsv",
  "estimators": "per (document,lane,depth) and per (lane,depth) n/median/mean/p25/p75/min/max/CV; per paired rep VOLE/SQLite ratio; across documents median + geometric-mean paired ratio with a fixed-seed cluster bootstrap 95% CI over the 12 documents; per-document win/tie/loss under a +/-10% band; best-of-3 (min) vs median-of-$REPS ratio-of-sums for the same samples",
  "accounting": "persistent bytes = sum of regular-file sizes (find -printf %s) for BOTH lanes; du -sb is never used (bind-mount directory inflation)",
  "exactness": "VOLE materialize --exact --packed and the SQLite retained blob, both checked against the source length + SHA-256",
  "rc_codes": "0 ok; 3 decline; 6 unsupported-feature; 124 timeout; 137 SIGKILL/OOM"
}
EOF

chown -R 1000:1000 "$CAMPAIGN" "$WORK" 2>/dev/null || true

echo "phase19.1 repeatability court: done — $CAMPAIGN"
