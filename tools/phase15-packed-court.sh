#!/usr/bin/env bash
# phase15-packed court (Phase 15.3) — packed seed store vs the one-file-per-node
# reference store.
#
# ## What this measures
#
# For a documented SUBSET of the frozen `real100-v1` corpus it ingests the SAME
# descriptor twice and compares the two persisted seed substrates:
#
#   fs    = `field-ingest INPUT --store DIR`            (one file per seed node)
#   packed = `field-ingest INPUT --store DIR --packed`  (append-only `fieldpack/`
#           segments + an immutable per-segment index; Phase 15.3)
#
# Both stores share `descriptor/`, `field/`, `index/`, and `cache/`; only the
# seed namespace differs (`seed/` vs `fieldpack/`). For each document it records:
#
#   * persistent bytes (`du -sb`) and file count (`find -type f | wc -l`) for
#     both stores, so the packed/fs byte ratio and file-count ratio are honest;
#   * wall ms of a COLD single observation against each store, in a fresh process
#     (`observe --page 1 --kind text` for pdf, `--block 0 --kind text` for
#     docx/epub — the same selector grammar `tools/real100-court.sh` uses);
#   * `materialize --exact` from each store, SHA-256 + byte length checked against
#     the manifest `sha256`/`byte_len` — the exactness witness for BOTH backends;
#   * the two field ids, asserted equal (the descriptor is identical, so the
#     `FieldId` must be too); a mismatch is recorded, never hidden.
#
# On ONE representative document it additionally runs `strace -c -f` around one
# cold observation against each store and records the syscall summary. The
# `doc-baseline` lane has `strace` but NOT `perf`.
#
# ## What this does NOT claim
#
# It does NOT assert that packed is better. The subset is small and selected for
# format/size-class spread, not for representativeness of the full 100-document
# population — this is a SUBSET, not the frozen frontier population. It makes no
# claim about content (real documents have unknown content) beyond stored bytes,
# file counts, wall cost, and byte-exact reconstruction.
#
# ## Subset selection (documented)
#
# `manifest.tsv` is read header-aware. Unless `IDS` is given, one document is
# chosen per `(format, size_class)` stratum — the SMALLEST member of each
# stratum, so the run stays bounded while still spanning pdf/docx/epub and every
# size class present. Strata are ordered by size-class rank (small -> large) and
# format, then `head -n LIMIT`. With the current manifest this is exactly 12
# strata, so `LIMIT=12` (default) selects all 12; the `>100MiB` stratum contains
# only large PDFs and is therefore the slowest member of the subset.
#
# ## Layout
#
# Every candidate service is a hard-capped compose lane; nothing runs on the
# host. This court runs in the digest-pinned `doc-baseline` service:
#   rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e
# `doc-baseline` is capped at mem_limit == memswap_limit == 6g (no zram
# evasion), pids_limit 4096, cpus 8.
#
# ## Exact invocation (commands.txt-style; never the host)
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase15-packed-court.sh
#   # bounded smoke:
#   docker compose run --rm --no-TTY doc-baseline \
#     sh -c 'LIMIT=4 bash tools/phase15-packed-court.sh'
#   # explicit ids:
#   docker compose run --rm --no-TTY doc-baseline \
#     sh -c 'IDS="nasa-pdf-0001 nist-docx-0001 nasa-epub-0001" bash tools/phase15-packed-court.sh'
set -uo pipefail

cd /work
export LC_ALL=C

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=evidence/campaigns/${STAMP}-phase15-packed-${SHA}
RAW=$CAMPAIGN/raw
WORK=${WORK:-evidence/scratch/phase15-packed}
mkdir -p "$RAW" "$WORK"

MANIFEST=${MANIFEST:-real100-v1/manifest.tsv}
CORPUS=${CORPUS:-real100-v1/documents}
OP_TIMEOUT=${OP_TIMEOUT:-180}
LIMIT=${LIMIT:-12}          # default: exactly the 12 (format, size_class) strata
IDS=${IDS:-}                # space/comma-separated ids override the stratum pick
REP_ID=${REP_ID:-}          # document to strace; empty -> first selected
# Build profile. The fs-vs-packed comparison is a same-binary ratio, so the
# optimization level cancels; debug keeps the court cheap. `PROFILE=release`
# measures `target/release/vole-document`. An explicit `BIN` overrides.
PROFILE=${PROFILE:-debug}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-${VOLE_BIN:-target/debug/vole-document}} ;;
esac

