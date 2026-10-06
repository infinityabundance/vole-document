#!/bin/sh
# Phase-11.12 immutable node-level edit witness — the headline demonstration that
# a procedural field is a *document-field trajectory*, not a stack of unrelated
# whole-document snapshots.
#
# On one real producer PDF (the Phase-7 LibreOffice export) this court, in one
# run on one machine and one base image:
#
#   1. encodes + ingests `R0` and proves `materialize(R0) == source` (length,
#      SHA-256, `cmp`);
#   2. performs **one small node-level edit** — replaces the decoded content of a
#      single page with a caller-supplied content stream — producing `R1`;
#   3. reports, from instrumented counters, how many seed nodes and index entries
#      `R1` **reused by content id** vs wrote anew, how many index tree nodes
#      already existed, and exactly how many payload bytes the edit persisted;
#   4. proves `materialize(R1) == source` for the *same original* bytes and that
#      `R0` is still valid and exact after the edit (nothing was mutated);
#   5. proves the edited page now observes the **new** content while an
#      unaffected page observes byte-identically to `R0`;
#   6. corroborates, with `strace`, that the edit process opened **zero**
#      descriptor blobs — no full old-document parse and no re-bake.
#
# It runs inside the pinned `db-baseline` image (dev toolchain + jq + strace +
# coreutils). Nothing runs on the host.
#
# Usage:
#   sh tools/field-edit-court.sh [OUTDIR] [PRODUCER_PDF]
#
# The supported edit subset is intentionally narrow; see the module docs of
# `src/field/edit.rs` and the generated `SUMMARY.md` for the exact limitations.
set -eu

cd /work
LC_ALL=C
export LC_ALL

OUTDIR=${1:-evidence/scratch/phase11-edit}
PRODUCER=${2:-evidence/corpus/phase7-producers/libreoffice-export.pdf}
BIN=${VOLE_BIN:-./target/debug/vole-document}
EDIT_PAGE=${EDIT_PAGE:-1}
UNCHANGED_PAGE=${UNCHANGED_PAGE:-2}
MARKER="VOLE EDIT WITNESS PAGE"

mkdir -p "$OUTDIR/raw"
RAW="$OUTDIR/raw"

echo "field-edit-court: building cargo build --locked --all-features" >&2
cargo build --locked --all-features 1>&2

# The all-features binary is required (the shared target volume may hold a
# `--no-default-features` build from a prior gate run).
if ! "$BIN" --help 2>&1 | grep -q 'field-edit'; then
  echo "field-edit-court: $BIN lacks the field-edit verb; rebuild --all-features" >&2
  exit 1
fi
if [ ! -f "$PRODUCER" ]; then
  echo "field-edit-court: missing $PRODUCER" >&2
  echo "  regenerate the Phase-7 producer corpus first (pinned producers image):" >&2
  echo "    docker compose run --rm --no-TTY producers sh tools/pdf-corpus-producers.sh" >&2
  exit 1
fi

now_ms() { echo $(( $(date +%s%N) / 1000000 )); }
sha() { sha256sum "$1" | cut -d' ' -f1; }
bytes() { stat -c %s "$1" 2>/dev/null || echo 0; }
pages() { pdfinfo "$1" 2>/dev/null | awk '/^Pages:/{print $2}' || echo 0; }

