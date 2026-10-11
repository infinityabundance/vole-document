#!/bin/sh
# Phase 21.22.1 — GeoJSON (RFC 7946, spatial Wave-2 format; JSON physical bytes +
# a bounded semantic sub-detection) surface + exact closure.
#
# Pre-registered (Phase-21 subphase 21.22.1). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture (including the JSON/Opaque controls),
#       `materialize(field) == source` (length + SHA-256 + cmp) after the source
#       file AND the standalone descriptor are deleted, in a fresh process. The
#       court FAILS unless this is 100% on every fixture.
#   H2 (surface + preserved representation) — the representation-preserving model is
#       exposed: the exact `"type"` token; `coordinates` nesting with each number's
#       exact span and literal spelling (never reparsed); `properties` order and
#       duplicate keys; `id`/`bbox`/`geometry`/`features` order; and foreign members.
#       Common metadata/text/search-match and native geojson-type/geojson-feature/
#       geojson-geometry/geojson-coordinates/geojson-property/geojson-find answer.
#   H3 (honest declines / boundaries) — an out-of-range feature/geometry, an unknown
#       property, and a GeometryCollection's coordinates decline typed (rc 6); a
#       malformed `--geojson-property` argument is a usage error (rc 2). The controls
#       pin the detection boundary: a plain JSON document, a JSON document with an
#       unrelated `"type"` string, and a GeoJSON type name with no consistent shape
#       all stay `Json`; prose stays `Opaque`. A GeoJSON selector on a Json/Opaque
#       field declines typed (rc 6).
#
# Runs in the pinned `dev` service (Rust toolchain + git + coreutils; deliberately
# NO python3 and NO jq, so the court depends only on the shipped binary, GNU
# `/usr/bin/printf` for the fixtures, and POSIX shell):
#   docker compose run --rm --no-TTY dev sh tools/phase21-22-1-geojson-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="2026-10-10"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-22-1-geojson-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-22
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== generate fixtures (POSIX printf only) ==="
mk() { /usr/bin/printf '%b' "$2" > "$1"; }

# --- NORMAL: detected as GeoJSON, full surface exercised ---------------------
# A FeatureCollection with a bbox, two features (a Point with a duplicated
# `properties` key and a foreign `vendor` member, and a Polygon with null
# properties), and a foreign root member `title`.
mk "$WORK/corpus/fc.geojson" \
'{"type":"FeatureCollection","bbox":[100.0,0.0,105.0,1.0],"features":[{"type":"Feature","id":1,"geometry":{"type":"Point","coordinates":[1.25,-2.5e1]},"properties":{"name":"A","name":"B","n":1e3},"vendor":"x"},{"type":"Feature","id":2,"geometry":{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,0]]]},"properties":null}],"title":"kept"}'
# A bare Point geometry.
mk "$WORK/corpus/point.geojson" '{"type":"Point","coordinates":[30.0,10.0]}'
# A LineString with preserved numeric spelling.
mk "$WORK/corpus/line.geojson" \
'{"type":"LineString","coordinates":[[30.0,10.0],[10.0,30.0],[40.0,40.0]]}'
# A GeometryCollection.
mk "$WORK/corpus/gc.geojson" \
'{"type":"GeometryCollection","geometries":[{"type":"Point","coordinates":[0,0]},{"type":"LineString","coordinates":[[0,0],[1,1]]}]}'

# --- CONTROLS ---------------------------------------------------------------
# Plain JSON: not GeoJSON → stays `Json`.
mk "$WORK/corpus/plain.json" '{"a":1,"b":[2,3]}'
# A JSON doc with an unrelated `"type"` string → stays `Json`.
mk "$WORK/corpus/unrelated.json" '{"type":"object","properties":{}}'
# A GeoJSON type name with no consistent shape → stays `Json`.
mk "$WORK/corpus/shapeless.json" '{"type":"Point"}'
# Plain prose: never GeoJSON → stays `Opaque`.
mk "$WORK/corpus/prose.txt" 'The quick brown fox.\nPlain prose, not GeoJSON.\n'

NORMAL="fc.geojson point.geojson line.geojson gc.geojson"
JSONCTL="plain.json unrelated.json shapeless.json"
OPAQUE="prose.txt"
ALL="$NORMAL $JSONCTL $OPAQUE"

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
decline_feature_ok=0
decline_geometry_ok=0
decline_property_ok=0
decline_gc_coords_ok=0
jsonctl_ok=0
opaque_ok=0
opaque_n=0
normal_n=0
usage_ok=0

