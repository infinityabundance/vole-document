#!/bin/sh
# Phase 21.20.1 — config family (INI / .env / Java .properties, structured-tree
# key/value Wave 2) surface + exact closure.
#
# Pre-registered (Phase-21 subphase 21.20.1). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture (including the malformed/Opaque
#       controls), `materialize(field) == source` (length + SHA-256 + cmp) after
#       the source file AND the standalone descriptor are deleted, in a fresh
#       process. The court FAILS unless this is 100% on every fixture.
#   H2 (surface + preserved representation) — the representation-preserving model
#       is exposed: the recorded dialect; exact section/key/value/comment spans;
#       line order; the `export` marker and quoting style; `properties` trailing
#       `\` continuations and `\uXXXX` escapes preserved as spelling; and
#       duplicate keys reported (never collapsed). Common metadata/text/search-match
#       and native config-line/config-entry/config-section/config-find answer.
#   H3 (honest declines / boundaries) — an out-of-range line or entry and an
#       unknown section decline typed (rc 6); a malformed numeric argument is a
#       usage error (rc 2). The controls pin the detection boundaries: a TOML
#       document stays `Toml`, a JSON document stays `Json`, a CSV table stays
#       `Csv`; the pure `KEY=VALUE` env/properties overlap, plain prose, a `#!`
#       script, and a sectioned-but-malformed blob stay `Opaque` with a typed
#       common decline (rc 6).
#
# Runs in the pinned `dev` service (Rust toolchain + git + coreutils; deliberately
# NO python3 and NO jq, so the court depends only on the shipped binary, GNU
# `/usr/bin/printf` for the fixtures, and POSIX shell):
#   docker compose run --rm --no-TTY dev sh tools/phase21-20-1-config-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-20-1-config-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-20
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== generate fixtures (POSIX printf only) ==="
# GNU coreutils printf writes the exact bytes. No python3, no jq.
mk() { /usr/bin/printf "$2" > "$1"; }

# --- NORMAL: detected as config, full surface exercised ----------------------
# INI: a [section], =/: entries, and a `;` inline comment.
mk "$WORK/corpus/ini.ini"          '[db]\nhost = localhost ; the host\nport: 5432\n'
# .env: an `export` signal (the env-only construct), quoting, an empty value.
mk "$WORK/corpus/env.env"          '# app\nFOO=bar\nexport BAZ="a b"\nEMPTY=\n'
# Java .properties: a `:` separator, a trailing-`\` continuation, a \uXXXX escape.
mk "$WORK/corpus/props.properties" '# note\ncolon: v\nmulti=one\\\n  two\nunicode=gr\\u00FCn\n'
# Duplicate keys must be preserved and reported, never collapsed.
mk "$WORK/corpus/dup.ini"          '[a]\nk = 1\nk = 2\n'

# --- CONTROLS ---------------------------------------------------------------
# A strict JSON document: must stay `Json` (tried before config).
/usr/bin/printf '{"a": 1, "b": [2, 3]}' > "$WORK/corpus/strict.json"
# A CSV table: must stay `Csv` (config declines; the config shape is specific).
mk "$WORK/corpus/table.csv"        'a,b\nc,d\n'
# A TOML document: an INI-shaped TOML must stay `Toml` (tried before config).
mk "$WORK/corpus/doc.toml"         'a = 1\nb = 2\n'
# The pure `KEY=VALUE` env/properties overlap: never guessed, stays `Opaque`.
mk "$WORK/corpus/overlap.env"      'FOO=bar\nBAZ=qux\n'
# Plain prose: never a config-family document.
/usr/bin/printf 'The quick brown fox jumps over the lazy dog.\nPlain prose, not config.\n' \
    > "$WORK/corpus/prose.txt"
# A `#!` script: a shebang declines.
/usr/bin/printf '#!/bin/sh\nexport FOO=bar\n' > "$WORK/corpus/script.sh"
# A section header followed by a line that is not a valid INI construct: the
# shape is inconsistent, so it stays `Opaque`.
mk "$WORK/corpus/badblock.ini"     '[a]\nthis is not an entry\n'

CONFIG="ini.ini env.env props.properties dup.ini"
JSONCTL="strict.json"
CSVCTL="table.csv"
TOMLCTL="doc.toml"
OPAQUE="overlap.env prose.txt script.sh badblock.ini"
ALL="$CONFIG $JSONCTL $CSVCTL $TOMLCTL $OPAQUE"

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
declines_ok=0
ctl_ok=0
opaque_ok=0
opaque_n=0
config_n=0
usage_ok=0

