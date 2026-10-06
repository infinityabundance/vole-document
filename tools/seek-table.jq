# Render the Phase-8 seek court JSONL as a Markdown table (run with
# `jq -rs -f tools/seek-table.jq queries.jsonl`). One row per pre-registered
# query. VOLE's primary metric is the instrumented `bytes_read` (the `strace`
# column cross-checks it at the descriptor-file syscall level); each sequential
# codec's `compRead` is the compressed bytes it must read to reach the same
# output offset, `CPU` = user+sys seconds.
"| query | a (B) | len | VOLE bytes_read | VOLE strace B | VOLE syscalls | VOLE CPU s | VOLE RSS kB | gzip compRead | gzip CPU s | gzip RSS kB | zstd compRead | zstd CPU s | zstd RSS kB | xz compRead | xz CPU s | xz RSS kB |",
"| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
(.[]
 | "| \(.query) | \(.range_start) | \(.range_len) | \(.voldoc.bytes_read) | \(.strace.bytes_read) | \(.strace.read_calls) | \((.vole_time.user_s + .vole_time.sys_s) * 1000 | round / 1000) | \(.vole_time.max_rss_kb) | \(.gzip.compressed_bytes_read) | \((.gzip.user_s + .gzip.sys_s) * 1000 | round / 1000) | \(.gzip.max_rss_kb) | \(.zstd.compressed_bytes_read) | \((.zstd.user_s + .zstd.sys_s) * 1000 | round / 1000) | \(.zstd.max_rss_kb) | \(.xz.compressed_bytes_read) | \((.xz.user_s + .xz.sys_s) * 1000 | round / 1000) | \(.xz.max_rss_kb) |"
 )
