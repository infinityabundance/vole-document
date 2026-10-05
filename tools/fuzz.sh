#!/bin/sh
# Bounded coverage-guided fuzz campaign for VOLE-Document (Phase 7).
#
# Docker only. Run inside the pinned `fuzz` service:
#
#   docker compose run --rm --no-TTY fuzz sh tools/fuzz.sh
#
# Env knobs (all optional):
#   FUZZ_SECONDS   per-target wall-clock budget in seconds   (default 60)
#   FUZZ_RSS_MB    libFuzzer RSS policy in MiB               (default 2048)
#   FUZZ_TARGETS   space-separated target list               (default: all ten)
#   FUZZ_OUT       output dir for logs and the summary       (default /work/fuzz/campaign)
#
# The campaign is *bounded*: every target gets at most FUZZ_SECONDS. Crash,
# OOM, timeout, and leak artifacts are written under
# /work/fuzz/artifacts/<target>/ (gitignored), and the campaign continues to
# the next target when one crashes, so one bad target cannot hide the rest.

set -u
cd /work

: "${FUZZ_SECONDS:=60}"
: "${FUZZ_RSS_MB:=2048}"
: "${FUZZ_TARGETS:=voldoc_parse voldoc_roundtrip dra_program rans_model rans_channel pdf_lexer pdf_scan pdf_xref deflate_replay materializer}"
: "${FUZZ_OUT:=/work/fuzz/campaign}"

ART=/work/fuzz/artifacts
CORP=/work/fuzz/corpus
SEEDS=/work/fuzz/seeds

# Fresh artifact set so the per-target artifact count reflects this campaign.
rm -rf "$ART"
mkdir -p "$ART" "$CORP" "$FUZZ_OUT"

echo "cargo-fuzz: $(cargo fuzz --version)"
echo "rustc: $(rustc --version)"
echo "cargo: $(cargo --version)"
echo "policy: FUZZ_SECONDS=$FUZZ_SECONDS FUZZ_RSS_MB=$FUZZ_RSS_MB"

printf 'target\tduration_s\texit\tartifacts\tcov\tft\texecs\teps\tpeak_rss_mb\n' > "$FUZZ_OUT/summary.tsv"

for t in $FUZZ_TARGETS; do
    mkdir -p "$CORP/$t" "$ART/$t"
    cp -n "$SEEDS"/. "$CORP/$t"/ 2>/dev/null

    log="$FUZZ_OUT/$t.log"
    start=$(date +%s)
    cargo fuzz run "$t" -- \
        -max_total_time="$FUZZ_SECONDS" \
        -rss_limit_mb="$FUZZ_RSS_MB" \
        -artifact_prefix="$ART/$t/" \
        -print_final_stats=1 > "$log" 2>&1
    status=$?
    end=$(date +%s)

    arts=$(ls "$ART/$t" 2>/dev/null | grep -cE '^(crash|oom|timeout|leak)-' || true)
    cov=$(grep -o 'cov: [0-9]*' "$log" | tail -1 | sed 's/cov: //')
    ft=$(grep -o 'ft: [0-9]*' "$log" | tail -1 | sed 's/ft: //')
    execs=$(grep 'stat::number_of_executed_units:' "$log" | tail -1 | sed 's/.*: *//')
    eps=$(grep 'stat::average_exec_per_sec:' "$log" | tail -1 | sed 's/.*: *//')
    peak=$(grep 'stat::peak_rss_mb:' "$log" | tail -1 | sed 's/.*: *//')

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$t" "$((end - start))" "$status" "$arts" \
        "${cov:-0}" "${ft:-0}" "${execs:-0}" "${eps:-0}" "${peak:-0}" \
        >> "$FUZZ_OUT/summary.tsv"

    echo "== $t: exit=$status duration=$((end - start))s artifacts=$arts cov=${cov:-0} ft=${ft:-0} execs=${execs:-0} eps=${eps:-0} peak_rss=${peak:-0}MB"
done

echo "summary: $FUZZ_OUT/summary.tsv"
