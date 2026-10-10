#!/bin/sh
# Phase 21.23.1 — KML 2.2 / GPX 1.1 (geospatial Wave-2 format; XML physical bytes +
# a bounded semantic sub-detection) surface + exact closure.
#
# Pre-registered (Phase-21 subphase 21.23.1). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture (including the XML/Opaque controls),
#       `materialize(field) == source` (length + SHA-256 + cmp) after the source
#       file AND the standalone descriptor are deleted, in a fresh process. The
#       court FAILS unless this is 100% on every fixture.
#   H2 (surface + preserved representation) — the representation-preserving model is
#       exposed: the recorded dialect (kml/gpx); exact element/attribute spans;
#       element order; attribute spelling (KML geometry `<coordinates>`, GPX
#       `<trkpt lat=… lon=…>`); the namespace declaration. Common metadata/text/
#       search-match and native gis-root/gis-field/gis-record/gis-record-field/
#       gis-point/gis-find answer.
#   H3 (honest declines / boundaries) — an out-of-range record/point, an unknown
#       field, and a KML record's non-direct `coordinates` decline typed (rc 6); a
#       malformed `--gis-record-field` argument is a usage error (rc 2). The controls
#       pin the detection boundary: a plain XML document and a shaped-but-invalid
#       `<kml>`/`<gpx>` (no namespace, or no structural child) stay `Xml`; prose
#       stays `Opaque`. A GIS selector on an Xml/Opaque field declines typed (rc 6).
#
# Runs in the pinned `dev` service (Rust toolchain + git + coreutils; deliberately
# NO python3 and NO jq, so the court depends only on the shipped binary, GNU
# `/usr/bin/printf` for the fixtures, and POSIX shell):
#   docker compose run --rm --no-TTY dev sh tools/phase21-23-1-gis-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="2026-10-10"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-23-1-gis-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-23
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== generate fixtures (POSIX printf only) ==="
mk() { /usr/bin/printf '%b' "$2" > "$1"; }

# --- NORMAL: detected as GIS, full surface exercised ------------------------
# An OGC KML 2.2 document: the KML namespace, a root `<name>`, a `<Document>` with a
# `<name>`, two `<Placemark>` features (a `<Point>` and a `<LineString>`, each with
# a `<coordinates>`), and a `<styleUrl>`.
mk "$WORK/corpus/kml.kml" \
'<kml xmlns="http://www.opengis.net/kml/2.2">\n<name>RootDoc</name>\n<Document>\n<name>Example</name>\n<Placemark>\n<name>First</name>\n<description>one</description>\n<styleUrl>#s1</styleUrl>\n<Point><coordinates>1.25,-2.5e1,0</coordinates></Point>\n</Placemark>\n<Placemark>\n<name>Second</name>\n<LineString><coordinates>0,0 1,1</coordinates></LineString>\n</Placemark>\n</Document>\n</kml>\n'
# A GPX 1.1 document: the GPX namespace, `<metadata>`, a `<wpt>`, a `<rte>` with a
# `<rtept>`, and a `<trk>`/`<trkseg>`/`<trkpt>` (with `lat`/`lon` attributes and
# `<ele>`/`<time>` children).
mk "$WORK/corpus/gpx.gpx" \
'<gpx xmlns="http://www.topografix.com/GPX/1/1" version="1.1" creator="test">\n<metadata><name>Example</name></metadata>\n<wpt lat="1.0" lon="2.0"><ele>10</ele><time>2024-01-01T00:00:00Z</time><name>W</name></wpt>\n<rte><name>R</name><rtept lat="3.0" lon="4.0"><ele>20</ele></rtept></rte>\n<trk><name>T</name><trkseg><trkpt lat="5.0" lon="6.0"><ele>30</ele><time>2024-01-02T00:00:00Z</time></trkpt></trkseg></trk>\n</gpx>\n'

