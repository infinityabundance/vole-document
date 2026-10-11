# Correction — Phase 26.6: the Arrow fixtures were regenerated (non-conformant → conformant)

- **Fixture source changed:** `tools/fixtures/make-arrow.py` (fixtures under
  `tools/fixtures/arrow/` were regenerated).
- **Related re-seal:** [`../2026-10-11-phase21-16-arrow-econ-74965426/`](../2026-10-11-phase21-16-arrow-econ-74965426/)

Phase 26.6 added a `pyarrow` lane to the Arrow economic court and, doing so, found
that `tools/fixtures/make-arrow.py` emitted **non-conformant** Arrow IPC (missing
`Field.type` union values; the footer `recordBatches` vector encoded as a vector of
*tables* rather than `Block` *structs*; `Block.metaDataLength` missing the 8-byte
message prefix). `pyarrow` rejects all of the previous hand-crafted files.

The generator and VOLE's footer `Block` reader were corrected to the Arrow wire
format, so the committed fixtures were **regenerated** and their SHA-256 values
changed. This campaign's recorded `src_sha256` values in `raw/results.json`
therefore no longer match the on-disk fixtures. Every *result* still holds — VOLE
detects, decodes, declines-typed, and byte-exactly materializes the same logical
fixtures — but the fixture digests must be read from the current
`tools/fixtures/arrow/` (or a fresh run), not from this sealed receipt.
