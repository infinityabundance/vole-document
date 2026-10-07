#!/usr/bin/env bash
# Phase 15.7 "durable cross-root derivations" court — the `N3` re-run over the
# REAL `real100-v1` publication/revision families.
#
# `N3` is the ADR-0035 go/no-go: "reuse ≈0 or below strongest CDC on reuse work".
# Phase 12 measured it once on a 3-document synthetic cohort (a DOCX and an EPUB
# embedding byte-identical resource bytes plus a control); the warm
# `retained_inverse_work_fraction` was 0.339907 but collapsed to 0.0 after
# `cache --clear` (in-process AND in a fresh OS process), and the borg CDC
# baseline saved `-1374` bytes → VIOLATED, a recorded negative
# (`evidence/campaigns/2026-10-06-phase12-share-controls-dce2705/`). `N3` had no
# SQLite comparison; its only baseline was borg CDC, so none is reproduced here.
#
# This court re-runs the same measurement on the real families. It does NOT
# introduce a durable store: it measures whether one is needed at all. Per
# family (`cross_format_family_id` = manifest field 17, `revision_family_id` =
# field 18), it opens ONE shared `FieldStore`, ingests every member into it (so
# ids can collide and share), and measures:
#
#   * representation sharing (per member: `nodes_id_shared`, `shared_resource_ids`,
#     `shared_resource_bytes`, `resource_blob_nodes`, `seed_bytes_written`,
#     `index_bytes_written`; plus the on-disk store bytes);
#   * cross-member WORK reuse: a cold (`--no-cache`) pass vs a warm in-order pass,
#     reporting `seed_nodes_reused` and `retained_inverse_work_fraction` computed
#     from integer receipts (node executions + cold input bytes), never source size;
#     a per-member EMPTY-cache pass gives each member's intra-observation (diamond)
#     reuse floor, so cross-member reuse is `warm - intra` (clamped ≥ 0);
#   * the durability falsifier: after the warm pass, `cache --clear` and a FRESH OS
#     process re-observe the member with the largest warm reuse — its reuse must
#     fall back to its empty-cache floor, i.e. the cross-member reuse did not
#     survive the clear (no durable output store). The same member observed in a
#     fresh process with the cache intact (non-zero reused above its floor) proves
#     the counter is live, so a collapse means "cache-only", not "dead".
#   * a per-family chunk-level CDC baseline (`tools/chunk-dedup.sh`, borg, frozen
#     `19,23,21,4095` plus two smaller sweeps, `--compression none`) — raw, no
#     compression claim.
#
# No compression claim is made anywhere: a shared blob is scored as state/work.
#
# Run in the pinned, capped `doc-baseline` service:
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase15-crossroot-court.sh
set -uo pipefail

cd /work
export LC_ALL=C

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
CAMPAIGN=${1:-evidence/campaigns/$(date -u +%Y-%m-%d)-phase15-crossroot-${SHA}}
SCRATCH=${CROSSROOT_SCRATCH:-evidence/scratch/phase15-crossroot}
MANIFEST=${MANIFEST:-real100-v1/manifest.tsv}
CORPUS=${CORPUS:-real100-v1/documents}
BIN=${VOLE_BIN:-./target/debug/vole-document}
PY=tools/fixtures/phase15-crossroot.py
PER_OP_TIMEOUT=${PER_OP_TIMEOUT:-300}
# 0 = no per-member byte cap (process every family member). A positive value
# skips members above the cap and records the skip — a bounded-work decision,
# never a result filter.
MAX_MEMBER_BYTES=${MAX_MEMBER_BYTES:-0}

# Snapshot provenance *before* this script creates its own receipt directory.
COMMIT=$(git rev-parse HEAD 2>/dev/null || echo unknown)
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)
DIRTY=$(git status --porcelain 2>/dev/null | head -40 | tr '\n' ';')
RUN_UTC=$(date -u +%Y-%m-%dT%H:%M:%SZ)

rm -rf "$CAMPAIGN"
mkdir -p "$CAMPAIGN/raw/cdc" "$SCRATCH"
: > "$CAMPAIGN/raw/measure.jsonl"

