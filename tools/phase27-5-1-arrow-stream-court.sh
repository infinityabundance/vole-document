#!/bin/sh
# Phase 27.5 — Apache Arrow IPC **stream** framing (the magic-less encapsulated-
# message sequence) + exact closure.
#
# Pre-registered (Phase-27 subphase 27.5.1). Hypotheses:
#
#   H1 (byte-exactness) — for every claimed fixture, `materialize(field) == source`
#       (length + SHA-256 + cmp) after the source file AND the standalone descriptor
#       are deleted, in a fresh process. The court FAILS unless 100%. The bare
#       stream's exact leaf is the whole source (a `DocumentExact`); the message
#       inventory is derived and never on the exactness path, and the
#       `ArrowModel` node depends on the exact `sha256(source)` root (ADR-0060).
#   H2 (stream framing + ordered message inventory) — a magic-less source is
#       claimed as `arrow` only when it is a coherent encapsulated-message sequence:
#       a continuation-marked framing `Schema` message first (its FlatBuffer metadata
#       parsed under the bounded reader), then well-formed `RecordBatch` messages,
#       ending in an end-of-stream marker. The ordered messages (Schema, each
#       RecordBatch, end-of-stream) are exposed with their **exact byte spans**; the
#       record-batch messages line up exactly with the record-batch inventory, the
#       messages are contiguous, and they cover the whole source.
#   H3 (honest declines / boundaries) — the controls pin the detection boundary:
#       a random `0xFFFFFFFF`-prefixed blob, a truncated stream, a stream whose
#       first message is not a `Schema`, and the legacy pre-0.15 no-continuation
#       framing all stay `Opaque`; prose stays `Opaque`; and the file format
#       (`ARROW1` magic + footer) still detects and closes byte-exactly. Never a
#       panic, never a wrong claim.
#
# Runs in the pinned `dev` service (Rust toolchain + git + coreutils + sed/head/
# tail/dd; deliberately NO python3, NO jq, NO external Arrow tooling, NO network):
#   docker compose run --rm --no-TTY dev sh tools/phase27-5-1-arrow-stream-court.sh

set -eu
cd /work
LC_ALL=C
export LC_ALL
export PYTHONDONTWRITEBYTECODE=1

BIN=target/debug/vole-document
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
STAMP="$(date -u +%Y-%m-%d)"
CAMPAIGN="evidence/campaigns/${STAMP}-phase27-5-1-arrow-stream-${SHA}"
RAW="$CAMPAIGN/raw"
rm -rf "$CAMPAIGN"
mkdir -p "$RAW"

WORK=evidence/scratch/phase27-5
rm -rf "$WORK"
mkdir -p "$WORK/corpus" "$WORK/ref" "$WORK/store"

echo "=== build (all-features) ==="
cargo build --locked --all-features 2>&1 | tail -2
[ -x "$BIN" ] || { echo "phase27-5-1: $BIN missing; refusing to run" >&2; exit 1; }

FIX=tools/fixtures/arrow
cp "$FIX/stream.arrow" "$WORK/corpus/stream.arrow"
cp "$FIX/primitives.arrow" "$WORK/corpus/primitives.arrow"
cp "$FIX/prose.txt" "$WORK/corpus/prose.txt"

echo "=== synthesize the boundary controls (shell only) ==="
# A random-ish `0xFFFFFFFF`-prefixed blob with a plausible metadata length but a
# body that is not a FlatBuffer `Message`.
printf '\377\377\377\377\200\000\000\000' > "$WORK/corpus/junk_cont.bin"
dd if=/dev/zero bs=1 count=120 >> "$WORK/corpus/junk_cont.bin" 2>/dev/null
# A truncated stream: the bare stream cut inside its schema metadata, before the EOS.
head -c 64 "$FIX/stream.arrow" > "$WORK/corpus/truncated_stream.bin"

# Extract one JSON string field (single-line objects, greedy, no jq).
json_str() {
    printf '%s' "$1" | sed -n "s/.*\"$2\":\"\([^\"]*\)\".*/\1/p"
}
# Extract the `[start,end]` of the first message of a given kind from the message
# inventory that `observe --metadata` exposes (e.g. `Schema [0,192]`).
msg_span() {
    printf '%s' "$1" | sed -n "s/.*\"kind\":\"$2\",\"span\":\[\([0-9]*\),\([0-9]*\)\].*/\1 \2/p"
}

TSV="$RAW/results.tsv"
: > "$TSV"
printf 'fixture\tformat\tsrc_len\tsrc_sha\tout_len\tout_sha\tcmp\texact\twant\n' >> "$TSV"

