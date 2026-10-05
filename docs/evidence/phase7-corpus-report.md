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

**Corpus-wide `correction/compressed` p10/p50/p90 = 0.000320 / 0.014716 /
0.097360** over all 24 replayed streams. The value `0.004518` in the
`pdf-make-samples` row is **that producer's subset median only** (6 streams, all
from `_synthetic/flate.pdf`); every per-producer row is a subset statistic and
must not be read as corpus-wide.

- **The Phase-6 win region appears only in our own hand-authored fixture.**
  `hand-base2.pdf` has two streams (objects 5 and 6) that are two copies of the
  *same* plaintext, both weakly coded: naive rANS complete cost 111,062 B vs
  **deduped 55,531 B**. `_synthetic/flate.pdf` shares `p1` across four streams
  with one stored appearance: naive 89,437 B vs **deduped 34,051 B**.
  `qpdf-preserve-objectstreams.pdf` shows the same 111,062 → 55,531 B geometry
  **only because qpdf `--object-streams=preserve` copied and renumbered the two
  byte-identical raw streams** (`ec028dc1…`, confirmed via
  `qpdf --show-object=N --raw-stream-data`) already present in `hand-base2.pdf`;
  the fixture already wins 112,011 → 56,885 B and qpdf adds only +41 B of margin,
  so **99.93% of the reported qpdf win is inherited from our fixture, not
  produced by qpdf**. No genuinely transformed producer output contains a
  shared-plaintext pair: no Ghostscript output does, and both qpdf re-compressions
  (`qpdf-compress`, `qpdf-linearize`) act on `hand-base1.pdf`, which has no
  shared plaintext.
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
11/17 = 0.647). The Phase-6 win geometry appears **only on our own hand-authored
fixtures** (`hand-base2.pdf`, `_synthetic/flate.pdf`); the qpdf
`--object-streams=preserve` output reproduces it merely by copying the fixture's
two byte-identical raw streams, so it is not evidence that a tested transformer
*produces* the region. Claims are scoped to locally generated files; qpdf and
Ghostscript are transformers, not authoring apps.

## Complete-cost court (Stage B, sealed)

The ratio tables above are a **diagnostic**; they do not decide. A second
measurement — the decisive one — seals the *complete-cost court* over the same
corpus at `evidence/campaigns/2026-10-05-phase7-court-99dc72e/` (code commit
`99dc72e`, driver `tools/pdf-court.sh`). It runs the real CLI over all 23 corpus
files and compares complete serialized `.voldoc` sizes:

- the unforced court (`encode FILE OUT`);
- four forced single lanes (`encode --force raw|byte-rans|pdf-deflate-replay|pdf-deflate-replay-rans FILE OUT`).

A forced lane the input does not propose is a typed `Usage` decline, recorded as
`null`. Every one of the 23 auto winners `verify`'d and `decode`|`cmp`'d against
its source byte-for-byte.

### `PDF_DEFLATE_REPLAY_RANS` vs `BYTE_RANS`

Win = replay-rANS's forced `encoded_len` is strictly smaller than `BYTE_RANS`'s;
lose = strictly larger; decline = the input proposes no replay lane (`null`).
Deltas are bytes.

| producer | files | win | lose | decline | win Δ total | lose Δ total |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| ghostscript | 5 | 0 | 5 | 0 | 0 | 24,823 |
| qpdf | 4 | 1 | 2 | 1 | **55,167** | 52,801 |
| hand (VOLE shell) | 2 | 1 | 1 | 0 | 55,126 | 20,716 |
| pdf-make-samples | 12 | 1 | 0 | 11 | 13,189 | 0 |
| **overall** | **23** | **3** | **8** | **12** | **123,482** | **98,340** |

Auto-court winners: `PDF_DEFLATE_REPLAY_RANS` **3**, `BYTE_RANS` **12**, `RAW`
**8**. The three replay wins are exactly the files with the Phase-6 win geometry
(a plaintext shared across streams with a weak/stored appearance) — **all three
self-authored** — and in all three the unforced court selects
`PDF_DEFLATE_REPLAY_RANS`:

| file | producer | `BYTE_RANS` | `PDF_DEFLATE_REPLAY_RANS` | Δ (win) |
| --- | --- | ---: | ---: | ---: |
| `qpdf-preserve-objectstreams.pdf` | **qpdf 11.3.0** (preserve-copy of `hand-base2.pdf`) | 112,147 | **56,980** | **55,167** |
| `hand-base2.pdf` | hand (VOLE shell writer) | 112,011 | **56,885** | **55,126** |
| `_synthetic/flate.pdf` | `pdf-make-samples` (our fixture) | 49,291 | **36,102** | **13,189** |

