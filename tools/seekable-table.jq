# Render the Phase-8.4 seekable/blocked baseline court JSONL as a Markdown table
# (run with `jq -rs -f tools/seekable-table.jq queries.jsonl`). One row per
# pre-registered byte range. `B` is bytes a random-access reader must read to
# serve the range (covering block(s) + index/framing); `s` is user+sys decode
# seconds. VOLE is the seeked `view`'s instrumented `bytes_read`.
"| query | a (B) | VOLE B | VOLE s | bgzip B | bgzip s | xz64k B | xz64k s | xz1m B | xz1m s | xz4m B | xz4m s | pixz B | pixz s |",
"| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
(.[]
 | "| \(.query) | \(.range_start) | \(.vole.bytes_read) | \((.vole.user_s + .vole.sys_s) * 1000 | round / 1000) | \(.bgzip.bytes_read) | \((.bgzip.user_s + .bgzip.sys_s) * 1000 | round / 1000) | \(.xz64k.bytes_read) | \((.xz64k.user_s + .xz64k.sys_s) * 1000 | round / 1000) | \(.xz1m.bytes_read) | \((.xz1m.user_s + .xz1m.sys_s) * 1000 | round / 1000) | \(.xz4m.bytes_read) | \((.xz4m.user_s + .xz4m.sys_s) * 1000 | round / 1000) | \(.pixz.bytes_read) | \((.pixz.user_s + .pixz.sys_s) * 1000 | round / 1000) |"
 )
