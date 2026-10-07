# real100-v1 — real NASA/NIST document corpus

A frozen corpus of **100 real, public NASA and NIST documents** (PDF, DOCX, EPUB)
for exactness and size courts. This directory holds the acquisition harness
output for the `real100-v1` branch. The pilot (`pilot/`) is development-only; the
frozen 100 is assembled here in `manifest.tsv` / `manifest.toml`.

**Status of this subphase:** harness complete; 20-document pilot frozen; **98 of
the 100 frozen-corpus documents acquired** (see [Current compliance](#current-compliance)).
The two stragglers are NIST DOCX shapes that are not yet sourced (see
[Known gaps](#known-gaps)).

## Layout

```text
real100-v1/
  README.md                 this file
  manifest.tsv              canonical manifest (tab-separated, 20 fields)
  manifest.toml             generated view of manifest.tsv
  SHA256SUMS                sha256 of every committed document
  documents/
    nasa/{pdf,epub}/        NASA PDFs and EPUBs (committed)
    nist/{pdf,docx,epub}/   NIST PDFs, DOCX, EPUBs (committed)
  pilot/                    the development-only real20-pilot (see pilot/README.md)
  sources/                  candidate catalogs, the frozen selection, probe +
                            diversity evidence (see sources/README.md)
  .cache/                   on-demand bytes for redistributable=false rows
                            (gitignored; not committed)
```

Every committed file has a verified SHA-256 and byte length. Nothing here is a
placeholder, a derivative, or a locally converted document: every document is
the bytes a NASA/NIST server served.

## Manifest schema (frozen)

`manifest.tsv` is the canonical store; `manifest.toml` is generated from it.
Exactly these 20 fields, in order:

```text
id agency title publication_id format source_url landing_page retrieved_utc
sha256 byte_len publication_year publication_family document_type
producer_or_origin_if_known size_class structural_tags
cross_format_family_id revision_family_id rights_status redistributable
```

- `format` is verified by the **bytes** (PDF header found within the first 1024
  bytes per spec; OPC ZIP members for DOCX/EPUB), never by extension or the
  server's Content-Type.
- `structural_tags` is a `;`-separated set. Objective tags (`scanned`,
  `figure-heavy`, `complex-xref`, `very-large`, and the DOCX/EPUB table / list /
  heading / image / spine facts) come from `tools/realcorpus/probe.py`, which
  reads the downloaded bytes with Poppler/qpdf and the Python stdlib. Tags the
  byte probe cannot classify (PDF `table-heavy`, `equation-heavy`,
  `multi-column`, `appendix-heavy`, `reference-heavy`, `simple`) are **curated
  from pre-performance attributes only** (series / stiType / title keywords) by
  `tools/realcorpus/select-real100.py`; no codec result ever influenced a tag.
- `cross_format_family_id` / `revision_family_id` group agency-published
  originals (e.g. a NIST SP PDF and its official DOCX supporting file), never a
  local conversion.
- `redistributable` is recorded honestly per source. NASA and NIST public
  documents are generally US-Government public domain with attribution, so all
  rows here are `redistributable=true` and their bytes are committed. A row
  marked `false` would keep only URL + metadata + SHA-256 + length and be
  fetched on demand (`fetch --fetch-missing`).

## Selection discipline

Selection used **pre-performance attributes only**: agency, format, series
(stiType / publication family), year, size class, structural category, and the
availability of cross-format / revision families. It never used a VOLE runtime
result, bytes read, ingest size, success/failure, token count, or any baseline
result to keep or drop a document, and no document was substituted because it
exposed a loss. The frozen selection is `sources/real100_selection.tsv`
(regenerated deterministically by `tools/realcorpus/select-real100.py`); freeze
is by SHA-256.

## Reproduce

Everything runs in the pinned, memory-capped `realcorpus` Docker service — never
on the host:

```sh
# discover candidate pools (read-only)
docker compose run --rm --no-TTY realcorpus \
  python3 tools/realcorpus/discover-nasa-ebooks.py --out real100-v1/sources/nasa_ebook_assets.tsv
docker compose run --rm --no-TTY realcorpus \
  python3 tools/realcorpus/discover-ntrs.py --out real100-v1/sources/nasa_ntrs_candidates.tsv \
    --query "NACA technical note" --query "technical memorandum" # ...

# regenerate the frozen selection, acquire (idempotent), probe-tag, verify
docker compose run --rm --no-TTY realcorpus python3 tools/realcorpus/select-real100.py
docker compose run --rm --no-TTY realcorpus sh tools/realcorpus/select-real100.sh
docker compose run --rm --no-TTY realcorpus \
  python3 tools/realcorpus/acquire.py retag --corpus real100-v1
docker compose run --rm --no-TTY realcorpus sh tools/realcorpus/verify.sh --diversity
```

## Current compliance

Composition (98 documents acquired):

```text
nasa pdf 40 | nasa epub 15 | nist pdf 20 | nist docx 13 | nist epub 10
NASA 55 / NIST 43 ; PDF 60 / DOCX 13 / EPUB 25
```

Diversity table — **49 PASS, 29 PARTIAL, 2 FAIL** (80 gates). Full output is in
`sources/diversity-real100.txt`. The 2 hard FAILs are marked below.

```text
== composition ==
  NASA PDF 40-40           40 PASS     NIST DOCX 15-15         13 PARTIAL
  NASA EPUB 15-15          15 PASS     NIST EPUB 10-10         10 PASS
  NIST PDF 20-20           20 PASS     NASA total 55-55        55 PASS
  PDF total 60-60          60 PASS     NIST total 45-45        43 PARTIAL
  DOCX total 15-15         13 PARTIAL  EPUB total 25-25        25 PASS
== nasa-era ==
  pre-1960 5-5  5 PASS   1960-1979 7-7  6 PARTIAL   1980-1999 8-8  7 PARTIAL
  2000-2014 8-8 7 PARTIAL  2015-2024 8-8 11 PARTIAL  2025-2026 4-4 4 PASS
== nasa-center ==
  distinct centers >=4  6 PASS   max single-center share <=40%  0 PASS
== nasa-doctype ==
  TM 8-10 6 PARTIAL   TR 8-10 7 PARTIAL   CR 5-7 4 PARTIAL
  conference 4-6 4 PASS   handbook/ref 3-5 2 PARTIAL   NACA/early 5 5 PASS
== nasa-struct ==
  scanned 8 14 PASS   table-heavy 8 13 PASS   figure-heavy 8 26 PASS
  equation-heavy 6 6 PASS   multi-column 5 6 PASS   very large 3 9 PASS
  appendix/ref-heavy 5 2 PARTIAL   simple born-digital 4 2 PARTIAL
== nasa-epub ==
  long 4 15 PASS  image 3 13 PASS  ref/footnote 2 4 PASS  simple 2 4 PASS
  nav 2 5 PASS  unusual/large 2 6 PASS
== nist-pdf ==
  SP 5 PASS  NISTIR 4 PASS  Handbooks 3 PASS  TN 3 PASS  FIPS 2 PASS  other 3 PASS
== nist-docx ==
  table-heavy 3 8 PASS  procedure 2 2 PASS  deep-headings 2 2 PASS  forms 2 10 PASS
  list-heavy 1 10 PASS  glossary/index 2 1 PARTIAL
  image-heavy 2 0 FAIL   long regulatory 1 0 FAIL
== nist-epub ==
  long 3 9 PASS  figure-heavy 2 7 PASS  reference 1 7 PASS  simple 1 3 PASS
  unusual 1 1 PASS   table-heavy 2 1 PARTIAL
== cross-format ==
  NASA PDF<->EPUB 10-12 12 PASS
  NIST PDF<->DOCX 8-10 2 PARTIAL     NIST PDF<->EPUB >=5 1 PARTIAL
== revision ==
  docs in revision/publication families 15-20  23 PARTIAL
== size ==
  <100KiB 10-10 8 PARTIAL   100KiB-1MiB 20-20 19 PARTIAL   1-10MiB 30-30 48 PARTIAL
  10-50MiB 20-20 18 PARTIAL  50-100MiB 10-10 3 PARTIAL   >100MiB 5-5 2 PARTIAL
== corpus-min ==
  tables 25 22 PARTIAL   figure/res 25 52 PASS   equations 15 6 PARTIAL
  appendix/ref 15 13 PARTIAL   scanned 10 15 PASS   complex 10 41 PASS
  multi-column 8 6 PARTIAL   rich DOCX 8 13 PASS   unusual DOCX 6 10 PASS
  spine EPUB 8 24 PASS   image EPUB 6 13 PASS   >100MiB 5 2 PARTIAL
```

Failed gates (2): **NIST DOCX image-heavy** and **NIST DOCX long-regulatory** —
both need Word documents whose bytes are image-heavy / long-and-regulatory;
the CSRC supporting-file DOCX set is forms, templates and guides, which are
table/list/heading-heavy but not image-heavy, and no long regulatory .docx
(a full handbook) was found published by NIST.

## Known gaps

Remaining steps to freeze the 100:

1. **2 NIST DOCX** to reach 15: source an image-heavy Word document and a long
   regulatory Word document from an official NIST publication (e.g. a Word
   version of a large handbook or regulation).
2. **NIST cross-format families**: only 5 DOCX families (SP 800-18r2, 800-34r1,
   800-53r5, 800-88r1, 800-218) and 1 EPUB↔PDF family (SP 800-115) are
   reachable, because the NIST PDF family gate fixes the SP slot count at 5.
   Adding the matching SP PDFs for the remaining DOCX/EPUB families raises the
   `NIST PDF<->DOCX` / `NIST PDF<->EPUB` family counts toward 8-10 / 5.
3. **NASA era/doctype balance**: 12 of the 40 NASA PDFs are e-book PDFs (tagged
   `document_type=EBOOK`) that form the NASA cross-format pairs; their upload
   year crowds the 2015-24 era band. Rebalancing (or sourcing e-books with older
   publication years) closes the era/doctype PARTIALs.
4. **Size distribution**: few 50-100 MiB and >100 MiB files; add larger NASA
   scanned reports / NIST handbooks.
5. **PDF structure tags** beyond the byte probe (table / equation / multi-column
   / appendix) are curated, not measured; a bounded PDF structure classifier is
   follow-up work.

## Provenance

Base image: `debian:bookworm-slim@sha256:3783cc01769c7b2b1b83a5c5ad96c815348e28ed7da68e2e3687004faa906251`
(same digest as the `tools` service), plus `curl`, `ca-certificates`, `python3`
(Debian bookworm = 3.11.2), `poppler-utils` and `qpdf` for the structural probe.
The acquisition test bed also recorded: Docker 29.1.5 / Compose v5.1.1,
`pdfinfo` 22.12.0, `qpdf` 11.3.0, CPU x86_64. Every fetch records the retrieval
timestamp, the effective URL, the SHA-256 and the byte length.
