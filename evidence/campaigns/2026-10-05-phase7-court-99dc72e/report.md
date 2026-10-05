# Campaign: 2026-10-05-phase7-court-99dc72e — Phase 7.0 complete-cost court over the producer corpus

This receipt **amends** `2026-10-05-phase7-corpus-b-c4eb77e` (do not rewrite it)
by measuring, for the first time, the *complete-cost court* over the stored-block
and real-producer corpus: does `PDF_DEFLATE_REPLAY_RANS` beat `BYTE_RANS` under
the standalone-`.voldoc` court (not just the `deflate-stats` ratio diagnostic)?

Measured binary: commit `99dc72e7cba108322454a59d1e3776ac5a2a4986`
(`binary_sha256 = e79d893e…`), identical to the `c4eb77e` lexer-fix lineage. **No
wire format, candidate, or decode path changed**; the only additions are the
court driver `tools/pdf-court.sh` and this receipt. The `deflate-stats` harness
remains diagnostics-only.

## Stage A — corpus reproducibility (qpdf)

`tools/pdf-corpus.sh` now passes `--deterministic-id` to **every** `qpdf`
invocation that writes an output, so qpdf derives `/ID` from a content hash.
Running the builder twice consecutively produced **byte-identical `qpdf-*.pdf`
outputs** (`sha256sum` over all four qpdf variants matched exactly between runs),
and byte-identical `hand-*` and `_synthetic/*` files.

The five `gs-*.pdf` (Ghostscript `pdfwrite`) outputs are **not** byte-reproducible:
Ghostscript embeds a per-run `/ID` and timestamp, so their SHA-256 differs run to
run. Their byte lengths, object structure, and every `FlateDecode`
`compressed_bytes` are stable, and the `deflate-stats` measurement is unchanged.
This caveat is recorded in `provenance.json` (`reproducibility`) rather than
chased. No third-party bytes are committed; the `.pdf` files stay gitignored.

## Method

For every one of the **23** corpus files the real CLI ran in the `dev` image
(`--all-features`):

- the ordinary complete-cost court: `encode FILE OUT`;
- four forced single lanes: `encode --force raw|byte-rans|pdf-deflate-replay|pdf-deflate-replay-rans FILE OUT`.

`encoded_len` is the **complete serialized `.voldoc` size**. A forced kind the
input does not propose is a typed `Usage` decline (`candidate … is not proposed
for this input`) and is recorded as `null`, never silently substituted. The auto
winner was additionally `verify`'d and its full `decode` byte-compared (`cmp`)
against the source. **All 23 files: `verify_ok = true` and `roundtrip_ok =
true`** (23/23 auto winners materialize byte-for-byte).

Driver: `sh tools/pdf-court.sh evidence/scratch/court evidence/corpus/phase7/*.pdf evidence/corpus/phase7/_synthetic/*`.
Raw rows: `results.json` (`per_file`).

## Headline: `PDF_DEFLATE_REPLAY_RANS` vs `BYTE_RANS`

Win = replay-rANS's forced `encoded_len` is strictly smaller than `BYTE_RANS`'s;
lose = strictly larger; decline = the input does not propose a replay lane
(`null`). Deltas are bytes.

| producer | files | win | lose | decline | win Δ total | lose Δ total |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| ghostscript | 5 | 0 | 5 | 0 | 0 | 24,823 |
| qpdf | 4 | 1 | 2 | 1 | **55,167** | 52,801 |
| hand (VOLE shell) | 2 | 1 | 1 | 0 | 55,126 | 20,716 |
| pdf-make-samples | 12 | 1 | 0 | 11 | 13,189 | 0 |
| **overall** | **23** | **3** | **8** | **12** | **123,482** | **98,340** |

Auto-court winners across the corpus: `PDF_DEFLATE_REPLAY_RANS` **3**,
`BYTE_RANS` **12**, `RAW` **8**. The three replay wins are exactly the files with
the Phase-6 win geometry (a plaintext shared across streams with a weak/stored
appearance), and in all three the unforced court selects
`PDF_DEFLATE_REPLAY_RANS`.

### Which files replay-rANS wins, and by how much

| file | producer | `BYTE_RANS` | `PDF_DEFLATE_REPLAY_RANS` | Δ (win) |
| --- | --- | ---: | ---: | ---: |
| `qpdf-preserve-objectstreams.pdf` | **qpdf 11.3.0** (transformer) | 112,147 | **56,980** | **55,167** |
| `hand-base2.pdf` | hand (VOLE shell writer) | 112,011 | **56,885** | **55,126** |
| `_synthetic/flate.pdf` | `pdf-make-samples` (our fixture) | 49,291 | **36,102** | **13,189** |

The only **real-producer transformer output** on which replay-rANS wins is
`qpdf-preserve-objectstreams.pdf`, by **55,167 bytes** (56,980 vs 112,147,
≈2.0×). It is the hand-written shared-plaintext geometry of `hand-base2.pdf`
surviving a qpdf `--object-streams=preserve` transform: two streams (objects 4
and 5) share one 90,541-byte weakly-coded plaintext, which the shared-channel
lane stores once. It is **not** an authoring-application result.

