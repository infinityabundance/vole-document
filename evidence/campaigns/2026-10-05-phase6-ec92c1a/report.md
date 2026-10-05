# Campaign: 2026-10-05-phase6-ec92c1a — Phase 6 — exact DEFLATE replay over forced-candidate ablation

## Method

The exact binary built from this commit materializes the deterministic
12-file sample corpus with `pdf-make-samples`, then for every corpus file
forces each candidate family in turn: `encode --force KIND IN OUT` for KIND in
{raw, rle, byte-rans, pdf-physical, pdf-channels, pdf-layout, pdf-layout-rans,
pdf-deflate-replay, pdf-deflate-replay-rans}.
A forced encode runs the *same* complete-cost court over a one-element
candidate set, so the forced descriptor is still serialized, decoded, and
byte-compared before it is returned; forcing selects a lane, it never bypasses
exactness. When the input does not propose a kind the command exits with a
typed Usage error ("is not proposed"), which is recorded honestly as `null`
rather than substituted. In parallel, the ordinary `encode` gives the auto
winner, and its output is decoded and `cmp`ed against the source and checked
with `verify`. The two replay lanes are only proposed for PDFs with a lone
`/FlateDecode` stream, so their forced sizes are non-null exactly for
`flate.pdf` on this corpus.

## Per-file table

All sizes are serialized `.voldoc` bytes; `null` means the input does not
propose that kind. `byte_cmp` is the auto winner's decoded output compared
to the source.

| file | source | RAW | RLE | BYTE_RANS | PHYS | CHAN | LAYOUT | LAY_RANS | DEFL_REP | DEFL_RANS | winner | byte_cmp |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| bigtext.pdf | 65549 | 65916 | 385391 | 38187 | 66053 | 41213 | 65962 | 38354 | null | null | BYTE_RANS | equal |
| classic.pdf | 329 | 696 | 2075 | 741 | 783 | 3786 | 744 | 896 | null | null | RAW | equal |
| flate.pdf | 57513 | 57880 | 343146 | 49263 | 58168 | 52512 | 57928 | 49453 | 56702 | 36068 | PDF_DEFLATE_REPLAY_RANS | equal |
| incremental.pdf | 456 | 823 | 2663 | 813 | 945 | 3871 | 867 | 966 | null | null | BYTE_RANS | equal |
| malformed.pdf | 85 | 452 | 837 | 545 | null | null | null | null | null | null | RAW | equal |
| many.pdf | 9881 | 10248 | 49942 | 5214 | 14278 | 11122 | 10102 | 5927 | null | null | BYTE_RANS | equal |
| mixedeol.pdf | 248 | 615 | 1646 | 680 | 692 | 4716 | 674 | 831 | null | null | RAW | equal |
| notpdf.bin | 41 | 408 | 576 | 478 | null | null | null | null | null | null | RAW | equal |
| objstm.pdf | 341 | 708 | 2148 | 753 | 805 | 3819 | 756 | 907 | null | null | RAW | equal |
| trapstream.pdf | 250 | 617 | 1654 | 684 | 694 | 4730 | 666 | 833 | null | null | RAW | equal |
| traptext.pdf | 266 | 633 | 1750 | 704 | 700 | 3253 | 682 | 857 | null | null | RAW | equal |
| xrefstream.pdf | 251 | 618 | 1783 | 724 | 700 | 3736 | null | null | null | null | RAW | equal |

## Head-to-head: PDF_DEFLATE_REPLAY_RANS vs BYTE_RANS

For each file the forced BYTE_RANS and PDF_DEFLATE_REPLAY_RANS sizes are
compared directly. `delta` is `PDF_DEFLATE_REPLAY_RANS - BYTE_RANS` (negative
means replay+rANS is smaller). A `declined` verdict means the replay candidate
was not proposed for that input and is never scored as a win or a loss.

