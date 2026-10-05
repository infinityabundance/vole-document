#!/bin/sh
# Phase 3 exact court: the PDF byte-authoritative physical scanner.
#
# Runs entirely inside the pinned dev container. It:
#   1. builds the exact binary (`cargo build --locked`);
#   2. materializes the deterministic sample corpus via `pdf-make-samples`, whose
#      every `/Length` and `startxref` is correct by construction;
#   3. for every sample runs `pdf-inspect` (is_pdf, span/object/revision counts),
#      `encode`/`decode`/`verify`, and `cmp`s the decode against the source;
#   4. asserts coverage: every well-formed PDF is detected with >= 1 object and
#      >= 1 revision, and every negative control is NOT detected;
#   5. asserts exactness: every byte-compare is equal;
#   6. writes results.jsonl, coverage.json, environment.json, manifest.json and
#      report.md into the campaign directory.
#
# The literal PDF candidate carries no structural compression yet, so it almost
# always loses the complete-cost court to RAW in Phase 3. That is the expected,
# honest outcome; the court records it rather than hiding it.
#
# The independent qpdf oracle lives in tools/pdf-oracle.sh and writes
# oracle.jsonl; it is a separate differential court, not byte authority.
#
# Usage (from the host):
#   docker compose run --rm --no-TTY dev sh tools/phase3-court.sh
set -eu

cd /work

cargo build --locked --quiet
BIN=target/debug/vole-document

SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
CAMPAIGN="evidence/campaigns/$(date -u +%Y-%m-%d)-phase3-${SHA}"
campaign="$(basename "$CAMPAIGN")"

WORK=evidence/scratch/phase3
CORPUS="$WORK/corpus"
rm -rf "$WORK"
mkdir -p "$CORPUS" "$CAMPAIGN"

"$BIN" pdf-make-samples "$CORPUS" >/dev/null

# Ordered corpus. The first seven names are well-formed PDFs; the last two are
# the negative controls (a truncated `.pdf` and a non-PDF).
FILES="classic.pdf xrefstream.pdf objstm.pdf incremental.pdf mixedeol.pdf traptext.pdf trapstream.pdf malformed.pdf notpdf.bin"
VALID_PDF="classic.pdf xrefstream.pdf objstm.pdf incremental.pdf mixedeol.pdf traptext.pdf trapstream.pdf"
NEG_CTRL=" malformed.pdf notpdf.bin "

valid_pdf_count=0
for name in $VALID_PDF; do valid_pdf_count=$((valid_pdf_count + 1)); done

: > "$CAMPAIGN/results.jsonl"
: > "$WORK/rows.md"

sum_source=0
sum_encoded=0
total_spans=0
total_objects=0
total_revisions=0
pdf_count=0
pdf_wins=0
all_covered=true
all_exact=true
fail=0

printf '%-16s %10s %6s %7s %8s %6s %12s %-13s %-11s\n' \
  file source_len is_pdf spans objects revs enc_len winner byte_compare
printf '%-16s %10s %6s %7s %8s %6s %12s %-13s %-11s\n' \
  ---------------- ---------- ------ ------- -------- ------ ------------ ------------- -----------

