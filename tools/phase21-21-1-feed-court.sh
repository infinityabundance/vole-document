#!/bin/sh
# Phase 21.21.1 — RSS 2.0 / Atom 1.0 feed (syndication Wave-2 format; XML physical
# bytes + a bounded semantic sub-detection) surface + exact closure.
#
# Pre-registered (Phase-21 subphase 21.21.1). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture (including the XML/Opaque controls),
#       `materialize(field) == source` (length + SHA-256 + cmp) after the source
#       file AND the standalone descriptor are deleted, in a fresh process. The
#       court FAILS unless this is 100% on every fixture.
#   H2 (surface + preserved representation) — the representation-preserving model is
#       exposed: the recorded dialect (rss/atom); exact element/attribute spans;
#       element order; attribute spelling (Atom `<link href=… rel=…>`, RSS
#       `<guid isPermaLink=…>`); the Atom namespace declaration. Common metadata/
#       text/search-match and native feed-channel/feed-field/feed-entry/
#       feed-entry-field/feed-find answer.
#   H3 (honest declines / boundaries) — an out-of-range record and an unknown field
#       decline typed (rc 6); a malformed `--feed-entry-field` argument is a usage
#       error (rc 2). The controls pin the detection boundary: a plain XML document,
#       an `<rss>` with no `<channel>`, a non-Atom `<feed>`, and an Atom `<feed>`
#       with no `<entry>` all stay `Xml` (tried before the generic XML detector),
#       declined here; prose stays `Opaque`. A feed selector on any control declines
#       typed (rc 6).
#
# Runs in the pinned `dev` service (Rust toolchain + git + coreutils; deliberately
# NO python3 and NO jq, so the court depends only on the shipped binary, GNU
# `/usr/bin/printf` for the fixtures, and POSIX shell):
#   docker compose run --rm --no-TTY dev sh tools/phase21-21-1-feed-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="2026-10-10"
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-21-1-feed-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-21
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== generate fixtures (POSIX printf only) ==="
# GNU coreutils printf writes the exact bytes. No python3, no jq.
mk() { /usr/bin/printf "$2" > "$1"; }

# --- NORMAL: detected as a feed, full surface exercised -----------------------
# RSS 2.0: a <channel>, channel fields, two <item> records, a <guid isPermaLink>.
mk "$WORK/corpus/rss.xml" \
'<?xml version="1.0"?>\n<rss version="2.0">\n<channel>\n<title>Example Feed</title>\n<link>https://example.com/</link>\n<description>An example</description>\n<language>en</language>\n<item>\n<title>First</title>\n<link>https://example.com/1</link>\n<description>one</description>\n<pubDate>Mon, 01 Jan 2024 00:00:00 GMT</pubDate>\n<guid isPermaLink="false">urn:1</guid>\n<category>news</category>\n</item>\n<item>\n<title>Second</title>\n<guid>urn:2</guid>\n</item>\n</channel>\n</rss>\n'
# Atom 1.0: the Atom namespace declaration, a <link href=… rel=…>, two <entry>.
mk "$WORK/corpus/atom.atom" \
'<?xml version="1.0"?>\n<feed xmlns="http://www.w3.org/2005/Atom">\n<id>urn:feed:1</id>\n<title>Example</title>\n<updated>2024-01-01T00:00:00Z</updated>\n<link href="https://example.com/" rel="alternate"/>\n<entry>\n<id>urn:entry:1</id>\n<title>First</title>\n<link href="https://example.com/1"/>\n<updated>2024-01-01T00:00:00Z</updated>\n<summary>one</summary>\n<content type="html">&lt;b&gt;one&lt;/b&gt;</content>\n</entry>\n<entry>\n<id>urn:entry:2</id>\n<title>Second</title>\n<link href="https://example.com/2"/>\n</entry>\n</feed>\n'

# --- CONTROLS ---------------------------------------------------------------
# A plain XML document: a more-specific feed claim declines → stays `Xml`.
/usr/bin/printf '<note><to>Tove</to><from>Jani</from></note>' > "$WORK/corpus/plain.xml"
# An `<rss>` root with no `<channel>`: not a feed → stays `Xml`.
/usr/bin/printf '<rss version="2.0"></rss>' > "$WORK/corpus/nochannel.xml"
# A `<feed>` with no Atom namespace: not a feed → stays `Xml`.
/usr/bin/printf '<feed><entry><id>x</id></entry></feed>' > "$WORK/corpus/nons.atom"
# Atom `<feed>` with no `<entry>`: not a feed → stays `Xml`.
/usr/bin/printf '<feed xmlns="http://www.w3.org/2005/Atom"><id>x</id></feed>' \
    > "$WORK/corpus/noentry.atom"
