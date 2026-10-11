#!/bin/sh
# Phase 21.28 — the syslog / log-stream adapter court.
#
# Pre-registered (Phase-21 subphase 21.28). Hypotheses:
#
#   H1 (byte-exactness) — for every fixture (including the Opaque, Json, Yaml, and
#       Jsonl controls), `materialize(field) == source` (length + SHA-256 + cmp)
#       after the source file AND the standalone descriptor are deleted, in a fresh
#       process. The court FAILS unless this is 100% on every fixture.
#   H2 (surface + preserved representation) — the representation-preserving
#       log-stream model is exposed: each record's recorded dialect (RFC 5424 / RFC
#       3164 / generic), its exact line span and terminator (LF/CRLF/none), its
#       decoded priority (facility/severity) and deepest structured-data nesting, and
#       every field's exact span (PRI/version/timestamp/hostname/app-name/procid/
#       msgid/structured-data element/id/parameter-name/parameter-value/tag/pid/
#       level/msg). Structured-data escaping, NILVALUE (`-`), and timestamp/level
#       spelling are preserved verbatim. Common metadata/text/search-match and native
#       logstream-line/logstream-field/logstream-find answer.
#   H3 (declines + boundaries) — an out-of-range record and an unsupported common
#       pair decline typed (rc 6), never a silent empty answer; a malformed
#       `--logstream-field` reference is a usage error (rc 2). The boundary controls
#       pin detection: plain prose stays `opaque`; a single JSON value stays `json`;
#       a JSONL stream stays `jsonl` (a native logstream selector on it declines typed,
#       rc 6); a BSD syslog stream whose every message lacks a `: ` is a sequence of
#       YAML mappings and honestly stays `yaml` (never stolen by logstream).
#
# Runs in the pinned `dev` service using only POSIX `sh`, coreutils, git, and the
# shipped binary (no python3, no jq):
#   docker compose run --rm --no-TTY dev sh tools/phase21-28-1-logstream-court.sh

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
CAMPAIGN="evidence/campaigns/${STAMP}-phase21-28-1-logstream-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase21-28-1
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
if ! cargo build --locked --all-features > "$RAW/build.log" 2>&1; then
    tail -40 "$RAW/build.log" >&2
    echo "PHASE 21.28.1 LOG-STREAM COURT: FAIL — all-features build failed" >&2
    exit 1
fi
tail -2 "$RAW/build.log"

echo "=== generate fixtures (POSIX printf only) ==="
mk() { /usr/bin/printf '%b' "$2" > "$1"; }

# --- NORMAL: log streams ----------------------------------------------------
# generic.log: a timestamp+level line, a bracketed-timestamp+level line, a bare
# level line (raw remainder kept verbatim).
mk "$WORK/corpus/generic.log" '2026-10-11T12:34:56Z INFO starting up\n[2026-10-11 12:00:00] ERROR boom: x\nDEBUG raw remainder kept\n'
# rfc5424.log: a HEADER + structured-data element with an escaped quote (k="a b"
# x="q\"r"), then a second record with NILVALUE structured-data (`-`).
mk "$WORK/corpus/rfc5424.log" '<165>1 2026-10-11T12:34:56.789Z host app 1234 ID47 [ex@1 k="a b" x="q\\"r"] hello world\n<165>1 2026-10-11T12:34:57Z host app 1234 ID48 - second\n'
# rfc3164.log: a single-space day, a double-space day, TAG[pid], and messages that
# contain `: ` (which keeps the YAML detector from reading the lines as mappings).
mk "$WORK/corpus/rfc3164.log" '<34>Oct 11 22:14:15 mymachine su[123]: su root failed: reason\n<34>Oct  1 01:02:03 h x: y: z\n'
# crlf.log: CRLF terminators.
mk "$WORK/corpus/crlf.log" 'INFO a tail\r\nWARN b tail\r\n'
# blank.log: a leading blank line and interior blank lines.
mk "$WORK/corpus/blank.log" '\nINFO first\n\n\nERROR last\n'

# --- CONTROLS (boundaries) --------------------------------------------------
# Plain prose: no format mark -> stays Opaque.
mk "$WORK/corpus/prose.txt" 'This is just prose text.\nIt has several lines, with punctuation,\nbut no log structure at all.\n'
# A single JSON value stays Json.
mk "$WORK/corpus/single.json" '{"a":1}\n'
# A newline-separated JSON stream stays Jsonl.
mk "$WORK/corpus/stream.jsonl" '{"a":1}\n{"b":2}\n'
# A BSD syslog stream whose every message lacks a `: ` is a sequence of YAML
# mappings and honestly stays Yaml (logstream never steals it).
mk "$WORK/corpus/bsd_nocolon.log" '<34>Oct 11 22:14:15 host app[1]: aaa\n<34>Oct 11 22:14:16 host app[2]: bbb\n'

