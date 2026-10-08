#!/bin/sh
# Phase 23 — durability: directory-`fsync` (GAP 1) + a model-based power-loss
# proxy (GAP 2).
#
# Closes the two gaps Phase 20.1 left explicitly open:
#
#   * GAP 1: `write_atomic` never `fsync`ed the containing DIRECTORY, so a rename
#     could be lost across a power cut even though the file's contents were
#     synced. Every atomic writer now `fsync`s the parent directory after the
#     rename and `fsync`s the directory when a new segment/index file is created.
#     This court MEASURES the cost (`--dir-sync=safe|off`).
#   * GAP 2: process death does not evict the page cache, so no `fsync` boundary
#     was exercised. This court runs a **model-based power-loss proxy**: every
#     durability barrier is logged (with the byte range it covered), the
#     post-power-loss state is reconstructed as "only barrier-covered bytes
#     survive", and the Phase-20.1 invariants are checked against it — under both
#     `SyncPolicy::Batch` and `Each` and both the fs and packed stores.
#
# Usage (from the host):
#   docker compose run --rm --no-TTY dev sh tools/phase23-powerloss-court.sh
#   docker compose run --rm --no-TTY \
#     -e DOCS="nist-pdf-0002" -e REPS=7 -e SKIP_GATES=1 dev \
#     sh tools/phase23-powerloss-court.sh
set -u

cd /work
export LC_ALL=C

# Artifact-size only; never changes test semantics. The container data root is a
# tight tmpfs, and full debug info overflows it.
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_PROFILE_DEV_INCREMENTAL=false

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=evidence/campaigns/${STAMP}-phase23-durability-${SHA}${TAG:+-$TAG}
RAW=$CAMPAIGN/raw
mkdir -p "$RAW"

DOCS=${DOCS:-nist-pdf-0002}
REPS=${REPS:-7}
SKIP_GATES=${SKIP_GATES:-0}
BASE_STABLE=$(sed -n 's/^ARG BASE_STABLE=//p' Dockerfile | head -1)

echo "=== Phase 23 durability court: campaign $CAMPAIGN ==="
echo "docs: $DOCS  reps: $REPS  skip_gates: $SKIP_GATES"

# ---------------------------------------------------------------------------
# 1. regression gates
# ---------------------------------------------------------------------------
mkdir -p "$RAW/gates"
gate() {
  name="$1"; shift
  echo "--- gate: $name"
  "$@" >"$RAW/gates/$name.log" 2>&1
  rc=$?
  echo "$rc" >"$RAW/gates/$name.rc"
  echo "gate $name rc=$rc"
  return 0
}

GATE_FMT=-1; GATE_CLIPPY=-1; GATE_ALL=-1; GATE_NODEF=-1; GATE_PHASE1=-1
if [ "$SKIP_GATES" != "1" ]; then
  gate fmt        cargo fmt --all --check
  gate clippy     cargo clippy --all-targets --all-features -- -D warnings
  gate allfeat    cargo test --locked --all-features
  gate nodefault  cargo test --locked --no-default-features
  gate phase1     sh tools/phase1-court.sh
  GATE_FMT=$(cat "$RAW/gates/fmt.rc")
  GATE_CLIPPY=$(cat "$RAW/gates/clippy.rc")
  GATE_ALL=$(cat "$RAW/gates/allfeat.rc")
  GATE_NODEF=$(cat "$RAW/gates/nodefault.rc")
  GATE_PHASE1=$(cat "$RAW/gates/phase1.rc")
fi

# ---------------------------------------------------------------------------
# 2. GAP 1 — measure the directory-`fsync` cost
# ---------------------------------------------------------------------------
# A plain (default-feature) binary, so the timing is not perturbed by the
# power-log instrumentation.
cargo build --locked >"$RAW/gates/build-plain.log" 2>&1
BUILD_RC=$?
echo "plain build rc=$BUILD_RC"

TIMING="$RAW/dirsync-timing.tsv"
printf 'backend\tdir_sync\tfilesystem\treps\tmedian_ms\tmin_ms\tmax_ms\n' >"$TIMING"

