# Phase 29.2 — coarse structural index (real bytes read)

The current lane reads the whole encoded descriptor for a narrow request.
The coarse index partitions the verbatim authority object into format-native
units and serves a request with real `pread` range reads. `closure` is the
honest physical total (units + persisted index + manifest/index/seed reads); the
control is `observe-batch` cold. Two request shapes are measured:

- **span**: a `--byte-range` over the largest native coarse unit (the index's
  native granularity).
- **reading**: the natural reading request (`--page 1`/`--block 0`/
  `--parquet-cell 0:0`, `--kind text`).

| fmt | shape | source B | control B | units B | index B | closure B | page-SQLite B | closure/control | verdict vs control | closure/SQLite | units/SQLite | verdict vs SQLite | coarse wall µs | coarse RSS kB | matches | exact |
|---|---|---:|---:|---:|---:|---:|---:|---:|:--:|---:|---:|:--:|---:|---:|:--:|:--:|
| pdf | span | 3236057 | 3236824 | 7091 | 30461 | 37552 | 28788 | 86.20x | WIN | 0.77x | 4.06x | LOSS | 11580 | 12188 | yes | yes |
| pdf | reading | 3236057 | 3245913 | 7087 | 30461 | 46545 | n/m | 69.74x | WIN | n/a | n/a | n/m | 11162 | 12652 | yes | yes |
| docx | span | 3284935 | 3285766 | 655411 | 218 | 655629 | 675956 | 5.01x | WIN | 1.03x | 1.03x | WIN | 13092 | 12764 | yes | yes |
| docx | reading | 3284935 | 3290209 | 3291799 | 218 | 3296460 | n/m | 1.00x | LOSS | n/a | n/a | n/m | 25268 | 20084 | yes | yes |
| odt | span | 3284480 | 3285310 | 655407 | 218 | 655625 | 680052 | 5.01x | WIN | 1.04x | 1.04x | WIN | 11522 | 13000 | yes | yes |
| odt | reading | 3284480 | 3289667 | 3291064 | 218 | 3295639 | n/m | 1.00x | LOSS | n/a | n/a | n/m | 19959 | 20184 | yes | yes |
| parquet | span | 4195070 | 4195784 | 1550025 | 201 | 1550226 | 1757300 | 2.71x | WIN | 1.13x | 1.13x | WIN | 13885 | 14736 | yes | yes |
| parquet | reading | 4195070 | 4196116 | 8390140 | 201 | 8390673 | n/m | 0.50x | LOSS | n/a | n/a | n/m | 25997 | 22900 | yes | yes |

## Phase 28.6 historical control (different corpus; same mechanism)

| fmt | 28.6 VOLE closure B | 28.6 source B | 28.6 page-SQLite B | 28.6 floor+dir B |
|---|---:|---:|---:|---:|
| pdf | 36414410 | 36404840 | 135284 | 83697 |
| docx | 2309293 | 2286200 | 28788 | 17400 |
| json | 16778341 | 16777304 | 12404 | 65584 |
| csv | 16778079 | 16777245 | 12404 | 65584 |
| parquet | 16780999 | 16779953 | 450676 | 401271 |

## Verdict

- exactness held: 4/4 formats `materialize --exact == source`.
- **span** requests: the coarse closure is narrower than the whole-descriptor
  control in 4/4 formats, and beats the page-SQLite control in 3/4
  (on source bytes only, ignoring the persistent directory: 4/4).
- **reading** requests: the coarse closure is narrower than the control in only
  1/4 formats — where the format model materializes the whole exact source
  (DOCX/ODT/Parquet), the coarse units are built but not exercised.
  Parquet's reading closure is 2.0x its control (the model materializes the
  whole `DocumentExact` root twice), the largest loss.

A format whose closure is not below the control is a *loss*, reported as such.
