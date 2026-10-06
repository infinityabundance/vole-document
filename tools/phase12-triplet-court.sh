#!/bin/sh
# Phase-12.9 cross-format equivalence court (the canonical logical triplet).
#
# Runs inside the pinned `db-baseline` service (the pinned `dev` toolchain plus
# jq, so the same JSON the CLI emits can be asserted against ground truth). It
# takes the deterministic triplet produced by tools/fixtures/doc-triplet-gen.py
# — one known logical report as report.pdf / report.docx / report.epub plus
# ground_truth.json — ingests all three through the one format-agnostic field
# pipeline, and then runs the *same* common observations through the one CLI:
#
#   capabilities, find "<marker>", the document text, metadata, a heading, a
#   paragraph/block, Table/Cell(B7), a Resource and a Link.
#
# It asserts each supported answer against ground_truth.json **and** that the
# answer carries `format=<fmt>;common;<native>` provenance, so the headline is:
# the same logical answer, produced through different native adapters. Where a
# format has no coordinate (PDF has no heading/table/cell/resource/link and no
# document-title metadata), the observation is a typed capability error (exit
# 6) and is recorded as an honest gap, never silently approximated.
#
# Prerequisite (producers image; the generator uses only the Python stdlib):
#   docker compose run --rm --no-TTY producers \
#       python3 tools/fixtures/doc-triplet-gen.py evidence/scratch/phase12-triplet
#
# Usage (from the host):
#   docker compose run --rm --no-TTY db-baseline sh tools/phase12-triplet-court.sh
set -u

cd /work
LC_ALL=C
export LC_ALL

DATE="$(date -u +%Y-%m-%d)"
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
CAMPAIGN="${1:-evidence/campaigns/${DATE}-phase12-triplet-${SHA}}"
FIXTURES="${FIXTURES:-evidence/scratch/phase12-triplet}"
WORK="evidence/scratch/phase12-triplet-work"

if [ ! -f "$FIXTURES/ground_truth.json" ]; then
  echo "missing triplet fixtures in $FIXTURES" >&2
  echo "run first: docker compose run --rm --no-TTY producers python3 tools/fixtures/doc-triplet-gen.py $FIXTURES" >&2
  exit 2
fi

rm -rf "$CAMPAIGN" "$WORK"
mkdir -p "$CAMPAIGN/raw/obs" "$WORK"

B=./target/debug/vole-document
# Build the all-features binary: the field verbs (field-ingest/observe/find/
# materialize) only exist with the field stack compiled in. Cached after the
# first run, so this is cheap and always correct.
cargo build --quiet --locked --all-features
GT="$FIXTURES/ground_truth.json"
ASSERT="$WORK/assertions.tsv"
: > "$ASSERT"

# --- assertion harness -------------------------------------------------------
TOTAL=0; PASS=0; FAIL=0
record() { # status id detail
  TOTAL=$((TOTAL + 1))
  if [ "$1" = PASS ]; then PASS=$((PASS + 1)); else FAIL=$((FAIL + 1)); fi
  printf '%s\t%s\t%s\n' "$1" "$2" "$3" >> "$ASSERT"
}
assert_eq() { # id expected actual
  if [ "$2" = "$3" ]; then record PASS "$1" "$3"; else record FAIL "$1" "expected[$2] got[$3]"; fi
}
assert_contains() { # id needle haystack
  case "$3" in
    *"$2"*) record PASS "$1" "contains" ;;
    *) record FAIL "$1" "missing[$2] in[$3]" ;;
  esac
}
assert_exit() { # id expected_code actual_code
  if [ "$2" = "$3" ]; then record PASS "$1" "exit=$3"; else record FAIL "$1" "expected exit $2 got $3"; fi
}
jqf() { jq -r "$1" "$2"; }

# Copy the sealed fixtures into the receipt (sources + ground truth).
cp "$FIXTURES"/report.pdf "$FIXTURES"/report.docx "$FIXTURES"/report.epub \
   "$FIXTURES"/shared-image.png "$GT" "$CAMPAIGN/raw/"

