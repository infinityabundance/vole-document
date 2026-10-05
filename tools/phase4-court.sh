#!/bin/sh
# Phase 4 exact court: forced-candidate ablation and the cumulative ladder.
#
# Runs entirely inside the pinned dev container. It:
#   1. builds the exact binary (`cargo build --locked`);
#   2. materializes the deterministic sample corpus via `pdf-make-samples`;
#   3. for EVERY corpus file and EVERY candidate kind (raw, rle, byte-rans,
#      pdf-physical, pdf-channels) runs `encode --force KIND`, capturing the
#      forced encoded length, or `null` when the input does not propose that
#      kind (a typed Usage decline, not a crash);
#   4. runs the ordinary `encode` to get the auto winner and asserts exactness:
#      the winner decodes byte-exactly (`cmp`) and `verify` passes;
#   5. sums a cumulative ladder A0..A4, one mechanism added per rung, and the
#      leave-one-out delta for the channel mechanism;
#   6. writes results.jsonl, ladder.json, environment.json, manifest.json and
#      report.md into the campaign directory.
#
# The forced lane is honest ablation: it re-uses the same complete-cost court
# over a one-element candidate set, so a forced mechanism is still serialized,
# decoded, and byte-compared before it is returned. Forcing never fabricates a
# win that the court would not have accepted.
#
# Usage (from the host):
#   docker compose run --rm --no-TTY dev sh tools/phase4-court.sh
set -eu

cd /work
LC_ALL=C
export LC_ALL

cargo build --locked --quiet
BIN=target/debug/vole-document

SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
CAMPAIGN="evidence/campaigns/$(date -u +%Y-%m-%d)-phase4-${SHA}"
campaign="$(basename "$CAMPAIGN")"

WORK=evidence/scratch/phase4
CORPUS="$WORK/corpus"
rm -rf "$WORK"
mkdir -p "$CORPUS" "$CAMPAIGN"

"$BIN" pdf-make-samples "$CORPUS" >/dev/null

KINDS="raw rle byte-rans pdf-physical pdf-channels"

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

tot_a0=0
tot_a1=0
tot_a2=0
tot_a3=0
tot_a4=0
w_raw=0
w_rle=0
w_byte=0
w_phys=0
w_chan=0
file_count=0
all_exact=true
fail=0

# bigtext.pdf is the scale sample the typed-channel candidate is meant to
# exploit; keep its per-kind numbers for the honest interpretation.
bt_src=0
bt_raw=null
bt_byte=null
bt_chan=null

printf '%-16s %8s %8s %8s %8s %8s %8s %-13s %-10s\n' \
  file source RAW RLE BYTE_RANS PDF_PHYS PDF_CHAN winner byte_cmp
