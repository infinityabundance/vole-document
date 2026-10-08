#!/usr/bin/env bash
# Phase 22.3.0 (measurement only) — duplicate-work court for a heterogeneous
# observation batch.
#
# ## Question
#
#   `DocumentFieldSession::observe_batch` serves a batch as a plain ordered loop
#   (`reqs.iter().map(|r| self.observe(r, ...))`). Before building ANY fused
#   executor, this court measures the deterministic *upper bound* on duplicate
#   work across a known batch: how much work is repeated per request, and how
#   much of it the **existing** resident session already removes via its
#   typed-model memo + on-disk derived cache.
#
# ## Method
#
#   For each of 12 `real100-v1` documents (4 pdf / 4 docx / 4 epub):
#
#   * build the packed store ONCE (`field-build --profile runtime --packed`);
#   * write `req.resident` (the frozen Phase-22.2 schedule, as-is) and
#     `req.indep` (the SAME lines with `--no-cache` appended to every line —
#     the session-level `--no-cache` on the observe-batch command line is parsed
#     but NOT propagated to the per-request evaluations, so to disable the memo
#     AND the derived cache for the whole batch the flag must be on every line);
#   * `rm -rf <store>/cache` before EVERY lane, then run
#       resident  -> <raw>/<id>.resident.jsonl
#       indep     -> <raw>/<id>.indep.jsonl     (cold cache)
#       indep2    -> <raw>/<id>.indep2.jsonl    (determinism; must equal indep
#                                                modulo wall_micros)
#       null      -> <raw>/<id>.null.jsonl      (two-request, largely-disjoint
#                                                control, indep lines; expected
#                                                struct_dup ≈ 1.0)
#   * record rc + wall (µs via EPOCHREALTIME) per lane.
#
#   The aggregator (`tools/fixtures/phase22-3-dup.py`) computes, per document,
#   DETERMINISTIC counter-based metrics (no statistics):
#     indep_exec/resident_exec  = Σ seed_nodes_executed (memo+cache off vs the
#                                 shipping default)
#     exec_dedup = indep_exec/resident_exec;  idx_dedup = indep_index/resident_index
#     bytes_dedup = indep_seed_bytes/resident_seed_bytes
#     cwrite_dedup = indep_cache_written/resident_cache_written
#   plus a SECONDARY, clearly-labelled closure proxy (`dependency_ids` / |U|),
#   which is UNSOUND as a work metric (it undercounts the executed closure and
#   DOCX/EPUB share model nodes) and is not used for the verdict.
#   Verdict (deterministic): CLEARS only if a format shows duplication that the
#   shipping session does NOT remove; SESSION-ALREADY-CAPTURES if duplication
#   exists but the resident session already removes it (exec_dedup >= 2);
#   NOTHING-TO-FUSE if no format shows duplicate work.
#
# ## Lane
#
# Never the host: the digest-pinned, hard-capped `doc-baseline` service
# (mem_limit == memswap_limit == 6g, pids_limit 4096, cpus 8).
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase22-3-dup-court.sh
#
# If BIN and BIN_SKIP_BUILD are set the cargo build is skipped (measure a
# pre-built binary). Otherwise the release binary is built with all features.
set -uo pipefail

cd /work
export LC_ALL=C
# Never leave root-owned bytecode caches in the tree for the host to clean up.
export PYTHONDONTWRITEBYTECODE=1

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=evidence/campaigns/${STAMP}-phase22-3-dup-${SHA}${TAG:+-$TAG}
RAW=$CAMPAIGN/raw
WORK=${WORK:-evidence/scratch/phase22-3-court}
rm -rf "$RAW"
mkdir -p "$RAW" "$WORK"

