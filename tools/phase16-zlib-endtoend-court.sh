#!/usr/bin/env bash
# Phase 16.1 — `zlib-rs` inflate backend, END-TO-END impact court.
#
# ## Question
#
# Phase 15.5's ablation measured inflate *alone*: `zlib-rs` decoded the real
# `real100-v1` members 1.58x faster than the then-shipped `miniz_oxide`, byte-for-
# byte identically and RSS-neutrally. Phase 16.1 makes `zlib-rs` the shipped
# backend. This court measures what that actually buys **end to end**: the wall
# time and peak RSS of `encode` + `field-ingest` on a documented subset, with the
# OLD backend and the NEW backend built into two binaries from the SAME source
# base, run on the SAME lane.
#
# Each operation is repeated REPS times; the aggregator reports the MEDIAN wall
# time and the peak RSS. `encode` never inflates (a stream's exact leaf is its raw
# compressed span), so it is the control column — any encode delta is lane noise,
# which bounds how small an ingest delta this court can honestly resolve.
#
# ## Arms (both honest, both from source)
#
#   old — the pre-16.1 tree, `git archive`d from OLD_REF into scratch and built
#         there with the same feature set; its `field` inflate is `miniz_oxide`.
#   new — the working tree (the Phase-16.1 change); its `field` inflate is
#         `zlib-rs`.
#
# The NEW working tree may be uncommitted; `environment.json` records both the
# commit and the dirty state, and OLD_REF pins the old arm exactly.
#
# ## Subset (documented)
#
# Header-aware over `manifest.tsv`. Default LIMIT=3 selects, first:
#   * a LARGE PDF (the smallest `pdf|50-100MiB` stratum);
#   * a large PACKAGE format (the smallest `epub|10-50MiB` stratum);
#   * another PACKAGE format (the smallest `docx|<100KiB` stratum),
# then remaining strata by size-class rank. So the subset always contains a large
# PDF and a package format. Override with `IDS="id1 id2 ..."`.
#
# ## Build
#
# `--release --locked --features docx,epub` (not `--all-features`): this keeps the
# shipped default `field` inflate plus the DOCX/EPUB package adapters, without
# dragging in the heavy `entropyfs-store` tree or the LGPL `deflate-replay`
# stack. A build failure is FATAL — the court refuses to measure a stale binary.
#
# ## Lane
#
# Runs in the digest-pinned `doc-baseline` service (mem_limit == memswap_limit ==
# 6g, pids_limit 4096, cpus 8), never on the host:
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase16-zlib-endtoend-court.sh
#   # bounded / explicit ids:
#   docker compose run --rm --no-TTY doc-baseline sh -c 'LIMIT=2 bash tools/phase16-zlib-endtoend-court.sh'
#   docker compose run --rm --no-TTY doc-baseline sh -c 'IDS="nist-docx-0001 nasa-epub-0002" bash tools/phase16-zlib-endtoend-court.sh'
set -uo pipefail

cd /work
export LC_ALL=C

OLD_REF=${OLD_REF:-ebb663644cc12796bdc879c60e5a946e9b885745}
SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=evidence/campaigns/${STAMP}-phase16-zlib-${SHA}
RAW=$CAMPAIGN/raw
WORK=${WORK:-evidence/scratch/phase16-zlib}
# Measured per-operation scratch lives on a fast path (not the bind-mounted host
# disk), so `field-ingest`'s store I/O does not swamp the CPU delta we are
# measuring. Only small evidence (TSV + per-doc JSON) is written to RAW.
MW=${MW:-/tmp/vole-phase16}
mkdir -p "$RAW" "$WORK" "$MW"

MANIFEST=${MANIFEST:-real100-v1/manifest.tsv}
CORPUS=${CORPUS:-real100-v1/documents}
OP_TIMEOUT=${OP_TIMEOUT:-900}
LIMIT=${LIMIT:-3}
REPS=${REPS:-3}
IDS=${IDS:-}
FEATURES=${FEATURES:-docx,epub}

echo "== phase16.1 zlib-rs end-to-end court ==" >&2

# --- build the NEW arm (working tree) --------------------------------------
echo "-- building NEW arm in /work (--release --locked --features $FEATURES)" >&2
if ! cargo build --release --locked --features "$FEATURES" >&2; then
    echo "phase16-zlib: NEW build FAILED; refusing to measure" >&2
    exit 1
fi
NEW_BIN=target/release/vole-document
[ -x "$NEW_BIN" ] || { echo "phase16-zlib: $NEW_BIN missing" >&2; exit 1; }

# --- build the OLD arm from OLD_REF ---------------------------------------
echo "-- exporting OLD arm source from $OLD_REF" >&2
OLDSRC="$WORK/oldsrc"
OLDTARGET="$WORK/oldtarget"
rm -rf "$OLDSRC"; mkdir -p "$OLDSRC"
if ! git archive --format=tar "$OLD_REF" | tar -x -C "$OLDSRC"; then
    echo "phase16-zlib: git archive $OLD_REF FAILED; refusing to measure" >&2
    exit 1
