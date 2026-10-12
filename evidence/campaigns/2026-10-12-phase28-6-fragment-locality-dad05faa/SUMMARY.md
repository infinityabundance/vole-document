# Phase 28.6 results — fragment locality (decision Experiment 2)

Branch `phase28` @ `dad05faa`. **Measurement only**: no wire format, no planner change, no decoder change. This is the decision gate for 28.8+ (physical fragmentation).

**Historical control (Phase 22.5):** VOLE's selective remote read transferred a
median **3.15x** more bytes than page-level SQLite (0 wins / 17 losses / 7
unresolved). This court re-measures the same question *locally and exactly*
for five large documents, with VOLE's own instrumented `bytes_read` as the
closure and a real page-level SQLite control.

## Request, and the ideal floor (bytes a directory would have to verify)

| fmt | source B | narrow request | required source B | required ranges |
|---|---:|---|---:|---:|
| pdf | 36404840 | `--page 1 --kind text` | 61856 | 11 |
| docx | 2286200 | `--block 0 --kind text` | 15192 | 2 |
| json | 16777304 | `--json-pointer /items/19999 --kind text` | 72 | 1 |
| csv | 16777245 | `--csv-range 10:0:12:3 --kind text` | 54 | 12 |
| parquet | 16779953 | `--parquet-cell 0:0 --kind text` | 400023 | 1 |

## Bytes read per lane (the headline table)

| fmt | current VOLE `bytes_read` | VOLE store-reads (strace) | ideal floor | best fixed size / B | best fragment floor+dir | page SQLite: pages | page SQLite: blob | VOLE / floor | VOLE / SQL(pages) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| pdf | 36,414,410 | 36,425,406 | 61,856 | 81,512 | 83,697 (native) | 135,284 (29 pages) | 359,911,700 | 435.1x | 269.2x |
| docx | 2,309,293 | 2,311,765 | 15,192 | 21,112 | 17,400 (native) | 28,788 (7 pages) | 4,165,764 | 132.7x | 80.2x |
| json | 16,778,341 | 16,780,605 | 72 | 4,096 | 65,584 (fixed_16384) | 12,404 (3 pages) | 8,308 | 255.8x | 1352.7x |
| csv | 16,778,079 | 16,780,215 | 54 | 4,096 | 65,584 (fixed_16384) | 12,404 (3 pages) | 53,540 | 255.8x | 1352.6x |
| parquet | 16,780,999 | 16,783,263 | 400,023 | 401,408 | 401,271 (native_chunk) | 450,676 (110 pages) | 16,523,380 | 41.8x | 37.2x |

## Cost of the VOLE lane (single cold observation; no network transport)

There is no remote transport in this lane, so p95/p99 latency is not
measured and is not claimed: the request is a local file read, so the
headline is BYTES. Wall time is one cold `observe-batch` (cache removed);
peak RSS is that process's high-water mark.

| fmt | VOLE wall ms | VOLE peak RSS KiB | bytes returned |
|---|---:|---:|---:|
| pdf | 110.7 | 146,336 | 193 |
| docx | 55.0 | 14,144 | 28 |
| json | 214.1 | 264,516 | 72 |
| csv | 56.0 | 69,716 | 844 |
| parquet | 61.5 | 69,688 | 1 |

## Fragment schemes per format (floor = required fragments; dir = 48 B/entry)

