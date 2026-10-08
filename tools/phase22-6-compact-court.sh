#!/usr/bin/env bash
# Phase 22.6 (review item P5) — compact structural representation — MEASUREMENT COURT.
#
# ## Question
#
#   Can an existing typed index / structural node be represented more compactly
#   while preserving the same observation capability? Candidates: interned
#   structural identifiers, common string dictionaries, delta-encoded coordinate
#   tables, compact relationship edges / shared offset maps, compressed bitmaps
#   with rank/select for sparse membership.
#
# ## What this court does (MEASUREMENT ONLY — no wire-format/src change)
#
#   For the frozen `real100-v1` population it builds each document's field store
#   with the fs backend (`field-build --profile runtime`, NO `--packed`), so every
#   seed node is one file and its canonical bytes are directly measurable, then:
#
#     1. attributes persistent bytes by namespace (`descriptor/`, `field/`,
#        `index/`, `seed/`) and, inside `seed/`, by `SeedNode` `NodeKind`;
#     2. sizes candidate compact encodings ON THE ACTUAL CANONICAL NODE BYTES and
#        proves byte-exact decode (see tools/fixtures/phase22-6-compact.py);
#     3. proves observation invariance by round-trip (decode == original bytes)
#        AND by rebuilding the typed store from the decoded bytes and re-running
#        the frozen `observe-batch` request set (a small labelled subset);
#     4. reports the saving as a fraction of the TOTAL persistent footprint and
#        compares it to a stated bar.
#
#   It does NOT change `src/`, the wire format, or ship a compact store: it only
#   sizes one. `descriptor/` (the exact authority) is left untouched.
#
# ## Bar
#
#   >= 5% of the total persistent footprint at zero observation loss. 5% is a
#   deliberate, conservative floor: below it the compaction cannot be defended as
#   "significant" against the exact-authority bytes it sits beside, and it is
#   large relative to the ~1-2% run-to-run accounting noise of the byte walk.
#
# ## Lane (never the host)
#
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase22-6-compact-court.sh
#   # bounded / explicit population:
#   docker compose run --rm --no-TTY doc-baseline sh -c \
#     'LIMIT=3 bash tools/phase22-6-compact-court.sh'
#   docker compose run --rm --no-TTY doc-baseline sh -c \
#     'IDS="nist-pdf-0002 nasa-pdf-0001" bash tools/phase22-6-compact-court.sh'
set -uo pipefail

cd /work
export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1

SHA=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
SHAFULL=$(git rev-parse HEAD 2>/dev/null || echo unknown)
STAMP=$(date -u +%Y-%m-%d)
CAMPAIGN=${CAMPAIGN:-evidence/campaigns/${STAMP}-phase22-6-compact-${SHA}${TAG:+-$TAG}}
RAW=$CAMPAIGN/raw
WORK=${WORK:-evidence/scratch/phase22-6-compact-work}
mkdir -p "$RAW/measure" "$RAW/build" "$RAW/obs" "$RAW/recon" "$WORK"

HARNESS=${HARNESS:-tools/fixtures/phase22-6-compact.py}
CORPUS=${CORPUS:-real100-v1/documents}
LIMIT=${LIMIT:-0}                 # 0 = full frozen population; >0 = first N
IDS=${IDS:-}                      # optional space-separated id restriction
BAR=${BAR:-0.05}
OP_TIMEOUT=${OP_TIMEOUT:-600}

PROFILE=${PROFILE:-release}
case "$PROFILE" in
    release) BUILD_ARGS="--release --locked --all-features"; BIN=${BIN:-target/release/vole-document} ;;
    *)       BUILD_ARGS="--locked --all-features";           BIN=${BIN:-${VOLE_BIN:-target/debug/vole-document}} ;;
esac

echo "== phase22.6 compact structural representation court ==" >&2
echo "-- building ($PROFILE) binary" >&2
# shellcheck disable=SC2086
if ! cargo build $BUILD_ARGS >&2; then
    echo "phase22-6-compact: build FAILED ($BUILD_ARGS); refusing to measure" >&2
    exit 1
fi
[ -x "$BIN" ] || { echo "phase22-6-compact: $BIN missing after build" >&2; exit 1; }

