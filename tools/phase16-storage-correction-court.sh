#!/usr/bin/env bash
# phase16 storage-correction court.
#
# ## Why this court exists
#
# The Phase-15.3 (`tools/phase15-packed-court.sh`) and Phase-16.2
# (`tools/phase16-packed-full-court.sh`) storage courts measured persistent bytes
# with `du -sb`.  `du -sb` is `--apparent-size`; for a DIRECTORY it counts the
# directory inode's own `st_size` (4096 B per directory on this ext4 bind mount)
# in addition to the bytes of the files inside it.  The one-file-per-node `fs`
# store has tens of thousands of directories (`seed/aa/bb/`, `index/aa/bb/`, ...),
# the packed store has a handful, and SQLite is a single file.  A `du -sb` ratio
# therefore mixes real file bytes with directory-inode accounting and inflates
# `fs` far more than `packed` or SQLite.  This court re-measures the SAME two
# comparisons with **file-size-only** accounting:
#
#   file bytes = sum of regular-file `st_size` over the store tree
#                (`find DIR -type f -printf '%s\n'`), directories contribute 0.
#
# It records BOTH accountings side by side (`*_files_bytes` and `*_du_bytes`) plus
# the regular-file count and the directory count, so the distortion is explicit
# and falsifiable, not asserted.
#
# ## What it does, per document
#
#   * `encode` once            -> `$d/doc.voldoc`
#   * `field-ingest ... --store $d/fs`                 one-file-per-node
#   * `field-ingest ... --store $d/pack --packed`      append-only fieldpack
#   * `phase12-baseline.py build`                      A1 SQLite + FTS5
#
# and records, for each substrate, the file-size-only bytes, the `du -sb` bytes,
# the regular-file count and the directory count, plus every exit code.
#
# ## Population
#
# Driven by the frozen `real100-v1` schedule; `IDS_FILE` restricts the run to a
# newline-separated id list (so the union of the 15.3 subset and the 16.2
# common-success population can be measured in ONE run under ONE binary).  This
# court does NOT redefine the populations: the 15.3 subset and the 16.2
# common-success set are supplied as id lists and read back by the aggregator.
#
# ## What it does NOT claim
#
# No content claim beyond stored bytes and exit codes; exact reconstruction is
# established by the 15.3 court.  Ratios are measured, never claimed.
#
# ## Lane
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase16-storage-correction-court.sh
#   # smoke (first N of the schedule):
#   docker compose run --rm --no-TTY doc-baseline sh -c 'LIMIT=2 bash tools/phase16-storage-correction-court.sh'
set -uo pipefail

cd /work
export LC_ALL=C

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=${CAMPAIGN:-evidence/campaigns/${STAMP}-phase16-storage-correction-${SHA}}
RAW=$CAMPAIGN/raw
WORK=${WORK:-evidence/scratch/phase16-storage-correction-work}
mkdir -p "$RAW" "$WORK"

MANIFEST=${MANIFEST:-real100-v1/manifest.tsv}
CORPUS=${CORPUS:-real100-v1/documents}
SCHEDULE=$RAW/schedule.json
BASE=tools/fixtures/phase12-baseline.py
OP_TIMEOUT=${OP_TIMEOUT:-180}
LIMIT=${LIMIT:-0}                 # 0 = all scheduled; >0 = first N (smoke)
IDS_FILE=${IDS_FILE:-}            # newline-separated ids restricting the run
PROFILE=${PROFILE:-release}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-${VOLE_BIN:-target/debug/vole-document}} ;;
esac

echo "== phase16 storage-correction court ==" >&2
echo "-- building ($PROFILE) binary" >&2
# shellcheck disable=SC2086
if ! cargo build $BUILD_ARGS >&2; then
    echo "phase16-storage-correction: build FAILED ($BUILD_ARGS); refusing to measure" >&2
    exit 1
fi
[ -x "$BIN" ] || { echo "phase16-storage-correction: $BIN missing after build" >&2; exit 1; }

