#!/bin/sh
# Phase-12.14 — reproducible flagship demo.
#
# One command, entirely inside the pinned `doc-baseline` image, that demonstrates
# the Phase-12 universal multi-format document field end to end with **real**
# command output — no recorded or fabricated numbers. Every value printed below
# is read from the CLI's own JSON at run time; where a step declines, the demo
# says so instead of hiding it.
#
# It proves, in order:
#
#   1. the canonical deterministic triplet (report.pdf / .docx / .epub), built by
#      tools/fixtures/doc-triplet-gen.py from the Python stdlib alone;
#   2. all three ingested through the one format-agnostic `field-ingest` pipeline
#      into one persistent store;
#   3. exit the ingest context and **delete the runtime sources** (and the
#      descriptors); a sealed oracle copy is kept for `capabilities` and `cmp`
#      only, and the store never references it (same design as the 12.10 court);
#   4. fresh processes query the store with the source gone: `capabilities` per
#      root, `find` the same marker across all three, native-provenance heading /
#      paragraph / Table·Cell(B7) on DOCX and EPUB, `--spine-item N` structure on
#      EPUB, and `explain --analyze` for one DOCX table-cell and one EPUB
#      spine-item, showing `whole_source_materialized: no`, the member decodes,
#      the XML parses, node reuse, and the four ADR-0027 physical byte classes;
#   5. a repeated query showing retained work (`nodes_reused` and a
#      `retained_inverse_work_fraction` computed from receipted integers, exactly
#      as `examples/phase12_share_court.rs` does);
#   6. `materialize --exact` for all three, checked by length, SHA-256 and `cmp`
#      against the oracle copies;
#   7. a pointer to the 12.11 lifetime receipt and where the source-retaining
#      SQLite+FTS5 baseline genuinely wins.
#
# Usage (from the host; the script runs inside the pinned service):
#   docker compose run --rm --no-TTY doc-baseline sh tools/phase12-demo.sh
set -u

cd /work
LC_ALL=C
export LC_ALL

B=${VOLE_BIN:-./target/debug/vole-document}
W=evidence/scratch/phase12-demo
ORACLE="$W/oracle"
SRC="$W/src"
STORE="$W/store"
GT="$ORACLE/ground_truth.json"

say() { printf '\n=== %s ===\n' "$*"; }
hdr() { printf '\n--- %s ---\n' "$*"; }

FAIL=0
jqf() { jq -r "$1" "$2" 2>/dev/null; }
sha() { sha256sum "$1" 2>/dev/null | cut -d' ' -f1; }
sz() { stat -c %s "$1" 2>/dev/null || echo -1; }

# Run one CLI invocation, capturing stdout JSON. A non-zero exit (an honest
# typed decline) is reported and recorded, never swallowed and never fatal to the
# whole demonstration — the demo finishes and reports the decline.
run_cli() { # OUTFILE CMD...
  _out=$1
  shift
  if "$@" >"$_out" 2>"$_out.err"; then
    return 0
  fi
  _rc=$?
  FAIL=$((FAIL + 1))
  printf '  DECLINED (exit %s): %s\n' "$_rc" "$*"
  if [ -s "$_out.err" ]; then sed 's/^/    /' "$_out.err"; fi
  return $_rc
}

# Assert one value equals another; record mismatches as failures.
check() { # id expected actual
  if [ "$2" = "$3" ]; then
    printf '  ok   %-34s %s\n' "$1" "$3"
  else
    FAIL=$((FAIL + 1))
    printf '  FAIL %-34s expected[%s] got[%s]\n' "$1" "$2" "$3"
  fi
}

# Print the four ADR-0027 physical byte classes plus the reuse/decoding counters.
print_actual() { # explain-json label
  jq -r --arg tag "$2" '
    .actual as $a |
    "  \($tag)",
    "    plan shape=\(.plan.shape) index_reads=\(.plan.index_reads) required_nodes=\(.plan.required_nodes)",
    "    format=\($a.format); adapter=\($a.adapter); whole_source_materialized=\($a.whole_source_materialized)",
    "    member_decodes=\($a.member_decodes)  xml_parses=\($a.xml_parses)  seed_nodes_executed=\($a.seed_nodes_executed)  seed_nodes_reused=\($a.seed_nodes_reused)",
    "    four byte classes: descriptor=\($a.descriptor_bytes_read) manifest=\($a.manifest_bytes_read) index=\($a.index_bytes_read) seed=\($a.seed_bytes_read)  (bytes_read=\($a.bytes_read), sum)"' "$1"
}

