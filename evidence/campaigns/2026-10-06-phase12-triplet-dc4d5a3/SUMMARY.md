# Phase 12.9 — cross-format equivalence court (canonical logical triplet)

**Result: 96/96 assertions passed, 0 failed.**

One known logical report was generated as `report.pdf`, `report.docx` and
`report.epub` (Python stdlib generator, `producers` image) with a
`ground_truth.json`. All three were ingested through the one format-agnostic
field pipeline and queried through the one common CLI.

## Same logical answer, different native provenance

| observation | PDF | DOCX | EPUB |
|---|---|---|---|
| `find XF12A` (marker) | yes | yes | yes |
| whole-document text | yes | yes | yes |
| metadata | yes (structural descriptor) | yes (story structure) | yes (package summary) |
| heading | typed decline | Introduction/Method/Findings | Introduction/Method/Findings |
| block/paragraph | typed decline | yes | yes |
| Table / Cell(B7) | typed decline | bravo-seven | bravo-seven |
| Resource (the one image) | typed decline | rel=`rIdImg` | href=`images/image1.png` (image/png) |
| Link | typed decline | text=`external` | href=`https://example.com/phase12` |

Every supported answer carries `format=<fmt>;common;<native>` provenance
(asserted in `raw/obs/*.json`), so the same logical answer is produced by
different native adapters over distinct byte representations.

## Honest gaps (recorded, not approximated)

* **No cross-format document-title observation.** The common vocabulary has no
  title selector: PDF `metadata` returns the structural field descriptor
  (`source_len`/`source_sha256`), DOCX `metadata` returns the story structure,
  and the EPUB `metadata` representation (the only one the capability set
  admits) is the package summary without the `dc:title` array. The title is
  therefore present in the sources and ground truth but is **not** claimed as a
  common observation.
* **PDF has no heading/table/cell/resource/link coordinate.** Those common
  selectors return the typed capability error (exit 6), asserted for each.
* **PDF `search-match` native provenance is the empty string** after the
  adapter tag (`format=pdf;common;`); PDF has no native search coordinate to
  name beyond the heuristic page text it searched.

## Exactness

`materialize --exact` reproduces every source byte-exactly
(length + SHA-256 + `cmp`), so the equivalence court does not trade exactness
for a common vocabulary.

## Assertions

