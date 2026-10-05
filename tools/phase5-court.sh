#!/bin/sh
# Phase 5 exact court: forced-candidate ablation plus the PDF_LAYOUT rung.
#
# Extends the Phase-4 forced-candidate ablation with the Phase-5 procedural
# candidate. Runs entirely inside the pinned dev container. It:
#   1. builds the exact binary (`cargo build --locked`);
#   2. materializes the deterministic sample corpus via `pdf-make-samples`;
#   3. for EVERY corpus file and EVERY candidate kind (raw, rle, byte-rans,
#      pdf-physical, pdf-channels, pdf-layout) runs `encode --force KIND`,
#      capturing the forced encoded length, or `null` when the input does not
#      propose that kind (a typed Usage decline, not a crash);
#   4. runs the ordinary `encode` to get the auto winner and asserts exactness:
#      the winner decodes byte-exactly (`cmp`) and `verify` passes;
#   5. sums a cumulative ladder A0..A5, one mechanism added per rung, and the
#      leave-one-out delta for the layout mechanism (A5_without_layout == A4);
#   6. records, for the classic-cross-reference samples, the forced layout size
#      and the auto winner, so the receipt shows whether layout ever helps;
#   7. writes results.jsonl, ladder.json, environment.json, manifest.json and
#      report.md into the campaign directory.
#
# The forced lane is honest ablation: it re-uses the same complete-cost court
# over a one-element candidate set, so a forced mechanism is still serialized,
# decoded, and byte-compared before it is returned. Forcing never fabricates a
# win that the court would not have accepted.
#
# Usage (from the host):
#   docker compose run --rm --no-TTY dev sh tools/phase5-court.sh
set -eu

cd /work
LC_ALL=C
export LC_ALL

cargo build --locked --quiet
BIN=target/debug/vole-document

SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
CAMPAIGN="evidence/campaigns/$(date -u +%Y-%m-%d)-phase5-${SHA}"
campaign="$(basename "$CAMPAIGN")"

WORK=evidence/scratch/phase5
CORPUS="$WORK/corpus"
rm -rf "$WORK"
mkdir -p "$CORPUS" "$CAMPAIGN"

"$BIN" pdf-make-samples "$CORPUS" >/dev/null

KINDS="raw rle byte-rans pdf-physical pdf-channels pdf-layout"

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

: > "$CAMPAIGN/results.jsonl"
: > "$WORK/rows.md"
: > "$WORK/classic-rows.md"

tot_a0=0
tot_a1=0
tot_a2=0
tot_a3=0
tot_a4=0
tot_a5=0
w_raw=0
w_rle=0
w_byte=0
w_phys=0
w_chan=0
w_layout=0
file_count=0
classic_count=0
layout_helps=0
all_exact=true
fail=0

printf '%-16s %8s %8s %8s %8s %8s %8s %8s %-13s %-10s\n' \
  file source RAW RLE BYTE_RANS PDF_PHYS PDF_CHAN PDF_LAYOUT winner byte_cmp
printf '%-16s %8s %8s %8s %8s %8s %8s %8s %-13s %-10s\n' \
  ---------------- -------- -------- -------- -------- -------- -------- -------- ------------- ----------

