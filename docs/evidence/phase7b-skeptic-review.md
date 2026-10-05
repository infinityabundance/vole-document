# Phase 7.0b — independent adversarial review

- **Reviewer:** independent adversarial reviewer (Phase 7.0b)
- **Scope:** the Phase-7.0b generator-family corpus and complete-cost court
  (`evidence/campaigns/2026-10-05-phase7-producers-e071250/`), and the headline
  claim that "a genuine authoring application produces the win region" and that
  Cairo is the "first authoring-generator witness of the shared-plaintext win
  region".
- **Method:** the corpus is a pure function of `tools/pdf-corpus-producers.sh`
  (the generator script is read directly) plus generic third-party compressors
  (`gzip -9`, `zlib -9`, `xz -9e`) run over the same `cairo-vector.pdf` bytes.
  No measured campaign number was altered.
- **Verdict:** the **numeric and exactness results SURVIVE** (all 4 auto winners
  are byte-exact; the court sizes are as sealed). The **authoring-generator
  framing is FALSIFIED**: the Cairo "win" is a *harness repeated-bytes artifact*,
  and `BYTE_RANS` is too weak a baseline to make the delta meaningful.

## Angles

| # | Angle | Verdict | Basis |
| --- | --- | --- | --- |
| 1 | Input construction: does the generator give the six pages any per-page variation? | **FALSIFIED (artifact)** | `tools/pdf-corpus-producers.sh` `gen_cairo.py` loops `for _p in range(pages)` drawing the identical line set with no page-dependent content; all six pages are the same drawing |
| 2 | Stream identity: are the six Cairo content streams identical in *compressed* bytes, not merely in plaintext? | **Artifact confirmed** | identical plaintext + identical DEFLATE level ⇒ identical compressed bytes; the corpus cannot separate "same plaintext, different coding" from "identical bytes" |
| 3 | Baseline strength: is `BYTE_RANS` a meaningful generic baseline? | **WEAK** | it is a whole-file order-0 byte-rANS lane with **no LZ / no long-range matching**; repeated identical streams are exactly what it cannot exploit |
| 4 | Generic-compressor comparison: how does generic LZ do on the same file? | **LZ wins ~2×** | `gzip -9` = **17,382 B**, `zlib -9` = **17,376 B**, `xz -9e` = **16,852 B** on `cairo-vector.pdf`, versus the 34,574 B reported as the "winning" `PDF_DEFLATE_REPLAY_RANS` size |
| 5 | Mechanism attribution: does the court distinguish plaintext-sharing from compressed-byte repetition? | **FALSIFIED (not distinguished)** | with identical compressed bytes the −24,137 B vs `BYTE_RANS` is a win over an order-0 lane on repeated bytes; it is not evidence about the shared-plaintext-vs-distinct-compression mechanism |
| 6 | Numeric/exactness: sealed court sizes and round-trip | **PASS** | the sizes are as sealed and every auto winner `verify` + `decode`/`cmp` byte-exacts; only the interpretation is wrong |

## Artifact finding (angles 1–2)

`tools/pdf-corpus-producers.sh` builds Cairo output with `pages=6` and a single
deterministic content document (`content_lines("producers", 170)`, from
`content.py`). `gen_cairo.py` draws that same 170-line page — the same 600
strokes, 200 rectangles and text — for every page, with no page-number or
otherwise page-dependent content. Cairo consequently emits **six byte-identical
page content streams**: not just the same plaintext, but the **same compressed
bytes**, because the same DEFLATE level is applied to identical input.

That makes the "large naive→dedup gap" (123,799 → 29,334) a statement about
repeated identical bytes, not about distinct compressions of a shared plaintext.
The court has no fixture in which a producer emits a *shared plaintext under a
different coding*, which is the geometry the Phase-6 win region is about.

## Baseline finding (angles 3–5)

`BYTE_RANS` order-0-codes the whole document with no LZ; on a file that is six
verbatim repeats of one page, it pays for the repetition. Generic compressors
match and exploit that repetition directly. Measured on the same
`cairo-vector.pdf`:

```text
source (cairo-vector.pdf)          58,424 B
gzip -9                            17,382 B
zlib -9  (zlib.compress(level=9))  17,376 B
xz -9e                             16,852 B
PDF_DEFLATE_REPLAY_RANS  (sealed)  34,574 B   <-- ~2.06x larger than xz -9e
BYTE_RANS                (sealed)  58,711 B
```

So the reported 24,137 B "win" is a win over a **weak order-0 baseline**, and the
complete file is still about **twice** the size a generic LZ achieves. The same
applies to ReportLab and pdfTeX, whose shared streams are likewise identical
repeats. This is the decisive methodology result: prior "wins" were measured
against `BYTE_RANS`, which is not a generic-compressor baseline.

## Corrected characterization

*Our deterministic generator repeated one identical page six times; Cairo emitted
six byte-identical streams (compressed bytes and plaintext both identical); this
witnesses a repeated-identical-bytes region already captured better by generic
LZ, not the shared-plaintext-vs-distinct-compression mechanism.*

The phrases "a genuine authoring application does produce the win region", "the
producer creates the geometry", and "first authoring-generator witness of the
shared-plaintext win region" are **withdrawn**. The Cairo file is a witness that
Cairo repeats bytes when handed a repeated page, which is unsurprising and is not
a population claim about real authoring output.

## Strongest remaining caveat

The Phase-6/7.0 exact-replay result and its byte-exactness are untouched. What is
**not** established — and was overclaimed — is that a real authoring application
*creates* the enabling condition (a plaintext shared across whole streams **and**
independently re-coded or weakly coded). The Phase-7.0b corpus does not test that:
its shared streams are identical repeats. Honest standing requires comparing
against `gzip`/`zstd`/`xz`/`brotli`, which the Phase-7.0c baseline ladder adds.
