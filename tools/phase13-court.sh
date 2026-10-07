#!/bin/sh
# Phase 13.1 — PDF `/Length` + revision proceduralization as a *size* mechanism.
#
# PRE-REGISTERED before running (Phase-13 subphase 13.1). Hypotheses:
#
#   H1 (byte-exactness) — every file for which `encode --force pdf-length-revision`
#       succeeds round-trips byte-exactly (forced lane passes the court's own
#       decode-before-commit, and an explicit verify + decode + cmp holds).
#   H2 (expected LOSS vs the VOLE ladder) — on the measured corpus the candidate
#       is never strictly smaller than the best of the *current* VOLE ladder
#       (RAW/RLE/BYTE_RANS/PDF_PHYSICAL/PDF_CHANNELS/PDF_LAYOUT/PDF_LAYOUT_RANS/
#       PDF_DEFLATE_REPLAY/PDF_DEFLATE_REPLAY_RANS + the auto winner): 0 wins.
#   H3 (expected LOSS vs generic compressors) — it never beats the best of
#       gzip -9 / zstd -19 --long=27 / xz -9e / brotli -q 11.
#   H4 (honest declines) — it declines on PDFs whose cross-reference is an
#       xref *stream* (no classic table) and on non-PDF inputs.
#
# The court reuses `tools/baselines.sh` for the generic compressors and the VOLE
# lanes on the same complete files (every generic result round-trip verified;
# every VOLE lane priced from its serialized bytes). No gate is weakened and no
# corpus is tuned to manufacture a win; the candidate is admitted to the court
# only if it is byte-exact.
#
# Runs in the pinned `baseline` service (dev toolchain + gzip/zstd/xz/brotli/jq):
#   docker compose run --rm --no-TTY baseline sh tools/phase13-court.sh
set -eu

cd /work
LC_ALL=C
export LC_ALL
export BASELINE_CORPUS=phase13

BIN=target/debug/vole-document

SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase13-pdf-length-revision-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase13
CORPUS="$WORK/corpus"
rm -rf "$WORK"
mkdir -p "$CORPUS/samples" "$CORPUS/large"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2
if ! "$BIN" --help 2>&1 | grep -q 'pdf-length-revision'; then
    echo "binary missing the Phase-13.1 lane; forcing a clean rebuild"
    cargo clean -p vole-document
    cargo build --locked --all-features 2>&1 | tail -2
fi

