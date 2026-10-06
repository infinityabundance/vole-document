#!/usr/bin/env bash
# Phase-12 acceptance gate §109 / `N6` — **PDF no regression**.
#
# Question: does routing a PDF through the Phase-12 *unified* field (A11, the full
# `--all-features` build with the `package`/`opc`/`docx`/`epub` adapters compiled
# in) regress the Phase-11 PDF path (A2, the `rans,store,field`-only build)?
#
# The two lanes are the *same* PDF pipeline — the document dispatch detects PDF
# first and hands the field to the unchanged PDF adapter — so the honest test is
# not "do they both work" but "are exactness and the per-observation work identical
# (or strictly not worse) for A11". For every PDF in the committed Phase-7 / Phase-8
# corpora this court measures, per lane:
#
#   * `materialize --exact` == source: length **and** SHA-256 **and** `cmp`
#     (the prime-directive triple);
#   * the `observe` answer (canonical JSON digest, volatile fields removed);
#   * `explain --analyze`: `whole_source_materialized`, `seed_nodes_executed`,
#     `seed_nodes_reused`, `member_decodes`, `xml_parses`, and the four ADR-0027
#     read classes (`descriptor`/`manifest`/`index`/`seed`) plus `bytes_read`;
#   * `find --text` on a token taken from page 1 (answer digest).
#
# The gate is closed only if, for every observation: both lanes are byte-exact,
# the answer digests match, `whole_source_materialized` is false, and A11's
# `bytes_read` / `seed_nodes_executed` / `member_decodes` / `xml_parses` are not
# greater than A2's (each delta recorded, never assumed). Any regression is printed
# with its exact magnitude and fails the court.
#
# Usage (inside the pinned `doc-baseline` service, which has jq + strace):
#   bash tools/phase12-pdf-noregression.sh [OUTDIR]
# Env: PDF_NOREG_CORPORA (space list; default the Phase-7/8 corpora).
set -u

cd /work
LC_ALL=C
export LC_ALL

OUTDIR=${1:-evidence/campaigns/$(date -u +%Y-%m-%d)-phase12-pdf-noregression-$(git rev-parse --short HEAD 2>/dev/null || echo unknown)}
CORPORA=${PDF_NOREG_CORPORA:-"evidence/corpus/phase7 evidence/corpus/phase7-producers evidence/corpus/phase8-large"}

# Snapshot provenance *before* this script creates its own receipt directory, so
# `dirty` reflects the committed tree under test, not the receipt being written.
COMMIT=$(git rev-parse HEAD 2>/dev/null || echo unknown)
COMMIT_SHORT=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)
DIRTY=$(git status --porcelain 2>/dev/null | head -40 | tr '\n' ';' || echo unknown)
RUN_UTC=$(date -u +%Y-%m-%dT%H:%M:%SZ)

RAW="$OUTDIR/raw"
rm -rf "$OUTDIR"
mkdir -p "$RAW/obs"

# Regenerable bytes (descriptors, stores, materialized outputs) live in gitignored
# scratch, never in the sealed receipt; only small JSON audit artifacts are copied in.
SCRATCH=${PDF_NOREG_SCRATCH:-evidence/scratch/phase12-pdf-noregression-work}
rm -rf "$SCRATCH"
mkdir -p "$SCRATCH"

BINDIR=$(mktemp -d)
trap 'rm -rf "$BINDIR" "$SCRATCH"' EXIT

jqf() { jq -r "$1" "$2" 2>/dev/null; }
sha() { sha256sum "$1" 2>/dev/null | cut -d' ' -f1; }
bytes() { stat -c %s "$1" 2>/dev/null || echo -1; }
# Canonical digest of an observe/explain JSON, dropping only genuinely volatile
# fields (timing, cache-write byte count). Everything else — value, spans,
# dependency ids, node counters — must match byte-for-byte across the two lanes.
digest() { jq -S -c 'del(.stats, .wall_micros) | if .actual then .actual |= del(.wall_micros, .cache_bytes_written) else . end' "$1" 2>/dev/null | sha256sum | cut -d' ' -f1; }

FAIL=0

# ---------------------------------------------------------------------------
# Build the two lanes inside the pinned service (never on the host).
# ---------------------------------------------------------------------------
echo "phase12-pdf-noregression: building A2 and A11 binaries" >&2
cargo build --locked --no-default-features --features rans,store,field 1>&2
cp target/debug/vole-document "$BINDIR/vole-a2"
cargo build --locked --all-features 1>&2
cp target/debug/vole-document "$BINDIR/vole-a11"
A2="$BINDIR/vole-a2"
A11="$BINDIR/vole-a11"

