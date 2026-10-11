#!/bin/sh
# Phase 21.27 — the MHTML (MIME HTML) adapter court.
#
# Pre-registered (Phase-21 subphase 21.27). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture (including the Opaque, Html, and Eml
#       controls), `materialize(field) == source` (length + SHA-256 + cmp) after the
#       source file AND the standalone descriptor are deleted, in a fresh process. The
#       court FAILS unless this is 100% on every fixture.
#   H2 (surface + preserved representation) — the MHTML model is exposed on top of the
#       reused MIME layer and HTML scanner: the MIME envelope (`multipart/related`,
#       `Snapshot-Content-Location`, `Content-Base`), the ordered parts and their exact
#       spans, the ordered sub-resources keyed by `Content-Location`/`Content-ID`, the
#       root HTML part (the `start=` `Content-ID`, else the first `text/html` part,
#       else the first part) parsed by the reusable HTML scanner, and
#       quoted-printable/base64 parts decoded for observations while the raw encoded
#       bytes stay in the source. Common metadata/text/resource/search-match and native
#       mhtml-root/mhtml-resource/mhtml-location/mhtml-find answer.
#   H3 (declines + boundaries) — an out-of-range native resource and an unsupported
#       common pair (the common `table`) decline typed (rc 6), never a silent empty
#       answer; a malformed `--mhtml-resource` argument is a usage error (rc 2). The
#       boundary controls pin detection: plain prose stays `opaque`; a plain HTML
#       document stays `html`; a plain MIME message stays `eml`; a plain
#       `multipart/related` **email** (no MHTML markers, no `text/html` part) stays
#       `eml`, and a `multipart/related` with a `text/html` part but no MHTML signal
#       also stays `eml` — MHTML is never mislabeled.
#
# Runs in the pinned `dev` service using only POSIX `sh`, coreutils, git, and the
# shipped binary (no python3, no jq):
#   docker compose run --rm --no-TTY dev sh tools/phase21-27-1-mhtml-court.sh

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
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-27-1-mhtml-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-27-1
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
if ! cargo build --locked --all-features > "$RAW/build.log" 2>&1; then
    tail -40 "$RAW/build.log" >&2
    echo "PHASE 21.27.1 MHTML COURT: FAIL — all-features build failed" >&2
    exit 1
fi
tail -2 "$RAW/build.log"

echo "=== generate fixtures (POSIX printf only) ==="
mk() { /usr/bin/printf '%b' "$2" > "$1"; }

# --- NORMAL: MHTML archives -------------------------------------------------
# base.mhtml: a `multipart/related` with `Snapshot-Content-Location` + `Content-Base`,
# a quoted-printable HTML root (Content-ID <root@x> selected by start=), a
# quoted-printable CSS sub-resource, and a base64 PNG sub-resource.
mk "$WORK/corpus/base.mhtml" 'From: <Saved by Blink>\r\nSubject: Example page\r\nDate: Mon, 01 Jan 2029 00:00:00 +0000\r\nMIME-Version: 1.0\r\nContent-Type: multipart/related; boundary="BOUND"; type="text/html"; start="<root@x>"\r\nSnapshot-Content-Location: http://example.com/page.html\r\nContent-Base: http://example.com/\r\n\r\n--BOUND\r\nContent-Type: text/html; charset=utf-8\r\nContent-Transfer-Encoding: quoted-printable\r\nContent-Location: http://example.com/page.html\r\nContent-ID: <root@x>\r\n\r\n<html><head><link rel=3D"stylesheet" href=3D"s.css"></head><body><p>Caf=C3=A9</p><img src=3D"i.png"></body></html>\r\n--BOUND\r\nContent-Type: text/css\r\nContent-Transfer-Encoding: quoted-printable\r\nContent-Location: http://example.com/s.css\r\n\r\np { color: =23458; }\r\n--BOUND\r\nContent-Type: image/png\r\nContent-Transfer-Encoding: base64\r\nContent-Location: http://example.com/i.png\r\nContent-ID: <img1@x>\r\n\r\niVBORw0KGgo=\r\n--BOUND--\r\n'
# nostart.mhtml: no `start=`; the root is the first `text/html` part; one sub-resource
# keyed by `Content-ID` only.
mk "$WORK/corpus/nostart.mhtml" 'From: a@b\r\nSubject: no start\r\nMIME-Version: 1.0\r\nContent-Type: multipart/related; boundary="N"\r\nSnapshot-Content-Location: http://n/\r\n\r\n--N\r\nContent-Type: text/html\r\n\r\n<html><body>plain</body></html>\r\n--N\r\nContent-Type: application/octet-stream\r\nContent-Transfer-Encoding: base64\r\nContent-ID: <blob@n>\r\n\r\nAAECAwQ=\r\n--N--\r\n'
# envelope.mhtml: the MHTML signal is a `From`+`Subject` envelope with a `text/html`
# part (no Snapshot-Content-Location/Content-Base).
mk "$WORK/corpus/envelope.mhtml" 'From: a@b\r\nSubject: envelope signal\r\nMIME-Version: 1.0\r\nContent-Type: multipart/related; boundary="E"\r\n\r\n--E\r\nContent-Type: text/html\r\n\r\n<h1>env</h1>\r\n--E--\r\n'

