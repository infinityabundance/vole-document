#!/usr/bin/env bash
# Phase 19.3 — the NEW direct build path over the FULL frozen `real100-v1`.
#
# ## Why this court exists
#
# The equal-contract build result (Phase 18.5/19.1: build 0.82×, paired 0.18×;
# storage 0.53×) was measured on a 12-document subset. Before ANY population-style
# claim, the new build architecture — a single-process `field-build` that
# serializes one fixed `runtime` program directly from source into a packed field
# store (no candidate search, no encode→ingest round trip) — must be shown to
# behave ROBUSTLY across all 100 real documents: which builds succeed, how long
# they take, how much memory they peak at, how many bytes/files they persist, and
# whether `materialize --exact` still closes byte-exactly.
#
# ## What it does, per document
#
#   * `field-build "$src" --store "$d/vstore" --profile runtime --packed
#      --sync=batch` under `/usr/bin/time -v` (wall + peak RSS), via `timeout`
#      (rc 0 ok / 124 timeout / 137 SIGKILL-OOM / other), guarded by the same
#      build-fail + docx/epub preflight as `tools/real100-court.sh`;
#   * persistent bytes as the sum of REGULAR-FILE sizes only (never `du -sb`:
#      the bind-mounted host fs inflates directory inodes), the regular-file and
#      directory counts, and the field id;
#   * a small cold-observation coverage set with the SAME selector grammar as the
#      frozen court (`--page 1 --kind text` for pdf, `--block 0 --kind text` for
#      docx/epub, plus `--metadata --kind metadata`), recorded answered/declined;
#   * `materialize --exact` compared against the manifest length + SHA-256.
#
# ## Accounting / comparison
#
# The OLD path's bytes in `2026-10-07-real100-release-baseline-866f489` and
# `2026-10-08-phase16-packed-full-0b21928` were measured with `du -sb`, so they
# are NOT compared byte-for-byte here; the aggregator compares against the
# file-size-corrected numbers in `2026-10-08-phase16-storage-correction-2978e1d`
# (same accounting: regular-file bytes only) and says so in the SUMMARY.
#
# ## Lane
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase19-real100-direct-court.sh
#   # bounded / explicit population:
#   docker compose run --rm --no-TTY doc-baseline sh -c 'LIMIT=2 bash tools/phase19-real100-direct-court.sh'
set -uo pipefail

cd /work
export LC_ALL=C

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=${CAMPAIGN:-evidence/campaigns/${STAMP}-phase19-real100-direct-${SHA}}
RAW=$CAMPAIGN/raw
WORK=${WORK:-evidence/scratch/phase19-real100-direct-work}
mkdir -p "$RAW" "$WORK"

MANIFEST=${MANIFEST:-real100-v1/manifest.tsv}
CORPUS=${CORPUS:-real100-v1/documents}
SCHEDULE=$RAW/schedule.json
OP_TIMEOUT=${OP_TIMEOUT:-180}
LIMIT=${LIMIT:-0}                 # 0 = the full frozen population; >0 = first N (smoke)
IDS=${IDS:-}                      # optional space-separated id restriction

PROFILE=${PROFILE:-release}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-${VOLE_BIN:-target/debug/vole-document}} ;;
esac
# NB: under PROFILE=release `VOLE_BIN` (set by the compose service) is NOT
# consulted — the release binary at target/release/vole-document is measured.

echo "== phase19.3 real100 direct-build robustness court ==" >&2
echo "-- building ($PROFILE) binary" >&2
# shellcheck disable=SC2086
if ! cargo build $BUILD_ARGS >&2; then
    echo "phase19-real100-direct: build FAILED ($BUILD_ARGS); refusing to measure" >&2
    exit 1
fi
[ -x "$BIN" ] || { echo "phase19-real100-direct: $BIN missing after build" >&2; exit 1; }

