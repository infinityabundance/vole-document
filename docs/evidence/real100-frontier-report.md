# `real100-v1` frontier court — where VOLE wins, ties, and loses on real documents

The frozen 100-document NASA/NIST corpus (`real100-v1`, manifest SHA-256
`1b4c873066b46052b64de755a8a5409d68ede14f117a539660b2d7f7de017595`) was run
through the **frozen** architecture and two honest baselines, with **no tuning**
of the architecture against the population before the court. The goal is a
*frontier map*, not a win count.

| lane | what it is |
|---|---|
| **VOLE** | the Phase-11/12 procedural `DocumentField` as frozen (auto encoder, all features) |
| **SQLite/FTS** | one-time-preprocessed, source-retaining SQLite + FTS5 (the 12.11 A1 lane) |
| **direct tooling** | per-query Poppler/`pdfinfo` for PDF; stdlib `zipfile` + XML for DOCX/EPUB (A0) |

Court: `tools/real100-court.sh`; schedule `tools/fixtures/real100-schedule.py`;
map `tools/fixtures/real100-frontier.py`; sealed campaign
`evidence/campaigns/2026-10-07-real100-frontier-8f10d00/` (`SUMMARY.md`, `raw/`).

## Coverage

| lane | ops answered | ops declined |
|---|---:|---:|
| VOLE | 399 | 301 |
| SQLite/FTS | 506 | 194 |
| direct tooling | 508 | 192 |

Exact reconstruction (materialize, length + SHA-256 vs the source):

| lane | byte-exact |
|---|---:|
| VOLE `materialize --exact` | 97/100 |
| SQLite/FTS (retained source blob) | 98/100 |
| direct tooling (the source file) | 100/100 |

## Frontier map (median wall per (stratum, workload); ±10% = tie)

`loss`/`win`/`tie` are relative to the fastest *answering* lane; `decline` means
the lane has no such observation for that format, or returned a typed error.

| workload | VOLE | SQLite/FTS | direct tooling |
|---|---|---|---|
| one text lookup | loss | win | loss |
| repeated text lookup | **win** | tie | loss |
| heading | loss | win | loss |
| table | **win** | tie | loss |
| resource | loss | win | loss |
| metadata | loss | win | loss |
| exact reconstruction | loss | loss | **win** |

By format (VOLE's region narrows sharply for EPUB):

| format | text_once | text_repeat | heading | table | resource | metadata | exact |
|---|---|---|---|---|---|---|---|
| PDF | loss | **win** | — | — | — | loss | loss |
| DOCX | loss | loss | loss | **win** | loss | **win** | loss |
| EPUB | loss | loss | loss | loss | loss | loss | loss |

## What the map says

1. **VOLE's defensible region is repeated, heterogeneous observation over
   parseable structure at small-to-moderate size — not cold single lookups.**
   VOLE wins repeated text (the second and later observations are served from the
   persisted derived state, so it never re-parses) and wins DOCX tables/metadata.
   It loses the *cold, one-shot* lookup to SQLite/FTS, whose index is prebuilt, and
   loses exact reconstruction to simply keeping the file.

2. **Two structurally defined VOLE failure regions appeared on real input** (both
   recorded, neither hidden):

   - **Real EPUB content.** Every NASA EPUB content document carries the standard
     XHTML `<!DOCTYPE …>`, which the bounded-XML policy forbids, so the EPUB
     adapter declines **all content observations** (`text`/`heading`/`table`,
     typed `InvalidXmlStructure`, rc 19). Only `metadata`/`resource` — read from
     the OPF/manifest, which contain no XHTML — answer. 62 of VOLE's 301 declines
     are EPUB.
   - **Very large PDFs (> 100 MiB).** VOLE encode was OOM-killed under the 6 GiB
     lane cap (rc 137) or hit the 180 s per-op timeout (rc 124) on 3 documents and
     produced no descriptor; the largest that did succeed produced a 367 MB
     descriptor for a ~400 MB scanned PDF.

3. **Storage is not a VOLE win here either.** VOLE's one-time descriptor+store is
   typically larger than the A1 database for PDFs (e.g. `nasa-pdf-0004`
   179 MB vs 82 MB; `nasa-pdf-0012` 181 MB vs 78 MB); for EPUB the two are
   comparable. See `raw/onetime.tsv`.

4. **PDF has no common structural selectors** (`heading`/`table`/`resource` all
   decline for every PDF in every lane), so the "mixed session" workload is only
   defined for DOCX/EPUB today.

## Caveats (why the numbers are scoped)

- Every op spawns a process, so tiny per-op walls are dominated by process
  startup; the *ratio* is the signal, not the millisecond.
- Cells mix answered and declined documents; a `loss`/`win` is computed over the
  documents where the lane answered. `decline` counts are in `raw/ops.tsv`.
- The >100 MiB failures are **resource bounds** (a memory cap and a timeout), not
  proofs that the architecture cannot encode large files; raising a cap is a
  deliberate, recordable act and was not done in this first court.
- This is the **first** court over the frozen population. Nothing was tuned.

## What product this implies

The measured region where the field pays for itself is narrow and specific:
**repeated, varied observations of the same document where its native structure
is parseable and the document is not a huge opaque binary.** Outside that region
the honest comparisons win — a prebuilt relational/FTS cache for cold lookups and
one-shot structure, direct tooling for raw extraction and exact copies. The two
failure regions above are the concrete engineering backlog the map produced.
