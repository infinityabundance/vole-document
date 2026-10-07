# real100-v1 — real NASA/NIST document corpus

A frozen corpus of **100 real, public NASA and NIST documents** (PDF, DOCX, EPUB)
for exactness and size courts.

```text
frozen = true
frozen_utc = 2026-10-07T08:37:41Z
documents = 100
manifest_sha256 = see sources/real100_selection.tsv + SHA256SUMS
```

This directory holds the acquisition harness output for the `real100-v1` branch.
The pilot (`pilot/`) is development-only; the frozen 100 is assembled here in
`manifest.tsv` / `manifest.toml`.

**Status:** frozen. Exactly 100 documents, every manifest field populated, every
byte length + SHA-256 verified against the downloaded bytes, and every
cross-format / revision family id assigned. Diversity: **66 PASS, 13 PARTIAL,
1 FAIL** of 80 gates (`sources/diversity-real100.txt`).

## Layout

```text
real100-v1/
  README.md                 this file
  manifest.tsv              canonical manifest (tab-separated, 20 fields)
  manifest.toml             generated view of manifest.tsv
  SHA256SUMS                sha256 of every committed document
  documents/
    nasa/{pdf,epub}/        NASA PDFs and EPUBs (gitignored bytes)
    nist/{pdf,docx,epub}/   NIST PDFs, DOCX, EPUBs (gitignored bytes)
  pilot/                    the development-only real20-pilot (see pilot/README.md)
  sources/                  candidate catalogs, the frozen selection, probe +
                            diversity evidence (see sources/README.md)
  .cache/                   on-demand bytes for redistributable=false rows
                            (gitignored; not committed)
```

**Document binaries are gitignored** (`documents/`, `pilot/documents/`). Nothing
here is a placeholder, a derivative, or a locally converted document: every
document is the bytes an agency server served. The manifest carries URL +
SHA-256 + byte length; `tools/realcorpus/fetch.sh` (or
`select-real100.sh`) re-fetches from the recorded URL and re-verifies the hash.

## Manifest schema (frozen)

`manifest.tsv` is the canonical store; `manifest.toml` is generated from it.
Exactly these 20 fields, in order:

```text
id agency title publication_id format source_url landing_page retrieved_utc
sha256 byte_len publication_year publication_family document_type
producer_or_origin_if_known size_class structural_tags
cross_format_family_id revision_family_id rights_status redistributable
```

- `format` is verified by the **bytes** (PDF header within the first 1024 bytes;
  OPC ZIP members for DOCX/EPUB), never by extension or the server's
  Content-Type.
- `structural_tags` is a `;`-separated set. Objective tags (`scanned`,
  `figure-heavy`, `complex-xref`, `very-large`, and the DOCX/EPUB table / list /
  heading / image / spine facts) come from `tools/realcorpus/probe.py`, which
  reads the downloaded bytes with Poppler/qpdf and the Python stdlib. Tags the
  byte probe cannot classify (PDF `table-heavy`, `equation-heavy`,
  `multi-column`, `appendix-heavy`, `reference-heavy`, `simple`,
  `long-regulatory`) are **curated from pre-performance attributes only**
  (series / stiType / title keywords) by `tools/realcorpus/select-real100.py`;
  no codec result ever influenced a tag.
- `cross_format_family_id` / `revision_family_id` group agency-published
  originals (e.g. a NIST SP PDF and its official DOCX/EPUB supporting file),
  never a local conversion.
- `redistributable` is recorded honestly per source. NASA and NIST public
  documents are generally US-Government public domain with attribution, so all
  rows here are `redistributable=true` and their bytes are committed.

## Selection discipline

Selection used **pre-performance attributes only**: agency, format, series
(stiType / publication family), year, measured byte size (from a read-only HEAD
probe for NTRS candidates), structural category, and the availability of
cross-format / revision families. It never used a VOLE runtime result, bytes
read, ingest size, success/failure, token count, or any baseline result to keep
or drop a document, and no document was substituted because it exposed a loss.
The frozen selection is `sources/real100_selection.tsv` (regenerated
deterministically by `tools/realcorpus/select-real100.py`); freeze is by SHA-256.

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

Composition (100 documents):

```text
nasa pdf 40 | nasa epub 15 | nist pdf 20 | nist docx 15 | nist epub 10
NASA 55 / NIST 45 ; PDF 60 / DOCX 15 / EPUB 25
```