**Every genuinely transformed producer output loses or declines.** All 5
Ghostscript variants (`gs-default/-ebook/-prepress/-printer/-screen`), the two
qpdf compression variants (`qpdf-compress` −26,255; `qpdf-linearize` −26,546)
and `hand-base1.pdf` (−20,716) **lose**, where each plaintext is unique and
strongly compressed; the 12 files with no replayable Flate lane
(`qpdf-nocompress` plus 11 synthetic fixtures) **decline**. The apparent single
"qpdf win" (`qpdf-preserve-objectstreams.pdf`, −55,167 B) is not a
transformed-producer result: `qpdf --object-streams=preserve` copied and
renumbered the two byte-identical raw streams already authored in
`hand-base2.pdf`, which itself wins 112,011 → 56,885 B; qpdf adds only +41 B of
margin, so **99.93% of the win is inherited from our fixture**. On this locally
generated corpus the exact-replay lane therefore **wins 3 / loses 8 / declines
12, and all 3 wins are self-authored** (two fixtures plus a preserved copy of
one). `--deterministic-id` is a `/ID`-only normalization (55,165 B no-flag vs
55,167 B flagged): it makes the court reproducible but neither creates nor
destroys the win, so the qpdf file must not be presented as an unmodified
producer artifact.

### qpdf corpus reproducibility

Every qpdf invocation now passes `--deterministic-id`, so two consecutive corpus
builds are **byte-identical** for all `qpdf-*.pdf` (and for `hand-*` and
`_synthetic/*`). The five `gs-*.pdf` outputs are **not** byte-reproducible:
Ghostscript embeds a per-run `/ID` and timestamp, so their SHA-256 changes run to
run, while byte length, object structure, and every `FlateDecode`
`compressed_bytes` stay stable. The caveat is recorded in `provenance.json`. No
third-party bytes are committed; the regenerable `.pdf` files stay gitignored.

**Verdict: RECORDED (complete-cost court).** On this locally generated corpus,
`PDF_DEFLATE_REPLAY_RANS` beats `BYTE_RANS` under complete cost on 3/23 files —
all three self-authored: two shared-plaintext fixtures (`hand-base2.pdf`,
`_synthetic/flate.pdf`) and a `qpdf --object-streams=preserve` **copy** of one of
them, whose win is 99.93% inherited from the fixture. Every genuinely transformed
producer output **loses (8) or declines (12)**. The Phase-6 win is real and
byte-exact, but its enabling condition (a plaintext shared across streams with a
large/weakly-coded appearance) is **not produced by the tested transformers** on
this corpus, which motivates Phase 7.2 (nested content proceduralization).
**No population claim is made:** the corpus is locally generated, qpdf and
Ghostscript are **transformers** (not authoring applications), and
browser/PDFium, LibreOffice, pdfTeX and Adobe outputs remain a recorded gap. No
wire format, candidate, or decode path changed; no new candidate is adopted.

---

# Amendment (2026-10-05) — Phase 7.0b generator-family corpus

> **New, separately scoped corpus — nothing above is rewritten.** Phase 7.0's
> decisive gap was that the tested producers (qpdf, Ghostscript) are *transformers*
> that never produce the shared plaintext the win region needs. Phase 7.0b adds four
> real *authoring generators* and re-asks the question.
>
> Generation script: `tools/pdf-corpus-producers.sh`; ledger:
> `evidence/corpus/phase7-producers/provenance.json`; raw measurement:
> `evidence/corpus/phase7-producers/deflate-stats.jsonl`; sealed receipt:
> `evidence/campaigns/2026-10-05-phase7-producers-e071250/`. No wire format,
> candidate, or decode path changed.

## New producers and how they ran

All four families were **actually added and run** (none skipped): ReportLab 3.6.12,
Cairo 1.20.1 / libcairo 1.16.0, LibreOffice Writer 7.4.7.2, pdfTeX
3.141592653-2.6-1.40.24 (TeX Live 2022). They run in the separate, opt-in
`producers` image (`debian:bookworm-slim@sha256:3783cc01…`, the same digest as
`tools`; image tag `vole-document/producers:bookworm`, ~724 MB, ~38 s cold build).
The separate stage keeps the fast `tools` semantic gate unchanged. Each output was
`qpdf --check`'d; `/ID` normalization
(`qpdf --deterministic-id --stream-data=preserve --object-streams=preserve`) was
applied only where it left every `/FlateDecode` payload byte-identical (ReportLab,
Cairo, LibreOffice); pdfTeX kept its raw output. ReportLab, Cairo and pdfTeX are
byte-reproducible run to run; LibreOffice is **not** (run-varying metadata).

