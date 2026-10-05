# Campaign: 2026-10-05-phase5-7193001 — Phase 5 — PDF_LAYOUT rung over forced-candidate ablation

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
| bigtext.pdf | 65549 | 65879 | 385354 | 38150 | 66016 | 46440 | 66066 | BYTE_RANS | equal |
| classic.pdf | 329 | 659 | 2038 | 704 | 746 | 3749 | 798 | RAW | equal |
| incremental.pdf | 456 | 786 | 2626 | 776 | 908 | 3834 | 968 | BYTE_RANS | equal |
| malformed.pdf | 85 | 415 | 800 | 508 | null | null | null | RAW | equal |
| mixedeol.pdf | 248 | 578 | 1609 | 643 | 655 | 4675 | 701 | RAW | equal |
| notpdf.bin | 41 | 371 | 539 | 441 | null | null | null | RAW | equal |
| objstm.pdf | 341 | 671 | 2111 | 716 | 768 | 3774 | 820 | RAW | equal |
| trapstream.pdf | 250 | 580 | 1617 | 647 | 657 | 4687 | 709 | RAW | equal |
| traptext.pdf | 266 | 596 | 1713 | 667 | 663 | 3216 | 715 | RAW | equal |
| xrefstream.pdf | 251 | 581 | 1746 | 687 | 663 | 3699 | null | RAW | equal |

## Classic-cross-reference samples: does layout help?

For the 7 files that propose PDF_LAYOUT, the forced layout size is
compared against the auto winner. `declined` means the candidate was not
proposed.

| file | source | PDF_LAYOUT | auto winner | auto_len | layout vs auto |
| --- | ---: | ---: | --- | ---: | --- |
| bigtext.pdf | 65549 | 66066 | BYTE_RANS | 38150 | loses |
| classic.pdf | 329 | 798 | RAW | 659 | loses |
| incremental.pdf | 456 | 968 | BYTE_RANS | 776 | loses |
| mixedeol.pdf | 248 | 701 | RAW | 578 | loses |
| objstm.pdf | 341 | 820 | RAW | 671 | loses |
| trapstream.pdf | 250 | 709 | RAW | 580 | loses |
| traptext.pdf | 266 | 715 | RAW | 596 | loses |

Layout wins on 0 of 7 classic-xref samples.

## Cumulative ladder

Each rung adds one mechanism and takes the per-file running minimum, summed
over the 10-file corpus. Adding a mechanism can only lower or hold
the total, never raise it.

| rung | mechanism added | total bytes | step delta |
| --- | --- | ---: | ---: |
| A0 | RAW | 71116 | — |
| A1 | + RLE | 71116 | 0 |
| A2 | + BYTE_RANS | 43377 | -27739 |
| A3 | + PDF_PHYSICAL | 43377 | 0 |
| A4 | + PDF_CHANNELS | 43377 | 0 |
| A5 | + PDF_LAYOUT | 43377 | 0 |

## Leave-one-out

Removing only the layout mechanism from the full portfolio leaves the A4
rung:

- A5_without_layout = A4 = 43377
- leave_one_out_layout_delta = A5 - A4 = 0

So the PDF layout mechanism saved 0 bytes across the corpus
relative to the same portfolio without it.

Auto-winner counts: RAW=8 RLE=0 BYTE_RANS=2
PDF_PHYSICAL=0 PDF_CHANNELS=0 PDF_LAYOUT=0.

## Honest interpretation

PDF_LAYOUT is *exact*: every forced layout descriptor serializes, is parsed
back, and materializes byte-for-byte, and the mechanism genuinely predicts
positions — it marks each object's introducer offset and the classic `xref`
section start, then emits the 10-digit offset fields and `startxref` value
from those marks rather than storing the digits (see the Phase-5.3 tests
`layout_predicts_entries` and `forced_layout_is_exact`).

It nevertheless loses the complete-cost court. On this corpus it is the auto
winner for 0 of 10 files, and it beats the auto winner on 0
of 7 classic-xref samples. The reason is framing: each predicted
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
