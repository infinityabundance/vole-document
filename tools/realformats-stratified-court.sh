#!/usr/bin/env bash
# Phase 21.15 (ITEM 2) — stratified real-world multi-format court.
#
# Ingests every pinned real sample in tools/realcorpus/formats-manifest.tsv (14
# Wave-2 formats) plus a malformed/hostile stratum derived from pinned real bytes
# (tools/realcorpus/hostile-strata.tsv), and reports **pooled costs ALONGSIDE
# medians**, stratified by format, size class, complexity, and the malformed flag,
# separated by **workload** (narrow observation vs full materialize).
#
#   docker compose run --rm --no-TTY realcorpus sh tools/realcorpus/fetch-formats.sh
#   docker compose run --rm --no-TTY analytical bash tools/realformats-stratified-court.sh
#
# If samples are absent it runs what it can and records SKIPPED/BLOCKED rows (it
# does NOT fabricate). FAILs only if a *present* sample is not byte-exact.

set -uo pipefail
cd /work
export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN="evidence/campaigns/${STAMP}-realformats-stratified-${SHA}"
RAW="$CAMPAIGN/raw"
MANIFEST=tools/realcorpus/formats-manifest.tsv
HOSTILE=tools/realcorpus/hostile-strata.tsv
CORPUS=realformats-v1

PROFILE=${PROFILE:-release}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-${VOLE_BIN:-target/debug/vole-document}} ;;
esac

rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

echo "=== build ($PROFILE) ==="
# shellcheck disable=SC2086
if ! cargo build $BUILD_ARGS 2>&1 | tail -2; then
    echo "realformats-stratified: cargo build FAILED ($BUILD_ARGS); refusing to measure" >&2
    exit 1
fi
[ -x "$BIN" ] || { echo "realformats-stratified: $BIN missing; refusing to run" >&2; exit 1; }

echo "=== run the stratified court ==="
set +e
python3 tools/fixtures/realformats-stratified.py --bin "$BIN" --corpus "$CORPUS" \
    --manifest "$MANIFEST" --hostile "$HOSTILE" --out "$RAW"
RC=$?
set -e
VERDICT=$(python3 -c "import json;print(json.load(open('$RAW/stratified.json'))['verdict'])")

cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.15 — stratified real-world multi-format court",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "measured_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "tree_state": "$(git status --porcelain | tr '\n' ' ' | sed 's/"/\\"/g')",
  "service": "analytical",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "python": "$(python3 --version)",
  "arch": "$(uname -m)",
  "profile": "$PROFILE",
  "build_args": "$BUILD_ARGS",
  "bin": "$BIN",
  "bin_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "manifest_sha256": "$(sha256sum "$MANIFEST" | cut -d' ' -f1)",
  "hostile_strata_sha256": "$(sha256sum "$HOSTILE" | cut -d' ' -f1)",
  "driver_sha256": "$(sha256sum tools/fixtures/realformats-stratified.py | cut -d' ' -f1)",
  "storage_accounting": "sum of regular-file sizes (getsize walk); du -sb never used (ADR-0049)",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF
cp "$RAW/stratified.json" "$CAMPAIGN/receipt.json"

cat > "$CAMPAIGN/commands.txt" <<'EOF'
# acquisition (network-capable, capped realcorpus lane):
docker compose run --rm --no-TTY realcorpus sh tools/realcorpus/fetch-formats.sh
# stratified court (pinned analytical lane; RELEASE VOLE + python3):
docker compose run --rm --no-TTY analytical bash tools/realformats-stratified-court.sh
# inside the court:
#   cargo build --release --locked --all-features
#   python3 tools/fixtures/realformats-stratified.py --bin <BIN> --corpus realformats-v1 \
#       --manifest tools/realcorpus/formats-manifest.tsv \
#       --hostile tools/realcorpus/hostile-strata.tsv --out <RAW>
# Per sample: field-build --profile runtime --packed on a scratch COPY of the bytes;
# the copy is DELETED; then a fresh `materialize --exact --packed` process must
# reproduce length + SHA-256 + cmp of the in-memory original.
EOF