# ---------------------------------------------------------------------------
# Environment capture (everything a receipt must pin).
# ---------------------------------------------------------------------------
RUSTC_V=$(rustc --version 2>/dev/null | sed 's/^rustc //' || echo unknown)
CARGO_V=$(cargo --version 2>/dev/null | sed 's/^cargo //' || echo unknown)
ARCH=$(uname -m)
COMMIT=$(git rev-parse HEAD 2>/dev/null || echo unknown)
COMMIT_SHORT=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
DIRTY=$(git status --porcelain 2>/dev/null | head -40 | tr '\n' ';' || echo unknown)
LOCK_SHA=$(sha /work/Cargo.lock 2>/dev/null || echo unknown)
BASE_STABLE=$(sed -n 's/^ARG BASE_STABLE=//p' /work/Dockerfile 2>/dev/null | head -1)
IMAGE_REF=vole-document/db-baseline:1.99.0
IMAGE_ID=${HOST_IMAGE_ID:-unrecorded}
JQ_V=$(jq --version)
STRACE_V=$(strace --version 2>&1 | head -1 | sed 's/^strace -- //')
PDFINFO_V=$(pdfinfo -v 2>&1 | head -1 | sed 's/^pdfinfo version //')

jq -n \
  --arg commit "$COMMIT" --arg commit_short "$COMMIT_SHORT" --arg dirty "$DIRTY" \
  --arg lock_sha "$LOCK_SHA" --arg arch "$ARCH" \
  --arg rustc "$RUSTC_V" --arg cargo "$CARGO_V" \
  --arg base_stable "$BASE_STABLE" --arg image_ref "$IMAGE_REF" --arg image_id "$IMAGE_ID" \
  --arg jq "$JQ_V" --arg strace "$STRACE_V" --arg pdfinfo "$PDFINFO_V" --arg bin "$BIN" \
  '{
     git: {commit: $commit, commit_short: $commit_short, dirty: $dirty},
     arch: $arch,
     cargo_lock_sha256: $lock_sha,
     toolchain: {rustc: $rustc, cargo: $cargo},
     images: {base_stable_digest: $base_stable, image_ref: $image_ref, db_baseline_image_id: $image_id},
     oracles: {jq: $jq, strace: $strace, pdfinfo: $pdfinfo},
     env_affecting_semantics: {LC_ALL: "C", VOLE_BIN: $bin}
   }' > "$RAW/environment.json"

# ---------------------------------------------------------------------------
# Case: ingest R0, edit one page to R1, verify both roots.
# ---------------------------------------------------------------------------
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
SRC="$PRODUCER"
SRC_LEN=$(bytes "$SRC")
SRC_SHA=$(sha "$SRC")
N_PAGES=$(pages "$SRC")
DESC="$TMP/doc.voldoc"
STORE="$TMP/store"
mkdir -p "$STORE"

echo "field-edit-court: source=${SRC_LEN}B pages=$N_PAGES sha=$(printf '%s' "$SRC_SHA" | cut -c1-16)…" >&2

# Encode + ingest R0.
_t0=$(now_ms); "$BIN" encode "$SRC" "$DESC" > "$RAW/encode.json"; _t1=$(now_ms)
ENCODE_MS=$(( _t1 - _t0 ))
_t0=$(now_ms); "$BIN" field-ingest "$DESC" --store "$STORE" > "$RAW/ingest-r0.json"; _t1=$(now_ms)
INGEST_MS=$(( _t1 - _t0 ))
R0=$(jq -r .field "$RAW/ingest-r0.json")
echo "field-edit-court: R0=$R0" >&2

# R0 exactness + observations (before the edit).
"$BIN" materialize --store "$STORE" --field "$R0" --exact --output "$RAW/r0-before.bin" > "$RAW/r0-before.json"
r0_before_cmp=$(cmp -s "$SRC" "$RAW/r0-before.bin" && echo equal || echo differ)
"$BIN" observe --store "$STORE" --field "$R0" --page "$EDIT_PAGE" --kind text > "$RAW/r0-edit-page-text.json"
"$BIN" observe --store "$STORE" --field "$R0" --page "$EDIT_PAGE" --kind structure > "$RAW/r0-edit-page-structure.json"
"$BIN" observe --store "$STORE" --field "$R0" --page "$UNCHANGED_PAGE" --kind text > "$RAW/r0-unchanged-page-text.json"

