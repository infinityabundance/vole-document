#!/bin/sh
# Phase-11.14 fine-unit share court — the decisive measurement for the
# finer-than-object direction named in FINDINGS.md §7.
#
# Over a small cohort built from the real Phase-7 producer corpus plus
# deterministic byte-identical repeats and a few-bytes-changed near-duplicate,
# this court measures, on one run, one machine and one base image:
#
#   * VOLE fine-unit sharing (src/field/share.rs): `share-account` stores every
#     inline fine unit and reports the cohort and per-stratum total/unique bytes,
#     with a per-kind table (`object`, `channel_payload`, `channel_header`,
#     `model`). `unique_bytes` counts each distinct unit once and EXCLUDES all
#     record/root framing and the program/GRAPH bytes, so it is a LOWER BOUND on
#     any store form, never an achieved store size.
#   * per-file generic LZ: the sum over files of `gzip -9`, `zstd -19` and
#     `xz -9e` (each decompressed and `cmp`-ed), plus the whole-cohort
#     `tar | xz -9e` single stream.
#   * generic content-defined-chunk dedup: borg with the frozen Phase-9.3
#     parameters `19,23,21,4095 --compression none` (the primary comparison),
#     plus a small parameter sensitivity sweep. Following ADR-0021's rigor, the
#     verdict uses the *strongest* CDC result, not a coarse strawman.
#
# All three are schemes for the SAME source population: VOLE's unique unit bytes
# are descriptor-derived; per-file LZ and CDC are source-derived.
#
# Exactness: every descriptor is `verify`-ed and `decode`-ed and byte-compared
# (`cmp`) against its source, so `materialize == source` holds for the whole
# cohort before any sharing number is reported.
#
# Runs inside the pinned `baseline` image (dev toolchain + gzip/zstd/xz/brotli/
# jq/borg). Nothing runs on the host.
#
# Usage:
#   sh tools/field-share-court.sh [OUTDIR] [COHORT_SRC]
set -eu

cd /work
LC_ALL=C
export LC_ALL

OUTDIR=${1:-evidence/scratch/phase11-share}
COHORT_SRC=${2:-evidence/corpus/phase7-producers}
BIN=${VOLE_BIN:-./target/debug/vole-document}

# Frozen Phase-9.3 parameters (the primary CDC comparison) and a finer sweep
# (the strongest available; the verdict uses the smallest unique bytes).
CDC_FROZEN="19,23,21,4095"
CDC_SWEEP="19,23,21,4095 10,15,11,127 11,16,13,4095 9,16,11,4095 8,16,10,4095"

mkdir -p "$OUTDIR/raw"

# Always ensure the measurement binary has the `field` feature (and thus the
# `share-account` verb); the shared cargo target volume may hold a
# `--no-default-features` build from a prior gate run. Incremental in practice.
echo "field-share-court: building cargo build --locked --all-features" >&2
cargo build --locked --all-features 1>&2

# The Phase-7 producer corpus is locally generated and gitignored; its sha256s
# and generator provenance are pinned in evidence/corpus/phase7-producers/.
for p in cairo-vector libreoffice-export pdftex-doc reportlab-multipage; do
  if [ ! -f "$COHORT_SRC/$p.pdf" ]; then
    echo "field-share-court: missing $COHORT_SRC/$p.pdf" >&2
    echo "  regenerate the Phase-7 producer corpus first (pinned producers image):" >&2
    echo "    docker compose run --rm --no-TTY producers sh tools/pdf-corpus-producers.sh" >&2
    exit 1
  fi
done

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
COHORT="$TMP/cohort"
DESC="$TMP/desc"
mkdir -p "$COHORT/producer" "$COHORT/repeat" "$COHORT/near" "$DESC"

# ---------------------------------------------------------------------------
# Cohort: real producers + deterministic repeats + a few-bytes-changed variant.
# ---------------------------------------------------------------------------
for p in cairo-vector libreoffice-export pdftex-doc reportlab-multipage; do
  cp "$COHORT_SRC/$p.pdf" "$COHORT/producer/$p.pdf"
