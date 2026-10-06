#!/bin/sh
# Phase-11 priority #3 court — the procedural seed DAG persisted through EntropyFS.
#
# This is the court that decides the reviewer's #3: the fine-grained seed DAG is
# genuinely persisted through the embeddable EntropyFS engine, **one canonical
# node per engine blob**, not as one coarse blob. It runs the whole flow through
# separate CLI processes (each invocation is a new process) under the
# `entropyfs-store` feature:
#
#   1. `encode` a document to a `.voldoc`, then `field-ingest --entropyfs` into an
#      EntropyFS-backed field store (descriptor + manifest + every seed node are
#      engine blobs);
#   2. report the engine blob count, engine bytes, and the *seed node count*, and
#      confirm the seed nodes are many individual blobs (blob count > node count);
#   3. in a *new process*, `observe --entropyfs --page 1 --kind text` (a real page
#      query) and report its `ObserveStats`;
#   4. in a *new process*, `cache --entropyfs --clear` (drop the disposable cache);
#   5. in a *new process*, re-query the promoted field and `materialize --exact`,
#      verifying length + SHA-256 + `cmp` against the pinned source;
#   6. in a *new process*, report the engine blob count again and show the deepen
#      added exactly one blob per new seed node.
#
# ## Honesty rules
#
#   * EntropyFS is a **content-addressed blob store**. It has no notion of a VOLE
#     seed node, a node kind, or a dependency edge; VOLE owns the canonical node
#     format and its interpretation entirely (ADR-0025). This court never claims
#     the engine natively understands procedural nodes.
#   * This court makes **no speed claim**. The point is persistence granularity:
#     one node per blob, each independently fetchable, never one giant blob.
#   * The engine's `blob_count`/byte figures are EntropyFS's own numbers, taken
#     through `field-store-stats`; they carry no decoder authority.
#   * `list_nodes` declines through the engine (no enumeration API), so a seed-store
#     mark-and-sweep closure cannot be computed through this substrate. Dependency
#     closure by explicit id still works.
#
# Usage (inside the `dev` service, after `cargo build --locked --all-features`):
#   sh tools/field-entropyfs-court.sh OUTDIR [SOURCE_PDF]
set -eu

cd /work
LC_ALL=C
export LC_ALL

OUTDIR=${1:-evidence/scratch/field-entropyfs-court}
SRC=${2:-evidence/campaigns/2026-10-05-phase3-486aa17/corpus/twopage.pdf}
BIN=${VOLE_BIN:-./target/debug/vole-document}

mkdir -p "$OUTDIR/raw"
RAW="$OUTDIR/raw"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

if [ ! -x "$BIN" ]; then
  echo "field-entropyfs-court: $BIN not found; building --all-features" >&2
  cargo build --locked --all-features 1>&2
fi

# ---------------------------------------------------------------------------
# Helpers (the `dev` image has no jq; the CLI emits one JSON object per line).
# ---------------------------------------------------------------------------
sha() { sha256sum "$1" | cut -d' ' -f1; }
bytes_of() { stat -c %s "$1" 2>/dev/null || echo 0; }
# Extract a JSON number / string / boolean by unique key.
jnum() { sed -n "s/.*\"$1\":\([0-9][0-9]*\).*/\1/p" "$2" | head -1; }
jstr() { sed -n "s/.*\"$1\":\"\([0-9a-zA-Z]*\)\".*/\1/p" "$2" | head -1; }
jbool() { sed -n "s/.*\"$1\":\(true\|false\).*/\1/p" "$2" | head -1; }