echo "== phase15.7 cross-root derivation court ==" >&2
echo "-- building (locked, all-features) binary" >&2
if ! cargo build --locked --all-features >"$CAMPAIGN/raw/build.log" 2>&1; then
    echo "phase15-crossroot: build FAILED; refusing to measure" >&2
    tail -20 "$CAMPAIGN/raw/build.log" >&2
    exit 1
fi

echo "-- planning families from $MANIFEST" >&2
python3 "$PY" plan "$MANIFEST" "$CORPUS" "$CAMPAIGN/raw" | tee "$CAMPAIGN/raw/plan.meta.json"

# --- helpers ---------------------------------------------------------------
emit_ingest() { # key kind family member fmt blen file ok reason
    python3 - "$@" <<'PY' >> "$CAMPAIGN/raw/measure.jsonl"
import sys, json
key, kind, family, member, fmt, blen, path, ok, reason = sys.argv[1:10]
rec = {"t": "ingest", "key": key, "kind": kind, "family": family,
       "member": member, "format": fmt, "byte_len": int(blen),
       "ok": ok == "true"}
if ok == "true":
    d = json.load(open(path))
    for k in ("field", "nodes_id_shared", "shared_resource_ids",
              "shared_resource_bytes", "resource_blob_nodes", "member_count",
              "seed_bytes_written", "index_bytes_written", "source_len"):
        rec[k] = d.get(k)
else:
    rec["reason"] = reason
print(json.dumps(rec))
PY
}

emit_obs() { # key kind family member arm file
    python3 - "$@" <<'PY' >> "$CAMPAIGN/raw/measure.jsonl"
import sys, json
key, kind, family, member, arm, path = sys.argv[1:7]
rec = {"t": "obs", "key": key, "kind": kind, "family": family,
       "member": member, "arm": arm, "ok": False}
try:
    d = json.load(open(path))
    a = d.get("actual") or {}
    rec.update(ok=True,
               executed=a.get("seed_nodes_executed"),
               reused=a.get("seed_nodes_reused"),
               work_units=a.get("inverse_work_units"),
               member_decodes=a.get("member_decodes"),
               xml_parses=a.get("xml_parses"),
               bytes_read=a.get("bytes_read"),
               basis=a.get("basis"))
except Exception as exc:
    rec["error"] = str(exc)
print(json.dumps(rec))
PY
}

emit_store() { # key kind family bytes
    python3 - "$@" <<'PY' >> "$CAMPAIGN/raw/measure.jsonl"
import sys, json
key, kind, family, b = sys.argv[1:5]
print(json.dumps({"t": "store", "key": key, "kind": kind, "family": family,
                  "store_bytes": int(b)}))
PY
}

emit_cdc() { # key file params
    python3 - "$@" <<'PY' >> "$CAMPAIGN/raw/cdc/$1.jsonl"
import sys, json
key, path, params = sys.argv[1:4]
d = json.load(open(path))
print(json.dumps({"key": key, "params": params,
                  "unique_bytes": d.get("borg_unique_bytes"),
                  "total_size": d.get("borg_total_size"),
                  "unique_chunks": d.get("borg_unique_chunks"),
                  "total_chunks": d.get("borg_total_chunks"),
                  "deterministic": d.get("borg_deterministic")}))
PY
}

sel_for() { # format -> printf the selector flags
    case "$1" in
        pdf)  printf '%s' "--page 1" ;;
        *)    printf '%s' "--block 0" ;;
    esac
}

run_explain() { # key member field sel extra out
    local key=$1 member=$2 field=$3 sel=$4 extra=$5 out=$6
    # shellcheck disable=SC2086
    timeout "$PER_OP_TIMEOUT" "$BIN" explain --store "$SCRATCH/$key/store" \
        --field "$field" $sel --kind text --analyze $extra >"$out" 2>"$out.err"
}

# --- main loop -------------------------------------------------------------
FAMKEYS=()
declare -A FAM_KIND FAM_ID
declare -A MEMBERS   # key -> newline-joined "id\tagency\tfmt\tpath\tblen"

while IFS=$'\t' read -r key kind family idx mid agency fmt path sha blen; do
    FAMKEYS+=("$key")
    FAM_KIND[$key]="$kind"
    FAM_ID[$key]="$family"
    MEMBERS[$key]+="${mid}"$'\t'"${agency}"$'\t'"${fmt}"$'\t'"${path}"$'\t'"${blen}"$'\n'
