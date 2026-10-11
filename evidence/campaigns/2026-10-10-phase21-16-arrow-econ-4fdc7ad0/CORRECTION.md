# Correction — Phase 26.6: the Arrow corpus was not conformant; re-sealed with a pyarrow lane

- **Superseded by:** [`../2026-10-11-phase21-16-arrow-econ-74965426/`](../2026-10-11-phase21-16-arrow-econ-74965426/)

This campaign's comparator read a **Parquet projection** because the pinned DuckDB
wheel has no Arrow IPC file reader, so the hand-crafted Arrow corpus was never
validated by a real Arrow reader. Phase 26.6 added a hash-pinned `pyarrow` lane and,
in doing so, found that `tools/fixtures/make-arrow.py` emitted **non-conformant**
Arrow IPC:

1. tag-only `Field.type` (Flatbuffers **union**) values were omitted — a bare union
   tag with no value, which the reference reader rejects as an unexpected null;
2. the footer `recordBatches` vector was encoded as a vector of *tables*, not the
   schema's `Block` **structs**;
3. `Block.metaDataLength` omitted the 8-byte continuation + length prefix.

`pyarrow` rejected every hand-crafted file. The generator and VOLE's footer `Block`
reader were corrected to the Arrow wire format, the committed fixtures under
`tools/fixtures/arrow/` were regenerated (their SHA-256 values therefore changed),
and the court was re-sealed with four lanes (VOLE, SQLite, DuckDB, pyarrow). This
campaign's storage method (sum of regular-file bytes, never `du -sb`) and its VOLE
exactness result stand; its fixture hashes and its single-comparator framing do not.
