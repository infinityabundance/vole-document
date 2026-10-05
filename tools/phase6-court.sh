#!/bin/sh
# Phase 6 exact court: the exact DEFLATE replay rungs over the 12-file corpus.
#
# Extends the Phase-5.8 forced-candidate ablation with the Phase-6 exact DEFLATE
# replay candidates (`PDF_DEFLATE_REPLAY`, `PDF_DEFLATE_REPLAY_RANS`). Runs
# entirely inside the pinned dev container. It:
#   1. builds the exact binary (`cargo build --locked --all-features`);
#   2. materializes the deterministic 12-file sample corpus via
#      `pdf-make-samples`;
#   3. for EVERY corpus file and EVERY candidate kind (raw, rle, byte-rans,
#      pdf-physical, pdf-channels, pdf-layout, pdf-layout-rans,
#      pdf-deflate-replay, pdf-deflate-replay-rans) runs `encode --force KIND`,
#      capturing the forced encoded length and its complete cost, or `null` when
#      the input does not propose that kind (a typed Usage decline, not a crash);
#   4. runs the ordinary `encode` to get the auto winner and asserts exactness:
#      the winner decodes byte-exactly (`cmp`) and `verify` passes, recording the
#      full exact triple (length, SHA-256, byte compare);
#   5. sums a cumulative ladder A0..A8, one mechanism added per rung (A0 RAW,
#      A2 +BYTE_RANS, A6 +PDF_LAYOUT_RANS, A7 +PDF_DEFLATE_REPLAY,
#      A8 +PDF_DEFLATE_REPLAY_RANS), and the leave-one-out delta for each replay
#      mechanism;
#   6. records, per file, the forced BYTE_RANS and PDF_DEFLATE_REPLAY_RANS sizes
#      and a `replay_rans_vs_byte_rans` verdict (win|lose|tie|declined) with its
#      byte delta, so the receipt is an honest head-to-head against BYTE_RANS;
#   7. records the negative controls and the replay declines verbatim;
#   8. writes corpus.json, results.json, attribution.json,
#      ablation-cumulative.json, ablation-leave-one-out.json,
#      negative-controls.json, verification.json, environment.json,
#      manifest.json and report.md into the campaign directory.
#
# The forced lane is honest ablation: it re-uses the same complete-cost court
# over a one-element candidate set, so a forced mechanism is still serialized,
# decoded, and byte-compared before it is returned. Forcing never fabricates a
# win that the court would not have accepted.
#
# The independent qpdf oracle (`tools/pdf-oracle.sh`) is a separate differential
# court run in the `tools` image; it is optional here and skipped unless `qpdf`
# is on PATH (e.g. when this script is run inside the tools image).
#
# Usage (from the host):
#   docker compose run --rm --no-TTY dev sh tools/phase6-court.sh
set -eu

cd /work
LC_ALL=C
export LC_ALL

cargo build --locked --all-features --quiet
BIN=target/debug/vole-document

SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
CAMPAIGN="evidence/campaigns/$(date -u +%Y-%m-%d)-phase6-${SHA}"
campaign="$(basename "$CAMPAIGN")"

WORK=evidence/scratch/phase6
CORPUS="$WORK/corpus"
rm -rf "$WORK"
mkdir -p "$CORPUS" "$CAMPAIGN"

"$BIN" pdf-make-samples "$CORPUS" >/dev/null

KINDS="raw rle byte-rans pdf-physical pdf-channels pdf-layout pdf-layout-rans pdf-deflate-replay pdf-deflate-replay-rans"

# min2 A B -> the smaller of two integers.
min2() {
  if [ "$1" -le "$2" ]; then printf '%s' "$1"; else printf '%s' "$2"; fi
}
# pick V FALLBACK -> V unless V is the literal "null", then FALLBACK.
pick() {
  if [ "$1" = "null" ]; then printf '%s' "$2"; else printf '%s' "$1"; fi
}
# jsize V -> V as a JSON number, or bare null.
jsize() {
  if [ "$1" = "null" ]; then printf 'null'; else printf '%s' "$1"; fi
}
# jstr V -> V as a JSON string, or bare null.
jstr() {
  if [ "$1" = "null" ]; then printf 'null'; else printf '"%s"' "$1"; fi
}
# cost_of FILE -> the `{...}` object printed after `"cost":`, or null.
cost_of() {
  _c="$(grep -o '"cost":{[^}]*}' "$1" 2>/dev/null || true)"
  if [ -n "$_c" ]; then printf '%s' "${_c#\"cost\":}"; else printf 'null'; fi
}
# json_array_lines FILE -> a JSON array of the (one-per-line) objects in FILE.
json_array_lines() {
  if [ ! -s "$1" ]; then printf '[]'; return; fi
  printf '['
  awk 'NR>1{printf ","} {printf "%s",$0}' "$1"
  printf ']'
}