| producer | file | Flate streams | reproducible | `corr/comp` p10/p50/p90 | naive rANS | dedup rANS |
| --- | --- | ---: | --- | --- | ---: | ---: |
| ReportLab | `reportlab-multipage.pdf` | 6 | yes | 0.054451 / 0.054451 / 0.054451 | 63,894 | 10,649 |
| Cairo | `cairo-vector.pdf` | 8 | yes | 0.010858 / 0.068028 / 0.115662 | 123,799 | 29,334 |
| LibreOffice | `libreoffice-export.pdf` | 63 | no | 0.034759 / 0.041039 / 0.058663 | 350,513 | 350,453 |
| pdfTeX | `pdftex-doc.pdf` | 10 | yes | 0.003134 / 0.055013 / 0.086036 | 104,299 | 32,579 |
| **overall** | 4 files | 87 | — | **0.034759 / 0.047945 / 0.068028** | — | — |

**Exact-replay acceptance 87/87 = 1.000** (0 declines). A large naive→dedup gap means
the producer emitted a plaintext **shared across whole streams**: ReportLab, Cairo
and pdfTeX do (six byte-identical repeated-page streams each); LibreOffice does not
(350,513 → 350,453). **Because our deterministic generator draws one identical page
six times with no per-page variation, those six streams are byte-identical in the
*compressed* bytes as well as the plaintext**, so a naive→dedup gap here cannot by
itself distinguish plaintext-sharing from plain compressed-byte repetition.

## Complete-cost court (`PDF_DEFLATE_REPLAY_RANS` vs `BYTE_RANS`)

| producer | `BYTE_RANS` | `PDF_DEFLATE_REPLAY_RANS` | Δ | verdict | auto winner |
| --- | ---: | ---: | ---: | --- | --- |
| **Cairo** | 58,711 | **34,574** | **+24,137** | **WIN** | `PDF_DEFLATE_REPLAY_RANS` |
| ReportLab | 11,144 | 14,456 | −3,312 | lose | `BYTE_RANS` |
| pdfTeX | 24,435 | 35,098 | −10,663 | lose | `RAW` (24,017) |
| LibreOffice | 72,791 | 375,265 | −302,474 | lose | `BYTE_RANS` |

Court totals: **win 1 / lose 3 / decline 0**; all 4 files `verify` + byte-compare
round-trip exact.

> **Correction (2026-10-05, Phase 7.0c).** The paragraph this replaces claimed that
> "a genuine authoring application does produce the win region" and called Cairo the
> "first authoring-generator witness of the shared-plaintext win region". An
> independent adversarial review found that to be a **harness artifact**, and the
> claim is withdrawn. Correct characterization: *our deterministic generator repeated
> one identical page six times; Cairo emitted six byte-identical streams (compressed
> bytes and plaintext both identical); this witnesses a repeated-identical-bytes
> region already captured better by generic LZ, not the
> shared-plaintext-vs-distinct-compression mechanism.* On this file `gzip -9` =
> 17,382 B, `zlib9` = 17,376 B and `xz -9e` = 16,852 B — about **half** the 34,574 B
> that `PDF_DEFLATE_REPLAY_RANS` reports as a "win". `BYTE_RANS` is a weak order-0
> baseline with no LZ, so the −24,137 B delta is a win over an order-0 lane on
> repeated identical bytes. It is **not** evidence that a real authoring application
> produces the shared-plaintext win region. See
> `docs/evidence/phase7b-skeptic-review.md`.

**What the court actually shows.** Cairo emits six byte-identical page content
streams for a repeated page; the shared-channel lane stores that plaintext once and
`PDF_DEFLATE_REPLAY_RANS` beats `BYTE_RANS` under complete cost by **24,137 B**
(58,711 → 34,574), and the unforced court selects it. That delta is real, but it is a
win over a **weak order-0** baseline on a **repeated-identical-bytes** region, not on
the shared-plaintext-vs-distinct-compression mechanism. ReportLab and pdfTeX emit the
same shared geometry but **lose** complete cost: ReportLab's file is small (10.9 KB)
so ~3.8 KB of DRA/model framing swamps the 6-way dedup (its dedup diagnostic
10,649 B even undercuts its 11,144 B `BYTE_RANS`, which is exactly why the diagnostic
is not the decision), and pdfTeX's already-tight streams make `BYTE_RANS` cheaper than
rANS(plaintext)+corr. LibreOffice shares nothing and loses badly.