median() {
  # $1 = file of one-per-line integers
  n=$(wc -l <"$1")
  sort -n "$1" | sed -n "$(( n / 2 + 1 ))p"
}

measure() {
  backend="$1"; dirsync="$2"; fslabel="$3"; base="$4"
  packed_flag=""
  [ "$backend" = "packed" ] && packed_flag="--packed"
  i=0
  : >"$base.times"
  while [ "$i" -lt "$REPS" ]; do
    store="$base/s-$dirsync-$i"
    rm -rf "$store"
    b=$(date +%s%N)
    ./target/debug/vole-document field-build "$DOC" --store "$store" \
      --profile runtime $packed_flag --sync=batch --dir-sync="$dirsync" \
      >/dev/null 2>&1
    e=$(date +%s%N)
    echo $(( (e - b) / 1000000 )) >>"$base.times"
    rm -rf "$store"
    i=$(( i + 1 ))
  done
  med=$(median "$base.times")
  mn=$(sort -n "$base.times" | head -1)
  mx=$(sort -n "$base.times" | tail -1)
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$backend" "$dirsync" "$fslabel" "$REPS" "$med" "$mn" "$mx" >>"$TIMING"
  echo "timing: $backend $dirsync $fslabel median=${med}ms (min=${mn} max=${mx})"
}

DOC=$(find real100-v1/documents -name "${DOCS%% *}.pdf" | head -1)
if [ -z "$DOC" ]; then DOC=$(find real100-v1/documents -name "*.pdf" | head -1); fi
echo "proxy document: $DOC"

# The cost of a directory fsync is invisible on the container's tmpfs data-root,
# and /work/target is a (tmpfs) named volume. Measure on the real host bind mount
# (`/work`, via the gitignored evidence/scratch) and, for a visible signal, on a
# larger document than the fast proxy uses. MEASURE_DOC overrides.
MEASURE_DOC=${MEASURE_DOC:-nist-pdf-0017}
MDOC=$(find real100-v1/documents -name "${MEASURE_DOC}.pdf" | head -1)
[ -z "$MDOC" ] && MDOC=$DOC
echo "measured document: $MDOC   host fs: $(stat -f -c %T /work 2>/dev/null || echo unknown)"

mkdir -p /tmp/phase23-measure /work/evidence/scratch/phase23-measure
for backend in fs packed; do
  for dirsync in off safe; do
    measure "$backend" "$dirsync" tmp  /tmp/phase23-measure/${backend}
    measure "$backend" "$dirsync" bind /work/evidence/scratch/phase23-measure/${backend}
  done
done
rm -rf /tmp/phase23-measure /work/evidence/scratch/phase23-measure

# ---------------------------------------------------------------------------
# 3. GAP 2 — the model-based power-loss proxy
# ---------------------------------------------------------------------------
export PHASE23_PROXY_RAW=$RAW/proxy.tsv
export PHASE23_DOCS="$DOCS"
export PHASE23_PROXY_SCRATCH=${PHASE23_PROXY_SCRATCH:-/tmp/phase23-proxy}

echo "=== power-loss proxy (features power-log,fault-inject) ==="
cargo test --locked --features "power-log,fault-inject" --test power_loss_proxy \
  -- --ignored --nocapture --test-threads=1 >"$RAW/proxy.log" 2>&1
PROXY_RC=$?
tail -n 8 "$RAW/proxy.log"

# ---------------------------------------------------------------------------
# 4. aggregate
# ---------------------------------------------------------------------------
sh tools/fixtures/phase23-powerloss-aggregate.sh "$RAW/proxy.tsv" "$TIMING" "$CAMPAIGN"
cat "$CAMPAIGN/summary.json"

cnt() { sed -n "s/^$1=//p" "$CAMPAIGN/counts.txt"; }

