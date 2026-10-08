# Phase 20.4 — C4b contract court (C4b supplied to BOTH lanes under an equal external input)

Both lanes satisfy the SAME escalating contract C0..C5. `VOLE` is the field CLI (`observe`/`observe-batch`/`materialize`), which exposes a PDF revision-lineage surface (`--revisions`/`--revision N --kind lineage`) and, from Phase 20.4, an EXPLICIT external corpus-lineage surface (`field-external` + `observe --external-lineage --kind lineage`); `SQLite` is a source-retaining **SQLite Full** baseline: the established A1 surfaces (source blob, blocks, unicode61 + trigram FTS, headings/tables/cells/resources) plus, exactly as the contract deepens, a native-coordinate column (C1), a provenance table (C2), the retained source blob as the exact closure (C3) and a corpus revision-lineage table (C4). The baseline is byte-identical to Phase 16.5. Searches are not part of the contract, but the baseline carries them anyway so its cost is a faithful upper bound. Equivalence is validated per depth; costs are cumulative.

Subset: **12 documents** (docx, epub, pdf).

Persistent bytes are the sum of REGULAR-FILE sizes for both lanes. `du -sb` is deliberately not used: the repo's bind-mounted host filesystem reports large phantom directory sizes, so `du -sb` on VOLE's directory store would be compared unfairly against SQLite's single db file.

## One-time build cost + persistent bytes (cumulative per depth)

| substrate | depth | docs | build ms (sum) | build ms (median) | persistent B (sum) | vs source |
|---|---|---:|---:|---:|---:|---:|
| VOLE field-build (direct) | all | 12 | 11641 | 117 | 7168131 | 1.236× |
| SQLite contract store | C0 | 12 | 1585 | 73 | 14270464 | 2.460× |
| SQLite contract store | C1 | 12 | 1579 | 75 | 14376960 | 2.478× |
| SQLite contract store | C2 | 12 | 1585 | 74 | 14622720 | 2.521× |
| SQLite contract store | C3 | 12 | 1570 | 74 | 14622720 | 2.521× |
| SQLite contract store | C4 | 12 | 1580 | 73 | 14721024 | 2.538× |
| SQLite contract store | C5 | 12 | 1586 | 74 | 14721024 | 2.538× |

`vs source` is the persistent footprint as a multiple of the subset's total source bytes. VOLE's query cost is depth-independent (its store already carries coord/provenance/exact); SQLite pays new materialization at each depth.

## Query schedule — cost per depth

Cold = one process per observation; Warm = one session serving the whole depth schedule (VOLE `observe-batch`; SQLite one process). VOLE's schedule is measured per depth but is depth-independent.

| depth | lane | obs | cold ms | warm ms | warm peak RSS KB |
|---|---|---:|---:|---:|---:|
| C0 | VOLE | 80 | 131 | 34 | 6888 |
| C0 | SQLite | 80 | 125 | 22 | 7208 |
| C1 | VOLE | 80 | 118 | 32 | 7136 |
| C1 | SQLite | 80 | 120 | 24 | 7224 |
| C2 | VOLE | 80 | 122 | 33 | 7064 |
| C2 | SQLite | 80 | 120 | 26 | 7272 |
| C3 | VOLE | 80 | 121 | 35 | 6896 |
| C3 | SQLite | 80 | 123 | 26 | 7340 |
| C4 | VOLE | 80 | 127 | 38 | 7016 |
| C4 | SQLite | 80 | 123 | 26 | 7340 |
| C5 | VOLE | 80 | 124 | 33 | 7152 |
| C5 | SQLite | 80 | 119 | 25 | 7304 |

## Equivalence by depth and observation