say "Phase 12.14 flagship demo — $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "  commit $(git rev-parse --short HEAD 2>/dev/null || echo unknown) (branch $(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown))"
echo "  service doc-baseline (pinned, memory-capped); all work below runs in it"

# The field verbs exist only with the field stack compiled in; --all-features
# also enables the Phase-12 docx/epub adapters. Cached after the first run.
cargo build --quiet --locked --all-features || {
  echo "fatal: all-features build failed"
  exit 1
}

rm -rf "$W"
mkdir -p "$ORACLE" "$SRC"

# ---------------------------------------------------------------------------
hdr "1. canonical triplet (deterministic, Python stdlib only)"
python3 tools/fixtures/doc-triplet-gen.py "$ORACLE" || {
  echo "fatal: triplet generation failed"
  exit 1
}
for fmt in pdf docx epub; do
  echo "  report.$fmt: $(sz "$ORACLE/report.$fmt") bytes sha256=$(sha "$ORACLE/report.$fmt")"
done

# ---------------------------------------------------------------------------
hdr "2. field-ingest all three through the one format-agnostic pipeline"
for fmt in pdf docx epub; do
  # Ingest from a *runtime* copy; the oracle copy is never fed to the pipeline.
  cp "$ORACLE/report.$fmt" "$SRC/report.$fmt"
  run_cli "$SRC/encode.$fmt.json" "$B" encode --force raw "$SRC/report.$fmt" "$SRC/report.$fmt.voldoc" || true
  run_cli "$SRC/ingest.$fmt.json" "$B" field-ingest "$SRC/report.$fmt.voldoc" --store "$STORE" || true
  eval "FIELD_$fmt=$(jqf '.field' "$SRC/ingest.$fmt.json")"
  fshow=$(eval "printf '%s' \"\$FIELD_$fmt\"")
  echo "  $fmt -> field $(printf %.16s "$fshow")..."
done

# ---------------------------------------------------------------------------
hdr "3. delete the runtime sources and ingest descriptors (source independence)"
for fmt in pdf docx epub; do
  rm -f "$SRC/report.$fmt" "$SRC/report.$fmt.voldoc"
  if [ -e "$SRC/report.$fmt" ] || [ -e "$SRC/report.$fmt.voldoc" ]; then
    FAIL=$((FAIL + 1))
    echo "  FAIL report.$fmt (or its descriptor) still present"
  else
    echo "  report.$fmt + descriptor deleted; only the field store remains"
  fi
done
echo "  (the oracle copies under $ORACLE are byte-identical sealed references for"
echo "   format detection and cmp; they are never ingested and the store never"
echo "   references them — the same design as the 12.10 removal court)"

# ---------------------------------------------------------------------------
hdr "4. fresh processes query the store with the source gone"
echo
echo "capabilities (format detected from bytes):"
for fmt in pdf docx epub; do
  run_cli "$W/cap.$fmt.json" "$B" capabilities "$ORACLE/report.$fmt" || true
  jq -c '{format,adapter,selectors:[.selectors[].selector],native_selectors}' "$W/cap.$fmt.json" 2>/dev/null |
    sed 's/^/  /'
done

echo
echo "find the same marker XF12A across all three:"
for fmt in pdf docx epub; do
  eval "field=\$FIELD_$fmt"
  run_cli "$W/find.$fmt.json" "$B" find --store "$STORE" --field "$field" --text XF12A || true
  val=$(jqf '.value' "$W/find.$fmt.json")
  prov=$(jqf '.provenance' "$W/find.$fmt.json")
  valc=$(printf '%s' "$val" | jq -c . 2>/dev/null)
  [ -n "$valc" ] || valc="$val"
  case "$val" in
    *XF12A*) printf '  ok   %-8s %s  [%s]\n' "$fmt" "$valc" "$prov" ;;
    *) FAIL=$((FAIL + 1)); printf '  FAIL %-8s marker absent: %s\n' "$fmt" "$valc" ;;
  esac