{
    echo "# Phase 21.15 (ITEM 2) — stratified real-world multi-format court"
    echo
    echo "**Question.** Beyond a 12-document smoke: ingest a **stratified** population"
    echo "of independently sourced real-world documents across the Wave-2 formats"
    echo "(XLSX, PPTX, ODS, ODP, JSON, YAML, CSV, Markdown, XML, HTML, TOML, JSONL, EML,"
    echo "Parquet), plus a malformed/hostile stratum, and report **pooled costs"
    echo "alongside medians** per format and per stratum, separated by workload (narrow"
    echo "observation vs full materialize)."
    echo
    echo "**Verdict: \`$VERDICT\`**"
    echo
    python3 - "$RAW/stratified.json" <<'PY'
import json, sys
r = json.load(open(sys.argv[1]))

def f(x, nd=1):
    return "-" if x is None else ("%.*f" % (nd, x))

def row(label, a):
    if not a:
        return "| %s | 0 | - | - | - | - | - | - | - | - | - | - |" % label
    return ("| %s | %d | %s | %s | %s | %s | %s | %s | %s | %s | %s |" % (
        label, a["n"],
        f(a["pooled_build_us"], 0), f(a["median_build_us"], 0),
        a["pooled_store_bytes"], a["median_store_bytes"],
        f(a["pooled_narrow_us"], 0), f(a["median_narrow_us"], 0),
        f(a["pooled_full_us"], 0), f(a["median_full_us"], 0),
        "yes" if a["all_exact"] else "NO"))

def table(title, key):
    print("## %s" % title)
    print()
    print("| stratum | n | pooled build us | median build us | pooled store B | median store B | pooled narrow us | median narrow us | pooled full us | median full us | exact |")
    print("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|")
    for k, a in sorted(r[key].items()):
        print(row(k, a))
    print()

print("## Counts")
print()
c = r["counts"]
print("| ran real | ran hostile | skipped | blocked | exact | samples |")
print("|---:|---:|---:|---:|---:|---:|")
print("| %d | %d | %d | %d | %d | %d |" % (
    c["ran_real"], c["ran_hostile"], c["skipped"], c["blocked"], c["exact"], c["samples"]))
print()
print("By format (ran): " + ", ".join("%s=%d" % (k, v["n"])
      for k, v in sorted(r["by_format"].items())))
print()
print("## Stratified tables")
print()
table("By format", "by_format")
table("By size class", "by_size_class")
table("By complexity", "by_complexity")
table("By stratum (real vs malformed/hostile)", "by_stratum")
table("By format x size class", "by_format_size")
print("### Workload dimension")
print()
print("`narrow` = one `observe %s` per document (a bounded metadata projection)."
      % r["narrow_selector"])
print("`full` = one `materialize %s` per document (byte-authority closure)."
      % r["full_selector"])
print("Both are pooled and median; a size/time-weighted world is `pooled`, an"
      " unweighted per-document world is `median`.")
print()
decl = [s for s in r["samples"] if s.get("format_detected") == "opaque"]
noobs = [s for s in r["samples"] if "build_us" in s and not s.get("obs_ok")]
print("**Detection honesty.** %d/%d samples were detected as `opaque` by the bounded"
      " detector (so the metadata observation declines fast, which lowers their"
      " `narrow` cost); they are still byte-exactly closed by the opaque floor. %d"
      " samples declined the narrow observation. The byte-authority gate is"
      " unaffected by detection." % (len(decl), r["counts"]["samples"], len(noobs)))
print()
print("**Pooled vs median.** *pooled* sums the cost over every sample (big files"
      " dominate); *median* is the unweighted per-document value. `pooled us/B` is"
      " total build time over total stored bytes; `median us/B` is the median of each"
      " sample's build-time-per-stored-byte.")
print()
if r["skipped"]:
    print("### Skipped (present-run only; not fabricated)")
    print()
    print("| id | format | stratum | reason |")
    print("|---|---|---|---|")
    for s in r["skipped"]:
        print("| %s | %s | %s | %s |" % (s["id"], s["format"], s.get("stratum", "-"), s["reason"]))
    print()
else:
    print("No real sample was skipped.")
    print()
if r["blocked"]:
    print("### Blocked (unrun; exact blocker recorded)")
    print()
    print("| id | stratum | reason |")
    print("|---|---|---|")
    for s in r["blocked"]:
        print("| %s | %s | %s |" % (s["id"], s.get("stratum", "-"), s["reason"]))
    print()
print("### Per-sample")
print()
print("| id | format | detected | stratum | size class | complexity | src B | store B | build us | narrow us | full us | exact |")
print("|---|---|---|---|---|---|---:|---:|---:|---:|---:|---|")
for s in r["samples"]:
    if "build_us" not in s:
        print("| %s | %s | - | %s | %s | %s | %s | - | - | - | - | NO (%s) |" % (
            s["id"], s["format"], s.get("stratum", "-"), s.get("size_class", "-"),
            s.get("complexity", "-"), s.get("source_bytes", "-"), s.get("error", "error")))
        continue
    print("| %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s |" % (
        s["id"], s["format"], s.get("format_detected", "-"), s.get("stratum", "-"),
        s["size_class"], s["complexity"], s["source_bytes"], s["store_bytes"],
        s["build_us"], s["narrow_us"], s["full_us"], s["exact"]))
PY
    echo
    echo "## Scope (honest)"
    echo
    echo "- **A stratified real-world population, still not a random sample of the"
    echo "  world.** Samples are pinned by tag/commit and SHA-256 from stable,"
    echo "  permissively licensed public sources (Apache POI, odfpy, Natural Earth,"
    echo "  Prometheus/Alertmanager, CommonMark, Maven, MDN, serde/cargo,"
    echo "  parquet-testing, openai-cookbook, CPython, jsonlines); the bytes are"
    echo "  gitignored and re-verified on every acquisition. It is stratified, not an"
    echo "  unbiased distribution."
    echo "- **Complexity is a curated label; size class is measured.** Size class comes"
    echo "  from the byte length; the complexity stratum is a curation label stated in"
    echo "  the manifest."
    echo "- **A malformed/hostile stratum is included.** It is derived deterministically"
    echo "  from pinned real bytes (truncation, wrong-magic, mid-structure corruption),"
    echo "  pinned by SHA-256 in \`tools/realcorpus/hostile-strata.tsv\`."
    echo "- **Only byte-exact closure is a byte-authority claim.** Each sample's bytes"
    echo "  are built from a scratch copy that is then deleted, and a fresh process must"
    echo "  reproduce \`length + SHA-256 + cmp\`."
    echo "- **Missing samples are SKIPPED/BLOCKED, never fabricated.** Nothing here is"
    echo "  run on the host; the acquisition ran in the capped \`realcorpus\` lane and the"
    echo "  court in the capped \`analytical\` lane."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$VERDICT" = "PASS" ] && [ "$RC" -eq 0 ]; then
    echo "REALFORMATS STRATIFIED COURT: PASS — campaign $CAMPAIGN"
    exit 0
fi
echo "REALFORMATS STRATIFIED COURT: FAIL — campaign $CAMPAIGN" >&2
exit 1