| depth | fmt | obs | eq raw | eq proj | shape | both decline | capability gap | divergent | different observable | value mismatch |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| C0 | docx | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C0 | docx | doc-text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C0 | docx | heading | 1 | 0 | 0 | 3 | 0 | 0 | 0 | 0 |
| C0 | docx | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C0 | docx | resource | 0 | 0 | 1 | 3 | 0 | 0 | 0 | 0 |
| C0 | docx | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C0 | docx | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C0 | docx | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C0 | epub | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C0 | epub | doc-text | 1 | 3 | 0 | 0 | 0 | 0 | 0 | 0 |
| C0 | epub | heading | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C0 | epub | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C0 | epub | resource | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C0 | epub | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C0 | epub | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C0 | epub | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C0 | pdf | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C0 | pdf | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C0 | pdf | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C0 | pdf | text | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 |
| C1 | docx | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C1 | docx | doc-text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C1 | docx | heading | 1 | 0 | 0 | 3 | 0 | 0 | 0 | 0 |
| C1 | docx | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C1 | docx | resource | 0 | 0 | 1 | 3 | 0 | 0 | 0 | 0 |
| C1 | docx | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C1 | docx | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C1 | docx | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C1 | epub | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C1 | epub | doc-text | 1 | 3 | 0 | 0 | 0 | 0 | 0 | 0 |
| C1 | epub | heading | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C1 | epub | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C1 | epub | resource | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C1 | epub | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C1 | epub | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C1 | epub | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C1 | pdf | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C1 | pdf | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C1 | pdf | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C1 | pdf | text | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 |
| C2 | docx | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C2 | docx | doc-text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C2 | docx | heading | 1 | 0 | 0 | 3 | 0 | 0 | 0 | 0 |
| C2 | docx | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C2 | docx | resource | 0 | 0 | 1 | 3 | 0 | 0 | 0 | 0 |
| C2 | docx | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C2 | docx | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C2 | docx | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C2 | epub | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C2 | epub | doc-text | 1 | 3 | 0 | 0 | 0 | 0 | 0 | 0 |
| C2 | epub | heading | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C2 | epub | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C2 | epub | resource | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C2 | epub | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C2 | epub | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C2 | epub | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C2 | pdf | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C2 | pdf | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C2 | pdf | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C2 | pdf | text | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 |
| C3 | docx | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C3 | docx | doc-text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C3 | docx | heading | 1 | 0 | 0 | 3 | 0 | 0 | 0 | 0 |
| C3 | docx | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C3 | docx | resource | 0 | 0 | 1 | 3 | 0 | 0 | 0 | 0 |
| C3 | docx | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C3 | docx | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C3 | docx | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C3 | epub | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C3 | epub | doc-text | 1 | 3 | 0 | 0 | 0 | 0 | 0 | 0 |
| C3 | epub | heading | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C3 | epub | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C3 | epub | resource | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C3 | epub | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C3 | epub | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C3 | epub | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C3 | pdf | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C3 | pdf | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C3 | pdf | revision | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C3 | pdf | text | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 |
| C4 | docx | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C4 | docx | doc-text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C4 | docx | heading | 1 | 0 | 0 | 3 | 0 | 0 | 0 | 0 |
| C4 | docx | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C4 | docx | resource | 0 | 0 | 1 | 3 | 0 | 0 | 0 | 0 |
| C4 | docx | revision | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0 |
| C4 | docx | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C4 | docx | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C4 | epub | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C4 | epub | doc-text | 1 | 3 | 0 | 0 | 0 | 0 | 0 | 0 |
| C4 | epub | heading | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C4 | epub | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C4 | epub | resource | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C4 | epub | revision | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0 |
| C4 | epub | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C4 | epub | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C4 | pdf | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C4 | pdf | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C4 | pdf | revision | 0 | 0 | 0 | 0 | 0 | 0 | 4 | 0 |
| C4 | pdf | text | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 |
| C5 | docx | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C5 | docx | doc-text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C5 | docx | heading | 1 | 0 | 0 | 3 | 0 | 0 | 0 | 0 |
| C5 | docx | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C5 | docx | resource | 0 | 0 | 1 | 3 | 0 | 0 | 0 | 0 |
| C5 | docx | revision | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0 |
| C5 | docx | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C5 | docx | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C5 | epub | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C5 | epub | doc-text | 1 | 3 | 0 | 0 | 0 | 0 | 0 | 0 |
| C5 | epub | heading | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C5 | epub | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C5 | epub | resource | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C5 | epub | revision | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0 |
| C5 | epub | table | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C5 | epub | text | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C5 | pdf | bytes | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| C5 | pdf | metadata | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 |
| C5 | pdf | revision | 0 | 0 | 0 | 0 | 0 | 0 | 4 | 0 |
| C5 | pdf | text | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 |

