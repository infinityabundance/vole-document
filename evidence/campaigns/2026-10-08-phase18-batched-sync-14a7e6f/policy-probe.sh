#!/usr/bin/env bash
# Phase 18.5 supporting probe — batched durability (`--sync=batch`, default) vs
# per-record durability (`--sync=each`) on the worst bind-mounted document, plus
# the packed-store exactness and warm-session checks.
#
# Never on the host:
#   docker compose run --rm --no-TTY doc-baseline bash evidence/scratch/p185-after/policy-probe.sh
set -uo pipefail
cd /work
export LC_ALL=C
BIN=${BIN:-target/release/vole-document}
F=${F:-real100-v1/documents/nist/pdf/nist-pdf-0017.pdf}
OUT=${OUT:-evidence/campaigns/2026-10-08-phase18-batched-sync-14a7e6f/raw}
W=${WORK:-evidence/scratch/p185-after/policy}
rm -rf "$W"; mkdir -p "$W" "$OUT"

now_ms() { echo $(( ${EPOCHREALTIME/./} / 1000 )); }

echo "# policy probe: $F ($(stat -c %s "$F") bytes), bin $BIN" >"$OUT/policy.txt"

# --- build wall, best-of-3 per durability policy ---------------------------
printf 'policy\trep\trc\tms\tfiles\n' >"$OUT/policy.tsv"
for pol in batch each; do
    for rep in 1 2 3; do
        st="$W/$pol.$rep"; rm -rf "$st"
        t0=$(now_ms)
        "$BIN" field-build "$F" --store "$st" --profile runtime --packed --sync="$pol" \
            >"$W/$pol.$rep.json" 2>"$W/$pol.$rep.err"; rc=$?
        t1=$(now_ms)
        files=$(find "$st" -type f 2>/dev/null | wc -l | tr -d ' ')
        printf '%s\t%s\t%s\t%s\t%s\n' "$pol" "$rep" "$rc" "$(( t1 - t0 ))" "$files" >>"$OUT/policy.tsv"
    done
done
cat "$OUT/policy.tsv" >>"$OUT/policy.txt"

# --- packed-store exactness on the batch-policy store -----------------------
st="$W/batch.1"
field=$(jq -r '.ingest.field' "$W/batch.1.json" 2>/dev/null)
{
    printf 'orig_len\t%s\n' "$(stat -c %s "$F")"
    printf 'orig_sha256\t%s\n' "$(sha256sum "$F" | cut -d' ' -f1)"
    "$BIN" materialize --store "$st" --field "$field" --exact --output "$W/out.bin" --packed \
        >"$W/mat.json" 2>"$W/mat.err"
    printf 'materialize_rc\t%s\n' "$?"
    if [ -s "$W/out.bin" ]; then
        printf 'mat_len\t%s\n' "$(stat -c %s "$W/out.bin")"
        printf 'mat_sha256\t%s\n' "$(sha256sum "$W/out.bin" | cut -d' ' -f1)"
        if cmp -s "$W/out.bin" "$F"; then printf 'byte_compare\tequal\n'; else printf 'byte_compare\tDIFF\n'; fi
    fi
    printf 'field\t%s\n' "$field"
} >"$OUT/exactness.txt"

# --- warm packed session (the 18.4 rc-6 regression) -------------------------
printf -- '--page 1 --kind text\n--metadata --kind metadata\n--revisions --kind lineage\n' >"$W/reqs"
t0=$(now_ms)
"$BIN" observe-batch --store "$st" --field "$field" --packed \
    --requests "$W/reqs" --repeat 20 >"$W/warm.jsonl" 2>"$W/warm.err"
wrc=$?; t1=$(now_ms)
{
    printf 'observe_batch_rc\t%s\n' "$wrc"
    printf 'wall_ms\t%s\n' "$(( t1 - t0 ))"
    printf 'answers\t%s\n' "$(grep -c . "$W/warm.jsonl")"
    printf 'stdout_head\n'
    head -3 "$W/warm.jsonl"
} >"$OUT/warm_packed.txt"

echo "== policy probe done =="
cat "$OUT/policy.tsv"
cat "$OUT/exactness.txt"
cat "$OUT/warm_packed.txt"
