#!/bin/sh
# Phase 13.4 — byte-level partial-materialization checkpoint records.
#
# PRE-REGISTERED before running (Phase-13 subphase 13.4). Hypotheses:
#
#   H1 (byte-exactness) — a checkpointed descriptor materializes exactly, and the
#       checkpoint lane serves the same bytes as the Phase-8 index lane for every
#       query; it never changes an output byte.
#   H2 (advisory) — a lying/corrupt/non-optional checkpoint is *rejected*: the
#       full parser fails closed and the seek reader falls back to the index lane
#       (identical bytes; it pays only for the rejected read), never to a guess.
#   H3 (work, EXPECTED NEGATIVE) — measured against the Phase-8 seek floor on the
#       same descriptor: the per-op checkpoint table is redundant with the
#       OBSERVATION_INDEX and its record is larger than the index record it lets
#       the reader skip, so bytes_read does not fall and CPU is unchanged. The
#       court reports the measured numbers; it asserts no direction.
#
# Runs in the pinned `dev` service (no external tools):
#   docker compose run --rm --no-TTY dev sh tools/phase13-4-checkpoint-court.sh
set -eu

cd /work
LC_ALL=C
export LC_ALL

SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase13-checkpoints-${SHA}"
RAW="$CAMPAIGN/raw"
mkdir -p "$RAW"

echo "=== court: tests/phase13_checkpoints.rs ==="
set +e
cargo test --all-features --locked --test phase13_checkpoints -- --nocapture 2>&1 | tee "$RAW/court.txt"
court_rc=$?
set -e

verdict="PASS"
[ "$court_rc" -eq 0 ] || verdict="FAIL"

cpu_model="$(sed -n 's/^model name[[:space:]]*: //p' /proc/cpuinfo | head -n1)"
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "13.4 — byte-level partial-materialization checkpoint records",
  "utc_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "git_short": "$SHA",
  "git_dirty_tracked": "$(git status --porcelain --untracked-files=no | wc -l | tr -d ' ')",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "service": "dev",
  "service_image_id": "$(docker inspect --format='{{.Id}}' vole-document/dev:1.99.0 2>/dev/null || echo unknown)",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "arch": "$(uname -m)",
  "cpu": "$cpu_model",
  "oracles": {},
  "env_semantics": { "CARGO_TERM_COLOR": "never" },
  "hypotheses": {
    "H1_byte_exact": "checkpointed descriptor materializes exactly; checkpoint lane == index lane, byte-for-byte",
    "H2_advisory": "lying/corrupt/non-optional checkpoint rejected; reader falls back to the index lane, exact bytes",
    "H3_work_negative": "per-op checkpoint table is redundant with the index and its record is larger; bytes_read does not fall, CPU unchanged"
  },
  "court": "tests/phase13_checkpoints.rs",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
# Phase 13.4 — byte-level checkpoint records (branch phase13)

# Court (integration; prints the measured bytes_read table)
docker compose run --rm --no-TTY dev sh -c 'cargo test --all-features --test phase13_checkpoints -- --nocapture'

# In-file unit/integration tests
docker compose run --rm --no-TTY dev sh -c 'cargo test --all-features --lib checkpoint'

# Full gate (raw transcript: raw/gate.txt, committed verbatim)
docker compose run --rm --no-TTY dev sh -c 'cargo fmt --all && \
  cargo clippy --all-targets --all-features -- -D warnings && \
  cargo clippy -- -D warnings && \
  cargo test --all-features --locked && \
  cargo test --no-default-features && sh tools/check-docs.sh'

# Toolchain / identity probes
docker compose run --rm --no-TTY dev sh -c 'rustc --version; cargo --version; sha256sum Cargo.lock; uname -m; grep -m1 "model name" /proc/cpuinfo'
docker inspect --format='{{.Id}}' vole-document/dev:1.99.0
EOF

echo
if [ "$verdict" = "PASS" ]; then
    echo "PHASE 13.4 COURT: PASS — campaign $CAMPAIGN"
else
    echo "PHASE 13.4 COURT: FAIL — see $RAW/court.txt" >&2
    exit 1
fi