| file | source | BYTE_RANS | DEFLATE_REPLAY_RANS | verdict | delta | auto winner |
| --- | ---: | ---: | ---: | --- | ---: | --- |
| bigtext.pdf | 65549 | 38187 | null | declined | null | BYTE_RANS |
| classic.pdf | 329 | 741 | null | declined | null | RAW |
| flate.pdf | 57513 | 49263 | 36068 | win | -13195 | PDF_DEFLATE_REPLAY_RANS |
| incremental.pdf | 456 | 813 | null | declined | null | BYTE_RANS |
| malformed.pdf | 85 | 545 | null | declined | null | RAW |
| many.pdf | 9881 | 5214 | null | declined | null | BYTE_RANS |
| mixedeol.pdf | 248 | 680 | null | declined | null | RAW |
| notpdf.bin | 41 | 478 | null | declined | null | RAW |
| objstm.pdf | 341 | 753 | null | declined | null | RAW |
| trapstream.pdf | 250 | 684 | null | declined | null | RAW |
| traptext.pdf | 266 | 704 | null | declined | null | RAW |
| xrefstream.pdf | 251 | 724 | null | declined | null | RAW |

PDF_DEFLATE_REPLAY_RANS wins 1, loses 0, ties 0, and is
declined by 11 of the 12 corpus files when measured
head-to-head against BYTE_RANS.

## Raw-plaintext replay vs BYTE_RANS

The exact same comparison for the raw-plaintext `PDF_DEFLATE_REPLAY` lane:

| file | source | DEFLATE_REPLAY | DEFLATE_REPLAY_RANS | verdict (raw vs BYTE_RANS) | delta |
| --- | ---: | ---: | ---: | --- | ---: |
| bigtext.pdf | 65549 | null | null | declined | null |
| classic.pdf | 329 | null | null | declined | null |
| flate.pdf | 57513 | 56702 | 36068 | lose | 7439 |
| incremental.pdf | 456 | null | null | declined | null |
| malformed.pdf | 85 | null | null | declined | null |
| many.pdf | 9881 | null | null | declined | null |
| mixedeol.pdf | 248 | null | null | declined | null |
| notpdf.bin | 41 | null | null | declined | null |
| objstm.pdf | 341 | null | null | declined | null |
| trapstream.pdf | 250 | null | null | declined | null |
| traptext.pdf | 266 | null | null | declined | null |
| xrefstream.pdf | 251 | null | null | declined | null |

## Cumulative ladder

Each rung adds one mechanism and takes the per-file running minimum, summed
over the 12-file corpus. Adding a mechanism can only lower or hold
the total, never raise it.

| rung | mechanism added | total bytes | step delta |
| --- | --- | ---: | ---: |
| A0 | RAW | 139614 | — |
| A1 | + RLE | 139614 | 0 |
| A2 | + BYTE_RANS | 98224 | -41390 |
| A3 | + PDF_PHYSICAL | 98224 | 0 |
| A4 | + PDF_CHANNELS | 98224 | 0 |
| A5 | + PDF_LAYOUT | 98224 | 0 |
| A6 | + PDF_LAYOUT_RANS | 98224 | 0 |
| A7 | + PDF_DEFLATE_REPLAY | 98224 | 0 |
| A8 | + PDF_DEFLATE_REPLAY_RANS | 85029 | -13195 |

## Leave-one-out

Each delta is the full A8 portfolio minus the same portfolio with exactly one
replay mechanism removed (the other replay variant and every earlier mechanism
retained):

- PDF_DEFLATE_REPLAY_RANS: A8_without = 98224, delta = -13195
- PDF_DEFLATE_REPLAY:      A8_without = 85029, delta = 0
- PDF_LAYOUT_RANS:         A8_without = 85029, delta = 0

Auto-winner counts: RAW=8 RLE=0 BYTE_RANS=3
PDF_PHYSICAL=0 PDF_CHANNELS=0 PDF_LAYOUT=0
PDF_LAYOUT_RANS=0 PDF_DEFLATE_REPLAY=0
PDF_DEFLATE_REPLAY_RANS=1.