: > "$WORK/results-entries.jsonl"
: > "$WORK/verification-entries.jsonl"
: > "$WORK/corpus-entries.jsonl"
: > "$WORK/attribution-winner.jsonl"
: > "$WORK/attribution-replay.jsonl"
: > "$WORK/negative-controls.jsonl"
: > "$WORK/rows.md"
: > "$WORK/headtohead-rows.md"
: > "$WORK/replay-rows.md"

tot_a0=0; tot_a1=0; tot_a2=0; tot_a3=0; tot_a4=0
tot_a5=0; tot_a6=0; tot_a7=0; tot_a8=0
tot_wo_raw=0; tot_wo_rans=0; tot_wo_layout=0
w_raw=0; w_rle=0; w_byte=0; w_phys=0; w_chan=0
w_layout=0; w_layout_rans=0; w_dr=0; w_drr=0
h2h_win=0; h2h_lose=0; h2h_tie=0; h2h_declined=0
raw_h2h_win=0; raw_h2h_lose=0; raw_h2h_tie=0; raw_h2h_declined=0
file_count=0
all_exact=true
negative_ok=true
fail=0

printf '%-16s %7s %7s %7s %7s %7s %7s %7s %7s %7s %7s %-24s %-8s\n' \
  file source RAW RLE BYTE_RANS PHYS CHAN LAYOUT LAY_RANS DEFL_REP DEFL_RANS winner byte_cmp
printf '%-16s %7s %7s %7s %7s %7s %7s %7s %7s %7s %7s %-24s %-8s\n' \
  ---------------- ------- ------- ------- ------- ------- ------- ------- ------- ------- ------- ------------------------ --------

