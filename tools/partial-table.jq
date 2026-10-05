# Render the Phase-7.3 partial-materialization query court JSONL as a Markdown
# table (run with `jq -rs -f tools/partial-table.jq queries.jsonl`). One row per
# pre-registered query; VOLE's primary "bytes processed" is
# descriptor_bytes_traversed + entropy_bytes_decoded, and each sequential codec's
# is the decompressed bytes it must inflate to reach the range.
"| query | a (B) | len | VOLE bytes | VOLE CPU s | VOLE RSS kB | gzip infl | gzip CPU s | gzip RSS kB | zstd infl | zstd CPU s | zstd RSS kB | xz infl | xz CPU s | xz RSS kB |",
"| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
(.[]
 | "| \(.query) | \(.range_start) | \(.range_len) | \(.voldoc.descriptor_bytes_traversed + .voldoc.entropy_bytes_decoded) | \((.vole_time.user_s + .vole_time.sys_s) * 1000 | round / 1000) | \(.vole_time.max_rss_kb) | \(.gzip.decompressed_bytes) | \((.gzip.user_s + .gzip.sys_s) * 1000 | round / 1000) | \(.gzip.max_rss_kb) | \(.zstd.decompressed_bytes) | \((.zstd.user_s + .zstd.sys_s) * 1000 | round / 1000) | \(.zstd.max_rss_kb) | \(.xz.decompressed_bytes) | \((.xz.user_s + .xz.sys_s) * 1000 | round / 1000) | \(.xz.max_rss_kb) |"
 )