### Which files replay-rANS loses

Every Ghostscript variant (`gs-default/-ebook/-prepress/-printer/-screen`), both
qpdf compression variants (`qpdf-compress` −26,255; `qpdf-linearize` −26,546),
and `hand-base1.pdf` (−20,716). In these files each `FlateDecode` plaintext is
unique and strongly compressed, so paying rANS-plaintext + correction per stream
costs more than whole-file `BYTE_RANS`. The 46-byte `qpdf-linearize` stream and
the 532-byte Ghostscript content streams are the extreme per-stream losses the
ratio harness predicted.

### Which files decline

`qpdf-nocompress.pdf` (qpdf `--compress-streams=n` leaves no `/FlateDecode`
stream, so no replay lane is proposed) and 11 of the 12 synthetic fixtures
(`bigtext`, `classic`, `incremental`, `malformed`, `many`, `mixedeol`,
`notpdf.bin`, `objstm`, `trapstream`, `traptext`, `xrefstream`) — only
`flate.pdf` carries replayable Flate streams. A decline is the honest signal
that the mechanism does not apply, recorded as `null`.

## Scope and claim discipline

- **No population claim.** The corpus is **locally generated** by pinned tools
  (qpdf 11.3.0, Ghostscript 10.00.0) from our own deterministic inputs plus the
  Phase-3 synthetic fixtures. It is **not** a producer-stratified survey.
- **qpdf and Ghostscript are transformers, not authoring applications.** qpdf
  output is a transform of our hand-written base. Browser/PDFium, LibreOffice,
  pdfTeX and Adobe outputs remain a recorded **gap**; nothing here substitutes
  for them.
- The win region is **conjunctive**: it requires a plaintext shared across
  streams *and* a weak/stored appearance. It appears on the specific shared-
  plaintext geometry above and nowhere else in this corpus.
- Complete cost is authoritative: all three replay wins are measured as
  serialized `.voldoc` sizes in the same court as `BYTE_RANS`, and every winner
  is verified and byte-compared. No entropy estimate is credited.
- No wire format, candidate, or decode path changed; no candidate is adopted or
  removed by this measurement.

**Verdict: RECORDED (measurement, complete-cost court).** On this locally
generated corpus, `PDF_DEFLATE_REPLAY_RANS` beats `BYTE_RANS` under complete cost
on 3/23 files — the two shared-plaintext fixtures (`hand-base2`, `flate`) and one
qpdf transformer output (`qpdf-preserve-objectstreams`, by 55,167 B). It loses on
all 5 Ghostscript variants and both qpdf compression variants, and declines on
the 12 files with no replayable Flate lane (1 qpdf + 11 synthetic). The corpus is
not a population sample; qpdf is a transformer.

---

## Forward amendment (2026-10-05) — independent adversarial review: producer framing corrected

An independent adversarial reviewer (Phase 7.0) reproduced the numbers in this
receipt and **falsified the producer framing**, not the measurement. The durable
record is `docs/evidence/phase7-skeptic-review.md`.

- **Provenance.** The shared plaintext in `qpdf-preserve-objectstreams.pdf` is
authored in our fixture `hand-base2.pdf` (`tools/pdf-corpus.sh` writes objects
5 and 6 as two copies of the same hand-built stored-block zlib payload);
`qpdf --object-streams=preserve` merely copied and renumbered it. The raw streams
are **byte-identical** (`ec028dc1…`, confirmed via
`qpdf --show-object=N --raw-stream-data`); the fixture already wins
112,011 → 56,885 B and qpdf adds only +41 B, so **99.93% of the 55,167 B win is
inherited from our fixture, not produced by qpdf**.
- **Consequence.** Every genuinely transformed producer output on this corpus
**loses or declines**: all 5 Ghostscript outputs, both qpdf re-compressions, and
`hand-base1.pdf` lose (8 total); 12 files with no replayable Flate lane decline.
The court is **win 3 / lose 8 / decline 12, all 3 wins self-authored** (or a
preserved copy of one). The phrase "The only **real-producer transformer output**
on which replay-rANS wins" above is **superseded**: the qpdf file is a preserved
copy of a self-authored fixture, not an unmodified producer artifact.
- **`--deterministic-id`.** A `/ID`-only normalization; it neither creates nor
destroys the win (55,165 B no-flag vs 55,167 B flagged). Kept for
reproducibility.
- **Corrected statistic.** `correction/compressed` corpus-wide **p50 = 0.014716**
(p10 0.000320, p90 0.097360); the value 0.004518 is the `pdf-make-samples` subset
median only, not a corpus-wide median.
- **Conclusion.** The Phase-6 win is real and byte-exact, but its enabling
condition (shared plaintext) is **not produced by the tested transformers**; this
motivates Phase 7.2 (nested content proceduralization). `results.json` is not
rewritten.