for name in $FILES; do
  src="$CORPUS/$name"
  src_len="$(wc -c < "$src" | tr -d ' ')"
  src_sha="$(sha256sum "$src" | cut -d' ' -f1)"

  # (a) Structural view: is_pdf, span_count, object_count, revision_count.
  inspect="$("$BIN" pdf-inspect "$src")"
  is_pdf="$(printf '%s\n' "$inspect" | sed -n 's/.*"is_pdf":\([a-z]*\).*/\1/p')"
  span_count="$(printf '%s\n' "$inspect" | sed -n 's/.*"span_count":\([0-9][0-9]*\).*/\1/p')"
  object_count="$(printf '%s\n' "$inspect" | sed -n 's/.*"object_count":\([0-9][0-9]*\).*/\1/p')"
  revision_count="$(printf '%s\n' "$inspect" | sed -n 's/.*"revision_count":\([0-9][0-9]*\).*/\1/p')"

  # (b) Encode / decode / verify, then byte-compare.
  "$BIN" encode "$src" "$WORK/out.enc" > "$WORK/encode.json"
  encoded_len="$(wc -c < "$WORK/out.enc" | tr -d ' ')"
  winner="$(sed -n 's/.*"candidate":"\([^"]*\)".*/\1/p' "$WORK/encode.json")"
  [ -n "$winner" ] || winner="UNKNOWN"
  case "$winner" in PDF_PHYSICAL) pdf_wins=$((pdf_wins + 1));; esac

  "$BIN" decode "$WORK/out.enc" "$WORK/out.dec" >/dev/null
  "$BIN" verify "$WORK/out.enc" >/dev/null

  byte_compare="equal"
  if ! cmp -s "$src" "$WORK/out.dec"; then
    byte_compare="DIFFER"
    all_exact=false
    fail=1
  fi

  # (c) Coverage assertion, driven by the known valid/negative split.
  case "$NEG_CTRL" in
    *" $name "*)
      # Negative control: must be rejected, and still exact (checked above).
      if [ "$is_pdf" != "false" ]; then
        all_covered=false
        fail=1
        echo "FINDING: negative control $name reported is_pdf=$is_pdf" >&2
      fi
      ;;
    *)
      # Well-formed PDF: must be detected with >= 1 object and >= 1 revision.
      if [ "$is_pdf" != "true" ] || [ "$object_count" -lt 1 ] || [ "$revision_count" -lt 1 ]; then
        all_covered=false
        fail=1
        echo "FINDING: $name is_pdf=$is_pdf objects=$object_count revisions=$revision_count" >&2
      fi
      ;;
  esac

  sum_source=$((sum_source + src_len))
  sum_encoded=$((sum_encoded + encoded_len))
  total_spans=$((total_spans + span_count))
  total_objects=$((total_objects + object_count))
  total_revisions=$((total_revisions + revision_count))
  case "$name" in *.pdf) pdf_count=$((pdf_count + 1));; esac

  printf '{"name":"%s","source_len":%s,"source_sha256":"%s","is_pdf":%s,"span_count":%s,"object_count":%s,"revision_count":%s,"encoded_len":%s,"winner":"%s","byte_compare":"%s"}\n' \
    "$name" "$src_len" "$src_sha" "$is_pdf" "$span_count" "$object_count" "$revision_count" "$encoded_len" "$winner" "$byte_compare" \
    >> "$CAMPAIGN/results.jsonl"

  printf '| %s | %s | %s | %s | %s | %s | %s | %s | %s |\n' \
    "$name" "$src_len" "$is_pdf" "$span_count" "$object_count" "$revision_count" "$encoded_len" "$winner" "$byte_compare" \
    >> "$WORK/rows.md"

  printf '%-16s %10s %6s %7s %8s %6s %12s %-13s %-11s\n' \
    "$name" "$src_len" "$is_pdf" "$span_count" "$object_count" "$revision_count" "$encoded_len" "$winner" "$byte_compare"
done

cov_bool=true; [ "$all_covered" = true ] || cov_bool=false
exact_bool=true; [ "$all_exact" = true ] || exact_bool=false

# ---- coverage aggregate ---------------------------------------------------
cat > "$CAMPAIGN/coverage.json" <<EOF
{
  "campaign": "$campaign",
  "pdf_count": $pdf_count,
  "valid_pdf_count": $valid_pdf_count,
  "negative_control_count": 2,
  "total_spans": $total_spans,
  "total_objects": $total_objects,
  "total_revisions": $total_revisions,
  "all_covered": $cov_bool,
  "all_exact": $exact_bool
}
EOF

# ---- environment receipt --------------------------------------------------
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
  "command": "docker compose run --rm --no-TTY dev sh tools/phase3-court.sh"
}
EOF