fi
echo "-- building OLD arm (--release --locked --features $FEATURES)" >&2
if ! CARGO_TARGET_DIR="$OLDTARGET" cargo build --release --locked \
        --features "$FEATURES" --manifest-path "$OLDSRC/Cargo.toml" >&2; then
    echo "phase16-zlib: OLD build FAILED; refusing to measure" >&2
    exit 1
fi
OLD_BIN="$OLDTARGET/release/vole-document"
[ -x "$OLD_BIN" ] || { echo "phase16-zlib: $OLD_BIN missing" >&2; exit 1; }

# --- subset selection ------------------------------------------------------
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
        printf "%d\t%s\t%s\t%s\t%s\t%s\t%s\n", p, f[h["id"]], f[h["agency"]], f[h["format"]], f[h["size_class"]], f[h["byte_len"]], f[h["sha256"]];
      }
    }
  ' "$MANIFEST" | sort -k1,1n -k2,2 | cut -f2- | head -n "$LIMIT"
}

select_docs >"$RAW/subset.tsv"
echo "-- subset ($(wc -l <"$RAW/subset.tsv") docs; LIMIT=$LIMIT; REPS=$REPS${IDS:+; IDS=$IDS})" >&2
cat "$RAW/subset.tsv" >&2

# --- provenance ------------------------------------------------------------
RUSTC_V=$(rustc --version 2>/dev/null | sed 's/"/\\"/g')
CARGO_V=$(cargo --version 2>/dev/null | sed 's/"/\\"/g')
ARCH=$(uname -m)
COMMIT=$(git rev-parse HEAD 2>/dev/null || echo unknown)
DIRTY=$(git --no-optional-locks status --porcelain 2>/dev/null | tr '\n' ';' | sed 's/"/\\"/g')
LOCK_SHA=$(sha256sum Cargo.lock | cut -d' ' -f1)
MANIFEST_SHA=$(sha256sum "$MANIFEST" | cut -d' ' -f1)
BASE_STABLE=rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e
NEW_BIN_SHA=$(sha256sum "$NEW_BIN" | cut -d' ' -f1)
OLD_BIN_SHA=$(sha256sum "$OLD_BIN" | cut -d' ' -f1)

cat >"$RAW/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "16.1 — zlib-rs inflate backend, end-to-end impact",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_commit": "$COMMIT",
  "git_dirty": "$DIRTY",
  "old_ref": "$OLD_REF",
  "new_bin_sha256": "$NEW_BIN_SHA",
  "old_bin_sha256": "$OLD_BIN_SHA",
  "arch": "$ARCH",
  "service": "doc-baseline",
  "base_image": "$BASE_STABLE",
  "rustc": "$RUSTC_V",
  "cargo": "$CARGO_V",
  "cargo_lock_sha256": "$LOCK_SHA",
  "manifest": "$MANIFEST",
  "manifest_sha256": "$MANIFEST_SHA",
  "features": "$FEATURES",
  "reps": $REPS,
  "measure_scratch": "$MW",
  "arms": {
    "old": "miniz_oxide (pre-16.1 shipped field inflate); build from OLD_REF",
    "new": "zlib-rs (Phase 16.1 shipped field inflate); build from the working tree"
  },
  "subset": "documented in tools/phase16-zlib-endtoend-court.sh (large PDF + package format)",
  "op_timeout_s": $OP_TIMEOUT,
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

cat >"$CAMPAIGN/commands.txt" <<'EOF'
# Phase 16.1 — zlib-rs end-to-end impact. Never run on the host.
docker compose run --rm --no-TTY doc-baseline bash tools/phase16-zlib-endtoend-court.sh
# bounded smoke / explicit ids:
docker compose run --rm --no-TTY doc-baseline sh -c 'LIMIT=2 bash tools/phase16-zlib-endtoend-court.sh'
docker compose run --rm --no-TTY doc-baseline sh -c 'IDS="nist-docx-0001 nasa-epub-0002" bash tools/phase16-zlib-endtoend-court.sh'
# correctness gate (independent encoder + real-corpus differential):
docker compose run --rm --no-TTY dev sh -c 'cargo test --locked --all-features --test deflate_backend_equivalence'
EOF

# --- measurement -----------------------------------------------------------
printf 'id\tarm\trep\tfmt\tsclass\tbyte_len\tenc_rc\tenc_wall_ms\tenc_rss_kb\ting_rc\ting_wall_ms\ting_rss_kb\tfield\n' >"$RAW/endtoend.tsv"
printf 'id\tarm\tfield\texact\n' >"$RAW/exactness.tsv"

wall_ms() { # $1 = /usr/bin/time -v output file
  awk -F'): ' '/Elapsed \(wall clock\) time/{n=split($2,a,":"); if(n==3) printf "%d",((a[1]*3600)+(a[2]*60)+a[3])*1000; else if(n==2) printf "%d",((a[1]*60)+a[2])*1000; else printf "0"}' "$1"
}
rss_kb() { awk -F': ' '/Maximum resident set size/{print $2}' "$1"; }

