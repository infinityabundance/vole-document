# Phase 7.0 — producer-stratified Flate corpus: correction-ratio result

> **Amended 2026-10-05.** The original measurement exposed a scanner-locality
> limitation (below) that suppressed 6 real Flate streams. After the Stage-A
> lexer fix (`find_endstream` locates `endstream` by right-termination, commit
> `c4eb77e`), the corpus was re-measured. The **corrected** numbers are below;
> the pre-fix numbers are retained for the record.
>
> - Original receipt: `evidence/campaigns/2026-10-05-phase7-corpus-f1f8d26/`
>   (code commit `f1f8d26`).
> - Amendment receipt: `evidence/campaigns/2026-10-05-phase7-corpus-b-c4eb77e/`
>   (code commit `c4eb77e`).
>
> Corpus ledger: `evidence/corpus/phase7/provenance.json`; raw measurement:
> `evidence/corpus/phase7/deflate-stats.jsonl`; generator: `tools/pdf-corpus.sh`.

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

After the fix: 24 `FlateDecode` streams seen; **24 replayed, 0 declined →
exact-replay acceptance 24/24 = 1.000**.

The earlier, pre-fix run saw only 17 streams (11 replayed, 6 declined;
acceptance 0.647). The count rose because the over-reading payload span had
swallowed whole stream objects: `gs-prepress` gained 1, and every qpdf-generated
file was undercounted (`qpdf-compress` +2, `qpdf-linearize` +2,
`qpdf-preserve-objectstreams` +2 — it had reported **zero** streams). All 6
former `not_zlib` declines now replay byte-exactly.

Per producer (p10/p50/p90 over replayed streams; nearest rank):

| producer | flate | replayed | declined | acceptance | `corr/comp` p10/p50/p90 | `(plain+corr)/comp` | `(rans+corr)/comp` |
| --- | ---: | ---: | ---: | ---: | --- | --- | --- |
| ghostscript | 7 | 7 | 0 | 1.000 | 0.014716 / 0.080827 / 0.080827 | 1.139985 / 14.135338 / 14.135338 | 1.234027 / 8.934210 / 8.934210 |
| hand | 3 | 3 | 0 | 1.000 | 0.000320 / 0.000320 / 0.000428 | 1.000143 / 1.000143 / 1.000192 | 0.613162 / 0.613215 / 0.613215 |
| qpdf | 8 | 8 | 0 | 1.000 | 0.000320 / 0.000428 / 0.608695 | 1.000143 / 1.000192 / 12.464622 | 0.613162 / 0.613215 / 7.670047 |
| pdf-make-samples | 6 | 6 | 0 | 1.000 | 0.000875 / 0.004518 / 0.097360 | 1.000531 / 4.839944 / 9.477712 | 0.577848 / 2.189030 / 5.511436 |
| **overall** | 24 | 24 | 0 | 1.000 | 0.000320 / 0.014716 / 0.097360 | 1.000143 / 1.692737 / 14.135338 | 0.613162 / 2.000000 / 8.934210 |

- **The Phase-6 win region appears** on the hand-written shared-plaintext
  fixtures *and now also on qpdf transformer output*. `qpdf-preserve-objectstreams.pdf`
  and `hand-base2.pdf` each have two streams sharing one 90,541-byte plaintext,
  both weakly coded: naive rANS complete cost 111,062 B vs **deduped 55,531 B**.
  `_synthetic/flate.pdf` shares `p1` across four streams with one stored
  appearance: naive 89,437 B vs **deduped 34,051 B**. This is scoped: the qpdf
  win is the hand shared-plaintext geometry surviving a qpdf
  `--object-streams=preserve` transform, not an authoring-app result. No
  Ghostscript output contains a shared-plaintext pair.
- **Replay loses** per stream where the correction/plaintext cannot amortize: a
  46-byte qpdf-linearize stream (`raw 0.609`, `plain 2.13`, `rans 2.70`), and
  unique strongly-compressed plaintext (Ghostscript content streams up to
  `rans 8.93`; `flate.pdf` objects 4/6/7/9 `rans` 2.19–5.51). Only the
  conjunctive shared-*and*-weak geometry wins at document level.

## Harness-locality finding (resolved)

The original run reported all 6 declines as `not_zlib`, which was **not** a
statement that the streams are not zlib: independent
`qpdf --show-object=N --raw-stream-data` reads showed `gs-default` object 5 and
`qpdf-compress` object 1 begin `78 9c`, a valid zlib header. Root cause: the VOLE
lexer's `find_endstream` required a preceding EOL, and Ghostscript (and qpdf)
emit `endstream` with no EOL immediately before it, so the scanner over-read and
sliced bytes beginning with `0x0a`.

**This is now fixed** (commit `c4eb77e`): `find_endstream` locates the `endstream`
keyword by right-termination — the byte after `endstream` must be PDF whitespace,
a PDF delimiter, or EOF; the byte before may be any payload byte. The physical
scanner's `/Length`-based resolution is unchanged. Re-measurement
(`2026-10-05-phase7-corpus-b-c4eb77e`) shows **0 declines**; the 6 former
`not_zlib` streams replay byte-exactly, and the corrected stream census is 24.

Because the transformer outputs now measure cleanly, the corpus is evidence
about the harness **and** the producers, though the scope caveat stands: qpdf and
Ghostscript are transformers, not authoring applications, and browser/PDFium,
LibreOffice, pdfTeX and Adobe outputs remain a recorded gap.

## Verdict

**RECORDED (measurement, amended).** No new candidate is adopted. After the
stream-boundary fix the same corpus yields acceptance 24/24 = 1.000 (was
11/17 = 0.647), and the Phase-6 win geometry now appears on qpdf transformer
output as well as the hand/synthetic fixtures. Claims are scoped to locally
generated files; qpdf/Ghostscript are transformers, not authoring apps.
