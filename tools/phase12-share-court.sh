#!/bin/sh
# Phase-12.8 cross-document procedural state court.
#
# Runs entirely inside the pinned `dev` container. It generates a **randomized**
# cohort — a DOCX and an EPUB embedding byte-identical resource bytes, plus a
# structurally identical no-sharing control whose only difference is the resource
# bytes — ingests them through the real field pipeline, and reports:
#
#   * what is genuinely shared under exact content identity (one content-addressed
#     ResourceBlob node id / one stored blob across the DOCX/EPUB boundary, and the
#     decoded member sharing that same blob);
#   * what is not shared (the Phase-11 PDF adapter extracts no embedded resource
#     blob; DEFLATE members and per-source spans are not content identity; the
#     control shares nothing);
#   * `nodes_id_shared` (a representation fact) reported separately from
#     `nodes_reused` (work), and per-document / aggregate persistent bytes written;
#   * `retained_inverse_work_fraction` from cold/warm receipts (never from source
#     size);
#   * `materialize_exact(root) == source` (length + SHA-256 + byte identity) for
#     every root.
#
# No compression claim is made: a shared blob is scored as state/work, never as an
# achieved store-size fraction, and the second document's exact source descriptor
# is stored independently.
#
# Receipt: SUMMARY.md, commands.txt, environment.json and a randomized raw/
# (resource bytes, fixtures, metrics.json, stdout). Usage (from the host):
#   docker compose run --rm --no-TTY dev sh tools/phase12-share-court.sh
set -eu

cd /work
LC_ALL=C
export LC_ALL

SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
CAMPAIGN="${1:-evidence/campaigns/2026-10-06-phase12-share-${SHA}}"
rm -rf "$CAMPAIGN"
mkdir -p "$CAMPAIGN/raw"

# Randomized cohort + full measurement in one feature-gated example. The example
# writes regenerable FieldStore bytes to the gitignored scratch, and only the
# fixtures + metrics.json into the sealed receipt.
cargo run --quiet --locked --all-features --example phase12_share_court -- \
  "$CAMPAIGN" random "evidence/scratch/phase12-share" > "$CAMPAIGN/raw/stdout.txt"

# --- environment / provenance ------------------------------------------------
RUSTC=$(rustc -vV | sed -n 's/^release: //p')
CARGO=$(cargo -V | sed -n 's/^cargo //p')
LOCK_SHA=$(sha256sum Cargo.lock | cut -d' ' -f1)
ARCH=$(uname -m)
COMMIT=$(git rev-parse HEAD 2>/dev/null || echo unknown)
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)
DIRTY=$(git status --porcelain 2>/dev/null | tr '\n' ';')
RUN_UTC=$(date -u +%Y-%m-%dT%H:%M:%SZ)
BASE_DEV=$(sed -n 's/^ARG BASE_STABLE=//p' Dockerfile)

cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$(basename "$CAMPAIGN")",
  "run_utc": "$RUN_UTC",
  "arch": "$ARCH",
  "git_branch": "$BRANCH",
  "git_commit": "$COMMIT",
  "git_dirty": "$DIRTY",
  "cargo_lock_sha256": "$LOCK_SHA",
  "dev": {
    "service_image": "vole-document/dev:1.99.0",
    "base_image": "$BASE_DEV",
    "rustc": "$RUSTC",
    "cargo": "$CARGO"
  },
  "features_under_test": "all-features (field, package, opc, docx, epub, entropyfs-store, deflate-replay, ...)",
  "env_vars_affecting_semantics": { "CARGO_TERM_COLOR": "never", "LC_ALL": "C" },
  "randomized": "the embedded resource bytes are read from /dev/urandom (seed recorded in metrics.json)",
  "notes": "All commands ran through compose.yaml inside the pinned dev image; nothing ran on the host except git and docker. No compression claim: a shared blob is scored as state/work."
}
EOF

cat >> "$CAMPAIGN/SUMMARY.md" <<EOF

---

## Environment (receipt)

- Commit under test: \`$COMMIT\` (branch \`$BRANCH\`), dirty files: \`$DIRTY\`
- Service image: \`vole-document/dev:1.99.0\`; base \`$BASE_DEV\`
- Toolchain: rustc \`$RUSTC\`, cargo \`$CARGO\`, arch \`$ARCH\`
- \`Cargo.lock\` sha256: \`$LOCK_SHA\`
- Run (UTC): $RUN_UTC
EOF

cat > "$CAMPAIGN/commands.txt" <<EOF
# Phase 12.8 cross-document procedural state court — exact commands
# Commit under test: $COMMIT (branch $BRANCH); dirty: $DIRTY
# Run (UTC): $RUN_UTC
# Base image (ARG BASE_STABLE): $BASE_DEV
# Service image: vole-document/dev:1.99.0
# rustc $RUSTC, cargo $CARGO, arch $ARCH
# Cargo.lock sha256: $LOCK_SHA
# All commands run inside the pinned dev image; nothing on the host.

docker compose run --rm --no-TTY dev sh tools/phase12-share-court.sh

# Inside phase12-share-court.sh:
#   cargo run --quiet --locked --all-features --example phase12_share_court -- CAMPAIGN random evidence/scratch/phase12-share
# The example writes into CAMPAIGN/raw/:
#   shared.resource.bin, control.resource.bin      (the exact embedded resource bytes)
#   docx_shared.docx, epub_shared.epub, epub_control.epub  (the randomized cohort)
#   metrics.json                                   (the measured receipt)
# and CAMPAIGN/SUMMARY.md. FieldStore bytes go to the gitignored evidence/scratch/.
EOF

# Integrity of the raw artifacts actually produced.
( cd "$CAMPAIGN" && sha256sum raw/* > raw.sha256 ) 2>/dev/null || true

echo "wrote $CAMPAIGN"