while IFS=$'\t' read -r id agency fmt sclass blen sha; do
  [ -n "$id" ] || continue
  src="$CORPUS/$agency/$fmt/$id.$fmt"
  echo "== $id ($fmt, $sclass, $blen B) ==" >&2
  for arm in old new; do
    if [ "$arm" = old ]; then bin="$OLD_BIN"; else bin="$NEW_BIN"; fi
    d="$MW/$id.$arm"; rm -rf "$d"; mkdir -p "$d"
    field=""
    if [ ! -f "$src" ]; then
      for rep in $(seq 1 "$REPS"); do
        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
          "$id" "$arm" "$rep" "$fmt" "$sclass" "$blen" 3 0 0 3 0 0 "" >>"$RAW/endtoend.tsv"
      done
      printf '%s\t%s\t%s\t%s\n' "$id" "$arm" "" 0 >>"$RAW/exactness.tsv"
      continue
    fi
    for rep in $(seq 1 "$REPS"); do
      enc_rc=1; ing_rc=1; enc_w=0; enc_r=0; ing_w=0; ing_r=0
      rm -rf "$d/store"
      timeout "$OP_TIMEOUT" /usr/bin/time -v "$bin" encode "$src" "$d/doc.voldoc" \
        >"$d/encode.json" 2>"$d/encode.time"; enc_rc=$?
      enc_w=$(wall_ms "$d/encode.time"); enc_r=$(rss_kb "$d/encode.time")
      if [ "$enc_rc" -eq 0 ]; then
        timeout "$OP_TIMEOUT" /usr/bin/time -v "$bin" field-ingest "$d/doc.voldoc" \
          --store "$d/store" >"$d/ingest.json" 2>"$d/ingest.time"; ing_rc=$?
        ing_w=$(wall_ms "$d/ingest.time"); ing_r=$(rss_kb "$d/ingest.time")
        field=$(jq -r '.field // empty' "$d/ingest.json" 2>/dev/null)
      fi
      printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$id" "$arm" "$rep" "$fmt" "$sclass" "$blen" \
        "$enc_rc" "${enc_w:-0}" "${enc_r:-0}" "$ing_rc" "${ing_w:-0}" "${ing_r:-0}" "$field" \
        >>"$RAW/endtoend.tsv"
    done
    # One exactness witness per (doc, arm) from the last rep's artifact.
    exact=0
    if [ -n "$field" ] && [ -d "$d/store" ]; then
      timeout "$OP_TIMEOUT" "$bin" materialize --store "$d/store" --field "$field" \
        --exact --output "$d/out" >"$d/mat.json" 2>"$d/mat.err"
      osha=$(sha256sum "$d/out" 2>/dev/null | cut -d' ' -f1)
      olen=$(stat -c %s "$d/out" 2>/dev/null || echo 0)
      if [ "$osha" = "$sha" ] && [ "$olen" = "$blen" ]; then exact=1; fi
    fi
    printf '%s\t%s\t%s\t%s\n' "$id" "$arm" "$field" "$exact" >>"$RAW/exactness.tsv"
    ed="$RAW/perdoc/$id"; mkdir -p "$ed"
    for f in "$d"/*.json "$d"/*.err; do [ -f "$f" ] && cp "$f" "$ed/$arm.$(basename "$f")"; done
    rm -rf "$d"
  done
done <"$RAW/subset.tsv"

# --- aggregate + seal ------------------------------------------------------
echo "-- aggregating" >&2
python3 tools/fixtures/phase16-zlib.py "$RAW" "$CAMPAIGN" | tee "$RAW/summary.txt"

cat >"$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "16.1 — adopt zlib-rs as the shipped DEFLATE inflate backend; end-to-end impact",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "measured_commit": "$COMMIT",
  "old_ref": "$OLD_REF",
  "tree_state": "$DIRTY",
  "base_image": "$BASE_STABLE",
  "service": "doc-baseline (mem_limit == memswap_limit == 6g, pids_limit 4096, cpus 8)",
  "rustc": "$RUSTC_V",
  "cargo": "$CARGO_V",
  "cargo_lock_sha256": "$LOCK_SHA",
  "features": "$FEATURES",
  "reps": $REPS,
  "manifest_sha256": "$MANIFEST_SHA",
  "arms": {
    "old": {"backend": "miniz_oxide", "binary_sha256": "$OLD_BIN_SHA", "source": "git archive $OLD_REF"},
    "new": {"backend": "zlib-rs", "binary_sha256": "$NEW_BIN_SHA", "source": "working tree"}
  },
  "correctness_gate": "every row's materialize --exact must reproduce the manifest SHA-256/byte length; both arms must yield the same field id",
  "byte_identity_witness": "tests/deflate_backend_equivalence.rs (independent flate2/zlib-rs encoder + real real100-v1 members vs the miniz_oxide reference)",
  "prior_microbench": "Phase 15.5 sealed at evidence/campaigns/2026-10-07-phase15-deflate-e676166: zlib-rs 1.58x GB/s, byte-identical, RSS-neutral",
  "court": "tools/phase16-zlib-endtoend-court.sh"
}
EOF

chown -R 1000:1000 "$CAMPAIGN" "$WORK" 2>/dev/null || true

echo "phase16.1 zlib-rs court: done — $CAMPAIGN"
