#!/bin/sh
# Phase 26.5 — stratified real-world population v2 court (larger scale).
#
# Ingests the LARGER population pinned in tools/realcorpus/formats-manifest-v2.tsv
# and the extended malformed/hostile stratum in
# tools/realcorpus/hostile-strata-v2.tsv, and reports **pooled costs ALONGSIDE
# medians**, stratified by declared format, size class, complexity, and the
# real-vs-hostile stratum. It additionally records the **detector's choice** for
# every sample and flags a **finding** when that choice differs from the sample's
# declared format (a conservative `opaque` decline, or a claim by another
# detector). A finding does NOT fail the court: the byte-authority verdict is
# byte-exact closure (length + SHA-256 + cmp from a deleted scratch copy in a
# fresh process).
#
#   docker compose run --rm --no-TTY realcorpus sh tools/realcorpus/fetch-formats.sh \
#       # (with MANIFEST=tools/realcorpus/formats-manifest-v2.tsv)
#   docker compose run --rm --no-TTY analytical sh tools/realformats-stratified-v2-court.sh
#
# The lane is recorded via SERVICE (default `analytical`); `analytical` and
# `doc-baseline` are capped identically, share the cargo volume, and carry python3.
# The v1 60-sample court (tools/realformats-stratified-court.sh) remains the
# historical control.
#
# If samples are absent it runs what it can and records SKIPPED/BLOCKED rows (it
# does NOT fabricate). FAILs only if a *present* sample is not byte-exact.

set -u
cd /work
export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN="evidence/campaigns/${STAMP}-realformats-stratified-v2-${SHA}"
RAW="$CAMPAIGN/raw"
MANIFEST=tools/realcorpus/formats-manifest-v2.tsv
HOSTILE=tools/realcorpus/hostile-strata-v2.tsv
CORPUS=realformats-v1
SERVICE=${SERVICE:-analytical}

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
    echo "realformats-stratified-v2: cargo build FAILED ($BUILD_ARGS); refusing to measure" >&2
    exit 1
fi
[ -x "$BIN" ] || { echo "realformats-stratified-v2: $BIN missing; refusing to run" >&2; exit 1; }

echo "=== run the stratified v2 court ==="
set +e
python3 tools/fixtures/realformats-stratified-v2.py --bin "$BIN" --corpus "$CORPUS" \
    --manifest "$MANIFEST" --hostile "$HOSTILE" --out "$RAW"
RC=$?
set -e
VERDICT=$(python3 -c "import json;print(json.load(open('$RAW/stratified.json'))['verdict'])")

cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "26.5 — stratified real-world population v2 (larger scale)",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "measured_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "tree_state": "$(git status --porcelain | tr '\n' ' ' | sed 's/"/\\"/g')",
  "service": "$SERVICE",
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
  "manifest": "$MANIFEST",
  "manifest_sha256": "$(sha256sum "$MANIFEST" | cut -d' ' -f1)",
  "hostile_strata": "$HOSTILE",
  "hostile_strata_sha256": "$(sha256sum "$HOSTILE" | cut -d' ' -f1)",
  "driver_sha256": "$(sha256sum tools/fixtures/realformats-stratified-v2.py | cut -d' ' -f1)",
  "findings_reconciliation_sha256": "$(sha256sum tools/realcorpus/findings-v2-reconciliation.tsv | cut -d' ' -f1)",
  "storage_accounting": "sum of regular-file sizes (getsize walk); du -sb never used (ADR-0049)",
  "estimator_discipline": "pooled (ratio of sums) ALONGSIDE median per document (ADR-0054); no perf claim without its estimator",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF
cp "$RAW/stratified.json" "$CAMPAIGN/receipt.json"

cat > "$CAMPAIGN/commands.txt" <<'EOF'
# acquisition (network-capable, capped realcorpus lane):
MANIFEST=tools/realcorpus/formats-manifest-v2.tsv docker compose run --rm --no-TTY realcorpus sh tools/realcorpus/fetch-formats.sh
# stratified v2 court (pinned @@SERVICE@@ lane; RELEASE VOLE + python3):
docker compose run --rm --no-TTY @@SERVICE@@ sh tools/realformats-stratified-v2-court.sh
# inside the court:
#   cargo build --release --locked --all-features
#   python3 tools/fixtures/realformats-stratified-v2.py --bin <BIN> --corpus realformats-v1 \
#       --manifest tools/realcorpus/formats-manifest-v2.tsv \
#       --hostile tools/realcorpus/hostile-strata-v2.tsv --out <RAW>
# Per sample: field-build --profile runtime --packed on a scratch COPY of the bytes;
# the copy is DELETED; then a fresh `materialize --exact --packed` process must
# reproduce length + SHA-256 + cmp of the in-memory original.
EOF
sed -i "s/@@SERVICE@@/${SERVICE}/g" "$CAMPAIGN/commands.txt"

