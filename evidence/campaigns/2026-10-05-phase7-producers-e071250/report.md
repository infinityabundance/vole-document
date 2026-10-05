# Campaign: 2026-10-05-phase7-producers-e071250 — Phase 7.0b generator-family corpus

Phase 7.0 established that the exact-replay win region requires a plaintext that is
**shared across streams**, and that the pinned producers (**qpdf**, **Ghostscript** —
both *transformers*, not authoring applications) do not generate it. This receipt
measures the missing families directly: real **authoring generators** with distinct
DEFLATE behaviour.

It does **not** amend or rewrite any Phase-7.0 receipt; it is a new, separately
scoped corpus and court. Measured binary: commit
`e0712503696b2b2cb609e83d75d820faf175ad92` (`binary_sha256 = fd14c0c6…`). **No wire
format, candidate, or decode path changed** — this is corpus + tooling only.

## Corpus (locally generated, no third-party bytes)

Four families, one PDF each, all produced inside the new opt-in `producers` image
(base `debian:bookworm-slim@sha256:3783cc01…`, same digest as `tools`; image
`vole-document/producers:bookworm`, ~724 MB) from the **same deterministic content
document** used by `tools/pdf-corpus.sh` (`gen_content`: "Line i of <seed>: …"):

| file | family (version) | Flate streams | byte-reproducible |
| --- | --- | ---: | --- |
| `reportlab-multipage.pdf` | ReportLab 3.6.12 (Python canvas, `pageCompression=1`, `useA85=0`) | 6 | yes |
| `cairo-vector.pdf` | Cairo 1.20.1 / libcairo 1.16.0 (pycairo PDF surface, vector-heavy) | 8 | yes |
| `libreoffice-export.pdf` | LibreOffice Writer 7.4.7.2 (headless HTML→PDF, `svp`) | 63 | **no** |
| `pdftex-doc.pdf` | pdfTeX 3.141592653-2.6-1.40.24 (TeX Live 2022) | 10 | yes |

Each document deliberately repeats an identical page several times (ReportLab 6,
Cairo 6, pdfTeX 6; LibreOffice 1700 identical paragraphs). That is a legitimate
document pattern and it is what makes cross-stream plaintext sharing possible; it is
**our deterministic input**, not a producer quirk we hand-wrote.

**/ID normalization.** Each output was passed through
`qpdf --deterministic-id --stream-data=preserve --object-streams=preserve` **only
when** a sorted multiset of the sha256/length of every `/FlateDecode` payload was
byte-identical before and after (verified with `qpdf --json` +
`qpdf --show-object=N --raw-stream-data`). That held for ReportLab, Cairo and
LibreOffice; for pdfTeX qpdf rewrites the (regenerated) xref/metadata streams, so the
**raw, already byte-reproducible** pdfTeX output was kept and the caveat recorded.
LibreOffice output is **not** byte-reproducible run to run (run-varying metadata); the
raw/provenance caveat stands. `provenance.json` carries every decision.

## Result — exact-replay acceptance

`deflate-stats` (preflate 0.7.6) over the four files: **87 Flate streams, 87 replayed,
0 declined → exact-replay acceptance 87/87 = 1.000**, across every family.
`correction/compressed` corpus-wide p10/p50/p90 = **0.034759 / 0.047945 / 0.068028**.

| producer | flate | replayed | acceptance | `corr/comp` p10/p50/p90 | naive rANS | dedup rANS |
| --- | ---: | ---: | ---: | --- | ---: | ---: |
| ReportLab | 6 | 6 | 1.000 | 0.054451 / 0.054451 / 0.054451 | 63,894 | **10,649** |
| Cairo | 8 | 8 | 1.000 | 0.010858 / 0.068028 / 0.115662 | 123,799 | **29,334** |
| LibreOffice | 63 | 63 | 1.000 | 0.034759 / 0.041039 / 0.058663 | 350,513 | 350,453 |
| pdfTeX | 10 | 10 | 1.000 | 0.003134 / 0.055013 / 0.086036 | 104,299 | **32,579** |

`naive rANS` / `dedup rANS` are the document aggregates from `deflate-stats`:
`replayed_rans_full_bytes` (one charge per stream) vs `replayed_rans_dedup_bytes` (one
charge per **unique** plaintext + correction). A large gap means the producer emitted
**shared plaintext across whole streams**: **ReportLab, Cairo and pdfTeX do** (6
identical page streams each); **LibreOffice does not** (350,513 → 350,453, i.e. 60 B).