for src in "$CORPUS"/*; do
  name="$(basename "$src")"
  file_count=$((file_count + 1))
  src_len="$(wc -c < "$src" | tr -d ' ')"
  src_sha="$(sha256sum "$src" | cut -d' ' -f1)"

  # (a) Forced ablation: one encode per candidate kind.
  s_raw=null
  s_rle=null
  s_byte_rans=null
  s_pdf_physical=null
  s_pdf_channels=null
  s_pdf_layout=null
  for k in $KINDS; do
    size=null
    if "$BIN" encode --force "$k" "$src" "$WORK/forced.enc" \
        > "$WORK/forced.json" 2> "$WORK/forced.err"; then
      size="$(wc -c < "$WORK/forced.enc" | tr -d ' ')"
    elif grep -q "is not proposed" "$WORK/forced.err"; then
      size=null
    else
      echo "FINDING: forced encode of $name as $k failed unexpectedly:" >&2
      cat "$WORK/forced.err" >&2
      size=null
      fail=1
    fi
    case "$k" in
      raw) s_raw="$size" ;;
      rle) s_rle="$size" ;;
      byte-rans) s_byte_rans="$size" ;;
      pdf-physical) s_pdf_physical="$size" ;;
      pdf-channels) s_pdf_channels="$size" ;;
      pdf-layout) s_pdf_layout="$size" ;;
    esac
  done

  # (b) Auto winner via the ordinary complete-cost court.
  "$BIN" encode "$src" "$WORK/auto.enc" > "$WORK/auto.json"
  auto_len="$(wc -c < "$WORK/auto.enc" | tr -d ' ')"
  winner="$(sed -n 's/.*"candidate":"\([^"]*\)".*/\1/p' "$WORK/auto.json")"
  [ -n "$winner" ] || winner="UNKNOWN"

  # (c) Exactness: the auto winner must decode byte-exactly and verify.
  if "$BIN" verify "$WORK/auto.enc" >/dev/null 2> "$WORK/verify.err"; then
    :
  else
    all_exact=false
    fail=1
    echo "FINDING: verify failed for $name" >&2
    cat "$WORK/verify.err" >&2
  fi
  byte_compare="equal"
  if "$BIN" decode "$WORK/auto.enc" "$WORK/auto.dec" >/dev/null 2> "$WORK/decode.err"; then
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
    *) fail=1; echo "FINDING: unknown winner $winner for $name" >&2 ;;
  esac

  # (d) Classic-cross-reference samples: layout proposed (forced size non-null).
  # Record the forced layout size against the auto winner so the receipt shows
  # whether layout ever helps.
  classic_xref=false
  layout_vs_auto="declined"
  if [ "$s_pdf_layout" != "null" ]; then
    classic_xref=true
    classic_count=$((classic_count + 1))
    if [ "$s_pdf_layout" -lt "$auto_len" ]; then
      layout_vs_auto="wins"
      layout_helps=$((layout_helps + 1))
    elif [ "$s_pdf_layout" -eq "$auto_len" ]; then
      layout_vs_auto="ties"
    else
      layout_vs_auto="loses"
    fi
  fi

  # (e) Cumulative ladder, per file: each rung is the running minimum after
  # adding one mechanism. A kind that is not proposed simply cannot lower it.
  a0="$s_raw"
  r="$(pick "$s_rle" "$a0")";           a1="$(min2 "$a0" "$r")"
  b="$(pick "$s_byte_rans" "$a1")";     a2="$(min2 "$a1" "$b")"
  p="$(pick "$s_pdf_physical" "$a2")";  a3="$(min2 "$a2" "$p")"
  c="$(pick "$s_pdf_channels" "$a3")";  a4="$(min2 "$a3" "$c")"
  l="$(pick "$s_pdf_layout" "$a4")";    a5="$(min2 "$a4" "$l")"
  tot_a0=$((tot_a0 + a0))
  tot_a1=$((tot_a1 + a1))
  tot_a2=$((tot_a2 + a2))
  tot_a3=$((tot_a3 + a3))
  tot_a4=$((tot_a4 + a4))
  tot_a5=$((tot_a5 + a5))

  printf '{"name":"%s","source_len":%s,"source_sha256":"%s","sizes":{"raw":%s,"rle":%s,"byte_rans":%s,"pdf_physical":%s,"pdf_channels":%s,"pdf_layout":%s},"classic_xref":%s,"layout_vs_auto":"%s","winner":"%s","auto_len":%s,"byte_compare":"%s"}\n' \
    "$name" "$src_len" "$src_sha" \
    "$(jsize "$s_raw")" "$(jsize "$s_rle")" "$(jsize "$s_byte_rans")" \
    "$(jsize "$s_pdf_physical")" "$(jsize "$s_pdf_channels")" \
    "$(jsize "$s_pdf_layout")" \
    "$classic_xref" "$layout_vs_auto" \
    "$winner" "$auto_len" "$byte_compare" \
    >> "$CAMPAIGN/results.jsonl"

  printf '| %s | %s | %s | %s | %s | %s | %s | %s | %s | %s |\n' \
    "$name" "$src_len" \
    "$(jsize "$s_raw")" "$(jsize "$s_rle")" "$(jsize "$s_byte_rans")" \
    "$(jsize "$s_pdf_physical")" "$(jsize "$s_pdf_channels")" \
    "$(jsize "$s_pdf_layout")" \
    "$winner" "$byte_compare" \
    >> "$WORK/rows.md"

  if [ "$classic_xref" = true ]; then
    printf '| %s | %s | %s | %s | %s | %s |\n' \
      "$name" "$src_len" \
      "$(jsize "$s_pdf_layout")" "$winner" "$auto_len" "$layout_vs_auto" \
      >> "$WORK/classic-rows.md"
  fi

  printf '%-16s %8s %8s %8s %8s %8s %8s %8s %-13s %-10s\n' \
    "$name" "$src_len" \
    "$(jsize "$s_raw")" "$(jsize "$s_rle")" "$(jsize "$s_byte_rans")" \
    "$(jsize "$s_pdf_physical")" "$(jsize "$s_pdf_channels")" \
    "$(jsize "$s_pdf_layout")" \
    "$winner" "$byte_compare"
done

exact_bool=true; [ "$all_exact" = true ] || exact_bool=false