| fmt | scheme | fragments | required fragments | floor B | directory B | floor+dir B |
|---|---|---:|---:|---:|---:|---:|
| csv | fixed_16384 | 1025 | 1 | 16,384 | 49,200 | 65,584 |
| csv | fixed_65536 | 257 | 1 | 65,536 | 12,336 | 77,872 |
| csv | fixed_4096 | 4097 | 1 | 4,096 | 196,656 | 200,752 |
| csv | fixed_262144 | 65 | 1 | 262,144 | 3,120 | 265,264 |
| csv | native | 490415 | 3 | 66 | 23,539,920 | 23,539,986 |
| docx | native | 46 | 2 | 15,192 | 2,208 | 17,400 |
| docx | fixed_16384 | 140 | 2 | 25,208 | 6,720 | 31,928 |
| docx | fixed_4096 | 559 | 6 | 21,112 | 26,832 | 47,944 |
| docx | fixed_65536 | 35 | 2 | 123,512 | 1,680 | 125,192 |
| docx | fixed_262144 | 9 | 2 | 451,192 | 432 | 451,624 |
| json | fixed_16384 | 1025 | 1 | 16,384 | 49,200 | 65,584 |
| json | fixed_65536 | 257 | 1 | 65,536 | 12,336 | 77,872 |
| json | fixed_4096 | 4097 | 1 | 4,096 | 196,656 | 200,752 |
| json | fixed_262144 | 65 | 1 | 262,144 | 3,120 | 265,264 |
| json | native | 11 | 1 | 16,777,228 | 528 | 16,777,756 |
| parquet | native_chunk | 26 | 1 | 400,023 | 1,248 | 401,271 |
| parquet | fixed_16384 | 1025 | 25 | 409,600 | 49,200 | 458,800 |
| parquet | fixed_65536 | 257 | 7 | 458,752 | 12,336 | 471,088 |
| parquet | fixed_262144 | 65 | 2 | 524,288 | 3,120 | 527,408 |
| parquet | fixed_4096 | 4097 | 98 | 401,408 | 196,656 | 598,064 |
| parquet | native_rg | 10 | 1 | 2,350,071 | 480 | 2,350,551 |
| pdf | native | 455 | 11 | 61,857 | 21,840 | 83,697 |
| pdf | fixed_16384 | 2222 | 8 | 130,664 | 106,656 | 237,320 |
| pdf | fixed_65536 | 556 | 4 | 228,968 | 26,688 | 255,656 |
| pdf | fixed_262144 | 139 | 2 | 491,112 | 6,672 | 497,784 |
| pdf | fixed_4096 | 8888 | 20 | 81,512 | 426,624 | 508,136 |

## Exactness

`materialize --exact == source` (length + SHA-256 + `cmp`): **5/5**

## Historical control (Phase 22.5, unchanged)

Phase 22.5 measured VOLE selective remote reads against a page-level SQLite
control (33 observations, 12 documents) and recorded a **model-scoped loss**:

| region | n | VOLE B | SQLite B | bytes ratio | verdict |
|---|---:|---:|---:|---:|---|
| all | 33 | 264,466 | 20,480 | **3.154** | loss |
| obs=text | 12 | 133,693 | 20,480 | 6.53 | loss |
| obs=bytes | 12 | 208,889 | 219,136 | 0.95 | unresolved |
| obs=metadata (pdf) | 4 | 708,608 | 8,192 | 86.5 | loss |
| obs=resource (docx/epub) | 5 | 109,995 | 12,288 | 8.95 | loss |

Source: `docs/phases/phase-22-5-results.md`,
`evidence/campaigns/2026-10-08-phase22-5-remote-*`. The present court re-asks
that question locally with the same VOLE closure definition and a real
page-level SQLite control.

## Limits and control notes (stated, not hidden)

- **VOLE closure is the honest physical total** the observation fetched
  (`descriptor+manifest+index+seed`). It reads the whole descriptor for a
  narrow request here (`descriptor_read_mode=full`), which is the Phase-22.5
  mechanism; the independent `strace` store-read count agrees within ~0.1%.
- **The fragment directory is a prototype, not a wire format.** The
  required byte set is derived per format (below); the directory cost is
  modelled at 48 B/entry and does not include a query->fragment resolution
  index beyond that per-fragment entry.
- **PDF**: required objects are the ones reachable from the page-1 node via
  qpdf's object graph (NOT through `/Parent`), plus the xref/trailer tail;
  object spans come from a `N G obj`...`endobj` scan, so the native fragment
  count is approximate (the required *bytes* are the union of those spans).
- **DOCX**: a DEFLATE member cannot be range-accessed inside the stream, so
  the required set is the whole compressed `word/document.xml` member plus the
  package directory. Paragraph locality is therefore bounded by the member,
  not the paragraph.
- **Parquet**: VOLE's contract has no predicate pushdown, so
  `--parquet-cell 0:0` (one column chunk = its narrowest observation) stands
  in for a predicate+projection. A real predicate would only shrink the
  required set (row-group pruning via min/max statistics), so the recorded
  floor is an **upper bound** for the predicate case.
- **SQLite**: two source-retaining envelopes are reported. `pages` (one row
  per 4 KiB page) is the honest page-level control; `blob` is the Phase-22.5
  `source_blob` analogue, whose overflow chain must be walked for a deep
  offset (hence its much larger reads). `mmap_size=0`, `cache_size=-8`.
- **Directory overhead is the binding constraint for row/record formats**:
  a per-record structural index has one entry per row/element and is larger
  than the source (see the schemes table); only a COARSE structural index
  (objects / members / row groups / column chunks) is viable.

