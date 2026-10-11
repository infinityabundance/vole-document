#!/bin/sh
# Phase 21.34 — scientific-container (NetCDF classic / HDF5-subset) surface + exact closure.
#
# Pre-registered (Phase-21 subphase 21.34.1). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture, `materialize(field) == source`
#       (length + SHA-256 + cmp) after the source file AND the standalone descriptor
#       are deleted, in a fresh process. The court FAILS unless this is 100% on every
#       fixture.
#   H2 (surface + preserved representation) — the representation-preserving model is
#       exposed: NetCDF's dimension table (incl. the unlimited dimension), global
#       attributes, and variable list (type / shape / record flag / exact data spans);
#       HDF5's group/dataset tree (`/dset`, `/grp/inner`) with the dataset datatype /
#       simple dataspace / contiguous layout and the root-group attribute; the recorded
#       dialect (`netcdf`/`hdf5`). Common metadata/text/search-match and native
#       hdf-dialect/hdf-object/hdf-dimension/hdf-attribute/hdf-find answer.
#   H3 (honest declines / boundaries) — a filtered dataset and a v2 object header
#       (out-of-subset HDF5 constructs) decline typed (rc 6); a malformed NetCDF header
#       declines structurally (rc 50); a bad-version/truncated/all-zero-sizes HDF5
#       superblock and prose stay `opaque` (an hdf selector on them declines rc 6).
#
# Runs in the pinned `dev` service using only POSIX `sh`, coreutils, git, and the
# shipped binary (no python3, no h5py, no h5dump, no netCDF):
#   docker compose run --rm --no-TTY dev sh tools/phase21-34-1-hdf-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
# The `dev` lane is hard-capped on memory; keep the debug-info symbol footprint bounded
# so a full all-features link fits (symbols only — no semantics change).
: "${CARGO_PROFILE_DEV_DEBUG:=0}"
export CARGO_PROFILE_DEV_DEBUG
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="2026-10-10"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-34-1-hdf-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-34
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== generate fixtures (dependency-free NetCDF/HDF5 writer) ==="
cargo run --locked --all-features --example hdf_fixtures -- "$WORK/corpus" 2>&1 | tail -1

# Fixture classes.
#   NORMAL      — valid NetCDF/HDF5: H1 exactness + H2 surface.
#   STRUCTURAL  — detects as `hdf` but the model is malformed (rc 50).
#   UNSUPPORTED — detects as `hdf` but an out-of-subset HDF5 construct (rc 6).
#   OPAQUE      — not detected as `hdf`; an hdf selector declines typed (rc 6).
NORMAL="netcdf_cdf1.nc netcdf_cdf2.nc hdf5_basic.h5 hdf5_userblock.h5"
STRUCTURAL="netcdf_badoffset.nc nc_truncated.nc nc_junk.nc"
UNSUPPORTED="hdf5_filtered.h5 hdf5_ohv2.h5"
OPAQUE="hdf5_badmagic.bin hdf5_truncated.h5 hdf5_junk.bin prose.txt"

dialect_for() {
    case "$1" in
        netcdf_*|nc_*) echo "netcdf" ;;
        *) echo "hdf5" ;;
    esac
}
obj_index_for() {
    case "$1" in
        netcdf_*) echo "2" ;;
        *) echo "1" ;;
    esac
}
obj_name_for() {
    case "$1" in
        netcdf_*) echo "temp" ;;
        *) echo "dset" ;;
    esac
}
attr_name_for() {
    case "$1" in
        netcdf_*) echo "title" ;;
        *) echo "root_attr" ;;
    esac
}
find_for() {
    case "$1" in
        netcdf_*) echo "lat" ;;
        *) echo "grp" ;;
    esac
}
is_in() {
    for x in $2; do [ "$x" = "$1" ] && return 0; done
    return 1
}
# Extract one JSON string field from a single-line JSON object (greedy, no jq).
json_str() {
    printf '%s' "$1" | sed -n "s/.*\"$2\":\"\([^\"]*\)\".*/\1/p"
}

TSV="$RAW/results.tsv"
: > "$TSV"
printf 'fixture\tformat\tsrc_len\tsrc_sha\tout_len\tout_sha\tcmp\texact\tdecline_rc\tctl_rc\n' >> "$TSV"

exact_ok=0
exact_fail=0
surface_fail=0
normal_n=0
obj_ok=0
obj_n=0
dim_ok=0
dim_n=0
attr_ok=0
attr_n=0
structural_ok=0
structural_n=0
unsupported_ok=0
unsupported_n=0
opaque_ok=0
opaque_n=0

