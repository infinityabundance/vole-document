# Reduce the baseline-ladder JSONL into a table + summary.
#
# A positive delta means the best VOLE lane is LARGER than the comparator
# (i.e. VOLE loses); a negative delta means VOLE is smaller (wins).
# `best_vole` is the smallest complete serialized `.voldoc` size across every
# VOLE lane (RAW, RLE, BYTE_RANS, the forced structural kinds, and the unforced
# auto winner), so it is the most favourable VOLE number available.

def gm: [.gzip9, .zstd19, .xz9e, .brotli11] | map(select(. != null)) | if length == 0 then null else min end;

def summ($g):
  {
    files: ($g | length),
    vole_beats_gzip:   ($g | map(select(.best_vole != null and .gzip9 != null and .best_vole < .gzip9)) | length),
    vole_beats_zstd:   ($g | map(select(.best_vole != null and .zstd19 != null and .best_vole < .zstd19)) | length),
    vole_beats_xz:     ($g | map(select(.best_vole != null and .xz9e != null and .best_vole < .xz9e)) | length),
    vole_beats_brotli: ($g | map(select(.best_vole != null and .brotli11 != null and .best_vole < .brotli11)) | length),
    vole_beats_any_generic: ($g | map(select(.best_vole != null and (gm) != null and .best_vole < (gm))) | length),
    vole_beats_byte_rans: ($g | map(select(.best_vole != null and .byte_rans != null and .best_vole < .byte_rans)) | length),
    total_bytes_vs_best_generic: ($g | map(select(.delta_vs_best_generic != null) | .delta_vs_best_generic) | add // 0),
    total_bytes_vs_byte_rans: ($g | map(select(.delta_vs_byte_rans != null) | .delta_vs_byte_rans) | add // 0)
  };

. as $rows
| ($rows | map(
    . as $r
    | ([{k:"gzip", v:$r.gzip9}, {k:"zstd", v:$r.zstd19}, {k:"xz", v:$r.xz9e}, {k:"brotli", v:$r.brotli11}]
        | map(select(.v != null))) as $gen
    | ($gen | if length == 0 then null else (sort_by(.v) | .[0]) end) as $bg
    | $r + {
        rank: ([{k:"raw", v:$r.raw}, {k:"gzip", v:$r.gzip9}, {k:"zstd", v:$r.zstd19},
                {k:"xz", v:$r.xz9e}, {k:"brotli", v:$r.brotli11},
                {k:"byte_rans", v:$r.byte_rans}, {k:"best_vole", v:$r.best_vole}]
               | map(select(.v != null)) | sort_by(.v) | map(.k)),
        best_generic: $bg.v,
        best_generic_name: $bg.k,
        delta_vs_best_generic: (if $bg == null or $r.best_vole == null then null else $r.best_vole - $bg.v end),
        delta_vs_byte_rans: (if $r.byte_rans == null or $r.best_vole == null then null else $r.best_vole - $r.byte_rans end)
      }
  )) as $rows2
| {
    units: "bytes",
    delta_convention: "positive = best VOLE lane is larger than the comparator (VOLE loses); negative = VOLE is smaller (VOLE wins)",
    rows: $rows2,
    summary: {
      overall: (summ($rows2)),
      by_corpus: ($rows2 | group_by(.corpus) | map(. as $g | summ($g) + {corpus: $g[0].corpus}))
    }
  }
