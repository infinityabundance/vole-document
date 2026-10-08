#!/bin/sh
# Phase 20.1 — the crash / power-cut fault-injection court.
#
# Turns ADR-0053's reasoned durability design into an empirically attacked
# claim. In one digest-pinned, hard-capped `dev` container it:
#
#   1. runs the regression gates (fmt, clippy, both cargo-test feature sets,
#      and the Phase-1 exact court);
#   2. runs the Phase-20.1 crash matrix (`tests/crash_recovery.rs`, ignored by
#      default) under BOTH `SyncPolicy::Batch` and `SyncPolicy::Each`, across
#      three injection families (process death, storage corruption, and
#      deterministic in-code abort behind `--features fault-inject`);
#   3. aggregates the per-case table into `MATRIX.md` / `SUMMARY.md`;
#   4. seals `environment.json`, `commands.txt`, `receipt.json`.
#
# Usage (from the host):
#   docker compose run --rm --no-TTY dev sh tools/phase20-crash-court.sh
#   docker compose run --rm --no-TTY \
#     -e DOCS="nist-pdf-0002" -e REPS=1 -e SKIP_GATES=1 dev \
#     sh tools/phase20-crash-court.sh
#
# Honesty: `SIGKILL`/`SIGABRT` do not evict the page cache, so this court does
# NOT prove fsync durability under true power loss; it proves ordering, prefix
# recovery, and the corruption fail-closed path. See `SUMMARY.md`.
set -u

cd /work
export LC_ALL=C

# Keep build artifacts small: the container's Docker data-root is a 26 GiB
# tmpfs, and full debug info overflows it (linker SIGBUS). These variables
# change artifact size only, never test semantics.
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_PROFILE_DEV_INCREMENTAL=false

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=evidence/campaigns/${STAMP}-phase20-crash-${SHA}${TAG:+-$TAG}
RAW=$CAMPAIGN/raw
mkdir -p "$RAW"

DOCS=${DOCS:-"nist-pdf-0017 nist-pdf-0002"}
REPS=${REPS:-2}
SKIP_GATES=${SKIP_GATES:-0}
BASE_STABLE=$(sed -n 's/^ARG BASE_STABLE=//p' Dockerfile | head -1)

export PHASE20_CRASH_RAW=$RAW/cases.tsv
export PHASE20_DOCS="$DOCS"
export PHASE20_REPS="$REPS"
export PHASE20_SCRATCH=${PHASE20_SCRATCH:-/tmp/phase20-scratch}

echo "=== Phase 20.1 crash court: campaign $CAMPAIGN ==="
echo "docs: $DOCS   reps: $REPS   scratch: $PHASE20_SCRATCH"

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
  gate clippy     cargo clippy --locked --all-targets --all-features -- -D warnings
  gate allfeat    cargo test --locked --all-features
  gate nodefault  cargo test --locked --no-default-features
  gate phase1     sh tools/phase1-court.sh
  GATE_FMT=$(cat "$RAW/gates/fmt.rc")
  GATE_CLIPPY=$(cat "$RAW/gates/clippy.rc")
  GATE_ALL=$(cat "$RAW/gates/allfeat.rc")
  GATE_NODEF=$(cat "$RAW/gates/nodefault.rc")
  GATE_PHASE1=$(cat "$RAW/gates/phase1.rc")
fi

echo "=== crash matrix (both policies, families A/B/C) ==="
cargo test --locked --features fault-inject --test crash_recovery \
  -- --ignored --nocapture --test-threads=1 >"$RAW/court.log" 2>&1
COURT_RC=$?
tail -n 20 "$RAW/court.log"

echo "=== aggregate ==="
sh tools/fixtures/phase20-crash-aggregate.sh "$RAW/cases.tsv" "$CAMPAIGN"
cat "$CAMPAIGN/summary.json"

# -------------------- commands.txt --------------------
cat >"$CAMPAIGN/commands.txt" <<EOF
# Phase 20.1 crash court — exact commands (all inside the pinned \`dev\` service)
docker compose run --rm --no-TTY dev sh tools/phase20-crash-court.sh

# gates
cargo fmt --all --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
cargo test --locked --no-default-features
sh tools/phase1-court.sh

# the court itself (env: CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
#   CARGO_PROFILE_DEV_INCREMENTAL=false LC_ALL=C PHASE20_CRASH_RAW=$RAW/cases.tsv
#   PHASE20_DOCS="$DOCS" PHASE20_REPS=$REPS PHASE20_SCRATCH=$PHASE20_SCRATCH)
cargo test --locked --features fault-inject --test crash_recovery -- --ignored --nocapture --test-threads=1

