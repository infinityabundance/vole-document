#!/bin/sh
# Phase 30.3 — incremental index under the gate's <=10% bar (SAME REAL MECHANISM).
#
# ## Question
#
#   Phase 29.3 shipped `field-incremental` (a versioned durable index with explicit
#   physical AND semantic dependency edges) and verified its correctness, but the
#   index cost **745,108 B = 27.3% of source**, above the Phase-28.7 gate's <=10%
#   bar. The 29.3 receipt named the cause: a persisted per-observation **identity
#   node** (32 B of every 128 B entry) that the 28.7 *model* had excluded. 30.3
#   asks the narrow, falsifiable question:
#
#     Can the durable index be shrunk below the gate's <=10%-of-source bar
#     WITHOUT weakening correctness or read cost?
#
#   The answer implemented (index format `VOLEINC2`):
#
#     * the per-observation **identity node is not persisted** — an observation's
#       enumerated key (`csv:row:12`, `docx:para:7`, ...) already encodes its
#       position, so the index is a **positional** array keyed exactly as the old
#       identity hash was (reuse still requires physical AND semantic signature
#       match, so the answers are unchanged);
#     * the dependency signatures (physical + semantic) are **interned** into one
#       sorted dictionary and referenced by **varint** (an unchanged region
#       re-draws a single signature);
#     * the 32 B **output content id** stays full width — it addresses a blob and
#       is verified on load, so it is never traded for bytes.
#
#   This court re-runs the 29.3 court verbatim (same fixture, same families, same
#   three lanes) and re-seals a receipt, keeping the 29.3 numbers as the
#   **historical control**. It still checks, non-negotiably:
#
#     1. exactness: every revision satisfies `materialize --exact == source`
#        (length + SHA-256), in a *fresh process*, after the source AND the
#        descriptor file are deleted.
#     2. correctness: the incremental lane's answers EQUAL a full rebuild's on
#        every revision — including the DOCX style-table and XLSX shared-string
#        cases where a physical-only CAS diverges.
#     3. economy: index bytes vs source (the sealed metric AND the honest
#        whole-set sum), reuse, bytes rewritten, latency.
#
# ## Verify
#
#   docker compose run --rm --no-TTY dev sh tools/phase30-3-incremental-court.sh
#
# Env: PROFILE (debug|release, default debug), WORK, CAMP, CONTROL.

set -u
cd /work
export LC_ALL=C PYTHONDONTWRITEBYTECODE=1

FIX=tools/fixtures/phase29-3-incremental.pl

PROFILE=${PROFILE:-debug}
BUILD_ARGS="-j 1"
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features -j 1"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features -j 1";           BIN=${BIN:-target/debug/vole-document} ;;
esac

if [ -x "$BIN" ] && [ -n "${BIN_NO_BUILD:-}" ]; then
    echo "-- using prebuilt $BIN (BIN_NO_BUILD set)" >&2
else
    echo "== phase 30.3 incremental-index court ==" >&2
    echo "-- building ($PROFILE) binary" >&2
    # shellcheck disable=SC2086
    if ! cargo build $BUILD_ARGS >&2; then
        echo "phase30-3: build FAILED ($BUILD_ARGS); refusing to measure" >&2
        exit 1
    fi
fi
[ -x "$BIN" ] || { echo "phase30-3: $BIN missing; refusing to measure" >&2; exit 1; }
command -v perl >/dev/null 2>&1 || {
    echo "phase30-3: need perl (family generation + aggregation) in the court service" >&2
    exit 1
}

SHORT=$(git rev-parse --short HEAD 2>/dev/null || echo nogit)
STAMP=$(date -u +%Y-%m-%d)
CAMP=${CAMP:-evidence/campaigns/${STAMP}-phase30-3-incremental-${SHORT}}
CONTROL=${CONTROL:-evidence/campaigns/2026-10-12-phase29-3-incremental-879e6334/out/receipt.json}
RAW=$CAMP/raw
OUT=$CAMP/out
WORK=${WORK:-evidence/scratch/phase30-3-court}
rm -rf "$CAMP" "$WORK"
mkdir -p "$RAW" "$OUT" "$WORK/revs" "$WORK/mat"
ABS=$(pwd)

