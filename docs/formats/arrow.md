# Arrow IPC

Apache Arrow IPC is the second **analytical** format of Phase 21 Wave 2. It is
not a package — the whole source is the document, and the exact leaf is the
source. The adapter is gated behind the **non-default, dependency-free**
`arrow = []` feature (the Flatbuffers metadata is read by a hand-written
bounds-checked reader).

## Authority boundary

Detection is byte-based and **conservative**: the Arrow file magic/footer
(`ARROW1`) **or** a coherent encapsulated-message **stream** framing (below), in
each case with a readable Flatbuffers schema/footer under the limits. A magic-only
file, a truncated file, a wrong-length body, and prose stay `Opaque` and round-trip
exactly through the RAW lane. A malformed Flatbuffers metadata block is a typed
decline.

## Detection and stream framing (Phase 27.5)

Arrow IPC has two framings. The **file** format leads with the `ARROW1` magic and
trails a footer. The **stream** format is a bare, **magic-less** sequence of
encapsulated messages. VOLE now claims the genuine stream framing as well: a
continuation-marked framing `Schema` message first (its Flatbuffer metadata parsed
under the bounded reader), then well-formed `RecordBatch`/`DictionaryBatch`
messages, ending in an **end-of-stream** marker. The ordered message inventory
(`Schema`, each record batch, end-of-stream) carries **exact byte spans**; the
messages are contiguous from offset 0 and cover the whole source, and the
record-batch message spans equal the record-batch inventory spans.

The legacy pre-0.15 **no-continuation** framing (a bare `int32` metadata size with
no `0xFFFFFFFF` continuation token) is deliberately **declined**: a magic-less
source has no other anchor, so treating an arbitrary leading word as a length would
widen the false-positive surface, and the stream parser admits only the
continuation form. A random `0xFFFFFFFF`-prefixed blob, a truncated stream, a
stream whose first message is not a `Schema`, and prose all stay `Opaque`.

A **spec-conformant** magic-less stream fixture (`tools/fixtures/arrow/stream.arrow`,
written by `tools/fixtures/make-arrow.py`) is now part of the corpus and closes
byte-exactly.

## Representation preservation

A bounded, dependency-free Flatbuffers reader that exposes the
footer/leading-schema inventory — the schema, and the record batches with **exact
source spans** — and decodes a subset of the columnar buffers: `Int` (all widths,
signed/unsigned), `FloatingPoint` (half/single/double), `Boolean`,
`Date`/`Time`/`Timestamp`/`Duration` (as raw integers), `Utf8`/`LargeUtf8`,
`Binary`/`LargeBinary`, `FixedSizeBinary`, with validity bitmaps. Both the file
and the stream formats, and multiple record batches, are supported.

## Supported observations

Common selectors: `metadata`, `text`, `table`, `cell`, `find`. Native:
`--arrow-schema`, `--arrow-column`, `--arrow-batch`, `--arrow-cell`.

## Unsupported / honest cost

**Typed declines** (never guessed): `Null`, `Decimal`, `Interval`, every nested
type (`List`/`LargeList`/`FixedSizeList`/`ListView`/`LargeListView`/`Struct`/`Map`/
`Union`/`RunEndEncoded`), `BinaryView`/`Utf8View`, dictionary-encoded fields,
big-endian bodies, and any `BodyCompression`. A decompression bomb declines typed
(`resource_limit`) before allocation. `Page(n)` is a typed decline.

## Security limits

Bounded schema/batch/column/row/decoded-byte/document caps; every Flatbuffers
read is bounds-checked and non-panicking. A source over any cap declines typed.
No external reference is fetched.

## Exact reconstruction

`materialize == original_bytes` (length + SHA-256 + `cmp`) for any admitted Arrow
IPC, and after the source **and** descriptor are deleted in a fresh process (the
21.16.1 court and the 21.16 economic court, exactness **18/18** and **3/3**). The
exact leaf is the whole source; the derived model is never on the exactness path
(ADR-0060: the model node depends on the `DocumentExact` root keyed on
`sha256(source)`).

## Known limitations

A bounded metadata reader, not an Arrow compute engine. The economic court is
measured on a **self-authored deterministic corpus**; the comparator is **DuckDB**,
which **wins the analytical axes** (columnar projection, predicate execution,
metadata statistics). Because the pinned DuckDB wheel exposes only the Arrow
**C data interface** scanners (`arrow_scan*`), not an Arrow IPC file reader, the
DuckDB lane reads a **Parquet projection** of the same logical table; that
substitution is recorded, not hidden. VOLE's unique claims are exact closure
(byte-authority) and the exact buffer span from the archival metadata; DuckDB is
not a source-retaining store (its Q6 declines `not-native`). Only exact closure
is a byte-authority claim.

## Relevant ADRs

[0029](../adr/0029-multi-format-authority-model.md),
[0031](../adr/0031-common-observation-model.md),
[0054](../adr/0054-repeatability-and-paired-measurement.md),
[0059](../adr/0059-xlsx-adapter-and-analytical-comparator.md),
[0060](../adr/0060-source-scoped-node-identity.md).

## Evidence

- Phase 21.16.1 Arrow court: `tools/phase21-16-1-arrow-court.sh` (exactness
  18/18, 6 typed declines, 4 opaque controls); campaign
  [2026-10-10-phase21-16-1-arrow-4fdc7ad0](../../evidence/campaigns/2026-10-10-phase21-16-1-arrow-4fdc7ad0/).
- Phase 21.16 economic court: `tools/phase21-16-arrow-court.sh` (source-
  retaining SQLite **and** a DuckDB Parquet-projection comparator; exactness
  3/3); campaign
  [2026-10-10-phase21-16-arrow-econ-4fdc7ad0](../../evidence/campaigns/2026-10-10-phase21-16-arrow-econ-4fdc7ad0/).
- Phase 27.5.1 Arrow **stream** court: `tools/phase27-5-1-arrow-stream-court.sh`
  (exactness 2/2 on the conformant magic-less stream, 5/5 near-miss `Opaque`
  controls); campaign
  [2026-10-12-phase27-5-1-arrow-stream-efb51887](../../evidence/campaigns/2026-10-12-phase27-5-1-arrow-stream-efb51887/).
- Results: [phase-21-plan.md](../phases/phase-21-plan.md),
  [phase-27-results.md](../phases/phase-27-results.md).