done

echo
echo "native-provenance heading / paragraph / Table·Cell(B7) on DOCX and EPUB:"
for fmt in docx epub; do
  eval "field=\$FIELD_$fmt"
  run_cli "$W/$fmt.heading.json" "$B" observe --store "$STORE" --field "$field" --heading 0 --kind text || true
  run_cli "$W/$fmt.block.json" "$B" observe --store "$STORE" --field "$field" --block 1 --kind text || true
  run_cli "$W/$fmt.cell.json" "$B" observe --store "$STORE" --field "$field" --cell 0:6:1 --kind text || true
  printf '  %-5s heading0  = %s  [%s]\n' "$fmt" "$(jqf '.text' "$W/$fmt.heading.json")" "$(jqf '.provenance' "$W/$fmt.heading.json")"
  printf '  %-5s block1    = %s\n' "$fmt" "$(jqf '.text' "$W/$fmt.block.json")"
  printf '  %-5s Cell(B7)  = %s\n' "$fmt" "$(jqf '.text' "$W/$fmt.cell.json")"
  check "$fmt.heading0" "$(jqf '.logical_report.headings[0]' "$GT")" "$(jqf '.text' "$W/$fmt.heading.json")"
  check "$fmt.block1" "$(jqf '.logical_report.paragraphs[0]' "$GT")" "$(jqf '.text' "$W/$fmt.block.json")"
  check "$fmt.cell_b7" "$(jqf '.logical_report.table.cell_b7' "$GT")" "$(jqf '.text' "$W/$fmt.cell.json")"
done

echo
echo "EPUB reading coordinate (reflowable EPUB has no intrinsic pages):"
run_cli "$W/epub.spine.json" "$B" observe --store "$STORE" --field "$FIELD_epub" --spine-item 0 --kind structure || true
echo "  selector=$(jqf '.selector' "$W/epub.spine.json") provenance=[$(jqf '.provenance' "$W/epub.spine.json")]"
echo "  structure (truncated to 600 chars):"
jq -r '.value' "$W/epub.spine.json" 2>/dev/null | cut -c1-600 | sed 's/^/    /'

echo
echo "explain --analyze — one DOCX table-cell and one EPUB spine-item:"
run_cli "$W/explain.docx.cell.json" "$B" explain --store "$STORE" --field "$FIELD_docx" --cell 0:6:1 --kind text --analyze || true
print_actual "$W/explain.docx.cell.json" "DOCX Cell(0:6:1) text"
run_cli "$W/explain.epub.spine.json" "$B" explain --store "$STORE" --field "$FIELD_epub" --spine-item 0 --kind structure --analyze || true
print_actual "$W/explain.epub.spine.json" "EPUB spine-item 0 structure"
# The 12.14 acceptance point: a narrow observation must not bake the whole source.
check "docx.cell.whole_source_materialized" "false" "$(jqf '.actual.whole_source_materialized' "$W/explain.docx.cell.json")"
check "epub.spine.whole_source_materialized" "false" "$(jqf '.actual.whole_source_materialized' "$W/explain.epub.spine.json")"

# ---------------------------------------------------------------------------
hdr "5. repeated query — retained work (nodes_reused + work fraction)"
# Prime the derived cache, then measure a no-cache recompute (cold) against the
# cache-served repeat (warm). The fraction is computed from the two receipts'
# integer work units exactly as examples/phase12_share_court.rs does:
#   units = seed_nodes_executed + bytes_read;  reused = drop in executions + drop
#   in cold input bytes;  fraction = reused / cold units.
run_cli "$W/reuse.prime.json" "$B" explain --store "$STORE" --field "$FIELD_epub" --spine-item 0 --kind structure --analyze || true
run_cli "$W/reuse.cold.json" "$B" explain --store "$STORE" --field "$FIELD_epub" --spine-item 0 --kind structure --analyze --no-cache || true
run_cli "$W/reuse.warm.json" "$B" explain --store "$STORE" --field "$FIELD_epub" --spine-item 0 --kind structure --analyze || true