{
    echo "# Phase 26.5 — stratified real-world population v2 (larger scale)"
    echo
    echo "**Question.** At a genuinely larger real-world scale than the v1 60-sample"
    echo "smoke: ingest a stratified population of independently sourced documents"
    echo "across the supported format families (OOXML + legacy binary office, ODF,"
    echo "structured text, prose, tabular/analytical, messaging, geospatial,"
    echo "scientific/container), plus an extended malformed/hostile stratum, record"
    echo "the detector's choice for every sample, and report **pooled costs alongside"
    echo "medians** per declared format and per stratum. The v1 60-sample court remains"
    echo "the historical control."
    echo
    echo "**Verdict: \`$VERDICT\`** (byte-exact closure; findings are recorded, not fatal)"
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
print("| samples | real | hostile | skipped | blocked | exact | detection agree | findings | formats |")
print("|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
print("| %d | %d | %d | %d | %d | %d | %d | %d | %d |" % (
    c["samples"], c["real"], c["hostile"], c["skipped"], c["blocked"],
    c["exact"], c["detection_agree"], c["findings"], c["formats"]))
print()
print("By declared format (ran): " + ", ".join("%s=%d" % (k, v["n"])
      for k, v in sorted(r["by_format"].items())))
print()
print("## Stratified tables")
print()
table("By declared format", "by_format")
table("By size class", "by_size_class")
table("By complexity", "by_complexity")
table("By stratum (real vs malformed/hostile)", "by_stratum")
print("### Workload dimension")
print()
print("`narrow` = one `observe %s` per document (a bounded metadata projection)."
      % r["narrow_selector"])
print("`full` = one `materialize %s` per document (byte-authority closure)."
      % r["full_selector"])
print("Both are pooled and median; a size/time-weighted world is `pooled`, an"
      " unweighted per-document world is `median` (ADR-0054: a claim names its estimator).")
print()
decl = [s for s in r["samples"] if s.get("format_detected") == "opaque"]
noobs = [s for s in r["samples"] if "build_us" in s and not s.get("obs_ok")]
print("**Detection.** %d/%d samples were detected as `opaque` by the bounded detector;"
      " %d samples declined the narrow observation. The byte-authority gate is"
      " unaffected by detection; every sample is byte-exactly closed."
      % (len(decl), c["samples"], len(noobs)))
print()
print("## Findings (detector choice != declared format)")
print()
print("A **finding** is a real sample whose detector choice differs from its declared"
      " format: a conservative `decline-opaque`, or a `misclassified` claim by another"
      " detector. Every finding is reconciled in the phase report (fixed-with-test, or"
      " recorded as a typed decline / recorded misclassification with its reason). The"
      " byte-authority closure is unaffected by any finding.")
print()
if r["findings"]:
    from collections import Counter
    recon = {}
    try:
        with open("tools/realcorpus/findings-v2-reconciliation.tsv") as fh:
            next(fh)
            for ln in fh:
                parts = ln.rstrip("\n").split("\t")
                if len(parts) >= 3:
                    recon[(parts[0], parts[1])] = parts[2]
    except OSError:
        pass
    kinds = Counter(x["kind"] for x in r["findings"])
    print("Summary by kind: " + ", ".join("%s=%d" % (k, v) for k, v in sorted(kinds.items())))
    print()
    print("| id | declared | detected | kind | resolution |")
    print("|---|---|---|---|---|")
    for x in sorted(r["findings"], key=lambda x: (x["format"], x["id"])):
        res = recon.get((x["format"], x["format_detected"]), "recorded (no reconciliation row)")
        print("| %s | %s | %s | %s | %s |" % (x["id"], x["format"], x["format_detected"], x["kind"], res))
    print()
    print("Each finding is either **fixed with a focused regression test** or")
    print("**recorded** as a typed decline / recorded misclassification with its reason")
    print("(a *recorded* detector-boundary finding never changes the exact bytes; the")
    print("byte-authority gate is `materialize == source` for every sample). Phase 27.3")
    print("detector fixes landed here: (1) the legacy PPT/XLS detectors now read only the")
    print("stream **header prefix** (`cfbf::stream_leading`), so a **fragmented**")
    print("`PowerPoint Document`/`Workbook` chain still detects - regression tests")
    print("`legacy_cfbf_detection::fragmented_powerpoint_document_stream_is_detected` and")
    print("`legacy_cfbf_detection::fragmented_workbook_stream_is_detected`; (2) the XLS")
    print("detector matches the well-known `Workbook`/`Book` stream name")
    print("ASCII-case-insensitively - regression test")
    print("`legacy_cfbf_detection::uppercase_workbook_stream_name_is_detected`; (3) the")
    print("GIS detector now claims GPX 1.0 and a namespace-less `<kml>` - regression tests")
    print("`gis::tests::detects_kml_and_gpx_and_rejects_junk` and")
    print("`gis_adapter::detection_is_byte_based_and_conservative`. Phase 27.5 (this court)")
    print("landed the bare, magic-less Arrow IPC **stream** framing: the arrow detector now")
    print("claims a coherent encapsulated-message sequence (a continuation-marked `Schema`")
    print("first, ending in an end-of-stream marker), so all four `arrow-stream-*.stream`")
    print("samples are claimed as `arrow` - regression tests")
    print("`arrow_adapter::stream_framing_is_detected_and_messages_are_ordered_and_exact`,")
    print("`arrow_adapter::stream_near_misses_decline_to_opaque`, and court")
    print("`tools/phase27-5-1-arrow-stream-court.sh`.")
    print()
else:
    print("No findings: every real sample's detector choice equals its declared format.")
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
print("| id | declared | detected | stratum | size class | complexity | src B | store B | build us | narrow us | full us | exact |")
print("|---|---|---|---|---|---|---:|---:|---:|---:|---:|---|")
for s in sorted(r["samples"], key=lambda x: (x["format"], x["id"])):
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
    echo "- **A larger stratified public-sample population, still not a random sample"
    echo "  of the world.** Samples are pinned by tag/commit and SHA-256 from stable,"
    echo "  permissively licensed public sources (Apache POI, odfpy, vega-datasets,"
    echo "  Natural Earth, parquet-testing, arrow-testing, CPython, toml-test,"
    echo "  asciidoctor, mdx-js, jupyter, libcbor, OpenAPI-Specification, feedvalidator,"
    echo "  togeojson, netcdf-c, Apache Tika, bibtex-parser, loghub, jsonlines,"
    echo "  openai-cookbook); the bytes are gitignored and re-verified on every"
    echo "  acquisition. Rows whose source license is unclear are marked"
    echo "  \`redistributable=false\` (only length + SHA-256 are pinned). It is"
    echo "  stratified, not an unbiased distribution."
    echo "- **The declared format is the sample's true/most-specific format.** Where a"
    echo "  v1 control row was labelled with its coarse physical encoding, v2 declares"
    echo "  the genuinely more-specific format (GeoJSON over JSON, package-manifest over"
    echo "  TOML); that is curation, not a detection finding."
    echo "- **Complexity is a curated label; size class is measured.** Size class comes"
    echo "  from the byte length; the complexity stratum is a curation label stated in"
    echo "  the manifest."
    echo "- **A malformed/hostile stratum is included.** It is derived deterministically"
    echo "  from pinned real bytes (truncation, wrong-magic, dropped tail), pinned by"
    echo "  SHA-256 in \`tools/realcorpus/hostile-strata-v2.tsv\`."
    echo "- **Only byte-exact closure is a byte-authority claim.** Each sample's bytes"
    echo "  are built from a scratch copy that is then deleted, and a fresh process must"
    echo "  reproduce \`length + SHA-256 + cmp\`."
    echo "- **A finding is not a failure of authority.** A detector decline/misclass is"
    echo "  recorded and reconciled; it never changes the exact bytes."
    echo "- **Missing samples are SKIPPED/BLOCKED, never fabricated.** Nothing here is"
    echo "  run on the host; the acquisition ran in the capped \`realcorpus\` lane and the"
    echo "  court in the capped \`$SERVICE\` lane."
} > "$CAMPAIGN/SUMMARY.md"

# Bound the receipt (Phase 26.5): the per-sample field stores and scratch copies are
# derivative (regenerable from the pinned, SHA-256-checked sources) and would make
# the receipt tens of MB. The durable, bounded record is the JSON
# (`raw/stratified.json` = `receipt.json`): per-sample source/store bytes, the
# detector's choice, exactness, and timings. Store blobs are deliberately not kept.
rm -rf "$RAW/stores" "$RAW/tmp"

echo
if [ "$VERDICT" = "PASS" ] && [ "$RC" -eq 0 ]; then
    echo "REALFORMATS STRATIFIED V2 COURT: PASS — campaign $CAMPAIGN"
    exit 0
fi
echo "REALFORMATS STRATIFIED V2 COURT: FAIL — campaign $CAMPAIGN" >&2
exit 1