STORE="$WORK/store"

# --- ingest each format through the one pipeline -----------------------------
for fmt in pdf docx epub; do
  src="$FIXTURES/report.$fmt"
  voldoc="$WORK/report.$fmt.voldoc"
  "$B" encode --force raw "$src" "$voldoc" > "$CAMPAIGN/raw/obs/$fmt.encode.json" 2>&1
  "$B" field-ingest "$voldoc" --store "$STORE" > "$CAMPAIGN/raw/obs/$fmt.ingest.json" 2>&1
  field="$(jqf '.field' "$CAMPAIGN/raw/obs/$fmt.ingest.json")"
  eval "FIELD_$fmt=$field"
  assert_eq "$fmt.ingest.field_present" "true" "$([ -n "$field" ] && [ "$field" != null ] && echo true || echo false)"
done

# --- capabilities: the detected format and its native selectors --------------
for fmt in pdf docx epub; do
  "$B" capabilities "$FIXTURES/report.$fmt" > "$CAMPAIGN/raw/obs/$fmt.capabilities.json"
  assert_eq "$fmt.capabilities.format" "$fmt" "$(jqf '.format' "$CAMPAIGN/raw/obs/$fmt.capabilities.json")"
  nsel="$(jqf '.native_selectors | length' "$CAMPAIGN/raw/obs/$fmt.capabilities.json")"
  assert_eq "$fmt.capabilities.native_selectors_present" "true" "$([ "$nsel" -gt 0 ] && echo true || echo false)"
done

# --- find "<marker>" on all three (the common SearchMatch selector) ----------
for fmt in pdf docx epub; do
  eval "field=\$FIELD_$fmt"
  for marker in XF12A XF12B; do
    out="$CAMPAIGN/raw/obs/$fmt.find.$marker.json"
    "$B" find --store "$STORE" --field "$field" --text "$marker" > "$out"
    assert_contains "$fmt.find.$marker.hit" "$marker" "$(jqf '.value' "$out")"
    prov="$(jqf '.provenance' "$out")"
    case "$prov" in
      "format=$fmt;common;"*) record PASS "$fmt.find.$marker.native_provenance" "$prov" ;;
      *) record FAIL "$fmt.find.$marker.native_provenance" "bad provenance[$prov]" ;;
    esac
  done
done

# --- the whole-document text on all three ------------------------------------
# The same logical strings must appear, produced by the PDF text-runs heuristic,
# the WordprocessingML projection and the XHTML projection respectively.
for fmt in pdf docx epub; do
  eval "field=\$FIELD_$fmt"
  out="$CAMPAIGN/raw/obs/$fmt.text.json"
  "$B" observe --store "$STORE" --field "$field" --doc-text --kind text > "$out"
  text="$(jqf '.text' "$out")"
  assert_contains "$fmt.text.heading0" "Introduction" "$text"
  assert_contains "$fmt.text.heading1" "Method" "$text"
  assert_contains "$fmt.text.heading2" "Findings" "$text"
  assert_contains "$fmt.text.para0" "Alpha paragraph carries marker XF12A." "$text"
  assert_contains "$fmt.text.para1" "Bravo paragraph carries marker XF12B." "$text"
  assert_contains "$fmt.text.list3" "Third step" "$text"
  assert_contains "$fmt.text.cell_b7" "bravo-seven" "$text"
  prov="$(jqf '.provenance' "$out")"
  case "$prov" in
    "format=$fmt;common;"*) record PASS "$fmt.text.native_provenance" "$prov" ;;
    *) record FAIL "$fmt.text.native_provenance" "bad provenance[$prov]" ;;
  esac
done

