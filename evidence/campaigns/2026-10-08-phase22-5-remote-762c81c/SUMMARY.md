# Phase 22.5 — remote selective materialization (remote-selective-v1, **MODEL**)

**This is an explicitly-labelled MODEL of remote selective reads, not a real S3 benchmark.**
Real object storage is unavailable in the pinned lane. The **bytes** a selective remote
read must transfer and the **request counts** are REAL local measurements (VOLE instrumented
`*_bytes_read`; SQLite `pread64` page offsets under `strace` with `mmap_size=0`, captured by a
real loopback HTTP `Range`/`206` server + coalescing client). The **latency** numbers apply the
stated cost model; the **geometry** (class bytes -> ranges) is an upper-bound model where the
exact node->offset map is not exposed. **A VOLE win is claimed only where the bytes ratio is
<= 0.5x and the modelled p95 is competitive, in a named region, under model remote-selective-v1 with the
constants below (date 2026-10-08).**

## Question

Can bounded *local* selective materialization (Phase 20.2) extend to **remote** storage? A
remote field fetches a compact directory and only the necessary immutable segments via
byte-range reads, with a range planner. The control is the tuned SQLite envelope given a
comparably optimised remote **page cache + source-range interface**.

## Model and constants (date 2026-10-08)

```text
T = N_requests * RTT + B_transferred / BW + C_decode  (sequential, P=1)
```

| profile | role | RTT | BW |
|---|---|---:|---:|
| cloud-same-region | PRIMARY | 25 ms | 100 MB/s |
| edge-fast | sensitivity | 5 ms | 1000 MB/s |
| wan-slow | sensitivity | 80 ms | 25 MB/s |

- `C_decode` = the measured **local** decode wall of the same observation (VOLE `stats.wall_micros`;
  SQLite the `sqlite3` query wall).  Requests are modelled sequentially (P=1, no pipelining).
- Coalescing: `gap=0` (merge only touching/overlapping ranges) — the byte-minimal plan.  A range
  server access log is cross-checked against the client's request count.

## Pre-registered observations

- **O1 text** — pdf `--page 1 --kind text`; docx/epub `--block 0 --kind text`.
- **O2 bytes** — `--byte-range 0..64 --kind exact`.
- **O3 resource** — docx/epub `--resource 0 --kind metadata`; pdf `--metadata --kind metadata`
  (the PDF contract has **no** resource observation; metadata substitutes, recorded).

## Method

- 12-document `real100-v1` subset (4 pdf / 4 docx / 4 epub), identical IDs to Phase 22.2.
- Per document: VOLE `field-build --profile runtime --packed` once; SQLite `converters.py` config
  `full` through C5 once.
- Per observation: cold VOLE `observe --no-cache` -> instrumented per-class bytes -> ranges in a
  flat store image -> real loopback range fetch; SQLite answer SQL (depth C0) under `strace`
  `pread64` with `PRAGMA mmap_size=0` -> page ranges -> real loopback range fetch.
- Controls: whole store image, whole `.db`.

## Results — VOLE selective vs SQLite page-level (per region)

### Pooled over documents
| region | n | VOLE B (med) | SQL B (med) | bytes_ratio (med) | VOLE sel. | SQL sel. | p95 lat VOLE (ms) | p95 lat SQL (ms) | lat_ratio (med) | W/T/L | verdict |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| all | 33 | 264466 | 20480 | 3.154 | 0.888 | 0.062 | 88.47 | 101.73 | 0.667 | 0/13/20 | loss |

### By observation
| region | n | VOLE B (med) | SQL B (med) | bytes_ratio (med) | VOLE sel. | SQL sel. | p95 lat VOLE (ms) | p95 lat SQL (ms) | lat_ratio (med) | W/T/L | verdict |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| obs=bytes | 12 | 208889 | 219136 | 0.950 | 0.879 | 0.379 | 41.15 | 90.94 | 0.349 | 0/12/0 | unresolved |
| obs=metadata | 4 | 708608 | 8192 | 86.500 | 0.797 | 0.008 | 41.96 | 26.73 | 1.258 | 0/0/4 | loss |
| obs=resource | 5 | 109995 | 12288 | 8.951 | 0.909 | 0.021 | 76.62 | 51.70 | 0.999 | 0/0/5 | loss |
| obs=text | 12 | 133693 | 20480 | 6.528 | 0.912 | 0.042 | 98.48 | 101.78 | 0.725 | 0/1/11 | loss |