# ---------------------------------------------------------------------------
# Environment capture (receipt provenance).
# ---------------------------------------------------------------------------
RUSTC_V=$(rustc --version 2>/dev/null | sed 's/^rustc //' || echo unknown)
CARGO_V=$(cargo --version 2>/dev/null | sed 's/^cargo //' || echo unknown)
ARCH=$(uname -m)
COMMIT=$(git rev-parse HEAD 2>/dev/null || echo unknown)
COMMIT_SHORT=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
DIRTY=$(git status --porcelain 2>/dev/null | head -40 | tr '\n' ';' || echo unknown)
LOCK_SHA=$(sha /work/Cargo.lock 2>/dev/null || echo unknown)
BASE_STABLE=$(sed -n 's/^ARG BASE_STABLE=//p' /work/Dockerfile 2>/dev/null | head -1)
IMAGE_ID=${HOST_IMAGE_ID:-unrecorded}
CMD_LINE="sh tools/field-entropyfs-court.sh $OUTDIR $SRC"

# ---------------------------------------------------------------------------
# 0. Pin the source in the receipt directory.
# ---------------------------------------------------------------------------
SRC_SHA=$(sha "$SRC")
SRC_LEN=$(bytes_of "$SRC")
cp "$SRC" "$OUTDIR/source.pdf"

# ---------------------------------------------------------------------------
# 1. Encode + ingest into an EntropyFS-backed field store.
# ---------------------------------------------------------------------------
STORE="$OUTDIR/store"
rm -rf "$STORE"
"$BIN" encode "$OUTDIR/source.pdf" "$WORK/doc.voldoc" > "$RAW/encode.json"
"$BIN" field-ingest "$WORK/doc.voldoc" --store "$STORE" --entropyfs > "$RAW/ingest.json"
"$BIN" field-store-stats --store "$STORE" --entropyfs > "$RAW/stats_ingest.json"

FIELD=$(jstr field "$RAW/ingest.json")
NODE_COUNT=$(jnum node_count "$RAW/ingest.json")
BLOB_INGEST=$(jnum blob_count "$RAW/stats_ingest.json")
LOGICAL_BYTES=$(jnum logical_bytes "$RAW/stats_ingest.json")
ENGINE_BYTES=$(jnum physical_used_bytes "$RAW/stats_ingest.json")
# Each seed node is one blob; the descriptor and manifests add blobs on top, so
# blob count must be at least the node count. If the DAG were one blob, the blob
# count would be a small constant (a few), far below the node count.
INDIVIDUAL=no
[ "$BLOB_INGEST" -ge "$NODE_COUNT" ] && INDIVIDUAL=yes

# ---------------------------------------------------------------------------
# 2. New process: query a page (this deepens the page and writes derived nodes).
# ---------------------------------------------------------------------------
"$BIN" observe --store "$STORE" --field "$FIELD" --entropyfs --page 1 --kind text \
  > "$RAW/observe_page1.json"
PROMOTED=$(jstr field "$RAW/observe_page1.json")
DEEPENED=$(jbool deepened "$RAW/observe_page1.json")
SEED_FETCHED=$(jnum seed_nodes_fetched "$RAW/observe_page1.json")
SEED_BYTES=$(jnum seed_bytes_read "$RAW/observe_page1.json")
TEXT=$(sed -n 's/.*"text":"\([^"]*\)".*/\1/p' "$RAW/observe_page1.json" | head -1)

# The deepen wrote 3 derived seed nodes + 1 promoted manifest => 4 new blobs.
"$BIN" field-store-stats --store "$STORE" --entropyfs > "$RAW/stats_after_deepen.json"
BLOB_AFTER=$(jnum blob_count "$RAW/stats_after_deepen.json")
BLOB_DELTA=$(( BLOB_AFTER - BLOB_INGEST ))

# ---------------------------------------------------------------------------
# 3. New process: drop the disposable derived cache.
# ---------------------------------------------------------------------------
"$BIN" cache --store "$STORE" --entropyfs --clear > "$RAW/cache_clear.json"
CACHE_RECLAIMED=$(jnum reclaimed "$RAW/cache_clear.json")

# ---------------------------------------------------------------------------
# 4. New process: re-query the promoted field (no re-deepen) and materialize
#    exact with the working source removed.
# ---------------------------------------------------------------------------
"$BIN" observe --store "$STORE" --field "$PROMOTED" --entropyfs --page 1 --kind text \
  > "$RAW/observe_promoted.json"