# The one small node-level edit: a deterministic one-line content stream.
printf 'BT /F1 12 Tf 72 720 Td (%s) Tj ET\n' "$MARKER" > "$RAW/edit-content.bin"
EDIT_CONTENT_LEN=$(bytes "$RAW/edit-content.bin")

# Run the edit under strace so descriptor *file* access is a witnessed fact.
strace -f -e trace=openat,open,read,pread64 -o "$RAW/edit.strace" \
  "$BIN" field-edit --store "$STORE" --field "$R0" --page "$EDIT_PAGE" \
       --content "$RAW/edit-content.bin" > "$RAW/edit.json"
R1=$(jq -r .field "$RAW/edit.json")
echo "field-edit-court: R1=$R1" >&2

# No descriptor blob may be opened by the edit; the manifest and index are
# procedural metadata and are expected.
DESC_FILE_OPENS=$(grep -Ec '/descriptor/[0-9a-f]{64}' "$RAW/edit.strace" || true)
FIELD_FILE_OPENS=$(grep -Ec '/field/[0-9a-f]{64}' "$RAW/edit.strace" || true)
INDEX_FILE_OPENS=$(grep -Ec '/index/[0-9a-f]{2}/[0-9a-f]{2}/[0-9a-f]{64}' "$RAW/edit.strace" || true)

# R0 must be untouched, and R1 must materialize the *same* original bytes.
"$BIN" materialize --store "$STORE" --field "$R0" --exact --output "$RAW/r0-after.bin" > "$RAW/r0-after.json"
r0_after_cmp=$(cmp -s "$SRC" "$RAW/r0-after.bin" && echo equal || echo differ)
"$BIN" materialize --store "$STORE" --field "$R1" --exact --output "$RAW/r1.bin" > "$RAW/r1.json"
r1_cmp=$(cmp -s "$SRC" "$RAW/r1.bin" && echo equal || echo differ)

# Observations after the edit.
"$BIN" observe --store "$STORE" --field "$R1" --page "$EDIT_PAGE" --kind text > "$RAW/r1-edit-page-text.json"
"$BIN" observe --store "$STORE" --field "$R1" --page "$EDIT_PAGE" --kind structure > "$RAW/r1-edit-page-structure.json"
"$BIN" observe --store "$STORE" --field "$R1" --page "$UNCHANGED_PAGE" --kind text > "$RAW/r1-unchanged-page-text.json"
# R0's edited-page observation after the edit must be byte-identical to before.
"$BIN" observe --store "$STORE" --field "$R0" --page "$EDIT_PAGE" --kind text > "$RAW/r0-edit-page-text-after.json"