### By observation x format
| region | n | VOLE B (med) | SQL B (med) | bytes_ratio (med) | VOLE sel. | SQL sel. | p95 lat VOLE (ms) | p95 lat SQL (ms) | lat_ratio (med) | W/T/L | verdict |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| obs=bytes/docx | 4 | 105576 | 114688 | 0.909 | 0.836 | 0.331 | 35.03 | 85.46 | 0.338 | 0/4/0 | unresolved |
| obs=bytes/epub | 4 | 98607 | 106496 | 0.926 | 0.884 | 0.337 | 37.93 | 88.04 | 0.336 | 0/4/0 | unresolved |
| obs=bytes/pdf | 4 | 708608 | 718848 | 0.982 | 0.797 | 0.703 | 41.93 | 91.85 | 0.397 | 0/4/0 | unresolved |
| obs=metadata/pdf | 4 | 708608 | 8192 | 86.500 | 0.797 | 0.008 | 41.96 | 26.73 | 1.258 | 0/0/4 | loss |
| obs=resource/docx | 1 | 953868 | 12288 | 77.626 | 0.798 | 0.008 | 79.31 | 51.59 | 1.537 | 0/0/1 | loss |
| obs=resource/epub | 4 | 101774 | 12288 | 8.282 | 0.912 | 0.029 | 63.72 | 51.71 | 0.996 | 0/0/4 | loss |
| obs=text/docx | 4 | 108576 | 22528 | 5.057 | 0.878 | 0.068 | 76.73 | 101.79 | 0.721 | 0/0/4 | loss |
| obs=text/epub | 4 | 102084 | 22528 | 4.603 | 0.915 | 0.052 | 64.83 | 101.75 | 0.661 | 0/0/4 | loss |
| obs=text/pdf | 4 | 296658 | 20480 | 14.485 | 0.816 | 0.021 | 102.97 | 76.75 | 1.164 | 0/1/3 | loss |

### By observation x size class
| region | n | VOLE B (med) | SQL B (med) | bytes_ratio (med) | VOLE sel. | SQL sel. | p95 lat VOLE (ms) | p95 lat SQL (ms) | lat_ratio (med) | W/T/L | verdict |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| obs=bytes/1-10MiB | 3 | 1238051 | 1249280 | 0.991 | 0.909 | 0.653 | 42.33 | 92.20 | 0.446 | 0/3/0 | unresolved |
| obs=bytes/100KiB-1MiB | 5 | 264466 | 274432 | 0.964 | 0.888 | 0.409 | 34.94 | 85.35 | 0.355 | 0/5/0 | unresolved |
| obs=bytes/<100KiB | 4 | 47239 | 55296 | 0.848 | 0.838 | 0.286 | 26.09 | 78.47 | 0.332 | 0/4/0 | unresolved |
| obs=metadata/1-10MiB | 2 | 1286418 | 8192 | 157.033 | 0.713 | 0.004 | 42.37 | 26.70 | 1.522 | 0/0/2 | loss |
| obs=metadata/100KiB-1MiB | 2 | 287928 | 8192 | 35.147 | 0.797 | 0.016 | 28.76 | 26.72 | 1.068 | 0/0/2 | loss |
| obs=resource/1-10MiB | 1 | 1243583 | 12288 | 101.203 | 0.987 | 0.006 | 65.86 | 51.60 | 1.276 | 0/0/1 | loss |
| obs=resource/100KiB-1MiB | 2 | 531932 | 12288 | 43.289 | 0.857 | 0.023 | 77.92 | 51.64 | 1.268 | 0/0/2 | loss |
| obs=resource/<100KiB | 2 | 66156 | 12288 | 5.384 | 0.877 | 0.054 | 51.33 | 51.71 | 0.987 | 0/0/2 | loss |
| obs=text/1-10MiB | 3 | 1114599 | 20480 | 50.614 | 0.917 | 0.012 | 103.53 | 99.25 | 1.217 | 0/1/2 | loss |
| obs=text/100KiB-1MiB | 5 | 273239 | 20480 | 13.342 | 0.918 | 0.042 | 84.70 | 96.71 | 0.836 | 0/0/5 | loss |
| obs=text/<100KiB | 4 | 49570 | 22528 | 2.176 | 0.882 | 0.120 | 52.39 | 101.80 | 0.589 | 0/0/4 | loss |

### Latency sensitivity — median VOLE/SQLite modelled latency ratio

| obs | cloud-same-region | edge-fast | wan-slow |
|---|---:|---:|---:|
| bytes | 0.349 | 0.335 | 0.355 |
| metadata | 1.258 | 1.089 | 1.340 |
| resource | 0.999 | 0.919 | 1.018 |
| text | 0.725 | 0.986 | 0.678 |