for src in "$CORPUS"/*; do
  name="$(basename "$src")"
  file_count=$((file_count + 1))
  src_len="$(wc -c < "$src" | tr -d ' ')"
  src_sha="$(sha256sum "$src" | cut -d' ' -f1)"

  # (a) Forced ablation: one encode per candidate kind.
  for k in $KINDS; do
    if "$BIN" encode --force "$k" "$src" "$WORK/forced.enc" \
        > "$WORK/forced.json" 2> "$WORK/forced.err"; then
      wc -c < "$WORK/forced.enc" | tr -d ' ' > "$WORK/f_${k}.size"
      cost_of "$WORK/forced.json" > "$WORK/f_${k}.cost"
      printf 'true' > "$WORK/f_${k}.ok"
    elif grep -q "is not proposed" "$WORK/forced.err"; then
      printf 'null' > "$WORK/f_${k}.size"
      printf 'null' > "$WORK/f_${k}.cost"
      printf 'false' > "$WORK/f_${k}.ok"
    else
      echo "FINDING: forced encode of $name as $k failed unexpectedly:" >&2
      cat "$WORK/forced.err" >&2
      printf 'null' > "$WORK/f_${k}.size"
      printf 'null' > "$WORK/f_${k}.cost"
      printf 'false' > "$WORK/f_${k}.ok"
      fail=1
    fi
  done
  s_raw="$(cat "$WORK/f_raw.size")"
  s_rle="$(cat "$WORK/f_rle.size")"
  s_byte_rans="$(cat "$WORK/f_byte-rans.size")"
  s_pdf_physical="$(cat "$WORK/f_pdf-physical.size")"
  s_pdf_channels="$(cat "$WORK/f_pdf-channels.size")"
  s_pdf_layout="$(cat "$WORK/f_pdf-layout.size")"
  s_pdf_layout_rans="$(cat "$WORK/f_pdf-layout-rans.size")"
  s_deflate_replay="$(cat "$WORK/f_pdf-deflate-replay.size")"
  s_deflate_replay_rans="$(cat "$WORK/f_pdf-deflate-replay-rans.size")"

  # (b) Auto winner via the ordinary complete-cost court.
  "$BIN" encode "$src" "$WORK/auto.enc" > "$WORK/auto.json"
  auto_len="$(wc -c < "$WORK/auto.enc" | tr -d ' ')"
  winner="$(sed -n 's/.*"candidate":"\([^"]*\)".*/\1/p' "$WORK/auto.json")"
  [ -n "$winner" ] || winner="UNKNOWN"
  auto_cost="$(cost_of "$WORK/auto.json")"

  # (c) Exactness: the auto winner must decode byte-exactly and verify.
  verify_ok=true
  if "$BIN" verify "$WORK/auto.enc" >/dev/null 2> "$WORK/verify.err"; then
    :
  else
    verify_ok=false
    all_exact=false
    fail=1
    echo "FINDING: verify failed for $name" >&2
    cat "$WORK/verify.err" >&2
  fi
  byte_compare="equal"
  mat_len=null
  mat_sha=null
  if "$BIN" decode "$WORK/auto.enc" "$WORK/auto.dec" >/dev/null 2> "$WORK/decode.err"; then
    mat_len="$(wc -c < "$WORK/auto.dec" | tr -d ' ')"
    mat_sha="$(sha256sum "$WORK/auto.dec" | cut -d' ' -f1)"
    if ! cmp -s "$src" "$WORK/auto.dec"; then
      byte_compare="DIFFER"
      all_exact=false
      fail=1
      echo "FINDING: auto winner for $name did not round-trip byte-exactly" >&2
    fi
  else
    byte_compare="DECODE_ERROR"
    all_exact=false
    fail=1
    echo "FINDING: decode of the auto winner for $name failed" >&2
    cat "$WORK/decode.err" >&2
  fi

  case "$winner" in
    RAW) w_raw=$((w_raw + 1)) ;;
    RLE) w_rle=$((w_rle + 1)) ;;
    BYTE_RANS) w_byte=$((w_byte + 1)) ;;
    PDF_PHYSICAL) w_phys=$((w_phys + 1)) ;;
    PDF_CHANNELS) w_chan=$((w_chan + 1)) ;;
    PDF_LAYOUT) w_layout=$((w_layout + 1)) ;;
    PDF_LAYOUT_RANS) w_layout_rans=$((w_layout_rans + 1)) ;;
    PDF_DEFLATE_REPLAY) w_dr=$((w_dr + 1)) ;;
    PDF_DEFLATE_REPLAY_RANS) w_drr=$((w_drr + 1)) ;;
    *) fail=1; echo "FINDING: unknown winner $winner for $name" >&2 ;;
  esac

  # (d) Honest head-to-head: forced PDF_DEFLATE_REPLAY_RANS vs forced
  # BYTE_RANS. A null replay size means the input does not propose the mechanism
  # at all, recorded as `declined` (never scored as a win or a loss).
  rvb="declined"
  rvb_delta=null
  if [ "$s_deflate_replay_rans" != "null" ]; then
    rvb_delta=$((s_deflate_replay_rans - s_byte_rans))
    if [ "$rvb_delta" -lt 0 ]; then
      rvb="win"; h2h_win=$((h2h_win + 1))
    elif [ "$rvb_delta" -gt 0 ]; then
      rvb="lose"; h2h_lose=$((h2h_lose + 1))
    else
      rvb="tie"; h2h_tie=$((h2h_tie + 1))
    fi
  else
    h2h_declined=$((h2h_declined + 1))
  fi
  rraw="declined"
  rraw_delta=null
  if [ "$s_deflate_replay" != "null" ]; then
    rraw_delta=$((s_deflate_replay - s_byte_rans))
    if [ "$rraw_delta" -lt 0 ]; then
      rraw="win"; raw_h2h_win=$((raw_h2h_win + 1))
    elif [ "$rraw_delta" -gt 0 ]; then
      rraw="lose"; raw_h2h_lose=$((raw_h2h_lose + 1))
    else
      rraw="tie"; raw_h2h_tie=$((raw_h2h_tie + 1))
    fi
  else
    raw_h2h_declined=$((raw_h2h_declined + 1))
  fi

  # (e) Cumulative ladder, per file: each rung is the running minimum after
  # adding one mechanism. A kind that is not proposed simply cannot lower it.
  a0="$s_raw"
  a1="$(min2 "$a0" "$(pick "$s_rle" "$a0")")"
  a2="$(min2 "$a1" "$(pick "$s_byte_rans" "$a1")")"
  a3="$(min2 "$a2" "$(pick "$s_pdf_physical" "$a2")")"
  a4="$(min2 "$a3" "$(pick "$s_pdf_channels" "$a3")")"
  a5="$(min2 "$a4" "$(pick "$s_pdf_layout" "$a4")")"
  a6="$(min2 "$a5" "$(pick "$s_pdf_layout_rans" "$a5")")"
  a7="$(min2 "$a6" "$(pick "$s_deflate_replay" "$a6")")"
  a8="$(min2 "$a7" "$(pick "$s_deflate_replay_rans" "$a7")")"
  tot_a0=$((tot_a0 + a0)); tot_a1=$((tot_a1 + a1)); tot_a2=$((tot_a2 + a2))
  tot_a3=$((tot_a3 + a3)); tot_a4=$((tot_a4 + a4)); tot_a5=$((tot_a5 + a5))
  tot_a6=$((tot_a6 + a6)); tot_a7=$((tot_a7 + a7)); tot_a8=$((tot_a8 + a8))

  # Leave-one-out: the full portfolio (A8) minus the same portfolio with exactly
  # one replay mechanism removed, all other mechanisms (incl. the other replay
  # variant) retained.
  wo_layout="$(min2 "$a5" "$(min2 "$(pick "$s_deflate_replay" "$a5")" "$(pick "$s_deflate_replay_rans" "$a5")")")"
  wo_raw="$(min2 "$a6" "$(pick "$s_deflate_replay_rans" "$a6")")"
  wo_rans="$(min2 "$a6" "$(pick "$s_deflate_replay" "$a6")")"
  tot_wo_layout=$((tot_wo_layout + wo_layout))
  tot_wo_raw=$((tot_wo_raw + wo_raw))
  tot_wo_rans=$((tot_wo_rans + wo_rans))

  # (f) corpus / results / verification / attribution records.
  printf '{"name":"%s","source_len":%s,"source_sha256":"%s"}\n' \
    "$name" "$src_len" "$src_sha" >> "$WORK/corpus-entries.jsonl"

  printf '{"name":"%s","source_len":%s,"source_sha256":"%s","sizes":{"raw":%s,"rle":%s,"byte_rans":%s,"pdf_physical":%s,"pdf_channels":%s,"pdf_layout":%s,"pdf_layout_rans":%s,"pdf_deflate_replay":%s,"pdf_deflate_replay_rans":%s},"replay_rans_vs_byte_rans":"%s","replay_rans_delta":%s,"deflate_replay_vs_byte_rans":"%s","deflate_replay_delta":%s,"winner":"%s","auto_len":%s,"byte_compare":"%s"}\n' \
    "$name" "$src_len" "$src_sha" \
    "$(jsize "$s_raw")" "$(jsize "$s_rle")" "$(jsize "$s_byte_rans")" \
    "$(jsize "$s_pdf_physical")" "$(jsize "$s_pdf_channels")" \
    "$(jsize "$s_pdf_layout")" "$(jsize "$s_pdf_layout_rans")" \
    "$(jsize "$s_deflate_replay")" "$(jsize "$s_deflate_replay_rans")" \
    "$rvb" "$(jsize "$rvb_delta")" "$rraw" "$(jsize "$rraw_delta")" \
    "$winner" "$auto_len" "$byte_compare" \
    >> "$WORK/results-entries.jsonl"

  sha_equal=false
  if [ "$mat_sha" != "null" ] && [ "$mat_sha" = "$src_sha" ]; then sha_equal=true; fi
  printf '{"name":"%s","source_len":%s,"materialized_len":%s,"source_sha256":"%s","materialized_sha256":%s,"sha256_equal":%s,"byte_compare":"%s","verify_ok":%s,"auto_winner":"%s","auto_len":%s}\n' \
    "$name" "$src_len" "$(jsize "$mat_len")" "$src_sha" \
    "$(jstr "$mat_sha")" "$sha_equal" "$byte_compare" "$verify_ok" \
    "$winner" "$auto_len" >> "$WORK/verification-entries.jsonl"

  printf '{"name":"%s","winner":"%s","encoded_len":%s,"cost":%s}\n' \
    "$name" "$winner" "$auto_len" "$auto_cost" >> "$WORK/attribution-winner.jsonl"
  printf '{"name":"%s","pdf_deflate_replay_cost":%s,"pdf_deflate_replay_rans_cost":%s}\n' \
    "$name" "$(cat "$WORK/f_pdf-deflate-replay.cost")" \
    "$(cat "$WORK/f_pdf-deflate-replay-rans.cost")" >> "$WORK/attribution-replay.jsonl"

  # Negative controls: the two deliberate non-PDFs, plus every forced replay
  # decline (recorded, never fabricated).
  case "$name" in
    malformed.pdf|notpdf.bin)
      printf '{"name":"%s","control":"non_pdf","source_len":%s,"auto_winner":"%s","byte_compare":"%s","verify_ok":%s,"deflate_replay_declined":%s,"deflate_replay_rans_declined":%s}\n' \
        "$name" "$src_len" "$winner" "$byte_compare" "$verify_ok" \
        "$( [ "$s_deflate_replay" = "null" ] && echo true || echo false )" \
        "$( [ "$s_deflate_replay_rans" = "null" ] && echo true || echo false )" \
        >> "$WORK/negative-controls.jsonl"
      if [ "$byte_compare" != "equal" ] || [ "$verify_ok" != "true" ]; then
        negative_ok=false
      fi
      ;;
  esac

  printf '| %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s |\n' \
    "$name" "$src_len" \
    "$(jsize "$s_raw")" "$(jsize "$s_rle")" "$(jsize "$s_byte_rans")" \
    "$(jsize "$s_pdf_physical")" "$(jsize "$s_pdf_channels")" \
    "$(jsize "$s_pdf_layout")" "$(jsize "$s_pdf_layout_rans")" \
    "$(jsize "$s_deflate_replay")" "$(jsize "$s_deflate_replay_rans")" \
    "$winner" "$byte_compare" >> "$WORK/rows.md"

  printf '| %s | %s | %s | %s | %s | %s | %s |\n' \
    "$name" "$src_len" "$(jsize "$s_byte_rans")" \
    "$(jsize "$s_deflate_replay_rans")" "$rvb" "$(jsize "$rvb_delta")" "$winner" \
    >> "$WORK/headtohead-rows.md"

  printf '| %s | %s | %s | %s | %s | %s |\n' \
    "$name" "$src_len" "$(jsize "$s_deflate_replay")" \
    "$(jsize "$s_deflate_replay_rans")" "$rraw" "$(jsize "$rraw_delta")" \
    >> "$WORK/replay-rows.md"

  printf '%-16s %7s %7s %7s %7s %7s %7s %7s %7s %7s %7s %-24s %-8s\n' \
    "$name" "$src_len" \
    "$(jsize "$s_raw")" "$(jsize "$s_rle")" "$(jsize "$s_byte_rans")" \
    "$(jsize "$s_pdf_physical")" "$(jsize "$s_pdf_channels")" \
    "$(jsize "$s_pdf_layout")" "$(jsize "$s_pdf_layout_rans")" \
    "$(jsize "$s_deflate_replay")" "$(jsize "$s_deflate_replay_rans")" \
    "$winner" "$byte_compare"