for f in $STRUCTURAL; do
    src="$WORK/ref/$f"
    cp "$WORK/corpus/$f" "$src"
    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    fmt="$(json_str "$build_json" format)"
    field="$(json_str "$build_json" field)"
    structural_n=$((structural_n + 1))
    [ "$fmt" = "hdf" ] || { echo "FINDING: $f detected as $fmt (want hdf)" >&2; surface_fail=$((surface_fail + 1)); }
    set +e
    "$BIN" observe --store "$WORK/store" --field "$field" --hdf-dialect --kind text \
        > "$RAW/$f.decline.txt" 2>&1
    ctl_rc=$?
    set -e
    if [ "$ctl_rc" -eq 50 ]; then structural_ok=$((structural_ok + 1)); else echo "FINDING: $f structural decline rc=$ctl_rc (want 50)" >&2; surface_fail=$((surface_fail + 1)); fi
    printf '%s\t%s\t-\t-\t-\t-\t-\t-\t%s\t%s\n' "$f" "$fmt" "$ctl_rc" "$ctl_rc" >> "$TSV"
done

for f in $UNSUPPORTED; do
    src="$WORK/ref/$f"
    cp "$WORK/corpus/$f" "$src"
    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    fmt="$(json_str "$build_json" format)"
    field="$(json_str "$build_json" field)"
    unsupported_n=$((unsupported_n + 1))
    [ "$fmt" = "hdf" ] || { echo "FINDING: $f detected as $fmt (want hdf)" >&2; surface_fail=$((surface_fail + 1)); }
    set +e
    "$BIN" observe --store "$WORK/store" --field "$field" --hdf-dialect --kind text \
        > "$RAW/$f.decline.txt" 2>&1
    ctl_rc=$?
    set -e
    if [ "$ctl_rc" -eq 6 ]; then unsupported_ok=$((unsupported_ok + 1)); else echo "FINDING: $f out-of-subset decline rc=$ctl_rc (want 6)" >&2; surface_fail=$((surface_fail + 1)); fi
    printf '%s\t%s\t-\t-\t-\t-\t-\t-\t%s\t%s\n' "$f" "$fmt" "$ctl_rc" "$ctl_rc" >> "$TSV"
done

for f in $OPAQUE; do
    src="$WORK/ref/$f"
    cp "$WORK/corpus/$f" "$src"
    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    fmt="$(json_str "$build_json" format)"
    field="$(json_str "$build_json" field)"
    opaque_n=$((opaque_n + 1))
    [ "$fmt" = "opaque" ] || { echo "FINDING: opaque control $f detected as $fmt (want opaque)" >&2; surface_fail=$((surface_fail + 1)); }
    set +e
    "$BIN" observe --store "$WORK/store" --field "$field" --hdf-dialect --kind text \
        > "$RAW/$f.hdf-decline.txt" 2>&1
    ctl_rc=$?
    set -e
    if [ "$ctl_rc" -eq 6 ]; then opaque_ok=$((opaque_ok + 1)); else echo "FINDING: opaque control $f hdf selector rc=$ctl_rc (want 6)" >&2; surface_fail=$((surface_fail + 1)); fi
    printf '%s\t%s\t-\t-\t-\t-\t-\t-\t-\t%s\n' "$f" "$fmt" "$ctl_rc" >> "$TSV"
done

