#!/usr/bin/env bash
# Phase 16.2 — full `real100-v1` packed-store storage court.
#
# ## Question
#
# On the 95 documents where BOTH VOLE `field-ingest` and the A1 SQLite build
# succeeded, VOLE's persistent fs store was **1.377x** SQLite's db
# (2,619,707,424 B vs 1,902,919,680 B; by format epub 1.118x / docx 1.310x /
# pdf 1.418x — see evidence/campaigns/2026-10-07-real100-release-baseline-866f489).
# The Phase-15.3 packed court showed fs -> packed gives **0.719x** persistent
# bytes on a 12-document *subset*. Those two ratios are on different populations
# and in different units of comparison, so they may **not** be multiplied.
#
# This court measures, on the FULL frozen population, the SAME population with
# BOTH VOLE backends and the SQLite baseline, and reports whether the packed
# backend closes the persistent-storage gap against SQLite.
#
# ## What it does, per document
#
#   * `encode` once            -> `$d/doc.voldoc`            (venc_rc / venc_ms)
#   * `field-ingest ... --store $d/fs`                 one-file-per-node (ving_fs_rc / ving_fs_ms)
#   * `field-ingest ... --store $d/pack --packed`      append-only fieldpack (ving_pack_rc / ving_pack_ms)
#   * `phase12-baseline.py build`                      A1 SQLite + FTS5 (a1_build_rc / a1_build_ms)
#
# and records the persistent bytes of each substrate (`du -sb`), the two field ids
# (asserted equal — the descriptor is identical, so the `FieldId` must be), and
# the A1 db bytes. Large PDFs may OOM-kill (139/137) or time out (124); those are
# recorded outcomes, never papered over.
#
# ## What it does NOT claim
#
# It makes no claim about content (real documents have unknown content) beyond
# stored bytes and exit codes. Exact reconstruction is established elsewhere
# (Phase 15.3 subset court; the frontier court's `materialize --exact` lane); this
# is a storage court and does not re-run materialization.
#
# ## Population and accounting
#
# The full frozen `real100-v1` population (`LIMIT=0`); `LIMIT=N` caps it for
# smoke. The manifest and the schedule are functions of the corpus alone and are
# hashed in the receipt. Storage accounting mirrors `tools/real100-court.sh`
# exactly: `fs_bytes`/`pack_bytes` are `du -sb` of the whole store directory
# (descriptor/field/index/cache + the seed namespace), `a1_bytes` is `du -sb` of
# the `.db` file, so the fs-vs-SQLite ratio is directly comparable to the
# established 1.377x. `a1_sidecar_bytes` additionally records any `-wal`/`-shm`
# left beside the db, so a WAL residual can never hide.
#
# ## Lane
#
# Never the host: runs in the digest-pinned, hard-capped `doc-baseline` service
# (mem_limit == memswap_limit == 6g, pids_limit 4096, cpus 8):
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase16-packed-full-court.sh
#   # bounded smoke:
#   docker compose run --rm --no-TTY doc-baseline sh -c 'LIMIT=4 bash tools/phase16-packed-full-court.sh'
set -uo pipefail

cd /work
export LC_ALL=C

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=evidence/campaigns/${STAMP}-phase16-packed-full-${SHA}${TAG:+-$TAG}
RAW=$CAMPAIGN/raw
WORK=${WORK:-evidence/scratch/phase16-packed-full}
mkdir -p "$RAW" "$WORK"

MANIFEST=${MANIFEST:-real100-v1/manifest.tsv}
CORPUS=${CORPUS:-real100-v1/documents}
SCHEDULE=$RAW/schedule.json
BASE=tools/fixtures/phase12-baseline.py
OP_TIMEOUT=${OP_TIMEOUT:-180}
LIMIT=${LIMIT:-0}   # 0 = the full frozen population; >0 caps docs (smoke)
# Build profile. The established 1.377x baseline is a release measurement; the
# default here is therefore release. `PROFILE=release` measures
# `target/release/vole-document` and — deliberately — does NOT consult `VOLE_BIN`
# (the `doc-baseline` service exports VOLE_BIN=./target/debug/...). An explicit
# `BIN` still overrides.
PROFILE=${PROFILE:-release}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-${VOLE_BIN:-target/debug/vole-document}} ;;
esac

