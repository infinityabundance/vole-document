# Campaign: 2026-10-05-phase7-corpus-f1f8d26 — Phase 7.0 producer-stratified Flate corpus

## Method

`tools/pdf-corpus.sh` (pinned `tools` image: qpdf 11.3.0, Ghostscript 10.00.0)
writes a hand-built classic-xref base PDF whose `/FlateDecode` payloads are
hand-built **stored-block zlib** streams, then re-produces that base through
distinct producer lineages:

- **Ghostscript** `pdfwrite` at `/default`, `/prepress`, `/printer`, `/ebook`,
  `/screen`;
- **qpdf** `--compress-streams=y --object-streams=generate
  --stream-data=compress`, `--linearize`, `--object-streams=preserve`, and a
  `--compress-streams=n --stream-data=uncompress` control;
- the **Phase-3 synthetic set** (`pdf-make-samples`), including the real-zlib
  `flate.pdf` fixture.

Every produced PDF is checked with the independent `qpdf --check` oracle; every
file's producer, producer version, exact command, SHA-256 and
`license:"locally-generated"` is recorded in
`evidence/corpus/phase7/provenance.json`. No third-party document bytes are used;
the regenerable `.pdf` bytes are gitignored.

`vole-document deflate-stats` then measures, **per `FlateDecode` stream**, the
exact-replay correction ratio: `correction/compressed`,
`(plaintext+correction)/compressed`, and `(rans(plaintext)+correction)/compressed`,
plus `replayed`/`declined`. Aggregate rANS complete cost is reported two ways, so
shared plaintext is not overcounted: `replayed_rans_full_bytes` (naive per-stream
sum) and `replayed_rans_dedup_bytes` (one rANS charge per unique plaintext + one
charge per unique correction blob, mirroring the shared-channel
`PDF_DEFLATE_REPLAY_RANS` candidate). These are diagnostics; the command changes
no wire format and no candidate behavior.

## Corpus

23 files: 11 producer variants produced by the script + 11 Phase-3 synthetic
PDFs + the `notpdf.bin` negative control. Producer groups: `ghostscript`,
`qpdf`, the hand VOLE shell writer, and `vole-document pdf-make-samples`.
**Scope warning:** qpdf and Ghostscript are *transformers*, not authoring
applications; two transformer lineages plus a hand writer is **not** a
producer-stratified survey of real authoring apps (browsers/PDFium,
LibreOffice, pdfTeX, Adobe). Those families could not be obtained lawfully here
and are recorded as a **gap**, not substituted.

## Per-file table

`flate` = `FlateDecode` streams seen; bytes are stream bytes (`data_len`) and
diagnostic aggregate costs. Only `_synthetic/flate.pdf` and the hand-built files
contribute replayed streams.

| file | producer | flate | replayed | declined | compressed | correction | rans_full | rans_dedup |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| gs-default.pdf | ghostscript | 1 | 0 | 1 | 2183 | 0 | 0 | 0 |
| gs-ebook.pdf | ghostscript | 1 | 0 | 1 | 2183 | 0 | 0 | 0 |
| gs-prepress.pdf | ghostscript | 1 | 0 | 1 | 4482 | 0 | 0 | 0 |
| gs-printer.pdf | ghostscript | 2 | 1 | 1 | 6133 | 41 | 3438 | 3438 |
| gs-screen.pdf | ghostscript | 1 | 0 | 1 | 2183 | 0 | 0 | 0 |
| hand-base1.pdf | hand | 1 | 1 | 0 | 67633 | 29 | 41470 | 41470 |
| hand-base2.pdf | hand | 2 | 2 | 0 | 181114 | 58 | 111062 | 55531 |
| qpdf-compress.pdf | qpdf | 1 | 0 | 1 | 72474 | 0 | 0 | 0 |
| qpdf-linearize.pdf | qpdf | 1 | 1 | 0 | 46 | 28 | 124 | 124 |
| qpdf-nocompress.pdf | qpdf | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| qpdf-preserve-objectstreams.pdf | qpdf | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| _synthetic/flate.pdf | pdf-make-samples | 6 | 6 | 0 | 56475 | 639 | 89437 | 34051 |
| (10 other synthetic PDFs) | pdf-make-samples | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

Totals: **17 Flate streams, 11 replayed, 6 declined → exact-replay acceptance
11/17 = 0.647.**

## Per-producer distributions

p10/p50/p90 are nearest-rank over **replayed** streams only (undefined ratios for
declines are excluded). `acceptance = replayed / flate`.

