#!/usr/bin/env bash
# phase15-workers court (Phase 15.4) — worker-count sweep + determinism witness.
#
# ## What this measures
#
# Phase 15.4 adds an optional bounded worker pool to `field-ingest`
# (`--workers N`, `parallel` feature; `1`/absent is serial, `0` is
# `available_parallelism`). For a documented SUBSET of the frozen `real100-v1`
# corpus this court:
#
#   1. `encode`s each document ONCE, then `field-ingest`s it with
#      `--workers 1 2 4 8 16`, recording wall ms for each count (the speedup
#      curve);
#   2. asserts the resulting `field` id is IDENTICAL across every worker count
#      (the pool parallelizes independent work and must not change the field);
#   3. `materialize --exact`es from every per-count store and checks SHA-256 +
#      byte length against the manifest `sha256`/`byte_len` — most importantly
#      for `--workers 1` (the determinism witness: every count reproduces the
#      serial result byte-for-byte).
#
# ## Lane caveat (labelled, not hidden)
#
# The `doc-baseline` lane is capped at `cpus: 8`. `--workers 8` saturates the
# lane; `--workers 16` OVERSUBSCRIBES it (16 runnable threads on 8 CPUs), so its
# wall time is informational, not a clean scaling point. The curve is read with
# that bound in mind.
#
# ## Requirements
#
# MUST be built with `parallel` (the script builds `--release --locked
# --all-features` and uses `target/release/vole-document`); a build failure is
# fatal — the court refuses to measure a stale binary. The docx/epub preflight
# guard from `tools/real100-court.sh` is copied: a mis-featured binary silently
# declines every DOCX/EPUB op, so we fail loudly instead.
#
# ## Subset selection (documented)
#
# Header-aware over `manifest.tsv`. Unless `IDS` is given, one document (the
# SMALLEST member) is chosen per `(format, size_class)` stratum, ordered so the
# required shapes come first:
#   * a LARGE PDF (the smallest pdf in the `50-100MiB` stratum) — first;
#   * a large PACKAGE format (epub `10-50MiB`) — second;
#   * another package format (docx `<100KiB`) — third;
#   * the remaining strata by size-class rank then format.
# `head -n LIMIT` (default 10) therefore always contains a large PDF and at least
# one package format. This is a SUBSET, not the frozen 100-document population.
#
# ## Layout / exact invocation (commands.txt-style; never the host)
#
# Runs in the digest-pinned `doc-baseline` service, hard-capped at
# mem_limit == memswap_limit == 6g (no zram evasion), pids_limit 4096, cpus 8:
#   rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase15-workers-court.sh
#   # bounded smoke / explicit ids:
#   docker compose run --rm --no-TTY doc-baseline sh -c 'LIMIT=4 bash tools/phase15-workers-court.sh'
#   docker compose run --rm --no-TTY doc-baseline \
#     sh -c 'IDS="nasa-pdf-0001 nist-docx-0001 nasa-epub-0001" bash tools/phase15-workers-court.sh'
set -uo pipefail

cd /work
export LC_ALL=C

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=evidence/campaigns/${STAMP}-phase15-workers-${SHA}
RAW=$CAMPAIGN/raw
WORK=${WORK:-evidence/scratch/phase15-workers}
mkdir -p "$RAW" "$WORK"

MANIFEST=${MANIFEST:-real100-v1/manifest.tsv}
CORPUS=${CORPUS:-real100-v1/documents}
OP_TIMEOUT=${OP_TIMEOUT:-300}
LIMIT=${LIMIT:-10}
IDS=${IDS:-}
WORKER_COUNTS=${WORKER_COUNTS:-"1 2 4 8 16"}
# `parallel` is required, so the worker court measures the release binary (an
# explicit `BIN` still overrides for a prebuilt artifact).
BUILD_ARGS="--release --locked --all-features"
BIN=${BIN:-target/release/vole-document}

echo "== phase15-workers court (Phase 15.4) ==" >&2
echo "-- building release binary (--release --locked --all-features)" >&2
# shellcheck disable=SC2086
if ! cargo build $BUILD_ARGS >&2; then
    echo "phase15-workers: build FAILED ($BUILD_ARGS); refusing to measure" >&2
    exit 1