# large.mhtml: a ~70 KB HTML root (7bit) with one base64 sub-resource.
{
    /usr/bin/printf '%b' 'From: a@b\r\nSubject: large\r\nMIME-Version: 1.0\r\nContent-Type: multipart/related; boundary="L"\r\nSnapshot-Content-Location: http://l/\r\n\r\n--L\r\nContent-Type: text/html\r\n\r\n<html><body>\r\n'
    i=0
    while [ "$i" -lt 4000 ]; do
        /usr/bin/printf '<p>line %s</p>\n' "$i"
        i=$((i + 1))
    done
    /usr/bin/printf '%b' '</body></html>\r\n--L\r\nContent-Type: application/octet-stream\r\nContent-Transfer-Encoding: base64\r\nContent-Location: http://l/blob.bin\r\n\r\nAAECAwQFBgc=\r\n--L--\r\n'
} > "$WORK/corpus/large.mhtml"

# --- CONTROLS (boundaries) --------------------------------------------------
# Plain prose: no format mark -> stays Opaque.
mk "$WORK/corpus/prose.txt" 'This is just some prose text.\nIt has several lines, with punctuation,\nbut no markup at all.\n'
# A plain HTML document (no MIME envelope) stays Html.
mk "$WORK/corpus/plain.html" '<!doctype html>\n<html><body><h1>Hi</h1></body></html>\n'
# A plain MIME message stays Eml.
mk "$WORK/corpus/plain.eml" 'From: a@b\r\nSubject: hi\r\nDate: Mon, 01 Jan 2029 00:00:00 +0000\r\n\r\na plain message body\r\n'
# A plain `multipart/related` **email** (no MHTML markers, no text/html part) stays Eml
# — it must never be mislabeled MHTML.
mk "$WORK/corpus/related.eml" 'From: a@b\r\nSubject: related email\r\nMIME-Version: 1.0\r\nContent-Type: multipart/related; boundary="r"\r\n\r\n--r\r\nContent-Type: text/plain\r\n\r\nplain part\r\n--r--\r\n'
# A `multipart/related` with a `text/html` part but no MHTML signal stays Eml.
mk "$WORK/corpus/related_no_marker.eml" 'MIME-Version: 1.0\r\nContent-Type: multipart/related; boundary="q"\r\n\r\n--q\r\nContent-Type: text/html\r\n\r\n<p>hi</p>\r\n--q--\r\n'