# ---------------------------------------------------------------------------
# Metadata (a receipt records its environment, never assumes it)
# ---------------------------------------------------------------------------
{
    echo "# Phase 30.3 receipt metadata"
    echo "generated_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "git_commit: $(git rev-parse HEAD 2>/dev/null || echo nogit)"
    echo "git_branch: $(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo nogit)"
    echo "git_dirty: $(test -n "$(git --no-optional-locks status --porcelain 2>/dev/null)" && echo yes || echo no)"
    echo "root: $ABS"
    echo "rustc: $(rustc -V 2>/dev/null || echo n/a)"
    echo "cargo: $(cargo -V 2>/dev/null || echo n/a)"
    echo "perl: $(perl -v 2>/dev/null | sed -n '2p' | sed 's/^ *//')"
    echo "arch: $(uname -m)"
    echo "nproc: $(nproc 2>/dev/null || echo '?')"
    echo "cargo_lock_sha256: $(sha256sum Cargo.lock | cut -d' ' -f1)"
    echo "binary: $BIN"
    echo "binary_sha256: $(sha256sum "$BIN" | cut -d' ' -f1)"
    echo "build_args: cargo build $BUILD_ARGS"
    echo "build_env: CARGO_PROFILE_DEV_DEBUG=${CARGO_PROFILE_DEV_DEBUG:-unset} (symbols-only; debug info does not affect semantics)"
    echo "service: dev (mem_limit == memswap_limit == 8g, pids_limit 4096)"
    echo "substrate: field-build --profile runtime --packed"
    echo "index_format: VOLEINC2 (versioned; 44 B header + interned signatures + varint refs + 32 B output id)"
    echo "index_entry_model: positional slot (identity = the enumerated key's ordinal, NOT persisted) + interned physical signature ref (varint) + interned semantic signature ref (varint) + output content id 32 B"
    echo "index_identity_scope: BLAKE3(format)[..16] in the header; a scope/version mismatch is 'no prior index', never reuse"
    echo "hash: BLAKE3-256 for observation/fragment/output ids (the store's own hash)"
    echo "identity_separation (ADR-0060): exact revision SHA-256, physical fragment id, semantic context id, and derived-observation output id stay DISTINCT; only the *persisted* identity node is dropped"
    echo "historical_control: evidence/campaigns/2026-10-12-phase29-3-incremental-879e6334 (index VOLEINC1, 27.29%)"
    echo "phase28_7_historical_control: evidence/campaigns/2026-10-12-phase28-7-revision-reuse-b06a2f76"
} > "$CAMP/METADATA.txt"

echo "== generating bounded revision families (28.7 algorithms, ported) ==" >&2
perl "$FIX" families --out "$WORK/revs" --csv-bytes 393216 --json-bytes 393216 \
    > "$RAW/manifest.tsv" 2>"$RAW/families.err" \
    || { echo "phase30-3: family generation failed" >&2; cat "$RAW/families.err" >&2; exit 1; }
cat "$RAW/manifest.tsv" >&2

TAB=$(printf '\t')
printf 'family\trev\tfield\tok\n' > "$RAW/exactness.tsv"

field_for() {  # $1=family $2=rev
    sed -n 's/.*"field":"\([0-9a-f]*\)".*/\1/p' "$RAW/$1/build.r$2.json" | head -1
}

while IFS="$TAB" read -r fam fmt change rev path bytes sha; do
    [ "$fam" = "family" ] && continue
    STORE="$WORK/store/$fam"
    mkdir -p "$RAW/$fam" "$STORE"
    echo "-- $fam r$rev: field-build ($fmt)" >&2
    "$BIN" field-build --profile runtime --packed --store "$STORE" \
        --voldoc "$WORK/d.voldoc" "$path" > "$RAW/$fam/build.r$rev.json" \
        || { echo "phase30-3: field-build failed ($fam r$rev)" >&2; exit 1; }
    field=$(field_for "$fam" "$rev")
    [ -n "$field" ] || { echo "phase30-3: no field id ($fam r$rev)" >&2; exit 1; }
    # Exactness AFTER source + descriptor deletion, in a fresh process.
    rm -f "$path" "$WORK/d.voldoc"
    "$BIN" materialize --store "$STORE" --field "$field" --packed --exact \
        --output "$WORK/mat/m.$fam.r$rev" > "$RAW/$fam/mat.r$rev.json" \
        || { echo "phase30-3: materialize failed ($fam r$rev)" >&2; exit 1; }
    msha=$(sha256sum "$WORK/mat/m.$fam.r$rev" | cut -d' ' -f1)
    if [ "$msha" = "$sha" ]; then ok=ok; else ok=FAIL; fi
    printf '%s\t%s\t%s\t%s\n' "$fam" "$rev" "$field" "$ok" >> "$RAW/exactness.tsv"
    [ "$ok" = "ok" ] || echo "phase30-3: EXACTNESS FAIL $fam r$rev" >&2
    echo "$field" > "$RAW/$fam/field.r$rev.txt"
done < "$RAW/manifest.tsv"

for fam in $(cut -f1 "$RAW/manifest.tsv" | tail -n +2 | awk '!seen[$0]++'); do
    STORE="$WORK/store/$fam"
    nrev=$(awk -F'\t' -v f="$fam" '$1==f{n++} END{print n}' "$RAW/manifest.tsv")
    for lane in full cas incremental; do
        IDX="$WORK/idx/$fam/$lane"
        mkdir -p "$RAW/$fam/$lane"
        for rev in $(seq 1 "$nrev"); do
            field=$(cat "$RAW/$fam/field.r$rev.txt")
            echo "-- $fam r$rev: lane=$lane" >&2
            "$BIN" field-incremental --store "$STORE" --field "$field" --packed \
                --index "$IDX" --lane "$lane" --verify \
                > "$RAW/$fam/$lane/r$rev.json" \
                || { echo "phase30-3: field-incremental failed ($fam r$rev $lane)" >&2; exit 1; }
        done
    done
done

echo "== aggregating + deciding ==" >&2
perl "$FIX" decide --raw "$RAW" --out "$OUT" --campaign "$CAMP" --short "$SHORT" \
    --label 30.3 --control "$CONTROL" || rc=$?
rc=${rc:-0}
echo "--- SUMMARY.md ---" >&2
cat "$OUT/SUMMARY.md" >&2
echo "--- DECISION.txt ---" >&2
cat "$OUT/DECISION.txt" >&2
exit "$rc"
