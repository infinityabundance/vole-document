# Campaign: 2026-10-05-phase5-4521778 — Phase 5 — PDF_LAYOUT rung over forced-candidate ablation

## Method

The exact binary built from this commit materializes the deterministic
sample corpus with `pdf-make-samples`, then for every corpus file forces
each candidate family in turn: `encode --force KIND IN OUT` for KIND in
{raw, rle, byte-rans, pdf-physical, pdf-channels, pdf-layout}. A forced
encode runs the *same* complete-cost court over a one-element candidate set,
so the forced descriptor is still serialized, decoded, and byte-compared
before it is returned; forcing selects a lane, it never bypasses exactness.
When the input does not propose a kind the command exits with a typed Usage
error ("is not proposed"), which is recorded honestly as `null` rather than
substituted. In parallel, the ordinary `encode` gives the auto winner, and
its output is decoded and `cmp`ed against the source and checked with
`verify`. The PDF_LAYOUT mechanism (Phase 5) is only proposed for
classic-cross-reference PDFs, so its forced size is non-null exactly for the
classic-xref samples.

## Per-file table

All sizes are serialized `.voldoc` bytes; `null` means the input does not
propose that kind. `byte_cmp` is the auto winner's decoded output compared
to the source.

| file | source | RAW | RLE | BYTE_RANS | PDF_PHYS | PDF_CHAN | PDF_LAYOUT | winner | byte_cmp |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| bigtext.pdf | 65549 | 65883 | 385358 | 38154 | 66020 | 46444 | 65929 | BYTE_RANS | equal |
| classic.pdf | 329 | 663 | 2042 | 708 | 750 | 3753 | 711 | RAW | equal |
| incremental.pdf | 456 | 790 | 2630 | 780 | 912 | 3838 | 834 | BYTE_RANS | equal |
| malformed.pdf | 85 | 419 | 804 | 512 | null | null | null | RAW | equal |
| many.pdf | 9881 | 10215 | 49909 | 5181 | 14245 | 11089 | 10069 | BYTE_RANS | equal |
| mixedeol.pdf | 248 | 582 | 1613 | 647 | 659 | 4679 | 641 | RAW | equal |
| notpdf.bin | 41 | 375 | 543 | 445 | null | null | null | RAW | equal |
| objstm.pdf | 341 | 675 | 2115 | 720 | 772 | 3778 | 723 | RAW | equal |
| trapstream.pdf | 250 | 584 | 1621 | 651 | 661 | 4691 | 633 | RAW | equal |
| traptext.pdf | 266 | 600 | 1717 | 671 | 667 | 3220 | 649 | RAW | equal |
| xrefstream.pdf | 251 | 585 | 1750 | 691 | 667 | 3703 | null | RAW | equal |

## Classic-cross-reference samples: does layout help?

For the 8 files that propose PDF_LAYOUT, the forced layout size is
compared against the auto winner. `declined` means the candidate was not
proposed.

| file | source | PDF_LAYOUT | auto winner | auto_len | layout vs auto |
| --- | ---: | ---: | --- | ---: | --- |
| bigtext.pdf | 65549 | 65929 | BYTE_RANS | 38154 | loses |
| classic.pdf | 329 | 711 | RAW | 663 | loses |
| incremental.pdf | 456 | 834 | BYTE_RANS | 780 | loses |
| many.pdf | 9881 | 10069 | BYTE_RANS | 5181 | loses |
| mixedeol.pdf | 248 | 641 | RAW | 582 | loses |
| objstm.pdf | 341 | 723 | RAW | 675 | loses |
| trapstream.pdf | 250 | 633 | RAW | 584 | loses |
| traptext.pdf | 266 | 649 | RAW | 600 | loses |

Layout wins on 0 of 8 classic-xref samples.

## Cumulative ladder

Each rung adds one mechanism and takes the per-file running minimum, summed
over the 11-file corpus. Adding a mechanism can only lower or hold
the total, never raise it.

| rung | mechanism added | total bytes | step delta |
| --- | --- | ---: | ---: |
| A0 | RAW | 81371 | — |
| A1 | + RLE | 81371 | 0 |
| A2 | + BYTE_RANS | 48598 | -32773 |
| A3 | + PDF_PHYSICAL | 48598 | 0 |
| A4 | + PDF_CHANNELS | 48598 | 0 |
| A5 | + PDF_LAYOUT | 48598 | 0 |

## Leave-one-out

Removing only the layout mechanism from the full portfolio leaves the A4
rung:

- A5_without_layout = A4 = 48598
- leave_one_out_layout_delta = A5 - A4 = 0

So the PDF layout mechanism saved 0 bytes across the corpus
relative to the same portfolio without it.

Auto-winner counts: RAW=8 RLE=0 BYTE_RANS=3
PDF_PHYSICAL=0 PDF_CHANNELS=0 PDF_LAYOUT=0.

## Honest interpretation

PDF_LAYOUT is *exact*: every forced layout descriptor serializes, is parsed
back, and materializes byte-for-byte, and the mechanism genuinely predicts
positions — it marks each object's introducer offset and the classic `xref`
section start, then emits the 10-digit offset fields and `startxref` value
from those marks rather than storing the digits (see the Phase-5.3 tests
`layout_predicts_entries` and `forced_layout_is_exact`).

It nevertheless loses the complete-cost court. On this corpus it is the auto
winner for 0 of 11 files, and it beats the auto winner on 0
of 8 classic-xref samples. The reason is framing: each predicted
10-digit offset replaces at most ten stored digits, but the DRA program must
carry a `MarkOffset` per object and per xref section plus an `EmitOffset` per
predicted entry, and each such op has fixed segment framing. That per-segment
framing exceeds the digits saved, so RAW (which stores the same digits inside
one opaque object) and BYTE_RANS (which entropy-codes them with the rest of
the file) stay cheaper. A5 therefore does not move below A4 on this corpus
(step delta 0, leave-one-out delta 0).

These are measured, scoped results for *this* deterministic corpus and this
commit. They say the current layout prediction does not yet pay on these
classic-xref PDFs; they do not say procedural xref regeneration cannot pay on
larger or denser files, where the same mark/emit framing is amortized over
many more predicted digits. The winner is always decided by actual serialized
bytes, and every auto winner round-trips byte-exactly (all_exact=true).

## Verdict

PASS