fi
# Preflight: a stale/mis-featured binary (e.g. a previous non-`--all-features`
# release build left in place after a failed build) silently answers rc=6 for
# every DOCX/EPUB op; fail loudly instead.
PREFLIGHT=""
for cand in real100-v1/documents/nist/docx/*.docx real100-v1/documents/nasa/epub/*.epub; do
    [ -f "$cand" ] && { PREFLIGHT="$cand"; break; }
done
if [ -n "$PREFLIGHT" ]; then
    expect=$(case "$PREFLIGHT" in *.docx) echo docx ;; *) echo epub ;; esac)
    if ! "$BIN" capabilities "$PREFLIGHT" 2>/dev/null | grep -q "\"adapter\":\"$expect\""; then
        echo "phase15-workers: preflight failed — $BIN does not support $expect; refusing to measure" >&2
        exit 1
    fi
fi
# The `parallel` feature is the whole point: the release build above uses
# `--all-features`, and the adapters are preflighted; a `--workers N > 1` without
# `parallel` is a typed usage error, which the rows below would surface as rc!=0.

# --- subset selection (documented above) -----------------------------------
select_docs() {
  if [ -n "$IDS" ]; then
    for want in $(printf '%s' "$IDS" | tr ',' ' '); do
      awk -F'\t' -v want="$want" '
        NR==1{for(i=1;i<=NF;i++)h[$i]=i; next}
        $h["id"]==want {print $h["id"]"\t"$h["agency"]"\t"$h["format"]"\t"$h["size_class"]"\t"$h["byte_len"]"\t"$h["sha256"]}
      ' "$MANIFEST"
    done
    return
  fi
  awk -F'\t' '
    BEGIN{rank["<100KiB"]=1;rank["100KiB-1MiB"]=2;rank["1-10MiB"]=3;rank["10-50MiB"]=4;rank["50-100MiB"]=5;rank[">100MiB"]=6}
    NR==1{for(i=1;i<=NF;i++)h[$i]=i; next}
    {
      fmt=$h["format"]; scl=$h["size_class"]; bl=$h["byte_len"]+0; k=fmt"|"scl;
      if(!(k in bestbl)||bl<bestbl[k]){bestbl[k]=bl;bestline[k]=$0}
    }
    END{
      for(k in bestline){
        split(k,a,"|"); fmt=a[1]; scl=a[2]; split(bestline[k],f,"\t");
        if(fmt=="pdf" && scl=="50-100MiB") p=1;
        else if(fmt=="epub" && scl=="10-50MiB") p=2;
        else if(fmt=="docx" && scl=="<100KiB") p=3;
        else p=10+rank[scl];
        printf "%d\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n", p, fmt, f[h["id"]], f[h["agency"]], fmt, scl, f[h["byte_len"]], f[h["sha256"]];
      }
    }
  ' "$MANIFEST" | sort -k1,1n -k2,2 | cut -f3- | head -n "$LIMIT"
}

now_ms() { echo $(( ${EPOCHREALTIME/./} / 1000 )); }

# --- raw table headers ------------------------------------------------------
printf 'id\tagency\tfmt\tsclass\tbyte_len\tworkers\trc\twall_ms\tfield\tsource_len\n' >"$RAW/workers.tsv"
printf 'id\tfields_seen\tfield_equal\texact_1\texact_2\texact_4\texact_8\texact_16\tall_exact\tmanifest_sha\n' >"$RAW/determinism.tsv"

echo "-- subset ($(select_docs | wc -l) docs; LIMIT=$LIMIT${IDS:+; IDS=$IDS})" >&2
select_docs >"$RAW/subset.tsv"

# --- provenance -------------------------------------------------------------
RUSTC_V=$(rustc --version 2>/dev/null | sed 's/^rustc //')
CARGO_V=$(cargo --version 2>/dev/null | sed 's/^cargo //')
ARCH=$(uname -m)
COMMIT=$(git rev-parse HEAD 2>/dev/null || echo unknown)
DIRTY=$(git --no-optional-locks status --porcelain 2>/dev/null | head -40 | tr '\n' ';')
LOCK_SHA=$(sha256sum Cargo.lock 2>/dev/null | cut -d' ' -f1)
JQ_V=$(jq --version 2>/dev/null)
IMAGE_ID=${HOST_IMAGE_ID:-unrecorded}
BASE_STABLE=rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e
MANIFEST_SHA=$(sha256sum "$MANIFEST" | cut -d' ' -f1)
cat >"$RAW/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "15.4 — bounded worker pool for field-ingest (worker-count sweep)",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_commit": "$COMMIT",
  "git_dirty": "$DIRTY",
  "arch": "$ARCH",
  "service": "doc-baseline",
  "base_image": "$BASE_STABLE",
  "image_id": "$IMAGE_ID",
  "rustc": "$RUSTC_V",
  "cargo": "$CARGO_V",
  "jq": "$JQ_V",
  "cargo_lock_sha256": "$LOCK_SHA",
  "manifest": "$MANIFEST",
  "manifest_sha256": "$MANIFEST_SHA",
  "bin": "$BIN",
  "features": "--all-features (parallel)",
  "worker_counts": "$WORKER_COUNTS",
  "lane_cpus": 8,
  "oversubscribed_counts": "16 (> lane cpus=8; informational)",
  "op_timeout_s": $OP_TIMEOUT,
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF
cat >"$CAMPAIGN/commands.txt" <<'EOF'
# Phase 15.4 — worker-count sweep + determinism witness (SUBSET, release build).
# Never run on the host; every lane is a hard-capped compose service.
docker compose run --rm --no-TTY doc-baseline bash tools/phase15-workers-court.sh
# bounded smoke / explicit ids (override):
docker compose run --rm --no-TTY doc-baseline sh -c 'LIMIT=4 bash tools/phase15-workers-court.sh'
docker compose run --rm --no-TTY doc-baseline sh -c 'IDS="nasa-pdf-0001 nist-docx-0001 nasa-epub-0001" bash tools/phase15-workers-court.sh'
EOF

# --- main loop --------------------------------------------------------------
while IFS=$'\t' read -r id agency fmt sclass blen sha; do
    d="$WORK/$id"
    rm -rf "$d"; mkdir -p "$d"
    src="$CORPUS/$agency/$fmt/$id.$fmt"
    echo "== $id ($fmt, $sclass, $blen B) ==" >&2

    declare -A EX
    fields_all=""
    enc_rc=1
    for n in 1 2 4 8 16 $WORKER_COUNTS; do EX[$n]=0; done

    if [ ! -f "$src" ]; then
        echo "   missing source $src" >&2
        enc_rc=3
    else
        t0=$(now_ms)
        timeout "$OP_TIMEOUT" "$BIN" encode "$src" "$d/doc.voldoc" >"$d/encode.json" 2>"$d/encode.err"; enc_rc=$?
        t1=$(now_ms)
    fi

    if [ "$enc_rc" -eq 0 ]; then
        for n in $WORKER_COUNTS; do
            store="$d/w$n"
            # ---- ingest with this worker count ----------------------------
            t0=$(now_ms)
            timeout "$OP_TIMEOUT" "$BIN" field-ingest "$d/doc.voldoc" --store "$store" \
                --workers "$n" >"$d/w$n.ingest.json" 2>"$d/w$n.ingest.err"; rc=$?
            t1=$(now_ms); wall=$(( t1 - t0 ))
            field=$(jq -r '.field // empty' "$d/w$n.ingest.json" 2>/dev/null)
            srclen=$(jq -r '.source_len // empty' "$d/w$n.ingest.json" 2>/dev/null)
            printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
                "$id" "$agency" "$fmt" "$sclass" "$blen" "$n" "$rc" "$wall" "$field" "$srclen" >>"$RAW/workers.tsv"
            fields_all="$fields_all$field"$'\n'

            # ---- determinism witness: exact materialize from this store ---
            if [ "$rc" -eq 0 ] && [ -n "$field" ]; then
                timeout "$OP_TIMEOUT" "$BIN" materialize --store "$store" --field "$field" \
                    --exact --output "$d/w$n.out" >"$d/w$n.mat.json" 2>"$d/w$n.mat.err"
                osha=$(sha256sum "$d/w$n.out" 2>/dev/null | cut -d' ' -f1)
                olen=$(stat -c %s "$d/w$n.out" 2>/dev/null || echo 0)
                if [ "$osha" = "$sha" ] && [ "$olen" = "$blen" ]; then EX[$n]=1; fi
            fi
            rm -rf "$store" "$d/w$n.out"
        done
    else
        # encode failed (or source missing): record a typed row per count.
        for n in $WORKER_COUNTS; do
            printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
                "$id" "$agency" "$fmt" "$sclass" "$blen" "$n" "$enc_rc" "0" "" "" >>"$RAW/workers.tsv"
        done
    fi

    # ---- determinism summary ----------------------------------------------
    fields_seen=$(printf '%s' "$fields_all" | sed '/^$/d' | sort -u | wc -l | tr -d ' ')
    field_equal=0
    if [ "$fields_seen" = "1" ] && [ -n "$(printf '%s' "$fields_all" | sed '/^$/d' | head -1)" ]; then
        field_equal=1
    fi
    all_exact=1
    for n in $WORKER_COUNTS; do [ "${EX[$n]}" = "1" ] || all_exact=0; done
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$id" "$fields_seen" "$field_equal" "${EX[1]}" "${EX[2]}" "${EX[4]}" "${EX[8]}" "${EX[16]}" \
        "$all_exact" "$sha" >>"$RAW/determinism.tsv"

    # Keep the small per-document JSON/err evidence; drop the bulk.
    ed="$RAW/perdoc/$id"; mkdir -p "$ed"
    for f in "$d"/*.json "$d"/*.err; do [ -f "$f" ] && cp "$f" "$ed/"; done
    rm -rf "$d"
done < <(select_docs)

echo "-- aggregating" >&2
python3 tools/fixtures/phase15-workers.py "$RAW" "$CAMPAIGN" | tee "$RAW/summary.txt"

echo "phase15-workers court: done — $CAMPAIGN"