# Document list from the corpus itself (robust to manifest quoting quirks).
DOCLIST="$RAW/docs.tsv"
: >"$DOCLIST"
while IFS= read -r p; do
    base=$(basename "$p")
    id=${base%.*}
    ext=${base##*.}
    printf '%s\t%s\t%s\n' "$id" "$ext" "$p" >>"$DOCLIST"
done < <(find "$CORPUS" -type f | LC_ALL=C sort)

if [ -n "$IDS" ]; then
    printf '%s\n' $IDS | LC_ALL=C sort >"$RAW/ids.txt"
    awk -F'\t' 'NR==FNR{want[$1]=1;next} $1 in want' "$RAW/ids.txt" "$DOCLIST" >"$RAW/docs.filtered.tsv"
    mv "$RAW/docs.filtered.tsv" "$DOCLIST"
fi
if [ "$LIMIT" -gt 0 ]; then
    head -n "$LIMIT" "$DOCLIST" >"$RAW/docs.limited.tsv"
    mv "$RAW/docs.limited.tsv" "$DOCLIST"
fi

# Invariance subset: first pdf, first docx, first epub (labelled, small).
INVARIANCE_IDS=${INVARIANCE_IDS:-}
if [ -z "$INVARIANCE_IDS" ]; then
    p=$(awk -F'\t' '$2=="pdf"{print $1; exit}' "$DOCLIST")
    d=$(awk -F'\t' '$2=="docx"{print $1; exit}' "$DOCLIST")
    e=$(awk -F'\t' '$2=="epub"{print $1; exit}' "$DOCLIST")
    INVARIANCE_IDS="$p $d $e"
fi
echo "-- invariance subset: $INVARIANCE_IDS" >&2

req_file() {
    # $1 = format, $2 = output file
    case "$1" in
        pdf)  printf '%s\n' "--page 1 --kind text" "--metadata --kind metadata" \
                     "--byte-range 0..64 --kind exact" >"$2" ;;
        docx|epub) printf '%s\n' "--block 0 --kind text" "--metadata --kind metadata" \
                     "--resource 0 --kind metadata" >"$2" ;;
        *)    printf '%s\n' "--metadata --kind metadata" >"$2" ;;
    esac
}

printf 'id\tfmt\trc\twall_ms\tsrc_len\tenc_len\tstore_bytes\tseed_bytes\tindex_bytes\tfield_bytes\tdesc_bytes\tnode_count\tindex_nodes\tmeasure_rc\n' >"$RAW/build.tsv"
printf 'id\tfmt\tobs_rc\trecon_rc\tdiff_rc\trecon_obs_rc\tsem_match\tfield\n' >"$RAW/invariance.tsv"

NDOC=$(wc -l <"$DOCLIST")
echo "-- $NDOC documents" >&2

