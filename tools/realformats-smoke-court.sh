#!/usr/bin/env bash
# Phase 21.5.3 (FIX 4) — stratified real-world multi-format smoke court.
#
# Ingests the pinned real samples acquired by tools/realcorpus/fetch-formats.sh
# (gitignored `realformats-v1/documents/`) and reports **pooled costs ALONGSIDE
# medians**, stratified by format and by size class, so the size/time-weighted
# picture the reviewer worried about is visible next to the unweighted one.
#
#   docker compose run --rm --no-TTY realcorpus sh tools/realcorpus/fetch-formats.sh
#   docker compose run --rm --no-TTY doc-baseline bash tools/realformats-smoke-court.sh
#
# If samples are absent it runs what it can and records SKIPPED rows (it does NOT
# fabricate). FAILs only if a *present* sample is not byte-exact.

set -uo pipefail
cd /work
export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1

BIN=${VOLE_BIN:-target/debug/vole-document}
SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN="evidence/campaigns/${STAMP}-realformats-smoke-${SHA}"
RAW="$CAMPAIGN/raw"
MANIFEST=tools/realcorpus/formats-manifest.tsv
CORPUS=realformats-v1

rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2
[ -x "$BIN" ] || { echo "realformats-smoke: $BIN missing; refusing to run" >&2; exit 1; }

echo "=== run the smoke court ==="
set +e
python3 tools/fixtures/realformats-smoke.py --bin "$BIN" --corpus "$CORPUS" \
    --manifest "$MANIFEST" --out "$RAW"
RC=$?
set -e
VERDICT=$(python3 -c "import json;print(json.load(open('$RAW/smoke.json'))['verdict'])")

cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.5.3 — stratified real-world multi-format smoke",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "measured_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "tree_state": "$(git status --porcelain | tr '\n' ' ' | sed 's/"/\\"/g')",
  "service": "doc-baseline",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "python": "$(python3 --version)",
  "arch": "$(uname -m)",
  "bin": "$BIN",
  "bin_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "manifest_sha256": "$(sha256sum "$MANIFEST" | cut -d' ' -f1)",
  "driver_sha256": "$(sha256sum tools/fixtures/realformats-smoke.py | cut -d' ' -f1)"
}
EOF
cp "$RAW/smoke.json" "$CAMPAIGN/receipt.json"

cat > "$CAMPAIGN/commands.txt" <<'EOF'
# acquisition (network-capable, capped realcorpus lane):
docker compose run --rm --no-TTY realcorpus sh tools/realcorpus/fetch-formats.sh
# smoke court (pinned doc-baseline lane; VOLE + python3):
docker compose run --rm --no-TTY doc-baseline bash tools/realformats-smoke-court.sh
# inside the court:
#   cargo build --locked --all-features
#   python3 tools/fixtures/realformats-smoke.py --bin <BIN> --corpus realformats-v1 \
#       --manifest tools/realcorpus/formats-manifest.tsv --out <RAW>
EOF

{
    echo "# Phase 21.5.3 (FIX 4) — stratified real-world multi-format smoke"
    echo
    echo "**Question.** A concrete, bounded first step toward comparing against"
    echo "independently sourced real-world documents: ingest the pinned samples in"
    echo "\`tools/realcorpus/formats-manifest.tsv\` (XLSX, PPTX, ODS, ODP, JSON, YAML"
    echo "from stable public sources) and report **pooled costs alongside medians**, so"
    echo "the size/time-weighted picture is visible next to the unweighted one."
    echo
    echo "**Verdict: \`$VERDICT\`**"
    echo
    python3 - "$RAW/smoke.json" <<'PY'
import json, sys
r = json.load(open(sys.argv[1]))
def row(label, a):
    if not a:
        return "| %s | 0 | - | - | - | - | - | - | - |" % label
    def f(x, nd=1):
        return "-" if x is None else ("%.*f" % (nd, x))
    return ("| %s | %d | %s | %s | %s | %s | %s | %s | %s |" % (
        label, a["n"],
        f(a["pooled_build_us"], 0), f(a["median_build_us"], 0),
        a["pooled_store_bytes"], a["median_store_bytes"],
        f(a["pooled_build_us_per_store_byte"], 5),
        f(a["median_build_us_per_store_byte"], 5),
        "yes" if a["all_exact"] else "NO"))
print("| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled us/B | median us/B | exact |")
print("|---|---:|---:|---:|---:|---:|---:|---:|---|")
print(row("ALL", r["overall"]))
for k, a in sorted(r["by_size_class"].items()):
    print(row("size " + k, a))
for k, a in sorted(r["by_format"].items()):
    print(row("format " + k, a))
print()
print("**Pooled vs median.** *pooled* sums the cost over every sample (so big files")
print("dominate); *median* is the unweighted per-document value (so every small")
print("fixture counts equally). `pooled us/B` is total build time over total stored")
print("bytes; `median us/B` is the median of each sample's build-time-per-stored-byte.")
print()
if r["skipped"]:
    print("### Skipped (not fabricated)")
    print()
    print("| id | format | reason |")
    print("|---|---|---|")
    for s in r["skipped"]:
        print("| %s | %s | %s |" % (s["id"], s["format"], s["reason"]))
    print()
else:
    print("All manifest samples were present and ingested.")
    print()
print("### Per-sample")
print()
print("| id | format | size class | src B | store B | build us | exact |")
print("|---|---|---|---:|---:|---:|---|")
for s in r["samples"]:
    print("| %s | %s | %s | %s | %s | %s | %s |" % (
        s["id"], s["format"], s.get("size_class", "-"), s.get("source_bytes", "-"),
        s.get("store_bytes", "-"), s.get("build_us", "-"), s.get("exact")))
PY
    echo
    echo "## Scope (honest)"
    echo
    echo "- **Bounded first step, not the population.** It ingests the pinned samples"
    echo "  in \`tools/realcorpus/formats-manifest.tsv\` (now 52 samples across 14"
    echo "  formats), stratified by format and size class. The full stratified court"
    echo "  (with a malformed/hostile stratum and a workload dimension) is"
    echo "  \`tools/realformats-stratified-court.sh\` (Phase 21.15)."
    echo "- The samples come from stable, permissively licensed public sources (Apache"
    echo "  POI / odfpy / Natural Earth / Prometheus), pinned by tag or commit and by"
    echo "  SHA-256; the bytes are gitignored and re-verified on every acquisition."
    echo "- Every present sample is required to materialize byte-exactly"
    echo "  (\`length + SHA-256 + cmp\`); a missing sample is recorded SKIPPED, never"
    echo "  replaced by a synthetic one."
    echo "- Nothing here is run on the host."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$VERDICT" = "PASS" ] && [ "$RC" -eq 0 ]; then
    echo "REALFORMATS SMOKE COURT: PASS — campaign $CAMPAIGN"
    exit 0
fi
echo "REALFORMATS SMOKE COURT: FAIL — campaign $CAMPAIGN" >&2
exit 1
