#!/bin/sh
# Phase 26.4 — legacy Outlook message (`.msg`, MS-OXMSG) semantic surface + exact closure.
#
# Pre-registered (Phase-26 subphase 26.4.1). Hypotheses:
#
#   H1 (byte-exactness) — for every normal fixture, `materialize(field) == source`
#       (length + SHA-256 + cmp) after the source file AND the standalone descriptor
#       are deleted, in a fresh process. The court FAILS unless this is 100% on every
#       normal fixture.
#   H2 (surface + preserved representation) — the representation-preserving model is
#       exposed on top of the reused CFBF/OLE2 physical layer: the top-level message
#       `__properties_version1.0` header (`Reserved`, `NextRecipientId`,
#       `NextAttachmentId`, `RecipientCount`, `AttachmentCount`, and the trailing
#       `Reserved`) and its fixed-size 16-byte property entries (raw tag/id/type,
#       flags, value bytes, exact span, owner) in order; the
#       `__substg1.0_<PPPPTTTT>` substreams (`0x001F` UTF-16LE, `0x001E` ANSI/CP1252,
#       `0x0102` binary) with their exact spans and decoded text; and the recipient
#       (`__recip_version1.0_<id>`) / attachment (`__attach_version1.0_<id>`)
#       sub-storages with their own property lists and substreams (order and ids
#       preserved), plus native `msg-summary`/`msg-property`/`msg-recipient`/
#       `msg-attachment`/`msg-find` and common `metadata`/`text`/`search-match`.
#   H3 (declines + boundaries) — a `.msg` with a plausible root
#       `__properties_version1.0` but a malformed body (a short property stream, an
#       overrunning entry, a bad substream name, an unreferenced or unsupported
#       substream, a count mismatch, a sub-storage without its own property stream, a
#       non-decimal sub-storage id) declines typed (rc 55); a generic compound file,
#       and an OLE file whose root lacks the property stream, stay `cfbf` and a `msg`
#       selector declines typed (rc 6); prose stays `opaque` (rc 6). Never a panic.
#
# Dependency-free (the fixtures are written by a small shipped Rust writer;
# deliberately NO python3, NO Outlook/LibreOffice, NO external OLE tooling, NO
# network):
#   docker compose run --rm --no-TTY dev sh tools/phase26-4-1-msg-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase26-4-1-msg-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase26-4
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2

echo "=== generate fixtures (dependency-free .msg writer) ==="
cargo run --locked --all-features --example msg_fixtures -- "$WORK/corpus" 2>&1 | tail -1

# Fixture classes.
#   NORMAL   — valid `.msg`: H1 exactness + H2 surface.
#   DECLINE  — detects as `msg` but the model is structurally malformed (rc 55).
#   CFBF_CTL — a valid compound file that is NOT a message (format `cfbf`);
#              a msg selector declines typed (rc 6).
#   OPAQUE   — not a compound file at all; a msg selector declines typed (rc 6).
NORMAL="msg_valid.msg"
DECLINE="msg_short_props.msg msg_entry_overrun.msg msg_bad_substream_name.msg msg_unreferenced_substream.msg msg_unsupported_substream.msg msg_count_mismatch.msg msg_recip_no_props.msg msg_bad_recip_suffix.msg"
CFBF_CTL="generic_cfbf.bin msg_missing_props.msg"
OPAQUE="prose.txt"

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
opaque_ok=0
opaque_n=0
normal_n=0
run_ok=0
run_n=0
decline_after_detect_ok=0
decline_n=0
cfbf_ctl_ok=0
cfbf_ctl_n=0

for f in $DECLINE; do
    src="$WORK/ref/$f"
    cp "$WORK/corpus/$f" "$src"
    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    fmt="$(json_str "$build_json" format)"
    field="$(json_str "$build_json" field)"
    decline_n=$((decline_n + 1))
    [ "$fmt" = "msg" ] || { echo "FINDING: $f detected as $fmt (want msg)" >&2; surface_fail=$((surface_fail + 1)); }
    set +e
    "$BIN" observe --store "$WORK/store" --field "$field" --msg-summary --kind text \
        > "$RAW/$f.decline.txt" 2>&1
    ctl_rc=$?
    set -e
    if [ "$ctl_rc" -eq 55 ]; then decline_after_detect_ok=$((decline_after_detect_ok + 1)); else echo "FINDING: $f structural decline rc=$ctl_rc (want 55)" >&2; fi
    printf '%s\t%s\t-\t-\t-\t-\t-\t-\t-\t%s\n' "$f" "$fmt" "$ctl_rc" >> "$TSV"