cold_ex=$(jqf '.actual.seed_nodes_executed' "$W/reuse.cold.json")
cold_in=$(jqf '.actual.bytes_read' "$W/reuse.cold.json")
warm_ex=$(jqf '.actual.seed_nodes_executed' "$W/reuse.warm.json")
warm_in=$(jqf '.actual.bytes_read' "$W/reuse.warm.json")
warm_reused=$(jqf '.actual.seed_nodes_reused' "$W/reuse.warm.json")

fraction=$(awk -v ce="${cold_ex:-0}" -v ci="${cold_in:-0}" -v we="${warm_ex:-0}" -v wi="${warm_in:-0}" '
  BEGIN {
    total = ce + ci
    if (total == 0) { printf "1.000000"; exit }
    r = (ce - we > 0 ? ce - we : 0) + (ci - wi > 0 ? ci - wi : 0)
    printf "%.6f", r / total
  }')

echo "  cold  (--no-cache): seed_nodes_executed=$cold_ex  bytes_read=$cold_in  units=$((cold_ex + cold_in))"
echo "  warm  (cache):      seed_nodes_executed=$warm_ex  bytes_read=$warm_in  nodes_reused=$warm_reused"
echo "  retained_inverse_work_fraction = $fraction"
if [ "${warm_reused:-0}" -gt 0 ]; then
  echo "  ok   the repeat was served from persisted work (nodes_reused > 0)"
else
  echo "  note nodes_reused = 0 on this run; the fraction above is still receipted,"
  echo "       not assumed (single-item EPUBs can leave little to reuse)"
fi

# ---------------------------------------------------------------------------
hdr "6. materialize --exact — length + SHA-256 + cmp vs the oracle"
for fmt in pdf docx epub; do
  eval "field=\$FIELD_$fmt"
  out="$W/materialized.report.$fmt"
  run_cli "$W/materialize.$fmt.json" "$B" materialize --store "$STORE" --field "$field" --exact --output "$out" || true
  if cmp -s "$out" "$ORACLE/report.$fmt"; then rc=equal; else rc=DIFFER; FAIL=$((FAIL + 1)); fi
  mlen=$(sz "$out")
  msha=$(sha "$out")
  olen=$(sz "$ORACLE/report.$fmt")
  osha=$(sha "$ORACLE/report.$fmt")
  echo "  $fmt len=$mlen sha256=$(printf %.12s "$msha")... cmp=$rc"
  check "$fmt.exact.len" "$olen" "$mlen"
  check "$fmt.exact.sha256" "$osha" "$msha"
  check "$fmt.exact.cmp" "equal" "$rc"
done

# ---------------------------------------------------------------------------
hdr "7. where the DB baseline wins (12.11 lifetime court)"
echo "  pointer : docs/phases/phase-12-results.md -> \"## Lifetime workload (12.11)\""
echo "  receipt : evidence/campaigns/2026-10-06-phase12-lifetime-3eaf576/SUMMARY.md"
echo "  honest  : the source-retaining SQLite+FTS5 cache (A1) wins the *large*"
echo "            document frontier — delta.docx bytes from N=10, delta.epub from"
echo "            N=100, delta.pdf from N=1000, and wall/CPU at N=1000 for"
echo "            delta.docx / delta.epub — because VOLE reads its full descriptor"
echo "            (and warm derived cache) on every observation while A1 does a"
echo "            covering-index point lookup. VOLE owns the small-document"
echo "            frontier and the cold one-time comparison. Every loss is in the"
echo "            receipt above; this demo makes no lifetime claim of its own."

# ---------------------------------------------------------------------------
say "result"
if [ "$FAIL" -eq 0 ]; then
  echo "  DEMO OK — every step completed; the three sources were byte-exactly"
  echo "  rematerialized after deletion, and no step declined."
  exit 0
else
  echo "  DEMO INCOMPLETE — $FAIL step(s) declined or mismatched (see above)."
  exit 1
fi