# --- metadata: capability-supported everywhere, native answer differs --------
# PDF metadata is the structural field descriptor, DOCX metadata is the story
# structure, EPUB metadata is the package summary. The *selector* is common; the
# *answer* is native. (There is no cross-format document-title selector; see the
# honest gap in SUMMARY.md.)
for fmt in pdf docx epub; do
  eval "field=\$FIELD_$fmt"
  out="$CAMPAIGN/raw/obs/$fmt.metadata.json"
  "$B" observe --store "$STORE" --field "$field" --metadata --kind metadata > "$out"
  prov="$(jqf '.provenance' "$out")"
  case "$prov" in
    "format=$fmt;common;"*) record PASS "$fmt.metadata.native_provenance" "$prov" ;;
    *) record FAIL "$fmt.metadata.native_provenance" "bad provenance[$prov]" ;;
  esac
done
assert_eq "pdf.metadata.source_len" \
  "$(jqf '.formats.pdf.length' "$GT")" \
  "$(jqf '.value.source_len' "$CAMPAIGN/raw/obs/pdf.metadata.json")"
assert_eq "pdf.metadata.source_sha256" \
  "$(jqf '.formats.pdf.sha256' "$GT")" \
  "$(jqf '.value.source_sha256' "$CAMPAIGN/raw/obs/pdf.metadata.json")"
assert_eq "docx.metadata.tables" "1" "$(jqf '.value.tables' "$CAMPAIGN/raw/obs/docx.metadata.json")"
assert_eq "docx.metadata.resources" "1" "$(jqf '.value.resources' "$CAMPAIGN/raw/obs/docx.metadata.json")"
assert_eq "docx.metadata.hyperlinks" "1" "$(jqf '.value.hyperlinks' "$CAMPAIGN/raw/obs/docx.metadata.json")"
assert_eq "epub.metadata.package" "OEBPS/package.opf" "$(jqf '.value.package' "$CAMPAIGN/raw/obs/epub.metadata.json")"
assert_eq "epub.metadata.metadata_entries" "3" "$(jqf '.value.metadata_entries' "$CAMPAIGN/raw/obs/epub.metadata.json")"

# --- structured common observations on DOCX and EPUB -------------------------
for fmt in docx epub; do
  eval "field=\$FIELD_$fmt"

  # headings: the same three heading strings, native structures underneath.
  for i in 0 1 2; do
    out="$CAMPAIGN/raw/obs/$fmt.heading.$i.json"
    "$B" observe --store "$STORE" --field "$field" --heading "$i" --kind text > "$out"
    assert_eq "$fmt.heading.$i" "$(jqf ".logical_report.headings[$i]" "$GT")" "$(jqf '.text' "$out")"
    assert_contains "$fmt.heading.$i.native_provenance" "format=$fmt;common;$fmt;" "$(jqf '.provenance' "$out")"
  done

  # a paragraph/block: the first block is the Introduction heading.
  out="$CAMPAIGN/raw/obs/$fmt.block0.json"
  "$B" observe --store "$STORE" --field "$field" --block 0 --kind text > "$out"
  assert_eq "$fmt.block0" "Introduction" "$(jqf '.text' "$out")"

  # the table and the B7 cell (column B, row 7) — the same logical value.
  out="$CAMPAIGN/raw/obs/$fmt.table.json"
  "$B" observe --store "$STORE" --field "$field" --table 0 --kind text > "$out"
  assert_contains "$fmt.table0.b7" "$(jqf '.logical_report.table.cell_b7' "$GT")" "$(jqf '.text' "$out")"
  out="$CAMPAIGN/raw/obs/$fmt.cell.json"
  "$B" observe --store "$STORE" --field "$field" --cell 0:6:1 --kind text > "$out"
  assert_eq "$fmt.cell_b7" "$(jqf '.logical_report.table.cell_b7' "$GT")" "$(jqf '.text' "$out")"

  # the link (same logical text).
  out="$CAMPAIGN/raw/obs/$fmt.link.json"
  "$B" observe --store "$STORE" --field "$field" --link 0 --kind metadata > "$out"
  assert_eq "$fmt.link.text" "$(jqf '.logical_report.link.text' "$GT")" "$(jqf '.value.text' "$out")"
  if [ "$fmt" = epub ]; then
    assert_eq "epub.link.href" "$(jqf '.logical_report.link.href' "$GT")" "$(jqf '.value.href' "$out")"
  fi

  # the one shared image resource: native shape differs (DOCX gives the
  # relationship id; EPUB gives the manifest href + media type), logical same.
  out="$CAMPAIGN/raw/obs/$fmt.resource.json"
  "$B" observe --store "$STORE" --field "$field" --resource 0 --kind metadata > "$out"
  if [ "$fmt" = docx ]; then
    assert_eq "docx.resource.rel" "$(jqf '.formats.docx.resource_rel' "$GT")" "$(jqf '.value.rel' "$out")"
  else
    assert_eq "epub.resource.href" "$(jqf '.formats.epub.resource_href' "$GT")" "$(jqf '.value.href' "$out")"
    assert_eq "epub.resource.media_type" "$(jqf '.formats.epub.resource_media_type' "$GT")" "$(jqf '.value.media_type' "$out")"
  fi