done

for f in $CFBF_CTL; do
    src="$WORK/ref/$f"
    cp "$WORK/corpus/$f" "$src"
    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    fmt="$(json_str "$build_json" format)"
    field="$(json_str "$build_json" field)"
    cfbf_ctl_n=$((cfbf_ctl_n + 1))
    [ "$fmt" = "cfbf" ] || { echo "FINDING: CFBF control $f detected as $fmt (want cfbf)" >&2; surface_fail=$((surface_fail + 1)); }
    set +e
    "$BIN" observe --store "$WORK/store" --field "$field" --msg-summary --kind text \
        > "$RAW/$f.decline.txt" 2>&1
    ctl_rc=$?
    set -e
    if [ "$ctl_rc" -eq 6 ]; then cfbf_ctl_ok=$((cfbf_ctl_ok + 1)); else echo "FINDING: CFBF control $f msg selector rc=$ctl_rc (want 6)" >&2; fi
    printf '%s\t%s\t-\t-\t-\t-\t-\t-\t-\t%s\n' "$f" "$fmt" "$ctl_rc" >> "$TSV"
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
    "$BIN" observe --store "$WORK/store" --field "$field" --msg-summary --kind text \
        > "$RAW/$f.decline.txt" 2>&1
    ctl_rc=$?
    set -e
    if [ "$ctl_rc" -eq 6 ]; then opaque_ok=$((opaque_ok + 1)); else echo "FINDING: opaque control $f msg selector rc=$ctl_rc (want 6)" >&2; fi
    printf '%s\t%s\t-\t-\t-\t-\t-\t-\t-\t%s\n' "$f" "$fmt" "$ctl_rc" >> "$TSV"
done