| producer | flate | replayed | declined | acceptance | raw p10/p50/p90 | plain p10/p50/p90 | rans p10/p50/p90 |
| --- | ---: | ---: | ---: | ---: | --- | --- | --- |
| ghostscript | 6 | 1 | 5 | 0.167 | 0.0147 / 0.0147 / 0.0147 | 1.140 / 1.140 / 1.140 | 1.234 / 1.234 / 1.234 |
| hand (VOLE shell) | 3 | 3 | 0 | 1.000 | 0.00032 / 0.00032 / 0.00043 | 1.0001 / 1.0001 / 1.0002 | 0.6132 / 0.6132 / 0.6132 |
| qpdf | 2 | 1 | 1 | 0.500 | 0.609 / 0.609 / 0.609 | 2.130 / 2.130 / 2.130 | 2.696 / 2.696 / 2.696 |
| pdf-make-samples | 6 | 6 | 0 | 1.000 | 0.000875 / 0.004518 / 0.097360 | 1.0005 / 4.8399 / 9.4777 | 0.5778 / 2.1890 / 5.5114 |
| **overall** | 17 | 11 | 6 | 0.647 | 0.00032 / 0.004518 / 0.097360 | 1.0001 / 1.140 / 8.519 | 0.6132 / 1.234 / 4.929 |

`raw` = `correction/compressed`; `plain` = `(plaintext+correction)/compressed`;
`rans` = `(rans(plaintext)+correction)/compressed`.

## Per-stream table

| producer | file | obj | compressed | replayed | decline | raw | plain | rans |
| --- | --- | ---: | ---: | --- | --- | ---: | ---: | ---: |
| ghostscript | gs-default | 5 | 2183 | false | not_zlib | — | — | — |
| ghostscript | gs-ebook | 5 | 2183 | false | not_zlib | — | — | — |
| ghostscript | gs-prepress | 5 | 4482 | false | not_zlib | — | — | — |
| ghostscript | gs-printer | 5 | 3347 | false | not_zlib | — | — | — |
| ghostscript | gs-printer | 13 | 2786 | true | — | 0.014716 | 1.139985 | 1.234027 |
| ghostscript | gs-screen | 5 | 2183 | false | not_zlib | — | — | — |
| hand | hand-base1 | 6 | 67633 | true | — | 0.000428 | 1.000192 | 0.613162 |
| hand | hand-base2 | 5 | 90557 | true | — | 0.000320 | 1.000143 | 0.613215 |
| hand | hand-base2 | 6 | 90557 | true | — | 0.000320 | 1.000143 | 0.613215 |
| qpdf | qpdf-compress | 1 | 72474 | false | not_zlib | — | — | — |
| qpdf | qpdf-linearize | 4 | 46 | true | — | 0.608695 | 2.130434 | 2.695652 |
| pdf-make-samples | flate.pdf | 4 | 3410 | true | — | 0.097360 | 9.477712 | 5.511436 |
| pdf-make-samples | flate.pdf | 6 | 3768 | true | — | 0.029458 | 8.518577 | 4.929140 |
| pdf-make-samples | flate.pdf | 7 | 2899 | true | — | 0.038634 | 4.839944 | 2.189030 |
| pdf-make-samples | flate.pdf | 8 | 31998 | true | — | 0.000875 | 1.000531 | 0.577848 |
| pdf-make-samples | flate.pdf | 9 | 6197 | true | — | 0.004518 | 5.166209 | 2.983701 |
| pdf-make-samples | flate.pdf | 10 | 8203 | true | — | 0.003413 | 1.002072 | 1.065951 |

## Does the Phase-6 win region appear?

The Phase-6 win region is a plaintext that is **shared across streams** *and* has
a **large/weakly-coded** appearance. It appears on the hand-written fixtures:

- `hand-base2.pdf`: two streams (objects 5 and 6) share one 90,541-byte
  plaintext, and **both** appearances are stored/weakly coded
  (`plain ≈ 1.00014`). The naive per-stream rANS complete cost is **111,062 B**;
  the deduplicated shared-channel cost is **55,531 B** — the dedup saving is the
  whole second copy, exactly the geometry the candidate exploits.
- `_synthetic/flate.pdf`: the `p1` plaintext is shared across four streams
  (objects 4/6/8/9) and one appearance (object 8) is stored
  (`compressed 31,998 ≈ plaintext 31,987`); naive 89,437 B vs deduped
  **34,051 B**. The Phase-6 positive persists.

## Where replay loses

