# Phase 30.4 — unit-addressed coarse *reading* (real bytes read)

Phase 29.2 found the coarse structural index narrowed the natural
*reading* request (page-1 / block-0 / parquet-cell text) for PDF only:
the DOCX/ODT/Parquet format-model nodes materialized the whole exact
`DocumentExact` source, so their units were built but inert. Phase 30.4
serves the reading request from the coarse member spans (the ZIP member /
column chunk) plus the ZIP central directory, so its closure is the unit +
directory, not the whole document. Corpus and reconstruction are unchanged
from 29.2, so those numbers are the historical control.

`closure` = real `pread` bytes (units + directory + manifest/index/seed);
`units B` = the coarse-lane physical bytes; `dir B` = persisted index file;
control = `observe-batch` cold whole-descriptor read. `page-SQLite` is the
Phase-28.6 page control over the request's required regions.

| fmt | shape | source B | control B | units B | dir B | closure B | page-SQLite B | closure/control | vs control | closure/SQLite | matches | exact |
|---|---|---:|---:|---:|---:|---:|---:|---:|:--:|---:|:--:|:--:|
| pdf | span | 3236057 | 3236824 | 7091 | 30461 | 37552 | 28788 | 86.20x | WIN | 0.77x | yes | yes |
| pdf | reading | 3236057 | 3245913 | 7087 | 30461 | 46545 | 24692 | 69.74x | WIN | 0.53x | yes | yes |
| docx | span | 3284935 | 3285766 | 655411 | 218 | 655629 | 675956 | 5.01x | WIN | 1.03x | yes | yes |
| docx | reading | 3284935 | 3290209 | 8422 | 218 | 13003 | 20596 | 253.03x | WIN | 1.58x | yes | yes |
| odt | span | 3284480 | 3285310 | 655407 | 218 | 655625 | 680052 | 5.01x | WIN | 1.04x | yes | yes |
| odt | reading | 3284480 | 3289667 | 7961 | 218 | 12456 | 20596 | 264.10x | WIN | 1.65x | yes | yes |
| parquet | span | 4195070 | 4195784 | 1550025 | 201 | 1550226 | 1757300 | 2.71x | WIN | 1.13x | yes | yes |
| parquet | reading | 4195070 | 4196116 | 8390140 | 201 | 8390673 | 4685956 | 0.50x | LOSS | 0.56x | yes | yes |

## Phase 29.2 historical control (same corpus bytes; reading request)

| fmt | 29.2 control B | 29.2 closure B | 29.2 verdict |
|---|---:|---:|:--:|
| pdf | 3245913 | 46545 | WIN |
| docx | 3290209 | 3296460 | LOSS |
| odt | 3289667 | 3295639 | LOSS |
| parquet | 4196116 | 8390673 | LOSS |

## Directory overhead

The persisted VCI1 index (the directory the read pays) is 30461 B for PDF
(~34 B/object unit), 218 B for DOCX/ODT (8 members + directory) and 201 B
for Parquet. It is charged once per observation in `closure`.

## Remaining loss

- **parquet** reading: still 2.0x the control. The `ParquetCell` projection
  slices the containing column chunk out of a whole-source buffer
  (`parquet_source()` materializes the `DocumentExact` root) and the
  `ParquetModel` node reads the whole source; making the cell unit-addressed
  needs a range source threaded through the Parquet value decoders, which this
  subphase does not do. A footer-only model would not flip it on its own
  (the whole-source read in `parquet_source()` remains).

## Verdict

- exactness held: 4/4 formats `materialize --exact == source`.
- **reading** requests: the coarse closure is now narrower than the
  whole-descriptor control in 3/4 formats (Phase 29.2: 1/4), and beats the
  page-SQLite control in 2/4 measured.
- **span** requests remain narrower than the control in 4/4 formats.

A format whose closure is not below the control is a *loss*, reported as such.