# ---------------------------------------------------------------------------
# 4b. SUMMARY.md (narrative + the GAP-1 cost table + the MATRIX grid)
# ---------------------------------------------------------------------------
TIMING_HDRLESS=$(mktemp)
tail -n +2 "$TIMING" >"$TIMING_HDRLESS"
{
  echo "# Phase 23 — durability: directory fsync (GAP 1) + a model-based power-loss proxy (GAP 2)"
  echo
  echo "## Question"
  echo
  echo "Phase 20.1 left two gaps open: (1) \`write_atomic\` never \`fsync\`ed the"
  echo "containing **directory**, so a rename could be lost across a power cut even"
  echo "though the file's contents were synced; (2) process death does not evict the"
  echo "page cache, so **no \`fsync\` boundary was exercised** and \`SyncPolicy::Batch\`"
  echo "and \`Each\` were indistinguishable — not evidence that batching is power-safe."
  echo
  echo "## GAP 1 — the change and its measured cost"
  echo
  echo "Every atomic writer now dir-fsyncs after the rename and on new segment/index"
  echo "creation: \`field::write_atomic\`, \`FsSeedStore::put_node\`,"
  echo "\`FsIndexStore::put\`, and the packed writer's \`ensure_open\`/\`seal\`. The"
  echo "default is \`DirSyncPolicy::Safe\`; \`--dir-sync=off\` is the explicit escape"
  echo "hatch used only to measure and to show the barrier is load-bearing."
  echo
  echo "| backend | dir_sync | filesystem | reps | median ms | min | max |"
  echo "|---|---|---|---|---|---|---|"
  while IFS="$(printf '\t')" read -r b q f r md mn mx; do
    echo "| $b | $q | $f | $r | $md | $mn | $mx |"
  done <"$TIMING_HDRLESS"
  echo
  echo "Cost is measured with the plain default-feature binary (no barrier-log"
  echo "overhead), median of \`$REPS\` fresh ingests of \`$MDOC\` (the fs store"
  echo "writes one dir-fsynced file per seed/index node; the packed store dir-fsyncs"
  echo "once per new segment and per sealed index)."
  echo
  echo "## GAP 2 — the model-based power-loss proxy"
  echo
  echo "Under the non-default \`power-log\` feature every durability barrier is logged"
  echo "in program order with the byte range it covered (\`fsync <path> <len>\`), plus"
  echo "\`create\`/\`rename\`/\`dirsync\`. \`tests/power_loss_proxy.rs\` folds that log"
  echo "into a per-path durability model — a file survives only if its create/rename"
  echo "was followed by a parent \`dirsync\`, and its content is truncated to the length"
  echo "at its last completed file barrier (0 if none) — then checks the Phase-20.1"
  echo "invariants against the reconstructed store: with no manifest it reopens, no"
  echo "partial node is fetchable, every present id re-hashes; with a manifest the root"
  echo "node and all index nodes exist and \`materialize --exact\` equals the source"
  echo "(length + byte compare, hence SHA-256), and cold observations match a clean"
  echo "baseline. Batch and Each x fs and packed, complete and cut (abort at"
  echo "\`record.after_body\`, \`flush.*\`, \`seal.before_idx\`, \`manifest.after_publish\`)."
  echo
  echo "## Result"
  echo
  echo "$(cnt verdict) — cases $(cnt total), PASS $(cnt pass), FAIL $(cnt fail), CRITICAL $(cnt critical)."
  echo "Shipped-arm CRITICAL: **$(cnt shipped_critical)**. Counterfactual (\`model-drop\`)"
  echo "CRITICAL: **$(cnt modeldrop_critical)** (expected; proves the model is sensitive)."
  echo "Complete \`safe\` ingests with a surviving manifest: $(cnt complete_safe_with_manifest)/$(cnt complete_safe_total)."
  echo "Complete \`off\` ingests whose published store was entirely lost: $(cnt complete_off_manifest_lost)/$(cnt complete_off_total)."
  echo
  echo "## Critical findings"
  echo
  if [ "$(cnt shipped_critical)" = "0" ]; then
    echo "None. Under the model, no shipped arm (\`safe\` or \`off\`, complete or cut)"
    echo "lost or corrupted a published field: every present id re-hashed, every cut"
    echo "store that could not serve failed closed with a typed error, and every"
    echo "reconstructed published manifest materialized exactly. The \`model-drop\` arms"
    echo "discard a directory barrier and are all CRITICAL, so the model would have"
    echo "flagged the pre-GAP-1 code (which did exactly that)."
  else
    echo "> **CRITICAL on a shipped arm** — see the affected rows in the table below."
  fi
  echo
  cat "$CAMPAIGN/MATRIX.md"
  echo
  echo "## What the model does and does not prove"
  echo
  echo "**Proves (under the model).** With \`DirSyncPolicy::Safe\`, a completed ingest's"
  echo "published manifest and the root/index nodes it references survive a simulated"
  echo "power loss; with \`--dir-sync=off\` the same run's entire published store is"
  echo "reconstructed as lost — so the directory fsync is required for the invariant,"
  echo "not decorative. Batch and Each are equivalent **for the published-manifest"
  echo "invariant** because \`put_field\` flushes the open segment before publishing;"
  echo "they differ only for the unreferenced open-segment tail (visible at the"
  echo "\`flush.before_sync\` cut: Batch loses the appended records, Each keeps them)."
  echo
  echo "**Does not prove.** This is a MODEL derived from a barrier log, not a real"
  echo "power cut: it does not exercise the storage device's volatile write cache, the"
  echo "filesystem journal, torn sectors, or a real directory-entry loss, and it assumes"
  echo "directory metadata is durable except for the file-entry barriers it tracks. A"
  echo "physical proxy inside the pinned, unprivileged, hard-capped \`dev\` lane was not"
  echo "available: \`drop_caches\` cannot discard dirty (un-fsynced) pages and"
  echo "\`dm-flakey\`/loopback fault devices need privileges the lane does not have."
  echo "Residual: a true hardware power cut could still lose a rename that the kernel"
  echo "had not yet written to the directory, or reorder writes the device coalesced,"
  echo "in ways a software model cannot see."
  echo
  echo "## Gates"
  echo
  echo "fmt=$GATE_FMT clippy=$GATE_CLIPPY test-all-features=$GATE_ALL test-no-default=$GATE_NODEF phase1=$GATE_PHASE1 (0 = pass, -1 = skipped)."
  echo
  echo "Receipt: \`$CAMPAIGN/receipt.json\`. Raw: \`$RAW/proxy.tsv\`, \`$RAW/dirsync-timing.tsv\`, \`$RAW/proxy.log\`."
} >"$CAMPAIGN/SUMMARY.md"
rm -f "$TIMING_HDRLESS"