- **qpdf-linearize object 4**: a 46-byte, strongly-coded stream. The fixed
  28-byte correction dominates (`raw 0.609`), so `(plaintext+corr)/compressed`
  = 2.13 and `(rans+corr)/compressed` = 2.70: replay is far *larger* than the
  original stream. Replay loses on small/tiny streams where fixed overhead
  cannot amortize.
- **Strongly-compressed appearances, measured individually**: the per-stream
  `rans` ratio on `flate.pdf` objects 4/6/7/9 ranges **2.19–5.51**. Replay+rANS
  loses per stream on unique, strongly-compressed plaintext; it wins only at the
  *document* level when a shared plaintext also has a weak appearance.
- **ghostscript / qpdf compress**: declined (see below).

## Harness-locality finding (honest caveat, not a zlib verdict)

All 6 declines are `not_zlib`, but that is **not** a statement that the streams
are not zlib. Independent `qpdf --show-object=N --raw-stream-data` reads show
that `gs-default` object 5 and `qpdf-compress` object 1 begin with `78 9c`, a
structurally valid zlib header. Root cause, isolated with a temporary span dump:

1. Ghostscript writes the `endstream` keyword with **no EOL immediately before
   it** (`… 0x91 endstream`). The PDF spec says the EOL there is a *should*, not
   a *shall*, and qpdf accepts these files.
2. The VOLE lexer's `find_endstream` (`src/adapter/pdf/lexer.rs`) **requires** a
   preceding `\n`/`\r`, so it skips the real terminator and swallows objects
   6–9 into one opaque payload.
3. Consequently the physical scanner cannot resolve the indirect `/Length`
   reference and falls back to a span whose `data_start` is the EOL *byte*
   itself, over-reading 2183 bytes. The sliced bytes begin with `0x0a`, so
   `try_replay_detailed` correctly-from-its-input returns `not_zlib`.

This is a **scanner locality limitation**, recorded as a finding. Fixing it would
change which streams are proposed for replay — i.e. candidate behavior — so it is
out of scope for Phase 7.0 (which adds only the `deflate-stats` aggregation and
the honest dedup field). It does **not** affect the Phase-6 fixtures, which use a
direct `/Length` and an EOL before `endstream`. The same locality issue is the
likely cause of `qpdf-preserve-objectstreams.pdf` reporting zero streams.

Because the transformer outputs mostly fail to be *measured* here, this campaign
is evidence about the **harness**, not evidence of absence of the win region on
real producers. That question remains open.

## Scope and verdict

- Exactness: the harness is diagnostics-only; it does not alter any wire format,
  candidate, or decode path. The only code change in Phase 7.0 Stage A is an
  added, content-deduplicated `replayed_rans_dedup_bytes` aggregate.
- Licensing: every byte is locally generated by pinned tools from our own
  deterministic inputs; no third-party bytes. qpdf/Ghostscript are transformers,
  not authoring apps.
- Producers not present (browsers/PDFium, LibreOffice, pdfTeX, Adobe) are a
  recorded gap.

**Verdict: RECORDED (measurement).** The corpus and ratio harness are built and
sealed; the Phase-6 win region is reproduced on the hand-written shared-plaintext
fixtures; the transformer outputs expose a lexer locality limitation in the
harness, which is recorded rather than papered over.

## Amendment (2026-10-05) — superseded by `2026-10-05-phase7-corpus-b-c4eb77e`

This receipt is **not rewritten**. The harness-locality limitation recorded above
was fixed in Stage A (`src/adapter/pdf/lexer.rs::find_endstream` now locates
`endstream` by right-termination, so a producer that omits the EOL before
`endstream` no longer defeats the scanner), and the same corpus and harness were
re-run. The corrected numbers are in the amendment receipt
`evidence/campaigns/2026-10-05-phase7-corpus-b-c4eb77e`:

- Flate streams **17 → 24**; replayed **11 → 24**; declined **6 → 0**;
  acceptance **0.647 → 1.000**.
- The 6 `not_zlib` declines are gone. The stream census also grew because the old
  over-read swallowed whole stream objects (all of qpdf's generated files
  reported fewer streams; `qpdf-preserve-objectstreams.pdf` reported zero).
- The Phase-6 shared-plaintext win geometry now also appears on qpdf transformer
  output (`qpdf-preserve-objectstreams.pdf`: naive 111,062 B vs deduped
  55,531 B).

The numbers above remain the historical record of the pre-fix harness. Neither
this report nor its manifest is rewritten; the amendment note and the new
receipt are the durable correction.