**Scope.** The measured delta is **conditional** on the input document repeating an
identical page (a legitimate pattern, and our deterministic input); it is **not** a
population claim and does not show that arbitrary real-world authoring output wins.
The independent review shows the repeated page here is also byte-identical in its
compressed form, so the region is a generic repeated-bytes one that LZ compresses far
better (see `docs/evidence/phase7b-skeptic-review.md`). Complete cost remains
authoritative, and the shared-plaintext geometry is necessary but not sufficient
(ReportLab and pdfTeX prove it). No candidate changed.

---

# Amendment (2026-10-05) — Phase 7.0c generic-compressor baseline ladder

> **New, separately scoped measurement — nothing above is rewritten.** Receipt:
> `evidence/campaigns/2026-10-05-phase7-baselines-7b9f662/`; driver
> `tools/baselines.sh`; full 27-row table `baseline-table.md`. No wire format,
> candidate, or decode path changed.

Phase 7.0/7.0b measured VOLE's candidates only against `BYTE_RANS`, a whole-file
**order-0 byte-rANS** lane with no LZ. A win over `BYTE_RANS` is not a generic
compression result. The ladder adds `gzip -9`, `zstd -19 --long=27`, `xz -9e` and
`brotli -q 11` (each round-trip verified lossless against the source) over both
corpora, and compares them to the **best VOLE lane** — the minimum complete
serialized `.voldoc` size across the auto winner, `RAW`, `RLE`, `BYTE_RANS` and
every forced structural kind (`pdf-physical`, `pdf-channels`, `pdf-layout`,
`pdf-layout-rans`, `pdf-deflate-replay`, `pdf-deflate-replay-rans`). Every auto
winner is `verify` + `decode`/`cmp` byte-exact.

**Overall (27 files): the best VOLE lane beats `gzip`/`zstd`/`xz`/`brotli` on 0
files.** The best generic compressor is smaller than the best VOLE lane on every
file, by **460,320 bytes** in total (`phase7` +410,355 over 23 files; `producers`
+49,965 over 4 files):

| corpus | files | VOLE beats gzip | zstd | xz | brotli | any generic | Δ bytes vs best generic |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| phase7 | 23 | 0 | 0 | 0 | 0 | **0** | +410,355 |
| producers | 4 | 0 | 0 | 0 | 0 | **0** | +49,965 |
| **overall** | **27** | **0** | **0** | **0** | **0** | **0** | **+460,320** |

The two files the sharing narrative rested on:

| file | source | gzip -9 | zstd -19 | xz -9e | brotli -q11 | BYTE_RANS | best VOLE | lane | VOLE − best generic |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: |
| `cairo-vector.pdf` | 58,424 | 17,382 | 16,836 | 16,852 | **16,670** | 58,711 | 34,574 | `PDF_DEFLATE_REPLAY_RANS` | **+17,904** |
| `_synthetic/flate.pdf` | 57,513 | 22,426 | 20,171 | 18,884 | 18,891 | 49,291 | 36,102 | `PDF_DEFLATE_REPLAY_RANS` | **+17,218** |

On the Cairo file the "winning" 34,574 B is **2.07×** the best generic result
(16,670 B brotli); on `flate.pdf` the 36,102 B is **1.91×** the best generic
(18,884 B xz). The ladder independently reproduces the Phase-7.0b reviewer's LZ
figures (`gzip -9` = 17,382 B, `xz -9e` = 16,852 B). The best VOLE lane *does*
beat `BYTE_RANS` on 13/27 files — but `BYTE_RANS` is the weak order-0 baseline, so
that is not a compression result.

**Verdict: RECORDED (baseline ladder).** Prior "wins" were relative to a weak
order-0 baseline and do not survive a generic-compressor comparison: on the honest
ladder (the complete file must losslessly recover the original, so all
framing/model overhead is charged) VOLE loses to every generic compressor on every
file tested. VOLE's byte-exact structural reconstruction is unchanged; its
*compression* claim does not survive on this corpus. No candidate is adopted,
removed, or changed.

# See also (2026-10-05) — Phase 7.3 query-cost court

> A separate axis, measured on a 32.22 MiB / 800-object deterministic PDF:
> **`docs/evidence/phase7-partial-report.md`** (receipt
> `evidence/campaigns/2026-10-05-phase7-partial-a5764c9/`, ADR-0018). It does not
> change anything above: whole-file size still loses (best VOLE 17,392,713 B vs
> xz 5,841,896 B, 2.98×). Partial materialization is a **scoped positive on
> random-access decode CPU** with **no I/O win in v1** (the CLI reads
> the whole descriptor, so `descriptor_bytes_traversed` is CPU-side only).