MANIFEST=${MANIFEST:-real100-v1/manifest.tsv}
CORPUS=${CORPUS:-real100-v1/documents}
AGG=tools/fixtures/phase22-3-dup.py
OP_TIMEOUT=${OP_TIMEOUT:-180}
DEFAULT_IDS="nist-pdf-0002 nist-pdf-0004 nist-pdf-0016 nist-pdf-0017 nist-docx-0005 nist-docx-0008 nist-docx-0009 nist-docx-0014 nist-epub-0003 nist-epub-0006 nist-epub-0008 nist-epub-0009"
IDS=${IDS:-$DEFAULT_IDS}

PROFILE=${PROFILE:-release}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-${VOLE_BIN:-target/debug/vole-document}} ;;
esac

if [ -x "$BIN" ] && [ -n "${BIN_SKIP_BUILD:-}" ]; then
    echo "-- using prebuilt $BIN (BIN_SKIP_BUILD set)" >&2
else
    echo "== phase22.3.0 duplicate-work court (resident vs indep vs null; cold cache per lane) ==" >&2
    echo "-- building ($PROFILE) binary" >&2
    # shellcheck disable=SC2086
    if ! cargo build $BUILD_ARGS >&2; then
        echo "phase22-3: build FAILED ($BUILD_ARGS); refusing to measure" >&2
        exit 1
    fi
fi
[ -x "$BIN" ] || { echo "phase22-3: $BIN missing; refusing to measure" >&2; exit 1; }