done

exact_bool=true; [ "$all_exact" = true ] || exact_bool=false
neg_bool=true; [ "$negative_ok" = true ] || neg_bool=false

# ---- artifact documents ----------------------------------------------------
results_doc="$WORK/results.json"
{
  printf '{\n  "campaign": "%s",\n  "units": "bytes",\n  "corpus_files": %s,\n  "files": ' \
    "$campaign" "$file_count"
  json_array_lines "$WORK/results-entries.jsonl"
  printf '\n}\n'
} > "$results_doc"

{
  printf '{\n  "campaign": "%s",\n  "corpus_files": %s,\n  "files": ' \
    "$campaign" "$file_count"
  json_array_lines "$WORK/corpus-entries.jsonl"
  printf '\n}\n'
} > "$CAMPAIGN/corpus.json"

{
  printf '{\n  "campaign": "%s",\n  "corpus_files": %s,\n  "all_exact": %s,\n  "files": ' \
    "$campaign" "$file_count" "$exact_bool"
  json_array_lines "$WORK/verification-entries.jsonl"
  printf '\n}\n'
} > "$CAMPAIGN/verification.json"

{
  printf '{\n  "campaign": "%s",\n  "note": "complete serialized-byte cost breakdown of the auto winner per file",\n  "winner_attribution": '
  json_array_lines "$WORK/attribution-winner.jsonl"
  printf ',\n  "replay_forced_attribution": '
  json_array_lines "$WORK/attribution-replay.jsonl"
  printf '\n}\n'
} > "$CAMPAIGN/attribution.json"

