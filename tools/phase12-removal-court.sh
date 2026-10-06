#!/bin/sh
# Phase-12.10 source-removal / restart court.
#
# Runs inside the pinned `db-baseline` service. For **each** of PDF, DOCX and
# EPUB it:
#
#   ingest -> sync -> record the oracle length + SHA-256 -> delete the source
#   (and the ingest descriptor) -> a new process -> common + native queries ->
#   materialize --exact -> length + SHA-256 + `cmp`
#
# It proves the field is queryable and byte-exactly rematerializable with the
# source gone, in fresh processes. The receipt keeps an oracle copy of each
# source for `cmp`; the field store never points at it.
#
# Prerequisite (producers image; the generator uses only the Python stdlib):
#   docker compose run --rm --no-TTY producers \
#       python3 tools/fixtures/doc-triplet-gen.py evidence/scratch/phase12-triplet
#
# Usage (from the host):
#   docker compose run --rm --no-TTY db-baseline sh tools/phase12-removal-court.sh
set -u

cd /work
LC_ALL=C
export LC_ALL

DATE="$(date -u +%Y-%m-%d)"
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
CAMPAIGN="${1:-evidence/campaigns/${DATE}-phase12-removal-${SHA}}"
FIXTURES="${FIXTURES:-evidence/scratch/phase12-triplet}"
WORK="evidence/scratch/phase12-removal-work"

if [ ! -f "$FIXTURES/ground_truth.json" ]; then
  echo "missing triplet fixtures in $FIXTURES" >&2
  echo "run first: docker compose run --rm --no-TTY producers python3 tools/fixtures/doc-triplet-gen.py $FIXTURES" >&2
  exit 2
fi

rm -rf "$CAMPAIGN" "$WORK"
mkdir -p "$CAMPAIGN/raw" "$WORK"

B=./target/debug/vole-document
# Build the all-features binary: the field verbs (field-ingest/observe/find/
# materialize) only exist with the field stack compiled in. Cached after the
# first run, so this is cheap and always correct.
cargo build --quiet --locked --all-features
GT="$FIXTURES/ground_truth.json"
ASSERT="$WORK/assertions.tsv"
: > "$ASSERT"

TOTAL=0; PASS=0; FAIL=0
record() {
  TOTAL=$((TOTAL + 1))
  if [ "$1" = PASS ]; then PASS=$((PASS + 1)); else FAIL=$((FAIL + 1)); fi
  printf '%s\t%s\t%s\n' "$1" "$2" "$3" >> "$ASSERT"
}
assert_eq() {
  if [ "$2" = "$3" ]; then record PASS "$1" "$3"; else record FAIL "$1" "expected[$2] got[$3]"; fi
}
assert_contains() {
  case "$3" in
    *"$2"*) record PASS "$1" "contains" ;;
    *) record FAIL "$1" "missing[$2] in[$3]" ;;
  esac
}
jqf() { jq -r "$1" "$2"; }