NORMAL="generic.log rfc5424.log rfc3164.log crlf.log blank.log"
CONTROL="prose.txt single.json stream.jsonl bsd_nocolon.log"
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
jsonl_ok=0
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
        [ "$fmt" = "logstream" ] || { echo "FINDING: $f detected as $fmt (want logstream)" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"format":"logstream"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata format" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --doc-text --kind text \
            > "$RAW/$f.text.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"text"' "$RAW/$f.text.json" || { echo "FINDING: $f doc-text" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --logstream-line 0 --kind metadata \
            > "$RAW/$f.line.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"dialect":' "$RAW/$f.line.json" || { echo "FINDING: $f line dialect" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --logstream-line 0 --kind exact \
            > "$RAW/$f.line.bin" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"value_hex"' "$RAW/$f.line.bin" || { echo "FINDING: $f line exact" >&2; surface_fail=$((surface_fail + 1)); }

        "$BIN" observe --store "$WORK/store" --field "$field" --logstream-find a --kind text \
            > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))
        grep -q '"matches":\[' "$RAW/$f.find.json" || { echo "FINDING: $f find" >&2; surface_fail=$((surface_fail + 1)); }

        # Common selectors: metadata/text/search-match resolve.
        "$BIN" observe --store "$WORK/store" --field "$field" --text a --kind text \
            > "$RAW/$f.common.search.json" 2>&1 || surface_fail=$((surface_fail + 1))

        case "$f" in
            generic.log)
                grep -q '"records":3' "$RAW/$f.metadata.json" || { echo "FINDING: generic records" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"generic":3' "$RAW/$f.metadata.json" || { echo "FINDING: generic dialect count" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"dialect":"generic"' "$RAW/$f.line.json" || { echo "FINDING: generic line dialect" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --logstream-field 0:level --kind exact \
                    > "$RAW/$f.level.bin" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"value_hex":"494e464f"' "$RAW/$f.level.bin" || { echo "FINDING: generic level exact (want INFO)" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q 'starting up' "$RAW/$f.text.json" || { echo "FINDING: generic doc-text" >&2; surface_fail=$((surface_fail + 1)); } ;;
            rfc5424.log)
                grep -q '"rfc5424":2' "$RAW/$f.metadata.json" || { echo "FINDING: 5424 dialect count" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"pri_records":2' "$RAW/$f.metadata.json" || { echo "FINDING: 5424 pri count" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"sd_elements":1' "$RAW/$f.metadata.json" || { echo "FINDING: 5424 sd count" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"dialect":"rfc5424"' "$RAW/$f.line.json" || { echo "FINDING: 5424 line dialect" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"pri":165' "$RAW/$f.line.json" || { echo "FINDING: 5424 pri" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"role":"structured-data"' "$RAW/$f.line.json" || { echo "FINDING: 5424 structured-data span" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --logstream-field 0:sd-element --kind exact \
                    > "$RAW/$f.sd.bin" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q 'role=sd-element' "$RAW/$f.sd.bin" || { echo "FINDING: 5424 sd-element exact" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"value_hex":"5b65784031' "$RAW/$f.sd.bin" || { echo "FINDING: 5424 sd-element bytes (want [ex@1)" >&2; surface_fail=$((surface_fail + 1)); } ;;
            rfc3164.log)
                grep -q '"rfc3164":2' "$RAW/$f.metadata.json" || { echo "FINDING: 3164 dialect count" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"pri_records":2' "$RAW/$f.metadata.json" || { echo "FINDING: 3164 pri count" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"dialect":"rfc3164"' "$RAW/$f.line.json" || { echo "FINDING: 3164 line dialect" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"pri":34' "$RAW/$f.line.json" || { echo "FINDING: 3164 pri" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"facility":4' "$RAW/$f.line.json" || { echo "FINDING: 3164 facility" >&2; surface_fail=$((surface_fail + 1)); }
                "$BIN" observe --store "$WORK/store" --field "$field" --logstream-field 0:pid --kind exact \
                    > "$RAW/$f.pid.bin" 2>&1 || surface_fail=$((surface_fail + 1))
                grep -q '"value_hex":"313233"' "$RAW/$f.pid.bin" || { echo "FINDING: 3164 pid exact (want 123)" >&2; surface_fail=$((surface_fail + 1)); } ;;
            crlf.log)
                grep -q '"crlf_records":2' "$RAW/$f.metadata.json" || { echo "FINDING: crlf count" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"terminator":"crlf"' "$RAW/$f.line.json" || { echo "FINDING: crlf terminator" >&2; surface_fail=$((surface_fail + 1)); } ;;
            blank.log)
                grep -q '"records":2' "$RAW/$f.metadata.json" || { echo "FINDING: blank records" >&2; surface_fail=$((surface_fail + 1)); }
                grep -q '"blank_lines":3' "$RAW/$f.metadata.json" || { echo "FINDING: blank_lines" >&2; surface_fail=$((surface_fail + 1)); } ;;
        esac

        # H3: an out-of-range record and an unsupported common pair decline typed
        # (rc 6), once; a malformed field reference is a usage error (rc 2), once.
        if [ "$f" = "generic.log" ]; then
            set +e
            "$BIN" observe --store "$WORK/store" --field "$field" --logstream-line 999999 --kind metadata \
                > /dev/null 2>&1
            d_rc1=$?
            "$BIN" observe --store "$WORK/store" --field "$field" --table 0 --kind text \
                > /dev/null 2>&1
            d_rc2=$?
            "$BIN" observe --store "$WORK/store" --field "$field" --logstream-field 0:nope --kind metadata \
                > /dev/null 2>&1
            u_rc=$?
            set -e
            [ "$d_rc1" -eq 6 ] && [ "$d_rc2" -eq 6 ] && decline_ok=$((decline_ok + 1)) \
                || echo "FINDING: typed declines rc=$d_rc1/$d_rc2 (want 6/6)" >&2
            [ "$u_rc" -eq 2 ] && usage_ok=$((usage_ok + 1)) || echo "FINDING: malformed --logstream-field rc=$u_rc (want 2)" >&2
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
            single.json)
                [ "$fmt" = "json" ] || { echo "FINDING: $f detected as $fmt (want json)" >&2; surface_fail=$((surface_fail + 1)); }
                boundary_ok=$((boundary_ok + 1)) ;;
            stream.jsonl)
                [ "$fmt" = "jsonl" ] || { echo "FINDING: $f detected as $fmt (want jsonl)" >&2; surface_fail=$((surface_fail + 1)); }
                set +e
                "$BIN" observe --store "$WORK/store" --field "$field" --logstream-line 0 --kind metadata \
                    > "$RAW/$f.logstream-decline.json" 2>&1
                control_rc=$?
                set -e
                [ "$control_rc" -eq 6 ] && jsonl_ok=$((jsonl_ok + 1)) || { echo "FINDING: jsonl logstream-line rc=$control_rc (want 6)" >&2; }
                boundary_ok=$((boundary_ok + 1)) ;;
            bsd_nocolon.log)
                [ "$fmt" = "yaml" ] || { echo "FINDING: $f detected as $fmt (want yaml)" >&2; surface_fail=$((surface_fail + 1)); }
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
    echo "# Phase 21.28.1 log-stream court — matrix"
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
    echo "logstream_fixtures $normal_n"
    echo "control_fixtures $control_n"
    echo "exact_ok $exact_ok"
    echo "exact_fail $exact_fail"
    echo "surface_fail $surface_fail"
    echo "boundary_ok $boundary_ok"
    echo "opaque_controls_ok $opaque_ok"
    echo "jsonl_control_ok $jsonl_ok"
    echo "typed_declines_ok $decline_ok"
    echo "usage_ok $usage_ok"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.28.1 — syslog / log-stream surface + exact closure + model",
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
    || [ "$boundary_ok" -ne 4 ] || [ "$opaque_ok" -ne 1 ] || [ "$jsonl_ok" -ne 1 ] \
    || [ "$decline_ok" -ne 1 ] || [ "$usage_ok" -ne 1 ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "21.28.1 — syslog / log-stream surface + exact closure + model",
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
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion, for every fixture including the Opaque, Json, Yaml, and Jsonl controls",
  "logstream": "a bounded, dependency-free line/event-stream adapter with a per-record RECORDED dialect: RFC 5424 syslog (<PRI>VERSION TIMESTAMP HOSTNAME APP-NAME PROCID MSGID STRUCTURED-DATA [MSG], with structured-data elements whose escaped quote, backslash, and bracket spellings are preserved, and NILVALUE preserved), RFC 3164 (BSD) syslog (<PRI>Mmm dd hh:mm:ss HOSTNAME TAG[pid]: MSG, timestamp spelling preserved), and a generic application log line (optional leading timestamp and/or level token); every record keeps its exact line span/terminator, priority (facility x 8 + severity), deepest structured-data nesting, and every field's exact span; a message is never normalized",
  "observations": ["metadata", "text", "search-match", "native logstream-line", "native logstream-field", "native logstream-find"],
  "declines": "an out-of-range record declines typed (rc 6); an unsupported common pair (table) declines typed (rc 6); a malformed --logstream-field reference is a usage error (rc 2); plain prose stays opaque (rc 6), never a panic",
  "detection_boundary": "log-stream detection is byte-based and conservative (at least two non-blank lines; EVERY non-blank line must match a dialect) and is tried AFTER every higher-priority structured/tabular/prose format (PDF/ZIP/Parquet/Arrow, the JSON family, MHTML/EML, YAML/TOML, config, CSV, MDX/Markdown/RST/AsciiDoc) and BEFORE the maximally ambiguous fixed-width heuristic. A single log-looking line inside prose does NOT claim the file.",
  "cannot_distinguish": "a prose file whose EVERY line begins with a level word (e.g. all lines start with INFO) is byte-for-byte indistinguishable from an application log and IS claimed; a lone leading ISO timestamp is accepted (so a file whose every line begins with a date is claimed); and a pure BSD syslog stream whose every message lacks a colon-space is a sequence of YAML key-colon-value mappings and stays yaml (honest over-claim by the earlier YAML detector, never stolen by logstream). Logstream never consults a file name.",
  "precedence": "log-stream detection runs after all structured/tabular/prose detectors (so JSON/JSONL/YAML/TOML/CSV/config/prose are never stolen) and before fixed-width",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 44 invalid-logstream-structure",
  "adr_0060": "the LogstreamModel node depends on the DocumentExact root (sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase21-28-1-logstream-court.sh
# inside the court:
#   cargo build --locked --all-features
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX (--metadata|--doc-text|--text PAT|--logstream-line N|--logstream-field N:ROLE|--logstream-find PAT|--table N) --kind metadata|text|exact
#   rm source F.voldoc
#   target/debug/vole-document materialize --store STORE --field HEX --exact --output OUT ; cmp OUT source
# Regression control (logstream must not break JSONL):
#   docker compose run --rm --no-TTY dev sh tools/phase21-12-1-jsonl-court.sh
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
    echo "# Phase 21.28.1 — syslog / log-stream adapter court"
    echo
    echo "**Question.** Does a log stream — RFC 5424 syslog, RFC 3164 (BSD) syslog, or a"
    echo "generic application log line — close exactly and expose a representation-"
    echo "preserving per-record model (each record's recorded dialect, exact line span and"
    echo "terminator, decoded priority, deepest structured-data nesting, and every field's"
    echo "exact span, with messages/NILVALUE/timestamp/level spelling preserved) on top of"
    echo "the whole-source exact leaf, while keeping a conservative detection boundary"
    echo "(prose stays \`opaque\`, a JSON value stays \`json\`, a JSONL stream stays"
    echo "\`jsonl\`, and a pure BSD \`key: value\` stream honestly stays \`yaml\`)?"
    echo
    echo "**Method.** Each self-authored fixture is generated in-court with GNU"
    echo "\`/usr/bin/printf\`, ingested via \`field-build\`, observed, then — after the"
    echo "**source file and the standalone descriptor are deleted** — rematerialized"
    echo "exactly in a fresh process and compared with \`length + SHA-256 + cmp\`. An"
    echo "out-of-range record and an unsupported common pair are required to decline typed;"
    echo "a malformed native field reference is a usage error; the prose, single-JSON,"
    echo "JSONL, and BSD-no-colon controls pin the detection boundaries. The court runs in"
    echo "the pinned \`dev\` service using only POSIX \`sh\`, coreutils, git, and the shipped"
    echo "binary (no python3, no jq)."
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
    echo "- **Shipped here:** byte-based, conservative log-stream detection (three"
    echo "  dialects); the bounded, span-preserving per-record model; native"
    echo "  \`logstream-line\`/\`logstream-field\`/\`logstream-find\`; common \`metadata\`/"
    echo "  \`text\`/\`search-match\`."
    echo "- **Precedence:** logstream is tried **after** every structured/tabular/prose"
    echo "  format (so JSON/JSONL/YAML/TOML/CSV/config/prose are never stolen) and"
    echo "  **before** the maximally ambiguous fixed-width heuristic."
    echo "- **Boundary (honest):** a prose file whose *every* line begins with a level"
    echo "  word is indistinguishable from a log and is claimed; a single log-looking line"
    echo "  inside prose is not claimed; and a pure BSD stream whose every message lacks a"
    echo "  \`: \` is a YAML mapping sequence and stays \`yaml\`."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the log-stream"
    echo "  model is derived (\`Q_gen\`) and never on the exactness path (ADR-0060: the"
    echo "  model node depends on the \`sha256(source)\` root)."
    echo "- **Not claimed here:** an economic court, a syslog daemon, or any rendering."
    echo "- **Never run on the host:** every command above ran in the pinned \`dev\`"
    echo "  container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" != "PASS" ]; then
    echo "PHASE 21.28.1 LOG-STREAM COURT: FAIL" >&2
    exit 1
fi
echo "PHASE 21.28.1 LOG-STREAM COURT: PASS — campaign $CAMPAIGN"
