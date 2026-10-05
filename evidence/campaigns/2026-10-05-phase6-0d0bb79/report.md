# Campaign: 2026-10-05-phase6-0d0bb79 — Phase 6 — exact DEFLATE replay over forced-candidate ablation

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
| bigtext.pdf | 65549 | 65944 | 385419 | 38215 | 66081 | 41241 | 65990 | 38382 | null | null | BYTE_RANS | equal |
| classic.pdf | 329 | 724 | 2103 | 769 | 811 | 3814 | 772 | 924 | null | null | RAW | equal |
| flate.pdf | 57513 | 57908 | 343174 | 49291 | 58196 | 52540 | 57956 | 49481 | 56736 | 36102 | PDF_DEFLATE_REPLAY_RANS | equal |
| incremental.pdf | 456 | 851 | 2691 | 841 | 973 | 3899 | 895 | 994 | null | null | BYTE_RANS | equal |
| malformed.pdf | 85 | 480 | 865 | 573 | null | null | null | null | null | null | RAW | equal |
| many.pdf | 9881 | 10276 | 49970 | 5242 | 14306 | 11150 | 10130 | 5955 | null | null | BYTE_RANS | equal |
| mixedeol.pdf | 248 | 643 | 1674 | 708 | 720 | 4744 | 702 | 859 | null | null | RAW | equal |
| notpdf.bin | 41 | 436 | 604 | 506 | null | null | null | null | null | null | RAW | equal |
| objstm.pdf | 341 | 736 | 2176 | 781 | 833 | 3847 | 784 | 935 | null | null | RAW | equal |
| trapstream.pdf | 250 | 645 | 1682 | 712 | 722 | 4758 | 694 | 861 | null | null | RAW | equal |
| traptext.pdf | 266 | 661 | 1778 | 732 | 728 | 3281 | 710 | 885 | null | null | RAW | equal |
| xrefstream.pdf | 251 | 646 | 1811 | 752 | 728 | 3764 | null | null | null | null | RAW | equal |

## Head-to-head: PDF_DEFLATE_REPLAY_RANS vs BYTE_RANS

For each file the forced BYTE_RANS and PDF_DEFLATE_REPLAY_RANS sizes are
compared directly. `delta` is `PDF_DEFLATE_REPLAY_RANS - BYTE_RANS` (negative
means replay+rANS is smaller). A `declined` verdict means the replay candidate
was not proposed for that input and is never scored as a win or a loss.

| file | source | BYTE_RANS | DEFLATE_REPLAY_RANS | verdict | delta | auto winner |
| --- | ---: | ---: | ---: | --- | ---: | --- |
| bigtext.pdf | 65549 | 38215 | null | declined | null | BYTE_RANS |
| classic.pdf | 329 | 769 | null | declined | null | RAW |
| flate.pdf | 57513 | 49291 | 36102 | win | -13189 | PDF_DEFLATE_REPLAY_RANS |
| incremental.pdf | 456 | 841 | null | declined | null | BYTE_RANS |
| malformed.pdf | 85 | 573 | null | declined | null | RAW |
| many.pdf | 9881 | 5242 | null | declined | null | BYTE_RANS |
| mixedeol.pdf | 248 | 708 | null | declined | null | RAW |
| notpdf.bin | 41 | 506 | null | declined | null | RAW |
| objstm.pdf | 341 | 781 | null | declined | null | RAW |
| trapstream.pdf | 250 | 712 | null | declined | null | RAW |
| traptext.pdf | 266 | 732 | null | declined | null | RAW |
| xrefstream.pdf | 251 | 752 | null | declined | null | RAW |

PDF_DEFLATE_REPLAY_RANS wins 1, loses 0, ties 0, and is
declined by 11 of the 12 corpus files when measured
head-to-head against BYTE_RANS.

## Raw-plaintext replay vs BYTE_RANS

The exact same comparison for the raw-plaintext `PDF_DEFLATE_REPLAY` lane:

| file | source | DEFLATE_REPLAY | DEFLATE_REPLAY_RANS | verdict (raw vs BYTE_RANS) | delta |
| --- | ---: | ---: | ---: | --- | ---: |
| bigtext.pdf | 65549 | null | null | declined | null |
| classic.pdf | 329 | null | null | declined | null |
| flate.pdf | 57513 | 56736 | 36102 | lose | 7445 |
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
| A0 | RAW | 139950 | — |
| A1 | + RLE | 139950 | 0 |
| A2 | + BYTE_RANS | 98560 | -41390 |
| A3 | + PDF_PHYSICAL | 98560 | 0 |
| A4 | + PDF_CHANNELS | 98560 | 0 |
| A5 | + PDF_LAYOUT | 98560 | 0 |
| A6 | + PDF_LAYOUT_RANS | 98560 | 0 |
| A7 | + PDF_DEFLATE_REPLAY | 98560 | 0 |
| A8 | + PDF_DEFLATE_REPLAY_RANS | 85371 | -13189 |

## Leave-one-out

Each delta is the full A8 portfolio minus the same portfolio with exactly one
replay mechanism removed (the other replay variant and every earlier mechanism
retained):

- PDF_DEFLATE_REPLAY_RANS: A8_without = 98560, delta = -13189
- PDF_DEFLATE_REPLAY:      A8_without = 85371, delta = 0
- PDF_LAYOUT_RANS:         A8_without = 85371, delta = 0

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
across every stream that produces it; on a file whose producer coding is weak
(low compression levels) and whose plaintext repeats across streams, the
shared channel re-expresses that weak coding far more cheaply than the stored
bitstreams, and it beats BYTE_RANS.

This is a scoped, measured result for *this* deterministic corpus and this
commit: the winning region is shared plaintext with weak producer coding, and
the losing region is unique, strongly-compressed plaintext (where the plaintext
is no smaller than the original bitstream). The winner is always decided by
actual serialized bytes, and every auto winner round-trips byte-exactly
(all_exact=true).

## Verdict

PASS