NORMAL="base.mhtml nostart.mhtml envelope.mhtml large.mhtml"
CONTROL="prose.txt plain.html plain.eml related.eml related_no_marker.eml"
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
usage_ok=0
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
        [ "$fmt" = "mhtml" ] || { echo "FINDING: $f detected as $fmt (want mhtml)" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"format":"mhtml"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata missing format" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"text"' "$RAW/$f.text.json" || { echo "FINDING: $f doc-text" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --mhtml-root --kind metadata \
            > "$RAW/$f.root.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"root_part":' "$RAW/$f.root.json" || { echo "FINDING: $f root" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --mhtml-root --kind text \
            > "$RAW/$f.root.text.json" 2>&1 || surface_fail=$((surface_fail + 1))

        "$BIN" observe --store "$WORK/store" --field "$field" --mhtml-location --kind metadata \
            > "$RAW/$f.location.json" 2>&1 || surface_fail=$((surface_fail + 1))

        "$BIN" observe --store "$WORK/store" --field "$field" --mhtml-find p --kind text \
            > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"matches":\[' "$RAW/$f.find.json" || { echo "FINDING: $f find" >&2; surface_fail=$((surface_fail + 1)); }

        # Common selectors: metadata/text/resource/search-match resolve.
        "$BIN" observe --store "$WORK/store" --field "$field" --text p --kind text \
            > "$RAW/$f.common.search.json" 2>&1 || surface_fail=$((surface_fail + 1))

        case "$f" in
            base.mhtml)
                grep -q '"resources":2' "$RAW/$f.metadata.json" || { echo "FINDING: base resources" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"has_snapshot_location":true' "$RAW/$f.metadata.json" || { echo "FINDING: base snapshot flag" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"has_content_base":true' "$RAW/$f.metadata.json" || { echo "FINDING: base content-base flag" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"root_part":1' "$RAW/$f.root.json" || { echo "FINDING: base root part" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --mhtml-resource 0 --kind metadata \
                    > "$RAW/$f.res0.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"location":"http://example.com/s.css"' "$RAW/$f.res0.json" || { echo "FINDING: base res0 location" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --mhtml-resource 1 --kind metadata \
                    > "$RAW/$f.res1.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"content_id":"<img1@x>"' "$RAW/$f.res1.json" || { echo "FINDING: base res1 content-id" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --mhtml-resource 1 --kind exact \
                    > "$RAW/$f.res1.bin" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"value_hex"' "$RAW/$f.res1.bin" || { echo "FINDING: base res1 exact" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --resource 0 --kind exact \
                    > "$RAW/$f.common.resource.bin" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"value_hex"' "$RAW/$f.common.resource.bin" || { echo "FINDING: base common resource" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"snapshot_content_location":"http://example.com/page.html"' "$RAW/$f.location.json" || { echo "FINDING: base snapshot value" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --mhtml-find stylesheet --kind text \
                    > "$RAW/$f.find.stylesheet.json" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"stylesheet"' "$RAW/$f.find.stylesheet.json" || { echo "FINDING: base find stylesheet" >&2; surface_fail=$((surface_fail + 1)); } ;;
            nostart.mhtml)
                grep -q '"resources":1' "$RAW/$f.metadata.json" || { echo "FINDING: nostart resources" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"root_part":1' "$RAW/$f.root.json" || { echo "FINDING: nostart root part" >&2; surface_fail=$((surface_fail + 1)); } ;;
            envelope.mhtml)
                grep -q '"has_snapshot_location":false' "$RAW/$f.metadata.json" || { echo "FINDING: envelope snapshot flag" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"has_from":true' "$RAW/$f.metadata.json" || { echo "FINDING: envelope from flag" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"has_subject":true' "$RAW/$f.metadata.json" || { echo "FINDING: envelope subject flag" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"has_html_part":true' "$RAW/$f.metadata.json" || { echo "FINDING: envelope html flag" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"resources":0' "$RAW/$f.metadata.json" || { echo "FINDING: envelope resources" >&2; surface_fail=$((surface_fail + 1)); } ;;
            large.mhtml)
                grep -q '"resources":1' "$RAW/$f.metadata.json" || { echo "FINDING: large resources" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q 'line 3999' "$RAW/$f.root.text.json" || { echo "FINDING: large root text" >&2; surface_fail=$((surface_fail + 1)); } ;;
        esac

        # H3: an out-of-range native resource and an unsupported common pair decline
        # typed (rc 6), once.
        if [ "$f" = "base.mhtml" ]; then
            set +e
            "$BIN" observe --store "$WORK/store" --field "$field" --mhtml-resource 999999 --kind metadata \
                > /dev/null 2>&1
            d_rc1=$?
            "$BIN" observe --store "$WORK/store" --field "$field" --table 0 --kind text \
                > /dev/null 2>&1
            d_rc2=$?
            set -e
            [ "$d_rc1" -eq 6 ] && [ "$d_rc2" -eq 6 ] && decline_ok=$((decline_ok + 1)) \
                || echo "FINDING: typed declines rc=$d_rc1/$d_rc2 (want 6/6)" >&2
            # A malformed native argument is a usage error (rc 2), once.
            set +e
            "$BIN" observe --store "$WORK/store" --field "$field" --mhtml-resource abc --kind metadata \
                > /dev/null 2>&1
            u_rc=$?
            set -e
            [ "$u_rc" -eq 2 ] && usage_ok=$((usage_ok + 1)) || echo "FINDING: malformed --mhtml-resource rc=$u_rc (want 2)" >&2
        fi
    else
        control_n=$((control_n + 1))
        case "$f" in
            prose.txt)
                [ "$fmt" = "opaque" ] || { echo "FINDING: $f detected as $fmt (want opaque)" >&2; surface_fail=$((surface_fail + 1)); }
                set +e
                "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
                    > "$RAW/$f.metadata.json" 2>&1
                control_rc=$?
                set -e
                [ "$control_rc" -eq 6 ] && opaque_ok=$((opaque_ok + 1)) || { echo "FINDING: opaque $f metadata rc=$control_rc (want 6)" >&2; }
                boundary_ok=$((boundary_ok + 1)) ;;
            plain.html)
                [ "$fmt" = "html" ] || { echo "FINDING: $f detected as $fmt (want html)" >&2; surface_fail=$((surface_fail + 1)); }
                boundary_ok=$((boundary_ok + 1)) ;;
            plain.eml)
                [ "$fmt" = "eml" ] || { echo "FINDING: $f detected as $fmt (want eml)" >&2; surface_fail=$((surface_fail + 1)); }
                boundary_ok=$((boundary_ok + 1)) ;;
            related.eml)
                [ "$fmt" = "eml" ] || { echo "FINDING: $f detected as $fmt (want eml)" >&2; surface_fail=$((surface_fail + 1)); }
                boundary_ok=$((boundary_ok + 1)) ;;
            related_no_marker.eml)
                [ "$fmt" = "eml" ] || { echo "FINDING: $f detected as $fmt (want eml)" >&2; surface_fail=$((surface_fail + 1)); }
                boundary_ok=$((boundary_ok + 1)) ;;
        esac
        # A common observation on an Opaque control declines typed (checked above).
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
    echo "# Phase 21.27.1 MHTML court — matrix"
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
    echo "mhtml_fixtures $normal_n"
    echo "control_fixtures $control_n"
    echo "exact_ok $exact_ok"
    echo "exact_fail $exact_fail"
    echo "surface_fail $surface_fail"
    echo "boundary_ok $boundary_ok"
    echo "opaque_controls_ok $opaque_ok"
    echo "typed_declines_ok $decline_ok"
    echo "usage_ok $usage_ok"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.27.1 — MHTML (MIME HTML) surface + exact closure + model",
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
    || [ "$boundary_ok" -ne 5 ] || [ "$opaque_ok" -ne 1 ] \
    || [ "$decline_ok" -ne 1 ] || [ "$usage_ok" -ne 1 ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.27.1 — MHTML (MIME HTML) surface + exact closure + model",
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
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion, for every fixture including the Opaque, Html, and four Eml controls",
  "mhtml": "a MIME multipart/related web archive that REUSES the EML adapter's bounded MIME layer (envelope, ordered parts, headers, exact spans) and the HTML adapter's bounded, error-recovering scanner (the root part), adding the ordered sub-resources keyed by Content-Location/Content-ID and the root HTML part (the start= Content-ID, else the first text/html part, else the first part); a Content-Transfer-Encoding value is decoded only for a derived observation and the encoded form's span is never substituted",
  "observations": ["metadata", "text", "resource", "search-match", "native mhtml-root", "native mhtml-resource", "native mhtml-location", "native mhtml-find"],
  "declines": "an out-of-range native resource declines typed (rc 6); an unsupported common pair (table) declines typed (rc 6); a malformed --mhtml-resource argument is a usage error (rc 2); plain prose stays opaque (rc 6), never a panic",
  "detection_boundary": "MHTML detection is tried BEFORE generic EML and requires a multipart/related MIME root plus an MHTML-SPECIFIC signal: a top-level Snapshot-Content-Location/Content-Base, or a From+Subject envelope with a text/html part. A plain multipart/related email (no markers / no text/html part), a plain MIME message, and a plain HTML document all decline here and stay Eml/Eml/Html.",
  "cannot_distinguish": "a plain multipart/related message that carries a text/html part AND a From+Subject envelope but is really an ordinary email is admitted as MHTML (the envelope signal is genuine, but the document could be a non-archived email); the detector does not perform Content-Base URL resolution and never rewrites Content-Location/Content-ID spellings; a start= value naming no part falls back to the first text/html part, then to the first part.",
  "precedence": "MHTML detection runs immediately before EML (EML is a weaker claim on a multipart/related message); before it run the strong magic-byte binaries (PDF/ZIP/Parquet/Arrow) and the JSON family",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 43 invalid-mhtml-structure",
  "adr_0060": "the MhtmlModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase21-27-1-mhtml-court.sh
# inside the court:
#   cargo build --locked --all-features
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--text PAT|--resource N|--mhtml-root|--mhtml-resource N|--mhtml-location|--mhtml-find PAT|--table N) --kind metadata|text|exact
#   rm source F.voldoc
#   target/debug/vole-document materialize --store STORE --field HEX --exact --output OUT ; cmp OUT source
# Regression controls (MHTML must not break EML/HTML):
#   docker compose run --rm --no-TTY doc-baseline sh tools/phase21-13-1-eml-court.sh
#   docker compose run --rm --no-TTY doc-baseline sh tools/phase21-10-1-html-court.sh
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
    echo "# Phase 21.27.1 — MHTML (MIME HTML) court"
    echo
    echo "**Question.** Does MHTML — a browser-saved MIME \`multipart/related\` web"
    echo "archive — close exactly and expose a representation-preserving model (the MIME"
    echo "envelope; the ordered parts with their exact spans; the ordered sub-resources"
    echo "keyed by \`Content-Location\`/\`Content-ID\`; the root HTML part parsed by the"
    echo "reused HTML scanner; quoted-printable/base64 parts decoded for observations"
    echo "while the raw encoded bytes stay in the source) on top of the whole-source exact"
    echo "leaf, while keeping an MHTML-**specific** detection boundary (a plain"
    echo "\`multipart/related\` email stays \`eml\`, a plain HTML document stays \`html\`,"
    echo "and plain prose stays \`opaque\`)?"
    echo
    echo "**Method.** Each self-authored fixture is generated in-court with GNU"
    echo "\`/usr/bin/printf\`, ingested via \`field-build\`, observed, then — after the"
    echo "**source file and the standalone descriptor are deleted** — rematerialized"
    echo "exactly in a fresh process and compared with \`length + SHA-256 + cmp\`. An"
    echo "out-of-range native resource and an unsupported common pair are required to"
    echo "decline typed; a malformed native argument is a usage error; the prose, HTML,"
    echo "plain MIME, and two \`multipart/related\` email controls pin the detection"
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
    echo "- **Shipped here:** byte-based, conservative, MHTML-specific detection before"
    echo "  EML; the reuse of the EML MIME layer and the HTML scanner; the ordered"
    echo "  sub-resource table; the canonical derived model; native \`mhtml-root\`/"
    echo "  \`mhtml-resource\`/\`mhtml-location\`/\`mhtml-find\`; common \`metadata\`/\`text\`/"
    echo "  \`resource\`/\`search-match\`."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the MHTML model"
    echo "  is derived (\`Q_gen\`) and never on the exactness path (ADR-0060: the model node"
    echo "  depends on the \`sha256(source)\` root, so no source-reading node aliases another"
    echo "  field's source)."
    echo "- **Encodings** are decoded only for a derived observation: the exact raw"
    echo "  (encoded) body span is retained, so the decoded bytes are never a substitute"
    echo "  for the encoded form (and vice versa)."
    echo "- **The boundary is honest:** a plain \`multipart/related\` message with a"
    echo "  \`From\`+\`Subject\` envelope and a \`text/html\` part is admitted as MHTML (the"
    echo "  envelope signal is genuine); it is a documented over-approximation. Content-Base"
    echo "  URL resolution is never performed."
    echo "- **Not claimed here:** a browser/rendering oracle, CSS/JS execution, or"
    echo "  sub-resource URL resolution."
    echo "- **Never run on the host:** every command above ran in the pinned \`dev\`"
    echo "  container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" != "PASS" ]; then
    echo "PHASE 21.27.1 MHTML COURT: FAIL" >&2
    exit 1
fi
echo "PHASE 21.27.1 MHTML COURT: PASS — campaign $CAMPAIGN"