# aggregate
sh tools/fixtures/phase20-crash-aggregate.sh $RAW/cases.tsv $CAMPAIGN
EOF

# -------------------- environment.json --------------------
DIRTY=$(git status --porcelain 2>/dev/null | tr '\n' ';' | sed 's/"/\\"/g')
LOCK_SHA=$(sha256sum Cargo.lock | cut -d' ' -f1)
cat >"$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "20.1 — crash / power-cut fault-injection court",
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
  "policies": "SyncPolicy::Batch, SyncPolicy::Each",
  "families": "A process death (SIGKILL/SIGABRT); B storage corruption (truncate/flip/zero/.idx-drop); C deterministic abort (--features fault-inject, VOLE_FAULT_POINT)",
  "aggregator": "tools/fixtures/phase20-crash-aggregate.sh",
  "env_affecting_semantics": {
    "LC_ALL": "C",
    "CARGO_PROFILE_DEV_DEBUG": "0 (artifact size only)",
    "CARGO_PROFILE_TEST_DEBUG": "0 (artifact size only)",
    "CARGO_PROFILE_DEV_INCREMENTAL": "false (artifact size only)",
    "PHASE20_REPS": "$REPS",
    "PHASE20_DOCS": "$DOCS"
  }
}
EOF

# -------------------- receipt.json --------------------
getc() { sed -n "s/^$1=//p" "$CAMPAIGN/counts.txt"; }
VERDICT=$(getc verdict)
TOTAL=$(getc total)
PASSN=$(getc pass)
FAILN=$(getc fail)
CRITN=$(getc critical)
cat >"$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "20.1 — crash / power-cut fault-injection court",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "measured_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "tree_state": "$DIRTY",
  "base_image": "$BASE_STABLE",
  "service": "dev (mem_limit == memswap_limit == 8g, pids_limit 4096)",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "cargo_lock_sha256": "$LOCK_SHA",
  "court": "tests/crash_recovery.rs (run via tools/phase20-crash-court.sh)",
  "aggregator": "tools/fixtures/phase20-crash-aggregate.sh",
  "docs": "$DOCS",
  "reps": "$REPS",
  "policies": ["Batch", "Each"],
  "case_count": $TOTAL,
  "pass": $PASSN,
  "fail": $FAILN,
  "critical": $CRITN,
  "verdict": "$VERDICT",
  "court_rc": $COURT_RC,
  "gates": {
    "fmt": $GATE_FMT,
    "clippy_all_features": $GATE_CLIPPY,
    "test_all_features": $GATE_ALL,
    "test_no_default_features": $GATE_NODEF,
    "phase1_court": $GATE_PHASE1
  },
  "claim": "Across process death, on-disk corruption, and deterministic aborts at named writer boundaries, under both sync policies, no injection made the packed seed store serve bytes that do not match the requested id, and every published manifest materialized/observed exactly or failed closed with a typed error.",
  "does_not_prove": "SIGKILL/SIGABRT do not evict the page cache, so fsync durability under true power loss and torn/lost rename are not measured (argued only)."
}
EOF

# Root-owned output -> the invoking user (container runs as root).
chown -R 1000:1000 "$CAMPAIGN" 2>/dev/null || true
for d in evidence/campaigns/${STAMP}-phase1-${SHA}*; do
  [ -e "$d" ] && chown -R 1000:1000 "$d" 2>/dev/null || true
done

echo "=== verdict: $VERDICT (cases=$TOTAL pass=$PASSN fail=$FAILN critical=$CRITN) ==="
echo "receipt: $CAMPAIGN/receipt.json"

BAD=0
[ "$CRITN" = "0" ] || BAD=1
[ "$FAILN" = "0" ] || BAD=1
[ "$COURT_RC" = "0" ] || BAD=1
for g in "$GATE_FMT" "$GATE_CLIPPY" "$GATE_ALL" "$GATE_NODEF" "$GATE_PHASE1"; do
  [ "$g" = "0" ] || [ "$g" = "-1" ] || BAD=1
done
if [ "$BAD" -ne 0 ]; then
  echo "PHASE 20.1 CRASH COURT: FAIL" >&2
  exit 1
fi
echo "PHASE 20.1 CRASH COURT: PASS"