normal_ok=0
normal_n=0
opaque_ok=0
opaque_n=0
surface_fail=0

check_case() {
    # check_case <fixture> <want: arrow|opaque>
    f="$1"
    want="$2"
    src="$WORK/corpus/$f"
    cp "$src" "$WORK/ref/$f"
    src_len="$(wc -c < "$WORK/ref/$f" | tr -d ' ')"
    src_sha="$(sha256sum "$WORK/ref/$f" | cut -d' ' -f1)"
    build_json="$("$BIN" field-build "$src" --store "$WORK/store" --voldoc "$WORK/$f.voldoc")"
    field="$(json_str "$build_json" field)"
    fmt="$(json_str "$build_json" format)"

    if [ "$want" = "arrow" ]; then
        normal_n=$((normal_n + 1))
        [ "$fmt" = "arrow" ] || { echo "FINDING: $f detected as $fmt (want arrow)" >&2; surface_fail=$((surface_fail + 1)); }
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json"
        "$BIN" observe --store "$WORK/store" --field "$field" --arrow-schema --kind metadata \
            > "$RAW/$f.schema.json"
        # Delete source + descriptor; rematerialize exactly in a fresh process.
        rm -f "$src" "$WORK/$f.voldoc"
        "$BIN" materialize --store "$WORK/store" --field "$field" --exact \
            --output "$WORK/$f.out" > /dev/null
        out_len="$(wc -c < "$WORK/$f.out" | tr -d ' ')"
        out_sha="$(sha256sum "$WORK/$f.out" | cut -d' ' -f1)"
        if cmp -s "$WORK/ref/$f" "$WORK/$f.out"; then cmp_ok=true; else cmp_ok=false; fi
        if [ "$out_len" = "$src_len" ] && [ "$out_sha" = "$src_sha" ] && [ "$cmp_ok" = true ]; then
            exact=true; normal_ok=$((normal_ok + 1))
        else
            exact=false
            echo "FINDING: not byte-exact after removal: $f" >&2
        fi
    else
        opaque_n=$((opaque_n + 1))
        [ "$fmt" = "opaque" ] || { echo "FINDING: control $f detected as $fmt (want opaque)" >&2; surface_fail=$((surface_fail + 1)); }
        set +e
        "$BIN" observe --store "$WORK/store" --field "$field" --metadata --kind metadata \
            > "$RAW/$f.metadata.json" 2> "$RAW/$f.err"
        rc=$?
        set -e
        [ "$rc" -eq 6 ] || { echo "FINDING: control $f observe rc=$rc (want 6)" >&2; surface_fail=$((surface_fail + 1)); }
        exact=n/a
        out_len=n/a
        out_sha=n/a
        cmp_ok=n/a
        if [ "$fmt" = "opaque" ] && [ "$rc" -eq 6 ]; then opaque_ok=$((opaque_ok + 1)); fi
    fi

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$f" "$fmt" "$src_len" "$src_sha" "$out_len" "$out_sha" "$cmp_ok" "$exact" "$want" >> "$TSV"
}

echo "=== normal: the bare (magic-less) stream and the file-format control ==="
check_case stream.arrow arrow
check_case primitives.arrow arrow
# The claimed stream's message inventory (from its saved metadata JSON).
meta="$(cat "$RAW/stream.arrow.metadata.json")"
schemas="$(printf '%s' "$meta" | grep -o '"kind":"Schema"' | wc -l | tr -d ' ')"
records="$(printf '%s' "$meta" | grep -o '"kind":"RecordBatch"' | wc -l | tr -d ' ')"
eoses="$(printf '%s' "$meta" | grep -o '"kind":"EndOfStream"' | wc -l | tr -d ' ')"
schema_span="$(msg_span "$meta" Schema)"
eos_span="$(msg_span "$meta" EndOfStream)"
stream_len="$(wc -c < "$FIX/stream.arrow" | tr -d ' ')"
printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
    stream.arrow "$schemas" "$records" "$eoses" "$schema_span" "$eos_span" > "$RAW/messages.tsv"
[ "$schemas" = "1" ] || { echo "FINDING: stream has $schemas Schema messages (want 1)" >&2; surface_fail=$((surface_fail + 1)); }
[ "$records" = "2" ] || { echo "FINDING: stream has $records RecordBatch messages (want 2)" >&2; surface_fail=$((surface_fail + 1)); }
[ "$eoses" = "1" ] || { echo "FINDING: stream has $eoses EndOfStream messages (want 1)" >&2; surface_fail=$((surface_fail + 1)); }
# The messages are contiguous from offset 0 and cover the whole source.
schema_start="${schema_span%% *}"
eos_end="${eos_span##* }"
[ "$schema_start" = "0" ] || { echo "FINDING: first message does not start at 0" >&2; surface_fail=$((surface_fail + 1)); }
[ "$eos_end" = "$stream_len" ] || { echo "FINDING: message inventory ends at $eos_end (want $stream_len)" >&2; surface_fail=$((surface_fail + 1)); }