# Fixed observation set (native PDF selectors the Phase-11 adapter serves).
OBS=(
  "page1-text|--page 1 --kind text"
  "metadata|--metadata --kind metadata"
  "byrange-exact|--byte-range 0..64 --kind exact"
  "page1-structure|--page 1 --kind structure"
)

: > "$RAW/perobs.tsv"
: > "$RAW/exactness.tsv"
: > "$RAW/corpus.tsv"

for corpus in $CORPORA; do
  for src in "$corpus"/*.pdf; do
    [ -e "$src" ] || continue
    name="${src#evidence/corpus/}"
    name=$(printf '%s' "$name" | tr '/' '_')
    src_len=$(bytes "$src")
    src_sha=$(sha "$src")
    printf '%s\t%s\t%s\t%s\n' "$name" "$src" "$src_len" "$src_sha" >> "$RAW/corpus.tsv"
    d="$SCRATCH/$name"
    mkdir -p "$d"
    echo "=== $name ${src_len}B ===" >&2

    SEARCH_TOKEN=""
    for lane in a2 a11; do
      if [ "$lane" = a2 ]; then bin="$A2"; else bin="$A11"; fi
      store="$d/store-$lane"
      mkdir -p "$store"

      if ! "$bin" encode --force raw "$src" "$d/$lane.voldoc" > "$d/$lane.enc.json" 2>"$d/$lane.enc.err"; then
        echo "FAIL $name/$lane: encode failed" >&2; FAIL=$((FAIL + 1)); continue
      fi
      if ! "$bin" field-ingest "$d/$lane.voldoc" --store "$store" > "$d/$lane.ing.json" 2>"$d/$lane.ing.err"; then
        echo "FAIL $name/$lane: field-ingest failed" >&2; FAIL=$((FAIL + 1)); continue
      fi
      field=$(jqf '.field' "$d/$lane.ing.json")
      echo "$field" > "$d/field-$lane"

      # --- exactness: length + SHA-256 + cmp --------------------------------
      out="$d/$lane.out"
      "$bin" materialize --store "$store" --field "$field" --exact --output "$out" > "$d/$lane.mat.json" 2>"$d/$lane.mat.err" || true
      mlen=$(bytes "$out"); msha=$(sha "$out")
      if cmp -s "$out" "$src"; then cmpst=equal; else cmpst=DIFFER; FAIL=$((FAIL + 1)); fi
      [ "$mlen" = "$src_len" ] || FAIL=$((FAIL + 1))
      [ "$msha" = "$src_sha" ] || FAIL=$((FAIL + 1))
      printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$lane" "$src_len" "$mlen" "$src_sha" "$msha" "$cmpst" >> "$RAW/exactness.tsv"

      # --- observations -----------------------------------------------------
      for spec in "${OBS[@]}"; do
        olabel=${spec%%|*}; oargs=${spec#*|}
        # shellcheck disable=SC2086
        "$bin" observe --store "$store" --field "$field" $oargs > "$d/$olabel.$lane.obs.json" 2>"$d/$olabel.$lane.obs.err" || true
        # shellcheck disable=SC2086
        "$bin" explain --store "$store" --field "$field" $oargs --analyze > "$d/$olabel.$lane.exp.json" 2>"$d/$olabel.$lane.exp.err" || true
      done

      # --- lexical search on a token derived from page 1 --------------------
      if [ -z "$SEARCH_TOKEN" ]; then
        SEARCH_TOKEN=$("$bin" observe --store "$store" --field "$field" --page 1 --kind text 2>/dev/null \
          | jq -r '.value // .text // empty' 2>/dev/null \
          | tr -c 'A-Za-z0-9' '\n' | awk 'length($0) >= 3 { print; exit }')
        [ -n "$SEARCH_TOKEN" ] || SEARCH_TOKEN="the"
      fi
      "$bin" find --store "$store" --field "$field" --text "$SEARCH_TOKEN" > "$d/search.$lane.obs.json" 2>"$d/search.$lane.err" || true
    done
    printf '%s\t%s\n' "$name" "$SEARCH_TOKEN" >> "$RAW/search_token.tsv"

    # --- compare the two lanes ----------------------------------------------
    for spec in "${OBS[@]}"; do
      olabel=${spec%%|*}
      for lane in a2 a11; do
        exp="$d/$olabel.$lane.exp.json"
        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
          "$name" "$olabel" "$lane" \
          "$(jqf '.plan.shape' "$exp")" "$(jqf '.plan.index_reads' "$exp")" "$(jqf '.plan.required_nodes' "$exp")" \
          "$(jqf '.actual.whole_source_materialized' "$exp")" \
          "$(jqf '.actual.seed_nodes_executed' "$exp")" "$(jqf '.actual.seed_nodes_reused' "$exp")" \
          "$(jqf '.actual.member_decodes' "$exp")" "$(jqf '.actual.xml_parses' "$exp")" \
          "$(jqf '.actual.descriptor_bytes_read' "$exp")" "$(jqf '.actual.manifest_bytes_read' "$exp")" \
          "$(jqf '.actual.index_bytes_read' "$exp")" "$(jqf '.actual.seed_bytes_read' "$exp")" \
          "$(jqf '.actual.bytes_read' "$exp")" >> "$RAW/perobs.tsv"
        [ "$lane" = a11 ] && printf '%s\t%s\n' "$(jqf '.actual.bytes_returned' "$exp")" "$(digest "$exp")" > "$d/$olabel.cmp.tmp"
        [ "$lane" = a2 ] && printf '%s\t%s\n' "$(jqf '.actual.bytes_returned' "$exp")" "$(digest "$exp")" > "$d/$olabel.a2.tmp"
      done
      # answer digest equality (observe + find)
      a2ans=$(digest "$d/$olabel.a2.obs.json")
      a11ans=$(digest "$d/$olabel.a11.obs.json")
      a2exp=$(cut -f2 "$d/$olabel.a2.tmp")
      a11exp=$(cut -f2 "$d/$olabel.cmp.tmp")
      same_answer=no; [ "$a2ans" = "$a11ans" ] && same_answer=yes
      same_work=no; [ "$a2exp" = "$a11exp" ] && same_work=yes
      [ "$same_answer" = yes ] || { echo "FAIL $name/$olabel: answer digest differs" >&2; FAIL=$((FAIL + 1)); }
      [ "$same_work" = yes ] || { echo "FAIL $name/$olabel: plan+actual digest differs (excluding volatile)" >&2; FAIL=$((FAIL + 1)); }
      printf '%s\t%s\tsame_answer=%s\tsame_work=%s\ta2_ans=%s\ta11_ans=%s\n' \
        "$name" "$olabel" "$same_answer" "$same_work" "${a2ans:0:12}" "${a11ans:0:12}" >> "$RAW/digests.tsv"
    done
    # search digest equality
    s2=$(digest "$d/search.a2.obs.json"); s11=$(digest "$d/search.a11.obs.json")
    [ "$s2" = "$s11" ] || { echo "FAIL $name/search: answer digest differs" >&2; FAIL=$((FAIL + 1)); }
    printf '%s\tsearch\tsame_answer=%s\tsame_work=%s\ta2_ans=%s\ta11_ans=%s\n' \
      "$name" "$([ "$s2" = "$s11" ] && echo yes || echo no)" "-" "${s2:0:12}" "${s11:0:12}" >> "$RAW/digests.tsv"

    # Small audit artifacts only (no *.out / *.voldoc / stores). Oversize answer
    # values (e.g. a very common search token on a 33 MB PDF) are hashed, not
    # copied, so the sealed receipt stays small; the canonical digest is already
    # recorded in raw/digests.tsv.
    mkdir -p "$RAW/obs/$name"
    for f in "$d"/*.json; do
      [ -e "$f" ] || continue
      sz=$(bytes "$f")
      if [ "$sz" -gt 262144 ]; then
        printf '%s\t%s\t%s\n' "$name" "$(basename "$f")" "$sz" >> "$RAW/obs/oversize.tsv"
        continue
      fi
      cp "$f" "$RAW/obs/$name/"
    done
  done
done

# ---------------------------------------------------------------------------
# Regression arithmetic: A11 must not read/execute more than A2 per observation.
# ---------------------------------------------------------------------------
: > "$RAW/regressions.tsv"
awk -F'\t' '
{
  key=$1"|"$2;
  if ($3=="a2") { br[key]=$16; ex[key]=$8; md[key]=$10; xp[key]=$11; wsm[key]=$7; }
  else { br11[key]=$16; ex11[key]=$8; md11[key]=$10; xp11[key]=$11; wsm11[key]=$7; }
}
END {
  n=0; bad=0;
  for (k in br) {
    n++;
    dbr=br11[k]-br[k]; dex=ex11[k]-ex[k]; dmd=md11[k]-md[k]; dxp=xp11[k]-xp[k];
    flag="ok";
    if (dbr>0||dex>0||dmd>0||dxp>0) { flag="REGRESSION"; bad++; }
    if (wsm[k]!="false"||wsm11[k]!="false") { flag="REGRESSION(wsm)"; bad++; }
    print k"\t"br[k]"\t"br11[k]"\t"dbr"\t"ex[k]"\t"ex11[k]"\t"dex"\t"dmd"\t"dxp"\t"flag;
  }
  print "TOTAL\t"n"\tREGRESSIONS\t"bad > "/dev/stderr";
}' "$RAW/perobs.tsv" > "$RAW/regressions.tsv" 2> "$RAW/regressions.count"
REG=$(awk -F'\t' '/^TOTAL/{print $4}' "$RAW/regressions.count")
[ "${REG:-0}" = 0 ] || FAIL=$((FAIL + REG))

# ---------------------------------------------------------------------------
# Environment + receipt.
# ---------------------------------------------------------------------------
RUSTC_V=$(rustc --version 2>/dev/null | sed 's/^rustc //' || echo unknown)
CARGO_V=$(cargo --version 2>/dev/null | sed 's/^cargo //' || echo unknown)
ARCH=$(uname -m)
LOCK_SHA=$(sha /work/Cargo.lock 2>/dev/null || echo unknown)
BASE_STABLE=$(sed -n 's/^ARG BASE_STABLE=//p' /work/Dockerfile 2>/dev/null | head -1)
BASE_TOOLS=$(sed -n 's/^ARG BASE_TOOLS=//p' /work/Dockerfile 2>/dev/null | head -1)
IMAGE_ID=${HOST_IMAGE_ID:-unrecorded}
POPPLER_V=$(pdftotext -v 2>&1 | head -1 | sed 's/^pdftotext version //')
JQ_V=$(jq --version)
STRACE_V=$(strace --version 2>/dev/null | head -1)
SQLITE_V=$(sqlite3 --version | awk '{print $1}')
NDOC=$(wc -l < "$RAW/corpus.tsv")
NOBS=$(wc -l < "$RAW/perobs.tsv")
NEXACT=$(awk -F'\t' '$7=="equal"' "$RAW/exactness.tsv" | wc -l)
NEXACTROWS=$(wc -l < "$RAW/exactness.tsv")

jq -n \
  --arg commit "$COMMIT" --arg commit_short "$COMMIT_SHORT" --arg branch "$BRANCH" --arg dirty "$DIRTY" \
  --arg lock_sha "$LOCK_SHA" --arg arch "$ARCH" \
  --arg rustc "$RUSTC_V" --arg cargo "$CARGO_V" \
  --arg base_stable "$BASE_STABLE" --arg base_tools "$BASE_TOOLS" --arg image_id "$IMAGE_ID" \
  --arg poppler "$POPPLER_V" --arg jq "$JQ_V" --arg strace "$STRACE_V" --arg sqlite "$SQLITE_V" \
  --arg run_utc "$RUN_UTC" --argjson ndoc "$NDOC" --argjson nobs "$NOBS" \
  --argjson nexact "$NEXACT" --argjson nexactrows "$NEXACTROWS" --argjson reg "${REG:-0}" \
  --argjson fail "$FAIL" \
  '{git:{commit:$commit,commit_short:$commit_short,branch:$branch,dirty:$dirty},
    arch:$arch, cargo_lock_sha256:$lock_sha, run_utc:$run_utc,
    service_image:"vole-document/doc-baseline:1.99.0", doc_baseline_image_id:$image_id,
    toolchain:{rustc:$rustc,cargo:$cargo},
    base_image:$base_stable, base_tools:$base_tools,
    oracles:{poppler_pdftotext:$poppler,sqlite3:$sqlite,strace:$strace,jq:$jq},
    lanes:{a2:"rans,store,field (Phase-11 PDF field)",a11:"--all-features (full Phase-12 unified field)"},
    counts:{documents:$ndoc,observations:$nobs,exact_equal:$nexact,exact_rows:$nexactrows,regressions:$reg,failures:$fail},
    camera:"fresh process per observation; observe answer + explain --analyze counters; exactness = length + SHA-256 + cmp vs the unmodified source; per-lane lanes built inside this service"}' \
  > "$OUTDIR/environment.json"

{
  echo "# Phase 12 \u2014 PDF no-regression gate (N6, \u00a7109)"
  echo
  echo "Generated by \`tools/phase12-pdf-noregression.sh\` inside the pinned, capped \`doc-baseline\` service."
  echo
  echo "Commit: \`$COMMIT_SHORT\` (branch \`$BRANCH\`, dirty: \`$DIRTY\`)  "
  echo "Service image \`vole-document/doc-baseline:1.99.0\` (id \`$IMAGE_ID\`), base \`$BASE_STABLE\`.  "
  echo "rustc \`$RUSTC_V\`, cargo \`$CARGO_V\`; Cargo.lock sha256 \`$LOCK_SHA\`; arch \`$ARCH\`; run (UTC) $RUN_UTC.  "
  echo "Oracles: poppler \`$POPPLER_V\`, sqlite3 \`$SQLITE_V\`, strace \`$STRACE_V\`."
  echo
  echo "**Lanes.** A2 = \`--no-default-features --features rans,store,field\` (the Phase-11 PDF field,"
  echo "no \`package\`/\`opc\`/\`docx\`/\`epub\`). A11 = \`--all-features\` (the full Phase-12 unified field)."
  echo "Both detect PDF from bytes and serve the *same* PDF adapter; the question is whether the"
  echo "extra adapters regress it."
  echo
  echo "**Corpus.** $NDOC PDFs from the committed Phase-7 / Phase-7-producers / Phase-8-large"
  echo "corpora (bytes gitignored and regenerable; hashes recorded in \`raw/corpus.tsv\`)."
  echo
  echo "**Exactness.** $NEXACT / $NEXACTROWS lane-documents satisfy \`materialize --exact\` with"
  echo "length == source length **and** SHA-256 == source SHA-256 **and** \`cmp\` == equal."
  echo
  echo "**Observations.** $NOBS \`explain --analyze\` rows over four native PDF selectors"
  echo "(\`page 1 text\`, \`metadata\`, \`byte-range 0..64 exact\`, \`page 1 structure\`)."
  echo
  echo "**Regression arithmetic.** $REG regression(s) (\`raw/regressions.tsv\`: A11 bytes-read /"
  echo "executions minus A2, per observation; positive is a regression)."
  echo
  if [ "$FAIL" -eq 0 ]; then
    echo "**VERDICT: gate CLOSED.** Every PDF is byte-exact under both lanes, every answer digest"
    echo "matches, and A11 reads/executes no more than A2 on any observation."
  else
    echo "**VERDICT: gate OPEN.** $FAIL check(s) failed; see \`raw/\` (regressions.tsv, digests.tsv)."
  fi
} > "$OUTDIR/SUMMARY.md"

cat > "$OUTDIR/commands.txt" <<EOF
# Phase-12 PDF no-regression gate (N6) — exact commands
# Commit under test: $COMMIT (branch $BRANCH); dirty: $DIRTY
# Service image: vole-document/doc-baseline:1.99.0 (id $IMAGE_ID); base $BASE_STABLE
# rustc $RUSTC_V, cargo $CARGO_V; Cargo.lock sha256: $LOCK_SHA; arch $ARCH
# Run (UTC): $RUN_UTC
# All commands run inside the pinned, capped doc-baseline image; nothing on the host.

docker compose run --rm --no-TTY -e HOST_IMAGE_ID=$IMAGE_ID \\
    doc-baseline bash tools/phase12-pdf-noregression.sh $OUTDIR

# Inside the script (fresh builds in the pinned service):
#   cargo build --locked --no-default-features --features rans,store,field   -> vole-a2
#   cargo build --locked --all-features                                      -> vole-a11
# per PDF and lane: encode --force raw; field-ingest; materialize --exact;
#   observe <selector>; explain --analyze <selector>; find --text <page-1 token>.
EOF

{
  echo "$OUTDIR"
  ( cd "$OUTDIR" && sha256sum SUMMARY.md commands.txt environment.json raw/*.tsv 2>/dev/null )
} > "$OUTDIR/raw.sha256"

echo "phase12-pdf-noregression: documents=$NDOC observations=$NOBS exact=$NEXACT/$NEXACTROWS regressions=$REG failures=$FAIL" >&2
echo "wrote $OUTDIR" >&2
[ "$FAIL" -eq 0 ]