while IFS=$'\t' read -r id fmt path; do
    [ -n "$id" ] || continue
    d="$WORK/$id"; rm -rf "$d"; mkdir -p "$d"
    echo "== $id ($fmt) ==" >&2

    t0=$(date +%s%3N)
    timeout "$OP_TIMEOUT" "$BIN" field-build "$path" --store "$d/vstore" --profile runtime \
        >"$RAW/build/$id.build.json" 2>"$RAW/build/$id.build.err"; brc=$?
    t1=$(date +%s%3N); wall=$(( t1 - t0 ))

    src=0; enc=0; sb=0; ib=0; fb=0; db=0; nc=0; inc=0; mrc=9
    if [ "$brc" -eq 0 ]; then
        src=$(jq -r '.source_len // 0' "$RAW/build/$id.build.json")
        enc=$(jq -r '.encoded_len // 0' "$RAW/build/$id.build.json")
        [ -d "$d/vstore/seed" ]   && sb=$(find "$d/vstore/seed"   -type f -printf '%s\n' | awk '{s+=$1} END{print s+0}')
        [ -d "$d/vstore/index" ]  && ib=$(find "$d/vstore/index"  -type f -printf '%s\n' | awk '{s+=$1} END{print s+0}')
        [ -d "$d/vstore/field" ]  && fb=$(find "$d/vstore/field"  -type f -printf '%s\n' | awk '{s+=$1} END{print s+0}')
        [ -d "$d/vstore/descriptor" ] && db=$(find "$d/vstore/descriptor" -type f -printf '%s\n' | awk '{s+=$1} END{print s+0}')
        nc=$(jq -r '.ingest.node_count // 0' "$RAW/build/$id.build.json" 2>/dev/null)
        inc=$(jq -r '.ingest.index_node_count // 0' "$RAW/build/$id.build.json" 2>/dev/null)

        # --- measure (candidates + byte-exact round-trip) ---
        python3 "$HARNESS" --store "$d/vstore" --doc "$id" --fmt "$fmt" \
            --build "$RAW/build/$id.build.json" \
            --out "$RAW/measure/measure-$id.json" >/dev/null 2>"$RAW/measure/measure-$id.err"
        mrc=$?

        field=$(jq -r '.ingest.field // empty' "$RAW/build/$id.build.json")
        req_file "$fmt" "$d/req.txt"

        # --- observation invariance (labelled subset), BEFORE any observe call
        #     mutates the store: rebuild the typed store from the DECODED bytes,
        #     prove byte-identity, then compare the semantic observe answers of
        #     the unmodified store and the rebuilt store (both start pristine).
        #     `observe` is stateful (demand-driven `deepen_page` adds a derived
        #     field), so `stats`/timing are stripped and two fresh copies must be
        #     compared; byte-identity is the primary proof. ---
        recrc=9; diffrc=9; orc=9; rorc=9; match=na
        case " $INVARIANCE_IDS " in
        *" $id "*)
            python3 "$HARNESS" reconstruct --store "$d/vstore" --out "$d/recon" \
                >"$RAW/recon/$id.json" 2>"$RAW/recon/$id.err"; recrc=$?
            if [ "$recrc" -eq 0 ]; then
                { diff -r "$d/vstore/seed" "$d/recon/seed" \
                  && diff -r "$d/vstore/index" "$d/recon/index" \
                  && diff -r "$d/vstore/field" "$d/recon/field"; } \
                  >"$RAW/recon/$id.diff" 2>&1; diffrc=$?
            fi
            ;;
        esac

        # --- frozen observation answers on the UNMODIFIED store ---
        if [ -n "$field" ]; then
            timeout "$OP_TIMEOUT" "$BIN" observe-batch --store "$d/vstore" --field "$field" \
                --requests "$d/req.txt" >"$RAW/obs/$id.orig.jsonl" 2>"$RAW/obs/$id.orig.err"; orc=$?
            if [ "$orc" -eq 0 ]; then
                jq -c 'del(.stats)' "$RAW/obs/$id.orig.jsonl" >"$RAW/obs/$id.orig.sem.jsonl" 2>/dev/null
            fi
        fi

        # --- re-run the SAME request set on the rebuilt store ---
        if [ "$recrc" -eq 0 ] && [ -n "$field" ]; then
            timeout "$OP_TIMEOUT" "$BIN" observe-batch --store "$d/recon" --field "$field" \
                --requests "$d/req.txt" >"$RAW/obs/$id.recon.jsonl" 2>"$RAW/obs/$id.recon.err"; rorc=$?
            if [ "$rorc" -eq 0 ]; then
                jq -c 'del(.stats)' "$RAW/obs/$id.recon.jsonl" >"$RAW/obs/$id.recon.sem.jsonl" 2>/dev/null
                if cmp -s "$RAW/obs/$id.orig.sem.jsonl" "$RAW/obs/$id.recon.sem.jsonl"; then match=yes; else match=NO; fi
            fi
        fi
        if case " $INVARIANCE_IDS " in *" $id "*) true;; *) false;; esac; then
            printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
                "$id" "$fmt" "$orc" "$recrc" "$diffrc" "$rorc" "$match" "$field" >>"$RAW/invariance.tsv"
        fi
    fi

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$id" "$fmt" "$brc" "$wall" "$src" "$enc" "$((sb+ib+fb+db))" "$sb" "$ib" "$fb" "$db" "$nc" "$inc" "$mrc" \
        >>"$RAW/build.tsv"

    [ "${KEEP:-0}" = "1" ] || rm -rf "$d/vstore" "$d/recon"
done <"$DOCLIST"

echo "-- aggregating" >&2
python3 "$HARNESS" aggregate --rawdir "$RAW/measure" --outdir "$CAMPAIGN" --bar "$BAR" \
    >"$RAW/aggregate.json" 2>"$RAW/aggregate.err" || { echo "aggregate FAILED" >&2; cat "$RAW/aggregate.err" >&2; }

# ---------------------------------------------------------------------------
# provenance
# ---------------------------------------------------------------------------
BASE_IMAGE=$(grep -m1 '^ARG BASE_STABLE=' Dockerfile | sed 's/^ARG BASE_STABLE=//')
CARGO_LOCK_SHA=$(sha256sum Cargo.lock 2>/dev/null | awk '{print $1}')
BIN_SHA=$(sha256sum "$BIN" 2>/dev/null | awk '{print $1}')
MANIFEST_SHA=$( [ -f real100-v1/manifest.tsv ] && sha256sum real100-v1/manifest.tsv | awk '{print $1}' || echo none)
RUSTC=$(rustc --version 2>/dev/null || echo unknown)
CARGO=$(cargo --version 2>/dev/null || echo unknown)
SQLITE=$(sqlite3 --version 2>/dev/null | awk '{print $1}' || echo none)
PYVER=$(python3 --version 2>/dev/null || echo unknown)
ARCH=$(uname -m)
UTC=$(date -u +%Y-%m-%dT%H:%M:%SZ)
DIRTY=$(git --no-optional-locks status --short 2>/dev/null | tr '\n' ';')

