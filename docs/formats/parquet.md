# Parquet

Parquet is the first **analytical** format of Phase 21 Wave 2. It is not a
package — the whole source is the document, and the exact leaf is the source.
The adapter is gated behind the **non-default, dependency-free** `parquet = []`
feature (no Thrift library is used).

## Authority boundary

Detection is byte-based and **conservative**: the `PAR1` magic at the start and
end of the file plus a readable Thrift-Compact footer under the limits. A
truncated file, a wrong-length tail, and prose stay `Opaque` and round-trip
exactly through the RAW lane. A truncated/bad-length footer that *looks* like
Parquet still round-trips exactly.

## Representation preservation

A bounded, dependency-free Thrift-Compact footer reader that exposes the
schema, the row-group and column-chunk inventory (**each chunk with its exact
source span** and statistics), and the decoded values for `PLAIN` and
`RLE_DICTIONARY` across the common physical types (`BOOLEAN`, `INT32`, `INT64`,
`FLOAT`, `DOUBLE`, `BYTE_ARRAY`, `FIXED_LEN_BYTE_ARRAY`), with `UNCOMPRESSED` and
`GZIP` codecs. Statistics and the exact chunk span are derived from the archival
footer, not re-derived by a query engine.

## Supported observations

Common selectors: `metadata`, `text`, `table`, `cell`, `find`. Native:
`--parquet-schema`, `--parquet-column`, `--parquet-row-group`, `--parquet-cell`.

## Unsupported / honest cost

**Typed declines** (never guessed): `INT96`; `DATA_PAGE_V2`; deprecated
`BIT_PACKED` levels; the `DELTA_*` / `GROUP_VAR_INT` / `BYTE_STREAM_SPLIT`
encodings; the `SNAPPY`/`ZSTD`/`BROTLI`/`LZO`/`LZ4` codecs; repeated/nested
columns; and legacy `min`/`max` statistics. A decompression bomb declines typed
(`resource_limit`) before allocation. `Page(n)` is a typed decline.

## Security limits

Bounded schema/row-group/chunk/row/decoded-byte/document caps; every Thrift read
is bounds-checked and non-panicking. A source over any cap declines typed. No
external reference is fetched.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted
Parquet, and after the source **and** descriptor are deleted in a fresh process
(the 21.14.1 court and the 21.14 economic court, exactness **13/13** and
**7/7**). The exact leaf is the whole source; the derived model is never on the
exactness path (ADR-0060: the model node depends on the `DocumentExact` root
keyed on `sha256(source)`).

## Known limitations

A bounded footer reader, not a columnar query engine. The economic court is
measured on a **self-authored deterministic corpus** and is deliberately
adversarial: for Parquet the comparator is **DuckDB itself**, and it **wins the
analytical axes** — columnar projection, predicate execution, compressed pages,
and selective reads via metadata statistics are native to it, while VOLE
materializes a bounded observation and counts in the driver. VOLE's unique claims
are exact closure (byte-authority) and the exact chunk span from the archival
footer; DuckDB is not a source-retaining store (its Q6 declines `not-native`).
Only exact closure is a byte-authority claim.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0059](../adr/0059-xlsx-adapter-and-analytical-comparator.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.14.1 Parquet court: `tools/phase21-14-1-parquet-court.sh` (exactness
  13/13, 3 typed declines, 3 opaque controls); campaign
  [2026-10-10-phase21-14-1-parquet-bfe32a33](../../evidence/campaigns/2026-10-10-phase21-14-1-parquet-bfe32a33/).
- Phase 21.14 economic court: `tools/phase21-14-parquet-court.sh` (source-
  retaining SQLite **and** the mandatory DuckDB/Parquet comparator; exactness
  7/7); campaign
  [2026-10-10-phase21-14-parquet-econ-bfe32a33](../../evidence/campaigns/2026-10-10-phase21-14-parquet-econ-bfe32a33/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md).