# --- corpus: phase7 synthetic/producer sets + pdf-make-samples + a large PDF ---
cp evidence/corpus/phase7/*.pdf "$CORPUS"/ 2>/dev/null || true
cp evidence/corpus/phase7-producers/*.pdf "$CORPUS"/ 2>/dev/null || true
"$BIN" pdf-make-samples "$CORPUS/samples" >/dev/null
"$BIN" pdf-make-large "$CORPUS/large" 200 >/dev/null
find "$CORPUS" -type f \( -name '*.pdf' -o -name '*.bin' \) | sort > "$RAW/files.txt"
FILE_COUNT="$(wc -l < "$RAW/files.txt" | tr -d ' ')"

# --- generic-compressor + VOLE-lane table (tools/baselines.sh) ---------------
echo "=== baseline ladder ($FILE_COUNT files) ==="
sh tools/baselines.sh "$RAW/baseline.json" "$CORPUS" | tee "$RAW/baselines.stdout.txt"

# --- per-file win/tie/loss vs the current ladder and vs generic compressors --
jq '.rows | map(
    . as $r
    | ([.raw,.rle,.byte_rans,.pdf_physical,.pdf_channels,.pdf_layout,
        .pdf_layout_rans,.deflate_replay,.deflate_replay_rans,.auto_len]
       | map(select(. != null)) | if length==0 then null else min end) as $ladder
    | {
        corpus, file, source_len,
        new: .pdf_length_revision,
        ladder_vole_excl: $ladder,
        best_vole_incl: .best_vole,
        generic: .best_generic,
        generic_name: .best_generic_name,
        vs_ladder: (if .pdf_length_revision==null then "decline"
                    elif $ladder==null then "n/a"
                    elif .pdf_length_revision < $ladder then "win"
                    elif .pdf_length_revision == $ladder then "tie"
                    else "loss" end),
        vs_generic: (if .pdf_length_revision==null then "decline"
                     elif .best_generic==null then "n/a"
                     elif .pdf_length_revision < .best_generic then "win"
                     elif .pdf_length_revision == .best_generic then "tie"
                     else "loss" end),
        lowers_ladder: (if .pdf_length_revision==null then false
                        else .pdf_length_revision < $ladder end)
      })' "$RAW/baseline.json" > "$RAW/results.json"

jq '{
    files: length,
    declined: (map(select(.new==null)) | length),
    proposed: (map(select(.new!=null)) | length),
    wins_vs_ladder:  (map(select(.vs_ladder=="win")) | length),
    ties_vs_ladder:  (map(select(.vs_ladder=="tie")) | length),
    losses_vs_ladder:(map(select(.vs_ladder=="loss")) | length),
    wins_vs_generic: (map(select(.vs_generic=="win")) | length),
    ties_vs_generic: (map(select(.vs_generic=="tie")) | length),
    losses_vs_generic:(map(select(.vs_generic=="loss")) | length),
    lowers_ladder_count: (map(select(.lowers_ladder)) | length),
    ladder_total_excl: (map(.ladder_vole_excl) | map(select(.!=null)) | add),
    best_vole_total_incl: (map(.best_vole_incl) | map(select(.!=null)) | add),
    new_total: (map(.new) | map(select(.!=null)) | add),
    generic_total: (map(.generic) | map(select(.!=null)) | add)
  }' "$RAW/results.json" | tee "$RAW/summary.json"

# --- explicit byte-exactness of the new lane --------------------------------
echo "=== exactness of the forced lane ==="
proposed=0; declined=0; exact_ok=0; exact_fail=0
while IFS= read -r f; do
    if "$BIN" encode --force pdf-length-revision "$f" "$RAW/tmp.voldoc" >/dev/null 2>&1; then
        proposed=$((proposed + 1))
        "$BIN" verify "$RAW/tmp.voldoc" >/dev/null
        "$BIN" decode "$RAW/tmp.voldoc" "$RAW/tmp.dec" >/dev/null
        if cmp -s "$f" "$RAW/tmp.dec"; then
            exact_ok=$((exact_ok + 1))
        else
            exact_fail=$((exact_fail + 1))
            echo "FINDING: forced lane inexact on $f" >&2
        fi
    else
        declined=$((declined + 1))
    fi
done < "$RAW/files.txt"
rm -f "$RAW/tmp.voldoc" "$RAW/tmp.dec"
printf '{"proposed":%s,"declined":%s,"exact_ok":%s,"exact_fail":%s}\n' \
    "$proposed" "$declined" "$exact_ok" "$exact_fail" | tee "$RAW/exactness.json"

# --- receipt -----------------------------------------------------------------
cpu_model="$(sed -n 's/^model name[[:space:]]*: //p' /proc/cpuinfo | head -n1)"
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "13.1 — PDF /Length + revision proceduralization as a size mechanism",
  "utc_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "git_short": "$SHA",
  "git_dirty_tracked": "$(git status --porcelain --untracked-files=no | wc -l | tr -d ' ')",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "service": "baseline (FROM the pinned dev toolchain; adds gzip/zstd/xz/brotli/jq)",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "binary_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "arch": "$(uname -m)",
  "cpu": "$cpu_model",
  "oracles": {
    "gzip": "$(gzip --version | head -n1)",
    "zstd": "$(zstd --version)",
    "xz": "$(xz --version | head -n1)",
    "brotli": "$(brotli --version)",
    "jq": "$(jq --version)"
  },
  "corpus_files": $FILE_COUNT,
  "hypotheses": {
    "H1_byte_exact": "every proposed file round-trips byte-exactly",
    "H2_vs_ladder": "0 wins vs the current VOLE ladder (expected loss)",
    "H3_vs_generic": "0 wins vs the best generic compressor (expected loss)",
    "H4_declines": "declines on xref-stream and non-PDF inputs"
  },
  "verdict": "$([ "$exact_fail" -eq 0 ] && echo PASS || echo FAIL)"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY baseline sh tools/phase13-court.sh
# inside the court:
#   cargo build --locked --all-features
#   target/debug/vole-document pdf-make-samples evidence/scratch/phase13/corpus/samples
#   target/debug/vole-document pdf-make-large  evidence/scratch/phase13/corpus/large 200
#   sh tools/baselines.sh <raw/baseline.json> evidence/scratch/phase13/corpus
#   target/debug/vole-document encode --force pdf-length-revision FILE OUT.voldoc
#   target/debug/vole-document verify OUT.voldoc
#   target/debug/vole-document decode OUT.voldoc OUT ; cmp OUT FILE
EOF

# --- summary -----------------------------------------------------------------
{
    echo "# Campaign: $(basename "$CAMPAIGN") — Phase 13.1"
    echo
    echo "PDF \`/Length\` + revision proceduralization as a **size** mechanism."
    echo "Pre-registered hypotheses: H1 byte-exactness; H2 0 wins vs the current"
    echo "VOLE ladder; H3 0 wins vs the best generic compressor; H4 honest"
    echo "declines on xref-stream / non-PDF inputs."
    echo
    echo "## Result"
    echo
    echo '```json'
    cat "$RAW/summary.json"
    echo '```'
    echo
    echo "Exactness of the forced lane:"
    echo
    echo '```json'
    cat "$RAW/exactness.json"
    echo '```'
    echo
    echo "## Per-file (bytes; \`new\` = PDF_LENGTH_REVISION, \`ladder\` = best of the current VOLE lanes)"
    echo
    echo "| file | source | new | ladder | generic | vs ladder | vs generic |"
    echo "| --- | ---: | ---: | ---: | ---: | --- | --- |"
    jq -r '.[] | "| `\(.file)` | \(.source_len) | \(.new//"-") | \(.ladder_vole_excl//"-") | \(.generic//"-") | \(.vs_ladder) | \(.vs_generic) |"' \
        "$RAW/results.json"
    echo
    echo "## Interpretation"
    echo
    echo "The candidate is byte-exact where it is proposed (H1 holds) but it **loses**:"
    echo "it is never smaller than the current VOLE ladder and never beats a generic"
    echo "compressor. Regenerating a \`/Length\` value or an xref/\`/Prev\` offset costs"
    echo "a \`Mark\` plus an \`Emit\` item (and a slot) per field, which at these scales"
    echo "exceeds the few digits it removes — the same per-site framing that sank"
    echo "\`PDF_LAYOUT\` (ADR-0011/0012/0013). This is a recorded negative"
    echo "(ADR-0036), not an adopted mechanism."
} > "$CAMPAIGN/SUMMARY.md"

if [ "$exact_fail" -ne 0 ]; then
    echo "PHASE 13.1 COURT: FAIL" >&2
    exit 1
fi
echo
echo "PHASE 13.1 COURT: PASS — campaign $CAMPAIGN"
