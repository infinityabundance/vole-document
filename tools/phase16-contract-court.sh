#!/usr/bin/env bash
# Phase 16 item 5 — the contract-equivalent heterogeneous-session court.
#
# ## Question
#
# The real100-v1 frontier court compares VOLE against a source-retaining
# SQLite+FTS baseline that answers only "value" lookups. That baseline is not
# answering the SAME contract VOLE answers. This court forces a source-retaining
# SQLite baseline to satisfy the SAME escalating capability surface (C0..C5), so
# the comparison is apples-to-apples — or shows SQLite still wins anyway.
#
# ## Contracts (see tools/fixtures/phase16-contract.py for the envelope)
#
#   C0 value only           C1 + native coordinate   C2 + provenance (class + span)
#   C3 + exact closure      C4 + revision lineage    C5 + heterogeneous batch
#
# ## What it does, per document
#
#   * VOLE `encode` + `field-ingest`                      -> the VOLE store + field
#   * SQLite `build --through 0..5`                       -> six escalating stores
#   * a heterogeneous observation schedule, per depth, for BOTH lanes:
#       VOLE cold  = one `observe` process per observation
#       VOLE warm  = one `observe-batch` session (mixed kinds)
#       SQLite cold = one `query` process per observation
#       SQLite warm = one `session` process
#   * exact original closure (length + SHA-256 + byte compare) for both lanes
#
# Every envelope is written to raw/env/C<d>/ and equivalence is decided by the
# aggregator, never by the court.
#
# ## Population (documented, bounded, deterministic)
#
# A fixed 12-document subset spanning pdf/docx/epub and the <100KiB /
# 100KiB-1MiB / 1-10MiB size classes, including two revision families
# (rev-nist-fips-140 pdf-0016/0017, rev-nist-sp800-34 docx-0005/0008). Override
# with `IDS="id1 id2 ..."`.
#
# ## Lane
#
# Never the host: the digest-pinned, hard-capped `doc-baseline` service
# (mem_limit == memswap_limit == 6g, pids_limit 4096, cpus 8):
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase16-contract-court.sh
#   docker compose run --rm --no-TTY doc-baseline sh -c 'IDS="nist-pdf-0017 nist-docx-0005" bash tools/phase16-contract-court.sh'
set -uo pipefail

cd /work
export LC_ALL=C

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=evidence/campaigns/${STAMP}-phase16-contract-${SHA}${TAG:+-$TAG}
RAW=$CAMPAIGN/raw
WORK=${WORK:-evidence/scratch/phase16-contract}
mkdir -p "$RAW" "$RAW/env" "$RAW/warm" "$WORK"

MANIFEST=${MANIFEST:-real100-v1/manifest.tsv}
CORPUS=${CORPUS:-real100-v1/documents}
BASE=tools/fixtures/phase16-contract.py
OP_TIMEOUT=${OP_TIMEOUT:-180}
DEPTHS=${DEPTHS:-0 1 2 3 4 5}
DEFAULT_IDS="nist-pdf-0002 nist-pdf-0004 nist-pdf-0016 nist-pdf-0017 nist-docx-0005 nist-docx-0008 nist-docx-0009 nist-docx-0014 nist-epub-0003 nist-epub-0006 nist-epub-0008 nist-epub-0009"
IDS=${IDS:-$DEFAULT_IDS}

PROFILE=${PROFILE:-release}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-${VOLE_BIN:-target/debug/vole-document}} ;;
esac

echo "== phase16.5 contract-equivalent court ==" >&2
echo "-- building ($PROFILE) binary" >&2
# shellcheck disable=SC2086
if ! cargo build $BUILD_ARGS >&2; then
    echo "phase16-contract: build FAILED ($BUILD_ARGS); refusing to measure" >&2
    exit 1
fi
[ -x "$BIN" ] || { echo "phase16-contract: $BIN missing; refusing to measure" >&2; exit 1; }

