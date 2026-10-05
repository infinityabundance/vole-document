# Render the baseline ladder as a markdown per-file table.
def cell: if . == null then "-" else tostring end;

"| corpus | file | source | gzip -9 | zstd -19 | xz -9e | brotli -q11 | BYTE_RANS | best VOLE | best VOLE lane | VOLE - best generic |",
"| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: |",
(.rows[] | "| \(.corpus) | `\(.file)` | \(.source_len) | \(.gzip9 | cell) | \(.zstd19 | cell) | \(.xz9e | cell) | \(.brotli11 | cell) | \(.byte_rans | cell) | \(.best_vole | cell) | \(.best_vole_lane) | \(.delta_vs_best_generic | cell) |")