Diversity table — **66 PASS, 13 PARTIAL, 1 FAIL** (80 gates). Full output is in
`sources/diversity-real100.txt`. The 1 hard FAIL is marked below.

```text
== composition ==
  NASA PDF 40-40           40 PASS     NIST DOCX 15-15         15 PASS
  NASA EPUB 15-15          15 PASS     NIST EPUB 10-10         10 PASS
  NIST PDF 20-20           20 PASS     NASA total 55-55        55 PASS
  PDF total 60-60          60 PASS     NIST total 45-45        45 PASS
  DOCX total 15-15         15 PASS     EPUB total 25-25        25 PASS
== nasa-era ==
  pre-1960 5-5  5 PASS   1960-1979 7-7  7 PASS   1980-1999 8-8  8 PASS
  2000-2014 8-8 8 PASS   2015-2024 8-8  8 PASS   2025-2026 4-4  4 PASS
== nasa-center ==
  distinct centers >=4  8 PASS   max single-center share <=40%  0 PASS
== nasa-doctype ==
  TM 8-10 8 PASS   TR 8-10 6 PARTIAL   CR 5-7 5 PASS
  conference 4-6 4 PASS   handbook/ref 3-5 2 PARTIAL   NACA/early 5 5 PASS
== nasa-struct ==
  scanned 8 17 PASS   table-heavy 8 15 PASS   figure-heavy 8 29 PASS
  equation-heavy 6 7 PASS   multi-column 5 6 PASS   very large 3 15 PASS
  appendix/ref-heavy 5 3 PARTIAL   simple born-digital 4 2 PARTIAL
== nasa-epub ==
  long 4 15 PASS  image 3 14 PASS  ref/footnote 2 3 PASS  simple 2 5 PASS
  nav 2 7 PASS  unusual/large 2 5 PASS
== nist-pdf ==
  SP 5 PASS  NISTIR 4 PASS  Handbooks 3 PASS  TN 3 PASS  FIPS 2 PASS  other 3 PASS
== nist-docx ==
  table-heavy 3 9 PASS  procedure 2 2 PASS  deep-headings 2 3 PASS  forms 2 10 PASS
  list-heavy 1 11 PASS  image-heavy 2 2 PASS  glossary/index 2 1 PARTIAL
  long regulatory 1 0 FAIL
== nist-epub ==
  long 3 9 PASS  figure-heavy 2 7 PASS  reference 1 7 PASS  simple 1 3 PASS
  unusual 1 1 PASS   table-heavy 2 2 PASS
== cross-format ==
  NASA PDF<->EPUB 10-12 10 PASS
  NIST PDF<->DOCX 8-10 1 PARTIAL   NIST PDF<->EPUB >=5 5 PASS
== revision ==
  docs in revision/publication families 15-20  21 PARTIAL
== size ==
  <100KiB 10-10 8 PARTIAL   100KiB-1MiB 20-20 17 PARTIAL   1-10MiB 30-30 40 PARTIAL
  10-50MiB 20-20 20 PASS   50-100MiB 10-10 10 PASS   >100MiB 5-5 5 PASS
== corpus-min ==
  tables 25 26 PASS   figure/res 25 59 PASS   equations 15 7 PARTIAL
  appendix/ref 15 13 PARTIAL   scanned 10 18 PASS   complex 10 38 PASS
  multi-column 8 6 PARTIAL   rich DOCX 8 15 PASS   unusual DOCX 6 10 PASS
  spine EPUB 8 24 PASS   image EPUB 6 14 PASS   >100MiB 5 5 PASS
```

## Close-out decisions and over-constrained quotas

This close-out resolved the earlier 2 hard FAILs except one, and closed the
composition gap from 98 to 100. Each change is by pre-performance attributes
only.