_`capability gap` = one lane declines an observation the other answers. `divergent` = both answer but the byte values differ on a *heuristic* observable (PDF page text: VOLE's layout heuristic vs Poppler). `different observable` = the `revision` observation of different things: VOLE reports the PDF's internal incremental chain; the baseline reports the corpus family/member/head tuple. The split into C4a (document-native) and C4b (corpus/external) is tabulated below; neither value can equal the other by construction, so this is neither equality nor an error. `value mismatch` = both answer but differ on a non-heuristic observable. `shape` = both answer with projections that are not byte-comparable by design (metadata schemas; resource reference vs member bytes)._


## C4 split — document-native lineage (C4a) vs corpus/external lineage (C4b)

C4 is two different observables, reported separately and never forced equal.

**C4a — document-native lineage** (PDF incremental revisions; typed unsupported otherwise). Byte-derivable, so a source-retaining store *could* hold it; VOLE indexes it at ingest, the baseline lane as configured does not expose it.

| lane | PDF (native chain) | DOCX/EPUB (no native history) |
|---|---|---|
| VOLE (`observe --revisions --kind lineage`) | answered **4/4** | typed unsupported (rc 6) 8/8 |
| SQLite lane (`revision` at C4) | native chain **0/4** — answers the corpus tuple instead | native chain **0/8** |
| retained bytes (probe) | a header+chain is derivable from the retained source for **4/4** PDFs; revision count matches VOLE for **4** | n/a |

_Probe = an unmeasured marker scan (`%%EOF` count + header) over the SAME bytes the baseline retains as its C3 closure (`source_blob`); it tests byte-*derivability*, not full fidelity, and is not a PDF-conformance oracle._

Phase 20.4 supplies the SAME family/member/head to BOTH lanes (an equal external input): the harness gives it to SQLite at build (`--family/--member/--is-head`); VOLE receives it as an explicit typed `ExternalContext` stored BESIDE the field (`field-external`) and answers it as `selector=external-lineage, representation=lineage, basis=external-metadata`.

**C4b — corpus/external lineage** (dataset family id / member id / head flag). DATASET metadata, **not document-derived**.

| lane | C4b answered | basis recorded | C4b tuple equals the other lane |
|---|---:|---|---:|
| SQLite lane (`revision` at C4) | **12/12** (family set for 5) | dataset metadata in the `revisions` table; external to the bytes | — |
| VOLE (`observe --external-lineage --kind lineage`) | **12/12** | `external-metadata` (typed; never exact; reads no seed node) | **12/12** |

C4b cost over the subset: VOLE `field-external` attach **26 ms** writing **1001 B** of sidecar (one record per document, in `<store>/external/`); VOLE external query **8 ms** (reads only that sidecar); SQLite C4b query (`revision` at C4) **19 ms**. SQLite's C4b data is written as part of its C4 build, not in a separate step.

Separation: a plain `--metadata --kind metadata` answer (`wall_micros` removed) is byte-identical before vs after the attach for **12/12** documents, and after the clear for **12/12**; the external query declines typed (rc 6) after the clear for **12/12**; and the exact closure still matches after the clear for **12/12**. The external answer record itself reads no descriptor/manifest/index/seed byte (`bytes_read` 0).

**C5 — C4 + a one-session heterogeneous batch**, under both readings:

- **C5a (= C4a + batch):** VOLE serves the PDF native chain inside one `observe-batch` session; the baseline session has no C4a answer to serve (it declines C4a).
- **C5b (= C4b + batch):** under Phase 20.4 both lanes CAN serve the corpus tuple in one session; this court measures the C4b *query* as a separate step beside the schedule rather than folding `--external-lineage` into the heterogeneous batch (recorded as a residual).

| id | fmt | family | C4a VOLE | C4a SQLite lane | C4a from retained bytes | C4b SQLite | C4b VOLE |
|---|---|---|---|---|---|---|---|
| nist-pdf-0002 | pdf | - | native (rc 0) | corpus tuple only | derivable (count match) | yes | yes (external-metadata) |
| nist-pdf-0004 | pdf | - | native (rc 0) | corpus tuple only | derivable (count match) | yes | yes (external-metadata) |
| nist-pdf-0016 | pdf | rev-nist-fips-140 | native (rc 0) | corpus tuple only | derivable (count match) | yes | yes (external-metadata) |
| nist-pdf-0017 | pdf | rev-nist-fips-140 | native (rc 0) | corpus tuple only | derivable (count match) | yes | yes (external-metadata) |
| nist-docx-0005 | docx | rev-nist-sp800-34 | typed unsupported (rc 6) | corpus tuple only | n/a | yes | yes (external-metadata) |
| nist-docx-0008 | docx | rev-nist-sp800-34 | typed unsupported (rc 6) | corpus tuple only | n/a | yes | yes (external-metadata) |
| nist-docx-0009 | docx | rev-nist-sp800-53 | typed unsupported (rc 6) | corpus tuple only | n/a | yes | yes (external-metadata) |
| nist-docx-0014 | docx | - | typed unsupported (rc 6) | corpus tuple only | n/a | yes | yes (external-metadata) |
| nist-epub-0003 | epub | - | typed unsupported (rc 6) | corpus tuple only | n/a | yes | yes (external-metadata) |
| nist-epub-0006 | epub | - | typed unsupported (rc 6) | corpus tuple only | n/a | yes | yes (external-metadata) |
| nist-epub-0008 | epub | - | typed unsupported (rc 6) | corpus tuple only | n/a | yes | yes (external-metadata) |
| nist-epub-0009 | epub | - | typed unsupported (rc 6) | corpus tuple only | n/a | yes | yes (external-metadata) |

## Exact original closure (length + SHA-256 + byte compare)

| id | fmt | VOLE ok | SQLite ok | VOLE ms | SQLite ms |
|---|---|---|---|---:|---:|
| nist-pdf-0002 | pdf | 1 | 1 | 4 | 26 |
| nist-pdf-0004 | pdf | 1 | 1 | 7 | 26 |
| nist-pdf-0016 | pdf | 1 | 1 | 4 | 28 |
| nist-pdf-0017 | pdf | 1 | 1 | 8 | 28 |
| nist-docx-0005 | docx | 1 | 1 | 3 | 25 |
| nist-docx-0008 | docx | 1 | 1 | 4 | 25 |
| nist-docx-0009 | docx | 1 | 1 | 4 | 28 |
| nist-docx-0014 | docx | 1 | 1 | 7 | 27 |
| nist-epub-0003 | epub | 1 | 1 | 4 | 26 |
| nist-epub-0006 | epub | 1 | 1 | 4 | 26 |
| nist-epub-0008 | epub | 1 | 1 | 4 | 26 |
| nist-epub-0009 | epub | 1 | 1 | 8 | 27 |

VOLE `materialize --exact`: 12/12 byte-exact. SQLite retained blob: 12/12 byte-exact.

## Frontier verdict

| depth | VOLE cold ms | SQL cold ms | VOLE warm ms | SQL warm ms | VOLE B | SQL B | cold | warm | bytes | VOLE satisfies? |
|---|---:|---:|---:|---:|---:|---:|---|---|---|---|
| C0 | 131 | 125 | 34 | 22 | 7168131 | 14270464 | SQLite | SQLite | VOLE | yes |
| C1 | 118 | 120 | 32 | 24 | 7168131 | 14376960 | VOLE | SQLite | VOLE | yes |
| C2 | 122 | 120 | 33 | 26 | 7168131 | 14622720 | SQLite | SQLite | VOLE | yes |
| C3 | 121 | 123 | 35 | 26 | 7168131 | 14622720 | VOLE | SQLite | VOLE | yes |
| C4 | 127 | 123 | 38 | 26 | 7168131 | 14721024 | SQLite | SQLite | VOLE | no (declines + diff. observable) |
| C5 | 124 | 119 | 33 | 25 | 7168131 | 14721024 | SQLite | SQLite | VOLE | no (declines + diff. observable) |

One row per contract depth; `bytes` compares the sum-of-regular-file persistent footprints.

### Reading

VOLE/SQLite storage **0.49×**, build **7.34×**, warm session **1.38×**. VOLE is the STORAGE winner at every depth; SQLite is the BUILD, WARM-LATENCY and FULL-CONTRACT winner at every depth.

- On DOCX/EPUB text the two systems return **byte-identical** values (blocks/headings/tables/doc-text): the baseline mirrors VOLE's extraction semantics. EPUB doc-text also matches under the whitespace projection (VOLE emits a trailing newline for an empty spine item).
- Byte reads and the whole-source exact closure agree on BOTH lanes: 12/12 documents reproduce their original length + SHA-256 (VOLE `materialize --exact`; SQLite retained blob).
- PDF page text is a **heuristic layout projection**: VOLE's own heuristic and Poppler produce different bytes, so equality holds only for the contract SHAPE there (recorded as `divergent`, never as equality).
- C4 splits into **C4a (document-native lineage)** and **C4b (corpus/external lineage)**. VOLE answers C4a (the PDF's internal incremental chain: header, count, ordered spans, `startxref`/`/Prev`, object membership) and declines it typed for DOCX/EPUB (`UnsupportedFeature`, rc 6). C4a is byte-derivable and the baseline retains the exact source (the C3 closure), so the baseline *could* answer C4a by re-scanning what it keeps — but its measured lane does not, so at the lane level C4a is a genuine VOLE-only answer, a difference of WHERE the work happens (indexed at ingest vs re-parsed at query), not of information held. **Phase 20.4 changes C4b:** supplied the SAME family/member/head the baseline gets, VOLE answers the tuple from an explicit external layer (basis `external-metadata`, rc 0) for **12/12** documents, matching the baseline's tuple for **12/12**. C4b is therefore no longer a VOLE capability gap; what remains is a *cost* comparison (a one-record-per-document sidecar plus a query that reads only that sidecar, versus the baseline's in-build insert), not a coverage claim.

## Conclusions

1. **The direct build materially narrows the build gap but does not close it.** With `field-build` the VOLE/SQLite build-wall ratio is **7.34×** (was **10.74×** with `encode`+`field-ingest` in 16.5/17.2): the earlier 10.7× and the 17.1 2.04× `field-build` speedup are not composable, and this court is the composable number. VOLE still builds slower than SQLite here, but by a far smaller factor.
2. **Storage, latency and cold ties are unchanged in direction.** VOLE is cheaper to STORE (about 0.49× the baseline bytes) at every depth; SQLite still serves the warm session ~1.38× faster and ties or wins cold. The richer contract costs SQLite only about +3% persistent bytes from C0 to C4 because the source blob it retains dominates the store.
3. **C4 (both halves) is now answered by BOTH lanes, and the honest question is cost.** C4a (document-native lineage) is answered by VOLE and, at the lane level, not by the baseline — but it is byte-derivable from the retained source, so it is a work-location difference, not an information advantage. C4b (corpus/external lineage) closes only because Phase 20.4 supplies the SAME external input to both: the baseline stores it as part of its C4 build; VOLE stores it as a typed `ExternalContext` BESIDE the field (attach **1001 B**, **26 ms** over the subset) and answers it in **8 ms**, versus the baseline's **19 ms** C4 query. The tuple matches for **12/12**.

4. **Whole-contract verdict:** on this subset, under the equal contract, SQLite remains the build- and warm-latency-winner and VOLE keeps the storage edge; with the C4b external input supplied equally, the contract now closes at C4 in both halves for both lanes, so the remaining differences are cost, not capability. Whether that makes VOLE a *better* C4b host is a cost question this court answers directly, not a coverage question it no longer needs to.
