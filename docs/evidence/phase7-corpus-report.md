# Phase 7.0 — producer-stratified Flate corpus: correction-ratio result

Campaign receipt: `evidence/campaigns/2026-10-05-phase7-corpus-f1f8d26/`
(code commit `f1f8d26`). Corpus ledger: `evidence/corpus/phase7/provenance.json`;
raw measurement: `evidence/corpus/phase7/deflate-stats.jsonl`; generator:
`tools/pdf-corpus.sh`.

## What was measured

`vole-document deflate-stats` reports, **per `FlateDecode` stream**, the exact
DEFLATE replay (preflate-0.7.6) correction ratio:

- `correction / compressed`
- `(plaintext + correction) / compressed`
- `(rANS(plaintext) + correction) / compressed`

plus exact-replay / decline counts, and two document aggregates for the rANS
complete cost: `replayed_rans_full_bytes` (naive per-stream sum) and
`replayed_rans_dedup_bytes` (one rANS charge per **unique** plaintext + one
charge per **unique** correction blob, mirroring the shared-channel
`PDF_DEFLATE_REPLAY_RANS` candidate). This is diagnostics only: it changes no
wire format and no candidate behavior.

## Corpus (locally generated, no third-party bytes)

23 files: 11 producer variants + 11 Phase-3 synthetic PDFs + a non-PDF control.
Producers: **Ghostscript 10.00.0** (`/default`, `/prepress`, `/printer`,
`/ebook`, `/screen`), **qpdf 11.3.0** (compress / linearize /
object-streams=preserve / nocompress control), a **hand-written stored-block-zlib
base**, and `pdf-make-samples`. Every produced PDF passes `qpdf --check`.

**Scope caveat:** qpdf and Ghostscript are *transformers*, not authoring
applications. Two transformer lineages plus a hand writer is **not** a
producer-stratified survey. Browser/PDFium, LibreOffice, pdfTeX and Adobe
outputs could not be obtained lawfully here and are a recorded **gap**, not
substituted.

## Result

17 `FlateDecode` streams seen; **11 replayed, 6 declined → exact-replay
acceptance 11/17 = 0.647**.

Per producer (p10/p50/p90 over replayed streams):

| producer | flate | replayed | declined | acceptance | `corr/comp` p10/p50/p90 | `(plain+corr)/comp` | `(rans+corr)/comp` |
| --- | ---: | ---: | ---: | ---: | --- | --- | --- |
| ghostscript | 6 | 1 | 5 | 0.167 | 0.0147 / 0.0147 / 0.0147 | 1.140 / 1.140 / 1.140 | 1.234 / 1.234 / 1.234 |
| hand | 3 | 3 | 0 | 1.000 | 0.00032 / 0.00032 / 0.00043 | 1.0001 / 1.0001 / 1.0002 | 0.6132 / 0.6132 / 0.6132 |
| qpdf | 2 | 1 | 1 | 0.500 | 0.609 / 0.609 / 0.609 | 2.130 / 2.130 / 2.130 | 2.696 / 2.696 / 2.696 |
| pdf-make-samples | 6 | 6 | 0 | 1.000 | 0.000875 / 0.004518 / 0.097360 | 1.0005 / 4.8399 / 9.4777 | 0.5778 / 2.1890 / 5.5114 |

- **The Phase-6 win region appears** on the hand-written shared-plaintext
  fixtures. `hand-base2.pdf` has two streams sharing one 90,541-byte plaintext,
  both weakly coded: naive rANS complete cost 111,062 B vs **deduped 55,531 B**.
  `_synthetic/flate.pdf` shares `p1` across four streams with one stored
  appearance: naive 89,437 B vs **deduped 34,051 B**.
- **Replay loses** where fixed correction overhead cannot amortize (a 46-byte
  qpdf-linearize stream: `raw 0.609`, `plain 2.13`, `rans 2.70`), and per stream
  on unique strongly-compressed plaintext (`flate.pdf` objects 4/6/7/9:
  `rans` 2.19–5.51). Only the conjunctive shared-*and*-weak geometry wins at
  document level.

## Harness-locality finding

All 6 declines are `not_zlib`, but that is not a claim that the streams are not
zlib: independent `qpdf --show-object --raw-stream-data` reads show `78 9c`
(valid zlib) for `gs-default` object 5 and `qpdf-compress` object 1. The VOLE
lexer's `find_endstream` requires an EOL immediately before `endstream`, which
Ghostscript omits (spec "should", qpdf-tolerated), so the scanner mislocates the
stream span and the sliced bytes begin with `0x0a`. This is a **scanner locality
limitation** in the harness, recorded as a finding. Fixing it would change
candidate proposals, so it is out of scope for Phase 7.0. The Phase-6 fixtures
(direct `/Length`, EOL before `endstream`) are unaffected.

Consequently this campaign is evidence about the **harness**, not evidence of
absence of the win region on real producers; that question stays open.

## Verdict

**RECORDED (measurement).** No new candidate is adopted. Claims are scoped to
locally generated files; qpdf/Ghostscript are transformers, not authoring apps.
