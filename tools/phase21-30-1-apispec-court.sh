#!/bin/sh
# Phase 21.30 — the API/specification adapter court.
#
# Pre-registered (Phase-21 subphase 21.30). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture (including the JSON and Opaque controls),
#       `materialize(field) == source` (length + SHA-256 + cmp) after the source file
#       AND the standalone descriptor are deleted, in a fresh process. The court FAILS
#       unless this is 100% on every fixture.
#   H2 (surface + preserved representation) — the representation-preserving API-spec
#       model is exposed: the **recorded dialect** (json_schema / openapi / swagger /
#       asyncapi); the **exact spec-version string** (`$schema` / `openapi` / `swagger`
#       / `asyncapi` value, never normalized); every recorded **object** (a role plus
#       exact value/naming spans and a member range); and every **`$ref`** (its exact
#       target string, preserved **verbatim** and never resolved). Member order,
#       duplicate keys, numeric/string spelling, and keyword spelling are preserved.
#       Native apispec-dialect/apispec-version/apispec-object/apispec-ref/apispec-find
#       and common metadata/text/search-match answer.
#   H3 (declines + boundaries) — an out-of-range object or `$ref` and an unsupported
#       common pair decline typed (rc 6), never a silent empty answer. The boundary
#       controls pin detection: a generic JSON object stays `json`, a JSON-Schema-*shaped*
#       object with no `$schema` stays `json`, and plain prose stays `opaque` (a native
#       apispec selector on any of them declines typed, rc 6).
#
# Runs in the pinned `dev` service using only POSIX `sh`, coreutils, git, and the
# shipped binary (no python3, no jq):
#   docker compose run --rm --no-TTY dev sh tools/phase21-30-1-apispec-court.sh

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
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-30-1-apispec-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-30-1
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
if ! cargo build --locked --all-features > "$RAW/build.log" 2>&1; then
    tail -40 "$RAW/build.log" >&2
    echo "PHASE 21.30.1 API-SPEC COURT: FAIL — all-features build failed" >&2
    exit 1
fi
tail -2 "$RAW/build.log"

echo "=== generate fixtures (POSIX printf only) ==="
mk() { /usr/bin/printf '%b' "$2" > "$1"; }

# --- NORMAL: API specifications ---------------------------------------------
# schema.json: a JSON Schema with properties/required/enum/definitions/$defs and a $ref.
mk "$WORK/corpus/schema.json" '{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"name":{"type":"string"},"age":{"type":"integer"}},"required":["name"],"enum":["a","b"],"definitions":{"Legacy":{"$ref":"#/$defs/Id"}},"$defs":{"Id":{"type":"string","format":"uuid"}}}\n'
# openapi.json: an OpenAPI 3.x document with paths/operations/components and a $ref.
mk "$WORK/corpus/openapi.json" '{"openapi":"3.1.0","info":{"title":"Demo","version":"1.0.0"},"paths":{"/pets":{"get":{"responses":{"200":{"description":"ok"}}}}},"components":{"schemas":{"Pet":{"$ref":"#/components/schemas/Pet"}}}}\n'
# swagger.json: a Swagger 2.0 document with basePath/paths/definitions.
mk "$WORK/corpus/swagger.json" '{"swagger":"2.0","info":{"title":"Demo","version":"1.0.0"},"basePath":"/v1","paths":{"/pets":{"get":{"responses":{"200":{"description":"ok"}}}}},"definitions":{"Pet":{"type":"object"}}}\n'
# asyncapi.json: an AsyncAPI document with channels/operations/servers.
mk "$WORK/corpus/asyncapi.json" '{"asyncapi":"2.6.0","info":{"title":"Demo","version":"1.0.0"},"channels":{"user/signedup":{"subscribe":{}}},"operations":{"sendUser":{"action":"send"}},"servers":{"prod":{"url":"broker.example.com"}}}\n'