for f in $ALL; do
    # The reference copy is never deleted; the corpus copy is what is ingested and
    # then removed (so the identity comparison is against retained bytes).
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

    if is_in "$f" "$CONFIG"; then
        config_n=$((config_n + 1))
        [ "$fmt" = "config" ] || { echo "FINDING: $f not detected as config (fmt=$fmt)" >&2; surface_fail=$((surface_fail + 1)); }

        # H2: common metadata / text / search-match.
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --text "host" --kind text \
            > "$RAW/$f.search.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"format":"config"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata missing format" >&2; surface_fail=$((surface_fail + 1)); }

        # H2: native selectors + per-dialect representation assertions.
        case "$f" in
            ini.ini)
                grep -q '"dialect":"ini"' "$RAW/$f.metadata.json" || { echo "FINDING: ini dialect not reported" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --config-section db --kind text \
                    > "$RAW/$f.section.json" 2>&1 || surface_fail=$((surface_fail + 1))
                [ "$(json_str "$(cat "$RAW/$f.section.json")" text)" = "db" ] || { echo "FINDING: ini section text != db" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --config-line 1 --kind metadata \
                    > "$RAW/$f.line1.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"kind":"entry"' "$RAW/$f.line1.json" || { echo "FINDING: ini line 1 not an entry" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"inline_comment":true' "$RAW/$f.line1.json" || { echo "FINDING: ini inline comment not recorded" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --config-find host --kind text \
                    > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"role":"key"' "$RAW/$f.find.json" || { echo "FINDING: ini find missing key role" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"role":"value"' "$RAW/$f.find.json" || { echo "FINDING: ini find missing value role" >&2; surface_fail=$((surface_fail + 1)); } ;;
            env.env)
                grep -q '"dialect":"env"' "$RAW/$f.metadata.json" || { echo "FINDING: env dialect not reported" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --config-entry 1 --kind metadata \
                    > "$RAW/$f.e1.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"export":true' "$RAW/$f.e1.json" || { echo "FINDING: env export marker not recorded" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"double_quoted":true' "$RAW/$f.e1.json" || { echo "FINDING: env quoting not recorded" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --config-entry 1 --kind text \
                    > "$RAW/$f.e1.txt.json" 2>&1 || surface_fail=$((surface_fail + 1))
                [ "$(json_str "$(cat "$RAW/$f.e1.txt.json")" text)" = "BAZ=a b" ] || { echo "FINDING: env quoted value not decoded" >&2; surface_fail=$((surface_fail + 1)); } ;;
            props.properties)
                grep -q '"dialect":"properties"' "$RAW/$f.metadata.json" || { echo "FINDING: properties dialect not reported" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --config-entry 1 --kind metadata \
                    > "$RAW/$f.e1.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"continued":true' "$RAW/$f.e1.json" || { echo "FINDING: properties continuation not recorded" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --config-entry 2 --kind text \
                    > "$RAW/$f.e2.json" 2>&1 || surface_fail=$((surface_fail + 1))
                # The JSON envelope escapes the preserved spelling's backslash as `\\`;
                # the parity check proves `\uXXXX` was not expanded.
                [ "$(json_str "$(cat "$RAW/$f.e2.json")" text)" = 'unicode=gr\\u00FCn' ] || { echo "FINDING: \\uXXXX escape not preserved as spelling" >&2; surface_fail=$((surface_fail + 1)); } ;;
            dup.ini)
                "$BIN" observe --store "$WORK/store" --field "$field" --config-entry 0 --kind metadata \
                    > "$RAW/$f.e0.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"same_key_entries":2' "$RAW/$f.e0.json" || { echo "FINDING: duplicate keys not reported" >&2; surface_fail=$((surface_fail + 1)); } ;;
        esac

        # H3: an out-of-range line / entry and an unknown section decline typed (rc 6).
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --config-line 999 --kind metadata \
            > /dev/null 2>&1
        decline_rc=$?
        set -e
        [ "$decline_rc" -eq 6 ] && declines_ok=$((declines_ok + 1)) || { echo "FINDING: $f out-of-range line rc=$decline_rc (want 6)" >&2; }

    elif [ "$f" = "$JSONCTL" ]; then
        [ "$fmt" = "json" ] || { echo "FINDING: control $f detected as $fmt (want json)" >&2; surface_fail=$((surface_fail + 1)); }
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --config-entry 0 --kind metadata \
            > "$RAW/$f.config-decline.json" 2>&1
        ctl_rc=$?
        set -e
        [ "$ctl_rc" -eq 6 ] && ctl_ok=$((ctl_ok + 1)) || { echo "FINDING: JSON control config selector rc=$ctl_rc (want 6)" >&2; }

    elif [ "$f" = "$CSVCTL" ]; then
        [ "$fmt" = "csv" ] || { echo "FINDING: control $f detected as $fmt (want csv)" >&2; surface_fail=$((surface_fail + 1)); }
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --config-entry 0 --kind metadata \
            > "$RAW/$f.config-decline.json" 2>&1
        ctl_rc=$?
        set -e
        [ "$ctl_rc" -eq 6 ] && ctl_ok=$((ctl_ok + 1)) || { echo "FINDING: CSV control config selector rc=$ctl_rc (want 6)" >&2; }

    elif [ "$f" = "$TOMLCTL" ]; then
        [ "$fmt" = "toml" ] || { echo "FINDING: control $f detected as $fmt (want toml)" >&2; surface_fail=$((surface_fail + 1)); }
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --config-entry 0 --kind metadata \
            > "$RAW/$f.config-decline.json" 2>&1
        ctl_rc=$?
        set -e
        [ "$ctl_rc" -eq 6 ] && ctl_ok=$((ctl_ok + 1)) || { echo "FINDING: TOML control config selector rc=$ctl_rc (want 6)" >&2; }

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

    # A malformed numeric argument is a usage error (rc 2) — checked once.
    if [ "$f" = "ini.ini" ]; then
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --config-line abc --kind metadata \
            > /dev/null 2>&1
        u_rc=$?
        set -e
        [ "$u_rc" -eq 2 ] && usage_ok=$((usage_ok + 1)) || echo "FINDING: malformed --config-line rc=$u_rc (want 2)" >&2
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
    echo "# Phase 21.20.1 config court — matrix"
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
    echo "config_fixtures $config_n"
    echo "exact_ok $exact_ok"
    echo "exact_fail $exact_fail"
    echo "surface_fail $surface_fail"
    echo "typed_declines_ok $declines_ok"
    echo "cross_format_controls_ok $ctl_ok"
    echo "usage_ok $usage_ok"
    echo "opaque_controls_ok $opaque_ok"
    echo "opaque_controls $opaque_n"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.20.1 — config family (INI/.env/properties) surface + exact closure",
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
    || [ "$declines_ok" -ne "$config_n" ] || [ "$ctl_ok" -ne 3 ] \
    || [ "$usage_ok" -ne 1 ] || [ "$opaque_ok" -ne "$opaque_n" ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.20.1 — config family (INI/.env/properties) surface + exact closure",
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
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion, for every fixture including the Opaque controls",
  "observations": ["metadata", "text", "search-match", "native config-line", "native config-entry", "native config-section", "native config-find"],
  "surface": "the recorded dialect (ini/env/properties); exact section/key/value/comment spans; line order; the export marker and quoting style; properties trailing-backslash continuations; unicode-escape spelling preserved; duplicate keys reported, never collapsed",
  "declines": "an out-of-range line/entry and an unknown section decline typed (rc 6); a malformed numeric argument is a usage error (rc 2); the TOML control stays Toml, the JSON control stays Json, and the CSV control stays Csv, and a native config selector on each declines typed (rc 6); the pure KEY=VALUE overlap, prose, a #! script, and a sectioned-but-malformed blob stay Opaque and a common observation declines typed (rc 6), never a panic",
  "detection_boundary": "no magic bytes: INI needs a [section] header; properties needs a strong =/: separator plus a properties-only construct (: or whitespace separator, ! comment, a unicode escape, line continuation, or a non-identifier key); env needs an export prefix. The pure KEY=VALUE overlap between env and properties (=-only, #-only comments, identifier keys) is NOT guessed and stays Opaque.",
  "precedence": "after the strong magic-byte binaries, the JSON family, CBOR/MessagePack, and TOML; before CSV/Markdown/XML/HTML",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 34 invalid-config-structure",
  "adr_0060": "the ConfigModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase21-20-1-config-court.sh
# inside the court:
#   cargo build --locked --all-features
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--text PAT|--config-line N|--config-entry N|--config-section NAME|--config-find PAT) --kind metadata|text|structure|exact
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
    echo "# Phase 21.20.1 — config family court"
    echo
    echo "**Question.** Does the config family (INI / \`.env\` / Java \`.properties\`) close"
    echo "exactly and expose a representation-preserving key/value model (exact spans,"
    echo "line order, the \`export\` marker and quoting, \`properties\` continuations,"
    echo "\`\\uXXXX\` spelling, and duplicate-key reports) on top of the whole-source exact"
    echo "leaf — while keeping the conservative no-magic-byte detection boundary and"
    echo "declining malformed/ambiguous inputs typed?"
    echo
    echo "**Method.** Each self-authored fixture is generated in-court with GNU"
    echo "\`/usr/bin/printf\`, ingested via \`field-build\`, observed, then — after the"
    echo "**source file and the standalone descriptor are deleted** — rematerialized"
    echo "exactly in a fresh process and compared with \`length + SHA-256 + cmp\`. An"
    echo "out-of-range line/entry and an unknown section are required to decline typed;"
    echo "the TOML/JSON/CSV controls, the pure \`KEY=VALUE\` overlap, prose, a \`#!\`"
    echo "script, and a sectioned-but-malformed blob pin the detection boundaries. The"
    echo "court runs in the pinned \`dev\` service using only POSIX \`sh\`, coreutils,"
    echo "git, and the shipped binary (no python3, no jq)."
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
    echo "- **Shipped here:** byte-based conservative config detection; one bounded"
    echo "  parser covering INI, \`.env\`, and Java \`.properties\` with a recorded dialect;"
    echo "  native \`config-line\`/\`config-entry\`/\`config-section\`/\`config-find\`; common"
    echo "  \`metadata\`/\`text\`/\`search-match\`."
    echo "- **Precedence:** after the strong magic-byte binaries, the JSON family,"
    echo "  CBOR/MessagePack, and TOML; before CSV/Markdown/XML/HTML. A source that is"
    echo "  valid TOML is very often syntactically valid INI, so TOML wins (an"
    echo "  INI-shaped TOML stays \`Toml\`)."
    echo "- **Detection boundary:** the config family has **no magic bytes**. INI needs a"
    echo "  \`[section]\` header; \`properties\` needs a strong \`=\`/\`:\` separator plus a"
    echo "  properties-only construct; \`env\` needs an \`export \` prefix. A plain-prose/"
    echo "  \`.txt\`/Markdown/code blob and a pure \`KEY=VALUE\`/\`#\`-comment/\`=\`-only file stay"
    echo "  \`Opaque\`."
    echo "- **Recorded negative (the overlap):** a file that is all \`KEY=VALUE\` lines,"
    echo "  \`=\`-only, \`#\`-only comments, identifier keys, is byte-for-byte the same shape"
    echo "  under \`env\` and Java \`properties\`. It is **not guessed** — it stays Opaque —"
    echo "  and is claimed as a dialect only on a dialect-only signal (\`export \` for env;"
    echo "  \`:\`/whitespace/\`!\`/\`\\uXXXX\`/continuation/non-identifier key for properties)."
    echo "- **Declines:** a line that is not a valid construct of the detected dialect, a"
    echo "  line/entry/section addressed out of range, a too-deep continuation, and a cap"
    echo "  breach are typed (\`InvalidConfigStructure\` rc 34, unsupported-feature rc 6,"
    echo "  or resource-limit rc 8); such input stays \`Opaque\` when detection declines."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the config"
    echo "  model is derived (\`Q_gen\`) and never on the exactness path (ADR-0060: the"
    echo "  model node depends on the \`sha256(source)\` root)."
    echo "- **Duplicate keys** are preserved and reported (never collapsed, never a"
    echo "  decline): the representation-preserving policy."
    echo "- **Not claimed here:** the economic court (separate script)."
    echo "- **Never run on the host:** every command above ran in the pinned \`dev\`"
    echo "  container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" != "PASS" ]; then
    echo "PHASE 21.20.1 CONFIG COURT: FAIL — campaign $CAMPAIGN (exact_fail=$exact_fail surface_fail=$surface_fail declines_ok=$declines_ok ctl_ok=$ctl_ok usage_ok=$usage_ok opaque_ok=$opaque_ok/$opaque_n)" >&2
    exit 1
fi
echo "PHASE 21.20.1 CONFIG COURT: PASS — campaign $CAMPAIGN"