cat >"$CAMPAIGN/environment.json" <<JSON
{
  "campaign": "$CAMPAIGN",
  "phase": "22.6 — compact structural representation (measurement only, no src change)",
  "utc": "$UTC",
  "git_commit": "$SHAFULL",
  "git_dirty": "$DIRTY",
  "arch": "$ARCH",
  "service": "doc-baseline",
  "base_image": "$BASE_IMAGE",
  "rustc": "$RUSTC",
  "cargo": "$CARGO",
  "sqlite3": "$SQLITE",
  "python": "$PYVER",
  "cargo_lock_sha256": "$CARGO_LOCK_SHA",
  "manifest": "real100-v1/manifest.tsv",
  "manifest_sha256": "$MANIFEST_SHA",
  "bin": "$BIN",
  "bin_sha256": "$BIN_SHA",
  "profile": "$PROFILE",
  "corpus": "$CORPUS",
  "bar": $BAR,
  "accounting": "persistent bytes = sum of regular-file sizes (find -printf %s); du -sb never used",
  "descriptor_bytes": "the exact-authority descriptor/ blob, measured by file size only (never read into memory)",
  "env_affecting_semantics": {"LC_ALL": "C", "PYTHONDONTWRITEBYTECODE": "1"}
}
JSON

cat >"$CAMPAIGN/commands.txt" <<'CMDS'
# Phase 22.6 (P5) — compact structural representation (MEASUREMENT only):
#   docker compose run --rm --no-TTY doc-baseline bash tools/phase22-6-compact-court.sh
# Bounded / explicit population:
#   docker compose run --rm --no-TTY doc-baseline sh -c \
#     'LIMIT=3 bash tools/phase22-6-compact-court.sh'
#   docker compose run --rm --no-TTY doc-baseline sh -c \
#     'IDS="nist-pdf-0002 nasa-pdf-0001" bash tools/phase22-6-compact-court.sh'
# The court builds each store with `field-build <src> --store <d> --profile runtime`
# (fs backend, no --packed) so every seed node is one file with directly
# measurable canonical bytes, sizes candidate compact encodings on those bytes
# (tools/fixtures/phase22-6-compact.py, pure-python BLAKE3 validated against the
# store's own node filenames), proves byte-exact decode, re-runs the frozen
# observe-batch request set on a store rebuilt from the decoded bytes, and never
# touches src/ or the wire format.
CMDS

cat >"$CAMPAIGN/receipt.json" <<JSON
{
  "campaign": "$CAMPAIGN",
  "phase": "22.6 — compact structural representation (P5)",
  "utc": "$UTC",
  "measured_commit": "$SHAFULL",
  "tree_state": "$DIRTY",
  "profile": "$PROFILE",
  "bin": "$BIN",
  "bin_sha256": "$BIN_SHA",
  "base_image": "$BASE_IMAGE",
  "service": "doc-baseline",
  "rustc": "$RUSTC",
  "cargo": "$CARGO",
  "sqlite3": "$SQLITE",
  "python": "$PYVER",
  "cargo_lock_sha256": "$CARGO_LOCK_SHA",
  "manifest_sha256": "$MANIFEST_SHA",
  "harness": "$HARNESS",
  "court": "tools/phase22-6-compact-court.sh",
  "bar": $BAR,
  "candidates": "E header-fold; A delta+varint coordinates; B shared string dictionary; D structural-id interning (content-address recompute, no id table); C sparse number-set bitmap+rank/select",
  "accounting": "persistent bytes = sum of regular-file sizes (find -printf %s)",
  "roundtrip": "combined blob decodes back to byte-identical seed+index+manifest files (BLAKE3 ids recomputed by topological content-addressing); observe-batch answers on the rebuilt store are compared to the unmodified store",
  "is_measurement_only": "YES — no src/ or wire-format change; stores are sized, never shipped",
  "rc_codes": "0 ok; 2 harness round-trip failure; 9 not attempted; 124 timeout; 137 SIGKILL/OOM"
}
JSON

echo "-- done — $CAMPAIGN" >&2
echo "campaign=$CAMPAIGN" 
cat "$RAW/aggregate.json" 2>/dev/null
