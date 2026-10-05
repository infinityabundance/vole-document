# Campaign: 2026-10-05-phase5-8-cf8048d — Phase 5.8 — PDF_LAYOUT_RANS rung over forced-candidate ablation

## Method

The exact binary built from this commit materializes the deterministic
enlarged sample corpus with `pdf-make-samples`, then for every corpus file
forces each candidate family in turn: `encode --force KIND IN OUT` for KIND in
{raw, rle, byte-rans, pdf-physical, pdf-channels, pdf-layout, pdf-layout-rans}.
A forced encode runs the *same* complete-cost court over a one-element
candidate set, so the forced descriptor is still serialized, decoded, and
byte-compared before it is returned; forcing selects a lane, it never bypasses
exactness. When the input does not propose a kind the command exits with a
typed Usage error ("is not proposed"), which is recorded honestly as `null`
rather than substituted. In parallel, the ordinary `encode` gives the auto
winner, and its output is decoded and `cmp`ed against the source and checked
with `verify`. The PDF_LAYOUT and PDF_LAYOUT_RANS mechanisms (Phases 5 and
5.8) are only proposed for classic-cross-reference PDFs, so their forced sizes
are non-null exactly for those samples.

## Per-file table

All sizes are serialized `.voldoc` bytes; `null` means the input does not
propose that kind. `byte_cmp` is the auto winner's decoded output compared
to the source.

| file | source | RAW | RLE | BYTE_RANS | PDF_PHYS | PDF_CHAN | PDF_LAYOUT | PDF_LAY_RANS | winner | byte_cmp |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| bigtext.pdf | 65549 | 65903 | 385378 | 38174 | 66040 | 46464 | 65949 | 38341 | BYTE_RANS | equal |
| classic.pdf | 329 | 683 | 2062 | 728 | 770 | 3773 | 731 | 883 | RAW | equal |
| incremental.pdf | 456 | 810 | 2650 | 800 | 932 | 3858 | 854 | 953 | BYTE_RANS | equal |
| malformed.pdf | 85 | 439 | 824 | 532 | null | null | null | null | RAW | equal |
| many.pdf | 9881 | 10235 | 49929 | 5201 | 14265 | 11109 | 10089 | 5914 | BYTE_RANS | equal |
| mixedeol.pdf | 248 | 602 | 1633 | 667 | 679 | 4699 | 661 | 818 | RAW | equal |
| notpdf.bin | 41 | 395 | 563 | 465 | null | null | null | null | RAW | equal |
| objstm.pdf | 341 | 695 | 2135 | 740 | 792 | 3798 | 743 | 894 | RAW | equal |
| trapstream.pdf | 250 | 604 | 1641 | 671 | 681 | 4711 | 653 | 820 | RAW | equal |
| traptext.pdf | 266 | 620 | 1737 | 691 | 687 | 3240 | 669 | 844 | RAW | equal |
| xrefstream.pdf | 251 | 605 | 1770 | 711 | 687 | 3723 | null | null | RAW | equal |

## Head-to-head: PDF_LAYOUT_RANS vs BYTE_RANS

For each file, the forced BYTE_RANS and PDF_LAYOUT_RANS sizes are compared
directly. `delta` is `PDF_LAYOUT_RANS - BYTE_RANS` (negative means layout+rANS
is smaller). A `declined` verdict means the layout+rANS candidate was not
proposed for that input and is never scored as a win or a loss.

| file | source | BYTE_RANS | PDF_LAYOUT_RANS | verdict | delta | auto winner |
| --- | ---: | ---: | ---: | --- | ---: | --- |
| bigtext.pdf | 65549 | 38174 | 38341 | lose | 167 | BYTE_RANS |
| classic.pdf | 329 | 728 | 883 | lose | 155 | RAW |
| incremental.pdf | 456 | 800 | 953 | lose | 153 | BYTE_RANS |
| malformed.pdf | 85 | 532 | null | declined | null | RAW |
| many.pdf | 9881 | 5201 | 5914 | lose | 713 | BYTE_RANS |
| mixedeol.pdf | 248 | 667 | 818 | lose | 151 | RAW |
| notpdf.bin | 41 | 465 | null | declined | null | RAW |
| objstm.pdf | 341 | 740 | 894 | lose | 154 | RAW |
| trapstream.pdf | 250 | 671 | 820 | lose | 149 | RAW |
| traptext.pdf | 266 | 691 | 844 | lose | 153 | RAW |
| xrefstream.pdf | 251 | 711 | null | declined | null | RAW |

PDF_LAYOUT_RANS wins 0, loses 8, ties 0, and is declined
by 3 of the 11 corpus files when measured head-to-head
against BYTE_RANS.

## Cumulative ladder

Each rung adds one mechanism and takes the per-file running minimum, summed
over the 11-file corpus. Adding a mechanism can only lower or hold
the total, never raise it.

| rung | mechanism added | total bytes | step delta |
| --- | --- | ---: | ---: |
| A0 | RAW | 81591 | — |
| A1 | + RLE | 81591 | 0 |
| A2 | + BYTE_RANS | 48818 | -32773 |
| A3 | + PDF_PHYSICAL | 48818 | 0 |
| A4 | + PDF_CHANNELS | 48818 | 0 |
| A5 | + PDF_LAYOUT | 48818 | 0 |
| A6 | + PDF_LAYOUT_RANS | 48818 | 0 |

## Leave-one-out

Removing only the layout+rANS mechanism from the full portfolio leaves the A5
rung:

- A6_without_layout_rans = A5 = 48818
- leave_one_out_layout_rans_delta = A6 - A5 = 0

So the layout+rANS mechanism saved 0 bytes across the corpus
relative to the same portfolio without it.

Auto-winner counts: RAW=8 RLE=0 BYTE_RANS=3
PDF_PHYSICAL=0 PDF_CHANNELS=0 PDF_LAYOUT=0
PDF_LAYOUT_RANS=0.

## Honest interpretation

PDF_LAYOUT_RANS is *exact*: every forced descriptor serializes, is parsed back,
and materializes byte-for-byte (the candidate builder gates its return on the
same serialize / parse / materialize / byte-compare check; see the Phase-5.8
tests `layout_rans_exact`, `layout_rans_deterministic`, and
`assert_rans_materializes_exactly`). The mechanism entropy-codes the layout plan's
literal data object in channel 0 and its item table in channel 1, so the whole
plan pays order-0 rANS framing instead of being stored as literal bytes.

It nevertheless does not beat BYTE_RANS on this corpus: head-to-head it wins
0, loses 8, ties 0, and is declined by 3 files,
and the A6 rung does not move below A5 (step delta 0,
leave-one-out delta 0). The reason is honest and structural: channel 0
entropy-codes essentially the whole file against a single global histogram,
while BYTE_RANS does the same with one channel — but PDF_LAYOUT_RANS *adds* a
second channel (the serialized item table) plus two model descriptors and a
`PackedChannels` program op. On `many.pdf` that plan channel alone is on the
order of 1.8 KiB of added metadata that BYTE_RANS never pays, and channel 0
codes nearly the whole file anyway, so the structural prediction removes fewer
bytes than the plan channel adds. Layout+rANS therefore stays above BYTE_RANS
wherever it is proposed: the plan channel is added metadata, not a saving.

These are measured, scoped results for *this* deterministic corpus and this
commit. They say the current layout plan does not amortize its own channel on
these files; they do not say a denser plan (holding only the marked positions
rather than a full item table) could not pay on larger or denser files. The
winner is always decided by actual serialized bytes, and every auto winner
round-trips byte-exactly (all_exact=true).

## Verdict

PASS
