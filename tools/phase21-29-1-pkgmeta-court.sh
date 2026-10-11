#!/bin/sh
# Phase 21.29 — the package-metadata adapter court.
#
# Pre-registered (Phase-21 subphase 21.29). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture (including the JSON, TOML, and Opaque
#       controls), `materialize(field) == source` (length + SHA-256 + cmp) after the
#       source file AND the standalone descriptor are deleted, in a fresh process. The
#       court FAILS unless this is 100% on every fixture.
#   H2 (surface + preserved representation) — the representation-preserving
#       package-metadata model is exposed: the **recorded dialect** (npm_package /
#       cargo_manifest / pyproject_manifest / cargo_lock / npm_lock); every recorded
#       section (a named table / array-of-tables / array element in TOML, or an
#       object / array member in JSON) with its exact name span (when named), value
#       span, role, kind, and entry range; and every key/value entry with its exact key
#       and value spans. Member order, duplicate keys, numeric and string spelling, and
#       inline-table / array-valued dependency specs are preserved verbatim. Native
#       pkgmeta-section/pkgmeta-entry/pkgmeta-key/pkgmeta-find and common metadata/
#       text/search-match answer.
#   H3 (declines + boundaries) — an out-of-range section or entry, an absent section or
#       key, and an unsupported common pair decline typed (rc 6), never a silent empty
#       answer; a malformed `--pkgmeta-key` reference is a usage error (rc 2). The
#       boundary controls pin detection: a generic JSON `name`+`version` stays `json`,
#       a generic TOML `name`/`version` stays `toml`, and plain prose stays `opaque`
#       (a native pkgmeta selector on any of them declines typed, rc 6).
#
# Runs in the pinned `dev` service using only POSIX `sh`, coreutils, git, and the
# shipped binary (no python3, no jq):
#   docker compose run --rm --no-TTY dev sh tools/phase21-29-1-pkgmeta-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
# The `dev` lane is hard-capped on memory; keep the debug-info symbol footprint
# bounded so a full all-features link fits (symbols only — no semantics change).
# Overridable by the caller, and recorded in the receipt.
: "${CARGO_PROFILE_DEV_DEBUG:=0}"
export CARGO_PROFILE_DEV_DEBUG
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="2026-10-10"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-29-1-pkgmeta-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-29-1
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
if ! cargo build --locked --all-features > "$RAW/build.log" 2>&1; then
    tail -40 "$RAW/build.log" >&2
    echo "PHASE 21.29.1 PACKAGE-METADATA COURT: FAIL — all-features build failed" >&2
    exit 1
fi
tail -2 "$RAW/build.log"

echo "=== generate fixtures (POSIX printf only) ==="
mk() { /usr/bin/printf '%b' "$2" > "$1"; }

# --- NORMAL: package manifests ----------------------------------------------
# package.json: an npm manifest with scripts + dependencies + devDependencies.
mk "$WORK/corpus/package.json" '{"name":"demo","version":"1.0.0","scripts":{"build":"tsc"},"dependencies":{"react":"^18.0.0","left-pad":"1.0.0"},"devDependencies":{"typescript":"^5.0.0"}}\n'
# Cargo.toml: a [package] table plus inline-table and plain dependency specs.
mk "$WORK/corpus/Cargo.toml" '[package]\nname = "demo"\nversion = "0.1.0"\nedition = "2021"\n\n[dependencies]\nserde = { version = "1", features = ["derive"] }\nregex = "1"\n'
# pyproject.toml: a PEP 621 [project] plus a [build-system].
mk "$WORK/corpus/pyproject.toml" '[build-system]\nrequires = ["setuptools>=61"]\nbuild-backend = "setuptools.build_meta"\n\n[project]\nname = "demo"\nversion = "1.0.0"\ndependencies = ["requests>=2"]\n'
# Cargo.lock: a versioned [[package]] array-of-tables.
mk "$WORK/corpus/Cargo.lock" 'version = 3\n\n[[package]]\nname = "demo"\nversion = "0.1.0"\ndependencies = ["serde"]\n'
# package-lock.json: an npm lockfile (lockfileVersion + packages).
mk "$WORK/corpus/package-lock.json" '{"name":"demo","version":"1.0.0","lockfileVersion":3,"packages":{"":{"name":"demo","version":"1.0.0"},"node_modules/react":{"version":"18.0.0"}}}\n'