## Whole-object vs selective (bytes moved)

How selective is each lane relative to its own whole object?  `store/select` = whole store /
VOLE selective; `db/page` = whole `.db` / SQLite page-level.  VOLE's selective read is often a
**large fraction of the whole store** (it must read the descriptor closure), while SQLite's
page-level read is a **tiny fraction of the whole `.db`** (a few 4 KiB pages) for the same query.

| obs | n | median store/select | median db/page | median VOLE/SQL |
|---|---:|---:|---:|---:|
| bytes | 12 | 1.14 | 2.7 | 0.950 |
| metadata | 4 | 1.28 | 145.5 | 86.500 |
| resource | 5 | 1.10 | 48.3 | 8.951 |
| text | 12 | 1.10 | 23.9 | 6.528 |

### Conservative control — whole-`.db` download (favours VOLE)

The gate uses the **best** SQLite interface (page-level).  For transparency, against the
conservative whole-`.db` download the modelled VOLE selective read transfers **fewer** bytes
than SQLite would:

| obs | n | median VOLE-selective / whole-`.db` | median whole-store / whole-`.db` |
|---|---:|---:|---:|
| bytes | 12 | 0.360 | 0.468 |
| metadata | 4 | 0.696 | 0.769 |
| resource | 5 | 0.336 | 0.367 |
| text | 12 | 0.331 | 0.468 |

## Notable exceptions

Rows where the modelled VOLE selective read transfers **fewer** bytes than the SQLite
page-level interface (13 of 33 answered):

| id | fmt | obs | VOLE B | SQL B | bytes_ratio | lat_ratio | note |
|---|---|---|---:|---:|---:|---:|---|
| nist-pdf-0017 | pdf | text | 11038 | 20480 | 0.54 | 1.36 | descriptor partial-read closure (observation index); modelled latency not competitive here |
| nist-docx-0008 | docx | bytes | 29579 | 40960 | 0.72 | 0.33 | exact byte-range: SQLite reads the whole source-blob overflow chain; VOLE modelled-latency advantage |
| nist-epub-0008 | epub | bytes | 36637 | 45056 | 0.81 | 0.33 | exact byte-range: SQLite reads the whole source-blob overflow chain; VOLE modelled-latency advantage |
| nist-docx-0005 | docx | bytes | 57841 | 65536 | 0.88 | 0.33 | exact byte-range: SQLite reads the whole source-blob overflow chain; VOLE modelled-latency advantage |
| nist-epub-0003 | epub | bytes | 90551 | 98304 | 0.92 | 0.33 | exact byte-range: SQLite reads the whole source-blob overflow chain; VOLE modelled-latency advantage |
| nist-epub-0006 | epub | bytes | 106663 | 114688 | 0.93 | 0.34 | exact byte-range: SQLite reads the whole source-blob overflow chain; VOLE modelled-latency advantage |
| nist-docx-0009 | docx | bytes | 153312 | 163840 | 0.94 | 0.34 | exact byte-range: SQLite reads the whole source-blob overflow chain; VOLE modelled-latency advantage |
| nist-pdf-0002 | pdf | bytes | 264466 | 274432 | 0.96 | 0.36 | exact byte-range: SQLite reads the whole source-blob overflow chain; VOLE modelled-latency advantage |
| nist-pdf-0016 | pdf | bytes | 311389 | 319488 | 0.97 | 0.36 | exact byte-range: SQLite reads the whole source-blob overflow chain; VOLE modelled-latency advantage |
| nist-pdf-0004 | pdf | bytes | 1105826 | 1118208 | 0.99 | 0.43 | exact byte-range: SQLite reads the whole source-blob overflow chain; VOLE modelled-latency advantage |
| nist-docx-0014 | docx | bytes | 949219 | 958464 | 0.99 | 0.42 | exact byte-range: SQLite reads the whole source-blob overflow chain; VOLE modelled-latency advantage |
| nist-epub-0009 | epub | bytes | 1238051 | 1249280 | 0.99 | 0.45 | exact byte-range: SQLite reads the whole source-blob overflow chain; VOLE modelled-latency advantage |
| nist-pdf-0017 | pdf | bytes | 1467009 | 1478656 | 0.99 | 0.46 | exact byte-range: SQLite reads the whole source-blob overflow chain; VOLE modelled-latency advantage |

## Coalescing effect (range planner)

The planner merges touching/overlapping ranges on the same object (`gap=0`).  It matters most
for SQLite's page-level plan, where dozens of 4 KiB page reads collapse into a few requests.

