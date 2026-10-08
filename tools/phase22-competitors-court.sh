#!/usr/bin/env bash
# Phase 22.1 — the equal-contract court against the SQLite COMPETITIVE ENVELOPE.
#
# ## Question
#
# The Phase-18/19 courts measured VOLE against ONE source-retaining SQLite
# baseline (`tools/fixtures/phase18-contract-packed.py`): `journal_mode=WAL`,
# `synchronous=NORMAL`, and TWO full FTS5 indexes — one of which (trigram) the
# contract never exercises. Its ~0.96x build ratio and ~0.9x byte ratio are
# legitimate AGAINST THAT BASELINE, but not evidence against the Pareto-optimal
# SQLite an expert would build. This court strengthens the opponent first.
#
# ## What is held fixed (so numbers stay comparable)
#
#   * the SAME 12-document subset (pdf/docx/epub, both revision families),
#   * the SAME contract depths C0..C5,
#   * the SAME answer SQL — the observation queries are delegated byte for byte
#     to the frozen control's `sql_for`, because the query is a property of the
#     CONTRACT, not of the physical tuning,
#   * the SAME accounting: persistent bytes = SUM OF REGULAR-FILE SIZES
#     (`find -printf %s`); `du -sb` is never used (ADR-0049),
#   * paired, interleaved repetitions (ADR-0054) with a per-rep warm-up.
#
# ## What this court ADDS
#
#   * the SQLite competitive envelope: `minimal`, `fts`, `structural`, `full`,
#     `adaptive`, `hybrid` (see tools/fixtures/phase22-competitors.py), each with
#     explicit PRAGMAs, transactions, batching and durability,
#   * the UNMODIFIED Phase-18 baseline as the historical control lane `hist`,
#   * a build decomposition (shared extraction vs per-config SQLite write),
#   * peak RSS for the SQLite lanes,
#   * a Pareto frontier (build / persistent bytes / cold / warm / coverage),
#   * a matched-durability headline (NORMAL) plus a measured `synchronous=FULL`
#     probe.
#
# ## Lane
#
# Never the host: the digest-pinned, hard-capped `doc-baseline` service
# (mem_limit == memswap_limit == 6g, pids_limit 4096, cpus 8):
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase22-competitors-court.sh
#   docker compose run --rm --no-TTY doc-baseline \
#     sh -c 'REPS=3 CONFIGS="full hybrid" bash tools/phase22-competitors-court.sh'
set -uo pipefail

cd /work
export LC_ALL=C

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=evidence/campaigns/${STAMP}-phase22-competitors-${SHA}${TAG:+-$TAG}
RAW=$CAMPAIGN/raw
WORK=${WORK:-evidence/scratch/phase22-competitors}
rm -rf "$RAW"
mkdir -p "$RAW" "$RAW/sql" "$RAW/warm" "$RAW/env" "$WORK"

MANIFEST=${MANIFEST:-real100-v1/manifest.tsv}
CORPUS=${CORPUS:-real100-v1/documents}
COMP=tools/fixtures/phase22-competitors.py
CONTROL=tools/fixtures/phase18-contract-packed.py
OP_TIMEOUT=${OP_TIMEOUT:-180}
REPS=${REPS:-3}
BOOT=${BOOT:-20000}
SEED=${SEED:-220019}
TIE=${TIE:-0.10}
DEPTHS=${DEPTHS:-0 1 2 3 4 5}
CONFIGS=${CONFIGS:-minimal fts structural full adaptive hybrid}
HISTDEPTHS=${HISTDEPTHS:-0 5}
DURABILITY=${DURABILITY:-normal}
DEFAULT_IDS="nist-pdf-0002 nist-pdf-0004 nist-pdf-0016 nist-pdf-0017 nist-docx-0005 nist-docx-0008 nist-docx-0009 nist-docx-0014 nist-epub-0003 nist-epub-0006 nist-epub-0008 nist-epub-0009"
IDS=${IDS:-$DEFAULT_IDS}

PROFILE=${PROFILE:-release}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-${VOLE_BIN:-target/debug/vole-document}} ;;
esac

echo "== phase22.1 competitor-envelope court (REPS=$REPS, configs=$CONFIGS) ==" >&2
echo "-- building ($PROFILE) binary" >&2
# shellcheck disable=SC2086
if ! CARGO_PROFILE_DEV_DEBUG=0 cargo build $BUILD_ARGS >&2; then
    echo "phase22: build FAILED ($BUILD_ARGS); refusing to measure" >&2
    exit 1