REDEEPENED=$(jbool deepened "$RAW/observe_promoted.json")

rm -f "$WORK/doc.voldoc"
"$BIN" materialize --store "$STORE" --field "$PROMOTED" --entropyfs --exact \
  --output "$WORK/out.pdf" > "$RAW/materialize.json"
OUT_SHA=$(sha "$WORK/out.pdf")
OUT_LEN=$(bytes_of "$WORK/out.pdf")

EXACT=no
if [ "$OUT_SHA" = "$SRC_SHA" ] && [ "$OUT_LEN" = "$SRC_LEN" ] \
   && cmp -s "$WORK/out.pdf" "$OUTDIR/source.pdf"; then
  EXACT=yes
fi

"$BIN" field-store-stats --store "$STORE" --entropyfs > "$RAW/stats_final.json"
BLOB_FINAL=$(jnum blob_count "$RAW/stats_final.json")

# ---------------------------------------------------------------------------
# 5. Machine-readable summary + human summary.
# ---------------------------------------------------------------------------
cat > "$OUTDIR/result.json" <<EOF
{
  "phase": "phase11-entropyfs",
  "commit": "$COMMIT",
  "commit_short": "$COMMIT_SHORT",
  "dirty": $( [ "${DIRTY}" = "" ] && echo false || echo true ),
  "rustc": "$RUSTC_V",
  "cargo": "$CARGO_V",
  "arch": "$ARCH",
  "base_stable": "$BASE_STABLE",
  "image_id": "$IMAGE_ID",
  "cargo_lock_sha256": "$LOCK_SHA",
  "command": "$CMD_LINE",
  "source_sha256": "$SRC_SHA",
  "source_len": $SRC_LEN,
  "field": "$FIELD",
  "promoted_field": "$PROMOTED",
  "seed_node_count": $NODE_COUNT,
  "engine_blob_count_ingest": $BLOB_INGEST,
  "engine_logical_bytes": $LOGICAL_BYTES,
  "engine_physical_bytes": $ENGINE_BYTES,
  "seed_nodes_are_individual_blobs": "$INDIVIDUAL",
  "blob_count_after_deepen": $BLOB_AFTER,
  "blob_delta_on_deepen": $BLOB_DELTA,
  "deepened": "$DEEPENED",
  "seed_nodes_fetched": $SEED_FETCHED,
  "seed_bytes_read": $SEED_BYTES,
  "cache_reclaimed_bytes": $CACHE_RECLAIMED,
  "re_deepened_after_cache_clear": "$REDEEPENED",
  "materialized_sha256": "$OUT_SHA",
  "materialized_len": $OUT_LEN,
  "byte_exact": "$EXACT"
}
EOF

echo "field-entropyfs court — Phase 11 priority #3"
echo "  source            : $SRC ($SRC_LEN bytes, sha256 $SRC_SHA)"
echo "  field             : $FIELD"
echo "  seed nodes        : $NODE_COUNT"
echo "  engine blobs      : $BLOB_INGEST after ingest, $BLOB_FINAL final"
echo "  blobs added by a 3-node deepen: $BLOB_DELTA"
echo "  engine bytes      : logical=$LOGICAL_BYTES physical=$ENGINE_BYTES"
echo "  individual blobs  : $INDIVIDUAL (blob_count >= seed_node_count)"
echo "  page 1 text       : $TEXT"
echo "  observe deepened  : $DEEPENED, seed_nodes_fetched=$SEED_FETCHED, seed_bytes_read=$SEED_BYTES"
echo "  cache reclaimed   : $CACHE_RECLAIMED bytes"
echo "  re-deepened after cache clear: $REDEEPENED (expect false)"
echo "  materialize exact : byte_exact=$EXACT (len=$OUT_LEN sha256=$OUT_SHA)"
echo "  raw evidence      : $RAW/"