# Plain prose: never a feed.
/usr/bin/printf 'The quick brown fox jumps over the lazy dog.\nPlain prose, not a feed.\n' \
    > "$WORK/corpus/prose.txt"

FEED="rss.xml atom.atom"
XMLCTL="plain.xml nochannel.xml nons.atom noentry.atom"
OPAQUE="prose.txt"
ALL="$FEED $XMLCTL $OPAQUE"

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
decline_entry_ok=0
decline_field_ok=0
ctl_ok=0
opaque_ok=0
opaque_n=0
feed_n=0
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

    if is_in "$f" "$FEED"; then
        feed_n=$((feed_n + 1))
        [ "$fmt" = "feed" ] || { echo "FINDING: $f not detected as feed (fmt=$fmt)" >&2; surface_fail=$((surface_fail + 1)); }

        # H2: common metadata / text / search-match.
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
        "$BIN" observe --store "$WORK/store" --field "$field" --text "Example Feed" --kind text \
            > "$RAW/$f.search.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"format":"feed"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata missing format" >&2; surface_fail=$((surface_fail + 1)); }

        # H2: native selectors + per-dialect representation assertions.
        case "$f" in
            rss.xml)
                grep -q '"dialect":"rss"' "$RAW/$f.metadata.json" || { echo "FINDING: rss dialect not reported" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"entries":2' "$RAW/$f.metadata.json" || { echo "FINDING: rss entry count not reported" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --feed-channel --kind metadata \
                    > "$RAW/$f.channel.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"entries":2' "$RAW/$f.channel.json" || { echo "FINDING: rss channel missing entries" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --feed-field language --kind text \
                    > "$RAW/$f.language.json" 2>&1 || surface_fail=$((surface_fail + 1))
                [ "$(json_str "$(cat "$RAW/$f.language.json")" text)" = "en" ] || { echo "FINDING: rss language field text != en" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --feed-entry 0 --kind metadata \
                    > "$RAW/$f.e0.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"name":"guid"' "$RAW/$f.e0.json" || { echo "FINDING: rss item missing guid" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"name":"isPermaLink"' "$RAW/$f.e0.json" || { echo "FINDING: guid attribute spelling not preserved" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"value":"false"' "$RAW/$f.e0.json" || { echo "FINDING: guid attribute value not preserved" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --feed-entry-field 0:guid --kind text \
                    > "$RAW/$f.guid.txt.json" 2>&1 || surface_fail=$((surface_fail + 1))
                [ "$(json_str "$(cat "$RAW/$f.guid.txt.json")" text)" = "urn:1" ] || { echo "FINDING: rss guid text != urn:1" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --feed-entry 1 --kind metadata \
                    > "$RAW/$f.e1.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"index":1' "$RAW/$f.e1.json" || { echo "FINDING: rss item order not preserved" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --feed-find First --kind text \
                    > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"entry":0' "$RAW/$f.find.json" || { echo "FINDING: rss find missing entry ordinal" >&2; surface_fail=$((surface_fail + 1)); } ;;
            atom.atom)
                grep -q '"dialect":"atom"' "$RAW/$f.metadata.json" || { echo "FINDING: atom dialect not reported" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"entries":2' "$RAW/$f.metadata.json" || { echo "FINDING: atom entry count not reported" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --feed-field link --kind text \
                    > "$RAW/$f.link.txt.json" 2>&1 || surface_fail=$((surface_fail + 1))
                [ "$(json_str "$(cat "$RAW/$f.link.txt.json")" text)" = "https://example.com/" ] || { echo "FINDING: atom link href not decoded" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --feed-field link --kind metadata \
                    > "$RAW/$f.link.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"name":"href"' "$RAW/$f.link.json" || { echo "FINDING: atom link missing href attribute" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"name":"rel"' "$RAW/$f.link.json" || { echo "FINDING: atom link missing rel attribute" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --feed-entry 0 --kind metadata \
                    > "$RAW/$f.e0.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"name":"content"' "$RAW/$f.e0.json" || { echo "FINDING: atom entry missing content" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --feed-entry-field 1:title --kind text \
                    > "$RAW/$f.title1.json" 2>&1 || surface_fail=$((surface_fail + 1))
                [ "$(json_str "$(cat "$RAW/$f.title1.json")" text)" = "Second" ] || { echo "FINDING: atom entry 1 title != Second" >&2; surface_fail=$((surface_fail + 1)); } ;;
        esac

        # H3: an out-of-range record and an unknown field decline typed (rc 6).
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --feed-entry 999 --kind metadata \
            > /dev/null 2>&1
        decline_rc=$?
        set -e
        [ "$decline_rc" -eq 6 ] && decline_entry_ok=$((decline_entry_ok + 1)) || { echo "FINDING: $f out-of-range entry rc=$decline_rc (want 6)" >&2; }
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --feed-field nope --kind text \
            > /dev/null 2>&1
        field_rc=$?
        set -e
        [ "$field_rc" -eq 6 ] && decline_field_ok=$((decline_field_ok + 1)) || { echo "FINDING: $f unknown field rc=$field_rc (want 6)" >&2; }

    elif is_in "$f" "$XMLCTL"; then
        [ "$fmt" = "xml" ] || { echo "FINDING: control $f detected as $fmt (want xml)" >&2; surface_fail=$((surface_fail + 1)); }
        # A feed selector on a non-feed (Xml) field declines typed.
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --feed-field title --kind text \
            > "$RAW/$f.feed-decline.json" 2>&1
        ctl_rc=$?
        set -e
        [ "$ctl_rc" -eq 6 ] && ctl_ok=$((ctl_ok + 1)) || { echo "FINDING: XML control $f feed selector rc=$ctl_rc (want 6)" >&2; }

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

    # A malformed `--feed-entry-field` argument is a usage error (rc 2) — checked once.
    if [ "$f" = "rss.xml" ]; then
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --feed-entry-field abc --kind metadata \
            > /dev/null 2>&1
        u_rc=$?
        set -e
        [ "$u_rc" -eq 2 ] && usage_ok=$((usage_ok + 1)) || echo "FINDING: malformed --feed-entry-field rc=$u_rc (want 2)" >&2
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
    echo "# Phase 21.21.1 feed court — matrix"
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
    echo "feed_fixtures $feed_n"
    echo "exact_ok $exact_ok"
    echo "exact_fail $exact_fail"
    echo "surface_fail $surface_fail"
    echo "typed_decline_records_ok $decline_entry_ok"
    echo "typed_decline_fields_ok $decline_field_ok"
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
  "phase": "21.21.1 — RSS 2.0 / Atom 1.0 feed surface + exact closure",
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
    || [ "$decline_entry_ok" -ne "$feed_n" ] || [ "$decline_field_ok" -ne "$feed_n" ] \
    || [ "$ctl_ok" -ne 4 ] \
    || [ "$usage_ok" -ne 1 ] || [ "$opaque_ok" -ne "$opaque_n" ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.21.1 — RSS 2.0 / Atom 1.0 feed surface + exact closure",
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
  "observations": ["metadata", "text", "search-match", "native feed-channel", "native feed-field", "native feed-entry", "native feed-entry-field", "native feed-find"],
  "surface": "the recorded dialect (rss/atom); exact element/attribute spans; element order; attribute spelling (Atom link href/rel, RSS guid isPermaLink); the Atom namespace declaration; entity references preserved as spelling (never expanded)",
  "declines": "an out-of-range record and an unknown field decline typed (rc 6); a malformed --feed-entry-field argument is a usage error (rc 2); the plain-XML / no-channel / non-Atom-feed / no-entry controls stay Xml and a native feed selector on each declines typed (rc 6); prose stays Opaque and a common observation declines typed (rc 6), never a panic",
  "detection_boundary": "a feed's physical bytes are XML, so detection is a bounded semantic sub-test run before the generic XML detector: an RSS root <rss> with a <channel> child, or an Atom root <feed> in the Atom namespace (http://www.w3.org/2005/Atom) with at least one <entry> child. A plain XML document stays Xml; an <rss>-shaped-but-invalid doc and a non-Atom/record-less <feed> stay Xml. RSS 1.0 (rdf:RDF root) and Atom 0.3 (a different namespace) are NOT recognized and stay Xml.",
  "precedence": "after the strong magic-byte binaries, the JSON family, CBOR/MessagePack, TOML, and the config family; before the generic standalone-XML and HTML detectors",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 35 invalid-feed-structure",
  "adr_0060": "the FeedModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase21-21-1-feed-court.sh
# inside the court:
#   cargo build --locked --all-features
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--text PAT|--feed-channel|--feed-field NAME|--feed-entry N|--feed-entry-field N:NAME|--feed-find PAT) --kind metadata|text|structure|exact
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
    echo "# Phase 21.21.1 — RSS/Atom feed court"
    echo
    echo "**Question.** Does the RSS 2.0 / Atom 1.0 feed adapter close exactly and"
    echo "expose a representation-preserving model (the recorded dialect; exact"
    echo "element/attribute spans; element order; Atom \`link href/rel\` and RSS"
    echo "\`guid isPermaLink\` attribute spelling; the Atom namespace declaration) on top"
    echo "of the whole-source exact leaf — while keeping the bounded semantic"
    echo "sub-detection boundary (before the generic XML detector) and declining"
    echo "malformed/ambiguous inputs typed?"
    echo
    echo "**Method.** Each self-authored fixture is generated in-court with GNU"
    echo "\`/usr/bin/printf\`, ingested via \`field-build\`, observed, then — after the"
    echo "**source file and the standalone descriptor are deleted** — rematerialized"
    echo "exactly in a fresh process and compared with \`length + SHA-256 + cmp\`. An"
    echo "out-of-range record and an unknown field are required to decline typed; the"
    echo "plain-XML / no-channel / non-Atom-feed / no-entry controls and prose pin the"
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
    echo "- **Shipped here:** byte-based bounded semantic feed detection; one bounded"
    echo "  RSS 2.0 / Atom 1.0 model reusing the shared XML parser with a recorded"
    echo "  dialect; native \`feed-channel\`/\`feed-field\`/\`feed-entry\`/"
    echo "  \`feed-entry-field\`/\`feed-find\`; common \`metadata\`/\`text\`/\`search-match\`."
    echo "- **Precedence:** after the strong magic-byte binaries, the JSON family,"
    echo "  CBOR/MessagePack, TOML, and the config family; before the generic"
    echo "  standalone-XML and HTML detectors. A feed is a more specific claim than a"
    echo "  bare XML tree, so it is tried first."
    echo "- **Detection boundary:** a feed's physical bytes are XML. RSS is claimed on"
    echo "  a \`<rss>\` root with a \`<channel>\` child; Atom on a \`<feed>\` root in the"
    echo "  Atom namespace with at least one \`<entry>\` child. A plain XML document, an"
    echo "  \`<rss>\`-shaped-but-invalid document, a non-Atom \`<feed>\`, and an Atom"
    echo "  \`<feed>\` with no \`<entry>\` all stay \`Xml\`; prose stays \`Opaque\`."
    echo "- **Recorded negative (not distinguished):** RSS 1.0 (its root is"
    echo "  \`rdf:RDF\`, an RDF graph) and Atom 0.3 (a different namespace URI) are"
    echo "  **not** recognized and stay \`Xml\`; only RSS 2.0's \`<rss><channel>\` shape"
    echo "  and Atom 1.0's namespace are claimed."
    echo "- **Declines:** an out-of-range record, an unknown field, a malformed"
    echo "  \`--feed-entry-field\` argument, a cap breach, and a non-feed source are"
    echo "  typed (\`InvalidFeedStructure\` rc 35, unsupported-feature rc 6,"
    echo "  resource-limit rc 8, or usage rc 2); such input stays \`Xml\`/\`Opaque\` when"
    echo "  detection declines."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the feed"
    echo "  model is derived (\`Q_gen\`) and never on the exactness path (ADR-0060: the"
    echo "  model node depends on the \`sha256(source)\` root)."
    echo "- **Not claimed here:** the economic court (separate script)."
    echo "- **Never run on the host:** every command above ran in the pinned \`dev\`"
    echo "  container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" != "PASS" ]; then
    echo "PHASE 21.21.1 FEED COURT: FAIL — campaign $CAMPAIGN (exact_fail=$exact_fail surface_fail=$surface_fail decline_entry_ok=$decline_entry_ok decline_field_ok=$decline_field_ok ctl_ok=$ctl_ok usage_ok=$usage_ok opaque_ok=$opaque_ok/$opaque_n)" >&2
    exit 1
fi
echo "PHASE 21.21.1 FEED COURT: PASS — campaign $CAMPAIGN"