fi
[ -x "$BIN" ] || { echo "phase22: $BIN missing; refusing to measure" >&2; exit 1; }

PREFLIGHT=""
for cand in real100-v1/documents/nist/docx/*.docx real100-v1/documents/nasa/epub/*.epub; do
    [ -f "$cand" ] && { PREFLIGHT="$cand"; break; }
done
if [ -n "$PREFLIGHT" ]; then
    expect=$(case "$PREFLIGHT" in *.docx) echo docx ;; *) echo epub ;; esac)
    if ! "$BIN" capabilities "$PREFLIGHT" 2>/dev/null | grep -q "\"adapter\":\"$expect\""; then
        echo "phase22: preflight failed — $BIN lacks $expect" >&2
        exit 1
    fi
fi

now_us() { echo $(( ${EPOCHREALTIME/./} )); }
rss_kb() { awk -F': ' '/Maximum resident set size/{print $2}' "$1"; }
fbytes() { find "$@" -type f -printf '%s\n' 2>/dev/null | awk '{s+=$1} END{print s+0}'; }
ffiles() { find "$1" -type f 2>/dev/null | wc -l | tr -d ' '; }

# Capability ceiling per configuration (mirrors phase22-competitors.py CAP).
cap_depths() {
    case "$1" in
        minimal|fts) echo "0" ;;
        structural)  echo "0 1" ;;
        *)           echo "$DEPTHS" ;;
    esac
}

# --- subset selection + revision-family head map (identical to Phase 18/19) ---
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
    echo "phase22: corpus verification FAILED; refusing to measure" >&2
    exit 1
fi

# --- contract envelope SQL, generated once (frozen control's sql_for) --------
python3 "$COMP" sqlgen --out "$RAW/sql" --depths "$DEPTHS" >"$RAW/sqlgen.json"

# --- observation sets (identical to Phase 18/19) ----------------------------
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
printf 'id\tfmt\tlane\tdepth\tbytes\tfiles\n' >"$RAW/bytes.tsv"
printf 'id\tfmt\tlane\tok\trc\tus\n' >"$RAW/exact.tsv"
printf 'id\trep\torder\n' >"$RAW/order.tsv"
printf 'id\tconfig\tdepth\tdurability\tdb_us\tbytes\n' >"$RAW/durability.tsv"

ALL_LANES="vole $CONFIGS hist"

# --- main loop --------------------------------------------------------------
while IFS='|' read -r id agency fmt sclass blen sha path fam is_head; do
    [ -n "$id" ] || continue
    d="$WORK/$id"; rm -rf "$d"; mkdir -p "$d"
    echo "== $id ($fmt, $sclass, $blen B, family=${fam:-none}, head=$is_head) ==" >&2

    field=""
    req="$d/vole.reqs"; : >"$req"
    obslist=$(obs_for "$fmt")

    for rep in $(seq 1 "$REPS"); do
        if [ $(( rep % 2 )) -eq 1 ]; then order="vole sqlite"; else order="sqlite vole"; fi
        printf '%s\t%s\t%s\n' "$id" "$rep" "$order" >>"$RAW/order.tsv"
        echo "   rep $rep/$REPS (order: $order)" >&2

        # ---- shared extraction (measured ONCE per doc-rep) ----
        rm -f "$d/extract.json"
        t0=$(now_us)
        timeout "$OP_TIMEOUT" python3 "$COMP" extract --format "$fmt" --source "$path" \
            --out "$d/extract.json" >/dev/null 2>"$d/extract.$rep.err"; rc=$?
        t1=$(now_us)
        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "extract" "0" "$rep" "$rc" \
            "$(( t1 - t0 ))" >>"$RAW/build_samples.tsv"

        # ---- build both lane groups in the interleaved order ----
        for grp in $order; do
            if [ "$grp" = vole ]; then
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
                for c in $CONFIGS; do
                    for dep in $(cap_depths "$c"); do
                        sd="$d/$c.C$dep"; rm -rf "$sd"; mkdir -p "$sd"
                        t0=$(now_us)
                        timeout "$OP_TIMEOUT" python3 "$COMP" build --config "$c" \
                            --format "$fmt" --source "$path" --db "$sd/store.db" \
                            --through "$dep" --durability "$DURABILITY" \
                            --family "$fam" --member "$id" --is-head "$is_head" \
                            --metrics "$sd/m.json" \
                            >/dev/null 2>"$d/$c.C$dep.build.$rep.err"; rc=$?
                        t1=$(now_us)
                        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "$c" "$dep" "$rep" "$rc" \
                            "$(( t1 - t0 ))" >>"$RAW/build_samples.tsv"
                        printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "$c" "$dep" \
                            "$(fbytes "$sd")" "$(ffiles "$sd")" >>"$RAW/bytes.tsv"
                    done
                done
                for dep in $HISTDEPTHS; do
                    sd="$d/hist.C$dep"; rm -rf "$sd"; mkdir -p "$sd"
                    t0=$(now_us)
                    timeout "$OP_TIMEOUT" python3 "$CONTROL" build --format "$fmt" --source "$path" \
                        --db "$sd/store.db" --through "$dep" --family "$fam" --member "$id" \
                        --is-head "$is_head" --metrics "$sd/metrics.json" \
                        >/dev/null 2>"$d/hist.C$dep.build.$rep.err"; rc=$?
                    t1=$(now_us)
                    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "hist" "$dep" "$rep" "$rc" \
                        "$(( t1 - t0 ))" >>"$RAW/build_samples.tsv"
                    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "hist" "$dep" \
                        "$(fbytes "$sd")" "$(ffiles "$sd")" >>"$RAW/bytes.tsv"
                done
            fi
        done

        # record VOLE store shape under every depth so per-depth byte ratios exist
        vb=$(fbytes "$d/vstore"); vf=$(ffiles "$d/vstore")
        for dep in $DEPTHS; do
            printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "vole" "$dep" "$vb" "$vf" >>"$RAW/bytes.tsv"
        done

        if [ -n "$field" ] && [ ! -s "$req" ]; then
            for obs in $obslist; do printf '%s\n' "$(vole_args "$fmt" "$obs")" >>"$req"; done
        fi

        # ---- query both lane groups in the interleaved order ----
        for grp in $order; do
            if [ "$grp" = vole ]; then
                if [ -z "$field" ]; then
                    for dep in $DEPTHS; do
                        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "vole" "$dep" "$rep" "session" 3 0 >>"$RAW/cold_samples.tsv"
                        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "vole" "$dep" "$rep" 3 0 0 0 >>"$RAW/warm_samples.tsv"
                    done
                    continue
                fi
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
                        if [ "$rep" -eq "$REPS" ]; then out="$envdir/$id.$fmt.$obs.vole.json"; else out=/dev/null; fi
                        t0=$(now_us)
                        # shellcheck disable=SC2086
                        timeout "$OP_TIMEOUT" "$BIN" observe $args --store "$d/vstore" \
                            --field "$field" --packed >"$out" 2>"$d/vole.$obs.err"; rc=$?
                        t1=$(now_us)
                        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "vole" "$dep" "$rep" "$obs" "$rc" \
                            "$(( t1 - t0 ))" >>"$RAW/cold_samples.tsv"
                    done
                    t0=$(now_us)
                    timeout "$OP_TIMEOUT" /usr/bin/time -v "$BIN" observe-batch --store "$d/vstore" --packed \
                        --field "$field" --requests "$req" 2>"$d/vole.C$dep.warm.$rep.time" \
                        >"$RAW/warm/rep$rep.C$dep.$id.$fmt.vole.jsonl"; rc=$?
                    t1=$(now_us)
                    rss=$(rss_kb "$d/vole.C$dep.warm.$rep.time")
                    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "vole" "$dep" "$rep" "$rc" \
                        "$(( t1 - t0 ))" "${rss:-0}" "$(printf '%s' "$obslist" | wc -w | tr -d ' ')" >>"$RAW/warm_samples.tsv"
                done
            else
                for lane in $CONFIGS hist; do
                    deps=""
                    for dep in $(cap_depths "$lane"); do
                        if [ "$lane" = hist ]; then
                            case " $HISTDEPTHS " in *" $dep "*) : ;; *) continue ;; esac
                        fi
                        [ -f "$d/$lane.C$dep/store.db" ] || continue
                        deps="$deps $dep"
                    done
                    [ -n "$deps" ] || continue
                    # adaptive: materialize on demand FIRST time a depth demands it;
                    # the whole cost is charged to that depth's cold total.
                    if [ "$lane" = adaptive ]; then
                        for dep in $deps; do
                            t0=$(now_us)
                            timeout "$OP_TIMEOUT" python3 "$COMP" ensure --config adaptive \
                                --db "$d/$lane.C$dep/store.db" --format "$fmt" --source "$path" \
                                --depth "$dep" --family "$fam" --member "$id" --is-head "$is_head" \
                                >/dev/null 2>"$d/$lane.C$dep.ensure.$rep.err"; rc=$?
                            t1=$(now_us)
                            printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "$lane" "$dep" "$rep" "ensure" "$rc" \
                                "$(( t1 - t0 ))" >>"$RAW/cold_samples.tsv"
                        done
                    fi
                    # untimed warm-up so a freshly written store is not timed cold
                    for dep in $deps; do
                        db="$d/$lane.C$dep/store.db"
                        stmts=""
                        for obs in $obslist; do stmts="$stmts $(cat "$RAW/sql/C$dep/$fmt.$obs.sql");"; done
                        timeout "$OP_TIMEOUT" sqlite3 -batch "$db" "$stmts" >/dev/null 2>&1 || true
                    done
                    for dep in $deps; do
                        db="$d/$lane.C$dep/store.db"
                        envdir="$RAW/env/C$dep"; mkdir -p "$envdir"
                        for obs in $obslist; do
                            sqlf="$RAW/sql/C$dep/$fmt.$obs.sql"
                            if [ "$rep" -eq "$REPS" ]; then out="$envdir/$id.$fmt.$obs.$lane.json"; else out=/dev/null; fi
                            t0=$(now_us)
                            timeout "$OP_TIMEOUT" sqlite3 -batch "$db" "$(cat "$sqlf")" \
                                >"$out" 2>"$d/$lane.C$dep.$obs.q.err"; rc=$?
                            t1=$(now_us)
                            printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "$lane" "$dep" "$rep" "$obs" "$rc" \
                                "$(( t1 - t0 ))" >>"$RAW/cold_samples.tsv"
                        done
                        stmts=""
                        for obs in $obslist; do stmts="$stmts $(cat "$RAW/sql/C$dep/$fmt.$obs.sql");"; done
                        t0=$(now_us)
                        timeout "$OP_TIMEOUT" /usr/bin/time -v sqlite3 -batch "$db" "$stmts" \
                            2>"$d/$lane.C$dep.warm.$rep.time" >"$RAW/warm/rep$rep.C$dep.$id.$fmt.$lane.jsonl"; rc=$?
                        t1=$(now_us)
                        rss=$(rss_kb "$d/$lane.C$dep.warm.$rep.time")
                        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "$lane" "$dep" "$rep" "$rc" \
                            "$(( t1 - t0 ))" "${rss:-0}" "$(printf '%s' "$obslist" | wc -w | tr -d ' ')" >>"$RAW/warm_samples.tsv"
                    done
                done
            fi
        done
    done

    # ---- exact original closure: VOLE + every competitor (C0 store) ----
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
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "vole" "$v_ok" "$v_rc" "$v_us" >>"$RAW/exact.tsv"
    for lane in $CONFIGS hist; do
        db="$d/$lane.C0/store.db"; [ -f "$db" ] || db="$d/$lane.C5/store.db"
        [ -f "$db" ] || continue
        t0=$(now_us)
        timeout "$OP_TIMEOUT" python3 "$COMP" materialize --db "$db" --out "$d/$lane.exact.bin" \
            >/dev/null 2>"$d/$lane.exact.err"; rc=$?
        t1=$(now_us)
        s=$(sha256sum "$d/$lane.exact.bin" 2>/dev/null | cut -d' ' -f1)
        l=$(stat -c %s "$d/$lane.exact.bin" 2>/dev/null || echo 0)
        ok=0; [ "$s" = "$sha" ] && [ "$l" = "$blen" ] && ok=1
        printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "$lane" "$ok" "$rc" "$(( t1 - t0 ))" >>"$RAW/exact.tsv"
    done

    # ---- durability probe: full@C5 NORMAL vs FULL (build wall + bytes) ----
    for dur in normal full; do
        sd="$d/dur.$dur"; rm -rf "$sd"; mkdir -p "$sd"
        t0=$(now_us)
        timeout "$OP_TIMEOUT" python3 "$COMP" build --config full --format "$fmt" \
            --source "$path" --db "$sd/store.db" --through 5 --durability "$dur" \
            --family "$fam" --member "$id" --is-head "$is_head" \
            --metrics "$sd/m.json" \
            >/dev/null 2>"$d/dur.$dur.err"; rc=$?
        t1=$(now_us)
        dus=$(jq -r '.db_us // 0' "$sd/m.json" 2>/dev/null)
        [ -n "$dus" ] || dus=$(( t1 - t0 ))
        printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "full" "5" "$dur" "$dus" "$(fbytes "$sd")" >>"$RAW/durability.tsv"
    done

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
  "phase": "22.1 — SQLite competitive envelope vs the equal-contract court",
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
  "configs": "$CONFIGS",
  "hist_depths": "$HISTDEPTHS",
  "depths": "$DEPTHS",
  "durability": "$DURABILITY",
  "op_timeout_s": $OP_TIMEOUT,
  "reps": $REPS,
  "bootstrap_resamples": $BOOT,
  "bootstrap_seed": $SEED,
  "tie_band": $TIE,
  "interleave": "odd reps VOLE-group-then-SQLite-group; even reps SQLite-group-then-VOLE (raw/order.tsv)",
  "timing": "microseconds via EPOCHREALTIME; a per-rep untimed warm-up precedes each lane's timed queries",
  "accounting": "persistent bytes = sum of regular-file sizes; du -sb never used (ADR-0049)",
  "sqlite_lane": "tools/fixtures/phase22-competitors.py (envelope) + the frozen tools/fixtures/phase18-contract-packed.py (historical control lane hist)",
  "answer_sql": "frozen control sql_for for every lane (the query is a contract property, not a tuning property)",
  "env_affecting_semantics": {"LC_ALL": "C", "CARGO_PROFILE_DEV_DEBUG": "0 (dev builds only)"}
}
EOF
cp "$RAW/environment.json" "$CAMPAIGN/environment.json"

cat >"$CAMPAIGN/commands.txt" <<'EOF'
# Phase 22.1 — SQLite competitive envelope, equal-contract court. Never on the host.
docker compose run --rm --no-TTY doc-baseline bash tools/phase22-competitors-court.sh
# bounded / explicit population:
docker compose run --rm --no-TTY doc-baseline \
  sh -c 'REPS=3 CONFIGS="full hybrid" bash tools/phase22-competitors-court.sh'
EOF

# --- aggregate + seal -------------------------------------------------------
echo "-- aggregating" >&2
python3 "$COMP" aggregate "$RAW" "$CAMPAIGN" --reps "$REPS" --boot "$BOOT" --seed "$SEED" --tie "$TIE" \
    | tee "$RAW/summary.txt"

cat >"$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "22.1 — SQLite competitive envelope vs the equal-contract court",
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
  "configs": "$CONFIGS",
  "capability_ceilings": {"minimal": 0, "fts": 0, "structural": 1, "full": 5, "adaptive": 5, "hybrid": 5, "hist": 5},
  "court": "tools/phase22-competitors-court.sh",
  "fixture": "tools/fixtures/phase22-competitors.py",
  "historical_control": "tools/fixtures/phase18-contract-packed.py (UNMODIFIED; lane hist)",
  "design": "paired, interleaved repetitions; every sample retained in raw/*_samples.tsv; per-rep untimed warm-up; the shared extraction is measured once per doc-rep and passed to every config via --extract-json",
  "accounting": "persistent bytes = sum of regular-file sizes (find -printf %s) for BOTH lanes; du -sb is never used (ADR-0049)",
  "durability": "matched headline at synchronous=NORMAL; a synchronous=FULL probe is measured separately (raw/durability.tsv)",
  "exactness": "VOLE materialize --exact --packed and every SQLite lane's retained blob, both checked against source length + SHA-256",
  "rc_codes": "0 ok; 3 decline; 6 unsupported-feature; 124 timeout; 137 SIGKILL/OOM"
}
EOF

chown -R 1000:1000 "$CAMPAIGN" "$WORK" 2>/dev/null || true

echo "phase22.1 competitor-envelope court: done — $CAMPAIGN"