# --- CONTROLS (boundaries) --------------------------------------------------
# A generic JSON object -> json.
mk "$WORK/corpus/generic.json" '{"name":"x","version":"1.0.0"}\n'
# A JSON-Schema-*shaped* object with no `$schema` -> json (honest ambiguity).
mk "$WORK/corpus/shape.json" '{"type":"object","properties":{"a":{"type":"string"}}}\n'
# Plain prose -> opaque.
mk "$WORK/corpus/prose.txt" 'This is just prose text.\nIt has several lines, with punctuation,\nbut no specification structure at all.\n'

NORMAL="schema.json openapi.json swagger.json asyncapi.json"
CONTROL="generic.json shape.json prose.txt"
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
decline_ok=0

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
        [ "$fmt" = "apispec" ] || { echo "FINDING: $f detected as $fmt (want apispec)" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"format":"apispec"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata format" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"text"' "$RAW/$f.text.json" || { echo "FINDING: $f doc-text" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --apispec-dialect --kind text \
            > "$RAW/$f.dialect.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"text"' "$RAW/$f.dialect.json" || { echo "FINDING: $f dialect" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --apispec-object 0 --kind metadata \
            > "$RAW/$f.object0.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"role":"root"' "$RAW/$f.object0.json" || { echo "FINDING: $f object0 role" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --apispec-find Demo --kind text \
            > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"matches":\[' "$RAW/$f.find.json" || { echo "FINDING: $f find" >&2; surface_fail=$((surface_fail + 1)); }

        # Common search-match resolves.
        "$BIN" observe --store "$WORK/store" --field "$field" --text Demo --kind text \
            > "$RAW/$f.common.search.json" 2>&1 || surface_fail=$((surface_fail + 1))

        case "$f" in
            schema.json)
                grep -q '"dialect":"json_schema"' "$RAW/$f.metadata.json" || { echo "FINDING: schema dialect" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"refs":1' "$RAW/$f.metadata.json" || { echo "FINDING: schema refs" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --apispec-version --kind text \
                    > "$RAW/$f.version.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q 'json-schema.org' "$RAW/$f.version.json" || { echo "FINDING: schema version text" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --apispec-ref 0 --kind exact \
                    > "$RAW/$f.ref0.bin" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"value_hex":"22232f24' "$RAW/$f.ref0.bin" || { echo "FINDING: schema ref target (want \"#/$...)" >&2; surface_fail=$((surface_fail + 1)); } ;;
            openapi.json)
                grep -q '"dialect":"openapi"' "$RAW/$f.metadata.json" || { echo "FINDING: openapi dialect" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"refs":1' "$RAW/$f.metadata.json" || { echo "FINDING: openapi refs" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --apispec-version --kind exact \
                    > "$RAW/$f.version.bin" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"value_hex":"22332e312e3022"' "$RAW/$f.version.bin" || { echo "FINDING: openapi version exact (want \"3.1.0\")" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --apispec-ref 0 --kind exact \
                    > "$RAW/$f.ref0.bin" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"value_hex":"22232f636f6d' "$RAW/$f.ref0.bin" || { echo "FINDING: openapi ref target (want \"#/components...)" >&2; surface_fail=$((surface_fail + 1)); } ;;
            swagger.json)
                grep -q '"dialect":"swagger"' "$RAW/$f.metadata.json" || { echo "FINDING: swagger dialect" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --apispec-version --kind text \
                    > "$RAW/$f.version.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '2\.0' "$RAW/$f.version.json" || { echo "FINDING: swagger version text" >&2; surface_fail=$((surface_fail + 1)); } ;;
            asyncapi.json)
                grep -q '"dialect":"asyncapi"' "$RAW/$f.metadata.json" || { echo "FINDING: asyncapi dialect" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"refs":0' "$RAW/$f.metadata.json" || { echo "FINDING: asyncapi refs" >&2; surface_fail=$((surface_fail + 1)); } ;;
        esac

        # H3: an out-of-range object/$ref and an unsupported common pair decline typed
        # (rc 6), once.
        if [ "$f" = "openapi.json" ]; then
            set +e
            "$BIN" observe --store "$WORK/store" --field "$field" --apispec-object 99 --kind metadata \
                > /dev/null 2>&1
            d_rc1=$?
            "$BIN" observe --store "$WORK/store" --field "$field" --apispec-ref 99 --kind metadata \
                > /dev/null 2>&1
            d_rc2=$?
            "$BIN" observe --store "$WORK/store" --field "$field" --table 0 --kind text \
                > /dev/null 2>&1
            d_rc3=$?
            set -e
            [ "$d_rc1" -eq 6 ] && [ "$d_rc2" -eq 6 ] && [ "$d_rc3" -eq 6 ] \
                && decline_ok=$((decline_ok + 1)) \
                || echo "FINDING: typed declines rc=$d_rc1/$d_rc2/$d_rc3 (want 6/6/6)" >&2
        fi
    else
        control_n=$((control_n + 1))
        case "$f" in
            generic.json|shape.json)
                [ "$fmt" = "json" ] || { echo "FINDING: $f detected as $fmt (want json)" >&2; surface_fail=$((surface_fail + 1)); }
                set +e
                "$BIN" observe --store "$WORK/store" --field "$field" --apispec-object 0 --kind metadata \
                    > "$RAW/$f.apispec-decline.json" 2>&1
                control_rc=$?
                set -e
                [ "$control_rc" -eq 6 ] && json_control_ok=$((json_control_ok + 1)) || { echo "FINDING: $f apispec-object rc=$control_rc (want 6)" >&2; }
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
    echo "# Phase 21.30.1 API-spec court — matrix"
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
    echo "apispec_fixtures $normal_n"
    echo "control_fixtures $control_n"
    echo "exact_ok $exact_ok"
    echo "exact_fail $exact_fail"
    echo "surface_fail $surface_fail"
    echo "boundary_ok $boundary_ok"
    echo "opaque_control_ok $opaque_ok"
    echo "json_control_ok $json_control_ok"
    echo "typed_declines_ok $decline_ok"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.30.1 — API/specification surface + exact closure + model",
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
    || [ "$json_control_ok" -ne 2 ] \
    || [ "$decline_ok" -ne 1 ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.30.1 — API/specification surface + exact closure + model",
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
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion, for every fixture including the Json and Opaque controls",
  "apispec": "a bounded API/specification adapter that REUSES the shared JSON parser (never a second parser) and records a per-document RECORDED dialect (json_schema / openapi / swagger / asyncapi) plus the exact spec-version string and a span-preserving projection: every object (a role plus exact value/naming spans and a member range) and every \u0024ref (its exact target string, preserved VERBATIM and never resolved); member order, duplicate keys, numeric/string spelling, and keyword spelling are preserved and never normalized",
  "observations": ["metadata", "text", "search-match", "native apispec-dialect", "native apispec-version", "native apispec-object", "native apispec-ref", "native apispec-find"],
  "declines": "an out-of-range object or \u0024ref and an unsupported common pair decline typed (rc 6); a generic JSON object and a JSON-Schema-shaped object (no \u0024schema) and plain prose decline typed (rc 6), never a panic",
  "detection_boundary": "API-spec detection is content-only (NEVER a file name) and is a bounded semantic test run BEFORE the generic JSON detector. It requires a strong, root-level string marker: a JSON-Schema \u0024schema URI (json-schema.org), an \u006fpenapi 3.x string, a swagger equal to \"2.0\", or a non-empty asyncapi string",
  "cannot_distinguish": "a JSON-Schema-SHAPED object with no \u0024schema (e.g. {\"type\":\"object\",\"properties\":{...}}) is byte-for-byte indistinguishable from a plain JSON tree and is deliberately NOT claimed (it stays json); a plain JSON object that merely contains a properties/paths/components key but lacks the marker also stays json; an openapi value that is not 3.x and a swagger value other than \"2.0\" are not claimed. Detection never consults a file name.",
  "refs": "\u0024ref targets are NEVER resolved, dereferenced, or fetched: a dangling, external, or cyclic reference is preserved verbatim",
  "precedence": "API-spec detection runs before the generic JSON detector, so a plain JSON value is never stolen",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 46 invalid-apispec-structure",
  "adr_0060": "the ApispecModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase21-30-1-apispec-court.sh
# inside the court:
#   cargo build --locked --all-features
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--text PAT|--apispec-dialect|--apispec-version|--apispec-object N|--apispec-ref N|--apispec-find PAT|--table N) --kind metadata|text|exact
#   rm source F.voldoc
#   target/debug/vole-document materialize --store STORE --field HEX --exact --output OUT ; cmp OUT source
# Regression controls (apispec must not break the reused JSON court):
#   docker compose run --rm --no-TTY dev sh tools/phase21-5-1-json-court.sh
# Full gate (dev service):
#   cargo fmt --all --check
#   cargo clippy --all-targets --all-features -- -D warnings
#   cargo test --locked --all-features
#   cargo test --locked
#   MSRV: docker compose run --rm --no-TTY msrv cargo build --locked --all-features
EOF

# --- SUMMARY.md -------------------------------------------------------------
{
    echo "# Phase 21.30.1 — API/specification adapter court"
    echo
    echo "**Question.** Do API specifications — a JSON Schema, an OpenAPI 3.x document,"
    echo "a Swagger 2.0 document, and an AsyncAPI document — close exactly and expose a"
    echo "representation-preserving model (the recorded dialect, the exact spec-version"
    echo "string, every object with its role and exact spans, and every \`\$ref\` preserved"
    echo "verbatim and never resolved) on top of the whole-source exact leaf, while keeping"
    echo "a conservative content-only detection boundary (a plain JSON object stays \`json\`,"
    echo "a JSON-Schema-shaped object with no \`\$schema\` stays \`json\`, and prose stays"
    echo "\`opaque\`)?"
    echo
    echo "**Method.** Each self-authored fixture is generated in-court with GNU"
    echo "\`/usr/bin/printf\`, ingested via \`field-build\`, observed, then — after the"
    echo "**source file and the standalone descriptor are deleted** — rematerialized"
    echo "exactly in a fresh process and compared with \`length + SHA-256 + cmp\`. An"
    echo "out-of-range object/\`\$ref\` and an unsupported common pair are required to"
    echo "decline typed; the generic-JSON, JSON-Schema-shaped, and prose controls pin the"
    echo "detection boundaries. The court runs in the pinned \`dev\` service using only"
    echo "POSIX \`sh\`, coreutils, git, and the shipped binary (no python3, no jq)."
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
    echo "- **Shipped here:** content-only, conservative API-spec detection (four recorded"
    echo "  dialects); the bounded, span-preserving object/member/\`\$ref\` model over the"
    echo "  reused JSON parser; native \`apispec-dialect\`/\`apispec-version\`/\`apispec-object\`/"
    echo "  \`apispec-ref\`/\`apispec-find\`; common \`metadata\`/\`text\`/\`search-match\`."
    echo "- **Precedence:** apispec is tried **before** the generic JSON detector, so a"
    echo "  plain JSON value is never stolen."
    echo "- **Boundary (honest):** a JSON-Schema-*shaped* object with no \`\$schema\` is"
    echo "  indistinguishable from a plain JSON tree and stays \`json\`; a plain JSON object"
    echo "  that merely has a \`properties\`/\`paths\`/\`components\` key without the marker also"
    echo "  stays \`json\`. Detection never consults a file name."
    echo "- **Refs:** \`\$ref\` targets are preserved verbatim and are **never** resolved,"
    echo "  dereferenced, or fetched (dangling/external/cyclic references are recorded as"
    echo "  written)."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the API-spec"
    echo "  model is derived (\`Q_gen\`) and never on the exactness path (ADR-0060: the model"
    echo "  node depends on the \`sha256(source)\` root). Nothing is normalized, resolved, or"
    echo "  re-serialized."
    echo "- **Not claimed here:** a schema validator, a \`\$ref\` resolver, or any rendering."
    echo "- **Never run on the host:** every command above ran in the pinned \`dev\`"
    echo "  container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" != "PASS" ]; then
    echo "PHASE 21.30.1 API-SPEC COURT: FAIL" >&2
    exit 1
fi
echo "PHASE 21.30.1 API-SPEC COURT: PASS — campaign $CAMPAIGN"