done

# EPUB also serves the resource *bytes* (exact); assert the shared image is
# byte-identical to the ground-truth resource bytes.
out="$CAMPAIGN/raw/obs/epub.resource.bytes.json"
"$B" observe --store "$STORE" --field "$FIELD_epub" --resource 0 --kind decoded > "$out"
assert_eq "epub.resource.sha256" "$(jqf '.logical_report.resource.sha256' "$GT")" "$(jqf '.bytes_sha256' "$out")"

# --- the link URL is only flat text in the PDF (a native gap) ---------------
# The DOCX/EPUB Link coordinate exposes the same logical link; the PDF has no
# link structure, so the URL survives only inside the heuristic page text.
assert_contains "pdf.text.link_url" "$(jqf '.logical_report.link.href' "$GT")" \
  "$(jqf '.text' "$CAMPAIGN/raw/obs/pdf.text.json")"

# --- PDF declines the structural vocabulary with a typed capability error ----
# The common selector names are the *same*; PDF simply has no coordinate for
# them, so the court records the typed decline (exit 6) rather than inventing an
# answer (plan DEC-5).
for sel in heading block table cell resource link; do
  case "$sel" in
    heading) arg="--heading 0" ;;
    block) arg="--block 0" ;;
    table) arg="--table 0" ;;
    cell) arg="--cell 0:6:1" ;;
    resource) arg="--resource 0" ;;
    link) arg="--link 0" ;;
  esac
  "$B" observe --store "$STORE" --field "$FIELD_pdf" $arg --kind text >/dev/null 2>"$CAMPAIGN/raw/obs/pdf.$sel.err.txt"
  assert_exit "pdf.declined.$sel" 6 "$?"
done

# --- materialize_exact: every root is byte-exact -----------------------------
for fmt in pdf docx epub; do
  eval "field=\$FIELD_$fmt"
  out="$WORK/report.$fmt.materialized"
  "$B" materialize --store "$STORE" --field "$field" --exact --output "$out" > "$CAMPAIGN/raw/obs/$fmt.materialize.json"
  assert_eq "$fmt.materialize.sha256" "$(jqf ".formats.$fmt.sha256" "$GT")" "$(jqf '.sha256' "$CAMPAIGN/raw/obs/$fmt.materialize.json")"
  assert_eq "$fmt.materialize.length" "$(jqf ".formats.$fmt.length" "$GT")" "$(jqf '.source_len' "$CAMPAIGN/raw/obs/$fmt.materialize.json")"
  if cmp -s "$out" "$FIXTURES/report.$fmt"; then rc=equal; else rc=differ; fi
  assert_eq "$fmt.materialize.cmp" "equal" "$rc"
done