done < <(tail -n +2 "$CAMPAIGN/raw/families.tsv")

# De-duplicate family keys while preserving order.
UNIQ_KEYS=()
declare -A SEEN
for k in "${FAMKEYS[@]}"; do
    if [ -z "${SEEN[$k]:-}" ]; then SEEN[$k]=1; UNIQ_KEYS+=("$k"); fi
done

for key in "${UNIQ_KEYS[@]}"; do
    kind=${FAM_KIND[$key]}
    family=${FAM_ID[$key]}
    echo "== family $key ($kind:$family) ==" >&2
    work="$SCRATCH/$key"
    rm -rf "$work"; mkdir -p "$work/store"

    # ---- ingest all members into ONE shared store ------------------------
    declare -A FIELD OFMT PATHS BLEN
    while IFS=$'\t' read -r mid agency fmt path blen; do
        [ -n "$mid" ] || continue
        FIELD[$mid]=""; OFMT[$mid]="$fmt"; PATHS[$mid]="$path"; BLEN[$mid]="$blen"
        src="$path"
        if [ ! -f "$src" ]; then
            emit_ingest "$key" "$kind" "$family" "$mid" "$fmt" "$blen" "/dev/null" false "missing-source"
            continue
        fi
        if [ "$MAX_MEMBER_BYTES" -gt 0 ] && [ "$blen" -gt "$MAX_MEMBER_BYTES" ]; then
            emit_ingest "$key" "$kind" "$family" "$mid" "$fmt" "$blen" "/dev/null" false "over-cap"
            continue
        fi
        if ! timeout "$PER_OP_TIMEOUT" "$BIN" encode "$src" "$work/$mid.voldoc" \
                >"$work/$mid.encode.json" 2>"$work/$mid.encode.err"; then
            emit_ingest "$key" "$kind" "$family" "$mid" "$fmt" "$blen" "/dev/null" false "encode"
            continue
        fi
        if ! timeout "$PER_OP_TIMEOUT" "$BIN" field-ingest "$work/$mid.voldoc" \
                --store "$work/store" >"$work/$mid.ingest.json" 2>"$work/$mid.ingest.err"; then
            emit_ingest "$key" "$kind" "$family" "$mid" "$fmt" "$blen" "/dev/null" false "ingest"
            continue
        fi
        FIELD[$mid]=$(sed -n 's/.*"field":"\([0-9a-f]\{64\}\)".*/\1/p' "$work/$mid.ingest.json" | head -1)
        emit_ingest "$key" "$kind" "$family" "$mid" "$fmt" "$blen" "$work/$mid.ingest.json" true ""
    done <<< "${MEMBERS[$key]}"

    emit_store "$key" "$kind" "$family" "$(du -sb "$work/store" 2>/dev/null | awk '{print $1+0}')"

    # Ordered list of members whose field was recovered.
    ORDER=()
    while IFS=$'\t' read -r mid agency fmt path blen; do
        [ -n "$mid" ] || continue
        [ -n "${FIELD[$mid]}" ] && ORDER+=("$mid")
    done <<< "${MEMBERS[$key]}"

    if [ "${#ORDER[@]}" -lt 2 ]; then
        echo "   fewer than 2 members ingested; recording representation only" >&2
    else
        # ---- cold pass (--no-cache): the work baseline -------------------
        rm -rf "$work/store/cache"
        for mid in "${ORDER[@]}"; do
            sel=$(sel_for "${OFMT[$mid]}")
            run_explain "$key" "$mid" "${FIELD[$mid]}" "$sel" "--no-cache" "$work/$mid.cold.json" || true
            emit_obs "$key" "$kind" "$family" "$mid" cold "$work/$mid.cold.json"
        done

        # ---- intra pass: empty cache, ONE fresh observation per member ---
        # The only reuse a single observation can show with an empty cache is
        # its own intra-observation (diamond) reuse. This is the floor that
        # cross-member reuse must exceed.
        for mid in "${ORDER[@]}"; do
            rm -rf "$work/store/cache"
            sel=$(sel_for "${OFMT[$mid]}")
            run_explain "$key" "$mid" "${FIELD[$mid]}" "$sel" "" "$work/$mid.intra.json" || true
            emit_obs "$key" "$kind" "$family" "$mid" intra "$work/$mid.intra.json"
        done

        # ---- warm in-order pass (cache primed by earlier members) --------
        rm -rf "$work/store/cache"
        for mid in "${ORDER[@]}"; do
            sel=$(sel_for "${OFMT[$mid]}")
            run_explain "$key" "$mid" "${FIELD[$mid]}" "$sel" "" "$work/$mid.warm.json" || true
            emit_obs "$key" "$kind" "$family" "$mid" warm "$work/$mid.warm.json"
        done

        first=${ORDER[0]}
        # ---- warm, FRESH process, cache intact (counter liveness) --------
        sel=$(sel_for "${OFMT[$first]}")
        run_explain "$key" "$first" "${FIELD[$first]}" "$sel" "" "$work/$first.warmfresh.json" || true
        emit_obs "$key" "$kind" "$family" "$first" warm_fresh "$work/$first.warmfresh.json"

        # ---- durability: post-`cache --clear` re-observe of the member with
        #      the largest warm reuse. Its reuse must fall back to that
        #      member's empty-cache floor; anything above it would be durable.
        star=$(for mid in "${ORDER[@]}"; do
            r=$(sed -n 's/.*"seed_nodes_reused":\([0-9]*\).*/\1/p' "$work/$mid.warm.json" 2>/dev/null | head -1)
            echo "${r:-0} $mid"
        done | sort -rn | head -1 | awk '{print $2}')
        [ -n "$star" ] || star=$first
        rm -rf "$work/store/cache"
        sel=$(sel_for "${OFMT[$star]}")
        run_explain "$key" "$star" "${FIELD[$star]}" "$sel" "" "$work/$star.postclear.json" || true
        emit_obs "$key" "$kind" "$family" "$star" postclear_star "$work/$star.postclear.json"
    fi

    # ---- per-family CDC baseline over the family's source bytes -----------
    cohort="$work/cdc-cohort"; rm -rf "$cohort"; mkdir -p "$cohort"
    while IFS=$'\t' read -r mid agency fmt path blen; do
        [ -n "$mid" ] || continue
        [ -f "$path" ] || continue
        cp -l "$path" "$cohort/$mid.$fmt" 2>/dev/null || cp "$path" "$cohort/$mid.$fmt" 2>/dev/null || true
    done <<< "${MEMBERS[$key]}"
    if [ -n "$(ls -A "$cohort" 2>/dev/null)" ]; then
        sh tools/chunk-dedup.sh "$cohort" "$work/cdc-default.json" 19,23,21,4095 none >/dev/null 2>&1 || true
        sh tools/chunk-dedup.sh "$cohort" "$work/cdc-16.json" 16,20,18,4095 none >/dev/null 2>&1 || true
        sh tools/chunk-dedup.sh "$cohort" "$work/cdc-12.json" 12,16,14,4095 none >/dev/null 2>&1 || true
        for p in default 16 12; do
            f="$work/cdc-$p.json"
            [ -s "$f" ] && emit_cdc "$key" "$f" "$p"
        done
    fi
    # Regenerable bytes; never sealed.
    rm -rf "$work/store" "$work/cdc-cohort"