for f in $ALL; do
    # The reference copy is never deleted; the corpus copy is ingested and then
    # removed (so the identity comparison is against retained bytes).
    src="$WORK/ref/$f"
    cp "$WORK/corpus/$f" "$src"
    src_len="$(wc -c < "$src" | tr -d ' ')"
    src_sha="$(sha256sum "$src" | cut -d' ' -f1)"

    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    field="$(json_str "$build_json" field)"
    fmt="$(json_str "$build_json" format)"

    decline_rc=-1
    ctl_rc=-1

    if is_in "$f" "$NORMAL"; then
        normal_n=$((normal_n + 1))
        [ "$fmt" = "geojson" ] || { echo "FINDING: $f not detected as geojson (fmt=$fmt)" >&2; surface_fail=$((surface_fail + 1)); }

        # H2: common metadata / text / search-match.
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --text "Feature" --kind text \
            > "$RAW/$f.search.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"format":"geojson"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata missing format" >&2; surface_fail=$((surface_fail + 1)); }

        # H2: native geojson-type.
        "$BIN" observe --store "$WORK/store" --field "$field" --geojson-type --kind text \
            > "$RAW/$f.type.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --geojson-type --kind exact \
            > "$RAW/$f.type.bin" 2>&1 || surface_fail=$((surface_fail + 1))

        # H2: native geojson-find (reuses the JSON match vocabulary).
        "$BIN" observe --store "$WORK/store" --field "$field" --geojson-find name --kind text \
            > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))

        case "$f" in
            fc.geojson)
                grep -q '"type":"FeatureCollection"' "$RAW/$f.metadata.json" || { echo "FINDING: fc type not reported" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"features":2' "$RAW/$f.metadata.json" || { echo "FINDING: fc feature count not reported" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"geometries":2' "$RAW/$f.metadata.json" || { echo "FINDING: fc geometry count not reported" >&2; surface_fail=$((surface_fail + 1)); }
                [ "$(json_str "$(cat "$RAW/$f.type.json")" text)" = "FeatureCollection" ] || { echo "FINDING: fc type text != FeatureCollection" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"value_hex":"2246656174757265436f6c6c656374696f6e22"' "$RAW/$f.type.bin" || { echo "FINDING: fc type exact != \"FeatureCollection\"" >&2; surface_fail=$((surface_fail + 1)); }
                # A feature descriptor reports its type, geometry, and foreign member.
                "$BIN" observe --store "$WORK/store" --field "$field" --geojson-feature 0 --kind metadata \
                    > "$RAW/$f.feature0.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"type":"Feature"' "$RAW/$f.feature0.json" || { echo "FINDING: fc feature missing type" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"vendor"' "$RAW/$f.feature0.json" || { echo "FINDING: fc foreign member dropped" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"type":"Point"' "$RAW/$f.feature0.json" || { echo "FINDING: fc feature geometry type missing" >&2; surface_fail=$((surface_fail + 1)); }
                # Coordinates preserve spelling (never reparsed).
                "$BIN" observe --store "$WORK/store" --field "$field" --geojson-coordinates 0 --kind metadata \
                    > "$RAW/$f.coords0.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"count":2' "$RAW/$f.coords0.json" || { echo "FINDING: fc coords count" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"spelling":"1.25"' "$RAW/$f.coords0.json" || { echo "FINDING: fc coords spelling 1.25" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"spelling":"-2.5e1"' "$RAW/$f.coords0.json" || { echo "FINDING: fc coords spelling -2.5e1" >&2; surface_fail=$((surface_fail + 1)); }
                # A property by name (Text decodes; duplicate keys both reported by find).
                "$BIN" observe --store "$WORK/store" --field "$field" --geojson-property 0:name --kind text \
                    > "$RAW/$f.prop.json" 2>&1 || surface_fail=$((surface_fail + 1))
                [ "$(json_str "$(cat "$RAW/$f.prop.json")" text)" = "A" ] || { echo "FINDING: fc property name text != A" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"role":"key"' "$RAW/$f.find.json" || { echo "FINDING: fc find missing key role" >&2; surface_fail=$((surface_fail + 1)); }
                # The second geometry is a Polygon (document order preserved).
                "$BIN" observe --store "$WORK/store" --field "$field" --geojson-geometry 1 --kind metadata \
                    > "$RAW/$f.geom1.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"type":"Polygon"' "$RAW/$f.geom1.json" || { echo "FINDING: fc geometry order/type" >&2; surface_fail=$((surface_fail + 1)); } ;;
            point.geojson)
                [ "$(json_str "$(cat "$RAW/$f.type.json")" text)" = "Point" ] || { echo "FINDING: point type text" >&2; surface_fail=$((surface_fail + 1)); } ;;
            line.geojson)
                "$BIN" observe --store "$WORK/store" --field "$field" --geojson-coordinates 0 --kind metadata \
                    > "$RAW/$f.coords0.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"spelling":"30.0"' "$RAW/$f.coords0.json" || { echo "FINDING: line spelling 30.0" >&2; surface_fail=$((surface_fail + 1)); } ;;
            gc.geojson)
                [ "$(json_str "$(cat "$RAW/$f.type.json")" text)" = "GeometryCollection" ] || { echo "FINDING: gc type text" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --geojson-geometry 0 --kind metadata \
                    > "$RAW/$f.geom0.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q 'GeometryCollection' "$RAW/$f.geom0.json" || { echo "FINDING: gc geometry type" >&2; surface_fail=$((surface_fail + 1)); }
                # A GeometryCollection has no coordinates → typed decline (rc 6).
                set +e
                "$BIN" observe --store "$WORK/store" --field "$field" --geojson-coordinates 0 --kind metadata \
                    > "$RAW/$f.coords-decline.json" 2>&1
                gc_coords_rc=$?
                set -e
                [ "$gc_coords_rc" -eq 6 ] && decline_gc_coords_ok=$((decline_gc_coords_ok + 1)) || { echo "FINDING: gc coordinates rc=$gc_coords_rc (want 6)" >&2; surface_fail=$((surface_fail + 1)); } ;;
        esac

        # H3: out-of-range feature/geometry and unknown property decline typed (rc 6).
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --geojson-feature 999 --kind metadata \
            > /dev/null 2>&1
        decline_rc=$?
        set -e
        [ "$decline_rc" -eq 6 ] && decline_feature_ok=$((decline_feature_ok + 1)) || { echo "FINDING: $f out-of-range feature rc=$decline_rc (want 6)" >&2; }
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --geojson-geometry 999 --kind metadata \
            > /dev/null 2>&1
        g_rc=$?
        set -e
        [ "$g_rc" -eq 6 ] && decline_geometry_ok=$((decline_geometry_ok + 1)) || { echo "FINDING: $f out-of-range geometry rc=$g_rc (want 6)" >&2; }
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --geojson-property 0:nope --kind text \
            > /dev/null 2>&1
        p_rc=$?
        set -e
        [ "$p_rc" -eq 6 ] && decline_property_ok=$((decline_property_ok + 1)) || { echo "FINDING: $f unknown property rc=$p_rc (want 6)" >&2; }

    elif is_in "$f" "$JSONCTL"; then
        # The boundary: a plain/unrelated/shapeless JSON document stays `Json`.
        [ "$fmt" = "json" ] || { echo "FINDING: control $f detected as $fmt (want json)" >&2; surface_fail=$((surface_fail + 1)); }
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --geojson-type --kind text \
            > "$RAW/$f.geojson-decline.json" 2>&1
        ctl_rc=$?
        set -e
        [ "$ctl_rc" -eq 6 ] && jsonctl_ok=$((jsonctl_ok + 1)) || { echo "FINDING: JSON control $f geojson selector rc=$ctl_rc (want 6)" >&2; }

    elif is_in "$f" "$OPAQUE"; then
        [ "$fmt" = "opaque" ] || { echo "FINDING: control $f detected as $fmt (want opaque)" >&2; surface_fail=$((surface_fail + 1)); }
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1
        ctl_rc=$?
        set -e
        [ "$ctl_rc" -eq 6 ] && opaque_ok=$((opaque_ok + 1)) || { echo "FINDING: opaque control $f metadata rc=$ctl_rc (want 6)" >&2; }
        opaque_n=$((opaque_n + 1))
    fi

    # A malformed `--geojson-property` argument is a usage error (rc 2) — checked once.
    if [ "$f" = "fc.geojson" ]; then
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --geojson-property abc --kind metadata \
            > /dev/null 2>&1
        u_rc=$?
        set -e
        [ "$u_rc" -eq 2 ] && usage_ok=$((usage_ok + 1)) || echo "FINDING: malformed --geojson-property rc=$u_rc (want 2)" >&2
    fi

    # H1: delete the source AND the standalone descriptor, then rematerialize
    # exactly in a fresh process and compare (length + SHA-256 + cmp).
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

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$f" "$fmt" "$src_len" "$src_sha" "$out_len" "$out_sha" "$cmp_ok" "$exact" \
        "$decline_rc" "$ctl_rc" >> "$TSV"
done

echo "=== exactness after source + descriptor deletion ==="
cat "$TSV"

fixtures_n="$(printf '%s\n' $ALL | wc -w | tr -d ' ')"

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 21.22.1 GeoJSON court — matrix"
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
    echo "geojson_fixtures $normal_n"
    echo "exact_ok $exact_ok"
    echo "exact_fail $exact_fail"
    echo "surface_fail $surface_fail"
    echo "typed_decline_features_ok $decline_feature_ok"
    echo "typed_decline_geometries_ok $decline_geometry_ok"
    echo "typed_decline_properties_ok $decline_property_ok"
    echo "typed_decline_gc_coordinates_ok $decline_gc_coords_ok"
    echo "json_controls_ok $jsonctl_ok"
    echo "usage_ok $usage_ok"
    echo "opaque_controls_ok $opaque_ok"
    echo "opaque_controls $opaque_n"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.22.1 — GeoJSON (RFC 7946) surface + exact closure",
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
  "fixtures_tool": "POSIX shell + GNU /usr/bin/printf (no python3, no jq); generated in-court, never committed",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
if [ "$exact_fail" -ne 0 ] || [ "$surface_fail" -ne 0 ] \
    || [ "$decline_feature_ok" -ne "$normal_n" ] || [ "$decline_geometry_ok" -ne "$normal_n" ] \
    || [ "$decline_property_ok" -ne "$normal_n" ] || [ "$decline_gc_coords_ok" -ne 1 ] \
    || [ "$jsonctl_ok" -ne 3 ] || [ "$usage_ok" -ne 1 ] || [ "$opaque_ok" -ne "$opaque_n" ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.22.1 — GeoJSON (RFC 7946) surface + exact closure",
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
  "fixtures": "$ALL",
  "fixture_tool": "POSIX shell + GNU /usr/bin/printf (in-court; not committed)",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion, for every fixture including the JSON/Opaque controls",
  "observations": ["metadata", "text", "search-match", "native geojson-type", "native geojson-feature", "native geojson-geometry", "native geojson-coordinates", "native geojson-property", "native geojson-find"],
  "surface": "the exact type token; coordinates nesting with each number's exact span and literal spelling (never reparsed); properties order and duplicate keys; id/bbox/geometry/features order; foreign members reported, never dropped",
  "declines": "an out-of-range feature/geometry, an unknown property, and a GeometryCollection's coordinates decline typed (rc 6); a malformed --geojson-property argument is a usage error (rc 2); the plain-JSON / unrelated-type / shapeless-type controls stay Json and a native GeoJSON selector on each declines typed (rc 6); prose stays Opaque and a common observation declines typed (rc 6), never a panic",
  "detection_boundary": "GeoJSON's physical bytes are JSON, so detection is a bounded semantic sub-test run before the generic JSON detector: the source must parse as exactly one JSON value whose root is an object with a string \"type\" member equal to one of the nine RFC 7946 type names AND whose shape is consistent (a geometry has an array coordinates/geometries, a Feature has geometry+properties, a FeatureCollection has an array features). A plain JSON document, a JSON document whose \"type\" is an unrelated string, and a GeoJSON type name with no consistent shape all stay Json; prose stays Opaque. A structurally-shaped-but-numerically-invalid GeoJSON (positions not validated against the RFC 7946 positional grammar) is claimed and preserved verbatim.",
  "precedence": "after the strong magic-byte binaries (PDF/ZIP/Parquet/Arrow); immediately before the generic JSON detector; before JSON5/JSONL/YAML/TOML/CSV/Markdown/XML/HTML and everything else",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 36 invalid-geojson-structure",
  "adr_0060": "the GeojsonModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase21-22-1-geojson-court.sh
# inside the court:
#   cargo build --locked --all-features
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--text PAT|--geojson-type|--geojson-feature N|--geojson-geometry N|--geojson-coordinates N|--geojson-property N:NAME|--geojson-find PAT) --kind metadata|text|structure|exact
#   rm source F.voldoc
#   target/debug/vole-document materialize --store STORE --field HEX --exact --output OUT ; cmp OUT source
# Full gate (dev service):
#   cargo fmt --all --check
#   cargo clippy --all-targets --all-features -- -D warnings
#   cargo test --locked --all-features
#   cargo test --locked
#   cargo test --locked --no-default-features
#   MSRV: docker compose run --rm --no-TTY msrv cargo build --locked --all-features
EOF

# --- SUMMARY.md -------------------------------------------------------------
{
    echo "# Phase 21.22.1 — GeoJSON (RFC 7946) court"
    echo
    echo "**Question.** Does the GeoJSON adapter close exactly and expose a"
    echo "representation-preserving model (the exact type token; coordinates nesting"
    echo "with each number's exact span/literal spelling; properties order and"
    echo "duplicate keys; id/bbox/geometry/features order; foreign members) on top of"
    echo "the whole-source exact leaf — while keeping the bounded semantic"
    echo "sub-detection boundary (before the generic JSON detector) and declining"
    echo "malformed/ambiguous inputs typed?"
    echo
    echo "**Method.** Each self-authored fixture is generated in-court with GNU"
    echo "\`/usr/bin/printf\`, ingested via \`field-build\`, observed, then — after the"
    echo "**source file and the standalone descriptor are deleted** — rematerialized"
    echo "exactly in a fresh process and compared with \`length + SHA-256 + cmp\`. An"
    echo "out-of-range feature/geometry, an unknown property, and a GeometryCollection's"
    echo "coordinates are required to decline typed; the plain-JSON / unrelated-type /"
    echo "shapeless-type controls and prose pin the detection boundaries. The court runs"
    echo "in the pinned \`dev\` service using only POSIX \`sh\`, coreutils, git, and the"
    echo "shipped binary (no python3, no jq)."
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
    echo "- **Shipped here:** byte-based bounded semantic GeoJSON detection; one bounded"
    echo "  GeoJSON model **reusing the shared JSON parser** (never a second JSON"
    echo "  parser); native \`geojson-type\`/\`geojson-feature\`/\`geojson-geometry\`/"
    echo "  \`geojson-coordinates\`/\`geojson-property\`/\`geojson-find\`; common"
    echo "  \`metadata\`/\`text\`/\`search-match\`."
    echo "- **Precedence:** after the strong magic-byte binaries (PDF/ZIP/Parquet/"
    echo "  Arrow); immediately before the generic JSON detector. A GeoJSON document is"
    echo "  a more specific claim than a bare JSON value, so it is tried first."
    echo "- **Detection boundary:** GeoJSON's physical bytes are JSON. A GeoJSON document"
    echo "  is claimed only when the root object's string \`\"type\"\` is one of the nine"
    echo "  RFC 7946 names **and** the shape is consistent (a geometry has an array"
    echo "  \`coordinates\`/\`geometries\`; a \`Feature\` has \`geometry\`+\`properties\`; a"
    echo "  \`FeatureCollection\` has an array \`features\`). A plain JSON document, a"
    echo "  JSON document whose \`\"type\"\` is unrelated, and a GeoJSON type name with no"
    echo "  consistent shape all stay \`Json\`; prose stays \`Opaque\`."
    echo "- **Recorded negative (not distinguished):** a structure that is shaped like"
    echo "  GeoJSON but is numerically invalid under RFC 7946 (a position that is not a"
    echo "  \`[lon, lat, (alt)]\` array of numbers, an unclosed ring, a wrong coordinate"
    echo "  arity) is still claimed and preserved **verbatim** — the adapter does not"
    echo "  validate the positional grammar, so it neither normalizes nor rejects such a"
    echo "  document."
    echo "- **Declines:** an out-of-range feature/geometry, an unknown property, a"
    echo "  GeometryCollection's coordinates, a malformed \`--geojson-property\` argument,"
    echo "  a cap breach, and a non-GeoJSON source are typed"
    echo "  (\`InvalidGeojsonStructure\` rc 36, unsupported-feature rc 6, resource-limit"
    echo "  rc 8, or usage rc 2); such input stays \`Json\`/\`Opaque\` when detection"
    echo "  declines."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the GeoJSON"
    echo "  model is derived (\`Q_gen\`) and never on the exactness path (ADR-0060: the"
    echo "  model node depends on the \`sha256(source)\` root)."
    echo "- **Not claimed here:** coordinate validation, CRS handling (RFC 7946 removed"
    echo "  CRS), the economic court (separate script)."
    echo "- **Never run on the host:** every command above ran in the pinned \`dev\`"
    echo "  container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" != "PASS" ]; then
    echo "PHASE 21.22.1 GEOJSON COURT: FAIL — campaign $CAMPAIGN (exact_fail=$exact_fail surface_fail=$surface_fail df=$decline_feature_ok dg=$decline_geometry_ok dp=$decline_property_ok dgc=$decline_gc_coords_ok jsonctl=$jsonctl_ok usage=$usage_ok opaque=$opaque_ok/$opaque_n)" >&2
    exit 1
fi
echo "PHASE 21.22.1 GEOJSON COURT: PASS — campaign $CAMPAIGN"