echo "== phase16.2 packed-full storage court ==" >&2
echo "-- building ($PROFILE) binary" >&2
# shellcheck disable=SC2086
if ! cargo build $BUILD_ARGS >&2; then
    echo "phase16-packed-full: build FAILED ($BUILD_ARGS); refusing to measure" >&2
    exit 1
fi
[ -x "$BIN" ] || { echo "phase16-packed-full: $BIN missing after build; refusing to measure" >&2; exit 1; }

# Preflight: the docx/epub adapters must be present, or a stale/mis-featured
# binary silently declines every package op. Fail loudly instead of measuring.
PREFLIGHT=""
for cand in real100-v1/documents/nist/docx/*.docx real100-v1/documents/nasa/epub/*.epub; do
    [ -f "$cand" ] && { PREFLIGHT="$cand"; break; }
done
if [ -n "$PREFLIGHT" ]; then
    expect=$(case "$PREFLIGHT" in *.docx) echo docx ;; *) echo epub ;; esac)
    if ! "$BIN" capabilities "$PREFLIGHT" 2>/dev/null | grep -q "\"adapter\":\"$expect\""; then
        echo "phase16-packed-full: preflight failed — $BIN does not support $expect; refusing to measure" >&2
        exit 1
    fi
fi

# The schedule is a function of the frozen manifest + corpus only.
echo "-- building the pre-registered schedule" >&2
python3 tools/fixtures/real100-schedule.py "$MANIFEST" "$CORPUS" "$SCHEDULE" >"$RAW/schedule.meta.json" 2>"$RAW/schedule.err"

# Selected population: id agency fmt path sha256 byte_len size_class
jq -r --argjson lim "$LIMIT" '([.documents[]] | (if $lim > 0 then .[:$lim] else . end))[] | [.id,.agency,.format,.path,.sha256,(.byte_len|tostring),.size_class] | @tsv' \
    "$SCHEDULE" >"$RAW/docs.tsv"

# Read-only corpus verification (never rewrites real100-v1/SHA256SUMS).
echo "-- verifying the frozen corpus ($(wc -l <"$RAW/docs.tsv" | tr -d ' ') docs; SHA-256 + length)" >&2
printf 'id\tsha_ok\tlen_ok\tpresent\n' >"$RAW/corpus_verify.tsv"
verify_rc=0
while IFS=$'\t' read -r id _agency _fmt path sha blen _sclass; do
    if [ ! -f "$path" ]; then
        printf '%s\t0\t0\t0\n' "$id" >>"$RAW/corpus_verify.tsv"
        verify_rc=1
        continue
    fi
    got_sha=$(sha256sum "$path" | cut -d' ' -f1)
    got_len=$(stat -c %s "$path")
    sha_ok=0; [ "$got_sha" = "$sha" ] && sha_ok=1
    len_ok=0; [ "$got_len" = "$blen" ] && len_ok=1
    printf '%s\t%s\t%s\t1\n' "$id" "$sha_ok" "$len_ok" >>"$RAW/corpus_verify.tsv"
    { [ "$sha_ok" -eq 1 ] && [ "$len_ok" -eq 1 ]; } || verify_rc=1
done <"$RAW/docs.tsv"
if [ "$verify_rc" -ne 0 ]; then
    echo "phase16-packed-full: corpus verification FAILED; refusing to measure" >&2
    grep -v $'\t1\t1\t1$' "$RAW/corpus_verify.tsv" >&2 || true
    exit 1
fi

now_ms() { echo $(( ${EPOCHREALTIME/./} / 1000 )); }

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
JQ_V=$(jq --version 2>/dev/null)
BASE_STABLE=rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e
cat >"$RAW/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "16.2 — full real100-v1 packed-store storage court (fs vs packed vs SQLite)",
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
  "jq": "$JQ_V",
  "cargo_lock_sha256": "$LOCK_SHA",
  "manifest": "$MANIFEST",
  "manifest_sha256": "$MANIFEST_SHA",
  "schedule": "$SCHEDULE",
  "bin": "$BIN",
  "bin_sha256": "$BIN_SHA",
  "profile": "$PROFILE",
  "limit": $LIMIT,
  "op_timeout_s": $OP_TIMEOUT,
  "env_affecting_semantics": {"LC_ALL": "C", "VOLE_BIN": "ignored under PROFILE=release"}
}
EOF

# House convention keeps environment.json under raw/; the task also lists it at
# the campaign root, so seal both (identical bytes).
cp "$RAW/environment.json" "$CAMPAIGN/environment.json"

cat >"$CAMPAIGN/commands.txt" <<'EOF'
# Phase 16.2 — full real100-v1 packed-store storage court. Never run on the host.
docker compose run --rm --no-TTY doc-baseline bash tools/phase16-packed-full-court.sh
# bounded smoke (LIMIT caps the population):
docker compose run --rm --no-TTY doc-baseline sh -c 'LIMIT=4 bash tools/phase16-packed-full-court.sh'
EOF

# --- raw table --------------------------------------------------------------
printf 'id\tagency\tfmt\tsclass\tbyte_len\tmanifest_sha\tvenc_rc\tvenc_ms\tving_fs_rc\tving_fs_ms\tving_pack_rc\tving_pack_ms\tfield_fs\tfield_pack\tfield_equal\tfs_bytes\tpack_bytes\ta1_build_rc\ta1_build_ms\ta1_bytes\ta1_sidecar_bytes\n' >"$RAW/storage.tsv"

# --- main loop --------------------------------------------------------------
while IFS=$'\t' read -r id agency fmt path sha blen sclass; do
    d="$WORK/$id"
    rm -rf "$d"; mkdir -p "$d"
    echo "== $id ($fmt, $sclass, $blen B) ==" >&2

    venc_rc=1; venc=0
    t0=$(now_ms)
    timeout "$OP_TIMEOUT" "$BIN" encode "$path" "$d/doc.voldoc" >"$d/encode.json" 2>"$d/encode.err"; venc_rc=$?
    t1=$(now_ms); venc=$(( t1 - t0 ))

    fs_rc=-1; fs_ms=-1; pk_rc=-1; pk_ms=-1
    field_fs=""; field_pk=""; field_equal=0
    fs_bytes=0; pk_bytes=0
    if [ "$venc_rc" -eq 0 ]; then
        t0=$(now_ms)
        timeout "$OP_TIMEOUT" "$BIN" field-ingest "$d/doc.voldoc" --store "$d/fs" \
            >"$d/fs.ingest.json" 2>"$d/fs.ingest.err"; fs_rc=$?
        t1=$(now_ms); fs_ms=$(( t1 - t0 ))

        t0=$(now_ms)
        timeout "$OP_TIMEOUT" "$BIN" field-ingest "$d/doc.voldoc" --store "$d/pack" --packed \
            >"$d/pack.ingest.json" 2>"$d/pack.ingest.err"; pk_rc=$?
        t1=$(now_ms); pk_ms=$(( t1 - t0 ))

        [ "$fs_rc" -eq 0 ] && field_fs=$(jq -r '.field // empty' "$d/fs.ingest.json" 2>/dev/null)
        [ "$pk_rc" -eq 0 ] && field_pk=$(jq -r '.field // empty' "$d/pack.ingest.json" 2>/dev/null)
        if [ -n "$field_fs" ] && [ "$field_fs" = "$field_pk" ]; then field_equal=1
        elif [ -n "$field_fs" ] || [ -n "$field_pk" ]; then
            echo "   WARNING: field id mismatch fs=$field_fs packed=$field_pk" >&2
        fi

        # Persistent footprint only for a SUCCESSFUL ingest (a failed ingest's
        # partial directory is not a valid footprint).
        [ "$fs_rc" -eq 0 ] && fs_bytes=$(du -sb "$d/fs" 2>/dev/null | awk '{print $1+0}')
        [ "$pk_rc" -eq 0 ] && pk_bytes=$(du -sb "$d/pack" 2>/dev/null | awk '{print $1+0}')
    fi

    # A1 SQLite build is independent of the VOLE encode outcome.
    t0=$(now_ms)
    timeout "$OP_TIMEOUT" python3 "$BASE" build --format "$fmt" --source "$path" \
        --db "$d/a1.db" --metrics "$d/a1.build.metrics.json" >"$d/a1.build.json" 2>"$d/a1.build.err"; a1_rc=$?
    t1=$(now_ms); a1_ms=$(( t1 - t0 ))
    a1_bytes=0; a1_side=0
    if [ "$a1_rc" -eq 0 ]; then
        a1_bytes=$(du -sb "$d/a1.db" 2>/dev/null | awk '{s+=$1} END{print s+0}')
        a1_side=$(du -sb "$d/a1.db-wal" "$d/a1.db-shm" 2>/dev/null | awk '{s+=$1} END{print s+0}')
    fi

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$id" "$agency" "$fmt" "$sclass" "$blen" "$sha" \
        "$venc_rc" "$venc" "$fs_rc" "$fs_ms" "$pk_rc" "$pk_ms" \
        "$field_fs" "$field_pk" "$field_equal" "$fs_bytes" "$pk_bytes" \
        "$a1_rc" "$a1_ms" "$a1_bytes" "$a1_side" >>"$RAW/storage.tsv"

    # Keep the small per-document JSON/err evidence; drop the bulk (voldoc,
    # stores, a1.db) so the campaign stays bounded.
    ed="$RAW/perdoc/$id"; mkdir -p "$ed"
    for f in "$d"/*.json "$d"/*.err; do [ -f "$f" ] && cp "$f" "$ed/"; done
    rm -rf "$d"
done <"$RAW/docs.tsv"

# --- aggregate + seal -------------------------------------------------------
echo "-- aggregating" >&2
python3 tools/fixtures/phase16-packed-full.py "$RAW" "$CAMPAIGN" | tee "$RAW/summary.txt"

cat >"$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "16.2 — full real100-v1 packed-store storage court (fs vs packed vs SQLite)",
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
  "limit": $LIMIT,
  "court": "tools/phase16-packed-full-court.sh",
  "aggregator": "tools/fixtures/phase16-packed-full.py",
  "accounting": "fs/pack = du -sb of the store dir; a1 = du -sb of the .db (matches tools/real100-court.sh, so comparable to the 1.377x baseline)",
  "established_baseline": "evidence/campaigns/2026-10-07-real100-release-baseline-866f489: common-success 95 docs, fs/a1 = 1.3767x",
  "populations": $(jq -c '.populations' "$RAW/aggregate.json"),
  "whole_population": $(jq -c '.whole_population' "$RAW/aggregate.json"),
  "common_success_all": $(jq -c '.common_success_all.overall' "$RAW/aggregate.json"),
  "common_success_fs_a1": $(jq -c '.common_success_fs_a1.overall' "$RAW/aggregate.json"),
  "verdict": $(jq -c '.verdict' "$RAW/aggregate.json"),
  "rc_codes": "0 success; 124 timeout (OP_TIMEOUT); 137 SIGKILL/OOM under the 6g cap; -1 not run (encode failed); other = the tool's own error code"
}
EOF

chown -R 1000:1000 "$CAMPAIGN" "$WORK" 2>/dev/null || true

echo "phase16.2 packed-full court: done — $CAMPAIGN"
