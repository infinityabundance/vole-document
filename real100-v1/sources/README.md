# sources — candidate catalogs, frozen selection, and evidence

Everything in this directory is either a **candidate pool** (discovered,
read-only), the **frozen selection**, or **measured evidence**. No document
bytes live here.

| File | What it is | How it is produced |
|---|---|---|
| `nasa_ebook_assets.tsv` | NASA official e-book assets: `kind page_title landing_page asset_url` for every `.pdf`/`.epub` on a NASA e-book landing page (67 EPUBs, 793 PDFs). | `tools/realcorpus/discover-nasa-ebooks.py` (nasa.gov WP search API + page scrape) |
| `nasa_ntrs_candidates.tsv` | NTRS technical-publication candidates: `id year sti_type center report_number title pdf_url landing_page` (550 rows across eras/centers/types). | `tools/realcorpus/discover-ntrs.py` |
| `real100_selection.tsv` | **The frozen selection.** One row per selected document with its pre-performance metadata, cross-format and revision family ids. | `tools/realcorpus/select-real100.py` (deterministic) |
| `probe-pilot.tsv` | Byte-probe facts for the pilot documents (pages, text chars, image/font inventory, object streams, DOCX tables/headings/lists, EPUB spine/media). | `tools/realcorpus/probe.py` |
| `diversity-real100.txt` | The 80-gate compliance table for the frozen corpus. | `tools/realcorpus/check-diversity.py --corpus real100-v1` |
| `diversity-pilot.txt` | The same table for the development pilot (informational). | `tools/realcorpus/check-diversity.py --corpus real100-v1/pilot` |

## Unreachable / substituted sources

Selection is by pre-performance attributes; a source that turns out to be
unreachable is recorded and skipped, never faked. One substitution was made in
this subphase for an **unreachable** source (not a loss-based substitution):

- NIST SP 800-124 Rev. 1 EPUB (`.../nistpubs/800-124-rev1/sp800_124_r1.epub`) is
  404 on `csrc.nist.gov` (it survives only on the legacy `csrc.nist.rip`
  mirror). The slot was filled with NIST SP 800-30 Rev. 1 EPUB, which is served
  by the canonical host.

## Discovery is read-only

The discovery scripts only enumerate candidate URLs. Recording a document — the
download, byte-format verification, SHA-256, byte length and manifest row — is a
separate, deliberate step run through `tools/realcorpus/acquire.py`.
