# Phase 21.14.1 Parquet court — matrix

| fixture | format | src len | out len | sha256 match | cmp | exact | decline rc | control rc |
| --- | --- | ---: | ---: | --- | --- | --- | ---: | ---: |
| `small_plain.parquet` | parquet | 972 | 972 | true | true | true | -1 | -1 |
| `optional.parquet` | parquet | 334 | 334 | true | true | true | -1 | -1 |
| `dictionary.parquet` | parquet | 329 | 329 | true | true | true | -1 | -1 |
| `gzip.parquet` | parquet | 976 | 976 | true | true | true | -1 | -1 |
| `multi_rg.parquet` | parquet | 1034 | 1034 | true | true | true | -1 | -1 |
| `two_pages.parquet` | parquet | 529 | 529 | true | true | true | -1 | -1 |
| `large.parquet` | parquet | 2097603 | 2097603 | true | true | true | -1 | -1 |
| `unsupported_codec.parquet` | parquet | 329 | 329 | true | true | true | 6 | -1 |
| `unsupported_encoding.parquet` | parquet | 158 | 158 | true | true | true | 6 | -1 |
| `bomb.parquet` | parquet | 192 | 192 | true | true | true | 8 | -1 |
| `prose.txt` | opaque | 87 | 87 | true | true | true | -1 | 6 |
| `truncated.parquet` | opaque | 968 | 968 | true | true | true | -1 | 6 |
| `badlen.parquet` | opaque | 972 | 972 | true | true | true | -1 | 6 |