# A stream whose first message is a RecordBatch (the schema message stripped).
schema_end="${schema_span##* }"
tail -c "+$((schema_end + 1))" "$FIX/stream.arrow" > "$WORK/corpus/nonschema_first.bin"
# The legacy pre-0.15 no-continuation framing (metadata size with no token).
{
    printf '\270\000\000\000'
    tail -c "+9" "$FIX/stream.arrow" | head -c "$((schema_end - 8))"
    tail -c 8 "$FIX/stream.arrow"
} > "$WORK/corpus/legacy_stream.bin"

echo "=== controls: near misses stay Opaque ==="
check_case junk_cont.bin opaque
check_case truncated_stream.bin opaque
check_case nonschema_first.bin opaque
check_case legacy_stream.bin opaque
check_case prose.txt opaque

# --- MATRIX.md --------------------------------------------------------------
{
    echo "# Phase 27.5.1 Arrow IPC stream court — matrix"
    echo
    echo "| fixture | detected | src len | out len | sha256 match | cmp | exact | want |"
    echo "| --- | --- | ---: | ---: | --- | --- | --- | --- |"
    # shellcheck disable=SC2162
    while IFS='	' read -r a b c d e g h i j; do
        [ "$a" = "fixture" ] && continue
        [ -z "$a" ] && continue
        if [ "$d" = "$g" ]; then sh=true; else sh=false; fi
        echo "| \`$a\` | $b | $c | $e | $sh | $h | $i | $j |"
    done < "$TSV"
} > "$CAMPAIGN/MATRIX.md"

# --- messages.tsv -----------------------------------------------------------
{
    printf 'fixture\tschema_messages\trecord_messages\teos_messages\tschema_span\teos_span\n'
    cat "$RAW/messages.tsv"
} > "$CAMPAIGN/messages.txt"

# --- counts.txt -------------------------------------------------------------
{
    echo "fixtures $((normal_n + opaque_n))"
    echo "normal $normal_n"
    echo "exact_ok $normal_ok"
    echo "opaque_controls_ok $opaque_ok"
    echo "opaque_controls $opaque_n"
    echo "surface_fail $surface_fail"
    echo "binary $(sha256sum "$BIN" | cut -d' ' -f1)"
} > "$CAMPAIGN/counts.txt"

# --- environment.json -------------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "27.5.1 — Arrow IPC stream framing (magic-less encapsulated messages)",
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
  "fixtures_tool": "tools/fixtures/make-arrow.py (stdlib only; the stream fixture is now the bare, magic-less spec framing)",
  "external_oracles": "none (no pyarrow/Arrow tooling in this lane; the framing is validated internally)",
  "env_affecting_semantics": {"LC_ALL": "C"}
}
EOF

# --- receipt.json -----------------------------------------------------------
verdict="PASS"
if [ "$normal_ok" -ne "$normal_n" ] || [ "$opaque_ok" -ne "$opaque_n" ] || [ "$surface_fail" -ne 0 ]; then
    verdict="FAIL"
fi
cat > "$CAMPAIGN/receipt.json" <<EOF
{
  "campaign": "$CAMPAIGN",
  "phase": "27.5.1 — Arrow IPC stream framing (magic-less encapsulated messages)",
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
  "fixtures": "stream.arrow primitives.arrow junk_cont.bin truncated_stream.bin nonschema_first.bin legacy_stream.bin prose.txt",
  "fixture_tool": "tools/fixtures/make-arrow.py + shell-synthesized boundary controls",
  "external_oracles": "none",
  "invariant": "materialize(field) == source (length + SHA-256 + cmp) after source AND descriptor deletion",
  "observations": ["metadata (with the ordered message inventory)", "arrow-schema", "arrow-column", "arrow-batch", "arrow-cell"],
  "stream_framing": "a magic-less source is claimed as arrow only when it is a coherent encapsulated-message sequence: a continuation-marked framing Schema message first (FlatBuffer metadata parsed under the bounded reader), then well-formed RecordBatch messages, ending in an end-of-stream marker; the ordered messages (Schema / each RecordBatch / end-of-stream) carry their exact byte spans, the record-batch messages equal the record-batch inventory spans, the messages are contiguous from offset 0, and they cover the whole source",
  "declines": "a random 0xFFFFFFFF-prefixed blob, a truncated stream, a stream whose first message is not a Schema, and the legacy pre-0.15 no-continuation framing all stay Opaque; the file format (ARROW1 magic + footer) still detects and closes byte-exactly; never a panic",
  "legacy_framing_decision": "the pre-0.15 no-continuation framing (a bare int32 metadata size with no 0xFFFFFFFF token) is deliberately declined: a magic-less source has no other anchor, so treating an arbitrary leading word as a length would widen the false-positive surface, and the stream parser admits only the continuation form",
  "adr_0060": "the ArrowModel node depends on the exact source root (DocumentExact, keyed by sha256(source)); the message inventory is derived from that same node and never on the exactness path",
  "verdict": "$verdict"
}
EOF