# Preflight: a stale/mis-featured binary silently answers rc=6 for every
# DOCX/EPUB op; fail loudly instead (same guard as tools/real100-court.sh).
PREFLIGHT=""
for cand in real100-v1/documents/nist/docx/*.docx real100-v1/documents/nasa/epub/*.epub; do
    [ -f "$cand" ] && { PREFLIGHT="$cand"; break; }
done
if [ -n "$PREFLIGHT" ]; then
    expect=$(case "$PREFLIGHT" in *.docx) echo docx ;; *) echo epub ;; esac)
    if ! "$BIN" capabilities "$PREFLIGHT" 2>/dev/null | grep -q "\"adapter\":\"$expect\""; then
        echo "phase19-real100-direct: preflight failed — $BIN does not support $expect; refusing to measure" >&2
        exit 1
    fi
fi

echo "-- verifying the frozen corpus (SHA-256 + length + format)" >&2
if ! sh tools/realcorpus/verify.sh --corpus real100-v1 >"$RAW/verify.txt" 2>&1; then
    echo "phase19-real100-direct: corpus verification FAILED; refusing to measure" >&2
    tail -20 "$RAW/verify.txt" >&2
    exit 1
fi

echo "-- building the pre-registered schedule" >&2
python3 tools/fixtures/real100-schedule.py "$MANIFEST" "$CORPUS" "$SCHEDULE" \
    >"$RAW/schedule.meta.json" 2>"$RAW/schedule.err"

# id agency fmt path sha256 byte_len size_class
jq -r '.documents[] | [.id,.agency,.format,.path,.sha256,(.byte_len|tostring),.size_class] | @tsv' \
    "$SCHEDULE" >"$RAW/docs_all.tsv"

if [ -n "$IDS" ]; then
    cp "$RAW/docs_all.tsv" "$RAW/docs.tsv"
    : >"$RAW/docs.filtered.tsv"
    for i in $IDS; do awk -v idd="$i" -F'\t' '$1==idd' "$RAW/docs.tsv" >>"$RAW/docs.filtered.tsv"; done
    mv "$RAW/docs.filtered.tsv" "$RAW/docs.tsv"
else
    cp "$RAW/docs_all.tsv" "$RAW/docs.tsv"
fi
if [ "$LIMIT" -gt 0 ]; then
    head -n "$LIMIT" "$RAW/docs.tsv" >"$RAW/docs.limit.tsv" && mv "$RAW/docs.limit.tsv" "$RAW/docs.tsv"
fi
echo "-- population: $(wc -l <"$RAW/docs.tsv" | tr -d ' ') documents" >&2

now_ms() { echo $(( ${EPOCHREALTIME/./} / 1000 )); }
rss_kb() { awk -F': ' '/Maximum resident set size/{print $2}' "$1" 2>/dev/null; }
# Persistent bytes = sum of REGULAR-FILE sizes; directories contribute 0.
# `du -sb` is never used (bind-mount directory-inode inflation).
fbytes() { find "$1" -type f -printf '%s\n' 2>/dev/null | awk '{s+=$1} END{print s+0}'; }
ffiles() { find "$1" -type f 2>/dev/null | wc -l | tr -d ' '; }
fdirs()  { find "$1" -type d 2>/dev/null | wc -l | tr -d ' '; }

# --- raw tables -------------------------------------------------------------
printf 'id\tagency\tfmt\tsclass\tbyte_len\tmanifest_sha\tbuild_rc\tbuild_ms\tbuild_rss_kb\tfield\tstore_files\tstore_dirs\tstore_bytes\texact_rc\texact_ok\texact_ms\texact_sha\texact_len\n' >"$RAW/build.tsv"
printf 'id\tfmt\tsclass\tobs\trc\tanswered\n' >"$RAW/coverage.tsv"

# --- main loop --------------------------------------------------------------
while IFS=$'\t' read -r id agency fmt path sha blen sclass; do
    [ -n "$id" ] || continue
    d="$WORK/$id"; rm -rf "$d"; mkdir -p "$d"
    echo "== $id ($fmt, $sclass, $blen B) ==" >&2

    # ---- direct build: ONE process, fixed runtime profile, packed, batch sync ----
    build_rc=1; build_ms=0; rss=0; field=""; nf=0; nd=0; nb=0
    t0=$(now_ms)
    timeout "$OP_TIMEOUT" /usr/bin/time -v "$BIN" field-build "$path" \
        --store "$d/vstore" --profile runtime --packed --sync=batch \
        >"$d/build.json" 2>"$d/build.time"; build_rc=$?
    t1=$(now_ms); build_ms=$(( t1 - t0 ))
    rss=$(rss_kb "$d/build.time")
    if [ "$build_rc" -eq 0 ]; then
        field=$(jq -r '.ingest.field // empty' "$d/build.json" 2>/dev/null)
        nf=$(ffiles "$d/vstore"); nd=$(fdirs "$d/vstore"); nb=$(fbytes "$d/vstore")
    fi
    echo "   field-build rc=$build_rc ms=$build_ms rss_kb=${rss:-0} field=${field:-none} files=$nf dirs=$nd bytes=$nb" >&2

    # ---- cold-observation coverage set (same grammar as tools/real100-court.sh) ----
    for obs in text metadata; do
        case "$obs:$fmt" in
            text:pdf) args="--page 1 --kind text" ;;
            text:*)   args="--block 0 --kind text" ;;
            metadata:*) args="--metadata --kind metadata" ;;
        esac
        if [ -n "$field" ]; then
            # shellcheck disable=SC2086
            timeout "$OP_TIMEOUT" "$BIN" observe $args --store "$d/vstore" \
                --field "$field" --packed >"$d/cov.$obs.json" 2>"$d/cov.$obs.err"; crc=$?
            [ "$crc" -eq 0 ] && ans=1 || ans=0
        else
            crc=-1; ans=0
        fi
        printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "$sclass" "$obs" "$crc" "$ans" >>"$RAW/coverage.tsv"
    done

    # ---- exact original closure: length + SHA-256 + (implicit) byte compare ----
    exact_rc=3; exact_ok=0; exact_ms=0; esha=""; elen=0
    if [ -n "$field" ]; then
        t0=$(now_ms)
        timeout "$OP_TIMEOUT" "$BIN" materialize --store "$d/vstore" --field "$field" \
            --exact --output "$d/exact.bin" --packed >/dev/null 2>"$d/exact.err"; exact_rc=$?
        t1=$(now_ms); exact_ms=$(( t1 - t0 ))
        esha=$(sha256sum "$d/exact.bin" 2>/dev/null | cut -d' ' -f1)
        elen=$(stat -c %s "$d/exact.bin" 2>/dev/null || echo 0)
        [ "$esha" = "$sha" ] && [ "$elen" = "$blen" ] && exact_ok=1
    fi

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$id" "$agency" "$fmt" "$sclass" "$blen" "$sha" \
        "$build_rc" "$build_ms" "${rss:-0}" "$field" "$nf" "$nd" "$nb" \
        "$exact_rc" "$exact_ok" "$exact_ms" "$esha" "$elen" >>"$RAW/build.tsv"

    # keep small evidence only; drop the (large) store + materialized bytes
    ed="$RAW/perdoc/$id"; mkdir -p "$ed"
    for f in "$d"/*.json "$d"/*.err "$d"/*.time; do [ -f "$f" ] && cp "$f" "$ed/"; done
    rm -rf "$d"
done <"$RAW/docs.tsv"

# --- provenance -------------------------------------------------------------
RUSTC_V=$(rustc --version 2>/dev/null | sed 's/"/\\"/g')
CARGO_V=$(cargo --version 2>/dev/null | sed 's/"/\\"/g')
ARCH=$(uname -m)
COMMIT=$(git rev-parse HEAD 2>/dev/null || echo unknown)
DIRTY=$(git --no-optional-locks status --porcelain 2>/dev/null | tr '\n' ';' | sed 's/"/\\"/g')
LOCK_SHA=$(sha256sum Cargo.lock 2>/dev/null | cut -d' ' -f1)
MANIFEST_SHA=$(sha256sum "$MANIFEST" | cut -d' ' -f1)
BIN_SHA=$(sha256sum "$BIN" 2>/dev/null | cut -d' ' -f1)
PY_V=$(python3 --version 2>&1)
BASE_STABLE=rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e
cat >"$RAW/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "19.3 — NEW direct build path (field-build --packed --sync=batch) over the FULL frozen real100-v1",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_commit": "$COMMIT",
  "git_dirty": "$DIRTY",
  "arch": "$ARCH",
  "service": "doc-baseline",
  "base_image": "$BASE_STABLE",
  "rustc": "$RUSTC_V",
  "cargo": "$CARGO_V",
  "python": "$PY_V",
  "cargo_lock_sha256": "$LOCK_SHA",
  "manifest": "$MANIFEST",
  "manifest_sha256": "$MANIFEST_SHA",
  "bin": "$BIN",
  "bin_sha256": "$BIN_SHA",
  "profile": "$PROFILE",
  "limit": $LIMIT,
  "ids": "$IDS",
  "op_timeout_s": $OP_TIMEOUT,
  "build_cmd": "field-build SRC --store DIR --profile runtime --packed --sync=batch (under /usr/bin/time -v)",
  "coverage_obs": "text (--page 1 / --block 0 --kind text) + metadata (--metadata --kind metadata), via observe --packed",
  "accounting": "persistent bytes = sum of regular-file sizes (find -printf %s); dirs = 0; du -sb never used",
  "env_affecting_semantics": {"LC_ALL": "C", "VOLE_BIN": "ignored under PROFILE=release"}
}
EOF
cp "$RAW/environment.json" "$CAMPAIGN/environment.json"

cat >"$CAMPAIGN/commands.txt" <<'EOF'
# Phase 19.3 — the NEW direct build path (field-build --packed --sync=batch) over
# the FULL frozen real100-v1. Never run on the host.
docker compose run --rm --no-TTY doc-baseline bash tools/phase19-real100-direct-court.sh
# smoke (first N of the schedule):
docker compose run --rm --no-TTY doc-baseline sh -c 'LIMIT=2 bash tools/phase19-real100-direct-court.sh'
# explicit population:
docker compose run --rm --no-TTY doc-baseline sh -c 'IDS="nist-pdf-0017 nist-docx-0005" bash tools/phase19-real100-direct-court.sh'
EOF

# --- aggregate + seal -------------------------------------------------------
echo "-- aggregating" >&2
OLD_STORAGE=${OLD_STORAGE:-evidence/campaigns/2026-10-08-phase16-storage-correction-2978e1d/raw/storage.tsv}
OLD_AGG=${OLD_AGG:-evidence/campaigns/2026-10-08-phase16-packed-full-0b21928/raw/aggregate.json}
python3 tools/fixtures/phase19-real100-direct.py "$RAW" "$CAMPAIGN" "$OLD_STORAGE" "$OLD_AGG" | tee "$RAW/summary.txt"

cat >"$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "19.3 — NEW direct build path (field-build --packed --sync=batch) over the FULL frozen real100-v1",
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
  "cargo_lock_sha256": "$LOCK_SHA",
  "manifest": "$MANIFEST",
  "manifest_sha256": "$MANIFEST_SHA",
  "documents": $(wc -l <"$RAW/docs.tsv" | tr -d ' '),
  "op_timeout_s": $OP_TIMEOUT,
  "court": "tools/phase19-real100-direct-court.sh",
  "aggregator": "tools/fixtures/phase19-real100-direct.py",
  "schedule": "tools/fixtures/real100-schedule.py (frozen, format-only op set)",
  "build_cmd": "field-build SRC --store DIR --profile runtime --packed --sync=batch",
  "accounting": "persistent bytes = sum of regular-file sizes (find -printf %s); directories contribute 0; du -sb is never used",
  "old_path_comparison": "the old path's bytes in 2026-10-07-real100-release-baseline-866f489 and 2026-10-08-phase16-packed-full-0b21928 were measured with du -sb (directory-inode inflated) and are NOT compared byte-for-byte; the aggregator compares against the file-size-corrected numbers in 2026-10-08-phase16-storage-correction-2978e1d (regular-file bytes only), and reads the old build-success count (encode 97/100) from 2026-10-08-phase16-packed-full-0b21928/raw/aggregate.json",
  "rc_codes": "0 ok; 3 decline / not-run; 6 unsupported-feature; 20 InvalidPackageStructure; 124 timeout; 137 SIGKILL-OOM; other = the raw rc",
  "exactness": "VOLE materialize --exact --packed checked against the manifest byte_len + sha256"
}
EOF

chown -R 1000:1000 "$CAMPAIGN" "$WORK" 2>/dev/null || true
echo "phase19.3 real100 direct-build robustness court: done — $CAMPAIGN"
