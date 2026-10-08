# Phase 16.2 — full `real100-v1` packed-store storage court

> **Corrected 2026-10-08 (Phase 16.6, ADR-0049).** Every ratio below used
> `du -sb`, which counts 4096 B per directory inode, so the one-file-per-node `fs`
> store is inflated (~52 %) while the packed store (0.8 %) and the single `.db`
> (0 %) are not. On file-bytes-only accounting the same 95 common-success
> documents give fs/SQLite **0.906×**, packed/SQLite **0.914×**, packed/fs
> **1.009×** — both VOLE backends are at/below SQLite on file bytes, and the
> "closes the gap" framing is mis-stated (there was no byte gap). Original numbers
> kept for the record; see
> [`../2026-10-08-phase16-storage-correction-2978e1d/CORRECTION.md`](../2026-10-08-phase16-storage-correction-2978e1d/CORRECTION.md).

Documents in population: **100**. Profile release, `doc-baseline` (6 GiB, cpus 8). Question: on the **common-success** population, does the `--packed` backend close the persistent-storage gap against the A1 SQLite db? Ratios are VOLE/SQLite (and pack/fs); < 1 means VOLE is smaller. The established baseline (`2026-10-07-real100-release-baseline-866f489`) reported fs/SQLite = **1.377×** on 95 common-success documents.

## Population and success

| stage | succeeded |
|---|---:|
| encode (`venc_rc==0`) | 97 / 100 |
| VOLE fs `field-ingest` | 97 / 100 |
| VOLE packed `field-ingest --packed` | 97 / 100 |
| A1 SQLite build | 98 / 100 |
| field id identical (fs == packed) | 97 / 97 |

* common success, fs + A1 (the 1.377× population shape): **95**
* common success, fs + packed + A1 (**the head-to-head**): **95**

Exit-code histogram (`0` success, `124` timeout, `137` SIGKILL/OOM, `-1` not run because encode failed):

| stage | rc -> count |
|---|---|
| venc_rc | `0`×97, `124`×2, `137`×1 |
| ving_fs_rc | `-1`×3, `0`×97 |
| ving_pack_rc | `-1`×3, `0`×97 |
| a1_build_rc | `0`×98, `1`×2 |

## Whole-population totals (successful footprints only)

| substrate | docs | bytes |
|---|---:|---:|
| VOLE fs store | 97 | 2,667,668,262 |
| VOLE packed store | 97 | 1,797,905,486 |
| A1 SQLite db | 98 | 2,891,784,192 |

_Whole-population totals are over different document sets per substrate and must never be read as a head-to-head._

## Head-to-head — common success (fs + packed + A1)

Population: **95** documents.

#### Overall

| comparison | VOLE (numerator) B | SQLite / fs (denominator) B | sum ratio | median per-doc ratio |
|---|---:|---:|---:|---:|
| VOLE fs vs SQLite | 2,619,707,424 | 1,902,919,680 | 1.377× | 1.342× |
| VOLE packed vs SQLite | 1,752,688,904 | 1,902,919,680 | 0.921× | 0.846× |
| VOLE packed vs VOLE fs | 1,752,688,904 | 2,619,707,424 | 0.669× | 0.486× |

### By format

#### pdf (57 docs)

| comparison | VOLE (numerator) B | SQLite / fs (denominator) B | sum ratio | median per-doc ratio |
|---|---:|---:|---:|---:|
| VOLE fs vs SQLite | 2,312,656,470 | 1,631,068,160 | 1.418× | 1.767× |
| VOLE packed vs SQLite | 1,483,577,606 | 1,631,068,160 | 0.910× | 0.861× |
| VOLE packed vs VOLE fs | 1,483,577,606 | 2,312,656,470 | 0.642× | 0.454× |

#### docx (15 docs)

| comparison | VOLE (numerator) B | SQLite / fs (denominator) B | sum ratio | median per-doc ratio |
|---|---:|---:|---:|---:|
| VOLE fs vs SQLite | 22,138,577 | 16,900,096 | 1.310× | 1.213× |
| VOLE packed vs SQLite | 14,855,621 | 16,900,096 | 0.879× | 0.476× |
| VOLE packed vs VOLE fs | 14,855,621 | 22,138,577 | 0.671× | 0.286× |

#### epub (23 docs)

| comparison | VOLE (numerator) B | SQLite / fs (denominator) B | sum ratio | median per-doc ratio |
|---|---:|---:|---:|---:|
| VOLE fs vs SQLite | 284,912,377 | 254,951,424 | 1.118× | 0.961× |
| VOLE packed vs SQLite | 254,255,677 | 254,951,424 | 0.997× | 0.710× |
| VOLE packed vs VOLE fs | 254,255,677 | 284,912,377 | 0.892× | 0.812× |


## Reconciliation — common success (fs + A1 only)

Population: **95** documents (the shape the 1.377× baseline used).

#### Overall

| comparison | VOLE fs B | SQLite B | sum ratio | median per-doc ratio |
|---|---:|---:|---:|---:|
| VOLE fs vs SQLite | 2,619,707,424 | 1,902,919,680 | 1.377× | 1.342× |

#### pdf (57 docs)

| comparison | VOLE fs B | SQLite B | sum ratio | median per-doc ratio |
|---|---:|---:|---:|---:|
| VOLE fs vs SQLite | 2,312,656,470 | 1,631,068,160 | 1.418× | 1.767× |

#### docx (15 docs)

| comparison | VOLE fs B | SQLite B | sum ratio | median per-doc ratio |
|---|---:|---:|---:|---:|
| VOLE fs vs SQLite | 22,138,577 | 16,900,096 | 1.310× | 1.213× |

#### epub (23 docs)

| comparison | VOLE fs B | SQLite B | sum ratio | median per-doc ratio |
|---|---:|---:|---:|---:|
| VOLE fs vs SQLite | 284,912,377 | 254,951,424 | 1.118× | 0.961× |


## Verdict

**Packed CLOSES the gap**: packed/SQLite = **0.921×** (sum, <= 1.0×), median per-doc 0.846× — the packed store is smaller than the SQLite db on the 95 common-success documents (margin +0.079× below 1.0). For the same population fs/SQLite = 1.377×.

## Failures (never hidden)

| id | fmt | size class | bytes | failing rc | not run |
|---|---|---|---:|---|---|
| nasa-epub-0010 | epub | 10-50MiB | 45,000,233 | a1_build_rc=1 | — |
| nasa-pdf-0001 | pdf | >100MiB | 408,854,600 | venc_rc=137 | ving_fs_rc, ving_pack_rc |
| nasa-pdf-0002 | pdf | >100MiB | 308,803,168 | venc_rc=124 | ving_fs_rc, ving_pack_rc |
| nasa-pdf-0003 | pdf | >100MiB | 217,666,672 | venc_rc=124 | ving_fs_rc, ving_pack_rc |
| nist-epub-0008 | epub | <100KiB | 35,841 | a1_build_rc=1 | — |