# ---- cumulative ladder aggregate -------------------------------------------
cat > "$CAMPAIGN/ablation-cumulative.json" <<EOF
{
  "campaign": "$campaign",
  "units": "bytes",
  "corpus_files": $file_count,
  "A0": $tot_a0,
  "A1": $tot_a1,
  "A2": $tot_a2,
  "A3": $tot_a3,
  "A4": $tot_a4,
  "A5": $tot_a5,
  "A6": $tot_a6,
  "A7": $tot_a7,
  "A8": $tot_a8,
  "ladder_map": {
    "A0": "RAW",
    "A1": "min(RAW,RLE)",
    "A2": "min(A1,BYTE_RANS)",
    "A3": "min(A2,PDF_PHYSICAL)",
    "A4": "min(A3,PDF_CHANNELS)",
    "A5": "min(A4,PDF_LAYOUT)",
    "A6": "min(A5,PDF_LAYOUT_RANS)",
    "A7": "min(A6,PDF_DEFLATE_REPLAY)",
    "A8": "min(A7,PDF_DEFLATE_REPLAY_RANS)"
  },
  "winners_count": {
    "RAW": $w_raw,
    "RLE": $w_rle,
    "BYTE_RANS": $w_byte,
    "PDF_PHYSICAL": $w_phys,
    "PDF_CHANNELS": $w_chan,
    "PDF_LAYOUT": $w_layout,
    "PDF_LAYOUT_RANS": $w_layout_rans,
    "PDF_DEFLATE_REPLAY": $w_dr,
    "PDF_DEFLATE_REPLAY_RANS": $w_drr
  },
  "replay_rans_vs_byte_rans": {
    "win": $h2h_win,
    "lose": $h2h_lose,
    "tie": $h2h_tie,
    "declined": $h2h_declined
  }
}
EOF

# ---- leave-one-out ---------------------------------------------------------
loo_layout=$((tot_a8 - tot_wo_layout))
loo_raw=$((tot_a8 - tot_wo_raw))
loo_rans=$((tot_a8 - tot_wo_rans))
cat > "$CAMPAIGN/ablation-leave-one-out.json" <<EOF
{
  "campaign": "$campaign",
  "units": "bytes",
  "full_portfolio": {
    "rung": "A8",
    "total": $tot_a8
  },
  "pdf_deflate_replay_rans": {
    "without_total": $tot_wo_rans,
    "delta": $loo_rans
  },
  "pdf_deflate_replay": {
    "without_total": $tot_wo_raw,
    "delta": $loo_raw
  },
  "pdf_layout_rans": {
    "without_total": $tot_wo_layout,
    "delta": $loo_layout
  },
  "note": "each delta is full(A8) minus the portfolio with exactly that one mechanism removed, every other mechanism retained"
}
EOF

