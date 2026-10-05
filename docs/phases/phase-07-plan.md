# Phase 7 — hardening, then nested content proceduralization

Branch: `phase7`. Base: `main` @ `186dc49` (`v0.1.0-alpha.8`).
Status: **IN PROGRESS**.

## Why

Phase 6 shipped the first measured positive (exact DEFLATE replay of a shared,
entropy-coded plaintext beats `BYTE_RANS` on `flate.pdf`). Two honest gaps were
recorded and must be closed before Phase 7's headline work:

1. **The Phase-6 win is one composed synthetic fixture.** The decisive unknown is
   whether the win region appears on PDFs produced by *real* PDF producers, and
   the number to watch is `correction_bytes / original_deflate_bytes` — how much
   of a producer's compressed bitstream is *predictable* from the plaintext and
   the preflate model.
2. **No libFuzzer/`cargo-fuzz` targets.** The current fuzzing surface is
   deterministic property/mutation tests plus `tools/soak-fuzz.sh`; the hostile
   parsers (`.voldoc` records, DRA, rANS, PDF lexer/scanner, replay wrapper)
   should be fuzzed by a coverage-guided harness.

## Subphases

- **7.0 — Producer-stratified Flate corpus + correction-ratio harness.**
  Build a corpus from genuinely distinct **generator families**, not just the
  producers already pinned locally. Target families (all with recorded provenance —
  producer, version, exact command, SHA-256 — and locally regenerated, not
  committed):
  - **locally available in the pinned `tools` image**: qpdf 11.3.0 and
    Ghostscript 10.00.0, plus the existing synthetic samples. Honesty about the
    limits of this: two producer lineages is *not* a producer-stratified survey,
    and the report must say so.
  - **to be acquired/located where possible**: browser/PDFium output, LibreOffice
    (writer/impress export), TeX (pdfTeX/`dvipdfmx`), Cairo/ReportLab, and any
    office/Adobe-derived samples that can be lawfully located. Each new family is
    admitted only with a committed provenance ledger entry; where a family cannot
    be obtained, the report records the gap rather than substituting synthetic
    stand-ins.
  Add a `vole-document deflate-stats` command that reports **per FlateDecode stream**
  and in aggregate: `compressed_bytes`, `plaintext_bytes`, `correction_bytes`,
  `rANS_plaintext_bytes`, and the ratios `correction/compressed`,
  `(plaintext+corr)/compressed`, `(rANS(plaintext)+corr)/compressed`, plus
  exact-replay/decline counts. The report must give **stream-level distributions**
  (p10/p50/p90 of `correction/compressed`, exact-replay acceptance rate, and the
  complete-cost win rate) **broken out by producer**, not only document averages —
  so that a single large stream cannot hide a family-wide decline. Seal a campaign
  receipt and write a scoped report.
- **7.1 — Coverage-guided fuzzing.** Add a `fuzz/` `cargo-fuzz` crate and a
  pinned-nightly `fuzz` compose service; targets for the `.voldoc`
  header/record parser, DRA decode/eval, rANS model + channel decode, PDF
  lexer/physical scanner, xref/revision parsing, the `DEFLATE_REPLAY` wrapper,
  and the materializer. Run a bounded campaign, record coverage, add minimized
  regression fixtures for anything found.
- **7.2+ — Nested PDF content proceduralization.** For selected safely-decoded
  content streams: tokenize operators/operands/resources, separate typed
  channels, reconstruct the logical stream byte-exactly, then replay the exact
  DEFLATE on top. The clearest embodiment of the thesis.

## Gates (unchanged)

- Every exact court requires length + SHA-256 + byte compare.
- Complete-cost court decides; losses and declines recorded verbatim.
- Docker only; one `phaseN` branch; commit + push per subphase; the phase branch
  is deleted after merge (tags are the durable record).
- Claims stay measured, scoped, and falsifiable; a skeptic tries to falsify each
  phase's headline.

## Corpus licensing note

The corpus is **locally generated** by pinned tools from our own deterministic
inputs; no third-party document bytes are committed. Provenance (producer,
version, exact command, SHA-256) is committed as a ledger; the regenerable
`.pdf` bytes are gitignored.