# --- CONTROLS ---------------------------------------------------------------
# Plain XML: not GIS → stays `Xml`.
mk "$WORK/corpus/plain.xml" '<note><to>Tove</to><from>Jani</from></note>'
# A `<kml>` with no KML namespace → stays `Xml`.
mk "$WORK/corpus/no-ns.kml" '<kml><Document><Placemark/></Document></kml>'
# A `<kml>` in the KML namespace with no structural child → stays `Xml`.
mk "$WORK/corpus/no-child.kml" '<kml xmlns="http://www.opengis.net/kml/2.2"/>'
# A `<gpx>` with no GPX namespace → stays `Xml`.
mk "$WORK/corpus/no-ns.gpx" '<gpx><wpt lat="0" lon="0"/></gpx>'
# A `<gpx>` in the GPX namespace with no structural child → stays `Xml`.
mk "$WORK/corpus/no-child.gpx" '<gpx xmlns="http://www.topografix.com/GPX/1/1"/>'
# Plain prose: never GIS → stays `Opaque`.
mk "$WORK/corpus/prose.txt" 'The quick brown fox.\nPlain prose, not GIS.\n'

NORMAL="kml.kml gpx.gpx"
XMLCTL="plain.xml no-ns.kml no-child.kml no-ns.gpx no-child.gpx"
OPAQUE="prose.txt"
ALL="$NORMAL $XMLCTL $OPAQUE"

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
decline_record_ok=0
decline_point_ok=0
decline_field_ok=0
xmlctl_ok=0
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
        [ "$fmt" = "gis" ] || { echo "FINDING: $f not detected as gis (fmt=$fmt)" >&2; surface_fail=$((surface_fail + 1)); }

        # H2: common metadata / text / search-match.
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --text "First" --kind text \
            > "$RAW/$f.search.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"format":"gis"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata missing format" >&2; surface_fail=$((surface_fail + 1)); }
        grep -q '"dialect":"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata missing dialect" >&2; surface_fail=$((surface_fail + 1)); }

        # H2: native gis-root / gis-record / gis-point / gis-find.
        "$BIN" observe --store "$WORK/store" --field "$field" --gis-root --kind structure \
            > "$RAW/$f.root.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --gis-record 0 --kind structure \
            > "$RAW/$f.record0.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --gis-point 0 --kind structure \
            > "$RAW/$f.point0.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --gis-find First --kind text \
            > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))

        case "$f" in
            kml.kml)
                grep -q '"dialect":"kml"' "$RAW/$f.metadata.json" || { echo "FINDING: kml metadata dialect" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"dialect":"kml"' "$RAW/$f.root.json" || { echo "FINDING: kml root dialect" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"records":2' "$RAW/$f.root.json" || { echo "FINDING: kml record count" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"points":2' "$RAW/$f.root.json" || { echo "FINDING: kml point count" >&2; surface_fail=$((surface_fail + 1)); }
                # gis-field: a root-level scalar metadata field.
                "$BIN" observe --store "$WORK/store" --field "$field" --gis-field name --kind text \
                    > "$RAW/$f.field.json" 2>&1 || surface_fail=$((surface_fail + 1))
                [ "$(json_str "$(cat "$RAW/$f.field.json")" text)" = "RootDoc" ] || { echo "FINDING: kml root name != RootDoc" >&2; surface_fail=$((surface_fail + 1)); }
                # gis-record: a Placemark, in document order.
                grep -q '"kind":"Placemark"' "$RAW/$f.record0.json" || { echo "FINDING: kml record kind" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q 'First' "$RAW/$f.record0.json" || { echo "FINDING: kml record field" >&2; surface_fail=$((surface_fail + 1)); }
                # gis-record-field: Text decodes; ExactBytes is the whole element span.
                "$BIN" observe --store "$WORK/store" --field "$field" --gis-record-field 0:name --kind text \
                    > "$RAW/$f.recfield.json" 2>&1 || surface_fail=$((surface_fail + 1))
                [ "$(json_str "$(cat "$RAW/$f.recfield.json")" text)" = "First" ] || { echo "FINDING: kml record name != First" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --gis-record-field 0:name --kind exact \
                    > "$RAW/$f.recfield.bin" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"value_hex":"3c6e616d653e46697273743c2f6e616d653e"' "$RAW/$f.recfield.bin" || { echo "FINDING: kml record name exact != <name>First</name>" >&2; surface_fail=$((surface_fail + 1)); }
                # gis-point: the first geometry is a Point with preserved coordinates.
                grep -q '"kind":"Point"' "$RAW/$f.point0.json" || { echo "FINDING: kml point kind" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '1.25,-2.5e1,0' "$RAW/$f.point0.json" || { echo "FINDING: kml point coordinates" >&2; surface_fail=$((surface_fail + 1)); } ;;
            gpx.gpx)
                grep -q '"dialect":"gpx"' "$RAW/$f.metadata.json" || { echo "FINDING: gpx metadata dialect" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"dialect":"gpx"' "$RAW/$f.root.json" || { echo "FINDING: gpx root dialect" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"records":3' "$RAW/$f.root.json" || { echo "FINDING: gpx record count" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"points":3' "$RAW/$f.root.json" || { echo "FINDING: gpx point count" >&2; surface_fail=$((surface_fail + 1)); }
                # gis-record: the first record is a wpt.
                grep -q '"kind":"wpt"' "$RAW/$f.record0.json" || { echo "FINDING: gpx record kind" >&2; surface_fail=$((surface_fail + 1)); }
                # gis-point 2 is the trkpt, with preserved lat/lon attributes + ele/time.
                "$BIN" observe --store "$WORK/store" --field "$field" --gis-point 2 --kind structure \
                    > "$RAW/$f.point2.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"kind":"trkpt"' "$RAW/$f.point2.json" || { echo "FINDING: gpx trkpt kind" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"lat":"5.0"' "$RAW/$f.point2.json" || { echo "FINDING: gpx trkpt lat" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"lon":"6.0"' "$RAW/$f.point2.json" || { echo "FINDING: gpx trkpt lon" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '30' "$RAW/$f.point2.json" || { echo "FINDING: gpx trkpt ele" >&2; surface_fail=$((surface_fail + 1)); } ;;
        esac

        # H3: an out-of-range record/point and an unknown field decline typed (rc 6).
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --gis-record 999 --kind metadata \
            > /dev/null 2>&1
        decline_rc=$?
        set -e
        [ "$decline_rc" -eq 6 ] && decline_record_ok=$((decline_record_ok + 1)) || { echo "FINDING: $f out-of-range record rc=$decline_rc (want 6)" >&2; }
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --gis-point 999 --kind metadata \
            > /dev/null 2>&1
        p_rc=$?
        set -e
        [ "$p_rc" -eq 6 ] && decline_point_ok=$((decline_point_ok + 1)) || { echo "FINDING: $f out-of-range point rc=$p_rc (want 6)" >&2; }
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --gis-field nope --kind text \
            > /dev/null 2>&1
        f_rc=$?
        set -e
        [ "$f_rc" -eq 6 ] && decline_field_ok=$((decline_field_ok + 1)) || { echo "FINDING: $f unknown field rc=$f_rc (want 6)" >&2; }

    elif is_in "$f" "$XMLCTL"; then
        # The boundary: a plain/invalid XML document stays `Xml`.
        [ "$fmt" = "xml" ] || { echo "FINDING: control $f detected as $fmt (want xml)" >&2; surface_fail=$((surface_fail + 1)); }
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --gis-root --kind structure \
            > "$RAW/$f.gis-decline.json" 2>&1
        ctl_rc=$?
        set -e
        [ "$ctl_rc" -eq 6 ] && xmlctl_ok=$((xmlctl_ok + 1)) || { echo "FINDING: XML control $f gis selector rc=$ctl_rc (want 6)" >&2; }

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

    # A malformed `--gis-record-field` argument is a usage error (rc 2) — checked once.
    if [ "$f" = "kml.kml" ]; then
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --gis-record-field abc --kind metadata \
            > /dev/null 2>&1
        u_rc=$?
        set -e
        [ "$u_rc" -eq 2 ] && usage_ok=$((usage_ok + 1)) || echo "FINDING: malformed --gis-record-field rc=$u_rc (want 2)" >&2
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
xmlctl_n="$(printf '%s\n' $XMLCTL | wc -w | tr -d ' ')"

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 21.23.1 KML/GPX court — matrix"
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
    echo "gis_fixtures $normal_n"
    echo "exact_ok $exact_ok"
    echo "exact_fail $exact_fail"
    echo "surface_fail $surface_fail"
    echo "typed_decline_records_ok $decline_record_ok"
    echo "typed_decline_points_ok $decline_point_ok"
    echo "typed_decline_fields_ok $decline_field_ok"
    echo "xml_controls_ok $xmlctl_ok"
    echo "xml_controls $xmlctl_n"
    echo "usage_ok $usage_ok"
    echo "opaque_controls_ok $opaque_ok"
    echo "opaque_controls $opaque_n"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.23.1 — KML 2.2 / GPX 1.1 geospatial surface + exact closure",
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
    || [ "$decline_record_ok" -ne "$normal_n" ] || [ "$decline_point_ok" -ne "$normal_n" ] \
    || [ "$decline_field_ok" -ne "$normal_n" ] \
    || [ "$xmlctl_ok" -ne "$xmlctl_n" ] || [ "$usage_ok" -ne 1 ] || [ "$opaque_ok" -ne "$opaque_n" ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.23.1 — KML 2.2 / GPX 1.1 geospatial surface + exact closure",
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
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion, for every fixture including the XML/Opaque controls",
  "observations": ["metadata", "text", "search-match", "native gis-root", "native gis-field", "native gis-record", "native gis-record-field", "native gis-point", "native gis-find"],
  "surface": "the recorded dialect (kml/gpx); exact element/attribute spans; element order; attribute spelling (KML geometry coordinates, GPX trkpt lat/lon); the namespace declaration; Placemark/geometry order (KML) and record/point order (GPX)",
  "declines": "an out-of-range record/point and an unknown root field decline typed (rc 6); a malformed --gis-record-field argument is a usage error (rc 2); the plain-XML and shaped-but-invalid controls stay Xml and a native GIS selector on each declines typed (rc 6); prose stays Opaque and a common observation declines typed (rc 6), never a panic",
  "detection_boundary": "KML and GPX are XML, so detection is a bounded semantic sub-test run before the generic XML detector: a <kml> root whose namespace prefix is bound to http://www.opengis.net/kml/2.2 WITH a Document/Folder/Placemark child, or a <gpx> root bound to http://www.topografix.com/GPX/1/1 WITH a metadata/wpt/rte/trk child. A plain XML document, a shaped-but-invalid <kml>/<gpx> (no namespace, or no structural child), and prose stay Xml/Opaque. Older KML (2.0/2.1) and GPX 1.0 use different namespace URIs and are NOT claimed (they stay Xml); a non-default bound prefix (e.g. <k:kml xmlns:k=…>) IS recognized; a shaped-but-invalid coordinate grammar is preserved verbatim, not validated.",
  "precedence": "after the strong magic-byte binaries (PDF/ZIP/Parquet/Arrow); after the JSON family, the binary structured-tree family, config, and feed; immediately before the generic standalone-XML detector; before HTML and everything else",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 37 invalid-gis-structure",
  "adr_0060": "the GisModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase21-23-1-gis-court.sh
# inside the court:
#   cargo build --locked --all-features
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--text PAT|--gis-root|--gis-field NAME|--gis-record N|--gis-record-field N:NAME|--gis-point N|--gis-find PAT) --kind metadata|text|structure|exact
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
    echo "# Phase 21.23.1 — KML 2.2 / GPX 1.1 court"
    echo
    echo "**Question.** Does the KML/GPX adapter close exactly and expose a"
    echo "representation-preserving model (the recorded dialect; exact element/attribute"
    echo "spans; element order; attribute spelling — KML geometry \`<coordinates>\`, GPX"
    echo "\`<trkpt lat=… lon=…>\`; the namespace declaration) on top of the whole-source"
    echo "exact leaf — while keeping the bounded semantic sub-detection boundary (before"
    echo "the generic XML detector) and declining malformed/ambiguous inputs typed?"
    echo
    echo "**Method.** Each self-authored fixture is generated in-court with GNU"
    echo "\`/usr/bin/printf\`, ingested via \`field-build\`, observed, then — after the"
    echo "**source file and the standalone descriptor are deleted** — rematerialized"
    echo "exactly in a fresh process and compared with \`length + SHA-256 + cmp\`. An"
    echo "out-of-range record/point and an unknown root field are required to decline"
    echo "typed; the plain-XML and shaped-but-invalid controls and prose pin the detection"
    echo "boundaries. The court runs in the pinned \`dev\` service using only POSIX \`sh\`,"
    echo "coreutils, git, and the shipped binary (no python3, no jq)."
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
    echo "- **Shipped here:** byte-based bounded semantic KML/GPX detection; one bounded"
    echo "  GIS model **reusing the shared XML parser** (never a second XML parser); native"
    echo "  \`gis-root\`/\`gis-field\`/\`gis-record\`/\`gis-record-field\`/\`gis-point\`/"
    echo "  \`gis-find\`; common \`metadata\`/\`text\`/\`search-match\`."
    echo "- **Precedence:** after the strong magic-byte binaries (PDF/ZIP/Parquet/Arrow),"
    echo "  the JSON family, the binary structured-tree family, config, and feed;"
    echo "  immediately before the generic standalone-XML detector. A KML/GPX document is"
    echo "  a more specific claim than a bare XML tree, so it is tried first."
    echo "- **Detection boundary:** KML and GPX are XML. A document is claimed only when a"
    echo "  \`<kml>\` root's bound namespace prefix is the OGC KML 2.2 URI **and** it has a"
    echo "  \`Document\`/\`Folder\`/\`Placemark\` child, or a \`<gpx>\` root's bound namespace"
    echo "  prefix is the GPX 1.1 URI **and** it has a \`metadata\`/\`wpt\`/\`rte\`/\`trk\`"
    echo "  child. A plain XML document and a shaped-but-invalid \`<kml>\`/\`<gpx>\` stay"
    echo "  \`Xml\`; prose stays \`Opaque\`."
    echo "- **Recorded negatives (not distinguished):** older KML namespace versions"
    echo "  (2.0/2.1, and the pre-OGC Google Earth namespaces) and GPX 1.0 use different"
    echo "  namespace URIs, so they are **not** claimed and stay \`Xml\`; only the KML 2.2"
    echo "  and GPX 1.1 URIs are recognized. A non-default bound prefix (e.g."
    echo "  \`<k:kml xmlns:k=\"…/kml/2.2\">\`) **is** recognized. The KML \`<coordinates>\`"
    echo "  and GPX \`lat\`/\`lon\` positional grammars are **not** validated; a"
    echo "  numerically-invalid shape is claimed and preserved verbatim."
    echo "- **Declines:** an out-of-range record/point, an unknown root field, a malformed"
    echo "  \`--gis-record-field\` argument, a cap breach, and a non-geospatial source are"
    echo "  typed (\`InvalidGisStructure\` rc 37, unsupported-feature rc 6, resource-limit"
    echo "  rc 8, or usage rc 2); such input stays \`Xml\`/\`Opaque\` when detection"
    echo "  declines."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the GIS model"
    echo "  is derived (\`Q_gen\`) and never on the exactness path (ADR-0060: the model node"
    echo "  depends on the \`sha256(source)\` root)."
    echo "- **Not claimed here:** coordinate validation, CRS/projection handling, the"
    echo "  economic court (separate script)."
    echo "- **Never run on the host:** every command above ran in the pinned \`dev\`"
    echo "  container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" != "PASS" ]; then
    echo "PHASE 21.23.1 GIS COURT: FAIL — campaign $CAMPAIGN (exact_fail=$exact_fail surface_fail=$surface_fail dr=$decline_record_ok dp=$decline_point_ok df=$decline_field_ok xmlctl=$xmlctl_ok usage=$usage_ok opaque=$opaque_ok/$opaque_n)" >&2
    exit 1
fi
echo "PHASE 21.23.1 GIS COURT: PASS — campaign $CAMPAIGN"