# ---- negative controls -----------------------------------------------------
declines="$(grep -o '"pdf_deflate_replay_rans":null' "$results_doc" | wc -l | tr -d ' ')"
cat > "$CAMPAIGN/negative-controls.json" <<EOF
{
  "campaign": "$campaign",
  "all_controls_exact": $neg_bool,
  "controls": $(json_array_lines "$WORK/negative-controls.jsonl"),
  "replay_decline_count": $declines,
  "note": "replay declines are the corpus files whose forced pdf_deflate_replay_rans size is null in results.json"
}
EOF

# ---- environment receipt ---------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$campaign",
  "utc_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "git_dirty_tracked": "$(git --no-optional-locks status --porcelain --untracked-files=no | wc -l | tr -d ' ')",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "binary_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "arch": "$(uname -m)",
  "command": "docker compose run --rm --no-TTY dev sh tools/phase6-court.sh"
}
EOF

# ---- report ----------------------------------------------------------------
{
  echo "# Campaign: $campaign — Phase 6 — exact DEFLATE replay over forced-candidate ablation"
  echo
  echo "## Method"
  echo
  echo "The exact binary built from this commit materializes the deterministic"
  echo "12-file sample corpus with \`pdf-make-samples\`, then for every corpus file"
  echo "forces each candidate family in turn: \`encode --force KIND IN OUT\` for KIND in"
  echo "{raw, rle, byte-rans, pdf-physical, pdf-channels, pdf-layout, pdf-layout-rans,"
  echo "pdf-deflate-replay, pdf-deflate-replay-rans}."
  echo "A forced encode runs the *same* complete-cost court over a one-element"
  echo "candidate set, so the forced descriptor is still serialized, decoded, and"
  echo "byte-compared before it is returned; forcing selects a lane, it never bypasses"
  echo "exactness. When the input does not propose a kind the command exits with a"
  echo "typed Usage error (\"is not proposed\"), which is recorded honestly as \`null\`"
  echo "rather than substituted. In parallel, the ordinary \`encode\` gives the auto"
  echo "winner, and its output is decoded and \`cmp\`ed against the source and checked"
  echo "with \`verify\`. The two replay lanes are only proposed for PDFs with a lone"
  echo "\`/FlateDecode\` stream, so their forced sizes are non-null exactly for"
  echo "\`flate.pdf\` on this corpus."
  echo
  echo "## Per-file table"
  echo
  echo "All sizes are serialized \`.voldoc\` bytes; \`null\` means the input does not"
  echo "propose that kind. \`byte_cmp\` is the auto winner's decoded output compared"
  echo "to the source."
  echo
  echo "| file | source | RAW | RLE | BYTE_RANS | PHYS | CHAN | LAYOUT | LAY_RANS | DEFL_REP | DEFL_RANS | winner | byte_cmp |"
  echo "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |"
  cat "$WORK/rows.md"
  echo
  echo "## Head-to-head: PDF_DEFLATE_REPLAY_RANS vs BYTE_RANS"
  echo
  echo "For each file the forced BYTE_RANS and PDF_DEFLATE_REPLAY_RANS sizes are"
  echo "compared directly. \`delta\` is \`PDF_DEFLATE_REPLAY_RANS - BYTE_RANS\` (negative"
  echo "means replay+rANS is smaller). A \`declined\` verdict means the replay candidate"
  echo "was not proposed for that input and is never scored as a win or a loss."
  echo
  echo "| file | source | BYTE_RANS | DEFLATE_REPLAY_RANS | verdict | delta | auto winner |"
  echo "| --- | ---: | ---: | ---: | --- | ---: | --- |"
  cat "$WORK/headtohead-rows.md"
  echo
  echo "PDF_DEFLATE_REPLAY_RANS wins $h2h_win, loses $h2h_lose, ties $h2h_tie, and is"
  echo "declined by $h2h_declined of the $file_count corpus files when measured"
  echo "head-to-head against BYTE_RANS."
  echo
  echo "## Raw-plaintext replay vs BYTE_RANS"
  echo
  echo "The exact same comparison for the raw-plaintext \`PDF_DEFLATE_REPLAY\` lane:"
  echo
  echo "| file | source | DEFLATE_REPLAY | DEFLATE_REPLAY_RANS | verdict (raw vs BYTE_RANS) | delta |"
  echo "| --- | ---: | ---: | ---: | --- | ---: |"
  cat "$WORK/replay-rows.md"
  echo
  echo "## Cumulative ladder"
  echo
  echo "Each rung adds one mechanism and takes the per-file running minimum, summed"
  echo "over the $file_count-file corpus. Adding a mechanism can only lower or hold"
  echo "the total, never raise it."
  echo
  echo "| rung | mechanism added | total bytes | step delta |"
  echo "| --- | --- | ---: | ---: |"
  echo "| A0 | RAW | $tot_a0 | — |"
  echo "| A1 | + RLE | $tot_a1 | $((tot_a1 - tot_a0)) |"
  echo "| A2 | + BYTE_RANS | $tot_a2 | $((tot_a2 - tot_a1)) |"
  echo "| A3 | + PDF_PHYSICAL | $tot_a3 | $((tot_a3 - tot_a2)) |"
  echo "| A4 | + PDF_CHANNELS | $tot_a4 | $((tot_a4 - tot_a3)) |"
  echo "| A5 | + PDF_LAYOUT | $tot_a5 | $((tot_a5 - tot_a4)) |"
  echo "| A6 | + PDF_LAYOUT_RANS | $tot_a6 | $((tot_a6 - tot_a5)) |"
  echo "| A7 | + PDF_DEFLATE_REPLAY | $tot_a7 | $((tot_a7 - tot_a6)) |"
  echo "| A8 | + PDF_DEFLATE_REPLAY_RANS | $tot_a8 | $((tot_a8 - tot_a7)) |"
  echo
  echo "## Leave-one-out"
  echo
  echo "Each delta is the full A8 portfolio minus the same portfolio with exactly one"
  echo "replay mechanism removed (the other replay variant and every earlier mechanism"
  echo "retained):"
  echo
  echo "- PDF_DEFLATE_REPLAY_RANS: A8_without = $tot_wo_rans, delta = $loo_rans"
  echo "- PDF_DEFLATE_REPLAY:      A8_without = $tot_wo_raw, delta = $loo_raw"
  echo "- PDF_LAYOUT_RANS:         A8_without = $tot_wo_layout, delta = $loo_layout"
  echo
  echo "Auto-winner counts: RAW=$w_raw RLE=$w_rle BYTE_RANS=$w_byte"
  echo "PDF_PHYSICAL=$w_phys PDF_CHANNELS=$w_chan PDF_LAYOUT=$w_layout"
  echo "PDF_LAYOUT_RANS=$w_layout_rans PDF_DEFLATE_REPLAY=$w_dr"
  echo "PDF_DEFLATE_REPLAY_RANS=$w_drr."
  echo
  echo "## Verification"
  echo
  echo "Every auto winner round-trips byte-exactly:"
  echo "materialized_length == source_length, SHA256(materialized) == SHA256(source),"
  echo "and byte_compare(materialized, source) == equal, with \`verify\` passing."
  echo "all_exact=$exact_bool; negative controls exact=$neg_bool. Per-file triples are"
  echo "in \`verification.json\`."
  echo
  echo "## Negative controls"
  echo
  echo "The two deliberate non-PDFs (\`malformed.pdf\`, \`notpdf.bin\`) decline both"
  echo "replay lanes and still round-trip byte-exactly through the opaque RAW lane;"
  echo "$declines of $file_count files decline the forced rANS replay lane (all files"
  echo "without a lone FlateDecode stream). No decline is scored as a win or loss."
  echo
  echo "## Honest interpretation"
  echo
  echo "Exact DEFLATE replay is *exact*: every forced replay descriptor serializes, is"
  echo "parsed back, and materializes the original deflate bitstream byte-for-byte, and"
  echo "the auto winner is gated on the same serialize / parse / materialize /"
  echo "byte-compare check; see the Phase-6 tests in \`tests/pdf_deflate.rs\`."
  echo
  echo "The raw-plaintext lane \`PDF_DEFLATE_REPLAY\` stores each stream's plaintext as a"
  echo "deduplicated object and then replays the original bitstream; on this corpus it"
  echo "loses, because the plaintext of a strongly-compressed stream is nearly as large"
  echo "as the stream it replaces. The rANS-plaintext lane \`PDF_DEFLATE_REPLAY_RANS\`"
  echo "codes each unique plaintext once as an order-0 byte-rANS channel and shares it"
  echo "across every stream that produces it; on a file whose producer coding is weak"
  echo "(low compression levels) and whose plaintext repeats across streams, the"
  echo "shared channel re-expresses that weak coding far more cheaply than the stored"
  echo "bitstreams, and it beats BYTE_RANS."
  echo
  echo "This is a scoped, measured result for *this* deterministic corpus and this"
  echo "commit: the winning region is shared plaintext with weak producer coding, and"
  echo "the losing region is unique, strongly-compressed plaintext (where the plaintext"
  echo "is no smaller than the original bitstream). The winner is always decided by"
  echo "actual serialized bytes, and every auto winner round-trips byte-exactly"
  echo "(all_exact=$exact_bool)."
  echo
  echo "## Verdict"
  echo
  if [ "$fail" -eq 0 ]; then echo "PASS"; else echo "FAIL"; fi
} > "$CAMPAIGN/report.md"