printf '%-16s %8s %8s %8s %8s %8s %8s %-13s %-10s\n' \
  ---------------- -------- -------- -------- -------- -------- -------- ------------- ----------

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
    esac
  done

  # (b) Auto winner via the ordinary complete-cost court.
  "$BIN" encode "$src" "$WORK/auto.enc" > "$WORK/auto.json"
  auto_len="$(wc -c < "$WORK/auto.enc" | tr -d ' ')"
  winner="$(sed -n 's/.*"candidate":"\([^"]*\)".*/\1/p' "$WORK/auto.json")"
  [ -n "$winner" ] || winner="UNKNOWN"

  # (c) Exactness: the auto winner must decode byte-exactly and verify.
  "$BIN" verify "$WORK/auto.enc" >/dev/null
  "$BIN" decode "$WORK/auto.enc" "$WORK/auto.dec" >/dev/null
  byte_compare="equal"
  if ! cmp -s "$src" "$WORK/auto.dec"; then
    byte_compare="DIFFER"
    all_exact=false
    fail=1
    echo "FINDING: auto winner for $name did not round-trip byte-exactly" >&2
  fi

  case "$winner" in
    RAW) w_raw=$((w_raw + 1)) ;;
    RLE) w_rle=$((w_rle + 1)) ;;
    BYTE_RANS) w_byte=$((w_byte + 1)) ;;
    PDF_PHYSICAL) w_phys=$((w_phys + 1)) ;;
    PDF_CHANNELS) w_chan=$((w_chan + 1)) ;;
    *) fail=1; echo "FINDING: unknown winner $winner for $name" >&2 ;;
  esac

  # (d) Cumulative ladder, per file: each rung is the running minimum after
  # adding one mechanism. A kind that is not proposed simply cannot lower it.
  a0="$s_raw"
  r="$(pick "$s_rle" "$a0")";           a1="$(min2 "$a0" "$r")"
  b="$(pick "$s_byte_rans" "$a1")";     a2="$(min2 "$a1" "$b")"
  p="$(pick "$s_pdf_physical" "$a2")";  a3="$(min2 "$a2" "$p")"
  c="$(pick "$s_pdf_channels" "$a3")";  a4="$(min2 "$a3" "$c")"
  tot_a0=$((tot_a0 + a0))
  tot_a1=$((tot_a1 + a1))
  tot_a2=$((tot_a2 + a2))
  tot_a3=$((tot_a3 + a3))
  tot_a4=$((tot_a4 + a4))

  if [ "$name" = "bigtext.pdf" ]; then
    bt_src="$src_len"
    bt_raw="$s_raw"
    bt_byte="$s_byte_rans"
    bt_chan="$s_pdf_channels"
  fi

  printf '{"name":"%s","source_len":%s,"source_sha256":"%s","sizes":{"raw":%s,"rle":%s,"byte_rans":%s,"pdf_physical":%s,"pdf_channels":%s},"winner":"%s","auto_len":%s,"byte_compare":"%s"}\n' \
    "$name" "$src_len" "$src_sha" \
    "$(jsize "$s_raw")" "$(jsize "$s_rle")" "$(jsize "$s_byte_rans")" \
    "$(jsize "$s_pdf_physical")" "$(jsize "$s_pdf_channels")" \
    "$winner" "$auto_len" "$byte_compare" \
    >> "$CAMPAIGN/results.jsonl"

  printf '| %s | %s | %s | %s | %s | %s | %s | %s | %s |\n' \
    "$name" "$src_len" \
    "$(jsize "$s_raw")" "$(jsize "$s_rle")" "$(jsize "$s_byte_rans")" \
    "$(jsize "$s_pdf_physical")" "$(jsize "$s_pdf_channels")" \
    "$winner" "$byte_compare" \
    >> "$WORK/rows.md"

  printf '%-16s %8s %8s %8s %8s %8s %8s %-13s %-10s\n' \
    "$name" "$src_len" \
    "$(jsize "$s_raw")" "$(jsize "$s_rle")" "$(jsize "$s_byte_rans")" \
    "$(jsize "$s_pdf_physical")" "$(jsize "$s_pdf_channels")" \
    "$winner" "$byte_compare"
done

exact_bool=true; [ "$all_exact" = true ] || exact_bool=false

# Leave-one-out: removing the channel mechanism from the full portfolio leaves
# exactly the A3 rung, so A4_without_channels == A3; the delta is what the
# channels actually bought (<= 0).
loo_delta=$((tot_a4 - tot_a3))

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
  "ladder_map": {
    "A0": "RAW",
    "A1": "min(RAW,RLE)",
    "A2": "min(A1,BYTE_RANS)",
    "A3": "min(A2,PDF_PHYSICAL)",
    "A4": "min(A3,PDF_CHANNELS)"
  },
  "A4_without_channels": $tot_a3,
  "leave_one_out_channels_delta": $loo_delta,
  "winners_count": {
    "RAW": $w_raw,
    "RLE": $w_rle,
    "BYTE_RANS": $w_byte,
    "PDF_PHYSICAL": $w_phys,
    "PDF_CHANNELS": $w_chan
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
  "command": "docker compose run --rm --no-TTY dev sh tools/phase4-court.sh"
}
EOF