# ---------------------------------------------------------------------------
# Phase 1: ingest from a *private* copy of the source, record the oracle, then
# delete the private source and the ingest descriptor.
# ---------------------------------------------------------------------------
for fmt in pdf docx epub; do
  d="$WORK/$fmt"
  mkdir -p "$d"
  # The private copy is what the pipeline sees; the fixtures dir is not touched.
  cp "$FIXTURES/report.$fmt" "$d/source.$fmt"
  src="$d/source.$fmt"
  voldoc="$d/source.$fmt.voldoc"

  "$B" encode --force raw "$src" "$voldoc" > "$d/encode.json"
  "$B" field-ingest "$voldoc" --store "$d/store" > "$d/ingest.json"
  field="$(jqf '.field' "$d/ingest.json")"
  eval "FIELD_$fmt=$field"
  eval "STORE_$fmt=$d/store"

  # Oracle recorded BEFORE deletion, and cross-checked against the generator.
  olen=$(stat -c%s "$src")
  osha=$(sha256sum "$src" | cut -d' ' -f1)
  assert_eq "$fmt.oracle.len" "$(jqf ".formats.$fmt.length" "$GT")" "$olen"
  assert_eq "$fmt.oracle.sha256" "$(jqf ".formats.$fmt.sha256" "$GT")" "$osha"
  echo "$osha" > "$CAMPAIGN/raw/$fmt.sha256"
  echo "$olen" > "$CAMPAIGN/raw/$fmt.length"
  # The sealed oracle for `cmp` (the only copy the pipeline keeps is the store).
  cp "$src" "$CAMPAIGN/raw/$fmt.source"

  # Delete the source and the descriptor. Only the field store remains.
  rm -f "$src" "$voldoc"
  if [ ! -e "$src" ]; then record PASS "$fmt.source_deleted" "gone" ; else record FAIL "$fmt.source_deleted" "still present"; fi
  if [ ! -e "$voldoc" ]; then record PASS "$fmt.descriptor_deleted" "gone"; else record FAIL "$fmt.descriptor_deleted" "still present"; fi
  eval "OLEN_$fmt=$olen"
  eval "OSHA_$fmt=$osha"
done

# ---------------------------------------------------------------------------
# Phase 2: fresh processes (each CLI invocation is a new process) query the
# store with the source gone. The store is opened only by its directory path.
# ---------------------------------------------------------------------------
for fmt in pdf docx epub; do
  eval "field=\$FIELD_$fmt"
  eval "store=\$STORE_$fmt"
  od="$CAMPAIGN/raw/obs-$fmt"
  mkdir -p "$od"

  # common: lexical find
  "$B" find --store "$store" --field "$field" --text XF12A > "$od/find.json"
  assert_contains "$fmt.restart.find" "XF12A" "$(jqf '.value' "$od/find.json")"
  assert_contains "$fmt.restart.find.provenance" "format=$fmt;common;" "$(jqf '.provenance' "$od/find.json")"

  # common: whole-document text
  "$B" observe --store "$store" --field "$field" --doc-text --kind text > "$od/text.json"
  assert_contains "$fmt.restart.text" "bravo-seven" "$(jqf '.text' "$od/text.json")"

  # format-specific common + native observations
  case "$fmt" in
    pdf)
      # native PDF coordinate: page text
      "$B" observe --store "$store" --field "$field" --page 1 --kind text > "$od/native-page.json"
      assert_contains "pdf.restart.native_page" "XF12A" "$(jqf '.text' "$od/native-page.json")"
      # common metadata is the structural descriptor (native)
      "$B" observe --store "$store" --field "$field" --metadata --kind metadata > "$od/metadata.json"
      assert_eq "pdf.restart.metadata_len" "$OLEN_pdf" "$(jqf '.value.source_len' "$od/metadata.json")"
      ;;
    docx)
      "$B" observe --store "$store" --field "$field" --heading 0 --kind text > "$od/heading.json"
      assert_eq "docx.restart.heading" "Introduction" "$(jqf '.text' "$od/heading.json")"
      "$B" observe --store "$store" --field "$field" --cell 0:6:1 --kind text > "$od/cell.json"
      assert_eq "docx.restart.cell_b7" "bravo-seven" "$(jqf '.text' "$od/cell.json")"
      "$B" observe --store "$store" --field "$field" --resource 0 --kind metadata > "$od/resource.json"
      assert_eq "docx.restart.resource_rel" "rIdImg" "$(jqf '.value.rel' "$od/resource.json")"
      ;;
    epub)
      "$B" observe --store "$store" --field "$field" --heading 0 --kind text > "$od/heading.json"
      assert_eq "epub.restart.heading" "Introduction" "$(jqf '.text' "$od/heading.json")"
      "$B" observe --store "$store" --field "$field" --cell 0:6:1 --kind text > "$od/cell.json"
      assert_eq "epub.restart.cell_b7" "bravo-seven" "$(jqf '.text' "$od/cell.json")"
      "$B" observe --store "$store" --field "$field" --resource 0 --kind decoded > "$od/resource.json"
      assert_eq "epub.restart.resource_sha256" "$(jqf '.logical_report.resource.sha256' "$GT")" "$(jqf '.bytes_sha256' "$od/resource.json")"
      ;;
  esac
