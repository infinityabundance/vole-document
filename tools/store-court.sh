#!/bin/sh
# Phase-9.3 store court: the decisive cross-document-sharing measurement.
#
# Over the Phase-9 cohort it computes the three frozen accounting universes
# (standalone / unique-reachable / amortized), the per-file generic-LZ ladder
# (via tools/baselines.sh), and the generic content-defined-chunk dedup baseline
# (via tools/chunk-dedup.sh, borg with frozen params). It verifies every
# standalone and every store-backed root byte-exactly (`decode` + `cmp` +
# `verify`), then states, per stratum and overall, whether the VOLE store's
# unique-reachable bytes beat per-file LZ and beat generic CDC.
#
# Runs inside the pinned `baseline` image (same base digest as `dev`, shares the
# cargo target volume, adds gzip/zstd/xz/brotli/jq/borg).
#
# Usage:
#   sh tools/store-court.sh [COHORT_DIR] [OUTDIR]
set -eu

cd /work
LC_ALL=C
export LC_ALL

COHORT=${1:-evidence/corpus/phase9}
OUTDIR=${2:-evidence/scratch/phase9-court}
BIN=${VOLE_BIN:-./target/debug/vole-document}
BASELINE_CORPUS=phase9
export BASELINE_CORPUS

mkdir -p "$OUTDIR"

if [ ! -x "$BIN" ]; then
  echo "store-court: $BIN not found; building (cargo build --locked --all-features)" >&2
  cargo build --locked --all-features 1>&2
fi

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
STORE="$TMP/store"
WORK="$TMP/work"
mkdir -p "$STORE" "$WORK"