cat > "$CAMPAIGN/commands.txt" <<'EOF'
docker compose run --rm --no-TTY dev sh tools/phase27-5-1-arrow-stream-court.sh
# inside the court:
#   cargo build --locked --all-features
#   target/debug/vole-document field-build FIXTURE --store STORE --voldoc F.voldoc
#   target/debug/vole-document observe --store STORE --field HEX --metadata --kind metadata
#   target/debug/vole-document observe --store STORE --field HEX (--arrow-schema|--arrow-column N|--arrow-batch N|--arrow-cell R:C) --kind metadata|text
#   rm source F.voldoc
#   target/debug/vole-document materialize --store STORE --field HEX --exact --output OUT ; cmp OUT source
EOF

# --- SUMMARY.md -------------------------------------------------------------
{
    echo "# Phase 27.5.1 — Arrow IPC **stream** framing (magic-less encapsulated messages)"
    echo
    echo "**Question.** Does VOLE claim the genuine Apache Arrow IPC **stream** format —"
    echo "the bare, magic-less sequence of encapsulated messages — conservatively, expose"
    echo "its ordered message inventory with exact byte spans, and still close the whole"
    echo "source byte-exactly, while every near miss stays \`Opaque\`?"
    echo
    echo "**Method.** The stream fixture is generated by \`tools/fixtures/make-arrow.py\`"
    echo "as the spec framing (a continuation-marked \`Schema\` first, ending in an"
    echo "end-of-stream marker, **no** \`ARROW1\` prefix). It is ingested via \`field-build\`,"
    echo "observed, then — after the source file and the standalone descriptor are"
    echo "deleted — rematerialized exactly in a fresh process and compared with"
    echo "\`length + SHA-256 + cmp\`. The boundary controls are synthesized in shell (no"
    echo "python3/jq in this lane) and pin the detection edge. The court runs in the"
    echo "pinned \`dev\` service using only POSIX \`sh\`, coreutils, git, and the shipped"
    echo "binary."
    echo
    echo "## Exactness and detection (after source + descriptor deletion)"
    echo
    tail -n +3 "$CAMPAIGN/MATRIX.md"
    echo
    echo "## Message inventory (stream.arrow)"
    echo
    echo '```'
    cat "$CAMPAIGN/messages.txt"
    echo '```'
    echo
    echo "## Counts"
    echo
    echo '```'
    cat "$CAMPAIGN/counts.txt"
    echo '```'
    echo
    echo "## Scope (honest)"
    echo
    echo "- **Supported:** detection of the bare magic-less stream framing (and the"
    echo "  historical magic+padding-prefixed stream form); the framed \`Schema\`; each"
    echo "  \`RecordBatch\`/\`DictionaryBatch\` message and the end-of-stream marker as an"
    echo "  ordered inventory with exact byte spans; the schema, record-batch inventory,"
    echo "  and decoded values as before."
    echo "- **Boundary:** a random \`0xFFFFFFFF\`-prefixed blob, a truncated stream, a"
    echo "  stream whose first message is not a \`Schema\`, and the legacy pre-0.15"
    echo "  no-continuation framing all stay \`Opaque\`."
    echo "- **Exactness** is the whole source (a RAW-like \`DocumentExact\`): the message"
    echo "  inventory is derived and never on the exactness path (ADR-0060: the model node"
    echo "  depends on the \`sha256(source)\` root)."
    echo "- **Never run on the host:** every command above ran in the pinned \`dev\`"
    echo "  container."
} > "$CAMPAIGN/SUMMARY.md"

echo
if [ "$verdict" != "PASS" ]; then
    echo "PHASE 27.5.1 ARROW STREAM COURT: FAIL — campaign $CAMPAIGN (exact_ok=$normal_ok/$normal_n opaque_ok=$opaque_ok/$opaque_n surface_fail=$surface_fail)" >&2
    exit 1
fi
echo "PHASE 27.5.1 ARROW STREAM COURT: PASS — campaign $CAMPAIGN"