| lane | obs | sum ranges before | sum requests after | reduction |
|---|---|---:|---:|---:|
| VOLE | bytes | 24 | 12 | 2.00 |
| VOLE | metadata | 8 | 4 | 2.00 |
| VOLE | resource | 20 | 10 | 2.00 |
| VOLE | text | 48 | 29 | 1.66 |
| VOLE | **total** | **100** | **55** | **1.82** |
| SQLite | bytes | 1459 | 36 | 40.53 |
| SQLite | metadata | 12 | 4 | 3.00 |
| SQLite | resource | 20 | 10 | 2.00 |
| SQLite | text | 76 | 40 | 1.90 |
| SQLite | **total** | **1567** | **90** | **17.41** |

## Integrity

- `materialize --exact`: **12/12** byte-exact (length + SHA-256; `cmp` equal **12/12**).
- Selective range GETs verified (HTTP 206 + exact length + SHA-256 vs source slice):
  **VOLE 55/55**, **SQLite 96/96**.
- Range-server access-log cross-check (client requests vs server log lines): **223 vs 223**.

## Verdict

Rule: a **VOLE win** requires the median bytes-transferred ratio `VOLE/SQLite <= 0.5x` **and**
the median modelled latency ratio `<= 1.0` at the PRIMARY profile.  `> 1.0` bytes is a **loss**;
anything between is **unresolved**.  All under model remote-selective-v1 with constants above (date 2026-10-08).

- **Verdict: LOSS[bytes_ratio=3.154][lat_ratio=0.667][wins=0][losses=17][unresolved=7]**
- VOLE-win regions: none
- VOLE-loss regions: obs=text, obs=metadata, obs=resource, obs=text/pdf, obs=metadata/pdf, obs=text/docx, obs=resource/docx, obs=text/epub, obs=resource/epub, obs=text/100KiB-1MiB, obs=metadata/100KiB-1MiB, obs=text/1-10MiB, obs=metadata/1-10MiB, obs=text/<100KiB, obs=resource/<100KiB, obs=resource/100KiB-1MiB, obs=resource/1-10MiB
- Unresolved regions: obs=bytes, obs=bytes/pdf, obs=bytes/docx, obs=bytes/epub, obs=bytes/100KiB-1MiB, obs=bytes/1-10MiB, obs=bytes/<100KiB

**How to read this verdict.**  VOLE does **not** achieve a *major* bytes reduction against the
best SQLite interface on this corpus: the median VOLE/SQLite bytes ratio is **3.154** (>1 = VOLE
transfers more).  The modelled latency ratio is **0.667** because VOLE issues very few requests
while SQLite issues several, but the gate's first condition (a major byte reduction) fails, so this
is a **LOSS** for the remote-selective byte court.

Two structural facts explain it and are worth naming:

1. SQLite's page-level remote read is **extremely selective** for narrow queries (a handful of 4 KiB
   pages, `db/page` in the tens-to-hundreds), because the tuned `full` envelope materializes the
   queried columns into pages.  VOLE's cold remote read must fetch the **descriptor closure**, which
   for most of these documents is roughly the whole encoded document.
2. The one region that is **competitive** is `obs=bytes` (an exact 0..64 byte-range): there the SQLite
   control must read the whole `source_blob` overflow chain, so VOLE bytes are at parity
   (`bytes_ratio` 0.950) with a large modelled latency advantage (`lat_ratio` 0.349).  It is recorded as
   **unresolved**, not a win, because the byte reduction is not *major*.

Declined observations (excluded from ratios): **3** of 36 (VOLE `resource` on documents that have
no resources is an unsupported capability, rc=6).

Against the **conservative** whole-`.db` control VOLE would transfer fewer bytes (whole-store /
whole-`.db` < 1 for most documents), but that control is exactly the one the brief says not to
force if a realistic alternative exists.  A page-level remote interface exists, so it is used.

## What this is NOT / does not prove

- **It is a MODEL.** No S3, no network egress, no real remote latency was measured. The loopback
  HTTP server measures request COUNT and bytes served; the latency figures are the stated cost
  model, not observations.
- VOLE's class bytes are exact (instrumented), but their **placement** inside each namespace
  region is an upper-bound model: the request count is a **lower bound** on the true scattered read.
- The SQLite page-level interface reads only the pages the query touches **because it may retain**
  the extracted rows; the whole-`.db` download remains the conservative control and is reported.
- No production code changed and **nothing ships**; this is a measurement court on the shipping
  binary.