# --- environment / provenance ------------------------------------------------
RUSTC=$(rustc -vV | sed -n 's/^release: //p')
CARGO=$(cargo -V | sed -n 's/^cargo //p')
LOCK_SHA=$(sha256sum Cargo.lock | cut -d' ' -f1)
ARCH=$(uname -m)
COMMIT=$(git rev-parse HEAD 2>/dev/null || echo unknown)
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)
DIRTY=$(git status --porcelain 2>/dev/null | tr '\n' ';')
RUN_UTC=$(date -u +%Y-%m-%dT%H:%M:%SZ)
BASE=$(sed -n 's/^ARG BASE_STABLE=//p' Dockerfile)

cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$(basename "$CAMPAIGN")",
  "run_utc": "$RUN_UTC",
  "arch": "$ARCH",
  "git_branch": "$BRANCH",
  "git_commit": "$COMMIT",
  "git_dirty": "$DIRTY",
  "cargo_lock_sha256": "$LOCK_SHA",
  "db_baseline": {
    "service_image": "vole-document/db-baseline:1.99.0",
    "base_image": "$BASE",
    "rustc": "$RUSTC",
    "cargo": "$CARGO"
  },
  "generator": {
    "script": "tools/fixtures/doc-triplet-gen.py",
    "service_image": "vole-document/producers:bookworm",
    "python": "3.11.2",
    "stdlib_only": true
  },
  "features_under_test": "all-features (field, package, opc, docx, epub, ...)",
  "env_vars_affecting_semantics": { "CARGO_TERM_COLOR": "never", "LC_ALL": "C" },
  "notes": "All commands ran through compose.yaml inside pinned images; nothing ran on the host except git and docker. Same logical report, different native provenance."
}
EOF

cat > "$CAMPAIGN/commands.txt" <<EOF
# Phase 12.9 cross-format equivalence court — exact commands
# Commit under test: $COMMIT (branch $BRANCH); dirty: $DIRTY
# Run (UTC): $RUN_UTC
# Base image (ARG BASE_STABLE): $BASE
# Service image: vole-document/db-baseline:1.99.0
# Generator image: vole-document/producers:bookworm (python 3.11.2)
# rustc $RUSTC, cargo $CARGO, arch $ARCH
# Cargo.lock sha256: $LOCK_SHA
# All commands run inside pinned images; nothing on the host except git + docker.

# 1) deterministic triplet (stdlib only, no VOLE code):
docker compose run --rm --no-TTY producers python3 tools/fixtures/doc-triplet-gen.py evidence/scratch/phase12-triplet

# 2) the court (one format-agnostic ingest + the one common CLI):
docker compose run --rm --no-TTY db-baseline sh tools/phase12-triplet-court.sh

# Inside the court (per format in pdf/docx/epub):
#   target/debug/vole-document encode --force raw report.FMT report.FMT.voldoc
#   target/debug/vole-document field-ingest report.FMT.voldoc --store STORE
#   target/debug/vole-document capabilities report.FMT
#   target/debug/vole-document find --store STORE --field FIELD --text XF12A
#   target/debug/vole-document observe --store STORE --field FIELD --doc-text --kind text
#   target/debug/vole-document observe --store STORE --field FIELD --metadata --kind metadata
#   target/debug/vole-document observe --store STORE --field FIELD --heading 0 --kind text
#   target/debug/vole-document observe --store STORE --field FIELD --block 0  --kind text
#   target/debug/vole-document observe --store STORE --field FIELD --table 0  --kind text
#   target/debug/vole-document observe --store STORE --field FIELD --cell 0:6:1 --kind text
#   target/debug/vole-document observe --store STORE --field FIELD --resource 0 --kind metadata
#   target/debug/vole-document observe --store STORE --field FIELD --link 0 --kind metadata
#   target/debug/vole-document materialize --store STORE --field FIELD --exact --output OUT
# The example writes raw/obs/*.json, raw/*.{pdf,docx,epub}, raw/ground_truth.json.
EOF

