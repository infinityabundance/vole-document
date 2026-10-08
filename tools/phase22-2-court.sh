#!/usr/bin/env bash
# Phase 22.2 (review item P1) — warm one-session court against the TUNED `full`
# SQLite competitive envelope.
#
# ## Question
#
#   Phase 22.1 re-measured the warm one-session lane against the Pareto-tuned
#   SQLite envelope: a resolved **loss** (VOLE/SQLite 1.211x `full`, 95% CI
#   1.006-1.483). Phase 22.2 asks whether a compact query-native physical layout
#   (a selector directory / hot-cold split) can close it. The profiling gate
#   (`tools/phase22-2-profile.sh`) decides that FIRST; this court measures the
#   warm position before/after whatever (if anything) is shipped.
#
#   This is the frozen Phase-20.3 design (`tools/phase20-warm-court.sh`, UNCHANGED
#   except for the SQLite lane) with the SQLite lane swapped from the Phase-18
#   historical control to the Phase-22.1 tuned `full` configuration.
#
# The design is deliberately IDENTICAL to 20.3 so the numbers stay comparable:
#
#   * each lane's store is built ONCE per document (VOLE `field-build --profile
#     runtime --packed`; SQLite the tuned `full` envelope via
#     tools/fixtures/phase22-competitors.py, `--through 5`);
#   * then the warm one-session query lane is run WARM_REPS (default 100) times
#     per (document, depth, lane), interleaved (odd rep VOLE first, even rep
#     SQLite first) so cache / thermal / drift meets both lanes equally;
#   * every individual sample is retained (raw/warm_samples.tsv);
#   * one untimed warm-up session per (document, depth, lane) precedes the timed
#     reps; and one untimed equivalence pass retains both lanes' JSONL;
#   * the answer SQL is the frozen control's `sql_for` (a contract property, not a
#     tuning property), so the tuned lane changes only the physics, not the query.
#
# ## Lane
#
# Never the host: the digest-pinned, hard-capped `doc-baseline` service
# (mem_limit == memswap_limit == 6g, pids_limit 4096, cpus 8):
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase22-2-court.sh
#   docker compose run --rm --no-TTY doc-baseline sh -c 'CONFIG=full WARM_REPS=100 bash tools/phase22-2-court.sh'
#
# If `BIN` is set and executable, the cargo build is skipped (used to measure a
# pre-built binary). Otherwise the release binary is built with all features.
set -uo pipefail

cd /work
export LC_ALL=C
# Never leave root-owned bytecode caches in the tree for the host to clean up.
export PYTHONDONTWRITEBYTECODE=1

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=evidence/campaigns/${STAMP}-phase22-2-${SHA}${TAG:+-$TAG}
RAW=$CAMPAIGN/raw
WORK=${WORK:-evidence/scratch/phase22-2-court}
rm -rf "$RAW"
mkdir -p "$RAW" "$RAW/sql" "$RAW/env" "$WORK"

MANIFEST=${MANIFEST:-real100-v1/manifest.tsv}
CORPUS=${CORPUS:-real100-v1/documents}
# SQLite lane: the tuned Phase-22.1 competitive envelope (build / sqlgen / materialize).
COMP=tools/fixtures/phase22-competitors.py
CONFIG=${CONFIG:-full}
# Warm aggregator: the UNCHANGED Phase-20.3 estimator (reuses the phase-19.1 statistics).
WARM=tools/fixtures/phase20-warm.py
OP_TIMEOUT=${OP_TIMEOUT:-180}
WARM_REPS=${WARM_REPS:-100}
BOOT=${BOOT:-20000}
SEED=${SEED:-220200}
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

if [ -x "$BIN" ] && [ -n "${BIN_SKIP_BUILD:-}" ]; then
    echo "-- using prebuilt $BIN (BIN_SKIP_BUILD set)" >&2
else
echo "== phase22.2 warm court vs tuned full (WARM_REPS=$WARM_REPS, interleaved, stores built once) ==" >&2
echo "-- building ($PROFILE) binary" >&2
# shellcheck disable=SC2086
if ! cargo build $BUILD_ARGS >&2; then
    echo "phase22-2: build FAILED ($BUILD_ARGS); refusing to measure" >&2
    exit 1
fi
fi
[ -x "$BIN" ] || { echo "phase22-2: $BIN missing; refusing to measure" >&2; exit 1; }

