# real20-pilot — development-only pilot

**Development-only. Never part of the frozen 100.** These 20 documents exist to
shake out producer quirks and harness bugs before the frozen corpus is fixed.
Selecting a pilot document into the frozen 100 does **not** reuse this row: the
document is acquired again under `real100-v1/` with its own id and hash.

```text
nasa/pdf  6   nasa/epub  4   nist/pdf  6   nist/docx  2   nist/epub  2
```

The pilot already surfaced one real quirk: several NTRS legacy scans carry a
short archive wrapper that embeds the original filename, so `%PDF-` is **not** at
byte 0. The format probe therefore locates the PDF header within the first 1024
bytes (as the PDF spec permits) instead of assuming offset 0 — without this, a
valid 202-page scan would be rejected as "not a PDF".

It also spans the five cells and the structural categories: NACA/early scans,
1960s–2010s technical publications, an e-book EPUB set (multi-chapter,
image-heavy, simple guide), NIST SP / NISTIR / TN / Handbook / FIPS PDFs, NIST
CSRC Word supporting files, and the legacy CSRC EPUB e-books. Two genuine
agency-published cross-format families are present (NIST SP 800-18r2
PDF↔DOCX, NIST SP 800-115 PDF↔EPUB).

## Files

- `manifest.tsv` / `manifest.toml` — the 20 rows (same 20-field schema as the
  frozen corpus).
- `SHA256SUMS` — hashes of the committed pilot bytes.
- `documents/{nasa/{pdf,epub},nist/{pdf,docx,epub}}/` — the committed bytes.

## Reproduce / verify

```sh
docker compose run --rm --no-TTY realcorpus sh tools/realcorpus/select-pilot.sh
docker compose run --rm --no-TTY realcorpus sh tools/realcorpus/verify.sh --corpus real100-v1/pilot
docker compose run --rm --no-TTY realcorpus \
  python3 tools/realcorpus/check-diversity.py --corpus real100-v1/pilot
```

The pilot's own compliance table is in `../sources/diversity-pilot.txt`; it is
informational only (20 documents cannot satisfy the 100-document gates).
