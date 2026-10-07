# real100-v1 corpus — close-out and freeze

Branch: `real100-v1`. Frozen 2026-10-07 (`frozen_utc = 2026-10-07T08:37:41Z`).
Deliverable: [the corpus README](../../real100-v1/README.md).

## What this is

A frozen corpus of **100 real, public NASA and NIST documents** (PDF/DOCX/EPUB)
for exactness and size courts. It is not a codec claim and asserts no VOLE
result. The document binaries are gitignored; the manifest
(`real100-v1/manifest.{tsv,toml}`), `SHA256SUMS`, the deterministic selection
(`real100-v1/sources/real100_selection.tsv`) and the diversity/probe evidence are
committed. Every row carries a source URL, SHA-256 and byte length; `verify.sh`
re-checks all three against the bytes and re-fetches from the recorded URL.

## Final composition and diversity

```text
nasa pdf 40 | nasa epub 15 | nist pdf 20 | nist docx 15 | nist epub 10
NASA 55 / NIST 45 ; PDF 60 / DOCX 15 / EPUB 25
diversity: 66 PASS, 13 PARTIAL, 1 FAIL (of 80 gates)
```

All composition gates PASS; all six NASA-era gates PASS
(5 / 7 / 8 / 8 / 8 / 4); all `NASA PDF<->EPUB` (10), `NIST PDF<->EPUB` (5),
size large bands (50-100 MiB = 10, >100 MiB = 5, 10-50 MiB = 20) and the
`NIST DOCX image-heavy` gate (2) PASS.

## Close-out (all by pre-performance attributes only)

- **+2 NIST DOCX** (98 → 100). NIST-wide search (856 CSRC publication pages,
  the `www.nist.gov` sitemap, OWM handbook pages, `nvlpubs`, search) shows
  NIST's published Word bytes are forms/templates/deltas. The image-heavy class
  was sourced from NIST's public **CFReDS** (CFTT) dataset Word documents; the
  **long regulatory** class does not exist as published NIST Word bytes and is
  recorded as an over-constrained FAIL (Handbooks 44/130/133/105 are PDF-only).
- **Cross-format.** Re-selected the 5 SP PDFs to the families that also publish
  an agency EPUB (800-115/123/30r1/144) plus the DOCX-matched 800-18r2, and added
  the FIPS 140-2 EPUB → `NIST PDF<->EPUB` 5 PASS. `PDF<->DOCX` is 1 (PARTIAL);
  with SP fixed at 5 and only 5 SP families publishing DOCX, 8-10 is unreachable.
- **Size bands.** Rebuilt the NASA selection around a curated 30-report NTRS list
  (era/doctype/measured size); 5 pre-1960 NACA annual-report compilations and
  large TR/TM/CR/conference/SP reports put 10 files in 50-100 MiB and 5 in
  >100 MiB.
- **Era rebalance.** E-book PDFs 12 → 10 (2 with 2025 uploads, 8 with 2015-24),
  NTRS fills the older bands → all six era gates PASS.
- **Harness fix.** `acquire.py cmd_add_tsv` had its add call mis-indented after
  `continue` (unreachable dead code; `select-real100.sh` added 0 rows). De-indented
  to the loop body. No `src/`, `tests/`, `fuzz/` or `Cargo.*` change.

## Over-constrained quotas (recorded, not fabricated)

NIST DOCX `long regulatory` (FAIL); NIST `PDF<->DOCX` 8-10 (PARTIAL, max 5);
`glossary/index` DOCX (PARTIAL, one index-bearing Word file exists); size
`<100KiB` / `100KiB-1MiB` / `1-10MiB` (PARTIAL; exact targets sum to 95 with the
large bands); NASA `TR`/`handbook-ref`/`simple born-digital`/`appendix-ref`
(PARTIAL; technical-doctype minimums sum to 33 > the 30 non-e-book NASA PDFs).
See the [corpus README](../../real100-v1/README.md).

## Reproduce (Docker only)

```sh
docker compose run --rm --no-TTY realcorpus python3 tools/realcorpus/select-real100.py
docker compose run --rm --no-TTY realcorpus sh tools/realcorpus/select-real100.sh
docker compose run --rm --no-TTY realcorpus python3 tools/realcorpus/acquire.py retag --corpus real100-v1
docker compose run --rm --no-TTY realcorpus sh tools/realcorpus/verify.sh --diversity
```