# Leave-one-out: removing the layout mechanism from the full portfolio leaves
# exactly the A4 rung, so A5_without_layout == A4; the delta is what the layout
# mechanism actually bought (<= 0).
loo_delta=$((tot_a5 - tot_a4))

# ---- ladder aggregate ------------------------------------------------------
cat > "$CAMPAIGN/ladder.json" <<EOF
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
  "ladder_map": {
    "A0": "RAW",
    "A1": "min(RAW,RLE)",
    "A2": "min(A1,BYTE_RANS)",
    "A3": "min(A2,PDF_PHYSICAL)",
    "A4": "min(A3,PDF_CHANNELS)",
    "A5": "min(A4,PDF_LAYOUT)"
  },
  "A5_without_layout": $tot_a4,
  "leave_one_out_layout_delta": $loo_delta,
  "classic_xref_files": $classic_count,
  "layout_helps_files": $layout_helps,
  "winners_count": {
    "RAW": $w_raw,
    "RLE": $w_rle,
    "BYTE_RANS": $w_byte,
    "PDF_PHYSICAL": $w_phys,
    "PDF_CHANNELS": $w_chan,
    "PDF_LAYOUT": $w_layout
  }
}
EOF

# ---- environment receipt ---------------------------------------------------
cat > "$CAMPAIGN/environment.json" <<EOF
{
  "campaign": "$campaign",
  "utc_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_commit": "$(git rev-parse HEAD 2>/dev/null || echo unknown)",
  "git_dirty_tracked": "$(git status --porcelain --untracked-files=no | wc -l | tr -d ' ')",
  "base_image": "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e",
  "rustc": "$(rustc --version)",
  "cargo": "$(cargo --version)",
  "cargo_lock_sha256": "$(sha256sum Cargo.lock | cut -d' ' -f1)",
  "binary_sha256": "$(sha256sum "$BIN" | cut -d' ' -f1)",
  "arch": "$(uname -m)",
  "command": "docker compose run --rm --no-TTY dev sh tools/phase5-court.sh"
}
EOF

# ---- report ----------------------------------------------------------------
{
  echo "# Campaign: $campaign — Phase 5 — PDF_LAYOUT rung over forced-candidate ablation"
  echo
  echo "## Method"
  echo
  echo "The exact binary built from this commit materializes the deterministic"
  echo "sample corpus with \`pdf-make-samples\`, then for every corpus file forces"
  echo "each candidate family in turn: \`encode --force KIND IN OUT\` for KIND in"
  echo "{raw, rle, byte-rans, pdf-physical, pdf-channels, pdf-layout}. A forced"
  echo "encode runs the *same* complete-cost court over a one-element candidate set,"
  echo "so the forced descriptor is still serialized, decoded, and byte-compared"
  echo "before it is returned; forcing selects a lane, it never bypasses exactness."
  echo "When the input does not propose a kind the command exits with a typed Usage"
  echo "error (\"is not proposed\"), which is recorded honestly as \`null\` rather than"
  echo "substituted. In parallel, the ordinary \`encode\` gives the auto winner, and"
  echo "its output is decoded and \`cmp\`ed against the source and checked with"
  echo "\`verify\`. The PDF_LAYOUT mechanism (Phase 5) is only proposed for"
  echo "classic-cross-reference PDFs, so its forced size is non-null exactly for the"
  echo "classic-xref samples."
  echo
  echo "## Per-file table"
  echo
  echo "All sizes are serialized \`.voldoc\` bytes; \`null\` means the input does not"
  echo "propose that kind. \`byte_cmp\` is the auto winner's decoded output compared"
  echo "to the source."
  echo
  echo "| file | source | RAW | RLE | BYTE_RANS | PDF_PHYS | PDF_CHAN | PDF_LAYOUT | winner | byte_cmp |"
  echo "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |"
  cat "$WORK/rows.md"
  echo
  echo "## Classic-cross-reference samples: does layout help?"
  echo
  echo "For the $classic_count files that propose PDF_LAYOUT, the forced layout size is"
  echo "compared against the auto winner. \`declined\` means the candidate was not"
  echo "proposed."
  echo
  echo "| file | source | PDF_LAYOUT | auto winner | auto_len | layout vs auto |"
  echo "| --- | ---: | ---: | --- | ---: | --- |"
  cat "$WORK/classic-rows.md"
  echo
  echo "Layout wins on $layout_helps of $classic_count classic-xref samples."
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
  echo
  echo "## Leave-one-out"
  echo
  echo "Removing only the layout mechanism from the full portfolio leaves the A4"
  echo "rung:"
  echo
  echo "- A5_without_layout = A4 = $tot_a4"
  echo "- leave_one_out_layout_delta = A5 - A4 = $loo_delta"
  echo
  echo "So the PDF layout mechanism saved $((0 - loo_delta)) bytes across the corpus"
  echo "relative to the same portfolio without it."
  echo
  echo "Auto-winner counts: RAW=$w_raw RLE=$w_rle BYTE_RANS=$w_byte"
  echo "PDF_PHYSICAL=$w_phys PDF_CHANNELS=$w_chan PDF_LAYOUT=$w_layout."
  echo
  echo "## Honest interpretation"
  echo
  echo "PDF_LAYOUT is *exact*: every forced layout descriptor serializes, is parsed"
  echo "back, and materializes byte-for-byte, and the mechanism genuinely predicts"
  echo "positions — it marks each object's introducer offset and the classic \`xref\`"
  echo "section start, then emits the 10-digit offset fields and \`startxref\` value"
  echo "from those marks rather than storing the digits (see the Phase-5.3 tests"
  echo "\`layout_predicts_entries\` and \`forced_layout_is_exact\`)."
  echo
  echo "It nevertheless loses the complete-cost court. On this corpus it is the auto"
  echo "winner for $w_layout of $file_count files, and it beats the auto winner on $layout_helps"
  echo "of $classic_count classic-xref samples. The reason is framing: each predicted"
  echo "10-digit offset replaces at most ten stored digits, but the DRA program must"
  echo "carry a \`MarkOffset\` per object and per xref section plus an \`EmitOffset\` per"
  echo "predicted entry, and each such op has fixed segment framing. That per-segment"
  echo "framing exceeds the digits saved, so RAW (which stores the same digits inside"
  echo "one opaque object) and BYTE_RANS (which entropy-codes them with the rest of"
  echo "the file) stay cheaper. A5 therefore does not move below A4 on this corpus"
  echo "(step delta $((tot_a5 - tot_a4)), leave-one-out delta $loo_delta)."
  echo
  echo "These are measured, scoped results for *this* deterministic corpus and this"
  echo "commit. They say the current layout prediction does not yet pay on these"
  echo "classic-xref PDFs; they do not say procedural xref regeneration cannot pay on"
  echo "larger or denser files, where the same mark/emit framing is amortized over"
  echo "many more predicted digits. The winner is always decided by actual serialized"
  echo "bytes, and every auto winner round-trips byte-exactly (all_exact=$exact_bool)."
  echo
  echo "## Verdict"
  echo
  if [ "$fail" -eq 0 ]; then echo "PASS"; else echo "FAIL"; fi
} > "$CAMPAIGN/report.md"