done

# --- aggregate + verdict + seal --------------------------------------------
echo "-- aggregating" >&2
python3 "$PY" aggregate "$CAMPAIGN" | tee "$CAMPAIGN/raw/n3.print.txt"

# --- environment / provenance ---------------------------------------------
RUSTC=$(rustc -vV | sed -n 's/^release: //p')
CARGO=$(cargo -V | sed -n 's/^cargo //p')
LOCK_SHA=$(sha256sum Cargo.lock | cut -d' ' -f1)
ARCH=$(uname -m)
BASE_STABLE=$(sed -n 's/^ARG BASE_STABLE=//p' Dockerfile)
BORG_V=$(borg --version | awk '{print $2}')
JQ_V=$(jq --version)
PY_V=$(python3 --version | awk '{print $2}')

cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$(basename "$CAMPAIGN")",
  "run_utc": "$RUN_UTC",
  "arch": "$ARCH",
  "git_branch": "$BRANCH",
  "git_commit": "$COMMIT",
  "git_dirty": "$DIRTY",
  "cargo_lock_sha256": "$LOCK_SHA",
  "doc_baseline": {
    "service_image": "vole-document/doc-baseline:1.99.0",
    "base_image": "$BASE_STABLE",
    "rustc": "$RUSTC",
    "cargo": "$CARGO",
    "borg": "$BORG_V",
    "jq": "$JQ_V",
    "python3": "$PY_V"
  },
  "features_under_test": "all-features (field, package, opc, docx, epub, entropyfs-store, deflate-replay, ...)",
  "env_vars_affecting_semantics": { "CARGO_TERM_COLOR": "never", "LC_ALL": "C" },
  "parameters": {
    "manifest": "$MANIFEST",
    "corpus": "$CORPUS",
    "per_op_timeout_s": $PER_OP_TIMEOUT,
    "max_member_bytes": $MAX_MEMBER_BYTES
  },
  "controls": {
    "cold": "explain --analyze --no-cache (cache bytes ignored)",
    "intra": "empty cache, one fresh-process observation per member (intra-observation reuse floor)",
    "warm": "explain --analyze in-order per family (shared store, cache on)",
    "warm_fresh": "fresh OS process, cache intact (counter liveness)",
    "postclear_star": "fresh OS process after the store's cache/ directory is removed, on the max-warm-reuse member (durability falsifier)",
    "cdc_baseline": "raw/cdc/<family>.jsonl (borg, --compression none, three frozen sweeps)"
  },
  "notes": "All commands ran through compose.yaml inside the pinned, capped doc-baseline image; nothing ran on the host except git and docker. N3 had no SQLite comparison; its only baseline was borg CDC, so none is reproduced. No compression claim: a shared blob is scored as state/work."
}
EOF