PREFLIGHT=""
for cand in real100-v1/documents/nist/docx/*.docx real100-v1/documents/nasa/epub/*.epub; do
    [ -f "$cand" ] && { PREFLIGHT="$cand"; break; }
done
if [ -n "$PREFLIGHT" ]; then
    expect=$(case "$PREFLIGHT" in *.docx) echo docx ;; *) echo epub ;; esac)
    if ! "$BIN" capabilities "$PREFLIGHT" 2>/dev/null | grep -q "\"adapter\":\"$expect\""; then
        echo "phase22-2: preflight failed — $BIN lacks $expect" >&2
        exit 1
    fi
fi

# microsecond wall clock (EPOCHREALTIME has 6 fractional digits).
now_us() { echo $(( ${EPOCHREALTIME/./} )); }
# Persistent bytes = sum of REGULAR-FILE sizes (identical accounting to Phase 18/19.1).
fbytes() { find "$@" -type f -printf '%s\n' 2>/dev/null | awk '{s+=$1} END{print s+0}'; }
ffiles() { find "$1" -type f 2>/dev/null | wc -l | tr -d ' '; }
fdirs()  { find "$1" -type d 2>/dev/null | wc -l | tr -d ' '; }

# --- subset selection + revision-family head map (identical to Phase 18/19.1) --
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
    echo "phase22-2: corpus verification FAILED; refusing to measure" >&2
    exit 1
fi

# --- contract envelope SQL, generated once (the frozen control's sql_for) -----
python3 "$COMP" sqlgen --out "$RAW/sql" --depths "$DEPTHS" >"$RAW/sqlgen.json"

# --- observation sets (identical to Phase 18/19.1) --------------------------
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
printf 'id\tfmt\tlane\tdepth\trc\tus\n' >"$RAW/once.tsv"
printf 'id\tfmt\tlane\tdepth\trep\trc\tus\tn\n' >"$RAW/warm_samples.tsv"
printf 'id\tfmt\tv_ok\tv_rc\tv_us\tsql_ok\tsql_rc\tsql_us\n' >"$RAW/exact.tsv"
printf 'id\trep\torder\n' >"$RAW/order.tsv"
printf 'id\tfmt\tlane\tfiles\tdirs\tbytes\n' >"$RAW/store_shape.tsv"

# --- main loop --------------------------------------------------------------
while IFS='|' read -r id agency fmt sclass blen sha path fam is_head; do
    [ -n "$id" ] || continue
    d="$WORK/$id"; rm -rf "$d"; mkdir -p "$d"
    echo "== $id ($fmt, $sclass, $blen B, family=${fam:-none}, head=$is_head) ==" >&2

    field=""
    req="$d/vole.reqs"; : >"$req"
    obslist=$(obs_for "$fmt")
    nobs=$(printf '%s' "$obslist" | wc -w | tr -d ' ')

    # ---- ONE-TIME store builds (outside the warm loop) ----
    rm -rf "$d/vstore"
    t0=$(now_us)
    timeout "$OP_TIMEOUT" "$BIN" field-build "$path" --store "$d/vstore" \
        --profile runtime --packed >"$d/vole.build.json" 2>"$d/vole.build.err"; rc=$?
    t1=$(now_us)
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "vole" "all" "$rc" "$(( t1 - t0 ))" >>"$RAW/once.tsv"
    if [ "$rc" -eq 0 ]; then
        field=$(jq -r '.ingest.field // empty' "$d/vole.build.json" 2>/dev/null)
    fi
    echo "   vole field-build rc=$rc field=${field:-none}" >&2

    # SQLite lane: the tuned `full` competitive envelope, built ONCE (`--through $LASTDEP`).
    rm -f "$d/full.db" "$d/full.db-wal" "$d/full.db-shm"
    t0=$(now_us)
    timeout "$OP_TIMEOUT" python3 "$COMP" build --config "$CONFIG" --format "$fmt" --source "$path" \
        --db "$d/full.db" --through "$LASTDEP" --durability "${DURABILITY:-normal}" \
        --family "$fam" --member "$id" --is-head "$is_head" \
        --metrics "$d/full.metrics.json" \
        >"$d/full.build.json" 2>"$d/full.build.err"; rc=$?
    t1=$(now_us)
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "sqlite" "$LASTDEP" "$rc" "$(( t1 - t0 ))" >>"$RAW/once.tsv"
    echo "   sqlite ($CONFIG) build rc=$rc (through C$LASTDEP)" >&2
    # persistent bytes are measured HERE, right after the store is built and before
    # any observation writes a disposable derived-cache entry (ADR-0049: regular-file
    # sizes, never du -sb).
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "sqlite" \
        "$(find "$d" -maxdepth 1 -name 'full.db*' -type f 2>/dev/null | wc -l | tr -d ' ')" "0" \
        "$(find "$d" -maxdepth 1 -name 'full.db*' -type f -printf '%s\n' 2>/dev/null | awk '{s+=$1} END{print s+0}')" \
        >>"$RAW/store_shape.tsv"
    # expose the single tuned store under the per-depth name the warm loop reads
    for dep in $DEPTHS; do ln -sf "full.db" "$d/a1c.$dep.db" 2>/dev/null || cp "$d/full.db" "$d/a1c.$dep.db"; done
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "vole" \
        "$(ffiles "$d/vstore")" "$(fdirs "$d/vstore")" "$(fbytes "$d/vstore")" \
        >>"$RAW/store_shape.tsv"

    # request file for the VOLE batch session
    if [ -n "$field" ]; then
        for obs in $obslist; do printf '%s\n' "$(vole_args "$fmt" "$obs")" >>"$req"; done
    fi

    sql_stmts() { # $1 = depth -> the concatenated SQL for the whole schedule
        local dep="$1" stmts="" obs
        for obs in $obslist; do stmts="$stmts $(cat "$RAW/sql/C$dep/$fmt.$obs.sql");"; done
        printf '%s' "$stmts"
    }

    # ---- untimed warm-up (one session per lane per depth) ----
    for dep in $DEPTHS; do
        if [ -n "$field" ]; then
            timeout "$OP_TIMEOUT" "$BIN" observe-batch --store "$d/vstore" --packed \
                --field "$field" --requests "$req" >/dev/null 2>&1 || true
        fi
        timeout "$OP_TIMEOUT" sqlite3 -batch "$d/a1c.$dep.db" "$(sql_stmts "$dep")" \
            >/dev/null 2>&1 || true
    done

    # ---- timed warm reps (interleaved order) ----
    for rep in $(seq 1 "$WARM_REPS"); do
        if [ $(( rep % 2 )) -eq 1 ]; then order="vole sqlite"; else order="sqlite vole"; fi
        printf '%s\t%s\t%s\n' "$id" "$rep" "$order" >>"$RAW/order.tsv"
        if [ $(( rep % 10 )) -eq 1 ]; then echo "   rep $rep/$WARM_REPS (order: $order)" >&2; fi

        for lane in $order; do
            if [ "$lane" = vole ]; then
                if [ -z "$field" ]; then
                    for dep in $DEPTHS; do
                        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "vole" "$dep" "$rep" 3 0 "$nobs" >>"$RAW/warm_samples.tsv"
                    done
                    continue
                fi
                for dep in $DEPTHS; do
                    t0=$(now_us)
                    timeout "$OP_TIMEOUT" "$BIN" observe-batch --store "$d/vstore" --packed \
                        --field "$field" --requests "$req" >/dev/null 2>"$d/vole.C$dep.warm.$rep.err"; rc=$?
                    t1=$(now_us)
                    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "vole" "$dep" "$rep" "$rc" \
                        "$(( t1 - t0 ))" "$nobs" >>"$RAW/warm_samples.tsv"
                done
            else
                for dep in $DEPTHS; do
                    stmts=$(sql_stmts "$dep")
                    t0=$(now_us)
                    timeout "$OP_TIMEOUT" sqlite3 -batch "$d/a1c.$dep.db" "$stmts" \
                        >/dev/null 2>"$d/a1c.C$dep.warm.$rep.err"; rc=$?
                    t1=$(now_us)
                    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "sqlite" "$dep" "$rep" "$rc" \
                        "$(( t1 - t0 ))" "$nobs" >>"$RAW/warm_samples.tsv"
                done
            fi
        done
    done

    # ---- untimed equivalence pass (retain both lanes' warm-session JSONL) ----
    for dep in $DEPTHS; do
        envdir="$RAW/env/C$dep"; mkdir -p "$envdir"
        if [ -n "$field" ]; then
            timeout "$OP_TIMEOUT" "$BIN" observe-batch --store "$d/vstore" --packed \
                --field "$field" --requests "$req" \
                >"$envdir/$id.$fmt.vole.jsonl" 2>"$d/equiv.C$dep.vole.err" || true
        else
            : >"$envdir/$id.$fmt.vole.jsonl"
        fi
        timeout "$OP_TIMEOUT" sqlite3 -batch "$d/a1c.$dep.db" "$(sql_stmts "$dep")" \
            >"$envdir/$id.$fmt.a1c.jsonl" 2>"$d/equiv.C$dep.a1c.err" || true
    done

    # ---- exact original closure (both lanes, from the one-time stores) ----
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
    timeout "$OP_TIMEOUT" python3 "$COMP" materialize --db "$d/full.db" --out "$d/sql.exact.bin" \
        >/dev/null 2>"$d/sql.exact.err"; sql_rc=$?
    t1=$(now_us); sql_us=$(( t1 - t0 ))
    ss=$(sha256sum "$d/sql.exact.bin" 2>/dev/null | cut -d' ' -f1)
    sl=$(stat -c %s "$d/sql.exact.bin" 2>/dev/null || echo 0)
    [ "$ss" = "$sha" ] && [ "$sl" = "$blen" ] && sql_ok=1
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "$v_ok" "$v_rc" "$v_us" "$sql_ok" "$sql_rc" "$sql_us" >>"$RAW/exact.tsv"

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
  "phase": "22.2 — warm one-session court vs the TUNED full SQLite envelope (stores built once, warm query repeated N times)",
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
  "warm_reps": $WARM_REPS,
  "bootstrap_resamples": $BOOT,
  "bootstrap_seed": $SEED,
  "tie_band": $TIE,
  "interleave": "odd reps VOLE-then-SQLite; even reps SQLite-then-VOLE (raw/order.tsv)",
  "store_builds": "ONCE per document per lane (VOLE field-build --packed; SQLite the tuned full envelope via tools/fixtures/phase22-competitors.py, --through 5), outside the warm loop",
  "warm_up": "one untimed session per (document, depth, lane) precedes the timed reps",
  "timing": "microseconds via EPOCHREALTIME; the measured warm lane is one session (observe-batch / one sqlite3 process) serving the whole schedule",
  "accounting": "persistent bytes = sum of regular-file sizes; du -sb never used",
  "sqlite_lane": "tools/fixtures/phase22-competitors.py config=full (the Phase-22.1 tuned envelope); answer SQL = the frozen control sql_for so only the physics change",
  "aggregator": "tools/fixtures/phase20-warm.py aggregate (reuses the phase-19.1 estimators)",
  "predecessor": "${PREDECESSOR:-}",
  "env_affecting_semantics": {"LC_ALL": "C", "VOLE_BIN": "ignored under PROFILE=release"}
}
EOF
cp "$RAW/environment.json" "$CAMPAIGN/environment.json"

cat >"$CAMPAIGN/commands.txt" <<'EOF'
# Phase 22.2 — warm one-session court against the TUNED `full` SQLite envelope:
# each lane's store is built ONCE, then the warm one-session query lane is repeated
# WARM_REPS (default 100) times per (document, depth, lane), interleaved, and every
# sample is retained. Never on the host.
docker compose run --rm --no-TTY doc-baseline bash tools/phase22-2-court.sh
# bounded / explicit population:
docker compose run --rm --no-TTY doc-baseline sh -c 'WARM_REPS=100 IDS="nist-pdf-0017 nist-docx-0005" bash tools/phase22-2-court.sh'
EOF

# --- aggregate + seal -------------------------------------------------------
echo "-- aggregating" >&2
PRE_ARGS=""
if [ -n "${PREDECESSOR:-}" ]; then PRE_ARGS="--predecessor $PREDECESSOR"; fi
# shellcheck disable=SC2086
python3 "$WARM" aggregate "$RAW" "$CAMPAIGN" --reps "$WARM_REPS" --boot "$BOOT" --seed "$SEED" --tie "$TIE" $PRE_ARGS \
    | tee "$RAW/summary.txt"

cat >"$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "22.2 — warm one-session court vs the TUNED full SQLite envelope (stores built once, warm query repeated N times)",
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
  "warm_reps": $WARM_REPS,
  "bootstrap_resamples": $BOOT,
  "bootstrap_seed": $SEED,
  "tie_band": $TIE,
  "depths": "$DEPTHS",
  "court": "tools/phase22-2-court.sh",
  "aggregator": "tools/fixtures/phase20-warm.py (reuses the phase-19.1 statistics estimators)",
  "predecessor_campaign": "${PREDECESSOR:-}",
  "sqlite_baseline": "tools/fixtures/phase22-competitors.py config=full (the Phase-22.1 tuned envelope); answer SQL = the frozen control sql_for",
  "design": "stores built ONCE per document per lane; warm one-session query lane repeated WARM_REPS times per (document, depth, lane); interleaved odd/even rep order; one untimed warm-up session per (document, depth, lane); every sample retained in raw/warm_samples.tsv",
  "estimators": "per (document,lane,depth) and per (lane,depth) n/median/mean/p25/p75/min/max/CV; per paired rep VOLE/SQLite ratio; across documents median + geometric-mean paired ratio (depth-summed per rep) with a fixed-seed cluster bootstrap 95% CI over the 12 documents; per-document win/tie/loss under a +/-10% band; per-depth breakdown C0..C5",
  "accounting": "persistent bytes = sum of regular-file sizes (find -printf %s) for BOTH lanes; du -sb is never used (bind-mount directory inflation)",
  "exactness": "VOLE materialize --exact --packed and the SQLite retained blob, both checked against the source length + SHA-256; plus a per-observation warm-session envelope comparison via the frozen Phase-18 _equiv logic",
  "rc_codes": "0 ok; 3 decline; 6 unsupported-feature; 124 timeout; 137 SIGKILL/OOM"
}
EOF

chown -R 1000:1000 "$CAMPAIGN" "$WORK" 2>/dev/null || true

echo "phase22.2 warm court: done — $CAMPAIGN"
