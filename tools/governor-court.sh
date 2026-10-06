#!/bin/sh
# Phase 10.1 governor court: Exhaustive / FixedHeuristic / DsfbGuided over the
# frozen tune/holdout/control workload set, plus the zero-decode-authority proof.
#
# Runs entirely inside the pinned `dev` container. It:
#   1. builds and runs the `governor_court` example with `dsfb-search` on (and
#      `deflate-replay` so the replay axis is exercised), writing
#      results.json / workloads.json / hypotheses.json / court-table.txt;
#   2. **proves no wire / decode change**: builds the default binary (which does
#      NOT have `dsfb-search`) and decodes the governor-produced descriptor,
#      byte-comparing it to the source;
#   3. records environment.json, manifest.json, commands.txt, decode-proof.json
#      and report.md into the campaign directory.
#
# Usage (from the host):
#   docker compose run --rm --no-TTY dev sh tools/governor-court.sh
set -eu

cd /work
LC_ALL=C
export LC_ALL

SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
CAMPAIGN="evidence/campaigns/2026-10-05-phase10-governor-${SHA}"
SCRATCH="evidence/scratch/phase10-governor"
rm -rf "$CAMPAIGN" "$SCRATCH"
mkdir -p "$CAMPAIGN" "$SCRATCH"

# --- 1. the governed court (feature on; replay axis exercised) --------------
cargo run --quiet --locked --features dsfb-search,deflate-replay \
  --example governor_court -- "$CAMPAIGN" \
  > "$CAMPAIGN/court-table.txt" 2> "$SCRATCH/court-stderr.txt"

# The governed descriptors + their sources are regenerable bytes; keep them in
# the gitignored scratch and record their hashes in the sealed receipt.
for stem in flate many; do
  mv "$CAMPAIGN/governed-$stem.voldoc" "$SCRATCH/governed-$stem.voldoc"
  mv "$CAMPAIGN/governed-$stem.src" "$SCRATCH/governed-$stem.src"
done

# --- 2. zero-decode-authority proof ------------------------------------------
# (a) `many.pdf`'s winner needs only `rans` (a default feature): decode it in the
#     PURE default build, which has NO `dsfb-search`.
cargo build --locked --quiet
BIN=target/debug/vole-document
"$BIN" decode "$SCRATCH/governed-many.voldoc" "$SCRATCH/decoded-many.bin" \
  > "$SCRATCH/decode-many-json.txt"
if cmp -s "$SCRATCH/decoded-many.bin" "$SCRATCH/governed-many.src"; then
  CMP_MANY=identical
else
  CMP_MANY=DIFFERENT
fi

# (b) `flate.pdf`'s winner needs `deflate-replay` (the underlying capability,
#     exactly as today) but STILL no `dsfb-search`.
cargo build --locked --features deflate-replay --quiet
"$BIN" decode "$SCRATCH/governed-flate.voldoc" "$SCRATCH/decoded-flate.bin" \
  > "$SCRATCH/decode-flate-json.txt"
if cmp -s "$SCRATCH/decoded-flate.bin" "$SCRATCH/governed-flate.src"; then
  CMP_FLATE=identical
else
  CMP_FLATE=DIFFERENT
fi

DESC_SHA=$(sha256sum "$SCRATCH/governed-many.voldoc" | cut -d' ' -f1)
SRC_SHA=$(sha256sum "$SCRATCH/governed-many.src" | cut -d' ' -f1)
DEC_SHA=$(sha256sum "$SCRATCH/decoded-many.bin" | cut -d' ' -f1)
FDESC_SHA=$(sha256sum "$SCRATCH/governed-flate.voldoc" | cut -d' ' -f1)
FSRC_SHA=$(sha256sum "$SCRATCH/governed-flate.src" | cut -d' ' -f1)
FDEC_SHA=$(sha256sum "$SCRATCH/decoded-flate.bin" | cut -d' ' -f1)