# --- preflight the docx/epub adapters (identical to Phase 22.2) -------------
PREFLIGHT=""
for cand in real100-v1/documents/nist/docx/*.docx real100-v1/documents/nasa/epub/*.epub; do
    [ -f "$cand" ] && { PREFLIGHT="$cand"; break; }
done
if [ -n "$PREFLIGHT" ]; then
    expect=$(case "$PREFLIGHT" in *.docx) echo docx ;; *) echo epub ;; esac)
    if ! "$BIN" capabilities "$PREFLIGHT" 2>/dev/null | grep -q "\"adapter\":\"$expect\""; then
        echo "phase22-3: preflight failed — $BIN lacks $expect" >&2
        exit 1
    fi
fi

# microsecond wall clock (EPOCHREALTIME has 6 fractional digits).
now_us() { echo $(( ${EPOCHREALTIME/./} )); }

# --- subset selection (verbatim from Phase 22.2) ----------------------------
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
    echo "phase22-3: corpus verification FAILED; refusing to measure" >&2
    exit 1
fi

# --- observation sets (verbatim from Phase 22.2) ----------------------------
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
printf 'id\tfmt\trc\tus\tfield\n' >"$RAW/build.tsv"
printf 'id\tfmt\tlane\trc\tus\tn_req\n' >"$RAW/lanes.tsv"

# --- main loop --------------------------------------------------------------
while IFS='|' read -r id agency fmt sclass blen sha path fam is_head; do
    [ -n "$id" ] || continue
    d="$WORK/$id"; rm -rf "$d"; mkdir -p "$d"
    echo "== $id ($fmt, $sclass, $blen B, family=${fam:-none}, head=$is_head) ==" >&2

    # ---- ONE-TIME packed store build ----
    rm -rf "$d/vstore"
    t0=$(now_us)
    timeout "$OP_TIMEOUT" "$BIN" field-build "$path" --store "$d/vstore" \
        --profile runtime --packed >"$d/build.json" 2>"$d/build.err"; brc=$?
    t1=$(now_us)
    field=""
    if [ "$brc" -eq 0 ]; then
        field=$(jq -r '.ingest.field // empty' "$d/build.json" 2>/dev/null)
    fi
    printf '%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "$brc" "$(( t1 - t0 ))" "$field" >>"$RAW/build.tsv"
    echo "   field-build rc=$brc field=${field:-none}" >&2

    # ---- request files ----
    reqr="$d/req.resident"; reqi="$d/req.indep"
    : >"$reqr"; : >"$reqi"
    obslist=$(obs_for "$fmt")
    for obs in $obslist; do
        a=$(vole_args "$fmt" "$obs")
        [ "$a" = DECLINE ] && continue
        printf '%s\n' "$a" >>"$reqr"
        printf '%s --no-cache\n' "$a" >>"$reqi"
    done
    nreq=$(wc -l <"$reqr" | tr -d ' ')

    # ---- null-overlap control (two largely-disjoint requests, indep mode) ----
    reqn="$d/req.null"; : >"$reqn"
    if [ "$fmt" = pdf ]; then
        printf '%s\n' "--page 1 --kind text --no-cache" >>"$reqn"
        printf '%s\n' "--page 2 --kind text --no-cache" >>"$reqn"
    else
        printf '%s\n' "--block 0 --kind text --no-cache" >>"$reqn"
        printf '%s\n' "--block 1 --kind text --no-cache" >>"$reqn"
    fi

    run_lane() { # $1 tag  $2 reqfile  $3 outfile
        local tag="$1" rf="$2" of="$3" rc t0 t1
        # cold derived cache before every lane
        rm -rf "$d/vstore/cache"
        if [ -z "$field" ]; then
            : >"$of"
            printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "$tag" "3" "0" \
                "$(wc -l <"$rf" | tr -d ' ')" >>"$RAW/lanes.tsv"
            return
        fi
        t0=$(now_us)
        timeout "$OP_TIMEOUT" "$BIN" observe-batch --store "$d/vstore" --packed \
            --field "$field" --requests "$rf" >"$of" 2>"$d/$tag.err"; rc=$?
        t1=$(now_us)
        printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$fmt" "$tag" "$rc" "$(( t1 - t0 ))" \
            "$(wc -l <"$rf" | tr -d ' ')" >>"$RAW/lanes.tsv"
        echo "   lane=$tag rc=$rc wall_us=$(( t1 - t0 ))" >&2
    }

    run_lane resident "$reqr" "$RAW/$id.resident.jsonl"
    run_lane indep    "$reqi" "$RAW/$id.indep.jsonl"
    run_lane indep2   "$reqi" "$RAW/$id.indep2.jsonl"
    run_lane null     "$reqn" "$RAW/$id.null.jsonl"

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
JQ_V=$(jq --version 2>/dev/null)
PY_V=$(python3 --version 2>&1)
BASE_STABLE=$(sed -n 's/^ARG BASE_STABLE=//p' Dockerfile | head -1)
DOC_IMAGE="vole-document/doc-baseline:1.99.0"

cat >"$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "22.3.0 — duplicate work across a heterogeneous observation batch (measurement only)",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_commit": "$COMMIT",
  "git_dirty": "$DIRTY",
  "arch": "$ARCH",
  "service": "doc-baseline",
  "service_caps": "mem_limit == memswap_limit == 6g, pids_limit 4096, cpus 8",
  "service_image": "$DOC_IMAGE",
  "base_image": "$BASE_STABLE",
  "rustc": "$RUSTC_V",
  "cargo": "$CARGO_V",
  "python": "$PY_V",
  "jq": "$JQ_V",
  "cargo_lock_sha256": "$LOCK_SHA",
  "manifest": "$MANIFEST",
  "manifest_sha256": "$MANIFEST_SHA",
  "bin": "$BIN",
  "bin_sha256": "$BIN_SHA",
  "profile": "$PROFILE",
  "subset": "$DEFAULT_IDS",
  "op_timeout_s": $OP_TIMEOUT,
  "lanes": "resident (default: memo+derived cache), indep (--no-cache on every request line), indep2 (repeat of indep), null (two largely-disjoint requests, indep mode)",
  "cold_cache": "rm -rf <store>/cache before every lane",
  "no_cache_gotcha": "the session-level --no-cache on observe-batch is parsed but not propagated; the flag is appended to every request line to disable the memo AND the derived cache",
  "store_builds": "ONCE per document (field-build --profile runtime --packed), outside all lanes",
  "timing": "microseconds via EPOCHREALTIME per lane (recorded only; the court is measurement-only and is not a timing comparison)",
  "accounting": "all metrics are deterministic node counts parsed from the observe-batch JSONL; no statistics or estimators",
  "aggregator": "$AGG",
  "env_affecting_semantics": {"LC_ALL": "C", "PYTHONDONTWRITEBYTECODE": "1"}
}
EOF
cp "$CAMPAIGN/environment.json" "$RAW/environment.json"

cat >"$CAMPAIGN/commands.txt" <<'EOF'
# Phase 22.3.0 — duplicate-work court (measurement only; never the host).
# Each document's packed store is built ONCE, then three lanes run from a cold
# derived cache: resident (memo+cache on), indep (--no-cache on every request
# line; run twice for determinism), and a two-request null-overlap control.
docker compose run --rm --no-TTY doc-baseline bash tools/phase22-3-dup-court.sh

# bounded / explicit population:
docker compose run --rm --no-TTY doc-baseline sh -c 'IDS="nist-pdf-0002 nist-pdf-0004" bash tools/phase22-3-dup-court.sh'

# per-document CLI the court drives (inside the container, /work = repo root):
#   BIN field-build <source> --store <dir> --profile runtime --packed
#   jq -r '.ingest.field // empty' <build.json>
#   rm -rf <dir>/cache
#   BIN observe-batch --store <dir> --packed --field <hex> --requests <req.resident>
#   rm -rf <dir>/cache
#   BIN observe-batch --store <dir> --packed --field <hex> --requests <req.indep>
# aggregate:
#   python3 tools/fixtures/phase22-3-dup.py <campaign>/raw <campaign>
EOF

# --- aggregate + seal -------------------------------------------------------
echo "-- aggregating" >&2
python3 "$AGG" "$RAW" "$CAMPAIGN" | tee "$RAW/summary.txt"

# --- receipt ----------------------------------------------------------------
VERDICT=$(grep '^verdict=' "$CAMPAIGN/counts.txt" | cut -d= -f2)
cat >"$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "22.3.0 — duplicate work across a heterogeneous observation batch (measurement only)",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "measured_commit": "$COMMIT",
  "tree_state": "$DIRTY",
  "profile": "$PROFILE",
  "bin": "$BIN",
  "bin_sha256": "$BIN_SHA",
  "base_image": "$BASE_STABLE",
  "service_image": "$DOC_IMAGE",
  "service": "doc-baseline (mem_limit == memswap_limit == 6g, pids_limit 4096, cpus 8)",
  "rustc": "$RUSTC_V",
  "cargo": "$CARGO_V",
  "cargo_lock_sha256": "$LOCK_SHA",
  "manifest_sha256": "$MANIFEST_SHA",
  "op_timeout_s": $OP_TIMEOUT,
  "court": "tools/phase22-3-dup-court.sh",
  "aggregator": "$AGG",
  "verdict": "$VERDICT",
  "claim": "measurement-only: deterministic per-document counter metrics (exec_dedup, idx_dedup, bytes_dedup, cwrite_dedup) for a known observation batch, plus a secondary closure proxy (dependency_ids/|U|, unsound) and a null-overlap control; the verdict is deterministic (CLEARS / SESSION-ALREADY-CAPTURES / NOTHING-TO-FUSE)",
  "does_not_prove": "node counts are a proxy for work, not time or bytes; dependency_ids is not the executed closure; no production code path change and nothing shipped",
  "rc_codes": "0 ok; 3 decline; 6 unsupported-feature; 124 timeout; 137 SIGKILL/OOM"
}
EOF

chown -R 1000:1000 "$CAMPAIGN" "$WORK" 2>/dev/null || true

echo "phase22.3.0 duplicate-work court: done — $CAMPAIGN (verdict=$VERDICT)"