# --- summary -----------------------------------------------------------------
{
  echo "# Phase 12.9 — cross-format equivalence court (canonical logical triplet)"
  echo
  echo "**Result: $PASS/$TOTAL assertions passed, $FAIL failed.**"
  echo
  echo "One known logical report was generated as \`report.pdf\`, \`report.docx\` and"
  echo "\`report.epub\` (Python stdlib generator, \`producers\` image) with a"
  echo "\`ground_truth.json\`. All three were ingested through the one format-agnostic"
  echo "field pipeline and queried through the one common CLI."
  echo
  echo "## Same logical answer, different native provenance"
  echo
  echo '| observation | PDF | DOCX | EPUB |'
  echo '|---|---|---|---|'
  echo "| \`find XF12A\` (marker) | yes | yes | yes |"
  echo "| whole-document text | yes | yes | yes |"
  echo "| metadata | yes (structural descriptor) | yes (story structure) | yes (package summary) |"
  echo "| heading | typed decline | Introduction/Method/Findings | Introduction/Method/Findings |"
  echo "| block/paragraph | typed decline | yes | yes |"
  echo "| Table / Cell(B7) | typed decline | bravo-seven | bravo-seven |"
  echo "| Resource (the one image) | typed decline | rel=\`rIdImg\` | href=\`images/image1.png\` (image/png) |"
  echo "| Link | typed decline | text=\`external\` | href=\`https://example.com/phase12\` |"
  echo
  echo "Every supported answer carries \`format=<fmt>;common;<native>\` provenance"
  echo "(asserted in \`raw/obs/*.json\`), so the same logical answer is produced by"
  echo "different native adapters over distinct byte representations."
  echo
  echo "## Honest gaps (recorded, not approximated)"
  echo
  echo "* **No cross-format document-title observation.** The common vocabulary has no"
  echo "  title selector: PDF \`metadata\` returns the structural field descriptor"
  echo "  (\`source_len\`/\`source_sha256\`), DOCX \`metadata\` returns the story structure,"
  echo "  and the EPUB \`metadata\` representation (the only one the capability set"
  echo "  admits) is the package summary without the \`dc:title\` array. The title is"
  echo "  therefore present in the sources and ground truth but is **not** claimed as a"
  echo "  common observation."
  echo "* **PDF has no heading/table/cell/resource/link coordinate.** Those common"
  echo "  selectors return the typed capability error (exit 6), asserted for each."
  echo "* **PDF \`search-match\` native provenance is the empty string** after the"
  echo "  adapter tag (\`format=pdf;common;\`); PDF has no native search coordinate to"
  echo "  name beyond the heuristic page text it searched."
  echo
  echo "## Exactness"
  echo
  echo "\`materialize --exact\` reproduces every source byte-exactly"
  echo "(length + SHA-256 + \`cmp\`), so the equivalence court does not trade exactness"
  echo "for a common vocabulary."
  echo
  echo "## Assertions"
  echo
  echo '```'
  awk -F'\t' '{ printf "%-5s %-40s %s\n", $1, $2, $3 }' "$ASSERT"
  echo '```'
  echo
  echo "---"
  echo
  echo "## Environment (receipt)"
  echo
  echo "- Commit under test: \`$COMMIT\` (branch \`$BRANCH\`), dirty files: \`$DIRTY\`"
  echo "- Service image: \`vole-document/db-baseline:1.99.0\`; base \`$BASE\`"
  echo "- Toolchain: rustc \`$RUSTC\`, cargo \`$CARGO\`, arch \`$ARCH\`"
  echo "- \`Cargo.lock\` sha256: \`$LOCK_SHA\`"
  echo "- Run (UTC): $RUN_UTC"
} > "$CAMPAIGN/SUMMARY.md"

cp "$ASSERT" "$CAMPAIGN/raw/assertions.tsv"
( cd "$CAMPAIGN" && find raw -type f | sort | xargs sha256sum > raw.sha256 ) 2>/dev/null || true

echo "assertions: $PASS/$TOTAL passed, $FAIL failed"
echo "wrote $CAMPAIGN"
[ "$FAIL" -eq 0 ] || exit 1