# stratum<TAB>path, sorted.
FILES="$TMP/files.txt"
find "$COHORT" -type f \( -name '*.pdf' -o -name '*.bin' \) | sort | while IFS= read -r _f; do
  _rel=${_f#"$COHORT"/}
  _stratum=${_rel%%/*}
  if [ "$_stratum" = "$_rel" ]; then _stratum="(root)"; fi
  printf '%s\t%s\n' "$_stratum" "$_f"
done > "$FILES"

N=$(wc -l < "$FILES" | tr -d ' ')
echo "store-court: cohort=$COHORT files=$N store=$STORE"

PERFILE="$OUTDIR/perfile.jsonl"
: > "$PERFILE"
ROOTS=""

# ---------------------------------------------------------------------------
# encode + externalize + exactness (both forms)
# ---------------------------------------------------------------------------
while IFS='	' read -r stratum f; do
  rel=${f#"$COHORT"/}
  uniq=$(printf '%s' "$rel" | tr '/.' '__')
  src="$WORK/$uniq.src.voldoc"
  root="$STORE/$uniq.src.voldoc"

  enc=$($BIN encode "$f" "$src")
  candidate=$(printf '%s' "$enc" | jq -r '.candidate')
  enc_len=$(printf '%s' "$enc" | jq -r '.encoded_len')
  n_obj=$(printf '%s' "$enc" | jq -r '.cost.objects')
  n_ext=$(printf '%s' "$enc" | jq -r '.cost.external_refs')

  # standalone exactness
  $BIN verify "$src" >/dev/null
  $BIN decode "$src" "$TMP/dec.standalone" >/dev/null
  cmp "$TMP/dec.standalone" "$f"

  # externalize into the shared store
  put=$($BIN store put "$src" "$STORE")
  root_bytes=$(printf '%s' "$put" | jq -r '.root_bytes')
  stored_objects=$(printf '%s' "$put" | jq -r '.stored_objects')

  # store-backed exactness (resolve through the store). `verify` has no resolver
  # (standalone only), so the store-backed check is `decode --store` (which
  # re-checks the whole-source INTEGRITY SHA-256) + byte compare.
  $BIN decode --store "$STORE" "$root" "$TMP/dec.store" >/dev/null
  cmp "$TMP/dec.store" "$f"

  src_bytes=$(wc -c < "$f" | tr -d ' ')
  printf '{"stratum":"%s","file":"%s","source_bytes":%s,"candidate":"%s","standalone_bytes":%s,"root_bytes":%s,"inline_objects":%s,"external_refs":%s,"stored_objects":%s}\n' \
    "$stratum" "$rel" "$src_bytes" "$candidate" "$enc_len" "$root_bytes" "$n_obj" "$n_ext" "$stored_objects" >> "$PERFILE"

  ROOTS="$ROOTS $root"
done < "$FILES"

# ---------------------------------------------------------------------------
# three universes (global + per stratum)
# ---------------------------------------------------------------------------
$BIN store account "$STORE" $ROOTS > "$OUTDIR/account-global.json"

STRATA=$(cut -f1 "$FILES" | sort -u)
STRATUM_ACCOUNTS="$TMP/stratum-accounts.jsonl"
: > "$STRATUM_ACCOUNTS"
for s in $STRATA; do
  sroots=""
  while IFS='	' read -r stratum f; do
    [ "$stratum" = "$s" ] || continue
    rel=${f#"$COHORT"/}
    uniq=$(printf '%s' "$rel" | tr '/.' '__')
    sroots="$sroots $STORE/$uniq.src.voldoc"
  done < "$FILES"
  acct=$($BIN store account "$STORE" $sroots)
  printf '%s' "$acct" | jq -c --arg s "$s" '. + {stratum:$s}' >> "$STRATUM_ACCOUNTS"
done

# ---------------------------------------------------------------------------
# per-file generic LZ (reuse the honest ladder)
# ---------------------------------------------------------------------------
sh tools/baselines.sh "$OUTDIR/baselines.json" "$COHORT" >/dev/null

# ---------------------------------------------------------------------------
# generic CDC baseline (borg, frozen params) — the strongest of a small sweep.
# A coarse chunker would be a strawman: sweep fixed pinned parameter sets and
# keep the smallest unique_csize, then use the winner for the per-stratum runs.
# ---------------------------------------------------------------------------
CDC_SWEEP="19,23,21,4095 11,16,13,4095 10,16,12,4095 8,16,10,4095 9,16,11,4095 10,15,11,1023 10,15,11,511 10,15,11,127"
CDC_SWEEP_ROWS="$TMP/cdc-sweep.jsonl"
: > "$CDC_SWEEP_ROWS"
BEST_U=""
BEST_P=""
for p in $CDC_SWEEP; do
  sh tools/chunk-dedup.sh "$COHORT" "$TMP/cdc-param-$p.json" "$p" >/dev/null
  printf '%s' "$(cat "$TMP/cdc-param-$p.json")" >> "$CDC_SWEEP_ROWS"
  printf '\n' >> "$CDC_SWEEP_ROWS"
  u=$(jq -r .borg_unique_bytes "$TMP/cdc-param-$p.json")
  if [ -z "$BEST_U" ] || [ "$u" -lt "$BEST_U" ]; then BEST_U=$u; BEST_P=$p; fi
done
cp "$TMP/cdc-param-$BEST_P.json" "$OUTDIR/cdc-global.json"
jq -s 'sort_by(.borg_unique_bytes)' "$CDC_SWEEP_ROWS" > "$OUTDIR/cdc-sweep.json"
echo "store-court: strongest CDC params=$BEST_P unique=$BEST_U"

# Same best params with chunk compression on: an upper bound on what a
# compressing chunk store achieves (borg --compression zstd,19).
sh tools/chunk-dedup.sh "$COHORT" "$OUTDIR/cdc-global-zstd.json" "$BEST_P" "zstd,19" >/dev/null

CDC_STRATA="$TMP/cdc-strata.jsonl"
: > "$CDC_STRATA"
CDC_STRATA_Z="$TMP/cdc-strata-z.jsonl"
: > "$CDC_STRATA_Z"
for s in $STRATA; do
  sh tools/chunk-dedup.sh "$COHORT/$s" "$TMP/cdc-$s.json" "$BEST_P" >/dev/null
  jq -c --arg s "$s" '. + {stratum:$s}' "$TMP/cdc-$s.json" >> "$CDC_STRATA"
  sh tools/chunk-dedup.sh "$COHORT/$s" "$TMP/cdc-$s-z.json" "$BEST_P" "zstd,19" >/dev/null
  jq -c --arg s "$s" '. + {stratum:$s}' "$TMP/cdc-$s-z.json" >> "$CDC_STRATA_Z"
done

# ---------------------------------------------------------------------------
# Reduce with jq into results.json
# ---------------------------------------------------------------------------
jq -n \
  --slurpfile global "$OUTDIR/account-global.json" \
  --slurpfile sacct "$STRATUM_ACCOUNTS" \
  --slurpfile base "$OUTDIR/baselines.json" \
  --slurpfile cdc "$OUTDIR/cdc-global.json" \
  --slurpfile cdcz "$OUTDIR/cdc-global-zstd.json" \
  --slurpfile cdcs "$CDC_STRATA" \
  --slurpfile cdcsz "$CDC_STRATA_Z" \
  --slurpfile perfile "$PERFILE" '
  def minlz: ([.gzip9,.zstd19,.xz9e,.brotli11] | map(select(.!=null)) | if length==0 then null else min end);
  def sumnm: (map(select(.!=null)) | add // 0);
  def verdict($x;$y): (if $x < $y then "WIN" elif ($x <= ($y*1.02)) then "TIE" else "LOSS" end);
  ($global[0]) as $G
  | ($base[0].rows) as $rows
  | ([$rows[] | minlz] | sumnm) as $minlz_sum
  | ([$rows[].gzip9] | sumnm) as $gzip_sum
  | ([$rows[].zstd19] | sumnm) as $zstd_sum
  | ([$rows[].xz9e] | sumnm) as $xz_sum
  | ([$rows[].brotli11] | sumnm) as $brotli_sum
  | ($cdc[0].borg_unique_bytes) as $cdc_unique
  | ($cdcz[0].borg_unique_bytes) as $cdc_zstd
  | ($G.standalone_bytes) as $S
  | ($G.unique_reachable_bytes) as $U
  | ($G.unique_object_bytes) as $UO
  | ($perfile | group_by(.stratum)) as $pfgroups
  | [ $pfgroups[] | . as $g
      | ($g[0].stratum) as $s
      | ($g | map(.file | split("/") | last)) as $bn
      | ([$rows[] | select(.file as $fn | ($bn | index($fn)) != null) | minlz] | sumnm) as $slz
      | ([$rows[] | select(.file as $fn | ($bn | index($fn)) != null) | .source_len] | sumnm) as $ssrc
      | ([$sacct[] | select(.stratum==$s)][0]) as $ac
      | ([$cdcs[] | select(.stratum==$s)][0].borg_unique_bytes // null) as $scdc
      | ([$cdcsz[] | select(.stratum==$s)][0].borg_unique_bytes // null) as $scdcz
      | {
          stratum: $s, files: ($g|length), source_bytes: $ssrc,
          standalone_bytes: $ac.standalone_bytes,
          unique_reachable_bytes: $ac.unique_reachable_bytes,
          unique_object_bytes: $ac.unique_object_bytes,
          perfile_min_lz_sum: $slz,
          cdc_unique_raw: $scdc,
          cdc_unique_zstd: $scdcz,
          verdict: {
            vs_perfile_lz: (if $scdc==null then "na" else verdict($ac.unique_reachable_bytes;$slz) end),
            vs_cdc: (if $scdc==null then "na" else verdict($ac.unique_reachable_bytes;$scdc) end),
            vs_cdc_compressed: (if $scdcz==null then "na" else verdict($ac.unique_reachable_bytes;$scdcz) end)
          }
        }
  ] as $strata
  | {
      cohort: "phase9",
      files: ($perfile|length),
      source_bytes: ([$perfile[].source_bytes]|sumnm),
      standalone_bytes: $S,
      unique_reachable_bytes: $U,
      unique_root_bytes: ([$G.per_root[].root_bytes]|sumnm),
      unique_object_bytes: $UO,
      amortized_bytes_total: $G.amortized_bytes,
      amortized_bytes_per_file: $G.per_root,
      reachable_objects: $G.unique_objects,
      store_physical_bytes: $G.stored_bytes,
      dangling: ($G.dangling|length),
      gzip9_sum: $gzip_sum, zstd19_sum: $zstd_sum, xz9e_sum: $xz_sum, brotli11_sum: $brotli_sum,
      perfile_min_lz_sum: $minlz_sum,
      cdc_unique_raw: $cdc_unique,
      cdc_unique_zstd: $cdc_zstd,
      borg_version: $cdc[0].borg_version,
      cdc_params: $cdc[0].chunker_params,
      cdc_deterministic: $cdc[0].borg_deterministic,
      verdict: {
        vs_perfile_lz: verdict($U;$minlz_sum),
        vs_cdc: verdict($U;$cdc_unique),
        vs_cdc_compressed: verdict($U;$cdc_zstd),
        vs_min_baseline: verdict($U;([$minlz_sum,$cdc_unique,$cdc_zstd]|min)),
        object_bytes_vs_cdc: verdict($UO;$cdc_unique)
      },
      strata: $strata
    }
  ' > "$OUTDIR/results.json"

# Verification: every standalone and every store-backed root was decoded and
# byte-compared above; `set -e` means a single mismatch aborts the court. Record
# the count and the (must-be-zero) closure dangling count.
jq --argjson n "$N" '. + {verification:{files_checked:$n, standalone_byte_exact:true, store_backed_byte_exact:true}}' \
  "$OUTDIR/results.json" > "$TMP/results.verified.json"
mv "$TMP/results.verified.json" "$OUTDIR/results.json"

# Markdown report.
jq -r '
  "# Phase 9.3 store court — cohort `" + .cohort + "`",
  "",
  "Cohort: \(.files) files, source \(.source_bytes) B. All standalone and all store-backed roots decoded and `cmp`-ed byte-exact (\(.verification.files_checked)/\(.verification.files_checked)); closure dangling \(.dangling).",
  "",
  "## Totals (bytes)",
  "",
  "| metric | value |",
  "|---|---:|",
  "| source_bytes | \(.source_bytes) |",
  "| standalone_bytes (S) | \(.standalone_bytes) |",
  "| unique_reachable_bytes (U) | \(.unique_reachable_bytes) |",
  "| amortized_bytes_total (A = U) | \(.amortized_bytes_total) |",
  "| unique_object_bytes | \(.unique_object_bytes) |",
  "| store_physical_bytes | \(.store_physical_bytes) |",
  "| gzip9_sum | \(.gzip9_sum) |",
  "| zstd19_sum | \(.zstd19_sum) |",
  "| xz9e_sum | \(.xz9e_sum) |",
  "| brotli11_sum | \(.brotli11_sum) |",
  "| perfile_min_lz_sum | \(.perfile_min_lz_sum) |",
  "| cdc_unique_raw | \(.cdc_unique_raw) |",
  "| cdc_unique_zstd (chunk compression on) | \(.cdc_unique_zstd) |",
  "| cdc_params | \(.cdc_params) |",
  "",
  "VOLE unique-reachable vs per-file min LZ: **\(.verdict.vs_perfile_lz)**; vs strongest CDC (borg \(.borg_version), params \(.cdc_params), compression none): **\(.verdict.vs_cdc)**; vs the same CDC with chunk compression (zstd,19): **\(.verdict.vs_cdc_compressed)**; vs min(both): **\(.verdict.vs_min_baseline)**.",
  "",
  "## Per stratum (bytes)",
  "",
  "| stratum | files | source | standalone | unique reachable | unique object | per-file min LZ | CDC raw | CDC zstd | vs LZ | vs CDC | vs CDC+zstd |",
  "|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---|---|",
  (.strata[] | "| \(.stratum) | \(.files) | \(.source_bytes) | \(.standalone_bytes) | \(.unique_reachable_bytes) | \(.unique_object_bytes) | \(.perfile_min_lz_sum) | \(.cdc_unique_raw) | \(.cdc_unique_zstd) | \(.verdict.vs_perfile_lz) | \(.verdict.vs_cdc) | \(.verdict.vs_cdc_compressed) |"),
  "",
  "WIN = VOLE unique-reachable < baseline (or within 2% for TIE). A store root alone is never compared to a whole file: `standalone_bytes` is the whole-file universe."
' "$OUTDIR/results.json" > "$OUTDIR/report.md"

echo "results: $OUTDIR/results.json"
echo "report:  $OUTDIR/report.md"
jq '{source_bytes,standalone_bytes,unique_reachable_bytes,amortized_bytes_total,unique_object_bytes,store_physical_bytes,perfile_min_lz_sum,cdc_unique_raw,verdict}' "$OUTDIR/results.json"