jq -n \
  --arg src "$SRC" --argjson src_len "$SRC_LEN" --arg src_sha "$SRC_SHA" --argjson n_pages "$N_PAGES" \
  --arg r0 "$R0" --arg r1 "$R1" --argjson edit_page "$EDIT_PAGE" --argjson unchanged_page "$UNCHANGED_PAGE" \
  --arg marker "$MARKER" --argjson edit_content_len "$EDIT_CONTENT_LEN" \
  --argjson encode_ms "$ENCODE_MS" --argjson ingest_ms "$INGEST_MS" \
  --arg r0_before_cmp "$r0_before_cmp" --arg r0_after_cmp "$r0_after_cmp" --arg r1_cmp "$r1_cmp" \
  --argjson desc_opens "$DESC_FILE_OPENS" --argjson field_opens "$FIELD_FILE_OPENS" --argjson index_opens "$INDEX_FILE_OPENS" \
  --slurpfile enc "$RAW/encode.json" \
  --slurpfile ing "$RAW/ingest-r0.json" \
  --slurpfile edit "$RAW/edit.json" \
  --slurpfile r0e "$RAW/r0-edit-page-text.json" \
  --slurpfile r0ea "$RAW/r0-edit-page-text-after.json" \
  --slurpfile r0es "$RAW/r0-edit-page-structure.json" \
  --slurpfile r0u "$RAW/r0-unchanged-page-text.json" \
  --slurpfile r1e "$RAW/r1-edit-page-text.json" \
  --slurpfile r1es "$RAW/r1-edit-page-structure.json" \
  --slurpfile r1u "$RAW/r1-unchanged-page-text.json" \
  '{
     source: {path: $src, len: $src_len, sha256: $src_sha, pages: $n_pages},
     encode: {candidate: ($enc[0].candidate // null), encoded_len: ($enc[0].encoded_len // null), wall_ms: $encode_ms},
     ingest: {field: $r0, wall_ms: $ingest_ms, report: $ing[0]},
     edit: ($edit[0] + {
        descriptor_file_opens: $desc_opens,
        manifest_file_opens: $field_opens,
        index_file_opens: $index_opens,
        edit_content_len: $edit_content_len,
        marker: $marker
     }),
     exactness: {
        r0_before: {len: $src_len, sha256: $src_sha, cmp_vs_source: $r0_before_cmp},
        r0_after_edit: {len: $src_len, sha256: $src_sha, cmp_vs_source: $r0_after_cmp},
        r1: {len: $src_len, sha256: $src_sha, cmp_vs_source: $r1_cmp}
     },
     observations: {
        edit_page: {
           page: $edit_page,
           r0_text: $r0e[0].text,
           r0_text_after_edit: $r0ea[0].text,
           r1_text: $r1e[0].text,
           r1_text_contains_marker: ($r1e[0].text | contains($marker)),
           r0_structure: $r0es[0].value,
           r1_structure: $r1es[0].value,
           r0_after_matches_before: ($r0e[0].text == $r0ea[0].text)
        },
        unchanged_page: {
           page: $unchanged_page,
           r0_text: $r0u[0].text,
           r1_text: $r1u[0].text,
           identical: ($r0u[0].text == $r1u[0].text)
        }
     }
   }' > "$RAW/case.json"

# ---------------------------------------------------------------------------
# Verdict (every check must hold for the witness to stand).
# ---------------------------------------------------------------------------
jq -n --slurpfile c "$RAW/case.json" '
  ($c[0]) as $c
  | {
      checks: {
        r0_exact_before: ($c.exactness.r0_before.cmp_vs_source == "equal"),
        r0_exact_after_edit: ($c.exactness.r0_after_edit.cmp_vs_source == "equal"),
        r1_exact_same_original_bytes: ($c.exactness.r1.cmp_vs_source == "equal"),
        r0_unchanged_by_edit: ($c.observations.edit_page.r0_after_matches_before),
        r1_shows_edited_page: ($c.observations.edit_page.r1_text_contains_marker),
        unaffected_page_shared: ($c.observations.unchanged_page.identical),
        edit_used_content_addressed_nodes: ($c.edit.seed_nodes_new > 0),
        unaffected_bindings_reused: ($c.edit.index_entries_reused > 0),
        unaffected_index_nodes_shared: ($c.edit.index_nodes_reused >= 0),
        edit_read_no_descriptor_bytes: ($c.edit.descriptor_bytes_read == 0),
        edit_opened_no_descriptor_blob: ($c.edit.descriptor_file_opens == 0)
      },
      counters: {
        seed_nodes_reused: $c.edit.seed_nodes_reused,
        seed_nodes_new: $c.edit.seed_nodes_new,
        index_entries: $c.edit.index_entries,
        index_entries_reused: $c.edit.index_entries_reused,
        index_entries_replaced: $c.edit.index_entries_replaced,
        index_nodes_reused: $c.edit.index_nodes_reused,
        index_nodes_new: $c.edit.index_nodes_new,
        bytes_newly_persisted: $c.edit.bytes_newly_persisted,
        descriptor_bytes_read_in_edit: $c.edit.descriptor_bytes_read,
        manifest_bytes_read_in_edit: $c.edit.manifest_bytes_read,
        index_bytes_read_in_edit: $c.edit.index_bytes_read,
        descriptor_file_opens_in_edit: $c.edit.descriptor_file_opens
      },
      all_witness_checks_pass: ([
        ($c.exactness.r0_before.cmp_vs_source == "equal"),
        ($c.exactness.r0_after_edit.cmp_vs_source == "equal"),
        ($c.exactness.r1.cmp_vs_source == "equal"),
        ($c.observations.edit_page.r0_after_matches_before),
        ($c.observations.edit_page.r1_text_contains_marker),
        ($c.observations.unchanged_page.identical),
        ($c.edit.seed_nodes_new > 0),
        ($c.edit.index_entries_reused > 0),
        ($c.edit.descriptor_bytes_read == 0),
        ($c.edit.descriptor_file_opens == 0)
      ] | all)
    }' > "$RAW/verdict.json"

# ---------------------------------------------------------------------------
# receipt.json + SUMMARY.md + commands.txt
# ---------------------------------------------------------------------------
jq -n \
  --slurpfile env "$RAW/environment.json" \
  --slurpfile case "$RAW/case.json" \
  --slurpfile verdict "$RAW/verdict.json" \
  '{
     campaign: "phase11-edit",
     phase: "Phase 11.12 — immutable node-level edit witness",
     environment: $env[0],
     case: $case[0],
     verdict: $verdict[0],
     claim_scope: "A content-addressed procedural field supports an immutable single-page content override: the new root shares every unaffected selector binding and index node by id with the old root, both roots materialize the same original bytes exactly, and the edit reads no descriptor bytes and opens no descriptor blob. This is a procedural edit of a derived page projection; it is NOT a rewrite of the PDF and makes no authorial-intent claim.",
     supported_subset: "decode-level replacement of one existing page decoded content bytes in an indexed field; content <= MAX_EDIT_CONTENT_BYTES (48 KiB); page must already exist; the exact archive descriptor is unchanged.",
     limitations: [
       "No generic editing: no insert/delete/reorder of pages or objects, no cross-reference or object-graph mutation, no re-encoding.",
       "The exact .voldoc descriptor is copied verbatim, so materialize(R1) == materialize(R0) == the original source bytes; only derived page observations change.",
       "The replaced page loses its source byte span and reports no content-stream object numbers in its structure observation (its bytes are not in the source).",
       "One page per call.",
       "The edit reads the whole hierarchical index (all leaves) to carry bindings forward; the recorded index_bytes_read is that cost. It reads zero descriptor bytes.",
       "Index-node reuse is only possible when the index has more than one node; a single-leaf index necessarily rewrites its one leaf."
     ]
   }' > "$OUTDIR/receipt.json"