PREFLIGHT=""
for cand in real100-v1/documents/nist/docx/*.docx real100-v1/documents/nasa/epub/*.epub; do
    [ -f "$cand" ] && { PREFLIGHT="$cand"; break; }
done
if [ -n "$PREFLIGHT" ]; then
    expect=$(case "$PREFLIGHT" in *.docx) echo docx ;; *) echo epub ;; esac)
    if ! "$BIN" capabilities "$PREFLIGHT" 2>/dev/null | grep -q "\"adapter\":\"$expect\""; then
        echo "phase16-contract: preflight failed — $BIN lacks $expect" >&2
        exit 1
    fi
fi

now_ms() { echo $(( ${EPOCHREALTIME/./} / 1000 )); }
rss_kb() { awk -F': ' '/Maximum resident set size/{print $2}' "$1"; }
# Persistent bytes as the sum of REGULAR-FILE sizes. `du -sb` is unusable here:
# the repo lives on a bind-mounted host filesystem that reports huge phantom
# directory sizes (a 2.1 MB VOLE store measures 30-130 MB under `du -sb`), while
# single-file SQLite dbs are unaffected -- an apples-to-oranges trap.
fbytes() { find "$@" -type f -printf '%s\n' 2>/dev/null | awk '{s+=$1} END{print s+0}'; }

# --- subset selection + revision-family head map ---------------------------
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
# head of a revision family = max byte_len, ties by id
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
    echo "phase16-contract: corpus verification FAILED; refusing to measure" >&2
    exit 1
fi

# --- generate the contract envelope SQL once (served via the sqlite3 CLI) ---
mkdir -p "$RAW/sql"
python3 "$BASE" sqlgen --out "$RAW/sql" --depths "$DEPTHS" >"$RAW/sqlgen.json"

# --- observation sets ------------------------------------------------------
obs_for() { # fmt -> echo space-separated obs
    case "$1" in
        pdf)  echo "bytes text metadata revision" ;;
        *)    echo "bytes text doc-text heading table resource metadata revision" ;;
    esac
}
vole_args() { # fmt obs -> echo observe args (or DECLINE)
    case "$2" in
        bytes)    echo "--byte-range 0..64 --kind exact" ;;
        text)     if [ "$1" = pdf ]; then echo "--page 1 --kind text"; else echo "--block 0 --kind text"; fi ;;
        doc-text) echo "--doc-text --kind text" ;;
        heading)  echo "--heading 0 --kind text" ;;
        table)    echo "--table 0 --kind text" ;;
        resource) echo "--resource 0 --kind metadata" ;;
        metadata) echo "--metadata --kind metadata" ;;
        revision) echo "--revision 0 --kind text" ;;
        *)        echo "DECLINE" ;;
    esac
}

# --- raw tables ------------------------------------------------------------
printf 'id\tfmt\tsclass\tbyte_len\tv_enc_rc\tv_enc_ms\tv_ing_rc\tv_ing_ms\tv_store_bytes\tv_desc_bytes\n' >"$RAW/onetime.tsv"
printf 'id\tfmt\tdepth\tbuild_rc\tbuild_ms\tdb_bytes\tblocks\theading_nodes\ttables\tcells\tresources\n' >"$RAW/build.tsv"
printf 'depth\tid\tfmt\tobs\tlane\trc\tms\n' >"$RAW/cold.tsv"
printf 'depth\tid\tfmt\tlane\trc\tms\trss_kb\tn\n' >"$RAW/warm.tsv"
printf 'id\tfmt\tv_ok\tv_rc\tv_ms\tsql_ok\tsql_rc\tsql_ms\n' >"$RAW/exact.tsv"

# --- main loop -------------------------------------------------------------
while IFS='|' read -r id agency fmt sclass blen sha path fam is_head; do
    [ -n "$id" ] || continue
    d="$WORK/$id"; rm -rf "$d"; mkdir -p "$d"
    echo "== $id ($fmt, $sclass, $blen B, family=${fam:-none}, head=$is_head) ==" >&2

    # ---- VOLE one-time build (encode + ingest) --------------------------
    venc_rc=1; venc=0; ving_rc=-1; ving=-1; field=""; v_store_bytes=0; v_desc_bytes=0
    t0=$(now_ms)
    timeout "$OP_TIMEOUT" "$BIN" encode "$path" "$d/doc.voldoc" >"$d/encode.json" 2>"$d/encode.err"; venc_rc=$?
    t1=$(now_ms); venc=$(( t1 - t0 ))
    if [ "$venc_rc" -eq 0 ]; then
        t0=$(now_ms)
        timeout "$OP_TIMEOUT" "$BIN" field-ingest "$d/doc.voldoc" --store "$d/vstore" >"$d/ingest.json" 2>"$d/ingest.err"; ving_rc=$?
        t1=$(now_ms); ving=$(( t1 - t0 ))
        [ "$ving_rc" -eq 0 ] && field=$(jq -r '.field // empty' "$d/ingest.json" 2>/dev/null)
        [ "$ving_rc" -eq 0 ] && v_store_bytes=$(fbytes "$d/vstore")
        v_desc_bytes=$(fbytes "$d/doc.voldoc")
    fi

    # ---- SQLite escalating builds ---------------------------------------
    for dep in $DEPTHS; do
        t0=$(now_ms)
        timeout "$OP_TIMEOUT" python3 "$BASE" build --format "$fmt" --source "$path" \
            --db "$d/a1c.$dep.db" --through "$dep" --family "$fam" --member "$id" \
            --is-head "$is_head" --metrics "$d/a1c.$dep.metrics.json" \
            >"$d/a1c.$dep.build.json" 2>"$d/a1c.$dep.build.err"; rc=$?
        t1=$(now_ms)
        ms=$(( t1 - t0 ))
        b=0; nb=0; nh=0; nt=0; ncl=0; nrs=0
        if [ "$rc" -eq 0 ]; then
            b=$(fbytes "$d/a1c.$dep.db" "$d/a1c.$dep.db-wal" "$d/a1c.$dep.db-shm")
            nb=$(jq -r '.blocks // 0' "$d/a1c.$dep.build.json" 2>/dev/null)
            nh=$(jq -r '.headings // 0' "$d/a1c.$dep.build.json" 2>/dev/null)
            nt=$(jq -r '.tables // 0' "$d/a1c.$dep.build.json" 2>/dev/null)
            ncl=$(jq -r '.cells // 0' "$d/a1c.$dep.build.json" 2>/dev/null)
            nrs=$(jq -r '.resources // 0' "$d/a1c.$dep.build.json" 2>/dev/null)
        fi
        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
            "$id" "$fmt" "$dep" "$rc" "$ms" "$b" "$nb" "$nh" "$nt" "$ncl" "$nrs" >>"$RAW/build.tsv"
    done

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$id" "$fmt" "$sclass" "$blen" "$venc_rc" "$venc" "$ving_rc" "$ving" \
        "$v_store_bytes" "$v_desc_bytes" >>"$RAW/onetime.tsv"

    # ---- query schedules, per depth -------------------------------------
    # Discard a warmup so the first (C0) timed cold pass is not penalized by a
    # cold page cache (the stores are read many times afterwards).
    if [ -n "$field" ]; then
        for obs in $(obs_for "$fmt"); do
            args=$(vole_args "$fmt" "$obs")
            # shellcheck disable=SC2086
            timeout "$OP_TIMEOUT" "$BIN" observe $args --store "$d/vstore" --field "$field" \
                >/dev/null 2>&1 || true
        done
    fi
    for obs in $(obs_for "$fmt"); do
        timeout "$OP_TIMEOUT" sqlite3 -batch "$d/a1c.5.db" "$(cat "$RAW/sql/C5/$fmt.$obs.sql")" \
            >/dev/null 2>&1 || true
    done
    for dep in $DEPTHS; do
        envdir="$RAW/env/C$dep"; mkdir -p "$envdir"
        obslist=$(obs_for "$fmt")
        # -- VOLE cold (one process per observation) --
        for obs in $obslist; do
            args=$(vole_args "$fmt" "$obs")
            outfile="$envdir/$id.$fmt.$obs.vole.json"
            if [ -z "$field" ]; then
                printf 'null\n' >"$outfile"
                printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$dep" "$id" "$fmt" "$obs" "vole" 3 0 >>"$RAW/cold.tsv"
                continue
            fi
            t0=$(now_ms)
            # shellcheck disable=SC2086
            timeout "$OP_TIMEOUT" "$BIN" observe $args --store "$d/vstore" --field "$field" \
                >"$outfile" 2>"$d/vole.$obs.err"; rc=$?
            t1=$(now_ms)
            [ -s "$outfile" ] || printf 'null\n' >"$outfile"
            printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$dep" "$id" "$fmt" "$obs" "vole" "$rc" "$(( t1 - t0 ))" >>"$RAW/cold.tsv"
        done
        # -- SQLite cold (one sqlite3 process per observation) --
        for obs in $obslist; do
            outfile="$envdir/$id.$fmt.$obs.a1c.json"
            sqlf="$RAW/sql/C$dep/$fmt.$obs.sql"
            : >"$outfile"
            t0=$(now_ms)
            timeout "$OP_TIMEOUT" sqlite3 -batch "$d/a1c.$dep.db" "$(cat "$sqlf")" \
                >"$outfile" 2>"$d/a1c.$dep.$obs.q.err"; rc=$?
            t1=$(now_ms)
            printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$dep" "$id" "$fmt" "$obs" "a1c" "$rc" "$(( t1 - t0 ))" >>"$RAW/cold.tsv"
        done

        # -- VOLE warm (one observe-batch session) --
        req="$d/vole.C$dep.reqs"; : >"$req"
        for obs in $obslist; do
            [ -z "$field" ] && continue
            printf '%s\n' "$(vole_args "$fmt" "$obs")" >>"$req"
        done
        if [ -n "$field" ] && [ -s "$req" ]; then
            wout="$RAW/warm/C$dep.$id.$fmt.vole.jsonl"; mkdir -p "$RAW/warm"
            t0=$(now_ms)
            timeout "$OP_TIMEOUT" /usr/bin/time -v "$BIN" observe-batch --store "$d/vstore" \
                --field "$field" --requests "$req" >"$wout" 2>"$d/vole.C$dep.warm.time"; rc=$?
            t1=$(now_ms)
            rss=$(rss_kb "$d/vole.C$dep.warm.time")
            printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$dep" "$id" "$fmt" "vole" "$rc" "$(( t1 - t0 ))" "${rss:-0}" "$(grep -c . "$req")" >>"$RAW/warm.tsv"
        else
            printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$dep" "$id" "$fmt" "vole" 3 0 0 0 >>"$RAW/warm.tsv"
        fi
        # -- SQLite warm (one sqlite3 session process, all statements) --
        stmts=""
        for obs in $obslist; do
            stmts="$stmts $(cat "$RAW/sql/C$dep/$fmt.$obs.sql");"
        done
        wout="$RAW/warm/C$dep.$id.$fmt.a1c.jsonl"
        t0=$(now_ms)
        timeout "$OP_TIMEOUT" /usr/bin/time -v sqlite3 -batch "$d/a1c.$dep.db" "$stmts" \
            2>"$d/a1c.C$dep.warm.time" >"$wout"; rc=$?
        t1=$(now_ms)
        rss=$(rss_kb "$d/a1c.C$dep.warm.time")
        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$dep" "$id" "$fmt" "a1c" "$rc" "$(( t1 - t0 ))" "${rss:-0}" "$(printf '%s' "$obslist" | wc -w | tr -d ' ')" >>"$RAW/warm.tsv"
    done

    # ---- exact original closure -----------------------------------------
    v_ok=0; v_rc=3; v_ms=0
    if [ -n "$field" ]; then
        t0=$(now_ms)
        timeout "$OP_TIMEOUT" "$BIN" materialize --store "$d/vstore" --field "$field" \
            --exact --output "$d/v.exact.bin" >/dev/null 2>"$d/v.exact.err"; v_rc=$?
        t1=$(now_ms); v_ms=$(( t1 - t0 ))
        vs=$(sha256sum "$d/v.exact.bin" 2>/dev/null | cut -d' ' -f1)
        vl=$(stat -c %s "$d/v.exact.bin" 2>/dev/null || echo 0)
        [ "$vs" = "$sha" ] && [ "$vl" = "$blen" ] && v_ok=1
    fi
    sql_ok=0; sql_rc=3; sql_ms=0
    t0=$(now_ms)
    timeout "$OP_TIMEOUT" python3 "$BASE" materialize --db "$d/a1c.5.db" --out "$d/sql.exact.bin" \
        >/dev/null 2>"$d/sql.exact.err"; sql_rc=$?
    t1=$(now_ms); sql_ms=$(( t1 - t0 ))
    ss=$(sha256sum "$d/sql.exact.bin" 2>/dev/null | cut -d' ' -f1)
    sl=$(stat -c %s "$d/sql.exact.bin" 2>/dev/null || echo 0)
    [ "$ss" = "$sha" ] && [ "$sl" = "$blen" ] && sql_ok=1
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "$v_ok" "$v_rc" "$v_ms" "$sql_ok" "$sql_rc" "$sql_ms" >>"$RAW/exact.tsv"

    # bound the per-doc scratch: keep small JSON/err evidence only
    ed="$RAW/perdoc/$id"; mkdir -p "$ed"
    for f in "$d"/*.json "$d"/*.err; do [ -f "$f" ] && cp "$f" "$ed/"; done
    rm -rf "$d"
done <"$RAW/subset.tsv"

# --- provenance ------------------------------------------------------------
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
  "phase": "16.5 — contract-equivalent heterogeneous-session court",
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
  "env_affecting_semantics": {"LC_ALL": "C", "VOLE_BIN": "ignored under PROFILE=release"}
}
EOF
cp "$RAW/environment.json" "$CAMPAIGN/environment.json"

cat >"$CAMPAIGN/commands.txt" <<'EOF'
# Phase 16 item 5 — contract-equivalent heterogeneous-session court. Never on the host.
docker compose run --rm --no-TTY doc-baseline bash tools/phase16-contract-court.sh
# bounded / explicit population:
docker compose run --rm --no-TTY doc-baseline sh -c 'IDS="nist-pdf-0017 nist-docx-0005" bash tools/phase16-contract-court.sh'
EOF

# --- aggregate + seal ------------------------------------------------------
echo "-- aggregating" >&2
python3 "$BASE" aggregate "$RAW" "$CAMPAIGN" | tee "$RAW/summary.txt"

cat >"$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "16.5 — contract-equivalent heterogeneous-session court",
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
  "depths": "$DEPTHS",
  "court": "tools/phase16-contract-court.sh",
  "baseline": "tools/fixtures/phase16-contract.py (contract baseline + aggregator)",
  "contracts": {
    "C0": "value only",
    "C1": "C0 + native coordinate",
    "C2": "C1 + provenance class + source span",
    "C3": "C2 + exact original closure (length + SHA-256 + cmp)",
    "C4": "C3 + revision lineage (family/member/head)",
    "C5": "C4 + heterogeneous batch served in one session"
  },
  "accounting": "persistent bytes = sum of regular-file sizes (find -printf %s) for BOTH lanes; du -sb is avoided because the bind-mounted host fs inflates directory sizes and would compare VOLE's directory store against SQLite's single file unfairly",
  "rc_codes": "0 ok; 3 decline; 6 unsupported-feature (VOLE revision); 124 timeout; 137 SIGKILL/OOM"
}
EOF

chown -R 1000:1000 "$CAMPAIGN" "$WORK" 2>/dev/null || true

echo "phase16.5 contract court: done — $CAMPAIGN"
