# phase16 zlib-rs inflate backend — end-to-end impact

OLD = `miniz_oxide` (pre-16.1 inflate), NEW = `zlib-rs` (shipped backend). Same documents, same capped `doc-baseline` lane, same `--features docx,epub` build; one process per operation. Wall ms is the median over the court's reps. `encode` does not inflate and is the control; the delta is `field-ingest`.

| id | fmt | class | bytes | arm | encode ms (med) | encode RSS MiB | ingest ms (med) | ingest RSS MiB |
|---|---|---|---:|---|---:|---:|---:|---:|
| nasa-epub-0006 | epub | 10-50MiB | 10867010 | old | 200 | 200.0 | 170 | 96.7 |
| nasa-epub-0006 | epub | 10-50MiB | 10867010 | new | 200 | 200.2 | 180 | 98.0 |
| nasa-pdf-0029 | pdf | 50-100MiB | 54750188 | old | 2310 | 924.7 | 1030 | 390.7 |
| nasa-pdf-0029 | pdf | 50-100MiB | 54750188 | new | 2300 | 925.2 | 920 | 390.6 |
| nist-docx-0013 | docx | <100KiB | 25400 | old | 0 | 7.1 | 0 | 3.2 |
| nist-docx-0013 | docx | <100KiB | 25400 | new | 0 | 6.9 | 0 | 3.2 |

## Aggregate (median per operation)

| arm | docs | rc-ok | encode total ms | ingest total ms | total ms | peak encode RSS MiB | peak ingest RSS MiB |
|---|---:|---:|---:|---:|---:|---:|---:|
| old | 3 | 3/3 | 2510 | 1200 | 3710 | 924.7 | 390.7 |
| new | 3 | 3/3 | 2500 | 1100 | 3600 | 925.2 | 390.6 |

| quantity | old | new | new/old |
|---|---:|---:|---:|
| encode total ms | 2510 | 2500 | 0.996x |
| field-ingest total ms | 1200 | 1100 | 0.917x |
| encode+ingest total ms | 3710 | 3600 | 0.970x |
| peak ingest RSS MiB | 390.7 | 390.6 | 1.000x |

Field-id agreement (old == new, per document): **yes**
Exactness gate (`materialize --exact` == manifest on every row): **PASS**

Caveat (labelled, not hidden): `encode` is backend-independent, so its new/old ratio is a measured NOISE FLOOR — an ingest delta smaller than that floor is not resolved by this court.

Phase-15.5 microbench context: inflate alone was 1.58x GB/s (miniz_oxide scalar -> zlib-rs) and byte-identical. The end-to-end factor above is smaller because inflate is only one part of ingest; it is reported, not asserted.