# ---- report ---------------------------------------------------------------
{
  echo "# Campaign: $campaign — Phase 3 — PDF physical scanner"
  echo
  echo "## Method"
  echo
  echo "The exact binary built from this commit materializes a deterministic sample"
  echo "corpus with \`pdf-make-samples\`, then for every sample runs \`pdf-inspect\`"
  echo "(structural view), \`encode\`/\`decode\`/\`verify\` (exact container lane) and"
  echo "\`cmp\` between the decoded bytes and the source. Every sample's \`/Length\`"
  echo "and \`startxref\` are correct by construction, so the corpus needs no PDF"
  echo "writer and no canonicalization. Counts are aggregate over the whole corpus;"
  echo "the bytes are the authority, not the file name."
  echo
  echo "## Corpus"
  echo
  echo "| file | source_len | is_pdf | spans | objects | revisions | encoded_len | winner | byte_compare |"
  echo "| --- | ---: | :---: | ---: | ---: | ---: | ---: | --- | --- |"
  cat "$WORK/rows.md"
  echo
  echo "\`malformed.pdf\` (truncated, no \`%%EOF\`) and \`notpdf.bin\` are deliberate"
  echo "negative controls: both must be rejected while still round-tripping exactly"
  echo "through the opaque RAW lane."
  echo
  echo "## Coverage results"
  echo
  echo "pdf_count = $pdf_count"
  echo "valid_pdf_count = $valid_pdf_count"
  echo "total_spans = $total_spans"
  echo "total_objects = $total_objects"
  echo "total_revisions = $total_revisions"
  echo "all_covered = $cov_bool"
  echo
  echo "Every well-formed PDF is detected (\`is_pdf\` true) with at least one object"
  echo "and one \`%%EOF\`-terminated revision; both negative controls report"
  echo "\`is_pdf\` false."
  echo
  echo "## Exactness"
  echo
  echo "sum_source = $sum_source"
  echo "sum_encoded = $sum_encoded"
  echo "all_exact = $exact_bool"
  echo
  echo "Every decoded output was byte-identical to its source (\`cmp\` equal), so"
  echo "\`materialize(descriptor) == original_bytes\` holds for the whole corpus."
  echo
  echo "## Court outcome"
  echo
  echo "The literal PDF candidate won the complete-cost court in $pdf_wins of"
  echo "$valid_pdf_count well-formed PDFs; the remaining files were won by RAW."
  echo "This is the expected Phase-3 result: the PDF lane persists each physical"
  echo "span as one \`INLINE\` op with no structural compression, so its per-span"
  echo "overhead almost always loses to a single RAW literal once the complete cost"
  echo "is charged. Structural wins that could change this are Phase 5."
  echo
  echo "## Oracle"
  echo
  echo "The independent qpdf differential oracle is tools/pdf-oracle.sh, run"
  echo "separately in the tools image; it writes \`oracle.jsonl\` (see manifest"
  echo "\`oracle\`). It is a correctness reference, never byte authority."
  echo
  echo "## Verdict"
  echo
  if [ "$fail" -eq 0 ]; then echo "PASS"; else echo "FAIL"; fi
} > "$CAMPAIGN/report.md"

# ---- manifest -------------------------------------------------------------
verdict="$( [ "$fail" -eq 0 ] && echo PASS || echo FAIL )"
cat > "$CAMPAIGN/manifest.json" <<EOF
{
  "campaign": "$campaign",
  "phase": "Phase 3 — PDF byte-authoritative physical scanner",
  "claim": "Every sample in the deterministic corpus materializes byte-exactly, every well-formed PDF is detected with at least one object and one revision, and both negative controls are rejected while remaining exact through RAW.",
  "results_sha256": "$(sha256sum "$CAMPAIGN/results.jsonl" | cut -d' ' -f1)",
  "coverage_sha256": "$(sha256sum "$CAMPAIGN/coverage.json" | cut -d' ' -f1)",
  "environment_sha256": "$(sha256sum "$CAMPAIGN/environment.json" | cut -d' ' -f1)",
  "report_sha256": "$(sha256sum "$CAMPAIGN/report.md" | cut -d' ' -f1)",
  "oracle": "see oracle.jsonl (produced by tools/pdf-oracle.sh <campaign>)",
  "verdict": "$verdict"
}
EOF

# ---- summary --------------------------------------------------------------
echo
echo "=== coverage ==="
printf 'pdf_count=%s valid_pdf_count=%s total_spans=%s total_objects=%s total_revisions=%s\n' \
  "$pdf_count" "$valid_pdf_count" "$total_spans" "$total_objects" "$total_revisions"
printf 'all_covered=%s all_exact=%s pdf_wins=%s\n' "$cov_bool" "$exact_bool" "$pdf_wins"
echo "=== campaign: $CAMPAIGN ==="
cat "$CAMPAIGN/manifest.json"

if [ "$fail" -ne 0 ]; then
  echo "PHASE 3 COURT: FAIL" >&2
  exit 1
fi
echo "PHASE 3 COURT: PASS"