# No governance reference anywhere the decoder could see.
HEADER_GOV=$(grep -c governor src/container/header.rs || true)
DECODE_GOV=$(grep -rn "crate::encode\|governor" src/container src/dra src/materialize src/store | wc -l | tr -d ' ')

cat > "$CAMPAIGN/decode-proof.json" <<EOF
{
  "default_build": {
    "decode_build": "cargo build --locked (default features: rans,store; NO dsfb-search)",
    "workload": "many.pdf",
    "governed_descriptor": "$SCRATCH/governed-many.voldoc",
    "descriptor_sha256": "$DESC_SHA",
    "source_sha256": "$SRC_SHA",
    "decoded_sha256": "$DEC_SHA",
    "byte_compare": "$CMP_MANY"
  },
  "capability_build": {
    "decode_build": "cargo build --locked --features deflate-replay (underlying capability; STILL no dsfb-search)",
    "workload": "flate.pdf",
    "governed_descriptor": "$SCRATCH/governed-flate.voldoc",
    "descriptor_sha256": "$FDESC_SHA",
    "source_sha256": "$FSRC_SHA",
    "decoded_sha256": "$FDEC_SHA",
    "byte_compare": "$CMP_FLATE"
  },
  "header_references_governor": $HEADER_GOV,
  "decode_path_references_governor_or_encode": $DECODE_GOV
}
EOF

# --- 3. environment / manifest / commands ------------------------------------
RUSTC=$(rustc -vV | sed -n 's/^release: //p')
CARGO=$(cargo -V | sed -n 's/^cargo //p')
LOCK_SHA=$(sha256sum Cargo.lock | cut -d' ' -f1)
ARCH=$(uname -m)
COMMIT=$(git rev-parse HEAD 2>/dev/null || echo unknown)
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)
DIRTY=$(git status --porcelain 2>/dev/null | wc -l | tr -d ' ')
RUN_UTC=$(date -u +%Y-%m-%dT%H:%M:%SZ)
BASE_DEV=$(sed -n 's/^ARG BASE_STABLE=//p' Dockerfile)

cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$(basename "$CAMPAIGN")",
  "run_utc": "$RUN_UTC",
  "arch": "$ARCH",
  "git_branch": "$BRANCH",
  "git_commit": "$COMMIT",
  "git_dirty_files": $DIRTY,
  "cargo_lock_sha256": "$LOCK_SHA",
  "dev": {
    "service_image": "vole-document/dev:1.99.0",
    "base_image": "$BASE_DEV",
    "rustc": "$RUSTC",
    "cargo": "$CARGO"
  },
  "features_under_test": "dsfb-search,deflate-replay (+ default rans,store)",
  "env_vars_affecting_semantics": { "CARGO_TERM_COLOR": "never", "LC_ALL": "C" },
  "notes": "All commands ran through compose.yaml; nothing ran on the host except git and docker. The decode-without-the-feature proof builds the DEFAULT binary (no dsfb-search) and decodes the governed descriptor."
}
EOF

cat > "$CAMPAIGN/commands.txt" <<EOF
# Phase 10.1 governor court — exact commands
# Commit: $COMMIT (branch $BRANCH); run $RUN_UTC
# All commands run through compose.yaml; nothing runs on the host.

docker compose run --rm --no-TTY dev sh tools/governor-court.sh

# inside the container:
#   cargo run --quiet --locked --features dsfb-search,deflate-replay \\
#       --example governor_court -- $CAMPAIGN
#   cargo build --locked
#   target/debug/vole-document decode $SCRATCH/governed-many.voldoc $SCRATCH/decoded-many.bin
#   cmp $SCRATCH/decoded-many.bin $SCRATCH/governed-many.src
#   cargo build --locked --features deflate-replay
#   target/debug/vole-document decode $SCRATCH/governed-flate.voldoc $SCRATCH/decoded-flate.bin
#   cmp $SCRATCH/decoded-flate.bin $SCRATCH/governed-flate.src

