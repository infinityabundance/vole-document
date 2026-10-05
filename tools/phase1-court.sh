#!/bin/sh
# Phase 1 exact court: runs entirely inside the pinned dev container.
#
# For every corpus item it proves the authoritative triple via the real CLI:
#   materialize(encode(X)) length == len(X)
#   sha256(materialize(encode(X))) == sha256(X)
#   cmp materialize(encode(X)) X == identical
# and records a machine-readable receipt.
#
# Usage (from the host):
#   docker compose run --rm --no-TTY dev sh tools/phase1-court.sh
set -eu

cd /work
BIN=target/debug/vole-document

cargo build --locked --quiet

SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
CAMPAIGN="evidence/campaigns/$(date -u +%Y-%m-%d)-phase1-${SHA}"
WORK="evidence/scratch/phase1"
rm -rf "$WORK"
mkdir -p "$WORK" "$CAMPAIGN"

# ---- deterministic corpus -------------------------------------------------
: > "$WORK/empty.bin"
printf 'A' > "$WORK/one.bin"
head -c 65536 /dev/zero > "$WORK/zeros-64k.bin"

# 256-byte pattern containing every byte value, then repeated to 1 MiB.
i=0
: > "$WORK/pattern256.bin"
while [ "$i" -lt 256 ]; do
  printf "\\$(printf '%03o' "$i")" >> "$WORK/pattern256.bin"
  i=$((i + 1))
done
n=0
: > "$WORK/seq-1m.bin"
while [ "$n" -lt 4096 ]; do
  cat "$WORK/pattern256.bin" >> "$WORK/seq-1m.bin"
  n=$((n + 1))
done

# Text-like document.
n=0
: > "$WORK/text-1m.bin"
while [ "$n" -lt 20000 ]; do
  printf 'The quick brown fox jumps over the lazy dog 0123456789.\n' >> "$WORK/text-1m.bin"
  n=$((n + 1))
done

# A real project file (this repo's manifest).
cp Cargo.toml "$WORK/cargo-toml.bin"

# A minimal synthetic PDF (kept ASCII so the fixture is reproducible).
cat > "$WORK/synthetic.pdf" <<'PDF'
%PDF-1.7
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R >>
endobj
4 0 obj
<< /Length 44 >>
stream
BT /F1 24 Tf 100 700 Td (Hello VOLE) Tj ET
endstream
endobj
trailer
<< /Size 5 /Root 1 0 R >>
startxref
0
%%EOF
PDF

echo "=== corpus ==="
ls -l "$WORK" | tail -n +2

: > "$CAMPAIGN/results.jsonl"

fail=0
first=1
printf '[' > "$CAMPAIGN/results.json"
for f in "$WORK"/*; do
  name="$(basename "$f")"
  enc="$f.voldoc"
  out="$f.decoded"

  "$BIN" encode "$f" "$enc" >/dev/null
  "$BIN" decode "$enc" "$out" >/dev/null
  "$BIN" verify "$enc" >/dev/null

  src_len="$(wc -c < "$f" | tr -d ' ')"
  enc_len="$(wc -c < "$enc" | tr -d ' ')"
  src_sha="$(sha256sum "$f" | cut -d' ' -f1)"
  out_sha="$(sha256sum "$out" | cut -d' ' -f1)"

  cmp_result="equal"
  if ! cmp -s "$f" "$out"; then
    cmp_result="DIFFER"
    fail=1
  fi
  if [ "$src_sha" != "$out_sha" ]; then
    cmp_result="SHA_DIFFER"
    fail=1
  fi

  if [ "$first" -eq 0 ]; then printf ',' >> "$CAMPAIGN/results.json"; fi
  first=0
  printf '{"name":"%s","source_len":%s,"encoded_len":%s,"source_sha256":"%s","reconstructed_sha256":"%s","byte_compare":"%s"}' \
    "$name" "$src_len" "$enc_len" "$src_sha" "$out_sha" "$cmp_result" >> "$CAMPAIGN/results.json"

  printf '{"name":"%s","source_len":%s,"encoded_len":%s,"sha256":"%s","byte_compare":"%s"}\n' \
    "$name" "$src_len" "$enc_len" "$src_sha" "$cmp_result" >> "$CAMPAIGN/results.jsonl"

  printf '%-20s src=%-9s enc=%-9s cmp=%s\n' "$name" "$src_len" "$enc_len" "$cmp_result"
done
printf ']' >> "$CAMPAIGN/results.json"

# ---- attribution + capabilities ------------------------------------------
"$BIN" inspect "$WORK/synthetic.pdf.voldoc" > "$CAMPAIGN/attribution.synthetic-pdf.json"
"$BIN" capabilities > "$CAMPAIGN/capabilities.json"

# ---- environment receipt --------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$(basename "$CAMPAIGN")",
  "utc_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "git_dirty_tracked": "$(git status --porcelain --untracked-files=no | wc -l | tr -d ' ')",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "binary_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "arch": "$(uname -m)",
  "command": "docker compose run --rm --no-TTY dev sh tools/phase1-court.sh"
}
EOF

cat > "$CAMPAIGN/manifest.json" <<EOF
{
  "campaign": "$(basename "$CAMPAIGN")",
  "phase": "Phase 1 — exact .voldoc core",
  "claim": "For every corpus item, materialize(encode(X)) == X byte-for-byte, with a matching SHA-256.",
  "results_sha256": "$(sha256sum "$CAMPAIGN/results.jsonl" | cut -d' ' -f1)",
  "environment_sha256": "$(sha256sum "$CAMPAIGN/environment.json" | cut -d' ' -f1)",
  "attribution_sha256": "$(sha256sum "$CAMPAIGN/attribution.synthetic-pdf.json" | cut -d' ' -f1)",
  "capabilities_sha256": "$(sha256sum "$CAMPAIGN/capabilities.json" | cut -d' ' -f1)",
  "verdict": "$( [ "$fail" -eq 0 ] && echo PASS || echo FAIL )"
}
EOF

echo "=== campaign: $CAMPAIGN ==="
cat "$CAMPAIGN/manifest.json"

if [ "$fail" -ne 0 ]; then
  echo "PHASE 1 COURT: FAIL" >&2
  exit 1
fi
echo "PHASE 1 COURT: PASS"