done
cp "$COHORT_SRC/reportlab-multipage.pdf" "$COHORT/repeat/reportlab.r1.pdf"
cp "$COHORT_SRC/reportlab-multipage.pdf" "$COHORT/repeat/reportlab.r2.pdf"
cp "$COHORT_SRC/libreoffice-export.pdf" "$COHORT/near/libreoffice.base.pdf"
cp "$COHORT_SRC/libreoffice-export.pdf" "$COHORT/near/libreoffice.variant.pdf"
VLI=$(wc -c < "$COHORT/near/libreoffice.variant.pdf" | tr -d ' ')
VOFF=$((VLI / 2))
printf 'VOLENEAR' | dd of="$COHORT/near/libreoffice.variant.pdf" bs=1 seek="$VOFF" conv=notrunc status=none

N=$(find "$COHORT" -type f | wc -l | tr -d ' ')
echo "field-share-court: cohort=$N files (producer 4, repeat 2, near 2), variant offset=$VOFF"

# Cohort manifest (stratum, relative path, length, sha256).
COHORTJSONL="$OUTDIR/raw/cohort.jsonl"
COHORTTSV="$TMP/cohort.tsv"
: > "$COHORTJSONL"
: > "$COHORTTSV"
for f in $(find "$COHORT" -type f | sort); do
  rel=${f#"$COHORT"/}
  stratum=${rel%%/*}
  len=$(wc -c < "$f" | tr -d ' ')
  sha=$(sha256sum "$f" | cut -d' ' -f1)
  printf '{"stratum":"%s","file":"%s","len":%s,"sha256":"%s"}\n' "$stratum" "$rel" "$len" "$sha" >> "$COHORTJSONL"
  printf '%s\t%s\t%s\t%s\n' "$stratum" "$rel" "$len" "$sha" >> "$COHORTTSV"
done

# ---------------------------------------------------------------------------
# Encode + exactness + share-account (VOLE fine units).
# ---------------------------------------------------------------------------
INDEX="$TMP/index.tsv"
: > "$INDEX"
ENCROWS="$OUTDIR/raw/encode.jsonl"
: > "$ENCROWS"
while IFS='	' read -r stratum rel len sha; do
  f="$COHORT/$rel"
  uniq=$(printf '%s' "$rel" | tr '/.' '__')
  desc="$DESC/$uniq.voldoc"
  json=$($BIN encode "$f" "$desc")
  cand=$(printf '%s' "$json" | jq -r .candidate)
  elen=$(printf '%s' "$json" | jq -r .encoded_len)
  graph=$(printf '%s' "$json" | jq -r '.cost.graph')
  # Exactness triple, required explicitly: length, SHA-256, byte compare.
  $BIN verify "$desc" >/dev/null
  $BIN decode "$desc" "$TMP/dec" >/dev/null
  cmp "$TMP/dec" "$f"
  dec_len=$(wc -c < "$TMP/dec" | tr -d ' ')
  dec_sha=$(sha256sum "$TMP/dec" | cut -d' ' -f1)
  [ "$dec_len" = "$len" ] || { echo "field-share-court: length mismatch $rel" >&2; exit 1; }
  [ "$dec_sha" = "$sha" ] || { echo "field-share-court: sha256 mismatch $rel" >&2; exit 1; }
  printf '{"stratum":"%s","file":"%s","source_len":%s,"source_sha256":"%s","candidate":"%s","encoded_len":%s,"graph_bytes":%s}\n' \
    "$stratum" "$rel" "$len" "$sha" "$cand" "$elen" "$graph" >> "$ENCROWS"
  printf '%s\t%s\t%s\n' "$stratum" "$rel" "$desc" >> "$INDEX"
done < "$COHORTTSV"

descs=$(cut -f3 "$INDEX" | tr '\n' ' ')
$BIN share-account --store "$TMP/share" $descs > "$OUTDIR/raw/share-cohort.json"

STRATAROWS="$OUTDIR/raw/share-strata.jsonl"
: > "$STRATAROWS"
for s in producer repeat near; do
  sdescs=$(awk -F'\t' -v s="$s" '$1==s {print $3}' "$INDEX" | tr '\n' ' ')
  $BIN share-account --store "$TMP/share-$s" $sdescs \
    | jq -c --arg s "$s" '. + {stratum:$s}' >> "$STRATAROWS"
done

# Consistency: the store holds exactly the reported unique units.
store_b=$(jq -r .store_bytes "$OUTDIR/raw/share-cohort.json")
uniq_b=$(jq -r .unique_bytes "$OUTDIR/raw/share-cohort.json")
if [ "$store_b" != "$uniq_b" ]; then
  echo "field-share-court: store_bytes ($store_b) != unique_bytes ($uniq_b)" >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# Per-file generic LZ (round-trip verified) and the tar | xz -9e single stream.
# ---------------------------------------------------------------------------
LZROWS="$OUTDIR/raw/lz.jsonl"
: > "$LZROWS"
for f in $(find "$COHORT" -type f | sort); do
  rel=${f#"$COHORT"/}
  stratum=${rel%%/*}
  src=$(wc -c < "$f" | tr -d ' ')
  gzip -9 -c "$f" > "$TMP/g.gz";  gzip -dc "$TMP/g.gz"  | cmp -s - "$f"
  zstd -19 --long=27 -q -c "$f" > "$TMP/z.zst"; zstd -d --long=27 -q -c "$TMP/z.zst" | cmp -s - "$f"
  xz -9e -c "$f" > "$TMP/x.xz";  xz -dc "$TMP/x.xz"  | cmp -s - "$f"
  g=$(wc -c < "$TMP/g.gz" | tr -d ' ')
  z=$(wc -c < "$TMP/z.zst" | tr -d ' ')
  x=$(wc -c < "$TMP/x.xz" | tr -d ' ')
  m=$g; [ "$z" -lt "$m" ] && m=$z; [ "$x" -lt "$m" ] && m=$x
  printf '{"stratum":"%s","file":"%s","source_len":%s,"gzip9":%s,"zstd19":%s,"xz9e":%s,"min_lz":%s}\n' \
    "$stratum" "$rel" "$src" "$g" "$z" "$x" "$m" >> "$LZROWS"
done

# Deterministic single-stream archive: fixed mtime/owner/group, numeric owners
# and sorted names, so the tar header (and therefore the xz size) does not depend
# on the copy timestamps. GNU tar (bookworm 1.34) supports all four flags.
TARFLAGS="--mtime=@0 --owner=0 --group=0 --numeric-owner --sort=name --format=ustar"
tar -C "$COHORT" $TARFLAGS -cf "$TMP/cohort.tar" .
tar -C "$COHORT" $TARFLAGS -cf - . | xz -9e -c > "$TMP/cohort.tar.xz"
tar_xz=$(wc -c < "$TMP/cohort.tar.xz" | tr -d ' ')
# Round-trip: the decompressed stream must be the exact same tar bytes; and a
# repack must be byte-identical (deterministic archive).
xz -dc "$TMP/cohort.tar.xz" | cmp -s - "$TMP/cohort.tar"
tar -C "$COHORT" $TARFLAGS -cf "$TMP/cohort.tar2" .
cmp -s "$TMP/cohort.tar" "$TMP/cohort.tar2"

# ---------------------------------------------------------------------------
# Generic CDC (borg, frozen params) + sensitivity sweep + per stratum.
# ---------------------------------------------------------------------------
sh tools/chunk-dedup.sh "$COHORT" "$OUTDIR/raw/cdc-frozen.json" "$CDC_FROZEN" none >/dev/null

SWEEPROWS="$TMP/cdc-sweep.jsonl"
: > "$SWEEPROWS"
BEST_P=""
BEST_U=""
for p in $CDC_SWEEP; do
  sh tools/chunk-dedup.sh "$COHORT" "$TMP/cdc-param-$p.json" "$p" none >/dev/null
  cat "$TMP/cdc-param-$p.json" >> "$SWEEPROWS"
  printf '\n' >> "$SWEEPROWS"
  u=$(jq -r .borg_unique_bytes "$TMP/cdc-param-$p.json")
  if [ -z "$BEST_U" ] || [ "$u" -lt "$BEST_U" ]; then BEST_U=$u; BEST_P=$p; fi
done
jq -s 'sort_by(.borg_unique_bytes)' "$SWEEPROWS" > "$OUTDIR/raw/cdc-sweep.json"
echo "field-share-court: strongest CDC params=$BEST_P unique=$BEST_U"

CDCSTRATA="$OUTDIR/raw/cdc-strata.jsonl"
: > "$CDCSTRATA"
for s in producer repeat near; do
  sh tools/chunk-dedup.sh "$COHORT/$s" "$TMP/cdc-$s-frozen.json" "$CDC_FROZEN" none >/dev/null
  jq -c --arg s "$s" --arg k frozen '. + {stratum:$s,kind:$k}' "$TMP/cdc-$s-frozen.json" >> "$CDCSTRATA"
  sh tools/chunk-dedup.sh "$COHORT/$s" "$TMP/cdc-$s-best.json" "$BEST_P" none >/dev/null
  jq -c --arg s "$s" --arg k best '. + {stratum:$s,kind:$k}' "$TMP/cdc-$s-best.json" >> "$CDCSTRATA"
done

# ---------------------------------------------------------------------------
# Reduce to results.json.
# ---------------------------------------------------------------------------
jq -n \
  --slurpfile vuln "$OUTDIR/raw/share-cohort.json" \
  --slurpfile vstrata "$STRATAROWS" \
  --slurpfile enc "$ENCROWS" \
  --slurpfile lz "$LZROWS" \
  --slurpfile cdc "$OUTDIR/raw/cdc-frozen.json" \
  --slurpfile cdcs "$OUTDIR/raw/cdc-sweep.json" \
  --slurpfile cdcst "$CDCSTRATA" \
  --argjson tarxz "$tar_xz" \
  --arg frozen "$CDC_FROZEN" \
  --arg bestp "$BEST_P" '
  def sumnm: (map(select(.!=null)) | add // 0);
  def verdict($x;$y): (if $y==null then "na" elif $x < $y then "WIN" elif ($x <= ($y*1.02)) then "TIE" else "LOSS" end);
  ($vuln[0]) as $V
  | ($cdcs[0] | map(.borg_unique_bytes) | min) as $cdc_best
  | ([$lz[].gzip9] | sumnm) as $gzip
  | ([$lz[].zstd19] | sumnm) as $zstd
  | ([$lz[].xz9e] | sumnm) as $xz
  | ([$lz[].min_lz] | sumnm) as $minlz
  | ([$enc[].encoded_len] | sumnm) as $descriptor_S
  | ([$enc[].graph_bytes] | sumnm) as $descriptor_graph
  | ($V.unique_bytes) as $vU
  | (if $minlz < $cdc_best then $minlz else $cdc_best end) as $strongest
  | {
      cohort: {
        files: ($enc|length),
        source_bytes: ([$lz[].source_len] | sumnm),
        strata: { producer: 4, repeat: 2, near: 2 }
      },
      vole_fine_units: {
        total_bytes: $V.total_bytes,
        unique_bytes: $V.unique_bytes,
        unit_count: $V.unit_count,
        unique_count: $V.unique_count,
        by_kind: $V.by_kind,
        store_bytes_equal_unique: ($V.store_bytes == $V.unique_bytes)
      },
      descriptor_standalone_bytes_S: $descriptor_S,
      descriptor_graph_bytes: $descriptor_graph,
      descriptor_nonunit_bytes: ($descriptor_S - $V.total_bytes),
      descriptor_nonunit_non_graph_bytes: (($descriptor_S - $V.total_bytes) - $descriptor_graph),
      vole_win_margin_vs_perfile_lz: ($minlz - $vU),
      perfile_lz: {
        gzip9_sum: $gzip, zstd19_sum: $zstd, xz9e_sum: $xz,
        perfile_min_lz_sum: $minlz, tar_xz9e_single_stream: $tarxz
      },
      cdc: {
        frozen_params: $frozen,
        frozen_unique_raw: $cdc[0].borg_unique_bytes,
        frozen_deterministic: $cdc[0].borg_deterministic,
        strongest_params: $bestp,
        strongest_unique_raw: $cdc_best
      },
      verdict_lower_bound: {
        universe: "VOLE unique unit bytes (descriptor-derived, excludes root/program framing) vs source-derived per-file LZ and CDC",
        vs_perfile_min_lz: verdict($vU; $minlz),
        vs_perfile_gzip9: verdict($vU; $gzip),
        vs_tar_xz9e: verdict($vU; $tarxz),
        vs_cdc_frozen: verdict($vU; $cdc[0].borg_unique_bytes),
        vs_cdc_strongest: verdict($vU; $cdc_best),
        vs_strongest_generic: verdict($vU; $strongest)
      },
      strata: [
        ($vstrata[] | . as $vs
          | ($vs.stratum) as $s
          | ([$lz[] | select(.stratum==$s)] | map(.min_lz) | sumnm) as $slz
          | ([$lz[] | select(.stratum==$s)] | map(.source_len) | sumnm) as $ssrc
          | ([$cdcst[] | select(.stratum==$s and .kind=="frozen")][0].borg_unique_bytes // null) as $scdcf
          | ([$cdcst[] | select(.stratum==$s and .kind=="best")][0].borg_unique_bytes // null) as $scdcb
          | {
              stratum: $s,
              files: ([$enc[] | select(.stratum==$s)] | length),
              source_bytes: $ssrc,
              descriptor_standalone_bytes: ([$enc[] | select(.stratum==$s)] | map(.encoded_len) | sumnm),
              descriptor_graph_bytes: ([$enc[] | select(.stratum==$s)] | map(.graph_bytes) | sumnm),
              descriptor_nonunit_bytes: (([$enc[] | select(.stratum==$s)] | map(.encoded_len) | sumnm) - $vs.total_bytes),
              vole_unique_bytes: $vs.unique_bytes,
              vole_total_bytes: $vs.total_bytes,
              by_kind: $vs.by_kind,
              perfile_min_lz_sum: $slz,
              cdc_frozen_unique: $scdcf,
              cdc_strongest_unique: $scdcb,
              verdict: {
                vs_perfile_lz: verdict($vs.unique_bytes; $slz),
                vs_cdc_frozen: verdict($vs.unique_bytes; $scdcf),
                vs_cdc_strongest: verdict($vs.unique_bytes; $scdcb)
              }
            })
      ]
    }
  ' > "$OUTDIR/results.json"

# ---------------------------------------------------------------------------
# Receipt (environment + results) and SUMMARY.md.
# ---------------------------------------------------------------------------
GIT_COMMIT=$(git rev-parse HEAD)
GIT_SHORT=$(git rev-parse --short HEAD)
GIT_DIRTY=$(git status --porcelain | tr '\n' ';' | sed 's/"/\\"/g')
LOCK_SHA=$(sha256sum Cargo.lock | cut -d' ' -f1)
RUSTC=$(rustc --version)
CARGO=$(cargo --version)
ARCH=$(uname -m)
BORG_V=$(borg --version | awk '{print $2}')
GZIP_V=$(gzip --version | head -1)
ZSTD_V=$(zstd --version)
XZ_V=$(xz --version | head -1)
JQ_V=$(jq --version)

jq -n \
  --arg invoker "tools/field-share-court.sh" \
  --arg commit "$GIT_COMMIT" --arg short "$GIT_SHORT" --arg dirty "$GIT_DIRTY" \
  --arg arch "$ARCH" --arg lock "$LOCK_SHA" \
  --arg rustc "$RUSTC" --arg cargo "$CARGO" \
  --arg base "rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e" \
  --arg image "vole-document/baseline:1.99.0" \
  --arg borg "$BORG_V" --arg gzip "$GZIP_V" --arg zstd "$ZSTD_V" --arg xz "$XZ_V" --arg jq "$JQ_V" \
  --argjson results "$(cat "$OUTDIR/results.json")" \
  '{
    campaign: "phase11-share-court",
    phase: "Phase 11.14 — finer-than-object shareable units (share-account + measured court)",
    environment: {
      git: { commit: $commit, commit_short: $short, dirty: $dirty },
      arch: $arch,
      cargo_lock_sha256: $lock,
      toolchain: { rustc: $rustc, cargo: $cargo },
      images: { base_stable_digest: $base, service_image: $image },
      oracles: { borg: $borg, gzip: $gzip, zstd: $zstd, xz: $xz, jq: $jq },
      env_affecting_semantics: { LC_ALL: "C", VOLE_BIN: "./target/debug/vole-document" }
    },
    accounting_universes: {
      source: "original document bytes (archival reality)",
      descriptor: "exact standalone .voldoc bytes (S); the fine-unit figures derive from these",
      vole_fine_units: "unique descriptor unit bytes (object/channel_payload/channel_header/model); EXCLUDES root framing and program/GRAPH bytes => lower bound, not an achieved store",
      perfile_lz: "sum of independent whole-source lossless compressor outputs",
      cdc: "content-defined-chunk store over whole sources (borg, raw)"
    },
    invoker: $invoker,
    results: $results
  }' > "$OUTDIR/receipt.json"

# commands.txt
{
  echo "# Phase 11.14 fine-unit share court — exact commands"
  echo "# Commit under test: $GIT_COMMIT (branch $(git rev-parse --abbrev-ref HEAD)); dirty: $GIT_DIRTY"
  echo "# All commands run inside the pinned baseline image; nothing on the host."
  echo
  echo "docker compose run --rm --no-TTY baseline sh tools/field-share-court.sh $OUTDIR"
  echo
  echo "# Inside field-share-court.sh:"
  echo "#   cohort: cp \$COHORT_SRC/{cairo-vector,libreoffice-export,pdftex-doc,reportlab-multipage}.pdf producer/"
  echo "#           repeat/reportlab.r{1,2}.pdf = two identical copies"
  echo "#           near/libreoffice.{base,variant}.pdf; variant = base with 8 bytes 'VOLENEAR' at offset len/2"
  echo "#   \\$BIN encode SRC DESC.voldoc   (auto complete-cost winner)"
  echo "#   \\$BIN verify DESC.voldoc ; \\$BIN decode DESC.voldoc DEC ; cmp DEC SRC   (exact-profile triple)"
  echo "#   \\$BIN share-account --store DIR DESC...   (cohort and per stratum)"
  echo "#   gzip -9 / zstd -19 --long=27 / xz -9e per file, each decompressed and cmp-ed"
  echo "#   tar -C cohort -cf - . | xz -9e -c   (single-stream cross-file baseline; round-trip checked)"
  echo "#   sh tools/chunk-dedup.sh COHORT OUT $CDC_FROZEN none   (frozen primary)"
  echo "#   sh tools/chunk-dedup.sh COHORT OUT PARAMS none        (sweep: $CDC_SWEEP)"
} > "$OUTDIR/commands.txt"

# SUMMARY.md
jq -r '
  "# Phase 11.14 fine-unit share court",
  "",
  "Generated by `tools/field-share-court.sh` inside the pinned `baseline` service.",
  "",
  "Cohort: \(.results.cohort.files) files, source \(.results.cohort.source_bytes) B (producer 4, repeat 2, near 2); every descriptor `verify`-ed and `decode`-ed and `cmp`-ed byte-exact.",
  "",
  "## Headline (bytes)",
  "",
  "| metric | bytes |",
  "|---|---:|",
  "| source_bytes | \(.results.cohort.source_bytes) |",
  "| descriptor standalone S (auto winners) | \(.results.descriptor_standalone_bytes_S) |",
  "| descriptor non-unit bytes (excluded from unique_bytes) | \(.results.descriptor_nonunit_bytes) |",
  "| — of which GRAPH/program bytes | \(.results.descriptor_graph_bytes) |",
  "| VOLE fine-unit total_bytes | \(.results.vole_fine_units.total_bytes) |",
  "| VOLE fine-unit unique_bytes (lower bound) | \(.results.vole_fine_units.unique_bytes) |",
  "| per-file gzip9 sum | \(.results.perfile_lz.gzip9_sum) |",
  "| per-file zstd19 sum | \(.results.perfile_lz.zstd19_sum) |",
  "| per-file xz9e sum | \(.results.perfile_lz.xz9e_sum) |",
  "| per-file min LZ sum | \(.results.perfile_lz.perfile_min_lz_sum) |",
  "| tar \\| xz -9e (single stream) | \(.results.perfile_lz.tar_xz9e_single_stream) |",
  "| CDC borg frozen (\(.results.cdc.frozen_params), raw) | \(.results.cdc.frozen_unique_raw) |",
  "| CDC borg strongest (\(.results.cdc.strongest_params), raw) | \(.results.cdc.strongest_unique_raw) |",
  "",
  "## VOLE fine units by kind",
  "",
  "| kind | total | unique |",
  "|---|---:|---:|",
  (.results.vole_fine_units.by_kind[] | "| \(.kind) | \(.total) | \(.unique) |"),
  "",
  "## Verdict (VOLE unique_bytes = lower bound vs each generic scheme)",
  "",
  "- vs per-file min LZ: **\(.results.verdict_lower_bound.vs_perfile_min_lz)**",
  "- vs per-file gzip -9: **\(.results.verdict_lower_bound.vs_perfile_gzip9)**",
  "- vs tar \\| xz -9e: **\(.results.verdict_lower_bound.vs_tar_xz9e)**",
  "- vs CDC frozen raw: **\(.results.verdict_lower_bound.vs_cdc_frozen)**",
  "- vs CDC strongest raw: **\(.results.verdict_lower_bound.vs_cdc_strongest)**",
  "- vs strongest generic (min of per-file LZ and CDC): **\(.results.verdict_lower_bound.vs_strongest_generic)**",
  "",
  "WIN = VOLE unique_bytes < baseline (TIE within 2%). `unique_bytes` excludes root framing and the program/GRAPH bytes, so a WIN is a WIN *of the lower bound only*, not an achieved store size.",
  "",
  "## Per stratum (bytes)",
  "",
  "| stratum | files | source | descriptor S | VOLE unique | VOLE total | per-file min LZ | CDC frozen | CDC strongest | vs LZ | vs CDC frozen | vs CDC strongest |",
  "|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---|---|",
  (.results.strata[] | "| \(.stratum) | \(.files) | \(.source_bytes) | \(.descriptor_standalone_bytes) | \(.vole_unique_bytes) | \(.vole_total_bytes) | \(.perfile_min_lz_sum) | \(.cdc_frozen_unique) | \(.cdc_strongest_unique) | \(.verdict.vs_perfile_lz) | \(.verdict.vs_cdc_frozen) | \(.verdict.vs_cdc_strongest) |"),
  "",
  "Per-stratum by_kind and full JSON are in `raw/` and `results.json`.",
  "",
  "## Accounting discipline",
  "",
  "A store root is never a whole document. `unique_bytes` is a lower bound: it excludes all record/root framing, the universe/graph, the integrity manifest, and any program bytes. It is only ever compared to the per-file compressor ladder and to generic CDC, never to a single file. The store is populated (`store_bytes == unique_bytes`) so the sharing rests on stored bytes, not a paper calculation."
' "$OUTDIR/receipt.json" > "$OUTDIR/SUMMARY.md"

echo "results: $OUTDIR/results.json"
echo "receipt: $OUTDIR/receipt.json"
echo "summary: $OUTDIR/SUMMARY.md"
jq '{source_bytes: .cohort.source_bytes, vole_unique: .vole_fine_units.unique_bytes, descriptor_S: .descriptor_standalone_bytes_S, min_lz: .perfile_lz.perfile_min_lz_sum, cdc_frozen: .cdc.frozen_unique_raw, cdc_strongest: .cdc.strongest_unique_raw, verdict: .verdict_lower_bound}' "$OUTDIR/results.json"