```
PASS  pdf.ingest.field_present                 true
PASS  docx.ingest.field_present                true
PASS  epub.ingest.field_present                true
PASS  pdf.capabilities.format                  pdf
PASS  pdf.capabilities.native_selectors_present true
PASS  docx.capabilities.format                 docx
PASS  docx.capabilities.native_selectors_present true
PASS  epub.capabilities.format                 epub
PASS  epub.capabilities.native_selectors_present true
PASS  pdf.find.XF12A.hit                       contains
PASS  pdf.find.XF12A.native_provenance         format=pdf;common;
PASS  pdf.find.XF12B.hit                       contains
PASS  pdf.find.XF12B.native_provenance         format=pdf;common;
PASS  docx.find.XF12A.hit                      contains
PASS  docx.find.XF12A.native_provenance        format=docx;common;docx;story=main;part=/word/document.xml;profile=v1-final-result-h1-f1-e1-c1-x0-t1-b1
PASS  docx.find.XF12B.hit                      contains
PASS  docx.find.XF12B.native_provenance        format=docx;common;docx;story=main;part=/word/document.xml;profile=v1-final-result-h1-f1-e1-c1-x0-t1-b1
PASS  epub.find.XF12A.hit                      contains
PASS  epub.find.XF12A.native_provenance        format=epub;common;epub;search;profile=v1-linear-only-excluded-h0-s1
PASS  epub.find.XF12B.hit                      contains
PASS  epub.find.XF12B.native_provenance        format=epub;common;epub;search;profile=v1-linear-only-excluded-h0-s1
PASS  pdf.text.heading0                        contains
PASS  pdf.text.heading1                        contains
PASS  pdf.text.heading2                        contains
PASS  pdf.text.para0                           contains
PASS  pdf.text.para1                           contains
PASS  pdf.text.list3                           contains
PASS  pdf.text.cell_b7                         contains
PASS  pdf.text.native_provenance               format=pdf;common;pdf;pages=1
PASS  docx.text.heading0                       contains
PASS  docx.text.heading1                       contains
PASS  docx.text.heading2                       contains
PASS  docx.text.para0                          contains
PASS  docx.text.para1                          contains
PASS  docx.text.list3                          contains
PASS  docx.text.cell_b7                        contains
PASS  docx.text.native_provenance              format=docx;common;docx;story=main;part=/word/document.xml;profile=v1-final-result-h1-f1-e1-c1-x0-t1-b1
PASS  epub.text.heading0                       contains
PASS  epub.text.heading1                       contains
PASS  epub.text.heading2                       contains
PASS  epub.text.para0                          contains
PASS  epub.text.para1                          contains
PASS  epub.text.list3                          contains
PASS  epub.text.cell_b7                        contains
PASS  epub.text.native_provenance              format=epub;common;epub;spine-items=1;profile=v1-linear-only-excluded-h0-s1
PASS  pdf.metadata.native_provenance           format=pdf;common;pdf;document-metadata
PASS  docx.metadata.native_provenance          format=docx;common;docx;story=main;part=/word/document.xml
PASS  epub.metadata.native_provenance          format=epub;common;epub;package=OEBPS/package.opf
PASS  pdf.metadata.source_len                  905
PASS  pdf.metadata.source_sha256               fae3034c76c49e0f99af687e710147353b25c48bd6cd2acdb557ac9e174ebd07
PASS  docx.metadata.tables                     1
PASS  docx.metadata.resources                  1
PASS  docx.metadata.hyperlinks                 1
PASS  epub.metadata.package                    OEBPS/package.opf
PASS  epub.metadata.metadata_entries           3
PASS  docx.heading.0                           Introduction
PASS  docx.heading.0.native_provenance         contains
PASS  docx.heading.1                           Method
PASS  docx.heading.1.native_provenance         contains
PASS  docx.heading.2                           Findings
PASS  docx.heading.2.native_provenance         contains
PASS  docx.block0                              Introduction
PASS  docx.table0.b7                           contains
PASS  docx.cell_b7                             bravo-seven
PASS  docx.link.text                           external
PASS  docx.resource.rel                        rIdImg
PASS  epub.heading.0                           Introduction
PASS  epub.heading.0.native_provenance         contains
PASS  epub.heading.1                           Method
PASS  epub.heading.1.native_provenance         contains
PASS  epub.heading.2                           Findings
PASS  epub.heading.2.native_provenance         contains
PASS  epub.block0                              Introduction
PASS  epub.table0.b7                           contains
PASS  epub.cell_b7                             bravo-seven
PASS  epub.link.text                           external
PASS  epub.link.href                           https://example.com/phase12
PASS  epub.resource.href                       images/image1.png
PASS  epub.resource.media_type                 image/png
PASS  epub.resource.sha256                     823d5d6eeee43d3e74e9a1b636b902dccd533c668ce433adf689fd10af8e42ba
PASS  pdf.text.link_url                        contains
PASS  pdf.declined.heading                     exit=6
PASS  pdf.declined.block                       exit=6
PASS  pdf.declined.table                       exit=6
PASS  pdf.declined.cell                        exit=6
PASS  pdf.declined.resource                    exit=6
PASS  pdf.declined.link                        exit=6
PASS  pdf.materialize.sha256                   fae3034c76c49e0f99af687e710147353b25c48bd6cd2acdb557ac9e174ebd07
PASS  pdf.materialize.length                   905
PASS  pdf.materialize.cmp                      equal
PASS  docx.materialize.sha256                  18a55a0830da155f482a4b3790b3a1f2671474edfa7a5e48a59d8ddccb8a2fd9
PASS  docx.materialize.length                  3458
PASS  docx.materialize.cmp                     equal
PASS  epub.materialize.sha256                  88f8b9120c8914911abadf30e25ef890faba70bfae28c67bc2ba7dfa2bbc09be
PASS  epub.materialize.length                  2858
PASS  epub.materialize.cmp                     equal
```

---

## Environment (receipt)

- Commit under test: `dc4d5a34666246f421558763e73807709e204fbc` (branch `phase12`), dirty files: ` M tools/phase12-removal-court.sh; M tools/phase12-triplet-court.sh;?? evidence/campaigns/2026-10-06-phase12-removal-dc4d5a3/;?? evidence/campaigns/2026-10-06-phase12-triplet-dc4d5a3/;`
- Service image: `vole-document/db-baseline:1.99.0`; base `rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e`
- Toolchain: rustc `1.99.0`, cargo `1.99.0 (5f94df478 2026-08-27)`, arch `x86_64`
- `Cargo.lock` sha256: `710e2b53719f567587472f4deb99a361530aa247dfb48633442c4a0dd576de1e`
- Run (UTC): 2026-10-06T15:42:16Z
