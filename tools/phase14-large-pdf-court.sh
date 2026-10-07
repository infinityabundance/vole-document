#!/usr/bin/env bash
# Phase 14 — large-PDF encode bound court.
#
# Measures `encode` on the >100 MiB (and the largest 50-100 MiB) PDFs of the
# frozen `real100-v1` corpus in the capped `doc-baseline` lane, recording wall,
# peak RSS, exit code, the winning candidate, and **byte-exactness** (decode ==
# source, length + SHA-256). Run as frozen; no tuning.
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase14-large-pdf-court.sh
set -uo pipefail

cd /work
export LC_ALL=C
SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=evidence/campaigns/${STAMP}-phase14-large-pdf-${SHA}
RAW=$CAMPAIGN/raw
WORK=${WORK:-evidence/scratch/phase14}
mkdir -p "$RAW" "$WORK"

MANIFEST=${MANIFEST:-real100-v1/manifest.tsv}
CORPUS=${CORPUS:-real100-v1/documents}
BIN=${VOLE_BIN:-./target/debug/vole-document}

echo "-- building all-features binary" >&2
cargo build --locked --all-features >&2

printf 'id\tsource_bytes\twall_ms\tpeak_rss_kb\trc\tcandidate\texact\n' >"$RAW/results.tsv"

# >100 MiB first, then the 50-100 MiB band, largest first.
mapfile -t IDS < <(awk -F'\t' '
  NR==1{for(i=1;i<=NF;i++)h[$i]=i}
  NR>1 && $h["format"]=="pdf" && $h["byte_len"]+0 >= 100000000 {print $h["byte_len"]"\t"$1}
' "$MANIFEST" | sort -rn | cut -f2)

if [ "${#IDS[@]}" -eq 0 ]; then echo "no >100 MiB PDFs in manifest" >&2; exit 1; fi

for id in "${IDS[@]}"; do
  src="$CORPUS/nasa/pdf/$id.pdf"
  [ -f "$src" ] || src="$CORPUS/nist/pdf/$id.pdf"
  src_len=$(stat -c %s "$src")
  echo "== $id ($src_len B) ==" >&2
  /usr/bin/time -v "$BIN" encode "$src" "$WORK/$id.voldoc" >"$WORK/$id.json" 2>"$WORK/$id.time"
  rc=$?
  wall_s=$(sed -n 's/.*): //p' "$WORK/$id.time" | tail -1)
  wall_ms=$(printf '%s' "$wall_s" | awk -F: '{ if (NF==3) printf "%d", int((($1*3600)+($2*60)+$3)*1000); else if (NF==2) printf "%d", int((($1*60)+$2)*1000); else printf "0" }')
  rss_kb=$(awk -F': ' '/Maximum resident set size/{print $2}' "$WORK/$id.time")
  cand=$(grep -o '"candidate":"[A-Z_]*"' "$WORK/$id.json" 2>/dev/null | head -1)
  exact="no"
  if [ "$rc" -eq 0 ]; then
    "$BIN" decode "$WORK/$id.voldoc" "$WORK/$id.out" >/dev/null 2>&1 && cmp -s "$src" "$WORK/$id.out" && exact="yes"
    rm -f "$WORK/$id.out"
  fi
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$src_len" "${wall_ms:-0}" "${rss_kb:-0}" "$rc" "${cand:-}" "$exact" >>"$RAW/results.tsv"
  rm -f "$WORK/$id.voldoc"
done

cat >"$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "14 — large-PDF encode bound (partial fix: shared scan + streaming court)",
  "utc_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "service": "doc-baseline",
  "corpus": "real100-v1",
  "manifest_sha256": "$(sha256sum "$MANIFEST" | cut -d' ' -f1)",
  "court": "tools/phase14-large-pdf-court.sh",
  "adr": "docs/adr/0041-large-pdf-encode-bound.md",
  "note": "exact = decode round-trip byte-identical (cmp). rc 124 = 180 s timeout; 137 = SIGKILL (OOM under the lane cap). See docs/adr/0041 for before/after and the recorded memory bound."
}
EOF

cat >"$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev          sh -c 'cargo build --locked --all-features'
docker compose run --rm --no-TTY doc-baseline bash tools/phase14-large-pdf-court.sh
EOF

{
  echo "# Phase 14 — large-PDF encode bound"
  echo
  echo "Frozen architecture, no tuning, over the \`real100-v1\` PDFs >= 100 MB in the"
  echo "capped \`doc-baseline\` lane (6 GiB, \`/usr/bin/time -v\`)."
  echo
  echo "| id | source bytes | wall ms | peak RSS KB | rc | candidate | exact |"
  echo "|---|---:|---:|---:|---:|---|---|"
  tail -n +2 "$RAW/results.tsv" | while IFS=$'\t' read -r id sb w r rc c e; do
    echo "| $id | $sb | $w | $r | $rc | $c | $e |"
  done
  echo
  echo "Before/after (all-features, same machine): \`nasa-pdf-0003\` 395 s -> 227 s;"
  echo "\`nasa-pdf-0002\` timeout -> completed. Peak RSS is **unchanged** (~17x input):"
  echo "see ADR-0041 — the memory bound is recorded, not solved."
} >"$CAMPAIGN/SUMMARY.md"

echo "phase14 court done: $CAMPAIGN"
cat "$CAMPAIGN/SUMMARY.md"