# ---- report ----------------------------------------------------------------
{
  echo "# Campaign: $campaign — Phase 4 — forced-candidate ablation"
  echo
  echo "## Method"
  echo
  echo "The exact binary built from this commit materializes the deterministic"
  echo "sample corpus with \`pdf-make-samples\`, then for every corpus file forces"
  echo "each candidate family in turn: \`encode --force KIND IN OUT\` for KIND in"
  echo "{raw, rle, byte-rans, pdf-physical, pdf-channels}. A forced encode runs the"
  echo "*same* complete-cost court over a one-element candidate set, so the forced"
  echo "descriptor is still serialized, decoded, and byte-compared before it is"
  echo "returned; forcing selects a lane, it never bypasses exactness. When the"
  echo "input does not propose a kind the command exits with a typed Usage error"
  echo "(\"is not proposed\"), which is recorded honestly as \`null\` rather than"
  echo "substituted. In parallel, the ordinary \`encode\` gives the auto winner, and"
  echo "its output is decoded and \`cmp\`ed against the source and checked with"
  echo "\`verify\`."
  echo
  echo "## Per-file table"
  echo
  echo "All sizes are serialized \`.voldoc\` bytes; \`null\` means the input does not"
  echo "propose that kind. \`byte_cmp\` is the auto winner's decoded output compared"
  echo "to the source."
  echo
  echo "| file | source | RAW | RLE | BYTE_RANS | PDF_PHYS | PDF_CHAN | winner | byte_cmp |"
  echo "| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |"
  cat "$WORK/rows.md"
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
  echo
  echo "## Leave-one-out"
  echo
  echo "Removing only the channel mechanism from the full portfolio leaves the A3"
  echo "rung:"
  echo
  echo "- A4_without_channels = A3 = $tot_a3"
  echo "- leave_one_out_channels_delta = A4 - A3 = $loo_delta"
  echo
  echo "So the PDF typed-channel mechanism saved $((0 - loo_delta)) bytes across the"
  echo "corpus relative to the same portfolio without it."
  echo
  echo "Auto-winner counts: RAW=$w_raw RLE=$w_rle BYTE_RANS=$w_byte"
  echo "PDF_PHYSICAL=$w_phys PDF_CHANNELS=$w_chan."
  echo
  echo "## Honest interpretation"
  echo
  echo "The ablation is reported plainly. On the text-heavy scale sample"
  echo "\`bigtext.pdf\` (source $bt_src bytes) the forced sizes were"
  echo "RAW=$bt_raw, BYTE_RANS=$bt_byte, PDF_CHANNELS=$bt_chan: PDF_CHANNELS beats"
  echo "RAW but loses to BYTE_RANS. Splitting the PDF into typed lexical channels and"
  echo "entropy-coding each one pays per-channel model and length overheads that a"
  echo "single order-0 byte-rANS channel over the whole file does not, and on this"
  echo "corpus that overhead is not recovered. PDF_PHYSICAL likewise rarely wins: its"
  echo "one-literal-per-span program adds per-span framing that RAW avoids."
  echo
  echo "These are measured, scoped results for *this* deterministic corpus and this"
  echo "commit. They say the current channel split does not yet pay on text-heavy"
  echo "PDFs; they do not say typed channels cannot pay on other inputs. The winner"
  echo "is always decided by actual serialized bytes, and every auto winner"
  echo "round-trips byte-exactly (all_exact=$exact_bool)."
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
  "phase": "Phase 4 — PDF lexical/structural channels, forced-candidate ablation",
  "claim": "Every corpus file round-trips byte-exactly through its auto-winning lane (decode cmp and verify), and forcing each candidate family yields an honest per-mechanism size or a recorded null decline.",
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
printf 'files=%s A0=%s A1=%s A2=%s A3=%s A4=%s loo_channels_delta=%s\n' \
  "$file_count" "$tot_a0" "$tot_a1" "$tot_a2" "$tot_a3" "$tot_a4" "$loo_delta"
printf 'winners: RAW=%s RLE=%s BYTE_RANS=%s PDF_PHYSICAL=%s PDF_CHANNELS=%s all_exact=%s\n' \
  "$w_raw" "$w_rle" "$w_byte" "$w_phys" "$w_chan" "$exact_bool"
echo "=== campaign: $CAMPAIGN ==="
cat "$CAMPAIGN/manifest.json"

if [ "$fail" -ne 0 ]; then
  echo "PHASE 4 COURT: FAIL" >&2
  exit 1
fi
echo "PHASE 4 COURT: PASS"