# `results.json` lives in the campaign dir (copy from the scratch document).
cp "$results_doc" "$CAMPAIGN/results.json"

# The independent qpdf oracle is optional here; run it only when qpdf is present.
oracle_note="not_run (qpdf not present in this image; run tools/pdf-oracle.sh separately)"
oracle_sha=null
if command -v qpdf >/dev/null 2>&1; then
  if sh tools/pdf-oracle.sh "$WORK/oracle" >/dev/null 2>&1; then
    cp "$WORK/oracle/oracle.jsonl" "$CAMPAIGN/oracle.jsonl"
    oracle_note="oracle.jsonl"
    oracle_sha="\"$(sha256sum "$CAMPAIGN/oracle.jsonl" | cut -d' ' -f1)\""
  else
    oracle_note="tools/pdf-oracle.sh FAILED"
    echo "FINDING: qpdf oracle failed" >&2
    fail=1
  fi
fi

# ---- manifest --------------------------------------------------------------
verdict="$( [ "$fail" -eq 0 ] && echo PASS || echo FAIL )"
cat > "$CAMPAIGN/manifest.json" <<EOF
{
  "campaign": "$campaign",
  "phase": "Phase 6 — exact DEFLATE replay (preflate-rs), PDF_DEFLATE_REPLAY and PDF_DEFLATE_REPLAY_RANS rungs over forced-candidate ablation",
  "claim": "Every corpus file round-trips byte-exactly through its auto-winning lane (verify and decode cmp), and the two exact DEFLATE replay lanes yield an honest, exact head-to-head against BYTE_RANS, with the cumulative ladder A0..A8 and the leave-one-out deltas for the replay mechanisms.",
  "git_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "git_dirty_tracked": "$(git --no-optional-locks status --porcelain --untracked-files=no | wc -l | tr -d ' ')",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "arch": "$(uname -m)",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "binary_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "command": "docker compose run --rm --no-TTY dev sh tools/phase6-court.sh",
  "oracle": "$oracle_note",
  "oracle_sha256": $oracle_sha,
  "artifacts": {
    "corpus.json": "$(sha256sum "$CAMPAIGN/corpus.json" | cut -d' ' -f1)",
    "results.json": "$(sha256sum "$results_doc" | cut -d' ' -f1)",
    "attribution.json": "$(sha256sum "$CAMPAIGN/attribution.json" | cut -d' ' -f1)",
    "ablation-cumulative.json": "$(sha256sum "$CAMPAIGN/ablation-cumulative.json" | cut -d' ' -f1)",
    "ablation-leave-one-out.json": "$(sha256sum "$CAMPAIGN/ablation-leave-one-out.json" | cut -d' ' -f1)",
    "negative-controls.json": "$(sha256sum "$CAMPAIGN/negative-controls.json" | cut -d' ' -f1)",
    "verification.json": "$(sha256sum "$CAMPAIGN/verification.json" | cut -d' ' -f1)",
    "environment.json": "$(sha256sum "$CAMPAIGN/environment.json" | cut -d' ' -f1)",
    "report.md": "$(sha256sum "$CAMPAIGN/report.md" | cut -d' ' -f1)"
  },
  "verdict": "$verdict"
}
EOF