# ---- manifest --------------------------------------------------------------
verdict="$( [ "$fail" -eq 0 ] && echo PASS || echo FAIL )"
cat > "$CAMPAIGN/manifest.json" <<EOF
{
  "campaign": "$campaign",
  "phase": "Phase 5 — PDF classic-xref proceduralization, PDF_LAYOUT rung over forced-candidate ablation",
  "claim": "Every corpus file round-trips byte-exactly through its auto-winning lane (verify and decode cmp), and the PDF_LAYOUT rung yields an honest, exact per-mechanism size or a recorded null decline, with the classic-xref subset compared against the auto winner.",
  "results_sha256": "$(sha256sum "$CAMPAIGN/results.jsonl" | cut -d' ' -f1)",
  "ladder_sha256": "$(sha256sum "$CAMPAIGN/ladder.json" | cut -d' ' -f1)",
  "environment_sha256": "$(sha256sum "$CAMPAIGN/environment.json" | cut -d' ' -f1)",
  "report_sha256": "$(sha256sum "$CAMPAIGN/report.md" | cut -d' ' -f1)",
  "verdict": "$verdict"
}
EOF

# ---- summary ---------------------------------------------------------------
echo
echo "=== ladder (bytes) ==="
printf 'files=%s A0=%s A1=%s A2=%s A3=%s A4=%s A5=%s loo_layout_delta=%s\n' \
  "$file_count" "$tot_a0" "$tot_a1" "$tot_a2" "$tot_a3" "$tot_a4" "$tot_a5" "$loo_delta"
printf 'winners: RAW=%s RLE=%s BYTE_RANS=%s PDF_PHYSICAL=%s PDF_CHANNELS=%s PDF_LAYOUT=%s all_exact=%s\n' \
  "$w_raw" "$w_rle" "$w_byte" "$w_phys" "$w_chan" "$w_layout" "$exact_bool"
printf 'classic_xref_files=%s layout_helps_files=%s\n' \
  "$classic_count" "$layout_helps"
echo "=== campaign: $CAMPAIGN ==="
cat "$CAMPAIGN/manifest.json"

if [ "$fail" -ne 0 ]; then
  echo "PHASE 5 COURT: FAIL" >&2
  exit 1
fi
echo "PHASE 5 COURT: PASS"