# ---------------------------------------------------------------------------
# 5. commands / environment / receipt
# ---------------------------------------------------------------------------
cat >"$CAMPAIGN/commands.txt" <<EOF
# Phase 23 durability court — exact commands (all inside the pinned \`dev\` service)
docker compose run --rm --no-TTY dev sh tools/phase23-powerloss-court.sh

# gates
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked --all-features
cargo test --locked --no-default-features
sh tools/phase1-court.sh

# GAP 1 measurement (default-feature binary; per (backend, dir_sync, filesystem)):
cargo build --locked
./target/debug/vole-document field-build \$DOC --store \$STORE --profile runtime [--packed] --sync=batch --dir-sync=safe|off

# GAP 2 proxy (env: PHASE23_PROXY_RAW, PHASE23_DOCS, PHASE23_PROXY_SCRATCH;
#   the child runs also set VOLE_POWER_LOG / VOLE_POWER_ROOT / VOLE_FAULT_POINT)
cargo test --locked --features "power-log,fault-inject" --test power_loss_proxy -- --ignored --nocapture --test-threads=1

# aggregate
sh tools/fixtures/phase23-powerloss-aggregate.sh $RAW/proxy.tsv $RAW/dirsync-timing.tsv $CAMPAIGN
EOF

DIRTY=$(git status --porcelain 2>/dev/null | tr '\n' ';' | sed 's/"/\\"/g')
LOCK_SHA=$(sha256sum Cargo.lock | cut -d' ' -f1)
cat >"$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "23 — directory fsync (GAP 1) + model-based power-loss proxy (GAP 2)",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "git_dirty": "$DIRTY",
  "arch": "$(uname -m)",
  "service": "dev",
  "service_caps": "mem_limit == memswap_limit == 8g, pids_limit 4096",
  "base_image": "$BASE_STABLE",
  "dev_image": "vole-document/dev:1.99.0",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "cargo_lock_sha256": "$LOCK_SHA",
  "bin": "target/debug/vole-document",
  "docs": "$DOCS",
  "reps": "$REPS",
  "doc_measured": "$MDOC",
  "host_fs": "$(stat -f -c %T /work 2>/dev/null || echo unknown)",
  "features_proxy": "power-log, fault-inject",
  "env_affecting_semantics": {
    "LC_ALL": "C",
    "CARGO_PROFILE_DEV_DEBUG": "0 (artifact size only)",
    "CARGO_PROFILE_TEST_DEBUG": "0 (artifact size only)",
    "CARGO_PROFILE_DEV_INCREMENTAL": "false (artifact size only)",
    "REPS": "$REPS"
  }
}
EOF