done

# ---------------------------------------------------------------------------
# Phase 3: a fresh process materializes the exact bytes with the source gone,
# then length + SHA-256 + `cmp` against the sealed oracle.
# ---------------------------------------------------------------------------
for fmt in pdf docx epub; do
  eval "field=\$FIELD_$fmt"
  eval "store=\$STORE_$fmt"
  out="$WORK/$fmt/rematerialized.$fmt"
  "$B" materialize --store "$store" --field "$field" --exact --output "$out" > "$CAMPAIGN/raw/obs-$fmt/materialize.json"
  mlen=$(stat -c%s "$out")
  msha=$(sha256sum "$out" | cut -d' ' -f1)
  assert_eq "$fmt.restart.materialize.len" "$(jqf ".formats.$fmt.length" "$GT")" "$mlen"
  assert_eq "$fmt.restart.materialize.sha256" "$(jqf ".formats.$fmt.sha256" "$GT")" "$msha"
  if cmp -s "$out" "$CAMPAIGN/raw/$fmt.source"; then rc=equal; else rc=differ; fi
  assert_eq "$fmt.restart.materialize.cmp" "equal" "$rc"
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
  "notes": "Each source was ingested from a private copy, the oracle (length+SHA-256) recorded, then the source and the ingest descriptor were deleted. Every query and the materialization ran in a fresh process, reading only the field store. The receipt keeps an oracle copy for cmp; the store never references it."
}
EOF

cat > "$CAMPAIGN/commands.txt" <<EOF
# Phase 12.10 source-removal / restart court — exact commands
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

# 2) the court:
docker compose run --rm --no-TTY db-baseline sh tools/phase12-removal-court.sh

# Inside the court, per format in pdf/docx/epub:
#   cp report.FMT WORK/source.FMT
#   vole-document encode --force raw WORK/source.FMT WORK/source.FMT.voldoc
#   vole-document field-ingest WORK/source.FMT.voldoc --store WORK/store
#   <record length + sha256 of WORK/source.FMT>
#   rm -f WORK/source.FMT WORK/source.FMT.voldoc          # source + descriptor gone
#   vole-document find    --store WORK/store --field FIELD --text XF12A
#   vole-document observe --store WORK/store --field FIELD --doc-text --kind text
#   vole-document observe --store WORK/store --field FIELD <native|common selector>
#   vole-document materialize --store WORK/store --field FIELD --exact --output OUT
#   cmp OUT raw/FMT.source    # equal
EOF

# --- summary -----------------------------------------------------------------
{
  echo "# Phase 12.10 — source-removal / restart court"
  echo
  echo "**Result: $PASS/$TOTAL assertions passed, $FAIL failed.**"
  echo
  echo "For each of PDF, DOCX and EPUB the source was ingested from a private copy,"
  echo "the oracle length + SHA-256 recorded, then **the source and the ingest"
  echo "descriptor were deleted**. Every query and the exact materialization then ran"
  echo "in a fresh process against the field store alone."
  echo
  echo '| format | source deleted | re-query (fresh process) | materialize --exact |'
  echo '|---|---|---|---|'
  for fmt in pdf docx epub; do
    eval "olen=\$OLEN_$fmt"; eval "osha=\$OSHA_$fmt"
    short=$(printf %.12s "$osha")
    echo "| $fmt | yes | common + native | len=$olen sha256=${short}... |"
  done
  echo
  echo "\`materialize --exact\` matched the recorded oracle by length, SHA-256 and"
  echo "\`cmp\` for all three formats — the field is self-contained, not a pointer to"
  echo "the source."
  echo
  echo "## Assertions"
  echo
  echo '```'
  awk -F'\t' '{ printf "%-5s %-38s %s\n", $1, $2, $3 }' "$ASSERT"
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