for f in $NORMAL; do
    src="$WORK/ref/$f"
    cp "$WORK/corpus/$f" "$src"
    src_len="$(wc -c < "$src" | tr -d ' ')"
    src_sha="$(sha256sum "$src" | cut -d' ' -f1)"

    build_json="$("$BIN" field-build "$WORK/corpus/$f" --store "$WORK/store" \
        --voldoc "$WORK/$f.voldoc")"
    field="$(json_str "$build_json" field)"
    fmt="$(json_str "$build_json" format)"

    normal_n=$((normal_n + 1))
    [ "$fmt" = "msg" ] || { echo "FINDING: $f not detected as msg (fmt=$fmt)" >&2; surface_fail=$((surface_fail + 1)); }

    # H2: common metadata / text / search-match + native summary / property /
    # recipient / attachment / find.
    "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
        > "$RAW/$f.metadata.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --msg-summary --kind text \
        > "$RAW/$f.summary.txt" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --msg-property 0 --kind metadata \
        > "$RAW/$f.property0.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --msg-property 0 --kind exact \
        > "$RAW/$f.property0.bin" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --msg-recipient 0 --kind metadata \
        > "$RAW/$f.recipient0.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --msg-recipient 0 --kind exact \
        > "$RAW/$f.recipient0.bin" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --msg-attachment 0 --kind metadata \
        > "$RAW/$f.attachment0.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --msg-attachment 0 --kind text \
        > "$RAW/$f.attachment0.txt" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --msg-find "Hello" --kind metadata \
        > "$RAW/$f.find.json" 2>&1 || surface_fail=$((surface_fail + 1))
    "$BIN" observe --store "$WORK/store" --field "$field" --msg-text --kind text \
        > "$RAW/$f.text.txt" 2>&1 || surface_fail=$((surface_fail + 1))

    grep -q '"format":"msg"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata missing format" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"stream":"__properties_version1.0"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata stream != __properties_version1.0" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"version":"outlook-msg-ms-oxmsg"' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata version mismatch" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"nameid":true' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata nameid != true" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"properties":6' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata property count != 6" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"substreams":10' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata substream count != 10" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"recipients":1' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata recipient count != 1" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"attachments":1' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata attachment count != 1" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"text_units":99' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata text_units != 99" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"recipient_count":1' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata recipient_count != 1" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"attachment_count":1' "$RAW/$f.metadata.json" || { echo "FINDING: $f metadata attachment_count != 1" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"text":"outlook-msg-ms-oxmsg"' "$RAW/$f.summary.txt" || { echo "FINDING: $f summary text != outlook-msg-ms-oxmsg" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"tag_hex":"0x001A001F"' "$RAW/$f.property0.json" || { echo "FINDING: $f property0 tag != 0x001A001F" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"id":26' "$RAW/$f.property0.json" || { echo "FINDING: $f property0 id != 26" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"property":"message-class"' "$RAW/$f.property0.json" || { echo "FINDING: $f property0 name != message-class" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"type_name":"unicode"' "$RAW/$f.property0.json" || { echo "FINDING: $f property0 type != unicode" >&2; surface_fail=$((surface_fail + 1)); }
    # The property entry's exact bytes are 16 bytes beginning `1F 00 1A 00` (tag, LE).
    if grep -q '"bytes_len":16' "$RAW/$f.property0.bin" && grep -q '1f001a00' "$RAW/$f.property0.bin"; then
        :
    else
        echo "FINDING: $f property0 exact bytes are not the 16-byte entry" >&2
        surface_fail=$((surface_fail + 1))
    fi
    grep -q '"kind":"recipient"' "$RAW/$f.recipient0.json" || { echo "FINDING: $f recipient0 kind != recipient" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"id":1' "$RAW/$f.recipient0.json" || { echo "FINDING: $f recipient0 id != 1" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q 'Carol' "$RAW/$f.recipient0.json" || { echo "FINDING: $f recipient0 missing the decoded display name" >&2; surface_fail=$((surface_fail + 1)); }
    # The storage's exact bytes are its 128-byte directory entry.
    grep -q '"bytes_len":128' "$RAW/$f.recipient0.bin" || { echo "FINDING: $f recipient0 exact bytes are not 128 bytes" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"kind":"attachment"' "$RAW/$f.attachment0.json" || { echo "FINDING: $f attachment0 kind != attachment" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"properties":4' "$RAW/$f.attachment0.json" || { echo "FINDING: $f attachment0 property count != 4" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"type_name":"binary"' "$RAW/$f.attachment0.json" || { echo "FINDING: $f attachment0 missing a binary substream" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q '"matches":\[{' "$RAW/$f.find.json" || { echo "FINDING: $f find returned no matches" >&2; surface_fail=$((surface_fail + 1)); }
    # The decoded text carries UTF-16LE, CP1252, and recipient/attachment text.
    grep -q 'Hello' "$RAW/$f.text.txt" || { echo "FINDING: $f text missing the UTF-16LE subject" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q 'Caf' "$RAW/$f.text.txt" || { echo "FINDING: $f text missing the CP1252 sender" >&2; surface_fail=$((surface_fail + 1)); }
    grep -q 'Carol' "$RAW/$f.text.txt" || { echo "FINDING: $f text missing the recipient display name" >&2; surface_fail=$((surface_fail + 1)); }

    # H2: the ANSI sender decodes to its exact text (the CP1252 high byte maps to
    # U+20AC) and the attachment text is its decoded ANSI/Unicode run.
    run_n=$((run_n + 1))
    if grep -q 'Café €' "$RAW/$f.text.txt" && grep -q 'long file.txt' "$RAW/$f.attachment0.txt"; then
        run_ok=$((run_ok + 1))
    else
        echo "FINDING: $f decoded text did not decode as expected" >&2
        surface_fail=$((surface_fail + 1))
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

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$f" "$fmt" "$src_len" "$src_sha" "$out_len" "$out_sha" "$cmp_ok" "$exact" \
        "-" "-" >> "$TSV"
done

echo "=== exactness after source + descriptor deletion ==="
cat "$TSV"

fixtures_n=$((normal_n + opaque_n + cfbf_ctl_n))

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 26.4.1 legacy Outlook message (.msg) court — matrix"
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
    echo "runs_ok $run_ok"
    echo "runs_n $run_n"
    echo "structural_declines_ok $decline_after_detect_ok"
    echo "structural_declines $decline_n"
    echo "cfbf_controls_ok $cfbf_ctl_ok"
    echo "cfbf_controls $cfbf_ctl_n"
    echo "opaque_controls_ok $opaque_ok"
    echo "opaque_controls $opaque_n"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "26.4.1 — legacy Outlook message (.msg, MS-OXMSG) semantic surface + exact closure on the CFBF/OLE2 physical layer",
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
  "fixtures_tool": "examples/msg_fixtures.rs (dependency-free .msg/MS-OXMSG-in-CFBF writer; no python3, no Outlook/LibreOffice, no external OLE tooling)",
  "external_oracles": "none (the MSG model is validated internally; no external Outlook/OLE parser is invoked)",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
if [ "$exact_fail" -ne 0 ] || [ "$surface_fail" -ne 0 ] \
    || [ "$exact_ok" -ne "$normal_n" ] \
    || [ "$run_ok" -ne "$run_n" ] \
    || [ "$decline_after_detect_ok" -ne "$decline_n" ] \
    || [ "$cfbf_ctl_ok" -ne "$cfbf_ctl_n" ] \
    || [ "$opaque_ok" -ne "$opaque_n" ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "26.4.1 — legacy Outlook message (.msg, MS-OXMSG) semantic surface + exact closure on the CFBF/OLE2 physical layer",
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
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "fixtures": "$NORMAL $DECLINE $CFBF_CTL $OPAQUE",
  "fixture_tool": "examples/msg_fixtures.rs",
  "external_oracles": "none",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion",
  "observations": ["metadata", "text", "search-match", "native msg-summary", "native msg-property", "native msg-recipient", "native msg-attachment", "native msg-find"],
  "surface": "the top-level message __properties_version1.0 header (Reserved, NextRecipientId, NextAttachmentId, RecipientCount, AttachmentCount, trailing Reserved) and its fixed-size 16-byte property entries (raw tag/id/type, flags, value bytes, exact span, owner) in order; the __substg1.0_<PPPPTTTT> substreams (0x001F UTF-16LE, 0x001E ANSI/CP1252, 0x0102 binary) with their exact spans and decoded text; and the recipient (__recip_version1.0_<id>) / attachment (__attach_version1.0_<id>) sub-storages with their own property lists and substreams (order and ids preserved), on top of the reused CFBF/OLE2 physical layer",
  "declines": "a .msg with a plausible root __properties_version1.0 but a malformed body (a property stream shorter than its 32-byte header, a fixed entry that overruns the stream, a __substg1.0_* name that is not PPPPTTTT hex, an unreferenced substream, an unsupported substream type, a RecipientCount/AttachmentCount mismatch, a sub-storage without its own property stream, a non-decimal sub-storage id) detects as msg and its model declines typed InvalidMsgStructure (rc 55); a generic compound file and an OLE file whose root lacks the property stream stay cfbf and a msg selector declines typed (rc 6); prose stays opaque (rc 6); never a panic",
  "precedence": "the MSG detector is built on top of the CFBF detector and runs after the CFBF physical layer and the Word/Excel/PowerPoint detectors and before the generic Cfbf claim, so a .msg becomes Msg while a generic compound file stays Cfbf; a non-CFBF source (including a .pptx OPC ZIP) is unaffected",
  "rc_codes": "0 ok; 6 unsupported-feature; 2 usage; 8 resource-limit; 55 invalid-msg-structure",
  "property_stream": "the top-level message __properties_version1.0 is a 32-byte header (Reserved(8), NextRecipientId(4), NextAttachmentId(4), RecipientCount(4), AttachmentCount(4), Reserved(8)) followed by 16-byte fixed entries (PropertyTag(4) = 0xPPPPTTTT, Flags(4), Value(8)); a sub-storage __properties_version1.0 is an 8-byte Reserved header followed by the same 16-byte entries",
  "adr_0060": "the MsgModel node depends on the exact source root (DocumentExact, keyed by sha256(source)); no source-reading node aliases another field's source",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase26-4-1-msg-court.sh
# inside the court:
#   cargo build --locked --all-features
#   cargo run --locked --all-features --example msg_fixtures -- CORPUS
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX
#       (--metadata|--msg-text|--msg-summary|--msg-property 0|--msg-recipient 0|--msg-attachment 0|--msg-find PAT)
#       --kind metadata|text|exact
#   rm source F.voldoc
#   target/debug/vole-document materialize --store STORE --field HEX --exact --output OUT ; cmp OUT source
# Full gate (dev service):
#   cargo fmt --all --check
#   cargo clippy --all-targets --all-features -- -D warnings
#   cargo test --locked --all-features   (use CARGO_PROFILE_TEST_DEBUG=0 if the all-features link OOM-kills)
#   cargo test --locked --features msg
#   CFBF regression control: sh tools/phase21-35-1-cfbf-court.sh
#   DOC regression control: sh tools/phase26-1-1-doc-court.sh
#   XLS regression control: sh tools/phase26-2-1-xls-court.sh
#   PPT regression control: sh tools/phase26-3-1-ppt-court.sh
#   MSRV: docker compose run --rm --no-TTY msrv cargo build --locked --all-features
EOF

echo "=== verdict: $verdict ==="
echo "receipt: $CAMPAIGN/receipt.json"
[ "$verdict" = "PASS" ]