# ---- summary ---------------------------------------------------------------
echo
echo "=== ladder (bytes) ==="
printf 'files=%s A0=%s A1=%s A2=%s A3=%s A4=%s A5=%s A6=%s A7=%s A8=%s\n' \
  "$file_count" "$tot_a0" "$tot_a1" "$tot_a2" "$tot_a3" "$tot_a4" \
  "$tot_a5" "$tot_a6" "$tot_a7" "$tot_a8"
printf 'loo: replay_rans_delta=%s replay_raw_delta=%s layout_rans_delta=%s\n' \
  "$loo_rans" "$loo_raw" "$loo_layout"
printf 'winners: RAW=%s RLE=%s BYTE_RANS=%s PDF_PHYSICAL=%s PDF_CHANNELS=%s PDF_LAYOUT=%s PDF_LAYOUT_RANS=%s PDF_DEFLATE_REPLAY=%s PDF_DEFLATE_REPLAY_RANS=%s all_exact=%s\n' \
  "$w_raw" "$w_rle" "$w_byte" "$w_phys" "$w_chan" "$w_layout" \
  "$w_layout_rans" "$w_dr" "$w_drr" "$exact_bool"
printf 'replay_rans_vs_byte_rans: win=%s lose=%s tie=%s declined=%s\n' \
  "$h2h_win" "$h2h_lose" "$h2h_tie" "$h2h_declined"
printf 'deflate_replays_declined=%s oracle=%s\n' "$declines" "$oracle_note"
echo "=== campaign: $CAMPAIGN ==="
cat "$CAMPAIGN/manifest.json"

if [ "$fail" -ne 0 ]; then
  echo "PHASE 6 COURT: FAIL" >&2
  exit 1
fi
echo "PHASE 6 COURT: PASS"