## Verification

Every auto winner round-trips byte-exactly:
materialized_length == source_length, SHA256(materialized) == SHA256(source),
and byte_compare(materialized, source) == equal, with `verify` passing.
all_exact=true; negative controls exact=true. Per-file triples are
in `verification.json`.

## Negative controls

The two deliberate non-PDFs (`malformed.pdf`, `notpdf.bin`) decline both
replay lanes and still round-trip byte-exactly through the opaque RAW lane;
11 of 12 files decline the forced rANS replay lane (all files
without a lone FlateDecode stream). No decline is scored as a win or loss.

## Honest interpretation

Exact DEFLATE replay is *exact*: every forced replay descriptor serializes, is
parsed back, and materializes the original deflate bitstream byte-for-byte, and
the auto winner is gated on the same serialize / parse / materialize /
byte-compare check; see the Phase-6 tests in `tests/pdf_deflate.rs`.

The raw-plaintext lane `PDF_DEFLATE_REPLAY` stores each stream's plaintext as a
deduplicated object and then replays the original bitstream; on this corpus it
loses, because the plaintext of a strongly-compressed stream is nearly as large
as the stream it replaces. The rANS-plaintext lane `PDF_DEFLATE_REPLAY_RANS`
codes each unique plaintext once as an order-0 byte-rANS channel and shares it
across every stream that produces it; on a file whose plaintext is shared across
streams *and* has at least one large/weakly-coded appearance, order-0 rANS of the
shared plaintext is cheaper than the compressed appearances it replaces, so it
beats BYTE_RANS. Only `p1` is shared on `flate.pdf` (four streams at levels
0/1/6/9); `p2` and `p3` are unique.

This is a scoped, measured result for *this* deterministic corpus and this
commit: the winning region is a shared plaintext that *also* has a large/weakly
coded appearance — neither sharing alone nor weak coding alone wins (see
`docs/evidence/phase6-skeptic-review.md` for the negative controls) — and the
losing region is unique, strongly-compressed plaintext (where the plaintext is
no smaller than the original bitstream). The winner is always decided by actual
serialized bytes, and every auto winner round-trips byte-exactly
(all_exact=true).

## Verdict

PASS

## Amendment (2026-10-05)

An independent adversarial review narrowed the interpretation of the winning
region to the conjunctive condition recorded above. No measured number changed:
the per-file tables, cumulative ladder, leave-one-out deltas, and verification
triples are identical to the sealed run. The reviewer's four negative controls
are recorded in `docs/evidence/phase6-skeptic-review.md`.

## Amendment (Phase 6.7, DRA v8) — superseded by `2026-10-05-phase6-0d0bb79`

This receipt is **not rewritten**. Phase 6.7 changed the wire semantics, so the
DRA graph moved to **version 8** with an explicit `replay_codec` tag
(`REPLAY_DEFLATE_PREFLATE_0_7_6`, declared experimental/version-coupled), a
statically enforced decode-time resource bound (`declared_output_len ≤ 2*P+1024`,
ADR-0016), and an **opt-in** `deflate-replay` feature (the default build is
permissive-only). The universe string therefore changed to
`phase6;exact-bytes;dra-8;…;deflate-replay-preflate-0.7.6-experimental` and every
serialized size shifted by the fixed universe/codec-tag increase.

The court was re-run byte-for-byte at commit `0d0bb79` into the new receipt
`evidence/campaigns/2026-10-05-phase6-0d0bb79`. Verdict remains **PASS**; the
`PDF_DEFLATE_REPLAY_RANS` win over `BYTE_RANS` on `flate.pdf` persists
(`win 1, lose 0, decline 11`). The v8 numbers in ADR-0015/PROJECT_STATE/README/
CHANGELOG are the re-baseline; the numbers above remain the historical v7 record.