for f in $NORMAL; do
    # The reference copy is never deleted; the corpus copy is ingested and then removed.
    src="$WORK/ref/$f"
    cp "$WORK/corpus/$f" "$src"
    src_len="$(wc -c < "$src" | tr -d ' ')"
    src_sha="$(sha256sum "$src" | cut -d' ' -f1)"

    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    field="$(json_str "$build_json" field)"
    fmt="$(json_str "$build_json" format)"

    normal_n=$((normal_n + 1))
    want_dialect="$(dialect_for "$f")"
    [ "$fmt" = "hdf" ] || { echo "FINDING: $f not detected as hdf (fmt=$fmt)" >&2; surface_fail=$((surface_fail + 1)); }

    # H2: common metadata / text / search-match + native dialect / find.
    "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
        > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
        > "$RAW/$f.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --hdf-dialect --kind text \
        > "$RAW/$f.dialect.txt" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --hdf-find "$(find_for "$f")" --kind metadata \
        > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))

    grep -q '"format":"hdf"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata missing format" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q "\"dialect\":\"$want_dialect\"" "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata dialect != $want_dialect" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q "\"text\":\"$want_dialect\"" "$RAW/$f.dialect.txt" || { echo "FINDING: $f dialect text != $want_dialect" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"matches":\[' "$RAW/$f.find.json" || { echo "FINDING: $f find returned no matches" >&2; surface_fail=$((surface_fail + 1)); }

    # H2: the native dataset index (ExactBytes + metadata).
    idx="$(obj_index_for "$f")"
    name="$(obj_name_for "$f")"
    obj_n=$((obj_n + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --hdf-object "$idx" --kind exact \
        > "$RAW/$f.object.bin" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --hdf-object "$idx" --kind metadata \
        > "$RAW/$f.object.json" 2>&1 || surface_fail=$((surface_fail + 1))
    grep -q "\"name\":\"$name\"" "$RAW/$f.object.json" || { echo "FINDING: $f object $idx name != $name" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"kind":"dataset"' "$RAW/$f.object.json" || { echo "FINDING: $f object $idx is not a dataset" >&2; surface_fail=$((surface_fail + 1)); }
    [ -s "$RAW/$f.object.bin" ] || { echo "FINDING: $f object $idx exact bytes empty" >&2; surface_fail=$((surface_fail + 1)); }
    obj_ok=$((obj_ok + 1))

    # H2: the native dimension (NetCDF only) and the global/root attribute.
    if [ "$want_dialect" = "netcdf" ]; then
        dim_n=$((dim_n + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --hdf-dimension 0 --kind metadata \
            > "$RAW/$f.dimension0.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"unlimited":true' "$RAW/$f.dimension0.json" || { echo "FINDING: $f dimension 0 is not unlimited" >&2; surface_fail=$((surface_fail + 1)); }
        dim_ok=$((dim_ok + 1))
    fi
    attr_n=$((attr_n + 1))
    aname="$(attr_name_for "$f")"
    "$BIN" observe --store "$WORK/store" --field "$field" --hdf-attribute 0 --kind metadata \
        > "$RAW/$f.attribute0.json" 2>&1 || surface_fail=$((surface_fail + 1))
    grep -q "\"name\":\"$aname\"" "$RAW/$f.attribute0.json" || { echo "FINDING: $f attribute 0 name != $aname" >&2; surface_fail=$((surface_fail + 1)); }
    attr_ok=$((attr_ok + 1))

    # H1: delete the source AND the standalone descriptor, then rematerialize exactly
    # in a fresh process and compare (length + SHA-256 + cmp).
    rm -f "$WORK/corpus/$f" "$WORK/$f.voldoc"
    "$BIN" materialize --store "$WORK/store" --field "$field" --exact \
        --output "$WORK/$f.out" > /dev/null
    out_len="$(wc -c < "$WORK/$f.out" | tr -d ' ')"
    out_sha="$(sha256sum "$WORK/$f.out" | cut -d' ' -f1)"
    if cmp -s "$src" "$WORK/$f.out"; then cmp_ok=true; else cmp_ok=false; fi
    if [ "$out_len" = "$src_len" ] && [ "$out_sha" = "$src_sha" ] && [ "$cmp_ok" = true ]; then
        exact=true; exact_ok=$((exact_ok + 1))
    else
        exact=false; exact_fail=$((exact_fail + 1))
        echo "FINDING: not byte-exact after removal: $f" >&2
    fi

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t-\t-\n' \
        "$f" "$fmt" "$src_len" "$src_sha" "$out_len" "$out_sha" "$cmp_ok" "$exact" >> "$TSV"
done

echo "=== exactness after source + descriptor deletion ==="
cat "$TSV"

fixtures_n=$((normal_n + structural_n + unsupported_n + opaque_n))

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 21.34.1 scientific-container court — matrix"
    echo
    echo "| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |"
    echo "| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |"
    # shellcheck disable=SC2162
    while IFS='	' read -r a b c d e g h i j k; do
        [ "$a" = "fixture" ] && continue
        [ -z "$a" ] && continue
        if [ "$d" = "$g" ]; then sh=true; else sh=false; fi
        echo "| \`$a\` | $b | $c | $e | $sh | $h | $i | $j | $k |"
    done < "$TSV"
} > "$CAMPAIGN/MATRIX.md"

# --- counts.txt -------------------------------------------------------------
{
    echo "fixtures $fixtures_n"
    echo "normal $normal_n"
    echo "exact_ok $exact_ok"
    echo "exact_fail $exact_fail"
    echo "surface_fail $surface_fail"
    echo "native_object_ok $obj_ok"
    echo "native_object_n $obj_n"
    echo "native_dimension_ok $dim_ok"
    echo "native_dimension_n $dim_n"
    echo "native_attribute_ok $attr_ok"
    echo "native_attribute_n $attr_n"
    echo "structural_declines_ok $structural_ok"
    echo "structural_declines $structural_n"
    echo "out_of_subset_declines_ok $unsupported_ok"
    echo "out_of_subset_declines $unsupported_n"
    echo "opaque_controls_ok $opaque_ok"
    echo "opaque_controls $opaque_n"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.34.1 — scientific-container (NetCDF classic / HDF5-subset) surface + exact closure",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "git_dirty": "$(git status --porcelain | tr '\n' ' ' | sed 's/"/\\"/g')",
  "arch": "$(uname -m)",
  "service": "dev",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "bin": "$BIN",
  "bin_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "profile": "debug",
  "fixtures_tool": "examples/hdf_fixtures.rs (dependency-free NetCDF/HDF5 writer; no python3, no h5py, no h5dump)",
  "env_affecting_semantics": {"LC_ALL": "C", "CARGO_PROFILE_DEV_DEBUG": "$CARGO_PROFILE_DEV_DEBUG"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
if [ "$exact_fail" -ne 0 ] || [ "$surface_fail" -ne 0 ] \
    || [ "$exact_ok" -ne "$normal_n" ] \
    || [ "$obj_ok" -ne "$obj_n" ] || [ "$obj_n" -ne "$normal_n" ] \
    || [ "$dim_n" -ne 2 ] || [ "$dim_ok" -ne "$dim_n" ] \
    || [ "$attr_ok" -ne "$attr_n" ] \
    || [ "$structural_ok" -ne "$structural_n" ] \
    || [ "$unsupported_ok" -ne "$unsupported_n" ] \
    || [ "$opaque_ok" -ne "$opaque_n" ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.34.1 — scientific-container (NetCDF classic / HDF5-subset) surface + exact closure",
  "utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "measured_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "tree_state": "$(git status --porcelain | tr '\n' ' ' | sed 's/"/\\"/g')",
  "service": "dev",
  "profile": "debug",
  "bin": "$BIN",
  "bin_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "fixtures": "$NORMAL $STRUCTURAL $UNSUPPORTED $OPAQUE",
  "fixture_tool": "examples/hdf_fixtures.rs",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion",
  "netcdf_supported": "NetCDF classic CDF-1 and CDF-2 in full: the magic, numrecs (incl. the STREAMING sentinel), the dimension table (incl. the unlimited/record dimension), the global attribute list, the variable list (name, dimensions, per-variable attributes, type, vsize, begin), and the record/array data layout — with exact spans",
  "hdf5_supported": "a documented bounded subset: superblock v0/v1 (offset/length size 2/4/8), the root group and the symbol-table link traversal to nested groups and contiguous datasets, v1 object headers with their message list (incl. object-header continuations), the fixed-point / floating-point / string datatype classes, the simple/scalar dataspace, the contiguous data layout, and v1 attribute messages",
  "hdf5_declined_typed": "superblock v2/v3 (v2 link machinery), v2/v3 object headers (SWMR/compact/checksummed), compact/chunked/virtual data layouts, filtered datasets (any filter-pipeline message), external data files, and every other datatype class (bitfield/opaque/compound/reference/enum/vlen/array); unknown object-header message types also decline typed",
  "observations": ["metadata", "text", "search-match", "native hdf-dialect", "native hdf-object", "native hdf-dimension", "native hdf-attribute", "native hdf-find"],
  "declines": "a filtered dataset and a v2 object header decline typed (rc 6); a malformed NetCDF header and a bad NetCDF tag decline structurally (rc 50); an out-of-range native index declines typed (rc 6); an hdf selector on an Opaque control declines typed (rc 6); never a panic",
  "detection_boundary": "content-only, NEVER a file name: NetCDF = the 4-byte 'CDF\\\\x01'/'CDF\\\\x02' magic at offset 0; HDF5 = the 8-byte signature at offset 0 OR at 512 (the user-block form), with a superblock version in 0..=3, offset/length sizes in {2,4,8}, and a superblock that fits inside the source",
  "cannot_distinguish": "HDF5 detection is a signature + superblock-header validation, not a full-file test; a random '\\\\x89HDF'-prefixed blob with a plausible superblock header is admitted and its model may then decline typed. NetCDF detection is the 4-byte magic only. The adapter does not decode/convert any dataset values (contiguous spans are recorded, not read), does not decompress filtered data, does not resolve external/virtual storage, does not traverse the v2 link machinery, and does not interpret NetCDF-4 (HDF5-backed) semantics beyond the HDF5 subset",
  "precedence": "the scientific container is inserted with the strong-magic binaries, after the office ZIP family and the generic archive container and before Parquet and every weak, no-magic-byte heuristic",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 50 invalid-hdf-structure",
  "adr_0060": "the HdfModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase21-34-1-hdf-court.sh
# inside the court:
#   cargo build --locked --all-features
#   cargo run --locked --all-features --example hdf_fixtures -- CORPUS
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--hdf-dialect|--hdf-object N|--hdf-dimension N|--hdf-attribute N|--hdf-find PAT|--text PAT) --kind metadata|text|exact
#   rm source F.voldoc
#   target/debug/vole-document materialize --store STORE --field HEX --exact --output OUT ; cmp OUT source
# Full gate (dev service):
#   cargo fmt --all --check
#   cargo clippy --all-targets --all-features -- -D warnings
#   cargo test --locked --all-features
#   cargo test --locked
#   MSRV: docker compose run --rm --no-TTY msrv cargo build --locked --all-features
EOF

# --- SUMMARY.md -------------------------------------------------------------
{
    echo "# Phase 21.34.1 — scientific-container (NetCDF classic / HDF5-subset) court"
    echo
    echo "**Question.** Does a NetCDF classic (CDF-1/CDF-2) or HDF5-subset file close"
    echo "exactly and expose a bounded, span-preserving model (NetCDF: the dimension table"
    echo "incl. the unlimited dimension, the global attributes, and the variable list; HDF5:"
    echo "the group/dataset tree with the dataset datatype / simple dataspace / contiguous"
    echo "layout and the root-group attribute; a recorded \`netcdf\`/\`hdf5\` dialect) on top of"
    echo "the whole-source exact leaf — while declining out-of-subset HDF5 constructs,"
    echo "malformed headers, and non-magic inputs typed?"
    echo
    echo "**Method.** Each fixture is generated by the dependency-free"
    echo "\`examples/hdf_fixtures.rs\` writer, ingested via \`field-build\`, observed, then —"
    echo "after the **source file and the standalone descriptor are deleted** —"
    echo "rematerialized exactly in a fresh process and compared with \`length + SHA-256 +"
    echo "cmp\`. The out-of-subset, structural, and Opaque controls pin the boundaries. The"
    echo "court runs in the pinned \`dev\` service using only POSIX \`sh\`, coreutils, git, and"
    echo "the shipped binary (no python3, no h5py, no h5dump, no netCDF)."
    echo
    echo "## Exactness (after source + descriptor deletion)"
    echo
    tail -n +3 "$CAMPAIGN/MATRIX.md"
    echo
    echo "## Counts"
    echo
    echo '```'
    cat "$CAMPAIGN/counts.txt"
    echo '```'
    echo
    echo "## Per-fixture observation files (raw/)"
    echo
    ls "$RAW" | sed 's/^/- `/; s/$/`/'
    echo
    echo "## Scope (honest)"
    echo
    echo "- **Shipped here:** content-only, conservative detection (NetCDF = the"
    echo "  \`CDF\\x01\`/\`CDF\\x02\` magic; HDF5 = the signature at 0 or 512 with a validated"
    echo "  superblock header); the bounded, span-preserving NetCDF-classic model; the"
    echo "  documented HDF5 subset; native \`hdf-dialect\`/\`hdf-object\`/\`hdf-dimension\`/"
    echo "  \`hdf-attribute\`/\`hdf-find\`; common \`metadata\`/\`text\`/\`search-match\`; a recorded"
    echo "  dialect."
    echo "- **Declines:** superblock v2/v3, v2/v3 object headers, compact/chunked/virtual"
    echo "  layouts, filtered datasets, external data files, and every datatype class other"
    echo "  than fixed-point/float/string (rc 6); malformed headers (rc 50); a bad/truncated"
    echo "  input stays \`Opaque\` rather than being claimed."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the model is"
    echo "  derived and never on the exactness path (ADR-0060: the model node depends on the"
    echo "  \`sha256(source)\` root)."
    echo "- **Not claimed here:** dataset value decoding, filtered-data decompression, the"
    echo "  HDF5 v2 link machinery, and the economic court (separate script)."
    echo "- **Never run on the host:** every command above ran in the pinned \`dev\` container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" != "PASS" ]; then
    echo "PHASE 21.34.1 HDF COURT: FAIL — campaign $CAMPAIGN (exact=$exact_ok/$normal_n surface_fail=$surface_fail obj=$obj_ok/$obj_n dim=$dim_ok/$dim_n attr=$attr_ok/$attr_n structural=$structural_ok/$structural_n unsupported=$unsupported_ok/$unsupported_n opaque=$opaque_ok/$opaque_n)" >&2
    exit 1
fi
echo "PHASE 21.34.1 HDF COURT: PASS — campaign $CAMPAIGN"