PREFLIGHT=""
for cand in real100-v1/documents/nist/docx/*.docx real100-v1/documents/nasa/epub/*.epub; do
    [ -f "$cand" ] && { PREFLIGHT="$cand"; break; }
done
if [ -n "$PREFLIGHT" ]; then
    expect=$(case "$PREFLIGHT" in *.docx) echo docx ;; *) echo epub ;; esac)
    if ! "$BIN" capabilities "$PREFLIGHT" 2>/dev/null | grep -q "\"adapter\":\"$expect\""; then
        echo "phase16-storage-correction: preflight failed — $BIN does not support $expect" >&2
        exit 1
    fi
fi

echo "-- building the pre-registered schedule" >&2
python3 tools/fixtures/real100-schedule.py "$MANIFEST" "$CORPUS" "$SCHEDULE" >"$RAW/schedule.meta.json" 2>"$RAW/schedule.err"

# id agency fmt path sha256 byte_len size_class
jq -r '.documents[] | [.id,.agency,.format,.path,.sha256,(.byte_len|tostring),.size_class] | @tsv' \
    "$SCHEDULE" >"$RAW/docs_all.tsv"

if [ -n "$IDS_FILE" ]; then
    awk 'NR==FNR{keep[$1]=1; next} ($1 in keep)' "$IDS_FILE" "$RAW/docs_all.tsv" >"$RAW/docs.tsv"
    echo "-- IDS_FILE=$IDS_FILE -> $(wc -l <"$RAW/docs.tsv" | tr -d ' ') documents" >&2
else
    cp "$RAW/docs_all.tsv" "$RAW/docs.tsv"
fi
if [ "$LIMIT" -gt 0 ]; then
    head -n "$LIMIT" "$RAW/docs.tsv" >"$RAW/docs.limit.tsv" && mv "$RAW/docs.limit.tsv" "$RAW/docs.tsv"
fi

now_ms() { echo $(( ${EPOCHREALTIME/./} / 1000 )); }

# --- accounting helpers -----------------------------------------------------
# file-size-only: sum of regular-file st_size; directories contribute 0.
fbytes()  { find "$1" -type f -printf '%s\n' 2>/dev/null | awk '{s+=$1} END{print s+0}'; }
nfiles()  { find "$1" -type f 2>/dev/null | wc -l | tr -d ' '; }
ndirs()   { find "$1" -type d 2>/dev/null | wc -l | tr -d ' '; }
dubytes() { du -sb "$1" 2>/dev/null | awk '{print $1+0}'; }

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
  "phase": "16 storage-correction — file-size-only vs du -sb accounting",
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
  "bin": "$BIN",
  "bin_sha256": "$BIN_SHA",
  "profile": "$PROFILE",
  "limit": $LIMIT,
  "ids_file": "$IDS_FILE",
  "op_timeout_s": $OP_TIMEOUT,
  "accounting_files": "find DIR -type f -printf '%s\\n' | awk sum  (regular-file st_size; dirs contribute 0)",
  "accounting_du": "du -sb DIR  (GNU du --apparent-size: counts directory inode st_size too)",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF
cp "$RAW/environment.json" "$CAMPAIGN/environment.json"

cat >"$CAMPAIGN/commands.txt" <<'EOF'
# Phase-16 storage-correction court. Never run on the host.
docker compose run --rm --no-TTY doc-baseline bash tools/phase16-storage-correction-court.sh
# smoke:
docker compose run --rm --no-TTY doc-baseline sh -c 'LIMIT=2 bash tools/phase16-storage-correction-court.sh'
# restricted population (newline-separated ids):
docker compose run --rm --no-TTY doc-baseline sh -c 'IDS_FILE=evidence/scratch/phase16-storage-correction/union96.txt bash tools/phase16-storage-correction-court.sh'
EOF

# --- raw table --------------------------------------------------------------
printf 'id\tagency\tfmt\tsclass\tbyte_len\tmanifest_sha\tvenc_rc\tvenc_ms\tving_fs_rc\tving_fs_ms\tving_pack_rc\tving_pack_ms\tfield_fs\tfield_pack\tfield_equal\tfs_files_bytes\tfs_du_bytes\tfs_files\tfs_dirs\tpack_files_bytes\tpack_du_bytes\tpack_files\tpack_dirs\ta1_build_rc\ta1_build_ms\ta1_files_bytes\ta1_du_bytes\ta1_side_bytes\n' >"$RAW/storage.tsv"

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
    fs_fb=0; fs_db=0; fs_nf=0; fs_nd=0
    pk_fb=0; pk_db=0; pk_nf=0; pk_nd=0
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

        # Persistent footprint for a SUCCESSFUL ingest only, in BOTH accountings.
        if [ "$fs_rc" -eq 0 ]; then
            fs_fb=$(fbytes "$d/fs"); fs_db=$(dubytes "$d/fs")
            fs_nf=$(nfiles "$d/fs"); fs_nd=$(ndirs "$d/fs")
        fi
        if [ "$pk_rc" -eq 0 ]; then
            pk_fb=$(fbytes "$d/pack"); pk_db=$(dubytes "$d/pack")
            pk_nf=$(nfiles "$d/pack"); pk_nd=$(ndirs "$d/pack")
        fi
    fi

    # A1 SQLite build is independent of the VOLE encode outcome.
    t0=$(now_ms)
    timeout "$OP_TIMEOUT" python3 "$BASE" build --format "$fmt" --source "$path" \
        --db "$d/a1.db" --metrics "$d/a1.build.metrics.json" >"$d/a1.build.json" 2>"$d/a1.build.err"; a1_rc=$?
    t1=$(now_ms); a1_ms=$(( t1 - t0 ))
    a1_fb=0; a1_db=0; a1_side=0
    if [ "$a1_rc" -eq 0 ]; then
        a1_fb=$(fbytes "$d/a1.db")
        a1_db=$(dubytes "$d/a1.db")
        for s in "$d/a1.db-wal" "$d/a1.db-shm"; do
            [ -f "$s" ] && a1_side=$(( a1_side + $(stat -c %s "$s") ))
        done
    fi

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$id" "$agency" "$fmt" "$sclass" "$blen" "$sha" \
        "$venc_rc" "$venc" "$fs_rc" "$fs_ms" "$pk_rc" "$pk_ms" \
        "$field_fs" "$field_pk" "$field_equal" \
        "$fs_fb" "$fs_db" "$fs_nf" "$fs_nd" \
        "$pk_fb" "$pk_db" "$pk_nf" "$pk_nd" \
        "$a1_rc" "$a1_ms" "$a1_fb" "$a1_db" "$a1_side" >>"$RAW/storage.tsv"

    ed="$RAW/perdoc/$id"; mkdir -p "$ed"
    for f in "$d"/*.json "$d"/*.err; do [ -f "$f" ] && cp "$f" "$ed/"; done
    rm -rf "$d"
done <"$RAW/docs.tsv"

echo "-- aggregating" >&2
python3 tools/fixtures/phase16-storage-correction.py "$RAW" "$CAMPAIGN" \
    "${SUBSET_IDS_FILE:-}" "${COMMON_IDS_FILE:-}" "${COMMON_LABEL:-}" | tee "$RAW/summary.txt"

chown -R 1000:1000 "$CAMPAIGN" "$WORK" 2>/dev/null || true
echo "phase16 storage-correction court: done — $CAMPAIGN"