cat >"$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "23 — directory fsync (GAP 1) + model-based power-loss proxy (GAP 2)",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "measured_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "tree_state": "$DIRTY",
  "base_image": "$BASE_STABLE",
  "service": "dev (mem_limit == memswap_limit == 8g, pids_limit 4096)",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "cargo_lock_sha256": "$LOCK_SHA",
  "gap1": "parent-directory fsync after every atomic rename; directory fsync on new packed segment / index file; DirSyncPolicy::Safe (default), --dir-sync=off escape hatch",
  "gap2_proxy": "tests/power_loss_proxy.rs (features power-log,fault-inject): barrier log -> reconstructed post-power-loss state -> Phase-20.1 invariants",
  "docs": "$DOCS",
  "reps": "$REPS",
  "proxy_cases": $(cnt total),
  "proxy_pass": $(cnt pass),
  "proxy_fail": $(cnt fail),
  "proxy_critical": $(cnt critical),
  "proxy_shipped_critical": $(cnt shipped_critical),
  "proxy_modeldrop_critical": $(cnt modeldrop_critical),
  "proxy_verdict": "$(cnt verdict)",
  "proxy_rc": $PROXY_RC,
  "build_rc": $BUILD_RC,
  "gates": {
    "fmt": $GATE_FMT,
    "clippy_all_features": $GATE_CLIPPY,
    "test_all_features": $GATE_ALL,
    "test_no_default_features": $GATE_NODEF,
    "phase1_court": $GATE_PHASE1
  },
  "claim": "Under the model (only bytes covered by a completed barrier survive), with the default DirSyncPolicy::Safe every completed ingest's published manifest and the root/index nodes it references survive the reconstructed power loss and materialize exactly; with --dir-sync=off the entire published store is lost, showing the directory fsync is load-bearing, not decorative.",
  "does_not_prove": "This is a MODEL derived from a barrier log, not a real power cut. It does not exercise the storage device's write cache, the filesystem journal, torn-sector behavior, or a real directory-entry loss; it assumes directory metadata is durable except for the file-entry barriers the model tracks."
}
EOF

chown -R 1000:1000 "$CAMPAIGN" 2>/dev/null || true
for d in evidence/campaigns/${STAMP}-phase1-${SHA}*; do
  [ -e "$d" ] && chown -R 1000:1000 "$d" 2>/dev/null || true
done

VERDICT=$(cnt verdict)
echo "=== proxy verdict: $VERDICT (cases=$(cnt total) pass=$(cnt pass) fail=$(cnt fail) critical=$(cnt critical) shipped_critical=$(cnt shipped_critical)) ==="
echo "receipt: $CAMPAIGN/receipt.json"

BAD=0
[ "$(cnt shipped_critical)" = "0" ] || BAD=1
[ "$(cnt fail)" = "0" ] || BAD=1
[ "$PROXY_RC" = "0" ] || BAD=1
[ "$BUILD_RC" = "0" ] || BAD=1
for g in "$GATE_FMT" "$GATE_CLIPPY" "$GATE_ALL" "$GATE_NODEF" "$GATE_PHASE1"; do
  [ "$g" = "0" ] || [ "$g" = "-1" ] || BAD=1
done
if [ "$BAD" -ne 0 ]; then
  echo "PHASE 23 DURABILITY COURT: FAIL" >&2
  exit 1
fi
echo "PHASE 23 DURABILITY COURT: PASS"