# --- CONTROLS (boundaries) --------------------------------------------------
# A generic JSON object with name+version and no third package-specific key -> json.
mk "$WORK/corpus/generic.json" '{"name":"x","version":"1.0.0"}\n'
# A generic TOML document with name/version but no [package] table -> toml.
mk "$WORK/corpus/generic.toml" 'name = "x"\nversion = "1.0.0"\n'
# Plain prose -> opaque.
mk "$WORK/corpus/prose.txt" 'This is just prose text.\nIt has several lines, with punctuation,\nbut no manifest structure at all.\n'

NORMAL="package.json Cargo.toml pyproject.toml Cargo.lock package-lock.json"
CONTROL="generic.json generic.toml prose.txt"
ALL="$NORMAL $CONTROL"

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
printf 'fixture\tformat\tsrc_len\tsrc_sha\tout_len\tout_sha\tcmp\texact\tcontrol_rc\n' >> "$TSV"

exact_ok=0
exact_fail=0
surface_fail=0
normal_n=0
control_n=0
boundary_ok=0
opaque_ok=0
json_control_ok=0
toml_control_ok=0
decline_ok=0
usage_ok=0

for f in $ALL; do
    # The reference copy is never deleted; the corpus copy is ingested and removed.
    src="$WORK/ref/$f"
    cp "$WORK/corpus/$f" "$src"
    src_len="$(wc -c < "$src" | tr -d ' ')"
    src_sha="$(sha256sum "$src" | cut -d' ' -f1)"

    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    field="$(json_str "$build_json" field)"
    fmt="$(json_str "$build_json" format)"

    control_rc=-1

    if is_in "$f" "$NORMAL"; then
        normal_n=$((normal_n + 1))
        [ "$fmt" = "pkgmeta" ] || { echo "FINDING: $f detected as $fmt (want pkgmeta)" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"format":"pkgmeta"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata format" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"text"' "$RAW/$f.text.json" || { echo "FINDING: $f doc-text" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --pkgmeta-section 0 --kind metadata \
            > "$RAW/$f.section0.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"role":"root"' "$RAW/$f.section0.json" || { echo "FINDING: $f section0 role" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --pkgmeta-find demo --kind text \
            > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"matches":\[' "$RAW/$f.find.json" || { echo "FINDING: $f find" >&2; surface_fail=$((surface_fail + 1)); }

        # Common search-match resolves.
        "$BIN" observe --store "$WORK/store" --field "$field" --text demo --kind text \
            > "$RAW/$f.common.search.json" 2>&1 || surface_fail=$((surface_fail + 1))

        case "$f" in
            package.json)
                grep -q '"dialect":"npm_package"' "$RAW/$f.metadata.json" || { echo "FINDING: npm dialect" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"sections":4' "$RAW/$f.metadata.json" || { echo "FINDING: npm sections" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"entries":9' "$RAW/$f.metadata.json" || { echo "FINDING: npm entries" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --pkgmeta-section 2 --kind metadata \
                    > "$RAW/$f.section2.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"role":"dependencies"' "$RAW/$f.section2.json" || { echo "FINDING: npm dependencies role" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --pkgmeta-key dependencies:react --kind exact \
                    > "$RAW/$f.dep.bin" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"value_hex":"225e3138' "$RAW/$f.dep.bin" || { echo "FINDING: npm dependency exact (want \"^18...)" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --pkgmeta-key scripts:build --kind text \
                    > "$RAW/$f.script.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q 'tsc' "$RAW/$f.script.json" || { echo "FINDING: npm script text" >&2; surface_fail=$((surface_fail + 1)); } ;;
            Cargo.toml)
                grep -q '"dialect":"cargo_manifest"' "$RAW/$f.metadata.json" || { echo "FINDING: cargo dialect" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"sections":5' "$RAW/$f.metadata.json" || { echo "FINDING: cargo sections" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"entries":9' "$RAW/$f.metadata.json" || { echo "FINDING: cargo entries" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"tables":3' "$RAW/$f.metadata.json" || { echo "FINDING: cargo tables" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"arrays":1' "$RAW/$f.metadata.json" || { echo "FINDING: cargo arrays" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --pkgmeta-key package:version --kind exact \
                    > "$RAW/$f.version.bin" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"value_hex":"22302e312e3022"' "$RAW/$f.version.bin" || { echo "FINDING: cargo version exact (want \"0.1.0\")" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --pkgmeta-key dependencies:serde --kind exact \
                    > "$RAW/$f.serde.bin" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"value_hex":"7b2076657273696f6e' "$RAW/$f.serde.bin" || { echo "FINDING: cargo inline-table spec exact" >&2; surface_fail=$((surface_fail + 1)); } ;;
            pyproject.toml)
                grep -q '"dialect":"pyproject_manifest"' "$RAW/$f.metadata.json" || { echo "FINDING: pyproject dialect" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"sections":5' "$RAW/$f.metadata.json" || { echo "FINDING: pyproject sections" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"entries":7' "$RAW/$f.metadata.json" || { echo "FINDING: pyproject entries" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --pkgmeta-key project:name --kind exact \
                    > "$RAW/$f.name.bin" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"value_hex":"2264656d6f22"' "$RAW/$f.name.bin" || { echo "FINDING: pyproject name exact (want \"demo\")" >&2; surface_fail=$((surface_fail + 1)); } ;;
            Cargo.lock)
                grep -q '"dialect":"cargo_lock"' "$RAW/$f.metadata.json" || { echo "FINDING: lock dialect" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"sections":4' "$RAW/$f.metadata.json" || { echo "FINDING: lock sections" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"entries":5' "$RAW/$f.metadata.json" || { echo "FINDING: lock entries" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --pkgmeta-entry 0 --kind exact \
                    > "$RAW/$f.entry0.bin" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"value_hex":"33"' "$RAW/$f.entry0.bin" || { echo "FINDING: lock root version=3 exact" >&2; surface_fail=$((surface_fail + 1)); } ;;
            package-lock.json)
                grep -q '"dialect":"npm_lock"' "$RAW/$f.metadata.json" || { echo "FINDING: npm-lock dialect" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"sections":4' "$RAW/$f.metadata.json" || { echo "FINDING: npm-lock sections" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"entries":9' "$RAW/$f.metadata.json" || { echo "FINDING: npm-lock entries" >&2; surface_fail=$((surface_fail + 1)); } ;;
        esac

        # H3: an out-of-range section/entry, an absent key, and an unsupported common
        # pair decline typed (rc 6), once; a malformed key reference is a usage error
        # (rc 2), once.
        if [ "$f" = "package.json" ]; then
            set +e
            "$BIN" observe --store "$WORK/store" --field "$field" --pkgmeta-section 99 --kind metadata \
                > /dev/null 2>&1
            d_rc1=$?
            "$BIN" observe --store "$WORK/store" --field "$field" --pkgmeta-entry 9999 --kind metadata \
                > /dev/null 2>&1
            d_rc2=$?
            "$BIN" observe --store "$WORK/store" --field "$field" --pkgmeta-key dependencies:nope --kind metadata \
                > /dev/null 2>&1
            d_rc3=$?
            "$BIN" observe --store "$WORK/store" --field "$field" --table 0 --kind text \
                > /dev/null 2>&1
            d_rc4=$?
            "$BIN" observe --store "$WORK/store" --field "$field" --pkgmeta-key nocolon --kind metadata \
                > /dev/null 2>&1
            u_rc=$?
            set -e
            [ "$d_rc1" -eq 6 ] && [ "$d_rc2" -eq 6 ] && [ "$d_rc3" -eq 6 ] && [ "$d_rc4" -eq 6 ] \
                && decline_ok=$((decline_ok + 1)) \
                || echo "FINDING: typed declines rc=$d_rc1/$d_rc2/$d_rc3/$d_rc4 (want 6/6/6/6)" >&2
            [ "$u_rc" -eq 2 ] && usage_ok=$((usage_ok + 1)) || echo "FINDING: malformed --pkgmeta-key rc=$u_rc (want 2)" >&2
        fi
    else
        control_n=$((control_n + 1))
        case "$f" in
            generic.json)
                [ "$fmt" = "json" ] || { echo "FINDING: $f detected as $fmt (want json)" >&2; surface_fail=$((surface_fail + 1)); }
                set +e
                "$BIN" observe --store "$WORK/store" --field "$field" --pkgmeta-section 0 --kind metadata \
                    > "$RAW/$f.pkgmeta-decline.json" 2>&1
                control_rc=$?
                set -e
                [ "$control_rc" -eq 6 ] && json_control_ok=$((json_control_ok + 1)) || { echo "FINDING: json pkgmeta-section rc=$control_rc (want 6)" >&2; }
                boundary_ok=$((boundary_ok + 1)) ;;
            generic.toml)
                [ "$fmt" = "toml" ] || { echo "FINDING: $f detected as $fmt (want toml)" >&2; surface_fail=$((surface_fail + 1)); }
                set +e
                "$BIN" observe --store "$WORK/store" --field "$field" --pkgmeta-section 0 --kind metadata \
                    > "$RAW/$f.pkgmeta-decline.json" 2>&1
                control_rc=$?
                set -e
                [ "$control_rc" -eq 6 ] && toml_control_ok=$((toml_control_ok + 1)) || { echo "FINDING: toml pkgmeta-section rc=$control_rc (want 6)" >&2; }
                boundary_ok=$((boundary_ok + 1)) ;;
            prose.txt)
                [ "$fmt" = "opaque" ] || { echo "FINDING: $f detected as $fmt (want opaque)" >&2; surface_fail=$((surface_fail + 1)); }
                set +e
                "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
                    > "$RAW/$f.metadata.json" 2>&1
                control_rc=$?
                set -e
                [ "$control_rc" -eq 6 ] && opaque_ok=$((opaque_ok + 1)) || { echo "FINDING: opaque $f metadata rc=$control_rc (want 6)" >&2; }
                boundary_ok=$((boundary_ok + 1)) ;;
        esac
        if [ "$f" != "prose.txt" ]; then
            "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
                > "$RAW/$f.metadata.json" 2>&1 || true
        fi
    fi

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

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$f" "$fmt" "$src_len" "$src_sha" "$out_len" "$out_sha" "$cmp_ok" "$exact" \
        "$control_rc" >> "$TSV"
done

echo "=== exactness after source + descriptor deletion ==="
cat "$TSV"

fixtures_n="$(printf '%s\n' $ALL | wc -w | tr -d ' ')"

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 21.29.1 package-metadata court — matrix"
    echo
    echo "| fixture | format | src len | out len | sha256 match | cmp | exact | control rc |"
    echo "| --- | --- | ---: | ---: | --- | --- | --- | ---: |"
    # shellcheck disable=SC2162
    while IFS='	' read -r a b c d e g h i j; do
        [ "$a" = "fixture" ] && continue
        [ -z "$a" ] && continue
        if [ "$d" = "$g" ]; then sh=true; else sh=false; fi
        echo "| \`$a\` | $b | $c | $e | $sh | $h | $i | $j |"
    done < "$TSV"
} > "$CAMPAIGN/MATRIX.md"

# --- counts.txt -------------------------------------------------------------
{
    echo "fixtures $fixtures_n"
    echo "pkgmeta_fixtures $normal_n"
    echo "control_fixtures $control_n"
    echo "exact_ok $exact_ok"
    echo "exact_fail $exact_fail"
    echo "surface_fail $surface_fail"
    echo "boundary_ok $boundary_ok"
    echo "opaque_control_ok $opaque_ok"
    echo "json_control_ok $json_control_ok"
    echo "toml_control_ok $toml_control_ok"
    echo "typed_declines_ok $decline_ok"
    echo "usage_ok $usage_ok"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.29.1 — package-metadata surface + exact closure + model",
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
  "env_affecting_semantics": {"LC_ALL": "C", "CARGO_PROFILE_DEV_DEBUG": "$CARGO_PROFILE_DEV_DEBUG"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
if [ "$exact_fail" -ne 0 ] || [ "$surface_fail" -ne 0 ] \
    || [ "$boundary_ok" -ne 3 ] || [ "$opaque_ok" -ne 1 ] \
    || [ "$json_control_ok" -ne 1 ] || [ "$toml_control_ok" -ne 1 ] \
    || [ "$decline_ok" -ne 1 ] || [ "$usage_ok" -ne 1 ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.29.1 — package-metadata surface + exact closure + model",
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
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion, for every fixture including the Json, Toml, and Opaque controls",
  "pkgmeta": "a bounded package-metadata adapter that REUSES the shared JSON and TOML parsers (never a second parser) and records a per-document RECORDED dialect (npm_package / cargo_manifest / pyproject_manifest / cargo_lock / npm_lock) plus a span-preserving projection: every section (a named table / array-of-tables / array element in TOML, or an object / array member in JSON) with its exact name/value spans, and every key/value entry with its exact key/value spans, grouped by section in document order; member order, duplicate keys, numeric/string spelling, and inline-table/array-valued dependency specs are preserved and never normalized",
  "observations": ["metadata", "text", "search-match", "native pkgmeta-section", "native pkgmeta-entry", "native pkgmeta-key", "native pkgmeta-find"],
  "declines": "an out-of-range section or entry, an absent section or key, and an unsupported common pair decline typed (rc 6); a malformed --pkgmeta-key reference is a usage error (rc 2); a generic JSON/TOML document and plain prose decline typed (rc 6), never a panic",
  "detection_boundary": "package-metadata detection is content-only (NEVER a file name) and is a bounded semantic test run BEFORE the generic JSON and TOML detectors. It requires a strong manifest shape: an npm package.json (a JSON object with string name+version AND a third package-specific key), an npm package-lock.json (lockfileVersion + an object packages/dependencies), a Cargo.toml (a [package] table with a string name), a pyproject.toml ([build-system] with requires, [project] with name+version, or [tool.poetry]), or a Cargo.lock (a top-level version + a [[package]] array-of-tables whose elements have a string name)",
  "cannot_distinguish": "a generic JSON object {\"name\":…,\"version\":…} with fewer than two package-specific keys is byte-for-byte indistinguishable from a minimal package.json and is deliberately NOT claimed (it stays json); a generic TOML document with name/version but no [package] table is indistinguishable from a Cargo manifest header and stays toml; and a virtual Cargo workspace manifest carrying ONLY [workspace] is not claimed and stays toml. Detection never consults a file name.",
  "precedence": "package-metadata detection runs before the generic JSON detector (and therefore before the generic TOML detector, which is itself later than JSON), so a plain JSON/TOML value is never stolen",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 45 invalid-pkgmeta-structure",
  "adr_0060": "the PkgmetaModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase21-29-1-pkgmeta-court.sh
# inside the court:
#   cargo build --locked --all-features
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--text PAT|--pkgmeta-section N|--pkgmeta-entry N|--pkgmeta-key SECTION:KEY|--pkgmeta-find PAT|--table N) --kind metadata|text|exact
#   rm source F.voldoc
#   target/debug/vole-document materialize --store STORE --field HEX --exact --output OUT ; cmp OUT source
# Regression controls (pkgmeta must not break the reused JSON/TOML courts):
#   docker compose run --rm --no-TTY dev sh tools/phase21-5-1-json-court.sh
#   docker compose run --rm --no-TTY dev sh tools/phase21-11-1-toml-court.sh
# Full gate (dev service):
#   cargo fmt --all --check
#   cargo clippy --all-targets --all-features -- -D warnings
#   cargo test --locked --all-features
#   cargo test --locked
#   MSRV: docker compose run --rm --no-TTY msrv cargo build --locked --all-features
EOF

# --- SUMMARY.md -------------------------------------------------------------
{
    echo "# Phase 21.29.1 — package-metadata adapter court"
    echo
    echo "**Question.** Do package manifests — an npm \`package.json\` / \`package-lock.json\`,"
    echo "a Cargo \`Cargo.toml\` / \`Cargo.lock\`, and a Python \`pyproject.toml\` — close exactly"
    echo "and expose a representation-preserving section/entry model (the recorded dialect,"
    echo "every section's exact name/value spans, and every key/value entry's exact spans,"
    echo "with member order, duplicate keys, numeric/string spelling, and inline-table /"
    echo "array-valued dependency specs preserved verbatim) on top of the whole-source exact"
    echo "leaf, while keeping a conservative content-only detection boundary (a generic JSON"
    echo "\`name\`+\`version\` stays \`json\`, a generic TOML \`name\`/\`version\` stays \`toml\`, and"
    echo "prose stays \`opaque\`)?"
    echo
    echo "**Method.** Each self-authored fixture is generated in-court with GNU"
    echo "\`/usr/bin/printf\`, ingested via \`field-build\`, observed, then — after the"
    echo "**source file and the standalone descriptor are deleted** — rematerialized"
    echo "exactly in a fresh process and compared with \`length + SHA-256 + cmp\`. An"
    echo "out-of-range section/entry, an absent key, and an unsupported common pair are"
    echo "required to decline typed; a malformed native key reference is a usage error; the"
    echo "generic-JSON, generic-TOML, and prose controls pin the detection boundaries. The"
    echo "court runs in the pinned \`dev\` service using only POSIX \`sh\`, coreutils, git, and"
    echo "the shipped binary (no python3, no jq)."
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
    echo "- **Shipped here:** content-only, conservative manifest detection (five recorded"
    echo "  dialects); the bounded, span-preserving section/entry model over the reused JSON"
    echo "  and TOML parsers; native \`pkgmeta-section\`/\`pkgmeta-entry\`/\`pkgmeta-key\`/"
    echo "  \`pkgmeta-find\`; common \`metadata\`/\`text\`/\`search-match\`."
    echo "- **Precedence:** pkgmeta is tried **before** the generic JSON detector (and"
    echo "  therefore before the generic TOML detector), so a plain JSON/TOML value is never"
    echo "  stolen."
    echo "- **Boundary (honest):** a generic JSON \`name\`+\`version\` with fewer than two"
    echo "  package-specific keys is indistinguishable from a minimal \`package.json\` and"
    echo "  stays \`json\`; a generic TOML \`name\`/\`version\` with no \`[package]\` table stays"
    echo "  \`toml\`; a virtual Cargo workspace manifest carrying only \`[workspace]\` stays"
    echo "  \`toml\`. Detection never consults a file name."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the manifest"
    echo "  model is derived (\`Q_gen\`) and never on the exactness path (ADR-0060: the model"
    echo "  node depends on the \`sha256(source)\` root). Nothing is normalized or"
    echo "  re-serialized."
    echo "- **Not claimed here:** an economic court, a package resolver, or any rendering."
    echo "- **Never run on the host:** every command above ran in the pinned \`dev\`"
    echo "  container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" != "PASS" ]; then
    echo "PHASE 21.29.1 PACKAGE-METADATA COURT: FAIL" >&2
    exit 1
fi
echo "PHASE 21.29.1 PACKAGE-METADATA COURT: PASS — campaign $CAMPAIGN"