1. **NIST DOCX 13 → 15 (composition PASS).** A NIST-wide search — CSRC SP 800 /
   SP 1800 / NISTIR / FIPS / TN / CSWP / AI / ITL publication pages (856 pages
   scraped for `.docx`), the `www.nist.gov` sitemap, the NIST OWM handbook pages,
   `nvlpubs.nist.gov`, and search — shows NIST's *published* Word bytes are forms,
   templates, comment/feedback forms and revision "deltas": SP 800-18r2 (4),
   800-34r1 (4), 800-53r5 (1), 800-88r1 (1), 800-218 (2), 800-140fr1 draft
   comment template, the chain-of-custody form and a DMP/EO-14028 template. None
   is image-heavy. The **image/resource-heavy** Word class was found in NIST's
   CFReDS (CFTT) public dataset documents, which genuinely exist as Word bytes:
   - `nist-docx-0014` — CFReDS *Data Leakage Case* `leakage-answers.docx`
     (13 embedded images, 72 tables, deep headings, ~108 KB of text);
   - `nist-docx-0015` — CFReDS *Mobile Device Data Population* (Android Z970)
     (11 embedded images).
   Both are NIST-hosted public Word documents (US-Gov public domain);
   `nist-docx image-heavy 2-2` now **PASSes**.
   The **long regulatory** Word class does **not** exist as published NIST Word
   bytes: NIST Handbooks 44 / 130 / 133 / 105 are PDF-only, and the only Word
   bytes NIST publishes are the forms/templates above. That gate is recorded as
   **FAIL — over-constrained**, not fabricated and not substituted by a local
   conversion (`long regulatory 1 0 FAIL`).
2. **NIST cross-format.** The NIST PDF family gate fixes **SP=5**. NIST publishes
   DOCX supporting files for only 5 SP families and EPUBs for a disjoint set, so
   `PDF<->DOCX` and `PDF<->EPUB` compete for the same 5 SP slots. This close-out
   re-selected the 5 SP PDFs to the families that also have agency EPUBs
   (800-115, 800-123, 800-30r1, 800-144) plus the DOCX-matched 800-18r2, and
   swapped one EPUB for the FIPS 140-2 EPUB, giving **`PDF<->EPUB` 5 — PASS**.
   `PDF<->DOCX` is therefore **1 (PARTIAL)**; the 8-10 target is
   **over-constrained** — even with all 5 SP slots assigned to DOCX families the
   maximum is 5, because no other NIST series (NISTIR/TN/FIPS/HB) publishes DOCX
   supporting files.
3. **Size bands.** The NASA selection was rebuilt around a curated 30-report NTRS
   list chosen by era/doctype/measured size: 5 pre-1960 NACA annual-report
   compilations (3 in the >100 MiB band), plus large TR/TM/CR/conference/SP
   reports. Result: **50-100 MiB 10 (PASS), >100 MiB 5 (PASS), 10-50 MiB 20
   (PASS)**. The three small bands (<100 KiB, 100 KiB-1 MiB, 1-10 MiB) remain
   PARTIAL: their exact targets sum with the three large bands to 95 documents,
   so with 100 documents they cannot all be satisfied at once (a spec
   over-constraint, not a sourcing gap).
4. **NASA era rebalance.** The e-book PDFs were reduced from 12 to **10** (still
   `NASA PDF<->EPUB 10` PASS) and chosen so two carry 2025 upload years and eight
   carry 2015-24 upload years; the 30 NTRS reports fill the older bands. All six
   era gates now **PASS** (5 / 7 / 8 / 8 / 8 / 4).
5. **Doctype / struct budget.** With only 30 non-e-book NASA PDFs and technical
   doctype minimums (TM 8 + TR 8 + CR 5 + conference 4 + handbook/ref 3 + NACA 5
   = 33) exceeding 30, the technical doctype gates cannot all pass; TR (6) and
   handbook/reference (2) are left PARTIAL. Likewise `simple born-digital` and
   `appendix/reference-heavy` (both driven by the SP/handbook budget) are
   PARTIAL, and the `glossary/index` DOCX gate needs a second index-bearing Word
   document that NIST does not publish.

## Harness fix

`tools/realcorpus/acquire.py` `cmd_add_tsv` had its add call mis-indented after
`continue`, so the block was unreachable and `add-tsv` recorded nothing (the
documented `select-real100.sh` path silently added 0 rows). The block was
de-indented to the loop body so the documented acquisition path works. No
`src/`, `tests/`, `fuzz/` or `Cargo.*` file was touched.

## Provenance

Base image: `debian:bookworm-slim@sha256:3783cc01769c7b2b1b83a5c5ad96c815348e28ed7da68e2e3687004faa906251`
(same digest as the `tools` service), plus `curl`, `ca-certificates`, `python3`
(Debian bookworm = 3.11.2), `poppler-utils` and `qpdf` for the structural probe.
The acquisition test bed also recorded: Docker 29.1.5 / Compose v5.1.1,
`pdfinfo` 22.12.0, `qpdf` 11.3.0, CPU x86_64. Every fetch records the retrieval
timestamp, the effective URL, the SHA-256 and the byte length. Freeze commit is
on branch `real100-v1`; `SHA256SUMS` is regenerated by
`tools/realcorpus/verify.sh`.