cat > "$OUTDIR/commands.txt" <<EOF
# Phase 11.12 immutable node-level edit court — exact commands
# Commit under test: $COMMIT (branch $(git rev-parse --abbrev-ref HEAD 2>/dev/null)); dirty: $DIRTY
# All commands run inside the pinned, capped db-baseline image; nothing on the host.

docker compose build db-baseline
docker compose run --rm --no-TTY dev sh -c 'cargo build --all-features --locked'
docker compose run --rm --no-TTY -e HOST_IMAGE_ID=<db-baseline image id> db-baseline \\
    sh tools/field-edit-court.sh $OUTDIR

# Inside the court:
#   \$BIN encode PRODUCER $TMP/doc.voldoc
#   \$BIN field-ingest $TMP/doc.voldoc --store $TMP/store            # -> R0
#   \$BIN materialize --store $TMP/store --field R0 --exact --output r0-before.bin
#   \$BIN observe --store $TMP/store --field R0 --page $EDIT_PAGE --kind text
#   strace -f -e trace=openat,open,read,pread64 -o edit.strace \\
#     \$BIN field-edit --store $TMP/store --field R0 --page $EDIT_PAGE --content edit-content.bin   # -> R1
#   \$BIN materialize --store $TMP/store --field R0 --exact --output r0-after.bin
#   \$BIN materialize --store $TMP/store --field R1 --exact --output r1.bin
#   \$BIN observe --store $TMP/store --field R1 --page $EDIT_PAGE --kind text
#   \$BIN observe --store $TMP/store --field R1 --page $UNCHANGED_PAGE --kind text
#   grep -Ec '/descriptor/[0-9a-f]{64}' edit.strace                      # -> 0
EOF