cat > "$CAMPAIGN/commands.txt" <<EOF
# Phase 15.7 durable cross-root derivations court — exact commands
# Commit under test: $COMMIT (branch $BRANCH); dirty: $DIRTY
# Run (UTC): $RUN_UTC
# Base image (ARG BASE_STABLE): $BASE_STABLE
# Service image: vole-document/doc-baseline:1.99.0
# rustc $RUSTC, cargo $CARGO, borg $BORG_V, jq $JQ_V, python3 $PY_V, arch $ARCH
# Cargo.lock sha256: $LOCK_SHA
# All commands run inside the pinned, capped doc-baseline image; nothing on the host.

docker compose run --rm --no-TTY doc-baseline bash tools/phase15-crossroot-court.sh

# Inside phase15-crossroot-court.sh:
#   (0) cargo build --locked --all-features
#   (1) python3 tools/fixtures/phase15-crossroot.py plan MANIFEST CORPUS CAMPAIGN/raw
#   (2) per family: encode MEMBER -> .voldoc; field-ingest into ONE shared store;
#         explain --analyze arms: --no-cache (cold); empty-cache one-shot per member
#         (intra floor); in-order warm; fresh-process warm (cache intact); fresh-process
#         post-cache-clear on the max-warm member (durability);
#         tools/chunk-dedup.sh COHORT OUT {19,23,21,4095|16,20,18,4095|12,16,14,4095} none
#   (3) python3 tools/fixtures/phase15-crossroot.py aggregate CAMPAIGN  ->  raw/summary.json, N3.txt
#   (4) python3 tools/fixtures/phase15-crossroot.py receipt  CAMPAIGN  ->  receipt.json, SUMMARY.md
# Stores and CDC cohorts are in the gitignored evidence/scratch/phase15-crossroot
# (regenerable bytes, never sealed).
EOF

( cd "$CAMPAIGN" && sha256sum raw/*.json raw/*.jsonl raw/cdc/*.jsonl raw/*.tsv > raw.sha256 2>/dev/null ) || true

# --- receipt.json + SUMMARY.md (sealed headline result) --------------------
python3 "$PY" receipt "$CAMPAIGN"

# Keep the sealed receipt's own hash out of raw.sha256 (it is written after);
# record it separately so the receipt is tamper-evident too.
( cd "$CAMPAIGN" && sha256sum receipt.json SUMMARY.md N3.txt >> raw.sha256 ) 2>/dev/null || true

echo "wrote $CAMPAIGN"