## Headline — complete-cost court (`PDF_DEFLATE_REPLAY_RANS` vs `BYTE_RANS`)

Win = replay-rANS's forced `encoded_len` is strictly smaller than `BYTE_RANS`'s;
lose = strictly larger; decline = no replay lane proposed. Deltas are bytes. Every
auto winner was `verify`'d and its full `decode` byte-compared (`roundtrip_ok`).

| producer | `BYTE_RANS` | `PDF_DEFLATE_REPLAY_RANS` | Δ | verdict | auto winner |
| --- | ---: | ---: | ---: | --- | --- |
| **Cairo** | 58,711 | **34,574** | **+24,137** | **WIN** | `PDF_DEFLATE_REPLAY_RANS` |
| ReportLab | 11,144 | 14,456 | −3,312 | lose | `BYTE_RANS` |
| pdfTeX | 24,435 | 35,098 | −10,663 | lose | `RAW` (24,017) |
| LibreOffice | 72,791 | 375,265 | −302,474 | lose | `BYTE_RANS` |

Court totals: **win 1 / lose 3 / decline 0**; all 4 files `verify_ok` and
`roundtrip_ok`.

**A genuine authoring application produces the win region.** Cairo — a real vector
PDF generator, not a transformer — emits six byte-identical page content streams when
the page is repeated; the shared-channel lane stores that plaintext once and
`PDF_DEFLATE_REPLAY_RANS` beats `BYTE_RANS` under complete cost by **24,137 bytes**
(58,711 → 34,574), and the unforced court selects it. This is the **first
authoring-generator witness** of the Phase-6/7.0 win region; Phase 7.0 had only
self-authored fixtures.

ReportLab and pdfTeX also produce shared plaintext (ReportLab's dedup diagnostic
10,649 B even undercuts its 11,144 B `BYTE_RANS`), but **lose complete cost**:
ReportLab's file is small (10.9 KB), so DRA/model framing overhead (~3.8 KB) swamps
the 6-way dedup, and pdfTeX's already-tight streams make `BYTE_RANS` (24,435 B)
cheaper than paying rANS(plaintext)+correction (dedup diagnostic 32,579 B).
LibreOffice shares nothing and loses badly (its 63 streams are mostly fonts/xref and
its content is strongly compressed).

## Scope and claim discipline

- **The win is conditional.** It requires the conjunctive geometry of Phase 6/7.0: a
  plaintext shared across whole streams **and** a coding where the shared-channel
  cost amortizes. Cairo exhibits it here only because the input document repeats an
  identical page six times. This is **not** a claim that arbitrary real-world PDFs
  win, and **not** a population claim — one file per family, locally generated.
- **The producer creates the geometry.** We supplied a repeated-page document; Cairo
  (and ReportLab, pdfTeX) chose to emit separate, byte-identical Flate content
  streams for it rather than reusing one object. That is the generator behaviour the
  Phase-7.0 question asked about.
- **Transformers vs authoring apps.** qpdf appears only as an /ID normalizer, and only
  when it leaves every Flate payload byte-identical; it never creates the shared
  geometry.
- **Complete cost is authoritative.** All figures are serialized `.voldoc` sizes from
  the real CLI court, in the same court as `BYTE_RANS`; the `deflate-stats` dedup
  aggregate is a diagnostic only (it does not include all framing/model overhead, so
  it over-states would-be wins — precisely why ReportLab's diagnostic gap does not
  become a court win).
- **No candidate changed.** No wire format, candidate, or decode path was modified; no
  candidate is adopted or removed by this measurement.

**Verdict: RECORDED.** On four locally generated, real-authoring-generator families,
exact-replay acceptance is 87/87 = 1.000; `PDF_DEFLATE_REPLAY_RANS` beats `BYTE_RANS`
under complete cost on **1/4** — the Cairo vector output, by 24,137 B — while
ReportLab (small-file framing), pdfTeX (tight streams) and LibreOffice (no sharing)
lose. A genuine authoring generator **does** produce the shared-plaintext win region
(on a repeated-page document); whether real-world authoring output does so often
enough to matter remains open and is a Phase-7.2 question.