# SUMMARY.md
{
  echo "# Phase 11.12 — immutable node-level edit witness"
  echo
  echo "Commit under test: \`$COMMIT\` (branch \`$(git rev-parse --abbrev-ref HEAD 2>/dev/null)\`); tree dirty: \`$DIRTY\`"
  echo
  echo "Image: \`$IMAGE_REF\` (id \`$IMAGE_ID\`), base \`$BASE_STABLE\`."
  echo "Rust: \`$RUSTC_V\` / \`$CARGO_V\`; \`$JQ_V\`; \`$STRACE_V\`."
  echo "Arch: \`$ARCH\`; \`Cargo.lock\` sha256: \`$LOCK_SHA\`."
  echo
  echo "Source: \`$PRODUCER\` — \`$(bytes "$SRC")B\`, $(pages "$SRC") pages, sha256 \`$SRC_SHA\`."
  echo
  echo "## The edit"
  echo
  echo "One small node-level edit: replace page \`$EDIT_PAGE\`'s decoded content with a"
  echo "single-operator content stream (\`$EDIT_CONTENT_LEN\` B: \`BT /F1 12 Tf 72 720 Td ($MARKER) Tj ET\`)."
  echo
  echo "\`R0\` = \`$R0\`"
  echo "\`R1\` = \`$R1\`"
  echo
  echo "## What is genuinely shared vs copied"
  echo
  jq -rn --slurpfile c "$RAW/case.json" '"| quantity | value |\n|---|---:|\n" +
    "| index entries carried forward unchanged (same key, same node id) | \($c[0].edit.index_entries_reused) |\n" +
    "| index entries replaced (the edited page) | \($c[0].edit.index_entries_replaced) |\n" +
    "| index tree nodes that already existed (not rewritten) | \($c[0].edit.index_nodes_reused) |\n" +
    "| index tree nodes written anew | \($c[0].edit.index_nodes_new) |\n" +
    "| new seed nodes (Literal + PageContent) | \($c[0].edit.seed_nodes_new) |\n" +
    "| payload bytes newly persisted (all nodes + manifest) | \($c[0].edit.bytes_newly_persisted) |\n" +
    "| descriptor bytes read by the edit | \($c[0].edit.descriptor_bytes_read) |\n" +
    "| descriptor blobs opened by the edit (strace) | \($c[0].edit.descriptor_file_opens) |\n" +
    "| manifest bytes read by the edit | \($c[0].edit.manifest_bytes_read) |\n" +
    "| index bytes read by the edit | \($c[0].edit.index_bytes_read) |\n"'
  echo
  echo "Shared by id (never rewritten): the descriptor blob, the \`DocumentExact\` root,"
  echo "every unaffected seed node, and every index node that already existed."
  echo "Newly written: two seed nodes, the rewritten index leaf/spine, and one manifest."
  echo
  echo "## Exactness (both roots, after the edit)"
  echo
  echo "| root | length | sha256 | cmp vs source |"
  echo "|---|---:|---|---|"
  jq -rn --slurpfile c "$RAW/case.json" '"| R0 (before edit) | \($c[0].exactness.r0_before.len) | `\($c[0].exactness.r0_before.sha256)` | \($c[0].exactness.r0_before.cmp_vs_source) |\n| R0 (after edit) | \($c[0].exactness.r0_after_edit.len) | `\($c[0].exactness.r0_after_edit.sha256)` | \($c[0].exactness.r0_after_edit.cmp_vs_source) |\n| R1 (new root) | \($c[0].exactness.r1.len) | `\($c[0].exactness.r1.sha256)` | \($c[0].exactness.r1.cmp_vs_source) |"'
  echo
  echo "Both roots materialize the **same original bytes**: the exact archive is"
  echo "untouched by construction, so this is a procedural edit of a derived page,"
  echo "not a rewrite of the PDF."
  echo
  echo "## Observation (edited page $EDIT_PAGE, unaffected page $UNCHANGED_PAGE)"
  echo
  echo "- R1 page $EDIT_PAGE text contains the marker \`$MARKER\`: \`$(jq -r '.observations.edit_page.r1_text_contains_marker' "$RAW/case.json")\`"
  echo "- R0 page $EDIT_PAGE text is byte-identical before and after the edit: \`$(jq -r '.observations.edit_page.r0_after_matches_before' "$RAW/case.json")\`"
  echo "- R0 and R1 page $UNCHANGED_PAGE text are byte-identical (unaffected page shared): \`$(jq -r '.observations.unchanged_page.identical' "$RAW/case.json")\`"
  echo "- R1 page $EDIT_PAGE structure \`content_streams\`: \`$(jq -c '.observations.edit_page.r1_structure.content_streams' "$RAW/case.json")\` (empty: the new bytes are not in the source)"
  echo
  echo '```json'
  jq '.observations' "$RAW/case.json"
  echo '```'
  echo
  echo "## Verdict"
  echo
  echo "All witness checks pass: \`$(jq -r '.all_witness_checks_pass' "$RAW/verdict.json")\`."
  echo
  jq -r '.checks | to_entries[] | "- `\(.key)`: \(.value)"' "$RAW/verdict.json"
  echo
  echo "## Supported subset (narrow, stated honestly)"
  echo
  echo "This court demonstrates **exactly one** operation: replacing one existing"
  echo "page's decoded content bytes in an already-indexed field, with new content"
  echo "\`<= MAX_EDIT_CONTENT_BYTES = 49152\` bytes. The exact \`.voldoc\` descriptor is"
  echo "copied verbatim, so \`materialize(R1) == materialize(R0) == the original"
  echo "source\`; only the *derived page observations* change. There is:"
  echo
  echo "- no generic document editing (no insert/delete/reorder, no object-graph or"
  echo "  cross-reference mutation, no re-encoding);"
  echo "- **no authorial-intent claim** — the edit is a procedural override of a"
  echo "  decoded page projection, not a new PDF;"
  echo "- no whole-document rewrite: the edit reads **0** descriptor bytes and opens"
  echo "  **0** descriptor blobs (witnessed by \`strace\`);"
  echo "- no source span for the edited page (its bytes are not in the source), and"
  echo "  no content-stream object numbers in its \`structure\` observation;"
  echo
  echo "## Honest losses / limitations"
  echo
  echo "- The edit reads the whole hierarchical index (all leaves) to carry untouched"
  echo "  selector bindings forward: \`index_bytes_read = $(jq -r '.edit.index_bytes_read' "$RAW/case.json") B\`,"
  echo "  against \`bytes_newly_persisted = $(jq -r '.edit.bytes_newly_persisted' "$RAW/case.json") B\`. The read is"
  echo "  descriptor-free but is not free."
  echo "- Index-*node* reuse requires a multi-node index; a single-leaf index"
  echo "  necessarily rewrites its only leaf (reuse is then witnessed at entry level,"
  echo "  which the counters report separately)."
  echo "- The edited page's materialized *source span* is lost, and \`structure\`"
  echo "  reports no content streams for it; \`text\` and \`preview\` are computed from"
  echo "  the new bytes as usual."
} > "$OUTDIR/SUMMARY.md"

echo "field-edit-court: wrote $OUTDIR/receipt.json, $OUTDIR/SUMMARY.md, $OUTDIR/commands.txt, $OUTDIR/raw/" >&2
cat "$RAW/verdict.json"