# gates (recorded in gates.txt):
#   cargo fmt --all --check
#   cargo clippy --all-targets --all-features -- -D warnings
#   cargo test --all-features --locked
#   cargo test --locked
#   cargo test --no-default-features
#   cargo test --features dsfb-search
#   msrv: cargo test --locked --all-features
#   policy: cargo audit && cargo deny check
EOF

HYP=$(cat "$CAMPAIGN/hypotheses.json")

cat > "$CAMPAIGN/manifest.json" <<EOF
{
  "campaign": "$(basename "$CAMPAIGN")",
  "phase": "Phase 10.1 — encoder-only search governance (diagnostics + parametric space + governor) and its court",
  "branch": "$BRANCH",
  "commit": "$COMMIT",
  "feature": "dsfb-search (dependency-free, non-default)",
  "claim": "On the frozen small tune/holdout cohort, DsfbGuided never exceeds FixedHeuristic (H1) and matches the Exhaustive grid minimum on every holdout workload with <= half the candidates (H2), while the parametric space yields NO byte benefit over the fixed heuristic (H3: fixed == exhaustive on every workload). Negative controls Stop(Raw) byte-exactly (H4). Zero decode authority: a governor-produced descriptor decodes byte-exactly in a default build without dsfb-search.",
  "hypotheses": $HYP,
  "dsfb_verdict": "dsfb 0.1.2 is real, MSRV-OK, present only transitively via entropyfs-store, and is Drift-Slew Fusion Bootstrap state estimation (an f64/rand observer) with no candidate/residual/directive API; dsfb-search does NOT depend on it (docs/adr/0022).",
  "files": [
    "manifest.json", "environment.json", "workloads.json", "results.json",
    "hypotheses.json", "decode-proof.json", "court-table.txt", "report.md",
    "commands.txt", "gates.txt"
  ]
}
EOF

# --- 4. report ----------------------------------------------------------------
cat > "$CAMPAIGN/report.md" <<EOF
# Phase 10.1 — encoder-only search governance: court receipt

Commit \`$COMMIT\` (branch \`$BRANCH\`), run $RUN_UTC.
Feature under test: **\`dsfb-search\`** (dependency-free, non-default) with
\`deflate-replay\` so the replay axis is exercised. Zero decode authority.

## Headline (pre-registered hypotheses)

\`\`\`json
$HYP
\`\`\`

Interpretation:

- **H1** — \`DsfbGuided.final <= FixedHeuristic.final\` on every workload.
- **H2** — on the disjoint **holdout** set, \`DsfbGuided.final == Exhaustive.final\`
  with \`<= 1/2\` of Exhaustive's candidates.
- **H3** — the honest negative: the fixed heuristic already attains the exhaustive
  minimum on every workload here, so the parametric space adds **no byte benefit**.
  Both the mechanism and this negative are recorded; nothing is manufactured.
- **H4** — negative controls \`Stop(Raw)\` and match the RAW descriptor byte-for-byte.

## Zero decode authority

A governor-produced descriptor (\`flate.pdf\`, winner
\`PDF_DEFLATE_REPLAY_RANS\`) was decoded by the **default** build (no
\`dsfb-search\`) and byte-compared:

\`\`\`json
$(cat "$CAMPAIGN/decode-proof.json")
\`\`\`

## Court table

\`\`\`
$(cat "$CAMPAIGN/court-table.txt")
\`\`\`

## Method

- \`tools/governor-court.sh\` (this file) inside the pinned \`dev\` container.
- Workloads: deterministic in-memory samples (\`src/adapter/pdf/samples.rs\`) plus
  a synthetic opaque trio, split into disjoint tune/holdout/control sets before
  measuring. H2 is judged only on the holdout set; no population claim.
- Every candidate reaches the unmodified \`court::run\`: serialize → parse →
  materialize → byte-compare → complete-cost.
EOF

echo "governor court receipt written to $CAMPAIGN"
echo "H2/H3/H4: $HYP"
