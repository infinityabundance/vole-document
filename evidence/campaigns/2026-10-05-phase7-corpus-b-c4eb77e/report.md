# Campaign: 2026-10-05-phase7-corpus-b-c4eb77e — Phase 7.0 amendment: corpus re-measured after the stream-boundary fix

This receipt **amends** `2026-10-05-phase7-corpus-f1f8d26` (do not rewrite it).
It re-runs the *same* generator and the *same* `deflate-stats` harness over the
*same* corpus after the Stage-A code fix, and records the corrected numbers. The
code measured is commit `c4eb77e6` ("lexer stream-end finds `endstream` without
requiring a preceding EOL (real-producer case)").

## What changed in the code (scope)

`src/adapter/pdf/lexer.rs::find_endstream` previously required the byte before
`endstream` to be CR or LF. Real producers — confirmed for **Ghostscript
10.00.0** — write the stream payload immediately followed by `endstream` with no
intervening EOL, so the opaque payload span over-read to a *later* `endstream`,
swallowing subsequent structure. The physical scanner then could not resolve the
correct `/Length` and mis-sliced the stream (the captured bytes began with `0x0a`
and `try_replay` declined them as `not_zlib`), even though `qpdf
--raw-stream-data` shows the stream begins `78 9c`.

The fix locates the `endstream` *keyword* by right-termination: the byte
immediately after `endstream` must be PDF whitespace, a PDF delimiter, or EOF;
the byte before may be anything. The physical scanner's `/Length`-based
resolution is unchanged. No wire format, candidate, or decode path changed; the
harness remains diagnostics-only.

## Method

`tools/pdf-corpus.sh` (pinned `tools` image: qpdf 11.3.0, Ghostscript 10.00.0)
writes a hand-built classic-xref base PDF whose `/FlateDecode` payloads are
hand-built **stored-block zlib** streams, then re-produces it through distinct
producer lineages:

- **Ghostscript** `pdfwrite` at `/default`, `/prepress`, `/printer`, `/ebook`,
  `/screen`;
- **qpdf** `--compress-streams=y --object-streams=generate
  --stream-data=compress`, `--linearize`, `--object-streams=preserve`, and a
  `--compress-streams=n --stream-data=uncompress` control;
- the **Phase-3 synthetic set** (`pdf-make-samples`), including the real-zlib
  `flate.pdf` fixture.

Every produced PDF passes the independent `qpdf --check` oracle; every file's
producer, version, exact command, SHA-256 and `license:"locally-generated"` is
in `evidence/corpus/phase7/provenance.json`. `vole-document deflate-stats`
measures per `FlateDecode` stream: `correction/compressed`,
`(plaintext+correction)/compressed`, `(rANS(plaintext)+correction)/compressed`,
`replayed`/`declined`, and the two aggregate complete costs
`replayed_rans_full_bytes` (naive per-stream sum) and
`replayed_rans_dedup_bytes` (one rANS charge per unique plaintext + one per
unique correction).

**Reproducibility caveat:** the transformer outputs (Ghostscript, qpdf) embed
per-run identifiers/timestamps, so their SHA-256 differs between runs even
though the byte length, object structure, and every `FlateDecode`
`compressed_bytes` are stable. The hand-written and synthetic fixtures are
byte-identical across runs.

## Result: old vs new

| metric | old (`f1f8d26`) | new (`c4eb77e`) |
| --- | ---: | ---: |
| `FlateDecode` streams seen | 17 | 24 |
| replayed | 11 | 24 |
| declined | 6 | 0 |
| acceptance | 0.647 | **1.000** |

The stream count rose from 17 to 24 (not merely 17→17 with 6 admitted): the old
over-read swallowed whole stream objects, so `gs-prepress` (+1) and every
qpdf-generated file (`qpdf-compress` +2, `qpdf-linearize` +2,
`qpdf-preserve-objectstreams` +2, previously **zero** streams) were undercounted.
All 24 streams now replay byte-exactly; the 6 former `not_zlib` declines are
gone.

## Per-producer distributions (post-fix)

p10/p50/p90 are nearest-rank over **replayed** streams (all streams here);
`acceptance = replayed / flate`. `raw` = `correction/compressed`.

| producer | flate | replayed | declined | acceptance | raw p10/p50/p90 | plain p10/p50/p90 | rans p10/p50/p90 |
| --- | ---: | ---: | ---: | ---: | --- | --- | --- |
| ghostscript | 7 | 7 | 0 | 1.000 | 0.014716 / 0.080827 / 0.080827 | 1.139985 / 14.135338 / 14.135338 | 1.234027 / 8.934210 / 8.934210 |
| hand (VOLE shell) | 3 | 3 | 0 | 1.000 | 0.000320 / 0.000320 / 0.000428 | 1.000143 / 1.000143 / 1.000192 | 0.613162 / 0.613215 / 0.613215 |
| qpdf | 8 | 8 | 0 | 1.000 | 0.000320 / 0.000428 / 0.608695 | 1.000143 / 1.000192 / 12.464622 | 0.613162 / 0.613215 / 7.670047 |
| pdf-make-samples | 6 | 6 | 0 | 1.000 | 0.000875 / 0.004518 / 0.097360 | 1.000531 / 4.839944 / 9.477712 | 0.577848 / 2.189030 / 5.511436 |
| **overall** | 24 | 24 | 0 | 1.000 | 0.000320 / 0.014716 / 0.097360 | 1.000143 / 1.692737 / 14.135338 | 0.613162 / 2.000000 / 8.934210 |

## Per-file table

| file | producer | flate | replayed | declined | compressed | correction | rans_full | rans_dedup |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| gs-default.pdf | ghostscript | 1 | 1 | 0 | 532 | 43 | 4753 | 4753 |
| gs-ebook.pdf | ghostscript | 1 | 1 | 0 | 532 | 43 | 4753 | 4753 |
| gs-prepress.pdf | ghostscript | 2 | 2 | 0 | 3318 | 84 | 8191 | 8191 |
| gs-printer.pdf | ghostscript | 2 | 2 | 0 | 3324 | 84 | 8198 | 8198 |
| gs-screen.pdf | ghostscript | 1 | 1 | 0 | 532 | 43 | 4753 | 4753 |
| hand-base1.pdf | hand | 1 | 1 | 0 | 67633 | 29 | 41470 | 41470 |
| hand-base2.pdf | hand | 2 | 2 | 0 | 181114 | 58 | 111062 | **55531** |
| qpdf-compress.pdf | qpdf | 3 | 3 | 0 | 72052 | 346 | 74349 | 74349 |
| qpdf-linearize.pdf | qpdf | 3 | 3 | 0 | 71919 | 347 | 74115 | 74115 |
| qpdf-nocompress.pdf | qpdf | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| qpdf-preserve-objectstreams.pdf | qpdf | 2 | 2 | 0 | 181114 | 58 | 111062 | **55531** |
| _synthetic/flate.pdf | pdf-make-samples | 6 | 6 | 0 | 56475 | 639 | 89437 | **34051** |
| (10 other synthetic PDFs) | pdf-make-samples | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

## Per-stream table

| producer | file | obj | compressed | replayed | decline | raw | plain | rans |
| --- | --- | ---: | ---: | --- | --- | ---: | ---: | ---: |
| ghostscript | gs-default | 5 | 532 | true | — | 0.080827 | 14.135338 | 8.934210 |
| ghostscript | gs-ebook | 5 | 532 | true | — | 0.080827 | 14.135338 | 8.934210 |
| ghostscript | gs-prepress | 5 | 532 | true | — | 0.080827 | 14.135338 | 8.934210 |
| ghostscript | gs-prepress | 10 | 2786 | true | — | 0.014716 | 1.139985 | 1.234027 |
| ghostscript | gs-printer | 5 | 538 | true | — | 0.079925 | 13.994423 | 8.847583 |
| ghostscript | gs-printer | 13 | 2786 | true | — | 0.014716 | 1.139985 | 1.234027 |
| ghostscript | gs-screen | 5 | 532 | true | — | 0.080827 | 14.135338 | 8.934210 |
| hand | hand-base1 | 6 | 67633 | true | — | 0.000428 | 1.000192 | 0.613162 |
| hand | hand-base2 | 5 | 90557 | true | — | 0.000320 | 1.000143 | 0.613215 |
| hand | hand-base2 | 6 | 90557 | true | — | 0.000320 | 1.000143 | 0.613215 |
| qpdf | qpdf-compress | 1 | 179 | true | — | 0.150837 | 1.692737 | 2.000000 |
| qpdf | qpdf-compress | 6 | 4240 | true | — | 0.068396 | 12.464622 | 7.670047 |
| qpdf | qpdf-compress | 7 | 67633 | true | — | 0.000428 | 1.000192 | 0.613162 |
| qpdf | qpdf-linearize | 4 | 46 | true | — | 0.608695 | 2.130434 | 2.695652 |
| qpdf | qpdf-linearize | 7 | 4240 | true | — | 0.068396 | 12.464622 | 7.670047 |
| qpdf | qpdf-linearize | 8 | 67633 | true | — | 0.000428 | 1.000192 | 0.613162 |
| qpdf | qpdf-preserve-objectstreams | 4 | 90557 | true | — | 0.000320 | 1.000143 | 0.613215 |
| qpdf | qpdf-preserve-objectstreams | 5 | 90557 | true | — | 0.000320 | 1.000143 | 0.613215 |
| pdf-make-samples | flate.pdf | 4 | 3410 | true | — | 0.097360 | 9.477712 | 5.511436 |
| pdf-make-samples | flate.pdf | 6 | 3768 | true | — | 0.029458 | 8.518577 | 4.929140 |
| pdf-make-samples | flate.pdf | 7 | 2899 | true | — | 0.038634 | 4.839944 | 2.189030 |
| pdf-make-samples | flate.pdf | 8 | 31998 | true | — | 0.000875 | 1.000531 | 0.577848 |
| pdf-make-samples | flate.pdf | 9 | 6197 | true | — | 0.004518 | 5.166209 | 2.983701 |
| pdf-make-samples | flate.pdf | 10 | 8203 | true | — | 0.003413 | 1.002072 | 1.065951 |

No declines remain; every stream's correction blob is strictly smaller than its
compressed stream (max `raw` = 0.608695 on a 46-byte qpdf-linearize stream).

## Does the Phase-6 win region appear on producer output?

The Phase-6 win region is a plaintext **shared across streams** that also has a
**large/weakly-coded** appearance, where the deduplicated shared-channel cost is
strictly below the naive per-stream sum. After the fix it now appears on a
**transformer producer output**, not only on the hand-written/synthetic
fixtures:

- `qpdf-preserve-objectstreams.pdf` (producer **qpdf 11.3.0**): two streams
  (objects 4 and 5) share one 90,541-byte plaintext, both stored/weakly coded.
  Naive `replayed_rans_full_bytes` **111,062 B** vs deduped
  `replayed_rans_dedup_bytes` **55,531 B**. Before the fix this file reported
  **zero streams**.
- `hand-base2.pdf` (hand writer): same geometry, 111,062 → **55,531 B**.
- `_synthetic/flate.pdf`: `p1` shared across four streams with one stored
  appearance, 89,437 → **34,051 B**.

This is scoped: the qpdf win is the Phase-3/hand shared-plaintext geometry
*surviving a qpdf `--object-streams=preserve` transform*, not an authoring-app
result. No Ghostscript output contains a shared-plaintext pair (`gs-prepress`
object 5 and object 10 have distinct plaintexts), so no win region appears
there. The deduplicated figures are diagnostics for the shared-channel
candidate; no new candidate is adopted.

## Where replay still loses per stream

`raw = correction/compressed` stays tiny for weak/stored appearances
(0.0003–0.08) but the *complete* `(rans+corr)/compressed` ratio exceeds 1 for
unique, strongly-compressed appearances (up to **8.93** on the 532-byte
Ghostscript content streams, whose plaintext is 7,477 B). Replay only wins at
document level under the conjunctive shared-*and*-weak geometry above.

## Scope and verdict

- Exactness/wire format: no wire format, candidate, or decode path changed; the
  harness is diagnostics-only. The only code change is the Stage-A lexer
  `find_endstream` right-termination rule.
- Licensing: every byte is locally generated by pinned tools from our own
  deterministic inputs; no third-party bytes. qpdf/Ghostscript are
  transformers, not authoring apps.
- Producers not present (browsers/PDFium, LibreOffice, pdfTeX, Adobe) remain a
  recorded **gap**, unchanged from the original receipt.

**Verdict: RECORDED (amendment).** The stream-boundary fix removes all 6
`not_zlib` declines and corrects the stream census (17 → 24); the Phase-6 win
geometry now appears on qpdf transformer output as well as the hand/synthetic
fixtures. No new candidate is adopted.