echo "== phase15-packed court (Phase 15.3) ==" >&2
echo "-- building ($PROFILE) binary" >&2
# shellcheck disable=SC2086
if ! cargo build $BUILD_ARGS >&2; then
    echo "phase15-packed: build FAILED ($BUILD_ARGS); refusing to measure" >&2
    exit 1
fi
# Preflight: a stale/mis-featured binary silently declines every DOCX/EPUB op.
# `--packed` and the docx/epub adapters must both be present or the court is
# meaningless; fail loudly instead of measuring a binary that cannot answer.
PREFLIGHT=""
for cand in real100-v1/documents/nist/docx/*.docx real100-v1/documents/nasa/epub/*.epub; do
    [ -f "$cand" ] && { PREFLIGHT="$cand"; break; }
done
if [ -n "$PREFLIGHT" ]; then
    expect=$(case "$PREFLIGHT" in *.docx) echo docx ;; *) echo epub ;; esac)
    if ! "$BIN" capabilities "$PREFLIGHT" 2>/dev/null | grep -q "\"adapter\":\"$expect\""; then
        echo "phase15-packed: preflight failed — $BIN does not support $expect; refusing to measure" >&2
        exit 1
    fi
fi

# Selector grammar, mirroring tools/real100-court.sh's v_argv for text_once.
v_argv() { # fmt -> echo the observe selector flags for a single cold text read
  case "$1" in
    pdf) echo "observe --page 1 --kind text" ;;
    *)   echo "observe --block 0 --kind text" ;;
  esac
}

# --- subset selection (documented above) -----------------------------------
# Emits: id<TAB>agency<TAB>fmt<TAB>sclass<TAB>blen<TAB>sha256
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
        printf "%d\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n", rank[scl]+0, fmt, f[h["id"]], f[h["agency"]], fmt, scl, f[h["byte_len"]], f[h["sha256"]];
      }
    }
  ' "$MANIFEST" | sort -k1,1n -k2,2 | cut -f3- | head -n "$LIMIT"
}

now_ms() { echo $(( ${EPOCHREALTIME/./} / 1000 )); }

# --- raw table headers ------------------------------------------------------
printf 'id\tagency\tfmt\tsclass\tbyte_len\tencode_rc\tfield_fs\tfield_packed\tfield_equal\tfs_bytes\tpacked_bytes\tfs_files\tpacked_files\tfs_cold_ms\tpacked_cold_ms\tfs_cold_rc\tpacked_cold_rc\tfs_exact_ok\tpacked_exact_ok\tmanifest_sha\n' >"$RAW/packed.tsv"
printf 'id\tbackend\tseconds\tcalls\n' >"$RAW/strace.tsv"

echo "-- subset ($(select_docs | wc -l) docs; LIMIT=$LIMIT${IDS:+; IDS=$IDS})" >&2
select_docs >"$RAW/subset.tsv"

# --- provenance -------------------------------------------------------------
RUSTC_V=$(rustc --version 2>/dev/null | sed 's/^rustc //')
CARGO_V=$(cargo --version 2>/dev/null | sed 's/^cargo //')
ARCH=$(uname -m)
COMMIT=$(git rev-parse HEAD 2>/dev/null || echo unknown)
DIRTY=$(git --no-optional-locks status --porcelain 2>/dev/null | head -40 | tr '\n' ';')
LOCK_SHA=$(sha256sum Cargo.lock 2>/dev/null | cut -d' ' -f1)
STRACE_V=$(strace --version 2>&1 | head -1 | sed 's/^strace -- version //')
JQ_V=$(jq --version 2>/dev/null)
IMAGE_ID=${HOST_IMAGE_ID:-unrecorded}
BASE_STABLE=rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e
MANIFEST_SHA=$(sha256sum "$MANIFEST" | cut -d' ' -f1)
cat >"$RAW/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "15.3 — packed seed store vs one-file-per-node reference",
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
  "strace": "$STRACE_V",
  "cargo_lock_sha256": "$LOCK_SHA",
  "manifest": "$MANIFEST",
  "manifest_sha256": "$MANIFEST_SHA",
  "bin": "$BIN",
  "profile": "$PROFILE",
  "op_timeout_s": $OP_TIMEOUT,
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF
cat >"$CAMPAIGN/commands.txt" <<'EOF'
# Phase 15.3 — packed seed store vs one-file-per-node reference (SUBSET).
# Never run on the host; every lane is a hard-capped compose service.
docker compose run --rm --no-TTY doc-baseline bash tools/phase15-packed-court.sh
# bounded smoke / explicit ids (override):
docker compose run --rm --no-TTY doc-baseline sh -c 'LIMIT=4 bash tools/phase15-packed-court.sh'
docker compose run --rm --no-TTY doc-baseline sh -c 'IDS="nasa-pdf-0001 nist-docx-0001 nasa-epub-0001" bash tools/phase15-packed-court.sh'
EOF

# --- strace helper ----------------------------------------------------------
# strace -c writes a summary table; the `total` line is `% time seconds ...
# calls total`, so seconds = $2 and calls = the field before `total`.
strace_total() { awk '/total/{print $2"\t"$(NF-2); exit}' "$1"; }

FIRST_ID=""
# --- main loop --------------------------------------------------------------
while IFS=$'\t' read -r id agency fmt sclass blen sha; do
    [ -z "$FIRST_ID" ] && FIRST_ID=$id
    d="$WORK/$id"
    rm -rf "$d"; mkdir -p "$d"
    src="$CORPUS/$agency/$fmt/$id.$fmt"
    echo "== $id ($fmt, $sclass, $blen B) ==" >&2

    if [ ! -f "$src" ]; then
        echo "   missing source $src" >&2
        printf '%s\t%s\t%s\t%s\t%s\t3\t\t\t0\t0\t0\t0\t0\t0\t0\t3\t3\t0\t0\t%s\n' \
            "$id" "$agency" "$fmt" "$sclass" "$blen" "$sha" >>"$RAW/packed.tsv"
        continue
    fi

    # ---- encode once -------------------------------------------------------
    t0=$(now_ms)
    timeout "$OP_TIMEOUT" "$BIN" encode "$src" "$d/doc.voldoc" >"$d/encode.json" 2>"$d/encode.err"; enc_rc=$?
    t1=$(now_ms)

    field_fs=""; field_pk=""; field_equal=0
    fs_bytes=0; pk_bytes=0; fs_files=0; pk_files=0
    fs_cold=-1; pk_cold=-1; fs_cold_rc=3; pk_cold_rc=3
    fs_ok=0; pk_ok=0

    if [ "$enc_rc" -eq 0 ]; then
        # ---- ingest into BOTH stores (identical descriptor) ----------------
        timeout "$OP_TIMEOUT" "$BIN" field-ingest "$d/doc.voldoc" --store "$d/fs" \
            >"$d/fs.ingest.json" 2>"$d/fs.ingest.err"
        timeout "$OP_TIMEOUT" "$BIN" field-ingest "$d/doc.voldoc" --store "$d/pack" --packed \
            >"$d/pack.ingest.json" 2>"$d/pack.ingest.err"
        field_fs=$(jq -r '.field // empty' "$d/fs.ingest.json" 2>/dev/null)
        field_pk=$(jq -r '.field // empty' "$d/pack.ingest.json" 2>/dev/null)
        if [ -n "$field_fs" ] && [ "$field_fs" = "$field_pk" ]; then field_equal=1;
        elif [ -n "$field_fs" ] || [ -n "$field_pk" ]; then
            echo "   WARNING: field id mismatch fs=$field_fs packed=$field_pk" >&2
        fi

        # ---- persistent footprint immediately after ingest -----------------
        fs_bytes=$(du -sb "$d/fs" 2>/dev/null | awk '{print $1+0}')
        pk_bytes=$(du -sb "$d/pack" 2>/dev/null | awk '{print $1+0}')
        fs_files=$(find "$d/fs" -type f 2>/dev/null | wc -l | tr -d ' ')
        pk_files=$(find "$d/pack" -type f 2>/dev/null | wc -l | tr -d ' ')

        argv=$(v_argv "$fmt")

        # ---- cold observation: one fresh process per store -----------------
        if [ -n "$field_fs" ]; then
            # shellcheck disable=SC2086
            t0=$(now_ms); timeout "$OP_TIMEOUT" "$BIN" $argv --store "$d/fs" --field "$field_fs" \
                >"$d/fs.observe.json" 2>"$d/fs.observe.err"; fs_cold_rc=$?; t1=$(now_ms); fs_cold=$(( t1 - t0 ))
            # shellcheck disable=SC2086
            t0=$(now_ms); timeout "$OP_TIMEOUT" "$BIN" $argv --store "$d/pack" --field "$field_pk" --packed \
                >"$d/pack.observe.json" 2>"$d/pack.observe.err"; pk_cold_rc=$?; t1=$(now_ms); pk_cold=$(( t1 - t0 ))
        fi

        # ---- exact reconstruction from BOTH stores -------------------------
        if [ -n "$field_fs" ]; then
            timeout "$OP_TIMEOUT" "$BIN" materialize --store "$d/fs" --field "$field_fs" \
                --exact --output "$d/fs.exact.bin" >"$d/fs.mat.json" 2>"$d/fs.mat.err"
            fs_sha=$(sha256sum "$d/fs.exact.bin" 2>/dev/null | cut -d' ' -f1)
            fs_len=$(stat -c %s "$d/fs.exact.bin" 2>/dev/null || echo 0)
            { [ "$fs_sha" = "$sha" ] && [ "$fs_len" = "$blen" ]; } && fs_ok=1

            timeout "$OP_TIMEOUT" "$BIN" materialize --store "$d/pack" --field "$field_pk" --packed \
                --exact --output "$d/pack.exact.bin" >"$d/pack.mat.json" 2>"$d/pack.mat.err"
            pk_sha=$(sha256sum "$d/pack.exact.bin" 2>/dev/null | cut -d' ' -f1)
            pk_len=$(stat -c %s "$d/pack.exact.bin" 2>/dev/null || echo 0)
            { [ "$pk_sha" = "$sha" ] && [ "$pk_len" = "$blen" ]; } && pk_ok=1
        fi

        # ---- syscall summary on ONE representative document ----------------
        rep=${REP_ID:-$FIRST_ID}
        if [ "$id" = "$rep" ] && [ -n "$field_fs" ]; then
            echo "   strace -c -f on representative $id" >&2
            # shellcheck disable=SC2086
            strace -c -f -o "$RAW/$id.fs.strace.txt" \
                "$BIN" $argv --store "$d/fs" --field "$field_fs" >/dev/null 2>&1
            # shellcheck disable=SC2086
            strace -c -f -o "$RAW/$id.packed.strace.txt" \
                "$BIN" $argv --store "$d/pack" --field "$field_pk" --packed >/dev/null 2>&1
            ts=$(strace_total "$RAW/$id.fs.strace.txt")
            printf '%s\tfs\t%s\t%s\n' "$id" "${ts%%$'\t'*}" "${ts##*$'\t'}" >>"$RAW/strace.tsv"
            ts=$(strace_total "$RAW/$id.packed.strace.txt")
            printf '%s\tpacked\t%s\t%s\n' "$id" "${ts%%$'\t'*}" "${ts##*$'\t'}" >>"$RAW/strace.tsv"
        fi
    fi

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$id" "$agency" "$fmt" "$sclass" "$blen" "$enc_rc" "$field_fs" "$field_pk" "$field_equal" \
        "$fs_bytes" "$pk_bytes" "$fs_files" "$pk_files" "$fs_cold" "$pk_cold" "$fs_cold_rc" "$pk_cold_rc" \
        "$fs_ok" "$pk_ok" "$sha" >>"$RAW/packed.tsv"

    # Keep the small per-document JSON/err evidence; drop the bulk (voldoc,
    # stores, materialized bins) so the campaign stays bounded.
    ed="$RAW/perdoc/$id"; mkdir -p "$ed"
    for f in "$d"/*.json "$d"/*.err; do [ -f "$f" ] && cp "$f" "$ed/"; done
    rm -rf "$d"
done < <(select_docs)

echo "-- aggregating" >&2
python3 tools/fixtures/phase15-packed.py "$RAW" "$CAMPAIGN" | tee "$RAW/summary.txt"

echo "phase15-packed court: done — $CAMPAIGN"
